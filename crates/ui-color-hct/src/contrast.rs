//! Contrast between tones, and the tone shifts that reach a requested ratio.

use crate::lab::{lstar_from_y, y_from_lstar};

/// WCAG's ratio, with luminance in the 0..100 domain the model works in.
pub fn ratio_of_ys(y1: f64, y2: f64) -> f64 {
    let lighter = y1.max(y2);
    let darker = if lighter == y2 { y1 } else { y2 };
    (lighter + 5.0) / (darker + 5.0)
}

/// Contrast of two tones; this is why a tone gap of 40 buys at least 3:1.
pub fn ratio_of_tones(a: f64, b: f64) -> f64 {
    ratio_of_ys(
        y_from_lstar(a.clamp(0.0, 100.0)),
        y_from_lstar(b.clamp(0.0, 100.0)),
    )
}

/// The tone `ratio` lighter than `tone`, without checking whether it exists.
pub fn lighter_unsafe(tone: f64, ratio: f64) -> f64 {
    lstar_from_y(ratio * (y_from_lstar(tone) + 5.0) - 5.0)
}

/// The tone `ratio` darker than `tone`, without checking whether it exists.
pub fn darker_unsafe(tone: f64, ratio: f64) -> f64 {
    lstar_from_y((y_from_lstar(tone) + 5.0) / ratio - 5.0)
}

const RATIO_EPSILON: f64 = 0.04;
const GAMUT_TOLERANCE: f64 = 0.4;

/// A tone `ratio` lighter than `tone`, or -1.0 when sRGB's luminance range cannot hold it.
pub fn lighter(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let dark_y = y_from_lstar(tone);
    let light_y = ratio * (dark_y + 5.0) - 5.0;
    let real = ratio_of_ys(light_y, dark_y);
    if real < ratio && (real - ratio).abs() > RATIO_EPSILON {
        return -1.0;
    }
    let value = lstar_from_y(light_y) + GAMUT_TOLERANCE;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

/// A tone `ratio` darker than `tone`, or -1.0 when the range cannot hold it.
pub fn darker(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let light_y = y_from_lstar(tone);
    let dark_y = (light_y + 5.0) / ratio - 5.0;
    let real = ratio_of_ys(light_y, dark_y);
    if real < ratio && (real - ratio).abs() > RATIO_EPSILON {
        return -1.0;
    }
    let value = lstar_from_y(dark_y) - GAMUT_TOLERANCE;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

/// Foreground rule: a tone below 60 wants a light foreground.
pub fn tone_prefers_light_foreground(tone: f64) -> bool {
    tone.round() < 60.0
}

/// …but only tones at or below 49 can actually carry one.
pub fn tone_allows_light_foreground(tone: f64) -> bool {
    tone.round() <= 49.0
}

/// Pull a tone out of the 50-59 dead zone where neither foreground works.
pub fn enable_light_foreground(tone: f64) -> f64 {
    if tone_prefers_light_foreground(tone) && !tone_allows_light_foreground(tone) {
        49.0
    } else {
        tone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The tone law is the spec's own acceptance basis: verify it, don't assume it.
    #[test]
    fn a_tone_gap_of_40_buys_at_least_three_to_one() {
        for tone in (0..=60).step_by(5) {
            let ratio = ratio_of_tones(f64::from(tone) + 40.0, f64::from(tone));
            assert!(ratio >= 3.0, "gap 40 at tone {tone} gave {ratio}");
        }
    }

    #[test]
    fn an_unreachable_lightening_is_reported_not_forced() {
        assert_eq!(lighter(95.0, 21.0), -1.0);
        assert_eq!(darker(5.0, 21.0), -1.0);
        assert_eq!(lighter(120.0, 3.0), -1.0);
    }

    fn in_range(tone: f64) -> f64 {
        assert!((0.0..=100.0).contains(&tone), "tone out of range: {tone}");
        tone
    }

    #[test]
    fn a_reachable_shift_meets_the_ratio_it_asked_for() {
        assert!(ratio_of_tones(in_range(lighter(30.0, 4.5)), 30.0) >= 4.5);
        assert!(ratio_of_tones(70.0, in_range(darker(70.0, 4.5))) >= 4.5);
    }
}
