//! The host an app runs on: one window backend per target, behind one name.
//!
//! An app depends on this crate and never names `ui-platform-wayland` or
//! `ui-platform-win32`. Which backend exists is the target tables' decision; the re-exports
//! below are the only place that choice appears, and app code writes none.
//!
//! Picking a backend is not the same as picking a capability. What one host can answer and
//! the other cannot is a runtime question, and the backend answers it with `Option` — a
//! frame rate, a dma-buf count — rather than by not existing.

#[cfg(unix)]
pub use ui_platform_wayland::{
    PortalThemeWatcher as ThemeWatch, Session, WaylandBackend as HostBackend, allow_debugger,
};

#[cfg(windows)]
pub use ui_platform_win32::{
    Session, Win32Backend as HostBackend, Win32ThemeWatcher as ThemeWatch, allow_debugger,
};

// The options and the turn belong to the portable contract, not to a backend: restating them
// on the platform side would give one app two structs that drift apart.
pub use ui_platform::{HostSession, SessionOptions, Tick};

/// The process's backend, opened.
///
/// A Wayland session has a display connection to make, so this can fail there; on Windows
/// the message queue already belongs to the thread, so nothing is opened.
pub fn connect() -> Result<HostBackend, String> {
    #[cfg(unix)]
    {
        ui_platform_wayland::WaylandBackend::connect().map_err(|error| error.to_string())
    }
    #[cfg(windows)]
    {
        Ok(ui_platform_win32::Win32Backend::new())
    }
}

// A target with no backend is a missing host, not a missing import; say which one it is.
#[cfg(not(any(unix, windows)))]
compile_error!("ui-host has no window backend for this target");
