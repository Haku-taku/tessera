//! A Win32 window presenting the engine's scene through D3D11 and a DXGI swap chain.
//!
//! Run: `cargo run -p ui-platform-win32 --example window_scene --release`
//! Knobs: `WINDOW_SECONDS=n` closes after n seconds; `WINDOW_CHECK=1` reads the presented
//! buffer back, asserts the scene's own pixels, resizes the window and asserts them again;
//! `WINDOW_DUMP=path` writes the presented frame as a PPM.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("window_scene: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use ui_core::colour::{
        Chromaticities, LinearEncoding, SCRGB_WHITE_NITS, TransferFunction, WhiteAdaptation,
        encode_for_pq_output, gamut_matrix_adapted,
    };
    use ui_core::geom::{Color, Corners};
    use ui_core::{Id, NodeData, NodeKind, SizeClass};
    use ui_platform::view::{App, AppLoop};
    use ui_platform::window::SurfaceSpec;
    use ui_platform::{Container, Element};
    use ui_platform_win32::{HdrOutput, Win32Presenter, Win32Window};

    const BACKDROP: Color = Color::from_rgba8(20, 22, 28, 255);
    const RED: Color = Color::from_rgba8(255, 0, 0, 255);
    const GREEN: Color = Color::from_rgba8(0, 255, 0, 255);
    const BLUE: Color = Color::from_rgba8(0, 0, 255, 255);
    const ACCENT: Color = Color::from_rgba8(120, 190, 255, 255);
    const INK: Color = Color::from_rgba8(235, 238, 245, 255);

    /// The scene: three primaries prove channel order, and an animated bar proves the loop
    /// keeps presenting rather than stopping after the first frame.
    struct Demo;

    fn text(id: &'static str, value: &str, size: f32, colour: Color) -> Box<dyn Element> {
        let decl = ui_core::Decl {
            id: Id::str(id),
            kind: NodeKind::Text,
            shapes: Vec::new(),
            data: NodeData::Text {
                ink: colour,
                block: std::rc::Rc::new(ui_core::TextBlock {
                    runs: vec![ui_core::TextRun::new(
                        value,
                        ui_core::TextStyle::new(size),
                    )],
                    align: ui_core::TextAlign::Start,
                    max_lines: None,
                    line_gap: 0.0,
                    indent: 0.0,
                    hanging: 0.0,
                    atoms: Vec::new(),
                    input_base: None,
                    line_numbers: None,
                    structures: Vec::new(),
                }),
                caret: None,
                selection: None,
                composing: None,
                nowrap: false,
            },
            size_class: SizeClass {
                width: ui_core::AxisSize::hug(0.0, 900.0),
                height: ui_core::AxisSize::hug(size, size * 1.4),
                grow_x: ui_core::Grow::No,
                grow_y: ui_core::Grow::No,
            },
            ink_margin: ui_core::geom::Insets::ZERO,
            box_model: Default::default(),
            offset: ui_core::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: ui_core::Sense::NONE,
            disabled: false,
            grid: Default::default(),
            placement: Default::default(),
            env: None,
            children: Vec::new(),
        };
        ui_platform::from_decl(decl)
    }

    fn swatch(id: &'static str, colour: Color) -> Box<dyn Element> {
        Box::new(
            Container::new(Id::str(id), NodeKind::Stack)
                .size(SizeClass::fixed(64.0, 64.0))
                .background(colour, Corners::ZERO),
        )
    }

    impl App for Demo {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
            let width = cx.width() as f32;
            let height = cx.height() as f32;
            let phase = ((cx.now() * 1.2).sin() * 0.5 + 0.5) as f32;
            Box::new(
                Container::new(Id::str("root"), NodeKind::Column)
                    .size(SizeClass::fixed(width, height))
                    .background(BACKDROP, Corners::ZERO)
                    .child(Box::new(
                        Container::new(Id::str("primaries"), NodeKind::Row)
                            .gap(8.0)
                            .child(swatch("red", RED))
                            .child(swatch("green", GREEN))
                            .child(swatch("blue", BLUE)),
                    ))
                    .child(text("title", "crabsnap on Win32 / D3D11", 22.0, INK))
                    .child(Box::new(
                        Container::new(Id::str("bar"), NodeKind::Row)
                            .size(SizeClass::fixed(24.0 + phase * (width - 48.0), 20.0))
                            .background(ACCENT, Corners::all(10.0)),
                    )),
            )
        }
    }

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * width + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    }

    /// What the presenter took: the output it is on and how its chain encodes light.
    #[derive(Clone, Copy)]
    enum Target {
        Sdr,
        Hdr {
            output: HdrOutput,
            encoding: LinearEncoding,
        },
    }

    impl Target {
        fn of(presenter: &Win32Presenter) -> Self {
            match (presenter.hdr(), presenter.hdr_encoding()) {
                (Some(output), Some(encoding)) => Self::Hdr { output, encoding },
                _ => Self::Sdr,
            }
        }
    }

    /// The codes the presented buffer holds for `want`.
    ///
    /// A linear destination holds light rather than codes: PQ over BT.2020 on one chain, or
    /// scRGB in 80-nit units on another, where the readback stops at the surface's 1.0.
    fn expected(want: Color, target: Target) -> [u8; 4] {
        let alpha = (want.a * 255.0).round() as u8;
        let (output, encoding) = match target {
            Target::Sdr => {
                return [
                    (want.r * 255.0).round() as u8,
                    (want.g * 255.0).round() as u8,
                    (want.b * 255.0).round() as u8,
                    alpha,
                ];
            }
            Target::Hdr { output, encoding } => (output, encoding),
        };
        let linear = [
            TransferFunction::Srgb.decode(want.r),
            TransferFunction::Srgb.decode(want.g),
            TransferFunction::Srgb.decode(want.b),
        ];
        let gamut = gamut_matrix_adapted(
            Chromaticities::SRGB,
            match encoding {
                LinearEncoding::Pq => Chromaticities::BT2020,
                LinearEncoding::ScRgb => Chromaticities::SRGB,
            },
            WhiteAdaptation::Bradford,
        );
        let mut codes = [0u8; 3];
        for (channel, row) in codes.iter_mut().zip(gamut) {
            let mapped: f32 = row.iter().zip(linear).map(|(m, l)| m * l).sum();
            let code = match encoding {
                LinearEncoding::Pq => {
                    encode_for_pq_output(mapped, output.reference_nits, output.min_nits)
                }
                LinearEncoding::ScRgb => {
                    (mapped * output.reference_nits / SCRGB_WHITE_NITS).clamp(0.0, 1.0)
                }
            };
            *channel = (code * 255.0).round() as u8;
        }
        [codes[0], codes[1], codes[2], alpha]
    }

    fn near(got: [u8; 4], want: Color, target: Target, tolerance: u8) -> bool {
        got.iter()
            .zip(expected(want, target))
            .all(|(got, want)| got.abs_diff(want) <= tolerance)
    }

    /// Assert the presented buffer at the scene's own points, in logical coordinates.
    fn verify(
        pixels: &[u8],
        size: (u32, u32),
        scale: f32,
        target: Target,
    ) -> Result<(), String> {
        let (width, height) = size;
        let at = |x: f32, y: f32| {
            (
                (x * scale).round().clamp(0.0, (width - 1) as f32) as u32,
                (y * scale).round().clamp(0.0, (height - 1) as f32) as u32,
            )
        };
        let logical = (width as f32 / scale, height as f32 / scale);
        let probes = [
            ("[1] the red swatch is red", 32.0, 32.0, RED),
            ("[2] the green swatch is green", 104.0, 32.0, GREEN),
            ("[3] the blue swatch is blue", 176.0, 32.0, BLUE),
            ("[4] the backdrop is the scene's", logical.0 - 8.0, logical.1 - 8.0, BACKDROP),
        ];
        let mut failures = 0;
        for (name, x, y, want) in probes {
            let (px, py) = at(x, y);
            let got = pixel(pixels, width, px, py);
            let ok = near(got, want, target, 12);
            println!(
                "{name:40} {} (at {px},{py}: {got:?})",
                if ok { "OK" } else { "FAIL" }
            );
            if !ok {
                failures += 1;
            }
        }
        if failures > 0 {
            return Err(format!("{failures} pixel probe(s) failed"));
        }
        Ok(())
    }

    fn write_ppm(path: &str, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
        let mut file = std::fs::File::create(path)
            .map_err(|error| format!("{path}: {error}"))?;
        write!(file, "P6\n{width} {height}\n255\n")
            .map_err(|error| format!("{path}: {error}"))?;
        for texel in pixels.as_chunks::<4>().0 {
            file.write_all(&[texel[0], texel[1], texel[2]])
                .map_err(|error| format!("{path}: {error}"))?;
        }
        Ok(())
    }

    let spec = SurfaceSpec::window(
        "crabsnap-win32-scene",
        "crabsnap — Win32 / D3D11",
        (960, 640),
    );
    let mut window = Win32Window::new(spec).map_err(|error| error.to_string())?;
    let mut presenter = Win32Presenter::new(&window).map_err(|error| error.to_string())?;
    let target = Target::of(&presenter);
    match target {
        Target::Hdr { output, encoding } => println!(
            "the output is HDR ({encoding:?}): SDR white {} nits, peak {} nits",
            output.reference_nits, output.peak_nits
        ),
        Target::Sdr => println!("the output is SDR: 8-bit sRGB"),
    }
    let mut app = AppLoop::with_stack(Demo, &[]);
    let seconds = std::env::var("WINDOW_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok());
    let check = std::env::var_os("WINDOW_CHECK").is_some();
    let dump = std::env::var_os("WINDOW_DUMP").and_then(|path| path.into_string().ok());
    // Check mode also proves the resize path: same window, new extent, new swap chain.
    let resized: Option<(u32, u32)> = check.then_some((720, 480));
    let started = Instant::now();
    let mut frames = 0u32;
    // 0: the opening extent is unproven; 1: waiting for the resized extent; 2: done.
    let mut stage = 0u32;
    let mut dumped = false;
    while window.alive() {
        if seconds.is_some_and(|limit| started.elapsed().as_secs_f32() >= limit) {
            break;
        }
        window.pump();
        let size = window.client_size();
        if size.0 == 0 || size.1 == 0 {
            std::thread::sleep(Duration::from_millis(16));
            continue;
        }
        let scale = window.scale();
        presenter
            .resize(size, scale)
            .map_err(|error| error.to_string())?;
        // Layout runs in logical units; text and primitives are rasterized at this scale.
        app.set_scale(scale);
        // The second frame proves the loop repeats, not just that startup drew once.
        let wants_capture = match stage {
            0 => frames >= 1,
            1 => true,
            _ => false,
        } && (check || dump.is_some());
        if wants_capture {
            presenter.capture_next_frame();
        }
        let outcome = app
            .frame_scene(&mut presenter, None)
            .map_err(|error| error.to_string())?;
        let Some(stats) = outcome.presented else {
            // A still scene presents nothing; sleep instead of spinning on an idle loop.
            std::thread::sleep(Duration::from_millis(4));
            continue;
        };
        frames += 1;
        if frames == 1 {
            println!(
                "first frame: {} draw calls, band {:?}, surface {size:?}",
                stats.draw_calls, stats.band
            );
        }
        let Some(pixels) = wants_capture.then(|| presenter.take_capture()).flatten() else {
            continue;
        };
        let extent = if stage == 0 { "opening" } else { "resized" };
        if check {
            verify(&pixels, size, scale, target)?;
            println!("PASS: the swap chain holds the scene at the {extent} size");
        }
        if let Some(path) = &dump
            && !dumped
        {
            write_ppm(path, size.0, size.1, &pixels)?;
            println!("wrote {path}");
            dumped = true;
        }
        match (stage, resized) {
            (0, Some(next)) => {
                window
                    .set_client_size(next)
                    .map_err(|error| error.to_string())?;
                stage = 1;
            }
            _ => stage = 2,
        }
        if stage == 2 && seconds.is_none() {
            break;
        }
    }
    println!("presented {frames} frame(s)");
    Ok(())
}
