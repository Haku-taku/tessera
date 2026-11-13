//! Effects as a category: a separable convolution plus a colour transform.
//!
//! The renderer never learns the name of an effect. It knows two operations — weighted
//! sampling of neighbours, and a 4x4 transform on premultiplied RGBA — and every effect is
//! expressed as those two. Adding a new one must not touch the renderer.

/// A one-dimensional normalised kernel; the 2D kernel is its outer product.
#[derive(Clone, Debug, PartialEq)]
pub struct Kernel {
    /// Samples on each side. `0` means the identity.
    pub radius: u32,
    /// Standard deviation the weights came from; `0` for a non-gaussian kernel.
    pub sigma: f32,
    /// `2 * radius + 1` weights summing to 1, or a single `1.0` for a windowed kernel.
    pub weights: Vec<f32>,
    /// Box passes per axis behind this kernel; `0` means an explicit weight row.
    pub boxes: u32,
}

impl Kernel {
    pub const IDENTITY: Kernel = Kernel {
        radius: 0,
        sigma: 0.0,
        weights: Vec::new(),
        boxes: 0,
    };

    /// Gaussian with `sigma`, truncated at three standard deviations.
    ///
    /// `radius` is the *actual* truncation the caller can afford; `guaranteed_reach` reports
    /// the analytic one, so a cheap blur is visibly cheaper rather than silently wrong.
    pub fn gaussian(radius: u32, sigma: f32) -> Self {
        if radius == 0 || sigma <= 0.0 {
            return Self::IDENTITY;
        }
        // A kernel wider than the shader's array loses its tail and stops summing to one,
        // which reads as "the image got darker" rather than "blurrier".
        let radius = radius.min(MAX_KERNEL_RADIUS);
        let r = radius as i32;
        let mut weights: Vec<f32> = (-r..=r)
            .map(|i| {
                let x = i as f32;
                (-(x * x) / (2.0 * sigma * sigma)).exp()
            })
            .collect();
        let sum: f32 = weights.iter().sum();
        if sum > 0.0 {
            for w in &mut weights {
                *w /= sum;
            }
        }
        Self {
            radius,
            sigma,
            weights,
            boxes: 0,
        }
    }

    /// How far the output can depend on an input pixel: the pass chain's own support.
    ///
    /// A truncated gaussian stops at its radius and a box chain reaches one radius per
    /// pass, so the band is a bound on what the kernel can read. `3σ` only says where a
    /// gaussian's tail is negligible; a kernel truncated wider than that reads past it.
    pub fn guaranteed_reach(&self) -> u32 {
        let analytic = (3.0 * self.sigma).ceil().max(0.0) as u32;
        self.band().max(analytic)
    }

    pub fn is_identity(&self) -> bool {
        self.radius == 0 || self.weights.is_empty()
    }

    /// How far this kernel samples outside its own pixel, in pixels.
    pub fn reach(&self) -> u32 {
        self.radius
    }

    /// Working band this kernel's passes need, which a windowed kernel widens per pass.
    pub fn band(&self) -> u32 {
        // Each extra box pass reads one more radius, so the band grows with the count.
        let passes = self.boxes.max(1);
        self.radius.saturating_mul(passes)
    }
}

/// Row-major 4x4 transform on premultiplied `[r, g, b, a]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorMatrix(pub [f32; 16]);

impl ColorMatrix {
    pub const IDENTITY: ColorMatrix = ColorMatrix([
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);

    /// Uniform fade. On premultiplied data every channel scales, alpha included.
    pub fn opacity(alpha: f32) -> Self {
        let a = alpha.clamp(0.0, 1.0);
        ColorMatrix([
            a, 0.0, 0.0, 0.0, //
            0.0, a, 0.0, 0.0, //
            0.0, 0.0, a, 0.0, //
            0.0, 0.0, 0.0, a,
        ])
    }

    /// Luma-preserving saturation, applied to the colour part only.
    pub fn saturate(s: f32) -> Self {
        const R: f32 = 0.2126;
        const G: f32 = 0.7152;
        const B: f32 = 0.0722;
        let (r, g, b) = (R * (1.0 - s), G * (1.0 - s), B * (1.0 - s));
        ColorMatrix([
            r + s,
            g,
            b,
            0.0, //
            r,
            g + s,
            b,
            0.0, //
            r,
            g,
            b + s,
            0.0, //
            0.0,
            0.0,
            0.0,
            1.0,
        ])
    }

    pub fn is_identity(&self) -> bool {
        self.0 == ColorMatrix::IDENTITY.0
    }

    /// True when this is nothing but a uniform alpha scale.
    pub fn as_opacity(&self) -> Option<f32> {
        let m = self.0;
        let a = m[0];
        let diagonal = [m[0], m[5], m[10], m[15]];
        if diagonal.iter().all(|d| *d == a) && (0.0..=1.0).contains(&a) {
            let off = [
                m[1], m[2], m[3], m[4], m[6], m[7], m[8], m[9], m[11], m[12], m[13], m[14],
            ];
            if off.iter().all(|v| *v == 0.0) {
                return Some(a);
            }
        }
        None
    }

    /// `self` applied after `inner`.
    pub fn multiply(&self, inner: &ColorMatrix) -> Self {
        let a = self.0;
        let b = inner.0;
        let mut out = [0.0f32; 16];
        for row in 0..4 {
            for col in 0..4 {
                out[row * 4 + col] = (0..4).map(|k| a[row * 4 + k] * b[k * 4 + col]).sum();
            }
        }
        ColorMatrix(out)
    }

    pub fn apply(&self, px: [f32; 4]) -> [f32; 4] {
        let m = &self.0;
        let mut out = [0.0f32; 4];
        for row in 0..4 {
            out[row] = (0..4).map(|k| m[row * 4 + k] * px[k]).sum();
        }
        out
    }
}

impl Default for ColorMatrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Widest radius every backend can run: the GLSL shader holds 33 taps, i.e. radius 16.
pub const MAX_KERNEL_RADIUS: u32 = 16;

/// One step of an effect chain.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub kernel: Kernel,
    pub color: ColorMatrix,
}

impl Effect {
    pub fn none() -> Self {
        Self {
            kernel: Kernel::IDENTITY,
            color: ColorMatrix::IDENTITY,
        }
    }

    /// Group opacity, expressed in the same algebra as everything else.
    pub fn opacity(alpha: f32) -> Self {
        Self {
            kernel: Kernel::IDENTITY,
            color: ColorMatrix::opacity(alpha),
        }
    }

    pub fn blur(radius: u32, sigma: f32) -> Self {
        Self {
            kernel: Kernel::gaussian(radius, sigma),
            color: ColorMatrix::IDENTITY,
        }
    }

    /// Gaussian approximated by three box passes, which costs the same at any radius.
    ///
    /// The box width lives in `radius`; `weights` stays a single tap so backends that only
    /// understand explicit kernels still see a well-formed (if unblurred) effect rather
    /// than an out-of-range array.
    pub fn box_blur(sigma: f32) -> Self {
        let radius = crate::boxblur::radius_for_sigma(sigma);
        Self {
            kernel: Kernel {
                radius,
                sigma: crate::boxblur::sigma_for_radius(radius),
                weights: vec![1.0],
                boxes: crate::boxblur::BOX_PASSES,
            },
            color: ColorMatrix::IDENTITY,
        }
    }

    /// Whether this effect expects a sliding-window box pass instead of a kernel row.
    pub fn is_box(&self) -> bool {
        self.kernel.boxes > 0
    }

    pub fn with_color(self, color: ColorMatrix) -> Self {
        Self {
            kernel: self.kernel,
            color: self.color.multiply(&color),
        }
    }

    pub fn is_noop(&self) -> bool {
        self.kernel.is_identity() && self.color.is_identity()
    }

    /// Pixels this effect's *result* depends on beyond its own rect; drives damage tests.
    ///
    /// This is the number a backend must not under-report: it decides which frame pixels
    /// invalidate the cached result.
    pub fn reach(&self) -> u32 {
        self.kernel.reach()
    }

    /// Internal working band the backend needs to produce that result.
    ///
    /// Separate from [`Effect::reach`] because it covers intermediate passes, not the
    /// pixels the output depends on. Each effect kind states its own value.
    pub fn band(&self) -> u32 {
        self.kernel.band()
    }
}

/// Fingerprint of a chain, for a result cache that must tell one chain from another.
///
/// Bits, not values: two chains are the same chain only when they would compute the same
/// pixels, and `0.0 == -0.0` is the kind of equality a pixel result does not care about
/// but a hash must not decide either way.
pub fn chain_fingerprint(effects: &[Effect]) -> u64 {
    // FNV-1a over the fields; stable for a given build and free of dependencies.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(&(effects.len() as u32).to_le_bytes());
    for effect in effects {
        let kernel = &effect.kernel;
        feed(&kernel.radius.to_le_bytes());
        feed(&kernel.boxes.to_le_bytes());
        feed(&kernel.sigma.to_bits().to_le_bytes());
        feed(&(kernel.weights.len() as u32).to_le_bytes());
        for weight in &kernel.weights {
            feed(&weight.to_bits().to_le_bytes());
        }
        for value in effect.color.0 {
            feed(&value.to_bits().to_le_bytes());
        }
    }
    hash
}

/// The same effect on the render grid: kernels are authored per logical dp, so a target
/// with more pixels per dp needs a wider one. One definition, or the backends disagree.
pub fn on_grid(effect: Effect, scale: f32) -> Effect {
    // Scale one is the authored grid; rebuilding a kernel there would only re-round it.
    if scale == 1.0 {
        return effect;
    }
    let kernel = effect.kernel.clone();
    let scaled = if kernel.boxes > 0 {
        Effect::box_blur(kernel.sigma * scale)
    } else if kernel.is_identity() {
        return effect;
    } else {
        Effect::blur(
            (kernel.radius as f32 * scale).round() as u32,
            kernel.sigma * scale,
        )
    };
    Effect {
        color: effect.color,
        ..scaled
    }
}

/// How far a chain of effects can reach, for damage and layer padding.
///
/// `guaranteed` is the analytic bound; `actual` is what the chain really uses. Keeping both
/// stops an approximation from being reported as if it were the real thing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectReach {
    pub guaranteed: u32,
    pub actual: u32,
}

impl EffectReach {
    pub fn of(effects: &[Effect]) -> Self {
        let mut reach = Self::default();
        for e in effects {
            // The band must cover the whole pass chain, not just the result's dependence.
            reach.actual += e.band();
            reach.guaranteed += e.kernel.guaranteed_reach();
        }
        reach
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blur declared per logical dp keeps its visual radius on a finer grid.
    #[test]
    fn a_blur_widens_with_the_render_grid() {
        let plain = Effect::box_blur(2.0);
        let dense = on_grid(plain.clone(), 2.0);
        assert!(
            (dense.kernel.sigma - 2.0 * plain.kernel.sigma).abs() < 0.6,
            "sigma tracks the grid: {} vs {}",
            plain.kernel.sigma,
            dense.kernel.sigma
        );
        let taps = Effect::blur(4, 1.5);
        let wide = on_grid(taps.clone(), 2.0);
        assert_eq!(wide.kernel.radius, 8, "a tap-limited kernel widens too");
        assert_eq!(wide.kernel.sigma, 3.0);
        assert_eq!(on_grid(taps, 1.0).kernel.radius, 4, "scale one is a no-op");
    }

    /// A windowed kernel needs a wider working band than the pixels it depends on.
    #[test]
    fn a_box_chain_reports_a_band_wider_than_its_reach() {
        let boxed = Effect::box_blur(6.0);
        assert!(
            boxed.band() > boxed.reach(),
            "the pass chain must reserve more than the result's dependence: {} vs {}",
            boxed.band(),
            boxed.reach()
        );
        // A kernel with a different pass count must report a different band, which a
        // hardcoded multiplier cannot do.
        let mut two_pass = boxed.clone();
        two_pass.kernel.boxes = 2;
        assert_ne!(
            two_pass.band(),
            boxed.band(),
            "the band must follow the kernel's own pass count"
        );
        assert_eq!(two_pass.band(), two_pass.kernel.radius * 2);
    }

    #[test]
    fn guaranteed_reach_is_analytic_and_can_exceed_the_truncation() {
        // A cheap blur is honestly reported as cheaper, not as if it were exact.
        let truncated = Kernel::gaussian(4, 2.5);
        assert_eq!(truncated.reach(), 4);
        assert_eq!(truncated.guaranteed_reach(), 8, "ceil(3 * 2.5)");

        let full = Kernel::gaussian(8, 2.5);
        assert_eq!(
            full.guaranteed_reach(),
            full.reach(),
            "truncation reaches the bound"
        );

        // A tap limit wider than 3σ is still read: the bound must not report the smaller
        // analytic radius while the kernel samples 16 pixels out.
        let wide = Kernel::gaussian(16, 1.5);
        assert_eq!(wide.guaranteed_reach(), 16, "the support, not ceil(3σ)");
        assert!(wide.guaranteed_reach() >= wide.band());
    }
}
