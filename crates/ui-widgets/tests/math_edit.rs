//! A press into an editable formula takes the keyboard, and the app hears the new source.
//!
//! The engine owns the editor and the caret; the app owns the source. Wiring either half
//! without the other leaves a formula that looks editable and edits nothing, which is what
//! this drives end to end without a window.

use ui_core::change::Change;
use ui_core::geom::{Color, Point};
use ui_core::layout::SizeClass;
use ui_core::{Decl, Event, Id, NodeData, NodeKind, PointerButton};
use ui_platform::{App, AppLoop, Element};

const FORMULA: Id = Id::str("formula");
const SIZE: f32 = 20.0;

struct Host {
    source: String,
    size: f32,
}

impl App for Host {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let widgets = ui_widgets::Cx::new(cx, ui_style::Env::root());
        let formula = ui_widgets::Math::new(FORMULA, self.source.clone(), self.size, Color::WHITE)
            .editable()
            .resolve_in(&widgets);
        let root = Decl::container(Id::str("root"), NodeKind::Column)
            .size(SizeClass::fixed(300.0, 120.0))
            .child(formula);
        ui_platform::from_decl(root)
    }

    fn changed(&mut self, id: Id, change: Change) -> bool {
        if id == FORMULA
            && let Change::Text(text) = change
        {
            self.source = text;
            return true;
        }
        false
    }
}

/// The laid-out formula's width, or `None` when the host has no math face to paint one.
fn formula_width<A: App>(loop_: &AppLoop<A>) -> Option<f32> {
    let nid = loop_.tree().find_by_id(FORMULA)?;
    match &loop_.tree().get(nid)?.data {
        NodeData::Math { formula, .. } => Some(formula.width),
        _ => None,
    }
}

#[test]
fn a_press_takes_the_keyboard_and_a_character_reaches_the_app() {
    let mut loop_ = AppLoop::new(Host {
        source: "x".to_owned(),
        size: SIZE,
    });
    loop_.build_with_dt((300, 120), None, 0.016);
    let Some(width) = formula_width(&loop_) else {
        eprintln!("skipped: no installed math face");
        return;
    };

    // The left edge sits before every glyph, so the press lands the caret at the start:
    // the editor's own caret would otherwise stay at the end, where the seed left it.
    let nid = loop_
        .tree()
        .find_by_id(FORMULA)
        .expect("the formula node is in the tree");
    let rect = loop_
        .tree()
        .get(nid)
        .expect("the node exists")
        .visual_rect();
    let start = Point::new(rect.origin.x + 0.25, rect.origin.y + rect.size.h * 0.5);
    loop_.dispatch(Event::PointerPressed {
        pos: start,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::PointerReleased {
        pos: start,
        button: PointerButton::Primary,
    });

    loop_.dispatch(Event::Text("+".to_owned()));
    let source = loop_.app_mut().source.clone();
    assert_ne!(
        source, "x",
        "the typed character must reach the app's source"
    );
    let typed = source
        .find('+')
        .unwrap_or_else(|| panic!("the new atom is in {source}"));
    let before = source.find('x').expect("the seed atom stays");
    assert!(
        typed < before,
        "the press put the caret before the seed, so the atom leads it: {source}"
    );

    loop_.build_with_dt((300, 120), Some(start), 0.016);
    let grown = formula_width(&loop_).expect("the formula still lays out");
    assert!(
        grown > width,
        "the painted formula follows the edit: {width} then {grown}"
    );

    // A press on ground the formula does not cover drops focus, so typing goes nowhere.
    let ground = Point::new(250.0, 100.0);
    loop_.dispatch(Event::PointerPressed {
        pos: ground,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::PointerReleased {
        pos: ground,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::Text("y".to_owned()));
    assert_eq!(
        loop_.app_mut().source,
        source,
        "an unfocused formula must not take characters"
    );
}

/// The laid-out formula, or `None` when the host has no math face to paint one.
fn formula_node<A: App>(loop_: &AppLoop<A>) -> Option<ui_core::math::Formula> {
    let nid = loop_.tree().find_by_id(FORMULA)?;
    match &loop_.tree().get(nid)?.data {
        NodeData::Math { formula, .. } => Some((**formula).clone()),
        _ => None,
    }
}

#[test]
fn a_text_container_measures_in_the_documents_own_text_stack() {
    // The math face carries no Han glyphs. Measured by the face's own fallback shaper
    // every character was a missing glyph: the formula laid out at zero width and
    // reported one diagnostic per character instead of drawing the text.
    let mut loop_ = AppLoop::new(Host {
        source: "\\text{中文}".to_owned(),
        size: SIZE,
    });
    loop_.build_with_dt((300, 120), None, 0.016);
    let Some(formula) = formula_node(&loop_) else {
        eprintln!("skipped: no installed math face");
        return;
    };
    assert!(
        formula.errors.is_empty(),
        "text the face lacks is not a refusal: {:?}",
        formula.errors
    );
    assert!(
        formula.width > 0.0,
        "the run has the width the covering face measured: {}",
        formula.width
    );

    let nid = loop_
        .tree()
        .find_by_id(FORMULA)
        .expect("the formula node is in the tree");
    let rect = loop_
        .tree()
        .get(nid)
        .expect("the node exists")
        .visual_rect();
    let first = Point::new(rect.origin.x + 1.0, rect.origin.y + rect.size.h * 0.5);
    loop_.dispatch(Event::PointerPressed {
        pos: first,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::PointerReleased {
        pos: first,
        button: PointerButton::Primary,
    });
    loop_.dispatch(Event::Text("字".to_owned()));
    assert_eq!(
        loop_.app_mut().source,
        "\\text{字中文}",
        "the press enters the container and the text takes the character"
    );
}

#[test]
fn a_size_change_reaches_the_edited_formula() {
    let mut loop_ = AppLoop::new(Host {
        source: "x".to_owned(),
        size: SIZE,
    });
    loop_.build_with_dt((300, 120), None, 0.016);
    let Some(small) = formula_width(&loop_) else {
        eprintln!("skipped: no installed math face");
        return;
    };

    loop_.app_mut().size = SIZE * 2.0;
    loop_.build_with_dt((300, 120), None, 0.016);
    let large = formula_width(&loop_).expect("the formula still lays out");
    assert!(
        large > small * 1.5,
        "the editor lays out at the control's new size: {small} then {large}"
    );
}

/// A document with one formula embedded as a child of its text input.
struct Embedded {
    document: String,
    edited: Option<(Id, String)>,
    changed_on_formula: bool,
}

const DOC: Id = Id::str("document");
const DOC_SOURCE: &str = r"before $\frac{1}{2}$ after";
const PAYLOAD: &str = r"\frac{1}{2}";

impl App for Embedded {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let widgets = ui_widgets::Cx::new(cx, ui_style::Env::root());
        let start = DOC_SOURCE
            .find(PAYLOAD)
            .expect("the payload is in the document");
        let formula = ui_widgets::Math::new(FORMULA, PAYLOAD, SIZE, Color::WHITE)
            .editable()
            .in_text(DOC, start..start + PAYLOAD.len())
            .resolve_in(&widgets);
        let root = Decl::container(DOC, NodeKind::Column)
            .size(SizeClass::fixed(400.0, 120.0))
            .sense(ui_core::Sense::FOCUSABLE)
            .text_input(self.document.clone())
            .child(formula);
        ui_platform::from_decl(root)
    }

    fn changed(&mut self, id: Id, change: Change) -> bool {
        let Change::Text(text) = change else {
            return false;
        };
        if id == FORMULA {
            self.changed_on_formula = true;
        }
        self.edited = Some((id, text));
        true
    }
}

#[test]
fn a_formula_inside_a_document_rewrites_its_own_bytes() {
    let mut loop_ = AppLoop::new(Embedded {
        document: DOC_SOURCE.to_owned(),
        edited: None,
        changed_on_formula: false,
    });
    loop_.build_with_dt((400, 120), None, 0.016);
    if formula_width(&loop_).is_none() {
        eprintln!("skipped: no installed math face");
        return;
    }

    let nid = loop_
        .tree()
        .find_by_id(FORMULA)
        .expect("the formula node is in the tree");
    let rect = loop_
        .tree()
        .get(nid)
        .expect("the node exists")
        .visual_rect();
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
    loop_.dispatch(Event::Text("+".to_owned()));

    let (id, text) = loop_
        .app_mut()
        .edited
        .clone()
        .expect("the edit reached the app");
    assert_eq!(
        id, DOC,
        "the document, not the formula, hears the new bytes"
    );
    assert!(
        text.starts_with("before $") && text.ends_with("$ after"),
        "the fences and the rest of the line survive: {text}"
    );
    assert!(
        text.contains('+'),
        "the typed character is in the document: {text}"
    );
    assert!(
        !loop_.app_mut().changed_on_formula,
        "the formula's own id is never reported"
    );
}
