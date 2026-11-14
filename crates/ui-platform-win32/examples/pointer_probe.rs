//! What a finger and a pen become for the window: the events the system's own promotion
//! produces when the window procedure handles no pointer message itself.
//!
//! Run: `cargo run -p ui-platform-win32 --example pointer_probe`
//!
//! The contacts go in through `InjectSyntheticPointerInput`, the path a real digitiser
//! takes, so this exercises the routing a touchscreen or a stylus really gets.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("pointer_probe: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

/// `PEN_MASK_PRESSURE`, which winuser.h has and the windows crate does not name.
#[cfg(windows)]
const PEN_MASK_PRESSURE: u32 = 0x0000_0002;

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_core::geom::Point;
    use ui_core::{Event, PointerButton};
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::Win32Window;
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::UI::Controls::{
        CreateSyntheticPointerDevice, DestroySyntheticPointerDevice, POINTER_FEEDBACK_DEFAULT,
        POINTER_TYPE_INFO, POINTER_TYPE_INFO_0,
    };
    use windows::Win32::UI::Input::Pointer::{
        InjectSyntheticPointerInput, POINTER_FLAG_DOWN, POINTER_FLAG_INCONTACT,
        POINTER_FLAG_INRANGE, POINTER_FLAG_PRIMARY, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
        POINTER_FLAGS, POINTER_INFO, POINTER_PEN_INFO, POINTER_TOUCH_INFO,
    };
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, PT_PEN, PT_TOUCH, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    };

    let mut window = Win32Window::new(SurfaceSpec::window(
        "crabsnap-win32-pointer",
        "crabsnap — pointer probe",
        (400, 300),
    ))
    .map_err(|error| error.to_string())?;
    unsafe {
        SetWindowPos(
            window.hwnd(),
            None,
            200,
            200,
            400,
            300,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .map_err(|error| format!("SetWindowPos: {error}"))?;
    let mut rect = RECT::default();
    unsafe { GetWindowRect(window.hwnd(), &mut rect) }
        .map_err(|error| format!("GetWindowRect: {error}"))?;
    let middle = Point {
        x: ((rect.left + rect.right) / 2) as f32,
        y: ((rect.top + rect.bottom) / 2) as f32,
    };
    let scale = window.scale();

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:56} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;
    for (touch, name) in [(true, "a finger"), (false, "a pen")] {
        let kind = if touch { PT_TOUCH } else { PT_PEN };
        let device = unsafe { CreateSyntheticPointerDevice(kind, 1, POINTER_FEEDBACK_DEFAULT) }
            .map_err(|error| format!("CreateSyntheticPointerDevice({name}): {error}"))?;
        let contact = |flags: POINTER_FLAGS, at: Point| {
            let pointer = POINTER_INFO {
                pointerType: kind,
                pointerFlags: flags,
                ptPixelLocation: POINT {
                    x: at.x as i32,
                    y: at.y as i32,
                },
                ..POINTER_INFO::default()
            };
            let touch_info = POINTER_TOUCH_INFO {
                pointerInfo: pointer,
                pressure: 32000,
                ..POINTER_TOUCH_INFO::default()
            };
            let pen_info = POINTER_PEN_INFO {
                pointerInfo: pointer,
                penMask: PEN_MASK_PRESSURE,
                pressure: 512,
                ..POINTER_PEN_INFO::default()
            };
            POINTER_TYPE_INFO {
                r#type: kind,
                Anonymous: if touch {
                    POINTER_TYPE_INFO_0 {
                        touchInfo: touch_info,
                    }
                } else {
                    POINTER_TYPE_INFO_0 { penInfo: pen_info }
                },
            }
        };
        let down = POINTER_FLAGS(
            POINTER_FLAG_INRANGE.0 | POINTER_FLAG_INCONTACT.0 | POINTER_FLAG_PRIMARY.0,
        );
        let inject = |at: Point, flag: POINTER_FLAGS| {
            let frame = [contact(POINTER_FLAGS(down.0 | flag.0), at)];
            unsafe { InjectSyntheticPointerInput(device, &frame) }
                .map_err(|error| format!("InjectSyntheticPointerInput({name}): {error}"))
        };
        let dragged = Point {
            x: middle.x + 40.0,
            y: middle.y + 20.0,
        };
        inject(middle, POINTER_FLAG_DOWN)?;
        std::thread::sleep(std::time::Duration::from_millis(60));
        inject(dragged, POINTER_FLAG_UPDATE)?;
        std::thread::sleep(std::time::Duration::from_millis(60));
        inject(dragged, POINTER_FLAG_UP)?;
        let events = drain(&mut window, 800);
        unsafe { DestroySyntheticPointerDevice(device) };

        let summary: Vec<String> = events
            .iter()
            .filter_map(|event| match event {
                Event::PointerMoved { pos } => Some(format!("move({:.0},{:.0})", pos.x, pos.y)),
                Event::PointerPressed { pos, button } => {
                    Some(format!("press({button:?} at {:.0},{:.0})", pos.x, pos.y))
                }
                Event::PointerReleased { button, .. } => Some(format!("release({button:?})")),
                _ => None,
            })
            .collect();
        println!("{name}: {}", summary.join(", "));

        // The promotion keeps the contact's coordinates, so a tap in the window's middle
        // must arrive at the middle of the client area, in logical units.
        let mut origin = POINT::default();
        if !unsafe { ClientToScreen(window.hwnd(), &mut origin) }.as_bool() {
            return Err(String::from("ClientToScreen: no client origin"));
        }
        let wanted = Point {
            x: (middle.x - origin.x as f32) / scale,
            y: (middle.y - origin.y as f32) / scale,
        };
        ok &= expect(
            &format!("{name}: a tap presses the primary button where it landed"),
            events.iter().any(|event| matches!(
                event,
                Event::PointerPressed { pos, button: PointerButton::Primary }
                    if (pos.x - wanted.x).abs() < 2.0 && (pos.y - wanted.y).abs() < 2.0
            )),
        );
        ok &= expect(
            &format!("{name}: a drag moves the pointer"),
            events
                .iter()
                .any(|event| matches!(event, Event::PointerMoved { .. })),
        );
        ok &= expect(
            &format!("{name}: lifting releases the primary button"),
            events.iter().any(|event| matches!(
                event,
                Event::PointerReleased {
                    button: PointerButton::Primary,
                    ..
                }
            )),
        );
    }
    if !ok {
        return Err(String::from("one or more probes failed"));
    }
    println!("PASS: touch and pen drive the pointer through promotion");
    Ok(())
}

/// Pump `window` for `millis` and collect everything it produced.
#[cfg(windows)]
fn drain(window: &mut ui_platform_win32::Win32Window, millis: u64) -> Vec<ui_core::Event> {
    let start = std::time::Instant::now();
    let mut events = Vec::new();
    while start.elapsed().as_millis() < u128::from(millis) {
        events.extend(window.take_events());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    events
}
