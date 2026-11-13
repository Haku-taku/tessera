//! The MD3 radio button: the ring's colour is the mark, and the dot grows out of the centre.

use super::*;

use ui_core::cx::Routing;
use ui_core::geom::Rect;

const ID: Id = Id::str("r");

fn build(
    store: &mut AnimStore,
    routing: Routing,
    now: f64,
    selected: bool,
    params: Md3Options,
) -> (Box<dyn Widget>, Decl, ui_widgets::WidgetState) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control =
        Md3Kit::with_scope(ui_style::StyleScope::default(), params).radio(ID, selected, &mut cx);
    let state = cx.state(ID);
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl, state)
}

#[test]
fn a_click_selects_an_unselected_radio_and_a_selected_one_reports_nothing() {
    // The group owns the selection, so a click only ever says "this one"; reporting on an
    // already-selected radio would let a click clear the group.
    let clicked = Routing {
        clicked_path: vec![ID],
        ..Routing::default()
    };
    let mut store = AnimStore::new();
    let (control, _, state) = build(
        &mut store,
        clicked.clone(),
        0.0,
        false,
        Md3Options::default(),
    );
    let change = control.change(&state, Rect::ZERO);
    assert!(
        matches!(&change, Some(ui_core::Change::Selected(Some(id))) if *id == ID),
        "{change:?}"
    );

    let mut store = AnimStore::new();
    let (control, _, state) = build(&mut store, clicked, 0.0, true, Md3Options::default());
    assert!(
        control.change(&state, Rect::ZERO).is_none(),
        "already selected: nothing changes"
    );
}
