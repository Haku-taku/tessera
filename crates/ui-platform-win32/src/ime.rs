//! Input-method state on Windows: the context's open status, the IME's window placement, and
//! the composition a keystroke produces.
//!
//! IMM32 has no surrounding text and no content type; those are TSF's. What it does have is
//! the composition string, which is what the engine's preedit is built from, and a position
//! for the composition and candidate windows, which is what the caret rectangle steers.
//!
//! Every function here makes its IMM calls with no caller state borrowed: a context change
//! sends messages back to the window procedure on this thread.

use ui_core::Event;
use ui_core::geom::Rect;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_EXCLUDE, CFS_POINT, COMPOSITIONFORM, CPS_CANCEL, GCS_COMPSTR, GCS_CURSORPOS,
    GCS_RESULTSTR, HIMC, IME_COMPOSITION_STRING, ISC_SHOWUICANDIDATEWINDOW,
    ISC_SHOWUICOMPOSITIONWINDOW, ImmGetCompositionStringW, ImmGetContext, ImmNotifyIME,
    ImmReleaseContext, ImmSetCandidateWindow, ImmSetCompositionWindow, ImmSetOpenStatus,
    NI_COMPOSITIONSTR,
};

/// Open or close the window's input context when `open` differs from `already`.
///
/// False means the requested status is not in force: the system has no context for this
/// window, or the input method refused. Either way the caller keeps asking.
pub(crate) fn set_open(hwnd: HWND, open: bool, already: bool) -> bool {
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_invalid() {
        return false;
    }
    let mut applied = true;
    if open != already {
        if open {
            applied = unsafe { ImmSetOpenStatus(context, true) }.as_bool();
        } else {
            // A composition the input method still holds would outlive the close and reach
            // a buffer that is no longer focused.
            unsafe {
                let _ = ImmNotifyIME(context, NI_COMPOSITIONSTR, CPS_CANCEL, 0);
            }
            applied = unsafe { ImmSetOpenStatus(context, false) }.as_bool();
        }
    }
    let _ = unsafe { ImmReleaseContext(hwnd, context) };
    applied
}

/// Put the composition window at the caret and tell the candidate list to keep off it.
///
/// Both forms are in window coordinates: `ptCurrentPos` is relative to the client area's
/// upper-left corner, not the screen, even for the candidate list. The list is placed by the
/// exclusion rectangle rather than by a point, so the system puts it where the caret is not.
pub(crate) fn place(hwnd: HWND, dpi: u32, caret: Rect) {
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_invalid() {
        return;
    }
    let scale = dpi.max(96) as f32 / 96.0;
    let left = (caret.origin.x * scale).round() as i32;
    let top = (caret.origin.y * scale).round() as i32;
    let area = RECT {
        left,
        top,
        right: left + (caret.size.w.max(1.0) * scale).round() as i32,
        bottom: top + (caret.size.h.max(1.0) * scale).round() as i32,
    };
    let composition = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: POINT { x: left, y: top },
        rcArea: area,
    };
    let candidate = CANDIDATEFORM {
        dwIndex: 0,
        dwStyle: CFS_EXCLUDE,
        ptCurrentPos: POINT { x: left, y: top },
        rcArea: area,
    };
    unsafe {
        let _ = ImmSetCompositionWindow(context, &composition);
        let _ = ImmSetCandidateWindow(context, &candidate);
    }
    let _ = unsafe { ImmReleaseContext(hwnd, context) };
}

/// The engine events one `WM_IME_COMPOSITION` carries, in the order they apply.
///
/// A commit is an `Event::Text` and clears the preedit; a live composition is a `Preedit`.
/// Both can arrive at once, and the composition that follows a commit is the one still open.
pub(crate) fn composition(hwnd: HWND, flags: u32) -> Vec<Event> {
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_invalid() {
        return Vec::new();
    }
    let committed = if flags & GCS_RESULTSTR.0 != 0 {
        string(context, GCS_RESULTSTR)
    } else {
        None
    };
    let composed = if flags & GCS_COMPSTR.0 != 0 {
        let text = string(context, GCS_COMPSTR).unwrap_or_default();
        // The cursor is in UTF-16 code units, which is not what the engine indexes by.
        let units = if flags & GCS_CURSORPOS.0 != 0 {
            let cursor = unsafe { ImmGetCompositionStringW(context, GCS_CURSORPOS, None, 0) };
            usize::try_from(cursor).unwrap_or(usize::MAX)
        } else {
            usize::MAX
        };
        Some((text, units))
    } else {
        None
    };
    let _ = unsafe { ImmReleaseContext(hwnd, context) };

    let mut events = Vec::new();
    if let Some(text) = committed.filter(|text| !text.is_empty()) {
        events.push(Event::Text(text));
    }
    match composed {
        Some((text, units)) if !text.is_empty() => {
            let cursor = utf16_byte(&text, units);
            events.push(Event::Preedit(Some(ui_core::Preedit::new(text, cursor))));
        }
        Some(_) => events.push(Event::Preedit(None)),
        // A commit ends the composition it belonged to.
        None if !events.is_empty() => events.push(Event::Preedit(None)),
        None => {}
    }
    events
}

/// The flags the IME reads from `WM_IME_SETCONTEXT` for the windows it may draw itself.
///
/// The engine draws the preedit, so the composition window is never the IME's. A secret
/// buffer hides the candidate list too: the list spells out what is being typed.
pub(crate) fn ui_flags(lparam: LPARAM, hidden: bool) -> LPARAM {
    let mut flags = lparam.0 as u32 & !ISC_SHOWUICOMPOSITIONWINDOW;
    if hidden {
        flags &= !ISC_SHOWUICANDIDATEWINDOW;
    }
    LPARAM(flags as isize)
}

/// One composition string as text; `None` when the input method has none.
fn string(context: HIMC, kind: IME_COMPOSITION_STRING) -> Option<String> {
    let bytes = unsafe { ImmGetCompositionStringW(context, kind, None, 0) };
    if bytes <= 0 {
        return None;
    }
    let mut wide = vec![0u16; (bytes as usize).div_ceil(2)];
    let read = unsafe {
        ImmGetCompositionStringW(
            context,
            kind,
            Some(wide.as_mut_ptr().cast()),
            (wide.len() * 2) as u32,
        )
    };
    if read <= 0 {
        return None;
    }
    let length = (read as usize / 2).min(wide.len());
    Some(String::from_utf16_lossy(wide.get(..length)?))
}

/// The byte offset `units` UTF-16 code units into `text`.
///
/// A position inside a surrogate pair snaps to the next character, the nearest caret a byte
/// index can name.
fn utf16_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (index, character) in text.char_indices() {
        if seen >= units {
            return index;
        }
        seen += character.len_utf16();
    }
    text.len()
}
