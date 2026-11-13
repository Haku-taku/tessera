//! The MD3 side sheet: the form axis, the docked edge and the line on its inner edge.

use super::*;

use crate::side_sheet::{Md3SideSheet, SIDE_SHEET_RADIUS, SheetEdge, SideSheetParams};
use crate::spec::{SideSheetFlags, SideSheetForm, side_sheet_spec};
use ui_core::draw::Shape;
use ui_core::layout::{AxisSize, Grow};

const SHEET: Id = Id::str("side-sheet/test");

fn build(params: SideSheetParams) -> Decl {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 800, 600, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3SideSheet::new_with(params, &mut cx, crate::pack::pack());
    cx.with_cx(|cx| control.decl(cx))
}

fn standard(edge: SheetEdge) -> Decl {
    build(
        SideSheetParams::new(SHEET, edge)
            .form(SideSheetForm::Standard)
            .barrier(false),
    )
}

/// The sheet as an app places it: a page row with the sheet against one edge.
fn docked(edge: SheetEdge) -> Decl {
    let sheet = standard(edge);
    let mut page = Decl::container(Id::str("side-sheet/page"), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .cross_align(1.0);
    page = match edge {
        SheetEdge::Start => page.child(sheet),
        SheetEdge::End => page
            .child(
                Decl::container(Id::str("side-sheet/pad"), NodeKind::Row).size(SizeClass {
                    width: AxisSize::hug(0.0, 0.0),
                    height: AxisSize::fill(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::No,
                }),
            )
            .child(sheet),
    };
    page
}

#[test]
fn the_two_forms_read_their_own_panel_from_the_table() {
    let standard = side_sheet_spec(SideSheetFlags::NONE, SideSheetForm::Standard);
    assert_eq!(standard.container.color, "surface");
    assert_eq!(standard.container.elevation, 0.0);
    assert_eq!(standard.container.width, 256.0);
    let modal = side_sheet_spec(SideSheetFlags::NONE, SideSheetForm::Modal);
    assert_eq!(modal.container.color, "surfaceContainerLow");
    assert_eq!(modal.container.elevation, 1.0);
    // The header and the line are the same in both forms.
    assert_eq!(modal.headline.type_, "title-large");
    assert_eq!(modal.divider.color, "outline");
}

#[test]
fn the_divider_sits_on_the_inner_edge_and_the_corner_on_the_modal_one() {
    // A start-docked sheet keeps its line on the right, an end-docked one on the left;
    // swapping the branch is the mistake this names.
    for (edge, left) in [(SheetEdge::Start, 0.0), (SheetEdge::End, 544.0)] {
        let tree = lay_out(&docked(edge), (800.0, 600.0));
        let surface = region_rect(&tree, SHEET.with("surface"));
        let divider = region_rect(&tree, SHEET.with("surface").with("divider"));
        let body = region_rect(&tree, SHEET.with("surface").with("body"));
        assert_eq!(surface, (left, 0.0, 256.0, 600.0), "{edge:?}");
        let divider_x = if edge == SheetEdge::Start {
            left + 255.0
        } else {
            left
        };
        assert_eq!(divider, (divider_x, 0.0, 1.0, 600.0), "{edge:?}");
        let body_x = if edge == SheetEdge::Start {
            left
        } else {
            left + 1.0
        };
        assert_eq!(body, (body_x, 0.0, 255.0, 600.0), "{edge:?}");
    }
    let modal = build(SideSheetParams::new(SHEET, SheetEdge::Start).barrier(false));
    let corners = match modal.shapes.first() {
        Some(Shape::Box { corners, .. }) => *corners,
        other => panic!("the modal surface is one box, got {other:?}"),
    };
    assert_eq!(
        (corners.tl, corners.tr, corners.br, corners.bl),
        (0.0, SIDE_SHEET_RADIUS, SIDE_SHEET_RADIUS, 0.0),
        "only the inner edge is rounded"
    );
}
