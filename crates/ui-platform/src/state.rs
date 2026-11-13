//! One frame's resolved interaction state for one node id.

use ui_core::Id;

/// What one node knows about the user this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WidgetState {
    /// The pointer is over this node or a descendant that lets hover pass through.
    pub hovered: bool,
    pub pressed: bool,
    /// A press and its release landed on this node with the pointer staying within
    /// `DRAG_THRESHOLD`; anything further is a drag, not a click.
    pub clicked: bool,
    /// A drag ended on this node within the last frame.
    pub dropped: bool,
    /// The node this drag was released onto, when that node accepts drops.
    pub drop_target: Option<ui_core::Id>,
    pub focused: bool,
    /// Focus arrived by keyboard, so a focus ring may be drawn; pointer focus may not.
    pub focus_visible: bool,
    /// Enter or Space landed this frame on this node or a focused descendant; a pointer
    /// activation is `clicked`. Prefer `activation()`, which is either.
    pub activated: bool,
    /// A press outside this popup asked it to close — the light-dismiss of every flyout.
    pub dismissed: bool,
    /// A chord declared on this node fired this frame.
    pub shortcut: Option<ui_core::ShortcutId>,
    /// The nearest drag subscriber owns the live press; armed at the press, not at the
    /// threshold, so a control can hold its value for the whole gesture.
    pub dragging: bool,
    /// The press on this node has travelled past `DRAG_THRESHOLD`: a real drag, not a tap.
    pub drag_started: bool,
    pub disabled: bool,
    pub selected: bool,
    /// The pointer in this node's own space while a press or drag is live here.
    /// `change()` receives it per node; `decl()` sees the drag owner's space instead.
    pub local: Option<ui_core::geom::Point>,
    /// Pointer movement since the previous event while a drag is live.
    pub drag_step: Option<ui_core::geom::Point>,
    /// Held modifiers, for a control that shapes itself by them (Alt-drag, chord preview).
    pub modifiers: ui_core::Modifiers,
}

impl WidgetState {
    /// The one semantic activation a control answers: a click, or Enter/Space while focused.
    ///
    /// Controls map this to their own change (`Toggled`, `Selected`, …); a control that reads
    /// only `clicked` is deaf to the keyboard, and the engine never reports activation itself.
    pub fn activation(&self) -> bool {
        self.clicked || self.activated
    }

    /// Read one node's state from the frame's routing facts.
    pub fn of(core: &ui_core::cx::Cx<'_>, id: Id) -> Self {
        Self {
            // Containers see hover through children, so the ancestor path is the primary
            // read; the equality arm covers the hit node being this one.
            hovered: core.hovered_within(id) || core.hovered() == Some(id),
            pressed: core.pressed_within(id) || core.pressed() == Some(id),
            clicked: core.clicked_within(id),
            dropped: core.dropped_within(id),
            drop_target: core.drop_target(),
            focused: core.focused() == Some(id),
            focus_visible: core.focus_visible() && core.focused() == Some(id),
            activated: core.activated_within(id),
            dismissed: core.dismissed() == Some(id),
            shortcut: core
                .shortcut()
                .filter(|(owner, _)| *owner == id)
                .map(|(_, id)| id),
            dragging: core.dragging_within(id),
            drag_started: core.drag_started_within(id),
            // Folded by the engine: a disabled ancestor makes this node inert too.
            disabled: core.is_disabled(id),
            selected: false,
            local: core.local(),
            drag_step: core.drag_step(),
            modifiers: core.modifiers(),
        }
    }
}
