//! The glass action sheet: a bottom-anchored card of actions with a cancel card below.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, OverlayAlign, SizeClass};
use ui_core::tree::ModalTarget;
use ui_core::{AnimSpec, Change, CursorShape, Decl, Id, NodeKind, Sense, TextAlign, Weight};
use ui_style::Style;
use ui_widgets::build::IMMEDIATE;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::card::{GlassCard, GlassCardParams};
use crate::divider::GlassDivider;
use crate::paint::role_of;

/// The reference's `EdgeInsets.all(8)` around the whole sheet.
pub const SHEET_INSET: f32 = 8.0;
/// `BorderRadius.circular(14)` on both cards.
pub const SHEET_RADIUS: f32 = 14.0;
/// `SizedBox(height: 8)` between the actions card and the cancel card.
pub const SHEET_GAP: f32 = 8.0;
/// The row's `EdgeInsets.symmetric(horizontal: 16, vertical: 16)`.
pub const SHEET_ROW_PAD: f32 = 16.0;
pub const SHEET_LABEL: f32 = 17.0;
pub const SHEET_ICON: f32 = 20.0;
pub const SHEET_ICON_GAP: f32 = 12.0;
/// The header's `fromLTRB(16, 16, 16, 12)`.
pub const SHEET_HEADER_PAD: Insets = Insets {
    left: 16.0,
    top: 16.0,
    right: 16.0,
    bottom: 12.0,
};
pub const SHEET_TITLE: f32 = 13.0;
pub const SHEET_MESSAGE: f32 = 13.0;
pub const SHEET_HEADER_GAP: f32 = 4.0;
/// The pressed or focused wash: `label` at 6 % (`glass_action_sheet.dart:393-397`).
pub const SHEET_HIGHLIGHT: f32 = 0.06;
/// The card colour over the glass: black/white at 65 % (`:216-218`).
pub const SHEET_CARD_ALPHA: f32 = 0.65;
/// `GlassDefaults.barrierColor` — 54 % black.
pub const SHEET_SCRIM: f32 = 0.54;

/// snappy(320 ms, bounce 0.15), the same arrival every glass overlay uses.
use crate::shared::REVEAL_SPRING;
/// How far the sheet rises as it arrives.
const SHEET_SLIDE: f32 = 24.0;
/// The wash leaves over 150 ms easeOutCubic; it arrives with no tween (`:388-392`).
const HIGHLIGHT_OUT: AnimSpec = AnimSpec::new(0.150, ui_style::motion::CSS_EASE_OUT);

/// One action of the sheet, named by the id the app gave it.
#[derive(Clone, Debug)]
pub struct GlassSheetAction {
    pub id: Id,
    pub label: String,
    pub icon: Option<IconSource>,
    /// Destructive actions take the system red (`GlassActionSheetStyle.destructive`).
    pub destructive: bool,
}

impl GlassSheetAction {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            icon: None,
            destructive: false,
        }
    }

    pub fn icon(mut self, icon: IconSource) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// Everything an action sheet is built with; the app owns what each action means.
pub struct GlassActionSheetParams<'a> {
    pub id: Id,
    pub title: &'a str,
    pub message: &'a str,
    pub actions: Vec<GlassSheetAction>,
    /// The separated cancel card; `None` shows no cancel card.
    pub cancel: Option<GlassSheetAction>,
    /// The sheet's width; `None` fills the window less the reference's 8 dp inset.
    pub width: Option<f32>,
    pub target: ModalTarget,
    /// Whether the sheet dims and blocks what is behind it.
    pub barrier: bool,
    /// `barrierDismissible`: whether a press outside the sheet closes it.
    pub dismissible: bool,
    pub disabled: bool,
}

impl<'a> GlassActionSheetParams<'a> {
    pub fn new(id: Id, actions: Vec<GlassSheetAction>) -> Self {
        Self {
            id,
            title: "",
            message: "",
            actions,
            cancel: None,
            width: None,
            target: ModalTarget::Global,
            barrier: true,
            dismissible: true,
            disabled: false,
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    pub fn message(mut self, message: &'a str) -> Self {
        self.message = message;
        self
    }

    pub fn cancel(mut self, cancel: GlassSheetAction) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
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

/// An action sheet: the whole tree is resolved once, since every part is fixed while it is up.
pub struct GlassActionSheet {
    body: Decl,
    fired: Option<Id>,
    /// The cancel card's row: pressing it is a dismissal, not a chosen action.
    cancel: Option<Id>,
    dismiss: bool,
    disabled: bool,
}

impl GlassActionSheet {
    pub fn new(params: GlassActionSheetParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassActionSheetParams {
            id,
            title,
            message,
            actions,
            cancel,
            width,
            target,
            barrier,
            dismissible,
            disabled,
        } = params;
        let sheet_id = id.with("sheet");
        let actions_card_id = id.with("actions");
        // The card is chrome the widget invents; the cancel *action* id belongs to the app,
        // so the two must not share a salt.
        let cancel_card_id = id.with("cancel-card");

        // A click is a one-frame fact: read it here, since `change()` sees no context.
        let fired = actions
            .iter()
            .chain(cancel.iter())
            .find(|a| cx.state(a.id).clicked)
            .map(|a| a.id);
        let dismiss = dismissible
            && barrier
            && cx.state(id).clicked
            && !cx.state(actions_card_id).clicked
            && !cx.state(cancel_card_id).clicked;

        let dark = pack.appearance() == Some(ui_design::Appearance::Dark);
        let label = role_of(pack, "textFillColorPrimary");
        let danger = role_of(pack, "systemRed");
        let card_fill = Color {
            a: SHEET_CARD_ALPHA,
            ..if dark { Color::BLACK } else { Color::WHITE }
        };
        let wash = Color {
            a: SHEET_HIGHLIGHT,
            ..if dark { Color::WHITE } else { Color::BLACK }
        };
        let reveal = {
            let mut node = cx.at(id);
            node.spring(id.with("reveal"), 1.0, REVEAL_SPRING.0, REVEAL_SPRING.1)
                .x
        };

        // ── The actions card: the header band, then a row per action, hairline-separated.
        let mut card_params = GlassCardParams::card(actions_card_id)
            .padding(Insets::ZERO)
            .radius(SHEET_RADIUS)
            .fill(card_fill);
        if let Some(w) = width {
            card_params = card_params.width(w);
        }
        let mut card = GlassCard::new(card_params, pack);
        if !title.is_empty() || !message.is_empty() {
            card = card.child(header(id.with("header"), title, message, label, width));
        }
        for (k, action) in actions.iter().enumerate() {
            if k > 0 || !title.is_empty() || !message.is_empty() {
                card = card.child(
                    GlassDivider::horizontal(actions_card_id.with(("divider", k)), pack).decl(cx),
                );
            }
            card = card.child(row_decl(action, false, label, danger, wash, disabled, cx));
        }
        let mut sheet = Decl::container(sheet_id, NodeKind::Column)
            .gap(SHEET_GAP)
            .padding(Insets::all(SHEET_INSET))
            .cross_align(if width.is_some() { 0.0 } else { 1.0 })
            .offset(0.0, (1.0 - reveal.clamp(0.0, 1.0)) * SHEET_SLIDE)
            .opacity(reveal.clamp(0.0, 1.0))
            .child(card.resolve());
        if let Some(cancel) = &cancel {
            let mut cancel_params = GlassCardParams::card(cancel_card_id)
                .padding(Insets::ZERO)
                .radius(SHEET_RADIUS)
                .fill(card_fill);
            if let Some(w) = width {
                cancel_params = cancel_params.width(w);
            }
            let cancel_card = GlassCard::new(cancel_params, pack);
            sheet = sheet.child(
                cancel_card
                    .child(row_decl(cancel, true, label, danger, wash, disabled, cx))
                    .resolve(),
            );
        }

        let body = if !barrier {
            sheet
        } else {
            // The scrim subscribes: a press outside the sheet must reach `change()`.
            let mut root = Decl::container(id, NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fill(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .modal(target)
                .cross_align(if width.is_some() { 0.0 } else { 1.0 })
                .cross_pos(0.5)
                .sense(Sense::CLICK)
                .cursor(CursorShape::Arrow)
                .disabled(disabled)
                .shapes(vec![Shape::Box {
                    corners: Corners::ZERO,
                    fill: Color {
                        a: SHEET_SCRIM,
                        ..Color::BLACK
                    },
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                }]);
            // The grower above pushes the sheet to the bottom edge; nothing is offset by hand.
            root = root.child(spacer(id.with("pad-top"))).child(sheet);
            root
        };
        Self {
            body,
            fired,
            cancel: cancel.as_ref().map(|a| a.id),
            dismiss,
            disabled,
        }
    }
}

/// The header band: a centred title, then a centred message 4 dp below it.
fn header(id: Id, title: &str, message: &str, ink: Color, width: Option<f32>) -> Decl {
    let mut band = Decl::container(id, NodeKind::Column)
        .padding(SHEET_HEADER_PAD)
        .gap(SHEET_HEADER_GAP)
        .cross_align(1.0);
    let secondary = Color { a: 0.60, ..ink };
    if !title.is_empty() {
        band = band.child(centred(
            id.with("title"),
            title,
            SHEET_TITLE,
            Weight::SEMIBOLD,
            ink,
            width,
        ));
    }
    if !message.is_empty() {
        band = band.child(centred(
            id.with("message"),
            message,
            SHEET_MESSAGE,
            Weight::REGULAR,
            secondary,
            width,
        ));
    }
    band
}

/// One centred line; the band stretches it, so `TextAlign::Center` has a full box to sit in.
fn centred(id: Id, body: &str, size: f32, weight: Weight, ink: Color, width: Option<f32>) -> Decl {
    let mut text = Text::new(id, body, size, ink)
        .weight(weight)
        .align(TextAlign::Center);
    if let Some(w) = width {
        text = text.max_width((w - SHEET_HEADER_PAD.left - SHEET_HEADER_PAD.right).max(0.0));
    }
    text.resolve()
}

/// One action row: the wash, an optional 20 dp glyph, then the 17 pt label, all centred.
fn row_decl(
    action: &GlassSheetAction,
    is_cancel: bool,
    label_ink: Color,
    danger: Color,
    wash: Color,
    disabled: bool,
    cx: &mut Cx<'_, '_>,
) -> Decl {
    let state = cx.state(action.id);
    let target = f32::from(!disabled && (state.pressed || state.focus_visible));
    let spec = if target > 0.5 {
        IMMEDIATE
    } else {
        HIGHLIGHT_OUT
    };
    let highlight = cx
        .at(action.id)
        .animate(action.id.with("wash"), target, spec);
    let ink = if action.destructive {
        danger
    } else {
        label_ink
    };
    let weight = if is_cancel {
        Weight::SEMIBOLD
    } else {
        Weight::REGULAR
    };

    // The stack centres the content on both axes, as the reference's `Row` does.
    let mut content = Decl::container(action.id.with("content"), NodeKind::Row)
        .gap(SHEET_ICON_GAP)
        .cross_align(0.0)
        .cross_pos(0.5);
    if let Some(icon) = action.icon {
        content = content.child(
            ui_widgets::Icon {
                id: action.id.with("icon"),
                source: icon,
                size: SHEET_ICON,
                ink,
            }
            .resolve(),
        );
    }
    content = content.child(
        Text::new(
            action.id.with("label"),
            action.label.clone(),
            SHEET_LABEL,
            ink,
        )
        .weight(weight)
        .resolve(),
    );

    let mut row = Decl::container(action.id, NodeKind::Stack)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .padding(Insets::all(SHEET_ROW_PAD))
        .stack_align(OverlayAlign::Center)
        .background(
            Color {
                a: wash.a * highlight,
                ..wash
            },
            Corners::ZERO,
        )
        .child(content);
    if !disabled {
        row = row
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            // The wash is the focus mark; the stop still has to be declared.
            .focus_stop(true)
            .cursor(CursorShape::PointingHand);
    }
    row
}

/// A zero-basis grower: it takes the spare height and draws nothing.
fn spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

impl Widget for GlassActionSheet {
    fn handles_disabled(&self) -> bool {
        // The rows take no pointer on their own; the adapter must not fade the sheet.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        if let Some(id) = self.fired {
            // Cancel is a dismissal: the reference pops with no value for it.
            return if Some(id) == self.cancel {
                Some(Change::Cancelled)
            } else {
                Some(Change::Selected(Some(id)))
            };
        }
        self.dismiss.then_some(Change::Cancelled)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.body.clone()
    }
}
