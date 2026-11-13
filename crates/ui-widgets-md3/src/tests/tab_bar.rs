//! The MD3 secondary navigation tab bar: equal shares, the underline, the centred content and
//! the ink crossfade.

use super::*;

use crate::tab_bar::Md3TabBar;
use ui_core::cx::Routing;
use ui_core::draw::Shape;
use ui_core::geom::{Point, Rect};

const BAR: Id = Id::str("bar");
const WIDTH: f32 = 300.0;

/// A 24dp mark in the tab's ink: the icon variant's stand-in for a real glyph.
fn dot(id: Id, ink: Color) -> Decl {
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::fixed(24.0, 24.0))
        .shapes(vec![Shape::Box {
            corners: ui_core::Corners::ZERO,
            fill: ink,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }])
}

fn items(with_icon: bool) -> Vec<Md3TabItem<'static>> {
    ["One", "Two", "Three"]
        .into_iter()
        .enumerate()
        .map(|(k, label)| {
            if with_icon {
                Md3TabItem::with_icon(tab_id(k), label, ui_widgets::IconSource::Custom(dot))
            } else {
                Md3TabItem::text(tab_id(k), label)
            }
        })
        .collect()
}

fn build(
    store: &mut AnimStore,
    routing: Routing,
    now: f64,
    with_icon: bool,
    selected: usize,
    params: Md3Options,
) -> (Md3TabBar, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let list = items(with_icon);
    let control = Md3TabBar::new_with(
        params,
        BAR,
        &list,
        selected,
        WIDTH,
        &mut cx,
        &crate::pack::pack(),
    );
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl)
}

fn tab_id(k: usize) -> Id {
    BAR.with(("tab", k))
}

#[test]
fn a_press_selects_the_tab_under_the_pointer() {
    // The bar maps the press point itself; the app never sees a coordinate, so a wrong share
    // selects the neighbour and the app switches to the wrong page.
    let mut store = AnimStore::new();
    let (control, _) = build(
        &mut store,
        Routing::default(),
        0.0,
        false,
        2,
        Md3Options::default(),
    );
    let bounds = Rect::new(0.0, 0.0, WIDTH, control.height());
    let press = |x: f32, y: f32| ui_widgets::WidgetState {
        clicked: true,
        local: Some(Point::new(x, y)),
        ..Default::default()
    };
    // Every share's first pixel belongs to that share, not to the one before it.
    for (x, want) in [(20.0, 0), (120.0, 1), (199.0, 1)] {
        match control.change(&press(x, 20.0), bounds) {
            Some(ui_core::Change::Selected(Some(id))) => assert_eq!(id, tab_id(want), "x={x}"),
            other => panic!("x={x} expected Selected(tab {want}), got {other:?}"),
        }
    }
}
