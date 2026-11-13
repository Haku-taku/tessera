//! The Markdown page: the same document read and edited by one control.
//!
//! The page owns the palette and the syntax highlighter; the control owns every block, its
//! geometry and its look, so the reading card and the editing card cannot drift apart.

use crate::shell::*;
use ui_core::text::{TextRun, TextStyle};

const DOC_W: f32 = 560.0;
/// Card padding on both sides, so the paragraph measures what it has room for.
const TEXT_W: f32 = DOC_W - 24.0;

/// The editor column's id; a block's id is this mixed with its path of child indices.
pub(crate) fn markdown_edit_root() -> Id {
    Id::of(("markdown-edit", Page::Markdown.tag()))
}

/// The reading card's column id; the same paths under a different root.
pub(crate) fn markdown_read_root() -> Id {
    Id::of(("markdown-read", Page::Markdown.tag()))
}

/// Both cards, from the trees the app parsed when their sources last changed. The knobs
/// above them are the code card's own options, so one page shows every combination.
pub(crate) fn markdown_page(
    code: ui_widgets::CodeOptions,
    kit: &Md3Kit,
    sample: Option<std::sync::Arc<ui_richtext::Document>>,
    sample_frames: Option<std::sync::Arc<ui_richtext::Prepared>>,
    source: Option<std::sync::Arc<ui_richtext::Document>>,
    frames: Option<std::sync::Arc<ui_richtext::Prepared>>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let empty = std::sync::Arc::new(ui_richtext::Document::default());
    let reading = markdown_card(
        code,
        "document",
        markdown_read_root(),
        sample.unwrap_or_else(|| empty.clone()),
        sample_frames,
        false,
        Msg::GALLERY_PAGE_MARKDOWN_TITLE,
        cx,
    );
    let editing = markdown_card(
        code,
        "editing",
        markdown_edit_root(),
        source.unwrap_or_else(|| empty.clone()),
        frames,
        true,
        Msg::GALLERY_MARKDOWN_EDIT_TITLE,
        cx,
    );
    Box::new(
        Container::new(
            Id::of(("markdown-stack", Page::Markdown.tag())),
            NodeKind::Column,
        )
        .size(SizeClass::hug(0.0, 0.0))
        .gap(16.0)
        .child(code_options_row(code, kit, cx))
        .child(reading)
        .child(editing),
    )
}

/// The stable id of one of the page's code-card toggles.
pub(crate) fn code_option_id(option: &'static str) -> Id {
    Id::of(("markdown-code", Page::Markdown.tag(), option))
}

/// The height a capped code card takes in the demo, in code lines.
pub(crate) const CODE_CAP_LINES: u32 = 8;

/// The code card's knobs: numbers, wrapping and a height cap, each one its own switch.
fn code_options_row(
    code: ui_widgets::CodeOptions,
    kit: &Md3Kit,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(
        Id::of(("markdown-code-options", Page::Markdown.tag())),
        NodeKind::Row,
    )
    .size(SizeClass::hug(0.0, 0.0))
    .gap(18.0)
    .cross_pos(0.5);
    let options = [
        (
            "numbers",
            code.line_numbers,
            Msg::GALLERY_MARKDOWN_CODE_NUMBERS,
        ),
        ("wrap", code.wrap, Msg::GALLERY_MARKDOWN_CODE_WRAP),
        (
            "height",
            code.max_height.is_some(),
            Msg::GALLERY_MARKDOWN_CODE_HEIGHT,
        ),
    ];
    for (tag, on, title) in options {
        let id = code_option_id(tag);
        let control = kit.toggle(&ToggleSpec::new(id, on, ""), cx);
        row = row.child(cx.element(control)).child_decl(label(
            id.with("label"),
            tr(cx, title).to_string(),
            13.0,
            muted(),
            220.0,
        ));
    }
    Box::new(row)
}

/// One document in a card: read, or edited in place. `tag` is the card's own role, which
/// the page's layout check looks up; it must not follow the mode, or two editable cards
/// would share one column id.
fn markdown_card(
    code: ui_widgets::CodeOptions,
    tag: &'static str,
    id: Id,
    document: std::sync::Arc<ui_richtext::Document>,
    prepared: Option<std::sync::Arc<ui_richtext::Prepared>>,
    editable: bool,
    title: Msg,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let palette = Palette::current();
    let control = ui_widgets::Markdown {
        id,
        document,
        prepared,
        editable,
        math_editing: ui_widgets::MathEditing::Structural,
        popover_fill: match palette.light {
            true => Color::from_rgba8(250, 250, 252, 255),
            false => Color::from_rgba8(52, 56, 66, 255),
        },
        width: TEXT_W,
        size: 14.0,
        ink: palette.body,
        marker_ink: palette.code,
        link_ink: palette.link,
        code_ink: palette.code,
        code_fill: palette.code_fill,
        code_style: Some(code_style),
        code,
        image: None,
        muted: muted(),
        caret_ink: palette.body,
        selection_fill: Color {
            a: 0.3,
            ..palette.link
        },
        selection_painter: None,
    };
    let body = cx.element(Box::new(control));
    column_hug(Page::Markdown, tag, title, body, DOC_W, cx)
}

/// The control's fenced blocks get the page's highlighter.
fn code_style(language: Option<&str>, source: &str, base: TextStyle) -> Option<Vec<TextRun>> {
    let grammar = ui_highlight::Language::from_info(language?)?;
    match ui_highlight::tokens(grammar, source) {
        Ok(tokens) => Some(highlighted(base, source, &tokens, &Palette::current())),
        Err(error) => {
            eprintln!("markdown control: {error}");
            None
        }
    }
}

/// `source` cut at the token ranges; the gaps between them keep the base style.
fn highlighted(
    base: TextStyle,
    source: &str,
    tokens: &[ui_highlight::Token],
    palette: &Palette,
) -> Vec<TextRun> {
    let mut runs = Vec::with_capacity(tokens.len() + 1);
    let mut end = 0;
    for token in tokens {
        // A range that is not on a character boundary would panic to slice; skip it rather
        // than trust the grammar, and the code still reads as plain text there.
        let Some(text) = source.get(token.range.clone()) else {
            continue;
        };
        if token.range.start > end {
            runs.push(TextRun::new(&source[end..token.range.start], base));
        }
        let mut run = TextRun::new(text, base).inked(palette.token(token.class));
        run.style.italic = token.class == ui_highlight::TokenClass::Comment;
        runs.push(run);
        end = token.range.end;
    }
    if end < source.len() {
        runs.push(TextRun::new(&source[end..], base));
    }
    runs
}

/// The page's own palette, chosen by the frame's appearance: the crate stays colourless and
/// a second theme is a second set of values rather than a second page.
struct Palette {
    body: Color,
    link: Color,
    code: Color,
    code_fill: Color,
    light: bool,
}

impl Palette {
    fn current() -> Self {
        if ui_style::StyleScope::of(&root_env()).appearance == ui_design::Appearance::Dark {
            Self {
                body: Color::from_rgba8(228, 232, 240, 255),
                link: Color::from_rgba8(140, 180, 255, 255),
                code: muted(),
                code_fill: Color::rgba(1.0, 1.0, 1.0, 0.07),
                light: false,
            }
        } else {
            Self {
                body: Color::from_rgba8(32, 35, 42, 255),
                link: Color::from_rgba8(20, 90, 200, 255),
                code: Color::from_rgba8(60, 66, 78, 255),
                code_fill: Color::rgba(0.0, 0.0, 0.0, 0.06),
                light: true,
            }
        }
    }

    /// One syntax colour; the two sets are One Dark and One Light.
    fn token(&self, class: ui_highlight::TokenClass) -> Color {
        use ui_highlight::TokenClass::*;
        match (self.light, class) {
            (false, Keyword) => Color::from_rgba8(198, 120, 221, 255),
            (false, Function) => Color::from_rgba8(97, 175, 239, 255),
            (false, Type | Attribute | Label) => Color::from_rgba8(229, 192, 123, 255),
            (false, String) => Color::from_rgba8(152, 195, 121, 255),
            (false, Number | Constant) => Color::from_rgba8(209, 154, 102, 255),
            (false, Property) => Color::from_rgba8(224, 108, 117, 255),
            (false, Comment) => Color::from_rgba8(128, 132, 139, 255),
            (false, Operator | Punctuation | Variable) => muted(),
            (true, Keyword) => Color::from_rgba8(166, 38, 164, 255),
            (true, Function) => Color::from_rgba8(64, 120, 242, 255),
            (true, Type | Attribute | Label) => Color::from_rgba8(193, 132, 1, 255),
            (true, String) => Color::from_rgba8(80, 161, 79, 255),
            (true, Number | Constant) => Color::from_rgba8(152, 104, 1, 255),
            (true, Property) => Color::from_rgba8(228, 86, 73, 255),
            (true, Comment) => Color::from_rgba8(160, 161, 167, 255),
            (true, Operator | Punctuation | Variable) => self.code,
        }
    }
}
