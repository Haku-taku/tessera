//! A scrim dims a code and an HDR highlight by the same step, checked against PQ codes.
//!
//! Run: `cargo run -p ui-gpu-d3d11 --example gpu_scrim_scene`

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
    use ui_core::colour::{Destination, Domain, OutputEncoding, ToneMapping};
    use ui_core::damage::Damage;
    use ui_core::draw::{BackdropEntry, BackdropMaterial, DrawList, RectPrim, Scrim};
    use ui_core::geom::{Color, Corners, Rect};
    use ui_gpu_d3d11::D3d11Device;
    use ui_gpu_scene::{Device, GpuRenderer, RenderTarget, TargetFormat, TargetId};

    const W: u32 = 64;
    const H: u32 = 64;
    const REFERENCE: f32 = 240.0;
    const PEAK: f32 = 470.0;
    /// The sRGB code of 2670 nits against a 240-nit reference.
    const HIGHLIGHT: f32 = 2.824;

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

    /// Left half an SDR mid grey, right half a highlight well past the output's peak.
    fn scene(scrim: Option<(f32, Rect)>) -> DrawList {
        let owner = NodeId::from_raw(1, 0);
        let full = Rect::new(0.0, 0.0, W as f32, H as f32);
        let mut list = DrawList::new(full);
        list.push_rect(
            owner,
            quad(Rect::new(0.0, 0.0, W as f32 / 2.0, H as f32), Color::rgba(0.5, 0.5, 0.5, 1.0)),
        );
        list.push_rect(
            owner,
            quad(
                Rect::new(W as f32 / 2.0, 0.0, W as f32 / 2.0, H as f32),
                Color::rgba(HIGHLIGHT, HIGHLIGHT, HIGHLIGHT, 1.0),
            ),
        );
        if let Some((opacity, hole)) = scrim {
            list.push_backdrop(BackdropEntry {
                rect: full,
                clip: full,
                corners: Corners::ZERO,
                radius: 0,
                sigma: 0.0,
                material: BackdropMaterial::Scrim(Scrim { opacity, hole }),
            });
        }
        list
    }

    fn pixel(data: &[u8], x: u32, y: u32) -> u8 {
        data[((y * W + x) * 4) as usize]
    }

    let mut device = D3d11Device::new().expect("D3D11 device");
    let mut gpu = GpuRenderer::new(&mut device).expect("renderer");
    gpu.set_destination(Destination {
        domain: Domain::Extended {
            reference_nits: REFERENCE,
            min_nits: 0.0,
            peak_nits: PEAK,
            encoding: OutputEncoding::Pq,
        },
        tone: ToneMapping::Clip,
    });
    let (target, _sampled) = device
        .create_target(W, H, TargetFormat::Rgba16F)
        .expect("target");
    let offscreen = Offscreen {
        target,
        width: W,
        height: H,
    };

    let mut read = |list: &DrawList| -> Vec<u8> {
        gpu.clear(&mut device, &offscreen, [0.0, 0.0, 0.0, 0.0]);
        gpu.render_frame(&mut device, &offscreen, list, &Damage::Full)
            .expect("render");
        gpu.read_pixels(&mut device, &offscreen).expect("readback")
    };

    let plain = read(&scene(None));
    let dimmed = read(&scene(Some((110.0 / 255.0, Rect::ZERO))));
    // A hole over the grey half: it stays lit, the highlight beside it is dimmed once, and
    // the edge between them is never darker than the dimmed side.
    let holed = read(&scene(Some((110.0 / 255.0, Rect::new(0.0, 0.0, W as f32 / 2.0, H as f32)))));
    let (grey, highlight) = (16, W - 16);
    println!(
        "plain   grey {} highlight {}",
        pixel(&plain, grey, H / 2),
        pixel(&plain, highlight, H / 2)
    );
    println!(
        "dimmed  grey {} highlight {}",
        pixel(&dimmed, grey, H / 2),
        pixel(&dimmed, highlight, H / 2)
    );
    println!(
        "holed   grey {} highlight {}",
        pixel(&holed, grey, H / 2),
        pixel(&holed, highlight, H / 2)
    );
    // PQ codes out of the output pass. The grey dims by the code step (0.5 -> 0.2843,
    // 51.5 -> 15.9 nits); the highlight lands on the peak's code times the step (1.341 ->
    // 0.7625, 128 nits), where a code multiply alone would leave 713 nits at PQ 182.
    let (plain_grey, plain_highlight, dimmed_grey, dimmed_highlight) = (
        pixel(&plain, grey, H / 2),
        pixel(&plain, highlight, H / 2),
        pixel(&dimmed, grey, H / 2),
        pixel(&dimmed, highlight, H / 2),
    );
    let mut failures = 0u32;
    let mut verdict = |ok: bool, name: &str| {
        println!("{} {name}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            failures += 1;
        }
    };
    verdict(plain_grey.abs_diff(113) <= 2, "the grey pictured at PQ 113");
    verdict(plain_highlight.abs_diff(219) <= 2, "the highlight reaches the output, PQ 219");
    verdict(dimmed_grey.abs_diff(86) <= 2, "the scrim dims the grey by the code step, PQ 86");
    verdict(
        dimmed_highlight.abs_diff(136) <= 2,
        "and the highlight to the peak's share, PQ 136, not PQ 182",
    );
    verdict(
        pixel(&holed, grey, H / 2).abs_diff(plain_grey) <= 1,
        "the hole leaves its grey exactly as the picture had it",
    );
    // Columns past the hole's edge: none may be darker than the dimmed highlight, which is
    // what two scrims meeting there would make.
    let edge_darkest = (W / 2..W)
        .map(|x| pixel(&holed, x, H / 2))
        .min()
        .unwrap_or(0);
    verdict(
        edge_darkest + 2 >= dimmed_highlight,
        "and the edge beside it is dimmed once, with no dark seam",
    );
    std::process::exit(if failures == 0 { 0 } else { 1 });
}
