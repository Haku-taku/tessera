//! The state-layer disc MD3 controls paint behind their shape.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Rect};
use ui_core::{Id, InkRadius, InkTarget, PressInk};
use ui_widgets::NodeCx;
use ui_widgets::build::param;

use crate::paint::between;

use crate::motion::REACTION;

/// This frame's disc: its colour and radius, or `None` when nothing is showing.
///
/// A toggleable's reaction grows from the **control's centre**, as Flutter's
/// `ToggleablePainter.paintRadialReaction` does; only surfaces that ripple (buttons, chips,
/// rows, tabs) splat at the pointer, and that ink belongs to the engine.
pub(crate) fn state_disc(
    ctx: &mut NodeCx<'_>,
    id: Id,
    diameter: f32,
    disabled: bool,
    pressed: bool,
    colour_of: impl Fn(bool, bool, bool) -> Color,
) -> Option<(Color, f32)> {
    let state = ctx.state();
    // The press splash grows from the centre; hover and focus reach the full disc and fade.
    let reaction = ctx.animate(id.with(param::PRESS), f32::from(pressed), REACTION);
    let hover = ctx.animate(id.with(param::HOVER), f32::from(state.hovered), REACTION);
    let focus = ctx.animate(
        id.with(param::FOCUS),
        f32::from(state.focus_visible),
        REACTION,
    );
    // Snap at the ends: a settled hover must report the exported colour, not a lerp ULP off.
    let colour = between(
        between(
            colour_of(false, false, true),
            colour_of(true, false, false),
            hover,
        ),
        colour_of(false, true, false),
        focus,
    );
    let full = diameter * 0.5;
    let radius = if state.hovered || state.focus_visible {
        full
    } else {
        reaction * full
    };
    // A running fade still counts as showing: its first frame is 0 but the disc exists.
    let showing = pressed || state.hovered || state.focus_visible || hover > 0.0 || focus > 0.0;
    (showing && radius > 0.0 && !disabled).then_some((colour, radius))
}

/// The ink a control splats on press, from an already-resolved state-layer colour.
pub(crate) fn press_ink(ink: Color, radius: InkRadius) -> PressInk {
    PressInk::new(ink, radius)
}

/// Press ink sized to and clipped by a named descendant — the effective background.
pub(crate) fn press_ink_in(ink: Color, radius: InkRadius, clip: Id) -> PressInk {
    PressInk::with_target(ink, radius, InkTarget::Descendant(clip))
}

/// Press ink sized to and clipped by the nth shape the control paints.
pub(crate) fn press_ink_shape(ink: Color, radius: InkRadius, index: usize) -> PressInk {
    PressInk::with_target(ink, radius, InkTarget::Shape(index))
}

/// A control's effective background: the one slot its state layer, its selection and its
/// ripple all use, whether or not this frame paints anything there.
///
/// It is a fixed slot, not the shape currently painted: a capsule that has not opened yet, or
/// an unselected row with no fill at all, still owns a rect and a curve to cut the ripple to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Background {
    pub rect: Rect,
    pub corners: Corners,
}

impl Background {
    /// A background at an explicit rect and curve, in the node's own space.
    pub fn new(rect: Rect, corners: Corners) -> Self {
        Self { rect, corners }
    }

    /// A stadium filling `width × height` from the node's origin.
    pub fn stadium(width: f32, height: f32) -> Self {
        Self::new(
            Rect::new(0.0, 0.0, width, height),
            Corners::all(height * 0.5),
        )
    }

    /// The ink a press splats: sized to and clipped by this background.
    pub fn press(&self, ink: Color, radius: InkRadius) -> PressInk {
        PressInk::with_target(
            ink,
            radius,
            InkTarget::Rect {
                rect: self.rect,
                corners: self.corners,
            },
        )
    }

    /// A fill covering the whole slot: a state layer or a selection.
    pub fn fill(&self, colour: Color) -> Shape {
        Shape::Rect {
            rect: self.rect,
            corners: self.corners,
            fill: colour,
            gradient: None,
            border: None,
        }
    }

    /// A focus ring on the slot's own edge.
    pub fn ring(&self, colour: Color, width: f32) -> Shape {
        Shape::Rect {
            rect: self.rect,
            corners: self.corners,
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((colour, width)),
        }
    }
}
