//! ui-design: one semantic layer over three complete native design systems.
//!
//! Two planes, not a lowest common denominator: `CommonColor` is one path across systems,
//! while every native token stays reachable through `Palette::colour`.

pub mod capabilities;
pub mod caps;
pub mod common;
pub mod context;
pub mod corner;
pub mod dimension;
pub mod fluent;
pub mod fluent_brand;
pub mod fluent_brand_v9;
pub mod fluent_hue_map;
pub mod hig;
pub mod md3;
pub mod metric;
pub mod scales;
pub mod source;
pub mod system;
pub mod value;
pub mod winui;

pub use capabilities::{
    Geometry, Motion, NativePalette, NoPalette, Palette, Typography, WithDimensions,
};
pub use caps::StyleCaps;
pub use common::CommonColor;
pub use context::{Appearance, Context, Contrast, Depth, Platform, WritingMode};
pub use corner::CornerShape;
pub use dimension::{
    Curvature, DimensionTokens, DurationStep, HitTarget, MotionScale, RadiusScale, RadiusStep,
    SpacingScale, SpacingStep, TypeRamp, TypeStep,
};
pub use fluent::FluentSystem;
pub use fluent_brand::{KEY_TONE, TONES, brand_ramp_from_seed};
pub use fluent_brand_v9::{
    LogDistribution, brand_ramp_from_seed as brand_ramp_v9_from_seed,
    brand_ramp_from_seed_with as brand_ramp_v9_with,
};
pub use hig::HigSystem;
pub use md3::{Md3Scheme, Role};
pub use metric::CommonMetric;
pub use scales::{dimension_caps, dimensions_of};
pub use source::Source;
pub use system::{MaterialSpec, NativeName, StyleSystem, SystemId, Theme};
pub use ui_color_hct::tone_of;
pub use value::{Reason, Resolution, Resolved, Rgba};
pub use winui::{Accent, GradientBrush, Paint, WinUISystem};
