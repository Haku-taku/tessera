//! MD3 icon button: a mark in a container whose shape morphs while pressed or checked.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{OverlayAlign, SizeClass};
use ui_core::{
    AnimSpec, Change, CursorShape, Decl, Easing, Id, InkRadius, NodeKind, PressInk, Sense,
};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, Icon, IconSource, Widget, WidgetState};

use crate::motion::REACTION;
use crate::paint::spec_ink;
use crate::spec::{IconButtonFlags, IconButtonSelection, icon_button_spec};
// The three axes are the control's own vocabulary: a caller names a style, a size and a width.
pub use crate::spec::{IconButtonSize, IconButtonStyle, IconButtonWidth};

/// The shape scale's `corner.small`: a pressed icon button's rounded square.
pub const ICON_BUTTON_PRESSED_RADIUS: f32 = 8.0;
/// `corner.medium`: the square a toggle rests on, checked or not.
pub const ICON_BUTTON_TOGGLE_RADIUS: f32 = 12.0;
/// The corner morph's clock: Compose's shape animation, which forbids a bounce.
const MORPH: AnimSpec = AnimSpec::new(0.1, Easing::EaseOut);

/// Everything an icon button is built with.
#[derive(Clone, Copy, Debug)]
pub struct IconButtonParams {
    pub id: Id,
    pub icon: IconSource,
    /// Standard, filled, tonal or outlined; `Base` paints the export's filled base.
    pub style: IconButtonStyle,
    pub size: IconButtonSize,
    /// The padding either side of the mark; `Base` would leave the box mark-wide.
    pub width: IconButtonWidth,
    /// `None` for a plain button, `Some` for a toggle in that state.
    pub selected: Option<bool>,
    pub disabled: bool,
}

impl IconButtonParams {
    /// A standard, small, default-width button with one mark.
    pub fn new(id: Id, icon: IconSource) -> Self {
        Self {
            id,
            icon,
            style: IconButtonStyle::Standard,
            size: IconButtonSize::Small,
            width: IconButtonWidth::Default,
            selected: None,
            disabled: false,
        }
    }

    pub fn style(mut self, style: IconButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn size(mut self, size: IconButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn width(mut self, width: IconButtonWidth) -> Self {
        self.width = width;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// One resolved frame of the button: its box, its inks and its one activation.
pub struct Md3IconButton {
    body: Decl,
    activated: bool,
}

/// The style as the token table's selection axis reads it.
fn selection_of(selected: Option<bool>) -> IconButtonSelection {
    match selected {
        None => IconButtonSelection::Base,
        Some(false) => IconButtonSelection::Unselected,
        Some(true) => IconButtonSelection::Selected,
    }
}

/// The corner the morph travels to: pressed wins over a toggle's square, which wins over
/// full round.
///
/// A toggle keeps the checked square while it is unchecked too. Its hover layer, the fill
/// it gains when checked and the box it rests on are then one shape; only the press narrows
/// it. A plain button has no checked state to agree with, so it stays round.
pub fn corner_target(pressed: bool, selected: Option<bool>, full: f32) -> f32 {
    if pressed {
        ICON_BUTTON_PRESSED_RADIUS
    } else if selected.is_some() {
        ICON_BUTTON_TOGGLE_RADIUS
    } else {
        full
    }
}

/// The three axes one query is made with, so a helper takes one argument per idea.
#[derive(Clone, Copy)]
struct Axes {
    style: IconButtonStyle,
    selection: IconButtonSelection,
    size: IconButtonSize,
    width: IconButtonWidth,
}

/// The one interaction state a frame paints as its layer, in Compose's order.
fn layer_ink(pack: &Style, axes: Axes, hovered: f32, focused: bool, pressed: f32) -> Option<Color> {
    let Axes {
        style,
        selection,
        size,
        width,
    } = axes;
    let spec = |hovered, focused, pressed| {
        icon_button_spec(
            IconButtonFlags {
                hovered,
                focused,
                pressed,
                ..IconButtonFlags::NONE
            },
            style,
            selection,
            size,
            width,
        )
    };
    // The export leaves the opacity out under `selected`/`unselected`: only the colour moves
    // with the toggle, so the strength comes from the style's own interaction state.
    let strength = |hovered, focused, pressed| {
        icon_button_spec(
            IconButtonFlags {
                hovered,
                focused,
                pressed,
                ..IconButtonFlags::NONE
            },
            style,
            IconButtonSelection::Base,
            size,
            width,
        )
        .state_layer
        .opacity
    };
    let (colour, opacity) = if pressed > 0.0 {
        (
            spec(false, false, true).state_layer.color,
            strength(false, false, true),
        )
    } else if hovered > 0.0 {
        (
            spec(true, false, false).state_layer.color,
            strength(true, false, false),
        )
    } else if focused {
        (
            spec(false, true, false).state_layer.color,
            strength(false, true, false),
        )
    } else {
        return None;
    };
    let fade = if pressed > 0.0 { pressed } else { hovered };
    (fade > 0.0).then(|| spec_ink(pack, colour, opacity * fade))
}

/// The box: a fill for the styles that have one, a stroke for outlined, the icon on top.
fn resolved(
    pack: &Style,
    params: &IconButtonParams,
    state: &WidgetState,
    hover: f32,
    press: f32,
    radius: f32,
) -> Decl {
    let selection = selection_of(params.selected);
    let disabled = params.disabled || state.disabled;
    let rest = icon_button_spec(
        IconButtonFlags {
            disabled,
            ..IconButtonFlags::NONE
        },
        params.style,
        selection,
        params.size,
        params.width,
    );
    let look = rest;
    let icon_size = look.icon.size;
    let height = look.container.height;
    let width = icon_size + look.root.leading_space + look.root.trailing_space;
    let ink = spec_ink(pack, look.icon.color, look.icon.opacity);
    let fill = match params.style {
        // The export's base is the filled button; a standard or outlined container has no
        // fill of its own, so the colour it would inherit is never painted.
        IconButtonStyle::Filled | IconButtonStyle::Tonal => {
            Some(spec_ink(pack, look.container.color, look.container.opacity))
        }
        _ => None,
    };
    let border = (params.style == IconButtonStyle::Outlined).then(|| {
        // The export carries no disabled outline opacity; dim the stroke by the same 0.38
        // the disabled mark uses, or a ruled-out button reads as an enabled one.
        let alpha = if disabled { 0.38 } else { 1.0 };
        (
            spec_ink(pack, look.outline.color, alpha),
            look.outline.width,
        )
    });
    let layer = if disabled {
        None
    } else {
        layer_ink(
            pack,
            Axes {
                style: params.style,
                selection,
                size: params.size,
                width: params.width,
            },
            hover,
            state.focus_visible,
            press,
        )
    };
    // The container, its state layer and its ripple are one shape: this box with this
    // frame's corner. Cut to a rect of its own, the hover layer drifted from the fill.
    let corners = Corners::all(radius);
    let boxed = |fill: Color, border: Option<(Color, f32)>| Shape::Box {
        corners,
        fill,
        gradient: None,
        border,
        shadow: None,
        image: None,
    };
    let mut shapes = Vec::with_capacity(2);
    if fill.is_some() || border.is_some() {
        shapes.push(boxed(fill.unwrap_or(Color::TRANSPARENT), border));
    }
    if let Some(ink) = layer {
        shapes.push(boxed(ink, None));
    }
    if state.focus_visible {
        let ring = icon_button_spec(
            IconButtonFlags {
                disabled,
                ..IconButtonFlags::NONE
            },
            params.style,
            selection,
            params.size,
            params.width,
        );
        let offset = ring.focus_indicator_outline.offset;
        shapes.push(Shape::Rect {
            rect: Rect::new(
                -offset,
                -offset,
                width + offset * 2.0,
                height + offset * 2.0,
            ),
            corners: Corners::all(radius + offset),
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((
                spec_ink(pack, ring.focus_indicator.color, 1.0),
                ring.focus_indicator.thickness,
            )),
        });
    }
    let press_colour = icon_button_spec(
        IconButtonFlags {
            pressed: true,
            ..IconButtonFlags::NONE
        },
        params.style,
        selection,
        params.size,
        params.width,
    );
    let press_ink = spec_ink(
        pack,
        press_colour.state_layer.color,
        press_colour.state_layer.opacity,
    );
    Decl::container(params.id, NodeKind::Stack)
        .size(SizeClass::fixed(width, height))
        .stack_align(OverlayAlign::Center)
        .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
        .focus_stop(true)
        .cursor(if disabled {
            CursorShape::Arrow
        } else {
            CursorShape::PointingHand
        })
        .disabled(disabled)
        // The ripple clips to the node's curve, this frame's corner: a target captured at
        // the press would keep the splash in the shape the button no longer has.
        .press_ink((!disabled).then(|| PressInk::new(press_ink, InkRadius::Cover)))
        .corners(corners)
        .shapes(shapes)
        .child(
            Icon {
                id: params.id.with("icon"),
                source: params.icon,
                size: icon_size,
                ink,
            }
            .resolve(),
        )
}

impl Md3IconButton {
    /// Resolve one frame: the mark, the morphing box, and whether the press fired.
    pub fn new_with(params: IconButtonParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let state = cx.state(params.id);
        let disabled = params.disabled || state.disabled;
        let mut node = cx.at(params.id);
        let hover = node.animate(
            params.id.with(param::HOVER),
            if state.hovered && !disabled { 1.0 } else { 0.0 },
            REACTION,
        );
        let press = node.animate(
            params.id.with(param::PRESS),
            if state.pressed && !disabled { 1.0 } else { 0.0 },
            REACTION,
        );
        // The corner's target: full round at rest, the pressed or checked square otherwise.
        let dims = icon_button_spec(
            IconButtonFlags::NONE,
            params.style,
            selection_of(params.selected),
            params.size,
            params.width,
        );
        let full = (dims.icon.size + dims.root.leading_space + dims.root.trailing_space)
            .min(dims.container.height)
            * 0.5;
        let target = corner_target(state.pressed && !disabled, params.selected, full);
        let radius = node.animate(params.id.with("radius"), target, MORPH);
        let activated = state.activation() && !disabled;
        let body = resolved(pack, &params, &state, hover, press, radius.min(full));
        Self { body, activated }
    }
}

impl Widget for Md3IconButton {
    fn handles_disabled(&self) -> bool {
        // The tokens dim a disabled button themselves; the adapter must not fade it twice.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        self.activated.then_some(Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.body.clone()
    }
}

/// The button's own size, for a caller that lays it out by hand: width, height, mark.
pub fn icon_button_size(
    style: IconButtonStyle,
    selection: IconButtonSelection,
    size: IconButtonSize,
    width: IconButtonWidth,
) -> (f32, f32, f32) {
    let spec = icon_button_spec(IconButtonFlags::NONE, style, selection, size, width);
    let box_width = spec.icon.size + spec.root.leading_space + spec.root.trailing_space;
    (box_width, spec.container.height, spec.icon.size)
}
