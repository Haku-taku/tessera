//! The liquid morph: a trigger ghost and a travelling droplet fused by a metaball neck.
//!
//! Ported from `liquid_morph_physics.dart` (the state) and `SheetMorphGeometry` (the blob).

use ui_core::draw::BlendShape;
use ui_core::geom::{Corners, Rect, lerp};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Easing, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::Widget;

use crate::HigOptions;
use crate::shared::*;

/// Everything the reference derives for one frame of the morph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphState {
    /// Droplet travel, 0..1 but overshoots while opening.
    pub path_t: f32,
    /// Droplet size, 0..1 on its own slower curve; negative on the close bounce.
    pub size_t: f32,
    /// Displacement of the trigger ghost while the spring bounces past closed.
    pub push_dx: f32,
    pub push_dy: f32,
    /// The trigger ghost's scale: gone by 40% so the droplet can detach.
    pub anchor_scale: f32,
    /// Metaball neck strength in dp.
    pub blend: f32,
    /// Squeeze pulse on the droplet during the close bounce.
    pub container_scale: f32,
}

/// The open spring: mass 1, stiffness 120, damping 16 (`liquid_morph_physics.dart:184-188`).
const OPEN_SPRING: (f32, f32) = (120.0, 16.0);
/// The close spring: stiffness 200, damping 21 — the same 0.74 ratio, faster (`:196-200`).
const CLOSE_SPRING: (f32, f32) = (200.0, 21.0);
/// `closeVelocityHint`: the pull that maximises the rubber-band catch (`:206`).
pub const CLOSE_VELOCITY_HINT: f32 = -3.2;

const BACK_OUT_AMPLITUDE: f32 = 2.5;
const MIN_BACK_OUT_AMPLITUDE: f32 = 0.7;
const ADAPTIVE_TRAVEL_DP: f32 = 100.0;
const ADAPTIVE_SCALE_DELTA: f32 = 1.5;
const ANCHOR_EASE: f32 = 0.40;
const BLEND_GAIN: f32 = 150.0;
const BLEND_MAX: f32 = 28.0;
const CLOSE_PROXIMITY: f32 = 0.6;
const MAX_PUSH_TRAVEL: f32 = 80.0;
const MAX_ANCHOR_DRIFT: f32 = 8.0;

/// `Curves.linearToEaseOut` (`flutter/lib/src/animation/curves.dart:1639`).
/// `Curves.linearToEaseOut`(`flutter/lib/src/animation/curves.dart:1639`).
const LINEAR_TO_EASE_OUT: Easing = Easing::Cubic(0.35, 0.91, 0.33, 0.97);
/// `Curves.easeOut`, which the closing re-merge ramp rides (`:470`).
const EASE_OUT: Easing = ui_style::motion::CSS_EASE_OUT;

/// `_BackOutCurve(amplitude)`: the overshooting travel curve.
pub fn back_out(t: f32, amplitude: f32) -> f32 {
    let t = t - 1.0;
    t * t * ((amplitude + 1.0) * t + amplitude) + 1.0
}

/// `_smootherStep`: the size curve for large morphs (`:531-534`).
pub fn smoother_step(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// `computeScaleDelta`: √(target area / source area), 1 for a zero-area source (`:316-327`).
pub fn compute_scale_delta(source: (f32, f32), target: (f32, f32)) -> f32 {
    let source_area = source.0 * source.1;
    if source_area <= 0.0 {
        return 1.0;
    }
    ((target.0 * target.1) / source_area).sqrt()
}

/// `computeAdaptiveBackOutAmplitude`: damp the J-curve by vertical travel (`:340-352`).
pub fn adaptive_amplitude(final_dy: f32) -> f32 {
    let dy = final_dy.abs();
    if dy <= ADAPTIVE_TRAVEL_DP {
        return BACK_OUT_AMPLITUDE;
    }
    (BACK_OUT_AMPLITUDE * ADAPTIVE_TRAVEL_DP / dy).clamp(MIN_BACK_OUT_AMPLITUDE, BACK_OUT_AMPLITUDE)
}

/// `computeBlendAttenuation`: keep the neck from swallowing a huge morph (`:362-365`).
pub fn blend_attenuation(scale_delta: f32) -> f32 {
    if scale_delta <= 3.0 {
        return 1.0;
    }
    (3.0 / scale_delta).clamp(0.2, 1.0)
}

/// One frame of the morph (`liquid_morph_physics.dart:360-515`).
pub fn compute(
    raw: f32,
    final_dx: f32,
    final_dy: f32,
    scale_delta: f32,
    is_closing: bool,
) -> MorphState {
    let clamped = raw.clamp(0.0, 1.0);
    // A spring that went past closed bounces the droplet beyond the anchor.
    let undershoot = if raw < 0.0 { raw } else { 0.0 };
    let adaptive = scale_delta > ADAPTIVE_SCALE_DELTA;
    let amplitude = if adaptive {
        adaptive_amplitude(final_dy)
    } else {
        BACK_OUT_AMPLITUDE
    };
    let path_t = if is_closing {
        clamped
    } else {
        back_out(clamped, amplitude)
    } + undershoot;
    let raw_size = if is_closing {
        clamped.powf(1.4)
    } else if adaptive && scale_delta > 3.0 {
        smoother_step(clamped)
    } else {
        LINEAR_TO_EASE_OUT.apply(clamped)
    };
    let size_t = raw_size + undershoot;
    let travel = (final_dx * final_dx + final_dy * final_dy).sqrt();
    let push_factor = if adaptive && travel > 0.0 {
        (MAX_PUSH_TRAVEL / travel).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let bounce = if raw < 0.0 { raw } else { 0.0 };
    let blend = if is_closing {
        let proximity = (1.0 - clamped / CLOSE_PROXIMITY).clamp(0.0, 1.0);
        (EASE_OUT.apply(proximity) * BLEND_MAX * blend_attenuation(scale_delta))
            .clamp(0.0, BLEND_MAX)
    } else {
        ((path_t - size_t).abs() * BLEND_GAIN * blend_attenuation(scale_delta))
            .clamp(0.0, BLEND_MAX)
    };
    MorphState {
        path_t,
        size_t,
        push_dx: final_dx * bounce * push_factor,
        push_dy: final_dy * bounce * push_factor,
        anchor_scale: if is_closing {
            (1.0 - (clamped - 0.45) / 0.40).clamp(0.0, 1.0)
        } else {
            (1.0 - clamped / ANCHOR_EASE).clamp(0.0, 1.0)
        },
        blend,
        container_scale: if raw > 1.0 {
            1.0 + (raw - 1.0) * 0.10
        } else if raw < 0.0 {
            1.0 + raw * 0.55
        } else {
            1.0
        },
    }
}

/// The frame of the travelling droplet (`SheetMorphGeometry.blobRect`, `:686-760`).
///
/// Size rides `size_t` while the centre rides `size_t` too, with the drift between the two
/// curves clamped to 8 dp: that is what roots the blob at the trigger instead of flying it.
pub fn blob_rect(trigger: Rect, destination: Rect, s: MorphState) -> Rect {
    let (w, h) = if s.size_t <= 0.0 {
        let squeeze = (1.0 + s.size_t * 0.25).clamp(0.5, 1.0);
        (trigger.size.w * squeeze, trigger.size.h * squeeze)
    } else {
        (
            lerp(trigger.size.w, destination.size.w, s.size_t),
            lerp(trigger.size.h, destination.size.h, s.size_t),
        )
    };
    let dx = destination.center().x - trigger.center().x;
    let dy = destination.center().y - trigger.center().y;
    let drift_x = (dx * (s.path_t - s.size_t)).clamp(-MAX_ANCHOR_DRIFT, MAX_ANCHOR_DRIFT);
    let drift_y = (dy * (s.path_t - s.size_t)).clamp(-MAX_ANCHOR_DRIFT, MAX_ANCHOR_DRIFT);
    let cx = trigger.center().x + dx * s.size_t + drift_x;
    let cy = trigger.center().y + dy * s.size_t + drift_y;
    scale_about(
        Rect::new(cx - w * 0.5, cy - h * 0.5, w, h),
        s.container_scale,
    )
}

/// The trigger ghost: the trigger rect, nudged by the closing bounce and scaled down (`:1830-1845`).
pub fn anchor_rect(trigger: Rect, s: MorphState) -> Rect {
    scale_about(
        Rect::new(
            trigger.origin.x + s.push_dx,
            trigger.origin.y + s.push_dy,
            trigger.size.w,
            trigger.size.h,
        ),
        s.anchor_scale,
    )
}

/// `blobRadius`: round through the detach, then relax into the destination's corners (`:770-780`).
pub fn blob_radius(width: f32, height: f32, target: f32, size_t: f32) -> f32 {
    let max_radius = width.min(height) * 0.5;
    let s = size_t.clamp(0.0, 1.0);
    if s <= 0.35 {
        return max_radius;
    }
    let t = Easing::Cubic(0.645, 0.045, 0.355, 1.0).apply((s - 0.35) / 0.65);
    lerp(max_radius, target.min(max_radius), t)
}

/// One morph surface: press it and the droplet detaches from the trigger.
pub struct GlassMorph {
    id: Id,
    area: (f32, f32),
    from: Rect,
    to: Rect,
    backdrop: Option<ui_core::BackdropBlur>,
    bbox: Rect,
}

impl GlassMorph {
    pub fn new(
        id: Id,
        area: (f32, f32),
        from: Rect,
        to: Rect,
        cx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(HigOptions::default(), id, area, from, to, cx, pack)
    }

    pub fn new_with(
        _params: HigOptions,
        id: Id,
        area: (f32, f32),
        from: Rect,
        to: Rect,
        cx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let open = cx.state().pressed || cx.state().dragging;
        let raw_id = id.with("raw");
        let before = cx.spring_value(raw_id).unwrap_or(0.0);
        let closing = !open && before.abs() > 1e-4;
        let (stiffness, damping) = if closing { CLOSE_SPRING } else { OPEN_SPRING };
        let hint = if closing { CLOSE_VELOCITY_HINT } else { 0.0 };
        let raw = cx
            .spring_hinted(raw_id, f32::from(open), stiffness, damping, hint)
            .x;
        let dx = to.center().x - from.center().x;
        let dy = to.center().y - from.center().y;
        let delta = compute_scale_delta((from.size.w, from.size.h), (to.size.w, to.size.h));
        let state = compute(raw, dx, dy, delta, closing);
        let droplet = blob_rect(from, to, state);
        let anchor = anchor_rect(from, state);
        // A fully shrunk trigger contributes nothing: a zero-size blob would still bump the
        // union at its centre, so it leaves the group instead of shrinking into a dot.
        let mut live = Vec::with_capacity(2);
        let mut bbox = droplet;
        if state.anchor_scale > 0.01 {
            bbox = union(anchor, droplet);
            live.push(BlendShape {
                rect: relative(anchor, bbox),
                corners: capsule(anchor),
            });
        }
        live.push(BlendShape {
            rect: relative(droplet, bbox),
            corners: Corners::all(blob_radius(
                droplet.size.w,
                droplet.size.h,
                droplet.size.h * 0.5,
                state.size_t,
            )),
        });
        // The neck belongs to the material, so the glass runs at full strength all the way.
        Self {
            id,
            area,
            from,
            to,
            bbox,
            backdrop: glass_blend(
                &crate::material::glass_recipe(pack, crate::material::Control::Switch),
                1.0,
                &live,
                state.blend,
            ),
        }
    }

    /// What the geometry was built from, for tests and for a caller that needs the bounds.
    pub fn bounds(&self) -> (Rect, Rect, Rect) {
        (self.from, self.to, self.bbox)
    }
}

impl Widget for GlassMorph {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let mut glass = Decl::container(self.id.with("glass"), NodeKind::Box)
            .offset(self.bbox.origin.x, self.bbox.origin.y)
            .size(SizeClass::fixed(self.bbox.size.w, self.bbox.size.h))
            .background(ui_core::geom::Color::TRANSPARENT, Corners::ZERO);
        if let Some(backdrop) = self.backdrop
            && let Some(p) = glass.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }
        Decl::container(self.id, NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fixed(self.area.0),
                height: AxisSize::fixed(self.area.1),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .sense(Sense::HOVER | Sense::CLICK | Sense::DRAG)
            .child(glass)
    }
}

fn scale_about(r: Rect, scale: f32) -> Rect {
    let w = r.size.w * scale;
    let h = r.size.h * scale;
    Rect::new(r.center().x - w * 0.5, r.center().y - h * 0.5, w, h)
}

fn capsule(r: Rect) -> Corners {
    Corners::all(r.size.h.min(r.size.w) * 0.5)
}

fn relative(r: Rect, bbox: Rect) -> Rect {
    Rect::new(
        r.origin.x - bbox.origin.x,
        r.origin.y - bbox.origin.y,
        r.size.w,
        r.size.h,
    )
}

fn union(a: Rect, b: Rect) -> Rect {
    let l = a.origin.x.min(b.origin.x);
    let t = a.origin.y.min(b.origin.y);
    let r = (a.origin.x + a.size.w).max(b.origin.x + b.size.w);
    let b = (a.origin.y + a.size.h).max(b.origin.y + b.size.h);
    Rect::new(l, t, r - l, b - t)
}
