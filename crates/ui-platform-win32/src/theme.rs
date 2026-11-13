//! The system light/dark preference, which Windows stores per user in the registry.

use ui_platform::theme::{ThemePreference, ThemeWatcher};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::PCWSTR;

/// Where Windows keeps the app colour scheme: `AppsUseLightTheme`, 1 for light.
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const VALUE: &str = "AppsUseLightTheme";

/// The system's app colour scheme, read fresh on every poll.
///
/// A registry read is served from the kernel's cached hive and costs microseconds, so the
/// poll needs no change notification of its own — and none can be missed this way.
#[derive(Clone, Copy, Debug)]
pub struct Win32ThemeWatcher {
    preference: ThemePreference,
}

impl Win32ThemeWatcher {
    pub fn new() -> Self {
        Self {
            preference: read(),
        }
    }
}

impl Default for Win32ThemeWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeWatcher for Win32ThemeWatcher {
    fn preference(&self) -> ThemePreference {
        self.preference
    }

    fn poll(&mut self) -> Option<ThemePreference> {
        let now = read();
        (now != self.preference).then(|| {
            self.preference = now;
            now
        })
    }
}

fn read() -> ThemePreference {
    let key = wide(PERSONALIZE);
    let name = wide(VALUE);
    let mut light: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(key.as_ptr()),
            PCWSTR::from_raw(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut light as *mut u32).cast()),
            Some(&mut size),
        )
    };
    // An error means the value is not set, which is Windows' way of stating no preference.
    match status.0 {
        0 if light == 0 => ThemePreference::PreferDark,
        0 => ThemePreference::PreferLight,
        _ => ThemePreference::NoPreference,
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
