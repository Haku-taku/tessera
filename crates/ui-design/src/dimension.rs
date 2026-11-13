//! Dimension tokens: corner radii, spacing, typography, motion, hit targets.
//!
//! Every scale carries its provenance, because none of the three systems publish the same
//! set of numbers: MD3 publishes shape and type, HIG publishes hit targets, Fluent publishes
//! everything, and all three leave some gaps we fill by convention.

use crate::source::Source;

/// How a corner is drawn. MD3 exposes both; HIG uses continuous only; Fluent rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Curvature {
    /// Circular arc; the CSS `border-radius` behaviour.
    Round,
    /// Continuous curvature (squircle); Apple's shape.
    Continuous,
    /// A straight chamfer, not a curve; MD3's `cut` family.
    Cut,
}

impl Curvature {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Round => "round",
            Self::Continuous => "continuous",
            Self::Cut => "cut",
        }
    }

    /// The engine shape this curvature maps onto. The engine expresses every corner on one
    /// superellipse axis, so the three names are presets, not kinds.
    pub const fn corner_shape(self) -> crate::corner::CornerShape {
        use crate::corner::CornerShape;
        match self {
            Self::Round => CornerShape::ROUND,
            Self::Continuous => CornerShape::SQUIRCLE,
            Self::Cut => CornerShape::BEVEL,
        }
    }
}

/// One named corner radius on a style's own scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiusStep {
    pub name: &'static str,
    /// Points; `-1.0` is the style's sentinel for "fully rounded" (a capsule).
    pub points: f32,
}

/// A style's full radius scale, plus the curvature family it uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiusScale {
    pub steps: &'static [RadiusStep],
    pub curvature: Curvature,
    /// Whether this style can switch to straight-cut corners (MD3 only).
    pub supports_cut: bool,
    pub source: Source,
}

impl RadiusScale {
    pub fn step(&self, name: &str) -> Option<RadiusStep> {
        self.steps.iter().find(|s| s.name == name).copied()
    }

    pub fn points(&self, name: &str) -> Option<f32> {
        self.step(name).map(|s| s.points)
    }
}

/// One spacing token, expressed as a multiple of the style's base grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpacingStep {
    pub name: &'static str,
    pub multiple: f32,
}

/// A style's spacing grid: one base, plus named multiples of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpacingScale {
    pub base_points: f32,
    pub steps: &'static [SpacingStep],
    /// Off-grid values the style also uses (MD3 spells out 2/4/6/10dp).
    pub nested: &'static [f32],
    pub source: Source,
}

impl SpacingScale {
    pub fn points(&self, name: &str) -> Option<f32> {
        self.steps
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.multiple * self.base_points)
    }
}

/// One named text style: size, line height and weight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStep {
    pub name: &'static str,
    pub size: f32,
    pub line_height: f32,
    pub weight: u16,
}

/// A style's typography ramp. `scalable` marks systems where sizes are a starting point that
/// the OS magnifies (HIG's Dynamic Type), not fixed points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeRamp {
    pub steps: &'static [TypeStep],
    pub scalable: bool,
    pub source: Source,
}

impl TypeRamp {
    pub fn step(&self, name: &str) -> Option<TypeStep> {
        self.steps.iter().find(|s| s.name == name).copied()
    }
}

/// One motion duration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DurationStep {
    pub name: &'static str,
    pub ms: f32,
}

/// A style's motion vocabulary. `curves` are CSS cubic-bezier control pairs.
///
/// Named lookups live on the [`crate::capabilities::Motion`] capability, beside every other
/// per-style answer, so the same question has one place to be asked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionScale {
    pub durations: &'static [DurationStep],
    pub curves: &'static [(&'static str, [f32; 4])],
    pub source: Source,
}

/// Hit-target minimums: `default` is the comfortable size, `minimum` the hard floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitTarget {
    pub default_points: f32,
    pub minimum_points: f32,
    pub source: Source,
}

/// Everything a style says about dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimensionTokens {
    pub radius: RadiusScale,
    pub spacing: SpacingScale,
    pub typography: TypeRamp,
    pub motion: MotionScale,
    pub hit_target: HitTarget,
}
