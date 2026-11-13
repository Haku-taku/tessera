//! The MD3 checkbox: box geometry, fill/outline resolution, and the check that draws itself in.

use super::*;

use ui_core::cx::Routing;
use ui_core::geom::Rect;

const ID: Id = Id::str("c");

fn build(
    store: &mut AnimStore,
    routing: Routing,
    now: f64,
    checked: bool,
    error: bool,
    params: Md3Options,
) -> (Box<dyn Widget>, Decl, ui_widgets::WidgetState) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3Kit::with_scope(ui_style::StyleScope::default(), params)
        .checkbox(ID, checked, error, &mut cx);
    let state = cx.state(ID);
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl, state)
}

#[test]
fn a_click_reports_the_opposite_value_and_a_disabled_box_ignores_it() {
    // The click handler is the only place the semantic value can flip; inverting it the wrong
    // way leaves a checkbox that looks right and never changes.
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
        false,
        Md3Options::default(),
    );
    let change = control.change(&state, Rect::ZERO);
    assert!(
        matches!(change, Some(ui_core::Change::Toggled { on: Some(true) })),
        "{change:?}"
    );

    let disabled = Md3Options {
        disabled: true,
        ..Md3Options::default()
    };
    let mut store = AnimStore::new();
    let (control, _, state) = build(&mut store, clicked, 0.0, false, false, disabled);
    assert!(
        control.change(&state, Rect::ZERO).is_none(),
        "a disabled box ignores clicks"
    );
}

#[test]
fn enter_toggles_like_a_click_rather_than_reporting_a_bare_activation() {
    // The keyboard and the pointer reach the same change; a box that answers only clicks
    // hands the app an `Activated` it cannot read as a new value.
    let keyboard = Routing {
        activated_path: vec![ID],
        focused: Some(ID),
        ..Routing::default()
    };
    let mut store = AnimStore::new();
    let (control, _, state) = build(
        &mut store,
        keyboard,
        0.0,
        false,
        false,
        Md3Options::default(),
    );
    let change = control.change(&state, Rect::ZERO);
    assert!(
        matches!(change, Some(ui_core::Change::Toggled { on: Some(true) })),
        "{change:?}"
    );
}
