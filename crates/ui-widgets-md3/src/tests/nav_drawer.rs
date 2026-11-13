//! The MD3 standard navigation drawer: the 360dp surface, the inset capsule, the headline
//! groups and the press that names a destination.

use super::*;

use crate::{DrawerEntry, DrawerItem, DrawerParams, Md3NavDrawer};
use ui_core::cx::Routing;
use ui_core::draw::Shape;
use ui_core::geom::{Point, Rect};

const DRAWER: Id = Id::str("drawer");
const HOME: Id = Id::str("home");
const SEARCH: Id = Id::str("search");

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

fn build(store: &mut AnimStore, selected: Option<Id>) -> (Md3NavDrawer, Decl) {
    build_at(store, selected, 0.0)
}

fn build_at(store: &mut AnimStore, selected: Option<Id>, now: f64) -> (Md3NavDrawer, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 500, 500, now, 0.0, 1.0, None);
    core.route(Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let entries = vec![
        DrawerEntry::Headline("Mail"),
        DrawerEntry::Item(DrawerItem::new(
            HOME,
            "Home",
            ui_widgets::IconSource::Custom(dot),
        )),
        DrawerEntry::Item(DrawerItem::new(
            SEARCH,
            "Search",
            ui_widgets::IconSource::Custom(dot),
        )),
    ];
    let control = Md3NavDrawer::new_with(
        Md3Options::default(),
        DrawerParams {
            id: DRAWER,
            entries: &entries,
            selected,
            disabled: false,
            mode: crate::DrawerMode::Standard,
        },
        &mut cx,
        crate::pack::pack(),
    );
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl)
}

#[test]
fn a_press_names_the_destination_under_the_pointer_and_reselection_reports_nothing() {
    // The drawer walks its own rows, headlines included; a headline is not a destination, and
    // re-selecting the current one must not make the app re-render.
    let mut store = AnimStore::new();
    let (control, _) = build(&mut store, Some(HOME));
    let bounds = Rect::new(0.0, 0.0, control.width(), 400.0);
    let press = |y: f32| ui_widgets::WidgetState {
        clicked: true,
        local: Some(Point::new(100.0, y)),
        ..Default::default()
    };
    match control.change(&press(120.0), bounds) {
        Some(ui_core::Change::Selected(Some(id))) => assert_eq!(id, SEARCH),
        other => panic!("expected Selected(search), got {other:?}"),
    }
    assert!(
        control.change(&press(60.0), bounds).is_none(),
        "the current destination"
    );
    // Just above the second capsule: the bands must follow the layout's 12dp top inset.
    assert!(
        control.change(&press(110.0), bounds).is_none(),
        "still the first destination"
    );
    assert!(
        control.change(&press(20.0), bounds).is_none(),
        "the headline is no destination"
    );
    assert!(
        control.change(&press(300.0), bounds).is_none(),
        "below the last destination"
    );
    let hover = ui_widgets::WidgetState {
        local: Some(Point::new(100.0, 120.0)),
        ..Default::default()
    };
    assert!(
        control.change(&hover, bounds).is_none(),
        "no click, no selection"
    );
}
