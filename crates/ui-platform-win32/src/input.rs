//! What a Win32 message means to the engine: virtual keys, held modifiers, wheel distance.
//!
//! Kept apart from the window procedure so the translation is a pure function of the
//! message and the keyboard state — the part worth reading twice.

use ui_core::{Key, Modifiers};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, MAPVK_VK_TO_CHAR, MapVirtualKeyW, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN,
    VK_END, VK_ESCAPE, VK_F1, VK_F24, VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_RETURN, VK_RIGHT,
    VK_RWIN, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP,
};

/// Logical pixels one wheel notch scrolls: three lines, the platform's own default.
pub(crate) const WHEEL_NOTCH: f32 = 48.0;

/// The modifiers held now.
///
/// Windows reports each key's state ambiently rather than attaching modifiers to a press,
/// which is the same model as the engine's `Modifiers`.
pub(crate) fn modifiers() -> Modifiers {
    Modifiers {
        shift: held(VK_SHIFT.0),
        ctrl: held(VK_CONTROL.0),
        alt: held(VK_MENU.0),
        logo: held(VK_LWIN.0) || held(VK_RWIN.0),
    }
}

fn held(key: u16) -> bool {
    // The high bit is "down"; the low bit is the toggle state and means nothing here.
    unsafe { GetKeyState(key as i32) as u16 & 0x8000 != 0 }
}

/// The named key `vk` stands for, when the engine keeps a name for it.
pub(crate) fn named(vk: u16) -> Option<Key> {
    let key = match vk {
        vk if vk == VK_ESCAPE.0 => Key::Escape,
        vk if vk == VK_RETURN.0 => Key::Enter,
        vk if vk == VK_SPACE.0 => Key::Space,
        vk if vk == VK_TAB.0 => Key::Tab,
        vk if vk == VK_BACK.0 => Key::Backspace,
        vk if vk == VK_DELETE.0 => Key::Delete,
        vk if vk == VK_HOME.0 => Key::Home,
        vk if vk == VK_END.0 => Key::End,
        vk if vk == VK_LEFT.0 => Key::Left,
        vk if vk == VK_RIGHT.0 => Key::Right,
        vk if vk == VK_UP.0 => Key::Up,
        vk if vk == VK_DOWN.0 => Key::Down,
        vk if vk == VK_SNAPSHOT.0 => Key::Print,
        vk if (VK_F1.0..=VK_F24.0).contains(&vk) => Key::Function((vk - VK_F1.0 + 1) as u8),
        _ => return None,
    };
    Some(key)
}

/// The character a command chord names: Ctrl+A arrives as `Char('a')`, never as text.
///
/// `MapVirtualKeyW` reports the character on the key's unshifted face, which is what a
/// chord is stored as.
pub(crate) fn command_char(vk: u16) -> Option<char> {
    let value = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_CHAR) };
    // Bit 31 marks a dead key: its character is composed with the next one, not commanded.
    if value == 0 || value & 0x8000_0000 != 0 {
        return None;
    }
    char::from_u32(value & 0xFFFF).map(|character| character.to_ascii_lowercase())
}
