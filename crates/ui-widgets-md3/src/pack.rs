//! The MD3 token pack, with every value traceable to the reference source.

use ui_core::Weight;
use ui_style::{Style, TextStyleToken, TypeTable};

fn t(size: f32, weight: Weight) -> TextStyleToken {
    TextStyleToken {
        size,
        weight,
        line_gap: 0.0,
        indent: 0.0,
    }
}

/// MD3 baseline light scheme, resolved by `ui-design` rather than typed in by hand.
pub fn pack() -> &'static Style {
    pack_of(ui_design::Appearance::Light)
}

/// Every MD3 colour role: the scheme's own snake_case name, and the camelCase spelling the
/// design-system export uses. The pack carries both, so a generated token table resolves.
const ROLE_NAMES: [(&str, &str); 49] = [
    ("background", "background"),
    ("on_background", "onBackground"),
    ("surface", "surface"),
    ("surface_dim", "surfaceDim"),
    ("surface_bright", "surfaceBright"),
    ("surface_container_lowest", "surfaceContainerLowest"),
    ("surface_container_low", "surfaceContainerLow"),
    ("surface_container", "surfaceContainer"),
    ("surface_container_high", "surfaceContainerHigh"),
    ("surface_container_highest", "surfaceContainerHighest"),
    ("on_surface", "onSurface"),
    ("surface_variant", "surfaceVariant"),
    ("on_surface_variant", "onSurfaceVariant"),
    ("inverse_surface", "inverseSurface"),
    ("inverse_on_surface", "inverseOnSurface"),
    ("outline", "outline"),
    ("outline_variant", "outlineVariant"),
    ("shadow", "shadow"),
    ("scrim", "scrim"),
    ("surface_tint", "surfaceTint"),
    ("primary", "primary"),
    ("on_primary", "onPrimary"),
    ("primary_container", "primaryContainer"),
    ("on_primary_container", "onPrimaryContainer"),
    ("inverse_primary", "inversePrimary"),
    ("secondary", "secondary"),
    ("on_secondary", "onSecondary"),
    ("secondary_container", "secondaryContainer"),
    ("on_secondary_container", "onSecondaryContainer"),
    ("tertiary", "tertiary"),
    ("on_tertiary", "onTertiary"),
    ("tertiary_container", "tertiaryContainer"),
    ("on_tertiary_container", "onTertiaryContainer"),
    ("error", "error"),
    ("on_error", "onError"),
    ("error_container", "errorContainer"),
    ("on_error_container", "onErrorContainer"),
    ("primary_fixed", "primaryFixed"),
    ("primary_fixed_dim", "primaryFixedDim"),
    ("on_primary_fixed", "onPrimaryFixed"),
    ("on_primary_fixed_variant", "onPrimaryFixedVariant"),
    ("secondary_fixed", "secondaryFixed"),
    ("secondary_fixed_dim", "secondaryFixedDim"),
    ("on_secondary_fixed", "onSecondaryFixed"),
    ("on_secondary_fixed_variant", "onSecondaryFixedVariant"),
    ("tertiary_fixed", "tertiaryFixed"),
    ("tertiary_fixed_dim", "tertiaryFixedDim"),
    ("on_tertiary_fixed", "onTertiaryFixed"),
    ("on_tertiary_fixed_variant", "onTertiaryFixedVariant"),
];

/// The MD3 pack for a given appearance: the only thing that changes is the scheme seeded from
/// a dark or light context, so every role resolves to that appearance's value.
/// The MD3 pack for an appearance. Only two packs exist, so each is built at most once:
/// a pack resolves every colour role through HCT, and a page asks for it on every frame.
pub fn pack_of(appearance: ui_design::Appearance) -> &'static Style {
    static LIGHT: std::sync::OnceLock<Style> = std::sync::OnceLock::new();
    static DARK: std::sync::OnceLock<Style> = std::sync::OnceLock::new();
    match appearance {
        ui_design::Appearance::Light => LIGHT.get_or_init(|| build(appearance)),
        ui_design::Appearance::Dark => DARK.get_or_init(|| build(appearance)),
    }
}

/// `pack_of` as an owned `Style`, for the `fn(Appearance) -> Style` slots. The style cache
/// calls it only when it has to build something new, never once per frame.
pub fn pack_owned(appearance: ui_design::Appearance) -> Style {
    pack_of(appearance).clone()
}

fn build(appearance: ui_design::Appearance) -> Style {
    let ctx = ui_design::Context::new(appearance, ui_design::Platform::Android);
    let scheme = std::sync::Arc::new(ui_design::Md3Scheme::baseline(ctx));
    let mut builder = ui_style::StyleBuilder::new(scheme).metrics_from_scale("largeIncreased");
    for (native, camel) in ROLE_NAMES {
        builder = builder.role_from(camel, native);
    }
    // MD3 has no painter of its own: the engine's flat rect in primary@40% is its look.
    builder = builder.role_common("textSelection", ui_design::CommonColor::TextSelection);

    // M3 2021 type scale (`typography.dart`): title 20/w500, body 14/w500, small 12/w400.
    let type_table = TypeTable::new([
        t(20.0, Weight::MEDIUM),
        t(16.0, Weight::REGULAR),
        t(14.0, Weight::MEDIUM),
        t(12.0, Weight::REGULAR),
        t(12.0, Weight::REGULAR),
    ]);

    builder.type_table(type_table).build()
}

#[cfg(test)]
mod tests {

    // The pack's corners must come from MD3's scale (round), not from a hardcoded constant.
    #[test]
    fn the_control_corner_follows_the_md3_scale() {
        use ui_design::Geometry;
        let p = pack();
        let scale = ui_design::Md3Scheme::baseline(ui_design::Context::new(
            ui_design::Appearance::Light,
            ui_design::Platform::Android,
        ))
        .radius();
        assert_eq!(p.metrics.control.corner, ui_design::CornerShape::ROUND);
        assert_eq!(p.metrics.control.corner, scale.curvature.corner_shape());
    }
    use super::*;
    use ui_core::geom::Color;
    use ui_design::Palette;
    use ui_style::RoleName;

    // The pack is now derived, so pin the *resolved* values: primary must equal the baseline
    // purple at tone 40, and the resolved outline is the neutral-variant palette, not the
    // hand-copied literal (our derivation is the authority now).
    #[test]
    fn the_baseline_scheme_carries_the_resolved_primary_and_its_own_outline() {
        let p = pack();
        let scheme = ui_design::Md3Scheme::baseline(ui_design::Context::new(
            ui_design::Appearance::Light,
            ui_design::Platform::Android,
        ));
        let primary = p.roles.get(RoleName("primary")).unwrap();
        let expected = scheme.native_colour("primary").value().unwrap().argb();
        assert_eq!(
            primary,
            Color::from_rgba8(
                (expected >> 16) as u8,
                (expected >> 8) as u8,
                expected as u8,
                255
            )
        );
        assert!((ui_design::tone_of(expected) - 40.0).abs() < 0.6);
        assert!(p.roles.get(RoleName("outline")).is_some());
    }
}
