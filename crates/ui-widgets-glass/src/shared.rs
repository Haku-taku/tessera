//! The glass material both interactive controls share.

use crate::paint::role_of;
use ui_core::BackdropBlur;
use ui_core::Id;
use ui_core::draw::{BLEND_SHAPES, BackdropMaterial, BlendShape, FocusVisual, LiquidGlass, Shape};
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_style::Style;

/// snappy(320 ms, bounce 0.15): every glass overlay's arrival.
pub const REVEAL_SPRING: (f32, f32) = (385.14, 33.38);
/// The thumb's position travel, shared by the slider and the switch.
pub const POSITION_SPRING: (f32, f32) = (157.91, 25.13);

/// `GlassFocusRingPainter`: a 1.5 dp stroke, 3 dp outside the shape.
pub const FOCUS_RING_WIDTH: f32 = 1.5;
pub const FOCUS_RING_OUTSET: f32 = 3.0;

/// The ring's ink: `CupertinoColors.activeBlue` at 85 %.
pub fn focus_ring_ink(pack: &Style) -> Color {
    Color {
        a: 0.85,
        ..role_of(pack, "systemBlue")
    }
}

/// The ring as a focus adorner, for a control whose box the engine draws the ring around.
pub fn focus_ring(pack: &Style) -> FocusVisual {
    FocusVisual {
        outer: focus_ring_ink(pack),
        outer_width: FOCUS_RING_WIDTH,
        inner: Color::TRANSPARENT,
        inner_width: 0.0,
        margin: Insets {
            left: -FOCUS_RING_OUTSET,
            top: -FOCUS_RING_OUTSET,
            right: -FOCUS_RING_OUTSET,
            bottom: -FOCUS_RING_OUTSET,
        },
        radius: 0.0,
    }
}

/// The same ring as a painted shape, for a control that draws its own outline.
pub fn focus_ring_shape(rect: Rect, corners: Corners, ink: Color) -> Shape {
    let o = FOCUS_RING_OUTSET;
    Shape::Rect {
        rect: Rect::new(
            rect.origin.x - o,
            rect.origin.y - o,
            rect.size.w + o * 2.0,
            rect.size.h + o * 2.0,
        ),
        corners: Corners {
            tl: corners.tl + o,
            tr: corners.tr + o,
            br: corners.br + o,
            bl: corners.bl + o,
            shape: corners.shape,
        },
        fill: Color::TRANSPARENT,
        gradient: None,
        border: Some((ink, FOCUS_RING_WIDTH)),
    }
}

/// The alignment a slot index sits at, -1..=1.
pub fn tab_alignment(index: usize, n: usize) -> f32 {
    if n <= 1 {
        return 0.0;
    }
    (-1.0 + 2.0 * index as f32 / (n - 1) as f32).clamp(-1.0, 1.0)
}

/// The index a pointer at `x` sits over.
pub fn index_at_x(x: f32, width: f32, n: usize) -> usize {
    let rel = (x / width.max(1.0)).clamp(0.0, 1.0);
    ((rel * n as f32) as usize).min(n.saturating_sub(1))
}

/// The alignment a pointer at `x` names, with the reference's rubber-band at the ends.
pub fn alignment_at_x(x: f32, width: f32, n: usize) -> f32 {
    let n = n.max(1);
    let rel = (x / width.max(1.0)).clamp(0.0, 1.0);
    let inset = 1.0 / (2.0 * n as f32);
    let range = 1.0 - 1.0 / n as f32;
    let normalized = if range <= 0.0 {
        0.5
    } else {
        (rel - inset) / range
    };
    let rubber = if normalized < 0.0 {
        -(-normalized * 0.4).clamp(0.0, 0.3)
    } else if normalized > 1.0 {
        1.0 + ((normalized - 1.0) * 0.4).clamp(0.0, 0.3)
    } else {
        normalized
    };
    2.0 * rubber - 1.0
}

/// The three springs a draggable lens rides: finger-driven, released, and blooming.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensSprings {
    pub position: (f32, f32),
    pub interactive: (f32, f32),
    pub bloom: (f32, f32),
}

/// The reference's preset for both draggable strips: `GlassSpring.interactive()` 150 ms
/// bounce 0.14 while the finger drags, `snappy(350)` released, `snappy(300)` bloom.
pub const LENS_SPRINGS: LensSprings = LensSprings {
    position: (322.27, 30.52),
    interactive: (1754.60, 72.05),
    bloom: (438.65, 35.60),
};

/// Where the lens is this frame: its alignment, bloom and jelly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensMotion {
    /// -1..=1, overshooting while a drag pulls past either end.
    pub align: f32,
    /// 0..=1 bloom, a spring, so it overshoots.
    pub thickness: f32,
    /// Horizontal velocity squashes the lens and stretches it vertically.
    pub jelly: (f32, f32),
}

/// The lens state both draggable strips share; every spring advances here.
pub fn lens_motion(
    id: Id,
    selected: usize,
    n: usize,
    width: f32,
    state: &ui_widgets::WidgetState,
    cx: &mut ui_widgets::NodeCx<'_>,
    springs: LensSprings,
) -> LensMotion {
    let n = n.max(1);
    let target = tab_alignment(selected.min(n - 1), n);

    // ── Alignment: the finger owns it while dragging, a spring owns it otherwise.
    let align_key = id.with("align");
    let prev_align = cx.value(align_key);
    let prev_velocity = cx.value(id.with("velocity"));
    // The finger owns the alignment only once the press is a real drag. A tap targets the
    // slot it landed on, which the spring below then travels to.
    let (align, velocity) = if state.drag_started {
        let local = state
            .local
            .map_or(prev_align, |p| alignment_at_x(p.x, width, n));
        // The finger sets the target; the interactive spring carries the lens there in 150 ms,
        // which is what makes the first move of a drag glide instead of snap.
        let s = cx.spring(
            align_key,
            local,
            springs.interactive.0,
            springs.interactive.1,
        );
        (s.x, s.v)
    } else if state.dropped {
        // A flick moves one slot further, the reference's `|v| > 0.5` rule.
        let rel = (prev_align + 1.0) * 0.5;
        let mut index = (rel * (n - 1) as f32).round().clamp(0.0, (n - 1) as f32) as usize;
        if prev_velocity.abs() > 0.5 {
            index = index
                .saturating_add(1)
                .min(n - 1)
                .max(index.saturating_sub(1));
        }
        let s = cx.spring(
            align_key,
            tab_alignment(index, n),
            springs.position.0,
            springs.position.1,
        );
        (s.x, s.v)
    } else {
        let s = cx.spring(align_key, target, springs.position.0, springs.position.1);
        (s.x, s.v)
    };
    cx.animate(id.with("velocity"), velocity, ui_widgets::build::IMMEDIATE);

    // ── Bloom: up while held or while travelling, down once it lands.
    let moving = (align - target).abs() > 0.05;
    let bloom_target = f32::from(state.pressed || state.dragging || moving);
    let thickness = cx
        .spring(
            id.with("thickness"),
            bloom_target,
            springs.bloom.0,
            springs.bloom.1,
        )
        .x;

    // ── Jelly: horizontal velocity squashes the lens and stretches it vertically.
    LensMotion {
        align,
        thickness,
        jelly: jelly_scale((velocity, 0.0), JELLY_VELOCITY, JELLY_MAX),
    }
}

/// The reference's jelly tuning: velocity scale 10, premium distortion 0.8.
pub const JELLY_VELOCITY: f32 = 10.0;
pub const JELLY_MAX: f32 = 0.8;

/// `buildJellyTransform` (`draggable_indicator_physics.dart:100-139`): the scale pair a body
/// gets from its own velocity — squashed along the motion, stretched across it.
pub fn jelly_scale(velocity: (f32, f32), velocity_scale: f32, max_distortion: f32) -> (f32, f32) {
    let (vx, vy) = velocity;
    let speed = (vx * vx + vy * vy).sqrt();
    if !speed.is_finite() || speed <= 0.0 {
        return (1.0, 1.0);
    }
    let (dx, dy) = (vx.abs() / speed, vy.abs() / speed);
    let d = (speed / velocity_scale.max(f32::EPSILON)).clamp(0.0, 1.0) * max_distortion;
    (
        (1.0 - dx * d * 0.5) * (1.0 + dy * d * 0.3),
        (1.0 - dy * d * 0.5) * (1.0 + dx * d * 0.3),
    )
}

/// A capsule scaled about its own centre by `scale`, still a capsule.
pub fn jelly_capsule(rect: Rect, scale: (f32, f32)) -> Rect {
    let w = rect.size.w * scale.0;
    let h = rect.size.h * scale.1;
    Rect::new(
        rect.origin.x + (rect.size.w - w) * 0.5,
        rect.origin.y + (rect.size.h - h) * 0.5,
        w,
        h,
    )
}

/// The jelly spring both thumbs read for velocity: `GlassSpring.snappy(350ms)`.
pub const THUMB_JELLY_SPRING: (f32, f32) = (322.27, 30.52);
/// A body at rest: no distortion, the capsule at its own size.
pub const NO_JELLY: (f32, f32) = (1.0, 1.0);
/// The slider's jelly tuning, which the switch shares: velocity scale 2, distortion 0.6
/// (`glass_slider.dart:624-632`).
pub const THUMB_JELLY_VELOCITY: f32 = 2.0;
pub const THUMB_JELLY_DISTORTION: f32 = 0.6;
/// The jelly's own smoothing: 0.2 s, critically damped.
pub const THUMB_JELLY_SMOOTH: (f32, f32) = (986.96, 62.83);

/// The jelly a thumb takes from its own motion. A frame-sampled pointer gives a velocity that
/// ripples once per frame, so it is smoothed before it shapes anything.
pub fn thumb_jelly(id: Id, velocity: f32, cx: &mut ui_widgets::NodeCx<'_>) -> (f32, f32) {
    let smoothed = cx.spring(
        id.with("jelly-smooth"),
        velocity,
        THUMB_JELLY_SMOOTH.0,
        THUMB_JELLY_SMOOTH.1,
    );
    jelly_scale(
        (smoothed.x, 0.0),
        THUMB_JELLY_VELOCITY,
        THUMB_JELLY_DISTORTION,
    )
}

/// The resting pill and the blooming lens of a strip's indicator, both in strip space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensGeometry {
    pub rest: Rect,
    pub lens: Rect,
}

/// The indicator's two rects at `align`, bloomed by `t` (`thickness`) and scaled by `jelly`.
#[allow(clippy::too_many_arguments)]
pub fn lens_geometry(
    align: f32,
    thickness: f32,
    jelly: (f32, f32),
    inner_w: f32,
    cell_w: f32,
    inset: f32,
    box_h: f32,
    expand: (f32, f32),
) -> LensGeometry {
    // `a` is the reference's 0..1 alignment; the resting pill is one cell wide.
    let a = (align + 1.0) * 0.5;
    let rest_x = inset + a * (inner_w - cell_w);
    let rest = Rect::new(rest_x, inset, cell_w, box_h - inset * 2.0);

    let t = thickness.clamp(0.0, 1.0);
    // The lens grows out of the resting pill: `expand` dp per side and up and down.
    let base_w = cell_w + expand.0 * 2.0 * t;
    let base_h = (box_h - inset * 2.0) + expand.1 * 2.0 * t;
    let lens_w = base_w * jelly.0;
    let lens_h = base_h * jelly.1;
    let lens_x = rest_x - expand.0 * t - (lens_w - base_w) * 0.5;
    let lens_y = (box_h - base_h) * 0.5 - (lens_h - base_h) * 0.5;
    LensGeometry {
        rest,
        lens: Rect::new(lens_x, lens_y, lens_w, lens_h),
    }
}

/// How much of the resting pill is still there: one at rest, gone once the glass blooms.
pub fn rest_fade(thickness: f32) -> f32 {
    (1.0 - thickness / 0.15).clamp(0.0, 1.0)
}

/// The reference's rubber band (`LiquidGlassSwitch.swift:511-518`, `LiquidGlassSlider.swift:771-778`):
/// past either end a value moves by the square root of its pull, in dp, over `travel` dp.
pub fn rubber_band_at(value: f32, travel: f32) -> f32 {
    if value < 0.0 {
        -(-value * travel).sqrt() / travel
    } else if value > 1.0 {
        1.0 + ((value - 1.0) * travel).sqrt() / travel
    } else {
        value
    }
}

/// The reference's `ShadowView` (`LiquidGlassView.swift:158-176`): its rim dropped 5.5 dp at
/// 0.2 with `shadowRadius` 3.5 — a blur radius, doubled here to 7 for a wider, softer rim.
pub const RIM_SHADOW_BLUR: f32 = 7.0;
pub const RIM_SHADOW_DY: f32 = 5.5;
pub const RIM_SHADOW_ALPHA: f32 = 0.2;
/// The casting band: the reference's outer path is 1 dp wider and 1.75 dp taller than the
/// silhouette, its inner path 1.75 dp shorter, so 3.5 dp is its vertical depth.
pub const RIM_SHADOW_RING: f32 = 3.5;

/// The rim shadow of a capsule at `(x, y, w, h)`, faded by `melt` as the glass takes over.
pub fn rim_shadow(x: f32, y: f32, w: f32, h: f32, melt: f32) -> ui_core::draw::Shape {
    let alpha = crate::tuning::value(crate::tuning::Knob::ShadowAlpha, RIM_SHADOW_ALPHA);
    ui_core::draw::Shape::Shadow {
        rect: ui_core::geom::Rect::new(x, y, w, h),
        corners: ui_core::geom::Corners::all(h * 0.5),
        colour: ui_core::geom::Color {
            a: alpha * melt.clamp(0.0, 1.0),
            ..ui_core::geom::Color::BLACK
        },
        blur: crate::tuning::value(crate::tuning::Knob::ShadowBlur, RIM_SHADOW_BLUR),
        dy: crate::tuning::value(crate::tuning::Knob::ShadowDy, RIM_SHADOW_DY),
        ring: crate::tuning::value(crate::tuning::Knob::ShadowRing, RIM_SHADOW_RING),
    }
}

/// The hairline the glass wears: the rim the shadow above is cast by, 1 dp of black.
///
/// The reference keeps the equivalent constants commented out (`LiquidGlassSwitch.swift:116`,
/// `:230-231`), so the width here follows its `ShadowView` ring instead: the outer path is
/// exactly 1 dp wider than the silhouette on each side.
pub const GLASS_RIM_W: f32 = 1.0;
pub const GLASS_RIM_ALPHA: f32 = 0.25;

/// How far the hairline sits outside the silhouette, in dp: the stroke is drawn inside a
/// rect grown by this much, so half of it lands beyond the glass edge.
pub const GLASS_RIM_OUTSET: f32 = 1.0;

/// The glass face plus the hairline iOS 26.2 draws at its edge, faded in by `melt`.
pub fn glass_face(
    rect: ui_core::geom::Rect,
    corners: ui_core::geom::Corners,
    fill: ui_core::geom::Color,
    melt: f32,
) -> Vec<ui_core::draw::Shape> {
    let mut shapes = vec![ui_core::draw::Shape::Rect {
        rect,
        corners,
        fill,
        gradient: None,
        border: None,
    }];
    // The hairline rides a rect grown by the outset — corners grown with it, so a capsule
    // stays one — and lands half outside the glass edge.
    if let Some(border) = rim_border(melt) {
        let o = GLASS_RIM_OUTSET;
        shapes.push(ui_core::draw::Shape::Rect {
            rect: ui_core::geom::Rect::new(
                rect.origin.x - o,
                rect.origin.y - o,
                rect.size.w + o * 2.0,
                rect.size.h + o * 2.0,
            ),
            corners: ui_core::geom::Corners {
                tl: corners.tl + o,
                tr: corners.tr + o,
                br: corners.br + o,
                bl: corners.bl + o,
                shape: corners.shape,
            },
            fill: ui_core::geom::Color::TRANSPARENT,
            gradient: None,
            border: Some(border),
        });
    }
    shapes
}

/// The rim's hairline, faded in with the glass — the white pill wears none.
pub fn rim_border(melt: f32) -> Option<(ui_core::geom::Color, f32)> {
    let alpha = crate::tuning::value(crate::tuning::Knob::BorderAlpha, GLASS_RIM_ALPHA);
    let width = crate::tuning::value(crate::tuning::Knob::BorderWidth, GLASS_RIM_W);
    let a = alpha * melt.clamp(0.0, 1.0);
    (a > 0.001 && width > 0.0).then_some((
        ui_core::geom::Color {
            a,
            ..ui_core::geom::Color::BLACK
        },
        width,
    ))
}

/// The glass both interactive controls carry: refraction only, no separate blur pass.
///
/// `None` below the threshold, where the white material still covers the shape and the
/// pass cannot show. `interaction` is the reference's `interactionIntensity`.
/// `interactionIntensity`.
pub fn glass_backdrop(glass: &LiquidGlass, interaction: f32) -> Option<BackdropBlur> {
    material(glass, interaction, &[], 0.0)
}

/// The same glass with `shapes` fused into it, in the node's own logical space.
///
/// `k` is the smooth-union radius in dp: two shapes further apart than `k` never neck.
pub fn glass_blend(
    glass: &LiquidGlass,
    interaction: f32,
    shapes: &[BlendShape],
    k: f32,
) -> Option<BackdropBlur> {
    material(glass, interaction, shapes, k)
}

fn material(
    glass: &LiquidGlass,
    interaction: f32,
    shapes: &[BlendShape],
    k: f32,
) -> Option<BackdropBlur> {
    let interaction = interaction.clamp(0.0, 1.0);
    if interaction <= 0.02 {
        return None;
    }
    let mut blend = [BlendShape::NONE; BLEND_SHAPES];
    for (slot, shape) in blend.iter_mut().zip(shapes) {
        *slot = *shape;
    }
    let count = shapes.len().min(BLEND_SHAPES) as u8;
    use crate::tuning::{Knob, value};
    let material = BackdropMaterial::LiquidGlass(LiquidGlass {
        refractive_index: value(Knob::RefractiveIndex, glass.refractive_index),
        thickness: value(Knob::Thickness, glass.thickness),
        capture_scale: value(Knob::CaptureScale, glass.capture_scale),
        dispersion: value(Knob::Dispersion, glass.dispersion),
        fresnel_intensity: value(Knob::FresnelIntensity, glass.fresnel_intensity),
        fresnel_distance: value(Knob::FresnelDistance, glass.fresnel_distance),
        fresnel_sharpness: value(Knob::FresnelSharpness, glass.fresnel_sharpness),
        glare_intensity: value(Knob::GlareIntensity, glass.glare_intensity),
        glare_distance: value(Knob::GlareDistance, glass.glare_distance),
        glare_sharpness: value(Knob::GlareSharpness, glass.glare_sharpness),
        glare_convergence: value(Knob::GlareConvergence, glass.glare_convergence),
        glare_bias: value(Knob::GlareBias, glass.glare_bias),
        glare_offset: value(Knob::GlareOffset, glass.glare_offset),
        blend,
        blend_count: count,
        blend_k: k,
        ..*glass
    });
    Some(BackdropBlur {
        radius: 1,
        // The frost is a real blur of the backdrop, taken before the capture: a coarse
        // capture upscaled with bilinear is exactly what read as blocky aliasing.
        sigma: value(Knob::FrostBlur, glass.frost),
        material,
    })
}
