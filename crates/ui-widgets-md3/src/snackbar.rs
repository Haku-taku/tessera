//! MD3 snackbar: an inverse-surface bar with body-medium text and an optional label-large
//! action, fading in and out with the engine's goal-driven animation.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AUTO, AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{AnimSpec, CursorShape, Decl, Easing, Id, InkRadius, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{faded, spec_ink};
use crate::spec::{SnackbarFlags, snackbar_spec};

/// MD3's `corner.extra-small`, which the export names `rounded_corners` without a number.
const CORNER: f32 = 4.0;
/// Flutter's `padding`: 24 fixed, 16 floating.
const SIDE_FIXED: f32 = 24.0;
const SIDE_FLOATING: f32 = 16.0;
/// The action's touch target; it overhangs the bar's 14dp padding, which is visible-safe.
const ACTION_H: f32 = 48.0;
/// material-web's enter: 150ms on `cubic-bezier(0, 0, 0.2, 1)`.
const ENTER: AnimSpec = AnimSpec::new(0.15, Easing::Cubic(0.0, 0.0, 0.2, 1.0));
use crate::motion::REACTION;

/// Everything a snackbar needs besides the design system's own tokens.
pub struct SnackbarParams<'a> {
    pub id: Id,
    pub message: &'a str,
    /// The action's label; a snackbar with an action must not auto-dismiss (the app owns that).
    pub action: Option<&'a str>,
    /// Two lines of body text instead of one, which is the export's 68dp branch.
    pub two_lines: bool,
    /// Floating snackbars inset their text by 16dp instead of 24.
    pub floating: bool,
    /// The export carries no width token; the app gives one.
    pub width: f32,
    /// Whether it is showing; the control fades between the two states.
    pub visible: bool,
    /// Float over the page bottom instead of taking a slot in the flow; the app lays it
    /// out as the last child of a full-size container. `None` leaves it in the flow;
    /// `Some(margin)` pins it to the bottom edge, centred, that far up.
    pub overlay: Option<f32>,
    /// How far in the bar has come: `0` sits wholly below the page's edge, `1` is in place.
    /// Above `1` it hangs past its rest position, which is how a spring's arrival reads.
    pub reveal: f32,
}

/// A snackbar; the app owns when it shows and whether it auto-dismisses.
pub struct Md3Snackbar {
    id: Id,
    message: String,
    action: Option<String>,
    height: f32,
    side: f32,
    width: f32,
    /// The fade, `0..=1`; nothing is painted at 0.
    alpha: f32,
    container: Color,
    text_ink: Color,
    action_ink: Color,
    layer_ink: Color,
    hover_a: f32,
    focus_a: f32,
    press_a: f32,
    shadow: Option<ShadowSpec>,
    text_size: f32,
    text_weight: ui_core::Weight,
    action_size: f32,
    action_weight: ui_core::Weight,
    two_lines: bool,
    /// The action's own hover/focus, read from its node.
    action_hover: f32,
    action_focus: f32,
    action_ring: f32,
    /// The bottom inset when floating over the page; `None` keeps the bar in the flow.
    overlay: Option<f32>,
    /// The arrival's progress, as the caller's clock gives it.
    reveal: f32,
}

impl Md3Snackbar {
    /// Same control with explicit parameters, so the semantic path and a direct caller
    /// produce identical `Decl`s.
    pub fn new_with(
        _params: Md3Options,
        p: SnackbarParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let has_action = p.action.is_some();
        let base = snackbar_spec(SnackbarFlags {
            with_single_line: !p.two_lines,
            with_two_lines: p.two_lines,
            ..SnackbarFlags::NONE
        });
        let hover = snackbar_spec(SnackbarFlags {
            with_single_line: !p.two_lines,
            with_two_lines: p.two_lines,
            hovered: true,
            ..SnackbarFlags::NONE
        });
        let focus = snackbar_spec(SnackbarFlags {
            with_single_line: !p.two_lines,
            with_two_lines: p.two_lines,
            focused: true,
            ..SnackbarFlags::NONE
        });
        let press = snackbar_spec(SnackbarFlags {
            with_single_line: !p.two_lines,
            with_two_lines: p.two_lines,
            pressed: true,
            ..SnackbarFlags::NONE
        });
        let fade = cx
            .at(p.id)
            .animate(p.id.with(param::PROGRESS), f32::from(p.visible), ENTER);
        let action_id = p.id.with("action");
        let (action_hover, action_focus, action_ring) = if has_action {
            let mut node = cx.at(action_id);
            let state = node.state();
            (
                node.animate(
                    action_id.with(param::HOVER),
                    f32::from(state.hovered),
                    REACTION,
                ),
                node.animate(
                    action_id.with(param::FOCUS),
                    f32::from(state.focus_visible),
                    REACTION,
                ),
                node.animate(
                    action_id.with(param::THICKNESS),
                    f32::from(state.focus_visible),
                    REACTION,
                ),
            )
        } else {
            (0.0, 0.0, 0.0)
        };
        let elevation = base.container.elevation;
        Self {
            id: p.id,
            message: p.message.to_string(),
            action: p.action.map(str::to_string),
            height: base.container.height,
            side: if p.floating {
                SIDE_FLOATING
            } else {
                SIDE_FIXED
            },
            width: p.width,
            alpha: fade,
            container: spec_ink(pack, base.container.color, 1.0),
            text_ink: spec_ink(pack, base.supporting_text.color, 1.0),
            action_ink: spec_ink(pack, base.action_label_text.color, 1.0),
            layer_ink: spec_ink(pack, hover.action_state_layer.color, 1.0),
            hover_a: hover.action_state_layer.opacity,
            focus_a: focus.action_state_layer.opacity,
            press_a: press.action_state_layer.opacity,
            shadow: (elevation > 0.0)
                .then(|| ShadowSpec::black(elevation * 2.0, 0.18, elevation * 0.5)),
            text_size: base.supporting_text.size,
            text_weight: ui_core::Weight(base.supporting_text.weight as u16),
            action_size: base.action_label_text.size,
            action_weight: ui_core::Weight(base.action_label_text.weight as u16),
            two_lines: p.two_lines,
            action_hover,
            action_focus,
            action_ring,
            overlay: p.overlay,
            reveal: p.reveal,
        }
    }

    /// The bar's own height, so a caller can reserve room for it.
    pub fn height(&self) -> f32 {
        self.height
    }
}

/// Pin `decl` to the page bottom, centred, and as far below it as `offset` says. An
/// out-of-flow box with neither side declared stays at its static origin, so the centring is
/// a pair of auto margins, not auto insets.
fn floating_at_bottom(decl: Decl, bottom: f32, offset: f32) -> Decl {
    decl.fixed()
        .inset(Insets {
            left: 0.0,
            right: 0.0,
            bottom,
            ..Insets::AUTO
        })
        .margin(Insets {
            left: AUTO,
            right: AUTO,
            top: 0.0,
            bottom: 0.0,
        })
        .offset(0.0, offset)
}

impl Widget for Md3Snackbar {
    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        // Only the action carries a click sense, so a click inside the bar is the action's —
        // the message area stays inert and cannot swallow a press meant for the page.
        (state.activation() && self.alpha > 0.0 && self.action.is_some())
            .then_some(ui_core::Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        // A bar that has not arrived hangs below the page's edge by its own height; the
        // overshoot of a value past 1 lifts it back above its rest place.
        let place = |decl: Decl| match self.overlay {
            Some(bottom) => {
                floating_at_bottom(decl, bottom, (1.0 - self.reveal) * (self.height + bottom))
            }
            None => decl,
        };
        // A fully faded bar paints nothing, so the app can keep it mounted across the exit.
        let empty =
            place(Decl::container(self.id, NodeKind::Row)
                .size(SizeClass::fixed(self.width, self.height)));
        if self.alpha <= 0.0 {
            return empty;
        }
        let root = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass::fixed(self.width, self.height))
            .padding(Insets {
                left: self.side,
                right: self.side,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(12.0)
            // The message and the action both centre on the bar's full height; padding the
            // content box vertically would squeeze the 48dp action out of a 20dp box.
            .cross_align(0.0)
            .cross_pos(0.5)
            .shapes(vec![Shape::Box {
                corners: Corners::all(CORNER),
                fill: faded(self.container, self.alpha),
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }]);
        let mut root = place(root);
        let mut message = Text::new(
            self.id.with("message"),
            self.message.clone(),
            self.text_size,
            faded(self.text_ink, self.alpha),
        )
        .weight(self.text_weight);
        if !self.two_lines {
            message = message.nowrap();
        }
        let mut message_decl = message.resolve();
        // The message takes the room the action leaves, and no more.
        message_decl.size_class.width = AxisSize::hug(0.0, 0.0);
        message_decl.size_class.grow_x = Grow::Yes;
        root = root.child(message_decl);
        if let Some(label) = &self.action {
            let action_id = self.id.with("action");
            let layer = self.action_hover * self.hover_a;
            let focus = self.action_focus * self.focus_a;
            let alpha = layer.max(focus);
            let mut shapes = Vec::with_capacity(2);
            if alpha > 0.0 {
                shapes.push(Shape::Box {
                    corners: Corners::all(ACTION_H * 0.5),
                    fill: Color {
                        a: self.layer_ink.a * alpha * self.alpha,
                        ..self.layer_ink
                    },
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                });
            }
            if self.action_ring > 0.0 {
                // The snackbar exports no focus ring; the action borrows its own ink.
                let ring = Color {
                    a: self.action_ink.a * self.action_ring * self.alpha,
                    ..self.action_ink
                };
                shapes.push(Shape::Box {
                    corners: Corners::all(ACTION_H * 0.5),
                    fill: Color::TRANSPARENT,
                    gradient: None,
                    border: Some((ring, 3.0)),
                    shadow: None,
                    image: None,
                });
            }
            let text = Text::new(
                action_id.with("label"),
                label.clone(),
                self.action_size,
                faded(self.action_ink, self.alpha),
            )
            .weight(self.action_weight)
            .nowrap()
            .resolve();
            let action = Decl::container(action_id, NodeKind::Row)
                .size(SizeClass {
                    width: AxisSize::hug(0.0, 0.0),
                    height: AxisSize::fixed(ACTION_H),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .padding(Insets {
                    left: 12.0,
                    right: 12.0,
                    top: 0.0,
                    bottom: 0.0,
                })
                .cross_align(0.0)
                .cross_pos(0.5)
                .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
                .cursor(CursorShape::PointingHand)
                .press_ink(Some(press_ink(
                    Color {
                        a: self.layer_ink.a * self.press_a * self.alpha,
                        ..self.layer_ink
                    },
                    InkRadius::Cover,
                )))
                // Stated, not inferred from the layer: that shape exists only while hovered,
                // so a press arriving before it would splash into a square pad.
                .corners(Corners::all(ACTION_H * 0.5))
                .shapes(shapes)
                .child(text);
            root = root.child(action);
        }
        root
    }
}
