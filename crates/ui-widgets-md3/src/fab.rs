//! MD3 floating action button: one mark on a square that lifts as it is touched.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{OverlayAlign, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{AnimSpec, Change, CursorShape, Decl, Easing, Id, InkRadius, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, Icon, IconSource, Widget, WidgetState};

use crate::layer::Background;
use crate::motion::REACTION;
use crate::paint::spec_ink;
use crate::spec::{FabFlags, FabLowering, fab_spec};
// The three axes are the control's own vocabulary: a caller names a colour, a size, a lift.
pub use crate::spec::{FabColor, FabSize};

/// The shape scale's `corner.medium`: the small FAB.
pub const FAB_SMALL_RADIUS: f32 = 12.0;
/// `corner.large`: the 56 dp FAB.
pub const FAB_RADIUS: f32 = 16.0;
/// `corner.large-increased`: the medium FAB. Compose names `ShapeDefaults.LargeIncreased`.
pub const FAB_MEDIUM_RADIUS: f32 = 20.0;
/// `corner.extra-large`: the large FAB.
pub const FAB_LARGE_RADIUS: f32 = 28.0;
/// The ring the export carries only under `surface`; every style uses it.
const RING_INK: &str = "secondary";
const RING_WIDTH: f32 = 3.0;
const RING_OFFSET: f32 = 2.0;
/// A disabled FAB's container and content, the pair MD3 uses for a ruled-out box.
const DISABLED_CONTAINER: f32 = 0.12;
const DISABLED_CONTENT: f32 = 0.38;
/// The hover and press clocks: the same reaction every other MD3 control uses.
const HOVER: AnimSpec = AnimSpec::new(0.1, Easing::EaseOut);

/// Everything a FAB is built with.
#[derive(Clone, Copy, Debug)]
pub struct FabParams {
    pub id: Id,
    pub icon: IconSource,
    pub size: FabSize,
    /// `Base` has the box but no colour; the default FAB is `PrimaryContainer`.
    pub color: FabColor,
    pub lowered: bool,
    pub disabled: bool,
}

impl FabParams {
    /// The default FAB: 56 dp, `primaryContainer`, not lowered.
    pub fn new(id: Id, icon: IconSource) -> Self {
        Self {
            id,
            icon,
            size: FabSize::Base,
            color: FabColor::PrimaryContainer,
            lowered: false,
            disabled: false,
        }
    }

    pub fn size(mut self, size: FabSize) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: FabColor) -> Self {
        self.color = color;
        self
    }

    pub fn lowered(mut self, lowered: bool) -> Self {
        self.lowered = lowered;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The corner a size rests on: the shape scale's `medium`, `large`, `large-increased`,
/// `extra-large` for the four boxes.
pub const fn fab_corner(size: FabSize) -> f32 {
    match size {
        FabSize::Small => FAB_SMALL_RADIUS,
        FabSize::Medium => FAB_MEDIUM_RADIUS,
        FabSize::Large => FAB_LARGE_RADIUS,
        FabSize::Base => FAB_RADIUS,
    }
}

/// The lowering as the table's axis reads it.
fn lowering_of(lowered: bool) -> FabLowering {
    if lowered {
        FabLowering::Lowered
    } else {
        FabLowering::Base
    }
}

/// One resolved FAB: its box, its ink, its layer and its one activation.
pub struct Md3Fab {
    body: Decl,
    activated: bool,
}

/// The layer a frame paints, in Compose's order: pressed, hovered, focused.
fn layer_ink(
    pack: &Style,
    size: FabSize,
    color: FabColor,
    lowering: FabLowering,
    hovered: f32,
    focused: bool,
    pressed: f32,
) -> Option<Color> {
    let spec = |hovered, focused, pressed| {
        fab_spec(
            FabFlags {
                hovered,
                focused,
                pressed,
            },
            color,
            size,
            lowering,
        )
    };
    let (colour, opacity, fade) = if pressed > 0.0 {
        let spec = spec(false, false, true);
        (spec.state_layer.color, spec.state_layer.opacity, pressed)
    } else if hovered > 0.0 {
        let spec = spec(true, false, false);
        (spec.state_layer.color, spec.state_layer.opacity, hovered)
    } else if focused {
        let spec = spec(false, true, false);
        (spec.state_layer.color, spec.state_layer.opacity, 1.0)
    } else {
        return None;
    };
    if colour.is_empty() || fade <= 0.0 {
        return None;
    }
    Some(spec_ink(pack, colour, opacity * fade))
}

fn resolved(pack: &Style, params: &FabParams, state: &WidgetState, hover: f32, press: f32) -> Decl {
    let disabled = params.disabled || state.disabled;
    let lowering = lowering_of(params.lowered);
    let look = fab_spec(FabFlags::NONE, params.color, params.size, lowering);
    let width = look.container.width;
    let height = look.container.height;
    let icon_size = look.icon.size;
    let corner = fab_corner(params.size);
    let fill = if disabled {
        spec_ink(pack, "onSurface", DISABLED_CONTAINER)
    } else {
        spec_ink(pack, look.container.color, 1.0)
    };
    let ink = spec_ink(
        pack,
        look.icon.color,
        if disabled { DISABLED_CONTENT } else { 1.0 },
    );
    let shadow = (look.container.elevation > 0.0 && !disabled)
        .then(|| ShadowSpec::elevation(look.container.elevation as u8));
    let layer = if disabled {
        None
    } else {
        layer_ink(
            pack,
            params.size,
            params.color,
            lowering,
            hover,
            state.focus_visible,
            press,
        )
    };
    let mut shapes = Vec::with_capacity(3);
    shapes.push(Shape::Box {
        corners: Corners::all(corner),
        fill,
        gradient: None,
        border: None,
        shadow,
        image: None,
    });
    if let Some(ink) = layer {
        shapes.push(Shape::Rect {
            rect: Rect::new(0.0, 0.0, width, height),
            corners: Corners::all(corner),
            fill: ink,
            gradient: None,
            border: None,
        });
    }
    if state.focus_visible && !disabled {
        shapes.push(Shape::Rect {
            rect: Rect::new(
                -RING_OFFSET,
                -RING_OFFSET,
                width + RING_OFFSET * 2.0,
                height + RING_OFFSET * 2.0,
            ),
            corners: Corners::all(corner + RING_OFFSET),
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((spec_ink(pack, RING_INK, 1.0), RING_WIDTH)),
        });
    }
    let pressed = fab_spec(
        FabFlags {
            pressed: true,
            ..FabFlags::NONE
        },
        params.color,
        params.size,
        lowering,
    );
    let press_ink = spec_ink(pack, pressed.state_layer.color, pressed.state_layer.opacity);
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
        .press_ink((!disabled).then(|| {
            Background::new(Rect::new(0.0, 0.0, width, height), Corners::all(corner))
                .press(press_ink, InkRadius::Cover)
        }))
        .corners(Corners::all(corner))
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

impl Md3Fab {
    /// Resolve one frame: the mark, the box and whether the press fired.
    pub fn new_with(params: FabParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let state = cx.state(params.id);
        let disabled = params.disabled || state.disabled;
        let mut node = cx.at(params.id);
        let hover = node.animate(
            params.id.with(param::HOVER),
            if state.hovered && !disabled { 1.0 } else { 0.0 },
            HOVER,
        );
        let press = node.animate(
            params.id.with(param::PRESS),
            if state.pressed && !disabled { 1.0 } else { 0.0 },
            REACTION,
        );
        let activated = state.activation() && !disabled;
        let body = resolved(pack, &params, &state, hover, press);
        Self { body, activated }
    }
}

impl Widget for Md3Fab {
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
