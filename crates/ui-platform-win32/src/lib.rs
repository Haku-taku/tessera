//! Win32 window and DXGI swap chain: the engine's scene on a real Windows window.
//!
//! The crate is empty off Windows. A Win32 window and a flip-model swap chain cannot exist
//! without user32 and dxgi, and there is nothing for a non-Windows build to fall back to, so
//! the workspace builds an empty library there.
//!
//! The swap chain follows its output into HDR when Windows is composing there, and stays
//! 8-bit sRGB otherwise. A screen capture duplicates one output or runs a capture session over
//! it, and returns the sRGB codes a window drew, even where Windows composes the desktop in
//! linear light. A clipboard copy is written in every format its content has, and a read
//! turns those formats back into text, a file list or a PNG. Keys, the pointer, the wheel,
//! text and the input method's composition reach the engine as its own events.

#![cfg(windows)]

mod backend;
mod capture;
mod clipboard;
mod dnd;
mod hotkeys;
mod ime;
mod input;
mod ipc;
mod locale;
mod modal;
mod outputs;
mod presenter;
mod session;
mod theme;
mod tray;
mod tsf;
mod wgc;
mod window;

pub use backend::Win32Backend;
pub use capture::{ScreenCapture, ScreenFrame};
pub use hotkeys::WINDOW_CLASS as HOTKEY_WINDOW_CLASS;
pub use locale::system_languages;
pub use outputs::HdrOutput;
pub use presenter::Win32Presenter;
pub use session::Session;
pub use theme::Win32ThemeWatcher;
pub use tsf::serves_input;
pub use ui_platform::session::{HostSession, SessionOptions, Tick};
pub use wgc::GraphicsCapture;
pub use window::Win32Window;

/// Why the window or its presentation path could not open.
#[derive(Debug)]
pub enum WindowError {
    /// A Win32 call failed.
    Win32(String),
    /// D3D11 or DXGI could not set up the device, the renderer or the swap chain.
    Gpu(String),
}

impl core::fmt::Display for WindowError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Win32(message) => write!(f, "win32: {message}"),
            Self::Gpu(message) => write!(f, "gpu: {message}"),
        }
    }
}

impl std::error::Error for WindowError {}

/// Let an outside debugger attach to this process.
///
/// Windows has no ptrace scope to widen: a debugger attaches on its own, so this is the
/// Windows answer to the same call, and it does nothing.
pub fn allow_debugger() {}
