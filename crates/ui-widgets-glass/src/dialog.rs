//! The glass dialog: a modal card holding a title, a message, app content and its actions.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ModalTarget;
use ui_core::{Change, CursorShape, Decl, Id, NodeKind, Sense, TextAlign, Weight};
use ui_style::Style;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::button::{GlassButton, GlassButtonParams};
use crate::card::{GlassCard, GlassCardParams};
use crate::paint::role_of;

/// iOS's alert width; the reference's `maxWidth` default.
pub const DIALOG_WIDTH: f32 = 280.0;
/// `GlassContainer.card` with the reference's `EdgeInsets.all(20)`.
pub const DIALOG_PAD: f32 = 20.0;
pub const DIALOG_RADIUS: f32 = 12.0;
/// The reference's title (18 bold) and message (14 at 1.4 line height).
pub const DIALOG_TITLE_SIZE: f32 = 18.0;
pub const DIALOG_MESSAGE_SIZE: f32 = 14.0;
/// 1.4 line height over a ~1.2 em font leaves 0.2 em of extra leading.
pub const DIALOG_MESSAGE_LEADING: f32 = 0.2 * DIALOG_MESSAGE_SIZE;
/// iOS's `secondaryLabel` is the label ink at 60%; this pack carries it as an alpha.
pub const DIALOG_MESSAGE_ALPHA: f32 = 0.60;
/// The gap each band leaves under itself, and the fixed one above the actions.
pub const DIALOG_GAP: f32 = 8.0;
pub const DIALOG_ACTIONS_GAP: f32 = 12.0;
/// Between two app content blocks, matching MD3's content band.
pub const DIALOG_CONTENT_GAP: f32 = 12.0;
/// `GlassButton.custom(height: 44, shape: LiquidRoundedSuperellipse(borderRadius: 12))`.
pub const DIALOG_ACTION_H: f32 = 44.0;
pub const DIALOG_ACTION_RADIUS: f32 = 12.0;
pub const DIALOG_ACTION_GAP: f32 = 8.0;
pub const DIALOG_ACTION_FONT: f32 = 16.0;
/// The dim a Cupertino dialog lays behind its surface (`_kCupertinoModalBarrierColor`).
pub const DIALOG_SCRIM: f32 = 0.20;

/// Which slot an action fills; the reference gives each its own ink and weight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassActionKind {
    #[default]
    Default,
    /// Bold label, and a blue glow in the reference — the glow has no counterpart here.
    Primary,
    /// The system's destructive red.
    Destructive,
}

/// One action of the dialog, named by the id the app gave it.
#[derive(Clone, Debug)]
pub struct GlassDialogAction {
    pub id: Id,
    pub label: String,
    pub kind: GlassActionKind,
}

impl GlassDialogAction {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            kind: GlassActionKind::Default,
        }
    }

    pub fn primary(mut self) -> Self {
        self.kind = GlassActionKind::Primary;
        self
    }

    pub fn destructive(mut self) -> Self {
        self.kind = GlassActionKind::Destructive;
        self
    }
}

/// Everything a dialog is built with; the actions and the width never change while it runs.
pub struct GlassDialogParams<'a> {
    pub id: Id,
    pub title: &'a str,
    pub message: &'a str,
    pub actions: Vec<GlassDialogAction>,
    /// The app's own blocks, between the message and the actions.
    pub content: Vec<Decl>,
    /// The surface's own width; the reference treats its number as a maximum.
    pub width: f32,
    pub target: ModalTarget,
    /// Whether the surface dims and blocks what is behind it.
    pub barrier: bool,
    /// Whether a press on the scrim dismisses; `showCupertinoDialog` defaults to false.
    pub dismissible: bool,
    pub disabled: bool,
}

impl<'a> GlassDialogParams<'a> {
    pub fn new(id: Id, title: &'a str, message: &'a str, actions: Vec<GlassDialogAction>) -> Self {
        Self {
            id,
            title,
            message,
            actions,
            content: Vec::new(),
            width: DIALOG_WIDTH,
            target: ModalTarget::Global,
            barrier: true,
            dismissible: false,
            disabled: false,
        }
    }

    pub fn content(mut self, content: Vec<Decl>) -> Self {
        self.content = content;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn target(mut self, target: ModalTarget) -> Self {
        self.target = target;
        self
    }

    pub fn barrier(mut self, barrier: bool) -> Self {
        self.barrier = barrier;
        self
    }

    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A dialog: the whole tree is resolved once, since every part is fixed while it is up.
pub struct GlassDialog {
    body: Decl,
    fired: Option<Id>,
    dismiss: bool,
    disabled: bool,
}

impl GlassDialog {
    pub fn new(params: GlassDialogParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassDialogParams {
            id,
            title,
            message,
            actions,
            content,
            width,
            target,
            barrier,
            dismissible,
            disabled,
        } = params;
        let surface_id = id.with("surface");
        let inner = (width - 2.0 * DIALOG_PAD).max(0.0);
        let ink = role_of(pack, "textFillColorPrimary");
        // A click is a one-frame fact: read it here, since `change()` sees no context.
        let fired = actions
            .iter()
            .find(|a| cx.state(a.id).clicked)
            .map(|a| a.id);
        let dismiss =
            dismissible && barrier && cx.state(id).clicked && !cx.state(surface_id).clicked;

        // Without a barrier the surface *is* the widget, so it carries the widget's id.
        let root_id = if barrier { surface_id } else { id };
        let mut card = GlassCard::new(
            GlassCardParams::card(root_id)
                .padding(Insets::all(DIALOG_PAD))
                .radius(DIALOG_RADIUS)
                .width(width),
            pack,
        );
        if !title.is_empty() {
            card = card
                .child(text(
                    surface_id.with("title"),
                    title,
                    DIALOG_TITLE_SIZE,
                    Weight::BOLD,
                    ink,
                    inner,
                    0.0,
                ))
                .child(gap(surface_id.with("gap-title"), DIALOG_GAP));
        }
        if !message.is_empty() {
            let secondary = Color {
                a: DIALOG_MESSAGE_ALPHA,
                ..ink
            };
            card = card
                .child(text(
                    surface_id.with("message"),
                    message,
                    DIALOG_MESSAGE_SIZE,
                    Weight::REGULAR,
                    secondary,
                    inner,
                    DIALOG_MESSAGE_LEADING,
                ))
                .child(gap(surface_id.with("gap-message"), DIALOG_GAP));
        }
        if !content.is_empty() {
            let mut blocks = Decl::container(surface_id.with("content"), NodeKind::Column)
                .gap(DIALOG_CONTENT_GAP)
                .cross_align(1.0);
            for block in content {
                blocks = blocks.child(block);
            }
            card = card
                .child(blocks)
                .child(gap(surface_id.with("gap-content"), DIALOG_GAP));
        }
        if !actions.is_empty() {
            card = card
                .child(gap(surface_id.with("gap-actions"), DIALOG_ACTIONS_GAP))
                .child(actions_decl(
                    surface_id.with("actions"),
                    &actions,
                    inner,
                    disabled,
                    pack,
                    cx,
                ));
        }
        // Stretch the bands: a centred line can only centre inside a full-width box.
        let surface = card.resolve().cross_align(1.0);

        let body = if !barrier {
            surface
        } else {
            // The scrim subscribes: a press outside the surface must reach `change()`.
            let mut root = Decl::container(id, NodeKind::Column)
                .size(SizeClass::hug(0.0, 0.0))
                .modal(target)
                .cross_align(0.0)
                .cross_pos(0.5)
                .sense(Sense::CLICK)
                .cursor(CursorShape::Arrow)
                .disabled(disabled)
                .shapes(vec![Shape::Box {
                    corners: Corners::ZERO,
                    fill: Color {
                        a: DIALOG_SCRIM,
                        ..Color::BLACK
                    },
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                }]);
            // Two equal growers centre the surface: the engine has no main-axis position.
            root = root
                .child(spacer(id.with("pad-top")))
                .child(surface)
                .child(spacer(id.with("pad-bottom")));
            root
        };
        Self {
            body,
            fired,
            dismiss,
            disabled,
        }
    }
}

/// One centred text band, wrapping at the surface's inner width.
fn text(
    id: Id,
    body: &str,
    size: f32,
    weight: Weight,
    ink: Color,
    wrap: f32,
    leading: f32,
) -> Decl {
    Text::new(id, body, size, ink)
        .weight(weight)
        .max_width(wrap)
        .line_gap(leading)
        .align(TextAlign::Center)
        .resolve()
}

/// A fixed-height, full-width spacer between two bands.
fn gap(id: Id, height: f32) -> Decl {
    Decl::container(id, NodeKind::Box).size(SizeClass {
        width: AxisSize::fill(0.0, 0.0),
        height: AxisSize::fixed(height),
        grow_x: Grow::No,
        grow_y: Grow::No,
    })
}

/// A zero-basis grower: it takes a share of the spare space and draws nothing.
fn spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

/// The actions: one row when they fit side by side, otherwise a stretched column.
fn actions_decl(
    id: Id,
    actions: &[GlassDialogAction],
    inner: f32,
    disabled: bool,
    pack: &Style,
    cx: &mut Cx<'_, '_>,
) -> Decl {
    let side_by_side = actions.len() <= 2 && labels_fit(actions, inner, cx);
    // A lone action takes the whole strip, as one `Expanded` fills its `Row`.
    let width = if side_by_side && actions.len() > 1 {
        (inner - DIALOG_ACTION_GAP) / 2.0
    } else {
        inner
    };
    let mut strip = if side_by_side {
        Decl::container(id, NodeKind::Row).gap(DIALOG_ACTION_GAP)
    } else {
        Decl::container(id, NodeKind::Column).gap(DIALOG_ACTION_GAP)
    }
    .cross_align(1.0);
    for action in actions {
        let button = action_button(action, width, disabled, pack, cx);
        strip = strip.child(button.decl(cx));
    }
    strip
}

/// Whether every label keeps to one line in half the strip, clear of the button's corners.
fn labels_fit(actions: &[GlassDialogAction], inner: f32, cx: &Cx<'_, '_>) -> bool {
    if actions.len() < 2 {
        return true;
    }
    let room = (inner - DIALOG_ACTION_GAP) / 2.0 - 2.0 * DIALOG_ACTION_RADIUS;
    actions.iter().all(|action| {
        Text::new(
            Id::str("glass/dialog-measure"),
            action.label.clone(),
            DIALOG_ACTION_FONT,
            Color::BLACK,
        )
        .weight(label_weight(action.kind))
        .width_in(cx)
        .is_some_and(|width| width <= room)
    })
}

/// The reference's `_labelStyle`: primary is bold, everything else is w600.
fn label_weight(kind: GlassActionKind) -> Weight {
    match kind {
        GlassActionKind::Primary => Weight::BOLD,
        GlassActionKind::Default | GlassActionKind::Destructive => Weight::SEMIBOLD,
    }
}

/// One action: the dialog's own button shape, sized to its slot.
fn action_button(
    action: &GlassDialogAction,
    width: f32,
    disabled: bool,
    pack: &Style,
    cx: &mut Cx<'_, '_>,
) -> GlassButton {
    let ink = match action.kind {
        GlassActionKind::Destructive => role_of(pack, "systemRed"),
        GlassActionKind::Default | GlassActionKind::Primary => {
            role_of(pack, "textFillColorPrimary")
        }
    };
    let mut params = GlassButtonParams::labelled(action.id, None, &action.label);
    params.width = Some(width);
    params.height = DIALOG_ACTION_H;
    params.radius = Some(DIALOG_ACTION_RADIUS);
    params.label_size = Some(DIALOG_ACTION_FONT);
    params.label_weight = Some(label_weight(action.kind));
    params.label_ink = Some(ink);
    params.disabled = disabled;
    GlassButton::new(params, &mut cx.at(action.id), pack)
}

impl Widget for GlassDialog {
    fn handles_disabled(&self) -> bool {
        // The action buttons fade themselves; the adapter must not fade the subtree again.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        self.dismiss.then_some(Change::Cancelled)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.body.clone()
    }
}
