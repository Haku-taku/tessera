//! A fenced block's own indentation and its closing line: what the code card paints, and
//! how many times. Both are shapes of the same construct, so they live in one file.

use ui_core::Id;
use ui_core::geom::{Color, Point, Rect};
use ui_core::{NodeData, PointerButton};
use ui_platform::{App, AppLoop, Element};

const DOC: Id = Id::str("fence");

struct Page {
    source: String,
    wrap: bool,
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
            width: 560.0,
            size: 14.0,
            ink: Color::BLACK,
            marker_ink: Color::BLACK,
            link_ink: Color::BLACK,
            code_ink: Color::BLACK,
            code_fill: Color::WHITE,
            code_style: None,
            code: ui_widgets::CodeOptions {
                wrap: self.wrap,
                ..ui_widgets::CodeOptions::default()
            },
            image: None,
            muted: Color::BLACK,
            caret_ink: Color::BLACK,
            selection_fill: Color::WHITE,
            selection_painter: None,
        };
        ui_platform::from_decl(markdown.resolve_in(&widgets))
    }
}

fn build(source: &str) -> AppLoop<Page> {
    build_with(source, true)
}

fn build_with(source: &str, wrap: bool) -> AppLoop<Page> {
    let mut loop_ = AppLoop::with_stack(
        Page {
            source: source.to_owned(),
            wrap,
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    loop_.build_with_dt((640, 800), None, 0.016);
    loop_
}

/// The text each text node actually paints, with the runs the card hides left out.
fn painted(loop_: &mut AppLoop<Page>) -> Vec<(Rect, String)> {
    let tree = loop_.tree();
    let mut out = Vec::new();
    fn walk(tree: &ui_core::Tree, nid: ui_core::NodeId, out: &mut Vec<(Rect, String)>) {
        let Some(node) = tree.get(nid) else {
            return;
        };
        if let NodeData::Text { block, .. } = &node.data {
            let text: String = block
                .runs
                .iter()
                .filter(|run| run.visibility.painted())
                .map(|run| run.text.as_str())
                .collect();
            if !text.trim().is_empty() {
                out.push((node.rect, text));
            }
        }
        for child in tree.children_of(Some(nid)) {
            walk(tree, *child, out);
        }
    }
    if let Some(root) = tree.root() {
        walk(tree, root, &mut out);
    }
    out
}

/// The body of the card whose code contains `needle`.
fn body(loop_: &mut AppLoop<Page>, needle: &str) -> Rect {
    painted(loop_)
        .into_iter()
        .find(|(_, text)| text.contains(needle))
        .map(|(rect, _)| rect)
        .unwrap_or_else(|| panic!("a code body holding {needle:?}"))
}

/// The fence inside a list item is indented by the item's content column. CommonMark strips
/// up to that many spaces from every content line, so no line starts with them: a reader
/// must not see the container's indentation inside the code.
#[test]
fn a_nested_fences_indentation_reaches_no_content_line() {
    let source = "1. 准备环境\n\n   然后执行：\n\n   ```bash\n   cargo build --release\n   cargo test --all-features\n   ```\n";
    let mut loop_ = build(source);
    let painted = painted(&mut loop_);
    assert!(
        painted
            .iter()
            .any(|(_, text)| text == "cargo build --release\ncargo test --all-features"),
        "the body paints both lines flush: {painted:?}"
    );
}

/// Deleting the break before a closing fence joins it to the last code line, and the block
/// runs to the end of its container. The card draws that fence on a line of its own without
/// moving a byte: the code stops where the fence begins, so the line is painted once and
/// only one view holds the caret. Nothing is made up for the break the source no longer has.
#[test]
fn a_closing_fence_at_the_end_of_a_line_is_drawn_as_its_own_line() {
    let source = "para\n\n```rust\nlet a = 1;\nlet b = 2;   ```\n";
    let mut loop_ = build(source);
    let rect = body(&mut loop_, "let a = 1;");
    let at = Point::new(
        rect.origin.x + rect.size.w - 4.0,
        rect.origin.y + rect.size.h - 4.0,
    );
    loop_.dispatch(ui_core::Event::PointerPressed {
        pos: at,
        button: PointerButton::Primary,
    });
    loop_.dispatch(ui_core::Event::PointerReleased {
        pos: at,
        button: PointerButton::Primary,
    });
    loop_.build_with_dt((640, 800), Some(at), 0.016);
    loop_.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Left,
    });
    loop_.build_with_dt((640, 800), Some(at), 0.016);
    let painted = painted(&mut loop_);
    let copies = painted
        .iter()
        .filter(|(_, text)| text.contains("let b = 2;"))
        .count();
    assert_eq!(copies, 1, "one line, one view: {painted:?}");
    assert!(
        painted.iter().any(|(_, text)| text == "```"),
        "the fence keeps its own line: {painted:?}"
    );
    assert_eq!(carets(&mut loop_), 1, "one caret, one view");
}

/// How many text nodes this frame gives the caret.
fn carets(loop_: &mut AppLoop<Page>) -> usize {
    let tree = loop_.tree();
    let mut count = 0;
    fn walk(tree: &ui_core::Tree, nid: ui_core::NodeId, count: &mut usize) {
        let Some(node) = tree.get(nid) else {
            return;
        };
        if let NodeData::Text { caret, .. } = &node.data {
            if caret.is_some() {
                *count += 1;
            }
        }
        for child in tree.children_of(Some(nid)) {
            walk(tree, *child, count);
        }
    }
    if let Some(root) = tree.root() {
        walk(tree, root, &mut count);
    }
    count
}

#[test]
fn an_unwrapped_strip_hands_the_engine_a_horizontal_bar() {
    // The card clips the line, so the strip is the only way to the rest of it. Without a bar
    // the overflow is invisible: the reader sees a line cut at the card's edge and no hint
    // that it continues.
    let source = "```rust\nlet unbroken = \"a line that is long enough to run past the card by a wide margin, several times over, because one word must reach the edge of the card and keep going\";\n```\n";
    let mut loop_ = build_with(source, false);
    // The bar's geometry is the previous frame's scroll state, as the scroll control's is.
    loop_.build_with_dt((640, 800), None, 0.016);
    let tree = loop_.tree();
    let node = tree
        .find_by_id(DOC.with(0usize).with("sideways"))
        .and_then(|nid| tree.get(nid))
        .expect("the card has a sideways strip");
    let bar = node
        .data
        .params()
        .and_then(|params| params.scrollbar)
        .expect("the strip declares a bar");
    assert!(
        bar.vertical.is_none(),
        "a sideways strip owns no vertical range"
    );
    let horizontal = bar.horizontal.expect("the sideways range needs a handle");
    assert!(
        horizontal.thumb.size.w < horizontal.track.size.w,
        "the thumb must show how much of the line is off card"
    );
}
