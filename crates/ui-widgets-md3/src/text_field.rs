//! MD3 text fields: one line of input, filled or outlined.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Id, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::LINE_HINT;
use ui_widgets::{Cx, IconSource, Text, Widget, WidgetState};

use crate::Md3Options;
use crate::paint::{lerp, role, spec_ink};
use crate::spec::{
    FilledTextFieldFlags, OutlinedTextFieldFlags, filled_text_field_spec, outlined_text_field_spec,
};

/// MD3's field height; the export's own `container.height` token is deprecated.
pub const TEXT_FIELD_HEIGHT: f32 = 56.0;
/// `rounded_corners` on the MD3 shape scale.
pub const TEXT_FIELD_RADIUS: f32 = 4.0;
/// Content inset when no icon occupies the side: the export's 12 plus the 4dp gap.
pub const TEXT_FIELD_PAD: f32 = 16.0;
/// The leading/trailing icon slot: a 48dp touch target.
pub const TEXT_FIELD_ICON_BOX: f32 = 48.0;
/// Space between an icon slot and the input text.
pub const TEXT_FIELD_ICON_GAP: f32 = 4.0;
/// Outlined fields leave this much above the box for the label's upper half.
pub const TEXT_FIELD_BAND: f32 = 8.0;
/// The label's backdrop reaches this far past its text on both sides: Flutter's gapPadding.
pub(crate) const LABEL_PAD: f32 = 4.0;
/// Gap between the field and its supporting text.
const SUBTEXT_GAP: f32 = 4.0;

/// `_kTransitionDuration` with `fastOutSlowIn`, shared by the label and the state.
const FIELD: AnimSpec = AnimSpec::new(0.167, crate::motion::FAST_OUT_SLOW_IN);
/// The caret's width from MD3, centred on the insertion point.
pub(crate) const CARET_WIDTH: f32 = 2.0;
/// Breathing room kept between a scrolled caret and the edge it just crossed.
pub(crate) const SCROLL_SLACK: f32 = 2.0;

/// How far a single-line field must shift left so its caret clears the right edge.
pub(crate) fn caret_scroll(cx: &Cx<'_, '_>, text_id: Id, width: f32) -> f32 {
    ui_widgets::text::caret_scroll(cx, text_id, width, CARET_WIDTH, SCROLL_SLACK)
}

/// Which field this is; the export has one token table per variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFieldVariant {
    Filled,
    Outlined,
}

/// Everything a text field is built with; the app owns the buffer.
pub struct TextFieldParams<'a> {
    pub id: Id,
    pub variant: TextFieldVariant,
    pub text: &'a str,
    pub label: &'a str,
    /// Shown in place of the label while the field is focused and empty.
    pub placeholder: &'a str,
    /// Helper or error line under the field; empty means none.
    pub supporting: &'a str,
    pub leading: Option<IconSource>,
    pub trailing: Option<IconSource>,
    pub error: bool,
    pub disabled: bool,
    /// A secret buffer: drawn as marks, not copyable, and hidden from the input method.
    pub secure: bool,
    /// Full width, icon slots included.
    pub width: f32,
    /// Rows this field offers on a right click, above the engine's own text rows.
    pub menu: Vec<ui_core::MenuItem>,
}

impl<'a> TextFieldParams<'a> {
    /// A bare field of `width`, with no label, hint, supporting text or icons.
    pub fn new(id: Id, variant: TextFieldVariant, width: f32) -> Self {
        Self {
            id,
            variant,
            text: "",
            label: "",
            placeholder: "",
            supporting: "",
            leading: None,
            trailing: None,
            error: false,
            disabled: false,
            secure: false,
            width,
            menu: Vec::new(),
        }
    }

    /// Rows this field offers on a right click, above the engine's own text rows.
    pub fn menu(mut self, items: Vec<ui_core::MenuItem>) -> Self {
        self.menu = items;
        self
    }

    pub fn text(mut self, text: &'a str) -> Self {
        self.text = text;
        self
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    pub fn placeholder(mut self, hint: &'a str) -> Self {
        self.placeholder = hint;
        self
    }

    pub fn supporting(mut self, text: &'a str) -> Self {
        self.supporting = text;
        self
    }

    pub fn leading(mut self, icon: IconSource) -> Self {
        self.leading = Some(icon);
        self
    }

    pub fn trailing(mut self, icon: IconSource) -> Self {
        self.trailing = Some(icon);
        self
    }

    pub fn error(mut self, error: bool) -> Self {
        self.error = error;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Hide the value: marks on screen, no copy, and no surrounding text for an input method.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }
}

/// One variant's values for one state, normalised across the two exports.
struct Parts {
    fill: &'static str,
    fill_op: f32,
    outline: &'static str,
    outline_op: f32,
    outline_w: f32,
    layer: &'static str,
    layer_op: f32,
    indicator: &'static str,
    indicator_op: f32,
    indicator_h: f32,
    caret: &'static str,
    label: &'static str,
    label_op: f32,
    label_size: f32,
    label_weight: f32,
    float_size: f32,
    input: &'static str,
    input_op: f32,
    input_size: f32,
    input_weight: f32,
    placeholder: &'static str,
    supporting: &'static str,
    supporting_op: f32,
    subtext_size: f32,
    subtext_weight: f32,
    leading: &'static str,
    leading_op: f32,
    trailing: &'static str,
    trailing_op: f32,
    icon_size: f32,
}

fn parts(
    variant: TextFieldVariant,
    error: bool,
    hovered: bool,
    focused: bool,
    disabled: bool,
) -> Parts {
    match variant {
        TextFieldVariant::Filled => {
            let v = filled_text_field_spec(FilledTextFieldFlags {
                error,
                hovered,
                focused,
                disabled,
            });
            Parts {
                fill: v.container.color,
                fill_op: v.container.opacity,
                // A filled field has no outline or focus ring; its line is the indicator.
                outline: "",
                outline_op: 0.0,
                outline_w: 0.0,
                layer: v.state_layer.color,
                layer_op: v.state_layer.opacity,
                indicator: v.active_indicator.color,
                indicator_op: v.active_indicator.opacity,
                indicator_h: v.active_indicator.height,
                caret: v.caret.color,
                label: v.label_text.color,
                label_op: v.label_text.opacity,
                label_size: v.label_text.size,
                label_weight: v.label_text.weight,
                float_size: v.label_text_populated.size,
                input: v.input_text.color,
                input_op: v.input_text.opacity,
                input_size: v.input_text.size,
                input_weight: v.input_text.weight,
                placeholder: v.input_text_placeholder.color,
                supporting: v.supporting_text.color,
                supporting_op: v.supporting_text.opacity,
                subtext_size: v.supporting_text.size,
                subtext_weight: v.supporting_text.weight,
                leading: v.leading_icon.color,
                leading_op: v.leading_icon.opacity,
                trailing: v.trailing_icon.color,
                trailing_op: v.trailing_icon.opacity,
                icon_size: v.leading_icon.size,
            }
        }
        TextFieldVariant::Outlined => {
            let v = outlined_text_field_spec(OutlinedTextFieldFlags {
                error,
                hovered,
                focused,
                disabled,
            });
            Parts {
                // An outlined field has no fill, layer or indicator; the outline is the state.
                fill: "",
                fill_op: 0.0,
                outline: v.outline.color,
                outline_op: v.outline.opacity,
                outline_w: v.outline.width,
                layer: "",
                layer_op: 0.0,
                indicator: "",
                indicator_op: 0.0,
                indicator_h: 0.0,
                caret: v.caret.color,
                label: v.label_text.color,
                label_op: v.label_text.opacity,
                label_size: v.label_text.size,
                label_weight: v.label_text.weight,
                float_size: v.label_text_populated.size,
                input: v.input_text.color,
                input_op: v.input_text.opacity,
                input_size: v.input_text.size,
                input_weight: v.input_text.weight,
                placeholder: v.input_text_placeholder.color,
                supporting: v.supporting_text.color,
                supporting_op: v.supporting_text.opacity,
                subtext_size: v.supporting_text.size,
                subtext_weight: v.supporting_text.weight,
                leading: v.leading_icon.color,
                leading_op: v.leading_icon.opacity,
                trailing: v.trailing_icon.color,
                trailing_op: v.trailing_icon.opacity,
                icon_size: v.leading_icon.size,
            }
        }
    }
}

/// A text field; the engine edits the buffer and reports it, the app owns it.
pub struct Md3TextField {
    id: Id,
    variant: TextFieldVariant,
    width: f32,
    text: std::sync::Arc<str>,
    label: String,
    placeholder: String,
    supporting: String,
    leading: Option<(IconSource, Color)>,
    trailing: Option<(IconSource, Color)>,
    icon_size: f32,
    fill: Color,
    outline: Option<(Color, f32)>,
    layer: Option<Color>,
    indicator: Option<(Color, f32)>,
    label_ink: Color,
    label_weight: Weight,
    input_ink: Color,
    placeholder_ink: Color,
    supporting_ink: Color,
    caret_ink: Color,
    label_size: f32,
    float_size: f32,
    input_size: f32,
    subtext_size: f32,
    input_weight: Weight,
    subtext_weight: Weight,
    label_t: f32,
    /// The caret inside the buffer, when this field holds focus.
    caret: Option<usize>,
    /// The selection inside the buffer, painted by the pack's own painter.
    painted: Option<ui_core::SelectionPrim>,
    /// The input method's composition, drawn at the caret but never merged into the buffer.
    preedit: Option<ui_core::Preedit>,
    focused: bool,
    disabled: bool,
    secure: bool,
    /// Rows this field offers on a right click.
    menu: Vec<ui_core::MenuItem>,
}

impl Md3TextField {
    /// Build one field. The caret comes from the engine, since it owns it.
    pub fn new_with(
        params: Md3Options,
        content: TextFieldParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let TextFieldParams {
            id,
            variant,
            text,
            label,
            placeholder,
            supporting,
            leading,
            trailing,
            error,
            disabled: off,
            secure,
            width,
            menu,
        } = content;
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
        let mut node = cx.at(id);
        let state = node.state();
        let disabled = params.disabled || off || state.disabled;
        let focused = state.focused && !disabled;
        let rest = parts(variant, error, state.hovered, focused, disabled);
        let hover = parts(variant, error, true, focused, disabled);
        let unfocused = parts(variant, error, state.hovered, false, disabled);
        let focused_spec = parts(variant, error, state.hovered, true, disabled);
        let hover_t = node.animate(id.with(param::HOVER), f32::from(state.hovered), FIELD);
        let focus_t = node.animate(id.with(param::THICKNESS), f32::from(focused), FIELD);
        // The label floats once the field is focused or holds text; the outline thickens
        // on focus alone, so the two travel on separate clocks.
        let populated = focused || !text.is_empty();
        let label_t = node.animate(id.with(param::PROGRESS), f32::from(populated), FIELD);

        // Only the filled variant names a state layer; an empty token paints nothing.
        let layer = (!hover.layer.is_empty()).then(|| {
            let ink = role(pack, hover.layer);
            let strength = hover_t * hover.layer_op;
            Color {
                a: ink.a * strength,
                ..ink
            }
        });
        let layer = layer.filter(|c| c.a > 0.0 && !disabled);
        let outline_w = lerp(unfocused.outline_w, focused_spec.outline_w, focus_t);
        let indicator_h = lerp(unfocused.indicator_h, focused_spec.indicator_h, focus_t);
        let outline = (outline_w > 0.0 && !rest.outline.is_empty())
            .then(|| (spec_ink(pack, rest.outline, rest.outline_op), outline_w));
        let indicator = (indicator_h > 0.0 && !rest.indicator.is_empty()).then(|| {
            (
                spec_ink(pack, rest.indicator, rest.indicator_op),
                indicator_h,
            )
        });
        Self {
            id,
            variant,
            width,
            text: text.into(),
            label: label.to_string(),
            placeholder: placeholder.to_string(),
            supporting: supporting.to_string(),
            leading: leading.map(|f| (f, spec_ink(pack, rest.leading, rest.leading_op))),
            trailing: trailing.map(|f| (f, spec_ink(pack, rest.trailing, rest.trailing_op))),
            icon_size: rest.icon_size,
            fill: spec_ink(pack, rest.fill, rest.fill_op),
            outline,
            layer,
            indicator,
            label_ink: spec_ink(pack, rest.label, rest.label_op),
            label_weight: Weight(rest.label_weight as u16),
            input_ink: spec_ink(pack, rest.input, rest.input_op),
            placeholder_ink: spec_ink(pack, rest.placeholder, 1.0),
            supporting_ink: spec_ink(pack, rest.supporting, rest.supporting_op),
            caret_ink: spec_ink(pack, rest.caret, 1.0),
            label_size: rest.label_size,
            float_size: rest.float_size,
            input_size: rest.input_size,
            subtext_size: rest.subtext_size,
            input_weight: Weight(rest.input_weight as u16),
            subtext_weight: Weight(rest.subtext_weight as u16),
            label_t,
            caret,
            painted,
            preedit,
            focused,
            disabled,
            secure,
            menu,
        }
    }

    /// The variant this field was built as.
    pub fn variant(&self) -> TextFieldVariant {
        self.variant
    }

    /// The height the field occupies, label band and supporting text included.
    pub fn height(&self) -> f32 {
        self.band() + TEXT_FIELD_HEIGHT + self.subtext_height()
    }

    /// The buffer this field is showing this frame.
    pub fn text(&self) -> &str {
        &self.text
    }

    fn band(&self) -> f32 {
        match self.variant {
            TextFieldVariant::Outlined => TEXT_FIELD_BAND,
            TextFieldVariant::Filled => 0.0,
        }
    }

    fn subtext_height(&self) -> f32 {
        if self.supporting.is_empty() {
            0.0
        } else {
            SUBTEXT_GAP + self.subtext_size * 1.4
        }
    }

    fn content_insets(&self) -> (f32, f32) {
        let left = if self.leading.is_some() {
            TEXT_FIELD_ICON_GAP
        } else {
            TEXT_FIELD_PAD
        };
        let right = if self.trailing.is_some() {
            TEXT_FIELD_ICON_GAP
        } else {
            TEXT_FIELD_PAD
        };
        (left, right)
    }

    /// The fill, the hover layer, the filled indicator and the outline, in paint order.
    ///
    /// `gap` is where the floating label interrupts the outline's top edge; the outlined
    /// border is therefore a separate shape, since a box's border cannot be cut.
    fn field_shapes(&self, gap: Option<(f32, f32)>) -> Vec<Shape> {
        let corners = match self.variant {
            TextFieldVariant::Outlined => Corners::all(TEXT_FIELD_RADIUS),
            // Filled: only the top corners are round, as UnderlineInputBorder does.
            TextFieldVariant::Filled => Corners {
                br: 0.0,
                bl: 0.0,
                ..Corners::all(TEXT_FIELD_RADIUS)
            },
        };
        let outlined = self.variant == TextFieldVariant::Outlined;
        let mut shapes = Vec::with_capacity(4);
        shapes.push(Shape::Box {
            corners,
            fill: self.fill,
            gradient: None,
            border: if outlined { None } else { self.outline },
            shadow: None,
            image: None,
        });
        if let Some(layer) = self.layer {
            shapes.push(Shape::Box {
                corners,
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        if let Some((ink, h)) = self.indicator {
            shapes.push(Shape::Rect {
                rect: Rect::new(0.0, TEXT_FIELD_HEIGHT - h, self.width, h),
                corners: Corners::ZERO,
                fill: ink,
                gradient: None,
                border: None,
            });
        }
        if let (true, Some((ink, width))) = (outlined, self.outline) {
            shapes.push(Shape::Outline {
                corners,
                gradient: None,
                colour: ink,
                width,
                gap,
            });
        }
        shapes
    }

    /// The interval the floating label cuts out of the outline's top edge.
    ///
    /// Flutter's `gapStart - gapPadding` and `(gapPadding * 2 + labelWidth) * gapPercentage`,
    /// so the gap opens with the label instead of snapping at the end of the float.
    fn outline_gap(&self, width: Option<f32>, line_h: f32, block_top: f32) -> Option<(f32, f32)> {
        let outlined = self.variant == TextFieldVariant::Outlined;
        let t = self.label_t.clamp(0.0, 1.0);
        if !outlined || self.label.is_empty() || self.outline.is_none() || t <= 0.0 {
            return None;
        }
        let (x, _, _) = self.label_geometry(line_h, block_top);
        let from = x.max(0.0);
        Some((from, from + (width? + 2.0 * LABEL_PAD) * t))
    }

    /// The box the engine measures for the label at `size`.
    fn label_box(&self, cx: &Cx<'_, '_>, size: f32) -> Option<(f32, f32)> {
        if self.label.is_empty() {
            return None;
        }
        Text::new(
            self.id.with("label-measure"),
            self.label.clone(),
            size,
            self.label_ink,
        )
        .weight(self.label_weight)
        .size_in(cx)
        .map(|m| (m.w, m.h))
    }

    /// The line box the input takes, measured the way layout measures it.
    fn input_line_h(&self, cx: &Cx<'_, '_>) -> f32 {
        let floor = self.input_size * LINE_HINT;
        Text::new(
            self.id.with("input-measure"),
            "M".to_string(),
            self.input_size,
            self.input_ink,
        )
        .weight(self.input_weight)
        .size_in(cx)
        .map_or(floor, |m| m.h.max(floor))
    }

    /// The label's left edge, top edge and text size at the current float progress.
    ///
    /// `block_top` is where the filled variant's label-and-line block starts once it floats.
    fn label_geometry(&self, h: f32, block_top: f32) -> (f32, f32, f32) {
        let t = self.label_t.clamp(0.0, 1.0);
        let size = lerp(self.label_size, self.float_size, t);
        let band = self.band();
        // Resting, the label is the whole block, so it takes the field's centre; it leaves that
        // centre only as the input line joins the block.
        let rest_top = band + (TEXT_FIELD_HEIGHT - h) * 0.5;
        let float_top = match self.variant {
            // Centred on the top border, so half the label sits above the box.
            TextFieldVariant::Outlined => band - h * 0.5,
            TextFieldVariant::Filled => block_top,
        };
        // The mask clips at the widget's edge, so a sub-pixel overhang is pulled down.
        let top = lerp(rest_top, float_top, t).max(0.0);
        let rest_x = if self.leading.is_some() {
            TEXT_FIELD_ICON_BOX + TEXT_FIELD_ICON_GAP
        } else {
            TEXT_FIELD_PAD
        };
        (lerp(rest_x, TEXT_FIELD_PAD, t) - LABEL_PAD, top, size)
    }
}

/// The composition splice every editor that draws live input shares.
pub(crate) use ui_widgets::text::typed_input;

/// One icon slot: a 48dp box that centres the mark in both axes.
fn icon_slot(id: Id, size: f32, source: IconSource, ink: Color) -> Decl {
    // The mark's em box centres in the slot; padding off `Text`'s line box would hang it.
    Decl::container(id, NodeKind::Stack)
        .size(SizeClass::fixed(TEXT_FIELD_ICON_BOX, TEXT_FIELD_HEIGHT))
        .stack_align(ui_core::layout::OverlayAlign::Center)
        .child(source.resolve_with_size(id.with("icon"), ink, size))
}

impl Widget for Md3TextField {
    fn handles_disabled(&self) -> bool {
        // The export carries the disabled colours itself, so the adapter must not fade again.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        // An edit reaches the app as `Change::Text` from the engine; a click here means a
        // focus change, not a value, so this widget never reports one.
        None
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let t = self.label_t.clamp(0.0, 1.0);
        // The label's own measured line box, floored the way layout floors a text leaf, so the
        // resting label centres on the same axis as the input line.
        let label_size = lerp(self.label_size, self.float_size, t);
        let label_box = self.label_box(cx, label_size);
        let label_h = label_box.map_or(label_size * LINE_HINT, |(_, h)| {
            h.max(label_size * LINE_HINT)
        });
        let filled = self.variant == TextFieldVariant::Filled;
        // A filled field centres the floated label and the input line as one block; an outlined
        // one centres the line alone, its label being on the border rather than in the box.
        let (float_label_h, input_h) = if filled && !self.label.is_empty() {
            let float_label_h = self
                .label_box(cx, self.float_size)
                .map_or(self.float_size * LINE_HINT, |(_, h)| {
                    h.max(self.float_size * LINE_HINT)
                });
            (float_label_h, self.input_line_h(cx))
        } else {
            (0.0, 0.0)
        };
        let block_top = (TEXT_FIELD_HEIGHT - float_label_h - input_h) * 0.5;
        let (left_in, right_in) = self.content_insets();
        let slot_w = |present: bool| {
            if present { TEXT_FIELD_ICON_BOX } else { 0.0 }
        };
        let content_w = (self.width
            - slot_w(self.leading.is_some())
            - slot_w(self.trailing.is_some())
            - left_in
            - right_in)
            .max(0.0);

        // The line hugs its own text; the field's row centres it with `cross_pos`, so the
        // typeface's line height decides the position, not a widget constant.
        let mut content = ui_widgets::text::LineEdit {
            id: self.id,
            text: self.text.as_ref(),
            caret: self.caret,
            preedit: self.preedit.as_ref(),
            painted: self.painted.clone(),
            placeholder: &self.placeholder,
            obscure: self.secure,
            hint_always: false,
            focused: self.focused,
            size: self.input_size,
            weight: self.input_weight,
            ink: self.input_ink,
            placeholder_ink: self.placeholder_ink,
            caret_ink: self.caret_ink,
            caret_width: CARET_WIDTH,
            slack: SCROLL_SLACK,
            width: content_w,
            left_inset: left_in,
            right_inset: right_in,
        }
        .resolve(cx);

        if filled && !self.label.is_empty() {
            // The floated label's line is part of the centred block, so the field grows a slot
            // for it and the row's `cross_pos` centres the pair — no line is positioned by hand.
            content = Decl::container(self.id.with("block"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fixed(content_w + left_in + right_in),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .child(
                    Decl::container(self.id.with("label-slot"), NodeKind::Box).size(
                        SizeClass::fixed(content_w + left_in + right_in, float_label_h * t),
                    ),
                )
                .child(content);
        }

        let mut field = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass::fixed(self.width, TEXT_FIELD_HEIGHT))
            .cross_align(0.0)
            // A row's cross axis is vertical: this centres the input line in the field box.
            .cross_pos(0.5)
            .shapes(self.field_shapes(self.outline_gap(
                label_box.map(|(w, _)| w),
                label_h,
                block_top,
            )))
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::Text
            })
            .disabled(self.disabled)
            .text_input(self.text.clone());
        if self.secure {
            field = field.text_input_hidden();
        }
        if let Some((source, ink)) = self.leading {
            field = field.child(icon_slot(
                self.id.with("leading"),
                self.icon_size,
                source,
                ink,
            ));
        }
        field = field.child(content);
        if let Some((source, ink)) = self.trailing {
            field = field.child(icon_slot(
                self.id.with("trailing"),
                self.icon_size,
                source,
                ink,
            ));
        }

        let band = self.band();
        let mut root = Decl::container(self.id.with("root"), NodeKind::Column)
            .size(SizeClass::fixed(self.width, self.height()))
            .cross_align(0.0)
            .cross_pos(0.0)
            .menu(self.menu.clone());
        if band > 0.0 {
            root = root.child(
                Decl::container(self.id.with("band"), NodeKind::Box)
                    .size(SizeClass::fixed(self.width, band)),
            );
        }
        root = root.child(field);
        if !self.supporting.is_empty() {
            let inner_w = (self.width - 2.0 * TEXT_FIELD_PAD).max(0.0);
            root = root.child(
                Decl::container(self.id.with("support"), NodeKind::Column)
                    .size(SizeClass::fixed(self.width, self.subtext_height()))
                    .padding(Insets {
                        left: TEXT_FIELD_PAD,
                        top: SUBTEXT_GAP,
                        right: TEXT_FIELD_PAD,
                        bottom: 0.0,
                    })
                    .cross_align(0.0)
                    .cross_pos(0.0)
                    .child(
                        Text::new(
                            self.id.with("supporting"),
                            self.supporting.clone(),
                            self.subtext_size,
                            self.supporting_ink,
                        )
                        .weight(self.subtext_weight)
                        .max_width(inner_w)
                        .resolve(),
                    ),
            );
        }
        if !self.label.is_empty() {
            let (x, y, size) = self.label_geometry(label_h, block_top);
            let label = Decl::container(self.id.with("label"), NodeKind::Row)
                .padding(Insets {
                    left: LABEL_PAD,
                    top: 0.0,
                    right: LABEL_PAD,
                    bottom: 0.0,
                })
                // The label floats above the box; a click on it belongs to the field.
                .sense(Sense::PASS_THROUGH)
                .cross_align(0.0)
                .cross_pos(0.0)
                .child(
                    Text::new(
                        self.id.with("label-text"),
                        self.label.clone(),
                        size,
                        self.label_ink,
                    )
                    .weight(self.label_weight)
                    .resolve(),
                );
            root = root.child(
                Decl::container(self.id.with("label-layer"), NodeKind::Overlay)
                    .size(SizeClass::fixed(self.width, self.height()))
                    // It covers the whole field, so it must not end the hit search.
                    .sense(Sense::PASS_THROUGH)
                    // The label hugs its text: a stretched row would paint the notch over
                    // the whole top border instead of cutting it around the label.
                    .cross_align(0.0)
                    .padding(Insets {
                        left: x,
                        top: y,
                        right: 0.0,
                        bottom: 0.0,
                    })
                    .child(label),
            );
        }
        root
    }
}
