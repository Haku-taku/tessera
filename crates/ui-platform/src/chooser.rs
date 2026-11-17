//! What a file chooser is asked for, and what it may answer, in no platform's terms.
//!
//! A native dialog on Windows and a portal dialog on Linux disagree about almost every
//! feature: folders are single-pick on one and undocumented on the other, filters are
//! advisory on one and absent on the other, and only the desktop can read its own hidden
//! files setting. The request below is what both, and the drawn picker, can be asked for;
//! [`ChooserCapabilities`] is how a provider says which quarters of it it can honour.
//!
//! Nothing here names a window, a thread or a protocol. A backend reports its capabilities
//! through [`SurfaceHandle::chooser_capabilities`](crate::host::SurfaceHandle), starts a
//! chooser with [`SurfaceHandle::choose`](crate::host::SurfaceHandle), and hands the answer
//! back over the loop's own turns; the host owns completion, so the caller sees one answer
//! exactly once, on the thread that builds frames.
//!
//! The contract returns filesystem paths only. Windows would also offer virtual shell items
//! (libraries, network places, portable devices), but a Linux portal cannot name them and a
//! drawn picker has no shell namespace to read, so a provider that cannot promise a path
//! sets `FOS_FORCEFILESYSTEM` rather than inventing a second result type.

use std::path::{Path, PathBuf};

/// What the user is choosing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChooserMode {
    /// One file that already exists.
    OpenFile,
    /// Several files that already exist.
    OpenFiles,
    /// One directory.
    SelectFolder,
    /// Several directories. Defined in the contract; a provider that cannot pick more than
    /// one says so, and the fallback policy decides what to do about it.
    SelectFolders,
    /// One file name to write.
    SaveFile,
    /// Several file names to write. The portal's `SaveFiles` is the only platform API that
    /// offers it; every provider must report what it actually did, because the desktop may
    /// rename a file to avoid a collision.
    SaveFiles,
}

impl ChooserMode {
    /// Every mode, in the order [`ChooserCapabilities::modes`] lists them.
    pub const ALL: [ChooserMode; 6] = [
        ChooserMode::OpenFile,
        ChooserMode::OpenFiles,
        ChooserMode::SelectFolder,
        ChooserMode::SelectFolders,
        ChooserMode::SaveFile,
        ChooserMode::SaveFiles,
    ];

    /// The mode's index in [`ChooserCapabilities::modes`].
    pub const fn index(self) -> usize {
        match self {
            ChooserMode::OpenFile => 0,
            ChooserMode::OpenFiles => 1,
            ChooserMode::SelectFolder => 2,
            ChooserMode::SelectFolders => 3,
            ChooserMode::SaveFile => 4,
            ChooserMode::SaveFiles => 5,
        }
    }

    /// Whether more than one path may come back.
    pub const fn is_multi(self) -> bool {
        matches!(
            self,
            ChooserMode::OpenFiles | ChooserMode::SelectFolders | ChooserMode::SaveFiles
        )
    }

    /// Whether the answer names paths that may not exist yet.
    pub const fn is_save(self) -> bool {
        matches!(self, ChooserMode::SaveFile | ChooserMode::SaveFiles)
    }

    /// Whether a directory is an acceptable answer by itself.
    pub const fn takes_directories(self) -> bool {
        matches!(self, ChooserMode::SelectFolder | ChooserMode::SelectFolders)
    }
}

/// One entry of a chooser's type menu.
///
/// `patterns` are glob patterns (`*.png`, `*.tar.gz`), the one spelling every provider can
/// translate. `mime_types` are a Linux hint: the portal uses them, the drawn picker may
/// match them, and Windows ignores them. A filter is advice to the user, not a promise:
/// portal desktops may let a name outside it through, and a caller must validate an answer
/// it cannot accept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTypeFilter {
    pub label: String,
    pub patterns: Vec<String>,
    pub mime_types: Vec<String>,
}

impl FileTypeFilter {
    pub fn new(label: impl Into<String>, patterns: Vec<String>) -> Self {
        Self {
            label: label.into(),
            patterns,
            mime_types: Vec::new(),
        }
    }

    /// The filter every dialog offers: anything with a name.
    pub fn all_files() -> Self {
        Self::new("All files", vec!["*".to_string()])
    }

    pub fn mime_types(mut self, mime_types: Vec<String>) -> Self {
        self.mime_types = mime_types;
        self
    }

    /// Whether `name` is one the filter names. Directories are not a filter's business:
    /// a provider tests them before asking, the way file managers do.
    pub fn matches(&self, name: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| glob_match(pattern, name))
    }

    /// Whether `mime` is one the filter names, exact or by a `type/*` wildcard.
    ///
    /// A provider that can read a file's type asks this in addition to [`matches`]; a name
    /// can match either half, as the portal's filters do.
    pub fn matches_mime(&self, mime: &str) -> bool {
        self.mime_types.iter().any(|named| {
            named.eq_ignore_ascii_case(mime)
                || named
                    .strip_suffix("/*")
                    .is_some_and(|kind| mime.split_once('/').is_some_and(|(actual, _)| kind.eq_ignore_ascii_case(actual)))
        })
    }
}

/// A name against a glob: `*` matches any run, `?` one character, everything else is
/// itself. Case-insensitive, as the filters in every file manager are.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let name = name.as_bytes();
    let (mut p, mut n) = (0, 0);
    let (mut star, mut resume) = (None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p].eq_ignore_ascii_case(&name[n])) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            resume = n;
            p += 1;
        } else if let Some(at) = star {
            // Backtrack to the last star and let it swallow one more byte.
            p = at + 1;
            resume += 1;
            n = resume;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

/// Where the dialog opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartAt {
    /// The folder the user last used, or the platform's idea of a sensible default.
    ///
    /// Windows maps this to `SetDefaultFolder`, which keeps the remembered folder when the
    /// one named here is not the last one; the portal may ignore it.
    Remembered(PathBuf),
    /// This folder, every time. Windows maps it to `SetFolder`.
    Folder(PathBuf),
    /// This existing file, selected. Save modes read it as "this name, in its folder".
    File(PathBuf),
}

impl StartAt {
    pub fn path(&self) -> &Path {
        match self {
            StartAt::Remembered(path) | StartAt::Folder(path) | StartAt::File(path) => path,
        }
    }
}

/// What a save does when the name is taken.
///
/// Advisory: the portal leaves the decision to the desktop, and a drawn dialog that cannot
/// ask must tell its caller, which checks before writing. A provider that can ask says so
/// with [`ChooserCapabilities::overwrite_prompt`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverwritePolicy {
    /// Ask before replacing an existing file.
    Prompt,
    /// Replace without asking.
    Allow,
    /// Refuse an existing name. No platform dialog enforces this; the caller must.
    Forbid,
}

/// The save-specific part of a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveOptions {
    /// The name the dialog starts with.
    pub suggested_name: Option<String>,
    /// The names to write, for [`ChooserMode::SaveFiles`]. A single save names its one file
    /// through `suggested_name`; a provider that offers SaveFiles writes these into the
    /// folder the user chose and reports the paths it actually used.
    pub names: Vec<String>,
    /// The extension to append when the user omits one.
    pub default_extension: Option<String>,
    pub overwrite: OverwritePolicy,
    /// Accept only the active filter's patterns. Windows honours this on save; elsewhere
    /// it is advice, like every filter.
    pub strict_types: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            suggested_name: None,
            names: Vec::new(),
            default_extension: None,
            overwrite: OverwritePolicy::Prompt,
            strict_types: false,
        }
    }
}

/// A place a provider adds to the dialog's sidebar, besides the system's own.
///
/// Windows maps it to `AddPlace`; the portal has no equivalent and reports so; the drawn
/// picker shows it beside the places its app supplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChooserPlace {
    pub label: String,
    pub path: PathBuf,
}

impl ChooserPlace {
    pub fn new(label: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            label: label.into(),
            path: path.into(),
        }
    }
}

/// One option of a [`ChoiceKind::Select`] choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
}

impl ChoiceOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// What a [`ChooserChoice`] offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    /// A drop-down of named values; the answer is the chosen `value`.
    Select(Vec<ChoiceOption>),
    /// A checkbox; the answer is `"true"` or `"false"`.
    Boolean,
}

/// An extra control a dialog may show, from the portal's `choices` and Windows'
/// `IFileDialogCustomize`.
///
/// A request that carries one is only run by a provider whose `choices` capability is true:
/// dropping a caller's option silently would change what the answer means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChooserChoice {
    pub id: String,
    pub label: String,
    pub kind: ChoiceKind,
    /// The initial value, as the answer will spell it.
    pub initial: String,
}

impl ChooserChoice {
    pub fn select(
        id: impl Into<String>,
        label: impl Into<String>,
        options: Vec<ChoiceOption>,
        initial: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: ChoiceKind::Select(options),
            initial: initial.into(),
        }
    }

    pub fn boolean(
        id: impl Into<String>,
        label: impl Into<String>,
        initial: bool,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: ChoiceKind::Boolean,
            initial: if initial { "true" } else { "false" }.to_string(),
        }
    }
}

/// Everything one chooser is asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChooserRequest {
    pub mode: ChooserMode,
    pub title: Option<String>,
    pub start: StartAt,
    pub filters: Vec<FileTypeFilter>,
    /// The filter selected when the dialog opens; `None` means the first, or "all files"
    /// when no filter was given.
    pub default_filter: Option<usize>,
    /// The accept button's label, when the caller wants its own word ("Export", "Attach").
    pub accept_label: Option<String>,
    pub cancel_label: Option<String>,
    /// Ask the dialog to show hidden files. A provider that cannot change the desktop's
    /// setting reports it as unsupported instead of quietly showing them.
    pub show_hidden: bool,
    /// Hold the parent's input while open. Windows dialogs are always modal; a provider
    /// that cannot be modeless reports so when this is false.
    pub modal: bool,
    pub extra_places: Vec<ChooserPlace>,
    pub choices: Vec<ChooserChoice>,
    /// Required for save modes; `validate` rejects its absence.
    pub save: Option<SaveOptions>,
    /// A stable name for the dialog's remembered state (folder, window size). Windows maps
    /// it to `SetClientGuid`; providers that keep no state ignore it.
    pub state_key: Option<String>,
}

impl ChooserRequest {
    pub fn open_file(start: impl Into<PathBuf>) -> Self {
        Self::for_mode(ChooserMode::OpenFile, StartAt::Remembered(start.into()))
    }

    pub fn open_files(start: impl Into<PathBuf>) -> Self {
        Self::for_mode(ChooserMode::OpenFiles, StartAt::Remembered(start.into()))
    }

    pub fn select_folder(start: impl Into<PathBuf>) -> Self {
        Self::for_mode(ChooserMode::SelectFolder, StartAt::Remembered(start.into()))
    }

    pub fn select_folders(start: impl Into<PathBuf>) -> Self {
        Self::for_mode(ChooserMode::SelectFolders, StartAt::Remembered(start.into()))
    }

    /// A save that starts at `path`: its folder, and its name when it has one.
    pub fn save_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut save = SaveOptions::default();
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            save.suggested_name = Some(name.to_string());
        }
        Self {
            save: Some(save),
            ..Self::for_mode(ChooserMode::SaveFile, StartAt::File(path))
        }
    }

    /// A save of several named files into a folder the user picks.
    ///
    /// The names are what the caller means to write; a provider may have to change one to
    /// avoid a collision, so the answer — not the request — is what the caller reads.
    pub fn save_files(start: impl Into<PathBuf>, names: Vec<String>) -> Self {
        let save = SaveOptions {
            names,
            ..SaveOptions::default()
        };
        Self {
            save: Some(save),
            ..Self::for_mode(ChooserMode::SaveFiles, StartAt::Remembered(start.into()))
        }
    }

    fn for_mode(mode: ChooserMode, start: StartAt) -> Self {
        Self {
            mode,
            title: None,
            start,
            filters: Vec::new(),
            default_filter: None,
            accept_label: None,
            cancel_label: None,
            show_hidden: false,
            modal: true,
            extra_places: Vec::new(),
            choices: Vec::new(),
            save: None,
            state_key: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn filter(mut self, filter: FileTypeFilter) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn filters(mut self, filters: Vec<FileTypeFilter>) -> Self {
        self.filters = filters;
        self
    }

    pub fn default_filter(mut self, index: usize) -> Self {
        self.default_filter = Some(index);
        self
    }

    pub fn accept_label(mut self, label: impl Into<String>) -> Self {
        self.accept_label = Some(label.into());
        self
    }

    pub fn cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = Some(label.into());
        self
    }

    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }

    /// A dialog that leaves the parent usable while it is open. Only drawable providers
    /// offer one.
    pub fn modeless(mut self) -> Self {
        self.modal = false;
        self
    }

    pub fn place(mut self, place: ChooserPlace) -> Self {
        self.extra_places.push(place);
        self
    }

    pub fn choice(mut self, choice: ChooserChoice) -> Self {
        self.choices.push(choice);
        self
    }

    pub fn save(mut self, save: SaveOptions) -> Self {
        self.save = Some(save);
        self
    }

    pub fn state_key(mut self, key: impl Into<String>) -> Self {
        self.state_key = Some(key.into());
        self
    }

    /// The filter the dialog opens on, if any.
    pub fn active_filter(&self) -> Option<&FileTypeFilter> {
        let index = self
            .default_filter
            .or(if self.filters.is_empty() { None } else { Some(0) })?;
        self.filters.get(index)
    }

    /// Reject a request no provider could act on, before one is asked to.
    ///
    /// The message names the field, because the caller is a program: a user never sees it.
    pub fn validate(&self) -> Result<(), String> {
        if self.mode.is_save() && self.save.is_none() {
            return Err("a save request must carry SaveOptions".to_string());
        }
        if let Some(save) = &self.save {
            if self.mode == ChooserMode::SaveFiles && save.names.is_empty() {
                return Err("a SaveFiles request must name at least one file".to_string());
            }
            for name in &save.names {
                if name.is_empty() || name.contains(['/', '\\', '\0']) {
                    return Err(format!("{name:?} is not a plain file name"));
                }
            }
        }
        if let Some(index) = self.default_filter
            && index >= self.filters.len()
        {
            return Err(format!(
                "default_filter {index} names no entry of {} filters",
                self.filters.len()
            ));
        }
        for (index, filter) in self.filters.iter().enumerate() {
            if filter.patterns.is_empty() && filter.mime_types.is_empty() {
                return Err(format!("filter {index} has neither patterns nor MIME types"));
            }
        }
        for choice in &self.choices {
            if let ChooserChoice {
                kind: ChoiceKind::Select(options),
                initial,
                id,
                ..
            } = choice
                && !options.iter().any(|option| &option.value == initial)
            {
                return Err(format!(
                    "choice {id} starts at {initial:?}, which is not one of its options"
                ));
            }
        }
        Ok(())
    }
}

/// What a chooser answered, in the caller's terms.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ChooserSelection {
    /// The chosen paths, in the order the dialog reported them. Never empty on success.
    pub paths: Vec<PathBuf>,
    /// The filter the answer was given under, when the provider knows which was active.
    pub filter: Option<usize>,
    /// The extra choices' values at accept time, in the request's order.
    pub choices: Vec<(String, String)>,
}

impl ChooserSelection {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            filter: None,
            choices: Vec::new(),
        }
    }

    /// The answer for a single-path mode, or the first path of a multi one.
    pub fn first(&self) -> Option<&Path> {
        self.paths.first().map(PathBuf::as_path)
    }
}

/// Why a chooser produced no selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChooserError {
    /// The user closed the dialog without choosing. Not a failure.
    Cancelled,
    /// The provider cannot honour this request; the caller may fall back to another one.
    /// The string names the first unmet feature.
    Unsupported(&'static str),
    /// The platform has no chooser at all right now (no portal service, no desktop).
    Unavailable(String),
    /// The dialog started and then failed.
    Failed(String),
}

impl ChooserError {
    /// Whether another provider could still succeed with the same request.
    pub fn offers_fallback(&self) -> bool {
        matches!(self, ChooserError::Unsupported(_) | ChooserError::Unavailable(_))
    }
}

impl std::fmt::Display for ChooserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChooserError::Cancelled => f.write_str("the chooser was cancelled"),
            ChooserError::Unsupported(feature) => {
                write!(f, "the chooser cannot offer {feature}")
            }
            ChooserError::Unavailable(reason) => write!(f, "the chooser is unavailable: {reason}"),
            ChooserError::Failed(reason) => write!(f, "the chooser failed: {reason}"),
        }
    }
}

impl std::error::Error for ChooserError {}

/// What a provider can do. A field is false unless the provider sets it.
///
/// The point of the list is the fallback: a request is given to a provider only when every
/// feature it names is true here, so a switch between implementations never silently drops
/// a filter, a label or a second path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChooserCapabilities {
    /// Indexed by [`ChooserMode::index`].
    pub modes: [bool; 6],
    /// Can show the request's patterns as a type menu.
    pub filters: bool,
    /// Can use [`FileTypeFilter::mime_types`]; patterns work without it.
    pub mime_filters: bool,
    /// Filters are advice only, and an answer outside them must be validated by the caller.
    pub filter_advice_only: bool,
    pub custom_places: bool,
    pub choices: bool,
    /// Can leave the parent usable while open.
    pub modeless: bool,
    /// Can force hidden files on, whatever the desktop's setting says.
    pub show_hidden: bool,
    /// Can start with an existing file selected (`StartAt::File`).
    pub start_at_file: bool,
    /// Can append a caller's default extension on save.
    pub default_extension: bool,
    /// Can ask before replacing an existing file.
    pub overwrite_prompt: bool,
    /// Can remember the folder and geometry under `state_key`.
    pub state_persistence: bool,
}

impl ChooserCapabilities {
    /// A provider that can do nothing; every mode false.
    pub fn none() -> Self {
        Self::default()
    }

    /// The same capabilities with `mode` set to `supported`.
    pub fn mode(mut self, mode: ChooserMode, supported: bool) -> Self {
        self.modes[mode.index()] = supported;
        self
    }

    /// The first feature of `request` this provider cannot honour, or `None` when it can
    /// run the request as asked.
    ///
    /// Order matters only for the message: the caller falls back for any reason. MIME types
    /// are not checked, because they narrow a filter that still works without them;
    /// persistence is not checked, because remembering a folder is an optimisation.
    pub fn missing(&self, request: &ChooserRequest) -> Option<&'static str> {
        if !self.modes[request.mode.index()] {
            return Some(mode_name(request.mode));
        }
        if !request.filters.is_empty() && !self.filters {
            return Some("file type filters");
        }
        if !request.extra_places.is_empty() && !self.custom_places {
            return Some("custom places");
        }
        if !request.choices.is_empty() && !self.choices {
            return Some("extra choices");
        }
        if !request.modal && !self.modeless {
            return Some("a modeless dialog");
        }
        if request.show_hidden && !self.show_hidden {
            return Some("forced hidden files");
        }
        if matches!(request.start, StartAt::File(_)) && !self.start_at_file {
            return Some("an initial file");
        }
        if request
            .save
            .as_ref()
            .is_some_and(|save| save.default_extension.is_some())
            && !self.default_extension
        {
            return Some("a default extension");
        }
        None
    }
}

fn mode_name(mode: ChooserMode) -> &'static str {
    match mode {
        ChooserMode::OpenFile => "opening a file",
        ChooserMode::OpenFiles => "opening several files",
        ChooserMode::SelectFolder => "choosing a folder",
        ChooserMode::SelectFolders => "choosing several folders",
        ChooserMode::SaveFile => "saving a file",
        ChooserMode::SaveFiles => "saving several files",
    }
}

/// Which implementation answers a request.
///
/// `Auto` is the default: the native provider when it can honour the whole request, the
/// drawn one otherwise. The other two exist so a caller can require one shape of dialog —
/// a sandbox needs the portal, a visual review needs the drawn picker — without changing
/// the request or the result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChooserPreference {
    /// Native if it can, drawn if it cannot.
    #[default]
    Auto,
    /// Only the platform's own dialog; an unsupported request is an error.
    NativeOnly,
    /// Only the drawn picker, on every platform.
    DrawnOnly,
}

/// Which implementation produced an answer, for logs and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooserSource {
    Native,
    Drawn,
}

/// One answer, with the implementation that gave it.
#[derive(Clone, Debug)]
pub struct ChooserAnswer {
    pub result: Result<ChooserSelection, ChooserError>,
    pub source: ChooserSource,
}

impl ChooserAnswer {
    pub fn native(result: Result<ChooserSelection, ChooserError>) -> Self {
        Self {
            result,
            source: ChooserSource::Native,
        }
    }

    pub fn drawn(result: Result<ChooserSelection, ChooserError>) -> Self {
        Self {
            result,
            source: ChooserSource::Drawn,
        }
    }
}

/// The chooser that is running, by the host's bookkeeping.
///
/// Stable for the chooser's whole life; a backend echoes it back with the answer, and the
/// host uses it to find the one callback that may be called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChooserId(u64);

impl ChooserId {
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native() -> ChooserCapabilities {
        ChooserCapabilities {
            filters: true,
            custom_places: true,
            choices: true,
            modeless: false,
            show_hidden: true,
            start_at_file: true,
            default_extension: true,
            overwrite_prompt: true,
            state_persistence: true,
            ..ChooserCapabilities::none()
                .mode(ChooserMode::OpenFile, true)
                .mode(ChooserMode::SelectFolder, true)
        }
    }

    #[test]
    fn a_supported_request_asks_nothing_more_of_the_provider() {
        let request = ChooserRequest::open_file("/tmp")
            .filter(FileTypeFilter::new("Images", vec!["*.png".into()]))
            .choice(ChooserChoice::boolean("keep", "Keep", true));
        assert_eq!(native().missing(&request), None);
    }

    #[test]
    fn each_unmet_feature_names_itself_and_asks_for_a_fallback() {
        let mut capabilities = native();
        capabilities.modes[ChooserMode::OpenFiles.index()] = false;
        let request = ChooserRequest::open_files("/tmp");
        assert_eq!(capabilities.missing(&request), Some("opening several files"));

        let folder = ChooserRequest::select_folder("/tmp").place(ChooserPlace::new("Home", "/"));
        capabilities.custom_places = false;
        assert_eq!(capabilities.missing(&folder), Some("custom places"));

        let skipped = ChooserRequest::open_file("/tmp").show_hidden(true);
        capabilities.show_hidden = false;
        assert_eq!(capabilities.missing(&skipped), Some("forced hidden files"));
        assert!(ChooserError::Unsupported("forced hidden files").offers_fallback());
    }

    #[test]
    fn a_filter_is_checked_but_its_mime_hints_are_not() {
        let mut capabilities = native();
        capabilities.filters = false;
        let request = ChooserRequest::open_file("/tmp").filter(FileTypeFilter::new(
            "Images",
            vec!["*.png".into()],
        ).mime_types(vec!["image/png".into()]));
        assert_eq!(capabilities.missing(&request), Some("file type filters"));

        capabilities.filters = true;
        assert_eq!(capabilities.missing(&request), None);
    }

    #[test]
    fn a_save_without_options_is_rejected_before_a_provider_sees_it() {
        let request = ChooserRequest {
            mode: ChooserMode::SaveFile,
            save: None,
            ..ChooserRequest::open_file("/tmp")
        };
        assert!(request.validate().is_err());
        assert!(ChooserRequest::save_file("/tmp/out.png").validate().is_ok());
    }

    #[test]
    fn a_default_filter_must_name_a_filter() {
        let request = ChooserRequest::open_file("/tmp").default_filter(2);
        assert!(request.validate().is_err());
    }

    #[test]
    fn a_save_of_several_files_must_name_each_of_them() {
        assert!(
            ChooserRequest::save_files("/tmp", Vec::new())
                .validate()
                .is_err(),
            "SaveFiles without names asks for a folder the caller already has"
        );
        assert!(
            ChooserRequest::save_files("/tmp", vec!["a.png".into(), "b.png".into()])
                .validate()
                .is_ok()
        );
        assert!(
            ChooserRequest::save_files("/tmp", vec!["sub/a.png".into()])
                .validate()
                .is_err(),
            "a name carrying a path would escape the folder the user picks"
        );
    }

    #[test]
    fn a_mime_filter_names_an_exact_type_or_a_kind() {
        let kind = FileTypeFilter::new("Images", vec!["*.png".into()])
            .mime_types(vec!["image/*".into()]);
        assert!(kind.matches_mime("image/png"));
        assert!(!kind.matches_mime("text/plain"));
        let exact = FileTypeFilter::new("Text", Vec::new()).mime_types(vec!["text/plain".into()]);
        assert!(exact.matches_mime("TEXT/PLAIN"), "types compare as tokens");
    }
}
