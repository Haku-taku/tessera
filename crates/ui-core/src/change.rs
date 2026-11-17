//! What a control reports upward, and the callback that receives it.

use std::any::Any;

use crate::id::Id;

/// A semantic report from a control: only what every style can agree on.
///
/// The closed variants are the common vocabulary and match in the compiler's eyes.
/// `Data` is the escape hatch for a control's own compound type; the framework never
/// looks inside it, so nothing generic may depend on what it holds.
#[derive(Debug)]
pub enum Change {
    /// A control settled on a new state; `None` is the indeterminate state a three-way
    /// checkbox reports. A two-way control only ever reports `Some`.
    Toggled { on: Option<bool> },

    /// A continuous control settled on a new value, in its own units. `f64` because the
    /// units are the control's, not the screen's: percent, seconds and money all fit.
    Value(f64),

    /// The whole text buffer after an edit; never a diff.
    Text(String),

    /// A member was chosen from a set; `None` means the selection was cleared.
    Selected(Option<Id>),

    /// A popup asked to close: a press outside it, or Escape. The app owns `open`.
    Dismissed,

    /// Activated without producing a value: a click, or Enter/Space on a focused node.
    /// The control decides which; the engine reports neither pointer nor key on its own.
    Activated,

    /// The secondary button on a node that declared [`crate::Sense::SECONDARY`]: `true` on
    /// the press, `false` on the release, wherever the pointer is by then.
    ///
    /// No point travels with it: a control that needs one reads the pointer, which the
    /// press already moved. `clicks` is the burst length the press belonged to, so a
    /// right-button double-click reads as 2 instead of two unrelated presses.
    Secondary { down: bool, clicks: u32 },

    /// The middle button on a node that declared [`crate::Sense::MIDDLE`]: `true` on the
    /// press, `false` on the release, wherever the pointer is by then.
    ///
    /// Same contract as [`Secondary`](Self::Secondary): the press already moved the
    /// pointer, so a control that needs a point reads it rather than receiving one.
    Middle { down: bool, clicks: u32 },

    /// A drag gesture began on the control, with no value yet.
    ///
    /// Distinct from `Activated`: a press that turns into a drag is not a click, and a
    /// control that must not confuse the two needs to see them apart.
    DragStarted,

    /// A control was dragged by `delta` since the press. The press point itself is not
    /// here: a control that maps position to a value reads it from its own `WidgetState`.
    Moved { dx: f64, dy: f64 },

    /// A drag ended on `target`, the node under the pointer that accepts drops.
    ///
    /// Reported to the node the drag started on, never to the target: the target hears
    /// [`DroppedOn`](Self::DroppedOn) with the payload. `target` is `None` when the drag
    /// ended over nothing that takes it, or over another application; `outcome` then still
    /// says how the other application took it, which a source that must finish a Wayland
    /// move needs. Both are `None` for a gesture released over nothing at all.
    Dropped {
        target: Option<Id>,
        outcome: Option<crate::dnd::DropOutcome>,
    },

    /// A drag is over this node and it accepts what the drag offers.
    ///
    /// `types` is the drag's MIME list, best first, so a target can preview what it is about
    /// to receive. Sent once on arrival and once more whenever the pointer moves from a
    /// non-target to this node; the node's `WidgetState::drag_over` stays true until
    /// [`DragExited`](Self::DragExited) or [`DroppedOn`](Self::DroppedOn).
    DragEntered { types: Vec<String> },

    /// The drag left this node without dropping; the highlight comes down.
    ///
    /// Never sent when the drag is released here: `DroppedOn` ends the hover itself.
    DragExited,

    /// A drag was released on this node and it takes what the drag carries.
    ///
    /// The payload is the drag's own bytes, decoded by the platform for a drag from another
    /// application and handed straight over for one inside this app. The report also says
    /// where and how the release landed, so a right drag can ask instead of acting.
    DroppedOn(crate::dnd::DropReport),

    /// The interaction that would have reported a change was abandoned.
    Cancelled,

    /// A registered shortcut fired on this node, named by its stable id.
    ///
    /// The node is the one that registered the binding, so a control can tell its own
    /// shortcut from a global one bound to the same chord.
    Shortcut(crate::shortcut::ShortcutId),

    /// A node that captures keys reported the chord the user pressed.
    ///
    /// Only a node subscribing to [`crate::Sense::CAPTURE_KEYS`] ever sees this.
    KeyChord(crate::shortcut::KeyChord),

    /// A row of this node's context menu was chosen, named by the row's stable id.
    MenuItem(Id),

    /// The menu this node opened closed without a pick.
    ///
    /// A pick reports only [`MenuItem`](Self::MenuItem). An app that opened a menu from
    /// state it built for the occasion needs this to drop that state when the user backs
    /// out with Escape or a press elsewhere.
    MenuClosed,

    /// A secondary press landed on this node, which declared no menu of its own.
    ///
    /// The point is where the press landed, so the app can open its own rows there with
    /// [`crate::cx::Cx::open_menu`]. The node is the natural owner of that menu and hears
    /// the pick and the dismissal.
    MenuRequested(crate::geom::Point),

    /// Clipboard content was pasted and no text editor consumed it.
    ///
    /// The payload is handed over whole, so an app without a caret can take a file list
    /// (or any other MIME type) the same way a drag target takes one.
    Paste(crate::clipboard::ClipboardContent),

    /// A control asked the keyboard focus to move to `target`.
    ///
    /// The engine applies it and does not forward it, like `ScrollBy`: focus belongs to the
    /// engine, and a menu walking its rows must not teach the app its own indices.
    Focus(Id),

    /// A control asked a scroller to move by this much.
    ///
    /// `owner` names the scroller; `None` lets the nearest owning ancestor take the request,
    /// which is what an arrow *inside* the viewport wants and a sibling arrow does not.
    ///
    /// The engine applies it with the same clamping and easing as `Cx::scroll_to`; an app
    /// that wants its own offset calls that instead.
    ScrollBy { dx: f64, dy: f64, owner: Option<Id> },

    /// A control's own type, carried opaquely; read it with [`Change::data`].
    Data(Box<dyn Any + Send>),
}

impl Change {
    /// Recover a control's own type. A mismatched type comes back unchanged, not as a panic.
    pub fn data<T: 'static>(self) -> Result<T, Change> {
        match self {
            Change::Data(b) => match b.downcast::<T>() {
                Ok(v) => Ok(*v),
                Err(b) => Err(Change::Data(b)),
            },
            other => Err(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_control_type_comes_back_or_the_change_is_returned_unchanged() {
        // Neither wrong-type nor wrong-variant may panic or silently vanish.
        let change = Change::Data(Box::new(7u32));
        assert_eq!(change.data::<u32>().ok(), Some(7u32));

        let wrong_type = Change::Data(Box::new(7u32));
        assert!(matches!(wrong_type.data::<String>(), Err(Change::Data(_))));

        let wrong_variant = Change::Activated;
        assert!(matches!(
            wrong_variant.data::<u32>(),
            Err(Change::Activated)
        ));
    }
}
