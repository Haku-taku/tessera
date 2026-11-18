//! A file picker as one dialog: the rows, the places, and the words around them.
//!
//! The picker reads nothing itself. A [`ListingSource`] answers what a directory holds, a
//! [`Thumbnails`] handle answers what a picture looks like, and the app that opens the dialog
//! supplies both; this crate decides what the answer means, which rows are in view and what a
//! click does. That split is what lets the model be driven from a table in a test and lets a
//! picker on Windows browse a namespace the Linux one cannot see.
//!
//! [`Picker`] is the `App` a dialog surface runs. [`PickerState`] is the same picker without
//! the window, for an app that must fall back to showing it in a modal of its own: the surface
//! capability is not on every platform, and a picker must never disappear because a window
//! could not be opened.

mod chooser;
mod icons;
mod model;
mod view;

pub use chooser::{Chooser, ChooserOpened, drawn_capabilities};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ui_core::change::Change;
use ui_core::geom::Point;
use ui_core::{Id, Key, KeyChord, Modifiers, ShortcutId};
use ui_platform::chooser::{
    ChooserChoice, ChooserPlace, ChooserSelection, ChoiceKind, FileTypeFilter, OverwritePolicy,
};
use ui_platform::file_operation::{
    ConflictPolicy, DeleteMode, FileOperations, LinkKind, OperationRequest,
};
use ui_platform::file_icons::FileIcons;
use ui_platform::places::{Device, Place};
use ui_platform::search::{SearchRequest, SearchState, Searches};
use ui_platform::thumbnails::Thumbnails;
use ui_platform::uri_list;
use ui_platform::view::App;
use ui_platform::volumes::{Volumes, VolumesState};
use ui_platform::watch::{Watch, Watches};
use ui_platform::{Element, KeepAlive};
use ui_style::{Env, Style, StyleScope};
use ui_widgets::RowHeights;

use crate::model::{typed_path, Action, Model, RangeShape, Subfolders};

pub use crate::model::{FileFilter, ListingSource, Mode, ThumbnailPolicy};
/// The appearance a pack is resolved for; a picker follows its env's scope.
pub use ui_design::Appearance;

/// A pack family: one resolved style per appearance, as each kit's `pack_of` is.
pub type PackOf = fn(Appearance) -> &'static Style;

/// A pack family's engine-menu material for an appearance, as each kit's `menu_style_of` is.
///
/// The dialog's context menus are the engine's, and the engine asks the `App` for their look:
/// a picker drawing with a pack must name that pack's menu material or its menus keep the
/// engine's fixed default.
pub type MenuStyleOf = fn(Appearance) -> ui_platform::menu::MenuStyle;


/// The smallest dialog the drawn picker asks to be sized to, in logical units.
///
/// The width is below the three columns' floor: the list scrolls sideways there rather than
/// squeezing a name, which is what the narrow-panel rule already does. The height leaves the
/// toolbar, a header and a few rows.
pub const MIN_SIZE: (u32, u32) = (360, 240);

/// How long a dragged item must rest on a folder before it opens, as Finder's hold does.
///
/// A full second: at the half second this started with, a pointer crossing a folder on its
/// way somewhere else already sprang, and the user lost the folder they were aiming at.
const SPRING_DELAY: f64 = 1.0;

/// How close to a scroller's edge a drag has to come before it starts moving the content.
const DRAG_SCROLL_EDGE: f32 = 36.0;
/// The edge scroll's fastest speed, in points per second at the very edge.
const DRAG_SCROLL_SPEED: f32 = 1200.0;

/// The shortcut that accepts the selection, as `Enter` or the accept button.
pub const OPEN: ShortcutId = ShortcutId("picker.open");
/// The shortcut that closes without a choice, as `Escape`.
pub const CANCEL: ShortcutId = ShortcutId("picker.cancel");
/// The shortcut that goes to the parent directory.
pub const UP: ShortcutId = ShortcutId("picker.up");
/// The shortcut that moves the selection up one row of the view.
pub const PREVIOUS: ShortcutId = ShortcutId("picker.previous");
/// The shortcut that moves the selection down one row of the view.
pub const NEXT: ShortcutId = ShortcutId("picker.next");
/// The shortcut that moves the selection one cell left in the grid.
pub const PREVIOUS_COLUMN: ShortcutId = ShortcutId("picker.previous-column");
/// The shortcut that moves the selection one cell right in the grid.
pub const NEXT_COLUMN: ShortcutId = ShortcutId("picker.next-column");
/// The shortcut that selects the first row.
pub const FIRST: ShortcutId = ShortcutId("picker.first");
/// The shortcut that selects the last row.
pub const LAST: ShortcutId = ShortcutId("picker.last");
/// The shortcut that marks or unmarks the cursor's row.
pub const TOGGLE: ShortcutId = ShortcutId("picker.toggle");
/// The shortcut that marks every row.
pub const SELECT_ALL: ShortcutId = ShortcutId("picker.select-all");
/// The shortcut that drops every mark, leaving the cursor where it is.
pub const DESELECT: ShortcutId = ShortcutId("picker.deselect");
/// The shortcut that turns the address field into a typed path.
pub const PATH_ENTRY: ShortcutId = ShortcutId("picker.path");
/// The shortcut that focuses the search field.
pub const SEARCH: ShortcutId = ShortcutId("picker.search");
/// The shortcut that writes the selection to the clipboard as cut.
pub const CUT: ShortcutId = ShortcutId("picker.cut");
/// The shortcut that writes the selection to the clipboard as a copy.
pub const COPY: ShortcutId = ShortcutId("picker.copy");
/// The shortcut that renames the row under the cursor.
pub const RENAME: ShortcutId = ShortcutId("picker.rename");
/// The shortcut that moves the selection to the recycle bin or trash.
pub const TRASH: ShortcutId = ShortcutId("picker.trash");
/// The shortcut that deletes the selection for good, after a confirmation.
pub const DELETE: ShortcutId = ShortcutId("picker.delete");
/// The shortcut that creates a folder and starts naming it.
pub const NEW_FOLDER: ShortcutId = ShortcutId("picker.new-folder");
/// The shortcut that opens the row menu for the row the keyboard stands on.
pub const MENU: ShortcutId = ShortcutId("picker.menu");
/// The shortcut that puts back what the last job did.
pub const UNDO: ShortcutId = ShortcutId("picker.undo");
/// The shortcut that shows the selection's facts.
pub const PROPERTIES: ShortcutId = ShortcutId("picker.properties");

/// The search service as the picker sees it.
///
/// A trait so the rows can be driven from a table in a test, as [`ListingSource`] is.
pub trait SearchSource: Send + Sync {
    fn start(&self, request: &SearchRequest);
    fn cancel(&self);
    fn state(&self) -> SearchState;
}

impl SearchSource for Searches {
    fn start(&self, request: &SearchRequest) {
        Searches::start(self, request.clone());
    }

    fn cancel(&self) {
        Searches::cancel(self);
    }

    fn state(&self) -> SearchState {
        Searches::state(self)
    }
}

/// One live watch, as the picker sees it.
pub trait WatchHandle: Send {
    /// Whether something changed since the last answer; asking clears the flag.
    fn changed(&self) -> bool;
}

impl WatchHandle for Watch {
    fn changed(&self) -> bool {
        Watch::changed(self)
    }
}

/// The watch service as the picker sees it.
pub trait WatchSource: Send + Sync {
    fn watch(&self, path: &Path) -> Option<Box<dyn WatchHandle>>;
}

impl WatchSource for Watches {
    fn watch(&self, path: &Path) -> Option<Box<dyn WatchHandle>> {
        Watches::watch(self, path).map(|watch| Box::new(watch) as Box<dyn WatchHandle>)
    }
}

/// A search field the toolbar is showing.
pub(crate) struct SearchField {
    pub(crate) query: String,
    /// The field has asked for focus once, so a later click elsewhere keeps it.
    pub(crate) focus_requested: bool,
}

/// The address field, typed by the user.
pub(crate) struct PathEntry {
    pub(crate) text: String,
    pub(crate) focus_requested: bool,
    /// The last commit named nothing the filesystem could open.
    pub(crate) error: bool,
}

/// Which of the two views the rows are drawn in.
///
/// The model's selection is a row index either way; the view adds what one screen row means,
/// which is a single row in the list and a row of cells in the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    Rows,
    Grid,
}

/// One shortcut per letter, because a shortcut id is a `&'static str`: there is no wildcard
/// "any character" binding, so a type-ahead press has to spell out the key that carries it.
const TYPE_AHEAD: [(char, ShortcutId); 26] = [
    ('a', ShortcutId("picker.type-ahead/a")),
    ('b', ShortcutId("picker.type-ahead/b")),
    ('c', ShortcutId("picker.type-ahead/c")),
    ('d', ShortcutId("picker.type-ahead/d")),
    ('e', ShortcutId("picker.type-ahead/e")),
    ('f', ShortcutId("picker.type-ahead/f")),
    ('g', ShortcutId("picker.type-ahead/g")),
    ('h', ShortcutId("picker.type-ahead/h")),
    ('i', ShortcutId("picker.type-ahead/i")),
    ('j', ShortcutId("picker.type-ahead/j")),
    ('k', ShortcutId("picker.type-ahead/k")),
    ('l', ShortcutId("picker.type-ahead/l")),
    ('m', ShortcutId("picker.type-ahead/m")),
    ('n', ShortcutId("picker.type-ahead/n")),
    ('o', ShortcutId("picker.type-ahead/o")),
    ('p', ShortcutId("picker.type-ahead/p")),
    ('q', ShortcutId("picker.type-ahead/q")),
    ('r', ShortcutId("picker.type-ahead/r")),
    ('s', ShortcutId("picker.type-ahead/s")),
    ('t', ShortcutId("picker.type-ahead/t")),
    ('u', ShortcutId("picker.type-ahead/u")),
    ('v', ShortcutId("picker.type-ahead/v")),
    ('w', ShortcutId("picker.type-ahead/w")),
    ('x', ShortcutId("picker.type-ahead/x")),
    ('y', ShortcutId("picker.type-ahead/y")),
    ('z', ShortcutId("picker.type-ahead/z")),
];

/// The modifiers an arrow may carry: Shift extends and Ctrl moves the cursor alone.
const SHIFT_OR_CTRL: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: false,
    logo: false,
};

/// What a row's pointer gesture reported, with the modifiers the engine saw.
///
/// `Change::Activated` carries no modifiers, and a picker's selection needs Ctrl and Shift;
/// the row's own widget therefore reports this instead, and the picker reads the downcast.
pub(crate) struct RowGesture {
    /// The pressed row, which is also the anchor a drag extends from.
    pub index: usize,
    /// The row under the pointer while a drag is live; `None` for a click.
    pub drag_row: Option<usize>,
    pub modifiers: Modifiers,
    /// How the gesture completed; `None` while a drag is live.
    pub trigger: Option<RowTrigger>,
}

/// What completed a row gesture. A double click and Enter both open the row; a single
/// click only marks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowTrigger {
    /// A completed click, carrying its burst length: two presses open the row.
    Click { clicks: u32 },
    /// Enter or Space on the focused row.
    Key,
}

/// The decision a right drag asks for, once the target knows what landed on it.
pub(crate) struct DropMenu {
    /// Where the release landed, so the panel hangs from the pointer.
    pub(crate) at: Point,
    /// The folder that would receive the files.
    pub(crate) destination: PathBuf,
    /// The paths the drag carried.
    pub(crate) paths: Vec<PathBuf>,
}

/// A clipboard write a build must make: the engine owns the clipboard, and only the frame
/// holds the context that requests it.
pub(crate) struct ClipboardWrite {
    paths: Vec<PathBuf>,
    effect: ui_core::DropEffect,
}

/// The platform menu the picker asked for, in the two steps its ownership of the input needs.
///
/// The command only records it: the popup runs a loop of its own until the user dismisses it,
/// so the frame asks the host to run it after the frame presents, and a later build re-reads
/// the folder, which a shell verb may have changed.
enum ShellMenuAsk {
    /// The frame now building hands the pair to the host.
    Issue(Vec<PathBuf>, Point),
    /// The host has returned; the build after it reads the folder again.
    Returned,
}

/// What a right press stood for, and what its menu's commands act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MenuTarget {
    /// A row of the list, by index: its menu is about the selection.
    Row(usize),
    /// The list's background: the folder on screen.
    Folder,
    /// A sidebar row: a folder the list is not showing.
    Place { owner: Id, path: PathBuf },
}

/// The read a menu waits on before it opens.
///
/// The shell's rows join the menu the press asked for, and the read runs extension code on
/// the host's thread; the menu therefore opens a frame late rather than blocking the press.
struct MenuRead {
    at: Point,
    target: MenuTarget,
    ticket: u64,
    /// The selection the read describes; the menu is about these rows.
    paths: Vec<PathBuf>,
    /// When the wait ends, on the engine's clock. Once the menu is up the wait is over and
    /// the answer lands whenever the host wakes the window with it.
    deadline: f64,
    /// The menu went up without the rows, so the answer now joins it in place.
    opened: bool,
}

/// What a build that polled a menu read does with its answer.
pub(crate) enum MenuReadStep {
    /// The menu has not opened yet; open it at this point, about this target.
    Open { at: Point, target: MenuTarget },
    /// The menu is up, and the rows that arrived after it replace the ones it shows.
    Refresh,
}

/// The menu whose rows a late read replaces.
pub(crate) struct OpenMenu {
    pub(crate) owner: Id,
    pub(crate) target: MenuTarget,
}

/// The shell rows an open menu merges, with the selection they describe.
pub(crate) struct ShellRows {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) rows: Vec<ui_platform::ShellMenuRow>,
    /// Rows the shell draws itself, which only the native menu can show.
    pub(crate) omitted: usize,
}

/// How long a menu waits for the shell's rows before it opens without them.
///
/// The read runs extension code the picker cannot bound; a menu that waited on it could
/// never open, while one that opens late is only late.
const MENU_READ_WAIT: f64 = 0.2;

/// The row that became an in-place field, and everything the field must remember.
pub(crate) struct RenameField {
    /// The file being renamed. The field follows the path, so a listing that re-sorts under
    /// it moves the field with the row instead of renaming its neighbour.
    pub(crate) path: PathBuf,
    /// The buffer the engine reports whole, as every input reports its text.
    pub(crate) buffer: String,
    /// The engine's focus has been asked for, as a field that opens with a key must.
    focus_requested: bool,
    /// The field has taken the keyboard; the base name is selected on the frame after it has.
    select_pending: bool,
    /// The field has been focused at least once, so a later loss of focus commits.
    had_focus: bool,
    /// Why the last commit was refused, so the field can say it in place.
    pub(crate) error: Option<RenameError>,
}

/// Why a name could not be committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenameError {
    /// The platform cannot store the name, or it is empty.
    Invalid,
    /// Another entry in the folder already has it.
    Taken(String),
}

/// A drag that has come to rest on a folder, waiting out the spring-loading hold.
pub(crate) struct Spring {
    pub(crate) path: PathBuf,
    /// When the drag entered the folder, on the frame clock the build reads.
    pub(crate) since: f64,
}

/// Whether moving `source` to `destination` would change anything.
///
/// A source already in the destination is no move, and a folder dropped inside itself —
/// onto itself included — would take its own subtree with it; neither is a drop a target
/// should stand for, so the hover refuses them before the release does.
fn moves_into(source: &Path, destination: &Path) -> bool {
    source.parent() != Some(destination) && !destination.starts_with(source)
}

/// The facts a Properties panel shows, captured when it opened.
pub(crate) struct Properties {
    /// The single item's name, or the count's word when several are selected.
    pub(crate) name: Option<String>,
    pub(crate) count: usize,
    pub(crate) folder: bool,
    pub(crate) extension: Option<String>,
    pub(crate) location: PathBuf,
    /// The summed bytes of what is selected; a folder's own size is not known here.
    pub(crate) size: Option<u64>,
    pub(crate) modified: Option<std::time::Duration>,
}

/// The folder chooser a Move to / Copy to command opened: the job the answer would start,
/// the selection it acts on, and the folder it is browsing.
///
/// The selection is captured when the chooser opens, so the job acts on what was marked
/// when the command was chosen rather than on whatever a later frame marks.
pub(crate) struct Transfer {
    pub(crate) effect: ui_core::DropEffect,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) folder: PathBuf,
}

/// The one job the footer carries, as the worker last reported it.
#[derive(Clone)]
pub(crate) struct Running {
    pub(crate) id: ui_platform::OperationId,
    pub(crate) kind: ui_platform::OperationKind,
    pub(crate) done: u64,
    pub(crate) total: u64,
    pub(crate) bytes_done: u64,
    pub(crate) bytes_total: u64,
    /// The sources the job was started on, sorted for the band's list.
    pub(crate) items: Vec<PathBuf>,
    /// The item being worked on.
    pub(crate) current: Option<PathBuf>,
}

/// A started job's kind and the batch it runs on, kept until its report arrives.
struct Job {
    kind: ui_platform::OperationKind,
    items: Vec<PathBuf>,
}

/// A destination collision the worker is waiting on.
#[derive(Clone)]
pub(crate) struct ConflictAsk {
    pub(crate) conflict: ui_platform::Conflict,
    pub(crate) apply_all: bool,
    /// The Rename answer's field: `Some` while the user is naming the newcomer.
    pub(crate) rename: Option<String>,
}

/// The failures a finished job left, named with the reason each one failed.
pub(crate) struct Failures {
    pub(crate) items: Vec<(PathBuf, ui_platform::OpError)>,
}

/// One command a row or background menu offers. The id is stable, because it is what the
/// engine reports back when the row is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuCommand {
    Open,
    Undo,
    Cut,
    Copy,
    Paste,
    MoveTo,
    CopyTo,
    Rename,
    Trash,
    Delete,
    NewFolder,
    CreateLink,
    CreateHardLink,
    CopyPaths,
    Properties,
    ShellMenu,
    SelectAll,
    Invert,
    ShowHidden,
    Refresh,
}

impl MenuCommand {
    pub(crate) fn id(self) -> Id {
        let name = match self {
            Self::Open => "open",
            Self::Undo => "undo",
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::MoveTo => "move-to",
            Self::CopyTo => "copy-to",
            Self::Rename => "rename",
            Self::Trash => "trash",
            Self::Delete => "delete",
            Self::NewFolder => "new-folder",
            Self::CreateLink => "create-link",
            Self::CreateHardLink => "create-hard-link",
            Self::CopyPaths => "copy-paths",
            Self::Properties => "properties",
            Self::ShellMenu => "shell-menu",
            Self::SelectAll => "select-all",
            Self::Invert => "invert",
            Self::ShowHidden => "show-hidden",
            Self::Refresh => "refresh",
        };
        Id::of(("picker/menu", name))
    }

    pub(crate) fn from_id(id: Id) -> Option<Self> {
        const ALL: [MenuCommand; 20] = [
            MenuCommand::Open,
            MenuCommand::Undo,
            MenuCommand::Cut,
            MenuCommand::Copy,
            MenuCommand::Paste,
            MenuCommand::MoveTo,
            MenuCommand::CopyTo,
            MenuCommand::Rename,
            MenuCommand::Trash,
            MenuCommand::Delete,
            MenuCommand::NewFolder,
            MenuCommand::CreateLink,
            MenuCommand::CreateHardLink,
            MenuCommand::CopyPaths,
            MenuCommand::Properties,
            MenuCommand::ShellMenu,
            MenuCommand::SelectAll,
            MenuCommand::Invert,
            MenuCommand::ShowHidden,
            MenuCommand::Refresh,
        ];
        ALL.into_iter().find(|command| command.id() == id)
    }
}

impl RowTrigger {
    /// Whether this gesture acts on the row rather than merely marking it.
    pub(crate) fn opens(self) -> bool {
        match self {
            RowTrigger::Click { clicks } => clicks >= 2,
            RowTrigger::Key => true,
        }
    }
}

/// What the dialog answered.
///
/// The selection is the portable contract's: one path for a single mode, several for a
/// multi one, and the filter they were chosen under. A save answers the path it would
/// write. `Cancelled` is an answer, not a failure; the checked wrappers in
/// [`crate::Chooser`] turn it into [`ui_platform::chooser::ChooserError::Cancelled`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerOutcome {
    /// The user accepted these paths.
    Selected(ChooserSelection),
    /// The user closed the dialog without choosing.
    Cancelled,
}

/// Everything the picker cannot make up for itself: where to start, what to show it with, and
/// where its answers come from.
///
/// Clonable so a caller can keep one for the in-window fallback while the dialog window takes
/// the other: the listing source is shared, not copied.
#[derive(Clone)]
pub struct PickerOptions {
    pub source: Arc<dyn ListingSource>,
    pub start: PathBuf,
    pub mode: Mode,
    /// The pack family the dialog draws with; the opening app picks the design system, and
    /// the appearance comes from `env`'s [`StyleScope`].
    pub pack_of: PackOf,
    /// The same pack family's material for the engine's context menus, which the dialog opens
    /// over its rows; it follows the same appearance.
    pub menu_style_of: MenuStyleOf,
    /// The scope the dialog's messages resolve in; it must carry a language, and its
    /// appearance decides which pack the dialog draws with (light when it states none).
    pub env: Arc<Env>,
    /// The platform's places, in the order it named them.
    pub places: Vec<Place>,
    /// Places the caller added to the sidebar, in the order it named them.
    pub extra_places: Vec<ChooserPlace>,
    /// The platform's mounted volumes, in the order it named them.
    pub devices: Vec<Device>,
    /// The platform's volumes that can be mounted but are not; a picker without one draws
    /// no removable section.
    pub volumes: Option<Volumes>,
    /// The platform's thumbnails, when it has them.
    pub thumbnails: Option<Thumbnails>,
    /// The platform's file-type icons, when it has them.
    pub file_icons: Option<FileIcons>,
    /// The platform's recursive search; a picker without one draws no search control.
    pub searches: Option<Arc<dyn SearchSource>>,
    /// The platform's directory watch; a picker without one does not refresh itself.
    pub watches: Option<Arc<dyn WatchSource>>,
    /// The platform's file operations; a picker without one cannot carry a drop out, though
    /// the operating system still takes a row dragged into another window.
    pub file_operations: Option<FileOperations>,
    /// The platform's own context menu, when it has one; its rows join the row menu when the
    /// host can carry them faithfully, and the native popup stays one row of ours for a menu
    /// it cannot.
    pub shell_menu: Option<Arc<dyn ui_platform::ShellMenuHost>>,
    pub thumbnails_policy: ThumbnailPolicy,
    /// The filters the type menu offers, in order. Empty shows every file.
    pub filters: Vec<FileTypeFilter>,
    pub default_filter: Option<usize>,
    /// An existing file to select once the listing lands; `None` selects nothing.
    pub start_at_file: Option<PathBuf>,
    /// The name a save mode starts with; `None` derives it from `start` when that names a
    /// file.
    pub save_name: Option<String>,
    /// The names a `SaveFiles` mode writes into the folder the user picks.
    pub save_names: Vec<String>,
    pub default_extension: Option<String>,
    /// What a save does when the chosen name exists.
    pub overwrite: OverwritePolicy,
    /// Whether a save accepts only the active filter's names.
    pub strict_types: bool,
    /// The extra controls the dialog shows beside its own.
    pub choices: Vec<ChooserChoice>,
    /// Start with hidden files shown.
    pub show_hidden: bool,
    /// The accept and cancel buttons' labels, when the caller wants its own words.
    pub accept_label: Option<String>,
    pub cancel_label: Option<String>,
}

impl PickerOptions {
    /// The required pieces; every optional service and the filter have a working default.
    pub fn new(
        source: impl ListingSource + 'static,
        start: impl Into<PathBuf>,
        pack_of: PackOf,
        menu_style_of: MenuStyleOf,
        env: Arc<Env>,
    ) -> Self {
        Self {
            source: Arc::new(source),
            start: start.into(),
            mode: Mode::OpenFile,
            pack_of,
            menu_style_of,
            env,
            places: Vec::new(),
            extra_places: Vec::new(),
            devices: Vec::new(),
            volumes: None,
            thumbnails: None,
            file_icons: None,
            searches: None,
            watches: None,
            file_operations: None,
            shell_menu: None,
            thumbnails_policy: ThumbnailPolicy::default(),
            filters: Vec::new(),
            default_filter: None,
            start_at_file: None,
            save_name: None,
            save_names: Vec::new(),
            default_extension: None,
            overwrite: OverwritePolicy::Prompt,
            strict_types: false,
            choices: Vec::new(),
            show_hidden: false,
            accept_label: None,
            cancel_label: None,
        }
    }

    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    pub fn places(mut self, places: Vec<Place>) -> Self {
        self.places = places;
        self
    }

    /// Add the caller's own sidebar entries, after the platform's places.
    pub fn extra_places(mut self, places: Vec<ChooserPlace>) -> Self {
        self.extra_places = places;
        self
    }

    /// Add the caller's own controls.
    pub fn choices(mut self, choices: Vec<ChooserChoice>) -> Self {
        self.choices = choices;
        self
    }

    pub fn devices(mut self, devices: Vec<Device>) -> Self {
        self.devices = devices;
        self
    }

    pub fn volumes(mut self, volumes: Option<Volumes>) -> Self {
        self.volumes = volumes;
        self
    }

    pub fn thumbnails(mut self, thumbnails: Option<Thumbnails>) -> Self {
        self.thumbnails = thumbnails;
        self
    }

    pub fn file_icons(mut self, file_icons: Option<FileIcons>) -> Self {
        self.file_icons = file_icons;
        self
    }

    /// Offer a search field, answered by the platform's walk.
    pub fn searches(mut self, searches: impl SearchSource + 'static) -> Self {
        self.searches = Some(Arc::new(searches));
        self
    }

    /// Refresh the rows when the platform reports a change to the directory.
    pub fn watches(mut self, watches: impl WatchSource + 'static) -> Self {
        self.watches = Some(Arc::new(watches));
        self
    }

    /// Let the dialog move and copy files by dragging them.
    pub fn file_operations(mut self, operations: FileOperations) -> Self {
        self.file_operations = Some(operations);
        self
    }

    /// Offer the platform's own context menu as one extra row of the row menu.
    pub fn shell_menu(mut self, host: Arc<dyn ui_platform::ShellMenuHost>) -> Self {
        self.shell_menu = Some(host);
        self
    }

    pub fn thumbnails_policy(mut self, policy: ThumbnailPolicy) -> Self {
        self.thumbnails_policy = policy;
        self
    }

    /// Add one filter, after any already added.
    pub fn filter(mut self, filter: FileTypeFilter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Replace the filter list; the active one is the first unless `default_filter` says
    /// otherwise.
    pub fn filters(mut self, filters: Vec<FileTypeFilter>) -> Self {
        self.filters = filters;
        self
    }

    pub fn default_filter(mut self, index: usize) -> Self {
        self.default_filter = Some(index);
        self
    }

    pub fn save_name(mut self, name: impl Into<String>) -> Self {
        self.save_name = Some(name.into());
        self
    }

    pub fn default_extension(mut self, extension: impl Into<String>) -> Self {
        self.default_extension = Some(extension.into());
        self
    }

    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
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
}

/// The picker without a window: the model, the services it draws from, and what a click means.
///
/// An app that cannot open a dialog surface puts this in a modal of its own and calls
/// [`view`](Self::view), [`changed`](Self::changed) and [`shortcut`](Self::shortcut) from its
/// own implementations of the same `App` methods. [`Picker`] is that wiring for a dialog
/// window.
pub struct PickerState {
    pub(crate) model: Model,
    pub(crate) pack: &'static Style,
    /// The pack's engine-menu material, asked for every frame so a theme change reaches it.
    pub(crate) menu_style_of: MenuStyleOf,
    pub(crate) env: Arc<Env>,
    pub(crate) places: Vec<Place>,
    pub(crate) extra_places: Vec<ChooserPlace>,
    pub(crate) devices: Vec<Device>,
    pub(crate) volumes: Option<Volumes>,
    /// The volume a click asked to mount, while its worker is bringing it up.
    pub(crate) mounting: Option<String>,
    /// The last mount attempt failed; the footer says so until the next one.
    pub(crate) mount_failed: bool,
    pub(crate) thumbnails: Option<Thumbnails>,
    pub(crate) file_icons: Option<FileIcons>,
    pub(crate) policy: ThumbnailPolicy,
    pub(crate) accept_label: Option<String>,
    pub(crate) cancel_label: Option<String>,
    /// The caller's own controls and the value each one currently holds.
    pub(crate) choices: Vec<ChooserChoice>,
    pub(crate) choice_values: Vec<String>,
    /// Whether the type menu is open, and which choice's menu is.
    pub(crate) filter_menu: bool,
    pub(crate) choice_menu: Option<usize>,
    /// Whether the caller's extra controls are expanded above the footer.
    pub(crate) options_open: bool,
    /// The save names a Replace question is waiting on, with the name that clashes.
    pub(crate) overwrite_confirm: Option<(Vec<PathBuf>, String)>,
    /// The file operations this dialog may start, when the caller supplied them.
    pub(crate) file_operations: Option<FileOperations>,
    /// The platform's own context menu, when the caller supplied one.
    pub(crate) shell_menu: Option<Arc<dyn ui_platform::ShellMenuHost>>,
    /// The service's revision as of the last frame, so a finished job refreshes the
    /// folders it touched exactly once.
    operations_seen: u64,
    /// The folders operations may have changed, invalidated when one finishes.
    touched: Vec<PathBuf>,
    /// The rows a drag that began here carries, kept for the move a Wayland source must
    /// finish after the drop.
    drag_paths: Vec<PathBuf>,
    /// The decision menu a right drag opened, waiting for an answer.
    pub(crate) drop_menu: Option<DropMenu>,
    /// The full name a hovered row's cut label stands for, drawn as a tooltip.
    pub(crate) name_tooltip: Option<(Id, String)>,
    /// A drag entered a folder row; the next build stamps when the hold began.
    spring_pending: Option<PathBuf>,
    /// The live hold: the folder and the frame time the drag entered it.
    pub(crate) spring: Option<Spring>,
    /// The paths the clipboard holds as cut; their rows dim until a paste consumes them.
    pub(crate) cut: Vec<PathBuf>,
    /// A clipboard write the next build makes, because only a frame holds a `Cx`.
    clipboard: Option<ClipboardWrite>,
    /// The one job the footer's band carries.
    pub(crate) running: Option<Running>,
    /// Every job still running, keyed by the id its events carry: its kind and its batch.
    jobs: std::collections::HashMap<ui_platform::OperationId, Job>,
    /// A new folder was asked for; the build names it, because the name is a message.
    pub(crate) new_folder: bool,
    /// A collision the worker waits on, with the panel's own state.
    pub(crate) conflict: Option<ConflictAsk>,
    /// The failures of the last finished job, shown until dismissed.
    pub(crate) failures: Option<Failures>,
    /// A permanent delete waiting for its confirmation.
    pub(crate) delete_ask: Option<Vec<PathBuf>>,
    /// The row being renamed in place, if one is.
    pub(crate) rename: Option<RenameField>,
    /// A context menu was asked for: where the press landed and what it stood for.
    pub(crate) context_menu: Option<(Point, MenuTarget)>,
    /// The Menu key asked for the cursor row's menu, which the build anchors to the row.
    pub(crate) menu_key: bool,
    /// The folder a menu's Paste and link commands land in, captured when the menu opened.
    pub(crate) menu_destination: PathBuf,
    /// Where the open menu was anchored, which is where a native menu must appear.
    pub(crate) menu_point: Point,
    /// The platform menu between its command and the build that re-reads the folder.
    shell_menu_ask: Option<ShellMenuAsk>,
    /// A menu held back until the shell's rows are in.
    menu_read: Option<MenuRead>,
    /// The menu the rows above replace when the read answers late.
    pub(crate) menu_open: Option<OpenMenu>,
    /// The shell rows the open menu merges, when its read was faithful.
    pub(crate) shell_rows: Option<ShellRows>,
    /// The shell command each of the open menu's rows runs.
    pub(crate) shell_tokens: std::collections::HashMap<Id, u32>,
    /// Commands the platform menu finished, as of the last frame.
    shell_invocations: u64,
    /// A text copy the next build hands to `Cx`.
    clipboard_text: Option<String>,
    /// Where the next clipboard paste lands, captured when the paste was asked for.
    paste_destination: Option<PathBuf>,
    /// Asked the platform for the clipboard; the answer arrives as `Change::Paste`.
    paste_requested: bool,
    /// The job a cut paste started, so its report can consume the dimming.
    cut_paste: Option<ui_platform::OperationId>,
    /// The inverses of this session's finished jobs, newest last.
    pub(crate) undo: ui_platform::UndoJournal,
    /// The job an Undo started, so its report is not journaled as a new action.
    undoing: Option<ui_platform::OperationId>,
    /// The copies an Undo would remove, waiting for the confirmation.
    pub(crate) undo_ask: Option<Vec<PathBuf>>,
    /// The facts panel for what was selected when it opened.
    pub(crate) properties: Option<Properties>,
    /// The folder chooser a Move to / Copy to command opened.
    pub(crate) transfer: Option<Transfer>,
    /// Which view the list region draws, and the cells per grid row last frame measured.
    pub(crate) layout: Layout,
    pub(crate) columns: usize,
    /// Row heights the scroll window measures; the rows are one height, so this mostly keeps
    /// the scroll container's contract rather than tracking wrapping.
    pub(crate) heights: RowHeights,
    /// The grid's own table: its rows are one cell tall, and a view keeps its own scroller,
    /// so the two tables never describe the same rows.
    pub(crate) grid_heights: RowHeights,
    /// The next build must scroll the selected row into view; a keyboard move sets it, a
    /// directory change clears it.
    pub(crate) reveal: bool,
    /// An action changed what the next frame must draw.
    pub(crate) dirty: bool,
    /// The save name field has asked for focus once, so a later click elsewhere keeps it.
    pub(crate) name_focus_requested: bool,
    /// The crumb whose subfolder menu is open; the panel is anchored to its arrow.
    pub(crate) crumb_menu: Option<PathBuf>,
    /// The search service, when the caller offered one, and the field when it is open.
    searches: Option<Arc<dyn SearchSource>>,
    pub(crate) search: Option<SearchField>,
    /// The watch service, the path it is watching, and the live watch.
    watches: Option<Arc<dyn WatchSource>>,
    watch: Option<Box<dyn WatchHandle>>,
    watched: Option<PathBuf>,
    /// The typed path field, when the address bar is open.
    pub(crate) path_entry: Option<PathEntry>,
    /// An answer produced while building (a field's Enter); the dialog delivers it next.
    pending: Option<PickerOutcome>,
    /// Claims on the services' caches: dropping them lets the idle sweep clear what this
    /// dialog read, which is right once it has closed and wrong while it merely sits idle.
    _cache_claims: Vec<KeepAlive>,
}

impl PickerState {
    pub fn new(options: PickerOptions) -> Self {
        let save_name = options.save_name.or_else(|| {
            options
                .mode
                .is_save()
                .then(|| {
                    options
                        .start
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_string)
                })
                .flatten()
        });
        // The dialog claims its services' caches for as long as it lives, so a minute of
        // idleness does not evict a listing or an icon and force the worker to make it again.
        let cache_claims: Vec<KeepAlive> = options
            .source
            .keep_alive()
            .into_iter()
            .chain(options.thumbnails.as_ref().map(Thumbnails::keep_alive))
            .chain(options.file_icons.as_ref().map(FileIcons::keep_alive))
            .collect();
        let mut model = Model::new(options.source, options.start, options.mode);        model.set_filters(options.filters, options.default_filter);
        model.set_default_extension(options.default_extension);
        model.set_save_names(options.save_names);
        model.set_save_policy(options.overwrite, options.strict_types);
        if let Some(name) = save_name {
            model.set_answer(name);
        }
        if let Some(path) = options.start_at_file {
            model.select_when_listed(path);
        }
        model.set_show_hidden(options.show_hidden);
        let choice_values = options
            .choices
            .iter()
            .map(|choice| choice.initial.clone())
            .collect();
        // A dialog is its own surface: it cannot read the opening window's env, so the
        // appearance is whatever scope the caller put on this one. Resolved once, because a
        // picker keeps one env for its whole life.
        let pack = (options.pack_of)(StyleScope::of(&options.env).appearance);
        // A command the host ran before this dialog opened has nothing to refresh here.
        let shell_invocations = options
            .shell_menu
            .as_ref()
            .map_or(0, |host| host.invocations());
        let state = Self {
            model,
            pack,
            menu_style_of: options.menu_style_of,
            env: options.env,
            places: options.places,
            extra_places: options.extra_places,
            devices: options.devices,
            volumes: options.volumes,
            mounting: None,
            mount_failed: false,
            thumbnails: options.thumbnails,
            file_icons: options.file_icons,
            policy: options.thumbnails_policy,
            accept_label: options.accept_label,
            cancel_label: options.cancel_label,
            choices: options.choices,
            choice_values,
            filter_menu: false,
            choice_menu: None,
            options_open: false,
            overwrite_confirm: None,
            operations_seen: options
                .file_operations
                .as_ref()
                .map_or(0, FileOperations::revision),
            file_operations: options.file_operations,
            shell_menu: options.shell_menu,
            touched: Vec::new(),
            drag_paths: Vec::new(),
            drop_menu: None,
            spring_pending: None,
            spring: None,
            cut: Vec::new(),
            clipboard: None,
            running: None,
            jobs: std::collections::HashMap::new(),
            new_folder: false,
            conflict: None,
            failures: None,
            delete_ask: None,
            name_tooltip: None,
            rename: None,
            context_menu: None,
            menu_key: false,
            menu_destination: PathBuf::new(),
            menu_point: Point::ZERO,
            shell_menu_ask: None,
            menu_read: None,
            menu_open: None,
            shell_rows: None,
            shell_tokens: std::collections::HashMap::new(),
            shell_invocations,
            clipboard_text: None,
            paste_destination: None,
            paste_requested: false,
            cut_paste: None,
            undo: ui_platform::UndoJournal::default(),
            undoing: None,
            undo_ask: None,
            properties: None,
            transfer: None,
            layout: Layout::Rows,
            columns: 1,
            heights: RowHeights::new(view::ROW_HEIGHT),
            grid_heights: RowHeights::new(view::GRID_ROW_PITCH),
            reveal: false,
            dirty: false,
            name_focus_requested: false,
            crumb_menu: None,
            searches: options.searches,
            search: None,
            watches: options.watches,
            watch: None,
            watched: None,
            path_entry: None,
            pending: None,
            _cache_claims: cache_claims,
        };
        // The service was built with the Host, possibly long before this dialog opened; a
        // stick inserted since then is not in the answer it holds.
        if let Some(volumes) = &state.volumes {
            volumes.refresh();
        }
        state
    }

    /// What accept would answer now, for a caller that draws its own accept state.
    pub fn selection(&self) -> Option<ChooserSelection> {
        let (paths, filter) = self.model.accepted()?;
        Some(ChooserSelection {
            paths,
            filter,
            choices: self.choice_values(),
        })
    }

    fn choice_values(&self) -> Vec<(String, String)> {
        self.choices
            .iter()
            .zip(&self.choice_values)
            .map(|(choice, value)| (choice.id.clone(), value.clone()))
            .collect()
    }

    /// Whether the crumb naming `segment` shows its arrow.
    ///
    /// Every crumb before the last is proved to have a child by the next one; the last shows
    /// an arrow only when its folder has a subfolder the listing can name. The menu answers to
    /// the same rule, so a visible arrow and an open menu cannot disagree.
    pub(crate) fn crumb_has_children(&self, segment: &Path) -> bool {
        if segment != self.model.path() {
            return true;
        }
        matches!(self.model.subfolders(segment), Subfolders::Ready(dirs) if !dirs.is_empty())
    }

    /// The folder whose subfolder menu is showing, once it still heads a crumb on the trail.
    ///
    /// Navigation is the one thing that can leave the menu anchored to a segment that no
    /// longer exists, so the menu is read through this rather than from the field.
    pub(crate) fn open_crumb_menu(&mut self) -> Option<PathBuf> {
        let menu = self.crumb_menu.clone()?;
        let alive = self.model.path().ancestors().any(|segment| segment == menu)
            && self.crumb_has_children(&menu);
        if !alive {
            self.crumb_menu = None;
        }
        alive.then_some(menu)
    }

    /// Build the picker's tree at `width`×`height` logical units.
    ///
    /// The clickable regions are elements, not declarations: a `Decl` has nowhere to report
    /// a click, so a tree built only from declarations paints and answers the keyboard's
    /// shortcuts but never the pointer.
    pub fn view(
        &mut self,
        cx: &mut ui_core::cx::Cx<'_>,
        width: f32,
        height: f32,
    ) -> Box<dyn Element> {
        // A field's Enter and Escape are node shortcuts, which the caret would otherwise
        // swallow; the engine reports them here before the frame is built.
        self.field_shortcut(cx);
        self.dismiss_path_entry(cx);
        self.run_spring(cx);
        self.apply_watch();
        self.sync_watch();
        self.apply_volumes();
        self.apply_search();
        self.model.poll();
        self.poll_operations();
        // The clipboard belongs to the platform, and the request names the frame that asks.
        if let Some(write) = self.clipboard.take() {
            cx.copy_files(&write.paths, write.effect);
        }
        if let Some(text) = self.clipboard_text.take() {
            cx.copy_text(text);
        }
        if std::mem::take(&mut self.paste_requested) {
            cx.request_paste();
        }
        // The shell's menu belongs to the platform's popup, which must not run inside a
        // build; the host runs this once the frame has presented.
        match self.shell_menu_ask.take() {
            Some(ShellMenuAsk::Issue(paths, at)) => {
                cx.request_shell_menu(paths, at);
                self.shell_menu_ask = Some(ShellMenuAsk::Returned);
            }
            Some(ShellMenuAsk::Returned) => {
                // A shell verb may have changed the folder; the read it may have invalidated
                // starts again, and reading the clock asks for the frame that starts it.
                self.dirty = true;
                self.model.retry();
                cx.now();
            }
            None => {}
        }
        self.rename_focus(cx);
        self.key_menu(cx);
        // A save opens onto its name, the way a platform's save dialog does: the field takes
        // focus and its buffer is editable without a click. Asked once, so a later click
        // elsewhere is not pulled back.
        if self.model.mode() == Mode::SaveFile && !self.name_focus_requested {
            self.name_focus_requested = true;
            cx.request_focus(view::answer_id());
        }
        if let Some(entry) = &mut self.path_entry
            && entry.focus_requested
        {
            entry.focus_requested = false;
            cx.request_focus(view::path_entry_id());
        }
        if let Some(search) = &mut self.search
            && search.focus_requested
        {
            search.focus_requested = false;
            cx.request_focus(view::search_id());
        }
        self.reveal_selection(cx);
        // Before the build: the list windows its rows from the offset, and a request set
        // after it would move only the next frame's rows.
        self.drag_autoscroll(cx);
        let mut widgets = ui_widgets::Cx::new(cx, self.env.clone());
        view::build(self, &mut widgets, width, height)
    }

    /// An answer a field's Enter produced while building; the dialog delivers it next.
    ///
    /// [`Picker`] drains this itself. An app drawing [`PickerState`] in a modal of its own
    /// must take it after `view` and answer as it answers `changed`/`shortcut`.
    pub fn take_outcome(&mut self) -> Option<PickerOutcome> {
        self.pending.take()
    }

    /// A node shortcut reported by the field that owns it.
    fn field_shortcut(&mut self, cx: &mut ui_core::cx::Cx<'_>) {
        let Some((owner, shortcut)) = cx.shortcut() else {
            return;
        };
        // A modal question takes the keyboard: a field behind it must not answer Enter,
        // and the conflict's own name field is the one that belongs to the question.
        if self.modal_open() && owner != view::conflict_name_id() {
            return;
        }
        if owner == view::path_entry_id() {
            self.dirty = true;
            if shortcut == OPEN {
                self.commit_path();
            } else if shortcut == CANCEL {
                self.path_entry = None;
            }
        } else if owner == view::search_id() {
            self.dirty = true;
            if shortcut == CANCEL {
                self.close_search();
            } else if shortcut == OPEN {
                self.pending = self.accept();
                self.leave_search_if_navigated();
            }
        } else if owner == view::answer_id() && shortcut == OPEN {
            self.pending = self.accept();
        } else if owner == view::rename_id() {
            self.dirty = true;
            if shortcut == OPEN {
                self.commit_rename();
            } else if shortcut == CANCEL {
                self.cancel_rename();
            }
        } else if owner == view::conflict_name_id() && shortcut == OPEN {
            self.answer_conflict_named();
        }
    }

    /// A press anywhere but the path field ends the edit without committing it: the field is
    /// a scope of its own, and a click elsewhere is the native address bar's way out.
    fn dismiss_path_entry(&mut self, cx: &mut ui_core::cx::Cx<'_>) {
        let Some(entry) = &self.path_entry else {
            return;
        };
        // The click that just opened the field is the one press it must not answer.
        if entry.focus_requested
            || cx.pressed().is_none()
            || cx.pressed_within(view::path_entry_id())
        {
            return;
        }
        self.path_entry = None;
        self.dirty = true;
    }

    /// Re-read the directory when the platform reports that something inside it changed.
    fn apply_watch(&mut self) {
        let Some(watch) = &self.watch else {
            return;
        };
        if watch.changed() && self.search.is_none() {
            self.model.refresh();
            self.dirty = true;
        }
    }

    /// A removable volume opened: the worker answers with the directory it was mounted at.
    fn apply_volumes(&mut self) {
        let Some(volumes) = &self.volumes else {
            return;
        };
        let outcomes = volumes.take_mounted();
        if outcomes.is_empty() {
            return;
        }
        self.mounting = None;
        self.dirty = true;
        for outcome in outcomes {
            match outcome {
                Ok(path) => {
                    self.mount_failed = false;
                    self.navigate(path);
                }
                Err(_reason) => self.mount_failed = true,
            }
        }
    }

    /// The volume a click named, when the list still offers it.
    fn mountable_clicked(&self, id: Id) -> Option<String> {
        let volumes = self.volumes.as_ref()?;
        let VolumesState::Ready(list) = volumes.state() else {
            return None;
        };
        list.iter()
            .find(|volume| view::mountable_id(&volume.id) == id)
            .map(|volume| volume.id.clone())
    }

    /// Watch the directory on screen; a navigation moves the watch with it.
    fn sync_watch(&mut self) {
        let Some(watches) = &self.watches else {
            return;
        };
        let path = self.model.path();
        if self.watched.as_deref() == Some(path) {
            return;
        }
        self.watch = watches.watch(path);
        self.watched = Some(path.to_path_buf());
    }

    /// Move the platform's matches into the rows.
    fn apply_search(&mut self) {
        let Some(searches) = &self.searches else {
            return;
        };
        if self.search.is_none() {
            return;
        }
        match searches.state() {
            SearchState::Idle => {}
            SearchState::Running(entries) => self.model.show_search(entries, false),
            SearchState::Done(entries) => self.model.show_search(entries, true),
            SearchState::Failed(reason) => self.model.search_failed(reason),
        }
    }

    /// Leave a search without reading a directory: a navigation supplies the rows.
    fn stop_search(&mut self) {
        if let Some(searches) = &self.searches {
            searches.cancel();
        }
        self.search = None;
    }

    /// Leave a search and put the directory back on screen.
    fn close_search(&mut self) {
        // Restoring the directory first: `end_search` only acts while the rows are a
        // search's, and stopping the walk already gives that up.
        self.model.end_search();
        self.stop_search();
        self.reveal = false;
    }

    /// A row activation may have navigated out of the search; the field goes with it.
    fn leave_search_if_navigated(&mut self) {
        if self.search.is_some() && !self.model.searching() {
            self.stop_search();
        }
    }

    /// Navigate, leaving the search and the typed path behind.
    fn navigate(&mut self, path: PathBuf) {
        self.stop_search();
        self.path_entry = None;
        self.mount_failed = false;
        self.model.navigate(path);
        self.reveal = false;
    }

    /// Turn the address slot into the path field, starting from the folder on screen.
    fn open_path_entry(&mut self) {
        // The address field replaces the trail, so a search's field goes with it; closing
        // the search puts the directory back on screen behind the field.
        self.close_search();
        self.crumb_menu = None;
        self.path_entry = Some(PathEntry {
            text: self.model.path().display().to_string(),
            focus_requested: true,
            error: false,
        });
        self.dirty = true;
    }

    /// Open the search field, or focus it when it is already open.
    fn open_search(&mut self) {
        if self.model.mode().is_save() || self.searches.is_none() {
            return;
        }
        self.path_entry = None;
        self.crumb_menu = None;
        match &mut self.search {
            Some(field) => field.focus_requested = true,
            None => {
                self.search = Some(SearchField {
                    query: String::new(),
                    focus_requested: true,
                })
            }
        }
        self.dirty = true;
    }

    /// The field reported its whole buffer; the walk starts over from the new query.
    fn set_search_query(&mut self, query: String) {
        let Some(field) = self.search.as_mut() else {
            return;
        };
        field.query = query;
        self.restart_search();
        self.dirty = true;
    }

    /// Ask the platform for the current query under the directory on screen.
    fn restart_search(&mut self) {
        let Some(searches) = &self.searches else {
            return;
        };
        let Some(field) = &self.search else {
            return;
        };
        let query = field.query.trim();
        if query.is_empty() {
            searches.cancel();
            self.model.end_search();
            return;
        }
        searches.start(
            &SearchRequest::new(self.model.path().to_path_buf(), query)
                .show_hidden(self.model.show_hidden()),
        );
    }

    /// Commit the typed path: a directory navigates, a file is selected in its parent.
    fn commit_path(&mut self) {
        let Some(text) = self.path_entry.as_ref().map(|entry| entry.text.clone()) else {
            return;
        };
        self.dirty = true;
        let input = text.trim();
        if input.is_empty() {
            return;
        }
        let path = typed_path(self.model.path(), input);
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {
                self.path_entry = None;
                self.navigate(path);
            }
            Ok(metadata) if metadata.is_file() => {
                self.path_entry = None;
                self.stop_search();
                if let Some(parent) = path.parent().map(Path::to_path_buf) {
                    self.model.navigate(parent);
                    self.model.select_when_listed(path);
                    self.reveal = true;
                }
            }
            _ => {
                if let Some(entry) = &mut self.path_entry {
                    entry.error = true;
                }
            }
        }
    }

    /// Move the list's offset the least distance that puts the selected row in view.
    ///
    /// This runs before the build because the row window is chosen from the offset the
    /// scroller reports, and a build reads the offset it was asked for earlier in the same
    /// frame: moving the offset here is what makes the frame declare the rows around the
    /// selection instead of the rows around where the list stood last frame.
    fn reveal_selection(&mut self, cx: &mut ui_core::cx::Cx<'_>) {
        if !std::mem::take(&mut self.reveal) {
            return;
        }
        let Some(index) = self.model.cursor else {
            return;
        };
        // The grid keeps its own scroller, so each view reveals into the offsets the user
        // left behind in that view.
        let (id, row, pitch) = match self.layout {
            Layout::Rows => (view::list_id(self.model.path()), index, view::ROW_HEIGHT),
            Layout::Grid => {
                let columns = self.columns.max(1);
                (
                    view::grid_id(self.model.path()),
                    index / columns,
                    view::GRID_ROW_PITCH,
                )
            }
        };
        let Some(geom) = cx.scroll_of(id) else {
            return;
        };
        let height = match self.layout {
            Layout::Rows => view::ROW_HEIGHT,
            Layout::Grid => view::GRID_CELL_HEIGHT,
        };
        let top = row as f32 * pitch;
        let bottom = top + height;
        if top < geom.offset_y {
            cx.set_scroll(id, geom.offset_x, top);
        } else if bottom > geom.offset_y + geom.viewport_h {
            cx.set_scroll(id, geom.offset_x, (bottom - geom.viewport_h).max(0.0));
        }
    }

    /// A control reported a change; `Some` is the dialog's answer.
    pub fn changed(&mut self, id: Id, change: Change) -> Option<PickerOutcome> {
        // The rows a menu merged belong to that menu; a later pick cannot run them.
        if matches!(change, Change::MenuClosed) {
            self.menu_read = None;
            self.menu_open = None;
            self.shell_rows = None;
            self.shell_tokens.clear();
            return None;
        }
        // The subfolder menu's window reports the engine's press-outside and its Escape chord
        // as one dismissal; closing the menu is everything the dialog does about them.
        if matches!(change, Change::Dismissed) {
            if id == view::crumb_menu_id() && self.crumb_menu.take().is_some() {
                self.dirty = true;
            }
            if id == view::filter_menu_id() && std::mem::take(&mut self.filter_menu) {
                self.dirty = true;
            }
            if let Some(index) = self.choice_menu
                && id == view::choice_menu_id(index)
            {
                self.choice_menu = None;
                self.dirty = true;
            }
            if id == view::drop_menu_id() && self.drop_menu.take().is_some() {
                self.dirty = true;
            }
            // The reading windows report Escape, Enter, a press outside (in the popup fallback)
            // and their close box as one dismissal. The questions answer through their buttons.
            if id == view::failures_id() {
                self.dismiss_failures();
            }
            if id == view::properties_id() {
                self.dismiss_properties();
            }
            return None;
        }
        // The name field is the dialog's own buffer: the engine reports the whole text, and
        // the model keeps it until accept joins it to the folder on screen. The search and
        // path fields report the same way.
        if let Change::Text(text) = change {
            if id == view::answer_id() {
                self.dirty = true;
                self.model.set_answer(text);
            } else if id == view::search_id() {
                self.set_search_query(text);
            } else if id == view::path_entry_id() {
                if let Some(entry) = &mut self.path_entry {
                    entry.text = text;
                    entry.error = false;
                }
                self.dirty = true;
            } else if id == view::rename_id() {
                self.set_rename_buffer(text);
            } else if id == view::conflict_name_id()
                && let Some(ask) = &mut self.conflict
            {
                ask.rename = Some(text);
                self.dirty = true;
            }
            return None;
        }
        // A file list the platform read back: the clipboard's verbs land here.
        if let Change::Paste(content) = change {
            self.paste_clipboard(content);
            return None;
        }
        // A context menu row the engine picked, named by its stable id; `id` is the node
        // whose menu it was, which is the background when no row stands behind it.
        if let Change::MenuItem(item) = change {
            return self.menu_chosen(id, item);
        }
        // A secondary press no node declared a menu for: the dialog owns the rows, so it
        // opens theirs at the point the press landed.
        if let Change::MenuRequested(at) = change {
            let target = match self.row_index(id) {
                Some(index) => {
                    if !self.model.is_selected(index) {
                        // A right press selects the row it lands on first, as every manager
                        // does; the menu that follows acts on what the user now sees marked.
                        let shape = self.range_shape();
                        self.model.row_clicked(index, Modifiers::NONE, shape, false);
                    }
                    MenuTarget::Row(index)
                }
                None => match self.sidebar_path(id) {
                    Some(path) => MenuTarget::Place { owner: id, path },
                    None => MenuTarget::Folder,
                },
            };
            self.context_menu = Some((at, target));
            self.dirty = true;
            return None;
        }
        // A row reports its own gesture rather than `Activated`, because a selection click
        // carries the modifiers that decide replace, toggle or extend.
        if let Change::Data(payload) = change {
            return match payload.downcast::<RowGesture>() {
                Ok(gesture) => self.row_gesture(*gesture),
                Err(_) => None,
            };
        }
        // The engine tells the drag's source when the gesture begins and ends; the row's
        // widget reports every position in between.
        if matches!(change, Change::DragStarted) {
            if let Some(index) = self.row_index(id) {
                self.dirty = true;
                self.model.begin_drag(index);
                self.drag_paths = self.model.drag_paths(index);
            }
            return None;
        }
        // A drop on a row or on the list's background: the payload names the files, the
        // target names the folder.
        if let Change::DroppedOn(report) = change {
            self.spring = None;
            self.spring_pending = None;
            self.drop_received(id, report);
            return None;
        }
        // The gesture ended. On Wayland the destination only copied a move, so the source
        // removes the originals here; on Windows an optimized move already did.
        if let Change::Dropped { outcome, .. } = &change {
            self.spring = None;
            self.spring_pending = None;
            // A drag that opened the decision menu has not decided anything yet; the
            // originals must survive until the row the user picks says which way it goes.
            let open_question = self.drop_menu.is_some();
            let finish = !open_question
                && outcome.is_some_and(|outcome| {
                    outcome.action == ui_core::DropEffect::Move
                        && outcome.performed_by == ui_core::Performer::Source
                });
            if finish {
                self.finish_outgoing_move();
            }
            if self.row_index(id).is_some() {
                self.model.end_drag();
                self.drag_paths.clear();
            }
            return None;
        }
        // The gesture ended with nothing taking the files; the sweep it began has to end too.
        if matches!(&change, Change::Cancelled) {
            self.spring = None;
            self.spring_pending = None;
            self.drag_paths.clear();
            if self.row_index(id).is_some() {
                self.model.end_drag();
            }
            return None;
        }
        // The drop highlight is a fact of the engine's live target, and a target change
        // paints nothing by itself: the frame after an entry or an exit must be built.
        if matches!(&change, Change::DragEntered { .. } | Change::DragExited) {
            self.dirty = true;
            // A drag that comes to rest on a folder — a row, a breadcrumb — opens it after
            // the hold, so the drop can land inside without starting over. Only a build
            // holds the clock.
            self.spring = None;
            self.spring_pending = matches!(&change, Change::DragEntered { .. })
                .then(|| self.spring_folder(id))
                .flatten();
            return None;
        }
        if !matches!(change, Change::Activated) {
            return None;
        }
        // The right-drag menu answered; the folder and the files were captured when the
        // release opened it, so a later frame cannot change what the rows refer to.
        if let Some(effect) = view::drop_menu_effect(id) {
            self.dirty = true;
            if let Some(menu) = self.drop_menu.take() {
                self.start_operation(menu.paths, menu.destination, effect);
            }
            return None;
        }
        if id == view::drop_menu_cancel_id() {
            self.dirty = true;
            self.drop_menu = None;
            return None;
        }
        // The conflict panel's rows answer the worker; the job waits until one does.
        if id == view::conflict_replace_id() {
            self.answer_conflict(ConflictPolicy::Replace);
            return None;
        }
        if id == view::conflict_skip_id() {
            self.answer_conflict(ConflictPolicy::Skip);
            return None;
        }
        if id == view::conflict_keep_both_id() {
            self.answer_conflict(ConflictPolicy::KeepBoth);
            return None;
        }
        if id == view::conflict_rename_id() {
            // Naming the newcomer opens a field, and only its own answer leaves the panel.
            if let Some(ask) = &mut self.conflict
                && ask.rename.is_none()
            {
                ask.rename = Some(ask.conflict.suggested_name.clone());
            }
            self.dirty = true;
            return None;
        }
        if id == view::conflict_rename_ok_id() {
            self.answer_conflict_named();
            return None;
        }
        if id == view::conflict_apply_all_id() {
            if let Some(ask) = &mut self.conflict {
                ask.apply_all = !ask.apply_all;
            }
            self.dirty = true;
            return None;
        }
        if id == view::conflict_cancel_id() {
            self.cancel_conflict();
            return None;
        }
        if id == view::operation_cancel_id() {
            self.cancel_running();
            return None;
        }
        if id == view::failures_close_id() {
            self.dismiss_failures();
            return None;
        }
        if id == view::delete_confirm_id() {
            self.confirm_delete();
            return None;
        }
        if id == view::delete_cancel_id() {
            self.delete_ask = None;
            self.dirty = true;
            return None;
        }
        if id == view::undo_confirm_id() {
            self.confirm_undo_copy();
            return None;
        }
        if id == view::undo_cancel_id() {
            self.cancel_undo_ask();
            return None;
        }
        if id == view::properties_close_id() {
            self.dismiss_properties();
            return None;
        }
        if id == view::transfer_accept_id() {
            self.confirm_transfer();
            return None;
        }
        if id == view::transfer_cancel_id() {
            self.dismiss_transfer();
            return None;
        }
        if id == view::transfer_up_id() {
            self.transfer_up();
            return None;
        }
        if let Some(path) = self.transfer_target(id) {
            self.transfer_into(path);
            return None;
        }
        if id == view::up_id() {
            self.dirty = true;
            self.go_up();
            return None;
        }
        if id == view::crumb_space_id() {
            self.open_path_entry();
            return None;
        }
        if id == view::hidden_id() {
            self.dirty = true;
            // The menu lists directories through the hidden-files setting, so an answer given
            // under the old one is not the list the dialog now stands behind.
            self.crumb_menu = None;
            let show = !self.model.show_hidden();
            self.model.set_show_hidden(show);
            // A search walked under the old rule would keep hidden matches the toggle just
            // turned off, so it walks again.
            if self.search.is_some() {
                self.restart_search();
            }
            self.reveal = false;
            return None;
        }
        if id == view::retry_id() {
            self.dirty = true;
            if self.search.is_some() {
                self.restart_search();
            } else {
                self.model.retry();
            }
            self.reveal = false;
            return None;
        }
        if id == view::search_button_id() {
            self.dirty = true;
            if self.search.is_some() {
                self.close_search();
            } else {
                self.open_search();
            }
            return None;
        }
        if id == view::open_id() {
            self.dirty = true;
            return self.accept();
        }
        if id == view::cancel_id() {
            return Some(PickerOutcome::Cancelled);
        }
        if id == view::options_id() {
            self.options_open = !self.options_open;
            self.dirty = true;
            return None;
        }
        if id == view::view_id() {
            self.layout = match self.layout {
                Layout::Rows => Layout::Grid,
                Layout::Grid => Layout::Rows,
            };
            self.dirty = true;
            return None;
        }
        if id == view::filter_id() {
            self.filter_menu = !self.filter_menu;
            self.choice_menu = None;
            self.dirty = true;
            return None;
        }
        if let Some(index) =
            (0..self.model.filters().len()).find(|index| view::filter_item_id(*index) == id)
        {
            self.filter_menu = false;
            self.model.set_active_filter(index);
            self.dirty = true;
            return None;
        }
        if let Some(index) =
            (0..self.choices.len()).find(|index| view::choice_button_id(*index) == id)
        {
            self.dirty = true;
            self.filter_menu = false;
            // A checkbox has no menu to open: its button is the whole control.
            if matches!(self.choices[index].kind, ChoiceKind::Boolean) {
                let value = &mut self.choice_values[index];
                *value = if value == "true" {
                    "false".to_string()
                } else {
                    "true".to_string()
                };
                self.choice_menu = None;
            } else {
                self.choice_menu = (self.choice_menu != Some(index)).then_some(index);
            }
            return None;
        }
        if let Some(index) = self.choice_menu {
            let picked = match &self.choices[index].kind {
                ChoiceKind::Select(options) => options
                    .iter()
                    .find(|option| view::choice_item_id(index, &option.value) == id)
                    .map(|option| option.value.clone()),
                ChoiceKind::Boolean => None,
            };
            if let Some(value) = picked {
                self.choice_values[index] = value;
                self.choice_menu = None;
                self.dirty = true;
                return None;
            }
        }
        if id == view::overwrite_replace_id() {
            return self.replace_overwrite();
        }
        if id == view::overwrite_cancel_id() {
            self.overwrite_confirm = None;
            self.dirty = true;
            return None;
        }
        // A sidebar row names a folder; the list, the drag targets and the menu all read
        // that one mapping rather than each searching the three lists.
        if let Some(path) = self.sidebar_path(id) {
            self.dirty = true;
            self.navigate(path);
            return None;
        }
        // A removable volume has no path until it is mounted: the click starts the mount,
        // and `apply_volumes` opens the point the platform answers with.
        if let Some(mount_id) = self.mountable_clicked(id) {
            if self.mounting.is_none() {
                let started = self
                    .volumes
                    .as_ref()
                    .is_some_and(|volumes| volumes.mount(&mount_id));
                // A worker that could not take the job never answers; the footer says so
                // rather than leaving the row looking inert.
                self.mount_failed = !started;
                self.dirty = true;
                if started {
                    self.mounting = Some(mount_id);
                }
            }
            return None;
        }
        // The arrow beside a crumb opens the folder's subfolders; the label beside it still
        // jumps, so the two halves of the capsule are two targets.
        let current = self.model.path().to_path_buf();
        if let Some(path) = current
            .ancestors()
            .find(|segment| view::crumb_arrow_id(segment) == id)
            .map(Path::to_path_buf)
        {
            self.dirty = true;
            self.crumb_menu = Some(path);
            return None;
        }
        // A row of an open menu names a subfolder of the folder the menu was opened on.
        if let Some(menu) = self.crumb_menu.clone()
            && let Subfolders::Ready(folders) = self.model.subfolders(&menu)
            && let Some(path) = folders
                .iter()
                .find(|entry| view::crumb_item_id(&entry.path) == id)
                .map(|entry| entry.path.clone())
        {
            self.dirty = true;
            self.crumb_menu = None;
            self.navigate(path);
            return None;
        }
        // A breadcrumb stands for one ancestor of the directory on screen, and its id
        // carries that path; clicking the trail jumps to whatever the segment names.
        if let Some(path) = current
            .ancestors()
            .find(|segment| view::crumb_id(segment) == id)
            .map(Path::to_path_buf)
        {
            self.dirty = true;
            if path != current {
                self.navigate(path);
            } else {
                // The address bar is how Explorer lets a click become typing; a crumb that
                // already names the folder on screen has nothing to jump to.
                self.open_path_entry();
            }
            return None;
        }
        None
    }

    /// The row a click or drag id names, when it is one of the rows on screen.
    ///
    /// The list's rows and the grid's cells are separate nodes with separate ids, and either
    /// view can be the drag's source.
    fn row_index(&self, id: Id) -> Option<usize> {
        self.model.rows().iter().position(|entry| {
            view::row_id(entry) == id || view::grid_cell_id(entry) == id
        })
    }

    /// What a drop on `id` means: the folder a directory row names, the folder a breadcrumb
    /// or a sidebar row stands for, or the folder on screen for the list's background.
    fn drop_destination(&self, id: Id) -> Option<PathBuf> {
        if let Some(index) = self.row_index(id) {
            let entry = self.model.rows().get(index)?;
            return entry.directory.then(|| entry.path.clone());
        }
        if let Some(path) = self.crumb_path(id) {
            return Some(path);
        }
        if let Some(path) = self.sidebar_path(id) {
            return Some(path);
        }
        // Only a node that declared `accept_drop` receives a drop, and the list's
        // background is the one that is neither a row nor a crumb: it stands for the
        // folder on screen.
        Some(self.model.path().to_path_buf())
    }

    /// The folder a breadcrumb capsule names, when `id` is one; the trail is the path's own
    /// ancestors, so no second list of them exists to go stale.
    fn crumb_path(&self, id: Id) -> Option<PathBuf> {
        self.model
            .path()
            .ancestors()
            .find(|segment| view::crumb_id(segment) == id)
            .map(Path::to_path_buf)
    }

    /// The folder a sidebar row names, when `id` is one; a removable volume has no path
    /// until it is mounted and so names nothing here.
    fn sidebar_path(&self, id: Id) -> Option<PathBuf> {
        self.extra_places
            .iter()
            .find(|place| view::extra_place_id(place) == id)
            .map(|place| place.path.clone())
            .or_else(|| {
                self.places
                    .iter()
                    .find(|place| view::place_id(place) == id)
                    .map(|place| place.path.clone())
            })
            .or_else(|| {
                self.devices
                    .iter()
                    .find(|device| view::device_id(device) == id)
                    .map(|device| device.path.clone())
            })
    }

    /// What the shell speaks for when the menu belongs to `target`: a row's menu is about
    /// the selection, while the background and a sidebar row stand for their folder.
    pub(crate) fn shell_paths(&self, target: &MenuTarget) -> Vec<PathBuf> {
        match target {
            MenuTarget::Row(_) => self.model.selection_paths(),
            MenuTarget::Folder => vec![self.model.path().to_path_buf()],
            MenuTarget::Place { path, .. } => vec![path.clone()],
        }
    }

    /// Whether the live drag could land on `destination`.
    ///
    /// The hover must answer what the release would, or a row would light up for a drop
    /// that changes nothing. A foreign payload names its paths only when decoded, so it is
    /// judged there, by the same rule.
    fn drop_accepts(&self, destination: &Path) -> bool {
        self.drag_paths.is_empty()
            || self
                .drag_paths
                .iter()
                .any(|source| moves_into(source, destination))
    }

    /// The folder a drag resting on `id` would open: a folder row, a breadcrumb, a sidebar
    /// row, or nothing when the destination is the folder already on screen.
    ///
    /// Entering is navigation, not a drop: a folder the release would refuse — the one the
    /// sources already sit in — must still open, or a drag could not come back through it.
    fn spring_folder(&self, id: Id) -> Option<PathBuf> {
        let folder = self.drop_destination(id)?;
        (folder != self.model.path()).then_some(folder)
    }

    /// Files landed on a row or on the background.
    fn drop_received(&mut self, id: Id, report: ui_core::DropReport) {
        let Some(destination) = self.drop_destination(id) else {
            return;
        };
        let Some(paths) = report
            .data
            .item(uri_list::MIME)
            .and_then(|bytes| uri_list::decode(bytes).ok())
        else {
            // The dialog takes files; text dropped on it is not a path list.
            return;
        };
        if paths.is_empty() {
            return;
        }
        // The release obeys the rule the hover drew: a source already in the destination,
        // or a folder the destination sits inside, is not something this drop moves.
        let paths: Vec<PathBuf> = paths
            .into_iter()
            .filter(|path| moves_into(path, &destination))
            .collect();
        if paths.is_empty() {
            return;
        }
        // Without the service nothing can land; opening a menu that cannot act would be a
        // promise the dialog cannot keep.
        if self.file_operations.is_none() {
            return;
        }
        self.dirty = true;
        // A right drag asks: nothing moves until the menu answers. A held modifier is the
        // answer itself, as in Explorer, so it performs the effect directly.
        if report.button == ui_core::PointerButton::Secondary
            && !report.modifiers.ctrl
            && !report.modifiers.shift
        {
            self.drop_menu = Some(DropMenu {
                at: report.pos,
                destination,
                paths,
            });
            return;
        }
        // With no modifier held, another application's payload says nothing about what it
        // prefers, so the same-volume rule answers — the one Explorer's plain drag follows.
        let mut effect = report.effect;
        if report.foreign
            && !report.modifiers.ctrl
            && !report.modifiers.shift
            && effect == ui_core::DropEffect::Copy
            && report.data.effects.contains(ui_core::DropEffect::Move)
            && self.same_volume(&paths, &destination)
        {
            effect = ui_core::DropEffect::Move;
        }
        // Wayland gives the move to the source: the destination copies, and the source
        // removes the originals after the compositor reports the drag finished.
        let effect = match effect {
            ui_core::DropEffect::Move if report.performer == ui_core::Performer::Source => {
                ui_core::DropEffect::Copy
            }
            effect => effect,
        };
        self.start_operation(paths, destination, effect);
    }

    /// Whether every source sits on the destination's volume: the test behind a plain
    /// drag's move-or-copy rule.
    fn same_volume(&self, paths: &[PathBuf], destination: &Path) -> bool {
        let Some(operations) = &self.file_operations else {
            return false;
        };
        paths.iter().all(|path| {
            let source = path.parent().unwrap_or(path);
            operations.same_volume(source, destination)
        })
    }

    /// Hand a drop to the platform's file operations.
    fn start_operation(
        &mut self,
        paths: Vec<PathBuf>,
        destination: PathBuf,
        effect: ui_core::DropEffect,
    ) {
        let request = match effect {
            ui_core::DropEffect::Copy => OperationRequest::copy(paths, &destination),
            ui_core::DropEffect::Move => OperationRequest::move_to(paths, &destination),
            ui_core::DropEffect::Link => {
                OperationRequest::link(paths, &destination, LinkKind::Symbolic)
            }
        };
        self.touched.push(destination);
        // A destination that already exists is the user's question, and the conflict panel
        // draws it: nothing is replaced or duplicated before the answer.
        self.start(request.with_policy(ConflictPolicy::Ask));
    }

    /// Start one batch and remember its kind and its sources, which its progress events do
    /// not carry.
    fn start(&mut self, request: OperationRequest) -> Option<ui_platform::OperationId> {
        let operations = self.file_operations.clone()?;
        let kind = request.kind;
        let mut items = request.sources.clone();
        sort_paths(&mut items);
        let id = operations.start(request);
        self.jobs.insert(id, Job { kind, items });
        self.dirty = true;
        Some(id)
    }

    /// Finish a move that left the dialog: the destination holds copies, so the originals
    /// go. This is not a user delete, so it does not go to the trash.
    fn finish_outgoing_move(&mut self) {
        let paths = std::mem::take(&mut self.drag_paths);
        if paths.is_empty() {
            return;
        }
        self.start(OperationRequest::delete(paths, DeleteMode::Permanent));
    }

    /// Read what the service published and keep the dialog's view of it current.
    fn poll_operations(&mut self) {
        let Some(operations) = &self.file_operations else {
            return;
        };
        let revision = operations.revision();
        if revision == self.operations_seen {
            return;
        }
        self.operations_seen = revision;
        let mut finished = Vec::new();
        for event in operations.take() {
            match event {
                ui_platform::OperationEvent::Progress(progress) => {
                    // A job the dialog did not start has no batch to list; the footer still
                    // reports it, as it did before the band existed.
                    let job = self.jobs.get(&progress.id);
                    self.running = Some(Running {
                        id: progress.id,
                        kind: job.map_or(ui_platform::OperationKind::Copy, |job| job.kind),
                        done: progress.done_items,
                        total: progress.total_items,
                        bytes_done: progress.bytes_done,
                        bytes_total: progress.bytes_total,
                        items: job.map(|job| job.items.clone()).unwrap_or_default(),
                        current: progress.current,
                    });
                }
                // The panel is the dialog's answer; the worker waits until a row is pressed.
                ui_platform::OperationEvent::Conflict(conflict) => {
                    if self.conflict.is_none() {
                        self.conflict = Some(ConflictAsk {
                            conflict,
                            apply_all: false,
                            rename: None,
                        });
                    }
                    self.dirty = true;
                }
                ui_platform::OperationEvent::Finished(report) => finished.push(report),
            }
        }
        if finished.is_empty() {
            return;
        }
        for report in finished {
            self.jobs.remove(&report.id);
            if self
                .running
                .as_ref()
                .is_some_and(|running| running.id == report.id)
            {
                self.running = None;
            }
            if self.undoing == Some(report.id) {
                // An inverse is spent only once it worked; a failed one stays on the stack
                // for a retry, beside the failure list that says why.
                self.undoing = None;
                if report.failed.is_empty() {
                    self.undo.pop();
                }
            } else {
                self.undo.record(&report);
            }
            self.consume_cut(&report);
            if !report.failed.is_empty() {
                self.failures = Some(Failures {
                    items: report.failed,
                });
            }
            self.dirty = true;
        }
        let mut paths = std::mem::take(&mut self.touched);
        paths.push(self.model.path().to_path_buf());
        paths.sort();
        paths.dedup();
        for path in paths {
            self.model.invalidate(&path);
        }
        self.dirty = true;
    }

    /// A cut paste consumes its own success: moved rows stop dimming and the clipboard
    /// carries where they went as a copy, so a second paste duplicates instead of looking
    /// for files that are no longer there. A failed row keeps its cut for a later attempt.
    fn consume_cut(&mut self, report: &ui_platform::OperationReport) {
        if self.cut_paste != Some(report.id) {
            return;
        }
        self.cut_paste = None;
        let moved: Vec<&PathBuf> = report.succeeded.iter().map(|outcome| &outcome.source).collect();
        let destinations: Vec<PathBuf> = report
            .succeeded
            .iter()
            .filter_map(|outcome| outcome.destination.clone())
            .collect();
        let remaining: Vec<PathBuf> = self
            .cut
            .iter()
            .filter(|path| !moved.contains(path))
            .cloned()
            .collect();
        if remaining.is_empty() {
            self.cut.clear();
            if !destinations.is_empty() {
                self.clipboard = Some(ClipboardWrite {
                    paths: destinations,
                    effect: ui_core::DropEffect::Copy,
                });
            }
        } else {
            self.cut = remaining.clone();
            self.clipboard = Some(ClipboardWrite {
                paths: remaining,
                effect: ui_core::DropEffect::Move,
            });
        }
        self.dirty = true;
    }

    /// Capture the selection's facts now, because the panel describes what was chosen when
    /// it opened rather than a selection that changes underneath it.
    fn open_properties(&mut self) {
        if self.modal_open() {
            return;
        }
        let entries = self.model.selection_entries();
        let Some(first) = entries.first() else {
            return;
        };
        let single = entries.len() == 1;
        let folder = single && first.directory;
        let extension = single
            .then(|| first.path.extension())
            .flatten()
            .and_then(|extension| extension.to_str())
            .map(str::to_string);
        let location = if single {
            first.path.parent().unwrap_or(&first.path).to_path_buf()
        } else {
            self.model.path().to_path_buf()
        };
        // A folder's contents are not read for this panel; only the files know their bytes.
        let size = if single {
            (!first.directory).then_some(first.len)
        } else {
            entries
                .iter()
                .any(|entry| !entry.directory)
                .then(|| {
                    entries
                        .iter()
                        .filter(|entry| !entry.directory)
                        .map(|entry| entry.len)
                        .sum()
                })
        };
        self.properties = Some(Properties {
            name: single.then(|| first.name.clone()),
            count: entries.len(),
            folder,
            extension,
            location,
            size,
            modified: single.then(|| first.modified).flatten(),
        });
        self.dirty = true;
    }

    /// A folder described as a background or sidebar menu's subject: the press stands for
    /// the folder itself, as Explorer's background menu does.
    fn open_folder_properties(&mut self, path: PathBuf) {
        if self.modal_open() {
            return;
        }
        let modified = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok());
        self.properties = Some(Properties {
            name: Some(view::crumb_label(&path)),
            // A single subject, named: the panel's heading is the folder's own name.
            count: 1,
            folder: true,
            extension: None,
            location: path.parent().unwrap_or(&path).to_path_buf(),
            // A folder's contents are not read for this panel, as for a folder row's.
            size: None,
            modified,
        });
        self.dirty = true;
    }

    fn dismiss_properties(&mut self) {
        if self.properties.take().is_some() {
            self.dirty = true;
        }
    }

    /// Open the folder chooser a Move to / Copy to command asks for.
    ///
    /// It starts in the folder on screen, which is where the selection lives; the chooser
    /// browses from there. The job is not started until its answer, so an abandoned
    /// chooser changes nothing.
    fn begin_transfer(&mut self, effect: ui_core::DropEffect) {
        if self.modal_open() || self.file_operations.is_none() {
            return;
        }
        let paths = self.model.selection_paths();
        if paths.is_empty() {
            return;
        }
        self.transfer = Some(Transfer {
            effect,
            paths,
            folder: self.model.path().to_path_buf(),
        });
        self.dirty = true;
    }

    fn dismiss_transfer(&mut self) {
        if self.transfer.take().is_some() {
            self.dirty = true;
        }
    }

    fn transfer_into(&mut self, path: PathBuf) {
        if let Some(ask) = &mut self.transfer {
            ask.folder = path;
            self.dirty = true;
        }
    }

    fn transfer_up(&mut self) {
        let Some(ask) = &mut self.transfer else {
            return;
        };
        let Some(parent) = ask.folder.parent().map(Path::to_path_buf) else {
            return;
        };
        ask.folder = parent;
        self.dirty = true;
    }

    /// The chooser's answer: the job runs into the folder on show, through the same path a
    /// drop takes, so conflicts, failures, undo and the refresh are the ones every job gets.
    fn confirm_transfer(&mut self) {
        let Some(ask) = self.transfer.take() else {
            return;
        };
        self.dirty = true;
        self.start_operation(ask.paths, ask.folder, ask.effect);
    }

    /// The folder a chooser row names, if the row is one of its jumps.
    ///
    /// The jump ids are keyed by path, so a row is resolved the way a list row is: by asking
    /// the lists the panel drew from again. A path that has gone since the press resolves to
    /// nothing and the chooser stays where it is.
    fn transfer_target(&self, id: Id) -> Option<PathBuf> {
        let ask = self.transfer.as_ref()?;
        if id == view::transfer_up_id() {
            return ask.folder.parent().map(Path::to_path_buf);
        }
        // The panel draws a shortcut only when it names a folder outside this one.
        let outside = |path: &Path| path != ask.folder && !path.starts_with(&ask.folder);
        if let Some(place) = self
            .places
            .iter()
            .find(|place| outside(&place.path) && view::transfer_place_id(&place.path) == id)
        {
            return Some(place.path.clone());
        }
        if let Some(place) = self
            .extra_places
            .iter()
            .find(|place| outside(&place.path) && view::transfer_extra_place_id(&place.path) == id)
        {
            return Some(place.path.clone());
        }
        if let Some(device) = self
            .devices
            .iter()
            .find(|device| outside(&device.path) && view::transfer_device_id(&device.path) == id)
        {
            return Some(device.path.clone());
        }
        let Subfolders::Ready(folders) = self.model.subfolders(&ask.folder) else {
            return None;
        };
        folders
            .into_iter()
            .find(|entry| view::transfer_folder_id(&entry.path) == id)
            .map(|entry| entry.path)
    }

    /// Hand the selection to the platform's own menu.
    ///
    /// The popup owns the user's input until it closes, so nothing blocks here: the build
    /// passes the request to the host, which runs it once this frame has gone to the screen
    /// and its menu panel is gone.
    fn show_shell_menu(&mut self) {
        // The menu's own paths: the selection a row stands for, or the folder behind the
        // background and a sidebar row.
        let target = self.menu_open.as_ref().map(|open| open.target.clone());
        let mut paths = match &target {
            Some(target) => self.shell_paths(target),
            None => self.model.selection_paths(),
        };
        if paths.is_empty() {
            paths.push(self.model.path().to_path_buf());
        }
        self.shell_menu_ask = Some(ShellMenuAsk::Issue(paths, self.menu_point));
        self.dirty = true;
    }

    /// Start the shell read a menu waits for before it opens.
    ///
    /// Returns false when the menu must open now: no host answers this selection, and the
    /// rows are the picker's own.
    pub(crate) fn begin_menu_read(
        &mut self,
        cx: &mut ui_widgets::Cx<'_, '_>,
        at: Point,
        target: MenuTarget,
    ) -> bool {
        self.menu_read = None;
        self.menu_open = None;
        self.shell_rows = None;
        self.shell_tokens.clear();
        let Some(host) = &self.shell_menu else {
            return false;
        };
        // A row's menu reads the selection; the background and a sidebar row read the
        // folder they stand for, as Explorer's background menu does.
        let paths = self.shell_paths(&target);
        if paths.is_empty() {
            return false;
        }
        if !host.available(&paths) {
            return false;
        }
        let ticket = host.read(paths.clone());
        self.menu_read = Some(MenuRead {
            at,
            target,
            ticket,
            paths,
            // Reading the clock is also what asks for the frames this wait runs on.
            deadline: cx.now() + MENU_READ_WAIT,
            opened: false,
        });
        true
    }

    /// The held-back menu, once the shell's rows are in or the wait is over, and the rows
    /// that arrive after it opened.
    ///
    /// A read that never answers in time leaves the menu to open without shell rows — the
    /// same menu a host that cannot merge them offers — and its rows still join it when the
    /// host wakes the window with the answer, however late. Extension code that blocks for a
    /// second must not cost the menu its shell rows, which was the first press's whole point.
    pub(crate) fn poll_menu_read(
        &mut self,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Option<MenuReadStep> {
        let mut pending = self.menu_read.take()?;
        let answer = self
            .shell_menu
            .as_ref()
            .and_then(|host| host.take_rows(pending.ticket));
        match answer {
            Some(answered) => {
                self.shell_rows = Some(ShellRows {
                    paths: std::mem::take(&mut pending.paths),
                    rows: answered.rows,
                    omitted: answered.omitted,
                });
                if pending.opened {
                    Some(MenuReadStep::Refresh)
                } else {
                    Some(MenuReadStep::Open {
                        at: pending.at,
                        target: pending.target,
                    })
                }
            }
            // Reading the clock asks for a frame, so it is only read while the menu is still
            // waiting: afterwards the host's wake is what brings the answer here.
            None if !pending.opened && cx.now() >= pending.deadline => {
                pending.opened = true;
                let step = MenuReadStep::Open {
                    at: pending.at,
                    target: pending.target.clone(),
                };
                self.menu_read = Some(pending);
                Some(step)
            }
            None => {
                self.menu_read = Some(pending);
                None
            }
        }
    }

    /// Run one row the platform's menu contributed.
    fn invoke_shell_command(&mut self, token: u32) {
        let Some(shell) = &self.shell_rows else {
            return;
        };
        let Some(host) = &self.shell_menu else {
            return;
        };
        host.invoke(shell.paths.clone(), token);
        self.dirty = true;
    }

    /// Re-read the folder when a platform command has finished, since a shell verb may have
    /// changed it.
    pub(crate) fn poll_shell_invocations(&mut self, cx: &mut ui_widgets::Cx<'_, '_>) {
        let Some(host) = &self.shell_menu else {
            return;
        };
        let done = host.invocations();
        if done == self.shell_invocations {
            return;
        }
        self.shell_invocations = done;
        self.dirty = true;
        self.model.retry();
        // The retry starts on the next frame's poll, and reading the clock is what asks
        // for that frame: nothing else has moved.
        cx.now();
    }

    /// A confirmation or chooser that owns the dialog until it is answered.
    ///
    /// The properties and failures windows only report, so they are not here: they stay
    /// open beside a list that keeps working.
    fn modal_open(&self) -> bool {
        self.conflict.is_some()
            || self.delete_ask.is_some()
            || self.undo_ask.is_some()
            || self.overwrite_confirm.is_some()
            || self.transfer.is_some()
            || self.rename.is_some()
    }

    /// Answer the open question with its default, as Enter does in a native dialog.
    fn confirm_modal(&mut self) -> bool {
        if self.delete_ask.is_some() {
            self.confirm_delete();
            return true;
        }
        if self.undo_ask.is_some() {
            self.confirm_undo_copy();
            return true;
        }
        if self.transfer.is_some() {
            self.confirm_transfer();
            return true;
        }
        if self.conflict.is_some() {
            // Naming the newcomer is the field's own Enter; without it the question's
            // first answer is Replace.
            self.answer_conflict(ConflictPolicy::Replace);
            return true;
        }
        false
    }

    /// Replace the file the save would overwrite: the Replace question's first answer.
    fn replace_overwrite(&mut self) -> Option<PickerOutcome> {
        let (paths, _) = self.overwrite_confirm.take()?;
        self.dirty = true;
        self.answer(paths)
    }

    /// Write the selection to the clipboard as a copy or a cut.
    ///
    /// The request waits for a build: the clipboard belongs to the platform, and only the
    /// frame holds the context that asks. A cut dims its rows until a paste consumes them.
    fn write_clipboard(&mut self, effect: ui_core::DropEffect) {
        let paths = self.model.selection_paths();
        if paths.is_empty() || self.modal_open() {
            return;
        }
        if effect == ui_core::DropEffect::Move {
            self.cut = paths.clone();
        } else {
            self.cut.clear();
        }
        self.clipboard = Some(ClipboardWrite { paths, effect });
        self.dirty = true;
    }

    /// Turn the row the cursor stands on into an in-place field.
    fn begin_rename(&mut self) {
        if self.modal_open() {
            return;
        }
        // A multi selection renames the row with focus, as every manager does: the marks
        // answer the dialog, and the field can only stand in one row.
        let path = match self.model.selection_paths().as_slice() {
            [one] => one.clone(),
            _ => match self.model.cursor.and_then(|index| self.model.rows().get(index)) {
                Some(entry) => entry.path.clone(),
                None => return,
            },
        };
        let Some(name) = path.file_name().map(|name| name.to_string_lossy().into_owned()) else {
            return;
        };
        self.open_rename(path, name);
    }

    /// Put `path`'s row into an in-place field holding `buffer`.
    fn open_rename(&mut self, path: PathBuf, buffer: String) {
        self.rename = Some(RenameField {
            path,
            buffer,
            focus_requested: true,
            select_pending: true,
            had_focus: false,
            error: None,
        });
        self.reveal = true;
        self.dirty = true;
    }

    /// Ask for the new field's keyboard, select its base name once it has it, and commit
    /// the name the moment the field loses the keys.
    fn rename_focus(&mut self, cx: &mut ui_core::cx::Cx<'_>) {
        let Some(field) = self.rename.as_ref() else {
            return;
        };
        // The field lives on a listed row. A folder that was just created has none until
        // the refresh lands, and the engine drops a focus request for a node no build made.
        if !self.model.rows().iter().any(|entry| entry.path == field.path) {
            return;
        }
        if field.focus_requested {
            if let Some(field) = &mut self.rename {
                field.focus_requested = false;
            }
            cx.request_focus(view::rename_id());
        }
        let focused = ui_platform::WidgetState::of(cx, view::rename_id()).focused;
        if !focused {
            if self.rename.as_ref().is_some_and(|field| field.had_focus) {
                self.commit_rename();
            }
            return;
        }
        if let Some(field) = &mut self.rename {
            field.had_focus = true;
            if field.select_pending {
                field.select_pending = false;
                let base = base_name_len(&field.buffer);
                // The selection is an edit with no bytes: the engine moves the caret and
                // the extension stays unselected, which is what a rename starts with.
                cx.edit_input_selecting(view::rename_id(), 0..0, "", (0, base));
            }
        }
    }

    /// Scroll the list or the sidebar while a drag rests near its edge.
    ///
    /// The pointer is still there, so no event would ask for the next frame; reading the
    /// clock once the content moved keeps the frames coming, as the spring's hold does.
    fn drag_autoscroll(&mut self, cx: &mut ui_core::cx::Cx<'_>) {
        // An in-app drag carries its paths; a drag from another application shows up only
        // as the engine's live session.
        if self.drag_paths.is_empty() && cx.drag_types().is_none() {
            return;
        }
        let Some(pointer) = cx.pointer() else {
            return;
        };
        let scrollers = [
            match self.layout {
                Layout::Grid => view::grid_id(self.model.path()),
                Layout::Rows => view::list_id(self.model.path()),
            },
            view::sidebar_scroll_id(),
        ];
        for id in scrollers {
            let Some(rect) = cx.rect_of(id) else {
                continue;
            };
            let Some(geom) = cx.scroll_of(id) else {
                continue;
            };
            if rect.is_empty() || !rect.contains(pointer) || geom.viewport_h <= 0.0 {
                continue;
            }
            let from_top = pointer.y - rect.min_y();
            let from_bottom = rect.max_y() - pointer.y;
            let speed = if from_top < DRAG_SCROLL_EDGE {
                -(1.0 - from_top / DRAG_SCROLL_EDGE) * DRAG_SCROLL_SPEED
            } else if from_bottom < DRAG_SCROLL_EDGE {
                (1.0 - from_bottom / DRAG_SCROLL_EDGE) * DRAG_SCROLL_SPEED
            } else {
                continue;
            };
            let limit = (geom.extent_h - geom.viewport_h).max(0.0);
            let offset = (geom.offset_y + speed * cx.dt()).clamp(0.0, limit);
            if (offset - geom.offset_y).abs() < 0.5 {
                continue;
            }
            cx.set_scroll(id, geom.offset_x, offset);
            cx.now();
        }
    }

    /// Stamp the hold a drag started over a folder and open the folder once it is long
    /// enough; reading the clock here is also what keeps the frames coming while it lasts.
    fn run_spring(&mut self, cx: &mut ui_core::cx::Cx<'_>) {
        if let Some(path) = self.spring_pending.take() {
            self.spring = Some(Spring {
                path,
                since: cx.now(),
            });
        }
        let Some(spring) = &self.spring else {
            return;
        };
        if cx.now() - spring.since < SPRING_DELAY {
            return;
        }
        let path = spring.path.clone();
        self.open_spring(path);
    }

    /// Open the folder a drag is held over; Space asks for the same thing at once.
    fn open_spring(&mut self, path: PathBuf) {
        self.spring = None;
        self.spring_pending = None;
        self.reveal = false;
        self.dirty = true;
        self.model.navigate(path);
    }

    /// Turn the Menu key into the cursor row's menu, anchored where that row is drawn.
    ///
    /// The engine opens a node's declared menu for this key; the picker has none, so it
    /// builds the same rows a right press would and lets the build open them.
    fn key_menu(&mut self, cx: &mut ui_core::cx::Cx<'_>) {
        if !std::mem::take(&mut self.menu_key) {
            return;
        }
        let row = self
            .model
            .cursor
            .filter(|index| self.model.rows().get(*index).is_some());
        let row_rect = row
            .and_then(|index| self.model.rows().get(index))
            .and_then(|entry| {
                let id = match self.layout {
                    Layout::Grid => view::grid_cell_id(entry),
                    Layout::Rows => view::row_id(entry),
                };
                cx.rect_of(id)
            });
        // No row means an empty folder, and no rect means the first frame: the pane's
        // corner is the one place the keyboard can point at without a row.
        let anchor = row_rect
            .map(|rect| Point::new(rect.min_x(), rect.max_y()))
            .or_else(|| {
                cx.rect_of(view::list_id(self.model.path()))
                    .map(|rect| Point::new(rect.min_x() + 8.0, rect.origin.y + 8.0))
            })
            .unwrap_or(Point::ZERO);
        let target = match row {
            Some(index) => MenuTarget::Row(index),
            None => MenuTarget::Folder,
        };
        self.context_menu = Some((anchor, target));
        self.menu_point = anchor;
        self.dirty = true;
    }

    /// The field reported its buffer; a refused name stops being refused the moment it is
    /// edited.
    fn set_rename_buffer(&mut self, text: String) {
        if let Some(field) = &mut self.rename {
            field.buffer = text;
            field.error = None;
        }
        self.dirty = true;
    }

    /// Commit the field's buffer as a rename, or say in place why it cannot be one.
    fn commit_rename(&mut self) {
        let Some(field) = self.rename.as_ref() else {
            return;
        };
        let source = field.path.clone();
        let name = field.buffer.clone();
        let Some(parent) = source.parent().map(Path::to_path_buf) else {
            self.rename = None;
            self.dirty = true;
            return;
        };
        if source.file_name().is_some_and(|old| old == name.as_str()) {
            // Nothing was typed: Escape and a stray Enter end the same way.
            self.rename = None;
            self.dirty = true;
            return;
        }
        let platform_refuses = name.is_empty()
            || name.contains(['/', '\0'])
            || self
                .file_operations
                .as_ref()
                .is_some_and(|operations| operations.validate_name(&name).is_err());
        if platform_refuses {
            self.rename_refused(RenameError::Invalid);
            return;
        }
        let target = parent.join(&name);
        // A case-only change names the same file on Windows, and its destination "exists".
        let case_only = source
            .to_string_lossy()
            .to_lowercase()
            == target.to_string_lossy().to_lowercase();
        if !case_only && target.exists() {
            self.rename_refused(RenameError::Taken(name.clone()));
            return;
        }
        if self.file_operations.is_some() {
            self.start(OperationRequest::rename(source, name));
            self.touched.push(parent);
            // The listing follows the folder's watch; the new name is what the cursor
            // should stand on once it lands.
            self.model.select_when_listed(target);
        } else {
            // A dialog without the service still renames: the worker is what the caller
            // left out, not the ability to name a file.
            match std::fs::rename(&source, &target) {
                Ok(()) => {
                    self.model.invalidate(&parent);
                    self.model.select_when_listed(target);
                    self.reveal = true;
                }
                Err(_) => {
                    // The platform's name rules passed; the filesystem refused the move
                    // itself (a lock, a share). The field stays open.
                    self.rename_refused(RenameError::Invalid);
                    return;
                }
            }
        }
        self.rename = None;
        self.dirty = true;
    }

    /// Keep the field open and say why the name was refused.
    fn rename_refused(&mut self, error: RenameError) {
        if let Some(field) = &mut self.rename {
            field.error = Some(error);
            // A refusal that followed a focus loss must take the keys back, or the field
            // sits unfocused while the next click commits the same refused name again.
            field.focus_requested = true;
        }
        self.dirty = true;
    }

    /// Escape in the field: the file keeps its name.
    fn cancel_rename(&mut self) {
        if self.rename.take().is_some() {
            self.dirty = true;
        }
    }

    /// Move the selection to the trash, or delete it for good after a confirmation.
    fn delete_selection(&mut self, permanent: bool) {
        if self.modal_open() {
            return;
        }
        let paths = self.model.selection_paths();
        if paths.is_empty() {
            return;
        }
        if permanent {
            self.delete_ask = Some(paths);
            self.dirty = true;
        } else {
            self.start_delete(paths, DeleteMode::Trash);
        }
    }

    /// Start a delete over the folders the files live in.
    fn start_delete(&mut self, paths: Vec<PathBuf>, mode: DeleteMode) {
        for path in &paths {
            if let Some(parent) = path.parent() {
                self.touched.push(parent.to_path_buf());
            }
        }
        self.start(OperationRequest::delete(paths, mode));
    }

    /// A file list arrived from the clipboard: land it where the paste asked for, or in
    /// the folder on screen.
    fn paste_clipboard(&mut self, content: ui_core::clipboard::ClipboardContent) {
        if self.modal_open() {
            return;
        }
        let requested_destination = self.paste_destination.take();
        let ui_core::clipboard::ClipboardContent::Files { paths, effect } = content else {
            // Text and bytes are an editor's; the dialog has no field holding the keyboard
            // here, and a name field keeps its own paste to itself.
            return;
        };
        if paths.is_empty() {
            return;
        }
        let destination = requested_destination.unwrap_or_else(|| self.model.path().to_path_buf());
        let request = match effect {
            ui_core::DropEffect::Copy => OperationRequest::copy(paths.clone(), &destination),
            ui_core::DropEffect::Move => OperationRequest::move_to(paths.clone(), &destination),
            ui_core::DropEffect::Link => {
                OperationRequest::link(paths.clone(), &destination, LinkKind::Symbolic)
            }
        };
        let Some(id) = self.start(request.with_policy(ConflictPolicy::Ask)) else {
            return;
        };
        self.touched.push(destination);
        if effect == ui_core::DropEffect::Move {
            // The rows a cut from elsewhere names dim until this paste lands: the report
            // decides what stays cut.
            self.cut_paste = Some(id);
            self.cut = paths;
        }
    }

    /// Answer the worker's question with the panel's choice.
    fn answer_conflict(&mut self, choice: ConflictPolicy) {
        let Some(ask) = self.conflict.take() else {
            return;
        };
        if let Some(operations) = &self.file_operations {
            operations.answer(
                ask.conflict.id,
                ui_platform::ConflictDecision {
                    choice,
                    apply_to_all: ask.apply_all,
                },
            );
        }
        self.dirty = true;
    }

    /// Answer with the name the conflict panel's field holds.
    fn answer_conflict_named(&mut self) {
        let Some(ask) = &self.conflict else {
            return;
        };
        let Some(name) = ask.rename.clone() else {
            return;
        };
        if name.is_empty() {
            return;
        }
        self.answer_conflict(ConflictPolicy::Rename(name));
    }

    /// Stop the job whose question the panel is showing.
    fn cancel_conflict(&mut self) {
        let Some(ask) = self.conflict.take() else {
            return;
        };
        if let Some(operations) = &self.file_operations {
            operations.cancel(ask.conflict.id);
        }
        self.dirty = true;
    }

    /// Stop the job the footer carries.
    fn cancel_running(&mut self) {
        let Some(running) = self.running.as_ref() else {
            return;
        };
        if let Some(operations) = &self.file_operations {
            operations.cancel(running.id);
        }
        self.dirty = true;
    }

    /// Close the failure list; the rows stay as the folder actually is.
    fn dismiss_failures(&mut self) {
        if self.failures.take().is_some() {
            self.dirty = true;
        }
    }

    /// Put back what the last finished job did.
    ///
    /// A copy asks first: the files it made may have been edited since, so removing them
    /// is a question, not a habit. Every other inverse starts at once and is spent only
    /// when its report comes back without failures.
    fn undo_last(&mut self) {
        if self.modal_open() || self.running.is_some() || self.file_operations.is_none() {
            return;
        }
        let Some(action) = self.undo.last().cloned() else {
            return;
        };
        if let ui_platform::UndoAction::RemoveCopy { created } = &action {
            self.undo_ask = Some(created.clone());
            self.dirty = true;
            return;
        }
        self.start_undo(&action);
    }

    /// Start an inverse and remember it, so its own report is not journaled as a new action.
    fn start_undo(&mut self, action: &ui_platform::UndoAction) {
        let Some(operation) = action.request() else {
            // An inverse the journal cannot express is dropped rather than left as a key
            // that does nothing forever.
            self.undo.pop();
            self.dirty = true;
            return;
        };
        self.touched.extend(action.touched());
        if let Some(id) = self.start(operation) {
            self.undoing = Some(id);
        }
    }

    /// The copy undo was confirmed; remove what the copy created.
    fn confirm_undo_copy(&mut self) {
        let Some(created) = self.undo_ask.take() else {
            return;
        };
        self.dirty = true;
        self.start_undo(&ui_platform::UndoAction::RemoveCopy { created });
    }

    fn cancel_undo_ask(&mut self) {
        if self.undo_ask.take().is_some() {
            self.dirty = true;
        }
    }

    /// A row of the context menu was chosen; `owner` names the node whose menu it was, so a
    /// command that acts on what the menu stands for can tell a folder's background and a
    /// sidebar row from a list row.
    fn menu_chosen(&mut self, owner: Id, id: Id) -> Option<PickerOutcome> {
        // A row the platform's menu contributed, named by the command it runs.
        if let Some(token) = self.shell_tokens.get(&id).copied() {
            self.invoke_shell_command(token);
            return None;
        }
        let Some(command) = MenuCommand::from_id(id) else {
            return None;
        };
        let target = self
            .menu_open
            .as_ref()
            .filter(|open| open.owner == owner)
            .map(|open| open.target.clone());
        if let Some(MenuTarget::Place { path, .. }) = &target {
            return self.place_chosen(path.clone(), command);
        }
        match command {
            MenuCommand::Open => {
                self.dirty = true;
                return self.accept();
            }
            MenuCommand::Undo => self.undo_last(),
            MenuCommand::Cut => self.write_clipboard(ui_core::DropEffect::Move),
            MenuCommand::Copy => self.write_clipboard(ui_core::DropEffect::Copy),
            MenuCommand::Paste => self.request_paste(),
            MenuCommand::MoveTo => self.begin_transfer(ui_core::DropEffect::Move),
            MenuCommand::CopyTo => self.begin_transfer(ui_core::DropEffect::Copy),
            MenuCommand::Rename => self.begin_rename(),
            MenuCommand::Trash => self.delete_selection(false),
            MenuCommand::Delete => self.delete_selection(true),
            MenuCommand::NewFolder => {
                self.new_folder = true;
                self.dirty = true;
            }
            MenuCommand::CreateLink => self.link_selection(LinkKind::Symbolic),
            MenuCommand::CreateHardLink => self.link_selection(LinkKind::Hard),
            MenuCommand::CopyPaths => {
                let text = self
                    .model
                    .selection_paths()
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    self.clipboard_text = Some(text);
                    self.dirty = true;
                }
            }
            MenuCommand::Properties => {
                // The background's menu stands for the folder on screen, as Explorer's
                // does; a row's menu speaks for the selection it was opened on.
                let folder = match &target {
                    Some(MenuTarget::Folder) => Some(self.model.path().to_path_buf()),
                    None if owner == view::list_id(self.model.path()) => {
                        Some(self.model.path().to_path_buf())
                    }
                    _ => None,
                };
                match folder {
                    Some(path) => self.open_folder_properties(path),
                    None => self.open_properties(),
                }
            }
            MenuCommand::ShellMenu => self.show_shell_menu(),
            MenuCommand::SelectAll => {
                self.dirty = true;
                self.model.select_all();
            }
            MenuCommand::Invert => {
                self.dirty = true;
                self.model.invert_selection();
            }
            MenuCommand::ShowHidden => {
                self.dirty = true;
                let show = !self.model.show_hidden();
                self.model.set_show_hidden(show);
                if self.search.is_some() {
                    self.restart_search();
                }
            }
            MenuCommand::Refresh => {
                self.dirty = true;
                if self.search.is_some() {
                    self.restart_search();
                } else {
                    self.model.retry();
                }
            }
        }
        None
    }

    /// A sidebar row's menu: its commands name the folder the row stands for, not the list
    /// the row sits beside.
    fn place_chosen(&mut self, path: PathBuf, command: MenuCommand) -> Option<PickerOutcome> {
        match command {
            MenuCommand::Open => {
                self.dirty = true;
                self.navigate(path);
            }
            MenuCommand::Properties => self.open_folder_properties(path),
            // The native menu and its merged rows read the place's folder; their paths are
            // already in the menu's own target.
            MenuCommand::ShellMenu => self.show_shell_menu(),
            _ => {}
        }
        None
    }

    /// Ask the platform for the clipboard, so the next `Change::Paste` lands where the
    /// menu was opened.
    fn request_paste(&mut self) {
        if self.file_operations.is_none() {
            return;
        }
        self.paste_destination = Some(self.menu_destination.clone());
        self.paste_requested = true;
        self.dirty = true;
    }

    /// Create a link to the selection in the folder a menu named.
    fn link_selection(&mut self, kind: LinkKind) {
        let paths = self.model.selection_paths();
        if paths.is_empty() {
            return;
        }
        let destination = self.menu_destination.clone();
        let request =
            OperationRequest::link(paths, &destination, kind).with_policy(ConflictPolicy::Ask);
        self.touched.push(destination);
        self.start(request);
    }

    /// Confirm the permanent delete the Shift chord asked about.
    fn confirm_delete(&mut self) {
        let Some(paths) = self.delete_ask.take() else {
            return;
        };
        self.start_delete(paths, DeleteMode::Permanent);
        self.dirty = true;
    }

    /// Make a folder named `base`, unique in this folder, and start renaming it.
    fn create_folder(&mut self, base: &str) {
        if self.model.searching() {
            // A search's rows belong to many folders; the new one has no row to stand in.
            return;
        }
        let folder = self.model.path().to_path_buf();
        let target = ui_platform::file_operation::available_name(&folder.join(base));
        match std::fs::create_dir(&target) {
            Ok(()) => {
                self.model.invalidate(&folder);
                self.model.select_when_listed(target.clone());
                let name = target
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| base.to_string());
                self.open_rename(target, name);
            }
            Err(error) => {
                self.failures = Some(Failures {
                    items: vec![(target, ui_platform::OpError::from_io(&error))],
                });
                self.dirty = true;
            }
        }
    }

    /// Whether a row's path is cut, for the view's dimming.
    pub(crate) fn is_cut(&self, path: &Path) -> bool {
        self.cut.iter().any(|cut| cut == path)
    }

    fn row_gesture(&mut self, gesture: RowGesture) -> Option<PickerOutcome> {
        self.dirty = true;
        let shape = self.range_shape();
        if let Some(row) = gesture.drag_row {
            self.model.drag_to(row, gesture.modifiers, shape);
            return None;
        }
        match self.model.row_clicked(
            gesture.index,
            gesture.modifiers,
            shape,
            gesture.trigger.is_some_and(RowTrigger::opens),
        ) {
            Action::Accept(paths) => self.confirm_or_answer(paths),
            Action::Redraw => {
                self.reveal = true;
                self.leave_search_if_navigated();
                None
            }
        }
    }

    /// Move the cursor `direction` rows the way a file manager's arrow does: plain selects,
    /// Shift extends from the anchor, Ctrl browses without changing the answer.
    fn step_selection(&mut self, direction: isize, held: Modifiers, vertical: bool) {
        let Some(next) = self.next_selection(direction, vertical) else {
            return;
        };
        let shape = self.range_shape();
        match (held.shift, held.ctrl) {
            (true, _) => self.model.extend_selection(next, shape),
            (false, true) => self.model.move_cursor(next),
            (false, false) => self.model.move_selection(next),
        };
    }

    /// The row `direction` screen rows away, `None` when there is nothing to move to.
    ///
    /// A vertical move in the grid keeps the cell's column, as Explorer does: the next row's
    /// same column is the target, and a shorter last row lands on its own last cell. A
    /// horizontal move is the linear neighbour, which is also what a whole grid row wraps to.
    fn next_selection(&self, direction: isize, vertical: bool) -> Option<usize> {
        let count = self.model.rows().len();
        if count == 0 {
            return None;
        }
        let last = count - 1;
        let Some(current) = self.model.cursor else {
            return Some(if direction > 0 { 0 } else { last });
        };
        if vertical
            && let Layout::Grid = self.layout
        {
            let columns = self.columns.max(1);
            let row = current / columns;
            let target = row as isize + direction;
            if target < 0 {
                return None;
            }
            return Some((target as usize * columns + current % columns).min(last));
        }
        Some(current.saturating_add_signed(direction).min(last))
    }

    /// What a range between two rows covers on screen, as the model marks it.
    fn range_shape(&self) -> RangeShape {
        match self.layout {
            Layout::Grid if self.columns > 1 => RangeShape::Grid {
                columns: self.columns,
            },
            _ => RangeShape::Column,
        }
    }

    /// An app-level shortcut fired; `Some` is the dialog's answer.
    ///
    /// A shortcut this picker does not answer leaves the state untouched, so an embedding app
    /// can offer the keys to something else instead.
    pub fn shortcut(&mut self, id: ShortcutId, held: Modifiers) -> Option<PickerOutcome> {
        // A modal question is the innermost scope: the list's keys wait until it is
        // answered, and only Enter and Escape still mean anything.
        if self.modal_open() && !matches!(id, OPEN | CANCEL) {
            return None;
        }
        match id {
            OPEN => {
                self.dirty = true;
                if self.overwrite_confirm.is_some() {
                    return self.replace_overwrite();
                }
                if self.confirm_modal() {
                    return None;
                }
                // A reading window has nothing to confirm, so Enter closes it first, as
                // Escape does. The next Enter is the dialog's own answer.
                if self.failures.is_some() {
                    self.dismiss_failures();
                    return None;
                }
                if self.properties.is_some() {
                    self.dismiss_properties();
                    return None;
                }
                if self.path_entry.is_some() {
                    self.commit_path();
                    return None;
                }
                let outcome = self.accept();
                self.leave_search_if_navigated();
                outcome
            }
            PATH_ENTRY => {
                self.open_path_entry();
                None
            }
            SEARCH => {
                self.open_search();
                None
            }
            SELECT_ALL => {
                self.dirty = true;
                self.model.select_all();
                None
            }
            CANCEL => {
                // A question, the in-place field or a menu is the innermost scope; Escape
                // closes it before it can close the dialog. The address and search fields
                // are the next scope out.
                if self.conflict.is_some() {
                    self.cancel_conflict();
                    return None;
                }
                if self.failures.is_some() {
                    self.dismiss_failures();
                    return None;
                }
                if self.delete_ask.take().is_some() {
                    self.dirty = true;
                    return None;
                }
                if self.undo_ask.take().is_some() {
                    self.dirty = true;
                    return None;
                }
                if self.transfer.is_some() {
                    self.dismiss_transfer();
                    return None;
                }
                if self.properties.is_some() {
                    self.dismiss_properties();
                    return None;
                }
                if self.rename.is_some() {
                    self.cancel_rename();
                    return None;
                }
                let overlay = self.overwrite_confirm.take().is_some()
                    | std::mem::take(&mut self.filter_menu)
                    | self.choice_menu.take().is_some()
                    | self.crumb_menu.take().is_some();
                if overlay {
                    self.dirty = true;
                    return None;
                }
                if self.path_entry.take().is_some() {
                    self.dirty = true;
                    return None;
                }
                if self.search.is_some() {
                    self.close_search();
                    return None;
                }
                Some(PickerOutcome::Cancelled)
            }
            CUT => {
                self.write_clipboard(ui_core::DropEffect::Move);
                None
            }
            COPY => {
                self.write_clipboard(ui_core::DropEffect::Copy);
                None
            }
            UNDO => {
                self.undo_last();
                None
            }
            PROPERTIES => {
                self.open_properties();
                None
            }
            RENAME => {
                self.begin_rename();
                None
            }
            TRASH => {
                self.delete_selection(false);
                None
            }
            DELETE => {
                self.delete_selection(true);
                None
            }
            NEW_FOLDER => {
                if !self.modal_open() {
                    self.new_folder = true;
                    self.dirty = true;
                }
                None
            }
            MENU => {
                // The row it opens for is the cursor's, and only a build holds the layout
                // that says where that row is.
                if !self.modal_open() {
                    self.menu_key = true;
                    self.dirty = true;
                }
                None
            }
            UP => {
                self.dirty = true;
                self.go_up();
                None
            }
            PREVIOUS => {
                self.dirty = true;
                self.step_selection(-1, held, true);
                self.reveal = true;
                None
            }
            NEXT => {
                self.dirty = true;
                self.step_selection(1, held, true);
                self.reveal = true;
                None
            }
            // The list has one column, so a horizontal arrow has nowhere to go: the picker
            // declares the chord either way, and in the list it moves nothing.
            PREVIOUS_COLUMN if self.layout == Layout::Grid => {
                self.dirty = true;
                self.step_selection(-1, held, false);
                self.reveal = true;
                None
            }
            NEXT_COLUMN if self.layout == Layout::Grid => {
                self.dirty = true;
                self.step_selection(1, held, false);
                self.reveal = true;
                None
            }
            FIRST => {
                self.dirty = true;
                self.model.select_first();
                self.reveal = true;
                None
            }
            LAST => {
                self.dirty = true;
                self.model.select_last();
                self.reveal = true;
                None
            }
            TOGGLE => {
                // A drag in flight has no selection to toggle; Space opens the folder the
                // hold has begun on, as it opens a hovered folder in the managers.
                if let Some(path) = self
                    .spring
                    .take()
                    .map(|spring| spring.path)
                    .or_else(|| self.spring_pending.take())
                {
                    self.open_spring(path);
                } else {
                    self.dirty = true;
                    self.model.toggle_cursor();
                }
                None
            }
            DESELECT => {
                self.dirty = true;
                self.model.clear_selection();
                None
            }
            _ => {
                if let Some((letter, _)) = TYPE_AHEAD.iter().find(|(_, shortcut)| *shortcut == id) {
                    self.type_ahead(*letter);
                }
                None
            }
        }
    }

    /// A letter jumped to the next name starting with it, as a file manager's type-ahead does.
    fn type_ahead(&mut self, letter: char) {
        // A question or a menu is the innermost scope: a letter pressed over one must not
        // move the list behind it.
        let overlay = self.modal_open()
            || self.filter_menu
            || self.choice_menu.is_some()
            || self.crumb_menu.is_some()
            // A typed path or a search owns the letters; the list behind it must not jump.
            || self.path_entry.is_some()
            || self.search.is_some();
        if overlay {
            return;
        }
        if self.model.type_ahead(letter) {
            self.dirty = true;
            self.reveal = true;
        }
    }

    /// The bindings a surface must install for [`shortcut`](Self::shortcut) to hear them.
    pub fn shortcuts() -> Vec<(KeyChord, ShortcutId)> {
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        let mut bindings = vec![
            (KeyChord::new(Key::Enter, Modifiers::NONE), OPEN),
            (KeyChord::new(Key::Enter, alt), PROPERTIES),
            (KeyChord::new(Key::Escape, Modifiers::NONE), CANCEL),
            (KeyChord::new(Key::Backspace, Modifiers::NONE), UP),
            (KeyChord::new(Key::Up, Modifiers::NONE).allowing(SHIFT_OR_CTRL), PREVIOUS),
            (KeyChord::new(Key::Down, Modifiers::NONE).allowing(SHIFT_OR_CTRL), NEXT),
            (KeyChord::new(Key::Left, Modifiers::NONE).allowing(SHIFT_OR_CTRL), PREVIOUS_COLUMN),
            (KeyChord::new(Key::Right, Modifiers::NONE).allowing(SHIFT_OR_CTRL), NEXT_COLUMN),
            (KeyChord::new(Key::Home, Modifiers::NONE), FIRST),
            (KeyChord::new(Key::End, Modifiers::NONE), LAST),
            (KeyChord::new(Key::Space, Modifiers::NONE), TOGGLE),
            (KeyChord::new(Key::Char('a'), ctrl), SELECT_ALL),
            (KeyChord::new(Key::Char('d'), ctrl), DESELECT),
            (KeyChord::new(Key::Char('l'), ctrl), PATH_ENTRY),
            (KeyChord::new(Key::Char('f'), ctrl), SEARCH),
            // The file verbs: the caret keeps these chords while it is open, because the
            // engine answers an editing input before an app binding.
            (KeyChord::new(Key::Char('x'), ctrl), CUT),
            (KeyChord::new(Key::Char('c'), ctrl), COPY),
            // The caret keeps Ctrl+Z while it is open, as it keeps the clipboard chords.
            (KeyChord::new(Key::Char('z'), ctrl), UNDO),
            (KeyChord::new(Key::Function(2), Modifiers::NONE), RENAME),
            (KeyChord::new(Key::Delete, Modifiers::NONE), TRASH),
            (KeyChord::new(Key::Delete, shift), DELETE),
            (KeyChord::new(Key::Char('n'), ctrl_shift), NEW_FOLDER),
            // Both desktops name the context-menu key and Shift+F10 `Key::Menu`, so one
            // binding covers a keyboard with the key and one without it.
            (KeyChord::new(Key::Menu, Modifiers::NONE), MENU),
        ];
        // Shift is allowed so a capital jumps like its lowercase: the platform reports the
        // shifted character and the chord matching compares letters case-insensitively.
        bindings.extend(
            TYPE_AHEAD
                .iter()
                .map(|(letter, shortcut)| {
                    (
                        KeyChord::new(Key::Char(*letter), Modifiers::NONE).allowing(shift),
                        *shortcut,
                    )
                }),
        );
        bindings
    }

    fn accept(&mut self) -> Option<PickerOutcome> {
        // The Replace question owns the next answer; a second press behind it changes
        // nothing.
        if self.overwrite_confirm.is_some() {
            return None;
        }
        match self.model.activate() {
            Action::Accept(paths) => self.confirm_or_answer(paths),
            Action::Redraw => {
                self.reveal = true;
                self.leave_search_if_navigated();
                None
            }
        }
    }

    /// Answer the accept, unless the overwrite policy asks first.
    ///
    /// The question is the drawn dialog's, not the provider's: the contract's
    /// `overwrite_prompt` capability is what lets a caller know it will be asked, and a
    /// dialog that answered straight away would replace a file the user never saw named.
    fn confirm_or_answer(&mut self, paths: Vec<PathBuf>) -> Option<PickerOutcome> {
        if self.model.mode().is_save()
            && self.model.overwrite() == OverwritePolicy::Prompt
            && let Some(clash) = paths.iter().find(|path| path.exists())
        {
            let name = clash
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| clash.display().to_string());
            self.overwrite_confirm = Some((paths, name));
            self.dirty = true;
            return None;
        }
        self.answer(paths)
    }

    fn answer(&self, paths: Vec<PathBuf>) -> Option<PickerOutcome> {
        Some(PickerOutcome::Selected(ChooserSelection {
            paths,
            filter: self.model.active_filter_index(),
            choices: self.choice_values(),
        }))
    }

    fn go_up(&mut self) {
        self.stop_search();
        self.path_entry = None;
        self.model.up();
        self.reveal = false;
    }
}

/// The dialog: a [`PickerState`] on a surface of its own, closed once it has answered.
pub struct Picker {
    state: PickerState,
    on_done: Option<Box<dyn FnOnce(PickerOutcome)>>,
    /// The answer was given; the next frame closes the window.
    closing: bool,
    /// The surface built at least one frame. A picker whose dialog could not open is
    /// dropped without ever drawing, and must not answer as if the user had closed it.
    started: bool,
}

impl Picker {
    pub fn new(options: PickerOptions, on_done: impl FnOnce(PickerOutcome) + 'static) -> Self {
        Self {
            state: PickerState::new(options),
            on_done: Some(Box::new(on_done)),
            closing: false,
            started: false,
        }
    }

    /// The shortcut bindings to pass to the surface's `OpenOptions`.
    pub fn shortcuts() -> Vec<(KeyChord, ShortcutId)> {
        PickerState::shortcuts()
    }

    fn answer(&mut self, outcome: PickerOutcome) {
        if let Some(on_done) = self.on_done.take() {
            on_done(outcome);
        }
        self.closing = true;
    }
}

impl Drop for Picker {
    /// A dialog the host retires without an answer — its parent closed, the process is
    /// tearing down — still answers once. The callback has seen exactly the same contract
    /// as the native provider's, whose entry the host flushes when the parent goes.
    fn drop(&mut self) {
        if !self.started {
            return;
        }
        if let Some(on_done) = self.on_done.take() {
            on_done(PickerOutcome::Cancelled);
        }
    }
}

impl App for Picker {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        self.started = true;
        if self.closing {
            // The answer is delivered; the window goes as soon as this frame is done.
            cx.close_window();
        }
        let (width, height) = (cx.width() as f32, cx.height() as f32);
        let tree = self.state.view(cx, width, height);
        // A field's Enter may have accepted while the tree was built; the answer goes out
        // after the frame that shows the result.
        if let Some(outcome) = self.state.take_outcome() {
            self.answer(outcome);
        }
        tree
    }

    fn changed(&mut self, id: Id, change: Change) -> bool {
        let outcome = self.state.changed(id, change);
        let dirty = std::mem::take(&mut self.state.dirty);
        if let Some(outcome) = outcome {
            self.answer(outcome);
            return true;
        }
        dirty
    }

    fn shortcut(&mut self, id: ShortcutId, held: Modifiers) -> bool {
        let outcome = self.state.shortcut(id, held);
        let dirty = std::mem::take(&mut self.state.dirty);
        if let Some(outcome) = outcome {
            self.answer(outcome);
            return true;
        }
        dirty
    }

    /// The rows' context menus are the engine's, so the pack that draws the dialog names
    /// their material; the appearance is the one the caller scoped `env` with.
    fn menu_style(&self) -> Option<ui_platform::menu::MenuStyle> {
        let appearance = StyleScope::of(&self.state.env).appearance;
        Some((self.state.menu_style_of)(appearance))
    }
}

impl PickerState {
    /// Whether the next frame has work an action requires; cleared by the frame that draws.
    ///
    /// An embedding app answers `changed`/`shortcut` with this so the engine rebuilds: the
    /// picker moved a selection or started reading a directory, and no other fact says so.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

/// How much of a name a rename selects: everything but the extension, as every manager
/// does, and the whole name for a dotfile or a name with no dot.
fn base_name_len(name: &str) -> usize {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => dot,
        _ => name.len(),
    }
}

/// Sort paths the way the dialog lists them: case-insensitively by the whole path, so one
/// batch reads in one order wherever it is shown.
pub(crate) fn sort_paths(paths: &mut [PathBuf]) {
    paths.sort_by_cached_key(|path| path.to_string_lossy().to_lowercase());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    use ui_platform::listing::{ListingRequest, ListingState};
    use ui_platform::Entry;

    struct Answers(RefCell<HashMap<ListingRequest, ListingState>>);

    impl ListingSource for Answers {
        fn get(&self, request: &ListingRequest) -> ListingState {
            match self.0.borrow().get(request) {
                Some(state) => state.clone(),
                None => ListingState::Loading,
            }
        }

        fn invalidate(&self, _path: &Path) {}
    }

    fn entry(path: &str) -> Entry {
        Entry {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: PathBuf::from(path),
            kind: ui_platform::FileKind::Generic,
            directory: false,
            link: false,
            len: 1,
            modified: None,
            hidden: false,
        }
    }

    struct Matches(std::sync::Mutex<SearchState>);

    impl SearchSource for Matches {
        fn start(&self, _request: &SearchRequest) {
            *self.0.lock().expect("the table") =
                SearchState::Done(Arc::new(vec![entry("/root/hit")]));
        }

        fn cancel(&self) {
            *self.0.lock().expect("the table") = SearchState::Idle;
        }

        fn state(&self) -> SearchState {
            self.0.lock().expect("the table").clone()
        }
    }

    fn state_at(root: &str) -> PickerState {
        let answers = Answers(RefCell::new(HashMap::from([
            (
                ListingRequest::new(root),
                ListingState::Ready(Arc::new(vec![entry(&format!("{root}/one"))])),
            ),
        ])));
        PickerState::new(
            PickerOptions::new(
                answers,
                root,
                ui_widgets_md3::pack_of,
                ui_widgets_md3::menu_style_of,
                ui_style::Env::root(),
            )
            .searches(Matches(std::sync::Mutex::new(SearchState::Idle))),
        )
    }

    fn searching_state() -> PickerState {
        let mut state = state_at("/root");
        state.model.poll();
        state.open_search();
        state.set_search_query("hit".to_string());
        state.apply_search();
        assert!(state.model.searching());
        assert_eq!(state.model.rows().len(), 1);
        assert_eq!(state.model.rows()[0].name, "hit");
        state
    }

    /// A folder whose child is another folder, so a drag has somewhere to spring into.
    fn spring_state() -> PickerState {
        let folder = Entry {
            name: "sub".to_string(),
            path: PathBuf::from("/root/sub"),
            kind: ui_platform::FileKind::Folder,
            directory: true,
            link: false,
            len: 0,
            modified: None,
            hidden: false,
        };
        let answers = Answers(RefCell::new(HashMap::from([
            (
                ListingRequest::new("/root"),
                ListingState::Ready(Arc::new(vec![folder])),
            ),
            (
                ListingRequest::new("/root/sub"),
                ListingState::Ready(Arc::new(vec![entry("/root/sub/child")])),
            ),
        ])));
        PickerState::new(PickerOptions::new(
            answers,
            "/root",
            ui_widgets_md3::pack_of,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        ))
    }

    /// Build one frame at `now`, which is what a hold's clock comes from.
    fn frame_at(state: &mut PickerState, store: &mut ui_core::AnimStore, now: f64) {
        let mut cx = ui_core::cx::Cx::new(store, 0, 900, 600, now, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
    }

    /// Spring-loading: a drag that rests on a folder opens it after the hold, and Space
    /// opens it at once instead of waiting.
    #[test]
    fn a_drag_resting_on_a_folder_opens_it_after_the_hold() {
        let mut store = ui_core::AnimStore::new();
        let mut state = spring_state();
        frame_at(&mut state, &mut store, 10.0);
        let folder = state.model.rows()[0].clone();
        assert!(folder.directory, "the premise: a folder row");
        assert!(state
            .changed(
                view::row_id(&folder),
                Change::DragEntered {
                    types: vec!["text/uri-list".to_string()],
                },
            )
            .is_none());
        frame_at(&mut state, &mut store, 10.0);
        frame_at(&mut state, &mut store, 10.9);
        assert_eq!(
            state.model.path(),
            Path::new("/root"),
            "a hold shorter than the delay opens nothing"
        );
        frame_at(&mut state, &mut store, 11.1);
        assert_eq!(
            state.model.path(),
            Path::new("/root/sub"),
            "the rest of the hold opened the folder the drag is over"
        );

        // Space is the same answer without the wait, and it must not toggle a selection
        // the drag never made.
        let mut state = spring_state();
        frame_at(&mut state, &mut store, 20.0);
        let folder = state.model.rows()[0].clone();
        state.changed(
            view::row_id(&folder),
            Change::DragEntered {
                types: vec!["text/uri-list".to_string()],
            },
        );
        frame_at(&mut state, &mut store, 20.0);
        assert!(state.shortcut(TOGGLE, Modifiers::NONE).is_none());
        assert_eq!(state.model.path(), Path::new("/root/sub"));
    }

    /// A target offers itself only for a drop that would change something, while entering a
    /// folder is navigation and happens whatever the release would do.
    #[test]
    fn a_drop_needs_a_change_but_an_entry_is_navigation() {
        let mut state = spring_state();
        state.drag_paths = vec![PathBuf::from("/root/sub")];
        assert!(
            !state.drop_accepts(Path::new("/root")),
            "the source already sits in this folder"
        );
        assert!(
            !state.drop_accepts(Path::new("/root/sub")),
            "a folder dropped on itself would take its subtree with it"
        );
        assert!(
            !state.drop_accepts(Path::new("/root/sub/child")),
            "the destination is inside the dragged folder"
        );
        assert!(state.drop_accepts(Path::new("/")), "another folder takes it");
        state.model.poll();
        let folder = state.model.rows()[0].clone();
        assert_eq!(
            state.spring_folder(view::row_id(&folder)),
            Some(PathBuf::from("/root/sub")),
            "entering is navigation: the folder the sources sit in still opens"
        );
        assert_eq!(
            state.drop_destination(view::crumb_id(Path::new("/"))),
            Some(PathBuf::from("/")),
            "a breadcrumb names the folder it stands for"
        );
    }

    /// A question is the innermost scope: Enter gives its default answer and no other key
    /// reaches the list behind the scrim.
    #[test]
    fn enter_answers_a_question_and_other_keys_wait() {
        let mut state = state_at("/root");
        state.model.poll();
        state.overwrite_confirm = Some((vec![PathBuf::from("/root/one")], "one".to_string()));
        assert!(state.shortcut(NEXT, Modifiers::NONE).is_none());
        assert!(
            state.overwrite_confirm.is_some(),
            "a list key must not answer the question"
        );
        assert_eq!(
            state.shortcut(OPEN, Modifiers::NONE),
            Some(PickerOutcome::Selected(ChooserSelection::new(vec![
                PathBuf::from("/root/one")
            ]))),
            "Enter takes the Replace question's first answer"
        );
        assert!(state.overwrite_confirm.is_none());
    }

    /// The platform menu leaves in a build, once, and the folder is read again after it.
    ///
    /// The popup runs the shell's own loop until it closes; calling it from the command
    /// froze the app, so the command only records the request and a frame hands it over.
    #[test]
    fn the_shell_menu_is_handed_to_the_host_and_the_folder_read_again() {
        let mut store = ui_core::AnimStore::new();
        let mut state = state_at("/root");
        state.model.poll();
        state.model.select_first();
        let at = ui_core::geom::Point::new(12.0, 34.0);
        state.menu_point = at;
        state.show_shell_menu();

        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 10.0, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
        let request = cx
            .take_shell_menu_request()
            .expect("the build hands the popup to the host");
        assert_eq!(request.paths, [PathBuf::from("/root/one")]);
        assert_eq!(request.at, at);

        // The host runs the popup after this frame; the build that follows is where a verb
        // it ran shows, so the listing is read again rather than kept stale.
        let _ = state.view(&mut cx, 900.0, 600.0);
        assert!(
            cx.take_shell_menu_request().is_none(),
            "one command asks for one popup"
        );
        assert!(
            state.model.selection_paths().is_empty(),
            "the rows may have moved under the selection, so it is dropped"
        );
    }

    /// A platform menu whose answer the test decides.
    struct FakeShell {
        answer: std::sync::Mutex<Option<ui_platform::ShellMenuRead>>,
        reads: std::sync::Mutex<Vec<Vec<PathBuf>>>,
        invoked: std::sync::Mutex<Vec<u32>>,
        invocations: std::sync::atomic::AtomicU64,
    }

    impl FakeShell {
        /// A host that also left `omitted` rows out of the merge.
        fn answering_partial(rows: Vec<ui_platform::ShellMenuRow>, omitted: usize) -> Arc<Self> {
            Arc::new(Self {
                answer: std::sync::Mutex::new(Some(ui_platform::ShellMenuRead { rows, omitted })),
                reads: std::sync::Mutex::new(Vec::new()),
                invoked: std::sync::Mutex::new(Vec::new()),
                invocations: std::sync::atomic::AtomicU64::new(0),
            })
        }

        /// A read that has not answered yet; one no test lands never answers at all.
        fn pending() -> Arc<Self> {
            Arc::new(Self {
                answer: std::sync::Mutex::new(None),
                reads: std::sync::Mutex::new(Vec::new()),
                invoked: std::sync::Mutex::new(Vec::new()),
                invocations: std::sync::atomic::AtomicU64::new(0),
            })
        }

        /// Land the answer now, as a read that finished after the menu had opened.
        fn answer_with(&self, rows: Vec<ui_platform::ShellMenuRow>, omitted: usize) {
            *self.answer.lock().expect("the answer") =
                Some(ui_platform::ShellMenuRead { rows, omitted });
        }
    }

    impl ui_platform::ShellMenuHost for FakeShell {
        fn available(&self, paths: &[PathBuf]) -> bool {
            !paths.is_empty()
        }

        fn read(&self, paths: Vec<PathBuf>) -> u64 {
            self.reads.lock().expect("the reads").push(paths);
            1
        }

        fn take_rows(&self, _ticket: u64) -> Option<ui_platform::ShellMenuRead> {
            self.answer.lock().expect("the answer").take()
        }

        fn invoke(&self, _paths: Vec<PathBuf>, token: u32) {
            self.invoked.lock().expect("the invokes").push(token);
            self.invocations
                .fetch_add(1, std::sync::atomic::Ordering::Release);
        }

        fn invocations(&self) -> u64 {
            self.invocations.load(std::sync::atomic::Ordering::Acquire)
        }

        fn show_native(&self, _paths: &[PathBuf], _at: (f32, f32)) -> bool {
            false
        }
    }

    fn shell_row(
        label: &str,
        token: u32,
        submenu: Option<Vec<ui_platform::ShellMenuRow>>,
    ) -> ui_platform::ShellMenuRow {
        ui_platform::ShellMenuRow {
            label: label.to_string(),
            token,
            enabled: true,
            checked: false,
            separator: false,
            submenu,
        }
    }

    /// The shell's rows join the row menu, and a merged row runs the command it names.
    #[test]
    fn the_shells_rows_join_the_menu_and_run_by_token() {
        let mut store = ui_core::AnimStore::new();
        let mut state = state_at("/root");
        state.model.poll();
        state.model.select_first();
        let host = FakeShell::answering_partial(
            vec![
                shell_row(
                    "Send to",
                    0,
                    Some(vec![shell_row("Compressed folder", 3, None)]),
                ),
                shell_row("Pin to Quick access", 4, None),
            ],
            1,
        );
        state.shell_menu = Some(host.clone());
        let at = ui_core::geom::Point::new(12.0, 34.0);
        state.context_menu = Some((at, MenuTarget::Row(0)));

        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 10.0, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
        let request = cx
            .take_menu_requests()
            .into_iter()
            .next()
            .expect("the menu opens once the rows are in");
        let shell = request
            .items
            .iter()
            .find(|item| item.label == "Shell Menu")
            .expect("the shell's rows joined the menu");
        let ui_core::menu::MenuRow::Submenu(children) = &shell.kind else {
            panic!("the shell's rows hang behind one row");
        };
        let ui_core::menu::MenuRow::Submenu(send_to) = &children[0].kind else {
            panic!("the shell's own cascade is carried as a cascade");
        };
        let merged = &send_to[0];
        assert_eq!(merged.label, "Compressed folder");
        let note = children.last().expect("the omitted rows are accounted for");
        assert_eq!(
            note.label, "Some items are only in the system menu",
            "the merge says what the native menu still holds"
        );
        assert!(!note.enabled, "the note is not a command");

        state.changed(Id::default(), Change::MenuItem(merged.id));
        assert_eq!(
            *host.invoked.lock().expect("the invokes"),
            [3],
            "the row runs the command id the read reported"
        );

        // A finished command may have changed the folder; the next build reads it again.
        let _ = state.view(&mut cx, 900.0, 600.0);
        assert!(
            state.model.selection_paths().is_empty(),
            "the listing starts over after the command"
        );
    }

    /// A menu with nothing readable, or one the host is too slow over, opens with the native
    /// row.
    #[test]
    fn an_unreadable_shell_menu_opens_with_the_native_row() {
        let mut store = ui_core::AnimStore::new();
        let at = ui_core::geom::Point::new(12.0, 34.0);

        // The read answered, but no row could be carried.
        let mut state = state_at("/root");
        state.model.poll();
        state.model.select_first();
        state.shell_menu = Some(FakeShell::answering_partial(Vec::new(), 2));
        state.context_menu = Some((at, MenuTarget::Row(0)));
        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 10.0, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
        let request = cx
            .take_menu_requests()
            .into_iter()
            .next()
            .expect("the menu opens anyway");
        assert!(
            request.items.iter().all(|item| item.label != "Shell Menu"),
            "no rows to merge"
        );
        assert!(
            request
                .items
                .iter()
                .any(|item| item.id == MenuCommand::ShellMenu.id()),
            "the native menu stays one row of ours"
        );

        // A read that never answers must not hold the menu for good.
        let mut state = state_at("/root");
        state.model.poll();
        state.model.select_first();
        state.shell_menu = Some(FakeShell::pending());
        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 20.0, 1.0 / 60.0, 1.0, None);
        state.context_menu = Some((at, MenuTarget::Row(0)));
        let _ = state.view(&mut cx, 900.0, 600.0);
        assert!(
            cx.take_menu_requests().is_empty(),
            "a slow read holds the menu back for a moment"
        );
        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 20.3, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
        assert!(
            cx.take_menu_requests()
                .into_iter()
                .next()
                .is_some_and(|request| request
                    .items
                    .iter()
                    .any(|item| item.id == MenuCommand::ShellMenu.id())),
            "the wait ends and the native row is what the menu offers"
        );
    }

    /// A read that answers after the menu opened still merges, without a second press.
    ///
    /// The first read of a file type is the slow one, because the shell loads its extensions
    /// for it; the ticket used to be dropped at the wait's end, so that press showed no
    /// "Shell Menu" row and only the next one did.
    #[test]
    fn a_shell_read_that_answers_late_joins_the_open_menu() {
        let mut store = ui_core::AnimStore::new();
        let mut state = state_at("/root");
        state.model.poll();
        state.model.select_first();
        let host = FakeShell::pending();
        state.shell_menu = Some(host.clone());
        let at = ui_core::geom::Point::new(12.0, 34.0);
        state.context_menu = Some((at, MenuTarget::Row(0)));

        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 20.0, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
        assert!(
            cx.take_menu_requests().is_empty(),
            "the read holds the menu back while it runs"
        );
        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 20.3, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
        let opened = cx
            .take_menu_requests()
            .into_iter()
            .next()
            .expect("the wait ends and the menu opens without the rows");
        assert!(
            opened
                .items
                .iter()
                .all(|item| item.label != "Shell Menu"),
            "nothing was read yet"
        );

        host.answer_with(vec![shell_row("Pin to Quick access", 4, None)], 0);
        let mut cx = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 20.4, 1.0 / 60.0, 1.0, None);
        let _ = state.view(&mut cx, 900.0, 600.0);
        let update = cx
            .take_menu_update()
            .expect("the late rows replace the open menu's rows");
        assert_eq!(update.owner, view::row_id(&state.model.rows()[0]));
        assert!(
            update
                .items
                .iter()
                .any(|item| item.label == "Shell Menu"),
            "the merge reaches the menu the press opened"
        );
    }

    #[test]
    fn closing_a_search_puts_the_directory_back() {
        let mut state = searching_state();
        state.close_search();
        state.model.poll();
        assert!(!state.model.searching());
        assert_eq!(state.model.rows()[0].name, "one");
    }

    #[test]
    fn going_up_at_a_root_leaves_the_search() {
        let mut state = state_at("/");
        state.model.poll();
        state.open_search();
        state.set_search_query("hit".to_string());
        state.apply_search();
        assert!(state.model.searching());
        state.go_up();
        state.model.poll();
        assert!(!state.model.searching());
        assert_eq!(state.model.rows()[0].name, "one");
    }
}