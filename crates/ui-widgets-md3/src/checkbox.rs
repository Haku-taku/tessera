//! MD3 checkbox, rebuilt from Flutter's `_CheckboxPainter`: one linear 200ms clock drives the
//! box squeeze, the fill, the outline and the check that draws itself along its own length.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{AnimSpec, CursorShape, Decl, Easing, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, NodeCx, Widget, WidgetState};

use crate::Md3Options;
use crate::glyph::{bars_along, mix_point};
use crate::layer::state_disc;
use crate::paint::lerp;
use crate::paint::{between, spec_ink};
use crate::spec::{CheckboxFlags, CheckboxSpec, checkbox_spec};

/// The unchecked state: its box size and outline width are the geometry's constants.
const OFF: CheckboxSpec = checkbox_spec(CheckboxFlags::NONE);

/// `Checkbox.width`, the box's side at both ends of the transition.
pub const EDGE: f32 = OFF.container.size;
/// `_kStrokeWidth`; also how far the box shrinks at the midpoint.
pub(crate) const STROKE: f32 = OFF.outline.width;
/// The export says `rounded_corners` without a radius; Flutter's M3 default is 2dp.
pub const EDGE_RADIUS: f32 = 2.0;
/// `MaterialTapTargetSize.padded`: the 18dp box sits in a 48dp target.
pub(crate) const TOUCH: f32 = 48.0;
/// Stroke width of the check, in the box's own space.
pub const CHECK_STROKE: f32 = 2.0;

/// The position controller's duration: linear, unlike the switch's curved pair.
const CHECK_MS: f32 = 0.2;
const CHECK: AnimSpec = AnimSpec::new(CHECK_MS, Easing::Linear);
use crate::motion::REACTION;

/// One glyph's leg. The outgoing glyph runs over the first half of the 200ms, the incoming
/// one over the second, so the two never overlap.
const GLYPH_OUT: AnimSpec = AnimSpec::new(CHECK_MS * 0.5, Easing::Linear);
const GLYPH_IN: AnimSpec = AnimSpec::new(CHECK_MS * 0.5, Easing::Linear).delayed(CHECK_MS * 0.5);

/// A checkbox's three values; MD3 draws a dash for the undetermined one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Off,
    On,
    /// Neither fully on nor off, e.g. a "select all" whose children disagree.
    Mixed,
}

impl CheckState {
    /// The two-state form, so a plain bool caller keeps its API.
    pub const fn from_bool(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }

    /// Whether the box is fully checked.
    pub const fn is_on(self) -> bool {
        matches!(self, Self::On)
    }

    /// Whether the box paints its active (filled) appearance; mixed counts as active.
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// The dash that stands for the undetermined state, grown from the midpoint outwards. One
/// rect with square ends, exactly Flutter's `_drawDash` line: `0.2·EDGE` to `0.8·EDGE`.
fn dash_bars(progress: f32) -> Vec<(f32, f32, f32, f32)> {
    let p = progress.clamp(0.0, 1.0);
    let mid = EDGE * 0.5;
    // Both ends travel outward together: the left one runs from `mid` to `0.2·EDGE`.
    let a = lerp(mid, EDGE * 0.2, p);
    let b = lerp(mid, EDGE * 0.8, p);
    vec![(a, mid - CHECK_STROKE * 0.5, (b - a).max(0.0), CHECK_STROKE)]
}

/// The check's three points, in the box's own 18dp space.
fn check_points() -> ((f32, f32), (f32, f32), (f32, f32)) {
    (
        (EDGE * 0.15, EDGE * 0.45),
        (EDGE * 0.40, EDGE * 0.70),
        (EDGE * 0.85, EDGE * 0.25),
    )
}

/// The bars that draw the check at `progress`; the short leg comes first, then the long one
/// grows out of the joint.
fn check_bars(progress: f32) -> Vec<(f32, f32, f32, f32)> {
    let (start, mid, end) = check_points();
    let p = progress.clamp(0.0, 1.0);
    if p < 0.5 {
        bars_along(start, mix_point(start, mid, p * 2.0), CHECK_STROKE)
    } else {
        let mut bars = bars_along(start, mid, CHECK_STROKE);
        bars.extend(bars_along(
            mid,
            mix_point(mid, end, (p - 0.5) * 2.0),
            CHECK_STROKE,
        ));
        bars
    }
}

/// One MD3 checkbox; every animation is advanced in `new_with`, so `decl` stays pure.
pub struct Md3Checkbox {
    id: Id,
    /// 0 = unchecked appearance, 1 = checked; the box and the check are pure functions of it.
    position: f32,
    fill: Color,
    outline: Color,
    outline_width: f32,
    /// The check's colour and its own progress, drawn only past the halfway point.
    check: Option<(Color, f32)>,
    /// The dash's colour and progress; the undetermined state's glyph.
    dash: Option<(Color, f32)>,
    /// The state-layer disc: colour and radius.
    layer: Option<(Color, f32)>,
    focus_fade: f32,
    focus_ring: Color,
    focus_gap: f32,
    focus_width: f32,
    /// The value this frame paints; a mixed box reports `on: true` when clicked.
    value: CheckState,
    disabled: bool,
}

impl Md3Checkbox {
    /// The unprefixed form: constants only, as a direct caller expects.
    pub fn new(id: Id, checked: bool, error: bool, ctx: &mut NodeCx<'_>, pack: &Style) -> Self {
        Self::new_with(Md3Options::default(), id, checked, error, ctx, pack)
    }

    /// Same control with explicit parameters, so the semantic path and a direct caller
    pub fn new_with(
        params: Md3Options,
        id: Id,
        checked: bool,
        error: bool,
        ctx: &mut NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_tri(params, id, CheckState::from_bool(checked), error, ctx, pack)
    }

    /// The three-valued form: `Mixed` paints the active box with a dash instead of a check.
    pub fn new_tri(
        params: Md3Options,
        id: Id,
        value: CheckState,
        error: bool,
        ctx: &mut NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let state = ctx.state();
        let disabled = state.disabled || params.disabled;
        let position = ctx.animate(
            id.with(param::PROGRESS),
            if value.is_active() { 1.0 } else { 0.0 },
            CHECK,
        );

        // The reference keys both the box and the state layer on the live interaction state.
        let lit = state.hovered || state.focus_visible;
        let on = checkbox_spec(CheckboxFlags {
            selected: true,
            error,
            hovered: lit,
            focused: state.focus_visible,
            pressed: state.pressed,
            disabled,
        });
        let off = checkbox_spec(CheckboxFlags {
            error,
            hovered: lit,
            focused: state.focus_visible,
            pressed: state.pressed,
            disabled,
            ..CheckboxFlags::NONE
        });

        // `_colorAt`: the fill arrives over the first quarter and then holds.
        let fill = if position >= 0.25 {
            spec_ink(pack, on.container.color, on.container.opacity)
        } else {
            between(
                spec_ink(pack, off.container.color, off.container.opacity),
                spec_ink(pack, on.container.color, on.container.opacity),
                position * 4.0,
            )
        };

        // `BorderSide.lerp(inactiveSide, activeSide, t)` while `t <= 0.5`; past that the box
        // uses `activeSide`, which the export leaves at width 0 and no colour.
        let (outline, outline_width) = if position <= 0.5 {
            (
                between(
                    // The export gives the outline no opacity of its own; Flutter's disabled
                    // ring is `onSurface` at 38%, which is the container's opacity here.
                    spec_ink(pack, off.outline.color, off.container.opacity),
                    spec_ink(pack, on.outline.color, on.container.opacity),
                    position,
                ),
                off.outline.width + (on.outline.width - off.outline.width) * position,
            )
        } else {
            (
                spec_ink(pack, on.outline.color, on.container.opacity),
                on.outline.width,
            )
        };

        // The glyphs are two instances with opposite legs: whichever is leaving shrinks over
        // the first half, whichever is arriving grows over the second, so they never overlap.
        let ink = spec_ink(pack, on.icon.color, 1.0);
        let glyph = |ctx: &mut NodeCx<'_>, salt: &'static str, on_now: bool| {
            let key = id.with(salt);
            let target = f32::from(on_now);
            let spec = if target > ctx.value(key) {
                GLYPH_IN
            } else {
                GLYPH_OUT
            };
            let progress = ctx.animate(key, target, spec);
            (progress > 0.0).then_some((ink, progress))
        };
        let check = glyph(ctx, "check", value.is_on());
        let dash = glyph(ctx, "dash", matches!(value, CheckState::Mixed));

        // The press splash grows from the centre at the state layer's own opacity; hover and
        // focus reach the full disc and only fade.
        let layer = state_disc(
            ctx,
            id,
            OFF.state_layer.size,
            disabled,
            state.pressed,
            |hovered, focused, pressed| {
                let s = checkbox_spec(CheckboxFlags {
                    selected: value.is_active(),
                    error,
                    hovered,
                    focused,
                    pressed,
                    disabled,
                });
                spec_ink(pack, s.state_layer.color, s.state_layer.opacity)
            },
        );

        // The ring keys on keyboard focus; the disc above fades on the same instance.
        let focus_fade = ctx.animate(
            id.with(param::FOCUS),
            f32::from(state.focus_visible),
            REACTION,
        );

        Self {
            id,
            position,
            fill,
            outline,
            outline_width,
            check,
            dash,
            layer,
            focus_fade,
            focus_ring: spec_ink(pack, OFF.focus_indicator.color, 1.0),
            focus_gap: OFF.focus_indicator_outline.offset,
            focus_width: OFF.focus_indicator.thickness,
            value,
            disabled,
        }
    }
}

impl Widget for Md3Checkbox {
    fn handles_disabled(&self) -> bool {
        // The token table paints disabled itself; the adapter must not fade again.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<ui_core::Change> {
        if self.disabled || !state.activation() {
            return None;
        }
        // A mixed box reports the value material-web gives it on click: checked.
        Some(ui_core::Change::Toggled {
            on: Some(!self.value.is_on()),
        })
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let origin = (TOUCH - EDGE) * 0.5;
        let mut shapes = Vec::with_capacity(8);
        // Keyboard focus is an overlay ring with a transparent gap; it takes no input.
        if self.focus_fade > 0.0 {
            // Compose draws it on the 48dp interactive bounds, inset, with a 25% corner.
            let side = TOUCH - self.focus_gap * 2.0;
            shapes.push(Shape::Rect {
                rect: Rect::new(self.focus_gap, self.focus_gap, side, side),
                corners: Corners::all(TOUCH * 0.25 - self.focus_gap),
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((
                    Color {
                        a: self.focus_fade,
                        ..self.focus_ring
                    },
                    self.focus_width,
                )),
            });
        }
        if let Some((colour, radius)) = self.layer {
            shapes.push(Shape::Rect {
                rect: Rect::new(
                    TOUCH * 0.5 - radius,
                    TOUCH * 0.5 - radius,
                    radius * 2.0,
                    radius * 2.0,
                ),
                corners: Corners::all(radius),
                fill: colour,
                gradient: None,
                border: None,
            });
        }
        // `_outerRectAt`: the box is smallest at the midpoint, where the fill is already full.
        let inset = 1.0 - (self.position - 0.5).abs() * 2.0;
        let side = EDGE - inset * STROKE;
        shapes.push(Shape::Rect {
            rect: Rect::new(origin + inset, origin + inset, side, side),
            corners: Corners::all(EDGE_RADIUS),
            fill: self.fill,
            gradient: None,
            border: (self.outline_width > 0.0).then_some((self.outline, self.outline_width)),
        });
        if let Some((colour, progress)) = self.check {
            // The check keeps the un-shrunk origin, as the reference does.
            for (bx, by, bw, bh) in check_bars(progress) {
                shapes.push(Shape::Rect {
                    rect: Rect::new(origin + bx, origin + by, bw, bh),
                    // Square ends: both references stroke the glyph with a butt cap.
                    corners: Corners::ZERO,
                    fill: colour,
                    gradient: None,
                    border: None,
                });
            }
        }
        if let Some((colour, progress)) = self.dash {
            for (bx, by, bw, bh) in dash_bars(progress) {
                shapes.push(Shape::Rect {
                    rect: Rect::new(origin + bx, origin + by, bw, bh),
                    corners: Corners::ZERO,
                    fill: colour,
                    gradient: None,
                    border: None,
                });
            }
        }
        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(TOUCH),
                height: AxisSize::fixed(TOUCH),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // The checkbox takes hover, click and focus; its shapes take nothing.
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .shapes(shapes)
    }
}
