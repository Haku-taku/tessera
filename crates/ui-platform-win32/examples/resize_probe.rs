//! What a window does while its border is being dragged: the frames a modal sizing loop
//! would have blocked.
//!
//! Run: `cargo run -p ui-platform-win32 --example resize_probe`
//!
//! The drag is posted message by message rather than injected, so the probe needs no mouse
//! and no foreground window: a border press names its own screen position, and each move
//! names the client position the pointer would have had.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("resize_probe: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PostMessageW};

/// A surface that fills its window, so a frame built for a new size is a different frame.
#[cfg(windows)]
struct Blank;

#[cfg(windows)]
impl ui_platform::view::App for Blank {
    fn view(
        &mut self,
        cx: &mut ui_core::cx::Cx<'_>,
    ) -> Box<dyn ui_platform::Element> {
        use ui_core::geom::{Color, Corners};
        use ui_core::{Id, NodeKind, SizeClass};

        Box::new(
            ui_platform::Container::new(Id::str("root"), NodeKind::Stack)
                .size(SizeClass::fixed(cx.width() as f32, cx.height() as f32))
                .background(Color::from_rgba8(24, 28, 36, 255), Corners::ZERO),
        )
    }
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_platform::session::SessionOptions;
    use ui_platform::view::AppLoop;
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::Session;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows::Win32::UI::WindowsAndMessaging::{
        HTBOTTOMRIGHT, HTTOPLEFT, MINMAXINFO, SendMessageW, WM_GETMINMAXINFO, WM_KEYDOWN,
        WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCLBUTTONDOWN,
    };

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:58} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;

    let options = SessionOptions {
        surface: SurfaceSpec::window("crabsnap-resize", "crabsnap — resize probe", (420, 300))
            .with_min_size((420, 300)),
        uncapped: false,
        cpu_presenter: false,
    };
    let mut session = Session::open(AppLoop::new(Blank), options)?;
    let hwnd = session.window().hwnd();

    // Draw until the first frame lands; its client size is what every delta below is from.
    // `tick` waits for its own frame, so the turns below start with the frame window open.
    let mut drew = false;
    for _ in 0..20 {
        drew |= session.tick()?.drew;
        if drew {
            break;
        }
    }
    if !drew {
        return Err(String::from("the window never drew its first frame"));
    }
    let (width, height) = session.window().client_size();
    let start = window_rect(hwnd)?;

    // The press a sizing border receives names the border's corner in screen pixels.
    post(
        hwnd,
        WM_NCLBUTTONDOWN,
        HTBOTTOMRIGHT as usize,
        packed(start.right, start.bottom),
    );
    session.tick()?;

    // The pointer moves out and down by the corner's delta, and the loop takes a turn.
    // Messages name the client position, so the probe converts the screen point it means.
    post(
        hwnd,
        WM_MOUSEMOVE,
        0,
        client_of(hwnd, start.right + 60, start.bottom + 40),
    );
    let tick = session.tick()?;
    let grown = session.window().client_size();
    ok &= expect(
        "a dragged border resizes the window",
        grown == (width + 60, height + 40),
    );

    // The frame the drag presented was built for the new size, not stretched from the old
    // one: the readback is the size the swap chain holds.
    let path = std::env::temp_dir().join("crabsnap-resize-probe.ppm");
    session.dump_frame_ppm(&path)?;
    let presented = ppm_size(&path);
    ok &= expect(
        "the frame after the move is the new size",
        presented == Some(grown),
    );
    ok &= expect("the move presented a frame", tick.drew && presented.is_some());
    println!("presented {presented:?} for client {grown:?} from ({width}, {height})");

    // The release ends the drag: a later move is the app's pointer, not the border's.
    post(hwnd, WM_LBUTTONUP, 0, packed(0, 0));
    session.tick()?;
    post(hwnd, WM_MOUSEMOVE, 0, packed(200, 30));
    session.tick()?;
    ok &= expect(
        "a move after the release leaves the size alone",
        session.window().client_size() == grown,
    );

    // A top-left edge moves the window as well as sizing it, which is the half where a sign
    // can be wrong without the bottom-right check noticing.
    let before = window_rect(hwnd)?;
    let press = (before.left + 2, before.top + 2);
    let target = (before.left + 60, before.top + 40);
    post(
        hwnd,
        WM_NCLBUTTONDOWN,
        HTTOPLEFT as usize,
        packed(press.0, press.1),
    );
    session.tick()?;
    post(hwnd, WM_MOUSEMOVE, 0, client_of(hwnd, target.0, target.1));
    session.tick()?;
    let dragged = window_rect(hwnd)?;
    ok &= expect(
        "the top-left edge moves the window's origin",
        dragged
            == RECT {
                left: before.left + (target.0 - press.0),
                top: before.top + (target.1 - press.1),
                right: before.right,
                bottom: before.bottom,
            },
    );

    // Escape puts the window back where the drag began, as the system's own sizing does.
    post(hwnd, WM_KEYDOWN, VK_ESCAPE.0 as usize, 0);
    session.tick()?;
    ok &= expect(
        "Escape restores the rectangle the drag began on",
        window_rect(hwnd)? == before,
    );

    // The opening size is also the declared floor: the shell is told it before a sizing
    // drag, and the drag's own clamp reads the same value.
    let mut limits = MINMAXINFO::default();
    unsafe {
        SendMessageW(
            hwnd,
            WM_GETMINMAXINFO,
            None,
            Some(LPARAM(&mut limits as *mut MINMAXINFO as isize)),
        );
    }
    ok &= expect(
        "the shell is told the spec's minimum",
        (limits.ptMinTrackSize.x, limits.ptMinTrackSize.y)
            == (start.right - start.left, start.bottom - start.top),
    );

    let shrinking = window_rect(hwnd)?;
    post(
        hwnd,
        WM_NCLBUTTONDOWN,
        HTBOTTOMRIGHT as usize,
        packed(shrinking.right, shrinking.bottom),
    );
    session.tick()?;
    post(
        hwnd,
        WM_MOUSEMOVE,
        0,
        client_of(hwnd, shrinking.right - 240, shrinking.bottom - 160),
    );
    session.tick()?;
    ok &= expect(
        "a drag stops at the declared minimum",
        session.window().client_size() == (width, height),
    );
    post(hwnd, WM_LBUTTONUP, 0, packed(0, 0));
    session.tick()?;

    let _ = std::fs::remove_file(&path);

    println!();
    println!("{}", if ok { "PASS" } else { "FAIL" });
    if ok {
        Ok(())
    } else {
        Err(String::from("some checks failed"))
    }
}

/// The window's rectangle on screen.
#[cfg(windows)]
fn window_rect(hwnd: HWND) -> Result<RECT, String> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|error| format!("GetWindowRect: {error}"))?;
    Ok(rect)
}

/// A screen position as the client position a mouse message would carry for it.
#[cfg(windows)]
fn client_of(hwnd: HWND, x: i32, y: i32) -> isize {
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    let mut point = windows::Win32::Foundation::POINT { x, y };
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        eprintln!("resize_probe: ({x}, {y}) is not on the window's client area");
    }
    packed(point.x, point.y)
}

/// A position as a message packs it: two signed 16-bit halves.
#[cfg(windows)]
fn packed(x: i32, y: i32) -> isize {
    ((y as isize & 0xFFFF) << 16) | (x as isize & 0xFFFF)
}

#[cfg(windows)]
fn post(hwnd: HWND, message: u32, wparam: usize, lparam: isize) {
    if let Err(error) = unsafe { PostMessageW(Some(hwnd), message, WPARAM(wparam), LPARAM(lparam)) }
    {
        eprintln!("resize_probe: {message:#x} was not posted: {error}");
    }
}

/// The size a PPM header names.
#[cfg(windows)]
fn ppm_size(path: &std::path::Path) -> Option<(u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    let header = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
    let mut parts = header.split_whitespace();
    (parts.next()? == "P6").then_some(())?;
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}
