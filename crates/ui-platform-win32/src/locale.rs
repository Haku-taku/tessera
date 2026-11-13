//! The user's preferred UI languages, as Windows keeps them.
//!
//! Windows has no locale environment a session states: the list is set in Settings and comes
//! back through `GetUserPreferredUILanguages`, in the BCP-47 tags every catalog negotiates.

use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows::core::PWSTR;

/// The tags Windows prefers, most preferred first; empty when it names none.
pub fn system_languages() -> Vec<String> {
    let mut count = 0u32;
    let mut units = 0u32;
    // The first call asks for the buffer's size; both numbers come back together.
    if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut units) }.is_err()
    {
        return Vec::new();
    }
    if count == 0 || units == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0u16; units as usize];
    // SAFETY: the buffer is the size the first call asked for, and it outlives this call.
    let read = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut units,
        )
    };
    if read.is_err() {
        return Vec::new();
    }
    buffer
        .split(|&unit| unit == 0)
        .filter(|tag| !tag.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}
