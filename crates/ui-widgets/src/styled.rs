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
        let cx = Cx::new(core, self.env.clone());
        let mut decl = self.control.decl(&cx);
        // A control that paints its own disabled colours must not be faded again.
        if cx.state(decl.id).disabled && !self.control.handles_disabled() {
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

    struct Block(Id);

    impl Widget for Block {
        fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
            Decl::container(self.0, NodeKind::Box).sense(ui_core::Sense::HOVER)
        }
    }

    #[test]
    fn a_control_that_handles_disabled_is_not_faded_a_second_time() {
        // The whole point of `handles_disabled`: a control that already paints 12%/38% must
        // reach the engine at full opacity, or it is dimmed twice.
        struct SelfDrawn(Id);
        impl Widget for SelfDrawn {
            fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
                Decl::container(self.0, NodeKind::Box)
            }
            fn handles_disabled(&self) -> bool {
                true
            }
        }
        let opacity = |d: &Decl| d.data.params().map(|p| p.opacity).unwrap_or(1.0);
        let run = |widget: Box<dyn Widget>| {
            let mut store = ui_core::AnimStore::new();
            let mut core = ui_core::cx::Cx::new(&mut store, 0, 100, 100, 0.0, 0.0, 1.0, None);
            core.route(ui_core::cx::Routing {
                disabled: vec![Id::str("x")],
                ..ui_core::cx::Routing::default()
            });
            opacity(&Styled::new(widget, Env::root()).decl(&mut core))
        };
        assert!(
            (run(Box::new(SelfDrawn(Id::str("x")))) - 1.0).abs() < 1e-6,
            "a self-drawing control keeps its opacity"
        );
        assert!(
            (run(Box::new(Block(Id::str("x")))) - 0.38).abs() < 1e-6,
            "a plain control still fades to 38%"
        );
    }
}
