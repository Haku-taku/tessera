//! The glass stepper: a pill with a minus and a plus side, repeating while held.

use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, OverlayAlign, SizeClass};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Easing, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::IMMEDIATE;
use ui_widgets::{Cx, IconSource, Widget};

use crate::material::{Control, glass_recipe};
use crate::paint::role_of;
use crate::shared::{focus_ring_ink, focus_ring_shape, glass_backdrop};

/// The reference's `height` and `width`: a 94×34 capsule.
pub const STEPPER_W: f32 = 94.0;
pub const STEPPER_H: f32 = 34.0;
/// `CupertinoIcons.minus` / `.plus` at 20 dp.
/// `CupertinoIcons.minus` / `.plus`, 20dp.
pub const STEPPER_ICON: f32 = 20.0;
/// `dividerWidth`, and the 25 % ink it is drawn in either appearance.
pub const STEPPER_DIVIDER: f32 = 0.5;
pub const STEPPER_DIVIDER_ALPHA: f32 = 0.25;
/// `AnimatedScale(0.88, 80 ms, easeOut)` on the pressed side.
pub const STEPPER_PRESS_SCALE: f32 = 0.88;
pub const STEPPER_PRESS_TIME: f32 = 0.08;
/// `AnimatedOpacity(0.3, 150 ms)` on a side that cannot step further.
pub const STEPPER_DISABLED_ALPHA: f32 = 0.3;
pub const STEPPER_FADE_TIME: f32 = 0.15;
/// `autoRepeatDelay` and `autoRepeatInterval`.
pub const STEPPER_REPEAT_DELAY: f32 = 0.5;
pub const STEPPER_REPEAT_INTERVAL: f32 = 0.1;
/// The reference compares against the ends with this tolerance.
const EPS: f64 = 1e-10;

/// Everything a stepper is built with; the app owns the value it reports.
pub struct GlassStepperParams {
    pub id: Id,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Stepping past an end cycles to the other one instead of stopping.
    pub wraps: bool,
    pub auto_repeat: bool,
    pub repeat_delay: f32,
    pub repeat_interval: f32,
    pub height: f32,
    pub width: f32,
    pub divider: f32,
    pub disabled: bool,
}

impl GlassStepperParams {
    /// The reference's defaults: 0..=100 by 1, held sides repeat, 94×34.
    pub fn new(id: Id, value: f64) -> Self {
        Self {
            id,
            value,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            wraps: false,
            auto_repeat: true,
            repeat_delay: STEPPER_REPEAT_DELAY,
            repeat_interval: STEPPER_REPEAT_INTERVAL,
            height: STEPPER_H,
            width: STEPPER_W,
            divider: STEPPER_DIVIDER,
            disabled: false,
        }
    }

    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn wraps(mut self, wraps: bool) -> Self {
        self.wraps = wraps;
        self
    }

    pub fn auto_repeat(mut self, on: bool) -> Self {
        self.auto_repeat = on;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// One side of the pill: its own id, its press curve and its enabled fade.
struct Side {
    id: Id,
    icon: IconSource,
    /// 1 while the side can step; it fades toward 0 as it cannot.
    fade: f32,
    /// Press progress 0..=1, mapped to [`STEPPER_PRESS_SCALE`].
    press: f32,
    enabled: bool,
    focus_ring: Option<Color>,
}

/// One glass stepper; every curve advances in `new`, `decl()` is pure.
pub struct GlassStepper {
    id: Id,
    width: f32,
    height: f32,
    divider: f32,
    minus: Side,
    plus: Side,
    ink: Color,
    divider_ink: Color,
    /// The value to report this frame, already stepped, wrapped and clamped.
    reported: Option<f64>,
    backdrop: Option<ui_core::BackdropBlur>,
}

impl GlassStepper {
    pub fn new(params: GlassStepperParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassStepperParams {
            id,
            value,
            min,
            max,
            step,
            wraps,
            auto_repeat,
            repeat_delay,
            repeat_interval,
            height,
            width,
            divider,
            disabled,
        } = params;
        let disabled = disabled || cx.state(id).disabled;
        let can_down = !disabled && (wraps || value > min + EPS);
        let can_up = !disabled && (wraps || value < max - EPS);

        let mut steps = 0i32;
        let sides: [(Id, IconSource, bool, bool); 2] = [
            (id.with("minus"), IconSource::sf("minus"), false, can_down),
            (id.with("plus"), IconSource::sf("plus"), true, can_up),
        ];
        let mut built = Vec::with_capacity(2);
        for (side_id, icon, up, enabled) in sides {
            let state = cx.state(side_id);
            let held = state.pressed || state.drag_started;
            let lit = held && enabled;

            let mut node = cx.at(side_id);
            let press = node.animate(
                side_id.with("press"),
                f32::from(lit),
                AnimSpec::new(STEPPER_PRESS_TIME, Easing::EaseOut),
            );
            let fade = node.animate(
                side_id.with("fade"),
                f32::from(enabled),
                AnimSpec::new(STEPPER_FADE_TIME, Easing::Linear),
            );

            // ── Pointer: the press edge fires once, then the clock repeats it while held
            // (`glass_stepper.dart:269-281`); keyboard activation fires once and never repeats.
            let was_held = node.value(side_id.with("held")) > 0.5;
            node.animate(side_id.with("held"), f32::from(held), IMMEDIATE);
            if held && enabled {
                if !was_held {
                    steps += if up { 1 } else { -1 };
                    node.forget_age(side_id.with("age"));
                    node.animate(side_id.with("ticks"), 0.0, IMMEDIATE);
                }
                if auto_repeat {
                    // Reading the age every frame keeps the subtree rebuilding while held.
                    let age = node.age(side_id.with("age"));
                    let due = if age >= repeat_delay {
                        1 + ((age - repeat_delay) / repeat_interval.max(0.001)) as i32
                    } else {
                        0
                    };
                    let done = node.value(side_id.with("ticks")) as i32;
                    if due > done {
                        steps += (due - done) * if up { 1 } else { -1 };
                        node.animate(side_id.with("ticks"), due as f32, IMMEDIATE);
                    }
                }
            } else {
                node.forget_age(side_id.with("age"));
                node.animate(side_id.with("ticks"), 0.0, IMMEDIATE);
            }
            if state.activated && enabled {
                steps += if up { 1 } else { -1 };
            }

            built.push(Side {
                id: side_id,
                icon,
                fade,
                press,
                enabled,
                focus_ring: state.focus_visible.then(|| focus_ring_ink(pack)),
            });
        }
        let mut built = built.into_iter();
        let minus = built.next().expect("minus side");
        let plus = built.next().expect("plus side");

        let base = if pack.appearance() == Some(ui_design::Appearance::Dark) {
            Color::WHITE
        } else {
            Color::BLACK
        };
        let backdrop = glass_backdrop(&glass_recipe(pack, Control::Container), 1.0);

        Self {
            id,
            width,
            height,
            divider,
            minus,
            plus,
            ink: role_of(pack, "textFillColorPrimary"),
            divider_ink: Color {
                a: STEPPER_DIVIDER_ALPHA,
                ..base
            },
            // A burst that has already reached the end reports nothing, as the reference's
            // `_canIncrement` guard does.
            reported: (steps != 0)
                .then(|| stepped(value, steps, min, max, step, wraps))
                .filter(|v| *v != value),
            backdrop,
        }
    }

    fn side_decl(&self, side: &Side) -> Decl {
        let w = (self.width - self.divider).max(1.0) * 0.5;
        let h = self.height;
        let scale = 1.0 - side.press * (1.0 - STEPPER_PRESS_SCALE);
        let alpha = STEPPER_DISABLED_ALPHA + (1.0 - STEPPER_DISABLED_ALPHA) * side.fade;
        let ink = self.ink;
        let mut node = Decl::container(side.id, NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(h),
                grow_x: Grow::Split(1),
                grow_y: Grow::No,
            })
            .stack_align(OverlayAlign::Center)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .focus_stop(side.enabled)
            .cursor(CursorShape::PointingHand)
            .child(
                ui_widgets::Icon {
                    id: side.id.with("icon"),
                    source: side.icon,
                    size: STEPPER_ICON * scale,
                    ink: Color { a: alpha, ..ink },
                }
                .resolve(),
            );
        if let Some(ring) = side.focus_ring {
            let d = h.min(w);
            node = node.shapes(vec![focus_ring_shape(
                Rect::new((w - d) * 0.5, (h - d) * 0.5, d, d),
                Corners::all(d * 0.5),
                ring,
            )]);
        }
        node
    }
}

impl Widget for GlassStepper {
    fn handles_disabled(&self) -> bool {
        // Each side paints its own 0.3 fade; the adapter must not fade the pill again.
        true
    }

    fn change(&self, _state: &ui_widgets::WidgetState, _bounds: Rect) -> Option<Change> {
        self.reported.map(Change::Value)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let corners = Corners::all(self.height * 0.5);
        let mut root = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass::fixed(self.width, self.height))
            .corners(corners)
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .background(Color::TRANSPARENT, corners);
        if let Some(backdrop) = self.backdrop
            && let Some(p) = root.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }
        root.child(self.side_decl(&self.minus))
            .child(
                Decl::container(self.id.with("divider"), NodeKind::Box)
                    .size(SizeClass::fixed(self.divider, self.height))
                    .background(self.divider_ink, Corners::all(0.0)),
            )
            .child(self.side_decl(&self.plus))
    }
}

/// Applies `steps` of `step` from `value`, wrapping or clamping at the ends exactly as the
/// reference's `_decrement` / `_increment` do (`glass_stepper.dart:183-201`).
///(`glass_stepper.dart:183-201`).
pub fn stepped(value: f64, steps: i32, min: f64, max: f64, step: f64, wraps: bool) -> f64 {
    let span = max - min;
    let mut v = value;
    for _ in 0..steps.unsigned_abs() {
        let mut next = if steps > 0 { v + step } else { v - step };
        if wraps && span > 0.0 {
            if next > max {
                next = min + (next - max) % span;
            } else if next < min {
                next = max - (min - next) % span;
            }
        }
        v = next.clamp(min, max);
    }
    v
}
