//! The Windows common item dialog: a [`ChooserRequest`] to `IFileDialog` and its answer back.
//!
//! The dialog runs its own message loop inside `Show`, so the caller blocks until the user
//! answers; the presenter queues the answer and the host delivers it on its next turn.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use ui_platform::chooser::{
    ChooserCapabilities, ChooserError, ChooserMode, ChooserRequest, ChooserSelection,
    OverwritePolicy, StartAt,
};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND, RPC_E_CHANGED_MODE};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FDAP_BOTTOM, FILEOPENDIALOGOPTIONS, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST,
    FOS_FORCEFILESYSTEM, FOS_FORCESHOWHIDDEN, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, FOS_STRICTFILETYPES, FileOpenDialog, FileSaveDialog, IFileDialog,
    IFileDialog2, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{GUID, HRESULT, Interface, PCWSTR};

/// What a Win32 surface offers: every dialog-shaped mode but the two the shell does not
/// document — several folders and several save names.
pub(crate) fn capabilities() -> ChooserCapabilities {
    let mut capabilities = ChooserCapabilities::none()
        .mode(ChooserMode::OpenFile, true)
        .mode(ChooserMode::OpenFiles, true)
        .mode(ChooserMode::SelectFolder, true)
        .mode(ChooserMode::SaveFile, true);
    capabilities.filters = true;
    capabilities.mime_filters = false;
    capabilities.filter_advice_only = false;
    capabilities.custom_places = true;
    capabilities.choices = false;
    capabilities.modeless = false;
    capabilities.show_hidden = true;
    capabilities.start_at_file = true;
    capabilities.default_extension = true;
    capabilities.overwrite_prompt = true;
    capabilities.state_persistence = true;
    capabilities
}

/// Run one dialog parented to `hwnd`, and report what it answered.
pub(crate) fn choose(
    hwnd: HWND,
    request: &ChooserRequest,
) -> Result<ChooserSelection, ChooserError> {
    let _apartment = Apartment::enter()?;
    let dialog = create_dialog(request.mode)?;
    apply_options(&dialog, request)?;
    apply_labels(&dialog, request)?;
    apply_filters(&dialog, request)?;
    apply_start(&dialog, request)?;
    apply_save(&dialog, request)?;
    apply_places(&dialog, request)?;
    if let Some(key) = &request.state_key {
        let guid = state_guid(key);
        // SAFETY: `guid` lives for the whole call, which is all the API reads.
        unsafe { dialog.SetClientGuid(&guid) }.map_err(|error| {
            ChooserError::Failed(format!(
                "the file dialog's state key could not be set: {error}"
            ))
        })?;
    }

    match unsafe { dialog.Show(Some(hwnd)) } {
        Ok(()) => {}
        Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
            return Err(ChooserError::Cancelled);
        }
        Err(error) => {
            return Err(ChooserError::Failed(format!(
                "the file dialog could not be shown: {error}"
            )));
        }
    }

    let filter = if request.filters.is_empty() {
        None
    } else {
        // The dialog reports its type index from one; ignoring a failure only loses the
        // answer's advisory filter, never the paths the user chose.
        unsafe { dialog.GetFileTypeIndex() }
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| usize::try_from(index).ok())
    };
    let paths = if request.mode.is_multi() && !request.mode.is_save() {
        let open: IFileOpenDialog = dialog.cast().map_err(|error| {
            ChooserError::Failed(format!(
                "the file dialog could not list its results: {error}"
            ))
        })?;
        selected_paths_from_array(&open)?
    } else {
        let item = unsafe { dialog.GetResult() }.map_err(|error| {
            ChooserError::Failed(format!("the file dialog has no result: {error}"))
        })?;
        vec![item_path(&item)?]
    };
    Ok(ChooserSelection {
        paths,
        filter,
        choices: Vec::new(),
    })
}

/// Every option this mapping decides. The system's own defaults are read first and these
/// bits are replaced, so a folder picker cannot inherit a file requirement from them.
const MANAGED_OPTIONS: FILEOPENDIALOGOPTIONS = FILEOPENDIALOGOPTIONS(
    FOS_FORCEFILESYSTEM.0
        | FOS_PATHMUSTEXIST.0
        | FOS_FILEMUSTEXIST.0
        | FOS_PICKFOLDERS.0
        | FOS_ALLOWMULTISELECT.0
        | FOS_FORCESHOWHIDDEN.0
        | FOS_OVERWRITEPROMPT.0
        | FOS_STRICTFILETYPES.0,
);

/// The options `request` asks for, in the dialog's own vocabulary.
fn options_for(request: &ChooserRequest) -> FILEOPENDIALOGOPTIONS {
    let mode = request.mode;
    let mut options = FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST;
    if mode.takes_directories() {
        options |= FOS_PICKFOLDERS;
    } else if !mode.is_save() {
        options |= FOS_FILEMUSTEXIST;
    }
    if mode.is_multi() {
        options |= FOS_ALLOWMULTISELECT;
    }
    if request.show_hidden {
        options |= FOS_FORCESHOWHIDDEN;
    }
    if let Some(save) = &request.save {
        if save.overwrite == OverwritePolicy::Prompt {
            options |= FOS_OVERWRITEPROMPT;
        }
        if save.strict_types {
            options |= FOS_STRICTFILETYPES;
        }
    }
    options
}

/// The type menu's entries: the label, and its patterns joined as the dialog spells them.
fn filter_specs(request: &ChooserRequest) -> Vec<(String, String)> {
    request
        .filters
        .iter()
        .map(|filter| {
            // A filter that only names MIME types still needs a pattern the shell accepts.
            let spec = if filter.patterns.is_empty() {
                String::from("*")
            } else {
                filter.patterns.join(";")
            };
            (filter.label.clone(), spec)
        })
        .collect()
}

/// A stable 128-bit FNV-1a of `key`, which the shell remembers the dialog's state by.
fn state_guid(key: &str) -> GUID {
    const OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let hash = key.as_bytes().iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
    });
    GUID::from_u128(hash)
}

/// COM must be an apartment on the thread that shows the dialog. Every value borrowed from
/// the apartment is dropped before this does, so the uninitialize order is safe.
struct Apartment {
    uninitialize: bool,
}

impl Apartment {
    fn enter() -> Result<Self, ChooserError> {
        // SAFETY: the call takes no pointer this caller owns, and the apartment it starts is
        // released in `drop` below.
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialized == RPC_E_CHANGED_MODE {
            // The thread already has an apartment of another model. The dialog can use it,
            // and uninitializing here would tear down a mode this call did not set.
            return Ok(Self {
                uninitialize: false,
            });
        }
        if initialized.is_err() {
            return Err(ChooserError::Failed(format!(
                "COM could not be initialized for the file dialog: {initialized:?}"
            )));
        }
        Ok(Self { uninitialize: true })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: every successful `CoInitializeEx` on this thread is paired with one
            // `CoUninitialize`, which is this one.
            unsafe { CoUninitialize() };
        }
    }
}

/// Create the dialog object for `mode`, still unconfigured.
fn create_dialog(mode: ChooserMode) -> Result<IFileDialog, ChooserError> {
    let class = if mode.is_save() {
        &FileSaveDialog
    } else {
        &FileOpenDialog
    };
    // SAFETY: the class id is a static GUID, and the returned object is an in-process shell
    // object owned by this apartment.
    unsafe { CoCreateInstance(class, None, CLSCTX_INPROC_SERVER) }.map_err(|error| {
        ChooserError::Failed(format!("the file dialog could not be created: {error}"))
    })
}

fn apply_options(dialog: &IFileDialog, request: &ChooserRequest) -> Result<(), ChooserError> {
    // SAFETY: the dialog is a live COM object on this thread.
    let existing = unsafe { dialog.GetOptions() }.map_err(|error| {
        ChooserError::Failed(format!(
            "the file dialog's options could not be read: {error}"
        ))
    })?;
    let options = (existing & !MANAGED_OPTIONS) | options_for(request);
    // SAFETY: as above.
    unsafe { dialog.SetOptions(options) }.map_err(|error| {
        ChooserError::Failed(format!(
            "the file dialog's options could not be set: {error}"
        ))
    })
}

fn apply_labels(dialog: &IFileDialog, request: &ChooserRequest) -> Result<(), ChooserError> {
    if let Some(title) = &request.title {
        set_text(|text| unsafe { dialog.SetTitle(text) }, title, "title")?;
    }
    if let Some(label) = &request.accept_label {
        set_text(
            |text| unsafe { dialog.SetOkButtonLabel(text) },
            label,
            "accept button label",
        )?;
    }
    if let Some(label) = &request.cancel_label {
        // Only the newer interface offers this label; the dialog without it keeps the
        // system's own wording rather than failing the whole choice.
        match dialog.cast::<IFileDialog2>() {
            Ok(dialog) => set_text(
                |text| unsafe { dialog.SetCancelButtonLabel(text) },
                label,
                "cancel button label",
            )?,
            Err(error) => log::debug!(
                "ui-platform-win32: the file dialog has no cancel button label: {error}"
            ),
        }
    }
    Ok(())
}

fn apply_filters(dialog: &IFileDialog, request: &ChooserRequest) -> Result<(), ChooserError> {
    if request.filters.is_empty() {
        return Ok(());
    }
    let specs = filter_specs(request);
    // The vectors must not move while the dialog reads them, so every string is built
    // before the `COMDLG_FILTERSPEC` table that points into them.
    let mut names = Vec::with_capacity(specs.len());
    let mut patterns = Vec::with_capacity(specs.len());
    for (name, pattern) in &specs {
        names.push(crate::window::wide(name));
        patterns.push(crate::window::wide(pattern));
    }
    let filter_specs: Vec<COMDLG_FILTERSPEC> = names
        .iter()
        .zip(&patterns)
        .map(|(name, pattern)| COMDLG_FILTERSPEC {
            pszName: PCWSTR::from_raw(name.as_ptr()),
            pszSpec: PCWSTR::from_raw(pattern.as_ptr()),
        })
        .collect();
    // SAFETY: every pointer in the table addresses a NUL-terminated string that outlives the
    // call, and the table is handed over as a slice of its own length.
    unsafe { dialog.SetFileTypes(&filter_specs) }.map_err(|error| {
        ChooserError::Failed(format!(
            "the file dialog's filters could not be set: {error}"
        ))
    })?;
    let index = request.default_filter.unwrap_or(0);
    let selected = u32::try_from(index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or_else(|| {
            ChooserError::Failed(String::from("the default filter index is too large"))
        })?;
    // `SetFileTypeIndex` counts from one.
    unsafe { dialog.SetFileTypeIndex(selected) }.map_err(|error| {
        ChooserError::Failed(format!(
            "the file dialog's filter could not be selected: {error}"
        ))
    })
}

fn apply_start(dialog: &IFileDialog, request: &ChooserRequest) -> Result<(), ChooserError> {
    match &request.start {
        StartAt::Remembered(path) => {
            let item = shell_item(path)?;
            // SAFETY: both objects are live, and the call only borrows the item.
            unsafe { dialog.SetDefaultFolder(&item) }.map_err(|error| {
                ChooserError::Failed(format!(
                    "the file dialog's default folder could not be set: {error}"
                ))
            })
        }
        StartAt::Folder(path) => {
            let item = shell_item(path)?;
            // SAFETY: as above.
            unsafe { dialog.SetFolder(&item) }.map_err(|error| {
                ChooserError::Failed(format!(
                    "the file dialog's folder could not be set: {error}"
                ))
            })
        }
        StartAt::File(path) => {
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                set_text(
                    |text| unsafe { dialog.SetFileName(text) },
                    &name,
                    "file name",
                )?;
            }
            match path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                Some(parent) => {
                    let item = shell_item(parent)?;
                    // SAFETY: both objects are live, and the call only borrows the item.
                    unsafe { dialog.SetFolder(&item) }.map_err(|error| {
                        ChooserError::Failed(format!(
                            "the file dialog's folder could not be set: {error}"
                        ))
                    })
                }
                // A bare file name has no folder to name; the dialog opens where it would
                // have opened anyway.
                None => Ok(()),
            }
        }
    }
}

fn apply_save(dialog: &IFileDialog, request: &ChooserRequest) -> Result<(), ChooserError> {
    let Some(save) = &request.save else {
        return Ok(());
    };
    if let Some(name) = &save.suggested_name {
        set_text(
            |text| unsafe { dialog.SetFileName(text) },
            name,
            "suggested file name",
        )?;
    }
    if let Some(extension) = &save.default_extension {
        // The API takes the extension without the leading period that a caller may spell.
        let extension = extension.strip_prefix('.').unwrap_or(extension);
        if !extension.is_empty() {
            set_text(
                |text| unsafe { dialog.SetDefaultExtension(text) },
                extension,
                "default extension",
            )?;
        }
    }
    Ok(())
}

fn apply_places(dialog: &IFileDialog, request: &ChooserRequest) -> Result<(), ChooserError> {
    for place in &request.extra_places {
        let item = shell_item(&place.path)?;
        // SAFETY: both objects are live, and the call only borrows the item.
        unsafe { dialog.AddPlace(&item, FDAP_BOTTOM) }.map_err(|error| {
            ChooserError::Failed(format!(
                "the place {} could not be added to the file dialog: {error}",
                place.path.display()
            ))
        })?;
    }
    Ok(())
}

fn selected_paths_from_array(dialog: &IFileOpenDialog) -> Result<Vec<PathBuf>, ChooserError> {
    // SAFETY: the dialog is a live COM object on this thread.
    let items = unsafe { dialog.GetResults() }.map_err(|error| {
        ChooserError::Failed(format!("the file dialog has no results: {error}"))
    })?;
    // SAFETY: as above.
    let count = unsafe { items.GetCount() }.map_err(|error| {
        ChooserError::Failed(format!(
            "the file dialog's results could not be counted: {error}"
        ))
    })?;
    if count == 0 {
        return Err(ChooserError::Failed(String::from(
            "the file dialog returned no paths",
        )));
    }
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        // SAFETY: `index` is below the count the array just reported.
        let item = unsafe { items.GetItemAt(index) }.map_err(|error| {
            ChooserError::Failed(format!(
                "the file dialog's result {index} could not be read: {error}"
            ))
        })?;
        paths.push(item_path(&item)?);
    }
    Ok(paths)
}

/// A shell item's filesystem path; the dialogs are opened with `FOS_FORCEFILESYSTEM`, so
/// every result has one.
fn item_path(item: &IShellItem) -> Result<PathBuf, ChooserError> {
    // SAFETY: the item is live, and the shell allocates the returned string for the caller.
    let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|error| {
        ChooserError::Failed(format!("a chosen item has no filesystem path: {error}"))
    })?;
    // SAFETY: the pointer is NUL-terminated UTF-16 and is freed once below.
    let text = unsafe { name.to_string() };
    // SAFETY: `GetDisplayName` hands ownership of the string to this caller.
    unsafe { CoTaskMemFree(Some(name.0 as *const c_void)) };
    let text = text.map_err(|error| {
        ChooserError::Failed(format!("a chosen path is not valid UTF-16: {error}"))
    })?;
    Ok(PathBuf::from(text))
}

fn shell_item(path: &Path) -> Result<IShellItem, ChooserError> {
    let wide = crate::window::wide(&path.to_string_lossy());
    // SAFETY: the path is NUL-terminated and outlives the call; the item is a COM object
    // owned by this apartment.
    unsafe {
        SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR::from_raw(wide.as_ptr()), None)
    }
    .map_err(|error| {
        ChooserError::Failed(format!(
            "{} could not be opened as a shell item: {error}",
            path.display()
        ))
    })
}

/// Set one of the dialog's strings, keeping the wide buffer alive across the call.
fn set_text(
    apply: impl FnOnce(PCWSTR) -> windows::core::Result<()>,
    value: &str,
    what: &str,
) -> Result<(), ChooserError> {
    let wide = crate::window::wide(value);
    apply(PCWSTR::from_raw(wide.as_ptr())).map_err(|error| {
        ChooserError::Failed(format!(
            "the file dialog's {what} could not be set: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_platform::chooser::{FileTypeFilter, SaveOptions};

    #[test]
    fn a_folder_picker_asks_for_folders_and_never_for_a_file() {
        let options = options_for(&ChooserRequest::select_folder(PathBuf::from("C:\\work")));
        assert!(options.contains(FOS_FORCEFILESYSTEM));
        assert!(options.contains(FOS_PICKFOLDERS));
        assert!(!options.contains(FOS_FILEMUSTEXIST));
        assert!(!options.contains(FOS_ALLOWMULTISELECT));
    }

    #[test]
    fn an_open_multi_picker_requires_existing_files_and_allows_several() {
        let options = options_for(&ChooserRequest::open_files(PathBuf::from("C:\\work")));
        assert!(options.contains(FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_FILEMUSTEXIST));
        assert!(options.contains(FOS_ALLOWMULTISELECT));
        assert!(!options.contains(FOS_PICKFOLDERS));
    }

    #[test]
    fn a_save_picker_requires_a_folder_but_not_an_existing_file() {
        let request = ChooserRequest::save_file("C:\\work\\out.png");
        let options = options_for(&request);
        assert!(options.contains(FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST));
        assert!(options.contains(FOS_OVERWRITEPROMPT));
        assert!(!options.contains(FOS_FILEMUSTEXIST));
        assert!(!options.contains(FOS_PICKFOLDERS));
        assert!(!options.contains(FOS_STRICTFILETYPES));

        let strict = request.save(SaveOptions {
            strict_types: true,
            ..SaveOptions::default()
        });
        assert!(options_for(&strict).contains(FOS_STRICTFILETYPES));
    }

    #[test]
    fn filter_patterns_join_with_semicolons_and_an_empty_list_accepts_any_name() {
        let request = ChooserRequest::open_file("C:\\work")
            .filter(FileTypeFilter::new(
                "Images",
                vec!["*.png".to_string(), "*.jpg".to_string()],
            ))
            .filter(
                FileTypeFilter::new("Anything", Vec::new())
                    .mime_types(vec!["image/png".to_string()]),
            );
        assert_eq!(
            filter_specs(&request),
            vec![
                (String::from("Images"), String::from("*.png;*.jpg")),
                (String::from("Anything"), String::from("*")),
            ]
        );
    }

    #[test]
    fn the_state_guid_is_a_deterministic_fnv1a_of_the_key() {
        assert_eq!(
            state_guid(""),
            GUID::from_u128(0x6c62_272e_07bb_0142_62b8_2175_6295_c58d)
        );
        assert_eq!(state_guid("crabsnap.picker"), state_guid("crabsnap.picker"));
        assert_ne!(state_guid("crabsnap.picker"), state_guid("crabsnap.other"));
    }
}
