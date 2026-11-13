//! Localised shortcut labels: `ui-core` keeps the chord, this layer names its keys.

use ui_core::{Key, KeyChord, Modifiers};
use ui_i18n::{Catalog, Msg};

use crate::cx::Cx;

/// A short readable form, e.g. `Ctrl + Shift + K`.
pub fn shortcut_label(chord: &KeyChord, cx: &Cx<'_, '_>) -> String {
    shortcut_label_in(cx.catalog(), chord)
}

/// The same, for a caller that has the catalog but no build context.
pub fn shortcut_label_in(catalog: &Catalog, chord: &KeyChord) -> String {
    let mut out = modifiers_label_in(catalog, chord.mods);
    out.push_str(&key_label_in(catalog, chord.key));
    out
}

/// The held modifiers, ready to be followed by a key: `Ctrl + Shift + `.
pub fn modifiers_label(mods: Modifiers, cx: &Cx<'_, '_>) -> String {
    modifiers_label_in(cx.catalog(), mods)
}

fn modifiers_label_in(catalog: &Catalog, mods: Modifiers) -> String {
    let mut out = String::new();
    for (held, msg) in [
        (mods.ctrl, Msg::WIDGETS_SHORTCUT_CTRL),
        (mods.alt, Msg::WIDGETS_SHORTCUT_ALT),
        (mods.shift, Msg::WIDGETS_SHORTCUT_SHIFT),
        (mods.logo, Msg::WIDGETS_SHORTCUT_SUPER),
    ] {
        if held {
            out.push_str(&catalog.text(msg));
            out.push_str(&catalog.text(Msg::WIDGETS_SHORTCUT_SEPARATOR));
        }
    }
    out
}

fn key_label_in(catalog: &Catalog, key: Key) -> String {
    match key {
        Key::Escape => catalog.text(Msg::WIDGETS_SHORTCUT_ESCAPE).to_string(),
        Key::Enter => catalog.text(Msg::WIDGETS_SHORTCUT_ENTER).to_string(),
        Key::Space => catalog.text(Msg::WIDGETS_SHORTCUT_SPACE).to_string(),
        Key::Tab => catalog.text(Msg::WIDGETS_SHORTCUT_TAB).to_string(),
        Key::Backspace => catalog.text(Msg::WIDGETS_SHORTCUT_BACKSPACE).to_string(),
        Key::Delete => catalog.text(Msg::WIDGETS_SHORTCUT_DELETE).to_string(),
        Key::Home => catalog.text(Msg::WIDGETS_SHORTCUT_HOME).to_string(),
        Key::End => catalog.text(Msg::WIDGETS_SHORTCUT_END).to_string(),
        Key::Left => catalog.text(Msg::WIDGETS_SHORTCUT_LEFT).to_string(),
        Key::Right => catalog.text(Msg::WIDGETS_SHORTCUT_RIGHT).to_string(),
        Key::Up => catalog.text(Msg::WIDGETS_SHORTCUT_UP).to_string(),
        Key::Down => catalog.text(Msg::WIDGETS_SHORTCUT_DOWN).to_string(),
        // A keyboard legend, not UI copy: no locale spells these differently.
        Key::Print => "Print".to_string(),
        Key::Function(n) => format!("F{n}"),
        // Letters are data, not UI copy: the chord keeps the character the user typed.
        Key::Char(c) => c.to_uppercase().to_string(),
    }
}
