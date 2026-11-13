//! What a system can do beyond the common layer, so shared widgets can enhance progressively.

/// Optional capabilities. A shared widget checks these before reaching for `native()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct StyleCaps {
    /// Has a tonal palette for every key colour (MD3).
    pub tonal_palettes: bool,
    /// Can produce any tone of any palette (MD3).
    pub arbitrary_tones: bool,
    /// Has first-class container roles (MD3 `primaryContainer`).
    pub containers: bool,
    /// Has named state tokens instead of computing overlays (Fluent).
    pub named_state_tokens: bool,
    /// Has a full shared/persona palette family (Fluent `colorPalette*`).
    pub shared_palettes: bool,
    /// Has layered background depth (HIG base/elevated).
    pub elevated_depth: bool,
    /// Has content materials (blur/vibrancy).
    pub materials: bool,
    /// Has a Liquid Glass style functional layer.
    pub glass: bool,
    /// Has a discrete corner-radius scale.
    pub radius_scale: bool,
    /// Corners may be cut (straight) rather than rounded (MD3 `cut`).
    pub cut_corners: bool,
    /// Has a published motion token set; otherwise motion is our convention.
    pub motion_tokens: bool,
}

impl StyleCaps {
    /// Nothing but the common layer.
    pub const MINIMAL: Self = Self {
        tonal_palettes: false,
        arbitrary_tones: false,
        containers: false,
        named_state_tokens: false,
        shared_palettes: false,
        elevated_depth: false,
        materials: false,
        glass: false,
        radius_scale: false,
        cut_corners: false,
        motion_tokens: false,
    };
}
