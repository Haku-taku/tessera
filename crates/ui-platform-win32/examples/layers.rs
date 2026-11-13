//! What a layer surface becomes on Win32: a popup on its output's monitor, placed by the
//! spec's rectangle, held at the spec's level, and shaped to its input region.
//!
//! Run: `cargo run -p ui-platform-win32 --example layers`
//!
//! `WindowFromPoint` is the hit test the desktop itself performs, so the click-through check
//! goes through the same routing a moving cursor would.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("layers: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

/// The strip an auto-hidden taskbar keeps for itself, in physical pixels.
#[cfg(windows)]
const AUTO_HIDE_STRIP: i32 = 2;

/// The monitor edge an auto-hidden taskbar hides against, when it hides on `monitor`.
#[cfg(windows)]
fn taskbar_edge(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Option<u32> {
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};
    use windows::Win32::UI::Shell::{
        ABM_GETSTATE, ABM_GETTASKBARPOS, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage,
    };
    let mut state = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..APPBARDATA::default()
    };
    if unsafe { SHAppBarMessage(ABM_GETSTATE, &mut state) } as u32 & ABS_AUTOHIDE == 0 {
        return None;
    }
    let mut position = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..APPBARDATA::default()
    };
    unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut position) };
    (unsafe { MonitorFromRect(&position.rc, MONITOR_DEFAULTTONULL) } == monitor)
        .then_some(position.uEdge)
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_core::geom::{Insets, Rect};
    use ui_platform::window::{
        Anchor, AutoHide, ExclusiveZone, KeyboardMode, Layer, LayerSpec, SurfaceSpec,
    };
    use ui_platform_win32::Win32Window;
    use windows::Win32::Foundation::{LPARAM, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::Shell::{ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, mouse_event,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, MA_ACTIVATE,
        MA_NOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetCursorPos, SetWindowPos,
        WM_MOUSEACTIVATE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, WindowFromPoint,
    };

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:56} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;

    // A window the manager places, opened first: it is what makes the process per-monitor
    // DPI aware before the monitor is read, and it gives click-through somewhere to land.
    let mut window = Win32Window::new(SurfaceSpec::window(
        "crabsnap-win32-layers",
        "crabsnap — layers probe",
        (360, 240),
    ))
    .map_err(|error| error.to_string())?;

    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(String::from("GetMonitorInfoW: no primary monitor"));
    }
    let (mut dpi_x, mut dpi_y) = (0, 0);
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
        .map_err(|error| format!("GetDpiForMonitor: {error}"))?;
    let scale = dpi_x.max(96) as f32 / 96.0;
    let at = |logical: f32| (logical * scale).round() as i32;
    let monitor_rect = info.rcMonitor;
    // Large enough to lie under every point of the layer the probe tests.
    unsafe {
        SetWindowPos(
            window.hwnd(),
            None,
            monitor_rect.left + at(160.0),
            monitor_rect.top + at(96.0),
            at(700.0),
            at(480.0),
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .map_err(|error| format!("SetWindowPos: {error}"))?;

    // A toolbar-shaped layer: top-left, a fixed size, and a margin that is its position.
    let mut toolbar = Win32Window::new(SurfaceSpec::layer(
        "crabsnap-win32-layers",
        "probe.toolbar",
        LayerSpec {
            output: None,
            level: Layer::Overlay,
            anchor: Anchor {
                top: true,
                bottom: false,
                left: true,
                right: false,
            },
            size: Some((320, 180)),
            margin: Insets::new(40.0, 24.0, 0.0, 0.0),
            exclusive: ExclusiveZone::Ignore,
            keyboard: KeyboardMode::None,
            auto_hide: AutoHide::Yield,
        },
    ))
    .map_err(|error| error.to_string())?;
    let hwnd = toolbar.hwnd();

    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let has = |style: u32| ex_style & style != 0;
    ok &= expect("an overlay layer is topmost", has(WS_EX_TOPMOST.0));
    ok &= expect(
        "a layer stays out of Alt+Tab and the taskbar",
        has(WS_EX_TOOLWINDOW.0),
    );
    ok &= expect(
        "keyboard None never activates the surface",
        has(WS_EX_NOACTIVATE.0),
    );

    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|error| format!("GetWindowRect: {error}"))?;
    ok &= expect(
        "a top-left anchor puts the margin at the position",
        rect.left == monitor_rect.left + at(40.0) && rect.top == monitor_rect.top + at(24.0),
    );
    ok &= expect(
        "a fixed size is honoured in logical units",
        rect.right - rect.left == at(320.0) && rect.bottom - rect.top == at(180.0),
    );
    ok &= expect(
        "a borderless popup's client area is its whole rect",
        toolbar.logical_size() == (320, 180),
    );

    // A layer that names its output belongs on that monitor, and a name no monitor answers to
    // falls back to the primary one: an unplugged display must not leave a surface nowhere.
    let mut described = MONITORINFOEXW::default();
    described.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !unsafe {
        GetMonitorInfoW(monitor, std::ptr::from_mut(&mut described).cast::<MONITORINFO>())
    }
    .as_bool()
    {
        return Err(String::from("GetMonitorInfoW: no name for the primary monitor"));
    }
    let end = described
        .szDevice
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(described.szDevice.len());
    let name = String::from_utf16_lossy(&described.szDevice[..end]);
    for (pattern, output) in [
        (
            "a layer naming an output covers that output's monitor",
            name.clone(),
        ),
        (
            "a name no monitor has falls back to the primary one",
            String::from(r"\\.\DISPLAY999"),
        ),
    ] {
        let placed = Win32Window::new(SurfaceSpec::layer(
            "crabsnap-win32-layers",
            "probe.output",
            LayerSpec {
                output: Some(output),
                auto_hide: AutoHide::Cover,
                ..LayerSpec::default()
            },
        ))
        .map_err(|error| error.to_string())?;
        let mut rect = RECT::default();
        unsafe { GetWindowRect(placed.hwnd(), &mut rect) }
            .map_err(|error| format!("GetWindowRect: {error}"))?;
        ok &= expect(pattern, rect == monitor_rect);
    }

    // A surface whose picture is the screen asks for the whole monitor: two rows short would
    // squeeze that picture, and a pointer would name a pixel it is not over.
    let covering = Win32Window::new(SurfaceSpec::layer(
        "crabsnap-win32-layers",
        "probe.covering",
        LayerSpec {
            auto_hide: AutoHide::Cover,
            ..LayerSpec::default()
        },
    ))
    .map_err(|error| error.to_string())?;
    let mut whole = RECT::default();
    unsafe { GetWindowRect(covering.hwnd(), &mut whole) }
        .map_err(|error| format!("GetWindowRect: {error}"))?;
    ok &= expect(
        "a surface that covers the edge takes the whole monitor",
        (whole.left, whole.top, whole.right, whole.bottom)
            == (
                monitor_rect.left,
                monitor_rect.top,
                monitor_rect.right,
                monitor_rect.bottom,
            ),
    );
    // It is topmost over the whole monitor, so it goes before the hit tests below.
    drop(covering);

    // One interactive patch; everything else on the layer must let the desktop through.
    toolbar.set_input_region(&[Rect::new(10.0, 10.0, 100.0, 40.0)]);
    let screen = |x: f32, y: f32| POINT {
        x: rect.left + at(x),
        y: rect.top + at(y),
    };
    let inside = unsafe { WindowFromPoint(screen(60.0, 30.0)) };
    let outside = unsafe { WindowFromPoint(screen(250.0, 150.0)) };
    ok &= expect("a consumed area belongs to the layer", inside == hwnd);
    ok &= expect(
        "the rest of the layer is click-through",
        outside == window.hwnd(),
    );
    let visual = unsafe { GetWindowLongPtrW(toolbar.visual_hwnd(), GWL_EXSTYLE) } as u32;
    let transparent = WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0;
    ok &= expect(
        "the painted window is the one the hit test passes over",
        toolbar.visual_hwnd() != hwnd && visual & transparent == transparent,
    );

    let activation = unsafe {
        SendMessageW(
            hwnd,
            WM_MOUSEACTIVATE,
            Some(WPARAM(hwnd.0 as usize)),
            Some(LPARAM(0)),
        )
    };
    ok &= expect(
        "a keyboard None click does not activate",
        activation.0 == MA_NOACTIVATE as isize,
    );

    // The pointer's visit: the shape decides where it is heard, and the leave a shaped
    // window cannot report has to come from the backend's own watch.
    let cursor = |x: f32, y: f32| {
        let point = screen(x, y);
        unsafe { SetCursorPos(point.x, point.y) }
            .map_err(|error| format!("SetCursorPos: {error}"))
    };
    cursor(60.0, 30.0)?;
    let moved = drain(&mut toolbar, 300)
        .iter()
        .any(|event| matches!(event, ui_core::Event::PointerMoved { .. }));
    ok &= expect("a move inside the shape reaches the surface", moved);
    cursor(250.0, 150.0)?;
    let left = drain(&mut toolbar, 400)
        .iter()
        .any(|event| matches!(event, ui_core::Event::PointerLeft));
    ok &= expect("leaving the shape is reported as a leave", left);

    // A window the manager places keeps its decoration and its ordinary stacking.
    let ex_style = unsafe { GetWindowLongPtrW(window.hwnd(), GWL_EXSTYLE) } as u32;
    ok &= expect(
        "a window keeps the manager's frame and stacking",
        ex_style & (WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0) == 0,
    );
    let activation = unsafe {
        SendMessageW(
            window.hwnd(),
            WM_MOUSEACTIVATE,
            Some(WPARAM(window.hwnd().0 as usize)),
            Some(LPARAM(0)),
        )
    };
    ok &= expect(
        "a window activates on click through the default procedure",
        activation.0 == MA_ACTIVATE as isize,
    );

    // An exclusive layer asks for the keyboard and activates on a click anywhere.
    let exclusive = Win32Window::new(SurfaceSpec::layer(
        "crabsnap-win32-layers",
        "probe.overlay",
        LayerSpec {
            level: Layer::Overlay,
            keyboard: KeyboardMode::Exclusive,
            ..LayerSpec::default()
        },
    ))
    .map_err(|error| error.to_string())?;
    let mut overlay = RECT::default();
    unsafe { GetWindowRect(exclusive.hwnd(), &mut overlay) }
        .map_err(|error| format!("GetWindowRect: {error}"))?;
    // The shell hides an auto-hidden taskbar behind a strip of the monitor edge and will not
    // reveal it again while a window covers that strip, so a surface that fills the monitor
    // has to leave exactly that much to the desktop.
    let (mut left, mut top) = (monitor_rect.left, monitor_rect.top);
    let (mut right, mut bottom) = (monitor_rect.right, monitor_rect.bottom);
    match taskbar_edge(monitor) {
        Some(ABE_TOP) => top += AUTO_HIDE_STRIP,
        Some(ABE_BOTTOM) => bottom -= AUTO_HIDE_STRIP,
        Some(ABE_LEFT) => left += AUTO_HIDE_STRIP,
        Some(ABE_RIGHT) => right -= AUTO_HIDE_STRIP,
        _ => {}
    }
    ok &= expect(
        "an all-edge anchor takes the monitor, less the auto-hide strip",
        (overlay.left, overlay.top, overlay.right, overlay.bottom) == (left, top, right, bottom),
    );
    let ex_style = unsafe { GetWindowLongPtrW(exclusive.hwnd(), GWL_EXSTYLE) } as u32;
    ok &= expect(
        "an exclusive layer can take focus",
        ex_style & WS_EX_NOACTIVATE.0 == 0,
    );
    let activation = unsafe {
        SendMessageW(
            exclusive.hwnd(),
            WM_MOUSEACTIVATE,
            Some(WPARAM(exclusive.hwnd().0 as usize)),
            Some(LPARAM(0)),
        )
    };
    ok &= expect(
        "a click on an exclusive layer activates it",
        activation.0 == MA_ACTIVATE as isize,
    );
    // It covers the monitor and claims all of it; the next probe clicks through to other
    // windows, so it has to go first.
    drop(exclusive);

    // The one thing a pointer-following surface cannot get from the engine: a still pointer
    // builds no frame, so the hand-back has to happen where the window sees the leave.
    let mut following = Win32Window::new(SurfaceSpec::layer(
        "crabsnap-win32-layers",
        "probe.follow",
        LayerSpec {
            output: None,
            level: Layer::Overlay,
            anchor: Anchor {
                top: true,
                bottom: false,
                left: true,
                right: false,
            },
            size: Some((240, 160)),
            margin: Insets::new(600.0, 400.0, 0.0, 0.0),
            exclusive: ExclusiveZone::Ignore,
            keyboard: KeyboardMode::FollowPointer,
            auto_hide: AutoHide::Yield,
        },
    ))
    .map_err(|error| error.to_string())?;
    let mut patch = RECT::default();
    unsafe { GetWindowRect(following.hwnd(), &mut patch) }
        .map_err(|error| format!("GetWindowRect: {error}"))?;
    // Everything it claims: this probe is about the leave, not the shape.
    following.set_input_region(&[Rect::new(0.0, 0.0, 240.0, 160.0)]);
    let mut managed = RECT::default();
    unsafe { GetWindowRect(window.hwnd(), &mut managed) }
        .map_err(|error| format!("GetWindowRect: {error}"))?;
    let foreground = || unsafe { GetForegroundWindow() };
    let click = |at: POINT| -> Result<(), String> {
        unsafe { SetCursorPos(at.x, at.y) }
            .map_err(|error| format!("SetCursorPos: {error}"))?;
        std::thread::sleep(std::time::Duration::from_millis(150));
        unsafe {
            mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
            mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        Ok(())
    };
    let middle = |rect: RECT| POINT {
        x: (rect.left + rect.right) / 2,
        y: (rect.top + rect.bottom) / 2,
    };
    // A click is queued, and the activation it asks for happens when the queue is dispatched;
    // the pump between a click and its answer is what gives the message procedure its turn.
    click(middle(managed))?;
    drain(&mut window, 200);
    ok &= expect(
        "a click on the manager's window focuses it",
        foreground() == window.hwnd(),
    );
    click(middle(patch))?;
    drain(&mut window, 200);
    ok &= expect(
        "a pointer-following layer takes the keyboard on a click",
        foreground() == following.hwnd(),
    );
    // A point of the manager's window the layer does not cover.
    let away = POINT {
        x: managed.left + at(40.0),
        y: managed.top + at(40.0),
    };
    unsafe { SetCursorPos(away.x, away.y) }
        .map_err(|error| format!("SetCursorPos: {error}"))?;
    drain(&mut window, 400);
    ok &= expect(
        "leaving it hands the keyboard back",
        foreground() == window.hwnd(),
    );

    if !ok {
        return Err(String::from("one or more probes failed"));
    }
    println!(
        "PASS: layer at ({}, {}) {}x{} physical, scale {scale}",
        rect.left - monitor_rect.left,
        rect.top - monitor_rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    );
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
