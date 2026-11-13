//! Subpixel positioning: quantize glyph origins to a small set of phases.

/// Horizontal subpixel phase of a glyph origin, in quarters of a pixel.
///
/// Four phases is the consensus: egui/epaint uses four bins, GPUI sets
/// `SUBPIXEL_VARIANTS_X = 4`. Vertical phases are [`VerticalBin`]'s business.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum SubpixelBin {
    #[default]
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
}

/// Number of horizontal phases; also the atlas multiplier per glyph.
pub const SUBPIXEL_PHASES: u8 = 4;

impl SubpixelBin {
    /// Split `pos` into a whole-pixel origin and a phase.
    ///
    /// Rounds to the *nearest* quarter pixel, which is what keeps a run's
    /// accumulated fractional advances from snapping a whole pixel at once.
    pub fn split(pos: f32) -> (i32, Self) {
        if !pos.is_finite() {
            return (0, Self::Zero);
        }
        // Work in quarter-pixel units so rounding and splitting share one integer.
        let quarters = (pos * SUBPIXEL_PHASES as f32).round() as i32;
        let whole = quarters.div_euclid(SUBPIXEL_PHASES as i32);
        let phase = quarters.rem_euclid(SUBPIXEL_PHASES as i32) as u8;
        (whole, Self::from_index(phase))
    }

    pub fn from_index(i: u8) -> Self {
        match i & 3 {
            0 => Self::Zero,
            1 => Self::One,
            2 => Self::Two,
            _ => Self::Three,
        }
    }

    pub fn index(self) -> u8 {
        self as u8
    }

    /// Phase as a fraction of a pixel: 0.0, 0.25, 0.5 or 0.75.
    pub fn as_float(self) -> f32 {
        self as u8 as f32 / SUBPIXEL_PHASES as f32
    }

    /// Reconstruct the quantized position from a split origin.
    pub fn recombine(whole: i32, bin: Self) -> f32 {
        whole as f32 + bin.as_float()
    }
}

/// Vertical subpixel phase of a glyph origin, in halves of a pixel.
///
/// Two phases rather than four, and a quarter pixel left over that the renderer samples
/// through: see [`VerticalBin::split`]. A bitmap per phase, so vertical variants multiply
/// the atlas exactly as the horizontal ones do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum VerticalBin {
    #[default]
    Zero = 0,
    Half = 1,
}

/// Number of vertical phases; also the atlas multiplier per glyph.
pub const VERTICAL_PHASES: u8 = 2;

impl VerticalBin {
    /// Split `pos` into a whole pixel, a phase, and the residual a sampler carries.
    ///
    /// The origin takes the nearest half pixel, and what is left is at most a quarter of one.
    /// A quarter pixel of bilinear shift is invisible, while re-rasterizing for every quarter
    /// would quadruple the atlas to buy what the sampler already does.
    pub fn split(pos: f32) -> (i32, Self, f32) {
        if !pos.is_finite() {
            return (0, Self::Zero, 0.0);
        }
        let halves = (pos * VERTICAL_PHASES as f32).round();
        let whole = (halves / VERTICAL_PHASES as f32).floor();
        let phase = (halves - whole * VERTICAL_PHASES as f32) as u8;
        let bin = if phase == 0 { Self::Zero } else { Self::Half };
        (whole as i32, bin, pos - (whole + bin.as_float()))
    }

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn from_index(i: u8) -> Self {
        if i.is_multiple_of(VERTICAL_PHASES) {
            Self::Zero
        } else {
            Self::Half
        }
    }

    /// Phase as a fraction of a pixel: 0.0 or 0.5.
    pub fn as_float(self) -> f32 {
        self as u8 as f32 / VERTICAL_PHASES as f32
    }
}

/// The phase pair a glyph is rasterized and cached at: one bin per axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Phases {
    pub x: SubpixelBin,
    pub y: VerticalBin,
}

impl Phases {
    pub const ZERO: Self = Self {
        x: SubpixelBin::Zero,
        y: VerticalBin::Zero,
    };

    pub fn new(x: SubpixelBin, y: VerticalBin) -> Self {
        Self { x, y }
    }
}

/// Whether a run should use subpixel phases at all.
///
/// CJK is excluded: thousands of characters times four phases would dominate the
/// atlas, and Han glyphs are near-square so horizontal jitter is less visible.
pub fn should_subpixel(script: crate::Script) -> bool {
    !matches!(
        script,
        crate::Script::HAN | crate::Script::HIRAGANA | crate::Script::HANGUL
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_rounds_to_the_nearest_quarter() {
        assert_eq!(SubpixelBin::split(0.0), (0, SubpixelBin::Zero));
        assert_eq!(SubpixelBin::split(0.1), (0, SubpixelBin::Zero));
        assert_eq!(SubpixelBin::split(0.13), (0, SubpixelBin::One));
        assert_eq!(SubpixelBin::split(0.4), (0, SubpixelBin::Two));
        assert_eq!(SubpixelBin::split(0.7), (0, SubpixelBin::Three));
        // 0.9 rounds up into the next whole pixel, back to phase zero.
        assert_eq!(SubpixelBin::split(0.9), (1, SubpixelBin::Zero));
    }

    #[test]
    fn vertical_split_lands_on_the_nearest_half_and_leaves_a_quarter_at_most() {
        let cases = [
            (20.0, 20, VerticalBin::Zero, 0.0),
            (20.1, 20, VerticalBin::Zero, 0.1),
            (20.3, 20, VerticalBin::Half, -0.2),
            (20.7, 20, VerticalBin::Half, 0.2),
            // Past three quarters it belongs to the next whole pixel, phase zero again.
            (20.9, 21, VerticalBin::Zero, -0.1),
        ];
        for (pos, whole, bin, residual) in cases {
            let (got_whole, got_bin, got_residual) = VerticalBin::split(pos);
            assert_eq!((got_whole, got_bin), (whole, bin), "at {pos}");
            assert!(
                (got_residual - residual).abs() < 1e-3,
                "at {pos}: {got_residual}"
            );
        }
        for step in 0..200 {
            let pos = step as f32 * 0.13;
            let (whole, bin, residual) = VerticalBin::split(pos);
            assert!(residual.abs() <= 0.25, "{pos} left {residual}");
            let placed = f64::from(whole) + f64::from(bin.as_float()) + f64::from(residual);
            assert!(
                (placed - f64::from(pos)).abs() < 1e-4,
                "{pos} landed {placed}"
            );
        }
    }

    #[test]
    fn accumulated_advances_do_not_jump_a_whole_pixel() {
        // A 10.37 px advance drifts by design; the quantized origin must never
        // move more than one quarter pixel beyond the ideal position.
        let advance = 10.37_f32;
        let mut pen = 0.0_f32;
        let mut previous: Option<i32> = None;
        for _ in 0..40 {
            let (whole, _bin) = SubpixelBin::split(pen);
            if let Some(prev) = previous {
                let jump = whole - prev;
                assert!(
                    jump == 10 || jump == 11,
                    "pen {pen} jumped {jump} px, expected 10 or 11"
                );
            }
            previous = Some(whole);
            pen += advance;
        }
    }
}
