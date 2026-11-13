//! Target-driven animation. Time is fed in from outside, never read from a clock.

use crate::collections::FastMap;

use crate::id::Id;

/// How a value travels between two ends.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// Fast start, slow finish; the default when entering a state.
    #[default]
    EaseOut,
    /// Slow start, fast finish.
    EaseIn,
    /// Slow at both ends.
    EaseInOut,
    /// CSS-style cubic bezier with the two implied endpoints (0,0) and (1,1).
    Cubic(f32, f32, f32, f32),
    /// Two cubics sharing one midpoint; a single cubic cannot express Material's
    /// emphasized family, which is piecewise by spec.
    Piece(&'static PieceCubic),
}

/// Two cubic segments joined at `mid`; tangents are the control lines at each end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieceCubic {
    pub a1: (f32, f32),
    pub b1: (f32, f32),
    pub mid: (f32, f32),
    pub a2: (f32, f32),
    pub b2: (f32, f32),
}

impl Easing {
    /// Map progress `t` in `0..=1` to eased progress.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::EaseIn => t.powi(3),
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t.powi(3)
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::Cubic(a, b, c, d) => solve_cubic(t, a, b, c, d),
            Easing::Piece(p) => p.apply(t),
        }
    }

    /// The curve to use when the value turns around mid-flight.
    ///
    /// Same curve after a reversal starts at near-zero speed and looks stuck. A two-part
    /// `Piece` has no mirrored form and is returned unchanged; a control that needs a
    /// different way back declares it with `AnimSpec::down` instead.
    pub fn mirrored(self) -> Self {
        match self {
            Easing::EaseOut => Easing::EaseIn,
            Easing::EaseIn => Easing::EaseOut,
            Easing::Cubic(a, b, c, d) => Easing::Cubic(1.0 - c, 1.0 - d, 1.0 - a, 1.0 - b),
            other => other,
        }
    }
}

impl PieceCubic {
    /// Evaluate: pick the segment by `x`, rescale it to the unit square, ease, scale back.
    pub fn apply(&self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let (first, sx, sy, base) = if t < self.mid.0 {
            (true, self.mid.0, self.mid.1, 0.0)
        } else {
            (false, 1.0 - self.mid.0, 1.0 - self.mid.1, self.mid.1)
        };
        let local = (t - if first { 0.0 } else { self.mid.0 }) / sx;
        let (p1, p2) = if first {
            (
                (self.a1.0 / sx, self.a1.1 / sy),
                (self.b1.0 / sx, self.b1.1 / sy),
            )
        } else {
            (
                ((self.a2.0 - self.mid.0) / sx, (self.a2.1 - self.mid.1) / sy),
                ((self.b2.0 - self.mid.0) / sx, (self.b2.1 - self.mid.1) / sy),
            )
        };
        solve_cubic(local, p1.0, p1.1, p2.0, p2.1) * sy + base
    }
}

/// CSS cubic-bezier evaluation: bisect the parameter for `x`, then read `y`.
fn solve_cubic(t: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let eval = |u: f32, v: f32, m: f32| {
        3.0 * u * (1.0 - m) * (1.0 - m) * m + 3.0 * v * (1.0 - m) * m * m + m * m * m
    };
    // 40 halvings take the 1.0 interval under the 0.001 bound; the reference loops forever.
    // Measured: output errs under 1.5% near t=0 (where x barely moves), under 0.3% of travel
    // per 60 Hz frame, so a tighter bound would buy no visible smoothness.
    let (mut start, mut stop) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let midpoint = (start + stop) / 2.0;
        let estimate = eval(a, c, midpoint);
        if (t - estimate).abs() < 0.001 {
            return eval(b, d, midpoint);
        }
        if estimate < t {
            start = midpoint;
        } else {
            stop = midpoint;
        }
    }
    eval(b, d, (start + stop) / 2.0)
}

/// Where `age` stands in a loop of `cycle` seconds, as a fraction of the cycle in `0.0..1.0`.
///
/// A negative or non-finite age reads as the loop's start, so a clock that has not begun
/// cannot send a sweep backwards.
pub fn phase(age: f32, cycle: f32) -> f32 {
    if age.is_finite() && cycle > 0.0 {
        (age.max(0.0) / cycle).fract()
    } else {
        0.0
    }
}

/// Piecewise interpolation over weighted stops, in order; weights are cumulative ends.
pub fn weighted<T: Copy>(
    t: f32,
    stops: &[(f32, Easing, T)],
    from: T,
    lerp: impl Fn(T, T, f32) -> T,
) -> T {
    let t = t.clamp(0.0, 1.0);
    let mut prev = 0.0;
    let mut start = from;
    for (end, curve, target) in stops {
        if t <= *end {
            let span = (end - prev).max(1e-6);
            return lerp(start, *target, curve.apply((t - prev) / span));
        }
        prev = *end;
        start = *target;
    }
    start
}

/// How long and along which curve a value travels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimSpec {
    pub duration: f32,
    pub easing: Easing,
    /// Held at the leg's start for this long before the curve begins.
    pub delay: f32,
    /// The leg used when the value travels *down*; `None` reuses this one.
    pub down: Option<Reverse>,
    /// The value range this instance spans, so a partial retarget takes a proportional
    /// slice of the duration. `None` keeps the previous leg's distance as the unit, which
    /// is what a per-frame drag follow wants.
    pub span: Option<f32>,
}

/// A downward leg: a shorter, differently-curved trip back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reverse {
    pub duration: f32,
    pub easing: Easing,
}

impl AnimSpec {
    pub const fn new(duration: f32, easing: Easing) -> Self {
        Self {
            duration,
            easing,
            delay: 0.0,
            down: None,
            span: None,
        }
    }

    /// Declare the value range this instance spans; a retarget then takes a proportional
    /// slice of the duration instead of a slice of the previous leg.
    pub const fn span(mut self, span: f32) -> Self {
        self.span = Some(span);
        self
    }

    /// Travel down with its own duration and curve, instead of mirroring this leg.
    pub const fn down(self, duration: f32, easing: Easing) -> Self {
        Self {
            down: Some(Reverse { duration, easing }),
            ..self
        }
    }

    /// The same leg, starting `delay` seconds later; the engine holds the value meanwhile.
    pub const fn delayed(self, delay: f32) -> Self {
        Self { delay, ..self }
    }

    /// A short, snappy default: 150 ms ease-out.
    pub const fn quick() -> Self {
        Self::new(0.15, Easing::EaseOut)
    }

    /// How far a leg that began at `start` has come at `now`, before easing: `0.0..=1.0`.
    ///
    /// A zero duration is a write and lands as soon as its delay is over; a clock that is
    /// non-finite or steps backwards holds the leg's far end rather than rewinding it.
    pub fn progress_at(&self, start: f64, now: f64) -> f32 {
        let start = start + self.delay.max(0.0) as f64;
        if self.duration <= 0.0 {
            return if now < start { 0.0 } else { 1.0 };
        }
        // A leg sampled at its own start shows the value it left, not a jump.
        if now <= start {
            return 0.0;
        }
        let elapsed = now - start;
        if !elapsed.is_finite() {
            return 1.0;
        }
        (elapsed.max(0.0) as f32 / self.duration).clamp(0.0, 1.0)
    }

    /// The leg's progress through its own curve: the one composition of the two.
    pub fn eased_at(&self, start: f64, now: f64) -> f32 {
        self.easing.apply(self.progress_at(start, now))
    }

    /// The same travel back: mirror the curve so reversal starts at the end's slope.
    pub fn mirrored(&self) -> Self {
        Self {
            easing: self.easing.mirrored(),
            ..*self
        }
    }
}

impl Default for AnimSpec {
    fn default() -> Self {
        Self::quick()
    }
}

/// One keyframe: a value reached at `at` (a fraction of the leg's duration), with the curve
/// that carries the value into it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub at: f32,
    pub value: f32,
    pub easing: Easing,
}

impl Stop {
    pub const fn new(at: f32, value: f32, easing: Easing) -> Self {
        Self { at, value, easing }
    }
}

/// Keyframes an instance can hold; enough for a squash phase plus a settle.
pub const MAX_STOPS: usize = 4;

fn colour_of(lanes: [f32; 4]) -> crate::geom::Color {
    crate::geom::Color {
        r: lanes[0],
        g: lanes[1],
        b: lanes[2],
        a: lanes[3],
    }
}

/// One value in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry {
    /// Channel values; only the first `lanes` are meaningful.
    from: [f32; 4],
    to: [f32; 4],
    /// 1 for a scalar, 4 for a colour. Every lane shares one clock, so the channels of a
    /// colour never desync.
    lanes: u8,
    /// When this leg's target was committed, in the engine's seconds. Progress is
    /// `now - committed_at`, so nothing accumulates and no clock is read here.
    committed_at: f64,
    /// The leg's timing; `committed_at` is when it began.
    spec: AnimSpec,
    /// Keyframes for this leg; empty means a plain tween from `from` to `to`.
    stops: [Stop; MAX_STOPS],
    stop_count: u8,
    /// Goal before the last turn; `None` if it never turned.
    previous_target: Option<f32>,
}

impl Entry {
    fn progress(&self, now: f64) -> f32 {
        self.spec.progress_at(self.committed_at, now)
    }

    /// All lanes at `now`; every lane shares this leg's clock.
    fn lanes_at(&self, now: f64) -> [f32; 4] {
        let t = self.progress(now);
        let mut out = self.from;
        if self.stop_count == 0 {
            let eased = self.spec.eased_at(self.committed_at, now);
            let lanes = out
                .iter_mut()
                .zip(self.from)
                .zip(self.to)
                .take(self.lanes as usize);
            for ((slot, from), to) in lanes {
                *slot = crate::geom::lerp(from, to, eased);
            }
            return out;
        }
        // Keyframes describe one channel; a colour never carries them.
        let mut scaled = [(0.0f32, Easing::Linear, 0.0f32); MAX_STOPS];
        for (slot, stop) in scaled
            .iter_mut()
            .zip(&self.stops[..self.stop_count as usize])
        {
            *slot = (stop.at, stop.easing, stop.value);
        }
        out[0] = weighted(
            t,
            &scaled[..self.stop_count as usize],
            self.from[0],
            crate::geom::lerp,
        );
        out
    }

    fn value(&self, now: f64) -> f32 {
        self.lanes_at(now)[0]
    }

    fn settled(&self, now: f64) -> bool {
        self.progress(now) >= 1.0
    }
}

/// Which way one instance is travelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Travel {
    /// The value is heading up.
    #[default]
    Forward,
    /// The value is heading down.
    Reverse,
}

/// One instance's state, read-only.
///
/// `leg_start` is the value the last retarget sampled, which is what a family needs to
/// freeze a derived quantity for the duration of one leg.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    pub value: f32,
    pub target: f32,
    pub leg_start: f32,
    pub travel: Travel,
    pub settled: bool,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            value: 0.0,
            target: 0.0,
            leg_start: 0.0,
            travel: Travel::Forward,
            settled: true,
        }
    }
}

/// One spring's state at this frame, in the engine's own units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpringState {
    /// Position, in the target's units.
    pub x: f32,
    /// Velocity in units per second.
    pub v: f32,
}

/// A spring integrated in engine seconds, so a dropped frame covers the same time as four
/// on-time ones instead of slowing the motion down.
#[derive(Clone, Copy, Debug)]
struct SpringEntry {
    /// Simulated in f64: at a page-scale coordinate one f32 ulp is 1.2e-4, wider than the
    /// settle tolerance, so an f32 integrator parks there for ever.
    x: f64,
    v: f64,
    target: f64,
    stiffness: f64,
    damping: f64,
    last: f64,
    /// When this entry was created; a tiny age means something re-creates it every frame.
    born: f64,
    /// A target set since the last integration: it has something to do, even at zero dt.
    pending: bool,
    /// Distance covered in the frame being reported, so "live" never outlives real motion.
    moved: f32,
    /// Per-frame tallies: who drove it, whether it integrated, and with what step.
    drives: u32,
    steps: u32,
    adv: u32,
    dt: f32,
}

/// How close counts as arrived: Flutter's `Tolerance.defaultTolerance`, both axes.
///
/// One pair of numbers for the whole engine: a second, looser threshold anywhere would end
/// a motion while its spring still reports movement.
const SPRING_SETTLED_DISTANCE: f32 = 1e-3;
const SPRING_SETTLED_SPEED: f32 = 1e-3;
/// Substep bound: semi-implicit Euler is stable here for any stiffness a control uses.
const SPRING_MAX_STEP: f32 = 1.0 / 240.0;
/// A long stall integrates one bounded slice, so the spring cannot explode.
const SPRING_MAX_DT: f32 = 0.1;

impl SpringEntry {
    fn live(&self) -> bool {
        (self.x - self.target).abs() > SPRING_SETTLED_DISTANCE as f64
            || self.v.abs() > SPRING_SETTLED_SPEED as f64
    }

    /// Integrate one slice with semi-implicit Euler, substepped so any stiffness is stable.
    fn step(&mut self, dt: f32) {
        let before = self.x;
        let steps = (dt / SPRING_MAX_STEP).ceil().max(1.0) as u32;
        let h = (dt / steps as f32) as f64;
        for _ in 0..steps {
            let a = self.stiffness * (self.target - self.x) - self.damping * self.v;
            self.v += a * h;
            self.x += self.v * h;
        }
        // Rest is exact, not merely small: `is_animating` must stop the frame loop.
        if !self.live() {
            self.x = self.target;
            self.v = 0.0;
        }
        self.moved += (self.x - before).abs() as f32;
        self.steps += 1;
        self.pending = false;
    }
}

/// One animation still in flight, as a diagnostic reads it.
#[derive(Clone, Copy, Debug)]
pub enum Moving {
    /// A tween: `progress` below 1 is what keeps the loop awake.
    Tween {
        id: Id,
        progress: f32,
        duration: f32,
        delay: f32,
    },
    /// A spring: it is live while it is away from `target` or still fast.
    Spring {
        id: Id,
        x: f32,
        v: f32,
        target: f32,
        /// Seconds since anything last advanced it; zero means it is being touched every frame.
        idle: f32,
        /// Seconds since this entry was created; near zero means it is rebuilt every frame.
        age: f32,
        /// Distance covered in the last frame: zero while "live" means nothing advanced it.
        moved: f32,
        /// Whether it still keeps the frame loop awake.
        awake: bool,
        /// This frame's tallies: driver calls, integrations, scheduler visits, last step.
        drives: u32,
        steps: u32,
        adv: u32,
        dt: f32,
    },
}

impl Moving {
    pub fn id(&self) -> Id {
        match self {
            Moving::Tween { id, .. } | Moving::Spring { id, .. } => *id,
        }
    }
}

impl std::fmt::Display for Moving {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Moving::Tween {
                id,
                progress,
                duration,
                delay,
            } => write!(
                f,
                "tween {id:?} progress={progress:.4} duration={duration} delay={delay}"
            ),
            Moving::Spring {
                id,
                x,
                v,
                target,
                idle,
                age,
                moved,
                awake,
                drives,
                steps,
                adv,
                dt,
            } => write!(
                f,
                "spring {id:?} x={x:.4} v={v:.4} target={target} idle={idle:.4} age={age:.3} moved={moved:.1e} awake={awake} drive={drives} step={steps} adv={adv} dt={dt:.1e}"
            ),
        }
    }
}

/// Every animation the current frame is running.
#[derive(Debug, Default)]
pub struct AnimStore {
    entries: FastMap<Id, Entry>,
    springs: FastMap<Id, SpringEntry>,
    /// When each time-driven id was first seen, so a loop can start at phase zero.
    births: FastMap<Id, f64>,
}

impl AnimStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drive the value at `id` toward `target`; returns what to draw this frame.
    ///
    /// First call settles at `target`: a value never animated has nowhere to travel from.
    pub fn animate(&mut self, id: Id, target: f32, spec: AnimSpec, now: f64) -> f32 {
        let Some(entry) = self.entries.get_mut(&id) else {
            self.entries.insert(
                id,
                Entry {
                    from: [target; 4],
                    to: [target; 4],
                    lanes: 1,
                    // Back-dated by one full duration: a value with nowhere to travel from
                    // is already there, so it must not animate.
                    committed_at: now - spec.duration.max(0.0) as f64,
                    spec,
                    stops: [Stop::new(0.0, target, Easing::Linear); MAX_STOPS],
                    stop_count: 0,
                    previous_target: None,
                },
            );
            return target;
        };

        if entry.to[0] == target {
            if entry.stop_count > 0 {
                // A plain tween was asked for while keyframes were still running: re-anchor
                // from the current value, or the old trajectory would keep driving the value
                // (it can travel away from the target before coming back).
                let current = entry.value(now);
                entry.from = [current; 4];
                entry.committed_at = now;
                entry.spec = spec;
                entry.stop_count = 0;
            }
            return entry.value(now);
        }

        // Retarget from the current value; starting from `entry.to` would jump.
        let current = entry.value(now);
        let turning_around = match entry.previous_target {
            Some(before) => (target - before).abs() < f32::EPSILON,
            None => (target - entry.from[0]).abs() < (target - entry.to[0]).abs(),
        };
        let easing = if turning_around && entry.previous_target.is_none() {
            entry.spec.easing.mirrored()
        } else {
            entry.spec.easing
        };
        // A shorter remaining distance takes proportionally less time. The unit is the
        // declared span when there is one; otherwise it is the previous leg, which is what
        // keeps a per-frame drag follow at a constant lag. A settled instance is not being
        // interrupted, so its next leg takes the declared time whatever the last one spanned.
        let total = (entry.to[0] - entry.from[0]).abs();
        let remaining = (target - current).abs();
        let fraction = match spec.span {
            Some(span) if span > 0.0 => (remaining / span).clamp(0.05, 1.0),
            _ if total > f32::EPSILON && !entry.settled(now) => {
                (remaining / total).clamp(0.05, 1.0)
            }
            _ => 1.0,
        };

        // A downward leg may carry its own duration and curve, so a control never has to
        // pick a spec by direction itself.
        let (leg_duration, leg_easing) = match spec.down {
            Some(down) if target < current => (down.duration, down.easing),
            _ => (spec.duration, easing),
        };
        entry.previous_target = Some(entry.to[0]);
        entry.from = [current; 4];
        entry.to = [target; 4];
        entry.committed_at = now;
        entry.spec = AnimSpec {
            duration: (leg_duration * fraction).max(0.0),
            easing: leg_easing,
            ..spec
        };
        // A retarget abandons the old keyframes: the new leg is a plain tween.
        entry.stop_count = 0;
        entry.value(now)
    }

    /// Drive an instance along keyframes; the last stop is the target.
    ///
    /// A fresh instance starts already settled at the last stop, so the first frame shows
    /// the state it is in rather than animating from nowhere.
    pub fn animate_stops(&mut self, id: Id, stops: &[Stop], spec: AnimSpec, now: f64) -> f32 {
        let n = stops.len().min(MAX_STOPS);
        let Some(last) = stops.get(n.wrapping_sub(1)) else {
            return self.animate(id, 0.0, spec, now);
        };
        let target = last.value;
        let mut buf = [Stop::new(0.0, target, Easing::Linear); MAX_STOPS];
        buf[..n].copy_from_slice(&stops[..n]);
        let Some(entry) = self.entries.get_mut(&id) else {
            self.entries.insert(
                id,
                Entry {
                    from: [target; 4],
                    to: [target; 4],
                    lanes: 1,
                    committed_at: now - spec.duration.max(0.0) as f64 - spec.delay.max(0.0) as f64,
                    spec,
                    stops: buf,
                    stop_count: n as u8,
                    previous_target: None,
                },
            );
            return target;
        };
        if entry.to[0] == target {
            // Same destination: keep the value continuous but refresh the shape, because
            // the stops may carry state-resolved values (a latched end, say).
            entry.stops = buf;
            entry.stop_count = n as u8;
            entry.spec = spec;
            return entry.value(now);
        }
        let current = entry.value(now);
        entry.previous_target = Some(entry.to[0]);
        entry.from = [current; 4];
        entry.to = [target; 4];
        entry.committed_at = now;
        entry.spec = spec;
        entry.stops = buf;
        entry.stop_count = n as u8;
        entry.value(now)
    }

    /// Drive a colour; all four channels share one clock, so they never desync.
    ///
    /// The control resolves the colour its state calls for; the engine carries the blend.
    pub fn animate_colour(
        &mut self,
        id: Id,
        target: crate::geom::Color,
        spec: AnimSpec,
        now: f64,
    ) -> crate::geom::Color {
        let to = [target.r, target.g, target.b, target.a];
        let Some(entry) = self.entries.get_mut(&id) else {
            self.entries.insert(
                id,
                Entry {
                    from: to,
                    to,
                    lanes: 4,
                    committed_at: now - spec.duration.max(0.0) as f64 - spec.delay.max(0.0) as f64,
                    spec,
                    stops: [Stop::new(0.0, 0.0, Easing::Linear); MAX_STOPS],
                    stop_count: 0,
                    previous_target: None,
                },
            );
            return target;
        };
        if entry.to == to && entry.lanes == 4 {
            return colour_of(entry.lanes_at(now));
        }
        let current = entry.lanes_at(now);
        // The clock is set by the channel that has the furthest to go.
        let interrupted = !entry.settled(now);
        let mut remaining_max = 0.0f32;
        let mut ratio_max = 0.0f32;
        for i in 0..4 {
            let total = (entry.to[i] - entry.from[i]).abs();
            let remaining = (to[i] - current[i]).abs();
            remaining_max = remaining_max.max(remaining);
            if total > f32::EPSILON && interrupted {
                ratio_max = ratio_max.max((remaining / total).clamp(0.05, 1.0));
            }
        }
        // A declared span scales by the furthest channel's distance; without one the
        // previous leg stays the unit. A settled instance is not interrupted, so it takes
        // the full leg whatever the last one spanned.
        let fraction = match spec.span {
            Some(span) if span > 0.0 => (remaining_max / span).clamp(0.05, 1.0),
            _ if ratio_max > 0.0 => ratio_max,
            _ => 1.0,
        };
        entry.previous_target = Some(entry.to[0]);
        entry.from = current;
        entry.to = to;
        entry.lanes = 4;
        entry.committed_at = now;
        entry.spec = AnimSpec {
            duration: (spec.duration * fraction).max(0.0),
            ..spec
        };
        entry.stop_count = 0;
        colour_of(entry.lanes_at(now))
    }

    /// Current colour, or `None` if this id was never animated as one.
    pub fn colour(&self, id: Id, now: f64) -> Option<crate::geom::Color> {
        let e = self.entries.get(&id)?;
        (e.lanes == 4).then(|| colour_of(e.lanes_at(now)))
    }

    /// Current value, or `None` if this id has never been animated.
    pub fn value(&self, id: Id, now: f64) -> Option<f32> {
        self.entries.get(&id).map(|e| e.value(now))
    }

    /// Start a value at `from` and drive it to `target`; a value already tracked just moves.
    ///
    /// Unlike [`animate`](Self::animate) the first call has somewhere to come from, which is
    /// what an entering node needs: CSS calls the same idea `@starting-style`.
    pub fn travel(&mut self, id: Id, from: f32, target: f32, spec: AnimSpec, now: f64) -> f32 {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.entries.entry(id) {
            slot.insert(Entry {
                from: [from; 4],
                to: [target; 4],
                lanes: 1,
                committed_at: now,
                spec,
                stops: [Stop::new(0.0, target, Easing::Linear); MAX_STOPS],
                stop_count: 0,
                previous_target: None,
            });
        }
        self.animate(id, target, spec, now)
    }

    /// Drop the instance at `id`, so a node that reappears travels from its start again.
    pub fn forget(&mut self, id: Id) {
        self.entries.remove(&id);
    }

    /// Seconds since `id` was first asked for: a loop's own clock, starting at zero.
    ///
    /// Unlike an animation this never settles; the caller is expected to keep asking.
    pub fn age(&mut self, id: Id, now: f64) -> f64 {
        let born = *self.births.entry(id).or_insert(now);
        (now - born).max(0.0)
    }

    /// The age a previous [`age`](Self::age) call established, or `None` before the first.
    pub fn age_of(&self, id: Id, now: f64) -> Option<f64> {
        self.births.get(&id).map(|born| (now - born).max(0.0))
    }

    /// One instance's state, or `None` if it was never animated.
    ///
    /// Read it after retargeting, so it describes this frame's leg.
    pub fn status(&self, id: Id, now: f64) -> Option<Status> {
        self.entries.get(&id).map(|e| Status {
            value: e.value(now),
            target: e.to[0],
            leg_start: e.from[0],
            travel: if e.to[0] >= e.from[0] {
                Travel::Forward
            } else {
                Travel::Reverse
            },
            settled: e.settled(now),
        })
    }

    /// Drive a spring at `id` toward `target`; returns position and velocity this frame.
    ///
    /// The first call settles at `target`: a spring never pushed has nowhere to travel from.
    pub fn spring(
        &mut self,
        id: Id,
        target: f32,
        stiffness: f32,
        damping: f32,
        now: f64,
    ) -> SpringState {
        self.spring_hinted(id, target, stiffness, damping, 0.0, now)
    }

    /// Where `id`'s spring is now, without advancing it; `None` before the first drive.
    pub fn spring_value(&self, id: Id) -> Option<SpringState> {
        self.springs.get(&id).map(|e| SpringState {
            x: e.x as f32,
            v: e.v as f32,
        })
    }

    /// Forget one spring, so the next drive starts at its target instead of the old
    /// trajectory; a leftover position would make the first frame of a new bounce jump.
    pub fn forget_spring(&mut self, id: Id) {
        self.springs.remove(&id);
    }

    /// Forget one id's birth, so its next [`age`](Self::age) read starts again at zero.
    pub fn forget_age(&mut self, id: Id) {
        self.births.remove(&id);
    }

    /// Same as [`spring`](Self::spring), but a target that changed this call starts at
    /// `hint` units/s: Flutter's `animateWith(SpringSimulation(..., velocity: hint))`.
    pub fn spring_hinted(
        &mut self,
        id: Id,
        target: f32,
        stiffness: f32,
        damping: f32,
        hint: f32,
        now: f64,
    ) -> SpringState {
        let entry = self.springs.entry(id).or_insert(SpringEntry {
            x: target as f64,
            v: 0.0,
            target: target as f64,
            stiffness: stiffness as f64,
            damping: damping as f64,
            last: now,
            born: now,
            pending: true,
            moved: 0.0,
            drives: 0,
            steps: 0,
            adv: 0,
            dt: 0.0,
        });
        // Retuning in place keeps the motion continuous when a style swaps the spring.
        if entry.target != target as f64 {
            entry.pending = true;
            if hint != 0.0 {
                entry.v = hint as f64;
            }
        }
        entry.target = target as f64;
        entry.stiffness = stiffness as f64;
        entry.damping = damping as f64;
        let dt = (now - entry.last).clamp(0.0, SPRING_MAX_DT as f64) as f32;
        entry.last = now;
        entry.drives += 1;
        entry.dt = dt;
        if dt > 0.0 && entry.live() {
            entry.step(dt);
        } else if !entry.live() {
            entry.x = entry.target;
            entry.v = 0.0;
        }
        SpringState {
            x: entry.x as f32,
            v: entry.v as f32,
        }
    }

    /// Step every spring to `now`, leaving each one at rest once it lands.
    ///
    /// A spring moves only when advanced; abandoned, it would claim motion for ever and keep
    /// the loop awake. A landed one stays as the value its next animation departs from.
    pub fn advance(&mut self, now: f64) {
        for entry in self.springs.values_mut() {
            let dt = (now - entry.last).clamp(0.0, SPRING_MAX_DT as f64) as f32;
            entry.last = now;
            if dt > 0.0 && entry.live() {
                entry.adv += 1;
                entry.dt = dt;
                entry.step(dt);
            }
        }
    }

    /// Zero the per-frame tallies before anything drives a spring this frame.
    pub fn begin_frame(&mut self) {
        for entry in self.springs.values_mut() {
            entry.moved = 0.0;
            entry.drives = 0;
            entry.steps = 0;
            entry.adv = 0;
        }
    }

    /// Whether anything is still moving.
    ///
    /// A spring keeps the loop awake only while it has a fresh target or actually moved:
    /// one left live but never advanced would otherwise spin the loop with nothing to draw.
    pub fn is_animating(&self, now: f64) -> bool {
        self.entries.values().any(|e| !e.settled(now))
            || self
                .springs
                .values()
                .any(|s| s.live() && (s.pending || s.moved > 0.0))
    }

    /// Whether one spring is still travelling; only its owner's calls advance it.
    pub fn spring_live(&self, id: Id) -> bool {
        self.springs.get(&id).is_some_and(|s| s.live())
    }

    /// How many springs are tracked, and how many of those still claim motion.
    pub fn spring_counts(&self) -> (usize, usize) {
        (
            self.springs.len(),
            self.springs.values().filter(|s| s.live()).count(),
        )
    }

    /// Everything still moving, with the numbers that say *why* it never lands.
    pub fn moving(&self, now: f64) -> Vec<Moving> {
        let mut out: Vec<Moving> = self
            .entries
            .iter()
            .filter(|(_, e)| !e.settled(now))
            .map(|(id, e)| Moving::Tween {
                id: *id,
                progress: e.progress(now),
                duration: e.spec.duration,
                delay: e.spec.delay,
            })
            .collect();
        out.extend(
            self.springs
                .iter()
                .filter(|(_, s)| s.live())
                .map(|(id, s)| Moving::Spring {
                    id: *id,
                    x: s.x as f32,
                    v: s.v as f32,
                    target: s.target as f32,
                    idle: (now - s.last) as f32,
                    age: (now - s.born) as f32,
                    moved: s.moved,
                    awake: s.pending || s.moved > 0.0,
                    drives: s.drives,
                    steps: s.steps,
                    adv: s.adv,
                    dt: s.dt,
                }),
        );
        out
    }

    /// Forget every animation, e.g. when the tree is rebuilt from scratch.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.springs.clear();
        self.births.clear();
    }

    /// How many ids are tracked, settled or not.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests;
