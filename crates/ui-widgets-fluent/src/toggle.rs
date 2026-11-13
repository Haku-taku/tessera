//! The Fluent toggle switch: a 40x20 track with a knob that swells when pointed at.

use ui_core::draw::FocusVisual;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::{Decl, Id, NodeKind, SizeClass};
use ui_style::motion::WINUI_SPLINE;
use ui_style::{ControlKind, Density, Style, TextRole};
use ui_widgets::build::param;
use ui_widgets::{NodeCx, Widget};

use crate::paint::*;
use crate::text::LabelText;

/// One Fluent `ToggleSwitch`; animation is advanced in `new`, `decl()` is pure.
pub struct FluentToggleSwitch {
    id: Id,
    track_fill: Color,
    border: Color,
    border_w: f32,
    knob_fill: Color,
    knob_size: f32,
    knob_x: f32,
    /// Where the finger put the knob this frame: a drag commits by this, never by the knob.
    finger: f32,
    /// The semantic state this control was built with; a click reports its opposite.
    semantic_on: bool,
    label: Option<LabelText>,
    focus: FocusVisual,
}

impl FluentToggleSwitch {
    pub fn new(id: Id, on: bool, label: &str, ctx: &mut NodeCx<'_>, pack: &Style) -> Self {
        let semantic_on = on;
        let m = pack
            .controls
            .get(ControlKind::ToggleSwitch, Density::Standard);
        let state = ctx.state();
        let disabled = state.disabled;
        let hovered = state.hovered && !disabled;
        let pressed = state.pressed && !disabled;
        // WinUI moves the knob over `ControlFasterAnimationDuration` (83 ms) on
        // `ControlFastOutSlowInKeySpline`, with linear colour key frames on the same clock.
        let spec = pack.motion.get(ui_style::MotionName::StateChange).spec;
        let colour_spec = pack.motion.get(ui_style::MotionName::ColourChange).spec;
        // A press leaves the knob alone; a drag glides it under the finger and tracks it; a tap
        // flips the semantic value and the knob glides there on WinUI's 83 ms.
        let target = u8::from(on) as f32;
        // The knob's cell is what travels, so the pointer maps onto the cell's centres; and
        // the last known position is banked, because a release outside the track leaves no
        // pointer here at all — the commit would then have nothing to read.
        let last_x = id.with("last-x");
        if let Some(l) = state.local {
            ctx.animate(last_x, l.x, ui_widgets::build::IMMEDIATE);
        }
        let finger = ((ctx.value(last_x) - CELL * 0.5) / (TRACK.0 - CELL)).clamp(0.0, 1.0);
        let p =
            ui_widgets::build::drag_follow(ctx, id, target, finger, state.drag_started, spec, spec);
        let knob = ctx.animate(id.with(param::WIDTH), knob_size(on, hovered, pressed), spec);
        let slot = if disabled {
            3
        } else if pressed {
            2
        } else if hovered {
            1
        } else {
            0
        };
        let on_names = [
            "ToggleSwitchFillOn",
            "ToggleSwitchFillOnPointerOver",
            "ToggleSwitchFillOnPressed",
            "ToggleSwitchFillOnDisabled",
        ];
        let off_names = [
            "ToggleSwitchFillOff",
            "ToggleSwitchFillOffPointerOver",
            "ToggleSwitchFillOffPressed",
            "ToggleSwitchFillOffDisabled",
        ];
        let stroke_off_names = [
            "ToggleSwitchStrokeOff",
            "ToggleSwitchStrokeOffPointerOver",
            "ToggleSwitchStrokeOffPressed",
            "ToggleSwitchStrokeOffDisabled",
        ];
        let track_fill = ctx.animate_colour(
            id.with(param::COLOUR),
            role_of(pack, if on { on_names[slot] } else { off_names[slot] }),
            if state.drag_started {
                ui_widgets::build::IMMEDIATE
            } else {
                colour_spec
            },
        );
        // The checked track carries no separate stroke: `ToggleSwitchStrokeOn` is the fill.
        let border = if on {
            track_fill
        } else {
            role_of(pack, stroke_off_names[slot])
        };
        let knob_fill = role_of(
            pack,
            if on {
                [
                    "ToggleSwitchKnobFillOn",
                    "ToggleSwitchKnobFillOnPointerOver",
                    "ToggleSwitchKnobFillOnPressed",
                    "ToggleSwitchKnobFillOnDisabled",
                ][slot]
            } else {
                [
                    "ToggleSwitchKnobFillOff",
                    "ToggleSwitchKnobFillOffPointerOver",
                    "ToggleSwitchKnobFillOffPressed",
                    "ToggleSwitchKnobFillOffDisabled",
                ][slot]
            },
        );
        Self {
            id,
            track_fill,
            border,
            border_w: if on { 0.0 } else { 1.0 },
            knob_fill,
            knob_size: knob,
            knob_x: knob_x(p, knob),
            finger,
            semantic_on,
            label: LabelText::new(
                pack,
                TextRole::Body,
                label,
                role_of(pack, "ToggleSwitchContentForeground"),
                f32::INFINITY,
            ),
            focus: FocusVisual {
                outer: role_of(pack, "FocusStrokeColorOuterBrush"),
                outer_width: 2.0,
                inner: role_of(pack, "FocusStrokeColorInnerBrush"),
                inner_width: 1.0,
                margin: m.focus_margin,
                radius: -m.focus_margin.left,
            },
        }
    }
}

impl Widget for FluentToggleSwitch {
    fn handles_disabled(&self) -> bool {
        // Disabled colours are painted here; the adapter must not fade again.
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        // A click flips the switch; the engine only says the click happened.
        if state.disabled {
            return None;
        }
        if state.clicked {
            return Some(ui_core::Change::Toggled {
                on: Some(!self.semantic_on),
            });
        }
        // A drag commits by the side the finger ended on, not by how far the knob has glided:
        // a quick flick lands the finger before the 83 ms follow is spent, and committing by
        // the knob would then snap the switch back to where it started.
        if state.dropped {
            let want_on = self.finger > 0.5;
            return (want_on != self.semantic_on)
                .then_some(ui_core::Change::Toggled { on: Some(want_on) });
        }
        None
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let thumb = Decl::container(self.id.with("thumb"), NodeKind::Box)
            .background(self.knob_fill, Corners::all(self.knob_size * 0.5))
            .size(SizeClass::fixed(self.knob_size, self.knob_size))
            .offset(self.knob_x, (TRACK.1 - self.knob_size) * 0.5);
        let track = Decl::container(self.id.with("track"), NodeKind::Box)
            .background(self.track_fill, Corners::all(TRACK.1 * 0.5))
            .border(self.border, self.border_w)
            .size(SizeClass::fixed(TRACK.0, TRACK.1))
            .child(thumb);
        let mut row = Decl::container(self.id, NodeKind::Row)
            // A switch takes hover, click and focus; its children take nothing.
            .sense(
                ui_core::Sense::HOVER
                    | ui_core::Sense::CLICK
                    | ui_core::Sense::DRAG
                    | ui_core::Sense::FOCUSABLE,
            )
            // A switch is a control you point at, and its thumb can be dragged across.
            .cursor(ui_core::CursorShape::Arrow)
            .focus_visual(Some(self.focus))
            .gap(12.0)
            .padding(Insets {
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(track);
        if let Some(label) = &self.label {
            row = row.child(label.decl(self.id.with("label")));
        }
        row
    }
}

pub const TRACK: (f32, f32) = (40.0, 20.0);
/// `SwitchKnob`'s own cell: the knob is centred in it and the cell travels the track.
pub const CELL: f32 = 20.0;
/// Knob side: 12 at rest, 14 hovered, 17 pressed — `SwitchKnobOn`/`SwitchKnobOff` key frames.
pub const KNOB: f32 = 12.0;
pub const KNOB_HOVER: f32 = 14.0;
pub const KNOB_PRESSED_ON: f32 = 17.0;
pub const KNOB_PRESSED_OFF: f32 = 14.0;
/// `ControlFasterAnimationDuration` = 83 ms, on `ControlFastOutSlowInKeySpline` (0,0,0,1).
pub const ANIM_MS: f32 = 83.0;
pub const CURVE: ui_core::Easing = WINUI_SPLINE;

/// The knob's side for a state; the checked knob alone swells to 17 while pressed.
pub fn knob_size(on: bool, hovered: bool, pressed: bool) -> f32 {
    if pressed {
        if on {
            KNOB_PRESSED_ON
        } else {
            KNOB_PRESSED_OFF
        }
    } else if hovered {
        KNOB_HOVER
    } else {
        KNOB
    }
}

/// Knob offset: its cell travels the track, and the knob sits centred in the cell.
pub fn knob_x(progress: f32, size: f32) -> f32 {
    (TRACK.0 - CELL) * progress.clamp(0.0, 1.0) + (CELL - size) * 0.5
}
