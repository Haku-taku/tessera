//! The rest of the loop, run from a modal gesture's own message loop.
//!
//! A platform gesture — a title-bar move, an OLE drag — runs a message loop of the shell's
//! own and does not return until the pointer is released. The surface that started it keeps
//! drawing from that loop's timer, but the host's turn is suspended, so every other surface
//! would hold its last frame for the whole gesture. The host installs this hook around each
//! surface's step; the platform's timer calls [`pump_others`], and the host steps the rest.
//!
//! One hook per thread: a process has one loop, and a gesture runs on the thread that
//! entered it. The pointer is never read after the step that installed it returns.

use std::cell::Cell;

thread_local! {
    /// The rest-of-the-loop pump the host installed for one surface's step.
    ///
    /// The pointer is only valid between [`arm`] and [`disarm`]: the host keeps the closure
    /// alive for the whole step, and the modal loop calls back on this thread, inside it.
    static OTHERS: Cell<Option<*mut dyn FnMut()>> = const { Cell::new(None) };
}

/// Make `pump` the rest-of-the-loop pump for the duration of one surface's step.
pub(crate) fn arm(pump: &mut dyn FnMut()) {
    // SAFETY: the host keeps the closure alive for the whole step, and a modal loop runs on
    // this thread and calls back only while that step is on the stack. The lifetime is
    // erased, not the validity.
    let erased: *mut dyn FnMut() = unsafe {
        std::mem::transmute::<*mut dyn FnMut(), *mut dyn FnMut()>(pump)
    };
    OTHERS.with(|slot| slot.set(Some(erased)));
}

/// Drop the pump [`arm`] installed.
pub(crate) fn disarm() {
    OTHERS.with(|slot| slot.set(None));
}

/// Run the rest of the loop from a modal gesture's timer; false when no host is stepping.
///
/// The gesture's own surface is not behind this hook: the host takes it out of the surface
/// list while its step is on the stack, and the platform draws it from the same timer.
pub fn pump_others() -> bool {
    let armed = OTHERS.with(Cell::get);
    let Some(pump) = armed else {
        return false;
    };
    // SAFETY: `arm` stores a closure that outlives the step, and `disarm` clears the slot
    // before that step returns.
    unsafe { (*pump)() };
    true
}
