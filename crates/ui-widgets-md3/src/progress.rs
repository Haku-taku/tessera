//! MD3 progress: the linear bar, the ring, and the expressive wavy bar, whose geometry
//! follows Compose's `WavyProgressIndicator` (Flutter's `material` has no wavy form).

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Point, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Easing, Id, NodeKind};
use ui_style::Style;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::Md3Options;
use crate::motion::{FAST_OUT_SLOW_IN, STANDARD_TRANSITION};
use crate::paint::{role, spec_ink};
use crate::spec::{ProgressIndicatorShape, ProgressIndicatorWave, progress_indicator_spec};

/// One full sweep of the indeterminate pair (`_kIndeterminateLinearDuration`).
pub const SWEEP_SECONDS: f32 = 1.8;

/// The circular sweep's path cycle: the arc grows and shrinks once per 1333 ms.
pub const PATH_SECONDS: f32 = 1.333;
/// The circular sweep's own rotation: one turn per 2222 ms.
pub const ROTATION_SECONDS: f32 = 2.222;
/// `_epsilon`: a zero sweep draws nothing, so the arc keeps a hair of it.
const MIN_SWEEP: f32 = 0.001 / std::f32::consts::TAU;

/// Twelve o'clock, in turns from three o'clock: where both ring arcs begin.
const START_TURN: f32 = -0.25;

/// `WavyProgressIndicatorDefaults.indicatorAmplitude`: the wave is flat at the range's ends.
const WAVE_FLAT_BELOW: f32 = 0.1;
const WAVE_FLAT_ABOVE: f32 = 0.95;
/// How far apart the wave's samples are; the chord error stays under a twentieth of a dp.
const WAVE_STEP: f32 = 2.0;
/// Compose's `StopIndicatorTrailingSpace`; the export's token is blank.
const STOP_TRAILING_SPACE: f32 = 6.0;

/// A progress indicator, linear or circular, classic or wavy. The value is the app's; it
/// reports nothing.
pub struct Md3ProgressIndicator {
    id: Id,
    /// The bar's width; the ring reads `size` instead.
    width: f32,
    /// The ring's diameter; `None` paints the bar.
    size: Option<f32>,
    /// `0.0..=1.0`; the caller clamps, this paints whatever it is handed.
    value: f32,
    /// The bar's height and the ring's stroke: the spec's 4 dp.
    thickness: f32,
    /// The wavy bar's taller box, which holds the wave.
    height: f32,
    track: Color,
    active: Color,
    /// The sweep's clock in engine seconds; `None` paints the determinate form.
    age: Option<f32>,
    /// `track_active_indicator_space`: the space the track leaves around a moving bar.
    gap: f32,
    /// The wave's peak and wavelength; zero on the forms that paint no wave.
    peak: f32,
    wavelength: f32,
    /// The wave's envelope, flat at the ends of the range and full between them.
    amplitude: f32,
    /// The wave's clock in seconds: one wavelength per second.
    wave_phase: Option<f32>,
    /// The stop dot's diameter; zero when the form paints none.
    stop: f32,
    wavy: bool,
}

impl Md3ProgressIndicator {
    /// Build a linear bar of `width`. The caller passes it because a bar that fills its
    /// slot has no width until layout solves the decl being built; `Md3Slider` and
    /// `Md3TabBar` take theirs the same way.
    pub fn new_with(
        _options: Md3Options,
        id: Id,
        width: f32,
        value: f32,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        Self::build(
            id,
            width,
            None,
            value.clamp(0.0, 1.0),
            None,
            false,
            cx,
            pack,
        )
    }

    /// The same bar without a value: it sweeps on its own clock until the app has one.
    pub fn indeterminate_with(
        _options: Md3Options,
        id: Id,
        width: f32,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        Self::build(id, width, None, 0.0, Some(age_of(id, cx)), false, cx, pack)
    }

    /// A wavy bar showing `value`: the expressive form, 10 dp tall.
    pub fn wavy_with(
        _options: Md3Options,
        id: Id,
        width: f32,
        value: f32,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        Self::build(id, width, None, value.clamp(0.0, 1.0), None, true, cx, pack)
    }

    /// A wavy bar without a value: two waves sweep, and each travels on its own clock.
    pub fn wavy_indeterminate_with(
        _options: Md3Options,
        id: Id,
        width: f32,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        Self::build(id, width, None, 0.0, Some(age_of(id, cx)), true, cx, pack)
    }

    /// A `size` ring without a value: the arc grows, travels and shrinks on its own clock.
    pub fn ring_indeterminate_with(
        _options: Md3Options,
        id: Id,
        size: f32,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        Self::build(
            id,
            size,
            Some(size),
            0.0,
            Some(age_of(id, cx)),
            false,
            cx,
            pack,
        )
    }

    /// A `size` ring showing `value`: the track, then an arc from twelve o'clock.
    pub fn ring_with(
        _options: Md3Options,
        id: Id,
        size: f32,
        value: f32,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        Self::build(
            id,
            size,
            Some(size),
            value.clamp(0.0, 1.0),
            None,
            false,
            cx,
            pack,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        id: Id,
        width: f32,
        size: Option<f32>,
        value: f32,
        age: Option<f32>,
        wavy: bool,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let shape = if size.is_some() {
            ProgressIndicatorShape::Circular
        } else {
            ProgressIndicatorShape::Linear
        };
        let spec = progress_indicator_spec(
            age.is_some(),
            shape,
            if wavy {
                ProgressIndicatorWave::WithWave
            } else {
                ProgressIndicatorWave::Base
            },
        );
        // The wave runs on its own clock even when the app has a value; a sweeping bar
        // already reads the same clock.
        let wave_phase = wavy.then(|| age_of(id, cx));
        let amplitude = if wavy {
            if age.is_some() {
                1.0
            } else {
                let target = if value > WAVE_FLAT_BELOW && value < WAVE_FLAT_ABOVE {
                    1.0
                } else {
                    0.0
                };
                let mut node = cx.at(id);
                node.animate(id.with("wave-amplitude"), target, STANDARD_TRANSITION)
            }
        } else {
            0.0
        };
        Self {
            id,
            width,
            size,
            value,
            thickness: spec.active_indicator.thickness,
            height: if wavy {
                spec.root.height
            } else {
                spec.active_indicator.thickness
            },
            track: role(pack, spec.track.color),
            active: spec_ink(pack, spec.active_indicator.color, 1.0),
            age,
            gap: spec.root.track_active_indicator_space,
            peak: spec.active_indicator.amplitude,
            wavelength: spec.active_indicator.wavelength,
            amplitude,
            wave_phase,
            stop: if wavy { spec.stop_indicator.size } else { 0.0 },
            wavy,
        }
    }

    /// The bar's own height, so a caller can reserve room for it.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// `track_active_indicator_space` as a share of a turn, measured on the stroke's centre
    /// line: Flutter's `startGap = (strokeWidth + trackGap) / arcRadius`.
    fn track_gap_turns(&self, radius: f32) -> f32 {
        let centre = (radius - self.thickness * 0.5).max(f32::EPSILON);
        (self.thickness + self.gap) / centre / std::f32::consts::TAU
    }

    /// The ring: one stroked arc on a `size` circle, round-capped as Compose draws it.
    fn ring(&self, size: f32) -> Decl {
        let radius = size * 0.5;
        let mut shapes = Vec::with_capacity(2);
        let arc = |trim: (f32, f32), round_caps: bool, colour: Color| Shape::Sector {
            centre: Point::new(radius, radius),
            radius,
            thickness: self.thickness,
            trim,
            round_caps,
            colour,
        };
        // No track while sweeping: M3 gives the circular indicator one only when it has a
        // value (`circularTrackColor => indeterminate ? null : secondaryContainer`).
        if let Some(age) = self.age {
            let (start, end) = ring_arc(age);
            shapes.push(arc((start, end), true, self.active));
        } else if self.value > 0.0 {
            // The track stops either side of the value arc by `track_active_indicator_space`;
            // the arc itself is butt-capped, the track round-capped, as Flutter paints them.
            let gap = self.track_gap_turns(radius);
            let end = START_TURN + 1.0 - gap;
            let start = START_TURN + self.value + gap;
            if end > start {
                shapes.push(arc((start, end), true, self.track));
            }
            shapes.push(arc(
                (START_TURN, START_TURN + self.value),
                false,
                self.active,
            ));
        } else {
            shapes.push(arc((0.0, 1.0), true, self.track));
        }
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass::fixed(size, size))
            .shapes(shapes)
    }

    /// The wavy bar: a straight track, the wave over the value, and the stop dot.
    fn wavy_bar(&self) -> Decl {
        let mid = self.height * 0.5;
        let phase = self.wave_phase.unwrap_or(0.0);
        let mut shapes = Vec::with_capacity(16);
        match self.age.map(|age| (age / SWEEP_SECONDS).fract()) {
            None => {
                let head = self.value * self.width;
                if head > 0.0 {
                    self.push_wave(&mut shapes, 0.0, head, self.active, mid, phase);
                }
                // Compose shortens the gap to nothing while the bar is still invisible.
                let from = if head > 0.0 { head + self.gap } else { 0.0 };
                if from < self.width {
                    shapes.push(rect(from, self.width, mid, self.thickness, self.track));
                }
                if self.stop > 0.0 {
                    let inset = self.stop * 0.5
                        + ((self.height - self.stop) * 0.5).min(STOP_TRAILING_SPACE);
                    shapes.push(circle(self.active, self.width - inset, mid, self.stop));
                }
            }
            Some(sweep) => {
                for (from, to, fill, wave) in self.sweep_marks(sweep) {
                    let (from, to) = (from * self.width, to * self.width);
                    if wave {
                        self.push_wave(&mut shapes, from, to, fill, mid, phase);
                    } else {
                        shapes.push(rect(from, to, mid, self.thickness, fill));
                    }
                }
            }
        }
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .shapes(shapes)
    }

    /// The two sweeping bars and the track between them, as `(from, to, fill, is_wave)` in
    /// fractions of the width, in paint order.
    fn sweep_marks(&self, phase: f32) -> Vec<(f32, f32, Color, bool)> {
        // The track shows only where the two bars are not, each edge held back by the 4dp
        // space; the ramp keeps that space from popping into existence.
        let gap = self.gap / self.width.max(1.0);
        let space = |v: f32| gap * v.clamp(0.0, 0.01) / 0.01;
        let mut marks = Vec::with_capacity(5);
        let mut segment = |from: f32, to: f32, fill: Color, wave: bool| {
            let (from, to) = (from.clamp(0.0, 1.0), to.clamp(0.0, 1.0));
            if to - from > 0.0 {
                marks.push((from, to, fill, wave));
            }
        };
        let [(tail1, head1), (tail2, head2)] = sweep_bars(phase);
        if head1 < 1.0 - gap {
            segment(
                if head1 > 0.0 {
                    head1 + space(head1)
                } else {
                    0.0
                },
                1.0,
                self.track,
                false,
            );
        }
        segment(tail1, head1, self.active, true);
        if tail1 > gap {
            let from = if head2 > 0.0 {
                head2 + space(head2)
            } else {
                0.0
            };
            let to = if tail1 < 1.0 {
                tail1 - space(1.0 - tail1)
            } else {
                1.0
            };
            segment(from, to, self.track, false);
        }
        segment(tail2, head2, self.active, true);
        if tail2 > gap {
            let to = if tail2 < 1.0 {
                tail2 - space(1.0 - tail2)
            } else {
                1.0
            };
            segment(0.0, to, self.track, false);
        }
        marks
    }

    /// One wave over `from..to`, sampled and stroked, with a round cap inside each end.
    fn push_wave(
        &self,
        shapes: &mut Vec<Shape>,
        from: f32,
        to: f32,
        fill: Color,
        mid: f32,
        phase: f32,
    ) {
        let radius = self.thickness * 0.5;
        let span = to - from;
        if span <= 0.0 {
            return;
        }
        let peak = self.peak * self.amplitude;
        let wavelength = self.wavelength.max(1.0);
        let at = |x: f32| {
            Point::new(
                x,
                mid + peak * (std::f32::consts::TAU * (x / wavelength + phase)).sin(),
            )
        };
        if span <= self.thickness {
            shapes.push(circle(fill, (from + to) * 0.5, mid, span));
            return;
        }
        let (start, end) = (from + radius, to - radius);
        let steps = (((end - start) / WAVE_STEP).ceil() as usize).max(1);
        let mut previous = at(start);
        for step in 1..=steps {
            let next = at(start + (end - start) * step as f32 / steps as f32);
            shapes.push(Shape::Line {
                from: previous,
                to: next,
                thickness: self.thickness,
                colour: fill,
            });
            previous = next;
        }
        shapes.push(circle(fill, start, at(start).y, self.thickness));
        shapes.push(circle(fill, end, at(end).y, self.thickness));
    }
}

/// A round-capped bar from `from` to `to` centred on `mid`, as the classic form paints one.
fn rect(from: f32, to: f32, mid: f32, thickness: f32, colour: Color) -> Shape {
    let radius = thickness * 0.5;
    Shape::Rect {
        rect: Rect::new(from, mid - radius, to - from, thickness),
        corners: Corners::all(radius),
        fill: colour,
        gradient: None,
        border: None,
    }
}

/// A dot of `diameter` centred at `(x, y)`.
fn circle(colour: Color, x: f32, y: f32, diameter: f32) -> Shape {
    let radius = diameter * 0.5;
    Shape::Rect {
        rect: Rect::new(x - radius, y - radius, diameter, diameter),
        corners: Corners::all(radius),
        fill: colour,
        gradient: None,
        border: None,
    }
}

/// The sweep's clock: reading the age every frame is what keeps the loop asking for frames.
fn age_of(id: Id, cx: &mut Cx<'_, '_>) -> f32 {
    cx.at(id).age(id.with("sweep"))
}

/// The four endpoints of the two sweeping bars at `phase`, as fractions of the width.
pub fn sweep_bars(phase: f32) -> [(f32, f32); 2] {
    // The curves are Flutter's, in fractions of the 1800 ms loop.
    let line1_head = interval(
        phase,
        0.0,
        750.0 / 1800.0,
        Easing::Cubic(0.2, 0.0, 0.8, 1.0),
    );
    let line1_tail = interval(
        phase,
        333.0 / 1800.0,
        1083.0 / 1800.0,
        Easing::Cubic(0.4, 0.0, 1.0, 1.0),
    );
    let line2_head = interval(
        phase,
        1000.0 / 1800.0,
        1567.0 / 1800.0,
        Easing::Cubic(0.0, 0.0, 0.65, 1.0),
    );
    let line2_tail = interval(
        phase,
        1267.0 / 1800.0,
        1.0,
        Easing::Cubic(0.10, 0.0, 0.45, 1.0),
    );
    [(line1_tail, line1_head), (line2_tail, line2_head)]
}

/// The ring's arc at `age` seconds, as `(start, end)` in turns from three o'clock, clockwise.
pub fn ring_arc(age: f32) -> (f32, f32) {
    // The head eases over the first half of the path cycle and the tail over the second, so
    // the arc grows to 270° and then shrinks back to nothing.
    let path = (age / PATH_SECONDS).fract();
    let head = FAST_OUT_SLOW_IN.apply((path * 2.0).min(1.0));
    let tail = FAST_OUT_SLOW_IN.apply(((path - 0.5) * 2.0).max(0.0));
    let rotation = (age / ROTATION_SECONDS).fract();
    // Flutter's `arcStart`: twelve o'clock, then the tail, the rotation and the offset.
    let start = -0.25 + tail * 0.75 + rotation + path * 0.25;
    let sweep = (head * 0.75 - tail * 0.75).max(MIN_SWEEP);
    (start, start + sweep)
}

/// `CurveTween(Interval(begin, end, curve))`: flat before, eased within, flat after.
fn interval(t: f32, begin: f32, end: f32, easing: Easing) -> f32 {
    if t <= begin {
        0.0
    } else if t >= end {
        1.0
    } else {
        easing.apply((t - begin) / (end - begin))
    }
}

impl Widget for Md3ProgressIndicator {
    fn change(
        &self,
        _state: &WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        // A progress bar is read-only: it reflects the app's value and produces no semantic
        // change of its own.
        None
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        if let Some(size) = self.size {
            return self.ring(size);
        }
        if self.wavy {
            return self.wavy_bar();
        }
        let radius = self.thickness * 0.5;
        // A zero-length segment paints nothing, so neither a fresh bar nor a sweep at its
        // ends shows a rounded nub (the slider's zero-length rule, reused).
        let bar = |from: f32, to: f32, fill: Color| rect(from, to, radius, self.thickness, fill);
        let mut shapes = Vec::with_capacity(5);
        match self.age.map(|age| (age / SWEEP_SECONDS).fract()) {
            None => {
                shapes.push(Shape::Box {
                    corners: Corners::all(radius),
                    fill: self.track,
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                });
                if self.value > 0.0 {
                    shapes.push(bar(0.0, self.value * self.width, self.active));
                }
            }
            Some(phase) => {
                for (from, to, fill, _) in self.sweep_marks(phase) {
                    shapes.push(bar(from * self.width, to * self.width, fill));
                }
            }
        }
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(self.thickness),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .shapes(shapes)
    }
}
