//! System-wide hotkeys on Windows: `RegisterHotKey` against a window of our own.
//!
//! Registering against a window keeps `WM_HOTKEY` out of every surface's queue: it arrives
//! at a message-only window nobody draws, whose procedure only records the id for `poll`.

use std::cell::RefCell;

use ui_core::event::Key;
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_platform::hotkeys::{GlobalHotkeyError, GlobalHotkeys};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError, HINSTANCE, HWND,
    LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
    VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT,
    VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_MESSAGE, RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_HOTKEY, WNDCLASSEXW,
};
use windows::core::{Error, PCWSTR};

/// The class the hotkey sink registers under. A probe finds the window by this name.
pub const WINDOW_CLASS: &str = "crabsnap-hotkey-sink";

thread_local! {
    /// Ids the sink window has received, waiting for `poll`. One sink exists per process, on
    /// the loop's thread, and the window procedure has no other way to reach its object.
    static PRESSED: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
}

/// The registry's half: the sink window and the ids it has handed out.
pub(crate) struct Win32Hotkeys {
    window: HWND,
    /// Registration id and the application's id for each chord currently bound.
    bound: Vec<(i32, ShortcutId)>,
    next_id: i32,
}

impl Win32Hotkeys {
    /// Create the sink window. Registration itself is what can fail per chord.
    pub(crate) fn open() -> Result<Self, String> {
        let class = wide(WINDOW_CLASS);
        let instance =
            unsafe { GetModuleHandleW(None) }.map_err(|error| format!("module handle: {error}"))?;
        let descriptor = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(sink_proc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR::from_raw(class.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassExW(&descriptor) } == 0 {
            let error = unsafe { GetLastError() };
            // The class outlives every sink, and one is opened per process; a rebuild of the
            // backend is the only way to reach this.
            if error != ERROR_CLASS_ALREADY_EXISTS {
                return Err(format!("RegisterClassExW failed: {}", error.0));
            }
        }
        // A message-only window: never shown, and not in the window list `FindWindow` walks.
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR::from_raw(class.as_ptr()),
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
        }
        .map_err(|error| format!("create the hotkey window: {error}"))?;
        Ok(Self {
            window,
            bound: Vec::new(),
            // Registration ids are an application range; 1 keeps 0's "no id" meaning free.
            next_id: 1,
        })
    }
}

impl GlobalHotkeys for Win32Hotkeys {
    fn register(&mut self, chord: KeyChord, id: ShortcutId) -> Result<(), GlobalHotkeyError> {
        let key = virtual_key(&chord.key).ok_or_else(|| {
            GlobalHotkeyError::Failed(format!("no virtual key for {:?}", chord.key))
        })?;
        // The registry binds the modifiers a chord requires; a modifier the chord leaves
        // optional cannot be expressed, and the chord is bound with the required set only.
        let mut modifiers = HOT_KEY_MODIFIERS(0);
        if chord.mods.ctrl {
            modifiers |= MOD_CONTROL;
        }
        if chord.mods.shift {
            modifiers |= MOD_SHIFT;
        }
        if chord.mods.alt {
            modifiers |= MOD_ALT;
        }
        if chord.mods.logo {
            modifiers |= MOD_WIN;
        }
        // One chord per application id: a second binding of the same id replaces the first.
        self.unregister(id);
        let registration = self.next_id;
        // SAFETY: the window is live for as long as this object is, and the ids are ours.
        let registered = unsafe { RegisterHotKey(Some(self.window), registration, modifiers, key) };
        registered.map_err(|error| {
            if code(&error) == ERROR_HOTKEY_ALREADY_REGISTERED.0 {
                // The registry reports another owner's chord with this code.
                GlobalHotkeyError::Taken
            } else {
                GlobalHotkeyError::Failed(error.to_string())
            }
        })?;
        self.next_id += 1;
        self.bound.push((registration, id));
        Ok(())
    }

    fn unregister(&mut self, id: ShortcutId) {
        let Some(index) = self.bound.iter().position(|(_, bound)| *bound == id) else {
            return;
        };
        let (registration, _) = self.bound.remove(index);
        // The window is live and the id was handed out here; a refused release leaves the
        // system to drop the binding with the process.
        let _ = unsafe { UnregisterHotKey(Some(self.window), registration) };
    }

    fn poll(&mut self) -> Vec<ShortcutId> {
        let pressed = PRESSED.with(|pressed| std::mem::take(&mut *pressed.borrow_mut()));
        pressed
            .into_iter()
            .filter_map(|registration| {
                self.bound
                    .iter()
                    .find(|(id, _)| *id == registration)
                    .map(|(_, id)| *id)
            })
            .collect()
    }
}

impl Drop for Win32Hotkeys {
    fn drop(&mut self) {
        for (registration, _) in &self.bound {
            let _ = unsafe { UnregisterHotKey(Some(self.window), *registration) };
        }
        // The window goes with the object; `DestroyWindow` is what ends it.
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.window) };
    }
}

/// The sink window's procedure: record the press, and leave everything else to defaults.
unsafe extern "system" fn sink_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_HOTKEY {
        PRESSED.with(|pressed| pressed.borrow_mut().push(wparam.0 as i32));
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// The virtual key a chord's key stands for, for the keys `RegisterHotKey` can bind.
fn virtual_key(key: &Key) -> Option<u32> {
    let code = match *key {
        Key::Escape => VK_ESCAPE.0,
        Key::Enter => VK_RETURN.0,
        Key::Space => VK_SPACE.0,
        Key::Tab => VK_TAB.0,
        Key::Backspace => VK_BACK.0,
        Key::Delete => VK_DELETE.0,
        Key::Insert => VK_INSERT.0,
        Key::Home => VK_HOME.0,
        Key::End => VK_END.0,
        Key::PageUp => VK_PRIOR.0,
        Key::PageDown => VK_NEXT.0,
        Key::Left => VK_LEFT.0,
        Key::Right => VK_RIGHT.0,
        Key::Up => VK_UP.0,
        Key::Down => VK_DOWN.0,
        Key::Print => VK_SNAPSHOT.0,
        Key::Function(number) if (1..=24).contains(&number) => VK_F1.0 + u16::from(number) - 1,
        // Letters and digits use their ASCII values as virtual keys. Punctuation does not
        // follow ASCII on every layout, so it is not offered.
        Key::Char(character) if character.is_ascii_alphanumeric() => {
            character.to_ascii_uppercase() as u16
        }
        _ => return None,
    };
    (code != 0).then_some(u32::from(code))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The Win32 code inside a failure from a `Result`-returning call.
fn code(error: &Error) -> u32 {
    error.code().0 as u32 & 0xFFFF
}
