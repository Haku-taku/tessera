//! One swappable style: colours, motion, shape, and the capability floor it resolves under.

use crate::capability::Capability;
use crate::common_plane::CommonPlane;
use crate::control_metrics::ControlMetricsTable;
use crate::material::Material;
use crate::motion::MotionTable;
use crate::roles::RoleDict;
use crate::roles::RoleName;
use crate::types::{MetricsSet, TypeTable, WidgetMetrics};
use std::sync::Arc;

/// A flat all-default table; a real pack fills every role with its own numbers.
pub fn stub_type_table() -> TypeTable {
    let base = crate::types::TextStyleToken {
        size: 13.0,
        weight: ui_core::Weight::REGULAR,
        line_gap: 0.0,
        indent: 0.0,
    };
    TypeTable::new([base; 5])
}

/// A trait-object palette in a `Style`, made `Debug`/`PartialEq` so the pack stays comparable.
#[derive(Clone)]
pub struct PaletteHandle(pub Arc<dyn ui_design::Palette>);

impl core::fmt::Debug for PaletteHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("<palette>")
    }
}

impl PartialEq for PaletteHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A widget resolves through exactly one of these per frame; an ancestor may override.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// The cross-system plane: every common token, resolved once for this system and context.
    /// `None` for a hand-written pack that names no design system at all (`legacy_pack`).
    pub common: Option<CommonPlane>,
    pub roles: RoleDict,
    pub motion: MotionTable,
    /// Corner radius, curvature family, border and padding per depth tier. The only
    /// shape source: a second radius table would drift against this one.
    pub metrics: MetricsSet,
    /// Per-control envelope: floor, padding, outline, focus reach. Tier metrics cannot hold a
    /// checkbox's 20 pt box next to a button's 11 pt inset.
    pub controls: ControlMetricsTable,
    /// Named text styles, resolved from the roles at pack build time.
    pub type_table: TypeTable,
    /// The default surface material for panels in this style; `None` means solid fill.
    pub material: Option<Material>,
    /// What the backend under this app can actually do.
    pub device: Capability,
    /// The live native vocabulary this pack was built from, so a token the curated `roles`
    /// list forgot can still resolve instead of painting transparent.
    pub palette: Option<PaletteHandle>,
    /// How this style draws a text selection; `None` means the engine's flat rect.
    pub selection: Option<ui_core::SelectionPainterRef>,
}

impl Style {
    /// Which design system this snapshot came from, if any.
    pub fn system(&self) -> Option<ui_design::SystemId> {
        self.common.as_ref().map(|p| p.system())
    }

    /// The appearance the pack was resolved for; `None` when it names no system.
    pub fn appearance(&self) -> Option<ui_design::Appearance> {
        self.common.as_ref().map(|p| p.context().appearance)
    }

    /// What the system can do beyond the common layer.
    pub fn caps(&self) -> ui_design::StyleCaps {
        self.common
            .as_ref()
            .map(|p| p.caps())
            .unwrap_or(ui_design::StyleCaps::MINIMAL)
    }

    /// A cross-system role. A pack that names no system has no concept of one.
    pub fn colour(
        &self,
        role: ui_design::CommonColor,
    ) -> ui_design::Resolution<ui_design::Resolved<ui_core::geom::Color>> {
        match &self.common {
            Some(plane) => plane.colour(role),
            None => ui_design::Resolution::Absent(ui_design::Reason::NotAConcept),
        }
    }

    /// A cross-system metric, in points.
    pub fn metric(
        &self,
        metric: ui_design::CommonMetric,
    ) -> ui_design::Resolution<ui_design::Resolved<f32>> {
        match &self.common {
            Some(plane) => plane.metric(metric),
            None => ui_design::Resolution::Absent(ui_design::Reason::NotAConcept),
        }
    }

    /// A common colour, or `fallback` when this pack has no such concept (a hand-written pack).
    pub fn colour_or(
        &self,
        role: ui_design::CommonColor,
        fallback: ui_core::geom::Color,
    ) -> ui_core::geom::Color {
        self.colour(role)
            .value()
            .map(|r| r.value)
            .unwrap_or(fallback)
    }

    /// A common metric, or `fallback` when this pack has no such concept.
    pub fn metric_or(&self, metric: ui_design::CommonMetric, fallback: f32) -> f32 {
        self.metric(metric)
            .value()
            .map(|r| r.value)
            .unwrap_or(fallback)
    }

    /// A cross-system role as the colour of a surface that has to hide what it covers.
    ///
    /// WinUI expresses a raised surface as a tint over the page — its card fill is white at
    /// five percent — which reads as coloured glass when it covers text rather than the
    /// page. A menu or a dialog composites the role over `Surface`, which is what the
    /// system's own compositor does, so the result is opaque whatever the pack's alpha.
    pub fn opaque_or(
        &self,
        role: ui_design::CommonColor,
        fallback: ui_core::geom::Color,
    ) -> ui_core::geom::Color {
        let base = self.colour_or(ui_design::CommonColor::Surface, fallback);
        self.colour_or(role, base).over(base)
    }

    /// What the pack may draw given what the backend can do.
    pub fn frost_allowed(&self, device: Capability) -> bool {
        device.supports_frost()
    }

    /// A standard content material by name (`regularMaterial`), as an engine surface.
    ///
    /// `None` when the pack names no design system or the system has no such material;
    /// a surface then keeps its opaque fill.
    pub fn material_named(&self, name: &str) -> Option<Material> {
        let plane = self.common.as_ref()?;
        let spec = plane.material(name).value()?;
        Some(Material::from_spec(spec.value))
    }

    /// The brush under `id` when the system paints more than a flat colour, e.g. WinUI's
    /// elevation border; `None` means the flat colour is the whole brush.
    pub fn brush(&self, id: &'static str) -> Option<ui_design::Paint> {
        self.palette.as_ref()?.0.native_brush(id)
    }

    /// A colour for `id`: curated role first, then the live native palette, else `None`.
    ///
    /// Returning `None` — never `TRANSPARENT` — makes a forgotten token a visible bug, not a
    /// silently blank pixel. Widgets that must draw something pick their own honest fallback.
    pub fn ink(&self, id: &'static str) -> Option<ui_core::geom::Color> {
        if let Some(c) = self.roles.get(crate::roles::RoleName(id)) {
            return Some(c);
        }
        let argb = self.palette.as_ref()?.0.native_colour(id).value()?.argb();
        Some(ui_core::geom::Color::from_rgba8(
            (argb >> 16) as u8,
            (argb >> 8) as u8,
            argb as u8,
            (argb >> 24) as u8,
        ))
    }
}

/// The one place a `Style` is assembled, so its two planes always name the same system.
pub struct StyleBuilder {
    system: Arc<dyn ui_design::StyleSystem>,
    roles: RoleDict,
    motion: MotionTable,
    metrics: MetricsSet,
    controls: ControlMetricsTable,
    type_table: TypeTable,
    material: Option<Material>,
    device: Capability,
    selection: Option<ui_core::SelectionPainterRef>,
}

impl StyleBuilder {
    /// Start from a system; every other field has a documented default the pack overrides.
    pub fn new(system: Arc<dyn ui_design::StyleSystem>) -> Self {
        Self {
            system,
            roles: RoleDict::new(),
            motion: MotionTable::legacy(),
            metrics: MetricsSet::NONE,
            controls: ControlMetricsTable::EMPTY,
            type_table: stub_type_table(),
            material: None,
            device: Capability::GPU_BASELINE,
            selection: None,
        }
    }

    /// How this style draws a text selection; a pack sets its own, an app may override it.
    pub fn selection_painter(mut self, painter: ui_core::SelectionPainterRef) -> Self {
        self.selection = Some(painter);
        self
    }

    /// A colour the pack computed itself, e.g. a component-composed wash.
    pub fn role(mut self, role: &'static str, colour: ui_core::geom::Color) -> Self {
        self.roles.set(RoleName(role), colour);
        self
    }

    /// Write role `role` from the system's own token `native`. A system that does not know a
    /// token it was told to use fails the build; it never becomes a transparent pixel.
    pub fn role_from(mut self, role: &'static str, native: &str) -> Self {
        let rgba = self
            .system
            .native_colour(native)
            .value()
            .unwrap_or_else(|| panic!("{} has no native token {native}", self.system.id().name()));
        self.roles.set(RoleName(role), colour_of_argb(rgba.argb()));
        self
    }

    /// The same through the common vocabulary, for a role the system only answers in translation.
    pub fn role_common(mut self, role: &'static str, common: ui_design::CommonColor) -> Self {
        let rgba = self
            .system
            .common_colour(common)
            .value()
            .unwrap_or_else(|| panic!("{} has no common role {common:?}", self.system.id().name()));
        self.roles
            .set(RoleName(role), colour_of_argb(rgba.value.argb()));
        self
    }

    /// Derive the three depth tiers from one radius step of the system's own scale.
    pub fn metrics_from_scale(mut self, step: &str) -> Self {
        let scale = self.system.radius();
        let radius = scale
            .points(step)
            .unwrap_or_else(|| panic!("{} has no radius step {step}", self.system.id().name()));
        let control = WidgetMetrics {
            radius,
            corner: scale.curvature.corner_shape(),
            border: 1.0,
            padding: ui_core::geom::Insets {
                left: 8.0,
                right: 8.0,
                top: 4.0,
                bottom: 4.0,
            },
            elevation: 0,
        };
        // Floating panels sit above the surface; controls stay flat.
        let overlay = WidgetMetrics {
            elevation: 3,
            ..control
        };
        self.metrics = MetricsSet {
            control,
            surface: control,
            overlay,
        };
        self
    }

    /// Replace the derived metrics wholesale, for a pack that shapes its depths differently.
    pub fn metrics(mut self, metrics: MetricsSet) -> Self {
        self.metrics = metrics;
        self
    }

    /// Replace the per-control envelope table, for a pack that sizes controls its own way.
    pub fn controls(mut self, controls: ControlMetricsTable) -> Self {
        self.controls = controls;
        self
    }

    pub fn motion(mut self, motion: MotionTable) -> Self {
        self.motion = motion;
        self
    }

    pub fn type_table(mut self, type_table: TypeTable) -> Self {
        self.type_table = type_table;
        self
    }

    pub fn material(mut self, material: Material) -> Self {
        self.material = Some(material);
        self
    }

    pub fn device(mut self, device: Capability) -> Self {
        self.device = device;
        self
    }

    /// Hand the one `Arc` to both planes and produce the snapshot.
    pub fn build(self) -> Style {
        // A system with a content-material family supplies the default surface: Apple's
        // `regularMaterial` is the one its guidance calls the content-layer default. An
        // explicit builder material always wins.
        let material = self.material.or_else(|| {
            self.system
                .caps()
                .materials
                .then(|| self.system.material("regularMaterial").value())
                .flatten()
                .map(|spec| Material::from_spec(spec.value))
        });
        Style {
            common: Some(CommonPlane::resolve(self.system.clone())),
            roles: self.roles,
            motion: self.motion,
            metrics: self.metrics,
            controls: self.controls,
            type_table: self.type_table,
            material,
            device: self.device,
            palette: Some(PaletteHandle(self.system)),
            selection: self.selection,
        }
    }
}

/// One colour blended between two palettes at the same `t`, so a palette-only token fades too.
struct BlendedPalette {
    from: Arc<dyn ui_design::Palette>,
    to: Arc<dyn ui_design::Palette>,
    t: f32,
}

impl ui_design::Palette for BlendedPalette {
    fn native_colour(&self, id: &str) -> ui_design::Resolution<ui_design::Rgba> {
        use ui_design::Resolution;
        match (
            self.from.native_colour(id).value(),
            self.to.native_colour(id).value(),
        ) {
            (Some(a), Some(b)) => Resolution::Value(blend_rgba(a, b, self.t)),
            (Some(a), None) => Resolution::Value(a),
            (None, Some(b)) => Resolution::Value(b),
            (None, None) => self.to.native_colour(id),
        }
    }

    fn native_brush(&self, id: &str) -> Option<ui_design::Paint> {
        use ui_design::{GradientBrush, Paint};
        // Geometry snaps to the target; only the stop colours blend, as `native_colour` does.
        match (self.from.native_brush(id), self.to.native_brush(id)) {
            (Some(Paint::Gradient(a)), Some(Paint::Gradient(b)))
                if a.stops.len() == b.stops.len() =>
            {
                let stops = b
                    .stops
                    .iter()
                    .zip(&a.stops)
                    .map(|((off, to), (_, from))| (*off, blend_rgba(*from, *to, self.t)))
                    .collect();
                Some(Paint::Gradient(GradientBrush { stops, ..b }))
            }
            (_, Some(brush)) => Some(brush),
            (Some(brush), None) => Some(brush),
            (None, None) => None,
        }
    }

    fn provenance(&self, id: &str) -> ui_design::Source {
        self.to.provenance(id)
    }
}

fn colour_of_argb(argb: u32) -> ui_core::geom::Color {
    ui_core::geom::Color::from_rgba8(
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    )
}

fn rgba_of_colour(c: ui_core::geom::Color) -> ui_design::Rgba {
    let q = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u32;
    ui_design::Rgba((q(c.a) << 24) | (q(c.r) << 16) | (q(c.g) << 8) | q(c.b))
}

/// Blend two palette colours at `t`, rounded back to the palette's 8-bit channels.
///
/// `ui_design::Palette` is vendor data in `Rgba`, so a palette-only token quantizes where a
/// role's f32 blend does not; a theme fade can therefore differ by one 8-bit step between
/// the two paths. Nothing compares them, and the endpoints stay exact.
fn blend_rgba(a: ui_design::Rgba, b: ui_design::Rgba, t: f32) -> ui_design::Rgba {
    let ca = colour_of_argb(a.argb());
    let cb = colour_of_argb(b.argb());
    rgba_of_colour(ca.lerp(cb, t))
}

/// Blend two packs into one at `t` (0 = `from`, 1 = `to`). Colours and lengths interpolate;
/// motion, metrics, type and material switch wholesale at the midpoint.
pub fn lerp_style(from: &Style, to: &Style, t: f32) -> Style {
    let t = t.clamp(0.0, 1.0);
    // The endpoints are the packets themselves, so a finished transition leaves no residue.
    if t <= 0.0 {
        return from.clone();
    }
    if t >= 1.0 {
        return to.clone();
    }
    // A half-way duration or sigma is a value neither end ever meant; switch instead.
    let side = if t < 0.5 { from } else { to };
    let mut roles = RoleDict::new();
    for name in from.roles.names().chain(to.roles.names()) {
        if let Some(c) = match (from.ink(name.0), to.ink(name.0)) {
            (Some(a), Some(b)) => Some(a.lerp(b, t)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        } {
            roles.set(name, c);
        }
    }
    let palette = match (&from.palette, &to.palette) {
        (Some(pf), Some(pt)) => Some(PaletteHandle(Arc::new(BlendedPalette {
            from: pf.0.clone(),
            to: pt.0.clone(),
            t,
        }))),
        (Some(p), None) | (None, Some(p)) => Some(p.clone()),
        (None, None) => None,
    };
    let common = match (&from.common, &to.common) {
        (Some(a), Some(b)) => Some(CommonPlane::lerp(a, b, t)),
        (Some(p), None) | (None, Some(p)) => Some(p.clone()),
        (None, None) => None,
    };
    Style {
        common,
        roles,
        motion: side.motion.clone(),
        metrics: side.metrics,
        controls: side.controls.clone(),
        type_table: side.type_table.clone(),
        material: side.material,
        device: to.device,
        palette,
        selection: to.selection,
    }
}

impl Style {
    /// The style for `scope`, resolved directly; the memoized entry point is `StyleCache::get`.
    pub fn resolve(
        scope: crate::env::StyleScope,
        resolver: fn(ui_design::Appearance) -> Style,
    ) -> Self {
        let scope = scope.settled();
        match scope.theme_from {
            Some(f) => lerp_style(&resolver(f), &resolver(scope.appearance), scope.theme_t),
            None => resolver(scope.appearance),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::geom::Color;
    use ui_design::{Rgba, StyleSystem};

    fn pack_with(roles: &[(&'static str, Color)], palette: Option<PaletteHandle>) -> Style {
        let mut dict = RoleDict::new();
        for (n, c) in roles {
            dict.set(crate::roles::RoleName(n), *c);
        }
        Style {
            common: None,
            roles: dict,
            motion: MotionTable::legacy(),
            metrics: MetricsSet::NONE,
            controls: ControlMetricsTable::EMPTY,
            type_table: stub_type_table(),
            material: None,
            device: Capability::GPU_BASELINE,
            palette,
            selection: None,
        }
    }

    #[test]
    fn non_colour_tokens_switch_at_the_midpoint_while_colours_keep_moving() {
        let a = pack_with(&[("primary", Color::from_rgba8(0, 0, 0, 255))], None);
        let mut b = pack_with(&[("primary", Color::from_rgba8(200, 100, 40, 255))], None);
        b.metrics.control.radius = 7.0;
        let before = lerp_style(&a, &b, 0.49);
        let after = lerp_style(&a, &b, 0.5);
        assert_eq!(before.metrics.control.radius, a.metrics.control.radius);
        assert_eq!(after.metrics.control.radius, b.metrics.control.radius);
        let (cb, ca) = (
            before.ink("primary").unwrap(),
            after.ink("primary").unwrap(),
        );
        assert!(cb.r < ca.r && cb.r > 0.0, "{cb:?} -> {ca:?}");
    }

    // A system that answers both vocabularies, so the builder's wiring is observable.
    struct Both;

    impl ui_design::StyleSystem for Both {
        fn id(&self) -> ui_design::SystemId {
            ui_design::SystemId::Md3
        }
        fn context(&self) -> ui_design::Context {
            ui_design::Context::new(ui_design::Appearance::Light, ui_design::Platform::Android)
        }
        fn common_colour(
            &self,
            role: ui_design::CommonColor,
        ) -> ui_design::Resolution<ui_design::Resolved<Rgba>> {
            match role {
                ui_design::CommonColor::Accent => {
                    ui_design::Resolution::Value(ui_design::Resolved::new(
                        Rgba(0xff11_2233),
                        ui_design::Source::Spec("test spec"),
                    ))
                }
                _ => ui_design::Resolution::Unmapped,
            }
        }
        fn common_metric(
            &self,
            _metric: ui_design::CommonMetric,
        ) -> ui_design::Resolution<ui_design::Resolved<f32>> {
            ui_design::Resolution::Unmapped
        }
        fn caps(&self) -> ui_design::StyleCaps {
            ui_design::StyleCaps::MINIMAL
        }
    }

    impl ui_design::Palette for Both {
        fn native_colour(&self, id: &str) -> ui_design::Resolution<Rgba> {
            match id {
                "primaryContainer" => ui_design::Resolution::Value(Rgba(0xff44_5566)),
                _ => ui_design::Resolution::Unmapped,
            }
        }
        fn provenance(&self, _id: &str) -> ui_design::Source {
            ui_design::Source::Convention("test")
        }
    }

    impl ui_design::WithDimensions for Both {
        fn dimensions(&self) -> ui_design::DimensionTokens {
            ui_design::dimensions_of(ui_design::SystemId::Md3, self.context())
        }
    }

    impl ui_design::Geometry for Both {}
    impl ui_design::Typography for Both {}
    impl ui_design::Motion for Both {}

    #[test]
    #[should_panic(expected = "MD3 has no native token nope")]
    fn a_token_the_system_does_not_know_fails_the_build() {
        let _ = StyleBuilder::new(Arc::new(Both))
            .role_from("x", "nope")
            .build();
    }
}
