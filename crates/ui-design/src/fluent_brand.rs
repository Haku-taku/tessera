//! Fluent v8 brand-ramp generation: the `_darken`/`_lighten` HSV model from Fluent's `shades.ts`.
//!
//! The Windows *default* blue ramp is a hand-authored table (`data::BRAND_RAMP`), not a product
//! of this model; this module generates a plausible 16-tone ramp for any other seed colour.

use crate::value::Rgba;

/// Tone steps of the 16-tone ramp, darkest to lightest; tone 80 is the seed itself.
pub const TONES: [u16; 16] = [
    10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160,
];

/// The tone that carries the seed colour unchanged.
pub const KEY_TONE: u16 = 80;

/// Per-step darken factor (v scaled by `1 - f`); matches Fluent's `_darken` shape.
pub const DARK_STEP: f32 = 0.14;

/// Per-step lighten factor (s scaled by `1 - f`, v lifted toward 1); Fluent's `_lighten` shape.
pub const LIGHT_STEP: f32 = 0.21;

/// HSV with all components in 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hsv {
    h: f32,
    s: f32,
    v: f32,
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> Hsv {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d) % 6.0 / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let h = if h < 0.0 { h + 1.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { d / max };
    Hsv { h, s, v: max }
}

fn hsv_to_rgb(hsv: Hsv) -> (u8, u8, u8) {
    let Hsv { h, s, v } = hsv;
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let q8 = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    (q8(r), q8(g), q8(b))
}

/// Fluent `_darken`: scale value down, keeping hue and saturation.
fn darken(hsv: Hsv, f: f32) -> Hsv {
    Hsv {
        v: hsv.v * (1.0 - f),
        ..hsv
    }
}

/// Fluent `_lighten`: drop saturation and lift value toward white.
fn lighten(hsv: Hsv, f: f32) -> Hsv {
    Hsv {
        s: hsv.s * (1.0 - f),
        v: hsv.v + (1.0 - hsv.v) * f,
        ..hsv
    }
}

/// Generate the 16-tone brand ramp for `seed`: tone 80 is the seed, darker below, lighter above.
pub fn brand_ramp_from_seed(seed: Rgba) -> [(u16, Rgba); 16] {
    let argb = seed.argb();
    let base = rgb_to_hsv((argb >> 16) as u8, (argb >> 8) as u8, argb as u8);
    let key = TONES
        .iter()
        .position(|t| *t == KEY_TONE)
        .expect("key tone present");

    let mut out = [(0u16, seed); 16];
    out[key] = (TONES[key], seed);

    // Darker: walk down from the seed to tone 10, darkening each step.
    let mut hsv = base;
    for j in (0..key).rev() {
        hsv = darken(hsv, DARK_STEP);
        let (r, g, b) = hsv_to_rgb(hsv);
        out[j] = (TONES[j], Rgba::from_rgb(r, g, b));
    }

    // Lighter: walk up from the seed to tone 160, lightening each step.
    let mut hsv = base;
    for j in (key + 1)..TONES.len() {
        hsv = lighten(hsv, LIGHT_STEP);
        let (r, g, b) = hsv_to_rgb(hsv);
        out[j] = (TONES[j], Rgba::from_rgb(r, g, b));
    }
    out
}
