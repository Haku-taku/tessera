//! The MD3 dialog: a modal surface with an icon, a title, a body and the app's actions.

use super::*;

use ui_core::cx::Routing;
use ui_core::geom::Rect;
use ui_core::{Change, ModalTarget};
use ui_widgets::WidgetState;

const D: Id = Id::str("dialog");
const WIDTH: f32 = 320.0;
const LONG: &str = "A body long enough to need more than one line inside the surface, \
                    because a dialog wraps at its own insets and not at the window.";

fn action(label: &str) -> DialogAction {
    DialogAction {
        id: D.with(label),
        label: label.to_string(),
    }
}

fn content() -> DialogContent {
    DialogContent {
        id: D,
        title: Some("Discard draft?".to_string()),
        body: Some(LONG.to_string()),
        content: Vec::new(),
        actions: vec![action("cancel"), action("discard")],
        icon: DialogIcon::None,
        build_icon: None,
        width: WIDTH,
        target: ModalTarget::Global,
        barrier: true,
    }
}

fn build(store: &mut AnimStore, routing: Routing, content: DialogContent) -> (Md3Dialog, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 300, 0.0, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let dialog = Md3Dialog::new_with(
        Md3Options::default(),
        content,
        &mut cx,
        &crate::pack::pack(),
    );
    let decl = cx.with_cx(|cx| dialog.decl(cx));
    (dialog, decl)
}

#[test]
fn a_click_on_the_scrim_cancels_and_a_click_inside_the_surface_does_not() {
    // A modal's barrier dismisses; a press on the surface's own padding must not, or the
    // dialog closes whenever the user misses a button by two pixels.
    let scrim = Routing {
        clicked_path: vec![D],
        ..Routing::default()
    };
    let mut store = AnimStore::new();
    let (dialog, _) = build(&mut store, scrim, content());
    assert!(matches!(
        dialog.change(&WidgetState::default(), Rect::ZERO),
        Some(Change::Cancelled)
    ));
    let inside = Routing {
        clicked_path: vec![D, D.with("surface"), D.with("body-box"), D.with("body")],
        ..Routing::default()
    };
    let mut store = AnimStore::new();
    let (dialog, _) = build(&mut store, inside, content());
    assert!(dialog.change(&WidgetState::default(), Rect::ZERO).is_none());
}
