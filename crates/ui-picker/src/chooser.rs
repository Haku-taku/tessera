//! One request, one answer, whichever implementation draws the dialog.
//!
//! The platform-native choosers and the drawn picker already share a contract
//! ([`ui_platform::chooser`]); this module is the policy that picks between them. A caller
//! builds a [`ChooserRequest`] once and a [`Chooser`] with the drawn options it would use
//! as a fallback, and nothing else changes when the answer came from Windows' own dialog,
//! the desktop's portal, or the picker in this crate.
//!
//! The choice is made per call, from the provider's own [`ChooserCapabilities`]: a native
//! dialog is used only when it can honour the whole request, because a switch that
//! silently drops a filter or a second path is worse than a dialog that looks different.
//! `CRABSNAP_FILE_CHOOSER=native|drawn` overrides the policy for diagnostics and reviews.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use ui_platform::chooser::{
    ChooserAnswer, ChooserCapabilities, ChooserError, ChooserId, ChooserMode, ChooserPreference,
    ChooserRequest, OverwritePolicy, StartAt,
};
use ui_platform::host::{MessageCtx, OpenOptions};
use ui_platform::window::SurfaceSpec;
use ui_platform::SurfaceId;

use crate::{Picker, PickerOptions, PickerOutcome};

/// How a chooser was shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooserOpened {
    /// The platform's own dialog is running; its answer reaches the callback later.
    Native(ChooserId),
    /// The drawn picker is running in a dialog surface of its own.
    Drawn(SurfaceId),
}

/// What the drawn picker can honour, in the contract's terms.
///
/// Every mode and control the contract defines is drawable. The one flag it does not raise
/// is `state_persistence`: remembering a folder across runs needs a store this crate has no
/// place for. MIME matching uses the framework's extension table rather than the desktop's,
/// which is why filters stay advice — a name outside the menu may still be typed.
pub fn drawn_capabilities() -> ChooserCapabilities {
    ChooserCapabilities {
        filters: true,
        mime_filters: true,
        filter_advice_only: true,
        custom_places: true,
        choices: true,
        modeless: true,
        show_hidden: true,
        start_at_file: true,
        default_extension: true,
        overwrite_prompt: true,
        state_persistence: false,
        ..ChooserCapabilities::none()
            .mode(ChooserMode::OpenFile, true)
            .mode(ChooserMode::OpenFiles, true)
            .mode(ChooserMode::SelectFolder, true)
            .mode(ChooserMode::SelectFolders, true)
            .mode(ChooserMode::SaveFile, true)
            .mode(ChooserMode::SaveFiles, true)
    }
}

/// A request and the implementations that may answer it.
///
/// The drawn options are optional: a caller that only ever wants the native dialog leaves
/// them out, and a caller running where no native chooser exists must supply them.
pub struct Chooser {
    request: ChooserRequest,
    preference: ChooserPreference,
    drawn: Option<PickerOptions>,
}

impl Chooser {
    pub fn new(request: ChooserRequest, drawn: Option<PickerOptions>) -> Self {
        Self {
            request,
            preference: ChooserPreference::Auto,
            drawn,
        }
    }

    pub fn preference(mut self, preference: ChooserPreference) -> Self {
        self.preference = preference;
        self
    }

    /// Show the chooser over `parent`, answering `done` once on the loop's thread.
    ///
    /// The `spec` is a dialog surface's; its title and modality are replaced by the
    /// request's when those ask for something. A drawn dialog opens with the shortcuts
    /// this crate binds appended to `options`; a caller should not add them itself.
    ///
    /// An `Err` means nothing opened and `done` was never called: the caller may show the
    /// same picker in a window of its own through [`PickerState`](crate::PickerState), which
    /// is the fallback a host without dialog surfaces needs.
    pub fn open(
        self,
        ctx: &mut MessageCtx<'_>,
        parent: SurfaceId,
        spec: SurfaceSpec,
        mut options: OpenOptions,
        done: impl FnOnce(ChooserAnswer) + 'static,
    ) -> Result<ChooserOpened, ChooserError> {
        self.request.validate().map_err(ChooserError::Failed)?;
        let preference = environment_preference(self.preference);
        // The callback lives behind an `Rc` only so a native provider that fails to start
        // does not swallow it: the drawn implementation must still be able to answer.
        let done = Rc::new(RefCell::new(Some(done)));

        if preference != ChooserPreference::DrawnOnly {
            let slot = Rc::clone(&done);
            let result = ctx.choose_native(parent, self.request.clone(), move |answer| {
                if let Some(done) = slot.borrow_mut().take() {
                    done(answer);
                }
            });
            match result {
                Ok(id) => return Ok(ChooserOpened::Native(id)),
                Err(error) if preference == ChooserPreference::NativeOnly => {
                    return Err(error);
                }
                Err(error) if !error.offers_fallback() => return Err(error),
                // The native provider could not take the request; the drawn one is next.
                Err(_) => {}
            }
        }

        let Some(done) = done.borrow_mut().take() else {
            return Err(ChooserError::Failed(
                "the chooser was answered while opening".to_string(),
            ));
        };
        let drawn_options = match self.drawn {
            Some(options) => options,
            None => {
                return Err(ChooserError::Unavailable(
                    "no native chooser can take this request and no drawn picker was supplied"
                        .to_string(),
                ));
            }
        };
        if let Some(feature) = drawn_capabilities().missing(&self.request) {
            return Err(ChooserError::Unsupported(feature));
        }
        let mut drawn = drawn_options.for_request(&self.request);
        drawn.accept_label = self.request.accept_label.clone();
        drawn.cancel_label = self.request.cancel_label.clone();
        let mut spec = spec;
        spec = spec
            .with_modal(self.request.modal)
            .with_min_size(crate::MIN_SIZE);
        if let Some(title) = &self.request.title {
            spec.name = title.clone();
        }
        options.shortcuts.extend(Picker::shortcuts());
        let picker = Picker::new(drawn, move |outcome| {
            let result = match outcome {
                PickerOutcome::Selected(selection) => Ok(selection),
                PickerOutcome::Cancelled => Err(ChooserError::Cancelled),
            };
            done(ChooserAnswer::drawn(result));
        });
        let id = ctx
            .open_dialog(parent, spec, Box::new(picker), options)
            .map_err(ChooserError::Failed)?;
        Ok(ChooserOpened::Drawn(id))
    }
}

/// The caller's preference unless the environment overrides it for a run.
fn environment_preference(chosen: ChooserPreference) -> ChooserPreference {
    match std::env::var("CRABSNAP_FILE_CHOOSER").as_deref() {
        Ok("native") => ChooserPreference::NativeOnly,
        Ok("drawn") => ChooserPreference::DrawnOnly,
        _ => chosen,
    }
}

impl PickerOptions {
    /// This picker configured to run `request`.
    ///
    /// The services (listing, places, thumbnails, icons, pack, environment) stay the
    /// caller's; everything the request answers for is replaced, so a caller cannot end up
    /// with a drawn dialog that disagrees with the request it was asked to show. A caller
    /// that holds one request for both implementations uses this to draw the same one.
    pub fn for_request(mut self, request: &ChooserRequest) -> Self {
        let start = match &request.start {
            StartAt::File(path) => path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_path_buf(),
            other => other.path().to_path_buf(),
        };
        let save_name = request
            .save
            .as_ref()
            .and_then(|save| save.suggested_name.clone())
            .or_else(|| match &request.start {
                StartAt::File(path) => path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string),
                _ => None,
            });
        self.start = start;
        self.start_at_file = match &request.start {
            StartAt::File(path) if !request.mode.is_save() => Some(path.clone()),
            _ => None,
        };
        self.mode = request.mode;
        self.filters = request.filters.clone();
        // The contract opens on the named filter, else the first one; `None` only means
        // "no filter" when the request carries none.
        self.default_filter = request
            .default_filter
            .or(if request.filters.is_empty() { None } else { Some(0) });
        self.extra_places = request.extra_places.clone();
        self.choices = request.choices.clone();
        self.save_name = save_name;
        self.save_names = request
            .save
            .as_ref()
            .map(|save| save.names.clone())
            .unwrap_or_default();
        self.default_extension = request
            .save
            .as_ref()
            .and_then(|save| save.default_extension.clone());
        self.overwrite = request
            .save
            .as_ref()
            .map(|save| save.overwrite)
            .unwrap_or(OverwritePolicy::Prompt);
        self.strict_types = request.save.as_ref().is_some_and(|save| save.strict_types);
        self.show_hidden = request.show_hidden;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawn_picker_reports_every_mode_the_contract_defines() {
        let capabilities = drawn_capabilities();
        for mode in ChooserMode::ALL {
            assert!(
                capabilities.modes[mode.index()],
                "{mode:?} is drawable and must not force a fallback"
            );
        }
    }
}
