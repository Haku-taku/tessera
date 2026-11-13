//! Fluent buttons: Default, Accent, Subtle and TextLink, from one XAML template family.

use ui_core::draw::FocusVisual;
use ui_core::geom::{Color, Corners};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind};

use ui_style::{ControlKind, Density, MotionName, Style, TextRole};
use ui_widgets::build::param;
use ui_widgets::{NodeCx, Widget, WidgetState};

use crate::paint::{Stroke, role_of, stroke_of};
use crate::text::LabelText;

/// Which of the three `<Style x:Key>` blocks in `Button_themeresources.xaml` this button is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    /// `DefaultButtonStyle`: a translucent control fill under an elevation border.
    Default,
    /// `AccentButtonStyle`: the accent fill, no elevation.
    Accent,
    /// `SubtleButtonStyle`: transparent until pointed at.
    Subtle,
    /// `DefaultHyperlinkButtonStyle`: accent ink, clear until pointed at.
    TextLink,
}

/// The four `CommonStates` each template answers, and the three brushes it swaps.
struct Variant {
    background: [&'static str; 4],
    foreground: [&'static str; 4],
    border: [&'static str; 4],
}

use crate::spec::{DISABLED, POINTER_OVER, PRESSED, REST};

fn variant_of(v: ButtonVariant) -> Variant {
    match v {
        ButtonVariant::Default => Variant {
            background: [
                "ButtonBackground",
                "ButtonBackgroundPointerOver",
                "ButtonBackgroundPressed",
                "ButtonBackgroundDisabled",
            ],
            foreground: [
                "ButtonForeground",
                "ButtonForegroundPointerOver",
                "ButtonForegroundPressed",
                "ButtonForegroundDisabled",
            ],
            border: [
                "ButtonBorderBrush",
                "ButtonBorderBrushPointerOver",
                "ButtonBorderBrushPressed",
                "ButtonBorderBrushDisabled",
            ],
        },
        ButtonVariant::Accent => Variant {
            background: [
                "AccentButtonBackground",
                "AccentButtonBackgroundPointerOver",
                "AccentButtonBackgroundPressed",
                "AccentButtonBackgroundDisabled",
            ],
            foreground: [
                "AccentButtonForeground",
                "AccentButtonForegroundPointerOver",
                "AccentButtonForegroundPressed",
                "AccentButtonForegroundDisabled",
            ],
            border: [
                "AccentButtonBorderBrush",
                "AccentButtonBorderBrushPointerOver",
                "AccentButtonBorderBrushPressed",
                "AccentButtonBorderBrushDisabled",
            ],
        },
        ButtonVariant::Subtle => Variant {
            background: [
                "SubtleButtonBackground",
                "SubtleButtonBackgroundPointerOver",
                "SubtleButtonBackgroundPressed",
                "SubtleButtonBackgroundDisabled",
            ],
            foreground: [
                "SubtleButtonForeground",
                "SubtleButtonForegroundPointerOver",
                "SubtleButtonForegroundPressed",
                "SubtleButtonForegroundDisabled",
            ],
            border: [
                "SubtleButtonBorderBrush",
                "SubtleButtonBorderBrushPointerOver",
                "SubtleButtonBorderBrushPressed",
                "SubtleButtonBorderBrushDisabled",
            ],
        },
        // `HyperlinkButton_themeresources.xaml` is its own file: accent ink, the same
        // subtle fills, and a stroke that stays clear in every state.
        ButtonVariant::TextLink => Variant {
            background: [
                "HyperlinkButtonBackground",
                "HyperlinkButtonBackgroundPointerOver",
                "HyperlinkButtonBackgroundPressed",
                "HyperlinkButtonBackgroundDisabled",
            ],
            foreground: [
                "HyperlinkButtonForeground",
                "HyperlinkButtonForegroundPointerOver",
                "HyperlinkButtonForegroundPressed",
                "HyperlinkButtonForegroundDisabled",
            ],
            border: [
                "HyperlinkButtonBorderBrush",
                "HyperlinkButtonBorderBrushPointerOver",
                "HyperlinkButtonBorderBrushPressed",
                "HyperlinkButtonBorderBrushDisabled",
            ],
        },
    }
}

/// Everything about one button that is fixed when it is built.
#[derive(Clone, Debug)]
pub struct ButtonParams {
    pub id: Id,
    pub label: String,
    pub variant: ButtonVariant,
    /// Build-time disabled, for a sample that must not respond before the first frame.
    pub disabled: bool,
    /// `HorizontalAlignment=Stretch`: take the column the parent gives, like a dialog action.
    pub fill_width: bool,
}

impl ButtonParams {
    /// A plain button, the template's own default.
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            variant: ButtonVariant::Default,
            disabled: false,
            fill_width: false,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn fill_width(mut self) -> Self {
        self.fill_width = true;
        self
    }
}

/// A resolved button: every colour, stroke and metric the decl needs.
pub struct FluentButton {
    id: Id,
    label: Option<LabelText>,
    fill: Color,
    stroke: Stroke,
    border_w: f32,
    pad: ui_core::geom::Insets,
    corners: Corners,
    focus: Option<FocusVisual>,
    size: SizeClass,
    /// `HorizontalAlignment=Stretch`; a stretched button centres its label.
    stretch: bool,
    disabled: bool,
}

impl FluentButton {
    pub fn new(params: ButtonParams, cx: &mut NodeCx<'_>, pack: &Style) -> Self {
        let ButtonParams {
            id,
            label,
            variant,
            disabled: off,
            fill_width,
        } = params;
        let state = cx.state();
        let disabled = off || state.disabled;
        let slot = if disabled {
            DISABLED
        } else if state.pressed {
            PRESSED
        } else if state.hovered {
            POINTER_OVER
        } else {
            REST
        };
        let v = variant_of(variant);
        // WinUI transitions Background with a 83 ms BrushTransition; Foreground and
        // BorderBrush swap on a discrete key frame, so only the fill is animated.
        let colour_spec = pack.motion.get(MotionName::ColourChange).spec;
        let fill = cx.animate_colour(
            id.with(param::COLOUR),
            role_of(pack, v.background[slot]),
            colour_spec,
        );
        let stroke = stroke_of(pack, v.border[slot]);
        let m = pack.controls.get(ControlKind::Button, Density::Standard);
        let corners = Corners::all(m.radius).with_shape(pack.metrics.control.corner);
        let focus = Some(FocusVisual {
            outer: role_of(pack, "FocusStrokeColorOuterBrush"),
            outer_width: 2.0,
            inner: role_of(pack, "FocusStrokeColorInnerBrush"),
            inner_width: 1.0,
            margin: m.focus_margin,
            // The adorner reaches `focus_margin` outside, so its curve grows by as much.
            radius: -m.focus_margin.left,
        });
        Self {
            id,
            label: LabelText::new(
                pack,
                TextRole::Body,
                &label,
                role_of(pack, v.foreground[slot]),
                f32::INFINITY,
            ),
            fill,
            stroke,
            border_w: m.border,
            pad: m.padding,
            corners,
            focus,
            stretch: fill_width,
            size: if fill_width {
                SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::hug(m.min_height, 0.0),
                    // A grid `*` column: equal shares whatever the label measures.
                    grow_x: Grow::Split(1),
                    grow_y: Grow::No,
                }
            } else {
                SizeClass {
                    width: AxisSize::hug(m.min_width, 0.0),
                    height: AxisSize::hug(m.min_height, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                }
            },
            disabled,
        }
    }
}

#[cfg(test)]
impl FluentButton {
    /// The fill this frame settled on, for the tests that pin which brush family was read.
    pub(crate) fn fill(&self) -> Color {
        self.fill
    }
}

impl Widget for FluentButton {
    fn handles_disabled(&self) -> bool {
        // Disabled brushes are painted here; the adapter must not fade a second time.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        (!self.disabled && state.clicked).then_some(ui_core::Change::Activated)
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let mut row = Decl::container(self.id, NodeKind::Row)
            .size(self.size)
            .padding(self.pad)
            .sense(ui_core::Sense::HOVER | ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
            // WinUI buttons keep the arrow: only hyperlinks change the pointer.
            .cursor(ui_core::CursorShape::Arrow)
            .disabled(self.disabled)
            .cross_align(0.0)
            .cross_pos(0.5)
            .corners(self.corners)
            .focus_visual(self.focus)
            .background(self.fill, self.corners);
        row = self.stroke.on(row, self.border_w);
        if let Some(label) = &self.label {
            let label = label.decl(self.id.with("label"));
            row = if self.stretch {
                // WinUI's content presenter centres in a stretched button; two equal growers
                // take the slack on both sides, so no fixed inset is needed.
                let slack = |id: Id| {
                    Decl::container(id, NodeKind::Column).size(SizeClass {
                        width: AxisSize::hug(0.0, 0.0),
                        height: AxisSize::hug(0.0, 0.0),
                        grow_x: Grow::Split(1),
                        grow_y: Grow::No,
                    })
                };
                row.child(slack(self.id.with("label-lead")))
                    .child(label)
                    .child(slack(self.id.with("label-trail")))
            } else {
                row.child(label)
            };
        }
        row
    }
}
