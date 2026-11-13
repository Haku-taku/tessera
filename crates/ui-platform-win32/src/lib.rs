//! Win32 window and DXGI swap chain: the engine's scene on a real Windows window.
//!
//! The crate is empty off Windows. A Win32 window and a flip-model swap chain cannot exist
//! without user32 and dxgi, and there is nothing for a non-Windows build to fall back to, so
//! the workspace builds an empty library there.
//!
//! What it does not do yet: input-method composition and non-text clipboard formats. Keys,
//! the pointer, the wheel and text reach the engine as its own events; the IME stays with
//! the window, so a composed character is not delivered.

#![cfg(windows)]

mod input;
mod presenter;
mod session;
mod theme;
mod window;

pub use presenter::Win32Presenter;
pub use session::Session;
pub use ui_platform::session::{HostSession, SessionOptions, Tick};
pub use theme::Win32ThemeWatcher;
pub use window::Win32Window;

/// Why the window or its presentation path could not open.
#[derive(Debug)]
pub enum WindowError {
    /// A Win32 call failed.
    Win32(String),
    /// Only the role of a window the manager places exists on this backend.
    UnsupportedRole,
    /// D3D11 or DXGI could not set up the device, the renderer or the swap chain.
    Gpu(String),
}

impl core::fmt::Display for WindowError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Win32(message) => write!(f, "win32: {message}"),
            Self::UnsupportedRole => write!(f, "this backend only opens windows, not layers"),
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
