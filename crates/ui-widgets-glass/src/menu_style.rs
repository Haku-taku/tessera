//! The HIG look for the engine's context menu, resolved from the glass menu's measurements.
//!
//! The engine owns collection, placement and hit testing; this only says how it paints. A
//! platform panel's surface has nothing behind it to frost, so the glass body is replaced by
//! the pack's raised surface and the menu keeps the glass shape and spacing.

use ui_core::geom::Color;
use ui_style::Style;
use ui_widgets::MenuStyle;

use crate::menu::{
    MENU_FONT, MENU_ICON, MENU_PAD, MENU_RADIUS, MENU_ROW_H, MENU_ROW_PAD_X, MENU_W,
};
use crate::paint::role_of;
use crate::pack::pack_of;

/// The engine menu's look for an appearance, as `PickerOptions::menu_style_of` and a host's
/// `App::menu_style` take it.
pub fn menu_style_of(appearance: ui_design::Appearance) -> MenuStyle {
    menu_style(pack_of(appearance))
}

/// The same look from a resolved pack, so a host drawing in this pack names no appearance.
pub fn menu_style(pack: &Style) -> MenuStyle {
    let ink = role_of(pack, "textFillColorPrimary");
    let opposite = match pack.appearance() {
        Some(ui_design::Appearance::Dark) => Color::BLACK,
        _ => Color::WHITE,
    };
    MenuStyle {
        surface: pack.colour_or(ui_design::CommonColor::SurfaceRaised, opposite.faded(0.06)),
        ink,
        muted: pack.colour_or(ui_design::CommonColor::TextMuted, ink.faded(0.5)),
        // The glass menu's own row fills: a 10 % wash of the ink's opposite.
        hover: Color {
            a: 0.10,
            ..opposite
        },
        border: Some((role_of(pack, "glassRim"), 1.0)),
        font_size: MENU_FONT,
        shortcut_ink: pack.colour_or(ui_design::CommonColor::TextMuted, ink.faded(0.5)),
        shortcut_size: 13.0,
        shortcut_weight: ui_core::Weight::REGULAR,
        shortcut_gap: 16.0,
        trailing_space: MENU_ROW_PAD_X,
        row_height: MENU_ROW_H,
        pad_x: MENU_ROW_PAD_X,
        pad_y: MENU_PAD.top,
        row_inset: 0.0,
        radius: MENU_RADIUS,
        min_width: MENU_W,
        max_width: f32::INFINITY,
        shadow: 20.0,
        frost: None,
        compact_height: MENU_ROW_H,
        compact_icon_size: MENU_ICON,
        compact_label_size: 13.0,
        compact_pad_x: MENU_PAD.left,
        compact_gap: 2.0,
    }
}
