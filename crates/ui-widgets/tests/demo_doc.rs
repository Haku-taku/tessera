//! The demo document must render without killing the window: it is the content the user
//! reported as fatal. Both ways in — a document handed over at build time, and one pasted
//! into an empty editor — are exercised here.

use ui_core::Id;
use ui_core::change::Change;
use ui_core::geom::Color;
use ui_platform::{App, AppLoop, Element};

const DOC: Id = Id::str("demo");

struct Page {
    source: String,
    editable: bool,
}

impl App for Page {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let document = ui_richtext::parse(&self.source);
        let widgets = ui_widgets::Cx::new(cx, ui_style::Env::root());
        let markdown = ui_widgets::Markdown {
            id: DOC,
            document: std::sync::Arc::new(document),
            prepared: None,
            editable: self.editable,
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

fn demo() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/demo.md"))
        .expect("the demo document")
}

#[test]
fn the_demo_document_renders() {
    let mut loop_ = AppLoop::with_stack(
        Page {
            source: demo(),
            editable: true,
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    for _ in 0..3 {
        loop_.build_with_dt((640, 900), None, 0.016);
    }
}

#[test]
fn pasting_the_demo_document_into_an_empty_editor_renders() {
    let mut loop_ = AppLoop::with_stack(
        Page {
            source: String::new(),
            editable: true,
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    loop_.build_with_dt((640, 900), None, 0.016);
    let at = ui_core::geom::Point::new(20.0, 20.0);
    loop_.dispatch(ui_core::Event::PointerPressed {
        pos: at,
        button: ui_core::PointerButton::Primary,
    });
    loop_.dispatch(ui_core::Event::PointerReleased {
        pos: at,
        button: ui_core::PointerButton::Primary,
    });
    loop_.dispatch(ui_core::Event::Paste(
        ui_core::clipboard::ClipboardContent::Text(demo()),
    ));
    assert!(
        loop_.app_mut().source.contains("分布式"),
        "the paste reached the app, which is what makes the next frame parse it"
    );
    for _ in 0..3 {
        loop_.build_with_dt((640, 900), None, 0.016);
    }
    loop_.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Down,
    });
    loop_.build_with_dt((640, 900), None, 0.016);
}

/// A reference definition writes only syntax. A reader has nothing to show for it, and a
/// line box would open a blank gap the source never meant to show.
#[test]
fn a_reference_definition_takes_no_line_in_the_reader() {
    fn height(source: &str, editable: bool) -> f32 {
        let mut loop_ = AppLoop::with_stack(
            Page {
                source: source.to_owned(),
                editable,
            },
            &["Noto Sans", "DejaVu Sans"],
        );
        loop_.build_with_dt((640, 900), None, 0.016);
        let tree = loop_.tree();
        let Some(root) = tree.root() else {
            return 0.0;
        };
        tree.get(root).map_or(0.0, |node| node.rect.size.h)
    }

    let with = height("text\n\n[badge]: https://e.com/x.png\n", false);
    let without = height("text\n\n", false);
    assert_eq!(
        with, without,
        "the definition must not open a line in the reader"
    );
}

/// The whole document as IME surrounding text is one Wayland message; over the protocol's
/// limit libwayland drops the connection and the window disappears.
#[test]
fn the_demo_documents_ime_surrounding_text_fits_one_request() {
    let mut loop_ = AppLoop::with_stack(
        Page {
            source: demo(),
            editable: true,
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    loop_.build_with_dt((640, 900), None, 0.016);
    let at = ui_core::geom::Point::new(24.0, 24.0);
    loop_.dispatch(ui_core::Event::PointerPressed {
        pos: at,
        button: ui_core::PointerButton::Primary,
    });
    loop_.dispatch(ui_core::Event::PointerReleased {
        pos: at,
        button: ui_core::PointerButton::Primary,
    });
    loop_.build_with_dt((640, 900), Some(at), 0.016);
    let state = loop_.ime_state();
    assert!(state.enabled, "the caret enables composition");
    let (text, caret, _anchor) = state.surrounding.expect("the editor names its text");
    assert!(
        text.len() <= 4000,
        "one Wayland request carries the surrounding text: {} bytes",
        text.len()
    );
    assert!(text.is_char_boundary(caret), "the caret names a character");
}
