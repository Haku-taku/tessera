//! The MD3 search view: the bar's surface grown over the results it finds.

use super::*;

use crate::search_view::{Md3SearchView, SearchViewChange, SearchViewParams};
use ui_core::cx::Routing;
use ui_core::geom::{Rect, Size};
use ui_widgets::WidgetState;

const VIEW: Id = Id::str("view");
const COLLAPSED: Size = Size::new(320.0, 56.0);
const OPEN: Size = Size::new(320.0, 420.0);
const ACTION: Id = Id::str("close");

fn icon(id: Id, ink: Color) -> Decl {
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::fixed(24.0, 24.0))
        .background(ink, ui_core::geom::Corners::ZERO)
}

fn build_at(
    store: &mut AnimStore,
    expanded: bool,
    now: f64,
    routing: Routing,
) -> (Md3SearchView, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 480, 600, now, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let params = SearchViewParams::new(VIEW, COLLAPSED, OPEN)
        .text("query")
        .hint("Search")
        .expanded(expanded)
        .leading(ui_widgets::IconSource::Custom(icon))
        .action(ACTION, ui_widgets::IconSource::Custom(icon))
        .results(vec![
            Decl::container(Id::str("hit"), NodeKind::Box).size(SizeClass::fixed(200.0, 40.0)),
        ]);
    let view = Md3SearchView::new_with(Md3Options::default(), params, &mut cx, crate::pack::pack());
    let decl = cx.with_cx(|cx| view.decl(cx));
    (view, decl)
}

fn build(store: &mut AnimStore, expanded: bool, now: f64) -> (Md3SearchView, Decl) {
    build_at(store, expanded, now, Routing::default())
}

#[test]
fn the_resting_bar_asks_to_open_and_an_open_view_does_not() {
    // The bar is a button that asks the app to open the view; once open, a click in the
    // header is an ordinary edit and must not keep asking — that loop would never settle.
    let mut store = AnimStore::new();
    let (shut, _) = build(&mut store, false, 0.0);
    let click = WidgetState {
        clicked: true,
        ..Default::default()
    };
    match shut
        .change(&click, Rect::ZERO)
        .expect("the bar reports")
        .data::<SearchViewChange>()
    {
        Ok(SearchViewChange::Activated) => {}
        Ok(other) => panic!("a resting click is not {other:?}"),
        Err(other) => panic!("not a search view change: {other:?}"),
    }
    let (open, _) = build(&mut store, true, 1.0);
    assert!(
        open.change(&click, Rect::ZERO).is_none(),
        "an open view stays open"
    );

    // An action reports itself, open or shut.
    let routing = Routing {
        clicked_path: vec![VIEW, ACTION],
        ..Routing::default()
    };
    let (shut, _) = build_at(&mut store, false, 1.0, routing);
    match shut
        .change(&click, Rect::ZERO)
        .expect("the action reports")
        .data::<SearchViewChange>()
    {
        Ok(SearchViewChange::Action(id)) => assert_eq!(id, ACTION),
        Ok(other) => panic!("an action click is not {other:?}"),
        Err(other) => panic!("not a search view change: {other:?}"),
    }
}
