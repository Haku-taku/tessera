//! The MD3 snackbar: the inverse-surface bar, its two heights, the body and action type, and
//! the fade the app drives.

use super::*;

use crate::{Md3Snackbar, SnackbarParams};
use ui_core::cx::Routing;
use ui_core::draw::Shape;
use ui_core::geom::Rect;

const BAR: Id = Id::str("snack");
const WIDTH: f32 = 400.0;

fn build(
    store: &mut AnimStore,
    two_lines: bool,
    visible: bool,
    overlay: Option<f32>,
) -> (Md3Snackbar, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 600, 200, 0.0, 0.0, 1.0, None);
    core.route(Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3Snackbar::new_with(
        Md3Options::default(),
        SnackbarParams {
            id: BAR,
            message: "Message archived",
            action: Some("Undo"),
            two_lines,
            floating: false,
            width: WIDTH,
            visible,
            overlay,
            reveal: 1.0,
        },
        &mut cx,
        &crate::pack::pack(),
    );
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl)
}

/// The same bar with the pointer on its action pad: the one shape the pad paints is the
/// hover layer, so a quiet frame has nothing to infer a curve from.
fn hovered_bar(store: &mut AnimStore) -> Decl {
    let pad = BAR.with("action");
    let mut core = ui_core::cx::Cx::new(store, 0, 600, 200, 0.0, 0.0, 1.0, None);
    core.route(Routing {
        hovered: Some(pad),
        hovered_path: vec![pad],
        ..Routing::default()
    });
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3Snackbar::new_with(
        Md3Options::default(),
        SnackbarParams {
            id: BAR,
            message: "Message archived",
            action: Some("Undo"),
            two_lines: false,
            floating: false,
            width: WIDTH,
            visible: true,
            overlay: None,
            reveal: 1.0,
        },
        &mut cx,
        &crate::pack::pack(),
    );
    cx.with_cx(|cx| control.decl(cx))
}

fn action_pad(decl: &Decl) -> &Decl {
    let pad = decl
        .children
        .iter()
        .find(|child| child.id == BAR.with("action"))
        .expect("the action pad");
    pad
}

#[test]
fn the_action_pad_states_the_curve_its_splash_is_cut_to() {
    // The splash is declared from the press, and it is cut to the node's own curve; the pad's
    // curve therefore cannot come from the layer, which a press that lands before any hover
    // does not paint yet.
    let mut quiet = AnimStore::new();
    let quiet_bar = build(&mut quiet, false, true, None).1;
    let rest = action_pad(&quiet_bar);
    let mut hot = AnimStore::new();
    let hot_bar = hovered_bar(&mut hot);
    let hovered = action_pad(&hot_bar);
    assert!(rest.shapes.is_empty(), "the layer appears only while hovered");
    let painted = hovered
        .shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Box { corners, .. } => Some(*corners),
            _ => None,
        })
        .expect("the hover layer");
    assert_eq!(
        rest.data.params().and_then(|params| params.corners),
        Some(painted),
        "the quiet frame states the layer's own curve"
    );
    let ink = hovered
        .data
        .params()
        .and_then(|params| params.press_ink)
        .map(|ink| ink.target);
    assert_eq!(ink, Some(ui_core::InkTarget::Node), "the splash reads that curve");
}

#[test]
fn a_hidden_snackbar_paints_nothing_and_reports_nothing() {
    // The app keeps the bar mounted through its exit; at zero progress it must paint no
    // container and no text, or the "gone" snackbar would still cover the page.
    let mut store = AnimStore::new();
    let (_, hidden) = build(&mut store, false, false, None);
    assert!(hidden.shapes.is_empty(), "a faded bar paints no container");
    assert!(hidden.children.is_empty(), "and no text");
    let state = ui_widgets::WidgetState {
        clicked: true,
        ..Default::default()
    };
    let bounds = Rect::new(0.0, 0.0, WIDTH, 48.0);
    let mut store = AnimStore::new();
    let (control, _) = build(&mut store, false, false, None);
    assert!(
        control.change(&state, bounds).is_none(),
        "a hidden bar reports nothing"
    );
}

#[test]
fn a_floating_bar_hangs_centred_above_the_page_bottom() {
    // An out-of-flow box with neither horizontal side declared keeps the static origin, so a
    // lost inset pair parks the bar at the page's left edge. The exit must not move it either.
    let root = |decl: Decl| Decl::container(Id::str("root"), NodeKind::Column).child(decl);
    let mut store = AnimStore::new();
    let (_, shown) = build(&mut store, false, true, Some(24.0));
    let tree = lay_out(&root(shown), (600.0, 400.0));
    let (x, y, _, h) = region_rect(&tree, BAR);
    assert!((x - (600.0 - WIDTH) * 0.5).abs() < 0.5, "centred, got x = {x}");
    assert!((y + h - 376.0).abs() < 0.5, "24 above the bottom, got y = {y}, h = {h}");

    let mut store = AnimStore::new();
    let (_, faded) = build(&mut store, false, false, Some(24.0));
    let tree = lay_out(&root(faded), (600.0, 400.0));
    let (faded_x, faded_y, _, _) = region_rect(&tree, BAR);
    assert!(
        (faded_x - x).abs() < 0.5 && (faded_y - y).abs() < 0.5,
        "the exit holds still, got ({faded_x}, {faded_y})"
    );
}
