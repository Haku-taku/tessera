//! Check Desktop Duplication against a window whose pixels are known.
//!
//! The probe draws a chart of known sRGB codes, screens it, and reads every patch back out of
//! the captured desktop: that catches a wrong adapter (a black frame), a wrong channel order,
//! a capture that is offset or mirrored, and a colour transform left un-undone — an advanced
//! colour desktop composes in linear light, not in the codes a window drew.
//!
//! Run: `cargo run -p ui-platform-win32 --example capture --release`

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("capture: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::time::{Duration, Instant};

    use ui_core::Damage;
    use ui_core::NodeId;
    use ui_core::draw::{DrawList, RectPrim};
    use ui_core::geom::{Color, Corners, Rect};
    use ui_platform::present::SceneTarget;
    use ui_platform::text_pass::TextPass;
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::{ScreenCapture, ScreenFrame, Win32Presenter, Win32Window};
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    /// Greys across the range, the three primaries, and one colour unlike any of them.
    const BARS: [Color; 8] = [
        Color::from_rgba8(0, 0, 0, 255),
        Color::from_rgba8(32, 32, 32, 255),
        Color::from_rgba8(128, 128, 128, 255),
        Color::from_rgba8(255, 255, 255, 255),
        Color::from_rgba8(255, 0, 0, 255),
        Color::from_rgba8(0, 255, 0, 255),
        Color::from_rgba8(0, 0, 255, 255),
        Color::from_rgba8(220, 30, 90, 255),
    ];

    /// The chart, one vertical bar per colour, in logical points.
    fn chart(size: (f32, f32)) -> DrawList {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, size.0, size.1));
        let bar = size.0 / BARS.len() as f32;
        for (index, colour) in BARS.iter().enumerate() {
            list.push_rect(
                NodeId::from_raw(index as u32 + 1, 0),
                RectPrim {
                    rect: Rect::new(index as f32 * bar, 0.0, bar, size.1),
                    radius: Corners::ZERO,
                    fill: *colour,
                    border: None,
                    border_gradient: None,
                    gradient: None,
                    notch: None,
                },
            );
        }
        list
    }

    /// The client area in screen pixels: the title bar and borders are Windows' own.
    fn client_rect(window: &Win32Window) -> Result<RECT, String> {
        let mut rect = RECT::default();
        unsafe { GetClientRect(window.hwnd(), &mut rect) }
            .map_err(|error| format!("GetClientRect: {error}"))?;
        let mut origin = windows::Win32::Foundation::POINT {
            x: rect.left,
            y: rect.top,
        };
        if !unsafe { ClientToScreen(window.hwnd(), &mut origin) }.as_bool() {
            return Err(String::from("ClientToScreen failed"));
        }
        Ok(RECT {
            left: origin.x,
            top: origin.y,
            right: origin.x + (rect.right - rect.left),
            bottom: origin.y + (rect.bottom - rect.top),
        })
    }

    let spec = SurfaceSpec::window(
        "crabsnap-win32-capture",
        "crabsnap — Win32 capture",
        (320, 240),
    );
    let mut window = Win32Window::new(spec).map_err(|error| error.to_string())?;
    let mut presenter = Win32Presenter::new(&window).map_err(|error| error.to_string())?;
    let text = TextPass::isolated();
    // Opened before the chart is drawn, so the frame grabbed below is the update that carried it.
    let mut capture = ScreenCapture::open(None)?;
    println!(
        "capturing {} at scale {}",
        capture.output(),
        capture.scale()
    );

    // The client area takes its DPI-scaled size only after the first turns of the loop, and a
    // chart laid out before that is narrower than the window: its last bars fall outside.
    let mut size = window.client_size();
    loop {
        window.pump();
        let settled = window.client_size();
        if settled.0 > 0 && settled == size {
            break;
        }
        size = settled;
    }
    let scale = window.scale();
    presenter
        .resize(size, scale)
        .map_err(|error| error.to_string())?;
    // The scene is in logical points and the presenter scales it, so the bars are laid out in
    // points or every one lands a quarter of a bar further right than the last.
    let list = chart((size.0 as f32 / scale, size.1 as f32 / scale));
    let client = client_rect(&window)?;
    println!(
        "window client on screen: ({}, {})-({}, {})",
        client.left, client.top, client.right, client.bottom
    );

    let mut failures = 0;
    let mut check = |what: &str, ok: bool| {
        println!("{what:56} {}", if ok { "OK" } else { "FAIL" });
        if !ok {
            failures += 1;
        }
    };

    let across = (client.right - client.left) as f32;
    let down = (client.bottom - client.top) as f32;
    let middle = client.top + (down / 2.0) as i32;
    let bar_centre =
        |index: usize| client.left + ((index as f32 + 0.5) * across / BARS.len() as f32) as i32;
    let want = |colour: Color| {
        [
            (colour.r * 255.0).round() as u8,
            (colour.g * 255.0).round() as u8,
            (colour.b * 255.0).round() as u8,
            255,
        ]
    };
    let at = |frame: &ScreenFrame, x: i32, y: i32| {
        let index = ((y as u32 * frame.width + x as u32) * 4) as usize;
        [
            frame.pixels[index],
            frame.pixels[index + 1],
            frame.pixels[index + 2],
            frame.pixels[index + 3],
        ]
    };
    let chart_is_up = |frame: &ScreenFrame| {
        BARS.iter().enumerate().all(|(index, colour)| {
            at(frame, bar_centre(index), middle)
                .iter()
                .zip(want(*colour))
                .all(|(got, want)| got.abs_diff(want) <= 2)
        })
    };

    // DWM shows the window only once it has composed one, and a new window animates in, so the
    // capture holds the desktop or a blend for a while: present and grab until the chart is up.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut frame = None;
    while Instant::now() < deadline {
        window.pump();
        presenter
            .present_scene(&list, &text, &Damage::Full, &[])
            .map_err(|error| format!("present: {error}"))?;
        if let Ok(next) = capture.grab(Duration::from_millis(50)) {
            let settled = chart_is_up(&next);
            frame = Some(next);
            if settled {
                break;
            }
        }
    }
    let frame = frame.ok_or_else(|| String::from("the capture returned no frame"))?;
    println!(
        "frame {}x{}, stride {}, colour {:?}",
        frame.width,
        frame.height,
        frame.stride(),
        frame.colour
    );

    let inside = client.left >= 0
        && client.top >= 0
        && (client.right as u32) <= frame.width
        && (client.bottom as u32) <= frame.height;
    check("the window's client area is inside the frame", inside);
    if !inside {
        return Err(String::from("the window is off the captured output"));
    }

    for (index, colour) in BARS.iter().enumerate() {
        let got = at(&frame, bar_centre(index), middle);
        let off = got
            .iter()
            .zip(want(*colour))
            .map(|(got, want)| got.abs_diff(want))
            .max()
            .unwrap_or(0);
        check(
            &format!("bar {index} reads as it was drawn ({got:?})"),
            off <= 2,
        );
    }

    // A capture that never left one buffer would pass the checks above by being uniform.
    let corner = at(&frame, frame.width as i32 - 1, frame.height as i32 - 1);
    let last = want(BARS[BARS.len() - 1]);
    check(
        &format!("the corner is the desktop, not the chart ({corner:?})"),
        corner != last,
    );

    if failures > 0 {
        return Err(format!("{failures} check(s) failed"));
    }
    println!("PASS: the duplicated desktop holds the codes the window was drawn with");
    Ok(())
}
