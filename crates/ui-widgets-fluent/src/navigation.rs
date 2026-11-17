//! The Fluent navigation view: a 48 dp compact strip that opens into a 320 dp pane.

use ui_core::draw::{Acrylic, BackdropMaterial, FocusVisual};
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{Anchor, AxisSize, Grow, OverlayAlign, OverlayPlacement, Side, SizeClass};
use ui_core::shortcut::ShortcutId;
use ui_core::{
    BackdropBlur, Change, CursorShape, Decl, Id, Key, KeyChord, Modifiers, NodeKind, Sense, Weight,
};
use ui_platform::{Container, Element};
use ui_style::{ControlKind, Density, Style, TextRole};
use ui_widgets::text::Text;
use ui_widgets::{Cx, Icon, WidgetState};

use crate::glyphs;
use crate::material::{AcrylicRecipe, acrylic_dark, acrylic_light};
use crate::paint::role_of;

/// `NavigationViewCompactPaneLength`, the `OpenPaneLength` default, and item metrics.
pub const COMPACT_PANE_W: f32 = 48.0;
pub const OPEN_PANE_W: f32 = 320.0;
pub const NAV_ITEM_MARGIN: Insets = Insets::new(4.0, 2.0, 4.0, 2.0);
pub const ICON_BOX_W: f32 = 40.0;
pub const ICON_SIZE: f32 = 16.0;
/// `NavigationViewItemContentPresenterMargin` left 4; the content grid's own
/// `Margin="0,0,14,0"` rides on the pill's right padding.
pub const LABEL_LEAD: f32 = 4.0;
pub const CONTENT_TRAIL: f32 = 14.0;
pub const INDICATOR_W: f32 = 3.0;
pub const INDICATOR_H: f32 = 16.0;
pub const INDICATOR_RADIUS: f32 = 2.0;
/// `PaneToggleButtonWidth` / `PaneToggleButtonHeight`; its corner is `ControlCornerRadius`.
pub const TOGGLE_W: f32 = 40.0;
pub const TOGGLE_H: f32 = 36.0;
/// `NavigationViewButtonHolderGridMargin`: 0,4 around the toggle.
pub const TOGGLE_MARGIN: Insets = Insets::new(0.0, 4.0, 0.0, 4.0);
/// `NavigationViewContentGridCornerRadius` and its `BorderThickness="1,1,0,0"`.
pub const CONTENT_RADIUS: f32 = 8.0;
pub const CONTENT_BORDER: f32 = 1.0;
/// `NavigationViewItemHeader`'s row height and inset, and how far its items step in.
pub const NAV_HEADER_H: f32 = 40.0;
pub const NAV_HEADER_INDENT: f32 = 16.0;
pub const NAV_ITEM_INDENT: f32 = 16.0;
/// The header's ink: `NavigationViewItemHeaderForeground`.
const HEADER_FG: &str = "NavigationViewItemHeaderForeground";

/// Escape closes the pane while it is open over the content.
pub const NAV_DISMISS_SHORTCUT: ShortcutId = ShortcutId("nav.dismiss");

/// `PaneDisplayMode`: how much of the pane stays on screen when it is closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneDisplayMode {
    /// WinUI's `Left`: the pane is in the flow, and `open` widens it from 48 to 320.
    #[default]
    Expanded,
    /// WinUI's `LeftCompact`: a 48 strip stays in the flow; opening covers the content.
    Compact,
    /// WinUI's `LeftMinimal`: no strip at all; the toggle sits above the content, and the
    /// pane exists only while it is open, over the content.
    Minimal,
}

use crate::spec::{DISABLED, POINTER_OVER, PRESSED, REST};

/// `NavigationViewItem`'s two brush families: plain and selected, four `CommonStates` each.
const ITEM_BG: [&str; 4] = [
    "NavigationViewItemBackground",
    "NavigationViewItemBackgroundPointerOver",
    "NavigationViewItemBackgroundPressed",
    "NavigationViewItemBackgroundDisabled",
];
const ITEM_BG_SELECTED: [&str; 4] = [
    "NavigationViewItemBackgroundSelected",
    "NavigationViewItemBackgroundSelectedPointerOver",
    "NavigationViewItemBackgroundSelectedPressed",
    "NavigationViewItemBackgroundSelectedDisabled",
];
const ITEM_FG: [&str; 4] = [
    "NavigationViewItemForeground",
    "NavigationViewItemForegroundPointerOver",
    "NavigationViewItemForegroundPressed",
    "NavigationViewItemForegroundDisabled",
];
const ITEM_FG_SELECTED: [&str; 4] = [
    "NavigationViewItemForegroundSelected",
    "NavigationViewItemForegroundSelectedPointerOver",
    "NavigationViewItemForegroundSelectedPressed",
    "NavigationViewItemForegroundSelectedDisabled",
];
/// The pane toggle borrows the item's rest brushes and has its own pointed-at ones.
const TOGGLE_BG: [&str; 4] = [
    "NavigationViewItemBackground",
    "NavigationViewButtonBackgroundPointerOver",
    "NavigationViewButtonBackgroundPressed",
    "NavigationViewButtonBackgroundDisabled",
];
const TOGGLE_FG: [&str; 4] = [
    "NavigationViewItemForeground",
    "NavigationViewButtonForegroundPointerOver",
    "NavigationViewButtonForegroundPressed",
    "NavigationViewButtonForegroundDisabled",
];

/// One destination: its `Content`, its `Icon` and its `IsEnabled`.
#[derive(Clone, Debug)]
pub struct NavigationItem {
    pub id: Id,
    pub label: String,
    /// A name from `glyphs`, i.e. one the vendored icon face carries.
    pub icon: String,
    pub disabled: bool,
    /// A `NavigationViewItemHeader`: a group title, never selectable, and the items after
    /// it step in under it.
    pub header: bool,
}

impl NavigationItem {
    pub fn new(id: Id, label: impl Into<String>, icon: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            icon: icon.into(),
            disabled: false,
            header: false,
        }
    }

    /// A group title above the destinations that belong to it.
    pub fn header(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            icon: String::new(),
            disabled: true,
            header: true,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// Everything the view is built with, named after the properties the reference exposes.
pub struct NavigationViewParams<'a> {
    pub id: Id,
    pub items: &'a [NavigationItem],
    /// `SelectedItem`'s index; out of range means nothing is selected.
    pub selected: usize,
    /// `IsPaneOpen`: the pane is 320 wide when open, 48 when closed.
    pub open: bool,
    /// `PaneDisplayMode`; it decides whether `open` widens the pane in place or covers
    /// the content with it.
    pub display_mode: PaneDisplayMode,
    /// The app's page for the selected destination. Elements, not declarations: a control
    /// inside a page has to stay in the element tree to report what it did.
    pub content: Vec<Box<dyn Element>>,
    pub disabled: bool,
}

impl<'a> NavigationViewParams<'a> {
    pub fn new(id: Id, items: &'a [NavigationItem]) -> Self {
        Self {
            id,
            items,
            selected: 0,
            open: true,
            display_mode: PaneDisplayMode::Expanded,
            content: Vec::new(),
            disabled: false,
        }
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn display_mode(mut self, mode: PaneDisplayMode) -> Self {
        self.display_mode = mode;
        self
    }

    pub fn content(mut self, content: Vec<Box<dyn Element>>) -> Self {
        self.content = content;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// One destination, resolved against the pack in force where it was built.
struct Item {
    id: Id,
    label: String,
    icon: Option<ui_widgets::IconSource>,
    picked: bool,
    fill: Color,
    ink: Color,
    disabled: bool,
    header: bool,
    /// How far the pill steps in: `NAV_ITEM_INDENT` under a header, 0 above the first one.
    indent: f32,
}

/// A navigation view, resolved against the pack in force where it was built.
pub struct FluentNavigationView {
    /// The assembled view. Element children have to mirror declaration children one for
    /// one, so the whole chrome is built out of containers rather than one declaration.
    root: Container,
    id: Id,
    items: Vec<Item>,
    /// The destination a press or Enter landed on this frame, if any.
    fired: Option<Id>,
    toggle: bool,
    toggle_fill: Color,
    toggle_ink: Color,
    mode: PaneDisplayMode,
    /// The strip's width in the flow this frame; 0 in `Minimal`.
    width: f32,
    /// The strip shows labels only where it is the open pane of `Expanded`.
    strip_labels: bool,
    /// The pane is open over the content, as `Compact`/`Minimal` open it.
    overlay: bool,
    /// A press outside the open pane, or Escape, asked it to close.
    dismissed: bool,
    open: bool,
    label_size: f32,
    /// `NavigationViewItemOnLeftMinHeight` and `OverlayCornerRadius` from the pack's row.
    item_min_h: f32,
    item_radius: f32,
    toggle_radius: f32,
    indicator: Color,
    content_fill: Color,
    content_stroke: Color,
    /// `NavigationViewDefaultPaneBackground` is in-app acrylic, painted by the backdrop pass.
    acrylic: Acrylic,
    blur_sigma: f32,
    pane_stroke: Color,
    focus: FocusVisual,
    toggle_focus: FocusVisual,
    disabled: bool,
}

impl FluentNavigationView {
    pub fn new(params: NavigationViewParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let NavigationViewParams {
            id,
            items,
            selected,
            open,
            display_mode,
            content,
            disabled,
        } = params;
        let label_size = pack.type_table.get(TextRole::Body).size;
        let m = pack
            .controls
            .get(ControlKind::NavigationViewItem, Density::Standard);
        let toggle_radius = pack
            .controls
            .get(ControlKind::Button, Density::Standard)
            .radius;
        // Items under a header step in; the first header starts the indentation.
        let mut grouped = false;
        let items: Vec<Item> = items
            .iter()
            .enumerate()
            .map(|(k, item)| {
                if item.header {
                    grouped = true;
                }
                let st = cx.state(item.id);
                let off = disabled || item.disabled;
                let slot = if off {
                    DISABLED
                } else if st.pressed {
                    PRESSED
                } else if st.hovered {
                    POINTER_OVER
                } else {
                    REST
                };
                let picked = !item.header && k == selected;
                Item {
                    id: item.id,
                    label: item.label.clone(),
                    icon: if item.header {
                        None
                    } else {
                        glyphs::icon(&item.icon)
                    },
                    picked,
                    fill: role_of(
                        pack,
                        if picked {
                            ITEM_BG_SELECTED[slot]
                        } else {
                            ITEM_BG[slot]
                        },
                    ),
                    ink: if item.header {
                        role_of(pack, HEADER_FG)
                    } else {
                        role_of(
                            pack,
                            if picked {
                                ITEM_FG_SELECTED[slot]
                            } else {
                                ITEM_FG[slot]
                            },
                        )
                    },
                    disabled: off,
                    header: item.header,
                    indent: if item.header || !grouped {
                        0.0
                    } else {
                        NAV_ITEM_INDENT
                    },
                }
            })
            .collect();
        // A press names one destination; picking the one already selected is not a change.
        let fired = items
            .iter()
            .filter(|it| !it.picked && !it.disabled)
            .find(|it| {
                let st = cx.state(it.id);
                st.activation()
            })
            .map(|it| it.id);
        // The open pane carries its own toggle, so there are two of them across the modes:
        // the strip's and the overlay's. Either one is the press this frame.
        let strip_toggle = cx.state(id.with("pane-toggle"));
        let overlay_toggle = cx.state(id.with("overlay-toggle"));
        let toggle = !disabled && (strip_toggle.activation() || overlay_toggle.activation());
        let toggle_slot = if disabled {
            DISABLED
        } else if strip_toggle.pressed || overlay_toggle.pressed {
            PRESSED
        } else if strip_toggle.hovered || overlay_toggle.hovered {
            POINTER_OVER
        } else {
            REST
        };
        // Only the open overlay is a popup root, so only then can the engine report a press
        // outside it; Escape is bound on the same node.
        let overlay = open && display_mode != PaneDisplayMode::Expanded;
        let pane_state = cx.state(id.with("overlay"));
        let dismissed =
            overlay && (pane_state.dismissed || pane_state.shortcut == Some(NAV_DISMISS_SHORTCUT));
        let width = match display_mode {
            PaneDisplayMode::Expanded if open => OPEN_PANE_W,
            PaneDisplayMode::Expanded | PaneDisplayMode::Compact => COMPACT_PANE_W,
            PaneDisplayMode::Minimal => 0.0,
        };
        let recipe: AcrylicRecipe = match pack.appearance() {
            Some(ui_design::Appearance::Dark) => acrylic_dark(),
            _ => acrylic_light(),
        };
        let mut view = Self {
            root: Container::new(id, NodeKind::Box),
            id,
            items,
            fired,
            toggle,
            toggle_fill: role_of(pack, TOGGLE_BG[toggle_slot]),
            toggle_ink: role_of(pack, TOGGLE_FG[toggle_slot]),
            mode: display_mode,
            width,
            strip_labels: display_mode == PaneDisplayMode::Expanded && open,
            overlay,
            dismissed,
            open,
            label_size,
            item_min_h: m.min_height,
            item_radius: m.radius,
            toggle_radius,
            indicator: role_of(pack, "NavigationViewSelectionIndicatorForeground"),
            content_fill: role_of(pack, "NavigationViewContentBackground"),
            content_stroke: role_of(pack, "NavigationViewContentGridBorderBrush"),
            acrylic: Acrylic {
                tint: recipe.tint,
                luminosity: recipe.luminosity,
                noise: recipe.noise,
                fallback: recipe.fallback,
            },
            blur_sigma: recipe.blur_sigma,
            // No `NavigationViewPaneBorderBrush` in the corpus; the content grid's own stroke
            // stands in for the reference's `ThemeShadow` on the overlay's edge.
            pane_stroke: role_of(pack, "NavigationViewContentGridBorderBrush"),
            focus: item_focus(pack, m.radius),
            toggle_focus: toggle_focus(pack, toggle_radius),
            disabled,
        };
        view.root = view.assemble(content);
        view
    }

    /// The strip's width this frame, so a caller can reserve room beside it; 0 in `Minimal`.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// A group title: no pill, no state, and no pointer of its own.
    fn group_header_decl(&self, item: &Item) -> Decl {
        Decl::container(item.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(NAV_HEADER_H, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(NAV_HEADER_INDENT, 0.0, CONTENT_TRAIL, 0.0))
            .cross_align(0.0)
            .cross_pos(0.5)
            .child({
                let mut label = Text::new(
                    item.id.with("label"),
                    item.label.clone(),
                    self.label_size,
                    item.ink,
                )
                .nowrap();
                // `BodyStrongTextBlockStyle`: the header is the one semibold row in the pane.
                label.weight = Weight::SEMIBOLD;
                label.resolve()
            })
    }

    /// One destination: the pill, its indicator and its icon-then-label content.
    fn item_decl(&self, item: &Item, labeled: bool) -> Decl {
        if item.header {
            return self.group_header_decl(item);
        }
        let indent = if labeled { item.indent } else { 0.0 };
        let mut icon_box = Decl::container(item.id.with("icon"), NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fixed(ICON_BOX_W),
                height: AxisSize::fixed(ICON_SIZE),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .stack_align(OverlayAlign::Center);
        if let Some(source) = item.icon {
            icon_box = icon_box.child(
                Icon {
                    id: item.id.with("glyph"),
                    source,
                    size: ICON_SIZE,
                    ink: item.ink,
                }
                .resolve(),
            );
        }
        // The indicator overlays the icon column's first 3 dp, so the icon box starts 3 dp
        // early: a negative margin pulls it back to the pill's edge without moving the label.
        let overlap = if item.picked { -INDICATOR_W } else { 0.0 };
        let mut pill = Decl::container(item.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(self.item_min_h, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .margin(Insets::new(
                NAV_ITEM_MARGIN.left + indent,
                NAV_ITEM_MARGIN.top,
                NAV_ITEM_MARGIN.right,
                NAV_ITEM_MARGIN.bottom,
            ))
            // The content grid's right margin is the pill's padding, so the wash still
            // spans the whole row while the label stops short of its edge.
            .padding(Insets {
                left: 0.0,
                top: 0.0,
                right: if labeled { CONTENT_TRAIL } else { 0.0 },
                bottom: 0.0,
            })
            // The children keep their own heights and centre in the 36 dp row, as the
            // reference's grid does; `cross_pos` is the parent's say over all of them.
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if item.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(item.disabled)
            .corners(Corners::all(self.item_radius))
            .focus_visual(Some(self.focus))
            .background(item.fill, Corners::all(self.item_radius));
        if item.picked {
            pill = pill.child(self.indicator_decl(&item.id));
        }
        pill = pill.child(icon_box.margin(Insets::new(overlap, 0.0, 0.0, 0.0)));
        if labeled {
            let label = Text::new(
                item.id.with("label"),
                item.label.clone(),
                self.label_size,
                item.ink,
            )
            .nowrap()
            .resolve()
            .margin(Insets::new(LABEL_LEAD, 0.0, 0.0, 0.0));
            pill = pill.child(label);
        }
        pill
    }

    /// `SelectionIndicator`: 3×16, radius 2, against the pill's start edge.
    fn indicator_decl(&self, id: &Id) -> Decl {
        Decl::container(id.with("bar"), NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(INDICATOR_W),
                height: AxisSize::fixed(INDICATOR_H),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background(self.indicator, Corners::all(INDICATOR_RADIUS))
    }

    /// The toggle's own row: the pane header, and in `Minimal` the band above the content.
    fn header_decl(&self, toggle: Id) -> Decl {
        Decl::container(toggle.with("header"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .child(self.toggle_decl(toggle))
    }

    /// The pane toggle: a 40×36 button inside the pane header's 4 dp holder margin.
    fn toggle_decl(&self, id: Id) -> Decl {
        let mut button = Decl::container(id, NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fixed(TOGGLE_W),
                height: AxisSize::fixed(TOGGLE_H),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .margin(TOGGLE_MARGIN)
            .stack_align(OverlayAlign::Center)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(CursorShape::Arrow)
            .disabled(self.disabled)
            .corners(Corners::all(self.toggle_radius))
            .focus_visual(Some(self.toggle_focus))
            .background(self.toggle_fill, Corners::all(self.toggle_radius));
        // The vendored icon face has no hamburger, so the toggle shows the chevron the
        // reference itself uses in `Minimal` mode: it points the way the pane would move.
        let name = if self.open {
            "chevron_left_12_regular"
        } else {
            "chevron_right_12_regular"
        };
        if let Some(source) = glyphs::icon(name) {
            button = button.child(
                Icon {
                    id: id.with("glyph"),
                    source,
                    // The 12 px variant is drawn for a 12 px em; scaling it up blurs the join.
                    size: 12.0,
                    ink: self.toggle_ink,
                }
                .resolve(),
            );
        }
        button
    }

    /// The content surface: top-left corner, with the border edges the reference draws.
    fn content_node(&self, content: Vec<Box<dyn Element>>) -> Container {
        let body = Container::new(self.id.with("content-body"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .children(content);
        // `BorderThickness="1,1,0,0"`: the left edge and the corner only while it is open.
        let corners = if self.open {
            Corners {
                tl: CONTENT_RADIUS,
                ..Corners::ZERO
            }
        } else {
            Corners::ZERO
        };
        let edge = |slot: &'static str, w: AxisSize, h: AxisSize| {
            Decl::container(self.id.with(slot), NodeKind::Box)
                .size(SizeClass {
                    width: w,
                    height: h,
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .background(self.content_stroke, Corners::ZERO)
        };
        // The edges are ordinary flow rows: a 1 dp strip above, and one beside the body.
        let mut inner =
            Container::new(self.id.with("content-inner"), NodeKind::Row).size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            });
        if self.open {
            inner = inner.child_decl(edge(
                "content-left",
                AxisSize::fixed(CONTENT_BORDER),
                AxisSize::fill(0.0, 0.0),
            ));
        }
        let area = Container::new(self.id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .map(|d| d.background(self.content_fill, corners))
            .child_decl(edge(
                "content-top",
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(CONTENT_BORDER),
            ));
        area.child(Box::new(inner.child(Box::new(body))))
    }

    /// The pane while it is open over the content: the compact overlay of `Compact`/`Minimal`.
    fn overlay_decl(&self) -> Decl {
        let id = self.id.with("overlay");
        // The pane overlaps the view by its own width, so it starts on the view's left edge
        // rather than beside it; its height is the view's, not the window's.
        let placement = OverlayPlacement::below(Anchor::Node(self.id))
            .side(Side::Left)
            .offset(-OPEN_PANE_W)
            .flip(false)
            .match_anchor_height(true);
        let corners = Corners::ZERO;
        let mut pane = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(OPEN_PANE_W),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .corners(corners)
            // The acrylic pass shades this box; the box itself only carries the shape.
            .background(Color::TRANSPARENT, corners)
            .border(self.pane_stroke, CONTENT_BORDER)
            .overlay(placement)
            // Asking for focus is what makes this the light-dismiss target while it is open.
            .sense(Sense::FOCUSABLE | Sense::HOVER)
            .shortcut(
                KeyChord::new(Key::Escape, Modifiers::NONE),
                NAV_DISMISS_SHORTCUT,
            );
        if let Some(p) = pane.data.params_mut() {
            p.backdrop_blur = Some(BackdropBlur {
                radius: (3.0_f32 * self.blur_sigma).ceil().max(1.0) as u32,
                sigma: self.blur_sigma,
                material: BackdropMaterial::Acrylic(self.acrylic),
            });
        }
        pane = pane.child(self.header_decl(self.id.with("overlay-toggle")));
        for item in &self.items {
            pane = pane.child(self.item_decl(item, true));
        }
        pane
    }
}

/// The ring around a destination: it wraps the item's 4,2 margin, as a menu row's does.
fn item_focus(pack: &Style, radius: f32) -> FocusVisual {
    FocusVisual {
        outer: role_of(pack, "FocusStrokeColorOuterBrush"),
        outer_width: 2.0,
        inner: role_of(pack, "FocusStrokeColorInnerBrush"),
        inner_width: 1.0,
        margin: Insets {
            left: -NAV_ITEM_MARGIN.left,
            top: -NAV_ITEM_MARGIN.top,
            right: -NAV_ITEM_MARGIN.right,
            bottom: -NAV_ITEM_MARGIN.bottom,
        },
        radius: radius + NAV_ITEM_MARGIN.left,
    }
}

/// The pane toggle's ring: the same vertical reach, the toggle's own corner.
fn toggle_focus(pack: &Style, radius: f32) -> FocusVisual {
    FocusVisual {
        outer: role_of(pack, "FocusStrokeColorOuterBrush"),
        outer_width: 2.0,
        inner: role_of(pack, "FocusStrokeColorInnerBrush"),
        inner_width: 1.0,
        margin: Insets::new(0.0, -TOGGLE_MARGIN.top, 0.0, -TOGGLE_MARGIN.bottom),
        radius: radius + TOGGLE_MARGIN.top,
    }
}

impl FluentNavigationView {
    /// The whole view, with `content` as the app's page.
    fn assemble(&self, content: Vec<Box<dyn Element>>) -> Container {
        let id = self.id;
        let fill = || AxisSize::fill(0.0, 0.0);
        let mut root = if self.mode == PaneDisplayMode::Minimal {
            // No strip at all: the toggle gets a header of its own above the content.
            Container::new(id, NodeKind::Column)
                .size(SizeClass {
                    width: fill(),
                    height: fill(),
                    grow_x: Grow::Yes,
                    grow_y: Grow::Yes,
                })
                .child_decl(self.header_decl(id.with("pane-toggle")))
                .child(Box::new(self.content_node(content)))
        } else {
            let mut pane = Decl::container(id.with("pane"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fixed(self.width),
                    height: fill(),
                    grow_x: Grow::No,
                    grow_y: Grow::Yes,
                })
                .child(self.header_decl(id.with("pane-toggle")));
            // The open overlay covers the strip, so its destinations leave the tree: two
            // nodes cannot share an id, and a hidden row would still answer for clicks.
            if !self.overlay {
                // A group title needs room for its text, which a 48 dp strip has not.
                for item in self
                    .items
                    .iter()
                    .filter(|it| self.strip_labels || !it.header)
                {
                    pane = pane.child(self.item_decl(item, self.strip_labels));
                }
            }
            Container::new(id, NodeKind::Row)
                .size(SizeClass {
                    width: fill(),
                    height: fill(),
                    grow_x: Grow::Yes,
                    grow_y: Grow::Yes,
                })
                .child_decl(pane)
                .child(Box::new(self.content_node(content)))
        };
        if self.overlay {
            root = root.child_decl(self.overlay_decl());
        }
        root
    }
}

impl Element for FluentNavigationView {
    fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        self.root.decl(cx)
    }

    fn children(&self) -> &[Box<dyn Element>] {
        Element::children(&self.root)
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        // The toggle reports the state it would settle into; the app still owns `open`.
        if self.toggle {
            return Some(Change::Toggled {
                on: Some(!self.open),
            });
        }
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        // A press outside the open pane, or Escape, closes it; the app owns `open`.
        self.dismissed.then_some(Change::Dismissed)
    }

    fn cache_key(&self) -> Option<ui_core::memo::CachedKey> {
        // `decl` clones a container the app composed; its inputs are not fingerprinted yet.
        None
    }
}
