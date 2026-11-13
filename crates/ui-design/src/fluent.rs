//! Fluent 2 as a design system: 459 tokens as data, plus the common-role adapter.

use crate::capabilities::{Geometry, Palette};
use crate::caps::StyleCaps;
use crate::common::CommonColor;
use crate::context::{Context, Contrast};
use crate::metric::CommonMetric;
use crate::source::Source;
use crate::system::{StyleSystem, SystemId};
use crate::value::{Reason, Resolution, Resolved, Rgba};

mod data {
    include!("../data/fluent_data.rs");
}

/// The package every value in this file traces back to.
const TOKEN_SPEC: Source = Source::Spec("@fluentui/tokens 1.0.0-alpha.24 (MIT)");

/// The three theme columns Fluent ships, in the order the data table uses.
fn theme_column(ctx: &Context) -> usize {
    match (ctx.contrast, ctx.appearance) {
        (Contrast::High, _) => 2,
        (_, crate::context::Appearance::Light) => 0,
        (_, crate::context::Appearance::Dark) => 1,
    }
}

/// Every Fluent token, unpruned; a lookup by the token's own name.
#[derive(Clone, Copy, Debug)]
pub struct FluentSystem {
    pub ctx: Context,
}

impl FluentSystem {
    pub const fn new(ctx: Context) -> Self {
        Self { ctx }
    }

    fn column(&self) -> usize {
        theme_column(&self.ctx)
    }

    /// The colour under this token name, if the token is a flat colour.
    fn color(&self, name: &str) -> Option<Rgba> {
        data::COLORS
            .iter()
            .chain(data::ALPHA_COLORS.iter())
            .find(|(n, _)| *n == name)
            .map(|(_, v)| Rgba::from_argb(v[self.column()]))
    }

    /// The brand ramp colour at the given tone (10..=160), if the ramp has one.
    pub fn brand_tone(tone: u16) -> Option<Rgba> {
        data::BRAND_RAMP
            .iter()
            .find(|(t, _)| *t == tone)
            .map(|(_, c)| Rgba::from_argb(*c))
    }

    /// How many tokens exist at all, for coverage accounting.
    pub fn token_count() -> usize {
        data::COLORS.len()
            + data::ALPHA_COLORS.len()
            + data::NUMBERS.len()
            + data::LENGTHS.len()
            + data::TEXTS.len()
            + data::COLOR_TOKENS_WITHOUT_VALUE.len()
    }
}

/// Which Fluent alias fills each common role. `None` means Fluent has no such concept.
const ADAPTER: &[(CommonColor, Option<&str>)] = &[
    (CommonColor::Accent, Some("colorBrandBackground")),
    (CommonColor::OnAccent, Some("colorBrandForeground1")),
    (CommonColor::AccentContainer, Some("colorBrandBackground2")),
    (
        CommonColor::OnAccentContainer,
        Some("colorBrandForeground2"),
    ),
    (CommonColor::Surface, Some("colorNeutralBackground1")),
    (CommonColor::OnSurface, Some("colorNeutralForeground1")),
    (CommonColor::SurfaceRaised, Some("colorNeutralBackground2")),
    (
        CommonColor::OnSurfaceRaised,
        Some("colorNeutralForeground2"),
    ),
    (CommonColor::TextMuted, Some("colorNeutralForeground3")),
    (
        CommonColor::TextPlaceholder,
        Some("colorNeutralForeground4"),
    ),
    (CommonColor::TextLink, Some("colorBrandForegroundLink")),
    (CommonColor::TextSelection, Some("colorBrandBackground")),
    (CommonColor::Divider, Some("colorNeutralStroke2")),
    (CommonColor::Stroke, Some("colorNeutralStroke1")),
    (
        CommonColor::SurfaceHover,
        Some("colorNeutralBackground1Hover"),
    ),
    (
        CommonColor::SurfacePressed,
        Some("colorNeutralBackground1Pressed"),
    ),
    (
        CommonColor::SurfaceSelected,
        Some("colorNeutralBackground1Selected"),
    ),
    (CommonColor::Error, Some("colorPaletteRedBackground3")),
    (CommonColor::OnError, Some("colorPaletteRedForeground1")),
    (CommonColor::Success, Some("colorPaletteGreenBackground3")),
    (
        CommonColor::Warning,
        Some("colorPaletteDarkOrangeBackground3"),
    ),
    (CommonColor::FocusRing, Some("colorStrokeFocus2")),
    // Fluent expresses scrims as RGBA overlays the component stacks; no single solid token.
    (CommonColor::Scrim, None),
];

/// Widget-facing names in this framework mapped onto Fluent's own token names.
const NATIVE_ALIAS: &[(&str, &str)] = &[
    ("accentFill", "colorBrandBackground"),
    ("onAccentFillColorPrimary", "colorNeutralForegroundOnBrand"),
    (
        "textOnAccentFillColorPrimary",
        "colorNeutralForegroundOnBrand",
    ),
    ("textFillColorPrimary", "colorNeutralForeground1"),
    ("textFillColorSecondary", "colorNeutralForeground2"),
    ("controlSolidFillColorDefault", "colorNeutralBackground1"),
    ("controlStrongFillColorDefault", "colorNeutralStroke1"),
];

impl StyleSystem for FluentSystem {
    fn id(&self) -> SystemId {
        SystemId::Fluent
    }

    fn context(&self) -> Context {
        self.ctx
    }

    fn common_colour(&self, role: CommonColor) -> Resolution<Resolved<Rgba>> {
        // WinUI's TextSelectionHighlightColorThemeBrush is accent-derived; white text on it.
        if role == CommonColor::TextSelection {
            return match self.color("colorBrandBackground") {
                Some(c) => Resolution::Value(Resolved::new(
                    c,
                    Source::Derived("winui TextSelectionHighlightColorThemeBrush"),
                )),
                None => Resolution::Unmapped,
            };
        }
        let Some(entry) = ADAPTER.iter().find(|(r, _)| *r == role) else {
            return Resolution::Absent(Reason::NotAConcept);
        };
        match entry.1 {
            None => Resolution::Absent(Reason::NotAConcept),
            Some(name) => match self.color(name) {
                Some(c) => Resolution::Value(Resolved::new(c, TOKEN_SPEC)),
                None => Resolution::Unmapped,
            },
        }
    }

    fn common_metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>> {
        let scale = self.radius();
        match metric {
            CommonMetric::Radius => match scale.points("medium") {
                Some(points) => Resolution::Value(Resolved::new(points, scale.source)),
                None => Resolution::Unmapped,
            },
            CommonMetric::StrokeWidth => Resolution::Value(Resolved::new(
                1.0,
                Source::Spec("@fluentui/tokens strokeWidthThin"),
            )),
        }
    }

    fn caps(&self) -> StyleCaps {
        StyleCaps {
            named_state_tokens: true,
            shared_palettes: true,
            radius_scale: true,
            motion_tokens: true,
            ..StyleCaps::MINIMAL
        }
    }
}

impl Palette for FluentSystem {
    fn native_colour(&self, id: &str) -> Resolution<Rgba> {
        if let Some(c) = self.color(id) {
            return Resolution::Value(c);
        }
        // A short alias table so widget-facing names land on Fluent's own tokens.
        if let Some(target) = NATIVE_ALIAS.iter().find(|(a, _)| *a == id).map(|(_, t)| *t)
            && let Some(c) = self.color(target)
        {
            return Resolution::Value(c);
        }
        // Every non-colour family is known to Fluent but has no colour here: one reason serves
        // them all, so they share one condition rather than two branches with the same body.
        let known = data::COLOR_TOKENS_WITHOUT_VALUE.contains(&id)
            || data::NUMBERS.iter().any(|(n, _)| *n == id)
            || data::LENGTHS.iter().any(|(n, _)| *n == id)
            || data::TEXTS.iter().any(|(n, _, _, _)| *n == id);
        if !known {
            return Resolution::Unmapped;
        }
        Resolution::Absent(Reason::NotAConcept)
    }

    fn provenance(&self, _id: &str) -> Source {
        TOKEN_SPEC
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{Appearance, Platform};
    use std::collections::HashSet;

    fn light() -> FluentSystem {
        FluentSystem::new(Context::new(Appearance::Light, Platform::Web))
    }

    fn dark() -> FluentSystem {
        FluentSystem::new(Context::new(Appearance::Dark, Platform::Web))
    }

    // Cross-check against the fluentui MCP's reported value; independent of our data file.
    #[test]
    fn the_default_text_colour_matches_the_published_token_value() {
        assert_eq!(
            light().native_colour("colorNeutralForeground1").value(),
            Some(Rgba::from_rgb(0x24, 0x24, 0x24))
        );
        assert_eq!(
            dark().native_colour("colorNeutralForeground1").value(),
            Some(Rgba::from_rgb(0xff, 0xff, 0xff))
        );
    }

    #[test]
    fn the_four_contrast_buckets_collapse_to_the_three_theme_columns() {
        let ctx = Context::new(Appearance::Light, Platform::Web);
        assert_eq!(theme_column(&ctx.with_contrast(Contrast::Standard)), 0);
        assert_eq!(theme_column(&ctx.with_contrast(Contrast::Low)), 0);
        assert_eq!(theme_column(&ctx.with_contrast(Contrast::High)), 2);
        let dark = Context::new(Appearance::Dark, Platform::Web).with_contrast(Contrast::High);
        assert_eq!(theme_column(&dark), 2, "high contrast outranks dark");
    }

    #[test]
    fn every_common_role_has_a_deliberate_adapter_entry() {
        let covered: HashSet<_> = ADAPTER.iter().map(|(r, _)| *r).collect();
        for role in CommonColor::ALL {
            assert!(
                covered.contains(&role),
                "{} has no adapter entry",
                role.name()
            );
        }
        assert_eq!(
            ADAPTER.len(),
            CommonColor::ALL.len(),
            "duplicate adapter entries"
        );
    }

    // The default web brand ramp's midpoint; documented as brand-80 #0f6cbd.
    #[test]
    fn the_brand_ramp_is_exposed_and_starts_and_ends_where_documented() {
        assert_eq!(
            FluentSystem::brand_tone(80),
            Some(Rgba::from_rgb(0x0f, 0x6c, 0xbd))
        );
        assert_eq!(
            FluentSystem::brand_tone(10),
            Some(Rgba::from_rgb(0x06, 0x17, 0x24))
        );
        assert_eq!(
            FluentSystem::brand_tone(160),
            Some(Rgba::from_rgb(0xeb, 0xf3, 0xfc))
        );
        assert_eq!(FluentSystem::brand_tone(15), None);
    }
}
