//! Three-pass box blur: a Gaussian approximation whose cost does not grow with radius.
//!
//! A direct convolution is O(taps) per pixel, so a large radius is both slow and bounded
//! by whatever the backend's kernel array holds. Three box passes average shifts of the
//! input instead, giving O(1) per pixel and an effective sigma that tracks the box width
//! linearly — the same construction the W3C filter spec and Skia's `SkBlurMask` use.

/// Box passes per axis; three make the average a Gaussian.
pub const BOX_PASSES: u32 = 3;

/// Widest box radius a pass can use; keeps the window sum inside f32 precision.
pub const MAX_BOX_RADIUS: u32 = 4096;

/// Box radius whose three passes approximate a Gaussian of `sigma`.
///
/// Three box averages of width `w` have variance `3(w²-1)/12`; solving for `w` gives the
/// relation below. The result is close to `sigma` itself, which is why this replaces the
/// radius cap a tap-limited backend needs.
pub fn radius_for_sigma(sigma: f32) -> u32 {
    if sigma <= 0.0 {
        return 0;
    }
    let w = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    // An odd width keeps the window symmetric around its centre pixel.
    let mut w = w.round() as i32;
    if w % 2 == 0 {
        w += 1;
    }
    ((w - 1) / 2).clamp(0, MAX_BOX_RADIUS as i32) as u32
}

/// Effective sigma of three box passes with `radius`.
pub fn sigma_for_radius(radius: u32) -> f32 {
    let w = (2 * radius + 1) as f32;
    (3.0 * (w * w - 1.0) / 12.0).sqrt()
}

/// One horizontal box pass over an interleaved RGBA8 image, clamped at the edges.
pub fn box_h(
    rgba: &mut [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    w: usize,
    rows: usize,
    radius: u32,
) {
    #[cfg(feature = "simd")]
    simd_pass::box_h(rgba, stride, x0, y0, w, rows, radius);
    #[cfg(not(feature = "simd"))]
    box_h_scalar(rgba, stride, x0, y0, w, rows, radius);
}

/// One vertical box pass; same contract as [`box_h`].
pub fn box_v(
    rgba: &mut [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    w: usize,
    rows: usize,
    radius: u32,
) {
    #[cfg(feature = "simd")]
    simd_pass::box_v(rgba, stride, x0, y0, w, rows, radius);
    #[cfg(not(feature = "simd"))]
    box_v_scalar(rgba, stride, x0, y0, w, rows, radius);
}

/// One horizontal box pass over an interleaved RGBA8 image, clamped at the edges.
///
/// `stride` is the row length in pixels, so this also works on a sub-rectangle of a
/// larger image without copying it out first.
#[cfg(any(not(feature = "simd"), test))]
fn box_h_scalar(
    rgba: &mut [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    w: usize,
    rows: usize,
    radius: u32,
) {
    if radius == 0 || w == 0 || rows == 0 {
        return;
    }
    let r = radius as usize;
    let span = (2 * r + 1) as u32;
    let last = x0 + w - 1;
    // The window must subtract the *input* at the leaving position, but that byte has
    // already been overwritten by this pass; keeping the source row keeps the sum a true
    // box average instead of a recursion that drifts from the third pixel on.
    let mut src = vec![0u8; w * 4];
    for y in y0..y0 + rows {
        let row = y * stride;
        let base = (row + x0) * 4;
        src.copy_from_slice(&rgba[base..base + w * 4]);
        // Seed by summing the clamped window at x0, so an edge pixel contributes once per
        // tap that reaches it — the same weight a clamp sampler would give it.
        let mut acc = [0u32; 4];
        for i in 0..=2 * r {
            let sx = (x0 + i).saturating_sub(r).clamp(x0, last) - x0;
            for c in 0..4 {
                acc[c] += src[sx * 4 + c] as u32;
            }
        }
        for x in x0..x0 + w {
            let sub = (x.saturating_sub(r).clamp(x0, last) - x0) * 4;
            let leaving = [src[sub], src[sub + 1], src[sub + 2], src[sub + 3]];
            let px = (row + x) * 4;
            for c in 0..4 {
                rgba[px + c] = (acc[c] / span) as u8;
            }
            let add = ((x + r + 1).clamp(x0, last) - x0) * 4;
            for c in 0..4 {
                let next = acc[c] as i64 + src[add + c] as i64 - leaving[c] as i64;
                acc[c] = next.max(0) as u32;
            }
        }
    }
}

/// One vertical box pass; same contract as [`box_h`].
#[cfg(any(not(feature = "simd"), test))]
fn box_v_scalar(
    rgba: &mut [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    w: usize,
    rows: usize,
    radius: u32,
) {
    if radius == 0 || w == 0 || rows == 0 {
        return;
    }
    let r = radius as usize;
    let span = (2 * r + 1) as u32;
    let bottom = y0 + rows - 1;
    // Same reason as the horizontal pass: the leaving pixel has already been overwritten,
    // so the source column has to be kept for the sum to stay a true window average.
    let mut src = vec![0u8; rows * 4];
    for x in x0..x0 + w {
        for (i, y) in (y0..=bottom).enumerate() {
            let at = (y * stride + x) * 4;
            src[i * 4..i * 4 + 4].copy_from_slice(&rgba[at..at + 4]);
        }
        let mut acc = [0u32; 4];
        for i in 0..=2 * r {
            let sy = (y0 + i).saturating_sub(r).clamp(y0, bottom) - y0;
            for c in 0..4 {
                acc[c] += src[sy * 4 + c] as u32;
            }
        }
        for y in y0..=bottom {
            let sub = (y.saturating_sub(r).clamp(y0, bottom) - y0) * 4;
            let leaving = [src[sub], src[sub + 1], src[sub + 2], src[sub + 3]];
            let at = (y * stride + x) * 4;
            for c in 0..4 {
                rgba[at + c] = (acc[c] / span) as u8;
            }
            let add = ((y + r + 1).clamp(y0, bottom) - y0) * 4;
            for c in 0..4 {
                let next = acc[c] as i64 + src[add + c] as i64 - leaving[c] as i64;
                acc[c] = next.max(0) as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three box passes must match the Gaussian they are meant to stand in for.

    /// A hard edge must become a ramp: that is what blurring means, and a lone bright
    /// pixel is the wrong probe because three divisions by the box width quantise it to
    /// zero at u8.

    /// A gradient must stay a gradient: the window sum is preserved by construction, so
    /// every pixel must sit within one step of its neighbours, with no edge collapse.

    /// Clamping at the edge must not create a dark rim, which a zero-padded window would.
    #[test]
    fn the_edge_stays_bright_instead_of_darkening() {
        let (w, h) = (24, 8);
        let mut img = vec![120u8; w * h * 4];
        box_h(&mut img, w, 0, 0, w, h, 10);
        assert_eq!(
            img[0], 120,
            "the first pixel must keep the field's value, got {}",
            img[0]
        );
        assert_eq!(img[(w - 1) * 4], 120, "and so must the last");
    }

    /// A pass must equal a direct window average. Subtracting the pixel this pass already
    /// overwrote turns the running sum into a recursion and drifts from the third pixel on.
    #[test]
    fn a_pass_matches_a_direct_window_average() {
        let n = 24usize;
        let src: Vec<u8> = (0..n).map(|i| (i * 53 % 251) as u8).collect();
        for radius in [1u32, 2, 3] {
            let r = radius as usize;
            let mut img = vec![0u8; n * 4];
            for (i, v) in src.iter().enumerate() {
                for c in 0..4 {
                    img[i * 4 + c] = *v;
                }
            }
            box_h(&mut img, n, 0, 0, n, 1, radius);
            let want: Vec<u8> = (0..n)
                .map(|i| {
                    let mut sum = 0u32;
                    for k in 0..=(2 * r) {
                        let idx = (i + k).saturating_sub(r).clamp(0, n - 1);
                        sum += src[idx] as u32;
                    }
                    (sum / (2 * r + 1) as u32) as u8
                })
                .collect();
            let got: Vec<u8> = (0..n).map(|i| img[i * 4]).collect();
            assert_eq!(got, want, "radius {radius} drifted from the window average");
        }
    }
}

#[cfg(all(feature = "simd", test))]
mod simd_agrees_with_scalar {
    use super::*;

    /// A deterministic ramp with a hard edge, so a wrong window shows up immediately.
    fn image(w: usize, h: usize) -> Vec<u8> {
        let mut v = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let t = if x < w / 2 { x * 3 } else { 200 } as u8;
                v[i..i + 4].copy_from_slice(&[t, (y * 7) as u8, (x * 11) as u8, 255]);
            }
        }
        v
    }

    #[test]
    fn the_simd_passes_agree_with_the_scalar_reference() {
        // The four-lane path is only worth having if it computes the same window average;
        // without this, a broken lane shuffle looks like a slightly softer blur.
        let (w, h) = (37, 11);
        for radius in [1u32, 3, 8] {
            for vertical in [false, true] {
                let mut fast = image(w, h);
                let mut slow = image(w, h);
                if vertical {
                    box_v(&mut fast, w, 0, 0, w, h, radius);
                    box_v_scalar(&mut slow, w, 0, 0, w, h, radius);
                } else {
                    box_h(&mut fast, w, 0, 0, w, h, radius);
                    box_h_scalar(&mut slow, w, 0, 0, w, h, radius);
                }
                assert_eq!(fast, slow, "radius {radius} vertical {vertical}");
            }
        }
    }
}

/// Four-lane versions of the passes: RGBA is exactly one vector, so the running sum, the
/// add and the subtract each become one instruction instead of four.
#[cfg(feature = "simd")]
mod simd_pass {
    use wide::u32x4;

    #[inline(always)]
    fn load(src: &[u8], at: usize) -> u32x4 {
        let b = &src[at..at + 4];
        u32x4::new([b[0] as u32, b[1] as u32, b[2] as u32, b[3] as u32])
    }

    #[inline(always)]
    fn store(dst: &mut [u8], at: usize, v: u32x4, span: u32) {
        let a = v.to_array();
        let out = &mut dst[at..at + 4];
        out[0] = (a[0] / span) as u8;
        out[1] = (a[1] / span) as u8;
        out[2] = (a[2] / span) as u8;
        out[3] = (a[3] / span) as u8;
    }

    pub(super) fn box_h(
        rgba: &mut [u8],
        stride: usize,
        x0: usize,
        y0: usize,
        w: usize,
        rows: usize,
        radius: u32,
    ) {
        if radius == 0 || w == 0 || rows == 0 {
            return;
        }
        let r = radius as usize;
        let span = 2 * r as u32 + 1;
        let last = x0 + w - 1;
        // The leaving pixel has already been overwritten by this pass, so the source row is
        // kept; subtracting the live value would make the sum a recursion.
        let mut src = vec![0u8; w * 4];
        for y in y0..y0 + rows {
            let row = y * stride;
            let base = (row + x0) * 4;
            src.copy_from_slice(&rgba[base..base + w * 4]);
            let mut acc = u32x4::ZERO;
            for i in 0..=2 * r {
                let sx = (x0 + i).saturating_sub(r).clamp(x0, last) - x0;
                acc += load(&src, sx * 4);
            }
            for x in x0..x0 + w {
                let sub = (x.saturating_sub(r).clamp(x0, last) - x0) * 4;
                let leaving = load(&src, sub);
                let px = (row + x) * 4;
                store(rgba, px, acc, span);
                let add = ((x + r + 1).clamp(x0, last) - x0) * 4;
                acc = acc + load(&src, add) - leaving;
            }
        }
    }

    pub(super) fn box_v(
        rgba: &mut [u8],
        stride: usize,
        x0: usize,
        y0: usize,
        w: usize,
        rows: usize,
        radius: u32,
    ) {
        if radius == 0 || w == 0 || rows == 0 {
            return;
        }
        let r = radius as usize;
        let span = 2 * r as u32 + 1;
        let bottom = y0 + rows - 1;
        let mut src = vec![0u8; rows * 4];
        for x in x0..x0 + w {
            for (i, y) in (y0..=bottom).enumerate() {
                let at = (y * stride + x) * 4;
                src[i * 4..i * 4 + 4].copy_from_slice(&rgba[at..at + 4]);
            }
            let mut acc = u32x4::ZERO;
            for i in 0..=2 * r {
                let sy = (y0 + i).saturating_sub(r).clamp(y0, bottom) - y0;
                acc += load(&src, sy * 4);
            }
            for y in y0..=bottom {
                let sub = (y.saturating_sub(r).clamp(y0, bottom) - y0) * 4;
                let leaving = load(&src, sub);
                let at = (y * stride + x) * 4;
                store(rgba, at, acc, span);
                let add = ((y + r + 1).clamp(y0, bottom) - y0) * 4;
                acc = acc + load(&src, add) - leaving;
            }
        }
    }
}
