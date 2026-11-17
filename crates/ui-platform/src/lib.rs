//! Presentation targets and the headless windowed app loop.

#[cfg(test)]
mod view_menu_test;
#[cfg(test)]
mod view_scroll_test;

use std::time::Duration;

/// How long a shared cache keeps answers nobody has asked for. A closed picker's pictures and
/// listings are the app's memory until they prove useful again; an idle one must not pin them.
pub(crate) const IDLE_WINDOW: Duration = Duration::from_secs(60);

pub mod chooser;
pub mod clipboard;
pub mod cursor;
pub mod driver;
pub mod element;
pub mod exit_probe;
pub mod file_icons;
pub mod file_operation;
pub mod files;
pub mod host;
pub mod hotkeys;
pub mod ipc;
pub mod jobs;
pub mod keep_alive;
pub mod listing;
pub mod locale;
pub mod menu;
pub mod modal;
pub mod output;
mod pictures;
pub mod places;
pub mod present;
pub mod search;
pub mod session;
pub mod shell_menu;
pub mod shortcut;
pub mod state;
pub mod text_pass;
pub mod theme;
pub mod thumbnails;
pub mod transfer;
pub mod tray;
pub mod uri_list;
pub mod view;
pub mod volumes;
pub mod watch;
pub mod window;

pub use chooser::{
    ChoiceKind, ChoiceOption, ChooserAnswer, ChooserCapabilities, ChooserChoice, ChooserError,
    ChooserId, ChooserMode, ChooserPlace, ChooserPreference, ChooserRequest, ChooserSelection,
    ChooserSource, FileTypeFilter, OverwritePolicy, SaveOptions, StartAt,
};
pub use clipboard::ClipboardCopy;
pub use element::{Container, Element, Leaf, from_decl};
pub use file_icons::{
    BadgeCorner, FileBadge, FileIcon, FileIconProvider, FileIconRequest, FileIconState, FileIcons,
};
pub use file_operation::{
    Conflict, ConflictDecision, ConflictPolicy, DeleteMode, FileOperations, FileOperator,
    FsOperator, LinkKind, OpError, OperationEvent, OperationId, OperationKind, OperationProgress,
    OperationReport, OperationRequest, Outcome, UndoAction, UndoCause, UndoJournal, available_name,
    unique_name,
};
pub use files::{FileKind, mime_type};
pub use host::{
    Backend, Effect, Engine, Host, LoopCtx, MessageCtx, OpenOptions, PresentationOptions, RunStats,
    SurfaceHandle, SurfaceId, SurfaceStep,
};
pub use keep_alive::KeepAlive;
pub use locale::posix_system_languages;
pub use listing::{DirectoryLister, Entry, ListingRequest, ListingState, Listings};
pub use output::OutputInfo;
pub use places::{Place, PlaceKind};
pub use present::{OffscreenTarget, PresentTarget, SurfaceKind};
pub use session::{HostSession, SessionOptions, Tick};
pub use shell_menu::{ShellMenuHost, ShellMenuRead, ShellMenuRow};
pub use state::WidgetState;
pub use text_pass::TextPass;
pub use theme::{FixedTheme, ThemePreference, ThemeWatcher};
pub use thumbnails::{
    FileVersion, ThumbnailPicture, ThumbnailRequest, ThumbnailState, Thumbnailer, Thumbnails,
};
pub use transfer::{Selection, SelectionReader};
pub use view::{App, AppLoop, FrameInfo, run_app};
pub use volumes::{Mountable, VolumeMounter, Volumes, VolumesState};
pub use window::{
    Anchor, AutoHide, Constraint, ExclusiveZone, KeyboardMode, Layer, LayerSpec, PopupSpec,
    SurfaceRole, SurfaceSpec, place_popup,
};
