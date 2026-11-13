//! A long list declares the rows its viewport shows, not the rows it holds.

use ui_core::{Decl, Id, NodeKind, SizeClass, geom};
use ui_platform::{App, AppLoop, Element, OffscreenTarget};
use ui_widgets::{RowWindow, Scroll};

const ROWS: usize = 10_000;
const LIST: Id = Id::str("virtual-list");

fn row_height(row: usize) -> f32 {
    if row % 2 == 0 { 24.0 } else { 40.0 }
}

fn row_id(row: usize) -> Id {
    Id::of(("virtual-row", row))
}

fn row_decl(row: usize) -> Decl {
    Decl::container(row_id(row), NodeKind::Column)
        .size(SizeClass::fixed(360.0, row_height(row)))
        .child(Decl::container(
            Id::of(("virtual-row-fill", row)),
            NodeKind::Box,
        ))
}

/// The whole list at `row_height` heights, with only the window declared.
struct LongList;

impl App for LongList {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let widgets = ui_widgets::Cx::new(cx, ui_style::Env::root());
        let window = RowWindow::of(widgets.core(), LIST, ROWS, 200.0, row_height);
        let mut column = Decl::container(Id::str("virtual-rows"), NodeKind::Column)
            .size(SizeClass::hug(0.0, 0.0));
        if window.above > 0.0 {
            column = column.child(
                Decl::container(Id::str("virtual-above"), NodeKind::Box)
                    .size(SizeClass::fixed(360.0, window.above)),
            );
        }
        for row in window.first..window.last {
            column = column.child(row_decl(row));
        }
        if window.below > 0.0 {
            column = column.child(
                Decl::container(Id::str("virtual-below"), NodeKind::Box)
                    .size(SizeClass::fixed(360.0, window.below)),
            );
        }
        ui_platform::from_decl(Scroll::fixed(LIST, 360.0, 300.0).child(column).resolve())
    }
}

fn extent_of<T: App>(lp: &AppLoop<T>) -> f32 {
    let node = lp
        .tree()
        .find_by_id(LIST)
        .and_then(|nid| lp.tree().get(nid))
        .expect("the scroller is laid out");
    node.scroll_extent.expect("layout measured the content").h
}

fn rect_of<T: App>(lp: &AppLoop<T>, id: Id) -> geom::Rect {
    lp.tree()
        .find_by_id(id)
        .and_then(|nid| lp.tree().get(nid))
        .map(|node| node.visual_rect())
        .expect("the node is laid out")
}

#[test]
fn a_long_list_declares_only_the_rows_its_viewport_shows() {
    let mut lp = AppLoop::new(LongList);
    let mut target = OffscreenTarget::new(360, 300);
    let first = lp.frame(&mut target, None);
    assert!(
        first.nodes < 100,
        "ten thousand rows must not become ten thousand nodes, got {}",
        first.nodes
    );
    assert!(
        lp.tree().find_by_id(row_id(0)).is_some(),
        "the first row is in view"
    );
    let total: f32 = (0..ROWS).map(row_height).sum();
    assert!(
        (extent_of(&lp) - total).abs() < 0.5,
        "the spacers keep the list's extent: {} against {total}",
        extent_of(&lp)
    );

    // A wheel moves the window; the rows it leaves are not declared at all.
    let pos = geom::Point::new(180.0, 150.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.dispatch(ui_core::Event::wheel(pos, 2000.0));
    lp.frame(&mut target, None);
    assert!(
        lp.tree().find_by_id(row_id(0)).is_none(),
        "row 0 scrolled out of the window"
    );
    assert!(
        lp.tree().find_by_id(row_id(60)).is_some(),
        "the rows the wheel reached are declared"
    );
}

/// The same exchange through the `List` control, which is the list the gallery shows.
struct WindowedList {
    pack: ui_style::Style,
    rows: Vec<String>,
    heights: ui_widgets::RowHeights,
}

impl App for WindowedList {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let widgets = ui_widgets::Cx::new(cx, ui_style::Env::root());
        let list = ui_widgets::List {
            id: LIST,
            width: 300.0,
            height: 200.0,
            row_height: 22.0,
            rows: self.rows.clone(),
            pack: &self.pack,
        }
        .resolve_in(&widgets, &mut self.heights);
        ui_platform::from_decl(list)
    }
}

fn list_ids(lp: &AppLoop<WindowedList>) -> (bool, bool) {
    let first = lp.tree().find_by_id(LIST.with(0usize)).is_some();
    let later = lp.tree().find_by_id(LIST.with(60usize)).is_some();
    (first, later)
}

#[test]
fn the_list_control_declares_only_the_rows_in_view() {
    let mut lp = AppLoop::new(WindowedList {
        pack: ui_style::legacy_pack(),
        rows: (0..1_000).map(|i| format!("row {i}")).collect(),
        heights: ui_widgets::RowHeights::new(22.0),
    });
    let mut target = OffscreenTarget::new(300, 200);
    let first = lp.frame(&mut target, None);
    assert!(
        first.nodes < 100,
        "a thousand rows must not become a thousand nodes, got {}",
        first.nodes
    );
    assert_eq!(
        list_ids(&lp),
        (true, false),
        "only the first rows are in view"
    );

    let node = lp
        .tree()
        .find_by_id(LIST)
        .and_then(|nid| lp.tree().get(nid))
        .expect("the list is laid out");
    let extent = node.scroll_extent.expect("layout measured the rows").h;
    assert!(
        (extent - 22_000.0).abs() < 0.5,
        "the spacers keep the list's extent, got {extent}"
    );

    let pos = geom::Point::new(150.0, 100.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.dispatch(ui_core::Event::wheel(pos, 1_320.0));
    lp.frame(&mut target, None);
    assert_eq!(
        list_ids(&lp),
        (false, true),
        "the wheel moves the declared rows down the list"
    );

    // The spacers and the row heights add up to the offset: row 60 begins 1,320 dp into
    // the list, which is exactly where the wheel put the viewport's top edge.
    let list_top = lp
        .tree()
        .find_by_id(LIST)
        .and_then(|nid| lp.tree().get(nid))
        .map(|node| node.visual_rect().origin.y)
        .expect("the list is laid out");
    let row_top = lp
        .tree()
        .find_by_id(LIST.with(60usize))
        .and_then(|nid| lp.tree().get(nid))
        .map(|node| node.visual_rect().origin.y)
        .expect("row 60 is declared");
    assert!(
        (row_top - list_top).abs() < 1.0,
        "row 60 sits at the viewport's top edge: {row_top} against {list_top}"
    );
}

const WRAPPING: usize = 400;
const WRAP_WIDTH: f32 = 220.0;
const WRAP_HEIGHT: f32 = 200.0;
const ROW_MIN: f32 = 22.0;
const SENTENCE: &str = "a row whose text wraps keeps every line it was given, and the list \
remembers the height layout measured so the rows above the window still add up when it \
comes back into view";

/// A list whose rows are taller than the estimate, so their heights can only come from layout.
struct WrappingList {
    pack: ui_style::Style,
    rows: Vec<String>,
    heights: ui_widgets::RowHeights,
}

impl App for WrappingList {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let widgets = ui_widgets::Cx::new(cx, ui_style::Env::root());
        let list = ui_widgets::List {
            id: LIST,
            width: WRAP_WIDTH,
            height: WRAP_HEIGHT,
            row_height: ROW_MIN,
            rows: self.rows.clone(),
            pack: &self.pack,
        }
        .resolve_in(&widgets, &mut self.heights);
        ui_platform::from_decl(list)
    }
}

#[test]
fn a_list_of_wrapping_rows_keeps_what_it_measured() {
    let mut lp = AppLoop::new(WrappingList {
        pack: ui_style::legacy_pack(),
        rows: (0..WRAPPING)
            .map(|row| format!("{row} {SENTENCE}"))
            .collect(),
        heights: ui_widgets::RowHeights::new(ROW_MIN),
    });
    let mut target = OffscreenTarget::new(WRAP_WIDTH as u32, WRAP_HEIGHT as u32);
    let first = lp.frame(&mut target, None);
    assert!(
        first.nodes < 100,
        "a window of wrapped rows is still a window, got {}",
        first.nodes
    );
    let wrapped = rect_of(&lp, LIST.with(0usize)).size.h;
    assert!(
        wrapped > ROW_MIN + 1.0,
        "a row that wraps grows past the one-line height, got {wrapped}"
    );

    // Walk the whole list a viewport at a time: every row passes through the window and
    // leaves behind the height layout gave it.
    let pos = geom::Point::new(WRAP_WIDTH * 0.5, WRAP_HEIGHT * 0.5);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    for _ in 0..(2 * WRAPPING) {
        lp.dispatch(ui_core::Event::wheel(pos, WRAP_HEIGHT));
        let frame = lp.frame(&mut target, None);
        assert!(
            frame.nodes < 100,
            "wrapped rows do not widen the window, got {}",
            frame.nodes
        );
    }
    let measured = extent_of(&lp);
    assert!(
        measured > WRAPPING as f32 * ROW_MIN * 1.5,
        "the extent is the sum of the measured rows, not the estimate: {measured}"
    );

    // Back to the top: the measured heights put row 0 exactly where the spacers say.
    for _ in 0..(2 * WRAPPING) {
        lp.dispatch(ui_core::Event::wheel(pos, -WRAP_HEIGHT));
        lp.frame(&mut target, None);
    }
    assert!(
        lp.tree().find_by_id(LIST.with(0usize)).is_some(),
        "the first row is back in view"
    );
    let list_top = rect_of(&lp, LIST).origin.y;
    let row_top = rect_of(&lp, LIST.with(0usize)).origin.y;
    assert!(
        (row_top - list_top).abs() < 1.0,
        "row 0 sits at the viewport's top edge: {row_top} against {list_top}"
    );
    assert!(
        (extent_of(&lp) - measured).abs() < 0.5,
        "the measured extent survives the trip back: {} against {measured}",
        extent_of(&lp)
    );
}

/// The index of the first declared row whose bottom passes the viewport's top edge.
fn viewport_top_row<T: App>(lp: &AppLoop<T>) -> usize {
    let list = rect_of(lp, LIST);
    (0..WRAPPING)
        .find(|row| {
            lp.tree()
                .find_by_id(LIST.with(*row))
                .and_then(|nid| lp.tree().get(nid))
                .is_some_and(|node| {
                    let rect = node.visual_rect();
                    rect.origin.y + rect.size.h > list.origin.y
                })
        })
        .expect("a row meets the viewport's top edge")
}

#[test]
fn a_stretch_never_measured_scrolls_at_the_measured_rate() {
    let mut lp = AppLoop::new(WrappingList {
        pack: ui_style::legacy_pack(),
        rows: (0..WRAPPING)
            .map(|row| format!("{row} {SENTENCE}"))
            .collect(),
        heights: ui_widgets::RowHeights::new(ROW_MIN),
    });
    let mut target = OffscreenTarget::new(WRAP_WIDTH as u32, WRAP_HEIGHT as u32);
    let pos = geom::Point::new(WRAP_WIDTH * 0.5, WRAP_HEIGHT * 0.5);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.frame(&mut target, None);
    lp.frame(&mut target, None);
    // Jump past the measured rows: the rows here have never been laid out.
    lp.dispatch(ui_core::Event::wheel(pos, 6_000.0));
    lp.frame(&mut target, None);
    lp.frame(&mut target, None);
    let before = viewport_top_row(&lp);
    lp.dispatch(ui_core::Event::wheel(pos, -300.0));
    lp.frame(&mut target, None);
    lp.frame(&mut target, None);
    let after = viewport_top_row(&lp);
    // The rows here are as tall as the ones already measured, so 300 points of travel
    // covers a couple of them, not a dozen.
    let advanced = before - after;
    assert!(
        advanced <= 4,
        "300 points of travel covers {} rows, more than the measured height allows",
        advanced
    );
}
