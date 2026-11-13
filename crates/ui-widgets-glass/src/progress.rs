//! The glass progress indicator: a 20 dp ring or a 4 dp bar, determinate or sweeping.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Point};
use ui_core::layout::{AxisSize, Clip, Grow, Len, LenInsets, Overflow, SizeClass};
use ui_core::{Decl, Id, NodeKind};
use ui_style::Style;
use ui_widgets::{Cx, Widget};

use crate::paint::role_of;

/// iOS 26's ring: 20 dp across, a 2.5 dp stroke, one turn per second while sweeping.
pub const PROGRESS_SIZE: f32 = 20.0;
pub const PROGRESS_STROKE: f32 = 2.5;
pub const PROGRESS_CYCLE: f32 = 1.0;
/// The indeterminate ring's visible arc: 90°, the iOS standard.
pub const PROGRESS_ARC: f32 = 0.25;
/// The bar: 4 dp tall, never narrower than 200 dp, its sweep 30 % of the track.
pub const PROGRESS_BAR_H: f32 = 4.0;
pub const PROGRESS_MIN_W: f32 = 200.0;
pub const PROGRESS_SWEEP: f32 = 0.3;
/// The track is 15 % of white or black by appearance, as `systemFill` is; the halo is the
/// ink at 30 %.
pub const PROGRESS_TRACK_ALPHA: f32 = 0.15;
pub const PROGRESS_GLOW_ALPHA: f32 = 0.3;
/// The halo's stroke is `strokeWidth + 2`, so it reaches one dp past each side.
pub const PROGRESS_GLOW_GROWTH: f32 = 1.0;
/// The sweep starts at twelve o'clock; a sector's zero sits at three.
const TWELVE_O_CLOCK: f32 = -0.25;

/// Which of the reference's two shapes this indicator is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressKind {
    Ring,
    Bar,
}

/// One indicator's inputs; `value` of `None` makes it sweep on its own clock.
#[derive(Clone, Copy, Debug)]
pub struct GlassProgressParams {
    pub id: Id,
    pub kind: ProgressKind,
    pub value: Option<f32>,
    pub size: f32,
    pub stroke: f32,
    pub height: f32,
    pub min_width: f32,
}

impl GlassProgressParams {
    /// A ring; `None` spins it.
    pub fn ring(id: Id, value: Option<f32>) -> Self {
        Self {
            id,
            kind: ProgressKind::Ring,
            value: value.map(|v| v.clamp(0.0, 1.0)),
            size: PROGRESS_SIZE,
            stroke: PROGRESS_STROKE,
            height: PROGRESS_BAR_H,
            min_width: PROGRESS_MIN_W,
        }
    }

    /// A bar; `None` sweeps it.
    pub fn bar(id: Id, value: Option<f32>) -> Self {
        Self {
            kind: ProgressKind::Bar,
            ..Self::ring(id, value)
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn stroke(mut self, stroke: f32) -> Self {
        self.stroke = stroke;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn min_width(mut self, min_width: f32) -> Self {
        self.min_width = min_width;
        self
    }
}

/// One glass indicator; the sweep's phase is read here so `decl()` stays pure.
pub struct GlassProgress {
    id: Id,
    kind: ProgressKind,
    value: Option<f32>,
    size: f32,
    stroke: f32,
    height: f32,
    min_width: f32,
    ink: Color,
    track: Color,
    /// The sweeping clock in turns of its one-second loop.
    phase: f32,
}

impl GlassProgress {
    pub fn new(params: GlassProgressParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassProgressParams {
            id,
            kind,
            value,
            size,
            stroke,
            height,
            min_width,
        } = params;
        // Only a sweeping indicator owns a clock; a determinate one must let the frame loop
        // rest, so it never asks for the time.
        let phase = match value {
            Some(_) => 0.0,
            None => {
                let mut node = cx.at(id);
                (node.age(id.with("phase")) / PROGRESS_CYCLE).fract()
            }
        };
        Self {
            id,
            kind,
            value,
            size: size.max(0.0),
            stroke: stroke.clamp(0.0, size),
            height: height.max(0.0),
            min_width: min_width.max(0.0),
            ink: role_of(pack, "systemBlue"),
            // The reference hardcodes `0x26FFFFFF`, which vanishes on a light surface; the
            // adaptive pair is what Apple's `systemFill` does.
            track: Color {
                a: PROGRESS_TRACK_ALPHA,
                ..if pack.appearance() == Some(ui_design::Appearance::Dark) {
                    Color::WHITE
                } else {
                    Color::BLACK
                }
            },
            phase,
        }
    }

    /// The visible arc in turns, or `None` when there is nothing to paint yet.
    fn arc(&self) -> Option<(f32, f32)> {
        match self.value {
            Some(v) if v > 0.0 => Some((TWELVE_O_CLOCK, TWELVE_O_CLOCK + v)),
            Some(_) => None,
            None => Some((
                TWELVE_O_CLOCK + self.phase,
                TWELVE_O_CLOCK + self.phase + PROGRESS_ARC,
            )),
        }
    }

    /// The bar's left edge and width as percentages of the track.
    fn span(&self) -> Option<(f32, f32)> {
        match self.value {
            Some(v) if v > 0.0 => Some((0.0, v * 100.0)),
            Some(_) => None,
            None => {
                let w = PROGRESS_SWEEP * 100.0;
                Some((self.phase * (100.0 + w) - w, w))
            }
        }
    }

    fn ring_decl(&self) -> Decl {
        let half = self.size * 0.5;
        let centre = Point::new(half, half);
        let mut shapes = Vec::with_capacity(3);
        shapes.push(Shape::Sector {
            centre,
            radius: half,
            thickness: self.stroke,
            trim: (0.0, 1.0),
            round_caps: true,
            colour: self.track,
        });
        if let Some((start, end)) = self.arc() {
            // The engine blurs no arc, so the reference's 4 dp glow is a wider translucent
            shapes.push(Shape::Sector {
                centre,
                radius: half + PROGRESS_GLOW_GROWTH,
                thickness: self.stroke + PROGRESS_GLOW_GROWTH * 2.0,
                trim: (start, end),
                round_caps: true,
                colour: Color {
                    a: PROGRESS_GLOW_ALPHA,
                    ..self.ink
                },
            });
            shapes.push(Shape::Sector {
                centre,
                radius: half,
                thickness: self.stroke,
                trim: (start, end),
                round_caps: true,
                colour: self.ink,
            });
        }
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(self.size),
                height: AxisSize::fixed(self.size),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .shapes(shapes)
    }

    fn bar_decl(&self) -> Decl {
        let corners = Corners::all(self.height * 0.5);
        // The root is both the track and the containing block the percentages resolve
        // against; it clips the sweep at its own rounded ends.
        let root = Decl::container(self.id, NodeKind::Box)
            .relative()
            .size(SizeClass {
                width: AxisSize::fill(self.min_width, self.min_width),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .corners(corners)
            .background(self.track, corners)
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            });
        match self.span() {
            Some((x, w)) => root.child(bar_fill(self.id.with("value"), self.ink, x, w, corners)),
            None => root,
        }
    }
}

impl Widget for GlassProgress {
    fn handles_disabled(&self) -> bool {
        // Neither shape has a disabled state, so nothing else may fade the indicator either.
        true
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        match self.kind {
            ProgressKind::Ring => self.ring_decl(),
            ProgressKind::Bar => self.bar_decl(),
        }
    }
}

/// One rounded bar: left edge at `x_pct` of the track, `width_pct` wide, full height.
///
/// Both edges are percentages, so the sweep slides without layout knowing the track's
/// pixel width — and may start outside it, which the track's clip cuts off.
fn bar_fill(id: Id, fill: Color, x_pct: f32, width_pct: f32, corners: Corners) -> Decl {
    Decl::container(id, NodeKind::Box)
        .absolute()
        .inset(LenInsets::new(
            Len::Pct(x_pct),
            Len::Px(0.0),
            Len::Pct(100.0 - x_pct - width_pct),
            Len::Px(0.0),
        ))
        .size(SizeClass {
            width: AxisSize::hug(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .background(fill, corners)
}
