//! Tone resolution: ContrastCurve, ToneDeltaPair and the three GetTone cases.

use ui_color_hct::{
    darker, enable_light_foreground, lighter, ratio_of_tones, tone_prefers_light_foreground,
};

/// A contrast requirement at four levels; `get` interpolates over -1..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve(pub [f64; 4]);

impl Curve {
    pub fn get(&self, level: f64) -> f64 {
        let [low, normal, medium, high] = self.0;
        if level <= -1.0 {
            low
        } else if level < 0.0 {
            lerp(low, normal, level + 1.0)
        } else if level < 0.5 {
            lerp(normal, medium, level / 0.5)
        } else if level < 1.0 {
            lerp(medium, high, (level - 0.5) / 0.5)
        } else {
            high
        }
    }
}

/// f64 because a tone is a CAM16 lightness: `ui_core::geom::lerp` is the f32 animation curve,
/// and rounding here would blur the contrast steps these levels exist to separate.
fn lerp(start: f64, stop: f64, amount: f64) -> f64 {
    (1.0 - amount) * start + amount * stop
}

/// Which side of a pair should sit nearer its background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polarity {
    Darker,
    Lighter,
    Nearer,
    Farther,
}

/// Two roles that must keep a minimum tone distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeltaPair {
    pub a: &'static str,
    pub b: &'static str,
    pub delta: f64,
    pub polarity: Polarity,
    pub stay_together: bool,
}

/// The foreground tone that reaches `ratio` against `bg`, preferring the native side.
pub fn foreground_tone(bg: f64, ratio: f64) -> f64 {
    let lighter_tone = lighter(bg, ratio);
    let darker_tone = darker(bg, ratio);
    // Unsafe variants always yield a number, so the comparison is well defined.
    let up = ui_color_hct::lighter_unsafe(bg, ratio);
    let down = ui_color_hct::darker_unsafe(bg, ratio);
    let lighter_ratio = ratio_of_tones(up, bg);
    let darker_ratio = ratio_of_tones(down, bg);
    let prefer_light = tone_prefers_light_foreground(bg);
    let negligible = (lighter_ratio - darker_ratio).abs() < 0.1 && lighter_ratio < ratio;
    let take_light = if prefer_light {
        lighter_ratio >= ratio || lighter_ratio >= darker_ratio || negligible
    } else {
        !(darker_ratio >= ratio || darker_ratio >= lighter_ratio)
    };
    if take_light {
        if lighter_tone < 0.0 {
            100.0
        } else {
            lighter_tone
        }
    } else if darker_tone < 0.0 {
        0.0
    } else {
        darker_tone
    }
}

/// GetTone case 1: the tone of `want` given its partner and the pair's constraint.
///
/// Arguments are (nearer, farther) in *role identity*, not in tone order: for polarity
/// `nearer`, `a` is the nearer role. Mirrors material_dynamic_colors' GetTone case 1.
pub fn resolve_delta_pair(
    want: &str,
    self_tone: f64,
    other_tone: f64,
    pair: &DeltaPair,
    is_dark: bool,
) -> (f64, f64) {
    // nearer/farther are decided by role identity: role `a` is nearer for polarity nearer.
    let (near_id, _far_id) = match pair.polarity {
        Polarity::Nearer => (pair.a, pair.b),
        Polarity::Farther => (pair.b, pair.a),
        Polarity::Lighter => {
            if is_dark {
                (pair.b, pair.a)
            } else {
                (pair.a, pair.b)
            }
        }
        Polarity::Darker => {
            if is_dark {
                (pair.a, pair.b)
            } else {
                (pair.b, pair.a)
            }
        }
    };
    let self_is_nearer = want == near_id;
    let (near_tone, far_tone) = if self_is_nearer {
        (self_tone, other_tone)
    } else {
        (other_tone, self_tone)
    };
    // Dark themes widen upward, light themes downward.
    let expand = if is_dark { 1.0 } else { -1.0 };
    let mut near = near_tone;
    let mut far = far_tone;
    if (far - near) * expand < pair.delta {
        far = (near + pair.delta * expand).clamp(0.0, 100.0);
        if (far - near) * expand < pair.delta {
            near = (far - pair.delta * expand).clamp(0.0, 100.0);
        }
    }
    // Keep both out of the 50..59 dead zone.
    near = enable_light_foreground(near);
    far = enable_light_foreground(far);
    (near, far)
}
