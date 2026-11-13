//! The glass toast: a non-blocking pill that floats above the content.

use ui_core::draw::{LiquidGlass, Shape};
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Clip, Grow, Len, LenInsets, Overflow, OverlayAlign, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{AnimSpec, Change, CursorShape, Decl, Easing, Id, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::{DRAG_FOLLOW, IMMEDIATE};
use ui_widgets::text::Text;
use ui_widgets::{Cx, Icon, IconSource, Widget, WidgetState};

use crate::material::{Control, glass_recipe};
use crate::paint::role_of;
use crate::shared::glass_backdrop;

/// `BoxConstraints(minHeight: 48)` on the pill.
pub const TOAST_MIN_HEIGHT: f32 = 48.0;
/// `maxWidth: 400`, the pill's ceiling.
pub const TOAST_MAX_WIDTH: f32 = 400.0;
/// The `Positioned(left: 16, right: 16)` and the safe-area 16 the overlay adds.
pub const TOAST_INSET: f32 = 16.0;
/// `BorderRadius.circular(24)`.
pub const TOAST_RADIUS: f32 = 24.0;
/// The message's `fontSize: 15`, `w500`.
pub const TOAST_LABEL: f32 = 15.0;
/// The icon's `size: 20` and the 12 dp gap on either side of the message.
pub const TOAST_ICON: f32 = 20.0;
pub const TOAST_GAP: f32 = 12.0;
/// `EdgeInsets.symmetric(horizontal: 16, vertical: 8/12)`.
pub const TOAST_PAD_H: f32 = 16.0;
pub const TOAST_PAD_V: f32 = 12.0;
pub const TOAST_PAD_V_ACTION: f32 = 8.0;
/// The action's own `padding: 12/6` and `minimumSize: Size(32, 32)`.
pub const TOAST_ACTION_PAD: Insets = Insets {
    left: 12.0,
    top: 6.0,
    right: 12.0,
    bottom: 6.0,
};
pub const TOAST_ACTION_MIN: f32 = 32.0;
pub const TOAST_ACTION_LABEL: f32 = 14.0;
/// The face is black or white at 70 %.
pub const TOAST_FACE_ALPHA: f32 = 0.70;
/// The rim and the halo are the semantic colour at 20 % and 15 %.
pub const TOAST_RIM_ALPHA: f32 = 0.20;
pub const TOAST_RIM: f32 = 1.0;
pub const TOAST_GLOW_ALPHA: f32 = 0.15;
pub const TOAST_GLOW_BLUR: f32 = 20.0;
pub const TOAST_GLOW_SPREAD: f32 = 2.0;
/// How far a swipe must travel before it dismisses; the reference leaves the threshold to
/// Flutter's `Dismissible` (40 % of the item plus a fling), and the engine reports no velocity.
pub const TOAST_DISMISS_TRAVEL: f32 = 24.0;
/// The arrival: 500 ms `easeOutCubic`, the fade taking the first half of it.
const SLIDE: AnimSpec = AnimSpec::new(0.5, Easing::Cubic(0.33, 1.0, 0.68, 1.0));
const FADE: AnimSpec = AnimSpec::new(0.25, Easing::Cubic(0.33, 1.0, 0.68, 1.0));
/// A released swipe that did not dismiss springs back rather than jumping.
const RETURN: AnimSpec = AnimSpec::new(0.25, Easing::EaseOut);

/// Where the toast sits; the slide arrives from the same edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassToastPosition {
    Top,
    Center,
    Bottom,
}

/// What the toast means, which is what its icon and accent carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassToastKind {
    Success,
    Error,
    Info,
    Warning,
    Neutral,
}

impl GlassToastKind {
    /// The accent this kind draws with; `Neutral` is the label ink at 60 %, as
    /// `CupertinoColors.secondaryLabel` is.
    fn accent(self, pack: &Style) -> Color {
        let ink = role_of(pack, "textFillColorPrimary");
        match self {
            Self::Success => role_of(pack, "systemGreen"),
            Self::Error => role_of(pack, "systemRed"),
            Self::Info => role_of(pack, "systemBlue"),
            Self::Warning => role_of(pack, "systemOrange"),
            Self::Neutral => Color { a: 0.6, ..ink },
        }
    }

    /// The reference's default glyph for this kind.
    fn glyph(self) -> &'static str {
        match self {
            Self::Success => "checkmark_circle_fill",
            Self::Error => "xmark_circle_fill",
            Self::Info => "info_circle_fill",
            Self::Warning => "exclamationmark_triangle_fill",
            Self::Neutral => "bubble_left_fill",
        }
    }
}

/// The action the toast offers, named by the id the app gave it.
#[derive(Clone, Debug)]
pub struct GlassToastAction {
    pub id: Id,
    pub label: String,
}

impl GlassToastAction {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
        }
    }
}

/// Everything a toast is built with.
pub struct GlassToastParams<'a> {
    pub id: Id,
    pub message: &'a str,
    /// The glyph before the message; `None` takes the kind's own.
    pub icon: Option<IconSource>,
    pub kind: GlassToastKind,
    pub position: GlassToastPosition,
    pub action: Option<GlassToastAction>,
    /// Whether a swipe in the leaving direction dismisses it.
    pub dismissible: bool,
}

impl<'a> GlassToastParams<'a> {
    pub fn new(id: Id, message: &'a str) -> Self {
        Self {
            id,
            message,
            icon: None,
            kind: GlassToastKind::Success,
            position: GlassToastPosition::Bottom,
            action: None,
            dismissible: true,
        }
    }

    pub fn icon(mut self, icon: IconSource) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn kind(mut self, kind: GlassToastKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn position(mut self, position: GlassToastPosition) -> Self {
        self.position = position;
        self
    }

    pub fn action(mut self, action: GlassToastAction) -> Self {
        self.action = Some(action);
        self
    }

    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
}

/// A toast: the whole tree is resolved once per frame, since it animates on its own clock.
pub struct GlassToast {
    body: Decl,
    /// The action pressed this frame, if any.
    fired: Option<Id>,
    /// A swipe travelled far enough to dismiss.
    swiped: bool,
}

impl GlassToast {
    pub fn new(params: GlassToastParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassToastParams {
            id,
            message,
            icon,
            kind,
            position,
            action,
            dismissible,
        } = params;

        let accent = kind.accent(pack);
        let ink = role_of(pack, "textFillColorPrimary");
        let dark = pack.appearance() == Some(ui_design::Appearance::Dark);
        let face = Color {
            a: TOAST_FACE_ALPHA,
            ..if dark { Color::BLACK } else { Color::WHITE }
        };
        let rim = Color {
            a: TOAST_RIM_ALPHA,
            ..accent
        };
        let glow = Color {
            a: TOAST_GLOW_ALPHA,
            ..accent
        };

        // ── A swipe follows the pointer and dismisses past the threshold; the direction is
        // ── the one that leaves the screen, so the toast never travels into the content.
        // The pill is the drag's origin, so its state is the one that carries the gesture.
        let pill_id = id.with("pill");
        let (fired, swiped, travel) =
            Self::gesture(pill_id, position, dismissible, action.as_ref(), cx);

        let reveal = cx.at(id).animate(id.with("reveal"), 1.0, SLIDE);
        let fade = cx.at(id).animate(id.with("fade"), 1.0, FADE);
        let slide = match position {
            GlassToastPosition::Top => -TOAST_MIN_HEIGHT,
            _ => TOAST_MIN_HEIGHT,
        };

        let mut pill = Decl::container(id.with("pill"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, TOAST_MAX_WIDTH),
                height: AxisSize::hug(TOAST_MIN_HEIGHT, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .gap(TOAST_GAP)
            .padding(Insets {
                left: TOAST_PAD_H,
                top: if action.is_some() {
                    TOAST_PAD_V_ACTION
                } else {
                    TOAST_PAD_V
                },
                right: TOAST_PAD_H,
                bottom: if action.is_some() {
                    TOAST_PAD_V_ACTION
                } else {
                    TOAST_PAD_V
                },
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .corners(Corners::all(TOAST_RADIUS))
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .shapes(vec![Shape::Box {
                corners: Corners::all(TOAST_RADIUS),
                fill: face,
                gradient: None,
                border: Some((rim, TOAST_RIM)),
                shadow: Some(ShadowSpec::glow(glow, TOAST_GLOW_BLUR, TOAST_GLOW_SPREAD)),
                image: None,
            }])
            .offset(0.0, travel + slide * (1.0 - reveal.clamp(0.0, 1.0)))
            .opacity(fade.clamp(0.0, 1.0));
        if dismissible {
            // The pill is the gesture's origin; a press on it must not become a click.
            pill = pill.sense(Sense::DRAG);
        }
        if let Some(backdrop) = glass_backdrop(&toast_glass(pack), 1.0)
            && let Some(p) = pill.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }

        let mut content = vec![
            Icon {
                id: id.with("icon"),
                source: icon.unwrap_or_else(|| IconSource::sf(kind.glyph())),
                size: TOAST_ICON,
                ink: accent,
            }
            .resolve(),
        ];
        content.push(
            Text::new(id.with("message"), message, TOAST_LABEL, ink)
                .weight(Weight::MEDIUM)
                .max_lines(2)
                .resolve(),
        );
        if let Some(action) = &action {
            content.push(action_decl(action, accent, cx));
        }
        for child in content {
            pill = pill.child(child);
        }

        // ── The layer: a fixed box hugging the pill, so the app underneath keeps its pointer.
        let insets = match position {
            GlassToastPosition::Top => LenInsets::new(
                Len::Px(TOAST_INSET),
                Len::Px(TOAST_INSET),
                Len::Px(TOAST_INSET),
                Len::Auto,
            ),
            GlassToastPosition::Bottom => LenInsets::new(
                Len::Px(TOAST_INSET),
                Len::Auto,
                Len::Px(TOAST_INSET),
                Len::Px(TOAST_INSET),
            ),
            GlassToastPosition::Center => {
                LenInsets::new(Len::Px(0.0), Len::Px(0.0), Len::Px(0.0), Len::Px(0.0))
            }
        };
        // The layer hugs the pill at the top and bottom edges, so centring on both axes only
        // matters for `Center`, where the layer fills the window.
        let body = Decl::container(id, NodeKind::Stack)
            .fixed()
            .inset(insets)
            .stack_align(OverlayAlign::Center)
            .sense(Sense::PASS_THROUGH)
            .child(pill);

        Self {
            body,
            fired,
            swiped,
        }
    }

    /// The swipe's travel this frame and the two facts `change()` cannot read itself.
    fn gesture(
        pill: Id,
        position: GlassToastPosition,
        dismissible: bool,
        action: Option<&GlassToastAction>,
        cx: &mut Cx<'_, '_>,
    ) -> (Option<Id>, bool, f32) {
        let fired = action.filter(|a| cx.state(a.id).activation()).map(|a| a.id);
        if !dismissible {
            return (fired, false, 0.0);
        }
        let mut node = cx.at(pill);
        let state = node.state();
        let live = pill.with("swipe-live");
        let base = pill.with("swipe-base");
        let travel_id = pill.with("swipe");
        let mut travel = node.value(travel_id);
        let mut swiped = false;
        let leaving = |raw: f32| match position {
            GlassToastPosition::Top => raw.min(0.0),
            _ => raw.max(0.0),
        };
        if state.dragging {
            if node.value(live) <= 0.5 {
                // The first drag frame banks where the finger started, so the pill moves from
                // where it is rather than jumping to the pointer.
                node.animate(live, 1.0, IMMEDIATE);
                node.animate(base, state.local.map_or(0.0, |p| p.y) - travel, IMMEDIATE);
            }
            let raw = state.local.map_or(0.0, |p| p.y) - node.value(base);
            travel = node.animate(travel_id, leaving(raw), DRAG_FOLLOW);
        } else if node.value(live) > 0.5 {
            node.animate(live, 0.0, IMMEDIATE);
            swiped = travel.abs() >= TOAST_DISMISS_TRAVEL;
            travel = node.animate(travel_id, 0.0, RETURN);
        }
        (fired, swiped, travel)
    }
}

/// The toast's own glass: the reference hands it `thickness: 25`, `refractiveIndex: 1.15`.
fn toast_glass(pack: &Style) -> LiquidGlass {
    LiquidGlass {
        refractive_index: 1.15,
        thickness: 25.0,
        ..glass_recipe(pack, Control::Container)
    }
}

/// The action: a capsule of the accent colour's text, no fill of its own.
fn action_decl(action: &GlassToastAction, accent: Color, cx: &mut Cx<'_, '_>) -> Decl {
    let state = cx.state(action.id);
    let pressed = f32::from(state.pressed);
    let wash = Color {
        a: 0.10 * pressed,
        ..accent
    };
    let mut node = Decl::container(action.id, NodeKind::Stack)
        .size(SizeClass {
            width: AxisSize::hug(TOAST_ACTION_MIN, 0.0),
            height: AxisSize::hug(TOAST_ACTION_MIN, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(TOAST_ACTION_PAD)
        .corners(Corners::all(TOAST_ACTION_MIN * 0.5))
        .stack_align(OverlayAlign::Center)
        .background(wash, Corners::all(TOAST_ACTION_MIN * 0.5))
        .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
        // The wash is the only mark it draws, so the stop has to be declared.
        .focus_stop(true)
        .cursor(CursorShape::PointingHand)
        .child(
            Text::new(
                action.id.with("label"),
                action.label.clone(),
                TOAST_ACTION_LABEL,
                accent,
            )
            .weight(Weight::SEMIBOLD)
            .nowrap()
            .resolve(),
        );
    node = node.opacity(if state.disabled { 0.38 } else { 1.0 });
    node
}

impl Widget for GlassToast {
    fn handles_disabled(&self) -> bool {
        // A toast has no disabled state; the adapter must not fade it either.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        self.swiped.then_some(Change::Cancelled)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.body.clone()
    }
}
