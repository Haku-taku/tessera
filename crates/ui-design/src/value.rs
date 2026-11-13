//! Colour type and the resolution outcomes a token may have.

/// Opaque sRGB, the only colour the renderer speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba(pub u32);

impl Rgba {
    pub const TRANSPARENT: Self = Self(0x0000_0000);

    pub const fn from_argb(argb: u32) -> Self {
        Self(argb)
    }

    pub const fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self(0xff00_0000 | (red as u32) << 16 | (green as u32) << 8 | blue as u32)
    }

    pub const fn argb(self) -> u32 {
        self.0
    }

    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// Replace the alpha channel, keeping the colour.
    pub const fn with_alpha(self, alpha: u8) -> Self {
        Self((self.0 & 0x00ff_ffff) | (alpha as u32) << 24)
    }
}

/// A value together with where it came from. A snapshot that drops this cannot tell a vendor
/// number from one we synthesized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved<T> {
    pub value: T,
    pub source: crate::source::Source,
}

impl<T> Resolved<T> {
    pub const fn new(value: T, source: crate::source::Source) -> Self {
        Self { value, source }
    }
}

/// What asking a system for a token produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resolution<T> {
    /// The system has a concrete value for this context.
    Value(T),
    /// The token belongs to another system; the caller must ask elsewhere.
    Unmapped,
    /// The system has a policy but cannot compute the value here (e.g. host-resolved).
    Absent(Reason),
}

/// Why a value is absent rather than merely unmapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reason {
    /// Resolved by the operating system at runtime; we only know the name.
    PlatformResolved(&'static str),
    /// Out of the sRGB gamut for this hue and tone.
    OutOfGamut,
    /// The system has no concept of this role at all.
    NotAConcept,
}

impl<T> Resolution<T> {
    pub fn value(self) -> Option<T> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    /// True when the caller must not silently fall back; this is the loud path.
    pub fn is_failure(self) -> bool {
        !matches!(self, Self::Value(_))
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Resolution<U> {
        match self {
            Self::Value(v) => Resolution::Value(f(v)),
            Self::Unmapped => Resolution::Unmapped,
            Self::Absent(r) => Resolution::Absent(r),
        }
    }
}
