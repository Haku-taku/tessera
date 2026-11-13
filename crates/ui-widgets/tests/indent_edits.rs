//! A block's indentation belongs to the block: every line writes it as ordinary text, the
//! fence line is the member the source spells out, and an edit there is carried to the rest.

use ui_core::Id;
use ui_core::change::Change;
use ui_core::geom::{Color, Point};
use ui_core::{NodeData, PointerButton};
use ui_platform::{App, AppLoop, Element};

const DOC: Id = Id::str("indent");

const SOURCE: &str = "2. **启动服务**\n\n   ```bash\n   RUST_LOG=info\n   make test\n   ```\n";

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
            width: 560.0,
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

    fn changed(&mut self, id: Id, change: Change) -> bool {
        let Change::Text(text) = change else {
            return false;
        };
        if id != DOC {
            return false;
        }
        self.source = text;
        true
    }
}

fn build(source: &str) -> AppLoop<Page> {
    let mut lp = AppLoop::with_stack(
        Page {
            source: source.to_owned(),
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    lp.build_with_dt((640, 800), None, 0.016);
    lp
}

/// The painted text of the node drawing `needle`, and where that node sits.
fn painted(lp: &mut AppLoop<Page>, needle: &str) -> (Point, ui_core::geom::Rect, String) {
    fn walk(
        tree: &ui_core::Tree,
        nid: ui_core::NodeId,
        needle: &str,
        out: &mut Option<(ui_core::geom::Rect, String)>,
    ) {
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
            if out.is_none() && text.contains(needle) {
                *out = Some((node.rect, text));
            }
        }
        for child in tree.children_of(Some(nid)) {
            walk(tree, *child, needle, out);
        }
    }
    let mut found = None;
    if let Some(root) = lp.tree().root() {
        walk(lp.tree(), root, needle, &mut found);
    }
    let (rect, text) = found.unwrap_or_else(|| panic!("no node paints {needle:?}"));
    (Point::new(rect.origin.x, rect.origin.y), rect, text)
}

fn press(lp: &mut AppLoop<Page>, at: Point) {
    lp.dispatch(ui_core::Event::PointerPressed {
        pos: at,
        button: PointerButton::Primary,
    });
    lp.dispatch(ui_core::Event::PointerReleased {
        pos: at,
        button: PointerButton::Primary,
    });
    rebuild(lp);
}

/// Draw the next frame: the card reveals a fence from the frame after the caret moved.
fn rebuild(lp: &mut AppLoop<Page>) {
    let mut target = ui_platform::present::OffscreenTarget::new(640, 800);
    lp.frame(&mut target, None);
}

/// The view that starts at buffer offset `base`, and where it sits.
fn view_at(lp: &mut AppLoop<Page>, base: usize) -> (Point, ui_core::geom::Rect) {
    fn walk(
        tree: &ui_core::Tree,
        nid: ui_core::NodeId,
        base: usize,
        out: &mut Option<ui_core::geom::Rect>,
    ) {
        let Some(node) = tree.get(nid) else {
            return;
        };
        if let NodeData::Text { block, .. } = &node.data
            && out.is_none()
            && block.input_base == Some(base)
        {
            *out = Some(node.rect);
        }
        for child in tree.children_of(Some(nid)) {
            walk(tree, *child, base, out);
        }
    }
    let mut found = None;
    if let Some(root) = lp.tree().root() {
        walk(lp.tree(), root, base, &mut found);
    }
    let rect = found.unwrap_or_else(|| panic!("no view starts at {base}"));
    (Point::new(rect.origin.x, rect.origin.y), rect)
}

/// Enter the block whose code holds `body`, which reveals its fences, then press the
/// first byte of the fence line: the indentation stands there, as its own view.
fn open_fence(lp: &mut AppLoop<Page>, body: &str) {
    let (origin, _, _) = painted(lp, body);
    press(lp, Point::new(origin.x + 8.0, origin.y + 3.0));
    let source = lp.app_mut().source.clone();
    let fence = source.find("```").expect("a fence");
    let line = source[..fence].rfind('\n').map_or(0, |at| at + 1);
    let (origin, rect) = view_at(lp, line);
    press(lp, Point::new(origin.x + 1.0, origin.y + rect.size.h * 0.5));
}

/// Every painted text node's text, top to bottom.
fn rows(lp: &mut AppLoop<Page>) -> Vec<String> {
    fn walk(tree: &ui_core::Tree, nid: ui_core::NodeId, out: &mut Vec<(f32, String)>) {
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
                out.push((node.rect.origin.y, text));
            }
        }
        for child in tree.children_of(Some(nid)) {
            walk(tree, *child, out);
        }
    }
    let mut out = Vec::new();
    if let Some(root) = lp.tree().root() {
        walk(lp.tree(), root, &mut out);
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|(_, text)| text).collect()
}

fn type_text(lp: &mut AppLoop<Page>, text: &str) {
    for c in text.chars() {
        lp.dispatch(ui_core::Event::Text(c.to_string()));
    }
    rebuild(lp);
}

fn key(lp: &mut AppLoop<Page>, key: ui_core::Key) {
    lp.dispatch(ui_core::Event::KeyPressed { key });
    rebuild(lp);
}

/// A fence the block's own line indents: two columns, no container in the way.
const INDENTED_FENCE: &str = "  ```rust\n  let a = 1;\n  let b = 2;\n  ```\n";

#[test]
fn the_spelled_indentation_carries_to_every_line() {
    let mut lp = build(INDENTED_FENCE);
    open_fence(&mut lp, "let a");
    // The fence's own spaces are the value the block spells out; a caret stands in them,
    // and a deletion there is the block's, not one line's.
    key(&mut lp, ui_core::Key::Home);
    key(&mut lp, ui_core::Key::Backspace);
    assert_eq!(
        lp.app_mut().source,
        " ```rust\n let a = 1;\n let b = 2;\n ```\n",
        "every line of the block takes the width the fence now holds"
    );
    // Out of the block the card is the rendered one: the code the reader sees keeps its
    // own columns, so nothing of the indentation shows there.
    key(&mut lp, ui_core::Key::Escape);
    let (_, _, code) = painted(&mut lp, "let a");
    assert_eq!(
        code, "let a = 1;\nlet b = 2;",
        "the code keeps its own columns, so nothing of the indentation shows"
    );
    open_fence(&mut lp, "let a");

    lp.dispatch(ui_core::Event::Text(" ".to_string()));
    assert_eq!(
        lp.app_mut().source,
        INDENTED_FENCE,
        "a typed space reaches the block's other lines as well"
    );
    key(&mut lp, ui_core::Key::Escape);
    let (_, _, code) = painted(&mut lp, "let a");
    assert_eq!(
        code, "let a = 1;\nlet b = 2;",
        "the code still reads the same"
    );
}

#[test]
fn the_blocks_spacing_stands_left_of_the_card() {
    let mut lp = build(INDENTED_FENCE);
    let (card, before, _) = painted(&mut lp, "let a");
    open_fence(&mut lp, "let a");
    // The bytes the block spells before its fence are a view of their own, laid before
    // the fence rather than read as part of it.
    let (spacing, _) = view_at(&mut lp, 0);
    let (fence, _) = view_at(&mut lp, 2);
    assert!(
        spacing.x < fence.x,
        "the spacing is a view before the fence: {spacing:?} vs {fence:?}"
    );
    // The spacing takes its room out of the card, so the card and its text move aside
    // while its right edge stays exactly where the page had it.
    let (now, after, _) = painted(&mut lp, "let a");
    assert!(now.x > card.x, "the spacing stands before the card's text");
    assert_eq!(
        now.x + after.size.w,
        card.x + before.size.w,
        "the card keeps its right edge"
    );
}

#[test]
fn a_quoted_fences_indentation_is_the_blocks() {
    // The quote's cell runs to the content's column, so the spaces after `> ` are the
    // block's value; a typed space there is carried to the content lines.
    let mut lp = build(">   ```rust\n>   let a = 1;\n>   ```\n");
    open_fence(&mut lp, "let a");
    type_text(&mut lp, " ");
    assert_eq!(
        lp.app_mut().source,
        ">    ```rust\n>    let a = 1;\n>    ```\n",
        "every line of the block, the closing fence included, follows the fence line"
    );
}

#[test]
fn a_code_lines_indentation_is_plain_text() {
    let mut lp = build(INDENTED_FENCE);
    let (origin, _, _) = painted(&mut lp, "let a");
    // A code line's copy of the value is ordinary text like any other: a deletion there
    // removes one of that line's spaces and reaches no other line.
    press(&mut lp, Point::new(origin.x + 40.0, origin.y + 3.0));
    key(&mut lp, ui_core::Key::End);
    key(&mut lp, ui_core::Key::Home);
    key(&mut lp, ui_core::Key::Backspace);
    assert_eq!(
        lp.app_mut().source,
        "  ```rust\n let a = 1;\n  let b = 2;\n  ```\n",
        "only the line the caret stands on loses a space"
    );
}

const ZERO: &str = "```rust\nlet a = 1;\n```\n";

#[test]
fn the_first_space_typed_on_a_fence_is_the_blocks() {
    let mut lp = build(ZERO);
    open_fence(&mut lp, "let a");
    // The block has no indentation yet, so the space typed at the caret has no copy to
    // follow: it is the block's own, and every line takes it.
    type_text(&mut lp, " ");
    assert_eq!(lp.app_mut().source, " ```rust\n let a = 1;\n ```\n");
    type_text(&mut lp, " ");
    assert_eq!(lp.app_mut().source, "  ```rust\n  let a = 1;\n  ```\n");
}

#[test]
fn home_stands_where_the_lines_text_begins() {
    let mut lp = build(SOURCE);
    open_fence(&mut lp, "RUST_LOG");
    // The press put the caret in the fence's indentation; Home moves it to the line's text,
    // which is where a deletion edits the indentation rather than the line break.
    key(&mut lp, ui_core::Key::Home);
    let caret = lp
        .caret()
        .map(|(_, at)| at)
        .expect("the caret is in the input");
    assert_eq!(
        lp.app_mut().source.get(caret..caret + 3),
        Some("```"),
        "Home stands at the fence"
    );
    key(&mut lp, ui_core::Key::Backspace);
    // The item writes no marker on the fence's line, so the column its cell reserves is
    // the block's own indentation there, and every line takes the deletion.
    assert_eq!(
        lp.app_mut().source,
        "2. **启动服务**\n\n  ```bash\n  RUST_LOG=info\n  make test\n  ```\n",
        "the block's own indentation moves on every line"
    );
    // A second Home reaches the line's first byte, before the indentation.
    key(&mut lp, ui_core::Key::Home);
    let caret = lp
        .caret()
        .map(|(_, at)| at)
        .expect("the caret is in the input");
    let line = lp.app_mut().source[..caret]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    assert_eq!(caret, line, "the second Home reaches the line's first byte");
}

/// A fence that opens a list item's own line: the spaces after the marker are the item's
/// padding, so a space typed there is plain and reaches no content line.
#[test]
fn a_fence_after_a_marker_owns_no_indentation() {
    let mut lp = build("- ```rust\n  let a = 1;\n  ```\n");
    open_fence(&mut lp, "let a");
    // The caret stands after the item's padding, where a fence's own indentation would be.
    let fence = lp.app_mut().source.find("```").expect("a fence");
    while lp.caret().map(|(_, at)| at).unwrap_or(0) < fence {
        key(&mut lp, ui_core::Key::Right);
    }
    type_text(&mut lp, " ");
    assert_eq!(
        lp.app_mut().source,
        "-  ```rust\n  let a = 1;\n  ```\n",
        "the space grows the item's padding and no line follows it"
    );
}

#[test]
fn an_indented_block_has_no_fences() {
    // Four columns open an indented block, not a fence: its first and last lines are code
    // even when they read like delimiters.
    let mut lp = build("    ```bash\n    echo hi\n    ```\n");
    let rows = rows(&mut lp);
    assert!(
        rows.iter().any(|text| text == "```bash\necho hi\n```"),
        "the whole text is code: {rows:?}"
    );
    assert!(
        !rows.iter().any(|text| text == "```"),
        "an indented block writes no closing fence: {rows:?}"
    );
}
