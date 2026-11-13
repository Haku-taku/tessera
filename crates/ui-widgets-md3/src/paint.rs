//! Token lookup and blending shared by every MD3 control.

use ui_core::geom::Color;
use ui_style::Style;

pub(crate) use ui_core::geom::lerp;

/// `Color::lerp_exact` in call form, so a blend nested inside another stays readable.
pub(crate) fn between(a: Color, b: Color, t: f32) -> Color {
    a.lerp_exact(b, t)
}

/// `Color::faded` in call form: one factor dims every ink a control owns.
pub(crate) fn faded(c: Color, k: f32) -> Color {
    c.faded(k)
}

/// One role resolved against the live pack; a miss is a bug, shown as magenta.
pub(crate) fn role(pack: &Style, name: &'static str) -> Color {
    let c = pack.ink(name);
    debug_assert!(c.is_some(), "neither the pack nor its palette knows {name}");
    c.unwrap_or(Color::from_rgba8(255, 0, 255, 255))
}

/// One token slot resolved against the live pack; an empty name means "paint nothing".
pub(crate) fn spec_ink(pack: &Style, token: &'static str, opacity: f32) -> Color {
    if token.is_empty() {
        return Color::TRANSPARENT;
    }
    role(pack, token).faded(opacity)
}
