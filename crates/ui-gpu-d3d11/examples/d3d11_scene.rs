//! A D3D11 smoke test: every program compiles, and one scene lands where the scene says.
//!
//! Run: `cargo run -p ui-gpu-d3d11 --example d3d11_scene`

#[cfg(windows)]
fn main() {
    windows_main();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the D3D11 backend is Windows only");
}

#[cfg(windows)]
fn windows_main() {
    use ui_core::NodeId;
    use ui_core::damage::Damage;
    use ui_core::draw::{DrawList, EllipsePrim, LinePrim, RectPrim, Sector, SectorPrim};
    use ui_core::geom::{Color, Corners, Point, Rect};
    use ui_gpu_d3d11::D3d11Device;
    use ui_gpu_scene::{Device, GpuRenderer, RenderTarget, TargetFormat, TargetId};

    const W: u32 = 160;
    const H: u32 = 120;

    /// The dark fill every part of the scene that draws nothing shows.
    const BACKDROP: Color = Color::from_rgba8(20, 22, 28, 255);
    const ORANGE: Color = Color::from_rgba8(255, 120, 40, 255);
    const BLUE: Color = Color::from_rgba8(90, 200, 255, 255);
    const GREEN: Color = Color::from_rgba8(120, 255, 120, 255);

    /// A target this example owns; the painter only needs its id and size.
    struct Offscreen {
        target: TargetId,
        width: u32,
        height: u32,
    }

    impl RenderTarget for Offscreen {
        fn size(&self) -> (u32, u32) {
            (self.width, self.height)
        }

        fn target_id(&self) -> TargetId {
            self.target
        }
    }

    fn quad(rect: Rect, fill: Color) -> RectPrim {
        RectPrim {
            rect,
            radius: Corners::ZERO,
            fill,
            gradient: None,
            border: None,
            border_gradient: None,
            notch: None,
        }
    }

    /// Every primitive the painter has, laid out so no two of them share a test point.
    fn scene() -> DrawList {
        let owner = NodeId::from_raw(1, 0);
        let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
        list.push_rect(
            owner,
            quad(Rect::new(0.0, 0.0, W as f32, H as f32), BACKDROP),
        );
        // A rounded box: its corner carries the SDF, its middle the fill.
        list.push_rect(
            owner,
            RectPrim {
                radius: Corners::all(8.0),
                ..quad(Rect::new(16.0, 16.0, 40.0, 40.0), Color::WHITE)
            },
        );
        list.push_ellipse(
            owner,
            EllipsePrim {
                bounds: Rect::new(100.0, 16.0, 44.0, 44.0),
                centre: Point { x: 122.0, y: 38.0 },
                radii: (20.0, 20.0),
                thickness: 0.0,
                fill: ORANGE,
                colour: Color::from_rgba8(0, 0, 0, 0),
            },
        );
        list.push_line(
            owner,
            LinePrim {
                from: Point { x: 20.0, y: 70.0 },
                to: Point { x: 70.0, y: 70.0 },
                thickness: 4.0,
                colour: BLUE,
            },
        );
        // A quarter turn clockwise from 3 o'clock, round caps on.
        list.push_sector(
            owner,
            SectorPrim {
                bounds: Rect::new(42.0, 78.0, 36.0, 36.0),
                colour: GREEN,
                sector: Sector {
                    centre: Point { x: 60.0, y: 96.0 },
                    radius: 18.0,
                    thickness: 6.0,
                    trim: (0.0, 0.25),
                    round_caps: true,
                },
            },
        );
        // The layer is much wider than its content, so a flipped composite shows.
        list.push_layer(
            Rect::new(88.0, 66.0, 64.0, 44.0),
            vec![ui_core::Effect::blur(6, 2.0)],
            Corners::ZERO,
        );
        list.push_rect(owner, quad(Rect::new(94.0, 70.0, 20.0, 8.0), Color::WHITE));
        list.pop_layer();
        list
    }

    fn report(name: &str, ok: bool, failures: &mut u32) {
        println!("{name:52} {}", if ok { "OK" } else { "FAIL" });
        if !ok {
            *failures += 1;
        }
    }

    /// Straight RGBA8 at a point of the readback.
    fn pixel(data: &[u8], x: u32, y: u32) -> [u8; 4] {
        let index = ((y * W + x) * 4) as usize;
        [
            data[index],
            data[index + 1],
            data[index + 2],
            data[index + 3],
        ]
    }

    fn rgba(colour: Color) -> [u8; 4] {
        [
            (colour.r * 255.0).round() as u8,
            (colour.g * 255.0).round() as u8,
            (colour.b * 255.0).round() as u8,
            (colour.a * 255.0).round() as u8,
        ]
    }

    fn near(pixel: [u8; 4], colour: Color, tolerance: u8) -> bool {
        pixel
            .iter()
            .zip(rgba(colour))
            .all(|(got, want)| got.abs_diff(want) <= tolerance)
    }

    fn brightness(pixel: [u8; 4]) -> u16 {
        u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2])
    }

    let mut device = D3d11Device::new().expect("D3D11 device");
    let (target, _sampled) = device
        .create_target(W, H, TargetFormat::Rgba8)
        .expect("target");
    let offscreen = Offscreen {
        target,
        width: W,
        height: H,
    };
    // Building the renderer compiles all eighteen programs: a shader error surfaces here.
    let mut gpu = GpuRenderer::new(&mut device).expect("renderer");
    gpu.clear(&mut device, &offscreen, [0.0, 0.0, 0.0, 0.0]);
    let stats = gpu
        .render_frame(&mut device, &offscreen, &scene(), &Damage::Full)
        .expect("render");
    println!(
        "{} draw calls over {} rects and {} frosts",
        stats.draw_calls, stats.rects, stats.frosts
    );
    let pixels = gpu.read_pixels(&mut device, &offscreen).expect("readback");
    let mut failures = 0;

    let checks = [
        (
            "[1] a rect fills where the scene put it",
            36,
            36,
            Color::WHITE,
            2,
        ),
        ("[2] its rounded corner is cut away", 55, 16, BACKDROP, 6),
        ("[3] the background shows outside it", 8, 8, BACKDROP, 2),
        ("[4] the ellipse fills its own centre", 122, 38, ORANGE, 6),
        ("[5] the line covers its path", 45, 70, BLUE, 6),
        ("[6] and nothing away from it", 45, 80, BACKDROP, 2),
    ];
    for (name, x, y, want, tolerance) in checks {
        let got = pixel(&pixels, x, y);
        let ok = near(got, want, tolerance);
        report(name, ok, &mut failures);
        if !ok {
            println!("      at ({x}, {y}) got {got:?}, want {:?}", rgba(want));
        }
    }

    // The quarter turn runs from 3 o'clock to 6 o'clock: 45 degrees is on the ring, while
    // 12 o'clock is the same ring with no sweep over it.
    let arc = pixel(&pixels, 71, 107);
    let gap = pixel(&pixels, 60, 80);
    report(
        "[7] the sector's arc is drawn where its sweep is",
        brightness(arc) > brightness(gap) + 60,
        &mut failures,
    );
    println!("      arc {arc:?} against gap {gap:?}");

    // The content sits in the layer's top left; the far side stays the backdrop, which is
    // only true if the composite kept the layer's own orientation.
    let content = pixel(&pixels, 100, 74);
    let far = pixel(&pixels, 140, 100);
    report(
        "[8] the blurred layer keeps its content at the top",
        brightness(content) > brightness(rgba(BACKDROP)) + 60 && near(far, BACKDROP, 2),
        &mut failures,
    );
    println!("      content {content:?}, far side {far:?}");

    if failures > 0 {
        eprintln!("\n{failures} invariant(s) violated");
        std::process::exit(1);
    }
    println!("\nD3D11 draws the scene where the scene says");
}
