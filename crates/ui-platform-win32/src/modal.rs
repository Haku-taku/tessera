//! Keeping the surface painting inside a system modal loop.
//!
//! Two gestures on Windows run a message loop of the shell's own on this thread, and do
//! not return until the pointer is released: OLE's `DoDragDrop`, and the title-bar move
//! that `DefWindowProc` starts. The host's frame loop is suspended for the whole call, but
//! both loops dispatch `WM_TIMER` to this window, so a timer armed around the call is what
//! lets the engine build and present a frame in the meantime. The pump itself is the
//! caller's, because only the turn that started the gesture can reach the engine.

use std::cell::Cell;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

/// Eight milliseconds keeps an animation alive without presenting anything a still app
/// would not have presented anyway.
const TICK_MS: u32 = 8;

/// The timer id; any id not used elsewhere on this window will do.
pub(crate) const TIMER_ID: usize = 0xD0D0;

thread_local! {
    /// The frame pump a modal gesture armed for its own duration.
    ///
    /// The pointer is only valid between [`arm`] and [`disarm`]: the caller keeps the
    /// closure alive for the whole modal call it is about to make, and the loop only calls
    /// back on this thread, inside that call.
    static PUMP: Cell<Option<*mut dyn FnMut()>> = const { Cell::new(None) };
    /// A frame is being built: a message it synchronously dispatches must not build another.
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// Make `pump` run from this window's timer until [`disarm`]; returns the timer's id, or
/// zero when the timer could not be started.
///
/// A gesture whose timer fails still runs; it simply keeps the last frame.
pub(crate) fn arm(hwnd: HWND, pump: &mut dyn FnMut()) -> usize {
    // SAFETY: the slot is only read while the modal call is on the stack, on this thread,
    // and the closure outlives it; the lifetime is erased, not the validity.
    let erased: *mut dyn FnMut() = unsafe {
        std::mem::transmute::<*mut dyn FnMut(), *mut dyn FnMut()>(pump)
    };
    PUMP.with(|slot| slot.set(Some(erased)));
    // SAFETY: the window is this thread's and the id is ours.
    unsafe { SetTimer(Some(hwnd), TIMER_ID, TICK_MS, None) }
}

/// Stop the timer [`arm`] started and drop its pump.
pub(crate) fn disarm(hwnd: HWND, timer: usize) {
    PUMP.with(|slot| slot.set(None));
    if timer != 0 {
        // SAFETY: the id came back from `SetTimer` on this window.
        unsafe {
            let _ = KillTimer(Some(hwnd), timer);
        }
    }
}

/// Run the frame pump a modal gesture armed, if any; true when one ran.
///
/// The window procedure calls this from `WM_TIMER`, which the shell's loop dispatches:
/// that is how a drag or a window move stops freezing the surface it started from.
pub(crate) fn pump_frame() -> bool {
    let armed = PUMP.with(Cell::get);
    let Some(pump) = armed else {
        return false;
    };
    if BUSY.with(Cell::get) {
        return false;
    }
    BUSY.with(|busy| busy.set(true));
    // SAFETY: `arm` stores a closure that outlives the whole modal call, and `disarm`
    // clears the slot before the borrow of the caller's frame ends.
    unsafe { (*pump)() };
    BUSY.with(|busy| busy.set(false));
    true
}
