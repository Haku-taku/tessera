//! Check the PQ output transform against the CPU formula by reading the buffer back.
//!
//! The screen cannot judge this (a compositor may tone-map HDR content), so the check
//! compares our own buffer against `encode_for_pq_output` for known sRGB inputs. A capture
//! is checked the other way round: it must be the sRGB interface whatever the buffer holds.
//!
//! Run: `cargo run -p ui-paint-gpu --example gpu_hdr_scene`

use ui_core::NodeId;
use ui_core::colour::{TransferFunction, encode_for_pq_output};
use ui_core::draw::{DrawList, RectPrim};
use ui_core::geom::{Color, Rect};
use ui_gpu_egl::{EglContext, output_render_node};
use ui_paint_gpu::{GlDevice, GpuRenderer, OffscreenFbo};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
const REFERENCE_NITS: f32 = 203.0;
const MIN_NITS: f32 = 0.0;
/// Above the 255 sRGB code, so nothing this scene draws needs the tone curve.
const PEAK_NITS: f32 = 1000.0;

/// sRGB codes to test: black, dark, mid, bright, white.
const CODES: [u8; 5] = [0, 32, 128, 192, 255];

fn block(rect: Rect, code: u8) -> RectPrim {
    let v = code as f32 / 255.0;
    RectPrim {
        rect,
        radius: ui_core::geom::Corners::all(0.0),
        fill: Color::rgba(v, v, v, 1.0),
        border: None,
        border_gradient: None,
        gradient: None,
        notch: None,
    }
}

fn scene(code: u8) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32));
    let full = Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
    list.push_rect(NodeId::from_raw(1, 0), block(full, code));
    list
}

/// A hard edge down the middle, so a resized or resampled frame shows up.
fn halves() -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32));
    let (w, h) = (WIDTH as f32, HEIGHT as f32);
    list.push_rect(
        NodeId::from_raw(2, 0),
        block(Rect::new(0.0, 0.0, w / 2.0, h), 255),
    );
    list.push_rect(
        NodeId::from_raw(3, 0),
        block(Rect::new(w / 2.0, 0.0, w / 2.0, h), 64),
    );
    list
}

fn main() {
    let node = std::env::args()
        .nth(1)
        .or_else(output_render_node)
        .unwrap_or_else(|| "/dev/dri/renderD128".to_string());
    let cnode = std::ffi::CString::new(node.clone()).expect("node path");
    let ctx = EglContext::new(cnode.as_c_str()).expect("EGL context");
    println!("GPU: {} ({node})", ctx.renderer());
    println!("reference {REFERENCE_NITS} nits, min {MIN_NITS} nits");

    let target = OffscreenFbo::new(ctx.gl(), WIDTH, HEIGHT).expect("target");
    let mut device = GlDevice::new(ctx.gl_handle(), ctx.is_gles());
    let mut gpu = GpuRenderer::new(&mut device).expect("renderer");
    gpu.set_destination(ui_core::colour::Destination {
        domain: ui_core::colour::Domain::Extended {
            reference_nits: REFERENCE_NITS,
            min_nits: MIN_NITS,
            peak_nits: PEAK_NITS,
            encoding: ui_core::colour::OutputEncoding::Pq,
        },
        tone: ui_core::colour::ToneMapping::Bt2390,
    });

    let mut failures = 0;
    println!("  sRGB  expected  actual  delta");
    for code in CODES {
        let list = scene(code);
        gpu.render(&mut device, &target, &list).expect("render");
        let data = gpu.read_pixels(&mut device, &target).expect("readback");

        // Centre pixel, straight RGBA after readback.
        let i = ((HEIGHT / 2) * WIDTH + WIDTH / 2) as usize * 4;
        let actual = data[i] as f32 / 255.0;

        let linear = TransferFunction::Srgb.decode(code as f32 / 255.0);
        let expected = encode_for_pq_output(linear, REFERENCE_NITS, MIN_NITS);

        // One 8-bit step plus a little slack for the shader's own rounding.
        let delta = (actual - expected).abs();
        let ok = delta <= 2.5 / 255.0;
        if !ok {
            failures += 1;
        }
        println!(
            "  {code:5}  {expected:8.4}  {actual:6.4}  {:+.4}  {}",
            actual - expected,
            if ok { "ok" } else { "FAIL" }
        );
    }

    // A transform that did nothing would pass every "same as input" check, so pin the
    // opposite: white must move well away from 1.0 under PQ.
    let white = TransferFunction::Srgb.decode(1.0);
    let white_code = encode_for_pq_output(white, REFERENCE_NITS, MIN_NITS);
    let moved = white_code < 0.9;
    if !moved {
        failures += 1;
    }
    println!(
        "[2] white encodes away from 1.0: {white_code:.4} {}",
        if moved { "ok" } else { "FAIL" }
    );

    // A capture is a picture of the interface, so it must be the sRGB input whatever the
    // output transfer does. Reading the presented buffer instead hands back its PQ codes.
    let captures = |gpu: &mut GpuRenderer, device: &mut GlDevice| {
        let mut worst = 0i32;
        for code in CODES {
            gpu.render(device, &target, &scene(code)).expect("render");
            let (w, h, px) = gpu.capture(device).expect("capture");
            let i = ((h / 2) * w + w / 2) as usize * 4;
            worst = worst.max((px[i] as i32 - code as i32).abs());
        }
        worst
    };
    let wrong = captures(&mut gpu, &mut device);
    let ok = wrong <= 1;
    if !ok {
        failures += 1;
    }
    println!(
        "[3] captured as the interface, not the encoding: worst {wrong}/255 {}",
        if ok { "ok" } else { "FAIL" }
    );

    // The HDR domain stores linear light, which needs f16: eight bits band in the shadows.
    let float = gpu.frame_format() == ui_paint_gpu::TargetFormat::Rgba16F;
    if !float {
        failures += 1;
    }
    println!(
        "[4] the HDR frame is f16: {} {}",
        gpu.frame_format().bytes_per_pixel() * 8,
        if float { "ok" } else { "FAIL" }
    );

    // A supersampled frame is larger than the picture; a capture averages it down.
    gpu.set_supersample(2);
    let wrong = captures(&mut gpu, &mut device);
    let ok = wrong <= 1;
    if !ok {
        failures += 1;
    }
    println!(
        "[5] the same from a supersampled frame: worst {wrong}/255 {}",
        if ok { "ok" } else { "FAIL" }
    );

    // An edge pins the frame's own size: a resize that did not happen crops the picture, and
    // a frame that never became float draws nothing at all.
    let edge = |gpu: &mut GpuRenderer, device: &mut GlDevice, ss: u32| {
        gpu.set_supersample(ss);
        gpu.render(device, &target, &halves()).expect("render");
        gpu.capture(device).expect("capture").2
    };
    let plain = edge(&mut gpu, &mut device, 1);
    let sampled = edge(&mut gpu, &mut device, 2);
    let differing = plain.iter().zip(&sampled).filter(|(a, b)| a != b).count();
    let ok = differing == 0;
    if !ok {
        failures += 1;
    }
    println!(
        "[6] supersampling keeps the same picture: {differing} bytes differ {}",
        if ok { "ok" } else { "FAIL" }
    );

    println!("\nFAIL={failures}");
    if failures > 0 {
        std::process::exit(1);
    }
}
