//! The glass switch: a 63×28 track under a 37×24 white pill that melts into a 58×38.33 lens.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::{AnimSpec, Decl, Easing, Id, NodeKind, SizeClass};
use ui_style::{Style, TextRole};
use ui_widgets::Widget;
use ui_widgets::build::IMMEDIATE;

use crate::HigOptions;
use crate::paint::*;
use crate::shared::*;
use crate::text::LabelText;

/// One glass switch; every curve advances in `new`, `decl()` is pure.
pub struct GlassSwitch {
    id: Id,
    /// The thumb's centre as progress 0..=1, rubber-banded past either end while dragging.
    pos: f32,
    /// How far the pill has melted into the lens, 0..=1 — a spring, so it overshoots.
    expand: f32,
    /// The track colour, cross-faded on its own 0.25 s clock.
    track: Color,
    /// The thumb's box and the white pill it draws, which are the same capsule.
    thumb: SwitchThumb,
    /// The pill's colour before the melt fades it.
    white: Color,
    backdrop: Option<ui_core::BackdropBlur>,
    /// Which end the drag has reached, if any: the reference toggles as the thumb hits it.
    edge: Option<bool>,
    semantic_on: bool,
    label: Option<LabelText>,
}

impl GlassSwitch {
    pub fn new(
        id: Id,
        on: bool,
        label: &str,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(HigOptions::default(), id, on, label, ctx, pack)
    }

    pub fn new_with(
        _params: HigOptions,
        id: Id,
        on: bool,
        label: &str,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let semantic_on = on;
        let state = ctx.state();
        let value = u8::from(on) as f32;

        // ── Position: a drag carries the thumb by the finger's own travel, and the pull past
        // either end is square-rooted; a tap springs it to the value.
        let pos_id = id.with("pos");
        let shown_id = id.with("shown");
        let last_x = id.with("last-x");
        if let Some(l) = state.local {
            ctx.animate(last_x, l.x, IMMEDIATE);
        }
        let pos = if state.pressed || state.drag_started {
            if ctx.value(id.with("bank")) <= 0.5 {
                ctx.animate(id.with("bank"), 1.0, IMMEDIATE);
                ctx.animate(id.with("from-pos"), ctx.value(shown_id), IMMEDIATE);
                ctx.animate(id.with("from-x"), ctx.value(last_x), IMMEDIATE);
            }
            let pulled = ctx.value(last_x) - ctx.value(id.with("from-x"));
            let progress = ctx.value(id.with("from-pos")) + pulled / travel();
            rubber_band_at(progress, travel())
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
        ctx.animate(shown_id, pos, IMMEDIATE);

        // ── Expand: the pill grows into the lens on a 0.4 s spring, and back on a 0.6 s one.
        // The hold is an animation, not a timestamp: reading the clock every frame keeps the
        // loop awake for good, while a decaying animation lets it rest the moment it ends.
        let hold = id.with("tap-hold");
        if state.activation() {
            ctx.animate(hold, 1.0, IMMEDIATE);
        }
        let tapped = ctx.animate(hold, 0.0, TAP_HOLD_SPEC) > 0.0;
        let target = f32::from(state.pressed || state.drag_started || tapped);
        let (k, c) = if target > 0.5 { EXPAND } else { CONTRACT };
        let expand = ctx.spring(id.with("expand"), target, k, c).x;

        // ── Track: systemGreen on, tertiaryLabel off, cross-faded in 0.25 s (`:281-291`).
        let colour = role_of(
            pack,
            if semantic_on {
                "glassTrackActive"
            } else {
                "glassTrackInactive"
            },
        );
        let track = ctx.animate_colour(id.with("track"), colour, TRACK_COLOUR);

        // ── Jelly: the same velocity the slider reads, so the pill squashes along its travel
        // and stretches across it instead of only inflating.
        let velocity = ctx
            .spring(
                id.with("jelly"),
                pos,
                THUMB_JELLY_SPRING.0,
                THUMB_JELLY_SPRING.1,
            )
            .v;
        let thumb = switch_thumb(pos, expand, thumb_jelly(id, velocity, ctx));
        let centre = thumb.x + thumb.w * 0.5;
        let (lo, hi) = (INSET + THUMB_W * 0.5, WIDTH - INSET - THUMB_W * 0.5);
        let edge = if centre <= lo + EDGE_TOGGLE && semantic_on {
            Some(false)
        } else if centre >= hi - EDGE_TOGGLE && !semantic_on {
            Some(true)
        } else {
            None
        };

        Self {
            id,
            pos,
            expand,
            track,
            thumb,
            white: role_of(pack, "glassThumb"),
            backdrop: glass_backdrop(
                &crate::material::glass_recipe(pack, crate::material::Control::Switch),
                expand.clamp(0.0, 1.0),
            ),
            edge,
            semantic_on,
            label: LabelText::new(
                pack,
                TextRole::Body,
                label,
                role_of(pack, "textFillColorPrimary"),
                f32::INFINITY,
            ),
        }
    }
}

impl Widget for GlassSwitch {
    fn handles_disabled(&self) -> bool {
        // The 0.5 fade is painted here; the adapter must not fade again.
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        if state.disabled {
            return None;
        }
        // A tap flips the switch; the drag path owns everything else.
        if state.clicked && !state.dragging {
            return Some(ui_core::Change::Toggled {
                on: Some(!self.semantic_on),
            });
        }
        // Reaching the far end flips it mid-drag, as the reference does (`:527-541`).
        if let Some(want_on) = self.edge
            && state.dragging
            && want_on != self.semantic_on
        {
            return Some(ui_core::Change::Toggled { on: Some(want_on) });
        }
        // On release, commit by the side the thumb came to rest on. The reference toggles
        // unconditionally here instead (`:544-553`), so dragging away from the current side
        // still flips it; that is a defect we do not copy.
        if state.dropped {
            let want_on = self.pos > 0.5;
            return (want_on != self.semantic_on)
                .then_some(ui_core::Change::Toggled { on: Some(want_on) });
        }
        None
    }

    fn decl(&self, cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let disabled = cx.state(self.id).disabled;
        let t = self.thumb;

        // Track: one flat colour, fixed to the 63×28 box.
        let track = Decl::container(self.id.with("track"), NodeKind::Box)
            .background(self.track, Corners::all(HEIGHT * 0.5))
            .size(SizeClass::fixed(WIDTH, HEIGHT));

        // Thumb: a clear glass capsule carrying the white pill, which fades as the glass
        // arrives — one shape from solid to lens, exactly as the reference's cross-fade.
        let pill = Color {
            a: self.white.a * (1.0 - self.expand).clamp(0.0, 1.0),
            ..self.white
        };
        let mut thumb = Decl::container(self.id.with("thumb"), NodeKind::Box)
            .background(Color::TRANSPARENT, Corners::all(t.h * 0.5))
            .size(SizeClass::fixed(t.w, t.h))
            .offset(t.x, t.y)
            .shapes(glass_face(
                ui_core::geom::Rect::new(0.0, 0.0, t.w, t.h),
                Corners::all(t.h * 0.5),
                pill,
                self.expand,
            ));
        if let Some(backdrop) = self.backdrop
            && let Some(p) = thumb.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }

        // The lens drops the reference's rim shadow; at rest the pill casts nothing.
        let shadow = Decl::container(self.id.with("shadow"), NodeKind::Box)
            .offset(t.x, t.y)
            .size(SizeClass::fixed(t.w, t.h))
            .shapes(vec![rim_shadow(0.0, 0.0, t.w, t.h, self.expand)]);

        let stack = Decl::container(self.id.with("stack"), NodeKind::Stack)
            .size(SizeClass::fixed(WIDTH, HEIGHT))
            .child(track)
            .child(shadow)
            .child(thumb);
        let mut row = Decl::container(self.id, NodeKind::Row)
            .sense(
                ui_core::Sense::HOVER
                    | ui_core::Sense::CLICK
                    | ui_core::Sense::DRAG
                    | ui_core::Sense::FOCUSABLE,
            )
            .cursor(ui_core::CursorShape::PointingHand)
            .gap(8.0)
            .padding(Insets {
                left: 0.0,
                top: 6.0,
                right: 0.0,
                bottom: 6.0,
            })
            .child(stack);
        if let Some(label) = &self.label {
            row = row.child(label.decl(self.id.with("label")));
        }
        if disabled {
            row = row.opacity(0.5);
        }
        row
    }
}

/// LiquidGlassKit's switch geometry (`LiquidGlassSwitch.swift:107-124`).
pub const WIDTH: f32 = 63.0;
pub const HEIGHT: f32 = 28.0;
/// `thumbPadding = 2`, which is `(HEIGHT - THUMB_H) / 2`.
pub const INSET: f32 = 2.0;
pub const THUMB_W: f32 = 37.0;
pub const THUMB_H: f32 = 24.0;
/// The lens the pill grows into: 58×38.333, which overflows the 28 dp track.
pub const EXPANDED_W: f32 = 58.0;
pub const EXPANDED_H: f32 = 38.333;
/// How close to an end the thumb must come to flip the switch mid-drag (`:527-541`).
pub const EDGE_TOGGLE: f32 = 5.0;
/// How long a tap holds the lens open before it contracts (`:424-430`).
pub const TAP_HOLD: f32 = 0.2;
/// The hold itself: an animation decaying from one to zero over `TAP_HOLD`.
pub const TAP_HOLD_SPEC: AnimSpec = AnimSpec::new(TAP_HOLD, Easing::Linear);

/// Apple's `(duration, dampingRatio)` springs, mapped by `k = (2π/T)²`, `c = 4πζ/T` — a
/// convention, since UIKit's own conversion is not published.
const EXPAND: (f32, f32) = (246.74, 18.85);
const CONTRACT: (f32, f32) = (109.66, 14.66);
/// `UIView.animate(withDuration: 0.25)` uses the ease-in-out curve.
const TRACK_COLOUR: AnimSpec = AnimSpec::new(0.250, Easing::Cubic(0.42, 0.0, 0.58, 1.0));

/// The thumb's travel between the inset edges.
pub fn travel() -> f32 {
    WIDTH - THUMB_W - 4.0
}

/// The thumb box at progress `pos`, melted into the lens by `t` — one capsule, no material
/// inside it: the pill and the glass are the same shape at every point of the morph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwitchThumb {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The corner radius, always half the height: a pill at every size.
    pub corner: f32,
}

/// The thumb at `pos` and melt `t`, squashed by `jelly`. `t` is a spring value, so it may leave
/// 0..=1 — that overshoot is the reference's own damping, and it makes the growth elastic.
pub fn switch_thumb(pos: f32, t: f32, jelly: (f32, f32)) -> SwitchThumb {
    let w = THUMB_W + (EXPANDED_W - THUMB_W) * t;
    let h = THUMB_H + (EXPANDED_H - THUMB_H) * t;
    let centre_x = INSET + THUMB_W * 0.5 + travel() * pos;
    let centre_y = INSET + THUMB_H * 0.5;
    let r = jelly_capsule(
        ui_core::geom::Rect::new(centre_x - w * 0.5, centre_y - h * 0.5, w, h),
        jelly,
    );
    SwitchThumb {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.w,
        h: r.size.h,
        corner: r.size.h * 0.5,
    }
}
