//! Check what the presented buffer holds when Windows is showing HDR.
//!
//! The screen cannot judge this — a panel tone-maps and the desktop compositor may rescale —
//! so the check reads the back buffer back and compares it with the transform the chain
//! declared: `encode_for_pq_output` for a PQ surface, and linear light in 80-nit units for
//! scRGB, the same authority the Wayland probe uses. On an SDR output the check runs the
//! other way: the buffer must still hold the sRGB codes, with no trace of an HDR transform.
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
    use ui_core::colour::{
        OutputEncoding, SCRGB_WHITE_NITS, TransferFunction, encode_for_pq_output,
    };
    use ui_core::draw::{DrawList, RectPrim};
    use ui_core::geom::{Color, Corners, Rect};
    use ui_platform::present::SceneTarget;
    use ui_platform::text_pass::TextPass;
    use ui_platform::window::{ColourPolicy, SurfaceSpec};
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

    let spec = SurfaceSpec::window("crabsnap-win32-hdr", "crabsnap — Win32 HDR", (480, 320))
        .with_colour(ColourPolicy::Auto);
    let mut window = Win32Window::new(spec).map_err(|error| error.to_string())?;
    let mut presenter = Win32Presenter::new(&window).map_err(|error| error.to_string())?;
    let hdr = presenter.hdr();
    let encoding = presenter.hdr_encoding();
    let text = TextPass::isolated();
    match (hdr, encoding) {
        (Some(output), Some(encoding)) => println!(
            "HDR output ({encoding:?}): SDR white {} nits, black {} nits, peak {} nits",
            output.reference_nits, output.min_nits, output.peak_nits
        ),
        _ => println!("SDR output: the swap chain is 8-bit sRGB"),
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
        let expected = match (hdr, encoding) {
            (Some(output), Some(OutputEncoding::Pq)) => {
                encode_for_pq_output(linear, output.reference_nits, output.min_nits)
            }
            // scRGB's unit is 80 nits, and the readback stops at the surface's own 1.0.
            (Some(output), Some(OutputEncoding::ScRgb)) => {
                (linear * output.reference_nits / SCRGB_WHITE_NITS).min(1.0)
            }
            _ => code as f32 / 255.0,
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
    // the opposite: on a PQ surface white lands well away from the top of the range, and on
    // scRGB a mid grey lands where linear light puts it rather than where the sRGB code does.
    let mid = 128.0 / 255.0;
    let (moved, observed) = match (hdr, encoding) {
        (Some(output), Some(OutputEncoding::Pq)) => {
            let white = encode_for_pq_output(1.0, output.reference_nits, output.min_nits);
            (white < 0.75, format!("white at {white:.4}"))
        }
        (Some(output), Some(OutputEncoding::ScRgb)) => {
            let value = (TransferFunction::Srgb.decode(mid) * output.reference_nits
                / SCRGB_WHITE_NITS)
                .min(1.0);
            (
                (value - mid).abs() > 3.0 / 255.0,
                format!("mid grey at {value:.4}"),
            )
        }
        _ => (true, String::from("no transform to reach the buffer")),
    };
    if !moved {
        failures += 1;
    }
    println!(
        "[6] the transform reached the buffer: {observed} {}",
        if moved { "ok" } else { "FAIL" }
    );

    if failures > 0 {
        return Err(format!("{failures} check(s) failed"));
    }
    println!(
        "PASS: {}",
        match encoding {
            Some(OutputEncoding::Pq) => "the buffer holds PQ codes the output describes",
            Some(OutputEncoding::ScRgb) => "the buffer holds scRGB light the output describes",
            None => "the buffer holds sRGB codes, as an SDR output expects",
        }
    );
    Ok(())
}
