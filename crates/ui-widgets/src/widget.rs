//! What a control gives the engine: this frame's declaration, and any semantic change.
//!
//! A widget is *code*: it holds the facts it was built with and answers two questions —
//! what to draw this frame, and whether the user changed anything. It holds no state and
//! keeps no value between frames; the engine owns all of that.
//!
//! Node properties (cursor, `Sense`, offset) travel with the `Decl`, so the engine routes
//! by them and never has to ask a widget that has moved on.

use ui_core::Decl;

use crate::cx::Cx;

/// A control that can describe itself to the engine.
pub trait Widget {
    /// Build this frame's declaration from the engine context.
    ///
    /// Everything the control may use — scope, interaction state, animations — comes
    /// through `cx`, so the result is a function of `(self, cx)` and stays reentrant.
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl;

    /// Report a semantic change for this frame, or `None` when nothing happened.
    ///
    /// Called once after layout, so `bounds` is this frame's final geometry and `state`
    /// carries the press point in this control's own space. A widget that only draws
    /// leaves this default in place.
    fn change(
        &self,
        _state: &crate::state::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        None
    }

    /// Whether this control already paints its own disabled appearance. When true the adapter
    /// must not fade the whole subtree again, or the control is dimmed twice.
    fn handles_disabled(&self) -> bool {
        false
    }

    /// Identity and a fingerprint of everything this control was built with, when `decl`
    /// reads nothing beyond `cx` and `self`. Anything left out of the fingerprint is a
    /// change the engine will never see.
    fn cache_key(&self) -> Option<ui_core::CachedKey> {
        None
    }
}

/// Resolve a control into this frame's declaration without placing it as an element.
///
/// For tests that inspect a declaration. A placed control must go through
/// `Cx::element`, the only route that carries `change()` to the engine — a `Decl` has
/// nowhere to put it, so a control placed as a bare declaration silently stops reporting.
pub fn decl_only(control: &dyn Widget, cx: &mut crate::cx::Cx<'_, '_>) -> Decl {
    cx.with_cx(|inner| control.decl(inner))
}

/// Blanket forwarding, so `Box<dyn Widget>` and `&dyn Widget` compose without ceremony.
impl<T: Widget + ?Sized> Widget for Box<T> {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        (**self).decl(cx)
    }

    fn change(
        &self,
        state: &crate::state::WidgetState,
        bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        (**self).change(state, bounds)
    }

    fn handles_disabled(&self) -> bool {
        (**self).handles_disabled()
    }

    fn cache_key(&self) -> Option<ui_core::CachedKey> {
        (**self).cache_key()
    }
}

impl<T: Widget + ?Sized> Widget for &T {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        (**self).decl(cx)
    }

    fn change(
        &self,
        state: &crate::state::WidgetState,
        bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        (**self).change(state, bounds)
    }

    fn handles_disabled(&self) -> bool {
        (**self).handles_disabled()
    }

    fn cache_key(&self) -> Option<ui_core::CachedKey> {
        (**self).cache_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::{Id, NodeKind};
    use ui_style::Env;

    /// A bare context for tests that do not care which node is hot.
    pub(crate) fn test_env() -> std::sync::Arc<Env> {
        Env::root()
    }

    struct Block {
        id: Id,
        draggable: bool,
    }

    impl Widget for Block {
        fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
            Decl::container(self.id, NodeKind::Box).sense(if self.draggable {
                ui_core::Sense::DRAG
            } else {
                ui_core::Sense::NONE
            })
        }
    }

    #[test]
    fn a_node_property_travels_with_the_declaration_not_with_the_widget() {
        // The point of keeping `Widget` to one method: the engine can answer "may this be
        // dragged?" from the tree alone, long after the widget is gone.
        // A boxed widget must forward without losing the declaration's node properties.
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 100, 100, 0.0, 0.0, 1.0, None);
        let cx = crate::cx::Cx::new(&mut core, test_env());
        let boxed: Box<dyn Widget> = Box::new(Block {
            id: Id::str("b"),
            draggable: true,
        });
        let forwarded = boxed.decl(&cx);
        assert!(
            forwarded.sense.contains(ui_core::Sense::DRAG),
            "the box forwards without losing it"
        );
        assert_eq!(forwarded.id, Id::str("b"));
        assert_eq!(forwarded.kind, NodeKind::Box);
    }
}
