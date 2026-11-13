//! HIG as a design system: fixed system colours, derived semantic roles, host-resolved names.

use crate::capabilities::{Geometry, Palette};
use crate::caps::StyleCaps;
use crate::common::CommonColor;
use crate::context::{Appearance, Context, Contrast, Platform};
use crate::metric::CommonMetric;
use crate::source::Source;
use crate::system::{MaterialSpec, StyleSystem, SystemId};
use crate::value::{Reason, Resolution, Resolved, Rgba};

mod data {
    include!("../data/hig_data.rs");
}

/// The accent is the published system blue; only the *choice* of it as `tintColor` is ours.
const ACCENT_SPEC: Source = Source::Spec("hig systemBlue as tintColor");

/// The four published contexts Apple ships for every system colour.
fn column(ctx: &Context) -> usize {
    let increased = ctx.contrast == Contrast::High;
    match (ctx.appearance, increased) {
        (Appearance::Light, false) => 0,
        (Appearance::Dark, false) => 1,
        (Appearance::Light, true) => 2,
        (Appearance::Dark, true) => 3,
    }
}

/// Apple HIG, with the semantic layer we derive ourselves because Apple publishes no values.
#[derive(Clone, Copy, Debug)]
pub struct HigSystem {
    pub ctx: Context,
}

impl HigSystem {
    pub const fn new(ctx: Context) -> Self {
        Self { ctx }
    }

    /// A published system colour by name, in the current context.
    pub fn system_color(&self, name: &str) -> Option<Rgba> {
        data::SYSTEM_COLORS
            .iter()
            .chain(data::GRAY_RAMP.iter())
            .chain(data::DISABLED_COLORS.iter())
            .chain(data::FILL_COLORS.iter())
            .find(|(n, _)| *n == name)
            .map(|(_, v)| Rgba::from_argb(v[column(&self.ctx)]))
    }

    /// A system colour name from HIG (`systemBlue`) in SwiftUI spelling (`blue`).
    pub fn swiftui_name(name: &str) -> Option<&'static str> {
        const PAIRS: &[(&str, &str)] = &[
            ("systemRed", "red"),
            ("systemOrange", "orange"),
            ("systemYellow", "yellow"),
            ("systemGreen", "green"),
            ("systemMint", "mint"),
            ("systemTeal", "teal"),
            ("systemCyan", "cyan"),
            ("systemBlue", "blue"),
            ("systemIndigo", "indigo"),
            ("systemPurple", "purple"),
            ("systemPink", "pink"),
            ("systemBrown", "brown"),
            ("systemGray", "gray"),
        ];
        PAIRS.iter().find(|(a, _)| *a == name).map(|(_, b)| *b)
    }

    /// The hit-target minimum in points for this context's platform.
    pub fn hit_target(&self) -> (f32, f32) {
        let key = match self.ctx.platform {
            Platform::Ios | Platform::Ipados => "ios_ipados",
            Platform::Macos => "macos",
            Platform::Android => "ios_ipados",
            Platform::Web | Platform::Desktop => "macos",
        };
        data::HIT_TARGETS
            .iter()
            .find(|(p, _, _)| *p == key)
            .map(|(_, default, minimum)| (*default, *minimum))
            .unwrap_or((44.0, 28.0))
    }

    /// Pick the accent colour: the `tintColor` role has no published value, so we use the
    /// system blue, which is what `tintColor` resolves to on a stock system.
    fn accent(&self) -> Rgba {
        self.system_color("systemBlue")
            .unwrap_or(Rgba::from_rgb(0, 122, 255))
    }

    /// A published system colour with its provenance, or our neutral step when it is missing.
    fn system_ink(&self, name: &str, fallback: Rgba) -> Resolved<Rgba> {
        match self.system_color(name) {
            Some(c) => Resolved::new(c, Source::Spec("hig system colors")),
            None => Resolved::new(fallback, Source::Derived("neutral tone fallback")),
        }
    }
}

/// Our own derived neutral steps, since Apple publishes only system colours.
fn neutral(tone: f64) -> Rgba {
    Rgba::from_argb(ui_color_hct::argb_from_tone(tone))
}

impl StyleSystem for HigSystem {
    fn id(&self) -> SystemId {
        SystemId::Hig
    }

    fn context(&self) -> Context {
        self.ctx
    }

    fn common_colour(&self, role: CommonColor) -> Resolution<Resolved<Rgba>> {
        let dark = self.ctx.is_dark();
        let ink = if dark { 92.0 } else { 12.0 };
        let base = if dark { 0.0 } else { 100.0 };
        let raised = if dark { 9.0 } else { 98.0 };
        // Apple publishes no colour-role table: every value here is our synthesis.
        let derived = |c| Resolved::new(c, Source::Derived("hig accent + neutral tone"));
        match role {
            CommonColor::Accent | CommonColor::TextLink | CommonColor::FocusRing => {
                Resolution::Value(Resolved::new(self.accent(), ACCENT_SPEC))
            }
            CommonColor::OnAccent | CommonColor::OnError => {
                Resolution::Value(derived(Rgba::from_rgb(0xff, 0xff, 0xff)))
            }
            CommonColor::AccentContainer => {
                Resolution::Value(derived(self.accent().with_alpha(0x2e)))
            }
            CommonColor::OnAccentContainer => Resolution::Value(derived(neutral(ink))),
            CommonColor::Surface => Resolution::Value(derived(neutral(base))),
            CommonColor::OnSurface | CommonColor::OnSurfaceRaised => {
                Resolution::Value(derived(neutral(ink)))
            }
            CommonColor::SurfaceRaised => Resolution::Value(derived(neutral(raised))),
            CommonColor::TextMuted => {
                Resolution::Value(derived(neutral(if dark { 62.0 } else { 44.0 })))
            }
            CommonColor::TextPlaceholder => {
                Resolution::Value(derived(neutral(if dark { 46.0 } else { 62.0 })))
            }
            CommonColor::Divider => {
                Resolution::Value(derived(neutral(if dark { 22.0 } else { 88.0 })))
            }
            CommonColor::Stroke => {
                Resolution::Value(derived(neutral(if dark { 32.0 } else { 78.0 })))
            }
            CommonColor::SurfaceHover => {
                Resolution::Value(derived(neutral(if dark { 14.0 } else { 95.0 })))
            }
            CommonColor::SurfacePressed => {
                Resolution::Value(derived(neutral(if dark { 18.0 } else { 92.0 })))
            }
            CommonColor::SurfaceSelected => {
                Resolution::Value(derived(self.accent().with_alpha(0x24)))
            }
            // Apple publishes no selection colour: the tint at 25% is our own reading of
            // the system's look, so the source says "convention", not "spec".
            CommonColor::TextSelection => Resolution::Value(Resolved::new(
                self.accent().with_alpha(0x40),
                Source::Convention("tintColor wash behind selected text"),
            )),
            CommonColor::Error => Resolution::Value(self.system_ink("systemRed", neutral(50.0))),
            CommonColor::Success => {
                Resolution::Value(self.system_ink("systemGreen", neutral(50.0)))
            }
            CommonColor::Warning => {
                Resolution::Value(self.system_ink("systemOrange", neutral(50.0)))
            }
            // A scrim is a live blur plus dim behind a sheet, never a flat colour.
            CommonColor::Scrim => Resolution::Absent(Reason::PlatformResolved("sheet backdrop")),
        }
    }

    fn common_metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>> {
        let scale = self.radius();
        match metric {
            CommonMetric::Radius => match scale.points("medium") {
                Some(points) => Resolution::Value(Resolved::new(points, scale.source)),
                None => Resolution::Unmapped,
            },
            // iOS separators are a hairline: 1px at any scale, i.e. 0.5pt at @2x.
            CommonMetric::StrokeWidth => Resolution::Value(Resolved::new(
                0.5,
                Source::Convention("ios hairline separator"),
            )),
        }
    }

    fn caps(&self) -> StyleCaps {
        StyleCaps {
            elevated_depth: true,
            materials: true,
            glass: true,
            ..StyleCaps::MINIMAL
        }
    }

    fn material(&self, name: &str) -> Resolution<Resolved<MaterialSpec>> {
        let Some((_, sigma, tints, fallbacks)) =
            data::MATERIAL_RECIPES.iter().find(|(n, ..)| *n == name)
        else {
            return Resolution::Unmapped;
        };
        let at = column(&self.ctx);
        Resolution::Value(Resolved::new(
            MaterialSpec {
                sigma: *sigma,
                tint: Rgba(tints.get(at).copied().unwrap_or(tints[0])),
                fallback: Rgba(fallbacks.get(at).copied().unwrap_or(fallbacks[0])),
            },
            Source::Convention("our derivation; Apple publishes no value"),
        ))
    }
}

impl Palette for HigSystem {
    fn native_colour(&self, id: &str) -> Resolution<Rgba> {
        // Published system colours are values; semantic roles and materials are names only.
        if data::SYSTEM_COLORS.iter().any(|(n, _)| *n == id)
            || data::GRAY_RAMP.iter().any(|(n, _)| *n == id)
            || data::DISABLED_COLORS.iter().any(|(n, _)| *n == id)
            || data::FILL_COLORS.iter().any(|(n, _)| *n == id)
        {
            return match self.system_color(id) {
                Some(c) => Resolution::Value(c),
                None => Resolution::Unmapped,
            };
        }
        if data::MATERIALS.contains(&id) {
            return Resolution::Absent(Reason::PlatformResolved(material_name(id)));
        }
        if data::SEMANTIC_ROLES.contains(&id) {
            return Resolution::Absent(Reason::PlatformResolved(semantic_name(id)));
        }
        Resolution::Unmapped
    }

    fn provenance(&self, id: &str) -> Source {
        let published = data::SYSTEM_COLORS.iter().any(|(n, _)| *n == id)
            || data::GRAY_RAMP.iter().any(|(n, _)| *n == id)
            || data::DISABLED_COLORS.iter().any(|(n, _)| *n == id)
            || data::FILL_COLORS.iter().any(|(n, _)| *n == id);
        if published {
            Source::Spec("Apple HIG system colours")
        } else if data::MATERIALS.contains(&id) {
            Source::Spec("Apple HIG materials")
        } else {
            Source::Convention("our derivation; Apple publishes no value")
        }
    }
}

/// Apple's materials are known by name but resolved by the system; keep the name readable.
fn material_name(name: &str) -> &'static str {
    match name {
        "ultraThinMaterial" => ".ultraThinMaterial",
        "thinMaterial" => ".thinMaterial",
        "regularMaterial" => ".regularMaterial",
        "thickMaterial" => ".thickMaterial",
        "ultraThickMaterial" => ".ultraThickMaterial",
        _ => ".bar",
    }
}

fn semantic_name(name: &str) -> &'static str {
    match name {
        "label" => "Color(.label)",
        "secondaryLabel" => "Color(.secondaryLabel)",
        "tertiaryLabel" => "Color(.tertiaryLabel)",
        "quaternaryLabel" => "Color(.quaternaryLabel)",
        "systemBackground" => "Color(.systemBackground)",
        "secondarySystemBackground" => "Color(.secondarySystemBackground)",
        "tertiarySystemBackground" => "Color(.tertiarySystemBackground)",
        "systemGroupedBackground" => "Color(.systemGroupedBackground)",
        "separator" => "Color(.separator)",
        "opaqueSeparator" => "Color(.opaqueSeparator)",
        "systemFill" => "Color(.systemFill)",
        "secondarySystemFill" => "Color(.secondarySystemFill)",
        "tertiarySystemFill" => "Color(.tertiarySystemFill)",
        "quaternarySystemFill" => "Color(.quaternarySystemFill)",
        _ => "Color(.tintColor)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light() -> HigSystem {
        HigSystem::new(Context::new(Appearance::Light, Platform::Ios))
    }

    fn dark() -> HigSystem {
        HigSystem::new(Context::new(Appearance::Dark, Platform::Ios))
    }

    fn increased(appearance: Appearance) -> HigSystem {
        HigSystem::new(Context::new(appearance, Platform::Ios).with_contrast(Contrast::High))
    }

    // Increased contrast is a distinct published value, not a computed tweak.
    #[test]
    fn increased_contrast_switches_to_its_own_published_value() {
        assert_eq!(
            increased(Appearance::Light)
                .system_color("systemBlue")
                .map(Rgba::argb),
            Some(0xff1e_6ef4)
        );
        assert_eq!(
            increased(Appearance::Light)
                .system_color("systemRed")
                .map(Rgba::argb),
            Some(0xffe9_152d)
        );
        assert_ne!(
            increased(Appearance::Light).system_color("systemBlue"),
            light().system_color("systemBlue")
        );
    }

    // HIG's contrast floor: body text must clear 4.5:1, and small text should aim at 7:1.
    #[test]
    fn text_pairs_clear_four_and_a_half_to_one_in_every_context() {
        for ctx in [
            light(),
            dark(),
            increased(Appearance::Light),
            increased(Appearance::Dark),
        ] {
            for (fg, bg) in [
                (CommonColor::OnSurface, CommonColor::Surface),
                (CommonColor::OnSurfaceRaised, CommonColor::SurfaceRaised),
            ] {
                let f = ctx.common_colour(fg).value().unwrap();
                let b = ctx.common_colour(bg).value().unwrap();
                let ratio = ui_color_hct::ratio_of_tones(
                    ui_color_hct::tone_of(f.value.argb()),
                    ui_color_hct::tone_of(b.value.argb()),
                );
                assert!(ratio >= 4.5, "{ctx:?} {fg:?}/{bg:?} = {ratio:.2}");
            }
        }
    }

    // Thickness ordering is the whole material contract ("thicker = more opaque"): a table
    // edit that flattens it makes every surface pick the wrong thickness. `bar` is a member
    // of the ramp, not an end of it.
    #[test]
    fn content_materials_thicken_in_the_published_order() {
        let ramp = [
            "ultraThinMaterial",
            "thinMaterial",
            "regularMaterial",
            "thickMaterial",
            "ultraThickMaterial",
        ];
        for system in [
            light(),
            dark(),
            increased(Appearance::Light),
            increased(Appearance::Dark),
        ] {
            let spec = |name: &str| system.material(name).value().expect(name).value;
            let alpha = |name: &str| spec(name).tint.argb() >> 24;
            for pair in ramp.windows(2) {
                assert!(
                    alpha(pair[1]) > alpha(pair[0]),
                    "{} is not thicker than {} in {system:?}",
                    pair[1],
                    pair[0]
                );
            }
            let bar = alpha("bar");
            assert!(bar > alpha("regularMaterial") && bar < alpha("ultraThickMaterial"));
            for name in ramp.iter().chain(["bar"].iter()) {
                let one = spec(name);
                assert!(one.sigma > 0.0, "{name} must blur");
                assert_eq!(
                    one.fallback.argb() >> 24,
                    255,
                    "{name} needs an opaque fallback"
                );
            }
        }
    }
}
