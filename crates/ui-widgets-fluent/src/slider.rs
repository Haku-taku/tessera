//! The Fluent slider: a 4 dp track under an 18 dp thumb whose dot swells on hover.

use ui_core::geom::{Color, Corners};
use ui_core::{AnimSpec, Decl, Easing, Id, NodeKind, SizeClass};
use ui_style::{MotionName, Style, TextRole};
use ui_widgets::Widget;
use ui_widgets::build::param;

use crate::FluentOptions;
use crate::paint::*;
use crate::text::LabelText;

/// One Fluent slider; every colour is resolved when it is built, `decl()` is pure.
pub struct FluentSlider {
    id: Id,
    p: f32,
    /// The value under the finger this frame, which a drag commits.
    finger: f32,
    /// The value this control was built with.
    value: f32,
    dot: f32,
    track: Color,
    value_fill: Color,
    dot_fill: Color,
    outer: Color,
    stroke: Stroke,
    track_h: f32,
    label: Option<LabelText>,
}

impl FluentSlider {
    /// The unprefixed form: constants only, as a direct caller expects.
    pub fn new(
        id: Id,
        value: f32,
        label: &str,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(FluentOptions::default(), id, value, label, ctx, pack)
    }

    /// Same control with explicit parameters, so the semantic path and a direct caller
    pub fn new_with(
        params: FluentOptions,
        id: Id,
        value: f32,
        label: &str,
        ctx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let state = ctx.state();
        let disabled = state.disabled;
        // WinUI's thumb sits where the pointer is: a drag carries it absolutely, not by the
        // finger's travel. The last position is banked because a release outside the bar
        // leaves no pointer here. A press holds still; a click glides to its value.
        let target = value.clamp(0.0, 1.0);
        let last_x = id.with("last-x");
        if let Some(l) = state.local {
            ctx.animate(last_x, l.x, ui_widgets::build::IMMEDIATE);
        }
        let finger = slider_value_at(ctx.value(last_x));
        // The thumb rides the finger, keeps that place on the release frame (the app has not
        // applied the value yet) and glides to a value set from outside: one contract, one place.
        let p = ui_widgets::build::drag_follow(
            ctx,
            id,
            target,
            finger,
            state.drag_started,
            ui_widgets::build::IMMEDIATE,
            SLIDER_POSITION,
        );
        // `SliderInnerThumb` is 12 pt, scaled to 14 hovered, 10 pressed and 14 disabled;
        // rest and disabled take the fast clock, hover and press the normal one.
        let dot_target = dot_size(state.hovered, state.pressed, disabled);
        let dot_motion = if state.hovered || state.pressed {
            MotionName::Standard
        } else {
            MotionName::StateChange
        };
        let dot = ctx.animate(
            id.with(param::WIDTH),
            dot_target,
            pack.motion.get(dot_motion).spec,
        );
        let slot = if disabled {
            3
        } else if state.pressed {
            2
        } else if state.hovered {
            1
        } else {
            0
        };
        // `SliderTrackFill` is the strong fill in every state; only the value fill ramps.
        let track = role_of(
            pack,
            [
                "SliderTrackFill",
                "SliderTrackFillPointerOver",
                "SliderTrackFillPressed",
                "SliderTrackFillDisabled",
            ][slot],
        );
        let active = role_of(
            pack,
            [
                "SliderTrackValueFill",
                "SliderTrackValueFillPointerOver",
                "SliderTrackValueFillPressed",
                "SliderTrackValueFillDisabled",
            ][slot],
        );
        let thumb_names = [
            "SliderThumbBackground",
            "SliderThumbBackgroundPointerOver",
            "SliderThumbBackgroundPressed",
            "SliderThumbBackgroundDisabled",
        ];
        let dot_colour = role_of(pack, thumb_names[slot]);
        Self {
            id,
            p,
            finger,
            value: target,
            dot,
            track,
            value_fill: active,
            dot_fill: dot_colour,
            outer: role_of(pack, "SliderOuterThumbBackground"),
            stroke: stroke_of(pack, "SliderThumbBorderBrush"),
            track_h: params.slider_track_h.max(0.0),
            label: LabelText::new(
                pack,
                TextRole::Body,
                label,
                role_of(
                    pack,
                    if disabled {
                        "SliderHeaderForegroundDisabled"
                    } else {
                        "SliderHeaderForeground"
                    },
                ),
                SLIDER_WIDTH,
            ),
        }
    }
}

impl Widget for FluentSlider {
    fn handles_disabled(&self) -> bool {
        // Disabled colours are painted here; the adapter must not fade again.
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        // A tap names the point under the pointer; a drag commits wherever the finger has the
        // thumb, every frame it moves — including the release, which may land outside the bar.
        if state.disabled {
            return None;
        }
        let v = if state.drag_started || state.dropped {
            self.finger
        } else if state.clicked {
            crate::slider_value_at(state.local?.x)
        } else {
            return None;
        };
        ((v - self.value).abs() > 0.0001).then_some(ui_core::Change::Value(v as f64))
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let travel = SLIDER_WIDTH - 2.0 * SLIDER_INSET;
        let centre = slider_thumb_x(self.p);
        let track_y = (SLIDER_HEIGHT - self.track_h) * 0.5;
        let radius = self.track_h * 0.5;
        let track = Decl::container(id.with("track"), NodeKind::Box)
            .offset(SLIDER_INSET, track_y)
            .size(SizeClass::fixed(travel, self.track_h))
            .background(self.track, Corners::all(radius))
            .child(
                Decl::container(id.with("active"), NodeKind::Box)
                    .size(SizeClass::fixed(travel * self.p, self.track_h))
                    .background(self.value_fill, Corners::all(radius)),
            );
        // The thumb's outer box carries the elevation stroke; the accent dot sits inside it.
        let reach = SLIDER_THUMB * 0.5 + OUTER_REACH;
        let mut thumb = Decl::container(id.with("thumb"), NodeKind::Box)
            .offset(centre - reach, (SLIDER_HEIGHT - 2.0 * reach) * 0.5)
            .size(SizeClass::fixed(2.0 * reach, 2.0 * reach))
            .background(self.outer, Corners::all(OUTER_CORNER));
        thumb = self.stroke.on(thumb, 1.0);
        let r = self.dot * 0.5;
        let thumb = thumb.child(
            Decl::container(id.with("thumbInner"), NodeKind::Box)
                .offset(reach - r, reach - r)
                .size(SizeClass::fixed(self.dot, self.dot))
                .background(self.dot_fill, Corners::all(r)),
        );
        // A Stack, not a Box: in a Box the thumb would flow *below* the track and only then
        // take its offset, which drops it a track-height too low.
        let bar = Decl::container(id.with("bar"), NodeKind::Stack)
            .size(SizeClass::fixed(SLIDER_WIDTH, SLIDER_HEIGHT))
            .child(track)
            .child(thumb);
        // The root carries the widget id — interaction state is keyed on the decl root —
        // and the header sits above the bar, four points off it (`SliderHeaderThemeMargin`).
        let mut root = Decl::container(id, NodeKind::Column)
            .gap(4.0)
            .sense(
                ui_core::Sense::HOVER
                    | ui_core::Sense::CLICK
                    | ui_core::Sense::DRAG
                    | ui_core::Sense::FOCUSABLE,
            )
            .cursor(ui_core::CursorShape::Arrow);
        if let Some(label) = &self.label {
            root = root.child(label.decl(id.with("label")));
        }
        root.child(bar)
    }
}

/// `SliderTrackThemeHeight`: the track is 4 pt.
pub const SLIDER_TRACK_H: f32 = 4.0;
/// `SliderHorizontalThumbWidth`: the thumb's own box is 18 pt.
pub const SLIDER_THUMB: f32 = 18.0;
/// The thumb's border reaches 2 pt past its box (`Margin="-2"`).
pub const OUTER_REACH: f32 = 2.0;
/// `SliderThumbCornerRadius` for that outer box.
pub const OUTER_CORNER: f32 = 10.0;
/// The track is inset by half the thumb, so the thumb stays inside at both ends.
pub const SLIDER_INSET: f32 = SLIDER_THUMB * 0.5;
/// The inner dot: 12 at rest, 14 hovered, 10 pressed, 14 disabled.
pub const DOT: f32 = 12.0;
pub const DOT_HOVER: f32 = 14.0;
pub const DOT_PRESSED: f32 = 10.0;
pub const DOT_DISABLED: f32 = 14.0;
/// Slider box; the reference is layout-driven, this example fixes a comparable width.
pub const SLIDER_WIDTH: f32 = 240.0;
pub const SLIDER_HEIGHT: f32 = 32.0;

/// The dot's diameter for a state; disabled wins over press, which wins over hover.
pub fn dot_size(hovered: bool, pressed: bool, disabled: bool) -> f32 {
    if disabled {
        DOT_DISABLED
    } else if pressed {
        DOT_PRESSED
    } else if hovered {
        DOT_HOVER
    } else {
        DOT
    }
}

pub fn slider_value_at(x: f32) -> f32 {
    ((x - SLIDER_INSET) / (SLIDER_WIDTH - 2.0 * SLIDER_INSET)).clamp(0.0, 1.0)
}

pub fn slider_thumb_x(progress: f32) -> f32 {
    SLIDER_INSET + (SLIDER_WIDTH - 2.0 * SLIDER_INSET) * progress.clamp(0.0, 1.0)
}

/// A value changed outside the slider eases in over 167 ms.
pub const SLIDER_POSITION: AnimSpec = AnimSpec::new(0.167, Easing::EaseInOut);
