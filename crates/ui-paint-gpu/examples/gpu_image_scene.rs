//! Checks the GPU image primitive: source sub-rect sampling, rounded corners, texture cache.
//!
//! The texture is a 2x2 grid of saturated colours, so a sampler that ignores `src`, flips y,
//! or tiles the source is visible as the wrong colour in a named region.
//!
//! Run: `cargo run -p ui-paint-gpu --example gpu_image_scene`

use std::sync::Arc;

use ui_core::draw::{CanvasId, DrawList, ImageEntry, ImagePrim, ImageSource, RectPrim, Sampling};
use ui_core::geom::{Color, Corners, Rect};
use ui_core::{Damage, NodeId};
use ui_paint::{CpuRenderer, Pixels};
use ui_paint_gpu::{CanvasUpload, EglContext, GpuRenderer, OffscreenFbo, RenderStats};

const W: u32 = 320;
const H: u32 = 240;
const BG: [u8; 3] = [20, 22, 28];

/// Side of the square test texture, and of one quadrant.
const TEX: u32 = 64;
const QUAD: u32 = TEX / 2;

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const YELLOW: [u8; 3] = [255, 255, 0];

const ID_A: u64 = 0xA1;
const ID_B: u64 = 0xB2;

fn texture() -> Vec<u8> {
    let mut data = Vec::with_capacity((TEX * TEX * 4) as usize);
    for y in 0..TEX {
        for x in 0..TEX {
            let top = y < QUAD;
            let left = x < QUAD;
            let c = match (top, left) {
                (true, true) => RED,
                (true, false) => GREEN,
                (false, true) => BLUE,
                (false, false) => YELLOW,
            };
            data.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    data
}

fn canvas() -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    // The frame is blitted over the target, so the background has to be a primitive.
    list.push_rect(
        NodeId::from_raw(1, 0),
        RectPrim {
            rect: Rect::new(0.0, 0.0, W as f32, H as f32),
            radius: Corners::ZERO,
            fill: Color::from_rgba8(BG[0], BG[1], BG[2], 255),
            gradient: None,
            border: None,
            border_gradient: None,
            notch: None,
        },
    );
    list
}

fn add_image(
    list: &mut DrawList,
    id: u64,
    data: &Arc<Vec<u8>>,
    rect: Rect,
    src: Rect,
    corners: Corners,
) {
    let texture = list.push_image(ImageEntry {
        id,
        width: TEX,
        height: TEX,
        source: ImageSource::Pixels(Arc::clone(data)),
        colour: ui_core::colour::ContentColour::SDR,
        sampling: ui_core::draw::Sampling::Smooth,
    });
    list.push_image_prim(
        NodeId::from_raw(2, 0),
        ImagePrim {
            rect,
            corners,
            texture,
            src,
        },
    );
}

fn draw(
    gpu: &mut GpuRenderer,
    ctx: &EglContext,
    target: &OffscreenFbo,
    list: &DrawList,
) -> (Vec<u8>, RenderStats) {
    gpu.clear(ctx.gl(), target, [0.0, 0.0, 0.0, 0.0]);
    let stats = gpu.render(ctx.gl(), target, list).expect("render");
    let pixels = gpu.read_pixels(ctx.gl(), target).expect("readback");
    (pixels, stats)
}

fn at(data: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2]]
}

/// Mean colour of an inclusive-exclusive rectangle.
fn mean(data: &[u8], x0: u32, y0: u32, x1: u32, y1: u32) -> [f64; 3] {
    let mut sum = [0.0f64; 3];
    let mut n = 0.0f64;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = at(data, x, y);
            for c in 0..3 {
                sum[c] += p[c] as f64;
            }
            n += 1.0;
        }
    }
    [sum[0] / n, sum[1] / n, sum[2] / n]
}

fn worst_channel(a: [f64; 3], b: [u8; 3]) -> f64 {
    (0..3)
        .map(|c| (a[c] - b[c] as f64).abs())
        .fold(0.0, f64::max)
}

/// Pixels in a region whose colour is further than `tol` from `want` on any channel.
fn foreign(data: &[u8], x0: u32, y0: u32, x1: u32, y1: u32, want: [u8; 3], tol: i32) -> u32 {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = at(data, x, y);
            if (0..3).any(|c| (p[c] as i32 - want[c] as i32).abs() > tol) {
                n += 1;
            }
        }
    }
    n
}

/// The same list through the CPU rasteriser.
fn cpu(list: &DrawList) -> Pixels {
    let mut target = Pixels::new(W, H);
    CpuRenderer::new().render(list, &mut target, &Damage::Full);
    target
}

/// Worst channel difference between the two backends' means over one region.
///
/// Means, not pixels: the edge coverage is implementation-defined, so a per-pixel equality
/// would demand that one backend imitate the other's anti-aliasing.
fn mean_gap(gpu: &[u8], cpu: &Pixels, r: (u32, u32, u32, u32)) -> f64 {
    let (x0, y0, x1, y1) = r;
    let mut sum = [0.0f64; 3];
    let mut n = 0.0f64;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = cpu.get(x, y);
            for c in 0..3 {
                sum[c] += p[c] as f64;
            }
            n += 1.0;
        }
    }
    let cpu_mean = [sum[0] / n, sum[1] / n, sum[2] / n];
    let gpu_mean = mean(gpu, x0, y0, x1, y1);
    (0..3)
        .map(|c| (gpu_mean[c] - cpu_mean[c]).abs())
        .fold(0.0, f64::max)
}

fn report(name: &str, value: f64, limit: f64, unit: &str, failures: &mut u32) {
    let ok = value <= limit;
    println!(
        "{name:34} {value:9.4}{unit}  (limit {limit}{unit})  {}",
        if ok { "OK" } else { "FAIL" }
    );
    if !ok {
        *failures += 1;
    }
}

fn report_min(name: &str, value: f64, floor: f64, unit: &str, failures: &mut u32) {
    let ok = value >= floor;
    println!(
        "{name:34} {value:9.4}{unit}  (floor {floor}{unit})  {}",
        if ok { "OK" } else { "FAIL" }
    );
    if !ok {
        *failures += 1;
    }
}

fn report_eq(name: &str, got: i64, want: i64, failures: &mut u32) {
    let ok = got == want;
    println!(
        "{name:34} {got:9}       (want {want})   {}",
        if ok { "OK" } else { "FAIL" }
    );
    if !ok {
        *failures += 1;
    }
}
const CANVAS_ID: CanvasId = CanvasId(7);

/// A 2x2 canvas: one column of each colour, in canvas pixels.
fn canvas_pixels(left: [u8; 3], right: [u8; 3]) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, 2.0, 2.0));
    for (x, colour) in [(0.0f32, left), (1.0, right)] {
        list.push_rect(
            DrawList::canvas_owner(),
            RectPrim {
                rect: Rect::new(x, 0.0, 1.0, 2.0),
                radius: Corners::ZERO,
                fill: Color::from_rgba8(colour[0], colour[1], colour[2], 255),
                gradient: None,
                border: None,
                border_gradient: None,
                notch: None,
            },
        );
    }
    list
}

/// One frame whose picture is the canvas, sampled at `rect`.
fn canvas_scene(sampling: Sampling, rect: Rect, id: u64) -> DrawList {
    let mut scene = canvas();
    let texture = scene.push_image(ImageEntry {
        id,
        width: 2,
        height: 2,
        source: ImageSource::Canvas(CANVAS_ID),
        colour: ui_core::colour::ContentColour::SDR,
        sampling,
    });
    scene.push_image_prim(
        NodeId::from_raw(3, 0),
        ImagePrim {
            rect,
            corners: Corners::ZERO,
            texture,
            src: Rect::new(0.0, 0.0, 2.0, 2.0),
        },
    );
    scene
}

fn draw_canvases(
    gpu: &mut GpuRenderer,
    ctx: &EglContext,
    target: &OffscreenFbo,
    list: &DrawList,
    canvases: &[CanvasUpload<'_>],
) -> (Vec<u8>, RenderStats) {
    gpu.clear(ctx.gl(), target, [0.0, 0.0, 0.0, 0.0]);
    let stats = gpu
        .render_frame_with_canvases(ctx.gl(), target, list, &Damage::Full, &[], canvases)
        .expect("render");
    let pixels = gpu.read_pixels(ctx.gl(), target).expect("readback");
    (pixels, stats)
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
    let data = Arc::new(texture());
    let mut failures = 0;

    // [1][2] The whole texture lands in the rect, upright: four quadrant centres keep their colour.
    let mut full = canvas();
    let full_rect = Rect::new(16.0, 16.0, 140.0, 140.0);
    add_image(
        &mut full,
        ID_A,
        &data,
        full_rect,
        Rect::new(0.0, 0.0, TEX as f32, TEX as f32),
        Corners::ZERO,
    );
    let (pixels, stats_full) = draw(&mut gpu, &ctx, &target, &full);
    println!("stats: {stats_full:?}");

    let interior = mean(&pixels, 24, 24, 148, 148);
    report_min(
        "[1] rect interior vs background",
        worst_channel(interior, BG),
        40.0,
        "",
        &mut failures,
    );

    // Quadrant centres are half of a destination quadrant, not half of a texture quadrant.
    let half = full_rect.size.w / 2.0;
    let mut worst = 0.0f64;
    for (dx, dy, want) in [
        (half * 0.5, half * 0.5, RED),
        (half * 1.5, half * 0.5, GREEN),
        (half * 0.5, half * 1.5, BLUE),
        (half * 1.5, half * 1.5, YELLOW),
    ] {
        let p = at(
            &pixels,
            (full_rect.origin.x + dx) as u32,
            (full_rect.origin.y + dy) as u32,
        );
        let err = (0..3)
            .map(|c| (p[c] as f64 - want[c] as f64).abs())
            .fold(0.0, f64::max);
        worst = worst.max(err);
    }
    report("[2] quadrant centres", worst, 6.0, "", &mut failures);

    // [3][4] A 32x32 source sub-rect scaled up 4x: tiling or whole-texture sampling shows green.
    let mut crop = canvas();
    let crop_rect = Rect::new(170.0, 16.0, 130.0, 130.0);
    add_image(
        &mut crop,
        ID_A,
        &data,
        crop_rect,
        Rect::new(0.0, 0.0, QUAD as f32, QUAD as f32),
        Corners::ZERO,
    );
    let (pixels, _) = draw(&mut gpu, &ctx, &target, &crop);
    let left = mean(&pixels, 175, 21, 235, 141);
    let right = mean(&pixels, 235, 21, 295, 141);
    report(
        "[3] cropped source, left half",
        worst_channel(left, RED),
        6.0,
        "",
        &mut failures,
    );
    report(
        "[4] cropped source, right half",
        worst_channel(right, RED),
        6.0,
        "",
        &mut failures,
    );
    report_eq(
        "[4b] foreign pixels in right half",
        foreign(&pixels, 235, 21, 295, 141, RED, 60) as i64,
        0,
        &mut failures,
    );

    // [5][6] Rounded corners cut the corner pixel but leave the straight edge filled.
    let mut round = canvas();
    let round_rect = Rect::new(100.0, 160.0, 120.0, 60.0);
    add_image(
        &mut round,
        ID_A,
        &data,
        round_rect,
        Rect::new(0.0, 0.0, QUAD as f32, QUAD as f32),
        Corners::all(24.0),
    );
    let (pixels, _) = draw(&mut gpu, &ctx, &target, &round);
    let corner = at(&pixels, 102, 162);
    report(
        "[5] pixel outside the corner radius",
        worst_channel([corner[0] as f64, corner[1] as f64, corner[2] as f64], BG),
        2.0,
        "",
        &mut failures,
    );
    let edge = at(&pixels, 160, 162);
    report(
        "[6] pixel on the straight top edge",
        worst_channel([edge[0] as f64, edge[1] as f64, edge[2] as f64], RED),
        6.0,
        "",
        &mut failures,
    );

    // [7][8][9] One upload per `ImageEntry::id`: a second frame with the same id reuses it.
    let (_, again) = draw(&mut gpu, &ctx, &target, &full);
    let mut other = canvas();
    add_image(
        &mut other,
        ID_B,
        &data,
        full_rect,
        Rect::new(0.0, 0.0, TEX as f32, TEX as f32),
        Corners::ZERO,
    );
    let (_, fresh) = draw(&mut gpu, &ctx, &target, &other);
    report_eq(
        "[7] uploads on first use",
        stats_full.image_uploads as i64,
        1,
        &mut failures,
    );
    report_eq(
        "[8] uploads on a repeated id",
        again.image_uploads as i64,
        0,
        &mut failures,
    );
    report_eq(
        "[9] uploads after an id change",
        fresh.image_uploads as i64,
        1,
        &mut failures,
    );
    report_eq(
        "[10] image primitives drawn",
        stats_full.images_drawn as i64,
        1,
        &mut failures,
    );

    // [11] With `src` inside [0,1] the wrap mode is invisible in pixels, so query the sampler.
    let clamp = glow::CLAMP_TO_EDGE as i32;
    match gpu.image_wrap(ctx.gl(), ID_B) {
        Some((s, t)) => {
            let bad = i64::from(s != clamp) + i64::from(t != clamp);
            report_eq("[11] wrap mode is CLAMP_TO_EDGE", bad, 0, &mut failures);
        }
        None => {
            println!("[11] wrap mode is CLAMP_TO_EDGE   texture missing  FAIL");
            failures += 1;
        }
    }

    // [12..15] The CPU presenter draws the same picture. Two backends that are never compared
    // drift, so each scene is rasterised both ways and their region means must agree.
    // The last scene straddles a seam between a transparent and an opaque texel, which only
    // matches when both paths interpolate premultiplied channels.
    let mut seam = canvas();
    let seam_texture = seam.push_image(ImageEntry {
        id: 0xC3,
        width: 2,
        height: 1,
        source: ImageSource::Pixels(Arc::new(vec![255, 0, 0, 0, 0, 0, 255, 255])),
        colour: ui_core::colour::ContentColour::SDR,
        sampling: ui_core::draw::Sampling::Smooth,
    });
    seam.push_image_prim(
        NodeId::from_raw(2, 0),
        ImagePrim {
            rect: Rect::new(100.0, 80.0, 120.0, 80.0),
            corners: Corners::ZERO,
            texture: seam_texture,
            src: Rect::new(0.0, 0.0, 2.0, 1.0),
        },
    );
    for (name, scene, region) in [
        ("[12] CPU vs GPU, full", &full, (24, 24, 148, 148)),
        ("[13] CPU vs GPU, cropped", &crop, (175, 21, 295, 141)),
        ("[14] CPU vs GPU, rounded", &round, (110, 170, 210, 210)),
        (
            "[15] CPU vs GPU, translucent seam",
            &seam,
            (145, 85, 175, 155),
        ),
    ] {
        let (gpu_pixels, _) = draw(&mut gpu, &ctx, &target, scene);
        let cpu_pixels = cpu(scene);
        report(
            name,
            mean_gap(&gpu_pixels, &cpu_pixels, region),
            4.0,
            "",
            &mut failures,
        );
    }

    // [16..19] A canvas is painted once, in its own pixels, and sampled by the frame that
    // names it: an unchanged version must reuse the target, a new one must replace it.
    let rect = Rect::new(16.0, 16.0, 128.0, 128.0);
    let first = canvas_pixels(RED, BLUE);
    let scene = canvas_scene(Sampling::Pixelated, rect, 0xD4);
    let (pixels, stats) = draw_canvases(
        &mut gpu,
        &ctx,
        &target,
        &scene,
        &[CanvasUpload {
            id: CANVAS_ID,
            size: (2, 2),
            list: Some(&first),
        }],
    );
    let left = at(&pixels, 20, 20);
    let right = at(&pixels, 140, 140);
    let wrong = (0..3)
        .map(|c| (left[c] as i32 - RED[c] as i32).abs())
        .max()
        .unwrap_or(0)
        .max(
            (0..3)
                .map(|c| (right[c] as i32 - BLUE[c] as i32).abs())
                .max()
                .unwrap_or(0),
        );
    report_eq(
        "[16] canvas columns reach the frame",
        wrong as i64,
        0,
        &mut failures,
    );
    report_eq(
        "[16b] canvas draws upload no picture",
        stats.image_uploads as i64,
        0,
        &mut failures,
    );

    let (pixels, _) = draw_canvases(
        &mut gpu,
        &ctx,
        &target,
        &scene,
        &[CanvasUpload {
            id: CANVAS_ID,
            size: (2, 2),
            list: None,
        }],
    );
    let kept = at(&pixels, 20, 20);
    report(
        "[17] unchanged canvas keeps its target",
        (0..3)
            .map(|c| (kept[c] as f64 - RED[c] as f64).abs())
            .fold(0.0, f64::max),
        1.0,
        "",
        &mut failures,
    );

    let second = canvas_pixels(GREEN, YELLOW);
    let (pixels, _) = draw_canvases(
        &mut gpu,
        &ctx,
        &target,
        &scene,
        &[CanvasUpload {
            id: CANVAS_ID,
            size: (2, 2),
            list: Some(&second),
        }],
    );
    let fresh = at(&pixels, 20, 20);
    report(
        "[18] a new version repaints the target",
        (0..3)
            .map(|c| (fresh[c] as f64 - GREEN[c] as f64).abs())
            .fold(0.0, f64::max),
        1.0,
        "",
        &mut failures,
    );

    // [19][20] The filter is chosen per draw, not at upload: the same target read nearest
    // takes one texel and read smooth averages the two.
    let small = Rect::new(16.0, 16.0, 32.0, 32.0);
    let mut sampled = canvas_scene(Sampling::Pixelated, small, 0xD5);
    let smooth_texture = sampled.push_image(ImageEntry {
        id: 0xD6,
        width: 2,
        height: 2,
        source: ImageSource::Canvas(CANVAS_ID),
        colour: ui_core::colour::ContentColour::SDR,
        sampling: Sampling::Smooth,
    });
    sampled.push_image_prim(
        NodeId::from_raw(4, 0),
        ImagePrim {
            rect: Rect::new(60.0, 16.0, 32.0, 32.0),
            corners: Corners::ZERO,
            texture: smooth_texture,
            src: Rect::new(0.0, 0.0, 2.0, 2.0),
        },
    );
    let (pixels, _) = draw_canvases(
        &mut gpu,
        &ctx,
        &target,
        &sampled,
        &[CanvasUpload {
            id: CANVAS_ID,
            size: (2, 2),
            list: Some(&second),
        }],
    );
    // Nearest keeps the texel boundaries hard: the same source coordinate that reads one
    // texel to the left of the seam reads the other one to its right.
    let nearest_left = at(&pixels, 30, 30);
    let nearest_right = at(&pixels, 40, 30);
    let hard = (0..3)
        .map(|c| (nearest_left[c] as f64 - GREEN[c] as f64).abs())
        .chain((0..3).map(|c| (nearest_right[c] as f64 - YELLOW[c] as f64).abs()))
        .fold(0.0, f64::max);
    report(
        "[19] nearest keeps texels hard",
        hard,
        1.0,
        "",
        &mut failures,
    );
    // Halfway between the two texel centres, so 43.75% of the way from green to yellow.
    let smooth = at(&pixels, 75, 30);
    report(
        "[20] smooth reads the same target blended",
        (smooth[0] as f64 - 111.5).abs(),
        24.0,
        "",
        &mut failures,
    );

    // [21] A still canvas keeps its target across every frame that samples it, not only
    // the frame after its paint.
    let still = [CanvasUpload {
        id: CANVAS_ID,
        size: (2, 2),
        list: None,
    }];
    let mut still_frames = 0;
    for _ in 0..2 {
        if gpu
            .render_frame_with_canvases(ctx.gl(), &target, &sampled, &Damage::Full, &[], &still)
            .is_ok()
        {
            still_frames += 1;
        }
    }
    report_eq(
        "[21] a still canvas survives every frame that samples it",
        still_frames,
        2,
        &mut failures,
    );

    gpu.release(ctx.gl());
    target.release(ctx.gl());

    println!("\nFAIL={failures}");
    if failures > 0 {
        std::process::exit(1);
    }
}
