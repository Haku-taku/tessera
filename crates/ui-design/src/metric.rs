//! The closed list of scalar metrics shared across style systems.
//!
//! Only metrics with a real reader belong here; see "Where a constant belongs" in
//! `docs/src/engine/style-objects.md`.

/// A scalar metric every design system can be asked to fill, or to refuse explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommonMetric {
    /// The default corner radius of a neutral control.
    Radius,
    /// Stroke / hairline / divider width.
    StrokeWidth,
}

impl CommonMetric {
    /// Every metric, for coverage sweeps.
    pub const ALL: [Self; 2] = [Self::Radius, Self::StrokeWidth];

    pub const COUNT: usize = Self::ALL.len();

    /// A stable name, usable in tests and gallery labels.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Radius => "radius",
            Self::StrokeWidth => "stroke_width",
        }
    }
}
