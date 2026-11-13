//! Pack lookups the controls share: a role, an alpha and the accent ramp.

use ui_core::draw::{Gradient, GradientAxis};
use ui_core::geom::Color;
use ui_design::{GradientBrush, Paint};
use ui_style::Style;

pub(crate) fn role_of(pack: &Style, name: &'static str) -> Color {
    // Curated role first, then the live native palette; a miss is a bug, shown as magenta.
    let c = pack.ink(name);
    debug_assert!(c.is_some(), "neither the pack nor its palette knows {name}");
    c.unwrap_or(Color::from_rgba8(255, 0, 255, 255))
}

/// A border brush as the decl takes it: one flat colour, or the run XAML draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Stroke {
    Solid(Color),
    Gradient(Gradient),
}

impl Stroke {
    /// Paint this stroke on `decl` at `width`, whichever kind it is.
    pub(crate) fn on(self, decl: ui_core::Decl, width: f32) -> ui_core::Decl {
        match self {
            Stroke::Solid(c) => decl.border(c, width),
            Stroke::Gradient(g) => decl.border_gradient(g, width),
        }
    }
}

/// The brush `name` names; a gradient brush keeps its XAML geometry and stops.
pub(crate) fn stroke_of(pack: &Style, name: &'static str) -> Stroke {
    match pack.brush(name) {
        Some(Paint::Gradient(b)) => Stroke::Gradient(gradient_of(&b)),
        _ => Stroke::Solid(role_of(pack, name)),
    }
}

fn gradient_of(b: &GradientBrush) -> Gradient {
    let dx = b.end.0 - b.start.0;
    let dy = b.end.1 - b.start.1;
    // WinUI's elevation borders are vertical; a horizontal brush still maps exactly.
    let (axis, len) = if dx.abs() >= dy.abs() {
        (GradientAxis::Horizontal, dx.abs())
    } else {
        (GradientAxis::Vertical, dy.abs())
    };
    let near = b
        .stops
        .first()
        .copied()
        .unwrap_or((0.0, ui_design::Rgba(0)));
    let far = b.stops.last().copied().unwrap_or((1.0, ui_design::Rgba(0)));
    let mut g = Gradient::new(colour(near.1), colour(far.1), axis).with_stops(near.0, far.0);
    // Absolute mapping runs the brush over its own length, not the box's.
    if !b.relative {
        g = g.over(len);
    }
    if b.flip {
        g = g.flipped();
    }
    g
}

fn colour(c: ui_design::Rgba) -> Color {
    let argb = c.argb();
    Color::from_rgba8(
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    )
}

/// Fluent's selection: the accent wash, with the selected glyphs inverted to white.
#[derive(Debug)]
pub struct FluentSelection;

impl ui_core::SelectionPainter for FluentSelection {
    fn paint(&self, geometry: &ui_core::SelectionGeometry<'_>) -> ui_core::SelectionPaint {
        ui_core::SelectionPaint {
            ink: Some(Color::WHITE),
            ..ui_core::flat_selection(geometry)
        }
    }
}

/// The single instance a pack hands to every text node it styles.
pub static FLUENT_SELECTION: FluentSelection = FluentSelection;
