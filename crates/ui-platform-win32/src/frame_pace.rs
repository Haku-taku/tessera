//! The frame-latency waitable: when the compositor will take the next frame.
//!
//! A flip-model chain hands out a waitable object that signals when fewer than the
//! configured maximum of frames are in flight. Waiting on it is what lets `Present` return
//! immediately: the surface draws only when the compositor has room, and a loop that wakes
//! on messages while it waits keeps input responsive in between.
//!
//! One of these belongs to a surface, not to a frame: the presenter owns the handle (the
//! chain closes it), the window answers [`can_draw`](FramePace::can_draw) from it, and the
//! backend adds it to the host's wait set.

use std::cell::Cell;

use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::WaitForSingleObject;

/// Whether one surface may present now, shared by the three callers above.
pub(crate) struct FramePace {
    /// The chain's waitable object; `None` when the chain refused one, which leaves pacing
    /// to `DwmFlush` and `Present(1)` as before.
    handle: Cell<Option<HANDLE>>,
    /// The compositor has opened a frame window. A surface starts drawable: nothing has been
    /// presented yet, so there is nothing to wait for.
    ready: Cell<bool>,
}

impl FramePace {
    pub(crate) fn new() -> Self {
        Self {
            handle: Cell::new(None),
            ready: Cell::new(true),
        }
    }

    /// Adopt a chain's waitable object, or report that it has none.
    ///
    /// Called again when a surface rebuilds its chain; the previous handle dies with the
    /// chain, so it is replaced rather than closed here.
    pub(crate) fn set_handle(&self, handle: Option<HANDLE>) {
        self.handle.set(handle);
        if handle.is_none() {
            // Without a waitable there is no window to wait for: every turn may draw.
            self.ready.set(true);
        }
    }

    pub(crate) fn handle(&self) -> Option<HANDLE> {
        self.handle.get()
    }

    /// Whether a frame may be drawn now.
    pub(crate) fn can_draw(&self) -> bool {
        self.handle.get().is_none() || self.ready.get()
    }

    /// A frame went to the chain: the next one waits for the compositor's window.
    pub(crate) fn presented(&self) {
        self.ready.set(false);
    }

    /// The compositor opened the window (or a deadline said to stop waiting for it).
    pub(crate) fn opened(&self) {
        self.ready.set(true);
    }

    /// Consume a signal that is already set without blocking; true when there was one.
    ///
    /// Used for the surfaces a multi-handle wait did not return on: the object is auto-reset,
    /// so this both records the window and clears it for the next one.
    pub(crate) fn take_signal(&self) -> bool {
        let Some(handle) = self.handle.get() else {
            return false;
        };
        // SAFETY: the chain owns the handle for as long as the presenter holds the chain.
        if unsafe { WaitForSingleObject(handle, 0) } == WAIT_OBJECT_0 {
            self.ready.set(true);
            true
        } else {
            false
        }
    }
}
