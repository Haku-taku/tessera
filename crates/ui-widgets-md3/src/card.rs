//! MD3 cards: one surface in three variants — elevated, filled, outlined.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{CursorShape, Decl, Id, InkRadius, NodeKind, PressInk, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{role, spec_ink};
use crate::spec::{
    ElevatedCardFlags, FilledCardFlags, OutlinedCardFlags, elevated_card_spec, filled_card_spec,
    outlined_card_spec,
};

use crate::motion::REACTION;

/// MD3's card corner; the export says `rounded_corners` without a number.
pub const CARD_RADIUS: f32 = 12.0;
/// MD3's content inset for a card; the export carries no padding token.
pub const CARD_PAD: f32 = 16.0;

/// Which card this is; the export has one token table per variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardVariant {
    Elevated,
    Filled,
    Outlined,
}

/// The values one variant resolves for one state, normalised across the three exports.
struct Parts {
    fill: &'static str,
    fill_op: f32,
    elevation: f32,
    outline: &'static str,
    outline_op: f32,
    outline_w: f32,
    layer: &'static str,
    layer_op: f32,
    ring: &'static str,
    ring_w: f32,
}

fn parts(
    variant: CardVariant,
    hovered: bool,
    focused: bool,
    pressed: bool,
    dragged: bool,
    disabled: bool,
) -> Parts {
    match variant {
        CardVariant::Elevated => {
            let v = elevated_card_spec(ElevatedCardFlags {
                hovered,
                focused,
                pressed,
                dragging: dragged,
                disabled,
            });
            Parts {
                fill: v.container.color,
                fill_op: v.container.opacity,
                elevation: v.container.elevation,
                // These two variants carry no `outline` object at all.
                outline: "",
                outline_op: 1.0,
                outline_w: 0.0,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                ring: v.focus_indicator.color,
                ring_w: v.focus_indicator.thickness,
            }
        }
        CardVariant::Filled => {
            let v = filled_card_spec(FilledCardFlags {
                hovered,
                focused,
                pressed,
                dragging: dragged,
                disabled,
            });
            Parts {
                fill: v.container.color,
                fill_op: v.container.opacity,
                elevation: v.container.elevation,
                // These two variants carry no `outline` object at all.
                outline: "",
                outline_op: 1.0,
                outline_w: 0.0,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                ring: v.focus_indicator.color,
                ring_w: v.focus_indicator.thickness,
            }
        }
        CardVariant::Outlined => {
            let v = outlined_card_spec(OutlinedCardFlags {
                hovered,
                focused,
                pressed,
                dragging: dragged,
                disabled,
            });
            Parts {
                fill: v.container.color,
                // Only this variant has no `container.opacity`: it disables the ring instead.
                fill_op: 1.0,
                elevation: v.container.elevation,
                outline: v.outline.color,
                outline_op: v.outline.opacity,
                outline_w: v.outline.width,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                ring: v.focus_indicator.color,
                ring_w: v.focus_indicator.thickness,
            }
        }
    }
}

/// A card: the surface the app fills with its own content.
///
/// The card draws no text, so content ink is the caller's: MD3 asks for `onSurface` on the
/// title and `onSurfaceVariant` on the body, both at `DISABLED_CONTENT_OPACITY` when the card
/// is disabled. A fixed colour makes a card readable in one appearance and not the other.
pub struct Md3Card {
    id: Id,
    variant: CardVariant,
    fill: Color,
    outline: Option<(Color, f32)>,
    shadow: Option<ShadowSpec>,
    layer: Option<Color>,
    /// The press ink the engine splats at the pointer.
    press: Option<PressInk>,
    ring: Option<(Color, f32)>,
    pad: f32,
    clickable: bool,
    disabled: bool,
    content: Vec<Decl>,
}

impl Md3Card {
    /// Build one card. `content` is whatever the app puts inside; an empty list gives an
    /// empty surface.
    pub fn new_with(
        params: Md3Options,
        id: Id,
        variant: CardVariant,
        content: Vec<Decl>,
        clickable: bool,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let mut node = cx.at(id);
        let state = node.state();
        let disabled = params.disabled || state.disabled;
        let rest = parts(
            variant,
            state.hovered,
            state.focus_visible,
            state.pressed,
            false,
            disabled,
        );
        let hover = parts(variant, true, false, false, false, disabled);
        let focus = parts(variant, false, true, false, false, disabled);
        let press = parts(variant, false, false, true, false, disabled);
        let hover_t = node.animate(id.with(param::HOVER), f32::from(state.hovered), REACTION);
        let focus_t = node.animate(
            id.with(param::FOCUS),
            f32::from(state.focus_visible),
            REACTION,
        );
        let layer_ink = role(pack, hover.layer);
        // A static card has no interaction, so it paints no layer even when hovered.
        let interactive = clickable && !disabled;
        let strength = (hover_t * hover.layer_op).max(focus_t * focus.layer_op);
        let layer = (strength > 0.0 && interactive).then_some(Color {
            a: layer_ink.a * strength,
            ..layer_ink
        });
        // A press is a splat from the pointer, not another rise of the flat layer.
        let ink = interactive.then(|| {
            press_ink(
                Color {
                    a: layer_ink.a * press.layer_op,
                    ..layer_ink
                },
                InkRadius::Cover,
            )
        });
        let ring_t = node.animate(
            id.with(param::THICKNESS),
            f32::from(state.focus_visible),
            REACTION,
        );
        let ring_ink = spec_ink(pack, rest.ring, 1.0);
        let ring = (ring_t > 0.0 && clickable && !disabled).then_some((
            Color {
                a: ring_ink.a * ring_t,
                ..ring_ink
            },
            rest.ring_w,
        ));
        let outline = (rest.outline_w > 0.0 && !rest.outline.is_empty()).then(|| {
            (
                spec_ink(pack, rest.outline, rest.outline_op),
                rest.outline_w,
            )
        });
        let shadow = (rest.elevation > 0.0)
            .then(|| ShadowSpec::black(rest.elevation * 2.0, 0.18, rest.elevation * 0.5));
        Self {
            id,
            variant,
            fill: spec_ink(pack, rest.fill, rest.fill_op),
            outline,
            shadow,
            layer,
            press: ink,
            ring,
            pad: CARD_PAD,
            clickable,
            disabled,
            content,
        }
    }

    /// The variant this card was built as.
    pub fn variant(&self) -> CardVariant {
        self.variant
    }

    /// The corner radius, which the export names but does not size.
    pub fn radius(&self) -> f32 {
        CARD_RADIUS
    }

    /// The content inset the card applies to whatever it holds.
    pub fn pad(&self) -> f32 {
        self.pad
    }
}

impl Widget for Md3Card {
    fn handles_disabled(&self) -> bool {
        // The export carries the disabled container itself, so the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        (self.clickable && !self.disabled && state.activation())
            .then_some(ui_core::Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut shapes = Vec::with_capacity(3);
        let mut b = Shape::Box {
            corners: Corners::all(CARD_RADIUS),
            fill: self.fill,
            gradient: None,
            border: self.outline,
            shadow: self.shadow,
            image: None,
        };
        if let Shape::Box { shadow, .. } = &mut b {
            *shadow = self.shadow;
        }
        shapes.push(b);
        if let Some(layer) = self.layer {
            shapes.push(Shape::Box {
                corners: Corners::all(CARD_RADIUS),
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        // The export insets the ring by `focus.indicator.outline.offset`; it is drawn on the
        // card's own edge instead, since the decl cannot know the final rect.
        if let Some((ink, width)) = self.ring {
            shapes.push(Shape::Box {
                corners: Corners::all(CARD_RADIUS),
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((ink, width)),
                shadow: None,
                image: None,
            });
        }
        let mut card = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::all(self.pad))
            .shapes(shapes);
        if self.clickable {
            card = card
                .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
                .cursor(if self.disabled {
                    CursorShape::Arrow
                } else {
                    CursorShape::PointingHand
                })
                .disabled(self.disabled)
                .press_ink(self.press);
        }
        card.children(self.content.iter().cloned())
    }
}
