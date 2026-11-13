//! The glass button: an icon (or icon and label) capsule that inflates under the finger.

use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{OverlayAlign, SizeClass};
use ui_core::{AnimSpec, CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::{Style, TextRole};
use ui_widgets::build::IMMEDIATE;
use ui_widgets::{IconSource, Widget};

use crate::HigOptions;
use crate::paint::*;
use crate::shared::*;
use crate::text::LabelText;

/// How the button's body reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassButtonStyle {
    /// The theme's `.glass` body (`glass_theme_data.dart:283-317`).
    #[default]
    Filled,
    /// The same body at `.glassProminent` strength (`glass_button.dart:893-901`).
    Prominent,
    /// No glass surface: the content sits on the backdrop, only the press lift shows.
    Transparent,
}

/// Everything a button is built with; the app owns what a press does.
pub struct GlassButtonParams<'a> {
    pub id: Id,
    pub icon: Option<IconSource>,
    pub label: &'a str,
    /// Rest width in dp; `None` measures a labelled capsule, or is the height for an icon.
    pub width: Option<f32>,
    pub height: f32,
    pub icon_size: f32,
    pub style: GlassButtonStyle,
    pub disabled: bool,
    /// Corner radius; `None` is the capsule the height implies.
    pub radius: Option<f32>,
    /// Label overrides, for a caller whose type ramp differs from `.glass`'s subtitle.
    pub label_size: Option<f32>,
    pub label_weight: Option<Weight>,
    pub label_ink: Option<Color>,
}

impl<'a> GlassButtonParams<'a> {
    /// An icon button at the reference's 56×56.
    pub fn icon(id: Id, icon: IconSource) -> Self {
        Self {
            id,
            icon: Some(icon),
            label: "",
            width: None,
            height: BUTTON_SIZE,
            icon_size: BUTTON_ICON,
            style: GlassButtonStyle::Filled,
            disabled: false,
            radius: None,
            label_size: None,
            label_weight: None,
            label_ink: None,
        }
    }

    /// A capsule around an optional icon and a label.
    pub fn labelled(id: Id, icon: Option<IconSource>, label: &'a str) -> Self {
        Self {
            id,
            icon,
            label,
            width: None,
            height: BUTTON_H,
            icon_size: BUTTON_ICON,
            style: GlassButtonStyle::Filled,
            disabled: false,
            radius: None,
            label_size: None,
            label_weight: None,
            label_ink: None,
        }
    }

    pub fn style(mut self, style: GlassButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = height;
        self
    }
}

/// One glass button; every curve advances in `new`, `decl()` is pure.
pub struct GlassButton {
    id: Id,
    /// Press inflation progress, 0..=1 — a spring, so it overshoots on release.
    press: f32,
    /// The resisted drag offset the stretch consumes, in dp.
    drag: (f32, f32),
    /// The rest box, before any press transform; a labelled capsule is measured in `decl`.
    rest: (Option<f32>, f32),
    /// The pressed surface's white lift, 0..=1.
    ambient: f32,
    icon: Option<IconSource>,
    icon_size: f32,
    ink: Color,
    label: Option<LabelText>,
    backdrop: Option<ui_core::BackdropBlur>,
    style: GlassButtonStyle,
    disabled: bool,
    /// Corner radius; `None` is the capsule the height implies.
    radius: Option<f32>,
    /// The ring's ink while the keyboard is on the button; `None` when it is not.
    focus_ring: Option<Color>,
}

impl GlassButton {
    pub fn new(
        params: GlassButtonParams<'_>,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(HigOptions::default(), params, ctx, pack)
    }

    pub fn new_with(
        _params: HigOptions,
        params: GlassButtonParams<'_>,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let id = params.id;
        let state = ctx.state();
        let ink = role_of(pack, "textFillColorPrimary");
        let label_ink = params.label_ink.unwrap_or(ink);
        let label = LabelText::new(
            pack,
            TextRole::Subtitle,
            params.label,
            label_ink,
            f32::INFINITY,
        )
        .map(|label| match (params.label_size, params.label_weight) {
            (Some(size), Some(weight)) => label.restyled(size, weight),
            (Some(size), None) => label.restyled(size, label.weight()),
            (None, Some(weight)) => label.restyled(label.size(), weight),
            (None, None) => label,
        });

        // ── Press: the button inflates by a fixed number of dp whatever its size
        // (`stretch.dart:135-144`), and a keyboard activation pulses the same way.
        // A decaying pulse, not a timestamp: the frame loop rests as soon as it ends.
        let pulse = id.with("pulse");
        if state.activated {
            ctx.animate(pulse, 1.0, IMMEDIATE);
        }
        let pulsed = ctx.animate(pulse, 0.0, crate::switch::TAP_HOLD_SPEC) > 0.0;
        let target = f32::from(state.pressed || state.drag_started || pulsed);
        let (k, c) = if target > 0.5 { PRESS_IN } else { PRESS_OUT };
        let press = ctx.spring(id.with("press"), target, k, c).x;

        // ── Drag: the finger's own travel since the press, held back by resistance and
        // carried by a spring so a flick does not tear the shape.
        let bank = id.with("bank");
        let origin = (id.with("origin-x"), id.with("origin-y"));
        let (raw_x, raw_y) = if state.pressed || state.drag_started {
            if ctx.value(bank) <= 0.5 {
                ctx.animate(bank, 1.0, IMMEDIATE);
                if let Some(l) = state.local {
                    ctx.animate(origin.0, l.x, IMMEDIATE);
                    ctx.animate(origin.1, l.y, IMMEDIATE);
                }
            }
            match state.local {
                Some(l) => (l.x - ctx.value(origin.0), l.y - ctx.value(origin.1)),
                None => (0.0, 0.0),
            }
        } else {
            ctx.animate(bank, 0.0, IMMEDIATE);
            (0.0, 0.0)
        };
        let (tx, ty) = resisted(raw_x, raw_y, BUTTON_RESISTANCE);
        let (dk, dc) = if state.pressed || state.drag_started {
            DRAG_FOLLOW
        } else {
            DRAG_RETURN
        };
        let drag = (
            ctx.spring(id.with("drag-x"), tx, dk, dc).x,
            ctx.spring(id.with("drag-y"), ty, dk, dc).x,
        );

        // ── Ambient lift: the whole surface brightens while held (`glass_button.dart:801-811`),
        // and focus or hover adds its own share on top.
        let lift = ctx.animate(
            id.with("lift"),
            target,
            if target > 0.5 { LIFT_UP } else { LIFT_DOWN },
        );
        let base = if pack.appearance() == Some(ui_design::Appearance::Dark) {
            AMBIENT_DARK
        } else {
            AMBIENT_LIGHT
        };
        let extra = if state.focus_visible {
            AMBIENT_FOCUS
        } else if state.hovered {
            AMBIENT_HOVER
        } else {
            0.0
        };
        let ambient = (lift * base + extra).clamp(0.0, 1.0);

        let control = match params.style {
            GlassButtonStyle::Filled | GlassButtonStyle::Transparent => {
                crate::material::Control::Button
            }
            GlassButtonStyle::Prominent => crate::material::Control::ButtonProminent,
        };
        let backdrop = (params.style != GlassButtonStyle::Transparent)
            .then(|| glass_backdrop(&crate::material::glass_recipe(pack, control), 1.0))
            .flatten();

        Self {
            id,
            press,
            drag,
            rest: (params.width, params.height),
            ambient,
            icon: params.icon,
            icon_size: params.icon_size,
            ink,
            label,
            backdrop,
            style: params.style,
            disabled: params.disabled || state.disabled,
            radius: params.radius,
            focus_ring: state.focus_visible.then(|| focus_ring_ink(pack)),
        }
    }

    /// The press highlight: the lift gathered into a soft spot under the finger, so a held
    /// button brightens where it is touched instead of evenly (`glass_button.dart`'s press
    /// highlight). Nothing shows without a pointer — a keyboard press has no spot.
    fn glare(
        &self,
        cx: &ui_widgets::Cx<'_, '_>,
        face: ButtonFace,
        corners: Corners,
    ) -> Option<Decl> {
        let strength = (self.press * GLARE_STRENGTH).clamp(0.0, 1.0);
        if strength < 0.01 {
            return None;
        }
        let pointer = cx.core().pointer()?;
        let rect = cx.core().rect_of(self.id)?;
        let half = GLARE_DIAMETER * 0.5;
        // The spot lives inside the inflated face, which the press shifts off the rest box;
        // without `face.x/y` the highlight would trail the finger by half the growth.
        let spot = Decl::container(self.id.with("glare"), NodeKind::Box)
            .size(SizeClass::fixed(GLARE_DIAMETER, GLARE_DIAMETER))
            .offset(
                pointer.x - rect.origin.x - face.x - half,
                pointer.y - rect.origin.y - face.y - half,
            )
            .background(
                Color {
                    a: strength,
                    ..Color::WHITE
                },
                Corners::all(half),
            )
            .effects(vec![ui_core::effect::Effect::blur(GLARE_BLUR, GLARE_SIGMA)]);
        Some(
            Decl::container(self.id.with("glare-clip"), NodeKind::Stack)
                .size(SizeClass::fixed(face.w, face.h))
                .corners(corners)
                .clip(ui_core::layout::Clip {
                    x: ui_core::layout::Overflow::Hidden,
                    y: ui_core::layout::Overflow::Hidden,
                })
                .child(spot),
        )
    }

    /// The rest box, measuring a labelled capsule when the caller gave no width.
    fn rest(&self, cx: &ui_widgets::Cx<'_, '_>) -> (f32, f32) {
        let h = self.rest.1;
        if let Some(width) = self.rest.0 {
            return (width, h);
        }
        let Some(label) = &self.label else {
            return (h, h);
        };
        let text = label.width_in(cx).unwrap_or(BUTTON_H * 2.0);
        let icon = if self.icon.is_some() {
            ui_widgets::icon_extent(self.icon_size) + BUTTON_LABEL_GAP
        } else {
            0.0
        };
        (text + icon + BUTTON_PAD * 2.0, h)
    }
}

impl Widget for GlassButton {
    fn handles_disabled(&self) -> bool {
        // The 0.5 fade is painted here; the adapter must not fade again.
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        (!self.disabled && state.clicked).then_some(ui_core::Change::Activated)
    }

    fn decl(&self, cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let rest = self.rest(cx);
        let f = button_face(rest, self.press, self.drag);
        let corners = Corners::all(self.radius.unwrap_or(f.corner));
        let scale = f.scale;

        // The press lift sits under the content, so a label or glyph stays crisp.
        let ambient = Decl::container(self.id.with("ambient"), NodeKind::Box)
            .size(SizeClass::fixed(f.w, f.h))
            .background(
                Color {
                    a: self.ambient,
                    ..Color::WHITE
                },
                corners,
            );

        let mut content = Decl::container(self.id.with("content"), NodeKind::Row)
            .gap(BUTTON_LABEL_GAP)
            .cross_align(0.0)
            .cross_pos(0.5);
        if let Some(icon) = self.icon {
            content = content.child(
                ui_widgets::Icon {
                    id: self.id.with("icon"),
                    source: icon,
                    size: self.icon_size * scale,
                    ink: self.ink,
                }
                .resolve(),
            );
        }
        if let Some(label) = &self.label {
            content = content.child(label.scaled(scale).decl(self.id.with("label")));
        }

        let mut face = Decl::container(self.id.with("face"), NodeKind::Stack)
            .offset(f.x, f.y)
            .size(SizeClass::fixed(f.w, f.h))
            .corners(corners)
            .stack_align(OverlayAlign::Center);
        if self.style != GlassButtonStyle::Transparent {
            face = face.shapes(glass_face(
                Rect::new(0.0, 0.0, f.w, f.h),
                corners,
                Color::TRANSPARENT,
                1.0,
            ));
        }
        if let Some(ink) = self.focus_ring {
            face = face.shapes(vec![focus_ring_shape(
                Rect::new(0.0, 0.0, f.w, f.h),
                corners,
                ink,
            )]);
        }
        if let Some(backdrop) = self.backdrop
            && let Some(p) = face.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }
        face = face.child(ambient);
        if let Some(glare) = self.glare(cx, f, corners) {
            face = face.child(glare);
        }
        face = face.child(content);

        let mut root = Decl::container(self.id, NodeKind::Stack)
            .size(SizeClass::fixed(rest.0, rest.1))
            .sense(Sense::HOVER | Sense::CLICK | Sense::DRAG | Sense::FOCUSABLE)
            // The ring is drawn on the face; the root still has to declare the stop.
            .focus_stop(true)
            .cursor(CursorShape::PointingHand)
            .child(face);
        if self.disabled {
            root = root.opacity(0.5);
        }
        root
    }
}

/// The button's transformed box: the press inflation, then the anchored stretch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonFace {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The capsule radius, half the shorter side.
    pub corner: f32,
    /// The uniform press scale its content follows.
    pub scale: f32,
}

/// The face at press progress `press` and drag offset `drag`, in the reference's order:
/// a uniform press scale about the centre, then an anchored stretch about a gliding pivot
/// (`stretch.dart:600-700`).
pub fn button_face(rest: (f32, f32), press: f32, drag: (f32, f32)) -> ButtonFace {
    let (w, h) = (rest.0.max(1.0), rest.1.max(1.0));
    let press_scale = 1.0 + press * BUTTON_PRESS_GROWTH / w.max(h);
    let (cx, cy) = (w * 0.5, h * 0.5);

    let (dx, dy) = (drag.0 * BUTTON_STRETCH, drag.1 * BUTTON_STRETCH);
    let ref_dim = w.max(h);
    let (rx, ry) = (dx.abs() / ref_dim, dy.abs() / ref_dim);
    let ar = w / h;
    let xd = if ar > 1.0 { 1.0 / ar } else { 1.0 };
    let yd = if ar < 1.0 { ar } else { 1.0 };
    let sx = (1.0 + (rx * BUTTON_INTENSITY * xd).clamp(0.0, 0.3))
        * (1.0 - (ry * BUTTON_SQUASH * xd).clamp(0.0, 0.15));
    let sy = (1.0 + (ry * BUTTON_INTENSITY * yd).clamp(0.0, 0.3))
        * (1.0 - (rx * BUTTON_SQUASH * yd).clamp(0.0, 0.15));
    let pivot_x = w * (0.5 - signed_smooth_step(dx, 20.0) * 0.5);
    let pivot_y = h * (0.5 - signed_smooth_step(dy, 20.0) * 0.5);
    let shift = (dx * BUTTON_DAMPING * xd, dy * BUTTON_DAMPING * yd);

    // The reference appends the stretch after the press scale, so a point travels through
    // the stretch first; mapping the two corners keeps the rect exact for either sign.
    let at = |x: f32, y: f32| {
        let (x, y) = (x + shift.0, y + shift.1);
        let (x, y) = (pivot_x + (x - pivot_x) * sx, pivot_y + (y - pivot_y) * sy);
        (cx + (x - cx) * press_scale, cy + (y - cy) * press_scale)
    };
    let (x0, y0) = at(0.0, 0.0);
    let (x1, y1) = at(w, h);
    let fw = (x1 - x0).abs();
    let fh = (y1 - y0).abs();
    ButtonFace {
        x: x0.min(x1),
        y: y0.min(y1),
        w: fw,
        h: fh,
        corner: fw.min(fh) * 0.5,
        scale: press_scale,
    }
}

/// `_signedSmoothStep` (`stretch.dart:517-524`): glide the pivot through the centre.
fn signed_smooth_step(x: f32, edge: f32) -> f32 {
    let t = (x / edge).clamp(-1.0, 1.0);
    let a = t.abs();
    let smooth = a * a * (3.0 - 2.0 * a);
    if t >= 0.0 { smooth } else { -smooth }
}

/// `OffsetResistanceExtension.withResistance` (`stretch.dart:790-800`): the further the
/// finger goes, the harder the shape holds back.
pub fn resisted(dx: f32, dy: f32, resistance: f32) -> (f32, f32) {
    let magnitude = (dx * dx + dy * dy).sqrt();
    if resistance == 0.0 || magnitude == 0.0 {
        return (dx, dy);
    }
    let held = magnitude / (1.0 + magnitude * resistance);
    let k = held / magnitude;
    (dx * k, dy * k)
}

/// The reference's default icon button (`glass_button.dart:136-138`).
pub const BUTTON_SIZE: f32 = 56.0;
pub const BUTTON_ICON: f32 = 24.0;
/// A labelled capsule's height and side padding.
pub const BUTTON_H: f32 = 44.0;
/// The press highlight's spot and its blur: wide enough to read as light, soft enough
/// that its edge never shows.
const GLARE_DIAMETER: f32 = 64.0;
const GLARE_BLUR: u32 = 16;
const GLARE_SIGMA: f32 = 5.0;
/// The spot's peak alpha at full press.
const GLARE_STRENGTH: f32 = 0.18;
pub const BUTTON_PAD: f32 = 20.0;
pub const BUTTON_LABEL_GAP: f32 = 6.0;
/// `LiquidStretch.nativePressGrowth`: the longest side grows this much, whatever the size
/// (`stretch.dart:135-144`).
pub const BUTTON_PRESS_GROWTH: f32 = 17.0;
/// `GlassDefaults.ambientBaseLight` and its dark half (`glass_defaults.dart:42-46`).
pub const AMBIENT_LIGHT: f32 = 0.3;
pub const AMBIENT_DARK: f32 = 0.14;
pub const AMBIENT_FOCUS: f32 = 0.15;
pub const AMBIENT_HOVER: f32 = 0.08;
/// `LiquidStretch` defaults, with `AnchorStretchSettings.nativeTremor` (`stretch.dart:80-87`).
pub const BUTTON_STRETCH: f32 = 0.5;
pub const BUTTON_RESISTANCE: f32 = 0.01;
pub const BUTTON_INTENSITY: f32 = 0.1;
pub const BUTTON_SQUASH: f32 = 0.1;
pub const BUTTON_DAMPING: f32 = 0.1;

/// snappy(250 ms, bounce 0.25) on the way in, bouncy(280 ms, bounce 0.45) on the way out
/// (`stretch.dart:255-264`).
pub(crate) const PRESS_IN: (f32, f32) = (631.65, 37.70);
pub(crate) const PRESS_OUT: (f32, f32) = (503.53, 24.68);
/// smooth(150 ms) while the finger is down, bouncy(500 ms, bounce 0.3) after it lifts.
const DRAG_FOLLOW: (f32, f32) = (1754.60, 83.78);
const DRAG_RETURN: (f32, f32) = (157.91, 17.59);
/// `ambientLiftDuration` 150 ms ease-out up, 60 ms down (`glass_defaults.dart:49-52`).
const LIFT_UP: AnimSpec = AnimSpec::new(0.150, ui_style::motion::CSS_EASE_OUT);
const LIFT_DOWN: AnimSpec = AnimSpec::new(0.060, ui_style::motion::CSS_EASE_OUT);
