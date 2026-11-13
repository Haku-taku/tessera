//! Key chords, and the stable ids a shortcut is registered under.

use crate::event::{Key, Modifiers};

/// A key plus the modifiers that must be held with it.
///
/// Matching is exact unless a modifier is named in `any`: one extra modifier makes it a
/// different chord, as in Qt and GPUI, but a command whose effect depends on a modifier
/// declares that modifier optional instead of binding every spelling of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    /// The key, with letters stored unshifted so `Ctrl+Shift+Z` has one spelling.
    pub key: Key,
    /// The modifiers that must be held.
    pub mods: Modifiers,
    /// The modifiers that may be held or not; each one is a wildcard.
    ///
    /// A wildcard never matches a *different* modifier: `A` with `Shift` in `any` answers
    /// `A` and `Shift+A`, and nothing with `Ctrl` held.
    pub any: Modifiers,
}

impl KeyChord {
    /// An exact chord: every modifier must be held, and no other may be.
    pub const fn new(key: Key, mods: Modifiers) -> Self {
        Self {
            key,
            mods,
            any: Modifiers::NONE,
        }
    }

    /// The same chord with these modifiers allowed either way as well.
    ///
    /// It adds to what the chord already leaves free, so a caller widening a chord parsed
    /// from a file cannot take away a modifier that file left optional.
    pub const fn allowing(self, any: Modifiers) -> Self {
        Self {
            any: self.any.union(any),
            ..self
        }
    }

    /// Whether the chord is the one pressed: same key, and every modifier it does not
    /// leave free agrees with the ones held.
    pub fn matches(&self, key: Key, mods: Modifiers) -> bool {
        let must_agree = !self.any.bits();
        (self.mods.bits() ^ mods.bits()) & must_agree == 0 && same_key(self.key, key)
    }

    /// How many modifiers the chord leaves free; the fewest wins when two both match.
    pub fn freedom(&self) -> u32 {
        self.any.bits().count_ones()
    }
}

/// The id the table answers for this press: the most specific match, the first registered
/// among equals.
///
/// A chord that leaves a modifier free is less specific than one that names it, so binding
/// `Shift+Left` beside `Left`-with-`Shift`-free keeps the exact spelling reachable whatever
/// the order the two were registered in.
pub fn best_match(
    table: &[(KeyChord, ShortcutId)],
    key: Key,
    mods: Modifiers,
) -> Option<ShortcutId> {
    let mut best: Option<(u32, ShortcutId)> = None;
    for (chord, id) in table {
        if !chord.matches(key, mods) {
            continue;
        }
        let freedom = chord.freedom();
        if best.is_none_or(|(seen, _)| freedom < seen) {
            best = Some((freedom, *id));
        }
    }
    best.map(|(_, id)| id)
}

/// Case-insensitive for letters: the platform reports the shifted character.
fn same_key(a: Key, b: Key) -> bool {
    match (a, b) {
        (Key::Char(x), Key::Char(y)) => x.eq_ignore_ascii_case(&y),
        _ => a == b,
    }
}

/// A stable name for a shortcut, so an app can override a built-in by id.
///
/// `&'static str` on purpose: an id that changes between runs cannot be overridden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShortcutId(pub &'static str);

impl ShortcutId {
    /// The ids the engine binds by default; every one of them is overridable.
    pub const COPY: Self = Self("text.copy");
    pub const CUT: Self = Self("text.cut");
    pub const PASTE: Self = Self("text.paste");
    pub const UNDO: Self = Self("text.undo");
    pub const REDO: Self = Self("text.redo");
    pub const SELECT_ALL: Self = Self("text.select-all");
    /// Deletes the character after the caret, as in Emacs and readline.
    pub const DELETE_FORWARD: Self = Self("text.delete-forward");
    pub const HOME: Self = Self("text.home");
    pub const END: Self = Self("text.end");
    pub const DOCUMENT_HOME: Self = Self("text.document-home");
    pub const DOCUMENT_END: Self = Self("text.document-end");
    pub const EXTEND_LEFT: Self = Self("text.extend-left");
    pub const EXTEND_RIGHT: Self = Self("text.extend-right");
    pub const EXTEND_UP: Self = Self("text.extend-up");
    pub const EXTEND_DOWN: Self = Self("text.extend-down");
    pub const EXTEND_WORD_LEFT: Self = Self("text.extend-word-left");
    pub const EXTEND_WORD_RIGHT: Self = Self("text.extend-word-right");
    pub const EXTEND_HOME: Self = Self("text.extend-home");
    pub const EXTEND_END: Self = Self("text.extend-end");
    pub const EXTEND_DOCUMENT_HOME: Self = Self("text.extend-document-home");
    pub const EXTEND_DOCUMENT_END: Self = Self("text.extend-document-end");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shifted_letter_matches_the_unshifted_chord_with_shift_held() {
        let chord = KeyChord::new(
            Key::Char('z'),
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::default()
            },
        );
        assert!(chord.matches(
            Key::Char('Z'),
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::default()
            }
        ));
        // The same letter without Shift is a different chord, not a near miss.
        assert!(!chord.matches(
            Key::Char('Z'),
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            }
        ));
    }

    /// A chord may leave a modifier free: it answers the press with and without it, and
    /// never one that holds a different modifier. A table carrying both spellings picks the
    /// one naming the modifier, whatever order they were registered in — the exact spelling
    /// must stay reachable beside a wider one.
    #[test]
    fn a_free_modifier_widens_the_chord_without_shadowing_an_exact_one() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let loose = KeyChord::new(Key::Left, Modifiers::NONE).allowing(shift);
        let exact = KeyChord::new(Key::Left, shift);
        assert!(loose.matches(Key::Left, Modifiers::NONE));
        assert!(loose.matches(Key::Left, shift));
        assert!(
            !loose.matches(Key::Left, ctrl),
            "a free Shift frees nothing else"
        );

        let fine = ShortcutId("test.fine");
        let coarse = ShortcutId("test.coarse");
        // The wider chord is registered first, so document order cannot be what decides.
        let table = [(loose, fine), (exact, coarse)];
        assert_eq!(best_match(&table, Key::Left, shift), Some(coarse));
        assert_eq!(best_match(&table, Key::Left, Modifiers::NONE), Some(fine));
        assert_eq!(best_match(&table, Key::Right, Modifiers::NONE), None);
    }
}
