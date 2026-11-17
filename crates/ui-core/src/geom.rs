//! Tiny geometry vocabulary shared by every crate.

use core::ops::{Add, Div, Mul, Sub};

/// Linear interpolation that is exact at both ends, so a settled transition reports the
/// target itself rather than a value one ULP off it.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    if t <= 0.0 {
        a
    } else if t >= 1.0 {
        b
    } else {
        a + (b - a) * t
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for Point {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    pub const ZERO: Self = Self { w: 0.0, h: 0.0 };

    pub const fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }

    pub fn is_zero(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

impl Add for Size {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.w + o.w, self.h + o.h)
    }
}

impl Div<f32> for Size {
    type Output = Self;
    fn div(self, s: f32) -> Self {
        Self::new(self.w / s, self.h / s)
    }
}

impl Size {
    /// Largest value in each axis.
    pub fn max(self, o: Self) -> Self {
        Self::new(self.w.max(o.w), self.h.max(o.h))
    }

    /// Smallest value in each axis.
    pub fn min(self, o: Self) -> Self {
        Self::new(self.w.min(o.w), self.h.min(o.h))
    }
}

/// Axis-aligned rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const ZERO: Self = Self {
        origin: Point::ZERO,
        size: Size::ZERO,
    };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(w, h),
        }
    }

    pub fn from_origin_size(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub fn min_x(self) -> f32 {
        self.origin.x
    }
    pub fn min_y(self) -> f32 {
        self.origin.y
    }
    pub fn max_x(self) -> f32 {
        self.origin.x + self.size.w
    }
    pub fn max_y(self) -> f32 {
        self.origin.y + self.size.h
    }

    pub fn is_empty(self) -> bool {
        self.size.w <= 0.0 || self.size.h <= 0.0
    }

    pub fn center(self) -> Point {
        Point::new(
            self.origin.x + self.size.w * 0.5,
            self.origin.y + self.size.h * 0.5,
        )
    }

    pub fn contains(self, p: Point) -> bool {
        p.x >= self.min_x() && p.x < self.max_x() && p.y >= self.min_y() && p.y < self.max_y()
    }

    pub fn intersects(self, o: Self) -> bool {
        self.min_x() < o.max_x()
            && o.min_x() < self.max_x()
            && self.min_y() < o.max_y()
            && o.min_y() < self.max_y()
    }

    pub fn intersection(self, o: Self) -> Option<Self> {
        if !self.intersects(o) {
            return None;
        }
        let x = self.min_x().max(o.min_x());
        let y = self.min_y().max(o.min_y());
        let x1 = self.max_x().min(o.max_x());
        let y1 = self.max_y().min(o.max_y());
        Some(Self::new(x, y, x1 - x, y1 - y))
    }

    pub fn union(self, o: Self) -> Self {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        let x = self.min_x().min(o.min_x());
        let y = self.min_y().min(o.min_y());
        let x1 = self.max_x().max(o.max_x());
        let y1 = self.max_y().max(o.max_y());
        Self::new(x, y, x1 - x, y1 - y)
    }

    /// Grow the rect by `n` on every side.
    pub fn inflate(self, n: f32) -> Self {
        if self.is_empty() {
            return self;
        }
        Self::new(
            self.origin.x - n,
            self.origin.y - n,
            self.size.w + n * 2.0,
            self.size.h + n * 2.0,
        )
    }

    /// Grow by per-side insets (positive grows outward).
    pub fn inflate_each(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        if self.is_empty() {
            return self;
        }
        Self::new(
            self.origin.x - left,
            self.origin.y - top,
            self.size.w + left + right,
            self.size.h + top + bottom,
        )
    }

    pub fn translate(self, by: Point) -> Self {
        Self::from_origin_size(self.origin + by, self.size)
    }

    pub fn round_out(self) -> Self {
        if self.is_empty() {
            return self;
        }
        let x0 = self.min_x().floor();
        let y0 = self.min_y().floor();
        let x1 = self.max_x().ceil();
        let y1 = self.max_y().ceil();
        Self::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// Distance a node may paint outside its own rect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    /// All sides `auto`: undeclared for placement insets, an even split for OOF margins.
    pub const AUTO: Self = Self {
        left: crate::layout::AUTO,
        top: crate::layout::AUTO,
        right: crate::layout::AUTO,
        bottom: crate::layout::AUTO,
    };

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn all(n: f32) -> Self {
        Self {
            left: n,
            top: n,
            right: n,
            bottom: n,
        }
    }

    pub fn is_zero(self) -> bool {
        self.left == 0.0 && self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0
    }

    /// Expand `r` by these insets.
    pub fn expand(self, r: Rect) -> Rect {
        r.inflate_each(self.left, self.top, self.right, self.bottom)
    }
}

/// Straight (non-premultiplied) sRGB colour, 0..=1 per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Hashed by bit pattern, so a control can fingerprint the colours it was given.
impl core::hash::Hash for Color {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        for c in [self.r, self.g, self.b, self.a] {
            core::hash::Hash::hash(&c.to_bits(), state);
        }
    }
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    /// Linear interpolation in the stored (non-premultiplied) space, `t` clamped to 0..=1.
    ///
    /// Straight component lerp, not perceptual: an animation of two mid-tones is the
    /// only place the difference shows, and matching it would need a colour space choice.
    /// A blend; at `t == 1` the arithmetic can still land a hair off `other`, so a value
    /// that settles on a token takes [`lerp_exact`](Self::lerp_exact).
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// `lerp` that returns `other` itself once `t` reaches 1: a settled control must
    /// report the token's colour, not a blend of it.
    pub fn lerp_exact(self, other: Self, t: f32) -> Self {
        if t <= 0.0 {
            self
        } else if t >= 1.0 {
            other
        } else {
            self.lerp(other, t)
        }
    }

    /// Multiply alpha, so a fading or disabled control dims its inks once.
    pub fn faded(self, k: f32) -> Self {
        Self {
            a: self.a * k,
            ..self
        }
    }

    /// This colour drawn over `base`: what a translucent tint shows on the surface under it.
    ///
    /// Source-over with straight channels, the same blend the painter applies, so a system
    /// that expresses a surface as a tint can be resolved to the colour it becomes.
    pub fn over(self, base: Self) -> Self {
        let sa = self.a.clamp(0.0, 1.0);
        let ba = base.a.clamp(0.0, 1.0);
        let a = sa + ba * (1.0 - sa);
        if a <= 0.0 {
            return Self::TRANSPARENT;
        }
        let channel = |s: f32, b: f32| (s * sa + b * ba * (1.0 - sa)) / a;
        Self {
            r: channel(self.r, base.r),
            g: channel(self.g, base.g),
            b: channel(self.b, base.b),
            a,
        }
    }

    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Build from 8-bit sRGB components.
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// sRGB transfer function: encoded value -> linear light.
    pub fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// sRGB transfer function: linear light -> encoded value.
    pub fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    /// Linear-light components with alpha premultiplied.
    pub fn to_linear_premul(self) -> [f32; 4] {
        let a = self.a.clamp(0.0, 1.0);
        [
            Self::srgb_to_linear(self.r.clamp(0.0, 1.0)) * a,
            Self::srgb_to_linear(self.g.clamp(0.0, 1.0)) * a,
            Self::srgb_to_linear(self.b.clamp(0.0, 1.0)) * a,
            a,
        ]
    }
}

/// Corner curve family, following CSS `corner-shape`.
///
/// The curve is a superellipse whose exponent is `2^k`, so named shapes are presets on
/// one continuous axis rather than an enum of kinds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerShape {
    k: f32,
}

impl Default for CornerShape {
    fn default() -> Self {
        Self::ROUND
    }
}

impl CornerShape {
    /// Circular arc; the CSS default and the only shape before `corner-shape` existed.
    pub const ROUND: Self = Self { k: 1.0 };
    /// iOS-style squircle, `superellipse(2)`.
    pub const SQUIRCLE: Self = Self { k: 2.0 };
    /// Straight diagonal cut, `superellipse(0)`.
    pub const BEVEL: Self = Self { k: 0.0 };
    /// No rounding at all, `superellipse(infinity)`.
    pub const SQUARE: Self = Self { k: f32::INFINITY };

    /// Raw CSS parameter; `1` is round, larger is squarer.
    pub const fn from_k(k: f32) -> Self {
        Self { k }
    }

    pub const fn k(self) -> f32 {
        self.k
    }

    /// Superellipse exponent `2^k`, clamped to the range the renderers evaluate.
    pub fn exponent(self) -> f32 {
        // k < 0 turns the corner concave (CSS `scoop`), which coverage does not handle
        // yet; above k = 6 the curve is within a pixel of square.
        let k = if self.k.is_nan() {
            1.0
        } else {
            self.k.clamp(0.0, 6.0)
        };
        2.0f32.powf(k)
    }
}

/// Per-corner radii, clockwise from top-left, plus the curve they follow.
///
/// The shape travels with the radii so a caller cannot set one and forget the other.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
    pub shape: CornerShape,
}

impl Corners {
    pub const ZERO: Self = Self {
        tl: 0.0,
        tr: 0.0,
        br: 0.0,
        bl: 0.0,
        shape: CornerShape::ROUND,
    };

    pub const fn all(r: f32) -> Self {
        Self {
            tl: r,
            tr: r,
            br: r,
            bl: r,
            shape: CornerShape::ROUND,
        }
    }

    /// Same radii, different curve family.
    pub const fn with_shape(mut self, shape: CornerShape) -> Self {
        self.shape = shape;
        self
    }

    pub const fn squircle(self) -> Self {
        self.with_shape(CornerShape::SQUIRCLE)
    }

    pub fn is_zero(self) -> bool {
        self.tl == 0.0 && self.tr == 0.0 && self.br == 0.0 && self.bl == 0.0
    }

    /// Clamp so opposing radii never exceed the available side length.
    pub fn clamp_to(self, size: Size) -> Self {
        let lim = (size.w.min(size.h) * 0.5).max(0.0);
        Self {
            tl: self.tl.clamp(0.0, lim),
            tr: self.tr.clamp(0.0, lim),
            br: self.br.clamp(0.0, lim),
            bl: self.bl.clamp(0.0, lim),
            shape: self.shape,
        }
    }
}

#[cfg(test)]
mod corner_shape_tests {
    use super::CornerShape;

    #[test]
    fn exponent_follows_the_css_mapping() {
        // CSS says `superellipse(K)` uses an exponent of `2^K`; the named shapes are
        // presets on that axis. Getting this wrong (e.g. reading it as `2*K`) makes
        // every shape but `round` disagree with browsers.
        assert_eq!(CornerShape::ROUND.exponent(), 2.0);
        assert_eq!(CornerShape::SQUIRCLE.exponent(), 4.0);
        assert_eq!(CornerShape::BEVEL.exponent(), 1.0);
        assert_eq!(CornerShape::from_k(3.0).exponent(), 8.0);
    }

    #[test]
    fn shape_survives_radius_clamping() {
        // Clamping radii must not silently reset the curve family.
        let c = CornerShape::SQUIRCLE;
        let corners = super::Corners::all(999.0)
            .with_shape(c)
            .clamp_to(super::Size::new(40.0, 40.0));
        assert_eq!(corners.shape, c);
        assert_eq!(corners.tl, 20.0);
    }
}
