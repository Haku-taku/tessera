//! A cached subtree keeps its pixels until damage lands inside it.
//!
//! Run: `cargo run -p ui-paint-gpu --example gpu_cached_scene`

use ui_core::NodeId;
use ui_core::damage::Damage;
use ui_core::draw::{DrawList, RectPrim};
use ui_core::geom::{Color, Corners, Rect};

use ui_paint_gpu::{EglContext, GpuRenderer, OffscreenFbo};

const W: u32 = 160;
const H: u32 = 80;

fn canvas_rect() -> Rect {
    Rect::new(20.0, 10.0, 60.0, 60.0)
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

/// One cached canvas holding `content`, plus an optional change outside it.
fn scene(content: Rect) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    list.push_cached_layer(7, canvas_rect(), Vec::new(), Corners::ZERO);
    list.push_rect(NodeId::from_raw(1, 0), quad(content));
    list.pop_layer();
    list
}

/// A cached canvas inside a half-opacity layer, beside a quad that moves.
fn nested_scene(mover: Rect) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    list.push_layer(
        Rect::new(0.0, 0.0, W as f32, H as f32),
        vec![ui_core::Effect::opacity(0.5)],
        Corners::ZERO,
    );
    list.push_rect(NodeId::from_raw(3, 0), quad(mover));
    list.push_cached_layer(8, canvas_rect(), Vec::new(), Corners::ZERO);
    list.push_rect(
        NodeId::from_raw(4, 0),
        quad(Rect::new(30.0, 20.0, 20.0, 20.0)),
    );
    list.pop_layer();
    list.pop_layer();
    list
}

/// Alpha channel at a point, from premultiplied BGRA.
fn alpha_at(data: &[u8], x: u32, y: u32) -> u8 {
    data[((y * W + x) * 4 + 3) as usize]
}

fn report(name: &str, value: f64, limit: f64, unit: &str, failures: &mut u32) {
    let ok = value <= limit;
    println!(
        "{name:38} {value:8.4}{unit}  (limit {limit}{unit})  {}",
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
    let mut gpu = GpuRenderer::new(ctx.gl(), ctx.is_gles()).expect("renderer");
    gpu.clear(ctx.gl(), &target, [0.0, 0.0, 0.0, 0.0]);
    let mut failures = 0;

    let inside = Rect::new(25.0, 15.0, 20.0, 20.0);
    let stats = gpu
        .render_frame(ctx.gl(), &target, &scene(inside), &Damage::Full)
        .expect("first frame");
    // A cold cache has nothing to reuse, so the first frame must paint the subtree.
    report(
        "[1] skips with a cold cache",
        stats.cached_skips as f64,
        0.0,
        "",
        &mut failures,
    );
    let px = gpu.read_pixels(ctx.gl(), &target).expect("readback");
    report(
        "[2] canvas painted",
        (255 - alpha_at(&px, 30, 20)) as f64,
        1.0,
        "/255",
        &mut failures,
    );

    // Damage either side of the canvas widens the band over it without touching it: the
    // subtree is reused, and the band's clearing is undone by its cached composite.
    let across = Damage::Rects(vec![
        Rect::new(0.0, 20.0, 18.0, 20.0),
        Rect::new(132.0, 20.0, 28.0, 20.0),
    ]);
    let stats = gpu
        .render_frame(ctx.gl(), &target, &scene(inside), &across)
        .expect("second frame");
    report(
        "[3] batches skipped over an untouched canvas",
        if stats.cached_skips > 0 { 0.0 } else { 1.0 },
        0.0,
        "",
        &mut failures,
    );
    let px = gpu.read_pixels(ctx.gl(), &target).expect("readback");
    report(
        "[4] canvas restored from its cache",
        (255 - alpha_at(&px, 30, 20)) as f64,
        1.0,
        "/255",
        &mut failures,
    );

    // Damage inside the canvas must repaint it, and the vacated pixels must go away: a
    // cached layer that only ever composited would leave the old square behind.
    let moved = Rect::new(45.0, 15.0, 20.0, 20.0);
    let inside_damage = Damage::Rects(vec![inside.union(moved)]);
    let stats = gpu
        .render_frame(ctx.gl(), &target, &scene(moved), &inside_damage)
        .expect("third frame");
    report(
        "[5] no skip when the canvas changed",
        stats.cached_skips as f64,
        0.0,
        "",
        &mut failures,
    );
    let px = gpu.read_pixels(ctx.gl(), &target).expect("readback");
    report(
        "[6] moved content painted",
        (255 - alpha_at(&px, 50, 20)) as f64,
        1.0,
        "/255",
        &mut failures,
    );
    report(
        "[7] vacated pixels cleared",
        alpha_at(&px, 30, 20) as f64,
        1.0,
        "/255",
        &mut failures,
    );

    // The band misses the cached canvas but not the layer holding it, so the canvas is reused
    // and the layer's own opacity still applies where the damage landed.
    let mover_a = Rect::new(120.0, 50.0, 20.0, 20.0);
    let mover_b = Rect::new(120.0, 10.0, 20.0, 20.0);
    gpu.render_frame(ctx.gl(), &target, &nested_scene(mover_a), &Damage::Full)
        .expect("nested first");
    let stats = gpu
        .render_frame(
            ctx.gl(),
            &target,
            &nested_scene(mover_b),
            &Damage::Rects(vec![mover_a.union(mover_b)]),
        )
        .expect("nested second");
    report(
        "[8] canvas skipped under a layer effect",
        if stats.cached_skips > 0 { 0.0 } else { 1.0 },
        0.0,
        "",
        &mut failures,
    );
    let px = gpu.read_pixels(ctx.gl(), &target).expect("readback");
    report(
        "[9] cached canvas keeps the layer's opacity",
        (127 - alpha_at(&px, 35, 25) as i32).unsigned_abs() as f64,
        2.0,
        "/255",
        &mut failures,
    );

    gpu.release(ctx.gl());
    target.release(ctx.gl());

    if failures > 0 {
        eprintln!("\n{failures} invariant(s) violated");
        std::process::exit(1);
    }
    println!("\ncached subtrees keep their pixels until damage lands inside them");
}
