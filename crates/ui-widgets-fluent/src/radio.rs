//! The Fluent radio button: a 20 pt circle with a dot that grows toward the eye.

use ui_core::draw::{FocusVisual, Shape};
use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind};

use ui_style::{ControlKind, Density, MotionName, Style, TextRole};
use ui_widgets::{NodeCx, Widget, WidgetState};

use crate::paint::role_of;
use crate::text::LabelText;

/// Everything about one radio button that is fixed when it is built.
#[derive(Clone, Debug)]
pub struct RadioParams {
    pub id: Id,
    pub label: String,
    pub selected: bool,
    pub disabled: bool,
}

impl RadioParams {
    pub fn new(id: Id, label: impl Into<String>, selected: bool) -> Self {
        Self {
            id,
            label: label.into(),
            selected,
            disabled: false,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// A resolved radio button.
pub struct FluentRadio {
    id: Id,
    label: Option<LabelText>,
    selected: bool,
    fill: Color,
    stroke: Color,
    dot_fill: Color,
    dot: f32,
    press_dot: f32,
    press_opacity: f32,
    ring: f32,
    border_w: f32,
    focus: FocusVisual,
    size: SizeClass,
    disabled: bool,
}

impl FluentRadio {
    pub fn new(params: RadioParams, cx: &mut NodeCx<'_>, pack: &Style) -> Self {
        let RadioParams {
            id,
            label,
            selected,
            disabled: off,
        } = params;
        let st = cx.state();
        let disabled = off || st.disabled;
        let (hovered, pressed) = (st.hovered && !disabled, st.pressed && !disabled);
        let m = pack
            .controls
            .get(ControlKind::RadioButton, Density::Standard);
        // The dot's own animation: 12 rest, 14 hovered, 10 pressed, over 250 ms.
        let normal = pack.motion.get(MotionName::Standard).spec;
        let fast = pack.motion.get(MotionName::Quick).spec;
        let dot_target = if pressed {
            GLYPH_PRESSED
        } else if hovered {
            GLYPH_HOVER
        } else {
            GLYPH
        };
        let dot = cx.animate(id.with("dot"), dot_target, normal);
        let press_dot = cx.animate(id.with("press-dot"), if pressed { 10.0 } else { 4.0 }, fast);
        let press_opacity = cx.animate(id.with("press-opacity"), f32::from(pressed), fast);
        let (fill_name, stroke_name, dot_name) = if disabled {
            (
                "RadioButtonOuterEllipseFillDisabled",
                "RadioButtonOuterEllipseStrokeDisabled",
                "RadioButtonCheckGlyphFillDisabled",
            )
        } else if selected {
            if pressed {
                (
                    "RadioButtonOuterEllipseCheckedFillPressed",
                    "RadioButtonOuterEllipseCheckedStrokePressed",
                    "RadioButtonCheckGlyphFillPressed",
                )
            } else if hovered {
                (
                    "RadioButtonOuterEllipseCheckedFillPointerOver",
                    "RadioButtonOuterEllipseCheckedStrokePointerOver",
                    "RadioButtonCheckGlyphFillPointerOver",
                )
            } else {
                (
                    "RadioButtonOuterEllipseCheckedFill",
                    "RadioButtonOuterEllipseCheckedStroke",
                    "RadioButtonCheckGlyphFill",
                )
            }
        } else if pressed {
            (
                "RadioButtonOuterEllipseFillPressed",
                "RadioButtonOuterEllipseStrokePressed",
                "RadioButtonCheckGlyphFill",
            )
        } else if hovered {
            (
                "RadioButtonOuterEllipseFillPointerOver",
                "RadioButtonOuterEllipseStrokePointerOver",
                "RadioButtonCheckGlyphFill",
            )
        } else {
            (
                "RadioButtonOuterEllipseFill",
                "RadioButtonOuterEllipseStroke",
                "RadioButtonCheckGlyphFill",
            )
        };
        let label_name = if disabled {
            "RadioButtonForegroundDisabled"
        } else if pressed {
            "RadioButtonForegroundPressed"
        } else if hovered {
            "RadioButtonForegroundPointerOver"
        } else {
            "RadioButtonForeground"
        };
        Self {
            id,
            label: LabelText::new(
                pack,
                TextRole::Body,
                &label,
                role_of(pack, label_name),
                f32::INFINITY,
            ),
            selected,
            fill: role_of(pack, fill_name),
            stroke: role_of(pack, stroke_name),
            dot_fill: role_of(pack, dot_name),
            dot,
            press_dot,
            press_opacity: press_opacity.clamp(0.0, 1.0),
            ring: RING,
            border_w: m.border,
            focus: FocusVisual {
                outer: role_of(pack, "FocusStrokeColorOuterBrush"),
                outer_width: 2.0,
                inner: role_of(pack, "FocusStrokeColorInnerBrush"),
                inner_width: 1.0,
                margin: m.focus_margin,
                radius: -m.focus_margin.left,
            },
            size: SizeClass {
                width: AxisSize::hug(m.min_width, 0.0),
                height: AxisSize::hug(m.min_height, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            disabled,
        }
    }

    fn circle(&self) -> Decl {
        let side = self.ring;
        let mut shapes = Vec::with_capacity(2);
        if self.selected {
            let r = self.dot * 0.5;
            shapes.push(Shape::Rect {
                rect: Rect::new(side * 0.5 - r, side * 0.5 - r, self.dot, self.dot),
                corners: Corners::all(r),
                fill: self.dot_fill,
                gradient: None,
                border: None,
            });
            if self.press_opacity > 0.0 {
                let r = self.press_dot * 0.5;
                shapes.push(Shape::Rect {
                    rect: Rect::new(
                        side * 0.5 - r,
                        side * 0.5 - r,
                        self.press_dot,
                        self.press_dot,
                    ),
                    corners: Corners::all(r),
                    fill: Color {
                        a: self.dot_fill.a * self.press_opacity,
                        ..self.dot_fill
                    },
                    gradient: None,
                    border: None,
                });
            }
        }
        Decl::container(self.id.with("circle"), NodeKind::Stack)
            .size(SizeClass::fixed(side, side))
            .background(self.fill, Corners::all(side * 0.5))
            .border(self.stroke, self.border_w)
            .shapes(shapes)
    }
}

impl Widget for FluentRadio {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        (!self.disabled && state.clicked).then_some(ui_core::Change::Activated)
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let mut row = Decl::container(self.id, NodeKind::Row)
            .size(self.size)
            .gap(GAP)
            .sense(ui_core::Sense::HOVER | ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
            .cursor(ui_core::CursorShape::Arrow)
            .disabled(self.disabled)
            .cross_align(0.0)
            .cross_pos(0.5)
            .focus_visual(Some(self.focus))
            .child(self.circle());
        if let Some(label) = &self.label {
            row = row.child(label.decl(self.id.with("label")));
        }
        row
    }
}

/// The 20 pt outer circle.
pub const RING: f32 = 20.0;
/// The dot at rest, hovered and pressed.
pub const GLYPH: f32 = 12.0;
pub const GLYPH_HOVER: f32 = 14.0;
pub const GLYPH_PRESSED: f32 = 10.0;
/// `RadioButtonPadding`'s left, i.e. the space between circle and label.
pub const GAP: f32 = 8.0;
