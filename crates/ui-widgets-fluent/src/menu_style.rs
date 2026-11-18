//! The Fluent look for the engine's context menu, resolved from the menu flyout's brushes.
//!
//! The engine owns collection, placement and hit testing; this only says how it paints. A
//! flyout is acrylic, but a platform panel's surface has nothing behind it to frost, so the
//! presenter's translucent brush is laid over acrylic's stand-in instead.

use ui_style::{ControlKind, Density, Style, TextRole};
use ui_widgets::MenuStyle;

use crate::material::{acrylic_dark, acrylic_light};
use crate::menu_flyout::{
    ACCEL_GAP, ICON_SLOT, ITEM_MARGIN, ITEM_PAD, PRESENTER_BORDER, PRESENTER_PAD, PRESENTER_RADIUS,
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
    let item = pack
        .controls
        .get(ControlKind::MenuFlyoutItem, Density::Standard);
    let label = pack.type_table.get(item.font);
    let chord = pack.type_table.get(TextRole::Caption);
    // `DesktopAcrylicTransparentBrush` is see-through by design; the stand-in is what WinUI
    // itself paints when the backdrop cannot be sampled.
    let recipe = match pack.appearance() {
        Some(ui_design::Appearance::Dark) => acrylic_dark(),
        _ => acrylic_light(),
    };
    MenuStyle {
        surface: role_of(pack, "MenuFlyoutPresenterBackground").over(recipe.fallback),
        ink: role_of(pack, "MenuFlyoutItemForeground"),
        muted: role_of(pack, "MenuFlyoutItemForegroundDisabled"),
        hover: role_of(pack, "MenuFlyoutItemBackgroundPointerOver"),
        border: Some((
            role_of(pack, "MenuFlyoutPresenterBorderBrush"),
            PRESENTER_BORDER,
        )),
        font_size: label.size,
        shortcut_ink: role_of(pack, "MenuFlyoutItemKeyboardAcceleratorTextForeground"),
        shortcut_size: chord.size,
        shortcut_weight: chord.weight,
        shortcut_gap: ACCEL_GAP,
        trailing_space: ITEM_PAD.right,
        // A flyout item hugs its text; this is the line the engine's row is held at, plus the
        // item's own padding.
        row_height: (ITEM_PAD.top + ITEM_PAD.bottom + label.size * LINE_HINT).round(),
        pad_x: ITEM_PAD.left,
        pad_y: PRESENTER_PAD.top,
        row_inset: ITEM_MARGIN.left,
        radius: PRESENTER_RADIUS,
        min_width: 0.0,
        max_width: f32::INFINITY,
        shadow: 8.0,
        frost: None,
        compact_height: (ITEM_PAD.top + ITEM_PAD.bottom + label.size * LINE_HINT).round(),
        compact_icon_size: ICON_SLOT,
        compact_label_size: chord.size,
        compact_pad_x: ITEM_PAD.left,
        compact_gap: 2.0,
    }
}

/// The height one unwrapped line of the pack's text takes, as the engine's rows hold it.
const LINE_HINT: f32 = 1.4;
