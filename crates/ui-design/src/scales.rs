//! The three styles' dimension tokens, as data plus the accessors that reach them.

use crate::capabilities::{Geometry, Motion, Typography, WithDimensions};
use crate::caps::StyleCaps;
use crate::dimension::DimensionTokens;
use crate::system::SystemId;

mod data {
    include!("../data/dimensions_data.rs");
}

// Each system answers the dimension traits from its own tables; the bodies are the trait
// defaults, so only the source is named here.
impl Geometry for crate::md3::Md3Scheme {}
impl Typography for crate::md3::Md3Scheme {}
impl Motion for crate::md3::Md3Scheme {}
impl Geometry for crate::fluent::FluentSystem {}
impl Typography for crate::fluent::FluentSystem {}
impl Motion for crate::fluent::FluentSystem {}
impl Geometry for crate::hig::HigSystem {}
impl Typography for crate::hig::HigSystem {}
impl Motion for crate::hig::HigSystem {}

impl WithDimensions for crate::md3::Md3Scheme {
    /// MD3's shape, spacing, type and motion scales.
    fn dimensions(&self) -> DimensionTokens {
        DimensionTokens {
            radius: data::MD3_RADIUS_SCALE,
            spacing: data::MD3_SPACING_SCALE,
            typography: data::MD3_TYPE_RAMP,
            motion: data::MD3_MOTION,
            hit_target: data::MD3_HIT,
        }
    }
}

impl WithDimensions for crate::fluent::FluentSystem {
    /// Fluent's dimension tokens.
    fn dimensions(&self) -> DimensionTokens {
        DimensionTokens {
            radius: data::FLUENT_RADIUS_SCALE,
            spacing: data::FLUENT_SPACING_SCALE,
            typography: data::FLUENT_TYPE_RAMP,
            motion: data::FLUENT_MOTION,
            hit_target: data::FLUENT_HIT,
        }
    }
}

impl WithDimensions for crate::hig::HigSystem {
    /// HIG's dimension tokens.
    fn dimensions(&self) -> DimensionTokens {
        DimensionTokens {
            radius: data::HIG_RADIUS_SCALE,
            spacing: data::HIG_SPACING_SCALE,
            typography: data::HIG_TYPE_RAMP,
            motion: data::HIG_MOTION,
            hit_target: data::HIG_HIT,
        }
    }
}

/// Reach a style's dimension tokens from the style's id and context.
pub fn dimensions_of(id: SystemId, ctx: crate::context::Context) -> DimensionTokens {
    match id {
        // Dimension tokens are context-free today, so any seed produces the same scales.
        SystemId::Md3 => crate::md3::Md3Scheme::baseline(ctx).dimensions(),
        SystemId::Fluent => crate::fluent::FluentSystem::new(ctx).dimensions(),
        SystemId::Hig => crate::hig::HigSystem::new(ctx).dimensions(),
    }
}

/// What a style can do with dimensions, so a shared widget can degrade well.
pub fn dimension_caps(system: &dyn crate::system::StyleSystem) -> StyleCaps {
    system.caps()
}
