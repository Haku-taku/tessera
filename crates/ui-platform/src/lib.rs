//! Presentation targets and the headless windowed app loop.

#[cfg(test)]
mod view_scroll_test;

pub mod clipboard;
pub mod cursor;
pub mod driver;
pub mod element;
pub mod exit_probe;
pub mod host;
pub mod hotkeys;
pub mod ipc;
pub mod jobs;
pub mod locale;
pub mod menu;
pub mod output;
pub mod present;
pub mod session;
pub mod shortcut;
pub mod state;
pub mod text_pass;
pub mod theme;
pub mod transfer;
pub mod tray;
pub mod view;
pub mod window;

pub use clipboard::ClipboardCopy;
pub use element::{Container, Element, Leaf, from_decl};
pub use host::{
    Backend, Effect, Host, LoopCtx, MessageCtx, OpenOptions, RunStats, SurfaceHandle, SurfaceId,
    SurfaceStep,
};
pub use locale::posix_system_languages;
pub use output::OutputInfo;
pub use present::{OffscreenTarget, PresentTarget, SurfaceKind};
pub use session::{HostSession, SessionOptions, Tick};
pub use state::WidgetState;
pub use text_pass::TextPass;
pub use theme::{FixedTheme, ThemePreference, ThemeWatcher};
pub use transfer::{Selection, SelectionReader};
pub use view::{App, AppLoop, FrameInfo, run_app};
pub use window::{
    Anchor, AutoHide, ExclusiveZone, KeyboardMode, Layer, LayerSpec, SurfaceRole, SurfaceSpec,
};
