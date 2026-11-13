//! MD3 switch, rebuilt from Flutter's `material/switch.dart`: one raw controller drives the
//! size sequence and the colour clock, while a curved view drives the thumb's position.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Rect};
use ui_core::{AnimSpec, CursorShape, Decl, Easing, Id, NodeKind, Sense, SizeClass, Travel};
use ui_style::Style;
use ui_widgets::build::{IMMEDIATE, param};
use ui_widgets::{Widget, WidgetState};

use crate::Md3Options;
use crate::motion::FAST_OUT_SLOW_IN;
use crate::paint::{between, lerp, role, spec_ink};
use crate::spec::{SwitchFlags, SwitchSpec, switch_spec};

/// Token snapshots the geometry derives from: rest, selected, pressed, with icon.
const REST: SwitchSpec = switch_spec(SwitchFlags::NONE);
const ON: SwitchSpec = switch_spec(SwitchFlags {
    selected: true,
    ..SwitchFlags::NONE
});
const PRESSED: SwitchSpec = switch_spec(SwitchFlags {
    pressed: true,
    ..SwitchFlags::NONE
});
const WITH_ICON: SwitchSpec = switch_spec(SwitchFlags {
    with_icon: true,
    ..SwitchFlags::NONE
});

/// The track box; the spec's `52x32` already includes the 2dp outline.
pub const TRACK: (f32, f32) = (REST.track.width, REST.track.height);
/// `EdgeInsets.symmetric(horizontal: 4)`; the vertical padding centres the 32dp track in
/// the 48dp control.
pub const PAD: (f32, f32) = (4.0, 8.0);
/// `switchWidth + padding.horizontal` by `switchHeight` (`switchMinSize.height + 8`).
/// `switchWidth + padding.horizontal` × `switchHeight`(`switchMinSize.height + 8`).
pub const WIDGET: (f32, f32) = (TRACK.0 + 2.0 * PAD.0, 48.0);

/// Handle diameters: inactive 16, active 24, pressed 28, with-icon 24 in **both** states.
pub const THUMB_INACTIVE: f32 = REST.handle.width;
pub const THUMB_ACTIVE: f32 = ON.handle.width;
pub const THUMB_PRESSED: f32 = PRESSED.handle.width;
pub const THUMB_WITH_ICON: f32 = WITH_ICON.handle.width;
/// `transitionalThumbSize`, hand-coded in Flutter as `Size(34, 22)`.
pub const THUMB_TRANSITIONAL: (f32, f32) = (34.0, 22.0);
/// `trackOutlineWidth`; the outline is painted in every state, only its colour moves.
pub const OUTLINE_WIDTH: f32 = REST.track_outline.width;
/// The radial reaction's full radius: `state-layer.size / 2`.
pub const SPLASH_RADIUS: f32 = REST.state_layer.size * 0.5;
/// Gap between the track and the focus ring, from `focus.indicator.offset`.
pub const FOCUS_RING_GAP: f32 = REST.focus_indicator.offset;
/// The token asks for 3dp, but the 60dp box leaves 4dp per side and the ring must not
/// enlarge the control, so its width is what the box has left.
pub const FOCUS_RING_WIDTH: f32 = (WIDGET.0 - TRACK.0) * 0.5 - FOCUS_RING_GAP;
/// `icon.size`.
pub const ICON_SIZE: f32 = REST.icon.size;
/// Bar thickness of the hand-drawn glyph; the engine has no rotation, so a diagonal is a
/// staircase of small squares.
pub const ICON_BAR: f32 = 2.2;

/// `toggleDuration` (M3).
/// `toggleDuration`(M3).
pub const TOGGLE_MS: f32 = 300.0;
/// `kRadialReactionDuration`.
pub const THUMB_REACTION_MS: f32 = 100.0;

/// `Curves.easeOutBack`.
const POSITION_CURVE: Easing = ui_style::motion::EASE_OUT_BACK;

/// `_colorAnimation = CurvedAnimation(positionController, Curves.easeOut)`: CSS `ease-out`,
/// not the engine's Penner `EaseOut`.
const COLOUR_CURVE: Easing = ui_style::motion::CSS_EASE_OUT;
/// The two size-sequence curves: `Cubic(0.31, 0, 0.56, 1)` and `Cubic(0.20, 0, 0, 1)`.
const SEQ_IN: Easing = Easing::Cubic(0.31, 0.0, 0.56, 1.0);
const SEQ_OUT: Easing = Easing::Cubic(0.20, 0.0, 0.0, 1.0);
/// The raw controller: linear in time, 300ms.
pub const PROGRESS: AnimSpec = AnimSpec::new(TOGGLE_MS / 1000.0, Easing::Linear).span(1.0);
/// The curved view of the same controller. The down leg uses the same `easeOutBack` **as a
/// leg-progress curve**: Flutter's `easeOutBack.flipped` applies to a falling controller.
pub const POSITION: AnimSpec = AnimSpec::new(TOGGLE_MS / 1000.0, POSITION_CURVE)
    .down(TOGGLE_MS / 1000.0, POSITION_CURVE)
    .span(1.0);
/// The colour clock is the raw controller on `Curves.easeOut`.
pub const COLOUR_SPEC: AnimSpec = AnimSpec::new(TOGGLE_MS / 1000.0, COLOUR_CURVE).span(1.0);
/// `reaction` runs 100ms on `fastOutSlowIn`.
pub const THUMB_REACTION: AnimSpec =
    AnimSpec::new(THUMB_REACTION_MS / 1000.0, FAST_OUT_SLOW_IN).span(1.0);
/// Hover and focus overlays fade in 50ms on `fastOutSlowIn`.
pub const THUMB_FADE: AnimSpec = AnimSpec::new(0.05, FAST_OUT_SLOW_IN).span(1.0);

/// `trackInnerLength = trackWidth - trackHeight`; the thumb centre travels 16 -> 36.
pub fn track_inner_length() -> f32 {
    TRACK.0 - TRACK.1
}

/// Thumb centre x for a curved position; the reference puts it on the track's end caps.
pub fn thumb_center_x(visual: f32) -> f32 {
    TRACK.1 * 0.5 + visual * track_inner_length()
}

/// Left edge of the thumb box; deliberately unclamped, as in the reference.
pub fn thumb_left(centre: f32, width: f32) -> f32 {
    centre - width * 0.5
}

/// One point on the size trajectory: the two endpoint sizes and the transitional pill.
///
/// `forward` follows the reference's 11/72/17 sequence, `false` its mirrored 17/72/11;
/// `progress` is the **raw** controller value, never the curved position.
pub fn size_at(forward: bool, progress: f32, off: f32, on: f32) -> (f32, f32) {
    let p = progress.clamp(0.0, 1.0);
    let (pw, ph) = THUMB_TRANSITIONAL;
    if forward {
        if p < 0.11 {
            let t = SEQ_IN.apply(p / 0.11);
            (lerp(off, pw, t), lerp(off, ph, t))
        } else if p < 0.83 {
            let t = SEQ_OUT.apply((p - 0.11) / 0.72);
            (lerp(pw, on, t), lerp(ph, on, t))
        } else {
            (on, on)
        }
    } else if p < 0.17 {
        (off, off)
    } else if p < 0.89 {
        let t = SEQ_OUT.mirrored().apply((p - 0.17) / 0.72);
        (lerp(off, pw, t), lerp(off, ph, t))
    } else {
        let t = SEQ_IN.mirrored().apply((p - 0.89) / 0.11);
        (lerp(pw, on, t), lerp(ph, on, t))
    }
}

/// Source-over compositing in stored sRGB against an opaque surface, matching
/// `Color.alphaBlend`.
fn over(src: Color, dst: Color) -> Color {
    let a = src.a.clamp(0.0, 1.0);
    Color {
        r: src.r * a + dst.r * (1.0 - a),
        g: src.g * a + dst.g * (1.0 - a),
        b: src.b * a + dst.b * (1.0 - a),
        a: a + dst.a * (1.0 - a),
    }
}

/// Read or write one latched endpoint; `None` means the reference stopped writing it.
fn latch(ctx: &mut ui_widgets::NodeCx<'_>, key: Id, target: Option<f32>, rest: f32) -> f32 {
    match target {
        Some(t) => ctx.animate(key, t, IMMEDIATE),
        None => ctx.status(key).map(|s| s.value).unwrap_or(rest),
    }
}

/// The two glyphs an MD3 switch thumb can carry. Kept as plain shapes, not a font: the
/// engine draws no icon fonts, and a check/cross is a few bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbGlyph {
    /// Drawn when the switch is on.
    Check,
    /// Drawn when the switch is off.
    Cross,
}

/// `_SwitchConfigM3.iconColor` at rest: selected reads `onPrimaryContainer` in Flutter and
/// `primary` in the export; unselected reads `surfaceContainerHighest`; disabled fades 38%.
pub fn icon_color(pack: &Style, on: bool, disabled: bool) -> Color {
    let s = switch_spec(SwitchFlags {
        selected: on,
        with_icon: true,
        disabled,
        ..SwitchFlags::NONE
    });
    spec_ink(pack, s.icon.color, s.icon.opacity)
}

/// The bars that draw `glyph`, in the glyph's own 16x16 space.
fn glyph_bars(glyph: ThumbGlyph) -> Vec<(f32, f32, f32, f32)> {
    let s = ICON_SIZE;
    let (a, b, c, d) = match glyph {
        ThumbGlyph::Check => (
            (s * 0.20, s * 0.52),
            (s * 0.42, s * 0.72),
            (s * 0.42, s * 0.72),
            (s * 0.80, s * 0.28),
        ),
        ThumbGlyph::Cross => (
            (s * 0.26, s * 0.26),
            (s * 0.74, s * 0.74),
            (s * 0.74, s * 0.26),
            (s * 0.26, s * 0.74),
        ),
    };
    let mut bars = crate::glyph::bars_along(a, b, ICON_BAR);
    bars.extend(crate::glyph::bars_along(c, d, ICON_BAR));
    bars
}

/// One MD3 switch; every animation is advanced in `new_with`, so `decl` stays pure.
pub struct Md3Toggle {
    id: Id,
    /// Curved position 0..=1, used for drawing and for the release decision.
    position: f32,
    /// Drawn thumb size.
    thumb: (f32, f32),
    thumb_color: Color,
    track_color: Color,
    outline: Color,
    outline_width: f32,
    /// Radial reaction: centre offset in the track, radius, colour.
    reaction: Option<(f32, f32, Color)>,
    /// The glyph on the thumb, if any, and its colour at this position.
    glyph: Option<(ThumbGlyph, Color)>,
    /// Keyboard-focus fade 0..=1; the ring is drawn only while this is above zero.
    focus_fade: f32,
    focus_ring_colour: Color,
    semantic_on: bool,
    disabled: bool,
}

impl Md3Toggle {
    /// The unprefixed form: constants only, as a direct caller expects.
    pub fn new(
        id: Id,
        on: bool,
        _label: &str,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(Md3Options::default(), id, on, ctx, pack)
    }

    /// Same control with explicit parameters, so the semantic path and a direct caller
    pub fn new_with(
        params: Md3Options,
        id: Id,
        on: bool,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let state = ctx.state();
        let disabled = state.disabled || params.disabled;
        let inner = track_inner_length();
        let pkey = id.with(param::PROGRESS);
        let vkey = id.with(param::POSITION);

        // A drag writes the finger's own position; a release frame reads it back without
        // retargeting, so the app's value change can arrive on the next frame.
        let (raw, eased) = if state.dragging {
            let before = ctx.value(pkey);
            let target = match state.drag_step {
                Some(d) => (before + d.x / inner).clamp(0.0, 1.0),
                None => before,
            };
            (
                ctx.animate(pkey, target, IMMEDIATE),
                ctx.animate(vkey, target, IMMEDIATE),
            )
        } else if state.dropped {
            (ctx.value(pkey), ctx.value(vkey))
        } else {
            let target = if on { 1.0 } else { 0.0 };
            (
                ctx.animate(pkey, target, PROGRESS),
                ctx.animate(vkey, target, POSITION),
            )
        };
        let progress_status = ctx.status(pkey).unwrap_or_default();

        // `reaction` runs 100ms on `fastOutSlowIn`, started by press or drag.
        let rkey = id.with(param::PRESS);
        let reaction = ctx.animate(
            rkey,
            f32::from(state.pressed || state.dragging),
            THUMB_REACTION,
        );
        let reaction_status = ctx.status(rkey).unwrap_or_default();
        let reaction_active = !(reaction_status.settled && reaction_status.value <= 0.0);
        let reaction_completed = reaction_status.settled && reaction_status.value >= 1.0;

        // The endpoints are re-derived only where the reference does: a completed press, or the
        // raw controller resting at an end. Mid-flight they stay latched.
        let inactive_size = if params.has_icon {
            THUMB_WITH_ICON
        } else {
            THUMB_INACTIVE
        };
        let active_size = THUMB_ACTIVE;
        let (off_target, on_target) = if reaction_completed {
            (
                Some(lerp(inactive_size, THUMB_PRESSED, reaction)),
                Some(lerp(active_size, THUMB_PRESSED, reaction)),
            )
        } else if raw <= 0.0 {
            (
                Some(lerp(inactive_size, THUMB_PRESSED, reaction)),
                Some(active_size),
            )
        } else if raw >= 1.0 {
            (
                Some(inactive_size),
                Some(lerp(active_size, THUMB_PRESSED, reaction)),
            )
        } else {
            (None, None)
        };
        let off_end = latch(ctx, id.with(param::SIZE_OFF), off_target, inactive_size);
        let on_end = latch(ctx, id.with(param::SIZE_ON), on_target, active_size);

        // The sequence follows the instance's travel, not the semantic target: on the
        // release frame the app has not flipped yet, so the target still points the old way.
        let forward = progress_status.travel == Travel::Forward;
        let (tw, th) = if reaction_completed {
            (THUMB_PRESSED, THUMB_PRESSED)
        } else {
            size_at(forward, raw, off_end, on_end)
        };

        // The four thumb branches come from the reference: pressed -> both ends pressed; a
        // forward leg keeps the off end pressed; a reverse leg the on end; rest follows hover.
        let lit = state.hovered || state.focus_visible;
        let traveling = !progress_status.settled;
        let off_pressed =
            reaction_active || (traveling && progress_status.travel == Travel::Forward);
        let on_pressed =
            reaction_active || (traveling && progress_status.travel == Travel::Reverse);
        let off = switch_spec(SwitchFlags {
            with_icon: params.has_icon,
            hovered: lit,
            focused: lit,
            pressed: off_pressed,
            disabled,
            ..SwitchFlags::NONE
        });
        let active = switch_spec(SwitchFlags {
            selected: true,
            with_icon: params.has_icon,
            hovered: lit,
            focused: lit,
            pressed: on_pressed,
            disabled,
        });
        let t = COLOUR_CURVE.apply(raw);
        let surface = role(pack, "surface");
        // `Color.alphaBlend(lerpedThumbColor, surfaceColor)`: a translucent handle is blended
        // over the surface so the track cannot show through it.
        let thumb_color = over(
            between(
                spec_ink(pack, off.handle.color, off.handle.opacity),
                spec_ink(pack, active.handle.color, active.handle.opacity),
                t,
            ),
            surface,
        );
        let track_color = between(
            spec_ink(pack, off.track.color, off.track.opacity),
            spec_ink(pack, active.track.color, active.track.opacity),
            t,
        );
        // The export gives the outline no opacity of its own; Flutter's disabled ring is
        // `onSurface` at the track's 12%, so the track's opacity is the right one.
        let outline = between(
            spec_ink(pack, off.track_outline.color, off.track.opacity),
            spec_ink(pack, active.track_outline.color, active.track.opacity),
            t,
        );

        // Hover and focus fade 50ms. `isFocused` in `paintRadialReaction` is the keyboard
        // focus highlight, so both the ring and the splash key on `focus_visible`.
        let hover_fade = ctx.animate(id.with(param::HOVER), f32::from(state.hovered), THUMB_FADE);
        let focus_fade = ctx.animate(
            id.with(param::FOCUS),
            f32::from(state.focus_visible),
            THUMB_FADE,
        );
        let layer = |selected: bool, hovered: bool, focused: bool, pressed: bool| {
            let s = switch_spec(SwitchFlags {
                selected,
                with_icon: params.has_icon,
                hovered,
                focused,
                pressed,
                disabled,
            });
            spec_ink(pack, s.state_layer.color, s.state_layer.opacity)
        };
        let base = between(
            layer(false, false, false, true),
            layer(true, false, false, true),
            eased,
        );
        let reaction_colour = base
            .lerp(layer(on, true, false, false), hover_fade)
            .lerp(layer(on, false, true, false), focus_fade);
        let radius = if state.hovered || state.focus_visible {
            SPLASH_RADIUS
        } else {
            reaction * SPLASH_RADIUS
        };
        // A running fade still counts as showing: its first frame is 0 but the circle exists.
        let showing = reaction_active
            || state.hovered
            || state.focus_visible
            || hover_fade > 0.0
            || focus_fade > 0.0;

        // `radialReactionOrigin.dx = thumbLeft + thumbSize.height / 2`: the splash sits on the
        // thumb's left cap centre, not its centroid.
        let centre = thumb_center_x(eased);
        let origin_x = thumb_left(centre, tw) + th * 0.5;
        let reaction_shape =
            (showing && radius > 0.0 && !disabled).then_some((origin_x, radius, reaction_colour));

        // `thumbIcon = currentValue < 0.5 ? inactiveIcon : activeIcon`; its colour lerps across
        // the whole travel.
        let glyph = params.has_icon.then(|| {
            let colour = between(
                icon_color(pack, false, disabled),
                icon_color(pack, true, disabled),
                eased,
            );
            let glyph = if eased < 0.5 {
                ThumbGlyph::Cross
            } else {
                ThumbGlyph::Check
            };
            (glyph, colour)
        });

        Self {
            id,
            position: eased,
            thumb: (tw, th),
            thumb_color,
            track_color,
            outline,
            outline_width: params.outline_width,
            reaction: reaction_shape,
            glyph,
            focus_fade,
            focus_ring_colour: spec_ink(pack, REST.focus_indicator.color, 1.0),
            semantic_on: on,
            disabled,
        }
    }
}

impl Widget for Md3Toggle {
    fn handles_disabled(&self) -> bool {
        // The token table paints disabled (12%/38%) itself; the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        if self.disabled {
            return None;
        }
        if state.activation() {
            return Some(ui_core::Change::Toggled {
                on: Some(!self.semantic_on),
            });
        }
        // `position.value >= 0.5 != widget.value`: the release decides by where the thumb
        // ended up, not by the pointer's last pixel.
        if state.dropped {
            let want = self.position >= 0.5;
            return (want != self.semantic_on)
                .then_some(ui_core::Change::Toggled { on: Some(want) });
        }
        None
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let mut shapes = Vec::with_capacity(8);
        // Keyboard focus is an overlay ring with a transparent gap; it never participates in
        // layout or hit testing.
        if self.focus_fade > 0.0 {
            let outset = FOCUS_RING_GAP + FOCUS_RING_WIDTH;
            let w = TRACK.0 + outset * 2.0;
            let h = TRACK.1 + outset * 2.0;
            shapes.push(Shape::Rect {
                rect: Rect::new(PAD.0 - outset, PAD.1 - outset, w, h),
                corners: Corners::all(h * 0.5),
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((
                    Color {
                        a: self.focus_fade,
                        ..self.focus_ring_colour
                    },
                    FOCUS_RING_WIDTH,
                )),
            });
        }
        // The outline is an inward border: the spec's 52x32 is the track's overall size,
        // outline included, so the border eats into the fill rather than adding around it.
        shapes.push(Shape::Rect {
            rect: Rect::new(PAD.0, PAD.1, TRACK.0, TRACK.1),
            corners: Corners::all(TRACK.1 * 0.5),
            fill: self.track_color,
            gradient: None,
            border: (self.outline_width > 0.0).then_some((self.outline, self.outline_width)),
        });
        if let Some((origin_x, r, colour)) = self.reaction {
            shapes.push(Shape::Rect {
                rect: Rect::new(
                    PAD.0 + origin_x - r,
                    PAD.1 + TRACK.1 * 0.5 - r,
                    r * 2.0,
                    r * 2.0,
                ),
                corners: Corners::all(r),
                fill: colour,
                gradient: None,
                border: None,
            });
        }
        let (tw, th) = self.thumb;
        let x = PAD.0 + thumb_left(thumb_center_x(self.position), tw);
        let y = PAD.1 + (TRACK.1 - th) * 0.5;
        shapes.push(Shape::Rect {
            rect: Rect::new(x, y, tw, th),
            corners: Corners::all(tw.min(th) * 0.5),
            fill: self.thumb_color,
            gradient: None,
            border: None,
        });
        if let Some((glyph, colour)) = self.glyph {
            let gx = x + (tw - ICON_SIZE) * 0.5;
            let gy = y + (th - ICON_SIZE) * 0.5;
            for (bx, by, bw, bh) in glyph_bars(glyph) {
                shapes.push(Shape::Rect {
                    rect: Rect::new(gx + bx, gy + by, bw, bh),
                    corners: Corners::all(bw.min(bh) * 0.5),
                    fill: colour,
                    gradient: None,
                    border: None,
                });
            }
        }
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass::fixed(WIDGET.0, WIDGET.1))
            // The switch takes hover, click, drag and focus; its shapes take nothing.
            .sense(Sense::HOVER | Sense::CLICK | Sense::DRAG | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .shapes(shapes)
    }
}
