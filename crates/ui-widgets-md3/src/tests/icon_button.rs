//! The MD3 icon button: the width arithmetic, the corner morph and which styles paint a box.

use super::*;

use crate::icon_button::{
    ICON_BUTTON_PRESSED_RADIUS, ICON_BUTTON_TOGGLE_RADIUS, IconButtonParams, IconButtonSize,
    IconButtonStyle, IconButtonWidth, Md3IconButton, corner_target, icon_button_size,
};
use ui_core::draw::Shape;
use ui_core::geom::Corners;

const BUTTON: Id = Id::str("icon-button/test");

fn build(params: IconButtonParams) -> Decl {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 600, 800, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3IconButton::new_with(params, &mut cx, crate::pack::pack());
    cx.with_cx(|cx| control.decl(cx))
}

/// The same button with the pointer on it: the hover layer only exists under real routing.
fn build_hot(params: IconButtonParams) -> Decl {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 600, 800, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing {
        hovered: Some(params.id),
        hovered_path: vec![params.id],
        ..ui_core::cx::Routing::default()
    });
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3IconButton::new_with(params, &mut cx, crate::pack::pack());
    cx.with_cx(|cx| control.decl(cx))
}

#[test]
fn the_box_is_the_mark_plus_both_spaces() {
    // Widths are not constants: each is the size's mark plus the width axis's two paddings.
    let medium = |width| {
        icon_button_size(
            IconButtonStyle::Filled,
            crate::spec::IconButtonSelection::Base,
            IconButtonSize::Medium,
            width,
        )
    };
    assert_eq!(medium(IconButtonWidth::Narrow), (48.0, 56.0, 24.0));
    assert_eq!(medium(IconButtonWidth::Default), (56.0, 56.0, 24.0));
    assert_eq!(medium(IconButtonWidth::Wide), (72.0, 56.0, 24.0));
    // A small default box is square; the largest wide one is not.
    assert_eq!(
        icon_button_size(
            IconButtonStyle::Standard,
            crate::spec::IconButtonSelection::Base,
            IconButtonSize::XSmall,
            IconButtonWidth::Default,
        ),
        (32.0, 32.0, 20.0)
    );
    assert_eq!(
        icon_button_size(
            IconButtonStyle::Standard,
            crate::spec::IconButtonSelection::Base,
            IconButtonSize::XLarge,
            IconButtonWidth::Wide,
        )
        .0,
        184.0
    );
}

#[test]
fn a_toggle_holds_one_square_and_a_press_narrows_it() {
    // The hover layer, the checked fill and the resting box are one shape for a toggle; a
    // plain button has no checked fill to agree with and stays round.
    assert_eq!(
        corner_target(true, Some(true), 20.0),
        ICON_BUTTON_PRESSED_RADIUS
    );
    assert_eq!(
        corner_target(true, Some(false), 20.0),
        ICON_BUTTON_PRESSED_RADIUS
    );
    assert_eq!(
        corner_target(false, Some(true), 20.0),
        ICON_BUTTON_TOGGLE_RADIUS
    );
    assert_eq!(
        corner_target(false, Some(false), 20.0),
        ICON_BUTTON_TOGGLE_RADIUS
    );
    assert_eq!(corner_target(false, None, 28.0), 28.0);
}

#[test]
fn the_fill_the_layer_and_the_ripple_are_one_shape() {
    // Three paints of one background: the container, the layer over it and the splash under
    // a press. A curve of its own would let the highlight drift from the fill it sits on, and
    // a ripple clipped to a shape the button has left behind outlives it as a ghost.
    let decl = build_hot(
        IconButtonParams::new(
            BUTTON,
            ui_widgets::IconSource::glyph("star", ui_widgets::IconSet::Outlined),
        )
        .style(IconButtonStyle::Tonal)
        .selected(true),
    );
    let params = decl.data.params().expect("a container");
    let curve = params.corners.expect("the node's own curve");
    let boxes: Vec<Corners> = decl
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Box { corners, .. } => Some(*corners),
            _ => None,
        })
        .collect();
    assert_eq!(boxes, vec![curve, curve], "container and hover layer");
    let ink = params.press_ink.expect("a press splashes");
    assert_eq!(
        ink.target,
        ui_core::InkTarget::Node,
        "the ripple clips to the node, which is that same background"
    );
}

#[test]
fn only_the_filled_and_tonal_styles_paint_a_box() {
    // The export's base is the filled button, so a standard or outlined query still returns
    // its container colour; painting it would put a filled disc under every icon.
    let shapes_of = |style| {
        let decl = build(
            IconButtonParams::new(
                BUTTON,
                ui_widgets::IconSource::glyph("star", ui_widgets::IconSet::Outlined),
            )
            .style(style),
        );
        decl.shapes.clone()
    };
    assert!(
        shapes_of(IconButtonStyle::Standard).is_empty(),
        "a standard button draws nothing at rest"
    );
    let outlined = shapes_of(IconButtonStyle::Outlined);
    assert!(
        matches!(outlined.as_slice(), [Shape::Box { border: Some((_, 1.0)), fill, .. }] if fill.a == 0.0),
        "an outlined button is a 1 dp stroke with no fill, got {outlined:?}"
    );
    for style in [IconButtonStyle::Filled, IconButtonStyle::Tonal] {
        let filled = shapes_of(style);
        assert!(
            matches!(filled.as_slice(), [Shape::Box { border: None, fill, .. }] if fill.a == 1.0),
            "{style:?} paints an opaque container, got {filled:?}"
        );
    }
}
