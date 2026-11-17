//! Bridges a style-aware control to the engine's element contract.

use std::sync::Arc;

use ui_core::Decl;
use ui_core::change::Change;
use ui_core::geom::Rect;
use ui_style::Env;

use crate::cx::Cx;
use crate::widget::Widget;

/// A control plus the scope it was built under, as the engine sees it.
pub struct Styled {
    control: Box<dyn Widget>,
    env: Arc<Env>,
}

impl Styled {
    pub fn new(control: Box<dyn Widget>, env: Arc<Env>) -> Self {
        Self { control, env }
    }
}

impl ui_platform::Element for Styled {
    fn decl(&self, core: &mut ui_core::cx::Cx<'_>) -> Decl {
        // The committed routing still describes last frame's tree: a control the app just
        // re-enabled would read itself inert and stay faded one frame. The declaration this
        // build produced, plus the inertness its containers declared, is the current fact.
        let inherited = core.inherited_disabled();
        let cx = Cx::new(core, self.env.clone());
        let mut decl = self.control.decl(&cx);
        let disabled = inherited || decl.disabled;
        core.note_disabled(decl.id, disabled);
        // A control that paints its own disabled colours must not be faded again.
        if disabled && !self.control.handles_disabled() {
            let base = decl_opacity(&decl);
            decl = decl.opacity(base * DISABLED_OPACITY);
        }
        decl
    }

    fn cache_key(&self) -> Option<ui_core::CachedKey> {
        // The scope travels in `env`; the app announces a theme change by invalidating
        // declarations, which is the same contract the engine already documents.
        self.control.cache_key()
    }

    fn change(&self, state: &ui_platform::WidgetState, bounds: Rect) -> Option<Change> {
        // One choke point: an inert control never reports, whatever its own code says.
        if state.disabled {
            return None;
        }
        self.control.change(state, bounds)
    }
}

/// Reference opacity for disabled content: MD3 states `onSurface` at 38%.
const DISABLED_OPACITY: f32 = 0.38;

/// The root's own group opacity, so the fade composes with what the control already set.
fn decl_opacity(decl: &Decl) -> f32 {
    decl.data.params().map(|p| p.opacity).unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::{Id, NodeKind};
    use ui_platform::Element;

    struct Block {
        id: Id,
        disabled: bool,
        self_drawn: bool,
    }

    impl Block {
        fn new(self_drawn: bool, disabled: bool) -> Self {
            Self {
                id: Id::str("x"),
                disabled,
                self_drawn,
            }
        }
    }

    impl Widget for Block {
        fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
            Decl::container(self.id, NodeKind::Box)
                .sense(ui_core::Sense::HOVER)
                .disabled(self.disabled)
        }

        fn handles_disabled(&self) -> bool {
            self.self_drawn
        }
    }

    #[test]
    fn a_control_that_handles_disabled_is_not_faded_a_second_time() {
        // The whole point of `handles_disabled`: a control that already paints 12%/38% must
        // reach the engine at full opacity, or it is dimmed twice.
        let opacity = |d: &Decl| d.data.params().map(|p| p.opacity).unwrap_or(1.0);
        let run = |widget: Box<dyn Widget>, stale: bool, scope: bool| {
            let mut store = ui_core::AnimStore::new();
            let mut core = ui_core::cx::Cx::new(&mut store, 0, 100, 100, 0.0, 0.0, 1.0, None);
            if stale {
                core.route(ui_core::cx::Routing {
                    disabled: vec![Id::str("x")],
                    ..ui_core::cx::Routing::default()
                });
            }
            let styled = Styled::new(widget, Env::root());
            let decl = if scope {
                core.with_disabled_scope(true, |core| styled.decl(core))
            } else {
                styled.decl(&mut core)
            };
            opacity(&decl)
        };
        assert!(
            (run(Box::new(Block::new(true, true)), false, false) - 1.0).abs() < 1e-6,
            "a self-drawing control keeps its opacity"
        );
        assert!(
            (run(Box::new(Block::new(false, true)), false, false) - 0.38).abs() < 1e-6,
            "a plain control that declares itself inert fades to 38%"
        );
        assert!(
            (run(Box::new(Block::new(false, true)), false, true) - 0.38).abs() < 1e-6,
            "an inert container fades the plain controls inside it"
        );
        // The bug this pins: the fade read the committed routing, so a control the same
        // build just enabled stayed faded until a second frame committed the change.
        assert!(
            (run(Box::new(Block::new(false, false)), true, false) - 1.0).abs() < 1e-6,
            "last frame's inertness must not fade a control this build enabled"
        );
    }
}
