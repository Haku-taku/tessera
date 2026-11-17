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
pub(crate) const TICK_MS: u32 = 8;

/// The timer id a gesture's pump is armed on; any id not used elsewhere will do.
pub(crate) const TIMER_ID: usize = 0xD0D0;

/// The timer id the shell's move or resize loop draws from.
///
/// Its own id, so a drag that armed [`TIMER_ID`] and a move the shell started never kill
/// each other's timer.
pub(crate) const SHELL_TIMER_ID: usize = 0xD0D1;

thread_local! {
    /// The frame pump a modal gesture armed for its own duration.
    ///
    /// The pointer is only valid between [`arm`] and [`disarm`]: the caller keeps the
    /// closure alive for the whole modal call it is about to make, and the loop only calls
    /// back on this thread, inside that call.
    static PUMP: Cell<Option<*mut dyn FnMut()>> = const { Cell::new(None) };
    /// The slot holds a step's move-loop pump that no loop reads yet: [`attach`] fills it
    /// before the step, in case a caption press inside that step starts the shell's move.
    /// A gesture that starts inside the step is the only loop that will read the slot, so
    /// it may take it over; an active loop's pump may not be displaced.
    static STANDBY: Cell<bool> = const { Cell::new(false) };
    /// A frame is being built: a message it synchronously dispatches must not build another.
    static BUSY: Cell<bool> = const { Cell::new(false) };
    /// Nesting depth of the shell's own move or resize loop on this thread.
    ///
    /// That loop dispatches this thread's messages until the pointer comes up, so nobody
    /// else may take them: a peek here would hand the move its own mouse messages outside
    /// the loop, and the window would stop following the pointer.
    static SHELL: Cell<u32> = const { Cell::new(0) };
}

/// Record that the shell's move or resize loop has taken this thread.
///
/// Counted, not a flag: a system menu opened inside a move opens a second loop.
pub(crate) fn enter_shell() {
    SHELL.with(|depth| depth.set(depth.get() + 1));
    // The slot now belongs to a loop that is reading it.
    STANDBY.with(|standby| standby.set(false));
}

/// Record that the shell's loop returned.
pub(crate) fn exit_shell() {
    SHELL.with(|depth| depth.set(depth.get().saturating_sub(1)));
}

/// Make `pump` run from this window's timer until [`disarm`]; returns the timer's id, or
/// zero when the timer could not be started.
///
/// A gesture whose timer fails still runs; it simply keeps the last frame.
pub(crate) fn arm(hwnd: HWND, pump: &mut dyn FnMut()) -> usize {
    if PUMP.with(Cell::get).is_some() && !STANDBY.with(Cell::get) {
        // A gesture's timer is already armed on this thread and serves the pump it has.
        // Overwriting the slot would leave the outer gesture without frames, and this
        // gesture simply runs without one — it still moves, the way it did before the pump.
        return 0;
    }
    // SAFETY: the slot is only read while the modal call is on the stack, on this thread,
    // and the closure outlives it; the lifetime is erased, not the validity.
    let erased: *mut dyn FnMut() = unsafe {
        std::mem::transmute::<*mut dyn FnMut(), *mut dyn FnMut()>(pump)
    };
    PUMP.with(|slot| slot.set(Some(erased)));
    STANDBY.with(|standby| standby.set(false));
    // SAFETY: the window is this thread's and the id is ours.
    let timer = unsafe { SetTimer(Some(hwnd), TIMER_ID, TICK_MS, None) };
    if timer == 0 {
        // No timer means no callback, so the slot would outlive the closure that filled it.
        PUMP.with(|slot| slot.set(None));
    }
    timer
}

/// Answer this thread's move-loop ticks with `pump`, without a timer of its own.
///
/// The shell's move loop arms the timer itself — `WM_ENTERSIZEMOVE` — so this only fills
/// the slot its `WM_TIMER` reads, with the pump of the surface that loop is moving. False
/// when a gesture already owns the slot: that gesture's tick runs its own pump, and this
/// window keeps the frame it has for the length of the gesture, as it did before.
pub(crate) fn attach(pump: &mut dyn FnMut()) -> bool {
    if PUMP.with(Cell::get).is_some() {
        return false;
    }
    // SAFETY: the caller keeps the closure alive for the whole pump-message call, and the
    // shell's loop only calls back on this thread, inside that call.
    let erased: *mut dyn FnMut() = unsafe {
        std::mem::transmute::<*mut dyn FnMut(), *mut dyn FnMut()>(pump)
    };
    PUMP.with(|slot| slot.set(Some(erased)));
    STANDBY.with(|standby| standby.set(true));
    true
}

/// Drop the pump [`attach`] installed; a false is a call that found the slot taken.
pub(crate) fn detach(held: bool) {
    if !held {
        return;
    }
    PUMP.with(|slot| slot.set(None));
    STANDBY.with(|standby| standby.set(false));
}

/// Stop the timer [`arm`] started and drop its pump.
///
/// A zero is a call that never armed — a refused nested gesture — and owns no slot to clear.
pub(crate) fn disarm(hwnd: HWND, timer: usize) {
    if timer == 0 {
        return;
    }
    PUMP.with(|slot| slot.set(None));
    STANDBY.with(|standby| standby.set(false));
    // SAFETY: the id came back from `SetTimer` on this window.
    unsafe {
        let _ = KillTimer(Some(hwnd), timer);
    }
}

/// Whether this thread's message queue belongs to someone else: a gesture's tick, or the
/// shell's move loop, which dispatches this thread's messages until the pointer comes up.
///
/// Either way the platform must not pump these messages again.
pub(crate) fn inside() -> bool {
    BUSY.with(Cell::get) || SHELL.with(Cell::get) > 0
}

/// Run the frame pump a modal gesture armed, if any; true when one ran.
///
/// The window procedure calls this from `WM_TIMER`, which the shell's loop dispatches:
/// that is how a drag or a window move stops freezing the surface it started from.
///
/// A gesture with no pump of its own — a title-bar move the shell started in
/// `DefWindowProc` — still serves the rest of the process: the surface being moved is
/// inside a call this thread cannot leave, but every other surface is one step away.
pub(crate) fn pump_frame() -> bool {
    if BUSY.with(Cell::get) {
        // A frame is already being built on this stack; whatever it dispatches waits for
        // the next tick rather than nesting another build inside it.
        return false;
    }
    BUSY.with(|busy| busy.set(true));
    let armed = PUMP.with(Cell::get);
    if let Some(pump) = armed {
        // SAFETY: `arm` stores a closure that outlives the whole modal call, and `disarm`
        // clears the slot before the borrow of the caller's frame ends.
        unsafe { (*pump)() };
    }
    // The shell's loop dispatches this thread's messages, but it cannot step the rest of
    // the engine: the host's turn is suspended for the gesture. Its surfaces draw from the
    // same timer, so one window's move or drag does not hold the others at their last frame.
    let stepped = ui_platform::modal::pump_others();
    BUSY.with(|busy| busy.set(false));
    armed.is_some() || stepped
}
