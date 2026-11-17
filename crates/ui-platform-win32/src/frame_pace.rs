//! One surface's frame window: whether the loop may build, and what to wait on.
//!
//! The loop builds a frame and hands it to the render thread, which presents it and sets
//! an event when it is done. That event is this pace's, and it is what the host's one
//! sleep and the blocking `tick` wait on. The render thread paces itself: `Present`
//! blocks at the compositor's vertical blank, so the loop's next frame is built while
//! the previous one is on the GPU rather than after it.
//!
//! The object is shared across threads, so its state is atomics; the event is created
//! here and closed when the last holder drops.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::core::PCWSTR;

/// One surface's frame window.
pub(crate) struct FramePace {
    /// The auto-reset event the render thread sets when it finishes a frame; `0` when the
    /// event could not be created, which leaves the flag alone as the gate.
    event: AtomicIsize,
    /// A frame may be built: true before the first frame and after each one completes.
    ready: AtomicBool,
}

impl FramePace {
    pub(crate) fn new() -> Self {
        // The event is only a wake signal, so a failure costs a polling loop, not correctness.
        let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .map(|handle| handle.0 as isize)
            .unwrap_or(0);
        Self {
            event: AtomicIsize::new(event),
            ready: AtomicBool::new(true),
        }
    }

    /// The event the loop and the host's wait set wake on, for "the frame is finished".
    pub(crate) fn handle(&self) -> Option<HANDLE> {
        let raw = self.event.load(Ordering::Acquire);
        (raw != 0).then_some(HANDLE(raw as *mut core::ffi::c_void))
    }

    /// Whether a frame may be built now.
    pub(crate) fn can_draw(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// The loop handed a frame to the render thread: the next one waits for it.
    pub(crate) fn presented(&self) {
        self.ready.store(false, Ordering::Release);
    }

    /// The render thread finished a frame (or gave up on one): the loop may draw again.
    pub(crate) fn finished(&self) {
        self.ready.store(true, Ordering::Release);
        if let Some(handle) = self.handle() {
            // SAFETY: the handle is this object's event and outlives the call.
            let _ = unsafe { SetEvent(handle) };
        }
    }

    /// Open the gate without waking anyone: a frame the loop could not hand over.
    pub(crate) fn release(&self) {
        self.ready.store(true, Ordering::Release);
    }

    /// Consume a signal that is already set without blocking; true when there was one.
    ///
    /// The host's wait can return on the event; consuming it there keeps the next wake
    /// meaningful. It opens no gate: `finished` records readiness before it sets the event,
    /// and a signal the loop sent a frame after is an older frame's, not this one's.
    pub(crate) fn take_signal(&self) -> bool {
        let Some(handle) = self.handle() else {
            return false;
        };
        // SAFETY: the event belongs to this object for as long as it is held.
        (unsafe { WaitForSingleObject(handle, 0) }) == WAIT_OBJECT_0
    }
}

impl Drop for FramePace {
    fn drop(&mut self) {
        let raw = *self.event.get_mut();
        if raw != 0 {
            // Nothing else waits on it: every holder is an `Arc` that is gone by now.
            let _ = unsafe { CloseHandle(HANDLE(raw as *mut core::ffi::c_void)) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame the loop sent while the previous one's signal was still set must keep the
    /// gate shut: building over a frame the render thread has not taken throws that frame
    /// away, and with it the atlas page it carried.
    #[test]
    fn a_signal_from_the_previous_frame_does_not_open_the_gate() {
        let pace = FramePace::new();
        pace.presented();
        pace.finished();
        assert!(pace.can_draw());
        // The next frame goes out before the host consumed the last signal, which is what
        // a turn that had input to dispatch does.
        pace.presented();
        assert!(pace.take_signal(), "the stale signal is there to be drained");
        assert!(!pace.can_draw(), "the frame just sent is still in flight");
    }
}
