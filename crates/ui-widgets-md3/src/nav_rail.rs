//! MD3 navigation rail: the navigation bar's destinations stacked in a fixed 80dp column, with
//! a 56×32 capsule that becomes a 56×56 circle when no labels are shown. The Expressive variant
//! is a 96dp / 220dp pair whose width travels between the two as the destinations' pill opens.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Point, Rect};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::{AnimSpec, CursorShape, Decl, Id, InkRadius, NodeKind, PressInk, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::{Background, press_ink_in};
use crate::nav_bar::{Md3NavItem, boxed};
use crate::paint::faded;
use crate::paint::{between, lerp, role, spec_ink};
use crate::spec::{NavigationRailFlags, navigation_rail_spec};

use crate::motion::{DESTINATION_SELECT, REACTION};
/// `_verticalIconLabelSpacingM3`.
const LABEL_GAP: f32 = 4.0;
/// `_verticalDestinationSpacingM3`: the room under a destination, label or not.
const BOTTOM_GAP: f32 = 12.0;
/// Baseline M3's "top margin of navigation rail content": 8dp around the destinations.
const RAIL_MARGIN: f32 = 8.0;

// The Expressive rail is hand-written: the export splits it into four token sets (`collapsed`,
// `expanded`, `item.vertical`, `item.horizontal`) that `gen_spec.py` would flatten into one
// undifferentiated soup of states. Every value below names the token it comes from.

/// `…collapsed.container.width` and `…expanded.container.width.minimum`.
const EXP_COLLAPSED: f32 = 96.0;
const EXP_EXPANDED: f32 = 220.0;
/// `collapsed.top-space` / `expanded.top-space`: 44dp at both ends.
const EXP_TOP: f32 = 44.0;
/// `item.container.height`; `collapsed.item.vertical-space` separates destinations.
const EXP_ITEM_H: f32 = 64.0;
const EXP_ITEM_GAP: f32 = 4.0;
/// `item.vertical.active-indicator.height` and `item.horizontal.active-indicator.height`.
const EXP_PILL_H: (f32, f32) = (32.0, 56.0);
/// `item.vertical.active-indicator.width`: the collapsed pill, which spans the rail expanded.
const EXP_PILL_W: f32 = 56.0;
/// `item.horizontal.full-width.leading-space`, also the pill's inner
/// `active-indicator.leading-space`: the same 16dp centres the icon while collapsed.
const EXP_INSET: f32 = 16.0;
/// `item.horizontal.active-indicator.icon-label-space`.
const EXP_GAP: f32 = 8.0;
/// The label's box when open: 220 − 2·16 (pill inset) − 2·16 (pill padding) − 24 − 8.
const EXP_LABEL_W: f32 = 124.0;
/// `md.sys.typescale.label-medium.line-height` and `label-large.line-height`.
const EXP_LABEL_LH: (f32, f32) = (16.0, 20.0);
/// M3's paired curve for a transition that begins and ends on screen: Standard, 300 ms.
const EXPAND: AnimSpec = crate::motion::STANDARD_TRANSITION;

/// Which destinations carry their label, mirroring `NavigationRailLabelType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RailLabelType {
    /// No destination carries a label; the indicator becomes the 56×56 circle.
    #[default]
    None,
    /// Only the selected destination carries its label.
    Selected,
    /// Every destination carries its label.
    All,
}

/// Which rail family to build: the baseline column, or the Expressive pair whose width travels
/// between collapsed and expanded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RailVariant {
    #[default]
    Classic,
    /// `expanded` is the target the width and the pill open toward, given by the app.
    Expressive { expanded: bool },
}

/// Everything a navigation rail needs besides the design system's own tokens.
pub struct RailParams<'a> {
    pub id: Id,
    /// The same destinations the bar takes; a rail is their vertical host.
    pub items: &'a [Md3NavItem<'a>],
    /// The selected destination's index; the app owns it.
    pub selected: usize,
    /// Which family to build; Expressive ignores it, its labels are always inside the pill.
    pub label_type: RailLabelType,
    pub variant: RailVariant,
    pub disabled: bool,
}

/// One destination's resolved paint data.
struct Item {
    id: Id,
    label: String,
    icon: IconSource,
    /// Whether this label type can show a label at all; `None` builds no label node.
    labelled: bool,
    /// The label's animated presence, `0..=1`: it fades and gives its room back together.
    reveal: f32,
    /// The destination's own box height, which also gives the hit-test bands.
    height: f32,
    icon_ink: Color,
    label_ink: Color,
    label_weight: Weight,
    layer: Option<Color>,
    press: Option<PressInk>,
    ring: f32,
    indicator: f32,
}

/// A navigation rail; the app owns which destination is selected.
pub struct Md3NavRail {
    id: Id,
    items: Vec<Item>,
    selected: usize,
    variant: RailVariant,
    /// 0 collapsed, 1 expanded; the Expressive geometry reads it, `Classic` keeps it at 0.
    expand: f32,
    width: f32,
    indicator_w: f32,
    indicator_h: f32,
    indicator_ink: Color,
    icon_size: f32,
    label_size: f32,
    label_h: f32,
    container: Color,
    disabled: bool,
}

impl Md3NavRail {
    /// Same control with explicit parameters, so the semantic path and a direct caller
    /// produce identical `Decl`s.
    pub fn new_with(
        params: Md3Options,
        p: RailParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let (expressive, expanded) = match p.variant {
            RailVariant::Classic => (false, false),
            RailVariant::Expressive { expanded } => (true, expanded),
        };
        // Expressive labels live inside the pill, so it always keeps the 56×32 capsule.
        let no_label = !expressive && p.label_type == RailLabelType::None;
        let base = navigation_rail_spec(NavigationRailFlags {
            no_label,
            ..NavigationRailFlags::NONE
        });
        let disabled = params.disabled || cx.state(p.id).disabled;
        let fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };
        // One tween drives the pill and the label; the root declares the width and lets the
        // engine interpolate it, so both read the same clock and land on the same frame.
        let expand = if expressive {
            cx.at(p.id)
                .animate(p.id.with("expand"), f32::from(expanded), EXPAND)
        } else {
            0.0
        };
        let medium = crate::typescale::LABEL_MEDIUM;
        let large = crate::typescale::LABEL_LARGE;
        let layer_ink = role(
            pack,
            navigation_rail_spec(NavigationRailFlags {
                no_label,
                hovered: true,
                ..NavigationRailFlags::NONE
            })
            .state_layer
            .color,
        );
        let hover_a = navigation_rail_spec(NavigationRailFlags {
            no_label,
            hovered: true,
            ..NavigationRailFlags::NONE
        })
        .state_layer
        .opacity;
        let focus_a = navigation_rail_spec(NavigationRailFlags {
            no_label,
            focused: true,
            ..NavigationRailFlags::NONE
        })
        .state_layer
        .opacity;
        let press_a = navigation_rail_spec(NavigationRailFlags {
            no_label,
            pressed: true,
            ..NavigationRailFlags::NONE
        })
        .state_layer
        .opacity;
        let mut items = Vec::with_capacity(p.items.len());
        for (k, item) in p.items.iter().enumerate() {
            let item_id = item.id;
            let mut node = cx.at(item_id);
            let state = node.state();
            let on = navigation_rail_spec(NavigationRailFlags {
                active: true,
                no_label,
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
            });
            let off = navigation_rail_spec(NavigationRailFlags {
                no_label,
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
                ..NavigationRailFlags::NONE
            });
            let sel = node.animate(
                item_id.with(param::PROGRESS),
                f32::from(k == p.selected),
                DESTINATION_SELECT,
            );
            let hover = node.animate(
                item_id.with(param::HOVER),
                f32::from(state.hovered),
                REACTION,
            );
            let focus = node.animate(
                item_id.with(param::FOCUS),
                f32::from(state.focus_visible),
                REACTION,
            );
            let alpha = (hover * hover_a).max(focus * focus_a);
            let layer = (alpha > 0.0 && !disabled).then_some(Color {
                a: layer_ink.a * alpha,
                ..layer_ink
            });
            let press = (!disabled).then(|| {
                // The ripple covers the capsule, not the whole destination: the indicator
                // is the effective background (Flutter `_IndicatorInkWell`).
                // (Flutter `_IndicatorInkWell`).
                press_ink_in(
                    Color {
                        a: layer_ink.a * press_a,
                        ..layer_ink
                    },
                    InkRadius::Cover,
                    item_id.with("indicator"),
                )
            });
            // The rail has no focus-indicator tokens; the keyboard ring borrows the capsule's
            // own ink rather than inventing a colour.
            let ring = if disabled {
                0.0
            } else {
                node.animate(
                    item_id.with(param::THICKNESS),
                    f32::from(state.focus_visible),
                    REACTION,
                )
            };
            // The label does not switch on: it fades in while its own room grows, so the
            // destinations below glide instead of jumping by a whole label's height.
            let labelled = expressive || p.label_type != RailLabelType::None;
            let reveal = if expressive {
                // The label only starts fading once the pill has room for it.
                ((expand - 0.2) / 0.6).clamp(0.0, 1.0)
            } else {
                match p.label_type {
                    RailLabelType::None => 0.0,
                    RailLabelType::All => 1.0,
                    RailLabelType::Selected => sel,
                }
            };
            items.push(Item {
                id: item_id,
                label: item.label.to_string(),
                icon: item.icon,
                labelled,
                reveal,
                height: if expressive {
                    EXP_ITEM_H
                } else {
                    destination_height(
                        base.active_indicator.height,
                        base.label_text.line_height,
                        reveal,
                    )
                },
                icon_ink: faded(
                    between(
                        spec_ink(pack, off.icon.color, 1.0),
                        spec_ink(pack, on.icon.color, 1.0),
                        sel,
                    ),
                    fade,
                ),
                label_ink: faded(
                    if expressive {
                        // The horizontal family exports no label colour; inside the pill the
                        // label must read `on-secondary-container`, the vertical token's
                        // `secondary` is for a label on the rail's own surface.
                        between(
                            role(pack, "secondary"),
                            role(pack, "onSecondaryContainer"),
                            expand,
                        )
                    } else {
                        between(
                            spec_ink(pack, off.label_text.color, 1.0),
                            spec_ink(pack, on.label_text.color, 1.0),
                            sel,
                        )
                    },
                    fade * reveal,
                ),
                label_weight: if expressive {
                    large.weight
                } else {
                    Weight(if k == p.selected {
                        on.label_text.weight as u16
                    } else {
                        off.label_text.weight as u16
                    })
                },
                layer,
                press,
                ring,
                indicator: if disabled { 0.0 } else { sel },
            });
        }
        Self {
            id: p.id,
            items,
            selected: p.selected,
            variant: p.variant,
            expand,
            // The declared width is always the target; the engine walks the layout to it.
            width: if expressive {
                if expanded {
                    EXP_EXPANDED
                } else {
                    EXP_COLLAPSED
                }
            } else {
                base.container.width
            },
            indicator_w: base.active_indicator.width,
            indicator_h: base.active_indicator.height,
            indicator_ink: spec_ink(pack, base.active_indicator.color, 1.0),
            icon_size: base.icon.size,
            label_size: if expressive {
                lerp(medium.size, large.size, expand)
            } else {
                base.label_text.size
            },
            label_h: if expressive {
                lerp(EXP_LABEL_LH.0, EXP_LABEL_LH.1, expand)
            } else {
                base.label_text.line_height
            },
            container: role(pack, base.container.color),
            disabled,
        }
    }

    /// The rail's own width, so a caller can reserve a column for it.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// The rail's own height: its destinations stacked inside the content margin. Only the
    /// width travels, so the height is the same at both ends of the Expressive pair.
    pub fn height(&self) -> f32 {
        if matches!(self.variant, RailVariant::Expressive { .. }) {
            let n = self.items.len() as f32;
            return EXP_TOP + n * EXP_ITEM_H + (n - 1.0).max(0.0) * EXP_ITEM_GAP;
        }
        RAIL_MARGIN * 2.0 + self.items.iter().map(|i| i.height).sum::<f32>()
    }

    /// Which destination a point in the rail's own space belongs to.
    pub fn item_at(&self, local: Point, height: f32) -> Option<usize> {
        if local.y < 0.0 || local.y > height {
            return None;
        }
        if matches!(self.variant, RailVariant::Expressive { .. }) {
            let y = local.y - EXP_TOP;
            if y < 0.0 {
                return None;
            }
            let stride = EXP_ITEM_H + EXP_ITEM_GAP;
            let k = (y / stride) as usize;
            // The gap between two destinations belongs to neither.
            return (k < self.items.len() && y - k as f32 * stride < EXP_ITEM_H).then_some(k);
        }
        // The margin is part of the rail's box but not of any destination's band.
        let y = local.y - RAIL_MARGIN;
        if y < 0.0 {
            return None;
        }
        let mut top = 0.0;
        for (k, item) in self.items.iter().enumerate() {
            if y < top + item.height {
                return Some(k);
            }
            top += item.height;
        }
        None
    }

    /// One destination, in the shape its family stacks: the baseline column or the pill.
    fn item_decl(&self, item: &Item) -> Decl {
        match self.variant {
            RailVariant::Classic => self.classic_item_decl(item),
            RailVariant::Expressive { .. } => self.expressive_item_decl(item),
        }
    }

    /// One destination: its state layer and ring, its indicator and centred content.
    fn classic_item_decl(&self, item: &Item) -> Decl {
        // A labelled variant stacks the capsule and the label from the top; a bare rail
        // splits the bottom room around the capsule, as Flutter's `none` label type does.
        let top = if item.labelled {
            0.0
        } else {
            (item.height - self.indicator_h) * 0.5
        };
        let mut content = Decl::container(item.id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(item.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: 0.0,
                right: 0.0,
                top,
                bottom: 0.0,
            })
            .gap(LABEL_GAP * item.reveal)
            .cross_align(0.0)
            .cross_pos(0.5);
        content = content.child(self.indicator_decl(item));
        if item.labelled {
            // The label's box gives its room back as the label fades, and clips, so a
            // half-open label shows only as much of itself as it has room for.
            let label = Text::new(
                item.id.with("label"),
                item.label.clone(),
                self.label_size,
                item.label_ink,
            )
            .weight(item.label_weight)
            .resolve();
            let slot = Decl::container(item.id.with("label-slot"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::hug(0.0, 0.0),
                    height: AxisSize::fixed(self.label_h * item.reveal),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .clip(Clip {
                    x: Overflow::Visible,
                    y: Overflow::Hidden,
                })
                .cross_align(0.0)
                .cross_pos(0.5)
                .child(label);
            content = content.child(slot);
        }
        Decl::container(item.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(item.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(item.press)
            .child(content)
    }

    /// The capsule (or circle) behind the icon, scaling in on x from the destination's centre.
    fn indicator_decl(&self, item: &Item) -> Decl {
        // Hover, focus and selection all paint inside the icon's capsule, not across the
        // whole destination: the capsule is the destination's effective background.
        let background = Background::stadium(self.indicator_w, self.indicator_h);
        let w = self.indicator_w * item.indicator;
        let mut shapes = Vec::with_capacity(3);
        if w > 0.0 {
            shapes.push(Shape::Rect {
                rect: Rect::new((self.indicator_w - w) * 0.5, 0.0, w, self.indicator_h),
                corners: Corners::all(w.min(self.indicator_h) * 0.5),
                // The capsule fades in with its width, so selection is a dissolve rather than
                // a hairline of full-strength colour appearing at once.
                fill: Color {
                    a: self.indicator_ink.a * item.indicator,
                    ..self.indicator_ink
                },
                gradient: None,
                border: None,
            });
        }
        if let Some(layer) = item.layer {
            shapes.push(background.fill(layer));
        }
        if item.ring > 0.0 {
            let ring = Color {
                a: self.indicator_ink.a * item.ring,
                ..self.indicator_ink
            };
            // MD3's focus indicator is 3dp (the bar's token says so); the rail exports none.
            shapes.push(background.ring(ring, 3.0));
        }
        Decl::container(item.id.with("indicator"), NodeKind::Column)
            .size(SizeClass::fixed(self.indicator_w, self.indicator_h))
            // The slot exists even when the capsule has not opened, so the ripple has a
            // capsule to be cut to and an unselected destination still shows one.
            .corners(Corners::all(self.indicator_h * 0.5))
            .padding(Insets {
                left: 0.0,
                right: 0.0,
                top: ((self.indicator_h - ui_widgets::icon_extent(self.icon_size)) * 0.5).max(0.0),
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .shapes(shapes)
            .child(self.icon_decl(item))
    }

    /// The icon with the `FILL` axis driving its selection state; a `Custom` source ignores it.
    fn icon_decl(&self, item: &Item) -> Decl {
        let filled = item.icon.fill(item.indicator);
        filled.resolve_with_size(item.id.with("icon"), item.icon_ink, self.icon_size)
    }

    /// An Expressive destination: one row whose pill spans the rail less its inset. The inset
    /// travels 20 → 16 so the pill's left edge lands on the token's full-width leading space.
    fn expressive_item_decl(&self, item: &Item) -> Decl {
        let inset = lerp((EXP_COLLAPSED - EXP_PILL_W) * 0.5, EXP_INSET, self.expand);
        Decl::container(item.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(item.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: inset,
                right: inset,
                top: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(item.press)
            .child(self.expressive_pill_decl(item))
    }

    /// The pill: icon and label in one row, clipped to its own stadium. Collapsed it is exactly
    /// the icon plus its 16dp padding; expanding, the label leaves through the clip.
    fn expressive_pill_decl(&self, item: &Item) -> Decl {
        let h = lerp(EXP_PILL_H.0, EXP_PILL_H.1, self.expand);
        let corners = Corners::all(h * 0.5);
        let mut shapes = Vec::with_capacity(3);
        if item.indicator > 0.0 {
            shapes.push(pill_box(
                corners,
                faded(self.indicator_ink, item.indicator),
                None,
            ));
        }
        if let Some(layer) = item.layer {
            shapes.push(pill_box(corners, layer, None));
        }
        if item.ring > 0.0 {
            let ring = Color {
                a: self.indicator_ink.a * item.ring,
                ..self.indicator_ink
            };
            shapes.push(pill_box(corners, Color::TRANSPARENT, Some((ring, 3.0))));
        }
        // A fixed label box keeps the text from re-wrapping as the pill opens; the pill's own
        // clip hides whatever has not been revealed yet.
        let label = Text::new(
            item.id.with("label"),
            item.label.clone(),
            self.label_size,
            item.label_ink,
        )
        .weight(item.label_weight)
        .max_lines(1)
        .resolve();
        let slot = Decl::container(item.id.with("label-slot"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed(EXP_LABEL_W),
                height: AxisSize::fixed(self.label_h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(label);
        Decl::container(item.id.with("indicator"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .corners(corners)
            .padding(Insets {
                left: EXP_INSET,
                right: EXP_INSET,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(EXP_GAP)
            .cross_align(0.0)
            .cross_pos(0.5)
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .shapes(shapes)
            .child(self.icon_decl(item))
            .child(slot)
    }
}

/// A destination's height at a label presence of `reveal`: the capsule, the label's room in
/// proportion, and the bottom spacing.
fn destination_height(indicator_h: f32, label_h: f32, reveal: f32) -> f32 {
    indicator_h + (LABEL_GAP + label_h) * reveal + BOTTOM_GAP
}

/// A node-sized rounded box: the Expressive pill follows whatever rect layout gives it, so its
/// fill, state layer and ring need no width of their own.
fn pill_box(corners: Corners, fill: Color, border: Option<(Color, f32)>) -> Shape {
    Shape::Box {
        corners,
        fill,
        gradient: None,
        border,
        shadow: None,
        image: None,
    }
}

impl Widget for Md3NavRail {
    fn handles_disabled(&self) -> bool {
        // The 38% content fade is painted here, so the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        // The press point names the destination; the keyboard has none (engine-audit F12).
        if self.disabled || !state.clicked {
            return None;
        }
        let k = self.item_at(state.local?, bounds.size.h)?;
        // Re-selecting the destination already selected is not a change.
        (k != self.selected).then(|| ui_core::Change::Selected(Some(self.items[k].id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let expressive = matches!(self.variant, RailVariant::Expressive { .. });
        let mut root = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(self.height()),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: 0.0,
                top: if expressive { EXP_TOP } else { RAIL_MARGIN },
                right: 0.0,
                bottom: if expressive { 0.0 } else { RAIL_MARGIN },
            })
            .gap(if expressive { EXP_ITEM_GAP } else { 0.0 })
            // The Expressive destinations fill the column so they track its interpolated
            // width; the baseline ones are already as wide as the rail.
            .cross_align(if expressive { 1.0 } else { 0.0 })
            .cross_pos(0.5)
            .shapes(vec![boxed(self.container, None)]);
        if expressive {
            // The app keeps declaring the target; the engine walks the width there before
            // layout, so the destinations reflow on the way instead of jumping at the end.
            root = root.transition_size(EXPAND);
        }
        for item in &self.items {
            root = root.child(self.item_decl(item));
        }
        root
    }
}
