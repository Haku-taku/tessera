//! MD3 split button: a filled action and a menu half sharing one height and two inner
//! corners that widen while the pointer is on either half.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Easing, Id, InkRadius, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Icon, IconSource, Widget, WidgetState};

use crate::button::label_style;
use crate::layer::press_ink;
use crate::paint::{role, spec_ink};
use crate::spec::{
    ButtonFlags, ButtonSize, ButtonStyle, ButtonToggle, SplitButtonFlags, button_spec,
    split_button_spec,
};
// The size axis is the caller's vocabulary, not an internal detail.
pub use crate::spec::SplitButtonSize;

use crate::motion::REACTION;

/// The corner morph's clock: the same 100 ms ease-out the icon button's shape uses.
const MORPH: AnimSpec = AnimSpec::new(0.1, Easing::EaseOut);

/// Everything a split button is built with.
#[derive(Clone, Debug)]
pub struct SplitButtonParams {
    pub id: Id,
    pub label: String,
    /// A mark before the label, sized by the filled button's table.
    pub leading: Option<IconSource>,
    pub size: SplitButtonSize,
    /// The menu is open: the trailing half becomes a pill.
    pub selected: bool,
    pub disabled: bool,
}

impl SplitButtonParams {
    /// A medium split button, at rest, enabled.
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            leading: None,
            size: SplitButtonSize::Medium,
            selected: false,
            disabled: false,
        }
    }

    pub fn leading(mut self, icon: IconSource) -> Self {
        self.leading = Some(icon);
        self
    }

    pub fn size(mut self, size: SplitButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The two halves' corners. The outer pair is always full; the inner pair is the shared
/// morph radius, and the trailing half alone becomes a pill while its menu is open.
pub fn half_corners(height: f32, inner: f32, selected: bool) -> (Corners, Corners) {
    let full = height * 0.5;
    let leading = Corners {
        tl: full,
        tr: inner,
        br: inner,
        bl: full,
        ..Corners::ZERO
    };
    let trailing = if selected {
        Corners::all(full)
    } else {
        Corners {
            tl: inner,
            tr: full,
            br: full,
            bl: inner,
            ..Corners::ZERO
        }
    };
    (leading, trailing)
}

/// The filled button's size for the same box, so the colours and type come from its table.
fn button_size(size: SplitButtonSize) -> ButtonSize {
    match size {
        SplitButtonSize::XSmall => ButtonSize::XSmall,
        SplitButtonSize::Small => ButtonSize::Small,
        SplitButtonSize::Medium => ButtonSize::Medium,
        SplitButtonSize::Large => ButtonSize::Large,
        SplitButtonSize::XLarge => ButtonSize::XLarge,
        SplitButtonSize::Base => ButtonSize::Medium,
    }
}

/// One half's paint, resolved for this frame.
struct HalfPaint {
    corners: Corners,
    fill: Color,
    layer: Option<Color>,
    press: Option<ui_core::PressInk>,
    ring: Option<(Color, f32)>,
}

/// The trailing arrow, sized by the table; every half takes the filled button's ink.
pub struct Md3SplitButton {
    id: Id,
    height: f32,
    gap: f32,
    leading: Decl,
    trailing: Decl,
    fired: Option<Id>,
}

impl Md3SplitButton {
    pub fn new_with(params: SplitButtonParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let SplitButtonParams {
            id,
            label,
            leading: leading_icon,
            size,
            selected,
            disabled: static_disabled,
        } = params;
        let leading_id = id.with("leading");
        let trailing_id = id.with("trailing");
        let leading_state = cx.state(leading_id);
        let trailing_state = cx.state(trailing_id);
        let disabled = static_disabled || leading_state.disabled || trailing_state.disabled;
        let hovered = leading_state.hovered || trailing_state.hovered;
        let pressed = leading_state.pressed || trailing_state.pressed;

        let spec = split_button_spec(
            SplitButtonFlags {
                selected,
                ..SplitButtonFlags::NONE
            },
            size,
        );
        let rest = spec.inner_corner.corner_size;
        // Hover and press carry the same widened corner in the export; one clock drives both.
        let wide = split_button_spec(
            SplitButtonFlags {
                hovered: true,
                ..SplitButtonFlags::NONE
            },
            size,
        )
        .inner_corner
        .corner_size;
        let height = spec.container.height;

        let mut node = cx.at(id);
        let morph = node.animate(
            id.with("corner"),
            if hovered || pressed { 1.0 } else { 0.0 },
            MORPH,
        );
        let (leading_hover, trailing_hover) = (
            node.animate(
                leading_id.with(param::HOVER),
                f32::from(leading_state.hovered),
                REACTION,
            ),
            node.animate(
                trailing_id.with(param::HOVER),
                f32::from(trailing_state.hovered),
                REACTION,
            ),
        );
        let (leading_focus, trailing_focus) = (
            node.animate(
                leading_id.with(param::FOCUS),
                f32::from(leading_state.focus_visible),
                REACTION,
            ),
            node.animate(
                trailing_id.with(param::FOCUS),
                f32::from(trailing_state.focus_visible),
                REACTION,
            ),
        );
        let (leading_ring, trailing_ring) = (
            node.animate(
                leading_id.with(param::THICKNESS),
                f32::from(leading_state.focus_visible),
                REACTION,
            ),
            node.animate(
                trailing_id.with(param::THICKNESS),
                f32::from(trailing_state.focus_visible),
                REACTION,
            ),
        );
        drop(node);

        let inner = rest + (wide - rest) * morph;
        let (leading_corners, trailing_corners) = half_corners(height, inner, selected);
        let button = button_size(size);
        let art = label_style(button);
        let colour = |hovered: bool, focused: bool, pressed: bool| {
            button_spec(
                ButtonFlags {
                    hovered,
                    focused,
                    pressed,
                    disabled,
                },
                ButtonStyle::Filled,
                button,
                ButtonToggle::Base,
            )
        };
        let base = colour(false, false, false);
        let fill = spec_ink(pack, base.container.color, base.container.opacity);
        let label_ink = spec_ink(pack, base.label_text.color, base.label_text.opacity);
        let icon_ink = spec_ink(pack, base.icon.color, base.icon.opacity);
        let layer_ink = role(pack, colour(true, false, false).state_layer.color);
        let press_ink_of = |pressed: bool| {
            (!disabled).then(|| {
                press_ink(
                    Color {
                        a: layer_ink.a * colour(false, false, pressed).state_layer.opacity,
                        ..layer_ink
                    },
                    InkRadius::Cover,
                )
            })
        };
        let ring_ink = spec_ink(pack, base.focus_indicator.color, 1.0);
        let layer_of = |hover: f32, focus: f32| {
            let strength = (hover * colour(true, false, false).state_layer.opacity)
                .max(focus * colour(false, true, false).state_layer.opacity);
            (strength > 0.0 && !disabled).then_some(Color {
                a: layer_ink.a * strength,
                ..layer_ink
            })
        };
        let ring_of = |thickness: f32| {
            (thickness > 0.0 && !disabled).then_some((
                Color {
                    a: ring_ink.a * thickness,
                    ..ring_ink
                },
                base.focus_indicator.thickness,
            ))
        };

        let leading_paint = HalfPaint {
            corners: leading_corners,
            fill,
            layer: layer_of(leading_hover, leading_focus),
            press: press_ink_of(leading_state.pressed),
            ring: ring_of(leading_ring),
        };
        let trailing_paint = HalfPaint {
            corners: trailing_corners,
            fill,
            layer: layer_of(trailing_hover, trailing_focus),
            press: press_ink_of(trailing_state.pressed),
            ring: ring_of(trailing_ring),
        };
        let label = Text::new(leading_id.with("label"), label, art.size, label_ink)
            .weight(art.weight)
            .resolve();
        // The split export carries no icon size or icon-label gap, so the mark takes both
        // from the filled button's table, the one its colours already come from.
        let leading_content = match leading_icon {
            Some(source) => Decl::container(leading_id.with("content"), NodeKind::Row)
                .gap(base.root.icon_label_space)
                .cross_align(0.0)
                .cross_pos(0.5)
                .child(
                    Icon {
                        id: leading_id.with("icon"),
                        source,
                        size: base.icon.size,
                        ink: icon_ink,
                    }
                    .resolve(),
                )
                .child(label),
            None => label,
        };
        let leading = half(
            leading_id,
            height,
            Insets {
                left: spec.leading_button.leading_space,
                right: spec.leading_button.trailing_space,
                top: 0.0,
                bottom: 0.0,
            },
            leading_paint,
            disabled,
            leading_content,
        );
        let trailing = half(
            trailing_id,
            height,
            Insets {
                left: spec.trailing_button.leading_space,
                right: spec.trailing_button.trailing_space,
                top: 0.0,
                bottom: 0.0,
            },
            trailing_paint,
            disabled,
            Icon {
                id: trailing_id.with("icon"),
                source: IconSource::glyph("arrow_drop_down", ui_widgets::IconSet::Outlined),
                size: spec.trailing_button_icon.size,
                ink: icon_ink,
            }
            .resolve(),
        );
        let fired = if disabled {
            None
        } else if leading_state.activation() {
            Some(leading_id)
        } else if trailing_state.activation() {
            Some(trailing_id)
        } else {
            None
        };
        Self {
            id,
            height,
            gap: spec.root.between_space,
            leading,
            trailing,
            fired,
        }
    }

    /// The halves' shared height, so a caller can align what sits beside them.
    pub fn height(&self) -> f32 {
        self.height
    }
}

/// One half: its box, its own pointer target and its content.
fn half(id: Id, height: f32, pad: Insets, paint: HalfPaint, disabled: bool, content: Decl) -> Decl {
    let mut shapes = Vec::with_capacity(3);
    shapes.push(Shape::Box {
        corners: paint.corners,
        fill: paint.fill,
        gradient: None,
        border: None,
        shadow: None,
        image: None,
    });
    if let Some(layer) = paint.layer {
        shapes.push(Shape::Box {
            corners: paint.corners,
            fill: layer,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        });
    }
    if let Some((ink, width)) = paint.ring {
        shapes.push(Shape::Box {
            corners: paint.corners,
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((ink, width)),
            shadow: None,
            image: None,
        });
    }
    Decl::container(id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::hug(0.0, 0.0),
            height: AxisSize::fixed(height),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(pad)
        // The half's content is centred in the space the paddings leave.
        .cross_align(0.0)
        .cross_pos(0.5)
        .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
        .cursor(if disabled {
            CursorShape::Arrow
        } else {
            CursorShape::PointingHand
        })
        .disabled(disabled)
        .press_ink(paint.press)
        .corners(paint.corners)
        .shapes(shapes)
        .child(content)
}

impl Widget for Md3SplitButton {
    fn handles_disabled(&self) -> bool {
        // The filled button's table carries the 12%/38% disabled pair, painted here.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        self.fired.map(|id| Change::Selected(Some(id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        // The root is a surface, never a target: every press belongs to a half.
        Decl::container(self.id, NodeKind::Row)
            .size(SizeClass::hug(0.0, 0.0))
            .gap(self.gap)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::PASS_THROUGH)
            .child(self.leading.clone())
            .child(self.trailing.clone())
    }
}
