//! The frame driver: intake, demand, pacing — everything a loop would otherwise repeat.

use std::time::{Duration, Instant};

use ui_core::Event;
use ui_core::geom::Point;

use crate::view::{App, AppLoop};

/// What a driver needs from the platform window.
///
/// The window implements this once; an app never re-derives the loop's rules from it.
pub trait FrameHost {
    /// Surface size in physical pixels.
    fn size(&self) -> (u32, u32);
    fn scale(&self) -> f32;
    fn pointer(&self) -> Option<Point>;
    /// Whether the compositor has opened a frame window (a frame callback arrived).
    fn can_draw(&self) -> bool;
    /// Block until some event arrives or `timeout` passes; true when events are waiting.
    /// The engine's own deadlines (the caret's blink) ride on this.
    fn wait_until(&mut self, timeout: Option<Duration>) -> bool;
    /// Block until some event arrives.
    fn wait(&mut self) {
        let _ = self.wait_until(None);
    }
    /// Block until the compositor's frame callback arrives.
    fn wait_for_frame(&mut self);
    fn should_close(&self) -> bool;
    /// Events since the last call, in arrival order.
    fn take_events(&mut self) -> Vec<Event>;
    /// A change the app made outside `view`, which arrives without an `Event`.
    fn take_silent_change(&mut self) -> bool;
}

/// What this turn should do; the driver has already slept or waited when it returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Turn {
    /// Nothing an event could change: the driver slept.
    Idle,
    /// A frame is wanted, but the compositor has not opened the window yet.
    Waiting,
    /// Build and present now.
    Draw { pointer: Option<Point> },
}

/// How long a looping host waits for a missing frame callback before waking anyway.
const FRAME_WAIT_CAP: Duration = Duration::from_millis(500);

/// Owns the loop's state: demand, pacing and the last surface size.
///
/// The states mirror GPUI's platform `FrameLoop`: a frame is wanted (`Ticking`), then the
/// loop waits for the callback (`AwaitingCallback`), then it draws again.
pub struct Driver {
    /// Ask for a frame on every turn instead of one per compositor callback.
    uncapped: bool,
    /// A frame was wanted on an earlier turn and has not been drawn yet.
    pending: bool,
    last_size: Option<(u32, u32)>,
    waited: Duration,
    waited_n: u32,
    last_wait: Duration,
    /// When the loop must wake with nothing arriving; the engine's own deadlines.
    wake_at: Option<Instant>,
    /// Events taken from the host this turn, kept for the caller's diagnostics.
    events: Vec<Event>,
}

impl Default for Driver {
    fn default() -> Self {
        Self::new()
    }
}

impl Driver {
    pub fn new() -> Self {
        Self {
            uncapped: false,
            pending: false,
            last_size: None,
            waited: Duration::ZERO,
            waited_n: 0,
            last_wait: Duration::ZERO,
            wake_at: None,
            events: Vec::new(),
        }
    }

    /// Events taken from the host on the last `step`, already dispatched to the app.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Drop the one-frame-per-callback gate. A stress knob, not a mode: frames the
    /// compositor never draws are neither shown nor counted.
    pub fn uncapped(mut self, on: bool) -> Self {
        self.uncapped = on;
        self
    }

    /// Time spent waiting for frame callbacks, and how many waits that was.
    pub fn wait_stats(&self) -> (Duration, u32) {
        (self.waited, self.waited_n)
    }

    /// How long the last `step` waited for a frame callback; a long one means a stalled
    /// output. The deadline that ends such a wait belongs to the window, not the caller.
    pub fn last_wait(&self) -> Duration {
        self.last_wait
    }

    /// After a drawn frame: wait for the compositor's callback, so the next turn starts
    /// inside a fresh frame window. Skipped while uncapped.
    pub fn pace(&mut self, host: &mut dyn FrameHost) {
        if self.uncapped {
            return;
        }
        let started = Instant::now();
        host.wait_for_frame();
        self.last_wait = started.elapsed();
        self.waited += self.last_wait;
        self.waited_n += 1;
    }

    /// When a looping host must call `decide` again even with nothing arriving.
    ///
    /// The engine's own deadlines, plus the cap that keeps a missing frame callback from
    /// holding the loop for good. A callback that does arrive wakes the connection sooner.
    pub fn deadline(&self) -> Option<Instant> {
        self.wake_at
    }

    /// Take this turn's events and fold everything that wants a frame into `pending`.
    ///
    /// A resized surface counts as demand even with no event: layout still has to run.
    fn intake<A: App>(&mut self, lp: &mut AppLoop<A>, host: &mut dyn FrameHost) -> bool {
        self.events = host.take_events();
        for event in &self.events {
            lp.dispatch(event.clone());
        }
        let size = host.size();
        self.pending |= lp.take_dispatched()
            || host.take_silent_change()
            || lp.is_animating()
            || self.last_size != Some(size);
        self.pending
    }

    /// Draw now: the callback has arrived (or `uncapped` waived it) and a frame is due.
    fn draw<A: App>(
        &mut self,
        lp: &mut AppLoop<A>,
        host: &mut dyn FrameHost,
        size: (u32, u32),
    ) -> Turn {
        self.pending = false;
        self.last_size = Some(size);
        // The app no longer reads the window's scale: it is layout input, so the driver sets it.
        lp.set_scale(host.scale());
        Turn::Draw {
            pointer: host.pointer(),
        }
    }

    /// Decide without waiting; the caller's loop owns the sleep.
    ///
    /// `Turn::Idle` means "wake me at [`deadline`](Self::deadline)"; `Turn::Waiting` means
    /// the compositor owes a frame callback. A frame window that opened while waiting is
    /// itself demand: the callback is an event the host never sees as one.
    pub fn decide<A: App>(&mut self, lp: &mut AppLoop<A>, host: &mut dyn FrameHost) -> Turn {
        let now = Instant::now();
        let fired = self.wake_at.is_some_and(|at| at <= now);
        self.wake_at = None;
        if host.should_close() {
            return Turn::Idle;
        }
        if !self.intake(lp, host) {
            if !fired {
                match lp.next_wake() {
                    Some(delay) if delay.is_zero() => {}
                    Some(delay) => {
                        self.wake_at = Some(now + delay);
                        return Turn::Idle;
                    }
                    None => return Turn::Idle,
                }
            }
            // Nothing arrived and the engine's own deadline did: the caret's toggle is
            // the demand. Without this the deadline would swallow it until input.
            self.pending = true;
        }
        let size = host.size();
        if !self.uncapped && !host.can_draw() {
            // `pending` survives the turn: the callback is the demand, and it is not an
            // event any host reports.
            self.wake_at = Some(now + FRAME_WAIT_CAP);
            return Turn::Waiting;
        }
        self.draw(lp, host, size)
    }

    /// Intake events, decide, and sleep or wait as the decision requires.
    ///
    /// The blocking form, for a caller that drives one surface itself (scripted probes,
    /// tests). A host loop uses [`decide`](Self::decide) and waits once for everything.
    pub fn step<A: App>(&mut self, lp: &mut AppLoop<A>, host: &mut dyn FrameHost) -> Turn {
        self.last_wait = Duration::ZERO;
        self.wake_at = None;
        if host.should_close() {
            return Turn::Idle;
        }
        // Consumed once: a turn that waits for the callback must still draw afterwards.
        if !self.intake(lp, host) {
            let wake = lp.next_wake();
            if host.wait_until(wake) || wake.is_none() {
                // An event arrived (the next turn dispatches it), or nothing is timed to
                // happen on its own.
                return Turn::Idle;
            }
            // Nothing arrived, and the engine's own deadline did: the caret's toggle is
            // the demand. Without this the wait would swallow it until unrelated input.
            self.pending = true;
        }
        if !self.uncapped && !host.can_draw() {
            let started = Instant::now();
            host.wait_for_frame();
            self.last_wait = started.elapsed();
            self.waited += self.last_wait;
            self.waited_n += 1;
            // Drawing in this turn keeps the gap between the callback and the commit as
            // short as the frame itself; a second turn costs a missed vblank.
            if !host.can_draw() {
                return Turn::Waiting;
            }
        }
        let size = host.size();
        self.draw(lp, host, size)
    }
}
