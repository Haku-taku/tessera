//! Elevation: a soft shadow is drawn under its shape, not through it, and an inset one stays
//! inside its surface.
//!
//! Run: `WAYLAND_DISPLAY=wayland-2 cargo run -p ui-paint-gpu --example gpu_shadow_scene`

use ui_core::NodeId;
use ui_core::draw::{DrawList, PrimKind, RectPrim, ShadowBound, ShadowPrim, ShadowSide};
use ui_core::geom::{Color, Corners, Rect};
use ui_paint_gpu::{EglContext, GpuRenderer, OffscreenFbo};

const W: u32 = 300;
const H: u32 = 320;
const ROUND: Rect = Rect {
    origin: ui_core::geom::Point { x: 80.0, y: 60.0 },
    size: ui_core::geom::Size { w: 140.0, h: 100.0 },
};
/// A translucent surface on the left and an inset one on the right, both below `ROUND`.
const CLEAR: Rect = Rect {
    origin: ui_core::geom::Point { x: 20.0, y: 230.0 },
    size: ui_core::geom::Size { w: 120.0, h: 60.0 },
};
const INSET: Rect = Rect {
    origin: ui_core::geom::Point { x: 160.0, y: 230.0 },
    size: ui_core::geom::Size { w: 120.0, h: 60.0 },
};
const BLUR: f32 = 10.0;
const ALPHA: f32 = 0.5;
const RADIUS: f32 = 12.0;

fn surface(list: &mut DrawList, rect: Rect, fill: Color) {
    list.push_rect(
        NodeId::from_raw(1, 0),
        RectPrim {
            rect,
            radius: Corners::all(RADIUS),
            fill,
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        },
    );
}

fn cast(list: &mut DrawList, rect: Rect, side: ShadowSide) {
    list.push_shadow(
        NodeId::from_raw(1, 1),
        ShadowPrim {
            rect,
            corners: Corners::all(RADIUS),
            colour: Color::rgba(0.0, 0.0, 0.0, ALPHA),
            blur: BLUR,
            dy: 0.0,
            ring: 0.0,
            bound: Some(ShadowBound { rect, side }),
        },
    );
}

fn scene() -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    // A white page, so shadow pixels are measurable as "less than white".
    surface(
        &mut list,
        Rect::new(0.0, 0.0, W as f32, H as f32),
        Color::WHITE,
    );
    // The surface itself: opaque, and must stay untinted by its own shadow.
    cast(&mut list, ROUND, ShadowSide::Outer);
    surface(&mut list, ROUND, Color::WHITE);
    // A translucent surface: its own cast must not show through it.
    cast(&mut list, CLEAR, ShadowSide::Outer);
    surface(&mut list, CLEAR, Color::rgba(1.0, 1.0, 1.0, 0.5));
    // An inset shadow is painted above its surface and cut at the surface's edge.
    surface(&mut list, INSET, Color::WHITE);
    cast(&mut list, INSET, ShadowSide::Inset);
    list
}

fn main() {
    let node = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/dri/renderD128".to_string());
    let cnode = std::ffi::CString::new(node.clone()).expect("node path");
    let egl = EglContext::new(cnode.as_c_str()).expect("EGL");
    let target = OffscreenFbo::new(egl.gl(), W, H).expect("target");
    let mut gpu = GpuRenderer::new(egl.gl(), egl.is_gles()).expect("renderer");
    let list = scene();
    gpu.clear(egl.gl(), &target, [0.0, 0.0, 0.0, 1.0]);
    let stats = gpu.render(egl.gl(), &target, &list).expect("render");
    let out = gpu.read_pixels(egl.gl(), &target).expect("readback");

    let mut fail = 0;
    let mut check = |name: &str, got: f32, limit: f32, want_below: bool| {
        let ok = if want_below {
            got <= limit
        } else {
            got >= limit
        };
        println!(
            "  {} {:>8.3}  ({} {:>5.2})  {}",
            if ok { "ok  " } else { "FAIL" },
            got,
            if want_below { "limit" } else { "floor" },
            limit,
            name
        );
        if !ok {
            fail += 1;
        }
    };

    // Readback is raw RGBA8 rows; these frames are opaque, so luma is enough.
    let luma = |x: u32, y: u32| -> f32 {
        let i = (y as usize * W as usize + x as usize) * 4;
        let p = &out[i..i + 3];
        (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
    };

    // 1. Directly under the middle of the shape: covered by the surface, so pure white.
    check(
        "the surface hides its own shadow",
        luma(150, 110),
        0.995,
        false,
    );

    // 2. On the edge pixel the silhouette is half a pixel away, so the cast carries about half
    // its peak: `blur` is two sigma (CSS and Skia), and peak alpha is `ALPHA`.
    let outside = luma(79, 110);
    check(
        "the shadow is near half its peak on the edge",
        1.0 - outside,
        0.20,
        false,
    );

    // 3. The falloff is monotone: the further from the edge, the closer to the page.
    let near = 1.0 - luma(79, 110);
    let far = 1.0 - luma(74, 110);
    check(
        "the shadow is stronger nearer the edge",
        near - far,
        0.02,
        false,
    );
    check(
        "the shadow ends inside its falloff",
        1.0 - luma(66, 110),
        0.01,
        true,
    );

    // 4. Three sigma out — 1.5 `blur` — the tail is below a thousandth of the peak.
    let beyond = luma((80.0 - 1.5 * BLUR) as u32 - 1, 110);
    check(
        "nothing casts beyond the falloff",
        1.0 - beyond,
        0.002,
        true,
    );

    // 5. The rounded corner keeps the shadow out of its notch.
    check(
        "no shadow in the corner notch",
        1.0 - luma(84, 64),
        0.10,
        true,
    );

    // 6. The translucent surface is white over a white page: if its own cast reached inside,
    // half-white would show it as grey.
    check(
        "a translucent surface is not tinted by its own cast",
        luma(80, 260),
        0.995,
        false,
    );
    check(
        "and still casts outside its edge",
        1.0 - luma(19, 260),
        0.15,
        false,
    );

    // 7. An inset shadow darkens the inner edge and stops at the surface's edge, on both
    // sides: nothing outside, nothing in the middle.
    check(
        "an inset shadow darkens the inner edge",
        1.0 - luma(161, 260),
        0.08,
        false,
    );
    check(
        "an inset shadow does not spill outside",
        1.0 - luma(159, 260),
        0.002,
        true,
    );
    check(
        "an inset shadow leaves the middle clear",
        luma(220, 260),
        0.995,
        false,
    );

    // 8. The CPU presenter evaluates the same distance and the same CDF, so its picture must
    // match the GPU's at every probe rather than merely looking like a shadow.
    let mut cpu_target = ui_paint::Pixels::new(W, H);
    ui_paint::CpuRenderer::new().render(&list, &mut cpu_target, &ui_core::Damage::Full);
    let cpu_luma = |x: u32, y: u32| -> f32 {
        let p = cpu_target.get(x, y);
        (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
    };
    let mut worst = 0.0f32;
    for (x, y) in [
        (150, 110),
        (74, 110),
        (78, 110),
        (70, 110),
        (66, 110),
        (84, 64),
        (80, 260),
        (19, 260),
        (161, 260),
        (159, 260),
        (220, 260),
    ] {
        worst = worst.max((cpu_luma(x, y) - luma(x, y)).abs());
    }
    check("CPU and GPU agree at every probe", worst, 0.01, true);

    println!(
        "  stats: rects {} shadows_drawn {} draw_calls {}",
        stats.rects, stats.shadows_drawn, stats.draw_calls
    );
    check(
        "every shadow primitive was drawn",
        stats.shadows_drawn as f32,
        3.0,
        false,
    );
    let kinds = list.prims.len_of(PrimKind::Shadow);
    check(
        "three shadow primitives in the list",
        kinds as f32,
        3.0,
        false,
    );

    println!("\nFAIL={fail}");
    std::process::exit(if fail == 0 { 0 } else { 1 });
}
