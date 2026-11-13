//! The Fluent token pack, with every value traceable to the reference source.

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

/// Fluent light theme, resolved by `ui-design` from the published token table.
pub fn pack() -> &'static Style {
    pack_of(ui_design::Appearance::Light)
}

/// The Fluent pack for a given appearance, resolved from WinUI 3's own `ThemeResource` graph.
///
/// The Web token names survive only as aliases; a control asks for the brush its XAML asks for.
/// The Fluent pack for an appearance. Only two packs exist, so each is built at most once:
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
    let ctx = ui_design::Context::new(appearance, ui_design::Platform::Desktop);
    let system = std::sync::Arc::new(ui_design::WinUISystem::new(ctx));
    let mut builder = ui_style::StyleBuilder::new(system).metrics_from_scale("medium");
    builder = builder
        .role_common("textSelection", ui_design::CommonColor::TextSelection)
        // WinUI inverts the glyphs to white over the accent wash.
        .selection_painter(ui_core::SelectionPainterRef(
            &crate::paint::FLUENT_SELECTION,
        ));

    // Fluent 2 type ramp: Base300 (14) for body, Base400 (16) for titles; ink comes from roles.
    let type_table = TypeTable::new([
        t(20.0, Weight::SEMIBOLD),
        t(16.0, Weight::SEMIBOLD),
        t(14.0, Weight::REGULAR),
        t(12.0, Weight::REGULAR),
        t(12.0, Weight::REGULAR),
    ]);

    builder
        .type_table(type_table)
        .controls(crate::spec::winui_controls())
        .motion(ui_style::MotionTable::winui())
        .build()
}

#[cfg(test)]
mod tests {

    // Fluent rounds its corners; the pack must say so from the scale, not a literal.
    #[test]
    fn the_control_corner_follows_the_fluent_scale() {
        let p = pack();
        let scale = ui_design::dimensions_of(
            ui_design::SystemId::Fluent,
            ui_design::Context::new(ui_design::Appearance::Light, ui_design::Platform::Web),
        )
        .radius;
        assert_eq!(p.metrics.control.corner, ui_design::CornerShape::ROUND);
        assert_eq!(p.metrics.control.corner, scale.curvature.corner_shape());
    }
    use super::*;

    #[test]
    fn the_pack_answers_in_winui_names_not_web_aliases() {
        // A control asks for the brush its XAML asks for; a Web name has no answer at all.
        let p = pack();
        assert!(p.ink("ButtonBackground").is_some());
        assert_eq!(p.ink("colorNeutralBackground1"), None);
    }
}
