//! Check what the presented buffer holds when Windows is showing HDR.
//!
//! The screen cannot judge this — a panel tone-maps and the desktop compositor may rescale —
//! so the check reads the back buffer back and compares it with `encode_for_pq_output`, the
//! same authority the Wayland probe uses. On an SDR output the check runs the other way:
//! the buffer must still hold the sRGB codes, with no trace of the PQ transform.
//!
//! Run: `cargo run -p ui-platform-win32 --example hdr --release`

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("hdr: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_core::Damage;
    use ui_core::NodeId;
    use ui_core::colour::{TransferFunction, encode_for_pq_output};
    use ui_core::draw::{DrawList, RectPrim};
    use ui_core::geom::{Color, Corners, Rect};
    use ui_platform::present::SceneTarget;
    use ui_platform::text_pass::TextPass;
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::{Win32Presenter, Win32Window};

    /// sRGB codes the buffer is read at: black, dark, mid, bright, white.
    const CODES: [u8; 5] = [0, 32, 128, 192, 255];

    /// The window filled with one sRGB grey.
    fn flat(width: u32, height: u32, code: u8) -> DrawList {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, width as f32, height as f32));
        let value = code as f32 / 255.0;
        list.push_rect(
            NodeId::from_raw(1, 0),
            RectPrim {
                rect: Rect::new(0.0, 0.0, width as f32, height as f32),
                radius: Corners::ZERO,
                fill: Color::rgba(value, value, value, 1.0),
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
        list
    }

    let spec = SurfaceSpec::window("crabsnap-win32-hdr", "crabsnap — Win32 HDR", (480, 320));
    let mut window = Win32Window::new(spec).map_err(|error| error.to_string())?;
    let mut presenter = Win32Presenter::new(&window).map_err(|error| error.to_string())?;
    let hdr = presenter.hdr();
    let text = TextPass::isolated();
    match hdr {
        Some(output) => println!(
            "HDR output: SDR white {} nits, black {} nits, peak {} nits",
            output.reference_nits, output.min_nits, output.peak_nits
        ),
        None => println!("SDR output: the swap chain is 8-bit sRGB"),
    }

    let (width, height) = presenter.size();
    let mut failures = 0;
    println!("  sRGB  expected  actual   delta");
    for code in CODES {
        window.pump();
        let list = flat(width, height, code);
        presenter.capture_next_frame();
        presenter
            .present_scene(&list, &text, &Damage::Full, &[])
            .map_err(|error| format!("present: {error}"))?;
        let pixels = presenter
            .take_capture()
            .ok_or_else(|| String::from("the presenter captured no frame"))?;
        let index = ((height / 2) * width + width / 2) as usize * 4;
        let actual = pixels[index] as f32 / 255.0;
        let linear = TransferFunction::Srgb.decode(code as f32 / 255.0);
        let expected = match hdr {
            Some(output) => encode_for_pq_output(linear, output.reference_nits, output.min_nits),
            None => code as f32 / 255.0,
        };
        // One 8-bit step plus the shader's own rounding.
        let delta = (actual - expected).abs();
        let ok = delta <= 3.0 / 255.0;
        if !ok {
            failures += 1;
        }
        println!(
            "  {code:5}  {expected:8.4}  {actual:6.4}  {:+7.4}  {}",
            actual - expected,
            if ok { "ok" } else { "FAIL" }
        );
    }

    // A transform that did nothing would pass every "same as the input" check above, so pin
    // the opposite: on an HDR surface white must land well away from the top of the range.
    let white = match hdr {
        Some(output) => encode_for_pq_output(1.0, output.reference_nits, output.min_nits),
        None => 1.0,
    };
    let moved = hdr.is_none() || white < 0.75;
    if !moved {
        failures += 1;
    }
    println!(
        "[6] white is not the top of the range: {white:.4} {}",
        if moved { "ok" } else { "FAIL" }
    );

    if failures > 0 {
        return Err(format!("{failures} check(s) failed"));
    }
    println!(
        "PASS: {}",
        match hdr {
            Some(_) => "the buffer holds PQ codes the output describes",
            None => "the buffer holds sRGB codes, as an SDR output expects",
        }
    );
    Ok(())
}
