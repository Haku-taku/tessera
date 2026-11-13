//! The Glass token pack, with every value traceable to the reference source.

use ui_core::Weight;
use ui_core::geom::Color;
use ui_style::{Style, TextStyleToken, TypeTable};

fn c(v: [u8; 4]) -> Color {
    Color::from_rgba8(v[0], v[1], v[2], v[3])
}

fn t(size: f32, weight: Weight) -> TextStyleToken {
    TextStyleToken {
        size,
        weight,
        line_gap: 0.0,
        indent: 0.0,
    }
}

/// Glass (Cupertino) colours: the track/rim literals are the reference's own numbers; the
/// text ink now comes from HIG so it cannot drift from the system colour.
pub fn pack() -> &'static Style {
    pack_of(ui_design::Appearance::Light)
}

/// The Glass pack for a given appearance; text ink follows the matching HIG context.
/// The Glass pack for an appearance. Only two packs exist, so each is built at most once:
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
    let ctx = ui_design::Context::new(appearance, ui_design::Platform::Ios);
    let hig = std::sync::Arc::new(ui_design::HigSystem::new(ctx));
    let builder = ui_style::StyleBuilder::new(hig)
        .metrics_from_scale("medium")
        // iOS-exact inactive track (`glass_switch.dart:511`), not a system fill.
        .role(
            "glassTrackInactive",
            match appearance {
                ui_design::Appearance::Light => c([0xC5, 0xC5, 0xC6, 0xFF]),
                ui_design::Appearance::Dark => c([0xFF, 0xFF, 0xFF, 0x33]),
            },
        )
        .role_from("glassTrackActive", "systemGreen")
        .role("glassThumb", c([0xFF, 0xFF, 0xFF, 255]))
        // The resting tab pill: black 10% in light, white 10% in dark.
        .role(
            "glassTabRest",
            match appearance {
                ui_design::Appearance::Light => c([0x00, 0x00, 0x00, 0x1A]),
                ui_design::Appearance::Dark => c([0xFF, 0xFF, 0xFF, 0x1A]),
            },
        )
        // What a selection wears in glass: a tab seen through the lens, a segment's pill.
        .role_common("glassSelectionAccent", ui_design::CommonColor::Accent)
        // Segmented control: an 8% black track under a 20% black pill in light, white in dark
        // (`glass_segmented_control.dart:589-591`, `:663-666`).
        .role(
            "glassSegmentTrack",
            match appearance {
                ui_design::Appearance::Light => c([0x00, 0x00, 0x00, 0x14]),
                ui_design::Appearance::Dark => c([0xFF, 0xFF, 0xFF, 0x1F]),
            },
        )
        .role(
            "glassSegmentRest",
            match appearance {
                ui_design::Appearance::Light => c([0x00, 0x00, 0x00, 0x14]),
                ui_design::Appearance::Dark => c([0xFF, 0xFF, 0xFF, 0x33]),
            },
        )
        // Slider fill: the system tint, as in iOS 27; the unfilled bar stays neutral.
        .role_from("glassSliderActive", "systemBlue")
        .role(
            "glassSliderInactive",
            match appearance {
                ui_design::Appearance::Light => c([0x00, 0x00, 0x00, 0x33]),
                ui_design::Appearance::Dark => c([0xFF, 0xFF, 0xFF, 0x33]),
            },
        )
        // Light-path rim: `rimThickness 0.7`, `edgeAlphaMultiplier 0.28`.
        .role("glassRim", c([0xFF, 0xFF, 0xFF, 0x47]))
        .role_common("textFillColorPrimary", ui_design::CommonColor::OnSurface)
        // Content faces are Apple's system fills, never glass. A grouped cell is white / #1C1C1E
        // (`secondarySystemGroupedBackground`), not the #F2F2F7 page it sits on.
        .role(
            "glassRowFill",
            match appearance {
                ui_design::Appearance::Light => c([0xFF, 0xFF, 0xFF, 0xFF]),
                ui_design::Appearance::Dark => c([0x1C, 0x1C, 0x1E, 0xFF]),
            },
        )
        .role_from("glassFieldFill", "tertiarySystemFill")
        .role_from("glassChipFill", "secondarySystemFill")
        // Disabled button appearance (Cupertino): system fills for the track, inactiveGray for text.
        .role_from("buttonFillDisabled", "quaternarySystemFill")
        .role_from("buttonTextDisabled", "inactiveGray")
        .role("buttonBorderDisabled", Color::TRANSPARENT)
        .role_common("textSelection", ui_design::CommonColor::TextSelection)
        .selection_painter(ui_core::SelectionPainterRef(&crate::paint::GLASS_SELECTION));
    let type_table = TypeTable::new([
        t(20.0, Weight::SEMIBOLD),
        t(16.0, Weight::SEMIBOLD),
        t(14.0, Weight::REGULAR),
        t(12.0, Weight::REGULAR),
        t(12.0, Weight::REGULAR),
    ]);

    builder.type_table(type_table).build()
}

#[cfg(test)]
mod tests {

    // The visible payoff of the mapping: HIG's continuous curvature is a squircle, so glass
    // corners must be SQUIRCLE — previously they were hardcoded ROUND.
    #[test]
    fn the_control_corner_is_continuous_because_hig_says_so() {
        use ui_design::Geometry;
        let p = pack();
        let scale = ui_design::HigSystem::new(ui_design::Context::new(
            ui_design::Appearance::Light,
            ui_design::Platform::Ios,
        ))
        .radius();
        assert_eq!(p.metrics.control.corner, ui_design::CornerShape::SQUIRCLE);
        assert_eq!(p.metrics.control.corner, scale.curvature.corner_shape());
        assert_ne!(p.metrics.control.corner, ui_design::CornerShape::ROUND);
    }
    use super::*;
}
