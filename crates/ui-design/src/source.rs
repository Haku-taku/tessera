//! Provenance: every value must answer "why should I believe this?".

/// Where a token value came from. Chosen so that a gallery page can print it verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A number published by the design system's own spec or source.
    Spec(&'static str),
    /// We computed it from spec inputs; the fields say which rule.
    Derived(&'static str),
    /// Not published by the vendor; our own convention. Must be self-verified.
    Convention(&'static str),
    /// Resolved by the host OS at runtime; the name is all we know.
    Platform(&'static str),
}

impl Source {
    /// A short label for display.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Spec(_) => "spec",
            Self::Derived(_) => "derived",
            Self::Convention(_) => "convention",
            Self::Platform(_) => "platform",
        }
    }

    /// The detail behind the label, e.g. the file or rule name.
    pub const fn detail(self) -> &'static str {
        match self {
            Self::Spec(d) | Self::Derived(d) | Self::Convention(d) | Self::Platform(d) => d,
        }
    }
}
