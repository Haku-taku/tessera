// Generated from docs/reference/{md3-scales,fluent-tokens,hig-tokens}.json.

use crate::dimension::{Curvature::*, DurationStep, HitTarget, MotionScale, RadiusScale,
    RadiusStep, SpacingScale, SpacingStep, TypeRamp, TypeStep};
use crate::source::Source;

pub static MD3_RADIUS: &[RadiusStep] = &[
    RadiusStep { name: "none", points: 0.0f32 },
    RadiusStep { name: "extraSmall", points: 4.0f32 },
    RadiusStep { name: "small", points: 8.0f32 },
    RadiusStep { name: "medium", points: 12.0f32 },
    RadiusStep { name: "large", points: 16.0f32 },
    RadiusStep { name: "largeIncreased", points: 20.0f32 },
    RadiusStep { name: "extraLarge", points: 28.0f32 },
    RadiusStep { name: "extraLargeIncreased", points: 32.0f32 },
    RadiusStep { name: "extraExtraLarge", points: 48.0f32 },
    RadiusStep { name: "full", points: -1.0f32 },
];
pub static MD3_RADIUS_SCALE: RadiusScale = RadiusScale {
    steps: MD3_RADIUS,
    curvature: Round,
    supports_cut: true,
    source: Source::Spec("m3.material.io/styles/shape/corner-radius-scale"),
};

// MD3 spacing: 8dp base, the documented multiples plus 2/4/6/10 nested values.
pub static MD3_SPACING: &[SpacingStep] = &[
    SpacingStep { name: "space50", multiple: 0.5f32 },
    SpacingStep { name: "space100", multiple: 1f32 },
    SpacingStep { name: "space150", multiple: 1.5f32 },
    SpacingStep { name: "space200", multiple: 2f32 },
    SpacingStep { name: "space300", multiple: 3f32 },
    SpacingStep { name: "space400", multiple: 4f32 },
    SpacingStep { name: "space500", multiple: 5f32 },
    SpacingStep { name: "space600", multiple: 6f32 },
    SpacingStep { name: "space800", multiple: 8f32 },
    SpacingStep { name: "space1000", multiple: 10f32 },
    SpacingStep { name: "space1200", multiple: 12f32 },
];
pub static MD3_NESTED: &[f32] = &[2f32, 4f32, 6f32, 10f32];
pub static MD3_SPACING_SCALE: SpacingScale = SpacingScale {
    base_points: 8f32,
    steps: MD3_SPACING,
    nested: MD3_NESTED,
    source: Source::Spec("m3.material.io/styles/spacing/overview"),
};

pub static MD3_TYPE: &[TypeStep] = &[
    TypeStep { name: "displayLarge", size: 57f32, line_height: 64f32, weight: 400 },
    TypeStep { name: "displayMedium", size: 45f32, line_height: 52f32, weight: 400 },
    TypeStep { name: "displaySmall", size: 36f32, line_height: 44f32, weight: 400 },
    TypeStep { name: "headlineLarge", size: 32f32, line_height: 40f32, weight: 400 },
    TypeStep { name: "headlineMedium", size: 28f32, line_height: 36f32, weight: 400 },
    TypeStep { name: "headlineSmall", size: 24f32, line_height: 32f32, weight: 400 },
    TypeStep { name: "titleLarge", size: 22f32, line_height: 28f32, weight: 400 },
    TypeStep { name: "titleMedium", size: 16f32, line_height: 24f32, weight: 500 },
    TypeStep { name: "titleSmall", size: 14f32, line_height: 20f32, weight: 500 },
    TypeStep { name: "bodyLarge", size: 16f32, line_height: 24f32, weight: 400 },
    TypeStep { name: "bodyMedium", size: 14f32, line_height: 20f32, weight: 400 },
    TypeStep { name: "bodySmall", size: 12f32, line_height: 16f32, weight: 400 },
    TypeStep { name: "labelLarge", size: 14f32, line_height: 20f32, weight: 500 },
    TypeStep { name: "labelMedium", size: 12f32, line_height: 16f32, weight: 500 },
    TypeStep { name: "labelSmall", size: 11f32, line_height: 16f32, weight: 500 },
];
pub static MD3_TYPE_RAMP: TypeRamp = TypeRamp {
    steps: MD3_TYPE,
    scalable: false,
    source: Source::Spec("M3 2021 type scale (typography.dart)"),
};

pub static MD3_DURATIONS: &[DurationStep] = &[
    DurationStep { name: "short1", ms: 50f32 },
    DurationStep { name: "short2", ms: 100f32 },
    DurationStep { name: "short3", ms: 150f32 },
    DurationStep { name: "short4", ms: 200f32 },
    DurationStep { name: "medium1", ms: 250f32 },
    DurationStep { name: "medium2", ms: 300f32 },
    DurationStep { name: "medium3", ms: 350f32 },
    DurationStep { name: "medium4", ms: 400f32 },
    DurationStep { name: "long1", ms: 450f32 },
    DurationStep { name: "long2", ms: 500f32 },
    DurationStep { name: "long3", ms: 550f32 },
    DurationStep { name: "long4", ms: 600f32 },
];
pub static MD3_CURVES: &[(&str, [f32; 4])] = &[
    ("emphasizedDecelerate", [0.05f32, 0.7f32, 0.1f32, 1.0f32]),
    ("emphasizedAccelerate", [0.3f32, 0.0f32, 0.8f32, 0.15f32]),
    ("standard", [0.2f32, 0.0f32, 0.0f32, 1.0f32]),
    ("standardDecelerate", [0.0f32, 0.0f32, 0.0f32, 1.0f32]),
    ("standardAccelerate", [0.3f32, 0.0f32, 1.0f32, 1.0f32]),
    ("fastOutSlowIn", [0.4f32, 0.0f32, 0.2f32, 1.0f32]),
    ("easeOutBack", [0.175f32, 0.885f32, 0.32f32, 1.275f32]),
];
pub static MD3_MOTION: MotionScale = MotionScale {
    durations: MD3_DURATIONS,
    curves: MD3_CURVES,
    source: Source::Spec("Flutter Curves (md3 docs motion tables unavailable)"),
};

pub static MD3_HIT: HitTarget = HitTarget {
    default_points: 48f32,
    minimum_points: 48f32,
    source: Source::Spec("M3 touch target 48dp"),
};

// Fluent radii: 0..40 plus `circular` (capsule sentinel -1).
pub static FLUENT_RADIUS: &[RadiusStep] = &[
    RadiusStep { name: "none", points: 0.0f32 },
    RadiusStep { name: "small", points: 2.0f32 },
    RadiusStep { name: "medium", points: 4.0f32 },
    RadiusStep { name: "large", points: 6.0f32 },
    RadiusStep { name: "xLarge", points: 8.0f32 },
    RadiusStep { name: "2XLarge", points: 12.0f32 },
    RadiusStep { name: "3XLarge", points: 16.0f32 },
    RadiusStep { name: "4XLarge", points: 24.0f32 },
    RadiusStep { name: "5XLarge", points: 32.0f32 },
    RadiusStep { name: "6XLarge", points: 40.0f32 },
    RadiusStep { name: "circular", points: -1.0f32 },
];
pub static FLUENT_RADIUS_SCALE: RadiusScale = RadiusScale {
    steps: FLUENT_RADIUS,
    curvature: Round,
    supports_cut: false,
    source: Source::Spec("@fluentui/tokens borderRadius"),
};

// Fluent spacing has no single base; store literal points with base 1.
pub static FLUENT_SPACING: &[SpacingStep] = &[
    SpacingStep { name: "none", multiple: 0.0f32 },
    SpacingStep { name: "xxs", multiple: 2.0f32 },
    SpacingStep { name: "xs", multiple: 4.0f32 },
    SpacingStep { name: "sNudge", multiple: 6.0f32 },
    SpacingStep { name: "s", multiple: 8.0f32 },
    SpacingStep { name: "mNudge", multiple: 10.0f32 },
    SpacingStep { name: "m", multiple: 12.0f32 },
    SpacingStep { name: "l", multiple: 16.0f32 },
    SpacingStep { name: "xl", multiple: 20.0f32 },
    SpacingStep { name: "xxl", multiple: 24.0f32 },
    SpacingStep { name: "xxxl", multiple: 32.0f32 },
];
pub static FLUENT_SPACING_SCALE: SpacingScale = SpacingScale {
    base_points: 1.0f32,
    steps: FLUENT_SPACING,
    nested: &[],
    source: Source::Spec("@fluentui/tokens spacings"),
};

// Fluent type: Base400 (16) body, Base500 (20) subtitle, Base600 (24) title, Base300 (14) caption.
pub static FLUENT_TYPE: &[TypeStep] = &[
    TypeStep { name: "title", size: 24f32, line_height: 32f32, weight: 600 },
    TypeStep { name: "subtitle", size: 20f32, line_height: 28f32, weight: 600 },
    TypeStep { name: "body", size: 16f32, line_height: 22f32, weight: 400 },
    TypeStep { name: "caption", size: 14f32, line_height: 20f32, weight: 400 },
    TypeStep { name: "hint", size: 12f32, line_height: 16f32, weight: 400 },
];
pub static FLUENT_TYPE_RAMP: TypeRamp = TypeRamp {
    steps: FLUENT_TYPE,
    scalable: false,
    source: Source::Spec("@fluentui/tokens fontSizeBase* + typographyStyles"),
};

pub static FLUENT_DURATIONS: &[DurationStep] = &[
    DurationStep { name: "ultrafast", ms: 50.0f32 },
    DurationStep { name: "faster", ms: 100.0f32 },
    DurationStep { name: "fast", ms: 150.0f32 },
    DurationStep { name: "normal", ms: 200.0f32 },
    DurationStep { name: "gentle", ms: 250.0f32 },
    DurationStep { name: "slow", ms: 300.0f32 },
    DurationStep { name: "slower", ms: 400.0f32 },
    DurationStep { name: "ultraslow", ms: 500.0f32 },
];
pub static FLUENT_CURVES: &[(&str, [f32; 4])] = &[
    ("acceleratemax", [0.9f32, 0.1f32, 1.0f32, 0.2f32]),
    ("acceleratemid", [1.0f32, 0.0f32, 1.0f32, 1.0f32]),
    ("acceleratemin", [0.8f32, 0.0f32, 0.78f32, 1.0f32]),
    ("deceleratemax", [0.1f32, 0.9f32, 0.2f32, 1.0f32]),
    ("deceleratemid", [0.0f32, 0.0f32, 0.0f32, 1.0f32]),
    ("deceleratemin", [0.33f32, 0.0f32, 0.1f32, 1.0f32]),
    ("easyeasemax", [0.8f32, 0.0f32, 0.2f32, 1.0f32]),
    ("easyease", [0.33f32, 0.0f32, 0.67f32, 1.0f32]),
    ("linear", [0.0f32, 0.0f32, 1.0f32, 1.0f32]),
];
pub static FLUENT_MOTION: MotionScale = MotionScale {
    durations: FLUENT_DURATIONS,
    curves: FLUENT_CURVES,
    source: Source::Spec("@fluentui/tokens durations + curves"),
};

// Fluent hit target: 32dp comfortable, 24dp floor (Fluent 2 control sizing).
pub static FLUENT_HIT: HitTarget = HitTarget {
    default_points: 32f32,
    minimum_points: 24f32,
    source: Source::Spec("@fluentui/tokens control sizing"),
};

// HIG radii: Apple publishes no scale; these are our convention (continuous only).
pub static HIG_RADIUS: &[RadiusStep] = &[
    RadiusStep { name: "small", points: 8.0f32 },
    RadiusStep { name: "medium", points: 12.0f32 },
    RadiusStep { name: "card", points: 16.0f32 },
    RadiusStep { name: "large", points: 20.0f32 },
    RadiusStep { name: "capsule", points: -1.0f32 },
];
pub static HIG_RADIUS_SCALE: RadiusScale = RadiusScale {
    steps: HIG_RADIUS,
    curvature: Continuous,
    supports_cut: false,
    source: Source::Convention("our 8pt convention; Apple publishes no radius scale"),
};

// HIG spacing: 8pt grid; Apple publishes no numeric scale, so these are convention.
pub static HIG_SPACING: &[SpacingStep] = &[
    SpacingStep { name: "xxs", multiple: 0.5f32 },
    SpacingStep { name: "xs", multiple: 1.0f32 },
    SpacingStep { name: "s", multiple: 1.5f32 },
    SpacingStep { name: "m", multiple: 2.0f32 },
    SpacingStep { name: "l", multiple: 2.5f32 },
    SpacingStep { name: "xl", multiple: 3.0f32 },
    SpacingStep { name: "xxl", multiple: 4.0f32 },
    SpacingStep { name: "xxxl", multiple: 5.0f32 },
];
pub static HIG_SPACING_SCALE: SpacingScale = SpacingScale {
    base_points: 8f32,
    steps: HIG_SPACING,
    nested: &[4f32],
    source: Source::Convention("8pt grid; Apple publishes no scale"),
};

// HIG type: the iOS ramp (default Dynamic Type size); scalable, unlike the other two.
pub static HIG_TYPE: &[TypeStep] = &[
    TypeStep { name: "title", size: 34f32, line_height: 41f32, weight: 400 },
    TypeStep { name: "subtitle", size: 22f32, line_height: 28f32, weight: 400 },
    TypeStep { name: "body", size: 17f32, line_height: 22f32, weight: 400 },
    TypeStep { name: "caption", size: 13f32, line_height: 18f32, weight: 400 },
    TypeStep { name: "hint", size: 12f32, line_height: 16f32, weight: 400 },
];
pub static HIG_TYPE_RAMP: TypeRamp = TypeRamp {
    steps: HIG_TYPE,
    scalable: true,
    source: Source::Spec("Apple HIG text styles (default Dynamic Type size)"),
};

// HIG motion: the HIG publishes no numbers, so these are our convention.
pub static HIG_DURATIONS: &[DurationStep] = &[
    DurationStep { name: "quick", ms: 200.0f32 },
    DurationStep { name: "standard", ms: 300.0f32 },
    DurationStep { name: "slow", ms: 500.0f32 },
];
pub static HIG_CURVES: &[(&str, [f32; 4])] = &[
    ("standard", [0.25f32, 0.1f32, 0.25f32, 1.0f32]),
    ("decelerate", [0.0f32, 0.0f32, 0.2f32, 1.0f32]),
    ("accelerate", [0.4f32, 0.0f32, 1.0f32, 1.0f32]),
];
pub static HIG_MOTION: MotionScale = MotionScale {
    durations: HIG_DURATIONS,
    curves: HIG_CURVES,
    source: Source::Convention("HIG publishes no motion numbers"),
};

// HIG hit target: the one dimension Apple does publish (iOS 44 default, 28 hard floor).
pub static HIG_HIT: HitTarget = HitTarget {
    default_points: 44f32,
    minimum_points: 28f32,
    source: Source::Spec("Apple HIG hit targets (iOS)"),
};
