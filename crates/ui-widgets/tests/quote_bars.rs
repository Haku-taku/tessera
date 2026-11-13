//! A quote's bar: one per quote level on every line the quote holds, and one unbroken run
//! down the construct.
//!
//! A line no block owns used to take its cells from every quote that started above it, so a
//! `>`-only line among nested quotes drew an earlier quote's bar as well as its own, shifted
//! the real bar one column right, and rounded the ends of both where the run broke.

use ui_core::Id;
use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Rect};
use ui_platform::{App, AppLoop, Element};

const DOC: Id = Id::str("quote-bars");

struct Page {
    source: String,
}

impl App for Page {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let document = ui_richtext::parse(&self.source);
        let widgets = ui_widgets::Cx::new(cx, ui_style::Env::root());
        let markdown = ui_widgets::Markdown {
            id: DOC,
            document: std::sync::Arc::new(document),
            prepared: None,
            editable: true,
            math_editing: ui_widgets::MathEditing::Structural,
            popover_fill: Color::WHITE,
            width: 400.0,
            size: 14.0,
            ink: Color::BLACK,
            marker_ink: Color::BLACK,
            link_ink: Color::BLACK,
            code_ink: Color::BLACK,
            code_fill: Color::WHITE,
            code_style: None,
            code: ui_widgets::CodeOptions::default(),
            image: None,
            muted: Color::BLACK,
            caret_ink: Color::BLACK,
            selection_fill: Color::WHITE,
            selection_painter: None,
        };
        ui_platform::from_decl(markdown.resolve_in(&widgets))
    }
}

/// One bar the frame draws: its box, and how its corners round off.
#[derive(Clone, Copy, Debug)]
struct Bar {
    rect: Rect,
    corners: Corners,
}

/// One line's bars, left to right.
#[derive(Debug)]
struct BarRow {
    y: f32,
    bars: Vec<Bar>,
}

/// The bars this frame draws, one `BarRow` per line that has any.
fn bar_rows(loop_: &mut AppLoop<Page>) -> Vec<BarRow> {
    let tree = loop_.tree();
    let mut bars = Vec::new();
    fn walk(tree: &ui_core::Tree, nid: ui_core::NodeId, bars: &mut Vec<Bar>) {
        let Some(node) = tree.get(nid) else {
            return;
        };
        let corners = node.shapes.iter().find_map(|shape| match shape {
            Shape::Box { corners, .. } => Some(*corners),
            _ => None,
        });
        // A quote's bar is the widget's only box this narrow; a task box and a card are wider.
        if let Some(corners) = corners
            && (node.rect.size.w - 3.0).abs() < 0.01
            && node.rect.size.h > 0.0
        {
            bars.push(Bar {
                rect: node.rect,
                corners,
            });
        }
        for child in tree.children_of(Some(nid)) {
            walk(tree, *child, bars);
        }
    }
    if let Some(root) = tree.root() {
        walk(tree, root, &mut bars);
    }
    let mut rows: Vec<BarRow> = Vec::new();
    for bar in bars {
        match rows
            .iter_mut()
            .find(|row| (row.y - bar.rect.origin.y).abs() < 0.01)
        {
            Some(row) => row.bars.push(bar),
            None => rows.push(BarRow {
                y: bar.rect.origin.y,
                bars: vec![bar],
            }),
        }
    }
    rows.sort_by(|a, b| a.y.total_cmp(&b.y));
    for row in &mut rows {
        row.bars
            .sort_by(|a, b| a.rect.origin.x.total_cmp(&b.rect.origin.x));
    }
    rows
}

/// Every line of a quote draws its own levels' bars and no others, the levels line up down
/// the quote, and a run between rows is square where the rows meet.
#[test]
fn a_blank_line_inside_a_quote_draws_only_its_own_bars() {
    // Ten marker lines: seven write one `>`, the inner quote's three write two. The blank
    // line between the quotes belongs to none, and the two `>`-only lines inside them used
    // to draw a bar borrowed from the first quote.
    let source = "> earlier\n>\n> still earlier\n\n> outer one\n>\n>> inner one\n>>\n>> inner two\n>\n> outer two\n";
    let mut loop_ = AppLoop::with_stack(
        Page {
            source: source.to_owned(),
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    loop_.build_with_dt((440, 900), None, 0.016);
    let rows = bar_rows(&mut loop_);

    let depths: Vec<usize> = rows.iter().map(|row| row.bars.len()).collect();
    let mut histogram = depths.clone();
    histogram.sort_unstable();
    assert_eq!(
        histogram,
        [1, 1, 1, 1, 1, 1, 1, 2, 2, 2],
        "one bar per quote level per line, got {depths:?}"
    );

    // The inner quote's lines are one run in the middle of the document.
    let deep: Vec<usize> = depths
        .iter()
        .enumerate()
        .filter(|(_, depth)| **depth == 2)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        deep,
        [5, 6, 7],
        "the inner quote covers three lines: {depths:?}"
    );

    // Each level keeps one x down the whole document, so a blank line's bars line up with
    // the text lines above and below it instead of standing one column off.
    let columns: Vec<f32> = rows
        .iter()
        .find(|row| row.bars.len() == 2)
        .map(|row| row.bars.iter().map(|bar| bar.rect.origin.x).collect())
        .unwrap_or_default();
    for row in &rows {
        for bar in &row.bars {
            let x = bar.rect.origin.x;
            assert!(
                columns.iter().any(|column| (column - x).abs() < 0.01),
                "a bar at {x} stands off the columns {columns:?}"
            );
        }
    }

    // Consecutive rows that share a level draw one bar: their rects meet and neither end
    // rounds off at the joint. A blank line between two quotes separates bars that do not
    // run into each other, so only rows one line apart are compared.
    for pair in rows.windows(2) {
        let (upper, lower) = (&pair[0], &pair[1]);
        let height = upper.bars[0].rect.size.h;
        if lower.y - upper.y > height + 0.01 {
            continue;
        }
        for top in &upper.bars {
            let x = top.rect.origin.x;
            let Some(bottom) = lower
                .bars
                .iter()
                .find(|bar| (bar.rect.origin.x - x).abs() < 0.01)
            else {
                continue;
            };
            assert!(
                (top.rect.origin.y + height - bottom.rect.origin.y).abs() < 0.01,
                "bars at {x} leave a gap between {:?} and {:?}",
                top.rect,
                bottom.rect
            );
            assert_eq!(
                (
                    top.corners.bl,
                    top.corners.br,
                    bottom.corners.tl,
                    bottom.corners.tr
                ),
                (0.0, 0.0, 0.0, 0.0),
                "bars at {x} round off where they meet"
            );
        }
    }
}
