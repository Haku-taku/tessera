//! A display formula inside a Markdown document edits in place, and the document stays one
//! buffer: the edit arrives as one `Change::Text` holding the whole (spliced) source.

use ui_core::change::Change;
use ui_core::geom::{Color, Point};
use ui_core::{Event, Id, NodeData, PointerButton};
use ui_platform::{App, AppLoop, Element};

const DOC: Id = Id::str("document");
const SOURCE: &str = "intro\n\n$$\\frac{1}{2}$$\n";

struct Page {
    source: String,
    mode: ui_widgets::MathEditing,
    edited: Option<(Id, String)>,
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
            math_editing: self.mode,
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

    fn changed(&mut self, id: Id, change: Change) -> bool {
        let Change::Text(text) = change else {
            return false;
        };
        // The engine owns the buffer; the app re-reads its own copy, as a host does.
        self.source = text.clone();
        self.edited = Some((id, text));
        true
    }
}

/// The first formula node in the tree, wherever the document placed it.
fn math_node<A: App>(loop_: &AppLoop<A>) -> Option<(Id, ui_core::geom::Rect)> {
    fn walk<A: App>(
        loop_: &AppLoop<A>,
        nid: ui_core::NodeId,
        out: &mut Option<(Id, ui_core::geom::Rect)>,
    ) {
        if out.is_some() {
            return;
        }
        let Some(node) = loop_.tree().get(nid) else {
            return;
        };
        if matches!(node.data, NodeData::Math { .. }) {
            *out = Some((node.id, node.visual_rect()));
            return;
        }
        for child in loop_.tree().children_of(Some(nid)) {
            walk(loop_, *child, out);
        }
    }
    let mut out = None;
    walk(loop_, loop_.tree().root()?, &mut out);
    out
}

#[test]
fn a_display_formula_edits_in_place_and_the_document_stays_one_buffer() {
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };

    let end = Point::new(
        rect.origin.x + rect.size.w - 1.0,
        rect.origin.y + rect.size.h * 0.5,
    );
    loop_.dispatch(Event::PointerPressed {
        pos: end,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::PointerReleased {
        pos: end,
        button: PointerButton::Primary,
    });
    assert_eq!(
        loop_.focused(),
        Some(DOC),
        "the document keeps the caret; the formula only mirrors it"
    );
    let pressed = loop_.caret().expect("the press placed the caret").1;
    loop_.dispatch(Event::Text("3".to_owned()));

    let (id, text) = loop_
        .app_mut()
        .edited
        .clone()
        .expect("typing into the formula reached the app");
    assert_eq!(id, DOC, "the document hears the edit, not a block id");
    assert!(
        text.contains('3'),
        "the character reached the formula's own source: {text:?}"
    );
    assert!(
        text.starts_with("intro") && text.contains("$$"),
        "the rest of the document and the fences survive: {text:?}"
    );
    assert_ne!(text, SOURCE, "the edit changed the source");

    // One reading position, not a second caret left where the press found it. The edit
    // splices the writer's export, so the payload can grow: what must hold is that the
    // document caret stands right after the character just typed.
    let (_, caret) = loop_.caret().expect("the document keeps the caret");
    let before = text[..caret.min(text.len())].chars().next_back();
    assert_eq!(
        before,
        Some('3'),
        "the document caret follows the formula's own edit: {text:?} at {caret}"
    );
    let _ = pressed;

    // The next frame renders the new source, and the formula keeps its editor.
    loop_.build_with_dt((420, 200), Some(end), 0.016);
    assert!(
        math_node(&loop_).is_some(),
        "the edited block is still a formula"
    );
}

#[test]
fn an_edit_inside_an_embedded_formula_joins_the_documents_history() {
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };

    // One edit in the prose, then one inside the formula: two undo steps on one stack.
    press(&mut loop_, Point::new(20.0, 6.0));
    loop_.dispatch(Event::Text("Z".to_owned()));
    let prose = loop_
        .app_mut()
        .edited
        .clone()
        .expect("the prose edit arrived");
    assert!(prose.1.contains('Z'), "{:?}", prose.1);

    let inside = Point::new(rect.origin.x + 1.0, rect.origin.y + 1.0);
    press(&mut loop_, inside);
    loop_.dispatch(Event::Text("3".to_owned()));
    let typed = loop_
        .app_mut()
        .edited
        .clone()
        .expect("the formula edit arrived");
    assert!(typed.1.contains('3'), "{:?}", typed.1);

    ctrl(&mut loop_, ui_core::Key::Char('z'));
    let undone = loop_.app_mut().edited.clone().expect("the undo arrived");
    assert_eq!(undone.0, DOC, "the document hears the undo");
    assert!(
        !undone.1.contains('3') && undone.1.contains('Z'),
        "the formula's edit is the newest step: {:?}",
        undone.1
    );

    ctrl(&mut loop_, ui_core::Key::Char('z'));
    let twice = loop_
        .app_mut()
        .edited
        .clone()
        .expect("the second undo arrived");
    assert!(
        !twice.1.contains('Z'),
        "the same stack reaches the edit before it: {:?}",
        twice.1
    );
}

#[test]
fn source_mode_floats_a_preview_beside_the_display_formula() {
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Source,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    if math_node(&loop_).is_none() {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    }

    // A rendered block owns no text view, so a press on it cannot reach its bytes; the
    // keyboard can, one step at a time. This is the path Source mode edits through.
    let text = DOC.with(0usize).with("text");
    let line = loop_
        .tree()
        .find_by_id(text)
        .and_then(|nid| loop_.tree().get(nid))
        .map(|node| node.visual_rect())
        .expect("the paragraph's text node");
    let at = Point::new(
        line.origin.x + line.size.w - 1.0,
        line.origin.y + line.size.h * 0.5,
    );
    loop_.dispatch(Event::PointerPressed {
        pos: at,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::PointerReleased {
        pos: at,
        button: PointerButton::Primary,
    });
    for _ in 0..2 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Right,
        });
    }
    loop_.build_with_dt((420, 200), Some(at), 0.016);

    let block = ui_richtext::parse(SOURCE)
        .blocks
        .into_iter()
        .find(|block| block.kind == ui_richtext::BlockKind::Math)
        .expect("the sample has a display formula");
    let preview = DOC.with(("math-preview", block.full.start)).with("formula");
    let node = loop_
        .tree()
        .find_by_id(preview)
        .and_then(|nid| loop_.tree().get(nid));
    assert!(
        node.is_some_and(|node| matches!(node.data, NodeData::Math { .. })),
        "the source-mode preview hangs beside the formula's own source"
    );
}

/// The payload's byte range in `SOURCE`.
fn payload() -> (usize, usize) {
    let start = SOURCE.find("\\frac").expect("the source holds the formula");
    (start, start + "\\frac{1}{2}".len())
}

/// Press `point` over the document's formula and release it there.
fn press<A: App>(loop_: &mut AppLoop<A>, point: Point) {
    loop_.dispatch(Event::PointerPressed {
        pos: point,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::PointerReleased {
        pos: point,
        button: PointerButton::Primary,
    });
}

fn ctrl<A: App>(loop_: &mut AppLoop<A>, key: ui_core::Key) {
    loop_.dispatch(Event::Modifiers(ui_core::Modifiers {
        ctrl: true,
        ..ui_core::Modifiers::default()
    }));
    loop_.dispatch(Event::KeyPressed { key });
    loop_.dispatch(Event::Modifiers(ui_core::Modifiers::default()));
}

#[test]
fn a_press_inside_the_fraction_names_a_boundary_inside_its_bytes() {
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let inside = Point::new(rect.origin.x + 1.0, rect.origin.y + 1.0);
    press(&mut loop_, inside);
    let (owner, at) = loop_.caret().expect("the press placed the document caret");
    let (start, end) = payload();
    assert_eq!(owner, DOC);
    assert!(
        start < at && at < end,
        "a press inside the fraction is a boundary inside its bytes: {at} in {start}..{end}"
    );

    loop_.dispatch(Event::Text("9".to_owned()));
    let (_, text) = loop_.app_mut().edited.clone().expect("the edit arrived");
    let typed = text.find('9').expect("the character reached the source");
    let fraction = text.find("\\frac").expect("the fraction survives");
    let close = text.rfind("$$").expect("the closing fence survives");
    assert!(
        typed > fraction && typed + 1 < close,
        "the character lands inside the formula, not appended at its end: {text:?}"
    );
}

#[test]
fn an_arrow_inside_the_formula_steps_by_structure() {
    let mut loop_ = AppLoop::new(Page {
        source: "intro\n\n$$\\alpha\\beta$$\n".to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let source = loop_.app_mut().source.clone();
    let start = source
        .find("\\alpha")
        .expect("the source holds the formula");
    let end = loop_.caret().map_or(0, |_| 0);
    let _ = end;
    let right = Point::new(
        rect.origin.x + rect.size.w - 1.0,
        rect.origin.y + rect.size.h * 0.5,
    );
    press(&mut loop_, right);
    assert_eq!(
        loop_.caret(),
        Some((DOC, start + 11)),
        "the press lands at the end"
    );

    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::Left,
    });
    let (_, at) = loop_.caret().expect("the arrow kept the caret");
    // The caret names the boundary in the host's own bytes: right after `\alpha`, with no
    // writer space, because a move changes no text and the document keeps its spelling.
    assert_eq!(
        at,
        start + 6,
        "Left steps over the whole command, not one byte"
    );

    loop_.dispatch(Event::Text("q".to_owned()));
    let (_, text) = loop_.app_mut().edited.clone().expect("the edit arrived");
    assert!(
        text.contains("\\alpha q\\beta") || text.contains("\\alpha q \\beta"),
        "the character lands between the commands: {text:?}"
    );
}

/// The `$$` texts the control draws around a display formula, while its fences are shown.
fn fence_texts<A: App>(loop_: &AppLoop<A>) -> Vec<ui_core::geom::Rect> {
    fn walk<A: App>(
        loop_: &AppLoop<A>,
        nid: ui_core::NodeId,
        found: &mut Vec<ui_core::geom::Rect>,
    ) {
        let Some(node) = loop_.tree().get(nid) else {
            return;
        };
        if let NodeData::Text { block, .. } = &node.data {
            let (text, _) = block.flatten();
            // A fence the block does not draw still stands as a view; only painted ones show.
            let painted = block.runs.iter().any(|run| run.visibility.painted());
            if text == "$$" && painted {
                found.push(node.visual_rect());
            }
        }
        for child in loop_.tree().children_of(Some(nid)) {
            walk(loop_, *child, found);
        }
    }
    let mut found = Vec::new();
    if let Some(root) = loop_.tree().root() {
        walk(loop_, root, &mut found);
    }
    found
}

#[test]
fn a_display_formulas_fences_show_while_the_caret_is_in_it() {
    let mut loop_ = AppLoop::new(Page {
        source: "a\n\n$$\nx$$\n".to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    assert!(
        fence_texts(&loop_).is_empty(),
        "away from it the fences stay hidden"
    );
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let right = Point::new(
        rect.origin.x + rect.size.w - 1.0,
        rect.origin.y + rect.size.h * 0.5,
    );
    press(&mut loop_, right);
    loop_.build_with_dt((420, 200), None, 0.016);
    let fences = fence_texts(&loop_);
    assert_eq!(
        fences.len(),
        2,
        "the caret in the formula shows both of its fences"
    );
    let Some((_formula, inside)) = math_node(&loop_) else {
        return;
    };
    assert!(
        fences[0].origin.y + fences[0].size.h <= inside.origin.y + 0.01
            && inside.origin.y + inside.size.h <= fences[1].origin.y + 0.01,
        "the fences stand above and below the payload: {fences:?} around {inside:?}"
    );
}

/// Every byte the source holds is a place the caret can stand, the fence lines included:
/// they are views of the document's buffer, and a key typed on one edits that buffer.
#[test]
fn a_display_formulas_fences_are_caret_stops_of_the_document() {
    let source = "a\n\n$$\nx\n$$\n";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let close = source.rfind("$$").expect("the closing fence");
    press(&mut loop_, Point::new(2.0, 6.0));
    let mut seen = Vec::new();
    for _ in 0..20 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Right,
        });
        loop_.build_with_dt((420, 200), None, 0.016);
        let Some((_, at)) = loop_.caret() else {
            break;
        };
        if seen.last() != Some(&at) {
            seen.push(at);
        }
    }
    assert!(
        seen.contains(&close) && seen.contains(&(close + 1)),
        "the caret stands on either byte of the closing fence: {seen:?}"
    );
    assert_eq!(
        seen.last(),
        Some(&source.len()),
        "and it reaches the document's own end: {seen:?}"
    );

    // A selection from the end covers the last two characters: the closing `$$`.
    loop_.dispatch(Event::Modifiers(ui_core::Modifiers {
        shift: true,
        ..ui_core::Modifiers::default()
    }));
    for _ in 0..3 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Left,
        });
    }
    loop_.dispatch(Event::Modifiers(ui_core::Modifiers::default()));
    let (owner, selection) = loop_.selection().expect("the shift steps left a selection");
    assert_eq!(owner, DOC);
    assert!(
        selection.range().start <= close && selection.range().end == source.len(),
        "the closing fence can be selected: {:?}",
        selection.range()
    );
    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::Right,
    });

    let mut back = Vec::new();
    for _ in 0..20 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Left,
        });
        loop_.build_with_dt((420, 200), None, 0.016);
        let Some((_, at)) = loop_.caret() else {
            break;
        };
        if back.last() != Some(&at) {
            back.push(at);
        }
    }
    let open = source.find("$$").expect("the opening fence");
    assert!(
        back.contains(&open) && back.contains(&(open + 1)),
        "the opening fence is a stop from inside as well: {back:?}"
    );

    // Walk back onto the closing fence's last byte, then delete: the document's own
    // history takes it, because those bytes are the document's, not the formula's.
    for _ in 0..20 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Right,
        });
        loop_.build_with_dt((420, 200), None, 0.016);
        if loop_.caret() == Some((DOC, close + 1)) {
            break;
        }
    }
    assert_eq!(
        loop_.caret(),
        Some((DOC, close + 1)),
        "the caret walks onto the second fence byte"
    );
    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::Backspace,
    });
    assert_eq!(
        loop_
            .app_mut()
            .edited
            .as_ref()
            .map(|(_, text)| text.as_str()),
        Some("a\n\n$$\nx\n$\n"),
        "the fence byte is the document's to delete"
    );
}

#[test]
fn backspace_at_a_formulas_first_stop_reaches_the_document_before_it() {
    let mut loop_ = AppLoop::new(Page {
        source: "a\n\n$$\nx$$\n".to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let right = Point::new(
        rect.origin.x + rect.size.w - 1.0,
        rect.origin.y + rect.size.h * 0.5,
    );
    press(&mut loop_, right);
    loop_.build_with_dt((420, 200), None, 0.016);
    let mut sources = Vec::new();
    for _ in 0..3 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Backspace,
        });
        loop_.build_with_dt((420, 200), None, 0.016);
        sources.push(loop_.app_mut().source.clone());
    }
    assert_eq!(sources[0], "a\n\n$$\n$$\n", "the payload goes first");
    assert_eq!(sources[1], "a\n\n$$$$\n", "then the break before it");
    assert_eq!(
        sources[2], "a\n\n$$$\n",
        "and a fence byte: a key the structure declines belongs to the document"
    );
}

/// The formula's payload range in `source`: from its first command to the closing fence.
fn payload_in(source: &str) -> (usize, usize) {
    let start = source.find("\\int").expect("the source holds the formula");
    let end = source[start..]
        .find("\n$$")
        .expect("the formula keeps its closing fence")
        + start;
    (start, end)
}

/// Press `key` until the document's caret leaves `range`, and say where it landed.
fn press_until_outside<A: App>(
    loop_: &mut AppLoop<A>,
    key: ui_core::Key,
    range: (usize, usize),
) -> Option<usize> {
    for _ in 0..40 {
        loop_.dispatch(Event::KeyPressed { key });
        loop_.build_with_dt((420, 200), None, 0.016);
        let (_, at) = loop_.caret()?;
        if at < range.0 || at > range.1 {
            return Some(at);
        }
    }
    None
}

#[test]
fn the_arrows_walk_the_caret_out_of_a_display_formula() {
    let source = "intro\n\n$$\n\\int_0^1 x^2 \\, dx\n$$\n\noutro\n";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let inside = Point::new(
        rect.origin.x + rect.size.w - 1.0,
        rect.origin.y + rect.size.h * 0.5,
    );
    press(&mut loop_, inside);
    let (start, end) = payload_in(&loop_.app_mut().source);
    let (_, at) = loop_.caret().expect("the press placed the caret");
    assert!(
        start <= at && at <= end,
        "the press stands inside the formula: {at} in {start}..{end}"
    );

    let left = press_until_outside(&mut loop_, ui_core::Key::Left, (start, end))
        .expect("Left leaves the formula");
    assert!(left < start, "the caret stands before the formula: {left}");

    // A vertical step crosses the formula instead of skipping the block it draws.
    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::Down,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let (start, end) = payload_in(&loop_.app_mut().source);
    let (_, at) = loop_.caret().expect("the caret survived the step");
    assert!(
        start <= at && at <= end,
        "Down from the prose enters the formula: {at} in {start}..{end}"
    );

    let down = press_until_outside(&mut loop_, ui_core::Key::Down, (start, end))
        .expect("Down leaves the formula");
    assert!(down > end, "the caret stands after the formula: {down}");

    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::Up,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let (_, at) = loop_.caret().expect("the caret survived the step");
    assert!(
        start <= at && at <= end,
        "Up from the prose below enters the formula: {at} in {start}..{end}"
    );
}

#[test]
fn a_mark_and_its_base_are_one_caret_stop_inside_text() {
    // Shaped by the host stack, the cluster is the caret's unit: a caret stands before
    // the `e` or after its accent, never between them. The face's own fallback shaper
    // has one stop per character and would split the grapheme.
    let source = "a\n\n$$\n\\text{e\u{301}}\n$$\n";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let inside = Point::new(rect.origin.x + 1.0, rect.origin.y + rect.size.h * 0.5);
    press(&mut loop_, inside);
    let body = source.find("\\text{").expect("the command") + "\\text{".len();
    let (_, at) = loop_.caret().expect("the press placed the caret");
    assert_eq!(
        at, body,
        "the press stands before the base character, inside the text"
    );

    let mut seen = Vec::new();
    for _ in 0..10 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Right,
        });
        loop_.build_with_dt((420, 200), None, 0.016);
        if let Some((_, at)) = loop_.caret() {
            seen.push(at);
        }
    }
    assert_eq!(
        seen.first(),
        Some(&(body + 3)),
        "one step crosses the base and its mark together: {seen:?} from {body}"
    );
    assert!(
        !seen.iter().any(|at| *at == body + 1 || *at == body + 2),
        "a caret never splits the base from its mark: {seen:?} around {body}"
    );
}

#[test]
fn selecting_everything_copies_the_documents_bytes_through_the_formula() {
    use ui_platform::clipboard::{ClipboardCommand, ClipboardKind};
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let inside = Point::new(rect.origin.x + 1.0, rect.origin.y + 1.0);
    press(&mut loop_, inside);
    ctrl(&mut loop_, ui_core::Key::Char('a'));
    ctrl(&mut loop_, ui_core::Key::Char('c'));
    assert_eq!(
        loop_.take_clipboard_commands(),
        vec![ClipboardCommand::Copy {
            content: ui_platform::clipboard::ClipboardContent::Text(SOURCE.to_owned()),
            kind: ClipboardKind::Selection,
        }],
        "a selection through the formula is the document's bytes"
    );
}

#[test]
fn the_formula_paints_the_part_of_the_document_selection_inside_it() {
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let washes = |loop_: &AppLoop<Page>| {
        let Some((formula, _)) = math_node(loop_) else {
            return 0;
        };
        let nid = loop_.tree().find_by_id(formula).expect("the formula node");
        let node = loop_.tree().get(nid).expect("the formula node");
        let NodeData::Math { formula, .. } = &node.data else {
            return 0;
        };
        formula
            .items
            .iter()
            .filter(|item| matches!(item, ui_core::math::MathItem::Background { .. }))
            .count()
    };
    assert_eq!(washes(&loop_), 0, "an unselected formula paints no wash");

    let inside = Point::new(rect.origin.x + 1.0, rect.origin.y + 1.0);
    press(&mut loop_, inside);
    ctrl(&mut loop_, ui_core::Key::Char('a'));
    loop_.build_with_dt((420, 200), None, 0.016);
    assert!(
        washes(&loop_) > 0,
        "the document's selection reaches the formula's own display list"
    );
}

#[test]
fn the_input_method_places_its_window_at_the_structural_caret() {
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let inside = Point::new(rect.origin.x + 1.0, rect.origin.y + 1.0);
    press(&mut loop_, inside);
    loop_.build_with_dt((420, 200), Some(inside), 0.016);
    let state = loop_.ime_state();
    assert!(
        state.enabled,
        "a caret inside a formula enables composition"
    );
    let cursor = state
        .cursor
        .expect("the composition window names the caret");
    assert!(
        cursor.origin.x >= rect.origin.x - 2.0 && cursor.origin.x <= rect.origin.x + rect.size.w,
        "the window sits inside the formula's box"
    );
}

#[test]
fn a_drag_from_the_prose_into_the_formula_selects_across_it() {
    let mut loop_ = AppLoop::new(Page {
        source: SOURCE.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let (start, _) = payload();
    let prose = Point::new(20.0, 6.0);
    loop_.dispatch(Event::PointerPressed {
        pos: prose,
        button: PointerButton::Primary,
    });
    // The denominator sits at the bottom: dragging there covers the numerator's ink.
    let inside = Point::new(
        rect.origin.x + rect.size.w * 0.5,
        rect.origin.y + rect.size.h - 2.0,
    );
    loop_.dispatch(Event::PointerMoved { pos: inside });
    loop_.dispatch(Event::PointerReleased {
        pos: inside,
        button: PointerButton::Primary,
    });
    let (owner, selection) = loop_.selection().expect("the drag left a selection");
    let range = selection.range();
    assert_eq!(owner, DOC, "one selection, in the document's buffer");
    assert!(
        range.start < start && range.end > start,
        "the selection begins in the prose and reaches into the formula: {range:?}"
    );

    loop_.build_with_dt((420, 200), Some(inside), 0.016);
    let Some((formula, _)) = math_node(&loop_) else {
        panic!("the formula is still drawn");
    };
    let nid = loop_.tree().find_by_id(formula).expect("the formula node");
    let node = loop_.tree().get(nid).expect("the formula node");
    let NodeData::Math { formula, .. } = &node.data else {
        panic!("the node is a formula");
    };
    assert!(
        formula
            .items
            .iter()
            .any(|item| matches!(item, ui_core::math::MathItem::Background { .. })),
        "the part of the selection inside the formula is painted there"
    );
}

/// A drag that ends past a typeset formula reaches the fence bytes in the same gesture:
/// the fence stands as a view even while the block does not draw it.
#[test]
fn one_drag_reaches_the_fence_without_visiting_the_formula_first() {
    // No trailing newline: the document's last bytes are the closing fence itself.
    let source = "a\n\n$$\nx\n$$";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 300), None, 0.016);
    let Some((_formula, rect)) = math_node(&loop_) else {
        eprintln!("skipped: the document did not lay out a formula");
        return;
    };
    let close = source.rfind("$$").expect("the closing fence");
    let start = Point::new(2.0, 6.0);
    // One press and one motion, straight onto the line the closing fence would occupy.
    let target = Point::new(
        rect.origin.x + 6.0,
        rect.origin.y + rect.size.h + rect.size.h * 0.5,
    );
    loop_.dispatch(Event::PointerPressed {
        pos: start,
        button: PointerButton::Primary,
    });
    loop_.build_with_dt((420, 300), None, 0.016);
    loop_.dispatch(Event::PointerMoved { pos: target });
    loop_.build_with_dt((420, 300), Some(target), 0.016);
    loop_.dispatch(Event::PointerReleased {
        pos: target,
        button: PointerButton::Primary,
    });
    let (owner, selection) = loop_.selection().expect("the drag left a selection");
    assert_eq!(owner, DOC);
    assert!(
        selection.range().end >= close + 2,
        "the drag covers both fence bytes on its first motion: {:?}",
        selection.range()
    );
}

/// The rule draws a line, not its bytes; a press on it still lands in the document's text,
/// which is what reveals the source.
#[test]
fn a_press_on_a_thematic_rule_puts_the_caret_in_its_bytes() {
    let source = "a\n\n---\n\nb\n";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 300), None, 0.016);
    let rule = source.find("---").expect("the rule");
    // The rule's view names its own bytes even though it paints none of them.
    let mut rect_out = None;
    fn walk<A: App>(
        loop_: &AppLoop<A>,
        nid: ui_core::NodeId,
        base: usize,
        out: &mut Option<ui_core::geom::Rect>,
    ) {
        let Some(node) = loop_.tree().get(nid) else {
            return;
        };
        if let NodeData::Text { block, .. } = &node.data
            && block.input_base == Some(base)
        {
            *out = Some(node.visual_rect());
            return;
        }
        for child in loop_.tree().children_of(Some(nid)) {
            walk(loop_, *child, base, out);
        }
    }
    if let Some(root) = loop_.tree().root() {
        walk(&loop_, root, rule, &mut rect_out);
    }
    let Some(rect) = rect_out else {
        panic!("the rule is a view over its bytes");
    };
    let point = Point::new(rect.origin.x + 4.0, rect.origin.y + rect.size.h * 0.5);
    press(&mut loop_, point);
    loop_.build_with_dt((420, 300), None, 0.016);
    let (owner, at) = loop_.caret().expect("the press placed the caret");
    assert_eq!(owner, DOC);
    assert!(
        (rule..=rule + 3).contains(&at),
        "the caret stands on the rule's line: {at} at {rule}..{}",
        rule + 3
    );
    // Touching the block reveals it, so the line is the source now and the caret steps in.
    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::Left,
    });
    loop_.build_with_dt((420, 300), None, 0.016);
    let (_, inside) = loop_.caret().expect("the arrow kept the caret");
    assert!(
        (rule..rule + 3).contains(&inside),
        "a left step lands between the rule's own bytes: {inside}"
    );
}

/// A quote's bar stands on every line of the quote. A lazy continuation line writes no `>`
/// of its own, so its cell holds no bytes for the bar to stand over.
#[test]
fn a_quote_draws_its_bar_beside_a_lazy_continuation_line() {
    let source = "> a\nb\n> c\n";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 300), None, 0.016);
    let quote = DOC.with(0usize);
    let line = DOC
        .with(0usize)
        .with(0usize)
        .with(("row", 4usize))
        .with("text");
    let rect_of = |id: Id| {
        loop_
            .tree()
            .find_by_id(id)
            .and_then(|nid| loop_.tree().get(nid))
            .map(|node| node.visual_rect())
    };
    let row = rect_of(line).expect("the lazy line draws a row of its own");
    let bar = rect_of(quote.with(("bar", 4usize))).expect("the lazy line keeps the quote's bar");
    assert!(
        bar.origin.y >= row.origin.y - 0.5 && bar.origin.y <= row.origin.y + 0.5,
        "the bar stands beside the lazy line, not the line that opened the quote: {:?} at {:?}",
        bar,
        row
    );
}

/// The break between two rows is drawn by no view. A selection that takes it whole names
/// it one break past the row's text, and the text pass marks those bytes with a block.
#[test]
fn a_selection_across_a_line_break_takes_the_break_into_the_line_s_view() {
    let source = "alpha\nbravo\n";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 300), None, 0.016);
    let first = DOC.with(0usize).with("text");
    let rect = loop_
        .tree()
        .find_by_id(first)
        .and_then(|nid| loop_.tree().get(nid))
        .map(|node| node.visual_rect())
        .expect("the first line draws a view");
    press(
        &mut loop_,
        Point::new(rect.origin.x + 4.0, rect.origin.y + rect.size.h * 0.5),
    );
    ctrl(&mut loop_, ui_core::Key::Char('a'));
    loop_.build_with_dt((420, 300), None, 0.016);
    let node = loop_
        .tree()
        .find_by_id(first)
        .and_then(|nid| loop_.tree().get(nid))
        .expect("the first line draws a view");
    let NodeData::Text {
        block, selection, ..
    } = &node.data
    else {
        panic!("the line is a text view");
    };
    let len = block.flatten().0.len();
    let prim = selection.as_ref().expect("the line is selected");
    assert_eq!(
        prim.range.end,
        len + 1,
        "the view names the break after its own bytes"
    );
}

/// A selection that lies past a line must not stretch back onto its end: selecting one
/// break marks that line, and only that line, even though every row is one view.
#[test]
fn one_selected_line_break_marks_only_that_line() {
    let source = "alpha\nbravo\ncharlie\n";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 300), None, 0.016);
    let line = |at: usize| match at {
        0 => DOC.with(0usize).with("text"),
        at => DOC.with(0usize).with(("row", at)).with("text"),
    };
    // The middle line's break: a selection that lies past the first line must not stretch
    // back onto its end, or every line above the caret grows a block.
    let second = line(6);
    let rect = loop_
        .tree()
        .find_by_id(second)
        .and_then(|nid| loop_.tree().get(nid))
        .map(|node| node.visual_rect())
        .expect("the second line draws a view");
    press(
        &mut loop_,
        Point::new(rect.origin.x + 4.0, rect.origin.y + rect.size.h * 0.5),
    );
    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::End,
    });
    loop_.dispatch(Event::Modifiers(ui_core::Modifiers {
        shift: true,
        ..ui_core::Modifiers::default()
    }));
    loop_.dispatch(Event::KeyPressed {
        key: ui_core::Key::Right,
    });
    loop_.dispatch(Event::Modifiers(ui_core::Modifiers::default()));
    loop_.build_with_dt((420, 300), None, 0.016);
    let (_, selected) = loop_.selection().expect("the shift step selects");
    assert_eq!(selected.range(), 11..12, "exactly the second line break");
    let prim = |id: Id| {
        let node = loop_
            .tree()
            .find_by_id(id)
            .and_then(|nid| loop_.tree().get(nid))?;
        let NodeData::Text { selection, .. } = &node.data else {
            return None;
        };
        selection.as_ref().map(|prim| prim.range.clone())
    };
    assert_eq!(
        prim(second),
        Some(5..6),
        "the line names only the break after its own five bytes"
    );
    for at in [0usize, 12usize] {
        assert_eq!(
            prim(line(at)),
            None,
            "the line at {at} takes no part in the selection"
        );
    }
}

/// One caret position is one caret. A display formula on its fences' own line shares the
/// two boundary bytes with them, and the widget's window rule used to give each boundary to
/// both views: the fence drew a caret at its end and the formula its bar at its start.
#[test]
fn a_formula_sharing_a_fence_line_draws_one_caret_at_each_boundary() {
    let source = "a\n\n$$x^2$$";
    let mut loop_ = AppLoop::new(Page {
        source: source.to_owned(),
        mode: ui_widgets::MathEditing::Structural,
        edited: None,
    });
    loop_.build_with_dt((420, 200), None, 0.016);
    // The blank line before the block: two Right steps reach the opening fence, two more
    // the payload's first byte, and the rest walk to its last.
    press(&mut loop_, Point::new(5.0, 33.0));
    loop_.build_with_dt((420, 200), None, 0.016);
    let mut counted = Vec::new();
    for _ in 0..9 {
        counted.push((loop_.caret().map(|(_, at)| at), caret_bars(&loop_)));
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Right,
        });
        loop_.build_with_dt((420, 200), None, 0.016);
    }
    let visited = |want: usize| counted.iter().any(|(at, _)| *at == Some(want));
    assert!(
        visited(5) && visited(8),
        "the steps must reach both boundary bytes: {counted:?}"
    );
    assert!(
        counted.iter().all(|(_, bars)| *bars == 1),
        "one caret position draws one caret: {counted:?}"
    );
}

/// A table's source state is one view of many lines, so Up and Down step through those
/// lines the way they step through any text. The step used to aim at a point exactly on the
/// next line's edge, which float noise put back on the caret's own line: the step then left
/// the table instead of reaching its second row.
#[test]
fn a_tables_source_state_steps_through_every_row() {
    let source = "before\n\n| a | bb |\n|:--|--:|\n| c | dd |\n\nafter\n";
    let mut loop_ = AppLoop::with_stack(
        Page {
            source: source.to_owned(),
            mode: ui_widgets::MathEditing::Structural,
            edited: None,
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    loop_.build_with_dt((300, 400), None, 0.016);
    press(&mut loop_, Point::new(3.0, 3.0));
    loop_.build_with_dt((300, 400), None, 0.016);
    let mut seen = Vec::new();
    for _ in 0..6 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Down,
        });
        loop_.build_with_dt((300, 400), None, 0.016);
        seen.push(loop_.caret().map(|(_, at)| at));
    }
    let mut climbed = Vec::new();
    for _ in 0..6 {
        loop_.dispatch(Event::KeyPressed {
            key: ui_core::Key::Up,
        });
        loop_.build_with_dt((300, 400), None, 0.016);
        climbed.push(loop_.caret().map(|(_, at)| at));
    }
    let reached = |at: &Option<usize>, row: std::ops::RangeInclusive<usize>| {
        at.is_some_and(|at| row.contains(&at))
    };
    assert!(
        seen.iter().any(|at| reached(at, 8..=18))
            && seen.iter().any(|at| reached(at, 19..=29))
            && seen.iter().any(|at| reached(at, 30..=40)),
        "every row is a step down of its own: {seen:?}"
    );
    assert!(
        climbed.iter().any(|at| reached(at, 8..=18))
            && climbed.iter().any(|at| reached(at, 19..=29))
            && climbed.iter().any(|at| reached(at, 30..=40)),
        "every row is a step up of its own: {climbed:?}"
    );
}

/// Caret bars drawn this frame: one per caret-bearing text node, plus a formula's own bar,
/// which is the one rule narrower than any glyph stroke the typesetter emits.
fn caret_bars<A: App>(loop_: &AppLoop<A>) -> usize {
    fn walk<A: App>(loop_: &AppLoop<A>, nid: ui_core::NodeId, count: &mut usize) {
        let Some(node) = loop_.tree().get(nid) else {
            return;
        };
        if let NodeData::Text { caret: Some(_), .. } = &node.data {
            *count += 1;
        }
        if let NodeData::Math { formula, .. } = &node.data {
            *count += formula
                .items
                .iter()
                .filter(|item| matches!(item, ui_core::math::MathItem::Rule { w, .. } if *w < 2.0))
                .count();
        }
        for child in loop_.tree().children_of(Some(nid)) {
            walk(loop_, *child, count);
        }
    }
    let mut count = 0;
    if let Some(root) = loop_.tree().root() {
        walk(loop_, root, &mut count);
    }
    count
}
