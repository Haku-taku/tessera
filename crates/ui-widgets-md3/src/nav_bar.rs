//! MD3 navigation bar: equal destinations on an 80dp surface, a 64×32 capsule behind the
//! selected icon, and labels the app can always show, never show, or show only when selected.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Point, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{CursorShape, Decl, Id, InkRadius, NodeKind, PressInk, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::{Background, press_ink_in};
use crate::paint::{between, faded, role, spec_ink};
use crate::spec::{NavigationBarFlags, navigation_bar_spec};

use crate::motion::{DESTINATION_SELECT, REACTION};
/// `labelPadding: EdgeInsets.only(top: 4)`; the export carries no icon-label gap.
const LABEL_GAP: f32 = 4.0;

/// Whether the bar shows the labels of its destinations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NavLabelBehavior {
    /// Every destination carries its label.
    #[default]
    AlwaysShow,
    /// No destination carries a label; only the icons remain.
    AlwaysHide,
    /// Only the selected destination carries a label.
    OnlyShowSelected,
}

/// One destination: the id the app knows it by, its label and its 24dp mark.
pub struct Md3NavItem<'a> {
    /// Owned by the app, so a reported `Change::Selected` names something it can map back.
    pub id: Id,
    pub label: &'a str,
    /// The icon slot: a source the bar resolves at the destination's icon id, in the ink and
    /// `FILL` axis the current state resolves to. A `Custom` source keeps the slot open to
    /// anything the app can paint.
    pub icon: IconSource,
}

impl<'a> Md3NavItem<'a> {
    /// A destination with an icon and a label.
    pub fn new(id: Id, label: &'a str, icon: IconSource) -> Self {
        Self { id, label, icon }
    }
}

/// Everything a navigation bar needs besides the design system's own tokens.
pub struct NavBarParams<'a> {
    pub id: Id,
    pub items: &'a [Md3NavItem<'a>],
    /// The selected destination's index; the app owns it, as with tabs.
    pub selected: usize,
    pub label_behavior: NavLabelBehavior,
    pub disabled: bool,
}

/// One destination's resolved paint data.
struct Item {
    id: Id,
    label: String,
    icon: IconSource,
    /// Whether the label's slot exists in the layout, whatever the label's alpha is.
    show_label: bool,
    /// Whether the label fades with the selection instead of always being drawn.
    fade_label: bool,
    /// The selection progress, `0..=1`, which slides the content between the labelled and
    /// the bare position.
    progress: f32,
    icon_ink: Color,
    label_ink: Color,
    label_weight: Weight,
    /// The state layer over the whole destination, already carrying its animated alpha.
    layer: Option<Color>,
    /// The press ink the engine splats at the pointer.
    press: Option<PressInk>,
    /// The keyboard focus ring's alpha; 0 when it is not showing.
    ring: f32,
    /// The indicator's scale on x, `0..=1`.
    indicator: f32,
}

/// A navigation bar; the app owns which destination is selected.
pub struct Md3NavBar {
    id: Id,
    items: Vec<Item>,
    selected: usize,
    height: f32,
    indicator_w: f32,
    indicator_h: f32,
    indicator_ink: Color,
    icon_size: f32,
    label_size: f32,
    label_h: f32,
    container: Color,
    ring_colour: Color,
    ring_width: f32,
    disabled: bool,
}

impl Md3NavBar {
    /// Same control with explicit parameters, so the semantic path and a direct caller
    /// produce identical `Decl`s.
    pub fn new_with(
        params: Md3Options,
        p: NavBarParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let base = navigation_bar_spec(NavigationBarFlags::NONE);
        let disabled = params.disabled || cx.state(p.id).disabled;
        // The export defines no disabled inks; MD3's rule for disabled content is 38%.
        let fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };
        // The state layer's ink only exists under hover/focus/pressed in the export.
        let layer_ink = role(
            pack,
            navigation_bar_spec(NavigationBarFlags {
                hovered: true,
                ..NavigationBarFlags::NONE
            })
            .state_layer
            .color,
        );
        let hover_a = navigation_bar_spec(NavigationBarFlags {
            hovered: true,
            ..NavigationBarFlags::NONE
        })
        .state_layer
        .opacity;
        let focus_a = navigation_bar_spec(NavigationBarFlags {
            focused: true,
            ..NavigationBarFlags::NONE
        })
        .state_layer
        .opacity;
        let press_a = navigation_bar_spec(NavigationBarFlags {
            pressed: true,
            ..NavigationBarFlags::NONE
        })
        .state_layer
        .opacity;
        let mut items = Vec::with_capacity(p.items.len());
        for (k, item) in p.items.iter().enumerate() {
            let item_id = item.id;
            let mut node = cx.at(item_id);
            let state = node.state();
            // The inks follow the destination's own state, as Flutter's destination theme does.
            let on = navigation_bar_spec(NavigationBarFlags {
                active: true,
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
            });
            let off = navigation_bar_spec(NavigationBarFlags {
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
                ..NavigationBarFlags::NONE
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
                // The ripple covers the icon's capsule, not the whole destination: the
                // indicator is the effective background (Flutter `_IndicatorInkWell`).
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
            let ring = if disabled {
                0.0
            } else {
                node.animate(
                    item_id.with(param::THICKNESS),
                    f32::from(state.focus_visible),
                    REACTION,
                )
            };
            // The slot and the fade are separate: only the selected-only bar reserves the
            // label's room on every destination, so a selection change moves the content
            // instead of resizing it.
            let (show_label, fade_label) = match p.label_behavior {
                NavLabelBehavior::AlwaysShow => (true, false),
                NavLabelBehavior::AlwaysHide => (false, false),
                NavLabelBehavior::OnlyShowSelected => (true, true),
            };
            let label_fade = if fade_label { sel } else { 1.0 };
            items.push(Item {
                id: item_id,
                label: item.label.to_string(),
                icon: item.icon,
                show_label,
                fade_label,
                progress: sel,
                icon_ink: faded(
                    between(
                        spec_ink(pack, off.icon.color, 1.0),
                        spec_ink(pack, on.icon.color, 1.0),
                        sel,
                    ),
                    fade,
                ),
                label_ink: faded(
                    between(
                        spec_ink(pack, off.label_text.color, 1.0),
                        spec_ink(pack, on.label_text.color, 1.0),
                        sel,
                    ),
                    fade * label_fade,
                ),
                label_weight: Weight(if k == p.selected {
                    on.label_text.weight as u16
                } else {
                    off.label_text.weight as u16
                }),
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
            height: base.container.height,
            indicator_w: base.active_indicator.width,
            indicator_h: base.active_indicator.height,
            indicator_ink: spec_ink(pack, base.active_indicator.color, 1.0),
            icon_size: base.icon.size,
            label_size: base.label_text.size,
            label_h: base.label_text.line_height,
            container: role(pack, base.container.color),
            ring_colour: spec_ink(pack, base.focus_indicator.color, 1.0),
            ring_width: base.focus_indicator.thickness,
            disabled,
        }
    }

    /// The bar's own height, so a caller can reserve room for it.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Which destination a point in the bar's own space belongs to.
    pub fn item_at(&self, local: Point, width: f32) -> Option<usize> {
        if self.items.is_empty() || local.y < 0.0 || local.y > self.height || width <= 0.0 {
            return None;
        }
        let share = width / self.items.len() as f32;
        Some(((local.x / share).floor().max(0.0) as usize).min(self.items.len() - 1))
    }

    /// The content block's height: the indicator, and the label when it shows.
    fn content_h(&self, show_label: bool) -> f32 {
        if show_label {
            self.indicator_h + LABEL_GAP + self.label_h
        } else {
            self.indicator_h
        }
    }

    /// One destination: its state layer and focus ring, its indicator and centred content.
    fn item_decl(&self, item: &Item) -> Decl {
        let content_h = self.content_h(item.show_label);
        // A selected-only destination holds the labelled position and the bare one at once and
        // moves between them with the selection: the icons glide, nothing resizes.
        let labelled = ((self.height - content_h) * 0.5).max(0.0);
        let bare = ((self.height - self.indicator_h) * 0.5).max(0.0);
        let top = if item.fade_label {
            bare + (labelled - bare) * item.progress
        } else {
            labelled
        };
        let mut content = Decl::container(item.id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(content_h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .gap(if item.show_label { LABEL_GAP } else { 0.0 })
            // Children hug their own width and are centred in the space they leave.
            .cross_align(0.0)
            .cross_pos(0.5);
        content = content.child(self.indicator_decl(item));
        if item.show_label {
            let label = Text::new(
                item.id.with("label"),
                item.label.clone(),
                self.label_size,
                item.label_ink,
            )
            .weight(item.label_weight)
            .resolve();
            content = content.child(label);
        }
        Decl::container(item.id, NodeKind::Column)
            .size(SizeClass {
                // Equal shares: a zero basis splits the bar evenly whatever the labels are.
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::Split(1),
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: 0.0,
                right: 0.0,
                top,
                bottom: 0.0,
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

    /// The capsule behind the icon: Flutter scales it in on x from the destination's centre.
    /// The icon with the `FILL` axis driving its selection state; a `Custom` source ignores it.
    fn icon_decl(&self, item: &Item) -> Decl {
        let filled = item.icon.fill(item.indicator);
        filled.resolve_with_size(item.id.with("icon"), item.icon_ink, self.icon_size)
    }

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
                a: self.ring_colour.a * item.ring,
                ..self.ring_colour
            };
            shapes.push(background.ring(ring, self.ring_width));
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
}

impl Widget for Md3NavBar {
    fn handles_disabled(&self) -> bool {
        // The 38% content fade is painted here, so the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        // The press point names the destination; the keyboard has none (engine-audit F12).
        if self.disabled || !state.clicked {
            return None;
        }
        // Destinations are equal shares of the bar, so the press point names one directly.
        let k = self.item_at(state.local?, bounds.size.w)?;
        // Re-selecting the destination already selected is not a change.
        (k != self.selected).then(|| ui_core::Change::Selected(Some(self.items[k].id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut root = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .shapes(vec![boxed(self.container, None)]);
        for item in &self.items {
            root = root.child(self.item_decl(item));
        }
        root
    }
}

/// A node-sized box: it follows whatever rect the layout hands the node.
pub(crate) fn boxed(fill: Color, border: Option<(Color, f32)>) -> Shape {
    Shape::Box {
        corners: Corners::ZERO,
        fill,
        gradient: None,
        border,
        shadow: None,
        image: None,
    }
}
