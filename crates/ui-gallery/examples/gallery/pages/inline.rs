// The inline page: atoms and vertical alignment inside a paragraph.

use crate::shell::*;
use ui_core::geom::Size;
use ui_core::text::{AtomBox, Decoration, OBJECT, TextBlock, TextRun, TextStyle, VerticalAlign};

/// tag and the block width its paragraph is measured at.
pub(crate) const INLINE_SAMPLES: [(&str, f32); 3] =
    [("baseline", 300.0), ("mixed", 300.0), ("narrow", 110.0)];

pub(crate) fn inline_text_id(tag: &'static str) -> Id {
    Id::of(("inline-text", Page::Inline.tag(), tag))
}

/// Builds a paragraph by interleaving text with atoms, tracking byte offsets.
struct Run {
    text: String,
    atoms: Vec<AtomBox>,
}

impl Run {
    fn new() -> Self {
        Self {
            text: String::new(),
            atoms: Vec::new(),
        }
    }

    fn word(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    fn atom(mut self, size: Size, align: VerticalAlign) -> Self {
        self.atoms
            .push(AtomBox::new(self.text.len(), size).align(align));
        self.text.push(OBJECT);
        self
    }

    fn build(self) -> TextBlock {
        let mut block = TextBlock::plain(self.text, TextStyle::new(14.0));
        block.atoms = self.atoms;
        block
    }
}

fn text_node(tag: &'static str, block: TextBlock, width: f32) -> Decl {
    let id = inline_text_id(tag);
    let mut decl = Decl::container(id, NodeKind::Text);
    decl.data = ui_core::NodeData::Text {
        ink: Color::from_rgba8(228, 232, 240, 255),
        block: std::sync::Arc::new(block),
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl.size_class = SizeClass {
        width: AxisSize::fixed(width),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    };
    decl
}

/// A text leaf that hugs its content, for a sample whose items align with each other
/// rather than wrapping inside one block.
fn hug_text(tag: &'static str, body: &str, size: f32) -> Decl {
    let mut decl = Decl::container(
        Id::of(("inline-row-text", Page::Inline.tag(), tag)),
        NodeKind::Text,
    );
    decl.data = ui_core::NodeData::Text {
        ink: Color::from_rgba8(228, 232, 240, 255),
        block: std::sync::Arc::new(TextBlock::plain(body, TextStyle::new(size))),
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl.size_class = SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    };
    decl
}

/// One rule of each style, each in its own ink so a check can count them.
fn deco_sample() -> Decl {
    let mut block = TextBlock::plain("", TextStyle::new(16.0));
    block.runs.clear();
    for (text, style, ink) in [
        (
            "single  ",
            Decoration::Single,
            Color::from_rgba8(255, 120, 140, 255),
        ),
        (
            "double  ",
            Decoration::Double,
            Color::from_rgba8(255, 200, 90, 255),
        ),
        (
            "dotted  ",
            Decoration::Dotted,
            Color::from_rgba8(120, 220, 140, 255),
        ),
        (
            "wavy  ",
            Decoration::Wavy,
            Color::from_rgba8(130, 190, 255, 255),
        ),
    ] {
        block.runs.push(
            TextRun::new(text, TextStyle::new(16.0))
                .inked(ink)
                .underlined_with(style),
        );
    }
    block.runs.push(
        TextRun::new("struck", TextStyle::new(16.0))
            .inked(Color::from_rgba8(200, 160, 255, 255))
            .struck_through(),
    );
    let mut decl = Decl::container(Id::of(("inline-deco", Page::Inline.tag())), NodeKind::Text);
    decl.data = ui_core::NodeData::Text {
        ink: Color::from_rgba8(228, 232, 240, 255),
        block: std::sync::Arc::new(block),
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl.size_class = SizeClass {
        width: AxisSize::fixed(COL_W - 24.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    };
    decl
}

/// Three items of different sizes on one shared baseline.
fn row_sample() -> Decl {
    Decl::container(Id::of(("inline-row", Page::Inline.tag())), NodeKind::Row)
        .size(SizeClass::hug(0.0, 0.0))
        .gap(10.0)
        .cross_baseline()
        .child(hug_text("small", "small", 12.0))
        .child(hug_text("large", "Large", 24.0))
        .child(
            Decl::container(
                Id::of(("inline-row-mark", Page::Inline.tag())),
                NodeKind::Box,
            )
            .size(SizeClass::fixed(20.0, 20.0))
            .background(Color::from_rgba8(120, 160, 255, 255), Corners::all(4.0)),
        )
}

/// A widget inside a sentence: the atom holds a child node, which the engine measures and
/// places like an inline-block.
fn holder_sample() -> Decl {
    let body = format!("Run {} now", OBJECT);
    let mut block = TextBlock::plain(body, TextStyle::new(15.0));
    let chip = Id::of(("inline-holder-chip", Page::Inline.tag()));
    block.atoms = vec![AtomBox::new(4, Size::new(46.0, 18.0)).holder(chip)];
    let mut decl = Decl::container(
        Id::of(("inline-holder", Page::Inline.tag())),
        NodeKind::Text,
    );
    decl.data = ui_core::NodeData::Text {
        ink: Color::from_rgba8(228, 232, 240, 255),
        block: std::sync::Arc::new(block),
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl.size_class = SizeClass {
        width: AxisSize::fixed(COL_W - 24.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    };
    // A widget in a line is an ordinary control: it takes focus and a press, and the
    // paragraph around it never treats that press as a caret placement.
    decl.child(
        Decl::container(chip, NodeKind::Box)
            .size(SizeClass::fixed(46.0, 18.0))
            .background(Color::from_rgba8(120, 160, 255, 255), Corners::all(9.0))
            .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
            .focus_visual(Some(ui_core::draw::FocusVisual {
                outer: Color::from_rgba8(255, 255, 255, 255),
                outer_width: 2.0,
                inner: Color::from_rgba8(0, 0, 0, 0),
                inner_width: 0.0,
                margin: ui_core::geom::Insets::all(-3.0),
                radius: 0.0,
            })),
    )
}

/// A justified paragraph: every full line spreads its leftover space across its gaps, and
/// the last line keeps its own width. The break is explicit, so the tail is short whatever
/// face the host resolves.
fn justify_sample() -> Decl {
    let mut block = TextBlock::plain(
        "Justified text spreads the room a full line has left across its gaps, so both of \
         its edges stay flush with the column.\nA short last line stays short.",
        TextStyle::new(15.0),
    );
    block.align = ui_core::TextAlign::Justify;
    let mut decl = Decl::container(
        Id::of(("inline-justify", Page::Inline.tag())),
        NodeKind::Text,
    );
    decl.data = ui_core::NodeData::Text {
        ink: Color::from_rgba8(228, 232, 240, 255),
        block: std::sync::Arc::new(block),
        caret: None,
        selection: None,
        composing: None,
        nowrap: false,
    };
    decl.size_class = SizeClass {
        width: AxisSize::fixed(COL_W - 24.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    };
    decl
}

fn sample(tag: &'static str, block: TextBlock, width: f32) -> Decl {
    let mut col = Decl::container(
        Id::of(("inline-sample", Page::Inline.tag(), tag)),
        NodeKind::Column,
    )
    .size(SizeClass::hug(0.0, 0.0))
    .gap(8.0);
    col = col.child(text_node(tag, block, width));
    col
}

pub(crate) fn inline_page(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Inline.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .gap(20.0);

    // Two icons on the text baseline: the classic inline-image row.
    let baseline = Run::new()
        .word(&tr(cx, Msg::GALLERY_INLINE_SYNC_1))
        .atom(Size::new(16.0, 16.0), VerticalAlign::Baseline)
        .word(&tr(cx, Msg::GALLERY_INLINE_SYNC_2))
        .atom(Size::new(24.0, 20.0), VerticalAlign::Baseline)
        .word(&tr(cx, Msg::GALLERY_INLINE_SYNC_3))
        .build();
    row = row.child(column_text(
        Page::Inline,
        "baseline",
        tr(cx, Msg::GALLERY_INLINE_COLUMN_BASELINE),
        from_decl(sample("baseline", baseline, 300.0)),
        COL_W,
        COL_CTRL_H,
    ));

    // One atom per alignment: each hangs from a different edge of the line box.
    let mixed = Run::new()
        .word(&tr(cx, Msg::GALLERY_INLINE_TALL_1))
        .atom(Size::new(14.0, 34.0), VerticalAlign::Top)
        .word(&tr(cx, Msg::GALLERY_INLINE_TALL_2))
        .atom(Size::new(22.0, 22.0), VerticalAlign::Middle)
        .word(&tr(cx, Msg::GALLERY_INLINE_TALL_3))
        .atom(Size::new(12.0, 18.0), VerticalAlign::Bottom)
        .word(&tr(cx, Msg::GALLERY_INLINE_TALL_4))
        .build();
    row = row.child(column_text(
        Page::Inline,
        "mixed",
        tr(cx, Msg::GALLERY_INLINE_COLUMN_MIXED),
        from_decl(sample("mixed", mixed, 300.0)),
        COL_W,
        COL_CTRL_H,
    ));

    // An 80-wide atom in a 110-wide block cannot share its line: it starts one.
    let narrow = Run::new()
        .word(&tr(cx, Msg::GALLERY_INLINE_NARROW_1))
        .atom(Size::new(80.0, 36.0), VerticalAlign::Bottom)
        .word(&tr(cx, Msg::GALLERY_INLINE_NARROW_2))
        .build();
    row = row.child(column_text(
        Page::Inline,
        "narrow",
        tr(cx, Msg::GALLERY_INLINE_COLUMN_NARROW),
        from_decl(sample("narrow", narrow, 110.0)),
        COL_W,
        COL_CTRL_H,
    ));

    // Items of different sizes share one baseline: the row is what a text button or a
    // price and its unit needs.
    row = row.child(column_text(
        Page::Inline,
        "row",
        tr(cx, Msg::GALLERY_INLINE_COLUMN_ROW),
        from_decl(row_sample()),
        COL_W,
        COL_CTRL_H,
    ));

    // The four rule styles side by side, on a second line of the page.
    let decorum = Container::new(Id::of(("decorow", Page::Inline.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .gap(20.0)
        .child(column_text(
            Page::Inline,
            "deco",
            tr(cx, Msg::GALLERY_INLINE_COLUMN_DECORATIONS),
            from_decl(deco_sample()),
            COL_W,
            120.0,
        ))
        .child(column_text(
            Page::Inline,
            "justify",
            tr(cx, Msg::GALLERY_INLINE_COLUMN_JUSTIFY),
            from_decl(justify_sample()),
            COL_W,
            120.0,
        ))
        .child(column_text(
            Page::Inline,
            "holder",
            tr(cx, Msg::GALLERY_INLINE_COLUMN_HOLDER),
            from_decl(holder_sample()),
            COL_W,
            120.0,
        ));

    let page = Container::new(Id::of(("bodycol", Page::Inline.tag())), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0)
        .child(Box::new(row))
        .child(Box::new(decorum));
    Box::new(page)
}
