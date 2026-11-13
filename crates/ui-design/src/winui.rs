//! WinUI 3 as a design system: the XAML `ThemeResource` graph, resolved in place.
//!
//! The table is generated from the pinned XAML corpus; every name a WinUI template
//! references resolves here — brushes chain to colours, colours to literals.

use crate::capabilities::{Palette, WithDimensions};
use crate::common::CommonColor;
use crate::context::{Context, Contrast};
use crate::dimension::DimensionTokens;
use crate::metric::CommonMetric;
use crate::source::Source;
use crate::system::{StyleSystem, SystemId};
use crate::value::{Reason, Resolution, Resolved, Rgba};

mod data {
    include!("../data/winui_data.rs");
}

/// The XAML corpus every value below comes from; the research doc pins the same SHA.
pub const XAML_SOURCE: Source =
    Source::Spec("microsoft/microsoft-ui-xaml@868fef9 controls/dev/*/*_themeresources.xaml");

/// WinUI's accent palette: the OS generates six shades from the user's accent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accent {
    pub base: Rgba,
    pub light1: Rgba,
    pub light2: Rgba,
    pub light3: Rgba,
    pub dark1: Rgba,
    pub dark2: Rgba,
    pub dark3: Rgba,
}

impl Accent {
    /// Windows' default blue and its published shade set; an app may replace all seven.
    pub const WINDOWS_DEFAULT: Self = Self {
        base: Rgba::from_argb(0xFF00_78D4),
        light1: Rgba::from_argb(0xFF5B_ADFF),
        light2: Rgba::from_argb(0xFF8C_CDFF),
        light3: Rgba::from_argb(0xFFC7_E0F4),
        dark1: Rgba::from_argb(0xFF00_5A9E),
        dark2: Rgba::from_argb(0xFF00_4578),
        dark3: Rgba::from_argb(0xFF00_2F54),
    };

    fn named(&self, name: &str) -> Option<Rgba> {
        Some(match name {
            "SystemAccentColor" => self.base,
            "SystemAccentColorLight1" => self.light1,
            "SystemAccentColorLight2" => self.light2,
            "SystemAccentColorLight3" => self.light3,
            "SystemAccentColorDark1" => self.dark1,
            "SystemAccentColorDark2" => self.dark2,
            "SystemAccentColorDark3" => self.dark3,
            _ => return None,
        })
    }
}

/// One resolved brush: a flat colour or the two-stop-plus gradient a template asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Color(Rgba),
    Gradient(GradientBrush),
}

/// A linear gradient in the control's own rect, with WinUI's stop offsets kept.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientBrush {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub relative: bool,
    /// Mirrored across the box's mid-line; WinUI's light theme flips its elevation borders.
    pub flip: bool,
    pub stops: Vec<(f32, Rgba)>,
}

/// The WinUI vocabulary for one context: the whole `ThemeResource` table, plus an accent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinUISystem {
    pub ctx: Context,
    pub accent: Accent,
}

impl WinUISystem {
    pub const fn new(ctx: Context) -> Self {
        Self {
            ctx,
            accent: Accent::WINDOWS_DEFAULT,
        }
    }

    pub const fn with_accent(ctx: Context, accent: Accent) -> Self {
        Self { ctx, accent }
    }

    /// Which column of the table this context reads; `None` in high contrast, where WinUI
    /// hands every colour to the system and only names survive.
    fn column(&self) -> Option<usize> {
        if self.ctx.contrast == Contrast::High {
            return None;
        }
        Some(match self.ctx.appearance {
            crate::context::Appearance::Light => 0,
            crate::context::Appearance::Dark => 1,
        })
    }

    fn entry(name: &str) -> Option<&'static data::Entry> {
        data::RESOURCES
            .binary_search_by(|e| e.name.cmp(name))
            .ok()
            .map(|i| &data::RESOURCES[i])
    }

    /// The brush or colour under `name`, following `StaticResource` chains.
    pub fn paint(&self, name: &str) -> Option<Paint> {
        let column = self.column()?;
        self.paint_at(name, column, 0)
    }

    fn paint_at(&self, name: &str, column: usize, depth: u8) -> Option<Paint> {
        if depth > 8 {
            return None;
        }
        if let Some(c) = self.accent.named(name) {
            return Some(Paint::Color(c));
        }
        match &Self::entry(name)?.def {
            data::Def::Paint(targets, opacities) => {
                let target = targets[column];
                let opacity = opacities[column];
                if target == "@system" {
                    return None;
                }
                if let Some(lit) = target.strip_prefix('#') {
                    let argb = u32::from_str_radix(lit, 16).ok()?;
                    return Some(Paint::Color(scale_alpha(Rgba(argb), opacity)));
                }
                match self.paint_at(target, column, depth + 1)? {
                    Paint::Color(c) => Some(Paint::Color(scale_alpha(c, opacity))),
                    Paint::Gradient(mut g) => {
                        for stop in &mut g.stops {
                            stop.1 = scale_alpha(stop.1, opacity);
                        }
                        Some(Paint::Gradient(g))
                    }
                }
            }
            data::Def::Gradient {
                start,
                end,
                relative,
                flip,
                stops,
            } => {
                let mut out = Vec::with_capacity(stops[column].len());
                for (offset, target) in stops[column] {
                    let c = if let Some(lit) = target.strip_prefix('#') {
                        Rgba(u32::from_str_radix(lit, 16).ok()?)
                    } else {
                        match self.paint_at(target, column, depth + 1)? {
                            Paint::Color(c) => c,
                            Paint::Gradient(_) => return None,
                        }
                    };
                    out.push((*offset, c));
                }
                Some(Paint::Gradient(GradientBrush {
                    start: *start,
                    end: *end,
                    relative: *relative,
                    flip: flip[column],
                    stops: out,
                }))
            }
            data::Def::Thickness(_) => None,
        }
    }

    /// A colour under `name`; `None` when the name is a gradient, a thickness or absent.
    pub fn colour(&self, name: &str) -> Option<Rgba> {
        match self.paint(name)? {
            Paint::Color(c) => Some(c),
            Paint::Gradient(_) => None,
        }
    }

    /// A named thickness, in points; XAML writes them as one, two or four numbers.
    pub fn thickness(&self, name: &str) -> Option<ui_core::geom::Insets> {
        let column = self.column()?;
        match &Self::entry(name)?.def {
            data::Def::Thickness(per_theme) => {
                let [l, t, r, b] = per_theme[column];
                Some(ui_core::geom::Insets {
                    left: l,
                    top: t,
                    right: r,
                    bottom: b,
                })
            }
            _ => None,
        }
    }

    /// Whether the table knows this name at all, whatever its kind.
    pub fn knows(name: &str) -> bool {
        Self::entry(name).is_some()
    }

    /// Every name in the table, for coverage sweeps.
    pub fn names() -> impl Iterator<Item = &'static str> {
        data::RESOURCES.iter().map(|e| e.name)
    }
}

fn scale_alpha(c: Rgba, opacity: f32) -> Rgba {
    if opacity >= 1.0 {
        return c;
    }
    let a = (c.alpha() as f32 * opacity).round().clamp(0.0, 255.0) as u8;
    c.with_alpha(a)
}

impl Palette for WinUISystem {
    fn native_colour(&self, id: &str) -> Resolution<Rgba> {
        if !Self::knows(id) && self.accent.named(id).is_none() {
            return Resolution::Unmapped;
        }
        match self.paint(id) {
            Some(Paint::Color(c)) => Resolution::Value(c),
            Some(Paint::Gradient(_)) => {
                Resolution::Absent(Reason::PlatformResolved("a gradient brush; ask paint()"))
            }
            None => Resolution::Absent(Reason::PlatformResolved("SystemColor* in high contrast")),
        }
    }

    fn native_brush(&self, id: &str) -> Option<Paint> {
        self.paint(id).filter(|p| matches!(p, Paint::Gradient(_)))
    }

    fn provenance(&self, id: &str) -> Source {
        if Self::knows(id) {
            XAML_SOURCE
        } else {
            Source::Platform("accent palette, generated by the OS")
        }
    }
}

impl WithDimensions for WinUISystem {
    fn dimensions(&self) -> DimensionTokens {
        // WinUI shares Fluent 2's radius ramp for now; its per-control metrics live in
        // the widget crate, where each template's own numbers are transcribed.
        crate::scales::dimensions_of(SystemId::Fluent, self.ctx)
    }
}

impl crate::capabilities::Geometry for WinUISystem {}
impl crate::capabilities::Typography for WinUISystem {}
impl crate::capabilities::Motion for WinUISystem {}

/// Each common role's WinUI brush. `None` where WinUI has no such concept.
const ADAPTER: &[(CommonColor, Option<&str>)] = &[
    (CommonColor::Accent, Some("AccentFillColorDefaultBrush")),
    (
        CommonColor::OnAccent,
        Some("TextOnAccentFillColorPrimaryBrush"),
    ),
    (CommonColor::AccentContainer, None),
    (CommonColor::OnAccentContainer, None),
    (
        CommonColor::Surface,
        Some("ApplicationPageBackgroundThemeBrush"),
    ),
    (CommonColor::OnSurface, Some("TextFillColorPrimaryBrush")),
    (
        CommonColor::SurfaceRaised,
        Some("CardBackgroundFillColorDefaultBrush"),
    ),
    (
        CommonColor::OnSurfaceRaised,
        Some("TextFillColorPrimaryBrush"),
    ),
    (CommonColor::TextMuted, Some("TextFillColorSecondaryBrush")),
    (
        CommonColor::TextPlaceholder,
        Some("TextFillColorTertiaryBrush"),
    ),
    (
        CommonColor::TextLink,
        Some("AccentTextFillColorPrimaryBrush"),
    ),
    (
        CommonColor::TextSelection,
        Some("TextControlSelectionHighlightColor"),
    ),
    (CommonColor::Divider, Some("DividerStrokeColorDefaultBrush")),
    (CommonColor::Stroke, Some("ControlStrokeColorDefaultBrush")),
    (
        CommonColor::SurfaceHover,
        Some("ControlFillColorSecondaryBrush"),
    ),
    (
        CommonColor::SurfacePressed,
        Some("ControlFillColorTertiaryBrush"),
    ),
    (
        CommonColor::SurfaceSelected,
        Some("ControlFillColorTertiaryBrush"),
    ),
    (CommonColor::Error, Some("SystemFillColorCriticalBrush")),
    (CommonColor::OnError, None),
    (CommonColor::Success, Some("SystemFillColorSuccessBrush")),
    (CommonColor::Warning, Some("SystemFillColorCautionBrush")),
    (CommonColor::FocusRing, Some("FocusStrokeColorOuterBrush")),
    (CommonColor::Scrim, Some("SmokeFillColorDefaultBrush")),
];

impl StyleSystem for WinUISystem {
    fn id(&self) -> SystemId {
        SystemId::Fluent
    }

    fn context(&self) -> Context {
        self.ctx
    }

    fn common_colour(&self, role: CommonColor) -> Resolution<Resolved<Rgba>> {
        let Some((_, name)) = ADAPTER.iter().find(|(r, _)| *r == role) else {
            return Resolution::Unmapped;
        };
        let Some(name) = name else {
            return Resolution::Absent(Reason::NotAConcept);
        };
        match self.colour(name) {
            Some(c) => Resolution::Value(Resolved::new(c, XAML_SOURCE)),
            None if self.column().is_none() => {
                Resolution::Absent(Reason::PlatformResolved("high contrast"))
            }
            None => Resolution::Unmapped,
        }
    }

    fn common_metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>> {
        match metric {
            CommonMetric::Radius => match self.thickness("ControlCornerRadius") {
                Some(t) => Resolution::Value(Resolved::new(t.top, XAML_SOURCE)),
                None => Resolution::Unmapped,
            },
            CommonMetric::StrokeWidth => Resolution::Value(Resolved::new(1.0, XAML_SOURCE)),
        }
    }

    fn caps(&self) -> crate::StyleCaps {
        crate::StyleCaps {
            named_state_tokens: true,
            ..crate::StyleCaps::MINIMAL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{Appearance, Platform};

    fn ctx(appearance: Appearance) -> Context {
        Context::new(appearance, Platform::Desktop)
    }

    fn light() -> WinUISystem {
        WinUISystem::new(ctx(Appearance::Light))
    }

    fn dark() -> WinUISystem {
        WinUISystem::new(ctx(Appearance::Dark))
    }

    #[test]
    fn a_brush_chains_through_to_its_theme_colour() {
        // `ButtonBackground` is a StaticResource of `ControlFillColorDefaultBrush`, which is a
        // StaticResource of `ControlFillColorDefault`: the template name must land on the colour.
        assert_eq!(light().colour("ButtonBackground"), Some(Rgba(0xB3FF_FFFF)));
        assert_eq!(dark().colour("ButtonBackground"), Some(Rgba(0x0FFF_FFFF)));
    }
}
