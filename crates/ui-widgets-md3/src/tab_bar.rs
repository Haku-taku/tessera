//! MD3 secondary navigation tabs: equal-width tabs over a 1dp divider, a 2dp primary
//! underline under the selected one, and the label ink crossfading between the two roles.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Point};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{
    AnimSpec, CursorShape, Decl, Easing, Id, InkRadius, NodeKind, PressInk, Sense, Weight,
};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{between, faded, role, spec_ink};
use crate::spec::{SecondaryNavigationTabFlags, secondary_navigation_tab_spec};

/// `_kTabHeight`: the text-only tab's box; the bar adds the indicator band below it.
pub const TAB_HEIGHT: f32 = 46.0;
/// `_kTextAndIconTabHeight`: a tab carrying a 24dp mark grows to this.
pub const TAB_ICON_HEIGHT: f32 = 72.0;
/// `kTabLabelPadding`: the label's horizontal breathing room.
pub const TAB_LABEL_PAD: f32 = 16.0;
/// `_TabsPrimaryDefaultsM3.iconMargin`, which `Tab.build` uses for M3 either way.
pub const TAB_ICON_MARGIN: f32 = 2.0;
/// `_TabsSecondaryDefaultsM3.dividerHeight`; the export deprecated its own divider tokens.
pub const TAB_DIVIDER: f32 = 1.0;

/// `kTabScrollDuration`: the controller's travel, which drives the ink crossfade.
const INK: AnimSpec = AnimSpec::new(0.3, Easing::Linear);
/// The web demo's indicator travel: 250ms on `EASING.EMPHASIZED`, not Flutter's 300ms.
const INDICATOR: AnimSpec = AnimSpec::new(0.25, Easing::Cubic(0.3, 0.0, 0.0, 1.0));
use crate::motion::REACTION;

/// One tab: the id the app knows it by, its label, and, for the icon variant, a 24dp mark
/// built in the tab's own ink.
pub struct Md3TabItem<'a> {
    /// Owned by the app, so a reported `Change::Selected` names something it can map back.
    pub id: Id,
    pub label: &'a str,
    /// `None` is the text-only tab; `Some(f)` builds the mark at `id` in the given ink.
    pub icon: Option<IconSource>,
}

impl<'a> Md3TabItem<'a> {
    /// A text-only tab.
    pub fn text(id: Id, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
        }
    }

    /// A tab with a 24dp mark above its label.
    pub fn with_icon(id: Id, label: &'a str, icon: IconSource) -> Self {
        Self {
            id,
            label,
            icon: Some(icon),
        }
    }
}

/// One tab's resolved paint data.
struct Tab {
    id: Id,
    label: String,
    label_ink: Color,
    icon: Option<(IconSource, Color)>,
    /// The state layer over the whole tab, already carrying its animated alpha.
    layer: Option<Color>,
    /// The press ink the engine splats at the pointer.
    press: Option<PressInk>,
    /// The keyboard focus ring's alpha; 0 when it is not showing.
    ring: f32,
}

/// A secondary navigation tab bar; the app owns which tab is selected.
pub struct Md3TabBar {
    id: Id,
    tabs: Vec<Tab>,
    /// The tab's own box height, before the indicator band.
    content_h: f32,
    selected: usize,
    /// The bar's laid-out width, which the engine cannot hand to `decl`; the underline's
    /// travel is one share of it, so without it there is no slide.
    width: f32,
    /// How far the underline has travelled, in tabs.
    slide: f32,
    label_size: f32,
    label_weight: Weight,
    icon_size: f32,
    indicator: Color,
    ring_colour: Color,
    ring_width: f32,
    divider: Color,
    disabled: bool,
}

impl Md3TabBar {
    /// Same control with explicit parameters, so the semantic path and a direct caller
    /// produce identical `Decl`s.
    pub fn new_with(
        params: Md3Options,
        id: Id,
        items: &[Md3TabItem<'_>],
        selected: usize,
        width: f32,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let with_icon = items.iter().any(|t| t.icon.is_some());
        let base = secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
            with_icon,
            ..SecondaryNavigationTabFlags::NONE
        });
        let active = secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
            active: true,
            with_icon,
            ..SecondaryNavigationTabFlags::NONE
        });
        // The state layer only exists under hover/focus/pressed in the export, so each state's
        // row carries both the ink (`onSurface` in all three) and that state's strength.
        let hover_spec = secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
            with_icon,
            hovered: true,
            ..SecondaryNavigationTabFlags::NONE
        });
        let focus_spec = secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
            with_icon,
            focused: true,
            ..SecondaryNavigationTabFlags::NONE
        });
        let press_spec = secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
            with_icon,
            pressed: true,
            ..SecondaryNavigationTabFlags::NONE
        });
        let layer_ink = role(pack, hover_spec.state_layer.color);
        let disabled = params.disabled || cx.state(id).disabled;
        // The export defines no disabled inks; MD3's rule for disabled content is 38%.
        let ink_fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };
        let mut tabs = Vec::with_capacity(items.len());
        for (k, item) in items.iter().enumerate() {
            let tab_id = item.id;
            let mut node = cx.at(tab_id);
            let state = node.state();
            // The ink is keyed on the tab's own interaction state, as Flutter's `_TabStyle`.
            let on = secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
                active: true,
                with_icon,
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
            });
            let off = secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
                with_icon,
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
                ..SecondaryNavigationTabFlags::NONE
            });
            let select = node.animate(tab_id.with(param::PROGRESS), f32::from(k == selected), INK);
            let label_ink = faded(
                between(
                    spec_ink(pack, off.label_text.color, 1.0),
                    spec_ink(pack, on.label_text.color, 1.0),
                    select,
                ),
                ink_fade,
            );
            let icon_ink = faded(
                between(
                    spec_ink(pack, off.icon.color, 1.0),
                    spec_ink(pack, on.icon.color, 1.0),
                    select,
                ),
                ink_fade,
            );
            // The press is a splat from the pointer, so only hover and focus stay in the
            // flat layer, and the stronger of the two wins instead of adding up.
            let hover_t = node.animate(
                tab_id.with(param::HOVER),
                f32::from(state.hovered),
                REACTION,
            );
            let focus_t = node.animate(
                tab_id.with(param::FOCUS),
                f32::from(state.focus_visible),
                REACTION,
            );
            let alpha = (hover_t * hover_spec.state_layer.opacity)
                .max(focus_t * focus_spec.state_layer.opacity);
            let layer = (alpha > 0.0 && !disabled).then_some(Color {
                a: layer_ink.a * alpha,
                ..layer_ink
            });
            let press = (!disabled).then(|| {
                press_ink(
                    Color {
                        a: layer_ink.a * press_spec.state_layer.opacity,
                        ..layer_ink
                    },
                    InkRadius::Cover,
                )
            });
            let ring = if disabled {
                0.0
            } else {
                node.animate(
                    tab_id.with(param::THICKNESS),
                    f32::from(state.focus_visible),
                    REACTION,
                )
            };
            tabs.push(Tab {
                id: tab_id,
                label: item.label.to_string(),
                label_ink,
                icon: item.icon.map(|f| (f, icon_ink)),
                layer,
                press,
                ring,
            });
        }
        let slide = cx
            .at(id)
            .animate(id.with(param::PROGRESS), selected as f32, INDICATOR);
        Self {
            id,
            tabs,
            content_h: if with_icon {
                TAB_ICON_HEIGHT
            } else {
                TAB_HEIGHT
            },
            selected,
            width,
            slide,
            label_size: base.label_text.size,
            label_weight: Weight(base.label_text.weight as u16),
            icon_size: if with_icon { active.icon.size } else { 0.0 },
            indicator: spec_ink(pack, active.active_indicator.color, 1.0),
            ring_colour: spec_ink(pack, base.focus_indicator.color, 1.0),
            ring_width: base.focus_indicator.thickness,
            divider: role(pack, "outlineVariant"),
            disabled,
        }
    }

    /// The tab strip's height: the tab box plus the band its underline occupies.
    pub fn strip_height(&self) -> f32 {
        self.content_h + self.indicator_height()
    }

    /// The whole control's height, divider included.
    pub fn height(&self) -> f32 {
        self.strip_height() + TAB_DIVIDER
    }

    /// Which tab a point in the bar's own space belongs to; the divider belongs to none.
    pub fn tab_at(&self, local: Point, width: f32) -> Option<usize> {
        if self.tabs.is_empty() || local.y < 0.0 || local.y > self.strip_height() || width <= 0.0 {
            return None;
        }
        let share = width / self.tabs.len() as f32;
        Some(((local.x / share).floor().max(0.0) as usize).min(self.tabs.len() - 1))
    }

    /// One tab node: its state layer and focus ring, its centred content, its underline.
    fn tab_decl(&self, tab: &Tab) -> Decl {
        let mut shapes = Vec::with_capacity(2);
        if let Some(layer) = tab.layer {
            shapes.push(boxed(layer, None));
        }
        // The export insets the ring by `outline.offset` (negative), which needs a width the
        // decl does not have; it is drawn on the tab's own edge instead.
        if tab.ring > 0.0 {
            let ring = Color {
                a: self.ring_colour.a * tab.ring,
                ..self.ring_colour
            };
            shapes.push(boxed(Color::TRANSPARENT, Some((ring, self.ring_width))));
        }
        let label = Text::new(
            tab.id.with("label"),
            tab.label.clone(),
            self.label_size,
            tab.label_ink,
        )
        .weight(self.label_weight)
        .resolve();
        // `Center(widthFactor: 1)` over a fixed-height box: the content block is centred.
        let inner = match tab.icon {
            Some(_) => self.icon_size + TAB_ICON_MARGIN + label.size_class.height.hint,
            None => label.size_class.height.hint,
        };
        let mut content = Decl::container(tab.id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.content_h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: TAB_LABEL_PAD,
                right: TAB_LABEL_PAD,
                top: ((self.content_h - inner) * 0.5).max(0.0),
                bottom: 0.0,
            })
            .gap(TAB_ICON_MARGIN)
            // Children hug their own width and are centred in the space they leave.
            .cross_align(0.0)
            .cross_pos(0.5);
        if let Some((source, ink)) = tab.icon {
            content = content.child(source.resolve(tab.id.with("icon"), ink));
        }
        content = content.child(label);
        Decl::container(tab.id, NodeKind::Column)
            .size(SizeClass {
                // Equal shares: a zero basis splits the bar evenly whatever the labels are.
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.strip_height()),
                grow_x: Grow::Split(1),
                grow_y: Grow::No,
            })
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(tab.press)
            .shapes(shapes)
            .child(content)
    }

    /// The one underline the whole bar shares: it slides to the selected tab instead of each
    /// tab fading its own in and out.
    fn indicator_decl(&self) -> Option<Decl> {
        if self.tabs.is_empty() || self.width <= 0.0 {
            return None;
        }
        let share = self.width / self.tabs.len() as f32;
        // Disabled bars mark no selection at all.
        let alpha = if self.disabled { 0.0 } else { self.indicator.a };
        Some(
            Decl::container(self.id.with("indicator"), NodeKind::Overlay)
                .size(SizeClass::fixed(share, self.indicator_height()))
                .offset(self.slide * share, self.content_h)
                .shapes(vec![boxed(
                    Color {
                        a: alpha,
                        ..self.indicator
                    },
                    None,
                )]),
        )
    }

    /// `active-indicator.height`, which is also the band the strip reserves for it.
    fn indicator_height(&self) -> f32 {
        secondary_navigation_tab_spec(SecondaryNavigationTabFlags {
            active: true,
            ..SecondaryNavigationTabFlags::NONE
        })
        .active_indicator
        .height
    }
}

impl Widget for Md3TabBar {
    fn handles_disabled(&self) -> bool {
        // The 38% content fade is painted here, so the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        // The press point names the tab; the keyboard has none (engine-audit F12).
        if self.disabled || !state.clicked {
            return None;
        }
        // Tabs are equal shares of the bar, so the press point names one without a lookup.
        let k = self.tab_at(state.local?, bounds.size.w)?;
        // Re-selecting the tab that is already selected is not a change; Flutter still calls
        // `onTap`, but the semantic door here reports only movement.
        (k != self.selected).then(|| ui_core::Change::Selected(Some(self.tabs[k].id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let fill = || AxisSize::fill(0.0, 0.0);
        let mut row = Decl::container(self.id.with("tabs"), NodeKind::Row).size(SizeClass {
            width: fill(),
            height: AxisSize::fixed(self.strip_height()),
            grow_x: Grow::No,
            grow_y: Grow::No,
        });
        for tab in &self.tabs {
            row = row.child(self.tab_decl(tab));
        }
        let divider = Decl::container(self.id.with("divider"), NodeKind::Column)
            .size(SizeClass {
                width: fill(),
                height: AxisSize::fixed(TAB_DIVIDER),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .shapes(vec![boxed(self.divider, None)]);
        let mut root = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: fill(),
                height: AxisSize::fixed(self.height()),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .child(row)
            .child(divider);
        if let Some(indicator) = self.indicator_decl() {
            root = root.child(indicator);
        }
        root
    }
}

/// A node-sized box: it follows whatever rect the layout hands the node.
fn boxed(fill: Color, border: Option<(Color, f32)>) -> Shape {
    Shape::Box {
        corners: Corners::ZERO,
        fill,
        gradient: None,
        border,
        shadow: None,
        image: None,
    }
}
