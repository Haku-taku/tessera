//! A tonal palette: one hue and one chroma, any tone; plus the key-colour search.

use crate::cam::cam_from_argb;
use crate::solve::argb_from_hcl;

/// What the search asks for when probing how much chroma a tone can hold.
const CHROMA_PROBE: f64 = 200.0;

/// Colours that share a hue and a chroma and differ only in tone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TonalPalette {
    pub hue: f64,
    pub chroma: f64,
}

impl TonalPalette {
    pub const fn new(hue: f64, chroma: f64) -> Self {
        Self { hue, chroma }
    }

    /// The palette a colour belongs to: its hue and chroma, whatever tone it sits at.
    pub fn of(argb: u32) -> Self {
        let cam = cam_from_argb(argb);
        Self::new(cam.hue, cam.chroma)
    }

    /// The colour at this tone; chroma shrinks when sRGB has no room for it.
    pub fn tone(&self, tone: f64) -> u32 {
        argb_from_hcl(self.hue, self.chroma, tone)
    }

    /// The most chroma this hue can reach at this tone inside sRGB.
    pub fn max_chroma(&self, tone: f64) -> f64 {
        cam_from_argb(argb_from_hcl(self.hue, CHROMA_PROBE, tone)).chroma
    }
}

/// The representative colour of a palette: tone and chroma where chroma peaks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyColor {
    pub hue: f64,
    pub requested_chroma: f64,
}

impl KeyColor {
    pub const fn new(hue: f64, requested_chroma: f64) -> Self {
        Self {
            hue,
            requested_chroma,
        }
    }

    /// Coarse scan for the peak, then refine by halves until the step is under 0.01.
    pub fn pivot_tone(&self) -> f64 {
        let palette = TonalPalette::new(self.hue, self.requested_chroma);
        let mut best_tone = 50.0;
        let mut best = f64::MIN;
        for tone in (0..=100)
            .map(|t| (t as f64) * 2.0 - 100.0)
            .filter(|t| (0.0..=100.0).contains(t))
        {
            let c = palette.max_chroma(tone);
            if c > best {
                best = c;
                best_tone = tone;
            }
        }
        let (mut lo, mut hi) = ((best_tone - 2.0).max(0.0), (best_tone + 2.0).min(100.0));
        while hi - lo > 0.01 {
            let m = (lo + hi) * 0.5;
            if palette.max_chroma(m - 0.01) >= palette.max_chroma(m + 0.01) {
                hi = m;
            } else {
                lo = m;
            }
        }
        (lo + hi) * 0.5
    }

    /// The palette's most saturated usable colour, at the chroma the request allows.
    pub fn create(&self) -> u32 {
        let tone = self.pivot_tone();
        let achievable = TonalPalette::new(self.hue, CHROMA_PROBE).max_chroma(tone);
        TonalPalette::new(self.hue, self.requested_chroma.min(achievable)).tone(tone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chroma_beyond_the_gamut_is_reported_as_capped() {
        let palette = TonalPalette::new(300.0, 120.0);
        assert!(palette.max_chroma(95.0) < 120.0);
        assert!(palette.max_chroma(50.0) > palette.max_chroma(95.0));
    }
}
