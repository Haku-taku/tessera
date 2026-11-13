//! The MD3 navigation bar: equal destinations, the 64×32 capsule, the label behaviour and the
//! press that names a destination.

use super::*;

use crate::{Md3NavBar, Md3NavItem, NavBarParams, NavLabelBehavior};
use ui_core::cx::Routing;
use ui_core::draw::Shape;
use ui_core::geom::{Point, Rect};

const BAR: Id = Id::str("nav");
const WIDTH: f32 = 300.0;

fn item_id(k: usize) -> Id {
    BAR.with(("item", k))
}

/// A 24dp mark in the destination's ink: the stand-in for a real glyph.
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

fn items() -> Vec<Md3NavItem<'static>> {
    ["Home", "Search", "Library"]
        .into_iter()
        .enumerate()
        .map(|(k, label)| Md3NavItem::new(item_id(k), label, ui_widgets::IconSource::Custom(dot)))
        .collect()
}

fn build(
    store: &mut AnimStore,
    selected: usize,
    label_behavior: NavLabelBehavior,
) -> (Md3NavBar, Decl) {
    build_at(store, selected, label_behavior, 0.0)
}

fn build_at(
    store: &mut AnimStore,
    selected: usize,
    label_behavior: NavLabelBehavior,
    now: f64,
) -> (Md3NavBar, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
    core.route(Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let list = items();
    let control = Md3NavBar::new_with(
        Md3Options::default(),
        NavBarParams {
            id: BAR,
            items: &list,
            selected,
            label_behavior,
            disabled: false,
        },
        &mut cx,
        &crate::pack::pack(),
    );
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl)
}

#[test]
fn a_press_names_the_destination_under_the_pointer_and_reselection_reports_nothing() {
    // The bar maps the press point itself, so a wrong share selects the neighbour; and
    // re-selecting the current destination must not make the app re-render.
    let mut store = AnimStore::new();
    let (control, _) = build(&mut store, 0, NavLabelBehavior::AlwaysShow);
    let bounds = Rect::new(0.0, 0.0, WIDTH, control.height());
    let press = |x: f32, y: f32| ui_widgets::WidgetState {
        clicked: true,
        local: Some(Point::new(x, y)),
        ..Default::default()
    };
    for (x, want) in [(199.0, 1), (260.0, 2)] {
        match control.change(&press(x, 40.0), bounds) {
            Some(ui_core::Change::Selected(Some(id))) => assert_eq!(id, item_id(want), "x={x}"),
            other => panic!("x={x} expected Selected(destination {want}), got {other:?}"),
        }
    }
    assert!(
        control.change(&press(99.0, 40.0), bounds).is_none(),
        "the current destination"
    );
    assert!(
        control.change(&press(120.0, 100.0), bounds).is_none(),
        "below the bar"
    );
    let hover = ui_widgets::WidgetState {
        local: Some(Point::new(20.0, 40.0)),
        ..Default::default()
    };
    assert!(
        control.change(&hover, bounds).is_none(),
        "no click, no selection"
    );
}
