//! Check the four dual-backend invariants by rendering one scene twice.
//!
//! Run: `cargo run -p ui-paint-gpu --example gpu_scene [render-node]`
//! Exits non-zero if an invariant is violated.

use std::io::Write;

use ui_core::NodeId;
use ui_core::damage::Damage;
use ui_core::draw::{DrawList, GlyphPrim, LinePrim, RectPrim};
use ui_core::geom::{Color, Corners, Rect};
use ui_gpu_egl::EglContext;
use ui_paint::{CoverageAtlas, CpuRenderer, Pixels};
use ui_paint_gpu::{AtlasUpload, GlDevice, GpuRenderer, OffscreenFbo};

const W: u32 = 240;
const H: u32 = 180;
const ATLAS: u32 = 64;
const TILE: u32 = 16;

/// Build a synthetic coverage atlas with three distinguishable tiles.
fn atlas() -> Vec<u8> {
    let mut data = vec![0u8; (ATLAS * ATLAS) as usize];
    for ty in 0..3u32 {
        for y in 0..TILE {
            for x in 0..TILE {
                let inside = match ty {
                    // A filled disc.
                    0 => {
                        let dx = x as f32 - 7.5;
                        let dy = y as f32 - 7.5;
                        dx * dx + dy * dy <= 56.0
                    }
                    // A diagonal wedge: its edge is where the monotonicity probe looks.
                    1 => (x as f32) < (y as f32) * 0.8 + 2.0,
                    // Vertical bars.
                    _ => x % 4 < 2,
                };
                let v = if inside { 255u8 } else { 0 };
                let px = ty * TILE + x;
                let py = y;
                data[(py * ATLAS + px) as usize] = v;
            }
        }
    }
    data
}

fn scene() -> DrawList {
    let owner = NodeId::from_raw(0, 0);
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    list.push_rect(
        owner,
        RectPrim {
            rect: Rect::new(0.0, 0.0, W as f32, H as f32),
            radius: Corners::ZERO,
            fill: Color::from_rgba8(20, 22, 28, 255),
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        },
    );
    // Opaque square: area conservation.
    list.push_rect(
        owner,
        RectPrim {
            rect: Rect::new(30.0, 20.0, 100.0, 100.0),
            radius: Corners::ZERO,
            fill: Color::from_rgba8(240, 240, 240, 255),
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        },
    );
    // Rounded box with border: the SDF path. The border is deliberately not white: a white
    // ring saturates and hides a shader that scales the border colour by its width.
    list.push_rect(
        owner,
        RectPrim {
            rect: Rect::new(150.0, 20.0, 70.0, 50.0),
            radius: Corners::all(12.0),
            fill: Color::from_rgba8(60, 130, 220, 255),
            border: Some((Color::from_rgba8(200, 120, 40, 255), 3.0)),
            border_gradient: None,
            gradient: None,
            notch: None,
        },
    );
    // Vertical gradient: the shader must sample per pixel, and must agree with the CPU
    // rasteriser sample for sample. A shader that ignored `a_grad` would draw `fill`.
    list.push_rect(
        owner,
        RectPrim {
            rect: Rect::new(140.0, 90.0, 80.0, 60.0),
            radius: Corners::all(10.0),
            fill: Color::from_rgba8(255, 255, 255, 255),
            border: None,
            border_gradient: None,
            gradient: Some(ui_core::draw::Gradient::new(
                Color::from_rgba8(255, 255, 255, 255),
                Color::from_rgba8(0, 0, 0, 255),
                ui_core::draw::GradientAxis::Vertical,
            )),
            notch: None,
        },
    );
    // Half-transparent overlays: overflow probe.
    for i in 0..8 {
        list.push_rect(
            owner,
            RectPrim {
                rect: Rect::new(160.0 + i as f32 * 6.0, 80.0, 30.0, 20.0),
                radius: Corners::all(4.0),
                fill: Color::from_rgba8(255, 200, 40, 64),
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
    }
    // Staircase: edge-position probe.
    for i in 0..20 {
        list.push_rect(
            owner,
            RectPrim {
                rect: Rect::new(30.0 + i as f32 * 5.0, 130.0, 5.0, 24.0 - i as f32),
                radius: Corners::ZERO,
                fill: Color::from_rgba8(200, 80, 90, 255),
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
    }
    // A stroked diagonal in the left margin: the analytic line path.
    list.push_line(
        owner,
        LinePrim {
            from: ui_core::geom::Point::new(12.0, 12.0),
            to: ui_core::geom::Point::new(28.0, 148.0),
            thickness: 2.0,
            colour: Color::from_rgba8(240, 210, 70, 255),
        },
    );
    // A stroked, filled ellipse in the band the taller canvas added: the analytic path has
    // to put the fill inside the curve and the stroke on it, as the CPU rasteriser does.
    list.push_ellipse(
        owner,
        ui_core::draw::EllipsePrim {
            bounds: Rect::new(30.0, 155.0, 60.0, 26.0),
            centre: ui_core::geom::Point::new(60.0, 168.0),
            radii: (26.0, 9.0),
            thickness: 4.0,
            fill: Color::from_rgba8(200, 90, 60, 255),
            colour: Color::from_rgba8(40, 220, 120, 255),
        },
    );
    // Glyphs from the synthetic atlas.
    for i in 0..3u32 {
        list.push_glyph(
            owner,
            GlyphPrim {
                rect: Rect::new(150.0 + i as f32 * 20.0, 110.0, 16.0, 16.0),
                atlas: Rect::new((i * TILE) as f32, 0.0, TILE as f32, TILE as f32),
                alpha: 1.0,
                color: Color::from_rgba8(120, 255, 160, 255),
                kind: ui_core::draw::GlyphKind::Coverage,
            },
        );
    }
    list
}

/// `Pixels` stores premultiplied BGRA; GL readback is straight RGBA.
fn to_rgb(data: &[u8], premultiplied_bgra: bool) -> Vec<[u8; 3]> {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            if premultiplied_bgra {
                let a = p[3] as u32;
                if a == 0 {
                    [0, 0, 0]
                } else if a == 255 {
                    [p[2], p[1], p[0]]
                } else {
                    let un = |c: u8| ((c as u32 * 255 + a / 2) / a).min(255) as u8;
                    [un(p[2]), un(p[1]), un(p[0])]
                }
            } else {
                [p[0], p[1], p[2]]
            }
        })
        .collect()
}

fn write_ppm(path: &str, rgb: &[[u8; 3]]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    write!(f, "P6\n{W} {H}\n255\n")?;
    let mut raw = Vec::with_capacity(rgb.len() * 3);
    for p in rgb {
        raw.extend_from_slice(p);
    }
    f.write_all(&raw)
}

fn main() {
    let list = scene();
    let mut cpu = CpuRenderer::new();
    cpu.set_atlas(CoverageAtlas {
        width: ATLAS,
        height: ATLAS,
        data: atlas(),
    });
    let mut pixels = Pixels::new(W, H);
    cpu.render(&list, &mut pixels, &Damage::Full);
    let cpu_rgb = to_rgb(pixels.as_bytes(), true);
    write_ppm("gpu_scene_cpu.ppm", &cpu_rgb).expect("write cpu ppm");

    let node = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/dri/renderD128".to_string());
    let cnode = std::ffi::CString::new(node.clone()).expect("node path");
    let ctx = EglContext::new(cnode.as_c_str()).expect("EGL context");
    println!("GPU: {} ({node})", ctx.renderer());

    let target = OffscreenFbo::new(ctx.gl(), W, H).expect("target");
    let mut device = GlDevice::new(ctx.gl_handle(), ctx.is_gles());
    let mut gpu = GpuRenderer::new(&mut device).expect("renderer");
    gpu.set_atlas(
        &mut device,
        AtlasUpload {
            width: ATLAS,
            height: ATLAS,
            data: atlas(),
        },
    )
    .expect("atlas");
    gpu.clear(&mut device, &target, [0.0, 0.0, 0.0, 1.0]);
    let stats = gpu.render(&mut device, &target, &list).expect("render");
    let raw = gpu.read_pixels(&mut device, &target).expect("readback");
    if std::env::var("GPU_RAW").is_ok() {
        // Un-premultiplied already; print a few overlay pixels to see the truth.
        for x in [165usize, 175, 185, 195] {
            let i = 90 * W as usize + x;
            println!("  raw x={x} y=90 -> {:?}", &raw[i * 4..i * 4 + 4]);
        }
    }
    let gpu_rgb = to_rgb(&raw, false);
    write_ppm("gpu_scene_gpu.ppm", &gpu_rgb).expect("write gpu ppm");
    println!("stats: {stats:?}");

    let mut failures = 0;

    // [0] Gradient: the shader must sample per pixel and match the CPU rasteriser. A
    // shader ignoring `a_grad` draws the flat `fill` (255 everywhere) and fails both parts.
    {
        let col = |rgb: &[[u8; 3]], y: usize| rgb[y * W as usize + 180][0] as i32;
        let ys = [95usize, 110, 125, 140];
        let ramp: Vec<i32> = ys.iter().map(|y| col(&gpu_rgb, *y)).collect();
        let monotone = ramp.windows(2).all(|w| w[1] < w[0]);
        report(
            "gradient ramps down the box",
            if monotone { 0.0 } else { 1.0 },
            0.0,
            "0 = monotone",
            &mut failures,
        );
        let worst = ys
            .iter()
            .map(|y| (col(&cpu_rgb, *y) - col(&gpu_rgb, *y)).abs())
            .max()
            .unwrap_or(0);
        report(
            "gradient matches the cpu rasteriser",
            worst as f64,
            3.0,
            "LSB",
            &mut failures,
        );
    }

    // [0b] Stroked line: the GPU's analytic segment must agree with the CPU rasteriser
    // along the stroke. Sampling three points catches a missing program or a wrong quad.
    {
        let at = |rgb: &[[u8; 3]], x: usize, y: usize| rgb[y * W as usize + x];
        let worst = [(20usize, 80usize), (16, 40), (24, 120)]
            .iter()
            .map(|&(x, y)| {
                let cpu = at(&cpu_rgb, x, y);
                let gpu = at(&gpu_rgb, x, y);
                (0..3)
                    .map(|c| (cpu[c] as i32 - gpu[c] as i32).abs())
                    .max()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        report(
            "line matches the cpu rasteriser",
            worst as f64,
            3.0,
            "LSB",
            &mut failures,
        );
    }

    // [0c] Ellipse: the GPU's analytic SDF must agree with the CPU rasteriser inside the
    // fill, on the stroke and outside it. The centre probe also says the program ran at all:
    // a missing or mis-strided instance leaves the background there.
    {
        let at = |rgb: &[[u8; 3]], x: usize, y: usize| rgb[y * W as usize + x];
        let probes = [(60usize, 168usize), (86, 168), (60, 156)];
        let worst = probes
            .iter()
            .map(|&(x, y)| {
                let cpu = at(&cpu_rgb, x, y);
                let gpu = at(&gpu_rgb, x, y);
                (0..3)
                    .map(|c| (cpu[c] as i32 - gpu[c] as i32).abs())
                    .max()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        report(
            "ellipse matches the cpu rasteriser",
            worst as f64,
            3.0,
            "LSB",
            &mut failures,
        );
        let filled = at(&gpu_rgb, 60, 168);
        report(
            "ellipse fill is painted",
            if filled[0] > 150 { 0.0 } else { 1.0 },
            0.0,
            "0 = painted",
            &mut failures,
        );
    }

    // [1] Area conservation: an opaque 100x100 square must cover the same area.
    let sq = |rgb: &[[u8; 3]]| -> f64 {
        let mut s = 0.0;
        for y in 20..120usize {
            for x in 30..130usize {
                s += rgb[y * W as usize + x][0] as f64 / 255.0;
            }
        }
        s
    };
    let (a_cpu, a_gpu) = (sq(&cpu_rgb), sq(&gpu_rgb));
    let rel = (a_gpu - a_cpu).abs() / a_cpu.max(1.0);
    report(
        "[1] area conservation",
        rel * 100.0,
        0.5,
        "%",
        &mut failures,
    );

    // [2] No overflow: every channel stays within [0,255] with no wraparound.
    let overflow = gpu_rgb
        .iter()
        .zip(&cpu_rgb)
        .filter(|(g, c)| {
            // A wraparound shows up as the GPU being far darker where the CPU is bright.
            g[0] as i32 + 40 < c[0] as i32 && c[0] > 200
        })
        .count();
    let overflow_pct = overflow as f64 / (W * H) as f64 * 100.0;
    report(
        "[2] overflow pixels",
        overflow_pct,
        0.05,
        "%",
        &mut failures,
    );

    // [3] Total ink: the whole frame's coverage sum.
    let ink = |rgb: &[[u8; 3]]| -> f64 {
        rgb.iter()
            .map(|p| {
                let d = |a: u8, b: u8| (a as f64 - b as f64).abs();
                (d(p[0], 20) + d(p[1], 22) + d(p[2], 28)) / 3.0
            })
            .sum()
    };
    let (i_cpu, i_gpu) = (ink(&cpu_rgb), ink(&gpu_rgb));
    let rel = (i_gpu - i_cpu).abs() / i_cpu.max(1.0);
    report("[3] total ink", rel * 100.0, 0.5, "%", &mut failures);

    // [4] Edge position: the staircase's left boundary must agree within 1 px.
    let mut max_shift = 0i32;
    for y in 132..150usize {
        let edge = |rgb: &[[u8; 3]]| -> Option<usize> {
            (25..150).find(|&x| rgb[y * W as usize + x][0] > 120)
        };
        if let (Some(c), Some(g)) = (edge(&cpu_rgb), edge(&gpu_rgb)) {
            max_shift = max_shift.max((c as i32 - g as i32).abs());
        }
    }
    report("[4] edge shift", max_shift as f64, 1.0, "px", &mut failures);

    // [5] Border ring: ring and fill are one area-weighted mix, so a pixel fully inside the
    // 3px ring must be the border colour. A width folded into the alpha slot turns it into
    // `2 * border - fill`, which the CPU path never produces.
    {
        let probes = [(185usize, 21usize), (152, 45), (185, 68)];
        let worst = probes
            .iter()
            .map(|&(x, y)| {
                let i = y * W as usize + x;
                (0..3)
                    .map(|k| (cpu_rgb[i][k] as i32 - gpu_rgb[i][k] as i32).abs())
                    .max()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        report(
            "[5] border ring colour",
            worst as f64,
            3.0,
            "LSB",
            &mut failures,
        );
    }

    target.release(ctx.gl());

    if failures > 0 {
        eprintln!("\n{failures} invariant(s) violated");
        std::process::exit(1);
    }
    println!("\nall invariants hold; wrote gpu_scene_cpu.ppm / gpu_scene_gpu.ppm");
}

fn report(name: &str, value: f64, limit: f64, unit: &str, failures: &mut u32) {
    let ok = value <= limit;
    println!(
        "{name:28} {value:8.4}{unit}  (limit {limit}{unit})  {}",
        if ok { "OK" } else { "FAIL" }
    );
    if !ok {
        *failures += 1;
    }
}
