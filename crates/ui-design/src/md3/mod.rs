//! MD3 as a design system: 49 roles resolved by rule, not looked up in a table.

use crate::capabilities::{Geometry, Palette};
use crate::caps::StyleCaps;
use crate::common::CommonColor;
use crate::context::Context;
use crate::metric::CommonMetric;
use crate::source::Source;
use crate::system::{StyleSystem, SystemId};
use crate::value::{Reason, Resolution, Resolved, Rgba};

pub mod palette;
mod tone;

use palette::{PaletteId, Palettes, SourceColor};
use tone::{Curve, DeltaPair, foreground_tone};

/// A role's tone as the source declares it: a rule, never a constant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToneRule {
    /// `s.is_dark ? dark : light`
    Pair {
        light: f64,
        dark: f64,
    },
    Const(f64),
    /// Dark is a constant; light follows a contrast curve.
    LightCurve {
        dark: f64,
        curve: Curve,
    },
    /// Light is a constant; dark follows a contrast curve.
    DarkCurve {
        light: f64,
        curve: Curve,
    },
    BothCurve {
        light: Curve,
        dark: Curve,
    },
}

impl ToneRule {
    fn raw(&self, is_dark: bool, level: f64) -> f64 {
        match *self {
            Self::Pair { light, dark } => {
                if is_dark {
                    dark
                } else {
                    light
                }
            }
            Self::Const(v) => v,
            Self::LightCurve { dark, curve } => {
                if is_dark {
                    dark
                } else {
                    curve.get(level)
                }
            }
            Self::DarkCurve { light, curve } => {
                if is_dark {
                    curve.get(level)
                } else {
                    light
                }
            }
            Self::BothCurve { light, dark } => {
                if is_dark {
                    dark.get(level)
                } else {
                    light.get(level)
                }
            }
        }
    }
}

/// Which surface the `highestSurface` pseudo-role means in this theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    HighestSurface,
    Background,
    Primary,
    Secondary,
    Tertiary,
    Error,
    PrimaryContainer,
    SecondaryContainer,
    TertiaryContainer,
    ErrorContainer,
    InverseSurface,
    PrimaryFixed,
    SecondaryFixed,
    TertiaryFixed,
    PrimaryFixedDim,
    SecondaryFixedDim,
    TertiaryFixedDim,
}

/// One of the 49 colour roles.
#[derive(Clone, Copy, Debug)]
pub struct Role {
    pub name: &'static str,
    pub palette: PaletteId,
    pub tone: ToneRule,
    pub is_background: bool,
    pub background: Option<Background>,
    pub second_background: Option<Background>,
    pub curve: Option<[f64; 4]>,
    pub delta: Option<DeltaPair>,
}

mod data {
    include!("../../data/md3_data.rs");
}

impl Role {
    /// The declared 49 roles, by name; the table is generated from the source.
    pub fn all() -> &'static [Role] {
        data::ROLES
    }

    pub fn by_name(name: &str) -> Option<&'static Role> {
        data::ROLES.iter().find(|r| r.name == name)
    }
}

/// An MD3 scheme: five palettes plus the context it resolves under.
#[derive(Clone, Copy, Debug)]
pub struct Md3Scheme {
    pub ctx: Context,
    pub palettes: Palettes,
    pub source_tone: f64,
}

impl Md3Scheme {
    /// Build a scheme from a seed colour, the way a real app would.
    pub fn from_source(ctx: Context, seed: u32) -> Self {
        let source = SourceColor::from_argb(seed);
        Self {
            ctx,
            palettes: source.tonal_spot().palettes(),
            source_tone: source.tone,
        }
    }

    /// A TonalSpot scheme whose seed is the default M3 baseline purple.
    pub fn baseline(ctx: Context) -> Self {
        Self::from_source(ctx, 0xff67_50a4)
    }

    fn level(&self) -> f64 {
        self.ctx.contrast.md3_level()
    }

    fn is_dark(&self) -> bool {
        self.ctx.is_dark()
    }

    /// The tone a role actually resolves to, after curves and delta pairs.
    pub fn tone_of(&self, role: &Role) -> f64 {
        let level = self.level();
        let is_dark = self.is_dark();
        let mut answer = role.tone.raw(is_dark, level);

        // Case 1: a delta pair fixes both members together; the nearer member keeps the rule.
        if let Some(pair) = role.delta {
            let other_name = if pair.a == role.name { pair.b } else { pair.a };
            if let Some(other) = Role::by_name(other_name) {
                let other_tone = other.tone.raw(is_dark, level);
                // `stay_together` means both sides keep the rule, as fixed roles do.
                if pair.stay_together {
                    return answer;
                }
                let (near, far) =
                    tone::resolve_delta_pair(role.name, answer, other_tone, &pair, is_dark);
                answer = if pair.a == role.name { near } else { far };
            }
            return answer;
        }

        // Case 2: a single background with a curve. A pair member keeps its delta instead.
        if role.delta.is_none()
            && let (Some(raw), Some(bg)) = (role.curve, role.background)
        {
            let bg_tone = self.background_tone(bg);
            let want = if level < 0.0 {
                raw[0]
            } else {
                Curve(raw).get(level)
            };
            if !ratio_of_tones_ge(answer, bg_tone, want) {
                answer = foreground_tone(bg_tone, want);
            }
            if role.is_background && (50.0..60.0).contains(&answer) {
                answer = if ratio_of_tones_ge(49.0, bg_tone, want) {
                    49.0
                } else {
                    60.0
                };
            }
        }
        answer
    }

    fn background_tone(&self, bg: Background) -> f64 {
        match bg {
            // In dark themes the highest surface is the bright one, else the dim one.
            Background::HighestSurface => {
                let name = if self.is_dark() {
                    "surface_bright"
                } else {
                    "surface_dim"
                };
                Role::by_name(name).map(|r| self.tone_of(r)).unwrap_or(0.0)
            }
            other => {
                let name = background_name(other);
                Role::by_name(name).map(|r| self.tone_of(r)).unwrap_or(0.0)
            }
        }
    }

    /// The colour of a role.
    pub fn color(&self, role: &Role) -> Rgba {
        Rgba::from_argb(self.palettes.tone(role.palette, self.tone_of(role)))
    }

    /// Read a native role exactly as M3's own token chain does:
    /// `md.sys.color.<role>` -> `md.ref.palette.<palette>` at a tone -> an sRGB value.
    /// Here that is: role name -> tone rule (evaluated) -> tonal palette -> hex, printed in
    /// the same three steps so a spec table row can be followed end to end.
    pub fn explain(&self, name: &str) -> Option<String> {
        let role = Role::by_name(name)?;
        let tone = self.tone_of(role);
        let argb = self.color(role);
        let hex = format!("#{:06X}", argb.argb() & 0x00ff_ffff);
        Some(format!(
            "md.sys.color.{name} -> md.ref.palette.{} at tone {tone:.2} (rule {:?}) -> {hex}",
            role.palette.name(),
            role.tone,
        ))
    }
}

fn ratio_of_tones_ge(a: f64, b: f64, want: f64) -> bool {
    ui_color_hct::ratio_of_tones(a, b) >= want
}

fn background_name(bg: Background) -> &'static str {
    match bg {
        Background::HighestSurface => "surface_bright",
        Background::Background => "background",
        Background::Primary => "primary",
        Background::Secondary => "secondary",
        Background::Tertiary => "tertiary",
        Background::Error => "error",
        Background::PrimaryContainer => "primary_container",
        Background::SecondaryContainer => "secondary_container",
        Background::TertiaryContainer => "tertiary_container",
        Background::ErrorContainer => "error_container",
        Background::InverseSurface => "inverse_surface",
        Background::PrimaryFixed => "primary_fixed",
        Background::SecondaryFixed => "secondary_fixed",
        Background::TertiaryFixed => "tertiary_fixed",
        Background::PrimaryFixedDim => "primary_fixed_dim",
        Background::SecondaryFixedDim => "secondary_fixed_dim",
        Background::TertiaryFixedDim => "tertiary_fixed_dim",
    }
}

/// The published table every `common_colour` answer comes from.
const ROLE_SPEC: &str = "m3 color roles";

/// Which MD3 role answers each common semantic colour.
const ADAPTER: &[(CommonColor, &str)] = &[
    (CommonColor::Accent, "primary"),
    (CommonColor::OnAccent, "on_primary"),
    (CommonColor::AccentContainer, "primary_container"),
    (CommonColor::OnAccentContainer, "on_primary_container"),
    (CommonColor::Surface, "surface"),
    (CommonColor::OnSurface, "on_surface"),
    (CommonColor::SurfaceRaised, "surface_container_high"),
    (CommonColor::OnSurfaceRaised, "on_surface"),
    (CommonColor::TextMuted, "on_surface_variant"),
    (CommonColor::TextPlaceholder, "outline"),
    (CommonColor::TextLink, "primary"),
    (CommonColor::TextSelection, "primary"),
    (CommonColor::Divider, "outline_variant"),
    (CommonColor::Stroke, "outline"),
    (CommonColor::SurfaceHover, "surface_container_high"),
    (CommonColor::SurfacePressed, "surface_container_highest"),
    (CommonColor::SurfaceSelected, "secondary_container"),
    (CommonColor::Error, "error"),
    (CommonColor::OnError, "on_error"),
    (CommonColor::Success, "tertiary"),
    (CommonColor::Warning, "tertiary_container"),
    (CommonColor::FocusRing, "primary"),
    (CommonColor::Scrim, "scrim"),
];

impl StyleSystem for Md3Scheme {
    fn id(&self) -> SystemId {
        SystemId::Md3
    }

    fn context(&self) -> Context {
        self.ctx
    }

    fn common_colour(&self, role: CommonColor) -> Resolution<Resolved<Rgba>> {
        let Some((_, name)) = ADAPTER.iter().find(|(r, _)| *r == role) else {
            return Resolution::Absent(Reason::NotAConcept);
        };
        let Some(native) = Role::by_name(name) else {
            return Resolution::Unmapped;
        };
        let colour = self.color(native);
        // MD3 has no selection token; Flutter's MaterialApp default is the primary at 40%.
        if role == CommonColor::TextSelection {
            return Resolution::Value(Resolved::new(
                colour.with_alpha(0x66),
                Source::Spec("flutter TextSelectionThemeData: primary @ 40%"),
            ));
        }
        Resolution::Value(Resolved::new(colour, Source::Spec(ROLE_SPEC)))
    }

    fn common_metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>> {
        let scale = self.radius();
        match metric {
            CommonMetric::Radius => match scale.points("medium") {
                Some(points) => Resolution::Value(Resolved::new(points, scale.source)),
                None => Resolution::Unmapped,
            },
            CommonMetric::StrokeWidth => {
                Resolution::Value(Resolved::new(1.0, Source::Spec("m3 outline width")))
            }
        }
    }

    fn caps(&self) -> StyleCaps {
        StyleCaps {
            tonal_palettes: true,
            arbitrary_tones: true,
            containers: true,
            radius_scale: true,
            cut_corners: true,
            motion_tokens: true,
            ..StyleCaps::MINIMAL
        }
    }
}

impl Palette for Md3Scheme {
    fn native_colour(&self, id: &str) -> Resolution<Rgba> {
        match Role::by_name(id) {
            Some(role) => Resolution::Value(self.color(role)),
            None => Resolution::Unmapped,
        }
    }

    fn provenance(&self, _id: &str) -> Source {
        Source::Derived("material_dynamic_colors.cc + CAM16/HCT")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{Appearance, Platform};
    use std::collections::HashSet;

    fn scheme(appearance: Appearance) -> Md3Scheme {
        Md3Scheme::baseline(Context::new(appearance, Platform::Android))
    }

    fn tone(s: &Md3Scheme, role: &str) -> f64 {
        s.tone_of(Role::by_name(role).unwrap())
    }

    // The documented defaults: primary 40, onPrimary 100, light surface 98, dark surface 6.
    #[test]
    fn the_documented_default_tones_reproduce() {
        let light = scheme(Appearance::Light);
        assert!(
            (tone(&light, "primary") - 40.0).abs() < 0.01,
            "{}",
            tone(&light, "primary")
        );
        assert!((tone(&light, "on_primary") - 100.0).abs() < 0.01);
        assert!((tone(&light, "surface") - 98.0).abs() < 0.01);
        let dark = scheme(Appearance::Dark);
        assert!((tone(&dark, "primary") - 80.0).abs() < 0.01);
        assert!((tone(&dark, "on_primary") - 20.0).abs() < 0.01);
        assert!((tone(&dark, "surface") - 6.0).abs() < 0.01);
    }

    #[test]
    fn the_forty_nine_roles_are_present_and_unique() {
        let names: HashSet<_> = Role::all().iter().map(|r| r.name).collect();
        assert_eq!(Role::all().len(), 49);
        assert_eq!(names.len(), 49);
        for expected in [
            "primary",
            "on_primary",
            "surface",
            "on_surface",
            "scrim",
            "outline_variant",
        ] {
            assert!(names.contains(expected), "{expected} missing");
        }
    }
}
