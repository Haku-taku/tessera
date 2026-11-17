//! The MD3 look for the engine's context menu, resolved from the `md.comp.menus` tokens.
//!
//! The engine owns collection, placement and hit testing; this only says how it paints.

use ui_style::Style;

use crate::paint::spec_ink;
use crate::spec::{MenusFlags, menus_spec};

/// `rounded_corners` on the MD3 shape scale, which the export names but does not number.
const RADIUS: f32 = 4.0;
/// MD3 gives the menu no width cap; a very long label simply makes a wide menu.
const MAX_WIDTH: f32 = 560.0;
/// How much of the surface's tone survives over a frost: enough for the ink to read on any
/// picture behind it, little enough that the menu still reads as glass.
const FROST_TINT: f32 = 0.8;

/// The menu's standard variant, as the engine's style.
///
/// `blur` is the frost behind the surface, in sigma; zero paints it opaque.
pub fn menu_style(pack: &Style, blur: f32) -> ui_widgets::MenuStyle {
    // `selected, active, first_child, last_child, hovered, focused, pressed, disabled`: a
    // container reads its tokens with no state on, a row with only the hover state.
    let s = menus_spec(MenusFlags::NONE);
    let row = menus_spec(MenusFlags {
        hovered: true,
        ..MenusFlags::NONE
    });
    // A backend that cannot sample gets the opaque surface: a translucent one would show
    // whatever it was drawn over, unfrosted.
    let frost = (blur > 0.0 && pack.frost_allowed(pack.device))
        .then(|| ui_core::BackdropBlur::css(blur));
    ui_widgets::MenuStyle {
        surface: spec_ink(
            pack,
            s.container.color,
            if frost.is_some() { FROST_TINT } else { 1.0 },
        ),
        ink: spec_ink(pack, row.menu_item_label_text.color, 1.0),
        muted: spec_ink(
            pack,
            row.menu_item_label_text.color,
            crate::DISABLED_CONTENT_OPACITY,
        ),
        hover: spec_ink(
            pack,
            row.menu_item_state_layer.color,
            row.menu_item_state_layer.opacity,
        ),
        // MD3 bounds the menu with its shadow, not a hairline.
        border: None,
        font_size: row.menu_item_label_text.size,
        // MD3 puts the shortcut in the trailing supporting text slot; Compose's font token is
        // `label-small`, while the export carries the label's own 14sp — the type scale wins.
        shortcut_ink: spec_ink(pack, row.menu_item_trailing_supporting_text.color, 1.0),
        shortcut_size: crate::typescale::LABEL_SMALL.size,
        shortcut_weight: crate::typescale::LABEL_SMALL.weight,
        shortcut_gap: row.menu_item.between_space,
        trailing_space: row.menu_item.trailing_space,
        row_height: row.menu_item.height,
        pad_x: row.menu_item.leading_space,
        pad_y: row.menu_item.top_space,
        // A row's shape takes the same air the token gives it above and below, so the
        // highlight's margins read equal on every side instead of tighter at the sides.
        row_inset: s.menu_item.top_space,
        radius: RADIUS,
        min_width: 112.0,
        max_width: MAX_WIDTH,
        shadow: s.container.elevation * 2.0,
        frost,
        // MD3's menu has no quick-action strip; the cell follows the label-small ramp and
        // the menu item's own spacing so a menu that uses one still reads as this menu.
        compact_height: 56.0,
        compact_icon_size: 20.0,
        compact_label_size: crate::typescale::LABEL_SMALL.size,
        compact_pad_x: s.menu_item.top_space,
        compact_gap: 2.0,
    }
}
