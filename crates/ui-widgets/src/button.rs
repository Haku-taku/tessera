//! The pressed/hovered/focused control. Its look comes entirely from the pack.

use ui_core::geom::Corners;

use ui_core::{Decl, Id, NodeKind};
use ui_style::{RoleDict, RoleName, Style};

use crate::state::WidgetState;

/// The roles a button reads from its pack. A pack that lacks one draws transparent,
/// which is how a custom pack opts out of a part instead of crashing.
pub mod roles {
    pub const FILL: &str = "buttonFill";
    pub const FILL_HOVER: &str = "buttonFillHover";
    pub const FILL_PRESSED: &str = "buttonFillPressed";
    pub const FOREGROUND: &str = "buttonText";
    pub const BORDER: &str = "buttonBorder";
    pub const FOCUS: &str = "focusStroke";
    pub const FILL_DISABLED: &str = "buttonFillDisabled";
    pub const FOREGROUND_DISABLED: &str = "buttonTextDisabled";
    pub const BORDER_DISABLED: &str = "buttonBorderDisabled";
}

/// A push button: semantics in, `Decl` out, no style of its own.
pub struct Button {
    pub id: Id,
    pub label: String,
}

impl Button {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
        }
    }

    /// Resolve this frame's look from the routed state plus the fade the app advanced.
    pub fn resolve(&self, state: WidgetState, pack: &Style, hover: f32) -> Decl {
        let metrics = &pack.metrics;
        // A pack that names a design system answers these in the common vocabulary; a
        // hand-written one only has its own roles, so the legacy names stay the fallback.
        let mut fill = pick_fill(&pack.roles, state, hover).unwrap_or_else(|| {
            pack.colour_or(
                ui_design::CommonColor::Accent,
                ui_core::geom::Color::TRANSPARENT,
            )
        });
        let mut foreground = pack
            .roles
            .get(RoleName(roles::FOREGROUND))
            .unwrap_or_else(|| {
                pack.colour_or(
                    ui_design::CommonColor::OnAccent,
                    ui_core::geom::Color::TRANSPARENT,
                )
            });
        let mut border = if state.focus_visible {
            pack.roles
                .get(RoleName(roles::FOCUS))
                .unwrap_or(ui_core::geom::Color::TRANSPARENT)
        } else {
            pack.roles
                .get(RoleName(roles::BORDER))
                .unwrap_or(ui_core::geom::Color::TRANSPARENT)
        };
        // Disabled: prefer the pack's disabled roles; otherwise dim the live colours.
        if state.disabled {
            fill = disabled_role(pack, roles::FILL_DISABLED, fill);
            foreground = disabled_role(pack, roles::FOREGROUND_DISABLED, foreground);
            border = disabled_role(pack, roles::BORDER_DISABLED, border);
        }
        let token = &pack.type_table;
        let caption = token.get(ui_style::TextRole::Caption);

        Decl::container(self.id, NodeKind::Column)
            // A button is a keyboard stop that takes clicks and shows hover.
            .sense(ui_core::Sense::HOVER | ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
            .background(fill, corners(pack, state))
            .border(border, metrics.control.border)
            .padding(metrics.control.padding)
            .cursor(ui_core::CursorShape::PointingHand)
            .child(crate::text::leaf(
                self.id.with("label"),
                self.label.clone(),
                caption.size,
                foreground,
                caption.weight,
                f32::INFINITY,
                caption.line_gap,
                caption.indent,
            ))
    }
}

/// Press wins outright; hover blends the base toward the hover role by `hover`.
/// `None` when the pack names no fill at all, so the caller can ask the common plane.
fn pick_fill(dict: &RoleDict, state: WidgetState, hover: f32) -> Option<ui_core::geom::Color> {
    let base = dict.get(RoleName(roles::FILL))?;
    if state.pressed {
        return Some(dict.get(RoleName(roles::FILL_PRESSED)).unwrap_or(base));
    }
    let lifted = dict.get(RoleName(roles::FILL_HOVER)).unwrap_or(base);
    Some(base.lerp(lifted, hover))
}

/// A disabled role from the pack, or `fallback` dimmed to 38% when the pack has none.
fn disabled_role(
    pack: &Style,
    id: &'static str,
    fallback: ui_core::geom::Color,
) -> ui_core::geom::Color {
    match pack.roles.get(RoleName(id)) {
        Some(c) => c,
        None => ui_core::geom::Color {
            a: fallback.a * 0.38,
            ..fallback
        },
    }
}

fn corners(pack: &Style, state: WidgetState) -> Corners {
    let metrics = &pack.metrics;
    let radius = pack.metric_or(ui_design::CommonMetric::Radius, metrics.control.radius);
    if state.pressed {
        // Pressed flattens in Fluent and MD3 alike; the pack's own radius still rules.
        Corners::all(radius).with_shape(ui_core::geom::CornerShape::ROUND)
    } else {
        Corners::all(radius).with_shape(metrics.control.corner)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::Id;
    use ui_core::geom::Color;
    use ui_style::{MetricsSet, MotionTable, RoleDict};

    fn pack() -> Style {
        let mut roles = RoleDict::new();
        roles.set(RoleName(roles::FILL), Color::from_rgba8(60, 130, 220, 255));
        roles.set(
            RoleName(roles::FILL_HOVER),
            Color::from_rgba8(110, 175, 245, 255),
        );
        roles.set(
            RoleName(roles::FILL_PRESSED),
            Color::from_rgba8(30, 70, 140, 255),
        );
        roles.set(RoleName(roles::FOREGROUND), Color::WHITE);
        Style {
            common: None,
            roles,
            motion: MotionTable::fluent(),
            metrics: MetricsSet::NONE,
            controls: ui_style::ControlMetricsTable::EMPTY,
            type_table: ui_style::stub_type_table(),
            material: None,
            device: ui_style::Capability::GPU_BASELINE,
            palette: None,
            selection: None,
        }
    }

    fn pressed_state() -> WidgetState {
        WidgetState {
            pressed: true,
            hovered: true,
            ..Default::default()
        }
    }

    fn idle_state() -> WidgetState {
        WidgetState::default()
    }

    fn fill_of(decl: &Decl) -> Color {
        decl.fill().expect("a background")
    }

    #[test]
    fn pressing_beats_a_full_hover() {
        let p = pack();
        let b = Button::new(Id::str("pill"), "go");
        let decl = b.resolve(pressed_state(), &p, 1.0);
        assert_eq!(fill_of(&decl), Color::from_rgba8(30, 70, 140, 255));
    }

    #[test]
    fn the_hover_value_blends_instead_of_switching() {
        // A hard switch reads as a different widget; the fade must be continuous.
        let p = pack();
        let b = Button::new(Id::str("pill"), "go");
        let half = fill_of(&b.resolve(idle_state(), &p, 0.5));
        // Midpoint of the two roles' channels, in 8-bit terms: 85, 152.5, 232.5 — the
        // lerp keeps the halves, so the expectation must too.
        let want = Color::rgb(85.0 / 255.0, 152.5 / 255.0, 232.5 / 255.0);
        for (v, w) in [
            (half.r, want.r),
            (half.g, want.g),
            (half.b, want.b),
            (half.a, want.a),
        ] {
            assert!((v - w).abs() < 1e-5, "{half:?} vs {want:?}");
        }
    }
}
