//! MD3 list items: one to three lines, an optional leading slot and a state layer.

use super::*;

use crate::spec::{
    ListFlags, ListLayout, ListLines, ListSelection, ListVariant, ListVideo, list_spec,
};
use ui_core::cx::Routing;
use ui_core::draw::Shape;

const ROW: Id = Id::str("row");

fn dot(id: Id, ink: Color, size: f32) -> Decl {
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::fixed(size, size))
        .shapes(vec![Shape::Box {
            corners: ui_core::Corners::ZERO,
            fill: ink,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }])
}

fn build(
    store: &mut AnimStore,
    routing: Routing,
    item: ListItemParams,
    params: Md3Options,
) -> (Md3ListItem, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 400, 0.0, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let widget = Md3ListItem::new_with(params, item, &mut cx, &crate::pack::pack());
    let decl = cx.with_cx(|cx| widget.decl(cx));
    (widget, decl)
}

fn plain(lines: ListLineCount) -> (Md3ListItem, Decl) {
    let mut store = AnimStore::new();
    let item = ListItemParams::new(ROW, "Label").lines(lines);
    build(&mut store, Routing::default(), item, Md3Options::default())
}

fn find(decl: &Decl, id: Id) -> Option<&Decl> {
    if decl.id == id {
        return Some(decl);
    }
    decl.children.iter().find_map(|c| find(c, id))
}

/// The node id an item derives for one of its named slots.
fn list_slot(id: Id, name: ListItemSlot) -> Id {
    ui_core::slot::slot_id(id, name.slot())
}

fn base_spec(selected: bool, lines: ListLines) -> crate::spec::ListSpec {
    list_spec(
        ListFlags::NONE,
        lines,
        if selected {
            ListSelection::Selected
        } else {
            ListSelection::Unselected
        },
        ListLayout::Base,
        ListVariant::Base,
        ListVideo::Base,
    )
}

#[test]
fn a_filled_slot_takes_its_id_from_the_item_and_the_slot_name() {
    // The app names a slot, never an id: the node carrying the content is derived from
    // (item, name), so swapping the content inside cannot change the slot's identity.
    let art = dot(Id::str("custom-leading"), Color::WHITE, 20.0);
    let mut store = AnimStore::new();
    let item = ListItemParams::new(ROW, "Label").slot(ListItemSlot::Leading, art);
    let (_, decl) = build(&mut store, Routing::default(), item, Md3Options::default());
    let slot = find(&decl, list_slot(ROW, ListItemSlot::Leading))
        .expect("the slot node must carry the derived id");
    assert!(
        find(slot, Id::str("custom-leading")).is_some(),
        "the app's content keeps its own id inside the slot"
    );
}

#[test]
fn an_app_fill_outranks_the_built_in_mark() {
    // `mark` is the item's default content; a named fill replaces it instead of drawing on
    // top of it, or the app's icon sits over the built-in one.
    let mut store = AnimStore::new();
    let item = ListItemParams::new(ROW, "Label")
        .leading(ListLeading::Icon)
        .mark(dot)
        .slot(
            ListItemSlot::Leading,
            dot(Id::str("app-mark"), Color::WHITE, 24.0),
        );
    let (_, decl) = build(&mut store, Routing::default(), item, Md3Options::default());
    let slot = find(&decl, list_slot(ROW, ListItemSlot::Leading)).expect("the slot node");
    assert!(
        find(slot, Id::str("app-mark")).is_some(),
        "the fill is what the slot shows"
    );
    assert!(
        find(slot, list_slot(ROW, ListItemSlot::Leading).with("mark")).is_none(),
        "the built-in mark must not be drawn beside an app fill"
    );
}

#[test]
fn an_unfilled_leading_slot_leaves_no_gap_before_the_text() {
    // Slots are 0..n: an item with no leading content starts its text at the row inset,
    // not after a placeholder plus the slot gap.
    let (_, decl) = plain(ListLineCount::One);
    assert!(find(&decl, list_slot(ROW, ListItemSlot::Leading)).is_none());
    let root = Decl::container(Id::str("root"), NodeKind::Row).child(decl);
    let tree = lay_out(&root, (400.0, 400.0));
    let (x, _, _, _) = region_rect(&tree, ROW);
    let (tx, _, _, _) = region_rect(&tree, ROW.with("text"));
    let inset = base_spec(false, ListLines::OneLine).root.leading_space;
    assert!(
        (tx - x - inset).abs() < 0.01,
        "text at {tx}, row at {x}, inset {inset}"
    );
}

#[test]
fn slots_lay_out_around_the_text_column_when_filled_backwards() {
    // The app may fill the trailing slot first, but the item's layout is its own: leading
    // before the text column, trailing after it. Fill order must move neither.
    let mut store = AnimStore::new();
    let item = ListItemParams::new(ROW, "Label")
        .leading(ListLeading::Icon)
        .slot(
            ListItemSlot::Trailing,
            dot(Id::str("trail-fill"), Color::WHITE, 8.0),
        )
        .slot(
            ListItemSlot::Leading,
            dot(Id::str("lead-fill"), Color::WHITE, 24.0),
        );
    let (_, decl) = build(&mut store, Routing::default(), item, Md3Options::default());
    let root = Decl::container(Id::str("root"), NodeKind::Row).child(decl);
    let tree = lay_out(&root, (400.0, 400.0));
    let (lx, _, lw, _) = region_rect(&tree, list_slot(ROW, ListItemSlot::Leading));
    let (tx, _, _, _) = region_rect(&tree, ROW.with("text"));
    let (rx, _, _, _) = region_rect(&tree, list_slot(ROW, ListItemSlot::Trailing));
    assert!(
        lx + lw <= tx,
        "the leading slot must precede the text: {lx}+{lw} vs {tx}"
    );
    assert!(
        tx <= rx,
        "the trailing slot must follow the text: {tx} vs {rx}"
    );
}
