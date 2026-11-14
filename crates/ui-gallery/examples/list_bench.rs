//! Naive long list versus a windowed one, so "extremely long page" gets a number.
//!
//! Run: `cargo run --release -p ui-gallery --example list_bench`

use std::time::Instant;

use ui_core::geom::{Color, Corners, Insets, Rect, Size};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{
    BuiltinLayout, Constraints, Decl, Id, LayoutEngine, NodeData, NodeKind, TextAlign, TextBlock,
    TextRun, TextStyle, Tree,
};
use ui_paint::CpuRenderer;

const ROW_H: f32 = 22.0;
const VIEW_H: f32 = 600.0;
const OVERSCAN: usize = 4;

fn row(i: usize, y: f32) -> Decl {
    let id = Id::of(("row", i));
    Decl::container(id, NodeKind::Box)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 400.0),
            height: AxisSize::fixed(ROW_H),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .offset(0.0, y)
        .background(
            if i.is_multiple_of(2) {
                Color::from_rgba8(30, 32, 38, 255)
            } else {
                Color::from_rgba8(26, 28, 34, 255)
            },
            Corners::ZERO,
        )
        .child(Decl {
            id: id.with("label"),
            kind: NodeKind::Text,
            shapes: Vec::new(),
            data: NodeData::Text {
                ink: Color::from_rgba8(220, 220, 226, 255),
                block: std::sync::Arc::new(TextBlock {
                    runs: vec![TextRun::new(format!("item {i}"), TextStyle::new(13.0))],
                    align: TextAlign::Start,
                    max_lines: None,
                    line_gap: 0.0,
                    indent: 0.0,
                    hanging: 0.0,
                    atoms: Vec::new(),
                    input_base: None,
                    line_numbers: None,
                    structures: Vec::new(),
                }),
                caret: None,
                selection: None,
                composing: None,
                nowrap: false,
            },
            size_class: SizeClass {
                width: AxisSize::hug(0.0, 200.0),
                height: AxisSize::hug(13.0, 18.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            ink_margin: Insets::ZERO,
            box_model: Default::default(),
            offset: ui_core::geom::Point::new(6.0, 2.0),
            cursor: None,
            sense: ui_core::Sense::NONE,
            disabled: false,
            grid: Default::default(),
            placement: Default::default(),
            env: None,
            drag: None,
            drag_out: false,
            drop_types: None,
            children: Vec::new(),
        })
}

/// Every item in the list: what a naive long page does.
fn naive(total: usize) -> Decl {
    let mut root = Decl::container(Id::str("page"), NodeKind::Column)
        .background(Color::from_rgba8(20, 22, 28, 255), Corners::ZERO);
    for i in 0..total {
        root = root.child(row(i, i as f32 * ROW_H));
    }
    root
}

/// Only the rows inside the viewport plus overscan: what a windowed list does.
fn windowed(total: usize, scroll: f32) -> (Decl, usize) {
    let first = ((scroll / ROW_H).floor() as usize).saturating_sub(OVERSCAN);
    let visible = (VIEW_H / ROW_H).ceil() as usize + OVERSCAN * 2;
    let last = (first + visible).min(total);
    let mut root = Decl::container(Id::str("page"), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 400.0),
            height: AxisSize::fixed(VIEW_H),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .background(Color::from_rgba8(20, 22, 28, 255), Corners::ZERO);
    for i in first..last {
        // Placed relative to the viewport, so coordinates never grow with the list.
        root = root.child(row(i, i as f32 * ROW_H - scroll));
    }
    (root, last - first)
}

fn main() {
    let mut tree = Tree::new();
    let mut engine = BuiltinLayout::new(ui_core::MonospaceMeasure::default());
    let mut renderer = CpuRenderer::new();
    let size = Size::new(400.0, VIEW_H);

    println!(
        "{:>9} {:>8} {:>9} {:>9} {:>9}",
        "items", "built", "apply", "layout", "raster"
    );
    for total in [1_000usize, 10_000, 100_000] {
        // Windowed: the row count is bounded by the viewport, not by the data.
        let scroll = (total / 2) as f32 * ROW_H;
        let (decl, built) = windowed(total, scroll);
        tree.apply(&decl);
        let root = tree.root().expect("root");
        engine.solve(&mut tree, root, Constraints::loose(size));
        let mut target = ui_paint::Pixels::new(400, VIEW_H as u32);
        ui_core::draw::list_from_tree(&tree, Rect::new(0.0, 0.0, 400.0, VIEW_H));

        let t = Instant::now();
        tree.apply(&windowed(total, scroll).0);
        let apply = t.elapsed();
        let root = tree.root().expect("root");
        let t = Instant::now();
        engine.solve(&mut tree, root, Constraints::loose(size));
        let layout = t.elapsed();
        let list = ui_core::draw::list_from_tree(&tree, Rect::new(0.0, 0.0, 400.0, VIEW_H));
        let t = Instant::now();
        renderer.render(&list, &mut target, &ui_core::damage::Damage::Full);
        let raster = t.elapsed();
        println!("{total:>9} {built:>8} {apply:>8.2?} {layout:>8.2?} {raster:>8.2?}");
    }

    // The naive path at 100k items, measured once: this is what the windowing avoids.
    let total = 100_000usize;
    let t = Instant::now();
    let decl = naive(total);
    tree.apply(&decl);
    let apply = t.elapsed();
    let root = tree.root().expect("root");
    let t = Instant::now();
    engine.solve(&mut tree, root, Constraints::loose(size));
    let layout = t.elapsed();
    println!(
        "naive {total}: nodes {} apply {apply:.2?} layout {layout:.2?}",
        tree.len()
    );
}
