//! The glass slider: a 6 dp track under a 37×24 white pill that melts into a 58×38.33 lens.

use ui_core::geom::{Color, Corners, Rect};
use ui_core::{Decl, Id, NodeKind, SizeClass};
use ui_style::{Style, TextRole};
use ui_widgets::Widget;
use ui_widgets::build::IMMEDIATE;

use crate::HigOptions;
use crate::paint::*;
use crate::shared::*;
use crate::text::LabelText;

/// One glass slider; every curve and spring advances in `new`, `decl()` is pure.
pub struct GlassSlider {
    id: Id,
    value: f32,
    /// The displayed value this frame, rubber band included; a drag commits its clamped form.
    pos: f32,
    track_active: Color,
    track_inactive: Color,
    /// The white pill and its shadow, both already faded by the melt.
    pill: Color,
    shadow: Color,
    /// How far the pill has melted into the lens, for the rim shadow it gains.
    melt: f32,
    /// The track and its filled portion, already rubber-banded and deformed.
    track: Rect,
    fill: Rect,
    /// The thumb's box and the material it carries.
    thumb: SliderThumb,
    backdrop: Option<ui_core::BackdropBlur>,
    label: Option<LabelText>,
}

impl GlassSlider {
    pub fn new(
        id: Id,
        value: f32,
        label: &str,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(HigOptions::default(), id, value, label, ctx, pack)
    }

    pub fn new_with(
        _params: HigOptions,
        id: Id,
        value: f32,
        label: &str,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let value = value.clamp(0.0, 1.0);
        let state = ctx.state();

        // ── Position: a drag carries the thumb by the finger's own travel, and the pull past
        // either end is square-rooted; a tap springs it to the value.
        let pos_id = id.with("pos");
        let shown_id = id.with("shown");
        let last_x = id.with("last-x");
        if let Some(l) = state.local {
            ctx.animate(last_x, l.x, IMMEDIATE);
        }
        let v = if state.pressed || state.drag_started {
            if ctx.value(id.with("bank")) <= 0.5 {
                ctx.animate(id.with("bank"), 1.0, IMMEDIATE);
                ctx.animate(id.with("from-v"), ctx.value(shown_id), IMMEDIATE);
                ctx.animate(id.with("from-x"), ctx.value(last_x), IMMEDIATE);
            }
            let pulled = ctx.value(last_x) - ctx.value(id.with("from-x"));
            rubber_band(ctx.value(id.with("from-v")) + pulled / slider_travel())
        } else {
            ctx.animate(id.with("bank"), 0.0, IMMEDIATE);
            if state.dropped {
                // The release frame still shows where the finger left it, and the spring is
                // re-seeded there: its stored state is pre-drag, so letting it run from that
                // would snap the thumb back before it sprang. The app has not applied the
                // value yet, so the overshoot must survive one frame.
                let shown = ctx.value(shown_id);
                ctx.forget_spring(pos_id);
                ctx.spring(pos_id, shown, POSITION_SPRING.0, POSITION_SPRING.1)
                    .x
            } else {
                ctx.spring(pos_id, value, POSITION_SPRING.0, POSITION_SPRING.1)
                    .x
            }
        };
        // What the thumb showed this frame, so the release frame and the next drag start from it.
        ctx.animate(shown_id, v, IMMEDIATE);

        // ── Expand: the pill grows into the lens on a 0.4 s spring, and back on a 0.6 s one.
        // A decaying hold, not a timestamp: the frame loop rests as soon as it ends.
        let hold = id.with("tap-hold");
        if state.activation() {
            ctx.animate(hold, 1.0, IMMEDIATE);
        }
        // A tap holds the lens open for 0.2 s before it contracts (`:747-753`).
        let tapped = ctx.animate(hold, 0.0, crate::switch::TAP_HOLD_SPEC) > 0.0;
        let target = f32::from(state.pressed || state.drag_started || tapped);
        let (k, c) = if target > 0.5 { EXPAND } else { CONTRACT };
        let t = ctx.spring(id.with("expand"), target, k, c).x;

        // ── Rubber band: the track follows the pull, and springs back in 0.3 s on release
        // (`:804-816`). A stiff spring carries the live value so both share one store.
        let offset_id = id.with("offset");
        let offset = if state.pressed || state.drag_started {
            ctx.spring(
                offset_id,
                rubber_band_offset(v),
                TRACK_FOLLOW.0,
                TRACK_FOLLOW.1,
            )
            .x
        } else {
            ctx.spring(offset_id, 0.0, TRACK_RETURN.0, TRACK_RETURN.1).x
        };

        // ── Jelly: the reference feeds a snappy spring the normalised position and keeps only
        // its velocity, so the thumb squashes along the drag and stretches across it.
        let velocity = ctx
            .spring(
                id.with("jelly"),
                v,
                THUMB_JELLY_SPRING.0,
                THUMB_JELLY_SPRING.1,
            )
            .v;
        let thumb = slider_thumb(v, t, thumb_jelly(id, velocity, ctx));
        let white = role_of(pack, "glassThumb");
        let pill = Color {
            a: white.a * (1.0 - t).clamp(0.0, 1.0),
            ..white
        };
        // The contracted thumb casts the reference's own shadow: 0.15, radius 5, no offset.
        let shadow = Color {
            a: 0.15 * (1.0 - t).clamp(0.0, 1.0),
            ..Color::BLACK
        };
        Self {
            id,
            value,
            pos: v,
            track_active: role_of(pack, "glassSliderActive"),
            track_inactive: role_of(pack, "glassSliderInactive"),
            pill,
            shadow,
            melt: t,
            track: track_rect(offset),
            fill: fill_rect(offset, thumb_centre(v)),
            thumb,
            backdrop: glass_backdrop(
                &crate::material::glass_recipe(pack, crate::material::Control::Slider),
                t.clamp(0.0, 1.0),
            ),
            label: LabelText::new(
                pack,
                TextRole::Caption,
                label,
                role_of(pack, "textFillColorPrimary"),
                SLIDER_WIDTH,
            ),
        }
    }
}

impl Widget for GlassSlider {
    fn handles_disabled(&self) -> bool {
        // The 0.5 fade is painted here; the adapter must not fade again.
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        // A tap sets the value outright; a drag follows the finger. Neither the press nor the
        // hold alone changes anything.
        if state.disabled || (!state.drag_started && !state.clicked) {
            return None;
        }
        let local = state.local?;
        // A tap names the point under the pointer; a drag commits where the thumb got to.
        let v = if state.drag_started {
            self.pos.clamp(0.0, 1.0)
        } else {
            crate::slider_value_at(local.x)
        };
        ((v - self.value).abs() > 0.0001).then_some(ui_core::Change::Value(v as f64))
    }

    fn decl(&self, cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let disabled = cx.state(self.id).disabled;
        // Both halves are pills: the reference rounds the fill to the track's own radius,
        // so a half-full slider ends in a cap rather than a squared edge.
        let t = self.track;
        let track = Decl::container(self.id.with("track"), NodeKind::Box)
            .offset(t.origin.x, t.origin.y)
            .size(SizeClass::fixed(t.size.w, t.size.h))
            .background(self.track_inactive, Corners::all(t.size.h * 0.5));
        let f = self.fill;
        let active = Decl::container(self.id.with("active"), NodeKind::Box)
            .offset(f.origin.x, f.origin.y)
            .size(SizeClass::fixed(f.size.w, f.size.h))
            .background(self.track_active, Corners::all(f.size.h * 0.5));

        let (x, y, w, h) = (self.thumb.x, self.thumb.y, self.thumb.w, self.thumb.h);
        let corners = Corners::all(self.thumb.corner);
        let shape = Rect::new(0.0, 0.0, w, h);
        let shadow = Decl::container(self.id.with("shadow"), NodeKind::Box)
            .offset(x, y)
            .size(SizeClass::fixed(w, h))
            .shapes(vec![ui_core::draw::Shape::Shadow {
                rect: shape,
                corners,
                colour: self.shadow,
                blur: 5.0,
                dy: 0.0,
                ring: 0.0,
            }]);

        // The lens drops the reference's rim shadow, which the contracted pill does not have.
        let rim = Decl::container(self.id.with("rim"), NodeKind::Box)
            .offset(x, y)
            .size(SizeClass::fixed(w, h))
            .shapes(vec![rim_shadow(0.0, 0.0, w, h, self.melt)]);

        // The glass is the whole shell; the white pill inside it fades as the glass takes over.
        let mut thumb = Decl::container(self.id.with("thumb"), NodeKind::Box)
            .background(Color::TRANSPARENT, corners)
            .size(SizeClass::fixed(w, h))
            .offset(x, y)
            .shapes(glass_face(shape, corners, self.pill, self.melt));
        if let Some(backdrop) = self.backdrop
            && let Some(p) = thumb.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }

        // The layers overlap only inside a Stack; a Box would flow them down the column.
        let stack = Decl::container(self.id.with("stack"), NodeKind::Stack)
            .size(SizeClass::fixed(SLIDER_WIDTH, SLIDER_BOX_H))
            .child(track)
            .child(active)
            .child(shadow)
            .child(rim)
            .child(thumb);
        let mut root = Decl::container(self.id, NodeKind::Column)
            .sense(
                ui_core::Sense::HOVER
                    | ui_core::Sense::CLICK
                    | ui_core::Sense::DRAG
                    | ui_core::Sense::FOCUSABLE,
            )
            .child(stack);
        if let Some(label) = &self.label {
            root = root.child(label.decl(self.id.with("label")));
        }
        if disabled {
            root = root.opacity(0.5);
        }
        root
    }
}

/// LiquidGlassKit's slider geometry (`LiquidGlassSlider.swift:150-160`).
pub const SLIDER_TRACK_H: f32 = 6.0;
pub const SLIDER_THUMB_W: f32 = 37.0;
pub const SLIDER_THUMB_H: f32 = 24.0;
/// `sliderHeight` 28: the expanded thumb overflows it, as the reference's `clipsToBounds`
pub const SLIDER_BOX_H: f32 = 28.0;
pub const SLIDER_WIDTH: f32 = 240.0;
/// The held thumb is the reference's 58×38.333 expanded size.
pub const SLIDER_EXPANDED_W: f32 = 58.0;
pub const SLIDER_EXPANDED_H: f32 = 38.333;

/// Apple's `(duration, dampingRatio)` springs, mapped by `k = (2π/T)²`, `c = 4πζ/T` — a
/// convention, since UIKit's own conversion is not published.
const EXPAND: (f32, f32) = (246.74, 18.85);
const CONTRACT: (f32, f32) = (109.66, 14.66);
/// The track's own return: 0.3 s at damping 0.8 (`:804-816`).
const TRACK_RETURN: (f32, f32) = (438.65, 33.51);
/// A stiff, slightly overdamped spring that simply carries the live pull: the track must not
const TRACK_FOLLOW: (f32, f32) = (4000.0, 160.0);

/// The value a pointer at `x` (slider-local) selects; the app owns the range.
pub fn slider_value_at(x: f32) -> f32 {
    raw_value_at(x).clamp(0.0, 1.0)
}
/// The value a pointer at `x` names, before either end clamps it.
pub fn raw_value_at(x: f32) -> f32 {
    (x - SLIDER_THUMB_W * 0.5) / slider_travel()
}
/// Travel between the thumb's two rest positions.
pub fn slider_travel() -> f32 {
    SLIDER_WIDTH - SLIDER_THUMB_W
}
/// The reference's rubber band over this slider's travel (`LiquidGlassSlider.swift:771-778`).
pub fn rubber_band(v: f32) -> f32 {
    rubber_band_at(v, slider_travel())
}
/// The rubber-band offset in dp: negative past the minimum, positive past the maximum.
pub fn rubber_band_offset(v: f32) -> f32 {
    if v < 0.0 {
        v * slider_travel()
    } else if v > 1.0 {
        (v - 1.0) * slider_travel()
    } else {
        0.0
    }
}
/// The thumb centre a value names, in dp from the slider's left edge.
pub fn thumb_centre(v: f32) -> f32 {
    SLIDER_THUMB_W * 0.5 + slider_travel() * v
}
/// The track under a rubber-band offset (`:371-386`): it slides a quarter of the pull
/// away, widens by half of it, and thins by a third — never below 2 dp.
pub fn track_rect(offset: f32) -> Rect {
    let shift = offset * 0.75 - offset.abs() * 0.25;
    let height = (SLIDER_TRACK_H - offset.abs() / 3.0).max(2.0);
    Rect::new(
        shift,
        (SLIDER_BOX_H - height) * 0.5,
        SLIDER_WIDTH + offset.abs() * 0.5,
        height,
    )
}
/// The filled portion (`:511-529`): it tracks the thumb, fades out over 10 dp at the
/// minimum end and swallows the whole track within 10 dp of the maximum.
pub fn fill_rect(offset: f32, centre: f32) -> Rect {
    let track = track_rect(offset);
    let min = SLIDER_THUMB_W * 0.5;
    let max = SLIDER_WIDTH - SLIDER_THUMB_W * 0.5;
    let k_min = ((centre - min) / 10.0).clamp(0.0, 1.0);
    let k_max = ((max - centre) / 10.0).clamp(0.0, 1.0);
    let width = (centre - track.origin.x).max(0.0) * k_min * k_max + track.size.w * (1.0 - k_max);
    Rect::new(track.origin.x, track.origin.y, width, track.size.h)
}
/// Where the value sits: the lens centre, running from half a lens to `width` minus half.
pub fn slider_thumb_x(progress: f32) -> f32 {
    thumb_centre(progress.clamp(0.0, 1.0))
}

/// The lens and its corner radius, in slider-local coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderThumb {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub corner: f32,
}

/// The lens at `progress`, melted into the expanded size by `t` and squashed by `jelly` — one
/// capsule, no material inside it: the white pill and the glass are the same shape throughout.
pub fn slider_thumb(progress: f32, t: f32, jelly: (f32, f32)) -> SliderThumb {
    let w = SLIDER_THUMB_W + (SLIDER_EXPANDED_W - SLIDER_THUMB_W) * t;
    let h = SLIDER_THUMB_H + (SLIDER_EXPANDED_H - SLIDER_THUMB_H) * t;
    // The lens grows about its centre, which is the point the value names; a rubber-banded
    // value puts that centre past the rest range, as the reference lets it.
    let centre = thumb_centre(progress);
    let r = jelly_capsule(
        Rect::new(centre - w * 0.5, (SLIDER_BOX_H - h) * 0.5, w, h),
        jelly,
    );
    SliderThumb {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.w,
        h: r.size.h,
        corner: r.size.h * 0.5,
    }
}
