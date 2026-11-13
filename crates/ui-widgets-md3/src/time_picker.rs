//! MD3 time input: a 12-hour clock typed into two fields with an AM/PM selector beside them.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{Change, CursorShape, Decl, Id, InkRadius, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, Text, Widget, WidgetState};

use crate::layer::press_ink;
use crate::paint::spec_ink;
use crate::spec::{TimePickerFlags, time_picker_spec};
use crate::text_field::caret_scroll;
use crate::typescale::TypeStyle;

/// `container` width: 24 + 96 + 24 + 96 + 12 + 52 + 24, the input dialog's own width.
pub const TIME_INPUT_WIDTH: f32 = 328.0;
/// `container` height: padding, title, gap, then the fields and their labels.
pub const TIME_INPUT_HEIGHT: f32 = 182.0;
/// Surface inset on every side.
pub const TIME_INPUT_PAD: f32 = 24.0;
/// `time-input-field.container.size`.
pub const TIME_INPUT_FIELD: (f32, f32) = (96.0, 72.0);
/// Flutter's M3 input decoration reuses the selector's 8dp radius for the fields.
pub const TIME_INPUT_FIELD_RADIUS: f32 = 8.0;
/// Width of the `:` column between the fields.
pub const TIME_INPUT_SEPARATOR: f32 = 24.0;
/// `period-selector.container.size`: the whole selector, its two tiles split the height.
pub const TIME_INPUT_PERIOD: (f32, f32) = (52.0, 72.0);
/// Selector corner radius; each tile's outer corners carry it.
pub const TIME_INPUT_PERIOD_RADIUS: f32 = 8.0;
/// Digits a time field keeps; anything beyond is dropped at the next build.
pub const TIME_INPUT_DIGITS: usize = 2;
/// The surface's corner: MD3's extra-large shape, shared with the date picker.
pub const TIME_INPUT_RADIUS: f32 = 28.0;

/// Gap between the title and the row below it.
const TITLE_GAP: f32 = 20.0;
/// Gap between a field and its supporting label.
const LABEL_GAP: f32 = 10.0;
/// Gap between the minute field and the period selector.
const PERIOD_GAP: f32 = 12.0;
/// The caret's width, centred on the insertion point.
const CARET_WIDTH: f32 = 2.0;
/// The height a title or supporting label occupies, from its token line height.
const LABEL_BAND: f32 = 16.0;

/// `fastOutSlowIn`, the curve the export's state-layer opacities belong to.
const STATE_FADE: ui_core::AnimSpec = ui_core::AnimSpec::new(0.05, crate::motion::FAST_OUT_SLOW_IN);

/// Which half of the day the clock hour belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Am,
    Pm,
}

impl Period {
    /// The 24-hour value of a 1–12 clock hour: 12 AM is 0 and 12 PM is 12.
    pub fn hour24(self, hour12: u32) -> u32 {
        let clock = hour12.clamp(1, 12) % 12;
        match self {
            Period::Am => clock,
            Period::Pm => clock + 12,
        }
    }

    /// The period and the 1–12 clock hour of a 24-hour value.
    pub fn from_hour24(hour24: u32) -> (Period, u32) {
        let day = hour24 % 24;
        let clock = match day % 12 {
            0 => 12,
            hour => hour,
        };
        (if day < 12 { Period::Am } else { Period::Pm }, clock)
    }
}

/// A field's digits as a number, or `None` when they are not all digits.
pub fn parse_digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The hour a field's text names: 1–12, never zero.
pub fn parse_hour(text: &str) -> Option<u32> {
    parse_digits(text).filter(|hour| (1..=12).contains(hour))
}

/// The minute a field's text names: 0–59.
pub fn parse_minute(text: &str) -> Option<u32> {
    parse_digits(text).filter(|minute| *minute < 60)
}

/// What a time field may hold: its ASCII digits, at most `max` of them.
pub fn fit_digits(text: &str, max: usize) -> String {
    text.chars()
        .filter(char::is_ascii_digit)
        .take(max)
        .collect()
}

/// The words a time input cannot invent; the app owns them so they can come from Fluent.
#[derive(Clone, Debug)]
pub struct TimePickerLabels {
    pub title: std::sync::Arc<str>,
    /// Under the hour field.
    pub hour: std::sync::Arc<str>,
    /// Under the minute field.
    pub minute: std::sync::Arc<str>,
    pub am: std::sync::Arc<str>,
    pub pm: std::sync::Arc<str>,
}

impl TimePickerLabels {
    /// Read every label from the catalog in force.
    pub fn from_cx(cx: &Cx<'_, '_>) -> Self {
        Self {
            title: cx.t(ui_i18n::Msg::MD3_TIME_PICKER_TITLE),
            hour: cx.t(ui_i18n::Msg::MD3_TIME_PICKER_HOUR),
            minute: cx.t(ui_i18n::Msg::MD3_TIME_PICKER_MINUTE),
            am: cx.t(ui_i18n::Msg::MD3_TIME_PICKER_AM),
            pm: cx.t(ui_i18n::Msg::MD3_TIME_PICKER_PM),
        }
    }
}

/// Everything the time input is built with; all of it is the app's, none of it the widget's.
pub struct TimePickerParams<'a> {
    pub id: Id,
    /// The hour buffer the app holds; at most two digits reach the engine. Edits arrive as
    /// `Change::Text`, and the new buffer comes back here on the next frame.
    pub hour: &'a str,
    /// The minute buffer, on the same terms as [`Self::hour`].
    pub minute: &'a str,
    pub period: Period,
    /// Labels to draw; `None` reads them from the catalog in force.
    pub labels: Option<TimePickerLabels>,
    pub disabled: bool,
}

impl<'a> TimePickerParams<'a> {
    /// A picker over the given buffers, with labels from the catalog in force.
    pub fn new(id: Id, hour: &'a str, minute: &'a str, period: Period) -> Self {
        Self {
            id,
            hour,
            minute,
            period,
            labels: None,
            disabled: false,
        }
    }
}

/// What the time input asks its app to remember. Typing reaches the app as `Change::Text`;
/// this reports what the engine cannot: a fitted buffer, and the period tile that was clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimePickerChange {
    /// The hour buffer did not fit its two digits; the app must write this text back, or the
    /// same mismatch is reported on every frame. It is text, not a number: a buffer like "00"
    /// has no valid hour, and a number would leave the app unable to drop the extra digit.
    Hour(String),
    /// The minute buffer did not fit its two digits, on the same terms as [`Self::Hour`].
    Minute(String),
    /// The period selector was clicked; it names the tile hit, not the old period.
    Period(Period),
}

/// The inks and type this frame resolves to, so assembling the decl repeats no lookups.
#[derive(Clone, Copy)]
struct Palette {
    surface: Color,
    title: Color,
    title_type: TypeStyle,
    field_fill: Color,
    field_ink: Color,
    field_focus_fill: Color,
    field_focus_ink: Color,
    field_focus_edge: Color,
    field_layer: Color,
    field_layer_opacity: f32,
    digit_type: TypeStyle,
    separator: Color,
    separator_type: TypeStyle,
    supporting: Color,
    supporting_type: TypeStyle,
    period_fill: Color,
    /// Ink of the selected tile. The table's last `period-selector` arm overwrites the
    /// selected label ink, so it is read back from the hovered+selected combination.
    period_selected_ink: Color,
    period_ink: Color,
    period_edge: Color,
    period_hover: Color,
    period_selected_hover: Color,
    period_type: TypeStyle,
    hover_opacity: f32,
    press_opacity: f32,
    focus_opacity: f32,
    focus_ring: Color,
    focus_ring_width: f32,
    focus_ring_offset: f32,
    elevation: f32,
}

/// One field, resolved.
struct Input {
    id: Id,
    /// The fitted buffer, which is also what the engine edits.
    text: std::sync::Arc<str>,
    /// The buffer with the input method's composition spliced in at the caret.
    shown: String,
    composing: Option<std::ops::Range<usize>>,
    caret: Option<usize>,
    painted: Option<ui_core::SelectionPrim>,
    focused: bool,
    ink: Color,
    shapes: Vec<Shape>,
}

/// One AM or PM tile, resolved.
struct Segment {
    id: Id,
    label: String,
    shapes: Vec<Shape>,
    ink: Color,
    ring: Option<Color>,
}

/// An MD3 12-hour time input; the app owns both buffers and the period.
pub struct Md3TimeInput {
    id: Id,
    period: Period,
    hour: Input,
    minute: Input,
    am: Segment,
    pm: Segment,
    title: String,
    hour_label: String,
    minute_label: String,
    fired: Option<TimePickerChange>,
    disabled: bool,
    palette: Palette,
    /// Measured line heights, so digits sit centred in their 72dp fields.
    line: LineHeights,
}

/// One measured line per size the surface uses.
#[derive(Clone, Copy)]
struct LineHeights {
    digit: f32,
    separator: f32,
    period: f32,
    supporting: f32,
}

fn type_style(size: f32, weight: f32) -> TypeStyle {
    TypeStyle {
        size,
        weight: Weight(weight as u16),
    }
}

impl Md3TimeInput {
    /// Resolve this frame's control: palette, both fields, both tiles and the one action.
    pub fn from_pack(pack: &Style, cx: &mut Cx<'_, '_>, params: TimePickerParams<'_>) -> Self {
        let field = time_picker_spec(TimePickerFlags {
            time_input_field: true,
            ..TimePickerFlags::NONE
        });
        let field_focus = time_picker_spec(TimePickerFlags {
            time_input_field: true,
            focused: true,
            ..TimePickerFlags::NONE
        });
        let field_hover = time_picker_spec(TimePickerFlags {
            time_input_field: true,
            hovered: true,
            ..TimePickerFlags::NONE
        });
        let period = time_picker_spec(TimePickerFlags {
            period_selector: true,
            ..TimePickerFlags::NONE
        });
        let selected = time_picker_spec(TimePickerFlags {
            period_selector: true,
            selected: true,
            ..TimePickerFlags::NONE
        });
        let hover = time_picker_spec(TimePickerFlags {
            period_selector: true,
            hovered: true,
            ..TimePickerFlags::NONE
        });
        let selected_hover = time_picker_spec(TimePickerFlags {
            period_selector: true,
            selected: true,
            hovered: true,
            ..TimePickerFlags::NONE
        });
        let press = time_picker_spec(TimePickerFlags {
            period_selector: true,
            pressed: true,
            ..TimePickerFlags::NONE
        });
        let focus = time_picker_spec(TimePickerFlags {
            period_selector: true,
            focused: true,
            ..TimePickerFlags::NONE
        });
        let separator = time_picker_spec(TimePickerFlags {
            separator: true,
            time_input_field: true,
            ..TimePickerFlags::NONE
        });
        let base = time_picker_spec(TimePickerFlags::NONE);
        let palette = Palette {
            surface: spec_ink(pack, base.container.color, 1.0),
            title: spec_ink(pack, base.headline.color, 1.0),
            title_type: type_style(base.headline.size, base.headline.weight),
            field_fill: spec_ink(pack, field.container.color, 1.0),
            field_ink: spec_ink(pack, field.label_text.color, 1.0),
            field_focus_fill: spec_ink(pack, field_focus.container.color, 1.0),
            field_focus_ink: spec_ink(pack, field_focus.label_text.color, 1.0),
            field_focus_edge: spec_ink(pack, field_focus.outline.color, 1.0),
            field_layer: spec_ink(pack, field_hover.state_layer.color, 1.0),
            field_layer_opacity: field_hover.state_layer.opacity,
            digit_type: type_style(field.label_text.size, field.label_text.weight),
            separator: spec_ink(pack, separator.root.color, 1.0),
            separator_type: type_style(separator.root.size, separator.root.weight),
            supporting: spec_ink(pack, field.supporting_text.color, 1.0),
            supporting_type: type_style(field.supporting_text.size, field.supporting_text.weight),
            period_fill: spec_ink(pack, selected.container.color, 1.0),
            period_selected_ink: spec_ink(pack, selected_hover.label_text.color, 1.0),
            period_ink: spec_ink(pack, period.label_text.color, 1.0),
            period_edge: spec_ink(pack, period.outline.color, 1.0),
            period_hover: spec_ink(pack, hover.state_layer.color, 1.0),
            period_selected_hover: spec_ink(pack, selected_hover.state_layer.color, 1.0),
            period_type: type_style(period.label_text.size, period.label_text.weight),
            hover_opacity: hover.state_layer.opacity,
            press_opacity: press.state_layer.opacity,
            focus_opacity: focus.state_layer.opacity,
            focus_ring: spec_ink(pack, base.focus_indicator.color, 1.0),
            focus_ring_width: base.focus_indicator.thickness,
            focus_ring_offset: base.focus_indicator_outline.offset,
            elevation: base.container.elevation,
        };
        let line = LineHeights {
            digit: measure_line(palette.digit_type.size),
            separator: measure_line(palette.separator_type.size),
            period: measure_line(palette.period_type.size),
            supporting: measure_line(palette.supporting_type.size),
        };
        let id = params.id;
        let disabled = params.disabled || cx.state(id).disabled;
        let hour_text = fit_digits(params.hour, TIME_INPUT_DIGITS);
        let minute_text = fit_digits(params.minute, TIME_INPUT_DIGITS);
        let mut fired = None;
        // Reported as text and whenever it differs: a buffer whose head is not a valid hour
        // ("00", "0") would never parse, and a number payload would leave it stuck forever.
        if hour_text != params.hour {
            fired = Some(TimePickerChange::Hour(hour_text.clone()));
        }
        if fired.is_none() && minute_text != params.minute {
            fired = Some(TimePickerChange::Minute(minute_text.clone()));
        }
        let labels = params
            .labels
            .clone()
            .unwrap_or_else(|| TimePickerLabels::from_cx(cx));
        let mut me = Self {
            id,
            period: params.period,
            hour: Input::placeholder(id.with("hour")),
            minute: Input::placeholder(id.with("minute")),
            am: Segment::placeholder(id.with("am")),
            pm: Segment::placeholder(id.with("pm")),
            title: labels.title.to_string(),
            hour_label: labels.hour.to_string(),
            minute_label: labels.minute.to_string(),
            fired,
            disabled,
            palette,
            line,
        };
        me.hour = me.input(pack, cx, id.with("hour"), &hour_text);
        me.minute = me.input(pack, cx, id.with("minute"), &minute_text);
        me.am = me.segment(cx, id.with("am"), &labels.am, true);
        me.pm = me.segment(cx, id.with("pm"), &labels.pm, false);
        me
    }

    pub fn hour_id(&self) -> Id {
        self.hour.id
    }

    pub fn minute_id(&self) -> Id {
        self.minute.id
    }

    pub fn am_id(&self) -> Id {
        self.am.id
    }

    pub fn pm_id(&self) -> Id {
        self.pm.id
    }

    /// The hour buffer handed to the engine this frame, after fitting.
    pub fn hour_text(&self) -> &str {
        &self.hour.text
    }

    /// The minute buffer handed to the engine this frame, after fitting.
    pub fn minute_text(&self) -> &str {
        &self.minute.text
    }

    /// The state layer of one node: ink at the opacity it is drawn, or nothing.
    fn layer(&self, cx: &mut Cx<'_, '_>, id: Id, ink: Color) -> Option<Color> {
        if self.disabled {
            return None;
        }
        let state = cx.state(id);
        if !(state.hovered || state.pressed || state.focus_visible) {
            return None;
        }
        let mut node = cx.at(id);
        let hover = node.animate(id.with(param::HOVER), f32::from(state.hovered), STATE_FADE);
        let press = node.animate(id.with(param::PRESS), f32::from(state.pressed), STATE_FADE);
        let opacity = (hover * self.palette.hover_opacity)
            .max(press * self.palette.press_opacity)
            .max(f32::from(state.focus_visible) * self.palette.focus_opacity);
        (opacity > 0.0).then_some(Color {
            a: ink.a * opacity,
            ..ink
        })
    }

    /// Resolve one field: its shapes, its ink and the engine's editing facts.
    fn input(&self, pack: &Style, cx: &mut Cx<'_, '_>, id: Id, text: &str) -> Input {
        let p = self.palette;
        let state = cx.state(id);
        let focused = state.focused && !self.disabled;
        let (fill, ink, edge) = if focused {
            (
                p.field_focus_fill,
                p.field_focus_ink,
                Some(p.field_focus_edge),
            )
        } else {
            (p.field_fill, p.field_ink, None)
        };
        let corners = Corners::all(TIME_INPUT_FIELD_RADIUS);
        let mut shapes = Vec::with_capacity(3);
        shapes.push(Shape::Box {
            corners,
            fill,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        });
        if p.field_layer_opacity > 0.0
            && !focused
            && let Some(layer) = self.layer(cx, id, p.field_layer)
        {
            shapes.push(Shape::Box {
                corners,
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        if let Some(edge) = edge {
            shapes.push(Shape::Box {
                corners,
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((edge, 2.0)),
                shadow: None,
                image: None,
            });
        }
        let caret = cx.core().caret_for(id);
        let preedit = cx.core().preedit_for(id);
        let painted = cx
            .core()
            .selection_for(id)
            .filter(|selection| !selection.is_collapsed())
            .map(|selection| ui_core::SelectionPrim {
                range: selection.range(),
                fill: pack
                    .ink(ui_widgets::text::roles::SELECTION)
                    .unwrap_or(Color::TRANSPARENT),
                painter: pack.selection,
            });
        let (shown, composing, caret) = compose(text, caret, preedit);
        Input {
            id,
            text: text.into(),
            shown,
            composing,
            caret,
            painted,
            focused,
            ink,
            shapes,
        }
    }

    /// Resolve one period tile: fill, layer, outline, focus ring and the click it reports.
    fn segment(&mut self, cx: &mut Cx<'_, '_>, id: Id, label: &str, am: bool) -> Segment {
        let p = self.palette;
        let selected = (self.period == Period::Am) == am;
        let corners = period_corners(am);
        let ink = if selected {
            p.period_selected_ink
        } else {
            p.period_ink
        };
        let mut shapes = Vec::with_capacity(4);
        if selected {
            shapes.push(Shape::Box {
                corners,
                fill: p.period_fill,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        let layer_ink = if selected {
            p.period_selected_hover
        } else {
            p.period_hover
        };
        if let Some(layer) = self.layer(cx, id, layer_ink) {
            shapes.push(Shape::Box {
                corners,
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        shapes.push(Shape::Box {
            corners,
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((p.period_edge, 1.0)),
            shadow: None,
            image: None,
        });
        let state = cx.state(id);
        if state.activation() && !self.disabled {
            let period = if am { Period::Am } else { Period::Pm };
            self.fired = Some(TimePickerChange::Period(period));
        }
        Segment {
            id,
            label: label.to_string(),
            shapes,
            ink,
            ring: state.focus_visible.then_some(p.focus_ring),
        }
    }

    /// The title band above the fields.
    fn title_decl(&self) -> Decl {
        let p = &self.palette;
        Decl::container(self.id.with("title-band"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(LABEL_BAND),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .child(
                Text::new(
                    self.id.with("title"),
                    self.title.clone(),
                    p.title_type.size,
                    p.title,
                )
                .weight(p.title_type.weight)
                .resolve(),
            )
    }

    /// One field with its supporting label below it.
    fn field_decl(&self, cx: &Cx<'_, '_>, input: &Input, label: &str) -> Decl {
        let p = &self.palette;
        let (w, h) = TIME_INPUT_FIELD;
        let text_top = ((h - self.line.digit) * 0.5).max(0.0);
        // Centring is the parent's `cross_pos`; a second `TextAlign::Center` would shift
        // the line by the spare width twice.
        let mut text = Text::new(
            input.id.with("digits"),
            input.shown.clone(),
            p.digit_type.size,
            input.ink,
        )
        .weight(p.digit_type.weight)
        .nowrap();
        if let Some(offset) = input.caret.filter(|_| !input.shown.is_empty()) {
            text = text.caret(offset.min(input.shown.len()), input.ink);
        }
        let text = text
            .composing(input.composing.clone())
            .painting(input.painted.clone());
        // A line wider than the field is pinned left and scrolled to its caret; one that fits
        // stays centred as the export shows it.
        let wider = text.width_in(cx).is_some_and(|line| line > w);
        let text = if wider {
            text.resolve()
                .offset(-caret_scroll(cx, input.id.with("digits"), w), 0.0)
        } else {
            text.resolve()
        };
        let content = Decl::container(input.id.with("content"), NodeKind::Column)
            .size(SizeClass::fixed(w, h))
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Visible,
            })
            .padding(Insets {
                left: 0.0,
                top: text_top,
                right: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(if wider { 0.0 } else { 0.5 })
            .child(text);
        let mut field = Decl::container(input.id, NodeKind::Stack)
            .size(SizeClass::fixed(w, h))
            .shapes(input.shapes.clone())
            .corners(Corners::all(TIME_INPUT_FIELD_RADIUS))
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::Text
            })
            .disabled(self.disabled)
            .text_input(input.text.clone())
            .text_input_purpose(ui_core::ime::ContentPurpose::Time)
            .child(content);
        if input.focused && input.text.is_empty() && input.composing.is_none() {
            // An empty buffer has no span to shape, so its caret is a box in the centre.
            let caret_h = self.line.digit;
            let cursor = Decl::container(input.id.with("caret"), NodeKind::Box)
                .size(SizeClass::fixed(CARET_WIDTH, caret_h))
                .shapes(vec![Shape::Box {
                    corners: Corners::ZERO,
                    fill: input.ink,
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                }]);
            field = field.child(
                Decl::container(input.id.with("caret-slot"), NodeKind::Stack)
                    .size(SizeClass::fixed(w, h))
                    .padding(Insets {
                        left: (w - CARET_WIDTH) * 0.5,
                        top: ((h - caret_h) * 0.5).max(0.0),
                        right: 0.0,
                        bottom: 0.0,
                    })
                    .child(cursor),
            );
        }
        let caption = Decl::container(input.id.with("label"), NodeKind::Column)
            .size(SizeClass::fixed(w, LABEL_BAND))
            .padding(Insets {
                left: 0.0,
                top: ((LABEL_BAND - self.line.supporting) * 0.5).max(0.0),
                right: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(
                Text::new(
                    input.id.with("label-text"),
                    label.to_string(),
                    p.supporting_type.size,
                    p.supporting,
                )
                .weight(p.supporting_type.weight)
                .resolve(),
            );
        Decl::container(input.id.with("column"), NodeKind::Column)
            .size(SizeClass::fixed(w, h + LABEL_GAP + LABEL_BAND))
            .cross_align(1.0)
            .gap(LABEL_GAP)
            .child(field)
            .child(caption)
    }

    /// The `:` between the two fields.
    fn separator_decl(&self) -> Decl {
        let p = &self.palette;
        let top = ((TIME_INPUT_FIELD.1 - self.line.separator) * 0.5).max(0.0);
        Decl::container(self.id.with("separator"), NodeKind::Column)
            .size(SizeClass::fixed(TIME_INPUT_SEPARATOR, TIME_INPUT_FIELD.1))
            .padding(Insets {
                left: 0.0,
                top,
                right: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(
                Text::new(
                    self.id.with("separator/text"),
                    ":".to_string(),
                    p.separator_type.size,
                    p.separator,
                )
                .weight(p.separator_type.weight)
                .resolve(),
            )
    }

    /// The stacked AM/PM selector.
    fn period_decl(&self) -> Decl {
        Decl::container(self.id.with("period"), NodeKind::Column)
            .size(SizeClass::fixed(TIME_INPUT_PERIOD.0, TIME_INPUT_PERIOD.1))
            .child(self.segment_decl(&self.am, true))
            .child(self.segment_decl(&self.pm, false))
    }

    /// One AM or PM tile.
    fn segment_decl(&self, segment: &Segment, am: bool) -> Decl {
        let p = &self.palette;
        let (w, h) = TIME_INPUT_PERIOD;
        let tile_h = h * 0.5;
        let top = ((tile_h - self.line.period) * 0.5).max(0.0);
        let mut shapes = segment.shapes.clone();
        if let Some(ring) = segment.ring {
            shapes.push(Shape::Box {
                corners: period_corners_inset(am, p.focus_ring_offset),
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((ring, p.focus_ring_width)),
                shadow: None,
                image: None,
            });
        }
        let label = Text::new(
            segment.id.with("label"),
            segment.label.clone(),
            p.period_type.size,
            segment.ink,
        )
        .weight(p.period_type.weight)
        .resolve();
        Decl::container(segment.id, NodeKind::Column)
            .size(SizeClass::fixed(w, tile_h))
            .padding(Insets {
                left: 0.0,
                top,
                right: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .shapes(shapes)
            .corners(period_corners(am))
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink((!self.disabled).then(|| {
                let ink = segment.ink;
                press_ink(
                    Color {
                        a: ink.a * p.press_opacity,
                        ..ink
                    },
                    InkRadius::Fixed(TIME_INPUT_PERIOD_RADIUS),
                )
            }))
            .child(label)
    }
}

impl Input {
    fn placeholder(id: Id) -> Self {
        Self {
            id,
            text: "".into(),
            shown: String::new(),
            composing: None,
            caret: None,
            painted: None,
            focused: false,
            ink: Color::TRANSPARENT,
            shapes: Vec::new(),
        }
    }
}

impl Segment {
    fn placeholder(id: Id) -> Self {
        Self {
            id,
            label: String::new(),
            shapes: Vec::new(),
            ink: Color::TRANSPARENT,
            ring: None,
        }
    }
}

/// The corners of one selector tile: AM keeps the top pair, PM the bottom pair.
fn period_corners(am: bool) -> Corners {
    let r = TIME_INPUT_PERIOD_RADIUS;
    if am {
        Corners {
            tl: r,
            tr: r,
            br: 0.0,
            bl: 0.0,
            shape: ui_core::geom::CornerShape::ROUND,
        }
    } else {
        Corners {
            tl: 0.0,
            tr: 0.0,
            br: r,
            bl: r,
            shape: ui_core::geom::CornerShape::ROUND,
        }
    }
}

/// The same tile corners pulled in by `inset`, for the focus ring.
fn period_corners_inset(am: bool, inset: f32) -> Corners {
    let mut corners = period_corners(am);
    let r = (TIME_INPUT_PERIOD_RADIUS - inset).max(0.0);
    if am {
        corners.tl = r;
        corners.tr = r;
    } else {
        corners.br = r;
        corners.bl = r;
    }
    corners
}

/// The buffer with the input method's composition spliced in, and where the caret sits in it.
fn compose(
    text: &str,
    caret: Option<usize>,
    preedit: Option<ui_core::Preedit>,
) -> (String, Option<std::ops::Range<usize>>, Option<usize>) {
    match preedit {
        Some(p) if !p.is_empty() => {
            let at = caret.unwrap_or(text.len()).min(text.len());
            let mut shown = String::with_capacity(text.len() + p.text.len());
            shown.push_str(&text[..at]);
            shown.push_str(&p.text);
            shown.push_str(&text[at..]);
            let caret = at + p.cursor.min(p.text.len());
            (shown, Some(at..at + p.text.len()), Some(caret))
        }
        _ => (text.to_string(), None, caret),
    }
}

/// The height one line occupies at `size`, as the text engine measures it.
fn measure_line(size: f32) -> f32 {
    Text::new(
        Id::str("time-input/probe"),
        "0".to_string(),
        size,
        Color::WHITE,
    )
    .resolve()
    .size_class
    .height
    .hint
}

impl Widget for Md3TimeInput {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let p = &self.palette;
        let row_height = TIME_INPUT_FIELD.1 + LABEL_GAP + LABEL_BAND;
        let row = Decl::container(self.id.with("row"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(row_height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .cross_pos(0.0)
            .child(self.field_decl(cx, &self.hour, &self.hour_label))
            .child(self.separator_decl())
            .child(self.field_decl(cx, &self.minute, &self.minute_label))
            .child(
                Decl::container(self.id.with("period-gap"), NodeKind::Column)
                    .size(SizeClass::fixed(PERIOD_GAP, 1.0)),
            )
            .child(self.period_decl());
        Decl::container(self.id, NodeKind::Column)
            .size(SizeClass::fixed(TIME_INPUT_WIDTH, TIME_INPUT_HEIGHT))
            .padding(Insets {
                left: TIME_INPUT_PAD,
                top: TIME_INPUT_PAD,
                right: TIME_INPUT_PAD,
                bottom: TIME_INPUT_PAD,
            })
            .cross_align(0.0)
            .gap(TITLE_GAP)
            .shapes(vec![Shape::Box {
                corners: Corners::all(TIME_INPUT_RADIUS),
                fill: p.surface,
                gradient: None,
                border: None,
                shadow: Some(ShadowSpec::elevation(p.elevation.clamp(0.0, 8.0) as u8)),
                image: None,
            }])
            .child(self.title_decl())
            .child(row)
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        Some(Change::Data(Box::new(self.fired.clone()?)))
    }
}
