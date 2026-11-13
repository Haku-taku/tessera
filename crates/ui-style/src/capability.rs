//! What the renderer underneath can actually do, so a style resolves to a recipe it
//! can run instead of drawing a plausible lie.

/// Reported by the backend at build time; widgets read it through the style pack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capability {
    /// Pixels behind a widget can be sampled, so backdrop blur and frost are real.
    pub backdrop: bool,
    /// Reduced-transparency preference is set by the user or the environment.
    pub reduced_transparency: bool,
}

impl Capability {
    /// The floor for a backend that cannot sample behind anything at all. The CPU
    /// rasterizer is NOT here: it blurs its own frame, so in-scene backdrop works.
    pub const NO_BACKDROP: Self = Self {
        backdrop: false,
        reduced_transparency: false,
    };

    /// The GL path today: backdrop and offscreen passes exist, arbitrary shaders do not.
    pub const GPU_BASELINE: Self = Self {
        backdrop: true,
        reduced_transparency: false,
    };

    /// Whether a frosted surface may draw its blur here, or must use its fallback colour.
    pub fn supports_frost(self) -> bool {
        self.backdrop && !self.reduced_transparency
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_transparency_downgrades_frost_to_no_frost() {
        // Fluent proves the pattern: `DisableAcrylic` yields an opaque fallback, no filter,
        // no noise; the capability must flip the same question.
        assert!(Capability::GPU_BASELINE.supports_frost());
        let strict = Capability {
            reduced_transparency: true,
            ..Capability::GPU_BASELINE
        };
        assert!(!strict.supports_frost());
        assert!(!Capability::NO_BACKDROP.supports_frost());
    }
}
