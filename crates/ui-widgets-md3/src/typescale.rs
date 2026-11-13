//! MD3's 2021 type scale: fifteen styles, each a size and a weight.
//!
//! The component export names the style it uses (`md.comp.button.medium.label-text` resolves to
//! `md.sys.typescale.title-medium`) but not its numbers, so the numbers live here.

use ui_core::Weight;

/// One type style: point size and weight. Line height and tracking are not modelled yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStyle {
    pub size: f32,
    pub weight: Weight,
}

const fn style(size: f32, weight: Weight) -> TypeStyle {
    TypeStyle { size, weight }
}

// Flutter's `text_theme.dart` carries the same table for the 2021 spec.
pub const DISPLAY_LARGE: TypeStyle = style(57.0, Weight::REGULAR);
pub const DISPLAY_MEDIUM: TypeStyle = style(45.0, Weight::REGULAR);
pub const DISPLAY_SMALL: TypeStyle = style(36.0, Weight::REGULAR);
pub const HEADLINE_LARGE: TypeStyle = style(32.0, Weight::REGULAR);
pub const HEADLINE_MEDIUM: TypeStyle = style(28.0, Weight::REGULAR);
pub const HEADLINE_SMALL: TypeStyle = style(24.0, Weight::REGULAR);
pub const TITLE_LARGE: TypeStyle = style(22.0, Weight::REGULAR);
pub const TITLE_MEDIUM: TypeStyle = style(16.0, Weight::MEDIUM);
pub const TITLE_SMALL: TypeStyle = style(14.0, Weight::MEDIUM);
pub const BODY_LARGE: TypeStyle = style(16.0, Weight::REGULAR);
pub const BODY_MEDIUM: TypeStyle = style(14.0, Weight::REGULAR);
pub const BODY_SMALL: TypeStyle = style(12.0, Weight::REGULAR);
pub const LABEL_LARGE: TypeStyle = style(14.0, Weight::MEDIUM);
pub const LABEL_MEDIUM: TypeStyle = style(12.0, Weight::MEDIUM);
pub const LABEL_SMALL: TypeStyle = style(11.0, Weight::MEDIUM);

/// Look up a scale by the name the export uses (`label-large`, `title-medium`, …).
pub fn named(scale: &str) -> TypeStyle {
    match scale {
        "display-large" => DISPLAY_LARGE,
        "display-medium" => DISPLAY_MEDIUM,
        "display-small" => DISPLAY_SMALL,
        "headline-large" => HEADLINE_LARGE,
        "headline-medium" => HEADLINE_MEDIUM,
        "headline-small" => HEADLINE_SMALL,
        "title-large" => TITLE_LARGE,
        "title-medium" => TITLE_MEDIUM,
        "title-small" => TITLE_SMALL,
        "body-large" => BODY_LARGE,
        "body-medium" => BODY_MEDIUM,
        "body-small" => BODY_SMALL,
        "label-large" => LABEL_LARGE,
        "label-medium" => LABEL_MEDIUM,
        "label-small" => LABEL_SMALL,
        other => {
            debug_assert!(false, "unknown type scale {other}");
            LABEL_LARGE
        }
    }
}
