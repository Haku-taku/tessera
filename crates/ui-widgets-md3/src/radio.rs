//! MD3 radio button, rebuilt from Flutter's `_RadioPainter`: a 20dp ring whose colour is the
//! mark, and an inner dot that grows out of the centre with the selection.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{AnimSpec, CursorShape, Decl, Easing, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, NodeCx, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::state_disc;
use crate::paint::{between, spec_ink};
use crate::spec::{RadioButtonFlags, RadioButtonSpec, radio_button_spec};

/// The unselected state; its `icon.size` is the ring's outer diameter.
const OFF: RadioButtonSpec = radio_button_spec(RadioButtonFlags::NONE);

/// `icon.size`: the ring's outer diameter.
pub const RING: f32 = OFF.icon.size;
/// The export carries no ring width; Flutter's M3 default side is 2dp.
pub(crate) const STROKE: f32 = 2.0;
/// Flutter's `_kInnerRadius`; the dot's radius at full selection.
pub const DOT_RADIUS: f32 = 4.5;
/// `MaterialTapTargetSize.padded`: the ring sits in a 48dp target.
pub(crate) const TOUCH: f32 = 48.0;

/// The position clock. The export carries no duration; Flutter animates the toggleable
/// position linearly, and the dot's radius is linear in it.
const SELECT_MS: f32 = 0.2;
const SELECT: AnimSpec = AnimSpec::new(SELECT_MS, Easing::Linear);

/// One MD3 radio button; every animation is advanced in `new_with`, so `decl` stays pure.
pub struct Md3Radio {
    id: Id,
    ring: Color,
    /// The inner dot's colour and radius, painted only once the selection has begun.
    dot: Option<(Color, f32)>,
    /// The state-layer disc: colour and radius.
    layer: Option<(Color, f32)>,
    semantic_on: bool,
    disabled: bool,
}

impl Md3Radio {
    /// The unprefixed form: constants only, as a direct caller expects.
    pub fn new(id: Id, selected: bool, ctx: &mut NodeCx<'_>, pack: &Style) -> Self {
        Self::new_with(Md3Options::default(), id, selected, ctx, pack)
    }

    /// Same control with explicit parameters, so the semantic path and a direct caller
    pub fn new_with(
        params: Md3Options,
        id: Id,
        selected: bool,
        ctx: &mut NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let state = ctx.state();
        let disabled = state.disabled || params.disabled;
        let position = ctx.animate(
            id.with(param::PROGRESS),
            if selected { 1.0 } else { 0.0 },
            SELECT,
        );

        // Both states read one `icon.color`, and the ring *is* the mark, so the colour lerps
        // across the whole travel rather than switching at a threshold.
        let lit = state.hovered || state.focus_visible;
        let on = radio_button_spec(RadioButtonFlags {
            selected: true,
            hovered: lit,
            focused: state.focus_visible,
            pressed: state.pressed,
            disabled,
        });
        let off = radio_button_spec(RadioButtonFlags {
            hovered: lit,
            focused: state.focus_visible,
            pressed: state.pressed,
            disabled,
            ..RadioButtonFlags::NONE
        });
        let ring = between(
            spec_ink(pack, off.icon.color, off.icon.opacity),
            spec_ink(pack, on.icon.color, on.icon.opacity),
            position,
        );
        // `drawCircle(center, innerRadius * position.value)`: the dot grows from nothing.
        let dot = (position > 0.0).then_some((ring, DOT_RADIUS * position));

        let layer = state_disc(
            ctx,
            id,
            OFF.state_layer.size,
            disabled,
            state.pressed,
            |hovered, focused, pressed| {
                let s = radio_button_spec(RadioButtonFlags {
                    selected,
                    hovered,
                    focused,
                    pressed,
                    disabled,
                });
                spec_ink(pack, s.state_layer.color, s.state_layer.opacity)
            },
        );

        Self {
            id,
            ring,
            dot,
            layer,
            semantic_on: selected,
            disabled,
        }
    }
}

impl Widget for Md3Radio {
    fn handles_disabled(&self) -> bool {
        // The token table paints disabled (38%) itself; the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<ui_core::Change> {
        // A radio that is already selected has nothing to report: the group is unchanged.
        if self.disabled || !state.activation() || self.semantic_on {
            return None;
        }
        Some(ui_core::Change::Selected(Some(self.id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let origin = (TOUCH - RING) * 0.5;
        let mut shapes = Vec::with_capacity(3);
        if let Some((colour, radius)) = self.layer {
            shapes.push(Shape::Rect {
                rect: Rect::new(
                    TOUCH * 0.5 - radius,
                    TOUCH * 0.5 - radius,
                    radius * 2.0,
                    radius * 2.0,
                ),
                corners: Corners::all(radius),
                fill: colour,
                gradient: None,
                border: None,
            });
        }
        // The ring is an inward border on a 20dp circle, so the mark never grows the control.
        shapes.push(Shape::Rect {
            rect: Rect::new(origin, origin, RING, RING),
            corners: Corners::all(RING * 0.5),
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((self.ring, STROKE)),
        });
        if let Some((colour, radius)) = self.dot {
            shapes.push(Shape::Rect {
                rect: Rect::new(
                    TOUCH * 0.5 - radius,
                    TOUCH * 0.5 - radius,
                    radius * 2.0,
                    radius * 2.0,
                ),
                corners: Corners::all(radius),
                fill: colour,
                gradient: None,
                border: None,
            });
        }
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(TOUCH),
                height: AxisSize::fixed(TOUCH),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // The radio takes hover, click and focus; its shapes take nothing.
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .shapes(shapes)
    }
}
