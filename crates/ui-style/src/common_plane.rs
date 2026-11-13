//! The cross-system plane: every common token resolved once, each carrying its source.

use std::sync::Arc;

use ui_core::geom::{Color, lerp};
use ui_design::{
    CommonColor, CommonMetric, Context, Resolution, Resolved, Rgba, StyleCaps, StyleSystem,
    SystemId,
};

/// A snapshot of one system's answers to the closed common lists. Refusals stay refusals:
/// a role the system cannot fill is `Absent`/`Unmapped`, never a made-up colour.
#[derive(Clone)]
pub struct CommonPlane {
    system: SystemId,
    context: Context,
    source: Arc<dyn StyleSystem>,
    colours: [Resolution<Resolved<Color>>; CommonColor::COUNT],
    metrics: [Resolution<Resolved<f32>>; CommonMetric::COUNT],
    caps: StyleCaps,
}

impl CommonPlane {
    /// Resolve every common token once. Cheap enough to do per scope, not per widget.
    pub fn resolve(source: Arc<dyn StyleSystem>) -> Self {
        let mut colours = [Resolution::Unmapped; CommonColor::COUNT];
        for (slot, role) in colours.iter_mut().zip(CommonColor::ALL) {
            *slot = source.common_colour(role).map(to_color);
        }
        let mut metrics = [Resolution::Unmapped; CommonMetric::COUNT];
        for (slot, metric) in metrics.iter_mut().zip(CommonMetric::ALL) {
            *slot = source.common_metric(metric);
        }
        Self {
            system: source.id(),
            context: source.context(),
            caps: source.caps(),
            source,
            colours,
            metrics,
        }
    }

    pub fn system(&self) -> SystemId {
        self.system
    }

    pub fn context(&self) -> Context {
        self.context
    }

    pub fn caps(&self) -> StyleCaps {
        self.caps
    }

    /// The system itself, which is also the entry to its own vocabulary.
    pub fn source(&self) -> &Arc<dyn StyleSystem> {
        &self.source
    }

    pub fn colour(&self, role: CommonColor) -> Resolution<Resolved<Color>> {
        self.colours[role_index(role)]
    }

    pub fn metric(&self, metric: CommonMetric) -> Resolution<Resolved<f32>> {
        self.metrics[metric_index(metric)]
    }

    /// A standard content material by name, answered by the system this plane was resolved
    /// from. Live rather than snapshotted: a pack asks for it at build time, not per frame.
    pub fn material(&self, name: &str) -> Resolution<Resolved<ui_design::MaterialSpec>> {
        self.source.material(name)
    }

    /// Blend two planes for a theme transition. The endpoints return the endpoint snapshots
    /// unchanged, so `t = 0`/`1` is exactly the from/to style.
    pub fn lerp(from: &Self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.0 {
            return from.clone();
        }
        if t >= 1.0 {
            return to.clone();
        }
        Self {
            system: to.system,
            context: to.context,
            caps: to.caps,
            source: to.source.clone(),
            colours: std::array::from_fn(|i| blend_colour(from.colours[i], to.colours[i], t)),
            metrics: std::array::from_fn(|i| blend_metric(from.metrics[i], to.metrics[i], t)),
        }
    }
}

// The Arc is how the values were obtained, not part of them: two snapshots resolved separately
// from the same system and context are equal.
impl PartialEq for CommonPlane {
    fn eq(&self, other: &Self) -> bool {
        self.system == other.system
            && self.context == other.context
            && self.colours == other.colours
            && self.metrics == other.metrics
            && self.caps == other.caps
    }
}

impl core::fmt::Debug for CommonPlane {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CommonPlane")
            .field("system", &self.system)
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}

fn role_index(role: CommonColor) -> usize {
    CommonColor::ALL
        .iter()
        .position(|r| *r == role)
        .expect("CommonColor::ALL lists every role")
}

fn metric_index(metric: CommonMetric) -> usize {
    CommonMetric::ALL
        .iter()
        .position(|m| *m == metric)
        .expect("CommonMetric::ALL lists every metric")
}

fn to_color(r: Resolved<Rgba>) -> Resolved<Color> {
    let argb = r.value.argb();
    Resolved::new(
        Color::from_rgba8(
            (argb >> 16) as u8,
            (argb >> 8) as u8,
            argb as u8,
            (argb >> 24) as u8,
        ),
        r.source,
    )
}

/// A blend is nobody's published value, so it says so.
const BLEND_SOURCE: ui_design::Source = ui_design::Source::Derived("theme transition blend");

fn blend_colour(
    a: Resolution<Resolved<Color>>,
    b: Resolution<Resolved<Color>>,
    t: f32,
) -> Resolution<Resolved<Color>> {
    match (a.value(), b.value()) {
        (Some(x), Some(y)) => {
            Resolution::Value(Resolved::new(x.value.lerp(y.value, t), BLEND_SOURCE))
        }
        (Some(x), None) => Resolution::Value(x),
        (None, Some(y)) => Resolution::Value(y),
        (None, None) => b,
    }
}

fn blend_metric(
    a: Resolution<Resolved<f32>>,
    b: Resolution<Resolved<f32>>,
    t: f32,
) -> Resolution<Resolved<f32>> {
    match (a.value(), b.value()) {
        (Some(x), Some(y)) => {
            Resolution::Value(Resolved::new(lerp(x.value, y.value, t), BLEND_SOURCE))
        }
        (Some(x), None) => Resolution::Value(x),
        (None, Some(y)) => Resolution::Value(y),
        (None, None) => b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_design::{Appearance, HigSystem, Md3Scheme, Platform, Reason};

    fn md3(appearance: Appearance) -> Arc<dyn StyleSystem> {
        Arc::new(Md3Scheme::baseline(Context::new(
            appearance,
            Platform::Android,
        )))
    }

    fn hig(appearance: Appearance) -> Arc<dyn StyleSystem> {
        Arc::new(HigSystem::new(Context::new(appearance, Platform::Ios)))
    }

    // The snapshot must copy the system's answer verbatim, source included; re-deriving it
    // here would let the two drift.
    #[test]
    fn the_snapshot_returns_the_systems_own_answer_and_source() {
        let system = md3(Appearance::Light);
        let plane = CommonPlane::resolve(system.clone());
        for role in CommonColor::ALL {
            let answer = system
                .common_colour(role)
                .value()
                .unwrap_or_else(|| panic!("{role:?}"));
            let snapshot = plane
                .colour(role)
                .value()
                .unwrap_or_else(|| panic!("{role:?}"));
            assert_eq!(snapshot.value, to_color(answer).value, "{role:?}");
            assert_eq!(snapshot.source, answer.source, "{role:?}");
        }
    }

    // HIG resolves a scrim at the compositor, not as a colour; a snapshot must not invent one.
    #[test]
    fn a_refusal_survives_the_snapshot() {
        let plane = CommonPlane::resolve(hig(Appearance::Light));
        assert_eq!(
            plane.colour(CommonColor::Scrim),
            Resolution::Absent(Reason::PlatformResolved("sheet backdrop"))
        );
    }
}
