//! The trait a design system implements, plus the registry that holds one instance per system.

use crate::caps::StyleCaps;
use crate::common::CommonColor;
use crate::context::Context;
use crate::metric::CommonMetric;
use crate::value::{Resolution, Resolved, Rgba};

/// Which design system a value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemId {
    Md3,
    Fluent,
    Hig,
}

impl SystemId {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Md3 => "MD3",
            Self::Fluent => "Fluent 2",
            Self::Hig => "HIG",
        }
    }
}

/// A named token in a system's own vocabulary, e.g. `("md3", "primaryContainer")`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeName {
    pub system: SystemId,
    pub name: &'static str,
}

impl NativeName {
    pub const fn new(system: SystemId, name: &'static str) -> Self {
        Self { system, name }
    }
}

/// One standard content material: what a translucent surface blurs and washes with.
///
/// A system that draws its own materials answers with this; the engine's job is only to
/// blur the backdrop and lay `tint` over it. `fallback` is the opaque stand-in a surface
/// draws when the user asked for reduced transparency or the backend cannot sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialSpec {
    /// Gaussian sigma of the backdrop blur, in points.
    pub sigma: f32,
    /// The neutral wash over the blurred backdrop.
    pub tint: Rgba,
    /// What to draw with nothing sampled behind it.
    pub fallback: Rgba,
}

/// A complete design system: identity, context, capabilities, plus the four responsibilities.
pub trait StyleSystem:
    crate::capabilities::Palette
    + crate::capabilities::WithDimensions
    + crate::capabilities::Geometry
    + crate::capabilities::Typography
    + crate::capabilities::Motion
    + Send
    + Sync
{
    /// Which system this is.
    fn id(&self) -> SystemId;

    /// The context this instance was built for.
    fn context(&self) -> Context;

    /// The shared-colour query, answered by this system's own rules. It lives here, not on
    /// `Palette`: a widget asks for a native token id; translating a shared role is the
    /// semantic variant's job.
    fn common_colour(&self, role: CommonColor) -> Resolution<Resolved<Rgba>>;

    /// The shared-metric query; same contract as `common_colour`, in points.
    fn common_metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>>;

    /// What this system can do beyond the common layer.
    fn caps(&self) -> StyleCaps;

    /// A standard content material by name, resolved for this system's context.
    ///
    /// Systems without a material family answer `Unmapped`; the caller keeps an opaque
    /// fill rather than inventing a blur.
    fn material(&self, _name: &str) -> Resolution<Resolved<MaterialSpec>> {
        Resolution::Unmapped
    }
}

/// A system together with the context it was resolved for.
pub struct Theme {
    pub system: Box<dyn StyleSystem>,
}

impl Theme {
    pub fn new(system: impl StyleSystem + 'static) -> Self {
        Self {
            system: Box::new(system),
        }
    }

    /// The common colour path with the usual loud failure.
    pub fn common_colour(&self, role: CommonColor) -> Resolution<Resolved<Rgba>> {
        self.system.common_colour(role)
    }

    /// The common metric path.
    pub fn common_metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>> {
        self.system.common_metric(metric)
    }

    /// The style's own vocabulary, for anything the common list does not cover.
    pub fn native_colour(&self, name: &str) -> Resolution<Rgba> {
        self.system.native_colour(name)
    }
}
