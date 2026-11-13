//! Blur as an instance of the effect category, checked by four invariants.
//!
//! The renderer has no `Blur` case: it runs a separable kernel and a colour matrix. Both
//! scenes go through the same layer path, so the only difference is the chain.
//!
//! Run: `cargo run -p ui-paint --example effects_cpu`

use ui_core::damage::Damage;
use ui_core::draw::{DrawList, RectPrim};
use ui_core::geom::{Color, Corners, Rect};
use ui_core::{Effect, NodeId};
use ui_paint::{CpuRenderer, Pixels};

const W: u32 = 320;
const H: u32 = 240;
const RADIUS: u32 = 6;
const SIGMA: f32 = 2.5;
/// Centre of the blurred square, in pixels.
const DOT: (u32, u32) = (72, 72);
const DOT_SIZE: f32 = 24.0;

fn scene(effects: Vec<Effect>) -> DrawList {
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    list.push_layer(
        Rect::new(0.0, 0.0, W as f32, H as f32),
        effects,
        Corners::ZERO,
    );
    list.push_rect(
        NodeId::from_raw(1, 0),
        RectPrim {
            rect: Rect::new(60.0, 60.0, DOT_SIZE, DOT_SIZE),
            radius: Corners::ZERO,
            fill: Color::WHITE,
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        },
    );
    list.pop_layer();
    list
}

fn render(effects: Vec<Effect>) -> Pixels {
    let list = scene(effects);
    let mut target = Pixels::new(W, H);
    let mut renderer = CpuRenderer::new();
    renderer.render(&list, &mut target, &Damage::Full);
    target
}

fn at(pixels: &Pixels, x: u32, y: u32) -> [u8; 4] {
    let o = ((y * pixels.width + x) * 4) as usize;
    [
        pixels.data[o],
        pixels.data[o + 1],
        pixels.data[o + 2],
        pixels.data[o + 3],
    ]
}

fn luma(px: [u8; 4]) -> u32 {
    px[0] as u32 + px[1] as u32 + px[2] as u32
}

fn main() {
    // A/B: identical geometry and identical layer path, only the chain differs.
    let sharp = render(vec![Effect::none()]);
    let blurred = render(vec![Effect::blur(RADIUS, SIGMA)]);

    let mut failures = 0;
    let mut check = |ok: bool, what: &str| {
        println!("[{what}] {}", if ok { "OK" } else { "FAIL" });
        if !ok {
            failures += 1;
        }
    };

    // 1. A normalised kernel redistributes ink, it does not create it.
    let sum = |p: &Pixels| -> u64 {
        p.data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| c[0] as u64 + c[1] as u64 + c[2] as u64)
            .sum()
    };
    let (a, b) = (sum(&sharp), sum(&blurred));
    let drift = (a as i64 - b as i64).abs();
    println!("total ink {a} -> {b} (drift {drift})");
    check(drift * 100 <= a.max(1) as i64, "1 mass is conserved");

    // 2. The peak drops: the same ink now covers more pixels.
    let (peak_a, peak_b) = (
        luma(at(&sharp, DOT.0, DOT.1)),
        luma(at(&blurred, DOT.0, DOT.1)),
    );
    println!("centre {} -> {}", peak_a, peak_b);
    check(peak_b < peak_a, "2 the centre gets dimmer");

    // 3. Ink reaches pixels that were background before.
    let (near_a, near_b) = (luma(at(&sharp, 58, DOT.1)), luma(at(&blurred, 58, DOT.1)));
    println!("just outside the edge {near_a} -> {near_b}");
    check(near_b > near_a + 8, "3 ink spreads outwards");

    // 4. The kernel's reach is bounded, so a distant pixel is byte-identical.
    let far_a = at(&sharp, 200, 200);
    let far_b = at(&blurred, 200, 200);
    println!("far pixel {far_a:?} vs {far_b:?}");
    check(far_a == far_b, "4 distant pixels are untouched");

    if std::env::var("DUMP_EFFECTS").is_ok() {
        use std::io::Write;
        for (name, p) in [("sharp", &sharp), ("blur", &blurred)] {
            let path = format!("/tmp/effects_{name}.ppm");
            let mut f = std::fs::File::create(&path).expect("ppm");
            write!(f, "P6\n{} {}\n255\n", p.width, p.height).expect("hdr");
            for px in p.data.as_chunks::<4>().0 {
                let _ = f.write_all(&[px[2], px[1], px[0]]);
            }
            println!("dumped {path}");
        }
    }

    if failures > 0 {
        eprintln!("\n{failures} invariant(s) failed");
        std::process::exit(1);
    }
    println!("\nblur behaves as a normalised kernel, not as a named special case");
}
