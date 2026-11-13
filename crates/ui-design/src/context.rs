//! The context a token resolves under: appearance, contrast, depth and platform.
//!
//! The three design systems do not share one theme switch: MD3 has three contrast levels,
//! Fluent has light/dark/high-contrast, HIG also distinguishes base/elevated depth and
//! increased contrast. Modelling the union beats inventing a lowest common denominator.

/// Light or dark appearance; every system has this axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Appearance {
    Light,
    Dark,
}

/// How much contrast to demand. Ordering runs from least to most demanding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Contrast {
    /// MD3 low / a deliberately softened scheme.
    Low,
    /// MD3 standard, Fluent default, HIG default.
    Standard,
    /// MD3 medium.
    Medium,
    /// MD3 high, Fluent high contrast, HIG increased contrast.
    High,
}

impl Contrast {
    /// MD3 writes contrast as a number in -1..=1; this maps it onto the four named levels.
    pub fn from_md3_level(level: f64) -> Self {
        if level <= -0.5 {
            Self::Low
        } else if level < 0.25 {
            Self::Standard
        } else if level < 0.75 {
            Self::Medium
        } else {
            Self::High
        }
    }

    /// The numeric contrast level MD3's resolution expects for this bucket.
    pub fn md3_level(self) -> f64 {
        match self {
            Self::Low => -1.0,
            Self::Standard => 0.0,
            Self::Medium => 0.5,
            Self::High => 1.0,
        }
    }
}

/// Whether the surface sits at the base layer or is elevated into the foreground.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Depth {
    Base,
    Elevated,
}

/// Which platform's conventions and hit-target minima apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    Ios,
    Ipados,
    Macos,
    Android,
    Web,
    Desktop,
}

/// Text direction; `Trailing` spacing flips with it, which MD3 calls out explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WritingMode {
    LeftToRight,
    RightToLeft,
}

/// The full context a resolution may depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Context {
    pub appearance: Appearance,
    pub contrast: Contrast,
    pub depth: Depth,
    pub platform: Platform,
    pub writing: WritingMode,
}

impl Context {
    /// The common case: standard contrast on a base layer, left to right.
    pub const fn new(appearance: Appearance, platform: Platform) -> Self {
        Self {
            appearance,
            contrast: Contrast::Standard,
            depth: Depth::Base,
            platform,
            writing: WritingMode::LeftToRight,
        }
    }

    pub const fn with_contrast(mut self, contrast: Contrast) -> Self {
        self.contrast = contrast;
        self
    }

    pub const fn with_depth(mut self, depth: Depth) -> Self {
        self.depth = depth;
        self
    }

    pub const fn with_writing(mut self, writing: WritingMode) -> Self {
        self.writing = writing;
        self
    }

    /// Whether this is a dark appearance; most formulas branch on this.
    pub const fn is_dark(self) -> bool {
        matches!(self.appearance, Appearance::Dark)
    }
}
