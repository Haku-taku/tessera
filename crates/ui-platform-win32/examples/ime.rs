//! What the input method does on Win32: the context opens with focus, the composition and
//! candidate windows sit at the caret, and a composition reaches the engine as events.
//!
//! Run: `cargo run -p ui-platform-win32 --example ime`
//!
//! The composition string is injected with `ImmSetCompositionString`, so the path from
//! `WM_IME_COMPOSITION` to `Event::Preedit` is exercised without a language IME installed.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("ime: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_core::Event;
    use ui_core::geom::Rect;
    use ui_platform::present::ImeState;
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::Win32Window;
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::UI::Input::Ime::{
        CANDIDATEFORM, CFS_EXCLUDE, COMPOSITIONFORM, GCS_COMPSTR, GCS_CURSORPOS, GCS_RESULTSTR,
        ImmGetCandidateWindow, ImmGetCompositionStringW, ImmGetCompositionWindow, ImmGetContext,
        ImmGetOpenStatus, ImmReleaseContext, ImmSetCompositionStringW, SCS_SETSTR,
    };
    use windows::Win32::UI::WindowsAndMessaging::{WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION};

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:56} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;

    let mut window = Win32Window::new(SurfaceSpec::window(
        "crabsnap-win32-ime",
        "crabsnap — ime probe",
        (480, 320),
    ))
    .map_err(|error| error.to_string())?;
    let hwnd = window.hwnd();
    let scale = window.scale();

    // A window with no text field focused must not intercept the keyboard.
    window.set_ime(ImeState::default());
    let context = unsafe { ImmGetContext(hwnd) };
    if context.is_invalid() {
        // Nothing on this machine can compose: there is no context to open, and no state to
        // steer. The rest of the probe would assert nothing, so the run reports the gap.
        println!("this machine offers no input context; nothing to probe");
        return Ok(());
    }
    let open = unsafe { ImmGetOpenStatus(context) }.as_bool();
    ok &= expect("an unfocused window leaves the input method closed", !open);

    // A caret 20 by 40 logical units into the client area, 18 high.
    let caret = Rect::new(20.0, 40.0, 2.0, 18.0);
    window.set_ime(ImeState {
        enabled: true,
        cursor: Some(caret),
        ..ImeState::default()
    });
    let open = unsafe { ImmGetOpenStatus(context) }.as_bool();
    ok &= expect("a caret in this frame opens the input method", open);

    let mut composition = COMPOSITIONFORM::default();
    unsafe { ImmGetCompositionWindow(context, &mut composition) }
        .ok()
        .map_err(|error| format!("ImmGetCompositionWindow: {error}"))?;
    let expected = POINT {
        x: (caret.origin.x * scale).round() as i32,
        y: (caret.origin.y * scale).round() as i32,
    };
    ok &= expect(
        "the composition window sits at the caret",
        composition.ptCurrentPos == expected,
    );

    let mut candidate = CANDIDATEFORM::default();
    unsafe { ImmGetCandidateWindow(context, 0, &mut candidate) }
        .ok()
        .map_err(|error| format!("ImmGetCandidateWindow: {error}"))?;
    // Window coordinates, like the composition window's: a screen point here lands one
    // window offset down and right of the caret, which is how the list was first seen.
    let caret_rect = RECT {
        left: expected.x,
        top: expected.y,
        right: expected.x + (caret.size.w * scale).round() as i32,
        bottom: expected.y + (caret.size.h * scale).round() as i32,
    };
    ok &= expect(
        "the candidate list avoids the caret's own rectangle, in window coordinates",
        candidate.dwStyle == CFS_EXCLUDE
            && candidate.ptCurrentPos == expected
            && candidate.rcArea == caret_rect,
    );

    // Inject a composition string and let the window procedure read it back. The text holds
    // a surrogate pair, so a cursor counted in UTF-16 units would land inside a character
    // rather than on a byte boundary.
    let text = "a😀b";
    let wide: Vec<u16> = text.encode_utf16().collect();
    let injected = unsafe {
        ImmSetCompositionStringW(
            context,
            SCS_SETSTR,
            Some(wide.as_ptr().cast()),
            (wide.len() * 2) as u32,
            None,
            0,
        )
    };
    ok &= expect(
        "the input method accepts an injected composition",
        injected.as_bool(),
    );
    let _ = unsafe { ImmReleaseContext(hwnd, context) };
    post(hwnd, WM_IME_COMPOSITION, GCS_COMPSTR.0 | GCS_CURSORPOS.0)?;
    let events = window.take_events();
    let preedit = events.iter().find_map(|event| match event {
        Event::Preedit(preedit) => preedit.clone(),
        _ => None,
    });
    ok &= expect(
        "a composition arrives as the engine's preedit",
        preedit.as_ref().is_some_and(|preedit| preedit.text == text),
    );
    // An injected composition reports no cursor position, so the caret sits at its end; a
    // cursor counted in UTF-16 units would be 4 here, inside the surrogate pair.
    ok &= expect(
        "the preedit cursor is a byte offset, at the end when the IME names none",
        preedit
            .as_ref()
            .is_some_and(|preedit| preedit.cursor == text.len()),
    );
    println!(
        "  composition {text:?}: {} bytes, preedit cursor {:?}",
        text.len(),
        preedit.as_ref().map(|preedit| preedit.cursor)
    );

    post(hwnd, WM_IME_ENDCOMPOSITION, 0)?;
    let events = window.take_events();
    ok &= expect(
        "ending the composition clears the preedit",
        events
            .iter()
            .any(|event| matches!(event, Event::Preedit(None))),
    );

    // A commit is the input method's own message, which injecting a composition cannot make;
    // the handler for it is the same one the preedit came through, so the flag is checked
    // here only for the string it would read.
    let context = unsafe { ImmGetContext(hwnd) };
    let result = unsafe { ImmGetCompositionStringW(context, GCS_RESULTSTR, None, 0) };
    let _ = unsafe { ImmReleaseContext(hwnd, context) };
    ok &= expect("no result string is pending outside a commit", result <= 0);

    if !ok {
        return Err(String::from("one or more probes failed"));
    }
    println!("PASS: composition, placement and open status at scale {scale}");
    Ok(())
}

#[cfg(windows)]
fn post(hwnd: windows::Win32::Foundation::HWND, message: u32, flags: u32) -> Result<(), String> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(hwnd),
            message,
            WPARAM(0),
            LPARAM(flags as isize),
        )
    }
    .map_err(|error| format!("PostMessageW: {error}"))
}
