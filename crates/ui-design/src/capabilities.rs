//! The four responsibilities a style answers, split apart so they can differ per handle.
//!
//! Colour, geometry, type and motion change for different reasons; see §22 of the design doc.

use crate::dimension::{
    DimensionTokens, HitTarget, MotionScale, RadiusScale, SpacingScale, TypeRamp, TypeStep,
};
use crate::source::Source;
use crate::value::{Reason, Resolution, Rgba};

/// One lookup path: a colour for a token id in the style's own vocabulary.
///
/// Deliberately no `common` here: resolving a shared role is the semantic *variant* of `Style`
/// doing work — translate via the style's adapter, else fall back — not something every palette
/// must re-implement. See §22.
pub trait Palette: Send + Sync {
    /// Resolve a token in this style's own vocabulary; never pruned, never silently guessed.
    fn native_colour(&self, id: &str) -> Resolution<Rgba>;

    /// The brush under `id` when it is more than one colour; `None` means `native_colour` rules.
    fn native_brush(&self, _id: &str) -> Option<crate::winui::Paint> {
        None
    }

    /// Where the value behind `id` comes from.
    fn provenance(&self, id: &str) -> Source;
}

/// The one source the dimension traits read from. Everything below is `Copy` over `&'static`
/// tables, so reading per call costs nothing and no cache can drift.
pub trait WithDimensions {
    fn dimensions(&self) -> DimensionTokens;
}

/// Shape and space. Defaults read `dimensions()`; override a single method only where this
/// style genuinely answers that question differently.
pub trait Geometry: WithDimensions {
    fn radius(&self) -> RadiusScale {
        self.dimensions().radius
    }

    fn spacing(&self) -> SpacingScale {
        self.dimensions().spacing
    }

    fn hit_target(&self) -> HitTarget {
        self.dimensions().hit_target
    }

    /// A named radius, e.g. `"largeIncreased"`.
    fn radius_points(&self, name: &str) -> Option<f32> {
        self.radius().points(name)
    }

    /// A named radius together with the curvature family, so a widget gets the shape too.
    fn corner(&self, name: &str) -> Option<crate::corner::CornerShape> {
        let scale = self.radius();
        scale.step(name).map(|_| scale.curvature.corner_shape())
    }
}

/// Type sizes only, deliberately **colour-free**: the spec ramps define size, weight and
/// leading, while ink is the component's role choice — colour here is that decision posing as a ramp.
pub trait Typography: WithDimensions {
    fn ramp(&self) -> TypeRamp {
        self.dimensions().typography
    }

    /// One named step of the ramp, e.g. `"body"`.
    fn step(&self, name: &str) -> Option<TypeStep> {
        self.ramp().step(name)
    }

    /// Whether the OS magnifies these sizes rather than fixing them (HIG's Dynamic Type).
    fn scalable(&self) -> bool {
        self.ramp().scalable
    }
}

/// Durations and easing curves. Provenance honesty lives on `MotionScale.source`.
pub trait Motion: WithDimensions {
    fn scale(&self) -> MotionScale {
        self.dimensions().motion
    }

    /// A named duration in milliseconds, e.g. `"medium2"`.
    fn duration_ms(&self, name: &str) -> Option<f32> {
        self.scale()
            .durations
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.ms)
    }

    /// A named easing curve.
    fn curve(&self, name: &str) -> Option<[f32; 4]> {
        self.scale()
            .curves
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, c)| *c)
    }
}

/// Answers nothing, refusing with `Absent`. Lets a `Style` be complete when a system has no
/// such concept without inventing a colour to fill the hole.
pub struct NoPalette;

impl Palette for NoPalette {
    fn native_colour(&self, _id: &str) -> Resolution<Rgba> {
        Resolution::Absent(Reason::NotAConcept)
    }

    fn provenance(&self, _id: &str) -> Source {
        Source::Convention("no palette answers; the caller must fall back")
    }
}

/// Names the style's own answer, so a `Style` says "this handle is the native palette" instead
/// of holding an anonymous `Arc<dyn Palette>`.
pub struct NativePalette<S: Palette>(pub S);

impl<S: Palette> Palette for NativePalette<S> {
    fn native_colour(&self, id: &str) -> Resolution<Rgba> {
        self.0.native_colour(id)
    }

    fn provenance(&self, id: &str) -> Source {
        self.0.provenance(id)
    }
}
