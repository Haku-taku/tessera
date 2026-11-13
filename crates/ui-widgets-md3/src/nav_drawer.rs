//! MD3 navigation drawer: a 360dp surface, 336×56 destination capsules inset 12dp, and
//! `title-small` headlines that group them. A standard drawer is a column the page lays
//! out; a modal drawer is a surface over a scrim, opened and closed by the app.

use core::hash::Hasher;
use ui_core::collections::FastHasher;
use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Point, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::{ModalTarget, Presence, ShadowSpec};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Id, InkRadius, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::fingerprint::Fingerprint;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::Background;
use crate::paint::faded;
use crate::paint::{between, role, spec_ink};
use crate::spec::{NavigationDrawerFlags, navigation_drawer_spec};

use crate::motion::{DESTINATION_SELECT, REACTION};
/// `tilePadding: EdgeInsets.symmetric(horizontal: 12)`, which leaves the 336dp capsule.
const TILE_INSET: f32 = 12.0;
/// The destination's own leading inset inside the capsule, from Flutter's destination row.
const TILE_LEADING: f32 = 16.0;
/// `SizedBox(width: 12)` between the mark and the label.
const ICON_GAP: f32 = 12.0;
/// The export carries only the headline's type style; this row's vertical rhythm is ours.
const HEADLINE_PAD: (f32, f32) = (16.0, 8.0);
/// `DrawerDefaults.scrimColor`: black at 32%, so the page behind stays readable.
const SCRIM_OPACITY: f32 = 0.32;
/// Material's pair for a surface entering from an edge: emphasized decelerate in,
/// accelerate out, because the entrance is the one the reader follows.
const ENTER: AnimSpec = AnimSpec::new(0.3, ui_style::motion::EMPHASIZED_DECELERATE);
const LEAVE: AnimSpec = AnimSpec::new(0.2, crate::motion::EMPHASIZED_ACCELERATE);

/// One drawer entry: a group headline or a destination.
pub enum DrawerEntry<'a> {
    /// A `title-small` group label.
    Headline(&'a str),
    /// A destination; the app owns the id it is selected by.
    Item(DrawerItem<'a>),
}

/// One destination: its id, label and optional 24dp mark.
pub struct DrawerItem<'a> {
    pub id: Id,
    pub label: &'a str,
    /// Built at the destination's icon id in the ink the current state resolves to.
    pub icon: Option<IconSource>,
}

impl<'a> DrawerItem<'a> {
    /// A destination with a mark and a label.
    pub fn new(id: Id, label: &'a str, icon: IconSource) -> Self {
        Self {
            id,
            label,
            icon: Some(icon),
        }
    }

    /// A text-only destination.
    pub fn text(id: Id, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
        }
    }
}

/// How the drawer relates to the page it is declared in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrawerMode {
    /// A column the page lays out beside its content.
    #[default]
    Standard,
    /// A surface over a scrim across the window; a press outside it asks to close.
    Modal,
}

/// Everything a drawer needs besides the design system's own tokens.
pub struct DrawerParams<'a> {
    pub id: Id,
    pub entries: &'a [DrawerEntry<'a>],
    /// The selected destination's id; the app owns it.
    pub selected: Option<Id>,
    pub disabled: bool,
    pub mode: DrawerMode,
}

/// One destination's resolved paint data.
struct Row {
    id: Id,
    label: String,
    icon: Option<IconSource>,
    icon_ink: Color,
    label_ink: Color,
    label_weight: Weight,
    layer: Option<Color>,
    /// The press ink's colour; `None` when the drawer is disabled.
    press: Option<Color>,
    ring: f32,
    indicator: f32,
}

/// One headline's resolved paint data.
struct Headline {
    id: Id,
    text: String,
    ink: Color,
}

/// A slot in visual order: the layout and the hit test walk the same list.
enum Slot {
    Headline(Headline),
    Row(Row),
}

/// A standard navigation drawer; the app owns which destination is selected.
pub struct Md3NavDrawer {
    id: Id,
    slots: Vec<Slot>,
    selected: Option<Id>,
    content_h: f32, // total content height for scroll bounds
    width: f32,
    capsule_w: f32,
    capsule_h: f32,
    indicator_ink: Color,
    label_size: f32,
    headline_size: f32,
    headline_h: f32,
    headline_weight: Weight,
    container: Color,
    ring_colour: Color,
    ring_width: f32,
    disabled: bool,
    /// Whether this drawer covers the page behind a scrim.
    modal: bool,
    shadow: Option<ShadowSpec>,
    scrim: Color,
    /// A press landed on the scrim instead of the surface: the app closes the drawer.
    dismissed: bool,
}

impl Md3NavDrawer {
    /// Same control with explicit parameters, so the semantic path and a direct caller
    /// produce identical `Decl`s.
    pub fn new_with(
        params: Md3Options,
        p: DrawerParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let (modal, standard) = match p.mode {
            DrawerMode::Modal => (true, false),
            DrawerMode::Standard => (false, true),
        };
        let base = navigation_drawer_spec(NavigationDrawerFlags {
            modal,
            standard,
            ..NavigationDrawerFlags::NONE
        });
        let disabled = params.disabled || cx.state(p.id).disabled;
        let fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };
        let layer_ink = role(
            pack,
            navigation_drawer_spec(NavigationDrawerFlags {
                modal,
                standard,
                hovered: true,
                ..NavigationDrawerFlags::NONE
            })
            .state_layer
            .color,
        );
        let hover_a = navigation_drawer_spec(NavigationDrawerFlags {
            modal,
            standard,
            hovered: true,
            ..NavigationDrawerFlags::NONE
        })
        .state_layer
        .opacity;
        let focus_a = navigation_drawer_spec(NavigationDrawerFlags {
            modal,
            standard,
            focused: true,
            ..NavigationDrawerFlags::NONE
        })
        .state_layer
        .opacity;
        let press_a = navigation_drawer_spec(NavigationDrawerFlags {
            modal,
            standard,
            pressed: true,
            ..NavigationDrawerFlags::NONE
        })
        .state_layer
        .opacity;
        let mut slots = Vec::new();
        for (k, entry) in p.entries.iter().enumerate() {
            match entry {
                DrawerEntry::Headline(text) => slots.push(Slot::Headline(Headline {
                    id: p.id.with(("headline", k)),
                    text: text.to_string(),
                    ink: spec_ink(pack, base.headline.color, 1.0),
                })),
                DrawerEntry::Item(item) => {
                    let item_id = item.id;
                    let mut node = cx.at(item_id);
                    let state = node.state();
                    let on = navigation_drawer_spec(NavigationDrawerFlags {
                        active: true,
                        modal,
                        standard,
                        hovered: state.hovered,
                        focused: state.focus_visible,
                        pressed: state.pressed,
                    });
                    let off = navigation_drawer_spec(NavigationDrawerFlags {
                        modal,
                        standard,
                        hovered: state.hovered,
                        focused: state.focus_visible,
                        pressed: state.pressed,
                        ..NavigationDrawerFlags::NONE
                    });
                    let sel = node.animate(
                        item_id.with(param::PROGRESS),
                        f32::from(p.selected == Some(item_id)),
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
                    // The ripple belongs to the capsule, not the whole row, and it is cut to
                    // the capsule's fixed slot: an unselected row still has one.
                    let press = (!disabled).then_some(Color {
                        a: layer_ink.a * press_a,
                        ..layer_ink
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
                    slots.push(Slot::Row(Row {
                        id: item_id,
                        label: item.label.to_string(),
                        icon: item.icon,
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
                            fade,
                        ),
                        label_weight: Weight(if p.selected == Some(item_id) {
                            on.label_text.weight as u16
                        } else {
                            off.label_text.weight as u16
                        }),
                        layer,
                        press,
                        ring,
                        indicator: if disabled { 0.0 } else { sel },
                    }));
                }
            }
        }
        let capsule_h = base.active_indicator.height;
        let headline_row_h = base.headline.line_height + HEADLINE_PAD.0 + HEADLINE_PAD.1;
        let content_h = slots
            .iter()
            .map(|slot| match slot {
                Slot::Headline(_) => headline_row_h,
                Slot::Row(_) => capsule_h,
            })
            .sum();
        Self {
            id: p.id,
            slots,
            selected: p.selected,
            content_h,
            width: base.container.width,
            capsule_w: base.active_indicator.width,
            capsule_h,
            indicator_ink: spec_ink(pack, base.active_indicator.color, 1.0),
            label_size: base.label_text.size,
            headline_size: base.headline.size,
            headline_h: base.headline.line_height,
            headline_weight: Weight(base.headline.weight as u16),
            container: role(pack, base.container.color),
            ring_colour: spec_ink(pack, base.focus_indicator.color, 1.0),
            ring_width: base.focus_indicator.thickness,
            disabled,
            dismissed: modal && cx.state(p.id).clicked && !cx.state(p.id.with("surface")).clicked,
            modal,
            shadow: (base.container.elevation > 0.0).then(|| {
                ShadowSpec::black(
                    base.container.elevation * 2.0,
                    0.18,
                    base.container.elevation * 0.5,
                )
            }),
            scrim: spec_ink(pack, "scrim", SCRIM_OPACITY),
        }
    }

    /// Total content height, for the app's scroll-boundary clamp.
    pub fn content_h(&self) -> f32 {
        self.content_h
    }

    /// The drawer's own width, so a caller can reserve a column for it.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Which destination a point in the drawer's own space names, walking the visual order.
    ///
    /// `local_y` must already be shifted by the scroll offset: the widget does not apply its own scroll transform.
    /// `height` is the laid-out visible column; the scroller clips below it, so nothing below must hit.
    pub fn row_at(&self, local: Point, width: f32, height: f32) -> Option<Id> {
        // Above the column the scroller may expose content scrolled in; below it, the next
        // slot is the one already clipped away. Both must miss.
        if width <= 0.0 || local.y < 0.0 || local.y >= height {
            return None;
        }
        let mut top = TILE_INSET;
        for slot in &self.slots {
            let h = match slot {
                Slot::Headline(_) => self.headline_row_h(),
                Slot::Row(_) => self.capsule_h,
            };
            if local.y >= top && local.y < top + h {
                return match slot {
                    Slot::Row(row) => Some(row.id),
                    Slot::Headline(_) => None,
                };
            }
            top += h;
        }
        None
    }

    /// A headline's whole row: its line plus the vertical rhythm above and below.
    fn headline_row_h(&self) -> f32 {
        self.headline_h + HEADLINE_PAD.0 + HEADLINE_PAD.1
    }

    /// One destination: its capsule, state layer, ring, mark and label.
    fn row_decl(&self, row: &Row) -> Decl {
        let w = self.capsule_w * row.indicator;
        let mut shapes = Vec::with_capacity(3);
        // Always first, so `Shape(0)` names the capsule whatever else is showing, and the
        // capsule fades in with its width instead of appearing at full strength.
        shapes.push(Shape::Rect {
            rect: Rect::new((self.capsule_w - w) * 0.5, 0.0, w, self.capsule_h),
            corners: Corners::all(w.min(self.capsule_h) * 0.5),
            fill: Color {
                a: self.indicator_ink.a * row.indicator,
                ..self.indicator_ink
            },
            gradient: None,
            border: None,
        });
        // The hover and focus layers follow the capsule, not the row's rectangle: a square
        // wash around a stadium reads as a different control.
        let background = Background::stadium(self.capsule_w, self.capsule_h);
        if let Some(layer) = row.layer {
            shapes.push(background.fill(layer));
        }
        if row.ring > 0.0 {
            let ring = Color {
                a: self.ring_colour.a * row.ring,
                ..self.ring_colour
            };
            shapes.push(background.ring(ring, self.ring_width));
        }
        let mut node = Decl::container(row.id, NodeKind::Row)
            .size(SizeClass::fixed(self.capsule_w, self.capsule_h))
            .padding(Insets {
                left: TILE_LEADING,
                right: TILE_LEADING,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(ICON_GAP)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(row.press.map(|ink| background.press(ink, InkRadius::Cover)))
            .shapes(shapes);
        if let Some(source) = row.icon {
            // MD3 fills a destination's icon while it is active; the indicator's own curve
            // drives the axis, so the fill arrives with the capsule instead of after it.
            let filled = source.fill(row.indicator);
            node = node.child(filled.resolve(row.id.with("icon"), row.icon_ink));
        }
        node.child(
            Text::new(
                row.id.with("label"),
                row.label.clone(),
                self.label_size,
                row.label_ink,
            )
            .weight(row.label_weight)
            .resolve(),
        )
    }

    /// A group headline: full width, inset so it lines up with the capsules' labels.
    fn headline_decl(&self, h: &Headline) -> Decl {
        Decl::container(h.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.headline_row_h()),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: TILE_INSET + TILE_LEADING,
                right: 0.0,
                top: HEADLINE_PAD.0,
                bottom: HEADLINE_PAD.1,
            })
            .cross_align(0.0)
            .cross_pos(0.0)
            .child(
                Text::new(h.id.with("text"), h.text.clone(), self.headline_size, h.ink)
                    .weight(self.headline_weight)
                    .resolve(),
            )
    }
}

impl Fingerprint for Slot {
    fn fingerprint(&self, h: &mut FastHasher) {
        match self {
            Self::Headline(headline) => {
                h.write_u8(0);
                headline.id.fingerprint(h);
                headline.text.fingerprint(h);
                headline.ink.fingerprint(h);
            }
            Self::Row(row) => {
                h.write_u8(1);
                row.id.fingerprint(h);
                row.label.fingerprint(h);
                row.icon.fingerprint(h);
                row.icon_ink.fingerprint(h);
                row.label_ink.fingerprint(h);
                row.label_weight.fingerprint(h);
                row.layer.fingerprint(h);
                row.press.fingerprint(h);
                row.ring.fingerprint(h);
                row.indicator.fingerprint(h);
            }
        }
    }
}

impl Widget for Md3NavDrawer {
    fn handles_disabled(&self) -> bool {
        // The 38% content fade is painted here, so the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        if self.disabled {
            return None;
        }
        if self.dismissed || state.dismissed {
            return Some(Change::Dismissed);
        }
        // The press point names the destination; the keyboard has none (engine-audit F12).
        if !state.clicked {
            return None;
        }
        let id = self.row_at(state.local?, bounds.size.w, bounds.size.h)?;
        // Re-selecting the destination already selected is not a change.
        (self.selected != Some(id)).then(|| Change::Selected(Some(id)))
    }

    fn cache_key(&self) -> Option<ui_core::CachedKey> {
        // Every field `decl` and its row and headline builders read. `headline_h`,
        // `content_h` and `selected` are not here: the first two are layout, and the
        // selection is already baked into the rows' inks and indicator.
        let fp = ui_widgets::fingerprint!(
            self.id,
            self.slots,
            self.width,
            self.capsule_w,
            self.capsule_h,
            self.indicator_ink,
            self.label_size,
            self.headline_size,
            self.headline_weight,
            self.container,
            self.ring_colour,
            self.ring_width,
            self.disabled,
            self.modal,
            self.scrim,
        );
        Some(ui_core::CachedKey::new(self.id, fp))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        // With a scrim the widget is the barrier and the surface is a child of it, the way
        // a modal dialog nests its surface; without one the drawer is the widget itself.
        let surface_id = if self.modal {
            self.id.with("surface")
        } else {
            self.id
        };
        let mut root = Decl::container(surface_id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            })
            .padding(Insets {
                left: 0.0,
                right: 0.0,
                top: TILE_INSET,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .shapes(vec![Shape::Box {
                // `corner.large.end`: the drawer rounds its trailing edge by 16dp.
                corners: Corners {
                    tl: 0.0,
                    tr: 16.0,
                    br: 16.0,
                    bl: 0.0,
                    ..Corners::ZERO
                },
                fill: self.container,
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }]);
        for slot in &self.slots {
            root = match slot {
                Slot::Headline(h) => root.child(self.headline_decl(h)),
                Slot::Row(row) => root.child(self.row_decl(row)),
            };
        }
        if !self.modal {
            return root;
        }
        // The surface travels in from its own edge while the scrim fades under it.
        root = root
            .enter(Presence::slide(ENTER, 0.0, Point::new(-self.width, 0.0)))
            .exit(Presence::slide(LEAVE, 0.0, Point::new(-self.width, 0.0)));
        let mut barrier = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass::hug(0.0, 0.0))
            .modal(ModalTarget::Global)
            .cross_align(0.0)
            .cross_pos(0.0)
            .sense(Sense::CLICK)
            .cursor(CursorShape::Arrow)
            .disabled(self.disabled)
            .shapes(vec![Shape::Box {
                corners: Corners::ZERO,
                fill: self.scrim,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }])
            .enter(Presence::fade(ENTER, 0.0))
            .exit(Presence::fade(LEAVE, 0.0));
        barrier = barrier.child(root);
        barrier
    }
}
