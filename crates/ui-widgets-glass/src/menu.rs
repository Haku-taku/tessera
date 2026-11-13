//! The glass menu: a frosted panel of rows hung from its anchor.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{Anchor, AxisSize, Grow, OverlayAlign, Side, SizeClass};
use ui_core::shortcut::ShortcutId;
use ui_core::{CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::{Style, TextRole};
use ui_widgets::{Cx, IconSource, Widget};

use crate::HigOptions;
use crate::material::Control;
use crate::overlay::{Panel, PanelParams, panel_dismissed};
use crate::paint::*;
use crate::shared::focus_ring;
use crate::text::LabelText;

/// One row of a glass menu, or a divider between rows.
#[derive(Clone, Debug)]
pub struct GlassMenuEntry<'a> {
    pub id: Id,
    pub label: &'a str,
    pub icon: Option<IconSource>,
    pub disabled: bool,
    pub destructive: bool,
    /// A divider instead of a row; `label` is then ignored.
    pub divider: bool,
}

impl<'a> GlassMenuEntry<'a> {
    pub fn item(id: Id, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
            disabled: false,
            destructive: false,
            divider: false,
        }
    }

    pub fn divider(id: Id) -> Self {
        Self {
            id,
            label: "",
            icon: None,
            disabled: false,
            destructive: false,
            divider: true,
        }
    }

    pub fn icon(mut self, icon: IconSource) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// Everything a menu is built with, named after the reference's properties.
pub struct GlassMenuParams<'a> {
    pub id: Id,
    /// The node the panel unfolds from.
    pub anchor: Anchor,
    pub side: Side,
    pub entries: &'a [GlassMenuEntry<'a>],
    /// `IsOpen`; a closed menu paints nothing and takes no keyboard.
    pub open: bool,
}

impl<'a> GlassMenuParams<'a> {
    pub fn new(id: Id, anchor: Anchor, entries: &'a [GlassMenuEntry<'a>]) -> Self {
        Self {
            id,
            anchor,
            side: Side::Below,
            entries,
            open: true,
        }
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
}

/// A resolved row: the facts `decl` needs, with the pack and this frame's state read.
struct Row {
    id: Id,
    label: Option<LabelText>,
    icon: Option<IconSource>,
    ink: Color,
    /// The hover/press fill for this frame.
    fill: Color,
    disabled: bool,
    divider: bool,
}

/// A glass menu, resolved against the pack in force where it was built.
pub struct GlassMenu {
    panel: Panel,
    rows: Vec<Row>,
    /// The row activated this frame, if any.
    fired: Option<Id>,
    divider: Color,
    /// The ring a row paints while the keyboard is on it; also what makes it a Tab stop.
    focus: ui_core::draw::FocusVisual,
}

impl GlassMenu {
    /// Takes the build `Cx`, not a `NodeCx`: every row reports from its own node, so the
    /// menu reads its children's state as well as advancing its own springs.
    pub fn new(params: GlassMenuParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        Self::new_with(HigOptions::default(), params, cx, pack)
    }

    pub fn new_with(
        _params: HigOptions,
        params: GlassMenuParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let GlassMenuParams {
            id,
            anchor,
            side,
            entries,
            open,
        } = params;
        let ink = role_of(pack, "textFillColorPrimary");
        let destructive = role_of(pack, "systemRed");
        let hover = Color {
            a: 0.10,
            ..Color::WHITE
        };
        let press = Color {
            a: 0.15,
            ..Color::WHITE
        };
        let rows: Vec<Row> = entries
            .iter()
            .map(|entry| {
                let st = cx.state(entry.id);
                let row_ink = if entry.destructive { destructive } else { ink };
                Row {
                    id: entry.id,
                    label: (!entry.divider)
                        .then(|| {
                            let text_ink = if entry.destructive {
                                row_ink
                            } else {
                                // A row's title sits at 90 % of the label ink (`:257-262`).
                                Color {
                                    a: row_ink.a * 0.9,
                                    ..row_ink
                                }
                            };
                            LabelText::new(
                                pack,
                                TextRole::Body,
                                entry.label,
                                text_ink,
                                f32::INFINITY,
                            )
                            .map(|l| l.restyled(MENU_FONT, Weight::REGULAR))
                        })
                        .flatten(),
                    icon: entry.icon,
                    ink: row_ink,
                    fill: menu_row_fill(st.hovered, st.pressed, hover, press),
                    disabled: entry.disabled,
                    divider: entry.divider,
                }
            })
            .collect();
        // The row that was activated reports from its own node.
        let fired = rows
            .iter()
            .find(|row| {
                let st = cx.state(row.id);
                !row.divider && !row.disabled && st.activation()
            })
            .map(|row| row.id);

        let panel = Panel::new(
            PanelParams {
                id,
                anchor,
                side,
                open,
                width: MENU_W,
                height: None,
                radius: MENU_RADIUS,
                padding: MENU_PAD,
                gap: MENU_ROW_GAP,
                control: Control::Menu,
                dismiss: MENU_DISMISS_SHORTCUT,
            },
            cx,
            pack,
        );
        Self {
            panel,
            rows,
            fired,
            divider: Color { a: 0.12, ..ink },
            focus: focus_ring(pack),
        }
    }
}

impl Widget for GlassMenu {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        if let Some(id) = self.fired {
            return Some(ui_core::Change::Selected(Some(id)));
        }
        // Escape is bound on the panel, so it reaches here only while the menu holds keys.
        panel_dismissed(state, MENU_DISMISS_SHORTCUT).then_some(ui_core::Change::Dismissed)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.panel
            .decl(self.rows.iter().map(|row| self.row_decl(row)).collect())
    }
}

impl GlassMenu {
    /// One row: a 44 dp capsule with an optional icon, or the divider band.
    fn row_decl(&self, row: &Row) -> Decl {
        if row.divider {
            // `GlassMenuDivider`: a 12 dp band with a hairline down its middle.
            let line_w = MENU_W - MENU_PAD.left - MENU_PAD.right - MENU_ROW_PAD_X * 2.0;
            return Decl::container(row.id, NodeKind::Stack)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fixed(MENU_DIVIDER_H),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .stack_align(OverlayAlign::Center)
                .child(
                    Decl::container(row.id.with("line"), NodeKind::Box)
                        .size(SizeClass::fixed(line_w, 0.5))
                        .background(self.divider, Corners::ZERO),
                );
        }
        // A row is a plain row: the title fills what the icon leaves, so it starts at the
        // leading edge as the reference's `Expanded` does.
        let mut node = Decl::container(row.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(MENU_ROW_H, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: MENU_ROW_PAD_X,
                top: MENU_ROW_PAD_Y,
                right: MENU_ROW_PAD_X,
                bottom: MENU_ROW_PAD_Y,
            })
            .corners(Corners::all(MENU_ITEM_RADIUS))
            .background(row.fill, Corners::all(MENU_ITEM_RADIUS))
            .gap(MENU_ICON_GAP)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .focus_visual(Some(self.focus))
            .cursor(CursorShape::PointingHand);
        if let Some(icon) = row.icon {
            node = node.child(
                ui_widgets::Icon {
                    id: row.id.with("icon"),
                    source: icon,
                    size: MENU_ICON,
                    ink: row.ink,
                }
                .resolve(),
            );
        }
        if let Some(label) = &row.label {
            let mut label = label.decl(row.id.with("label"));
            label.size_class.grow_x = Grow::Yes;
            node = node.child(label);
        }
        if row.disabled {
            node = node.opacity(0.4);
        }
        node
    }
}

/// A row's fill for this frame: press over hover, none at rest (`:276-284`).
pub fn menu_row_fill(hovered: bool, pressed: bool, hover: Color, press: Color) -> Color {
    if pressed {
        press
    } else if hovered {
        hover
    } else {
        Color::TRANSPARENT
    }
}

/// The panel's width, `GlassMenu.menuWidth`.
pub const MENU_W: f32 = 200.0;
/// `menuBorderRadius` and `itemBorderRadius`.
pub const MENU_RADIUS: f32 = 32.0;
pub const MENU_ITEM_RADIUS: f32 = 24.0;
/// `_kBodyVerticalPadding` and the horizontal inset the rows sit in.
pub const MENU_PAD: Insets = Insets {
    left: 12.0,
    top: 12.0,
    right: 12.0,
    bottom: 12.0,
};
pub const MENU_ROW_GAP: f32 = 2.0;
/// `GlassMenuItem.height`, its padding, icon size and the gap after the icon.
pub const MENU_ROW_H: f32 = 44.0;
pub const MENU_ROW_PAD_X: f32 = 16.0;
pub const MENU_ROW_PAD_Y: f32 = 8.0;
pub const MENU_ICON: f32 = 20.0;
pub const MENU_ICON_GAP: f32 = 12.0;
/// A row's title is 17 pt at w400.
pub const MENU_FONT: f32 = 17.0;
/// `GlassMenuDivider.height`.
pub const MENU_DIVIDER_H: f32 = 12.0;

/// The id the panel binds Escape to.
pub const MENU_DISMISS_SHORTCUT: ShortcutId = ShortcutId("glass.menu.dismiss");
