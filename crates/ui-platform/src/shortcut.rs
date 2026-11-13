//! App-level shortcut bindings, plus the engine's built-in table.

use ui_core::shortcut::{best_match, KeyChord, ShortcutId};
use ui_core::{Key, Modifiers};

/// Chord → id pairs, split by layer; the first match within a layer wins.
#[derive(Clone, Debug, Default)]
pub struct Bindings {
    /// Bindings the app registered for the whole loop.
    pub app: Vec<(KeyChord, ShortcutId)>,
    /// The engine's own bindings; an app may rebind or drop any of them by id.
    pub builtin: Vec<(KeyChord, ShortcutId)>,
}

impl Bindings {
    /// The engine's defaults, ready for overrides.
    pub fn with_builtins() -> Self {
        Self {
            app: Vec::new(),
            builtin: builtins(),
        }
    }

    /// Bind a chord for the whole app; it fires when no control claimed it.
    pub fn bind(&mut self, chord: KeyChord, id: ShortcutId) {
        self.app.push((chord, id));
    }

    /// Point `id` at `chord` instead of whatever it was bound to, in its own layer.
    ///
    /// Rebinding a built-in is how an app replaces the engine's text-editing keys.
    pub fn rebind(&mut self, id: ShortcutId, chord: KeyChord) {
        let builtin = self.builtin.iter().any(|(_, i)| *i == id);
        self.app.retain(|(_, i)| *i != id);
        self.builtin.retain(|(_, i)| *i != id);
        if builtin {
            self.builtin.push((chord, id));
        } else {
            self.app.push((chord, id));
        }
    }

    /// Drop every binding of `id`, so the chord reaches nothing.
    pub fn unbind(&mut self, id: ShortcutId) {
        self.app.retain(|(_, i)| *i != id);
        self.builtin.retain(|(_, i)| *i != id);
    }

    /// The app-level id bound to this chord, if any.
    pub fn app_id(&self, key: Key, mods: Modifiers) -> Option<ShortcutId> {
        best_match(&self.app, key, mods)
    }

    /// The built-in id bound to this chord, if any.
    pub fn builtin_id(&self, key: Key, mods: Modifiers) -> Option<ShortcutId> {
        best_match(&self.builtin, key, mods)
    }

    /// The chord `id` currently answers, in the order a key press resolves: the app's layer
    /// first, then the engine's. A menu row shows this, so a rebind shows through.
    pub fn chord_of(&self, id: ShortcutId) -> Option<KeyChord> {
        self.app
            .iter()
            .chain(self.builtin.iter())
            .find(|(_, i)| *i == id)
            .map(|(chord, _)| *chord)
    }
}

fn ctrl(c: char) -> KeyChord {
    KeyChord::new(
        Key::Char(c),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    )
}

fn plain(key: Key) -> KeyChord {
    KeyChord::new(key, Modifiers::default())
}

fn shifted(key: Key) -> KeyChord {
    KeyChord::new(
        key,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    )
}

fn ctrl_key(key: Key) -> KeyChord {
    KeyChord::new(
        key,
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    )
}

fn ctrl_shifted(key: Key) -> KeyChord {
    KeyChord::new(
        key,
        Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        },
    )
}

/// Every built-in the engine binds; each is overridable through its id.
fn builtins() -> Vec<(KeyChord, ShortcutId)> {
    vec![
        (ctrl('c'), ShortcutId::COPY),
        (ctrl('x'), ShortcutId::CUT),
        (ctrl('v'), ShortcutId::PASTE),
        (ctrl('z'), ShortcutId::UNDO),
        (ctrl_shifted(Key::Char('z')), ShortcutId::REDO),
        // Ctrl+Y is the Windows spelling of redo; both chords reach the same id.
        (ctrl('y'), ShortcutId::REDO),
        (ctrl('a'), ShortcutId::SELECT_ALL),
        (ctrl('d'), ShortcutId::DELETE_FORWARD),
        (plain(Key::Home), ShortcutId::HOME),
        (plain(Key::End), ShortcutId::END),
        (ctrl_key(Key::Home), ShortcutId::DOCUMENT_HOME),
        (ctrl_key(Key::End), ShortcutId::DOCUMENT_END),
        (shifted(Key::Left), ShortcutId::EXTEND_LEFT),
        (shifted(Key::Right), ShortcutId::EXTEND_RIGHT),
        (ctrl_shifted(Key::Left), ShortcutId::EXTEND_WORD_LEFT),
        (ctrl_shifted(Key::Right), ShortcutId::EXTEND_WORD_RIGHT),
        (shifted(Key::Up), ShortcutId::EXTEND_UP),
        (shifted(Key::Down), ShortcutId::EXTEND_DOWN),
        (shifted(Key::Home), ShortcutId::EXTEND_HOME),
        (shifted(Key::End), ShortcutId::EXTEND_END),
        (ctrl_shifted(Key::Home), ShortcutId::EXTEND_DOCUMENT_HOME),
        (ctrl_shifted(Key::End), ShortcutId::EXTEND_DOCUMENT_END),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebinding_a_builtin_moves_it_and_frees_the_old_chord() {
        let mut bindings = Bindings::with_builtins();
        let ctrl = |c: char| {
            KeyChord::new(
                Key::Char(c),
                Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            )
        };
        let press = |c: char| (Key::Char(c), ctrl(c).mods);
        assert_eq!(
            bindings.builtin_id(press('c').0, press('c').1),
            Some(ShortcutId::COPY)
        );

        // A chord with no built-in of its own, so the rebind is the only match.
        bindings.rebind(ShortcutId::COPY, ctrl('j'));

        assert_eq!(
            bindings.builtin_id(press('j').0, press('j').1),
            Some(ShortcutId::COPY)
        );
        assert_eq!(
            bindings.builtin_id(press('c').0, press('c').1),
            None,
            "the old chord no longer reaches copy"
        );
    }
}
