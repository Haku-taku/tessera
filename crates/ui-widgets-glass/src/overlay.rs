//! The overlay sheet the glass menu and popover share: a frosted panel hung from its anchor.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{Anchor, AxisSize, Grow, OverlayAlign, OverlayPlacement, Side, SizeClass};
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::{Decl, Id, Key, Modifiers, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::{Cx, WidgetState};

use crate::material::{Control, glass_recipe};
use crate::shared::glass_backdrop;

/// snappy(320 ms, bounce 0.15): the sheet's arrival, shared by every overlay.
use crate::shared::REVEAL_SPRING;

/// Everything an overlay sheet is built with.
pub(crate) struct PanelParams {
    pub id: Id,
    /// The node the panel unfolds from.
    pub anchor: Anchor,
    pub side: Side,
    /// `IsOpen`; a closed panel paints nothing and takes no keyboard.
    pub open: bool,
    pub width: f32,
    /// A fixed height for scrollable content; `None` hugs the children.
    pub height: Option<f32>,
    pub radius: f32,
    pub padding: Insets,
    pub gap: f32,
    pub control: Control,
    /// The id Escape is bound to; each overlay owns one so two can be up at once.
    pub dismiss: ShortcutId,
}

/// A resolved sheet: the facts `decl` needs, with the pack and this frame's springs read.
pub(crate) struct Panel {
    id: Id,
    anchor: Anchor,
    side: Side,
    open: bool,
    width: f32,
    height: Option<f32>,
    radius: f32,
    padding: Insets,
    gap: f32,
    dismiss: ShortcutId,
    /// Open progress 0..=1: the panel fades in as it arrives.
    reveal: f32,
    backdrop: Option<ui_core::BackdropBlur>,
}

impl Panel {
    pub(crate) fn new(params: PanelParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let PanelParams {
            id,
            anchor,
            side,
            open,
            width,
            height,
            radius,
            padding,
            gap,
            control,
            dismiss,
        } = params;
        let reveal = {
            let mut node = cx.at(id);
            node.spring(
                id.with("reveal"),
                f32::from(open),
                REVEAL_SPRING.0,
                REVEAL_SPRING.1,
            )
            .x
        };
        Self {
            id,
            anchor,
            side,
            open,
            width,
            height,
            radius,
            padding,
            gap,
            dismiss,
            reveal,
            backdrop: glass_backdrop(&glass_recipe(pack, control), 1.0),
        }
    }

    /// The sheet with its children: a column at its own insets, hung from the anchor.
    pub(crate) fn decl(&self, children: Vec<Decl>) -> Decl {
        let id = self.id;
        // A closed panel is not an overlay at all: leaving one would make it the keyboard
        // scope and light-dismiss target of a sheet nobody can see.
        if !self.open {
            return Decl::container(id, NodeKind::Box)
                .absolute()
                .size(SizeClass::fixed(0.0, 0.0));
        }
        let placement = OverlayPlacement {
            anchor: self.anchor,
            side: self.side,
            align: OverlayAlign::Start,
            offset: 0.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
        };
        let corners = Corners::all(self.radius);
        let mut panel = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: self.height.map_or(AxisSize::hug(0.0, 0.0), AxisSize::fixed),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(self.padding)
            .gap(self.gap)
            .corners(corners)
            .background(Color::TRANSPARENT, corners)
            .opacity(self.reveal.clamp(0.0, 1.0))
            .overlay(placement)
            // Asking for focus is what makes this the keyboard scope while it is open.
            .sense(Sense::FOCUSABLE | Sense::HOVER)
            .shortcut(KeyChord::new(Key::Escape, Modifiers::NONE), self.dismiss);
        if let Some(backdrop) = self.backdrop
            && let Some(p) = panel.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }
        for child in children {
            panel = panel.child(child);
        }
        panel
    }
}

/// Whether this frame asks the sheet to close: a press outside it, or Escape.
pub(crate) fn panel_dismissed(state: &WidgetState, dismiss: ShortcutId) -> bool {
    state.dismissed || state.shortcut == Some(dismiss)
}
