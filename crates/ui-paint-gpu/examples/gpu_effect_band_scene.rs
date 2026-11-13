//! A layer effect draws in the band, and keeps its pixels when the band is elsewhere.
//!
//! Run: `cargo run -p ui-paint-gpu --example gpu_effect_band_scene`

use ui_core::NodeId;
use ui_core::damage::Damage;
use ui_core::draw::{DrawList, RectPrim};
use ui_core::geom::{Color, Corners, Rect};

use ui_paint_gpu::{EglContext, GpuRenderer, OffscreenFbo};

const W: u32 = 200;
const H: u32 = 80;

const LEFT: Rect = Rect {
    origin: ui_core::geom::Point { x: 30.0, y: 20.0 },
    size: ui_core::geom::Size { w: 20.0, h: 20.0 },
};
const RIGHT: Rect = Rect {
    origin: ui_core::geom::Point { x: 140.0, y: 20.0 },
    size: ui_core::geom::Size { w: 20.0, h: 20.0 },
};
const MOVER_A: Rect = Rect {
    origin: ui_core::geom::Point { x: 10.0, y: 60.0 },
    size: ui_core::geom::Size { w: 10.0, h: 10.0 },
};
const MOVER_B: Rect = Rect {
    origin: ui_core::geom::Point { x: 10.0, y: 66.0 },
    size: ui_core::geom::Size { w: 10.0, h: 10.0 },
};
/// The blurred square moved two pixels, which a two-sigma blur spreads past its old box.
const LEFT_MOVED: Rect = Rect {
    origin: ui_core::geom::Point { x: 32.0, y: 22.0 },
    size: ui_core::geom::Size { w: 20.0, h: 20.0 },
};

/// The layer box the painter would give this subtree: both squares grown by the blur's reach.
fn layer() -> Rect {
    Rect::new(26.0, 16.0, 142.0, 28.0)
}

fn quad(rect: Rect) -> RectPrim {
    RectPrim {
        rect,
        radius: Corners::ZERO,
        fill: Color::WHITE,
        gradient: None,
        border: None,
        border_gradient: None,
        notch: None,
    }
}

/// Two squares inside one blurred layer, plus a plain square outside it that can move.
fn scene(left: Rect, mover: Rect) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    list.push_layer(layer(), vec![ui_core::Effect::blur(4, 2.0)], Corners::ZERO);
    list.push_rect(NodeId::from_raw(1, 0), quad(left));
    list.push_rect(NodeId::from_raw(2, 0), quad(RIGHT));
    list.pop_layer();
    list.push_rect(NodeId::from_raw(3, 0), quad(mover));
    list
}

/// Alpha channel at a point, from premultiplied BGRA.
fn alpha_at(data: &[u8], x: u32, y: u32) -> u8 {
    data[((y * W + x) * 4 + 3) as usize]
}

/// Pixels the two surfaces disagree on, in premultiplied BGRA.
fn apart(a: &[u8], b: &[u8]) -> u32 {
    let mut differing = 0;
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        if p != q {
            differing += 1;
        }
    }
    differing
}

fn report(name: &str, value: f64, limit: f64, unit: &str, failures: &mut u32) {
    let ok = value <= limit;
    println!(
        "{name:44} {value:8.4}{unit}  (limit {limit}{unit})  {}",
        if ok { "OK" } else { "FAIL" }
    );
    if !ok {
        *failures += 1;
    }
}

fn main() {
    let node = std::env::args()
        .nth(1)
        .or_else(ui_paint_gpu::output_render_node)
        .unwrap_or_else(|| "/dev/dri/renderD128".to_string());
    let cnode = std::ffi::CString::new(node.clone()).expect("node path");
    let ctx = EglContext::new(cnode.as_c_str()).expect("EGL context");
    println!("GPU: {} ({node})", ctx.renderer());

    let target = OffscreenFbo::new(ctx.gl(), W, H).expect("target");
    let reference = OffscreenFbo::new(ctx.gl(), W, H).expect("reference");
    let mut gpu = GpuRenderer::new(ctx.gl(), ctx.is_gles()).expect("renderer");
    let mut truth = GpuRenderer::new(ctx.gl(), ctx.is_gles()).expect("reference renderer");
    gpu.clear(ctx.gl(), &target, [0.0, 0.0, 0.0, 0.0]);
    truth.clear(ctx.gl(), &reference, [0.0, 0.0, 0.0, 0.0]);
    let mut failures = 0;

    // A blurred square spreads past its own box: this is the tail a cull would cut off.
    gpu.render_frame(ctx.gl(), &target, &scene(LEFT, MOVER_A), &Damage::Full)
        .expect("first frame");
    truth
        .render_frame(ctx.gl(), &reference, &scene(LEFT, MOVER_A), &Damage::Full)
        .expect("first reference");
    let px = gpu.read_pixels(ctx.gl(), &target).expect("readback");
    let tail = alpha_at(&px, 27, 17);
    report(
        "[1] the blur reaches past its box",
        if tail > 0 && tail < 255 { 0.0 } else { 1.0 },
        0.0,
        "",
        &mut failures,
    );

    // Damage far from the layer: the frame is partial, the layer keeps its pixels, and the
    // result is what a full redraw would have produced.
    let far = Damage::Rects(vec![MOVER_A.union(MOVER_B)]);
    let stats = gpu
        .render_frame(ctx.gl(), &target, &scene(LEFT, MOVER_B), &far)
        .expect("second frame");
    truth
        .render_frame(ctx.gl(), &reference, &scene(LEFT, MOVER_B), &Damage::Full)
        .expect("second reference");
    let expect = truth.read_pixels(ctx.gl(), &reference).expect("readback");
    let px = gpu.read_pixels(ctx.gl(), &target).expect("readback");
    report(
        "[2] a band clear of the layer is still partial",
        if stats.band.is_some() { 0.0 } else { 1.0 },
        0.0,
        "",
        &mut failures,
    );
    report(
        "[3] the untouched effect layer is skipped",
        if stats.band_skips > 0 { 0.0 } else { 1.0 },
        0.0,
        "",
        &mut failures,
    );
    report(
        "[4] pixels equal to a full redraw",
        apart(&px, &expect) as f64,
        0.0,
        " px",
        &mut failures,
    );

    // Damage inside the layer: the layer is redrawn whole, so the blur reads its neighbours
    // across the band's edge instead of the hole a cull would leave there.
    let near = Damage::Rects(vec![LEFT.union(LEFT_MOVED)]);
    let stats = gpu
        .render_frame(ctx.gl(), &target, &scene(LEFT_MOVED, MOVER_B), &near)
        .expect("third frame");
    truth
        .render_frame(
            ctx.gl(),
            &reference,
            &scene(LEFT_MOVED, MOVER_B),
            &Damage::Full,
        )
        .expect("third reference");
    let expect = truth.read_pixels(ctx.gl(), &reference).expect("readback");
    let px = gpu.read_pixels(ctx.gl(), &target).expect("readback");
    report(
        "[5] a band inside the layer is partial too",
        if stats.band.is_some() { 0.0 } else { 1.0 },
        0.0,
        "",
        &mut failures,
    );
    report(
        "[6] a damaged effect layer is never skipped",
        stats.band_skips as f64,
        0.0,
        "",
        &mut failures,
    );
    report(
        "[7] seam-free at the band's edge",
        apart(&px, &expect) as f64,
        0.0,
        " px",
        &mut failures,
    );

    gpu.release(ctx.gl());
    truth.release(ctx.gl());
    target.release(ctx.gl());
    reference.release(ctx.gl());

    if failures > 0 {
        eprintln!("\n{failures} invariant(s) violated");
        std::process::exit(1);
    }
    println!("\nlayer effects draw inside the band and keep their pixels outside it");
}
