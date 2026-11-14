//! `KeyChord` to-and-from the portal's `preferred_trigger` string, with no D-Bus involved.
//!
//! Grammar is the freedesktop Shortcuts spec: `CTRL`/`ALT`/`SHIFT`/`NUM`/`LOGO` joined by `+`,
//! then a keysym name from `xkbcommon-keysyms.h` minus the `XKB_KEY_` prefix.

use ui_core::event::{Key, Modifiers};
use ui_core::shortcut::KeyChord;

/// Modifier token for the control key.
const CTRL: &str = "CTRL";
/// Modifier token for the alt key.
const ALT: &str = "ALT";
/// Modifier token for the shift key.
const SHIFT: &str = "SHIFT";
/// Modifier token for the super/meta key.
const LOGO: &str = "LOGO";

/// The keysym name for a named (non-character) key, from `xkbcommon-keysyms.h`, or `None`
/// when this engine has no spelling for it. Characters go through [`char_keysym`].
fn named_keysym(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Escape => "Escape",
        Key::Enter => "Return",
        Key::Space => "space",
        Key::Tab => "Tab",
        Key::Backspace => "BackSpace",
        Key::Delete => "Delete",
        Key::Insert => "Insert",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "Prior",
        Key::PageDown => "Next",
        Key::Left => "Left",
        Key::Right => "Right",
        Key::Up => "Up",
        Key::Down => "Down",
        Key::Print => "Print",
        Key::Function(_) | Key::Char(_) => return None,
    })
}

/// Keysym name for a character: ASCII letters lowercase (unshifted, as `shortcut.rs` stores
/// them), digits themselves, punctuation by its spec name. Unmapped characters give `None`.
fn char_keysym(c: char) -> Option<&'static str> {
    if c.is_ascii_alphabetic() {
        return Some(LETTER_NAMES[(c.to_ascii_lowercase() as u8 - b'a') as usize]);
    }
    if c.is_ascii_digit() {
        return Some(DIGIT_NAMES[(c as u8 - b'0') as usize]);
    }
    Some(match c {
        ' ' => "space",
        '!' => "exclam",
        '"' => "quotedbl",
        '#' => "numbersign",
        '$' => "dollar",
        '%' => "percent",
        '&' => "ampersand",
        '\'' => "apostrophe",
        '(' => "parenleft",
        ')' => "parenright",
        '*' => "asterisk",
        '+' => "plus",
        ',' => "comma",
        '-' => "minus",
        '.' => "period",
        '/' => "slash",
        ':' => "colon",
        ';' => "semicolon",
        '<' => "less",
        '=' => "equal",
        '>' => "greater",
        '?' => "question",
        '@' => "at",
        '[' => "bracketleft",
        '\\' => "backslash",
        ']' => "bracketright",
        '^' => "asciicircum",
        '_' => "underscore",
        '`' => "grave",
        '{' => "braceleft",
        '|' => "bar",
        '}' => "braceright",
        '~' => "asciitilde",
        _ => return None,
    })
}

/// Lowercase letters, indexed `a..z`; each keysym name is the letter itself.
const LETTER_NAMES: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];

/// Digits, indexed `0..9`; each keysym name is the digit itself (`XKB_KEY_0` .. `XKB_KEY_9`).
const DIGIT_NAMES: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// The keysym name for a chord's key, `None` if none is known.
fn key_keysym(key: Key) -> Option<&'static str> {
    match key {
        Key::Char(c) => char_keysym(c),
        Key::Function(n) => F_NAMES.get(usize::from(n).checked_sub(1)?).copied(),
        other => named_keysym(other),
    }
}

/// Function keysyms, indexed from F1; `xkbcommon-keysyms.h` names them `F1` .. `F24`.
const F_NAMES: [&str; 24] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15",
    "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
];

/// Encode a chord as a portal `preferred_trigger`, or `None` if the key has no known name.
///
/// Locks (`Num`/`Caps`) are never emitted, matching the engine's exact-match rule; `Shift`
/// stays because it selects a different chord, not a lock state.
pub fn chord_to_trigger(chord: KeyChord) -> Option<String> {
    let key = key_keysym(chord.key)?;
    // Order fixed by the spec's `CTRL, ALT, SHIFT, NUM, LOGO` enumeration, so output is stable.
    let mut parts: Vec<&str> = Vec::with_capacity(5);
    if chord.mods.ctrl {
        parts.push(CTRL);
    }
    if chord.mods.alt {
        parts.push(ALT);
    }
    if chord.mods.shift {
        parts.push(SHIFT);
    }
    if chord.mods.logo {
        parts.push(LOGO);
    }
    parts.push(key);
    Some(parts.join("+"))
}

/// Reverse of [`chord_to_trigger`]: parse a `preferred_trigger` back into a chord.
///
/// `None` for any token outside the recognized vocabulary, so a trigger the portal chose that
/// this engine cannot name never fabricates a bogus chord.
pub fn trigger_to_chord(trigger: &str) -> Option<KeyChord> {
    let mut tokens = trigger.split('+').peekable();
    let mut mods = Modifiers::default();
    let mut key: Option<Key> = None;
    while let Some(tok) = tokens.next() {
        match tok {
            CTRL => mods.ctrl = true,
            ALT => mods.alt = true,
            SHIFT => mods.shift = true,
            LOGO => mods.logo = true,
            // The key is the last token; a name here that we can't resolve rejects the whole
            // string, so a modifier never stands alone as the trigger.
            _ if tokens.peek().is_none() => key = Some(keysym_to_key(tok)?),
            _ => return None,
        }
    }
    Some(KeyChord::new(key?, mods))
}

/// A keysym name back to a `Key`, the inverse of [`key_keysym`] over the mapped set.
fn keysym_to_key(name: &str) -> Option<Key> {
    match name {
        "Escape" => return Some(Key::Escape),
        "Return" => return Some(Key::Enter),
        "space" => return Some(Key::Space),
        "Tab" => return Some(Key::Tab),
        "BackSpace" => return Some(Key::Backspace),
        "Delete" => return Some(Key::Delete),
        "Insert" => return Some(Key::Insert),
        "Home" => return Some(Key::Home),
        "End" => return Some(Key::End),
        // The portal answers with the keysym name the compositor knows, which is the
        // canonical "Prior"/"Next" as often as the "Page_Up"/"Page_Down" alias.
        "Prior" | "Page_Up" => return Some(Key::PageUp),
        "Next" | "Page_Down" => return Some(Key::PageDown),
        "Left" => return Some(Key::Left),
        "Right" => return Some(Key::Right),
        "Up" => return Some(Key::Up),
        "Down" => return Some(Key::Down),
        _ => {}
    }
    // A single lowercase letter or digit is its own character; otherwise punctuation.
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        let b = bytes[0];
        if b.is_ascii_lowercase() || b.is_ascii_digit() {
            return Some(Key::Char(b as char));
        }
    }
    let c = match name {
        "exclam" => '!',
        "quotedbl" => '"',
        "numbersign" => '#',
        "dollar" => '$',
        "percent" => '%',
        "ampersand" => '&',
        "apostrophe" => '\'',
        "parenleft" => '(',
        "parenright" => ')',
        "asterisk" => '*',
        "plus" => '+',
        "comma" => ',',
        "minus" => '-',
        "period" => '.',
        "slash" => '/',
        "colon" => ':',
        "semicolon" => ';',
        "less" => '<',
        "equal" => '=',
        "greater" => '>',
        "question" => '?',
        "at" => '@',
        "bracketleft" => '[',
        "backslash" => '\\',
        "bracketright" => ']',
        "asciicircum" => '^',
        "underscore" => '_',
        "grave" => '`',
        "braceleft" => '{',
        "bar" => '|',
        "braceright" => '}',
        "asciitilde" => '~',
        _ => return None,
    };
    Some(Key::Char(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(key: Key, ctrl: bool, alt: bool, shift: bool, logo: bool) -> KeyChord {
        KeyChord::new(
            key,
            Modifiers {
                ctrl,
                alt,
                shift,
                logo,
            },
        )
    }

    #[test]
    fn a_chord_encodes_to_portal_trigger_string() {
        // Ctrl+Shift+K: spec modifier order, key as the lowercase keysym name.
        assert_eq!(
            chord_to_trigger(chord(Key::Char('k'), true, false, true, false)).as_deref(),
            Some("CTRL+SHIFT+k")
        );
        // Ctrl+1 is a digit key whose keysym name is the digit itself.
        assert_eq!(
            chord_to_trigger(chord(Key::Char('1'), true, false, false, false)).as_deref(),
            Some("CTRL+1")
        );
        // Alt+Slash: the symbol key is `slash`, not the `/` character.
        assert_eq!(
            chord_to_trigger(chord(Key::Char('/'), false, true, false, false)).as_deref(),
            Some("ALT+slash")
        );
        // Meta+Enter: `LOGO` for the super key, `Return` for the enter keysym name.
        assert_eq!(
            chord_to_trigger(chord(Key::Enter, false, false, false, true)).as_deref(),
            Some("LOGO+Return")
        );
    }
}
