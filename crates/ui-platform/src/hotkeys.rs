//! System-wide hotkeys: the interface every desktop backend implements.

use ui_core::event::{Key, Modifiers};
use ui_core::shortcut::{KeyChord, ShortcutId};

/// Why a global hotkey could not be bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlobalHotkeyError {
    /// This backend has no system-wide hotkeys at all (X11, headless, a bare compositor).
    Unsupported,
    /// The desktop asked the user and the user said no, or the desktop closed the request.
    Refused,
    /// Another application already owns the chord.
    Taken,
    /// The backend failed for its own reason, with the text it gave.
    Failed(String),
}

/// What an app needs from the desktop, and nothing about how the desktop does it.
///
/// Wayland has no protocol for this: the portal is the portable path and a compositor's
/// private IPC is the fallback; Windows and macOS each have their own registry.
pub trait GlobalHotkeys {
    /// Ask the desktop for `chord`; a press is later reported by [`Self::poll`].
    ///
    /// The desktop may prompt the user: on Wayland the portal owns the binding UI.
    fn register(&mut self, chord: KeyChord, id: ShortcutId) -> Result<(), GlobalHotkeyError>;

    /// Give the chord back.
    fn unregister(&mut self, id: ShortcutId);

    /// Hotkeys pressed since the last call, in order.
    ///
    /// Never blocks: the host calls it once per frame, beside its other event sources.
    fn poll(&mut self) -> Vec<ShortcutId>;
}

/// Parse the spelling a configuration file uses: `Ctrl+Shift+4`, `Print`, `Alt+F2`.
///
/// Modifiers come first in any order; the last token is the key. Letters are stored
/// unshifted, so `Ctrl+Shift+Z` and `Ctrl+Shift+z` are the same chord. A modifier written
/// with a trailing `?` is optional — `Shift?+A` answers both `A` and `Shift+A` — so a
/// command whose effect depends on a modifier is one line, not one per spelling. The names
/// are the engine's, not a platform's: a backend maps `Key` to whatever its registry expects.
pub fn parse_chord(text: &str) -> Result<KeyChord, String> {
    let mut required = Modifiers::default();
    let mut optional = Modifiers::default();
    let mut parts: Vec<&str> = text
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let Some(key) = parts.pop() else {
        return Err("empty chord".into());
    };
    for part in parts {
        let (name, free) = match part.strip_suffix('?') {
            Some(name) => (name, true),
            None => (part, false),
        };
        let bit = match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
            "shift" => Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
            "alt" => Modifiers {
                alt: true,
                ..Modifiers::NONE
            },
            "super" | "logo" | "meta" | "win" => Modifiers {
                logo: true,
                ..Modifiers::NONE
            },
            other => return Err(format!("unknown modifier {other}")),
        };
        // A modifier cannot be both demanded and left free: the chord would match only
        // when the two rules disagree, which is never.
        if (if free { required } else { optional }).bits() & bit.bits() != 0 {
            return Err(format!("modifier {name} is both required and optional"));
        }
        if free {
            optional = optional.union(bit);
        } else {
            required = required.union(bit);
        }
    }
    Ok(KeyChord::new(parse_key(key)?, required).allowing(optional))
}

/// Parse the chords one action is bound to: `A, Left`, or a single `Ctrl+S`.
///
/// The framework's binding table already holds several chords per id — redo answers both
/// `Ctrl+Shift+Z` and `Ctrl+Y` — so the file's comma is the whole of what a list needs.
pub fn parse_chords(text: &str) -> Result<Vec<KeyChord>, String> {
    let mut chords = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err("an empty chord".into());
        }
        chords.push(parse_chord(part)?);
    }
    Ok(chords)
}

/// The key a chord's last token names.
fn parse_key(text: &str) -> Result<Key, String> {
    let lower = text.to_ascii_lowercase();
    let named = match lower.as_str() {
        "escape" | "esc" => Key::Escape,
        "enter" | "return" => Key::Enter,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "print" | "printscreen" => Key::Print,
        _ => {
            if let Some(n) = lower
                .strip_prefix('f')
                .and_then(|digits| digits.parse::<u8>().ok())
                .filter(|n| (1..=24).contains(n))
            {
                return Ok(Key::Function(n));
            }
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => {
                    return Ok(Key::Char(c.to_ascii_lowercase()));
                }
                _ => return Err(format!("unknown key {text}")),
            }
        }
    };
    Ok(named)
}

/// A backend with no system-wide hotkeys; every desktop without support uses it.
#[derive(Default)]
pub struct NoGlobalHotkeys;

impl GlobalHotkeys for NoGlobalHotkeys {
    fn register(&mut self, _chord: KeyChord, _id: ShortcutId) -> Result<(), GlobalHotkeyError> {
        Err(GlobalHotkeyError::Unsupported)
    }

    fn unregister(&mut self, _id: ShortcutId) {}

    fn poll(&mut self) -> Vec<ShortcutId> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_chord_is_read_as_modifiers_then_one_key() {
        assert_eq!(
            parse_chord("Print"),
            Ok(KeyChord::new(Key::Print, Modifiers::NONE))
        );
        assert_eq!(
            parse_chord(" ctrl + SHIFT+W "),
            Ok(KeyChord::new(
                Key::Char('w'),
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Modifiers::NONE
                }
            )),
            "spaces are decoration and letters are stored unshifted"
        );
        assert_eq!(
            parse_chord("Super+space"),
            Ok(KeyChord::new(
                Key::Space,
                Modifiers {
                    logo: true,
                    ..Modifiers::NONE
                }
            ))
        );
        assert_eq!(
            parse_chord("Alt+F12"),
            Ok(KeyChord::new(
                Key::Function(12),
                Modifiers {
                    alt: true,
                    ..Modifiers::NONE
                }
            ))
        );

        assert!(parse_chord("Hyper+x").is_err(), "an unknown modifier");
        assert!(parse_chord("Ctrl+").is_err(), "no key at all");
        assert!(
            parse_chord("Ctrl+Shift").is_err(),
            "a modifier is not a key"
        );
        assert!(parse_chord("Ctrl+loud").is_err(), "an unknown key");
        assert!(
            parse_chord("F25").is_err(),
            "outside the function-key range"
        );
    }

    /// A `?` leaves a modifier to the press, which is how one line covers a command whose
    /// effect depends on it. Only that modifier is freed, and one modifier cannot be both.
    #[test]
    fn a_question_mark_leaves_one_modifier_to_the_press() {
        let chord = parse_chord("Shift?+A").expect("one optional modifier");
        assert_eq!(chord.key, Key::Char('a'));
        assert_eq!(chord.mods, Modifiers::NONE, "nothing is required");
        assert_eq!(
            chord.any,
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        );
        assert!(chord.matches(Key::Char('a'), Modifiers::NONE));
        assert!(chord.matches(
            Key::Char('A'),
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        ));

        let mixed = parse_chord("Ctrl+Alt?+Left").expect("one of each");
        assert!(mixed.mods.ctrl, "Ctrl is required");
        assert!(mixed.any.alt, "Alt is left to the press");
        assert_eq!(mixed.key, Key::Left);

        assert!(
            parse_chord("Shift+Shift?+A").is_err(),
            "a modifier cannot be required and left free at once"
        );
        assert!(parse_chord("Hyper?+A").is_err(), "an unknown modifier");
    }

    #[test]
    fn a_list_holds_one_chord_per_comma() {
        assert_eq!(
            parse_chords("A"),
            Ok(vec![parse_chord("A").expect("a plain letter")])
        );
        assert_eq!(
            parse_chords(" A , Left "),
            Ok(vec![
                KeyChord::new(Key::Char('a'), Modifiers::NONE),
                KeyChord::new(Key::Left, Modifiers::NONE)
            ]),
            "positions keep their order: the first one is what a menu shows"
        );
        assert!(parse_chords("A,").is_err(), "a trailing comma names nothing");
        assert!(parse_chords("A, ,B").is_err(), "and neither does a hole");
        assert!(parse_chords("A,Hyper+B").is_err(), "every chord is checked");
    }
}
