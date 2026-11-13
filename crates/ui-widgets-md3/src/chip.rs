//! MD3 chips: four kinds over one 32dp pill — assist, suggestion, filter and input.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{CursorShape, Decl, Id, InkRadius, NodeKind, PressInk, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{role, spec_ink};
use crate::spec::{
    AssistChipFlags, AssistChipVariant, FilterChipFlags, FilterChipVariant, InputChipFlags,
    SuggestionChipFlags, SuggestionChipVariant, assist_chip_spec, filter_chip_spec,
    input_chip_spec, suggestion_chip_spec,
};
use crate::typescale::TypeStyle;

use crate::motion::REACTION;

/// `md.sys.shape.corner.full` is not used here; chips take the 8dp `rounded_corners`.
pub const CHIP_RADIUS: f32 = 8.0;
/// Leading pad without an icon (MD3 chip spec: 16dp at both ends).
pub const CHIP_PAD: f32 = 16.0;
/// The pad shrinks to 8dp on a side that carries an 18dp mark.
pub const CHIP_PAD_ICON: f32 = 8.0;
/// `icon-label-space`: the gap between a mark and the label.
pub const CHIP_GAP: f32 = 8.0;

/// Which chip this is; the export has a token table per kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipKind {
    Assist,
    Suggestion,
    Filter,
    Input,
}

/// Flat chips are outlined, elevated chips carry a shadow; input chips are always flat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipVariant {
    Flat,
    Elevated,
}

/// The values one kind resolves for one state, normalised across the four exports.
struct Parts {
    height: f32,
    fill: &'static str,
    fill_op: f32,
    elevation: f32,
    outline: &'static str,
    outline_op: f32,
    outline_w: f32,
    label: &'static str,
    label_op: f32,
    lead: &'static str,
    lead_op: f32,
    trail: &'static str,
    trail_op: f32,
    layer: &'static str,
    layer_op: f32,
    ring: &'static str,
    ring_w: f32,
    label_size: f32,
    label_weight: f32,
    icon_size: f32,
}

/// What the widget needs to know about one chip at build time.
#[derive(Clone, Copy)]
struct Knobs {
    variant: ChipVariant,
    selected: bool,
    lead: bool,
    trail: bool,
    disabled: bool,
}

fn variant_of(
    variant: ChipVariant,
) -> (AssistChipVariant, SuggestionChipVariant, FilterChipVariant) {
    match variant {
        ChipVariant::Flat => (
            AssistChipVariant::Flat,
            SuggestionChipVariant::Flat,
            FilterChipVariant::Flat,
        ),
        ChipVariant::Elevated => (
            AssistChipVariant::Elevated,
            SuggestionChipVariant::Elevated,
            FilterChipVariant::Elevated,
        ),
    }
}

/// Resolve one kind for one interaction state into the shared shape.
fn parts(
    kind: ChipKind,
    k: Knobs,
    hovered: bool,
    focused: bool,
    pressed: bool,
    dragged: bool,
) -> Parts {
    let (a, s, f) = variant_of(k.variant);
    match kind {
        ChipKind::Assist => {
            let v = assist_chip_spec(
                AssistChipFlags {
                    with_icon: k.lead,
                    hovered,
                    focused,
                    pressed,
                    dragging: dragged,
                    disabled: k.disabled,
                },
                a,
            );
            Parts {
                height: v.container.height,
                fill: v.container.color,
                fill_op: v.container.opacity,
                elevation: v.container.elevation,
                outline: v.outline.color,
                outline_op: v.outline.opacity,
                outline_w: v.outline.width,
                label: v.label_text.color,
                label_op: v.label_text.opacity,
                lead: v.icon.color,
                lead_op: v.icon.opacity,
                trail: "",
                trail_op: 1.0,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                ring: v.focus_indicator.color,
                ring_w: v.focus_indicator.thickness,
                label_size: v.label_text.size,
                label_weight: v.label_text.weight,
                icon_size: v.icon.size,
            }
        }
        ChipKind::Suggestion => {
            let v = suggestion_chip_spec(
                SuggestionChipFlags {
                    with_leading_icon: k.lead,
                    hovered,
                    focused,
                    pressed,
                    dragging: dragged,
                    disabled: k.disabled,
                },
                s,
            );
            Parts {
                height: v.container.height,
                fill: v.container.color,
                fill_op: v.container.opacity,
                elevation: v.container.elevation,
                outline: v.outline.color,
                outline_op: v.outline.opacity,
                outline_w: v.outline.width,
                label: v.label_text.color,
                label_op: v.label_text.opacity,
                lead: v.leading_icon.color,
                lead_op: v.leading_icon.opacity,
                trail: "",
                trail_op: 1.0,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                ring: v.focus_indicator.color,
                ring_w: v.focus_indicator.thickness,
                label_size: v.label_text.size,
                label_weight: v.label_text.weight,
                icon_size: v.leading_icon.size,
            }
        }
        ChipKind::Filter => {
            let v = filter_chip_spec(
                FilterChipFlags {
                    selected: k.selected,
                    with_icon: k.lead,
                    with_leading_icon: k.lead,
                    with_trailing_icon: k.trail,
                    hovered,
                    focused,
                    pressed,
                    dragging: dragged,
                    disabled: k.disabled,
                },
                f,
            );
            Parts {
                height: v.container.height,
                fill: v.container.color,
                fill_op: v.container.opacity,
                elevation: v.container.elevation,
                outline: v.outline.color,
                outline_op: v.outline.opacity,
                outline_w: v.outline.width,
                label: v.label_text.color,
                label_op: v.label_text.opacity,
                lead: v.leading_icon.color,
                lead_op: v.leading_icon.opacity,
                trail: v.trailing_icon.color,
                trail_op: v.trailing_icon.opacity,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                ring: v.focus_indicator.color,
                ring_w: v.focus_indicator.thickness,
                label_size: v.label_text.size,
                label_weight: v.label_text.weight,
                icon_size: v.icon.size,
            }
        }
        ChipKind::Input => {
            let v = input_chip_spec(InputChipFlags {
                selected: k.selected,
                with_leading_icon: k.lead,
                with_trailing_icon: k.trail,
                hovered,
                focused,
                pressed,
                dragging: dragged,
                disabled: k.disabled,
                ..InputChipFlags::NONE
            });
            Parts {
                height: v.container.height,
                fill: v.container.color,
                fill_op: v.container.opacity,
                elevation: v.container.elevation,
                outline: v.outline.color,
                outline_op: v.outline.opacity,
                outline_w: v.outline.width,
                label: v.label_text.color,
                label_op: v.label_text.opacity,
                lead: v.leading_icon.color,
                lead_op: v.leading_icon.opacity,
                trail: v.trailing_icon.color,
                trail_op: v.trailing_icon.opacity,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                ring: v.focus_indicator.color,
                ring_w: v.focus_indicator.thickness,
                label_size: v.label_text.size,
                label_weight: v.label_text.weight,
                icon_size: v.leading_icon.size,
            }
        }
    }
}

/// A chip; `selected` is the app's for filter and input chips.
pub struct Md3Chip {
    id: Id,
    label: String,
    kind: ChipKind,
    selected: bool,
    lead: Option<(IconSource, Color)>,
    trail: Option<(IconSource, Color)>,
    height: f32,
    icon_size: f32,
    pad_l: f32,
    pad_r: f32,
    gap: f32,
    fill: Option<Color>,
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

/// Everything about one chip that is fixed when it is built. `lead` and `trail` are the same
/// type, so naming them here is what keeps them from being swapped.
pub struct ChipParams {
    pub id: Id,
    pub label: String,
    pub kind: ChipKind,
    pub variant: ChipVariant,
    pub selected: bool,
    /// Draws the mark before the label in the ink it is given.
    pub lead: Option<IconSource>,
    /// Draws the mark after the label in the ink it is given.
    pub trail: Option<IconSource>,
}

impl ChipParams {
    /// An unselected assist chip, flat, with no marks.
    pub fn new(id: Id, label: impl Into<String>, kind: ChipKind) -> Self {
        Self {
            id,
            label: label.into(),
            kind,
            variant: ChipVariant::Flat,
            selected: false,
            lead: None,
            trail: None,
        }
    }

    pub fn variant(mut self, variant: ChipVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn lead(mut self, lead: IconSource) -> Self {
        self.lead = Some(lead);
        self
    }

    pub fn trail(mut self, trail: IconSource) -> Self {
        self.trail = Some(trail);
        self
    }
}

impl Md3Chip {
    /// Build one chip. Every axis here is chosen when it is built, not while it runs.
    pub(crate) fn new_with(
        params: Md3Options,
        ChipParams {
            id,
            label,
            kind,
            variant,
            selected,
            lead,
            trail,
        }: ChipParams,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let mut node = cx.at(id);
        let state = node.state();
        let disabled = params.disabled || state.disabled;
        let knobs = Knobs {
            variant,
            selected,
            lead: lead.is_some(),
            trail: trail.is_some(),
            disabled,
        };
        let rest = parts(
            kind,
            knobs,
            state.hovered,
            state.focus_visible,
            state.pressed,
            false,
        );
        let hover = parts(kind, knobs, true, false, false, false);
        let focus = parts(kind, knobs, false, true, false, false);
        let press = parts(kind, knobs, false, false, true, false);
        let hover_t = node.animate(id.with(param::HOVER), f32::from(state.hovered), REACTION);
        let focus_t = node.animate(
            id.with(param::FOCUS),
            f32::from(state.focus_visible),
            REACTION,
        );
        let layer_ink = role(pack, hover.layer);
        let strength = (hover_t * hover.layer_op).max(focus_t * focus.layer_op);
        let layer = (strength > 0.0 && !disabled).then_some(Color {
            a: layer_ink.a * strength,
            ..layer_ink
        });
        // A press is a splat from the pointer, not another rise of the flat layer.
        let ink = (!disabled).then(|| {
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
        let ring = (ring_t > 0.0 && !disabled).then_some((
            Color {
                a: ring_ink.a * ring_t,
                ..ring_ink
            },
            rest.ring_w,
        ));
        let fill = (!rest.fill.is_empty()).then(|| spec_ink(pack, rest.fill, rest.fill_op));
        let outline = (rest.outline_w > 0.0 && !rest.outline.is_empty()).then(|| {
            (
                spec_ink(pack, rest.outline, rest.outline_op),
                rest.outline_w,
            )
        });
        let shadow = (rest.elevation > 0.0 && !disabled)
            .then(|| ShadowSpec::black(rest.elevation * 2.0, 0.18, rest.elevation * 0.5));
        let label_ink = spec_ink(pack, rest.label, rest.label_op);
        let lead_ink = spec_ink(pack, rest.lead, rest.lead_op);
        let trail_ink = spec_ink(pack, rest.trail, rest.trail_op);
        Self {
            id,
            label,
            kind,
            selected,
            lead: lead.map(|f| (f, lead_ink)),
            trail: trail.map(|f| (f, trail_ink)),
            height: rest.height,
            icon_size: rest.icon_size,
            pad_l: if lead.is_some() {
                CHIP_PAD_ICON
            } else {
                CHIP_PAD
            },
            pad_r: if trail.is_some() {
                CHIP_PAD_ICON
            } else {
                CHIP_PAD
            },
            gap: CHIP_GAP,
            fill,
            outline,
            shadow,
            layer,
            press: ink,
            label_ink,
            label_style: TypeStyle {
                size: rest.label_size,
                weight: Weight(rest.label_weight as u16),
            },
            ring,
            disabled,
        }
    }

    /// The pill's own height, which the export sets at 32dp for every kind.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// The mark's size, from the export's `icon.size` / `leading-icon.size`.
    pub fn icon_size(&self) -> f32 {
        self.icon_size
    }

    /// A node-sized pill.
    fn pill(&self, fill: Color, border: Option<(Color, f32)>) -> Shape {
        Shape::Box {
            corners: Corners::all(CHIP_RADIUS),
            fill,
            gradient: None,
            border,
            shadow: None,
            image: None,
        }
    }
}

impl Widget for Md3Chip {
    fn handles_disabled(&self) -> bool {
        // The export carries its own disabled opacities (38% content, 12% container).
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        if self.disabled || !state.activation() {
            return None;
        }
        match self.kind {
            // Assist and suggestion chips only fire an action.
            ChipKind::Assist | ChipKind::Suggestion => Some(ui_core::Change::Activated),
            // Filter and input chips hold a state the app owns.
            ChipKind::Filter | ChipKind::Input => Some(ui_core::Change::Toggled {
                on: Some(!self.selected),
            }),
        }
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut shapes = Vec::with_capacity(3);
        if let Some(fill) = self.fill {
            let mut b = self.pill(fill, self.outline);
            if let Shape::Box { shadow, .. } = &mut b {
                *shadow = self.shadow;
            }
            shapes.push(b);
        } else if let Some(border) = self.outline {
            shapes.push(self.pill(Color::TRANSPARENT, Some(border)));
        }
        if let Some(layer) = self.layer {
            shapes.push(self.pill(layer, None));
        }
        // The export insets the ring by `focus.indicator.outline.offset`; it is drawn on the
        // chip's own edge instead, since the decl cannot know the final rect.
        if let Some((ink, width)) = self.ring {
            shapes.push(self.pill(Color::TRANSPARENT, Some((ink, width))));
        }
        let mut row = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: self.pad_l,
                right: self.pad_r,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(self.gap)
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
            // Stated, not inferred: the chip is one radius whatever the variant paints, and a
            // variant that paints no container would otherwise lose the curve mid-fade.
            .corners(Corners::all(CHIP_RADIUS))
            .shapes(shapes);
        if let Some((source, ink)) = self.lead {
            row = row.child(source.resolve_with_size(self.id.with("lead"), ink, self.icon_size));
        }
        row = row.child(
            Text::new(
                self.id.with("label"),
                self.label.clone(),
                self.label_style.size,
                self.label_ink,
            )
            .weight(self.label_style.weight)
            .resolve(),
        );
        if let Some((source, ink)) = self.trail {
            row = row.child(source.resolve_with_size(self.id.with("trail"), ink, self.icon_size));
        }
        row
    }
}
