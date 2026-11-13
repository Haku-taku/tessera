//! MD3 text fields: the label's float rule and the band an outlined field reserves.

use super::*;

use crate::text_field::{Md3TextField, TextFieldParams, TextFieldVariant};
use ui_core::NodeData;
use ui_core::cx::Routing;

const FIELD: Id = Id::str("field");
const WIDTH: f32 = 200.0;

/// Lay out a filled field holding `text`, with the caret at `caret_x` in the input's space.
fn routed(text: &str, caret_x: Option<f32>) -> Tree {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 400, 0.0, 0.0, 1.0, None);
    core.route(Routing {
        caret_in_text: caret_x.map(|x| (FIELD.with("input"), x)),
        ..Routing::default()
    });
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let widget = Md3TextField::new_with(
        Md3Options::default(),
        TextFieldParams::new(FIELD, TextFieldVariant::Filled, WIDTH).text(text),
        &mut cx,
        &crate::pack::pack(),
    );
    laid_out(cx.with_cx(|cx| widget.decl(cx)))
}

/// The width the input may occupy: the content box minus its side padding.
fn content_w(tree: &Tree) -> f32 {
    let content = tree
        .get(tree.find_by_id(FIELD.with("content")).expect("content"))
        .expect("node");
    let pad = content.box_model.padding;
    content.rect.size.w - pad.left - pad.right
}

fn input_offset(tree: &Tree) -> f32 {
    let node = tree
        .get(tree.find_by_id(FIELD.with("input")).expect("input"))
        .expect("node");
    node.offset.x
}

#[test]
fn a_single_line_field_scrolls_exactly_far_enough_to_show_the_caret() {
    // A single-line editor never wraps; it slides the line under its mask, and only once the
    // caret has actually left the visible width.
    let text = "a buffer long enough that the field cannot show all of it at once";
    let tree = routed(text, None);
    let node = tree
        .get(tree.find_by_id(FIELD.with("input")).expect("input"))
        .expect("node");
    assert!(
        matches!(node.data, NodeData::Text { nowrap: true, .. }),
        "the input draws on one line"
    );
    let visible = content_w(&tree);

    let inside = routed(text, Some(visible * 0.5));
    assert_eq!(
        input_offset(&inside),
        0.0,
        "a caret inside the width scrolls nothing"
    );

    let past = routed(text, Some(visible + 40.0));
    let scrolled = -input_offset(&past);
    assert!(
        (40.0..=44.0).contains(&scrolled),
        "scrolled past the caret by the caret's own width and its slack, got {scrolled}"
    );
}
