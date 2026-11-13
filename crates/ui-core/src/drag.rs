//! Drag: a press that turns into a movement, and the four things it can become.

use crate::geom::Point;
use crate::id::Id;

/// Distance in logical units a press must travel before it becomes a drag.
///
/// Small enough to feel immediate, large enough that a shaky click stays a click.
pub const DRAG_THRESHOLD: f32 = 4.0;

/// Whether a move from `from` to `to` is far enough to count as a drag.
///
/// One predicate for both "starts a drag" and "is no longer a click", so no press can fall
/// between the two. The metric is the larger axis, not the diagonal: a straight line of
/// `DRAG_THRESHOLD` on either axis is already a drag.
pub fn past_threshold(from: Point, to: Point) -> bool {
    (to.x - from.x).abs().max((to.y - from.y).abs()) >= DRAG_THRESHOLD
}

/// Where a drag currently is.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum DragState {
    /// Nothing pressed.
    #[default]
    Idle,
    /// Pressed on `on` at `at`, but not yet past the threshold.
    Pressed { on: Id, at: Point },
    /// Moving; `on` keeps receiving moves even outside its own rect.
    Dragging { on: Id, from: Point, current: Point },
}

impl DragState {
    /// The node that owns this drag, if any.
    pub fn owner(&self) -> Option<Id> {
        match self {
            DragState::Idle => None,
            DragState::Pressed { on, .. } | DragState::Dragging { on, .. } => Some(*on),
        }
    }

    /// Whether the pointer is captured: moves keep going to the owner regardless of
    /// where the pointer is.
    pub fn captures_pointer(&self) -> bool {
        matches!(self, DragState::Dragging { .. })
    }

    /// Total displacement from the press point.
    pub fn delta(&self) -> Point {
        match self {
            DragState::Idle | DragState::Pressed { .. } => Point::new(0.0, 0.0),
            DragState::Dragging { from, current, .. } => {
                Point::new(current.x - from.x, current.y - from.y)
            }
        }
    }
}

/// What the framework tells the app about a drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragEvent {
    /// The press passed the threshold; the drag is live.
    ///
    /// `local` is the press point in the dragged node's own space. The tracker has no tree,
    /// so it fills it with the surface point and the platform rewrites it.
    Started { on: Id, from: Point, local: Point },
    /// The pointer moved; `delta` is from the press point, `step` is since the previous
    /// event, `local` is the current pointer.
    Moved {
        on: Id,
        delta: Point,
        step: Point,
        local: Point,
    },
    /// Released over `target` (or over nothing).
    Dropped { on: Id, target: Option<Id> },
    /// Cancelled by `Escape`; the app should restore its starting state.
    Cancelled { on: Id },
}

/// The drag state machine, fed one pointer event at a time.
#[derive(Clone, Debug, Default)]
pub struct DragTracker {
    state: DragState,
}

impl DragTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> DragState {
        self.state
    }

    pub fn is_dragging(&self) -> bool {
        matches!(self.state, DragState::Dragging { .. })
    }

    /// A button went down on `on`.
    pub fn press(&mut self, on: Id, at: Point) {
        self.state = DragState::Pressed { on, at };
    }

    /// The pointer moved to `to`; returns `Started` exactly once per drag.
    pub fn moved(&mut self, to: Point) -> Option<DragEvent> {
        match self.state {
            DragState::Idle => None,
            DragState::Pressed { on, at } => {
                if !past_threshold(at, to) {
                    return None;
                }
                self.state = DragState::Dragging {
                    on,
                    from: at,
                    current: to,
                };
                Some(DragEvent::Started {
                    on,
                    from: at,
                    local: at,
                })
            }
            DragState::Dragging { on, from, current } => {
                self.state = DragState::Dragging {
                    on,
                    from,
                    current: to,
                };
                Some(DragEvent::Moved {
                    on,
                    delta: Point::new(to.x - from.x, to.y - from.y),
                    step: Point::new(to.x - current.x, to.y - current.y),
                    local: to,
                })
            }
        }
    }

    /// The button came up; `target` is what is under the pointer.
    ///
    /// A press that never passed the threshold produces nothing: it was a click, and
    /// the click path already handled it.
    pub fn release(&mut self, target: Option<Id>) -> Option<DragEvent> {
        let state = std::mem::take(&mut self.state);
        match state {
            DragState::Idle | DragState::Pressed { .. } => None,
            DragState::Dragging { on, .. } => Some(DragEvent::Dropped { on, target }),
        }
    }

    /// `Escape` while dragging; restores `Idle` and reports the cancellation.
    pub fn cancel(&mut self) -> Option<DragEvent> {
        let state = std::mem::take(&mut self.state);
        match state {
            DragState::Idle | DragState::Pressed { .. } => None,
            DragState::Dragging { on, .. } => Some(DragEvent::Cancelled { on }),
        }
    }

    /// Forget the current drag without reporting anything, e.g. on pointer leave.
    pub fn reset(&mut self) {
        self.state = DragState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> Id {
        Id::from_u64(n)
    }

    #[test]
    fn a_shaky_click_never_becomes_a_drag() {
        // Below the threshold the press is a click; emitting `Started` would make every
        // click also drag whatever it hit.
        let mut t = DragTracker::new();
        t.press(id(1), Point::new(10.0, 10.0));
        assert_eq!(t.moved(Point::new(12.0, 11.0)), None);
        assert_eq!(t.release(Some(id(1))), None);
        assert!(!t.is_dragging());
    }

    #[test]
    fn escape_cancels_without_dropping() {
        // A cancelled drag must not also report a drop: the app would apply the move it
        // was told to abandon.
        let mut t = DragTracker::new();
        t.press(id(1), Point::new(10.0, 10.0));
        t.moved(Point::new(50.0, 50.0));
        assert_eq!(t.cancel(), Some(DragEvent::Cancelled { on: id(1) }));
        assert_eq!(t.release(Some(id(2))), None, "cancel already ended it");
        assert_eq!(t.state(), DragState::Idle);
    }
}
