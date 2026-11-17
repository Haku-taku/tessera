//! Input state: keyboard keysyms and pointer position.

use std::collections::HashSet;

use ui_core::geom::Point;
use xkbcommon::xkb;

/// A key event translated from the compositor's evdev codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// Unicode text this key produces, when it produces any.
    pub text: Option<char>,
    /// Whether a modifier (Ctrl/Alt/Super) was held.
    pub modified: bool,
}

/// The character a keysym names, by the standard keysym-to-Unicode rule.
///
/// Latin-1 keysyms are their code point; Unicode keysyms carry it offset by `0x0100_0000`.
fn keysym_char(sym: u32) -> Option<char> {
    let code = match sym {
        0x20..=0x7e | 0xa0..=0xff => sym,
        0x0100_0100..=0x0110_ffff => sym - 0x0100_0000,
        _ => return None,
    };
    char::from_u32(code)
}

/// Everything the UI needs to know about input.
pub struct InputState {
    /// Keys currently held, by keysym.
    held: HashSet<u32>,
    /// Characters typed since the last frame consumed them.
    typed: Vec<char>,
    /// Pointer position, when over the surface.
    pointer: Option<Point>,
    /// xkb state: translates evdev codes to keysyms using the compositor's keymap.
    xkb_state: Option<xkb::State>,
    xkb_context: xkb::Context,
}

impl std::fmt::Debug for InputState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputState")
            .field("held", &self.held.len())
            .field("typed", &self.typed)
            .field("pointer", &self.pointer)
            .finish()
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        Self {
            held: HashSet::new(),
            typed: Vec::new(),
            pointer: None,
            xkb_state: None,
            // The context must outlive every keymap built from it.
            xkb_context: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
        }
    }

    /// Install the compositor's keymap from the descriptor it handed over.
    ///
    /// This is the only path that works for a Wayland keymap: it ends with a NUL,
    /// which `new_from_string` cannot accept but the fd path handles.
    ///
    /// # Safety
    /// `fd` must be the keymap descriptor from a `wl_keyboard.keymap` event.
    pub unsafe fn set_keymap_fd(&mut self, fd: std::os::fd::OwnedFd, size: usize) -> bool {
        // The caller's contract is this function's, so the unsafe block repeats it.
        unsafe {
            match xkb::Keymap::new_from_fd(
                &self.xkb_context,
                fd,
                size,
                xkb::KEYMAP_FORMAT_TEXT_V1,
                xkb::KEYMAP_COMPILE_NO_FLAGS,
            ) {
                Ok(Some(keymap)) => {
                    self.xkb_state = Some(xkb::State::new(&keymap));
                    true
                }
                _ => false,
            }
        }
    }

    /// Install a keymap from text that has no trailing NUL.
    ///
    /// Kept for tests; real Wayland keymaps go through `set_keymap_fd`.
    pub fn set_keymap(&mut self, text: &str) {
        let keymap = xkb::Keymap::new_from_string(
            &self.xkb_context,
            text.trim_end_matches('\0').to_string(),
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        );
        self.xkb_state = keymap.map(|km| xkb::State::new(&km));
    }

    pub fn has_keymap(&self) -> bool {
        self.xkb_state.is_some()
    }

    pub fn set_pointer(&mut self, p: Option<Point>) {
        self.pointer = p;
    }

    pub fn pointer(&self) -> Option<Point> {
        self.pointer
    }

    /// Update a pressed key and return the character naming it, if any.
    ///
    /// Named by keysym, not by the text the key would type: with Ctrl held xkb's utf8 is a
    /// control character, so a chord would otherwise arrive as no key at all.
    pub fn key(&mut self, evdev_code: u32, pressed: bool) -> Option<char> {
        let state = self.xkb_state.as_mut()?;
        let keycode = xkb::Keycode::new(evdev_code + 8); // XKB keycodes are evdev + 8.
        let raw = state.key_get_one_sym(keycode).raw();
        if pressed {
            self.held.insert(raw);
        } else {
            self.held.remove(&raw);
        }
        if !pressed {
            return None;
        }
        keysym_char(raw)
    }

    /// Whether this key asks for a menu: the Menu keysym, or Shift+F10 where there is none.
    ///
    /// Win32 names the same pair, so a client binding the Menu key hears both desktops
    /// alike and a keyboard without the key keeps the chord.
    pub fn menu_key(&mut self, evdev_code: u32) -> bool {
        let Some(state) = self.xkb_state.as_mut() else {
            return false;
        };
        let keycode = xkb::Keycode::new(evdev_code + 8);
        let symbol = state.key_get_one_sym(keycode).raw();
        symbol == 0xff67
            || (symbol == 0xffc7 && state.mod_name_is_active("Shift", xkb::STATE_MODS_EFFECTIVE))
    }

    /// The text this key types right now, composed with any dead key before it.
    ///
    /// Differs from [`Self::key`] where the layout composes: a dead key followed by `e`
    /// names `e` but types `é`.
    pub fn text(&mut self, evdev_code: u32) -> Option<char> {
        let state = self.xkb_state.as_mut()?;
        let utf8 = state.key_get_utf8(xkb::Keycode::new(evdev_code + 8));
        utf8.chars().next().filter(|c| !c.is_control())
    }

    /// Apply a modifier change from the compositor.
    pub fn update_modifiers(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        if let Some(state) = self.xkb_state.as_mut() {
            state.update_mask(depressed, latched, locked, 0, 0, group);
        }
    }

    /// Record a typed character for the next frame.
    pub fn push_text(&mut self, c: char) {
        if !c.is_control() {
            self.typed.push(c);
        }
    }

    /// Take all typed characters since the last call.
    pub fn take_typed(&mut self) -> Vec<char> {
        std::mem::take(&mut self.typed)
    }

    /// Whether the named keysym is currently held.
    pub fn is_held(&self, keysym: u32) -> bool {
        self.held.contains(&keysym)
    }

    /// Held keysym count, for diagnostics.
    pub fn held_count(&self) -> usize {
        self.held.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_text_is_consumed_once() {
        let mut s = InputState::new();
        s.push_text('a');
        s.push_text('b');
        assert_eq!(s.take_typed(), vec!['a', 'b']);
        assert!(s.take_typed().is_empty(), "second take must be empty");
    }

    #[test]
    fn a_control_chord_still_names_its_letter() {
        // Ctrl turns xkb's utf8 into a control character; the key must still say which one.
        let mut s = InputState::new();
        let keymap = xkb::Keymap::new_from_names(&s.xkb_context, "", "", "us", "", None, 0)
            .expect("the us layout is installed");
        s.xkb_state = Some(xkb::State::new(&keymap));
        s.key(29, true); // KEY_LEFTCTRL
        s.update_modifiers(1 << 2, 0, 0, 0);
        assert_eq!(s.key(46, true), Some('c'), "Ctrl+C must name the letter c");
    }
}
