//! The Fluent progress ring: a 32 dp Lottie ring, determinate or spinning.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Point};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Easing, Id, NodeKind};
use ui_style::Style;
use ui_widgets::{NodeCx, Widget};

use crate::paint::role_of;

/// The style's `Width` / `Height`, and its `MinWidth` / `MinHeight`.
pub const RING_SIZE: f32 = 32.0;
pub const RING_MIN: f32 = 16.0;
/// Both Lottie visuals run on a two-second clock.
pub const RING_CYCLE: f32 = 2.0;
/// `ProgressRingDeterminate`: a 32 canvas, radius 8, sprite scale 1.77.
const DETERMINATE_CANVAS: f32 = 32.0;
const DETERMINATE_RADIUS: f32 = 8.0;
const DETERMINATE_SPRITE: f32 = 1.77;
/// `ProgressRingIndeterminate`: an 80 canvas, radius 7, sprite scale 5.
const INDETERMINATE_CANVAS: f32 = 80.0;
const INDETERMINATE_RADIUS: f32 = 7.0;
const INDETERMINATE_SPRITE: f32 = 5.0;
/// `StrokeThickness` in both Lottie files, in path units.
const LOTTIE_STROKE: f32 = 1.5;
/// The spline both files share: `{0.167,0.167}` to `{0.833,0.833}`.
const EASE: Easing = Easing::Cubic(0.167, 0.167, 0.833, 0.833);
/// Lottie never trims to exactly zero; the determinate file starts at 1e-4.
const TRIM_FLOOR: f32 = 0.0001;
/// Lottie's trim zero sits at twelve o'clock, a quarter turn before the sector's three.
const TWELVE_O_CLOCK: f32 = 0.25;

/// The determinate `TrimEnd` keyframes: progress, trim (`ProgressRingDeterminate.cpp`).
///
/// The arc is drawn through this table rather than straight from the value, because the
/// reference animates the trim over the same 2 s player progress.
const DETERMINATE_TRIM: [(f32, f32); 7] = [
    (0.0, TRIM_FLOOR),
    (0.008_333_334, TRIM_FLOOR),
    (0.25, 0.25),
    (0.5, 0.5),
    (0.75, 0.75),
    (0.983_333_35, 0.966_666_64),
    (0.991_666_7, 1.0),
];

/// Everything a progress ring is built with.
#[derive(Clone, Copy, Debug)]
pub struct ProgressRingParams {
    pub id: Id,
    /// `0.0..=1.0`; the caller clamps, the ring paints what it is handed.
    pub value: f32,
    pub indeterminate: bool,
    /// WinUI's `IsActive`; inactive hides the ring but keeps its slot.
    pub active: bool,
    pub size: f32,
}

impl ProgressRingParams {
    pub fn new(id: Id, value: f32) -> Self {
        Self {
            id,
            value: value.clamp(0.0, 1.0),
            indeterminate: false,
            active: true,
            size: RING_SIZE,
        }
    }

    pub fn indeterminate(mut self, on: bool) -> Self {
        self.indeterminate = on;
        self
    }

    pub fn active(mut self, on: bool) -> Self {
        self.active = on;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

/// One Fluent progress ring; the spinning phase is read here so `decl()` stays pure.
pub struct FluentProgressRing {
    id: Id,
    value: f32,
    indeterminate: bool,
    active: bool,
    size: f32,
    foreground: Color,
    background: Color,
    /// The spinning visual's phase in turns of its two-second clock.
    phase: f32,
}

impl FluentProgressRing {
    pub fn new(params: ProgressRingParams, ctx: &mut NodeCx<'_>, pack: &Style) -> Self {
        let ProgressRingParams {
            id,
            value,
            indeterminate,
            active,
            size,
        } = params;
        // Only the spinning ring owns a clock; a determinate one must let the frame loop rest.
        let phase = if indeterminate && active {
            turns(ctx.age(id.with("phase")))
        } else {
            0.0
        };
        Self {
            id,
            value,
            indeterminate,
            active,
            size: size.max(RING_MIN),
            foreground: role_of(pack, "ProgressRingForegroundThemeBrush"),
            background: role_of(pack, "ProgressRingBackgroundThemeBrush"),
            phase,
        }
    }

    /// The Lottie sprite's centreline radius and stroke width, in this ring's pixels.
    fn stroke(&self) -> (Point, f32, f32) {
        let (canvas, radius, sprite) = if self.indeterminate {
            (
                INDETERMINATE_CANVAS,
                INDETERMINATE_RADIUS,
                INDETERMINATE_SPRITE,
            )
        } else {
            (DETERMINATE_CANVAS, DETERMINATE_RADIUS, DETERMINATE_SPRITE)
        };
        let k = self.size / canvas;
        let path = radius * sprite * k;
        let width = LOTTIE_STROKE * sprite * k;
        let centre = Point::new(self.size * 0.5, self.size * 0.5);
        (centre, path, width)
    }
}

impl Widget for FluentProgressRing {
    fn handles_disabled(&self) -> bool {
        // The template has no disabled state, so nothing else may fade the ring either.
        true
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let root = Decl::container(id, NodeKind::Box).size(SizeClass {
            width: AxisSize::fixed(self.size),
            height: AxisSize::fixed(self.size),
            grow_x: Grow::No,
            grow_y: Grow::No,
        });
        if !self.active {
            return root;
        }
        let (centre, path, width) = self.stroke();
        // The sector measures its radius at the stroke's outer edge, which is where the
        // Lottie stroke, centred on the path, reaches.
        let radius = path + width * 0.5;
        let mut shapes = Vec::with_capacity(2);
        // The default `ProgressRingBackgroundThemeBrush` is transparent, so this track is
        // usually invisible; it still travels, as it does in the reference.
        shapes.push(arc(centre, radius, width, 0.0, 1.0, self.background, false));
        if self.indeterminate {
            let (start, end) = spinning_trim(self.phase);
            let turn = spinning_rotation(self.phase);
            shapes.push(arc(
                centre,
                radius,
                width,
                start + turn,
                end + turn,
                self.foreground,
                true,
            ));
        } else {
            let trim = determinate_trim(self.value);
            shapes.push(arc(centre, radius, width, 0.0, trim, self.foreground, true));
        }
        root.shapes(shapes)
    }
}

/// One stroked arc, stated in Lottie turns (zero at twelve o'clock, clockwise).
fn arc(
    centre: Point,
    radius: f32,
    thickness: f32,
    start: f32,
    end: f32,
    colour: Color,
    round_caps: bool,
) -> Shape {
    Shape::Sector {
        centre,
        radius,
        thickness,
        trim: (start - TWELVE_O_CLOCK, end - TWELVE_O_CLOCK),
        round_caps,
        colour,
    }
}

/// The phase of the two-second loop, in turns.
pub fn turns(age: f32) -> f32 {
    ui_core::anim::phase(age, RING_CYCLE)
}

/// The determinate arc's end, through the reference's own trim keyframes.
pub fn determinate_trim(value: f32) -> f32 {
    let mut stops = [(0.0f32, EASE, 0.0f32); DETERMINATE_TRIM.len() - 1];
    for (slot, pair) in stops.iter_mut().zip(DETERMINATE_TRIM.windows(2)) {
        *slot = (pair[1].0, EASE, pair[1].1);
    }
    ui_core::anim::weighted(value, &stops, TRIM_FLOOR, ui_core::geom::lerp)
}

/// The spinning container's turn count: 450° per half cycle, each half eased from rest.
pub fn spinning_rotation(phase: f32) -> f32 {
    let half = 450.0 / 360.0;
    if phase < 0.5 {
        half * EASE.apply(phase / 0.5)
    } else {
        half + half * EASE.apply((phase - 0.5) / 0.5)
    }
}

/// The visible arc before rotation: the head grows to a half ring, then the tail catches up.
pub fn spinning_trim(phase: f32) -> (f32, f32) {
    if phase < 0.5 {
        // The second sprite: `TrimEnd` 1e-4 → 0.5 while opacity holds at 1.
        (0.0, LOTTIE_HALF_END * EASE.apply(phase / 0.5) + TRIM_FLOOR)
    } else {
        // The first sprite: `TrimEnd` pinned at 0.5, `TrimStart` 0 → 0.5.
        (0.5 * EASE.apply((phase - 0.5) / 0.5), 0.5)
    }
}

/// `0.5 - 1e-4`, the second sprite's `TrimEnd` keyframe's destination.
const LOTTIE_HALF_END: f32 = 0.5 - TRIM_FLOOR;
