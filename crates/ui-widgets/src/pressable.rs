//! A caller-drawn node offered to the engine as one control.

use ui_core::geom::Rect;
use ui_core::{Change, Decl};

use crate::cx::Cx;
use crate::state::WidgetState;
use crate::widget::Widget;

/// A painted region that reports [`Change::Activated`] when it is clicked or receives
/// Enter/Space while focused.
///
/// For a control whose look the caller already built — a picker's row, a chip, a tile. A
/// `Decl` carries no `change()`, so the same declaration placed as a bare child paints and
/// is otherwise inert. Wrap it here and place the wrapper through [`Cx::element`]; the
/// declaration's root id is the id the change names.
pub struct Pressable {
    decl: Decl,
    paints_disabled: bool,
}

impl Pressable {
    pub fn new(decl: Decl) -> Self {
        Self {
            decl,
            paints_disabled: false,
        }
    }

    /// The declaration already carries its disabled colours, so the adapter must not fade
    /// the whole control a second time.
    pub fn painted_disabled(mut self) -> Self {
        self.paints_disabled = true;
        self
    }
}

impl Widget for Pressable {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.decl.clone()
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        state.activation().then_some(Change::Activated)
    }

    fn handles_disabled(&self) -> bool {
        self.paints_disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::{Id, NodeKind};

    fn pressable() -> Pressable {
        Pressable::new(
            Decl::container(Id::str("row"), NodeKind::Row).sense(ui_core::Sense::CLICK),
        )
    }

    #[test]
    fn a_pointer_press_and_a_key_press_are_the_same_activation() {
        // The control's semantic is activation. Reading only `clicked` would leave the
        // keyboard deaf; reading only `activated` would leave the pointer deaf.
        let bounds = Rect::new(0.0, 0.0, 100.0, 32.0);
        let click = WidgetState {
            clicked: true,
            ..WidgetState::default()
        };
        let enter = WidgetState {
            activated: true,
            ..WidgetState::default()
        };
        for state in [click, enter] {
            assert!(matches!(
                pressable().change(&state, bounds),
                Some(Change::Activated)
            ));
        }
        assert!(
            pressable()
                .change(&WidgetState::default(), bounds)
                .is_none(),
            "an idle row reports nothing"
        );
    }
}
