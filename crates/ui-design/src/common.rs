//! The closed list of semantic colours shared across style systems.
//!
//! This is a *path*, not a lowest common denominator: every native token stays reachable
//! through `native()`, and a shared widget that needs something richer asks `caps()` first.

/// A colour role every design system can be asked to fill, or to refuse explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommonColor {
    /// The product's accent: MD3 `primary`, Fluent `colorBrandBackground`, HIG `tintColor`.
    Accent,
    /// Text/symbol on the accent background.
    OnAccent,
    /// A lower-emphasis accent container (MD3 `primaryContainer`, HIG `secondarySystemFill`).
    AccentContainer,
    /// Content on `AccentContainer`.
    OnAccentContainer,
    /// The page/app base surface.
    Surface,
    /// Content on `Surface`.
    OnSurface,
    /// A raised or grouped surface (card, sheet, dialog).
    SurfaceRaised,
    /// Content on `SurfaceRaised`.
    OnSurfaceRaised,
    /// De-emphasised text.
    TextMuted,
    /// Placeholder text.
    TextPlaceholder,
    /// Links.
    TextLink,
    /// The wash behind selected text.
    TextSelection,
    /// Hairline between regions.
    Divider,
    /// The stroke around an interactive surface.
    Stroke,
    /// Hover state for a neutral surface.
    SurfaceHover,
    /// Pressed state for a neutral surface.
    SurfacePressed,
    /// Selected state for a neutral surface.
    SurfaceSelected,
    /// Error accent.
    Error,
    /// Content on `Error`.
    OnError,
    /// Success accent.
    Success,
    /// Warning accent.
    Warning,
    /// Focus ring.
    FocusRing,
    /// Scrim behind modals.
    Scrim,
}

impl CommonColor {
    /// Every role, for coverage sweeps.
    pub const ALL: [Self; 23] = [
        Self::Accent,
        Self::OnAccent,
        Self::AccentContainer,
        Self::OnAccentContainer,
        Self::Surface,
        Self::OnSurface,
        Self::SurfaceRaised,
        Self::OnSurfaceRaised,
        Self::TextMuted,
        Self::TextPlaceholder,
        Self::TextLink,
        Self::TextSelection,
        Self::Divider,
        Self::Stroke,
        Self::SurfaceHover,
        Self::SurfacePressed,
        Self::SurfaceSelected,
        Self::Error,
        Self::OnError,
        Self::Success,
        Self::Warning,
        Self::FocusRing,
        Self::Scrim,
    ];

    /// For fixed-size arrays that must hold one entry per role.
    pub const COUNT: usize = Self::ALL.len();

    /// A stable name, usable in tests and gallery labels.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::OnAccent => "on_accent",
            Self::AccentContainer => "accent_container",
            Self::OnAccentContainer => "on_accent_container",
            Self::Surface => "surface",
            Self::OnSurface => "on_surface",
            Self::SurfaceRaised => "surface_raised",
            Self::OnSurfaceRaised => "on_surface_raised",
            Self::TextMuted => "text_muted",
            Self::TextPlaceholder => "text_placeholder",
            Self::TextLink => "text_link",
            Self::TextSelection => "text_selection",
            Self::Divider => "divider",
            Self::Stroke => "stroke",
            Self::SurfaceHover => "surface_hover",
            Self::SurfacePressed => "surface_pressed",
            Self::SurfaceSelected => "surface_selected",
            Self::Error => "error",
            Self::OnError => "on_error",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::FocusRing => "focus_ring",
            Self::Scrim => "scrim",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_all_table_lists_every_role_exactly_once() {
        let names: HashSet<_> = CommonColor::ALL.iter().map(|r| r.name()).collect();
        assert_eq!(names.len(), CommonColor::ALL.len());
        assert!(names.contains("accent") && names.contains("on_accent"));
    }
}
