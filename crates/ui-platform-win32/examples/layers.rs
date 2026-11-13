//! What a layer surface becomes on Win32: a popup on the primary monitor, placed by the
//! spec's rectangle, held at the spec's level, and transparent outside its input region.
//!
//! Run: `cargo run -p ui-platform-win32 --example layers`
//!
//! `WindowFromPoint` is the hit test the desktop itself performs, so the click-through check
//! goes through the same `WM_NCHITTEST` a moving cursor would.

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

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_core::geom::{Insets, Rect};
    use ui_platform::window::{Anchor, ExclusiveZone, KeyboardMode, Layer, LayerSpec, SurfaceSpec};
    use ui_platform_win32::Win32Window;
    use windows::Win32::Foundation::{LPARAM, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, MA_ACTIVATE, MA_NOACTIVATE, SWP_NOACTIVATE,
        SWP_NOZORDER, SendMessageW, SetWindowPos, WM_MOUSEACTIVATE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WindowFromPoint,
    };

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:56} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;

    // A window the manager places, opened first: it is what makes the process per-monitor
    // DPI aware before the monitor is read, and it gives click-through somewhere to land.
    let window = Win32Window::new(SurfaceSpec::window(
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
    ok &= expect(
        "an all-edge anchor takes the whole monitor",
        overlay.left == monitor_rect.left
            && overlay.top == monitor_rect.top
            && overlay.right == monitor_rect.right
            && overlay.bottom == monitor_rect.bottom,
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
