//! A nested construct stands at the content column its parent gives it.
//!
//! The source spells that column as indentation, and the enclosing construct's cell already
//! reserves it. Counting the bytes a second time in pixels pushed the nested marker, the code
//! card below it and the quote bar one column right of the item's own text.

use ui_core::Id;
use ui_core::geom::Color;
use ui_platform::{App, AppLoop, Element};

const DOC: Id = Id::str("indent-columns");
const WIDTH: f32 = 560.0;

/// A top-level bullet, then an ordered item holding a nested bullet, a fence and a quote.
const SOURCE: &str = "- top\n\n1. one\n   - nested\n\n   ```bash\n   x\n   ```\n\n   > q\n";

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
            width: WIDTH,
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

fn id_at(path: &[usize]) -> Id {
    path.iter().fold(DOC, |id, index| id.with(*index))
}

#[test]
fn a_nested_construct_stands_at_its_parents_content_column() {
    let mut loop_ = AppLoop::with_stack(
        Page {
            source: SOURCE.to_owned(),
        },
        &["Noto Sans", "DejaVu Sans"],
    );
    loop_.build_with_dt(((WIDTH + 60.0) as u32, 1200), None, 0.016);
    let tree = loop_.tree();
    let width_of = |path: &[usize]| {
        let id = id_at(path).with("text");
        let node = tree
            .find_by_id(id)
            .and_then(|nid| tree.get(nid))
            .unwrap_or_else(|| panic!("row {path:?} has a text node"));
        node.rect.size.w
    };
    // The bar and the card by what they paint: a 3-wide box, and a box casting a shadow.
    let (mut bar_x, mut card_x) = (None, None);
    fn walk(
        tree: &ui_core::Tree,
        nid: ui_core::NodeId,
        bar: &mut Option<f32>,
        card: &mut Option<f32>,
    ) {
        let Some(node) = tree.get(nid) else {
            return;
        };
        if (node.rect.size.w - 3.0).abs() < 0.01 && node.rect.size.h > 0.0 && bar.is_none() {
            *bar = Some(node.rect.origin.x);
        }
        let casts = node.shapes.iter().any(|shape| {
            matches!(
                shape,
                ui_core::draw::Shape::Box {
                    shadow: Some(_),
                    ..
                }
            )
        });
        if casts && card.is_none() {
            *card = Some(node.rect.origin.x);
        }
        for child in tree.children_of(Some(nid)) {
            walk(tree, *child, bar, card);
        }
    }
    if let Some(root) = tree.root() {
        walk(tree, root, &mut bar_x, &mut card_x);
    }
    let card = card_x.expect("the code card is painted");
    let bar_x = bar_x.expect("the quote bar is painted");

    // Row widths give each construct's room: `WIDTH` less the rooms its cells take.
    let (top, item, nested) = (
        width_of(&[0, 0, 0]),
        width_of(&[1, 0, 0]),
        width_of(&[1, 0, 1, 0, 0]),
    );
    let nested_room = WIDTH - nested;
    let item_room = WIDTH - item;
    let bullet_room = WIDTH - top;
    assert!(
        (nested_room - item_room - bullet_room).abs() < 0.01,
        "the nested item takes the parent's column plus the bullet's room: \
         nested {nested_room}, parent {item_room}, bullet {bullet_room}"
    );

    // Both block-level constructs inside the item start where its text does.
    assert!(
        (card - item_room).abs() < 0.01,
        "the code card starts at the item's column: card {card}, column {item_room}"
    );
    assert!(
        (bar_x - item_room).abs() < 0.01,
        "the quote bar starts at the item's column: bar {bar_x}, column {item_room}"
    );

    // The bar stands in the quote's cell, left of the text it shadows.
    let quote_text = width_of(&[1, 0, 3, 0]);
    let text_x = WIDTH - quote_text;
    assert!(
        bar_x + 3.0 <= text_x + 0.01,
        "the bar must not reach into the quoted text: bar ends at {}, text starts at {text_x}",
        bar_x + 3.0
    );
}
