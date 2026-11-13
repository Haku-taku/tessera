//! Fluent's acrylic recipe: the WinUI two-stage tint plus the grain layer.
//!
//! Ported from `fluent_ui/lib/src/controls/surfaces/acrylic.dart` (`AcrylicHelper`);
//! the GPU pass only consumes the resulting colours, so this stays portable and testable.

use ui_core::geom::Color;

/// The reference blur amount: `acrylic.dart:8` (`kBlurAmount = 30`).
pub const BLUR_SIGMA: f32 = 30.0;

/// The grain layer's opacity: `acrylic.dart:552` (`Opacity(opacity: 0.02)`).
pub const NOISE_OPACITY: f32 = 0.02;

/// One acrylic surface's resolved values, ready for the backdrop pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcrylicRecipe {
    /// Tint colour; `a` already includes WinUI's tint-opacity modifier.
    pub tint: Color,
    /// Luminosity colour; `a` is the luminosity stage's strength.
    pub luminosity: Color,
    pub noise: f32,
    pub blur_sigma: f32,
    /// What to draw instead when the backdrop cannot be sampled.
    pub fallback: Color,
}

fn hsv_value(c: Color) -> f32 {
    c.r.max(c.g).max(c.b)
}

fn hsv_saturation(c: Color) -> f32 {
    let v = hsv_value(c);
    if v <= 0.0 {
        return 0.0;
    }
    (v - c.r.min(c.g).min(c.b)) / v
}

/// Keep hue and saturation, replace the HSV value; black has no hue to keep.
fn with_value(c: Color, v: f32) -> Color {
    let old = hsv_value(c);
    if old <= 0.0 {
        return Color::rgba(v, v, v, c.a);
    }
    let k = v / old;
    Color::rgba(
        (c.r * k).min(1.0),
        (c.g * k).min(1.0),
        (c.b * k).min(1.0),
        c.a,
    )
}

/// WinUI's tint-opacity suppression curve: `acrylic.dart:646-678`.
pub fn tint_opacity_modifier(color: Color) -> f32 {
    const MID_POINT: f32 = 0.50;
    const WHITE_MAX: f32 = 0.45;
    const MID_POINT_MAX: f32 = 0.90;
    const BLACK_MAX: f32 = 0.85;
    let value = hsv_value(color);
    if value == MID_POINT {
        return MID_POINT_MAX;
    }
    let (lowest_max, max_deviation) = if value > MID_POINT {
        (WHITE_MAX, 1.0 - MID_POINT)
    } else {
        (BLACK_MAX, MID_POINT)
    };
    let mut suppression = MID_POINT_MAX - lowest_max;
    let deviation = (value - MID_POINT).abs() / max_deviation;
    let saturation = hsv_saturation(color);
    if saturation > 0.0 {
        suppression *= (1.0 - saturation * 2.0).max(0.0);
    }
    MID_POINT_MAX - suppression * deviation
}

/// `AcrylicHelper.getEffectiveTintColor`: `acrylic.dart:597-611`.
pub fn effective_tint(color: Color, tint_opacity: f32, luminosity_opacity: Option<f32>) -> Color {
    let opacity = tint_opacity.clamp(0.0, 1.0);
    let modifier = match luminosity_opacity {
        Some(_) => 1.0,
        None => tint_opacity_modifier(color),
    };
    Color::rgba(color.r, color.g, color.b, color.a * opacity * modifier)
}

/// `AcrylicHelper.getLuminosityColor`: `acrylic.dart:613-644`.
pub fn luminosity_color(tint: Color, luminosity_opacity: Option<f32>, tint_opacity: f32) -> Color {
    let source = Color::rgba(
        tint.r,
        tint.g,
        tint.b,
        tint.a * tint_opacity.clamp(0.0, 1.0),
    );
    match luminosity_opacity {
        Some(l) => Color::rgba(source.r, source.g, source.b, l.clamp(0.0, 1.0)),
        None => {
            let clamped = hsv_value(source).clamp(0.125, 0.965);
            let rgb = with_value(source, clamped);
            let mapped = source.a * (1.03 - 0.15) + 0.15;
            Color::rgba(rgb.r, rgb.g, rgb.b, mapped.min(1.0))
        }
    }
}

/// Light theme acrylic: `theme.dart:485-492` with `acrylic.dart:8` and `:552`.
pub fn acrylic_light() -> AcrylicRecipe {
    // 0xFFFCFCFC tint, TintOpacity 0.0, TintLuminosityOpacity 0.85, fallback 0xFFF9F9F9.
    let tint = Color::from_rgba8(0xFC, 0xFC, 0xFC, 255);
    AcrylicRecipe {
        tint: effective_tint(tint, 0.0, Some(0.85)),
        luminosity: luminosity_color(tint, Some(0.85), 0.0),
        noise: NOISE_OPACITY,
        blur_sigma: BLUR_SIGMA,
        fallback: Color::from_rgba8(0xF9, 0xF9, 0xF9, 255),
    }
}

/// Dark theme acrylic: the same fields with the dark column of `theme.dart:485-492`.
pub fn acrylic_dark() -> AcrylicRecipe {
    let tint = Color::from_rgba8(0x2C, 0x2C, 0x2C, 255);
    AcrylicRecipe {
        tint: effective_tint(tint, 0.15, Some(0.96)),
        luminosity: luminosity_color(tint, Some(0.96), 0.15),
        noise: NOISE_OPACITY,
        blur_sigma: BLUR_SIGMA,
        fallback: Color::from_rgba8(0x2C, 0x2C, 0x2C, 255),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Automatic mode clamps the value into [0.125, 0.965] and maps alpha through [0.15, 1.03].
    #[test]
    fn automatic_luminosity_clamps_the_value_and_maps_the_opacity() {
        let white = luminosity_color(Color::WHITE, None, 1.0);
        assert!(
            (hsv_value(white) - 0.965).abs() < 1e-5,
            "value {}",
            hsv_value(white)
        );
        assert!((white.a - 1.0).abs() < 1e-6, "alpha {}", white.a);

        let black = luminosity_color(Color::BLACK, None, 1.0);
        assert!(
            (hsv_value(black) - 0.125).abs() < 1e-5,
            "value {}",
            hsv_value(black)
        );

        let half = luminosity_color(Color::rgb(0.4, 0.4, 0.4), None, 0.5);
        let expected = 0.5 * (1.03 - 0.15) + 0.15;
        assert!(
            (half.a - expected).abs() < 1e-6,
            "alpha {} vs {expected}",
            half.a
        );
    }
}
