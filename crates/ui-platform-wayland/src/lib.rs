//! Wayland XDG window target: `wl_shm` reference path plus an optional dma-buf path.

#![cfg(unix)]

pub mod backend;
pub(crate) mod clipboard;
pub mod clipboard_host;
pub mod colour;
pub(crate) mod conn;
pub(crate) mod cursor_shape;
pub mod dmabuf;
#[cfg(feature = "gpu")]
pub mod gpu_window;
pub mod input;
pub mod ipc;
pub mod link;
#[cfg(feature = "global-hotkeys")]
pub mod portal_hotkeys;
pub mod session;
pub mod shm;
pub mod theme;
pub(crate) mod wheel;
pub mod window;

pub use backend::WaylandBackend;
pub use clipboard_host::ClipboardHost;
pub use dmabuf::{DmaBufBuffer, DmaBufError, DmaBufFormats, DmaBufImage, FormatSupport};
#[cfg(feature = "gpu")]
pub use gpu_window::{
    ColourPolicy, GpuWindow, GpuWindowConfig, GpuWindowError, PresentOutcome, SurfaceColour,
};
pub use input::{InputState, KeyEvent};
pub use link::WaylandLink;
#[cfg(feature = "global-hotkeys")]
pub use portal_hotkeys::PortalHotkeys;
pub use session::Session;
pub use ui_platform::session::{HostSession, SessionOptions, Tick};
pub use shm::ShmBuffer;
pub use theme::PortalThemeWatcher;
#[cfg(feature = "gpu")]
pub use ui_paint_gpu::partial_frames;
pub use window::{HdrSurface, WaylandWindow, WindowError};
