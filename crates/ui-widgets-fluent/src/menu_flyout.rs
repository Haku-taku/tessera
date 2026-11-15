//! The Fluent menu flyout: a command list on acrylic, hung from its anchor.

use ui_core::BackdropBlur;
use ui_core::draw::{Acrylic, BackdropMaterial, FocusVisual};
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{Anchor, AxisSize, Grow, OverlayPlacement, Presentation, Side, SizeClass};
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::{Change, Decl, Id, Key, Modifiers, NodeKind, Sense};
use ui_style::{ControlKind, Density, Style, TextRole};
use ui_widgets::text::Text;
use ui_widgets::{Cx, Icon, IconSource, Widget};

use crate::material::{AcrylicRecipe, acrylic_dark, acrylic_light};
use crate::paint::role_of;
use crate::popup::{PopupMotion, popup_motion, popup_window};

/// `MenuFlyoutPresenterThemePadding`: the scroll viewer's margin inside the border.
pub const PRESENTER_PAD: Insets = Insets {
    left: 0.0,
    top: 2.0,
    right: 0.0,
    bottom: 2.0,
};
/// `MenuFlyoutThemeMinHeight` and `OverlayCornerRadius`.
pub const PRESENTER_MIN_HEIGHT: f32 = 32.0;
pub const PRESENTER_RADIUS: f32 = 8.0;
/// `MenuFlyoutPresenterBorderThemeThickness` and `ControlCornerRadius`.
pub const PRESENTER_BORDER: f32 = 1.0;
pub const ITEM_RADIUS: f32 = 4.0;
/// `MenuFlyoutItemMargin`, `MenuFlyoutItemThemePadding` and the touch/narrow variant.
pub const ITEM_MARGIN: Insets = Insets {
    left: 4.0,
    top: 2.0,
    right: 4.0,
    bottom: 2.0,
};
pub const ITEM_PAD: Insets = Insets {
    left: 11.0,
    top: 8.0,
    right: 11.0,
    bottom: 9.0,
};
pub const ITEM_PAD_NARROW: Insets = Insets {
    left: 11.0,
    top: 4.0,
    right: 11.0,
    bottom: 5.0,
};
/// `MenuFlyoutItemPlaceholderThemeThickness`: the text's indent once the menu has icons.
pub const PLACEHOLDER: f32 = 28.0;
/// The `IconRoot` viewbox's side, and the gap the placeholder leaves after it.
pub const ICON_SLOT: f32 = 16.0;
pub const ICON_GAP: f32 = PLACEHOLDER - ICON_SLOT;
/// `MenuFlyoutSeparatorHeight` and `MenuFlyoutSeparatorThemePadding`.
pub const MENU_SEPARATOR_HEIGHT: f32 = 1.0;
pub const MENU_SEPARATOR_PAD: Insets = Insets {
    left: -4.0,
    top: 1.0,
    right: -4.0,
    bottom: 1.0,
};
/// The accelerator's `Margin="24,4,0,0"`, in `CaptionTextBlockStyle`.
pub const ACCEL_GAP: f32 = 24.0;
pub const ACCEL_TOP: f32 = 4.0;
/// Escape closes the flyout, as `MenuFlyout` binds it.
pub const MENU_DISMISS_SHORTCUT: ShortcutId = ShortcutId("menu.dismiss");

/// One row of the menu: a command, or a divider between groups.
#[derive(Clone, Debug)]
pub enum MenuFlyoutEntry {
    Item(MenuFlyoutItem),
    Separator,
}

/// One command: its text, optional icon and accelerator, and whether it can be run.
#[derive(Clone, Debug)]
pub struct MenuFlyoutItem {
    pub id: Id,
    pub label: String,
    pub icon: Option<IconSource>,
    pub accelerator: Option<String>,
    pub disabled: bool,
}

impl MenuFlyoutItem {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            icon: None,
            accelerator: None,
            disabled: false,
        }
    }

    pub fn icon(mut self, icon: IconSource) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn accelerator(mut self, text: impl Into<String>) -> Self {
        self.accelerator = Some(text.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// Everything the flyout is built with, named after the properties the reference exposes.
pub struct MenuFlyoutParams<'a> {
    pub id: Id,
    /// The node the panel unfolds from: `FlyoutBase.Target`.
    pub anchor: Anchor,
    /// `FlyoutBase.Placement`; the reference flips to the opposite side at an edge.
    pub side: Side,
    pub items: &'a [MenuFlyoutEntry],
    /// `IsOpen`; a closed flyout paints nothing and takes no keyboard.
    pub open: bool,
    /// The narrow padding is the reference's pen/mouse/keyboard variant; touch keeps the
    /// default. `None` means "not told", which takes the default.
    pub narrow: Option<bool>,
}

impl<'a> MenuFlyoutParams<'a> {
    pub fn new(id: Id, anchor: Anchor, items: &'a [MenuFlyoutEntry]) -> Self {
        Self {
            id,
            anchor,
            side: Side::Below,
            items,
            open: true,
            narrow: None,
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

    pub fn narrow(mut self, narrow: bool) -> Self {
        self.narrow = Some(narrow);
        self
    }
}

use crate::spec::{DISABLED, POINTER_OVER, PRESSED, REST};

/// The brush names each state column swaps in, transcribed from the item template.
struct Brushes {
    fill: [&'static str; 4],
    ink: [&'static str; 4],
    accel: [&'static str; 4],
}

const ITEM_BRUSHES: Brushes = Brushes {
    fill: [
        "MenuFlyoutItemBackground",
        "MenuFlyoutItemBackgroundPointerOver",
        "MenuFlyoutItemBackgroundPressed",
        "MenuFlyoutItemBackgroundDisabled",
    ],
    ink: [
        "MenuFlyoutItemForeground",
        "MenuFlyoutItemForegroundPointerOver",
        "MenuFlyoutItemForegroundPressed",
        "MenuFlyoutItemForegroundDisabled",
    ],
    accel: [
        "MenuFlyoutItemKeyboardAcceleratorTextForeground",
        "MenuFlyoutItemKeyboardAcceleratorTextForegroundPointerOver",
        "MenuFlyoutItemKeyboardAcceleratorTextForegroundPressed",
        "MenuFlyoutItemKeyboardAcceleratorTextForegroundDisabled",
    ],
};

/// One resolved item: the facts `decl` needs, with the pack already read.
struct Entry {
    id: Id,
    label: String,
    icon: Option<IconSource>,
    accelerator: Option<String>,
    disabled: bool,
    separator: bool,
}

/// A menu flyout, resolved against the pack in force where it was built.
pub struct FluentMenuFlyout {
    id: Id,
    entries: Vec<Entry>,
    anchor: Anchor,
    side: Side,
    open: bool,
    /// Whether the menu holds any icon, which indents every label by the placeholder.
    placeholder: bool,
    pad: Insets,
    surface: Color,
    stroke: Color,
    separator: Color,
    /// The four state columns, resolved once: `decl` picks one by this frame's state.
    fill: [Color; 4],
    ink: [Color; 4],
    accel_ink: [Color; 4],
    label_size: f32,
    accel_size: f32,
    acrylic: Acrylic,
    blur_sigma: f32,
    focus: FocusVisual,
    /// The item whose row took this frame's click or Enter.
    fired: Option<Id>,
    /// This frame's entrance, and whether the panel is still on stage while closing.
    motion: PopupMotion,
}

impl FluentMenuFlyout {
    pub fn new(params: MenuFlyoutParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let MenuFlyoutParams {
            id,
            anchor,
            side,
            items,
            open,
            narrow,
        } = params;
        let m = pack
            .controls
            .get(ControlKind::MenuFlyoutItem, Density::Standard);
        let entries: Vec<Entry> = items
            .iter()
            .enumerate()
            .map(|(index, entry)| match entry {
                MenuFlyoutEntry::Item(item) => Entry {
                    id: item.id,
                    label: item.label.clone(),
                    icon: item.icon,
                    accelerator: item.accelerator.clone(),
                    disabled: item.disabled,
                    separator: false,
                },
                MenuFlyoutEntry::Separator => Entry {
                    id: id.with(("separator", index)),
                    label: String::new(),
                    icon: None,
                    accelerator: None,
                    disabled: false,
                    separator: true,
                },
            })
            .collect();
        let placeholder = entries.iter().any(|e| e.icon.is_some());
        let fired = entries
            .iter()
            .find(|e| {
                let st = cx.state(e.id);
                st.activation()
            })
            .map(|e| e.id);
        // `AcrylicBrush_themeresources.xaml`: the flyout's own backdrop, in-app on our surface.
        let recipe: AcrylicRecipe = match pack.appearance() {
            Some(ui_design::Appearance::Dark) => acrylic_dark(),
            _ => acrylic_light(),
        };
        // `MenuFlyoutItemMargin` sits inside the control, so the ring wraps the row's margin too.
        let focus = FocusVisual {
            outer: role_of(pack, "FocusStrokeColorOuterBrush"),
            outer_width: 2.0,
            inner: role_of(pack, "FocusStrokeColorInnerBrush"),
            inner_width: 1.0,
            margin: Insets {
                left: -ITEM_MARGIN.left,
                top: -ITEM_MARGIN.top,
                right: -ITEM_MARGIN.right,
                bottom: -ITEM_MARGIN.bottom,
            },
            radius: 0.0,
        };
        // The panel unfolds toward its trigger; the window keeps it clipped to its final rect.
        let motion = popup_motion(cx, id, anchor, side, open);
        Self {
            id,
            entries,
            anchor,
            side,
            open,
            placeholder,
            pad: match narrow {
                Some(true) => ITEM_PAD_NARROW,
                _ => ITEM_PAD,
            },
            surface: role_of(pack, "MenuFlyoutPresenterBackground"),
            stroke: role_of(pack, "MenuFlyoutPresenterBorderBrush"),
            separator: role_of(pack, "MenuFlyoutSeparatorBackground"),
            fill: ITEM_BRUSHES.fill.map(|n| role_of(pack, n)),
            ink: ITEM_BRUSHES.ink.map(|n| role_of(pack, n)),
            accel_ink: ITEM_BRUSHES.accel.map(|n| role_of(pack, n)),
            label_size: pack.type_table.get(m.font).size,
            accel_size: pack.type_table.get(TextRole::Caption).size,
            acrylic: Acrylic {
                tint: recipe.tint,
                luminosity: recipe.luminosity,
                noise: recipe.noise,
            },
            blur_sigma: recipe.blur_sigma,
            focus,
            fired,
            motion,
        }
    }

    /// One command row: icon slot, label, accelerator, on the state's fill.
    fn item_decl(&self, cx: &Cx<'_, '_>, entry: &Entry) -> Decl {
        let st = cx.state(entry.id);
        let slot = if entry.disabled || st.disabled {
            DISABLED
        } else if st.pressed {
            PRESSED
        } else if st.hovered {
            POINTER_OVER
        } else {
            REST
        };
        let mut row = Decl::container(entry.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .margin(ITEM_MARGIN)
            .padding(self.pad)
            .background(self.fill[slot], Corners::all(ITEM_RADIUS))
            // A closing menu takes nothing: the row is inert while the surface fades out.
            .sense(match self.open {
                true => Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE,
                false => Sense::NONE,
            })
            .cursor(ui_core::CursorShape::Arrow)
            .disabled(entry.disabled)
            .focus_visual(Some(self.focus))
            .cross_align(0.0)
            .cross_pos(0.5);
        // The placeholder holds the icon column open even for rows without an icon, so the
        // labels line up: that is the whole of `IconPlaceholder` in the template.
        if self.placeholder {
            let mut slot_decl = Decl::container(entry.id.with("icon"), NodeKind::Box)
                .size(SizeClass::fixed(ICON_SLOT, ICON_SLOT));
            if let Some(source) = entry.icon {
                slot_decl = slot_decl.child(
                    Icon {
                        id: entry.id.with("glyph"),
                        source,
                        size: ICON_SLOT,
                        ink: self.ink[slot],
                    }
                    .resolve(),
                );
            }
            row = row.child(slot_decl);
        }
        let indent = if self.placeholder { ICON_GAP } else { 0.0 };
        let mut label = Text::new(
            entry.id.with("label"),
            entry.label.clone(),
            self.label_size,
            self.ink[slot],
        )
        .resolve();
        label.size_class.grow_x = Grow::Yes;
        label = label.margin(Insets {
            left: indent,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        });
        row = row.child(label);
        if let Some(accel) = &entry.accelerator {
            row = row.child(
                Text::new(
                    entry.id.with("accel"),
                    accel.clone(),
                    self.accel_size,
                    self.accel_ink[slot],
                )
                .resolve()
                .margin(Insets {
                    left: ACCEL_GAP,
                    top: ACCEL_TOP,
                    right: 0.0,
                    bottom: 0.0,
                }),
            );
        }
        row
    }

    /// One divider: `MenuFlyoutSeparatorHeight` on `MenuFlyoutSeparatorBackground`.
    fn separator_decl(&self, entry: &Entry) -> Decl {
        Decl::container(entry.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(MENU_SEPARATOR_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .margin(MENU_SEPARATOR_PAD)
            .background(self.separator, Corners::ZERO)
    }
}

impl Widget for FluentMenuFlyout {
    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<Change> {
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        // Escape is bound on the overlay root, so it reaches here only while the menu has keys.
        if state.dismissed || state.shortcut == Some(MENU_DISMISS_SHORTCUT) {
            return Some(Change::Dismissed);
        }
        None
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let id = self.id;
        // The overlay root carries the widget's own id: the engine attributes a dismissal and
        // the Escape chord to that node, so it is the id the app sees.
        let window = id;
        let panel_id = id.with("panel");
        // A closed flyout is not an overlay at all: leaving one would make it the keyboard
        // scope and light-dismiss target of a panel nobody can see.
        if !self.motion.alive {
            return Decl::container(window, NodeKind::Box)
                .absolute()
                .size(SizeClass::fixed(0.0, 0.0));
        }
        let placement = OverlayPlacement {
            anchor: self.anchor,
            side: self.side,
            align: ui_core::layout::OverlayAlign::Start,
            offset: 0.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Parent,
        };
        let corners = Corners::all(PRESENTER_RADIUS);
        let mut panel = Decl::container(panel_id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(PRESENTER_MIN_HEIGHT, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(PRESENTER_PAD)
            .corners(corners)
            // The acrylic pass shades this box; without a box shape there is nothing to shade.
            .background(self.surface, corners)
            .border(self.stroke, PRESENTER_BORDER);
        if let Some(p) = panel.data.params_mut() {
            p.backdrop_blur = Some(BackdropBlur {
                radius: (3.0_f32 * self.blur_sigma).ceil().max(1.0) as u32,
                sigma: self.blur_sigma,
                material: BackdropMaterial::Acrylic(self.acrylic),
            });
        }
        for entry in &self.entries {
            panel = if entry.separator {
                panel.child(self.separator_decl(entry))
            } else {
                panel.child(self.item_decl(cx, entry))
            };
        }
        // The window holds the final rect and clips the panel sliding inside it; asking for
        // focus there is what makes the flyout the keyboard scope while it is open.
        let mut window = popup_window(window)
            .overlay(placement)
            .sense(match self.open {
                true => Sense::FOCUSABLE | Sense::HOVER,
                false => Sense::NONE,
            });
        if self.open {
            window = window.shortcut(
                KeyChord::new(Key::Escape, Modifiers::NONE),
                MENU_DISMISS_SHORTCUT,
            );
        }
        window.child(self.motion.apply(panel))
    }
}
