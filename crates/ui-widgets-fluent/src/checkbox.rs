//! The Fluent check box: a 20 pt square with a 12 pt glyph, 32 pt row.

use ui_core::draw::FocusVisual;
use ui_core::geom::{Color, Corners};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind};

use ui_style::{ControlKind, Density, Style, TextRole};
use ui_widgets::{Icon, NodeCx, Widget, WidgetState};

use crate::glyphs;
use crate::paint::role_of;
use crate::text::LabelText;

/// What the box shows; WinUI calls the third value `Indeterminate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    Indeterminate,
}

/// Everything about one check box that is fixed when it is built.
#[derive(Clone, Debug)]
pub struct CheckBoxParams {
    pub id: Id,
    pub label: String,
    pub state: CheckState,
    pub disabled: bool,
    /// `IsThreeState`: a click cycles Unchecked → Checked → Indeterminate → Unchecked.
    pub three_state: bool,
}

impl CheckBoxParams {
    pub fn new(id: Id, label: impl Into<String>, state: CheckState) -> Self {
        Self {
            id,
            label: label.into(),
            state,
            disabled: false,
            three_state: false,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn three_state(mut self) -> Self {
        self.three_state = true;
        self
    }
}

/// One value state's four brush families, as the template names them.
struct StateBrushes {
    fill: [&'static str; 4],
    stroke: [&'static str; 4],
    glyph: [&'static str; 4],
    label: [&'static str; 4],
}

fn brushes(state: CheckState) -> StateBrushes {
    match state {
        CheckState::Unchecked => StateBrushes {
            fill: [
                "CheckBoxCheckBackgroundFillUnchecked",
                "CheckBoxCheckBackgroundFillUncheckedPointerOver",
                "CheckBoxCheckBackgroundFillUncheckedPressed",
                "CheckBoxCheckBackgroundFillUncheckedDisabled",
            ],
            stroke: [
                "CheckBoxCheckBackgroundStrokeUnchecked",
                "CheckBoxCheckBackgroundStrokeUncheckedPointerOver",
                "CheckBoxCheckBackgroundStrokeUncheckedPressed",
                "CheckBoxCheckBackgroundStrokeUncheckedDisabled",
            ],
            glyph: [
                "CheckBoxCheckGlyphForegroundUnchecked",
                "CheckBoxCheckGlyphForegroundUncheckedPointerOver",
                "CheckBoxCheckGlyphForegroundUncheckedPressed",
                "CheckBoxCheckGlyphForegroundUncheckedDisabled",
            ],
            label: [
                "CheckBoxForegroundUnchecked",
                "CheckBoxForegroundUncheckedPointerOver",
                "CheckBoxForegroundUncheckedPressed",
                "CheckBoxForegroundUncheckedDisabled",
            ],
        },
        CheckState::Checked => StateBrushes {
            fill: [
                "CheckBoxCheckBackgroundFillChecked",
                "CheckBoxCheckBackgroundFillCheckedPointerOver",
                "CheckBoxCheckBackgroundFillCheckedPressed",
                "CheckBoxCheckBackgroundFillCheckedDisabled",
            ],
            stroke: [
                "CheckBoxCheckBackgroundStrokeChecked",
                "CheckBoxCheckBackgroundStrokeCheckedPointerOver",
                "CheckBoxCheckBackgroundStrokeCheckedPressed",
                "CheckBoxCheckBackgroundStrokeCheckedDisabled",
            ],
            glyph: [
                "CheckBoxCheckGlyphForegroundChecked",
                "CheckBoxCheckGlyphForegroundCheckedPointerOver",
                "CheckBoxCheckGlyphForegroundCheckedPressed",
                "CheckBoxCheckGlyphForegroundCheckedDisabled",
            ],
            label: [
                "CheckBoxForegroundChecked",
                "CheckBoxForegroundCheckedPointerOver",
                "CheckBoxForegroundCheckedPressed",
                "CheckBoxForegroundCheckedDisabled",
            ],
        },
        CheckState::Indeterminate => StateBrushes {
            fill: [
                "CheckBoxCheckBackgroundFillIndeterminate",
                "CheckBoxCheckBackgroundFillIndeterminatePointerOver",
                "CheckBoxCheckBackgroundFillIndeterminatePressed",
                "CheckBoxCheckBackgroundFillIndeterminateDisabled",
            ],
            stroke: [
                "CheckBoxCheckBackgroundStrokeIndeterminate",
                "CheckBoxCheckBackgroundStrokeIndeterminatePointerOver",
                "CheckBoxCheckBackgroundStrokeIndeterminatePressed",
                "CheckBoxCheckBackgroundStrokeIndeterminateDisabled",
            ],
            glyph: [
                "CheckBoxCheckGlyphForegroundIndeterminate",
                "CheckBoxCheckGlyphForegroundIndeterminatePointerOver",
                "CheckBoxCheckGlyphForegroundIndeterminatePressed",
                "CheckBoxCheckGlyphForegroundIndeterminateDisabled",
            ],
            label: [
                "CheckBoxForegroundIndeterminate",
                "CheckBoxForegroundIndeterminatePointerOver",
                "CheckBoxForegroundIndeterminatePressed",
                "CheckBoxForegroundIndeterminateDisabled",
            ],
        },
    }
}

/// A resolved check box: every colour and metric the decl needs.
pub struct FluentCheckBox {
    id: Id,
    label: Option<LabelText>,
    state: CheckState,
    fill: Color,
    stroke: Color,
    glyph: Option<(&'static str, Color)>,
    glyph_size: f32,
    side: f32,
    radius: f32,
    border_w: f32,
    focus: FocusVisual,
    size: SizeClass,
    disabled: bool,
    three_state: bool,
}

impl FluentCheckBox {
    pub fn new(params: CheckBoxParams, cx: &mut NodeCx<'_>, pack: &Style) -> Self {
        let CheckBoxParams {
            id,
            label,
            state,
            disabled: off,
            three_state,
        } = params;
        let st = cx.state();
        let disabled = off || st.disabled;
        let slot = if disabled {
            3
        } else if st.pressed {
            2
        } else if st.hovered {
            1
        } else {
            0
        };
        let b = brushes(state);
        let m = pack.controls.get(ControlKind::CheckBox, Density::Standard);
        // The template's down-level glyphs: `CheckBoxCheckedGlyph` and its dash.
        let glyph_name = match state {
            CheckState::Unchecked => None,
            CheckState::Checked => Some("checkmark_12_regular"),
            CheckState::Indeterminate => Some("subtract_12_regular"),
        };
        let glyph_ink = role_of(pack, b.glyph[slot]);
        Self {
            id,
            label: LabelText::new(
                pack,
                TextRole::Body,
                &label,
                role_of(pack, b.label[slot]),
                f32::INFINITY,
            ),
            state,
            fill: role_of(pack, b.fill[slot]),
            stroke: role_of(pack, b.stroke[slot]),
            glyph: glyph_name.map(|n| (n, glyph_ink)),
            glyph_size: m.icon_size,
            side: BOX,
            radius: m.radius,
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
            three_state,
        }
    }

    /// The box itself: the square, with the glyph centred on it when there is one.
    fn box_decl(&self) -> Decl {
        // The slot is the square: the template's `NormalRectangle` is exactly `CheckBoxSize`.
        // The glyph's own box is its em square; centring that box is the slot's job.
        let mut slot = Decl::container(self.id.with("box"), NodeKind::Stack)
            .size(SizeClass::fixed(self.side, self.side))
            .stack_align(ui_core::layout::OverlayAlign::Center)
            .background(self.fill, Corners::all(self.radius))
            .border(self.stroke, self.border_w);
        if let Some((name, ink)) = self.glyph
            && let Some(source) = glyphs::icon(name)
        {
            slot = slot.child(
                Icon {
                    id: self.id.with("glyph"),
                    source,
                    size: self.glyph_size,
                    ink,
                }
                .resolve(),
            );
        }
        slot
    }
}

impl Widget for FluentCheckBox {
    fn handles_disabled(&self) -> bool {
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        if self.disabled || !state.clicked {
            return None;
        }
        // `ToggleButton.OnToggle`: checked goes on to indeterminate before it goes back off.
        let next = match (self.three_state, self.state) {
            (false, _) => Some(self.state != CheckState::Checked),
            (true, CheckState::Unchecked) => Some(true),
            (true, CheckState::Checked) => None,
            (true, CheckState::Indeterminate) => Some(false),
        };
        Some(ui_core::Change::Toggled { on: next })
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
            .child(self.box_decl());
        if let Some(label) = &self.label {
            row = row.child(label.decl(self.id.with("label")));
        }
        row
    }
}

/// `CheckBoxSize`: the square's side.
pub const BOX: f32 = 20.0;

/// The content presenter's left margin, i.e. the space between box and label.
pub const GAP: f32 = 8.0;
