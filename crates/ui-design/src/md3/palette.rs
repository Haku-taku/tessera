//! The five key colours of the default (TonalSpot) scheme and the palettes derived from them.

use ui_color_hct::{Cam, TonalPalette, cam_from_argb};

/// Which tonal palette a role reads from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaletteId {
    Primary,
    Secondary,
    Tertiary,
    Neutral,
    NeutralVariant,
    Error,
}

impl PaletteId {
    pub const ALL: [Self; 6] = [
        Self::Primary,
        Self::Secondary,
        Self::Tertiary,
        Self::Neutral,
        Self::NeutralVariant,
        Self::Error,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Tertiary => "tertiary",
            Self::Neutral => "neutral",
            Self::NeutralVariant => "neutral_variant",
            Self::Error => "error",
        }
    }
}

/// TonalSpot's key-colour rule: source hue, per-palette chroma, tertiary hue + 60.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TonalSpot {
    pub source_hue: f64,
    pub source_chroma: f64,
    pub source_tone: f64,
}

impl TonalSpot {
    /// The palettes for a source colour.
    pub fn palettes(&self) -> Palettes {
        let h = self.source_hue;
        Palettes {
            primary: TonalPalette::new(h, 36.0),
            secondary: TonalPalette::new(h, 16.0),
            tertiary: TonalPalette::new((h + 60.0) % 360.0, 24.0),
            neutral: TonalPalette::new(h, 6.0),
            neutral_variant: TonalPalette::new(h, 8.0),
            // The error palette ignores the source: hue 25, chroma 84.
            error: TonalPalette::new(25.0, 84.0),
        }
    }
}

/// One tonal palette per key colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palettes {
    pub primary: TonalPalette,
    pub secondary: TonalPalette,
    pub tertiary: TonalPalette,
    pub neutral: TonalPalette,
    pub neutral_variant: TonalPalette,
    pub error: TonalPalette,
}

impl Palettes {
    pub fn get(&self, id: PaletteId) -> TonalPalette {
        match id {
            PaletteId::Primary => self.primary,
            PaletteId::Secondary => self.secondary,
            PaletteId::Tertiary => self.tertiary,
            PaletteId::Neutral => self.neutral,
            PaletteId::NeutralVariant => self.neutral_variant,
            PaletteId::Error => self.error,
        }
    }

    /// The colour at `tone` in the named palette.
    pub fn tone(&self, id: PaletteId, tone: f64) -> u32 {
        self.get(id).tone(tone)
    }
}

/// Everything a scheme needs from its source colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceColor {
    pub hue: f64,
    pub chroma: f64,
    pub tone: f64,
}

impl SourceColor {
    /// Read hue, chroma and tone straight off an sRGB colour.
    pub fn from_argb(argb: u32) -> Self {
        let Cam { hue, chroma, .. } = cam_from_argb(argb);
        Self {
            hue,
            chroma,
            tone: ui_color_hct::tone_of(argb),
        }
    }

    pub fn tonal_spot(&self) -> TonalSpot {
        TonalSpot {
            source_hue: self.hue,
            source_chroma: self.chroma,
            source_tone: self.tone,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TonalSpot's numbers are the spec table; the derived hue/chroma must match it exactly.
    #[test]
    fn tonal_spot_uses_the_documented_hue_and_chroma_per_palette() {
        let palettes = TonalSpot {
            source_hue: 40.0,
            source_chroma: 50.0,
            source_tone: 40.0,
        }
        .palettes();
        assert_eq!(palettes.primary.chroma, 36.0);
        assert_eq!(palettes.secondary.chroma, 16.0);
        assert_eq!(palettes.tertiary.chroma, 24.0);
        assert_eq!(palettes.neutral.chroma, 6.0);
        assert_eq!(palettes.neutral_variant.chroma, 8.0);
        assert!(
            (palettes.tertiary.hue - 100.0).abs() < 1e-9,
            "tertiary hue {}",
            palettes.tertiary.hue
        );
        assert!((palettes.primary.hue - 40.0).abs() < 1e-9);
    }
}
