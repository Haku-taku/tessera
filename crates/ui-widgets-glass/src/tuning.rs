//! Live knobs for the glass material, so the gallery can tune it without a rebuild.

use std::sync::atomic::{AtomicU32, Ordering};

/// The bit pattern of "no override": `f32::from_bits` reads it as NaN.
const UNSET: u32 = u32::MAX;

/// One tunable number, named once so the slider, the state and the shader agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knob {
    ShadowBlur,
    ShadowDy,
    ShadowAlpha,
    ShadowRing,
    BorderWidth,
    BorderAlpha,
    RefractiveIndex,
    Thickness,
    CaptureScale,
    FrostBlur,
    Dispersion,
    FresnelIntensity,
    FresnelDistance,
    FresnelSharpness,
    GlareIntensity,
    GlareDistance,
    GlareSharpness,
    GlareConvergence,
    GlareBias,
    GlareOffset,
}

impl Knob {
    pub const COUNT: usize = 20;

    /// Every knob, in the order the gallery lists them.
    pub const ALL: [Knob; Self::COUNT] = [
        Knob::ShadowBlur,
        Knob::ShadowDy,
        Knob::ShadowAlpha,
        Knob::ShadowRing,
        Knob::BorderWidth,
        Knob::BorderAlpha,
        Knob::RefractiveIndex,
        Knob::Thickness,
        Knob::CaptureScale,
        Knob::FrostBlur,
        Knob::Dispersion,
        Knob::FresnelIntensity,
        Knob::FresnelDistance,
        Knob::FresnelSharpness,
        Knob::GlareIntensity,
        Knob::GlareDistance,
        Knob::GlareSharpness,
        Knob::GlareConvergence,
        Knob::GlareBias,
        Knob::GlareOffset,
    ];

    fn slot(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// The untranslated slider caption: this page tunes the engine, not a locale.
    pub fn label(self) -> &'static str {
        match self {
            Knob::ShadowBlur => "shadow blur",
            Knob::ShadowDy => "shadow dy",
            Knob::ShadowAlpha => "shadow α",
            Knob::ShadowRing => "shadow ring",
            Knob::BorderWidth => "border w",
            Knob::BorderAlpha => "border α",
            Knob::RefractiveIndex => "refractive",
            Knob::Thickness => "thickness",
            Knob::CaptureScale => "capture scale",
            Knob::FrostBlur => "frost blur",
            Knob::Dispersion => "dispersion",
            Knob::FresnelIntensity => "fresnel",
            Knob::FresnelDistance => "fresnel dist",
            Knob::FresnelSharpness => "fresnel sharp",
            Knob::GlareIntensity => "glare",
            Knob::GlareDistance => "glare dist",
            Knob::GlareSharpness => "glare sharp",
            Knob::GlareConvergence => "glare conv",
            Knob::GlareBias => "glare bias",
            Knob::GlareOffset => "glare angle",
        }
    }

    /// The slider's `(low, high)` in the knob's own unit.
    pub fn range(self) -> (f32, f32) {
        match self {
            Knob::ShadowBlur => (0.0, 20.0),
            Knob::ShadowDy => (-10.0, 16.0),
            Knob::ShadowAlpha => (0.0, 1.0),
            Knob::ShadowRing => (0.0, 12.0),
            Knob::BorderWidth => (0.0, 4.0),
            Knob::BorderAlpha => (0.0, 1.0),
            Knob::RefractiveIndex => (1.0, 2.0),
            Knob::Thickness => (0.0, 40.0),
            Knob::CaptureScale => (0.5, 1.5),
            Knob::FrostBlur => (0.0, 40.0),
            Knob::Dispersion => (0.0, 20.0),
            Knob::FresnelIntensity => (0.0, 1.0),
            Knob::FresnelDistance => (0.0, 120.0),
            Knob::FresnelSharpness => (-4.0, 8.0),
            Knob::GlareIntensity => (0.0, 1.0),
            Knob::GlareDistance => (0.0, 120.0),
            Knob::GlareSharpness => (-4.0, 8.0),
            Knob::GlareConvergence => (0.0, std::f32::consts::PI),
            Knob::GlareBias => (0.0, 4.0),
            Knob::GlareOffset => (0.0, std::f32::consts::TAU),
        }
    }
}

/// The value a knob has when nothing is pinned: the rim constants, or the kit's own field.
pub fn default_of(knob: Knob, kit: &ui_core::draw::LiquidGlass) -> f32 {
    use crate::shared::{
        GLASS_RIM_ALPHA, GLASS_RIM_W, RIM_SHADOW_ALPHA, RIM_SHADOW_BLUR, RIM_SHADOW_DY,
        RIM_SHADOW_RING,
    };
    match knob {
        Knob::ShadowBlur => RIM_SHADOW_BLUR,
        Knob::ShadowDy => RIM_SHADOW_DY,
        Knob::ShadowAlpha => RIM_SHADOW_ALPHA,
        Knob::ShadowRing => RIM_SHADOW_RING,
        Knob::BorderWidth => GLASS_RIM_W,
        Knob::BorderAlpha => GLASS_RIM_ALPHA,
        Knob::RefractiveIndex => kit.refractive_index,
        Knob::Thickness => kit.thickness,
        Knob::CaptureScale => kit.capture_scale,
        Knob::FrostBlur => kit.frost,
        Knob::Dispersion => kit.dispersion,
        Knob::FresnelIntensity => kit.fresnel_intensity,
        Knob::FresnelDistance => kit.fresnel_distance,
        Knob::FresnelSharpness => kit.fresnel_sharpness,
        Knob::GlareIntensity => kit.glare_intensity,
        Knob::GlareDistance => kit.glare_distance,
        Knob::GlareSharpness => kit.glare_sharpness,
        Knob::GlareConvergence => kit.glare_convergence,
        Knob::GlareBias => kit.glare_bias,
        Knob::GlareOffset => kit.glare_offset,
    }
}

/// Where a real value sits on its slider, `0..=1`.
pub fn normalised(knob: Knob, v: f32) -> f32 {
    let (lo, hi) = knob.range();
    if (hi - lo).abs() < 1e-6 {
        0.0
    } else {
        ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
    }
}

/// The real value a slider position names.
pub fn denormalised(knob: Knob, t: f32) -> f32 {
    let (lo, hi) = knob.range();
    lo + t.clamp(0.0, 1.0) * (hi - lo)
}

static VALUES: [AtomicU32; Knob::COUNT] = [const { AtomicU32::new(UNSET) }; Knob::COUNT];

/// The value in force: the override if the gallery set one, else the recipe's own.
pub fn value(knob: Knob, fallback: f32) -> f32 {
    get(knob).unwrap_or(fallback)
}

/// Pin a knob. `None` hands it back to the recipe.
pub fn set(knob: Knob, v: Option<f32>) {
    let bits = v.map(f32::to_bits).unwrap_or(UNSET);
    VALUES[knob.slot()].store(bits, Ordering::Relaxed);
}

/// Whether the gallery has pinned this knob, and to what.
pub fn get(knob: Knob) -> Option<f32> {
    let v = f32::from_bits(VALUES[knob.slot()].load(Ordering::Relaxed));
    (!v.is_nan()).then_some(v)
}

/// Hand every knob back to its recipe.
pub fn reset() {
    for knob in Knob::ALL {
        set(knob, None);
    }
}
