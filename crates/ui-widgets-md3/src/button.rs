//! MD3 buttons: five styles, five sizes, one pill with a centred icon and label.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{CursorShape, Decl, Id, InkRadius, NodeKind, PressInk, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{role, spec_ink};
use crate::spec::{ButtonFlags, ButtonSize, ButtonStyle, ButtonToggle, button_spec};
use crate::typescale::{self, TypeStyle};

use crate::motion::REACTION;

/// The label's type style, which the export names per size but does not give numbers for.
pub fn label_style(size: ButtonSize) -> TypeStyle {
    let spec = button_spec(
        ButtonFlags::NONE,
        ButtonStyle::Filled,
        size,
        ButtonToggle::Base,
    );
    typescale::named(spec.root.label_text)
}

/// Whether this style paints a container at all: text buttons never, outlined only when the
/// toggle is selected (its selected colour is a real fill, `inverse-surface`).
/// `inverse-surface`).
fn paints_container(style: ButtonStyle, toggle: ButtonToggle, disabled: bool) -> bool {
    match style {
        ButtonStyle::Text => false,
        ButtonStyle::Outlined => toggle == ButtonToggle::Selected && !disabled,
        _ => true,
    }
}

/// Everything about one button that is fixed when it is built.
#[derive(Clone, Debug)]
pub struct ButtonParams {
    pub id: Id,
    pub label: String,
    pub style: ButtonStyle,
    pub size: ButtonSize,
    pub toggle: ButtonToggle,
    /// Draws the leading icon in the ink it is given; `None` leaves the label centred.
    pub icon: Option<IconSource>,
    /// The container's curve. `None` is the button's own pill; a group passes the corners
    /// that follow from where the member sits in it.
    pub corners: Option<Corners>,
}

impl ButtonParams {
    /// A filled, medium, plain button — the export's default.
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            style: ButtonStyle::Filled,
            size: ButtonSize::Medium,
            toggle: ButtonToggle::Base,
            icon: None,
            corners: None,
        }
    }

    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Make it a toggle; the three values are `Base`, `Selected` and `Unselected`.
    pub fn toggle(mut self, toggle: ButtonToggle) -> Self {
        self.toggle = toggle;
        self
    }

    pub fn icon(mut self, icon: IconSource) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// A resolved button: every colour and metric the decl needs, already animated.
pub struct Md3Button {
    id: Id,
    label: String,
    icon: Option<(IconSource, Color)>,
    icon_size: f32,
    height: f32,
    /// The container's curve, which a group overrides for the side facing a neighbour.
    corners: Corners,
    pad: Insets,
    gap: f32,
    container: Option<Color>,
    outline: Option<(Color, f32)>,
    shadow: Option<ShadowSpec>,
    layer: Option<Color>,
    /// The press ink the engine splats at the pointer.
    press: Option<PressInk>,
    label_ink: Color,
    label_style: TypeStyle,
    ring: Option<(Color, f32)>,
    disabled: bool,
}

impl Md3Button {
    /// Build one button. `Base` is a plain button; the other two make it a toggle, whose state
    /// the app owns; a press is reported as `Change::Activated`.
    pub(crate) fn new_with(
        params: Md3Options,
        ButtonParams {
            id,
            label,
            style,
            size,
            toggle,
            icon,
            corners,
        }: ButtonParams,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let mut node = cx.at(id);
        let state = node.state();
        let disabled = params.disabled || state.disabled;
        let spec = button_spec(
            ButtonFlags {
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
                disabled,
            },
            style,
            size,
            toggle,
        );
        // The layer's ink is the same under hover/focus/press; only its strength differs.
        let hover = button_spec(
            ButtonFlags {
                hovered: true,
                disabled,
                ..ButtonFlags::NONE
            },
            style,
            size,
            toggle,
        );
        let focus = button_spec(
            ButtonFlags {
                focused: true,
                disabled,
                ..ButtonFlags::NONE
            },
            style,
            size,
            toggle,
        );
        let press = button_spec(
            ButtonFlags {
                pressed: true,
                disabled,
                ..ButtonFlags::NONE
            },
            style,
            size,
            toggle,
        );
        let hover_t = node.animate(id.with(param::HOVER), f32::from(state.hovered), REACTION);
        let focus_t = node.animate(
            id.with(param::FOCUS),
            f32::from(state.focus_visible),
            REACTION,
        );
        let layer_ink = role(pack, hover.state_layer.color);
        let strength =
            (hover_t * hover.state_layer.opacity).max(focus_t * focus.state_layer.opacity);
        let layer = (strength > 0.0 && !disabled).then_some(Color {
            a: layer_ink.a * strength,
            ..layer_ink
        });
        // A press is a splat from the pointer, not another rise of the flat layer.
        let ink = (!disabled).then(|| {
            press_ink(
                Color {
                    a: layer_ink.a * press.state_layer.opacity,
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
        let ring_ink = spec_ink(pack, spec.focus_indicator.color, 1.0);
        let ring = (ring_t > 0.0 && !disabled).then_some({
            (
                Color {
                    a: ring_ink.a * ring_t,
                    ..ring_ink
                },
                spec.focus_indicator.thickness,
            )
        });
        let container = paints_container(style, toggle, disabled)
            .then(|| spec_ink(pack, spec.container.color, spec.container.opacity));
        let outline = (spec.outline.width > 0.0)
            .then(|| (spec_ink(pack, spec.outline.color, 1.0), spec.outline.width));
        // MD3's elevation is a 1dp ambient+caster pair; one analytic shadow stands in for both.
        let shadow = (spec.container.elevation > 0.0).then(|| {
            ShadowSpec::black(
                spec.container.elevation * 2.0,
                0.18,
                spec.container.elevation * 0.5,
            )
        });
        let label_ink = spec_ink(pack, spec.label_text.color, spec.label_text.opacity);
        let icon_ink = spec_ink(pack, spec.icon.color, spec.icon.opacity);
        Self {
            id,
            label,
            icon: icon.map(|f| (f, icon_ink)),
            icon_size: spec.icon.size,
            height: spec.container.height,
            pad: Insets {
                left: spec.root.leading_space,
                right: spec.root.trailing_space,
                // The label's line box is shorter than the container; nothing else pads it.
                top: 0.0,
                bottom: 0.0,
            },
            gap: spec.root.icon_label_space,
            corners: corners.unwrap_or_else(|| Corners::all(spec.container.height * 0.5)),
            container,
            outline,
            shadow,
            layer,
            press: ink,
            label_ink,
            label_style: label_style(size),
            ring,
            disabled,
        }
    }

    /// The container's own height, which is also the pill's diameter.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// The width the pill takes around its label, measured as layout will measure it.
    ///
    /// `None` when the host supplied no measurer; a caller must then keep its own fallback.
    pub fn width_in(&self, cx: &ui_widgets::Cx<'_, '_>) -> Option<f32> {
        let label = Text::new(
            self.id.with("label"),
            self.label.clone(),
            self.label_style.size,
            self.label_ink,
        )
        .weight(self.label_style.weight);
        let text = label.width_in(cx)?;
        let icon = if self.icon.is_some() {
            ui_widgets::icon_extent(self.icon_size) + self.gap
        } else {
            0.0
        };
        Some(self.pad.left + self.pad.right + icon + text)
    }

    /// A node-sized box: it follows whatever rect the layout hands the node.
    fn boxed(&self, fill: Color, border: Option<(Color, f32)>) -> Shape {
        Shape::Box {
            corners: self.corners,
            fill,
            gradient: None,
            border,
            shadow: None,
            image: None,
        }
    }
}

impl Widget for Md3Button {
    fn handles_disabled(&self) -> bool {
        // The 38% content fade comes from the export's `opacity` tokens, painted here.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        (!self.disabled && state.activation()).then_some(ui_core::Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut shapes = Vec::with_capacity(3);
        match self.container {
            Some(fill) => {
                let mut b = self.boxed(fill, self.outline);
                if let Shape::Box { shadow, .. } = &mut b {
                    *shadow = self.shadow;
                }
                shapes.push(b);
            }
            // An outlined button still draws its ring when the container itself is clear.
            None => {
                if let Some(border) = self.outline {
                    shapes.push(self.boxed(Color::TRANSPARENT, Some(border)));
                }
            }
        }
        if let Some(layer) = self.layer {
            shapes.push(self.boxed(layer, None));
        }
        // The export insets the ring by `outline.offset` (outwards); it is drawn on the
        // button's own edge instead, since the decl cannot know the final rect.
        if let Some((ink, width)) = self.ring {
            shapes.push(self.boxed(Color::TRANSPARENT, Some((ink, width))));
        }
        let mut row = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(self.pad)
            .gap(self.gap)
            // Children hug their own size and are centred in the space left over.
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(self.press)
            // The pill is the button's curve whatever it paints: a text button has no
            // container at rest, so a curve inferred from the shapes would square off.
            .corners(self.corners)
            .shapes(shapes);
        if let Some((source, ink)) = self.icon {
            row = row.child(source.resolve(self.id.with("icon"), ink));
        }
        row.child(
            Text::new(
                self.id.with("label"),
                self.label.clone(),
                self.label_style.size,
                self.label_ink,
            )
            .weight(self.label_style.weight)
            .resolve(),
        )
    }
}
