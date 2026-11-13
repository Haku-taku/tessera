//! Feed the window procedure a scripted message sequence and print what the engine sees.
//!
//! Run: `cargo run -p ui-platform-win32 --example input_probe`
//!
//! The messages are posted, so this exercises the window procedure and the translation of
//! a message into `ui_core::Event`; only the system's own routing of a real mouse is left
//! out. Modifier state is read from the keyboard, so a command chord cannot be posted.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("input_probe: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_core::geom::Point;
    use ui_core::{Event, Key, PointerButton};
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::Win32Window;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        PostMessageW, WM_CHAR, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        WM_MOUSEWHEEL,
    };

    let mut window = Win32Window::new(SurfaceSpec::window(
        "crabsnap-win32-input",
        "crabsnap — input probe",
        (400, 300),
    ))
    .map_err(|error| error.to_string())?;
    // The messages below are client coordinates in physical pixels; the events come back
    // in logical units, which is what layout uses.
    let scale = window.scale();
    let at = |x: i32, y: i32| LPARAM(((y << 16) | (x & 0xFFFF)) as isize);
    let post = |message: u32, wparam: usize, lparam: LPARAM| {
        unsafe { PostMessageW(Some(window.hwnd()), message, WPARAM(wparam), lparam) }
            .map_err(|error| format!("PostMessageW: {error}"))
    };

    post(WM_MOUSEMOVE, 0, at(40, 30))?;
    post(WM_LBUTTONDOWN, 0, at(40, 30))?;
    post(WM_LBUTTONUP, 0, at(40, 30))?;
    post(WM_MOUSEWHEEL, 120usize << 16, at(40, 30))?;
    post(WM_CHAR, 'x' as usize, LPARAM(0))?;
    post(WM_KEYDOWN, 0x1B, LPARAM(0))?;
    // The real cursor is not over the window, so Windows follows the synthetic move with a
    // leave; that is the same branch a pointer leaving for real takes, and it is asserted
    // below rather than fought with.
    // `take_events` pumps the queue: the posts above are dispatched there.
    let events = window.take_events();

    let logical = Point::new(40.0 / scale, 30.0 / scale);
    let expect = |pattern: &str, found: bool| {
        println!("{pattern:52} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;
    let near = |a: Point, b: Point| (a.x - b.x).abs() < 1.0 && (a.y - b.y).abs() < 1.0;
    ok &= expect(
        "a move arrives at the window's logical position",
        events
            .iter()
            .any(|event| matches!(event, Event::PointerMoved { pos } if near(*pos, logical))),
    );
    ok &= expect(
        "a press names the primary button",
        events.iter().any(|event| matches!(
            event,
            Event::PointerPressed { button: PointerButton::Primary, .. }
        )),
    );
    ok &= expect(
        "a release follows the press",
        events.iter().any(|event| matches!(
            event,
            Event::PointerReleased { button: PointerButton::Primary, .. }
        )),
    );
    ok &= expect(
        "a notch scrolls down by one step",
        events
            .iter()
            .any(|event| matches!(event, Event::Scroll { delta, .. } if *delta == Point::new(0.0, -48.0))),
    );
    ok &= expect(
        "a character arrives as text",
        events
            .iter()
            .any(|event| matches!(event, Event::Text(text) if text == "x")),
    );
    ok &= expect(
        "a named key keeps its name",
        events
            .iter()
            .any(|event| matches!(event, Event::KeyPressed { key: Key::Escape })),
    );
    // The pointer the driver reads must agree with the events it just saw.
    ok &= expect(
        "a leave clears the pointer once the cursor is gone",
        events
            .iter()
            .any(|event| matches!(event, Event::PointerLeft))
            && window.pointer().is_none(),
    );
    if !ok {
        return Err(String::from("one or more probes failed"));
    }
    println!("PASS: {} events translated", events.len());
    Ok(())
}
