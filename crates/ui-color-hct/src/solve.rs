//! HCL (hue, chroma, L\*) to sRGB: iterate CAM16's J, then shrink chroma to fit the gamut.

use crate::cam::linrgb_from_jch;
use crate::lab::{argb_from_linrgb, argb_from_tone, y_from_lstar};
use crate::viewing::Viewing;

const Y_WEIGHT: [f64; 3] = [0.2126, 0.7152, 0.0722];
const TONE_TOLERANCE: f64 = 0.002;
const GAMUT_SLACK: f64 = 100.01;

/// Newton-style search on J for the linear colour at this hue, chroma and luminance.
fn linrgb_by_j(hue: f64, chroma: f64, y: f64, v: &Viewing) -> Option<[f64; 3]> {
    let mut j = y.sqrt() * 11.0;
    for round in 0..5 {
        let lin = linrgb_from_jch(j, chroma, hue, v);
        if lin.iter().any(|c| *c < 0.0) {
            return None;
        }
        let fnj = Y_WEIGHT[0] * lin[0] + Y_WEIGHT[1] * lin[1] + Y_WEIGHT[2] * lin[2];
        if fnj <= 0.0 {
            return None;
        }
        if round == 4 || (fnj - y).abs() < TONE_TOLERANCE {
            return if lin.iter().any(|c| *c > GAMUT_SLACK) {
                None
            } else {
                Some(lin)
            };
        }
        j -= (fnj - y) * j / (2.0 * fnj);
    }
    None
}

/// A colour at the requested hue, chroma and tone, with chroma reduced if sRGB has no room.
pub fn argb_from_hcl_in(hue: f64, chroma: f64, lstar: f64, v: &Viewing) -> u32 {
    if chroma < 1e-4 || lstar < 1e-4 || lstar > 99.9999 {
        return argb_from_tone(lstar);
    }
    let y = y_from_lstar(lstar);
    if let Some(lin) = linrgb_by_j(hue, chroma, y, v) {
        return argb_from_linrgb(lin);
    }
    // Along this hue and tone, attainable chroma is monotone: bisect down to the boundary.
    let (mut lo, mut hi) = (0.0f64, chroma);
    let mut best = argb_from_tone(lstar);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        match linrgb_by_j(hue, mid, y, v) {
            Some(lin) => {
                best = argb_from_linrgb(lin);
                lo = mid;
            }
            None => hi = mid,
        }
    }
    best
}

thread_local! {
    /// A theme asks for the same few hundred colours on every frame, and each solve is a
    /// Newton loop plus a gamut bisection; remembering them turns the cost into a lookup.
    static SOLVED: std::cell::RefCell<std::collections::HashMap<(u64, u64, u64), u32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

pub fn argb_from_hcl(hue: f64, chroma: f64, lstar: f64) -> u32 {
    // Keyed on the exact bits: an animated theme still misses, but never collides.
    let key = (hue.to_bits(), chroma.to_bits(), lstar.to_bits());
    if let Some(argb) = SOLVED.with_borrow(|cache| cache.get(&key).copied()) {
        return argb;
    }
    let argb = argb_from_hcl_in(hue, chroma, lstar, &Viewing::srgb());
    SOLVED.with_borrow_mut(|cache| {
        // A gradient-heavy or animating theme could otherwise grow this without end.
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(key, argb);
    });
    argb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::cam_from_argb;
    use crate::lab::tone_of;

    #[test]
    fn an_in_gamut_colour_round_trips_to_the_same_pixels() {
        for argb in [
            0xff67_50a4u32,
            0xff0f_6cbd,
            0xff34_c759,
            0xffb4_d6fa,
            0xff24_2424,
        ] {
            let cam = cam_from_argb(argb);
            let back = argb_from_hcl(cam.hue, cam.chroma, tone_of(argb));
            for shift in [16, 8, 0] {
                let a = (argb >> shift) & 0xff;
                let b = (back >> shift) & 0xff;
                assert!(
                    (a as i32 - b as i32).abs() <= 1,
                    "{argb:#x} -> {back:#x} at {shift}"
                );
            }
        }
    }

    #[test]
    fn an_impossible_chroma_is_reduced_instead_of_wrapping() {
        // Deep violet at near-white is far outside sRGB at full chroma.
        let argb = argb_from_hcl(300.0, 120.0, 92.0);
        let cam = cam_from_argb(argb);
        assert!(
            cam.chroma < 120.0,
            "chroma should shrink, got {}",
            cam.chroma
        );
        assert!(
            cam.chroma > 1.0,
            "chroma should stay colourful, got {}",
            cam.chroma
        );
        assert!((cam.hue - 300.0).abs() < 3.0, "hue drifted to {}", cam.hue);
    }
}
