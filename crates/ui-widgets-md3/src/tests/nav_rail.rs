//! The MD3 navigation rail: a fixed-width column, the capsule that becomes a circle without
//! labels, and the press that names a destination.

use super::*;

use crate::spec::{NavigationRailFlags, navigation_rail_spec};
use crate::{Md3NavItem, Md3NavRail, RailLabelType, RailParams, RailVariant};
use ui_core::cx::Routing;
use ui_core::draw::Shape;
use ui_core::geom::{Point, Rect};

const RAIL: Id = Id::str("rail");

fn item_id(k: usize) -> Id {
    RAIL.with(("item", k))
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

fn build(store: &mut AnimStore, selected: usize, label_type: RailLabelType) -> (Md3NavRail, Decl) {
    build_at(store, selected, label_type, 0.0)
}

fn build_at(
    store: &mut AnimStore,
    selected: usize,
    label_type: RailLabelType,
    now: f64,
) -> (Md3NavRail, Decl) {
    build_variant(store, selected, label_type, RailVariant::Classic, now)
}

fn build_variant(
    store: &mut AnimStore,
    selected: usize,
    label_type: RailLabelType,
    variant: RailVariant,
    now: f64,
) -> (Md3NavRail, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 400, now, 0.0, 1.0, None);
    core.route(Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let list = items();
    let control = Md3NavRail::new_with(
        Md3Options::default(),
        RailParams {
            id: RAIL,
            items: &list,
            selected,
            label_type,
            variant,
            disabled: false,
        },
        &mut cx,
        crate::pack::pack(),
    );
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl)
}

#[test]
fn a_press_names_the_destination_under_the_pointer_and_reselection_reports_nothing() {
    // The rail maps the press by the stacked bands; an off-by-one band selects the neighbour,
    // and re-selecting the current destination must not make the app re-render.
    let spec = navigation_rail_spec(NavigationRailFlags::NONE);
    let mut store = AnimStore::new();
    // `Selected` gives the destinations unequal heights, which is where the bands can slip.
    let (control, _) = build(&mut store, 0, RailLabelType::Selected);
    let labelled = spec.active_indicator.height + 4.0 + spec.label_text.line_height + 12.0;
    let bare = spec.active_indicator.height + 12.0;
    let margin = 8.0;
    assert_eq!(control.height(), labelled + bare * 2.0 + margin * 2.0);
    let bounds = Rect::new(0.0, 0.0, control.width(), control.height());
    let press = |y: f32| ui_widgets::WidgetState {
        clicked: true,
        local: Some(Point::new(40.0, y)),
        ..Default::default()
    };
    for (y, want) in [(labelled + 1.0, 1), (labelled + bare + 1.0, 2)] {
        match control.change(&press(margin + y), bounds) {
            Some(ui_core::Change::Selected(Some(id))) => assert_eq!(id, item_id(want), "y={y}"),
            other => panic!("y={y} expected Selected(destination {want}), got {other:?}"),
        }
    }
    // Points where equal division would disagree with the real bands: the labelled destination
    // is taller, so its last 4dp and the next one's are only reachable by the real heights.
    assert!(
        control
            .change(&press(margin + labelled - 4.0), bounds)
            .is_none(),
        "still the current one"
    );
    match control.change(&press(margin + labelled + bare - 4.0), bounds) {
        Some(ui_core::Change::Selected(Some(id))) => {
            assert_eq!(id, item_id(1), "still destination 1")
        }
        other => panic!("expected Selected(destination 1), got {other:?}"),
    }
    assert!(
        control
            .change(&press(margin + labelled * 0.5), bounds)
            .is_none(),
        "the current destination"
    );
    assert!(
        control.change(&press(margin - 4.0), bounds).is_none(),
        "the top margin is no band"
    );
    assert!(
        control
            .change(&press(control.height() + 5.0), bounds)
            .is_none(),
        "below the rail"
    );
    let hover = ui_widgets::WidgetState {
        local: Some(Point::new(40.0, 10.0)),
        ..Default::default()
    };
    assert!(
        control.change(&hover, bounds).is_none(),
        "no click, no selection"
    );
}
