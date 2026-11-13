//! The MD3 switch: the reference's size trajectory, four-branch palette, release rule.

use super::*;

use ui_core::Change;
use ui_core::cx::Routing;
use ui_core::geom::Point;

const ID: Id = Id::str("t");

/// Build one toggle in `store` at `now`, with the given routing and params.
fn build(
    store: &mut AnimStore,
    routing: Routing,
    now: f64,
    on: bool,
    params: Md3Options,
) -> (Box<dyn Widget>, Decl, ui_widgets::WidgetState) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let spec = ui_style::ToggleSpec::new(ID, on, "");
    let control =
        Md3Kit::with_scope(ui_style::StyleScope::default(), params).toggle(&spec, &mut cx);
    let state = cx.state(ID);
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl, state)
}

#[test]
fn a_release_decides_by_the_dragged_position_against_the_current_value() {
    // `position.value >= 0.5 != widget.value`: a drag to 0.6 turns an off switch on, and a
    // drag to 0.4 leaves it off; starting on, the same positions mirror.
    let params = Md3Options::default();
    for (start, dx, want) in [
        (false, 12.0, Some(true)),
        (false, 8.0, None),
        (true, -12.0, Some(false)),
        (true, -8.0, None),
    ] {
        let mut store = AnimStore::new();
        build(&mut store, Routing::default(), 0.0, start, params);
        build(
            &mut store,
            Routing {
                drag_owner: Some(ID),
                drag_step: Some(Point::new(dx, 0.0)),
                ..Routing::default()
            },
            0.0,
            start,
            params,
        );
        let (control, _, state) = build(
            &mut store,
            Routing {
                dropped_path: vec![ID],
                ..Routing::default()
            },
            0.0,
            start,
            params,
        );
        let got = match control.change(&state, ui_core::geom::Rect::ZERO) {
            Some(Change::Toggled { on }) => on,
            None => None,
            other => panic!("unexpected change {other:?}"),
        };
        assert_eq!(got, want, "start={start} dx={dx}");
    }
}
