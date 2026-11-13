//! A Markdown document placed row by row, read or edited in place.
//!
//! The document is cut into rows by `ui_richtext::scene`: one row per source line, syntax
//! kept as a hidden run, an object standing in for a formula or a marker. This control only
//! turns a row into declarations, so a construct never owns layout code of its own.
//!
//! Editing and reading draw the same tree. The difference is in the scene: an editor passes
//! the selection, so the row under the caret reveals the syntax the reader never sees.

use std::collections::HashMap;
use std::ops::Range;

use ui_core::geom::{Color, Corners, Insets, Point, Size};
use ui_core::layout::{
    AUTO, Anchor, AutoFlow, AxisSize, Clip, Grow, Overflow, OverlayPlacement, SizeClass, Track,
    TrackList,
};
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::text::{
    Decoration, Family, GenericFamily, LineNumbers, Room, TextAlign, VerticalAlign, Visibility,
};
use ui_core::tree::{ImageFit, Presence};
use ui_core::{
    AnimSpec, AtomBox, CursorShape, Decl, Easing, Id, Key, Modifiers, NodeData, NodeKind, Sense,
    TextBlock, TextRun, TextStyle, Weight,
};
use ui_i18n::Msg;
use ui_richtext::{
    Alignment as TableAlignment, Block, BlockKind, Cell, CellLook, Document, Form, ObjectKind,
    Part, Role, Row, RowKind, RowStyle,
};

/// Width of the bar a quote sets its content behind, the room after it, and the corner that
/// rounds the bar off where the quote begins and ends.
const QUOTE_BAR: f32 = 3.0;
const QUOTE_GAP: f32 = 4.0;
const QUOTE_CAP: f32 = 2.0;

/// The bytes each card line loses to its containers and to the block's own indentation.
///
/// The split matters: the containers' columns belong to the containers and never move with
/// the block's value, while the block's own indentation is what a fence line spells and the
/// other lines share.
#[derive(Default)]
struct CardIndents {
    /// The closing fence line's share of the block's value.
    close: Option<Range<usize>>,
    body: Vec<LineIndent>,
    /// The bytes of the block's own indentation each line loses to the parser.
    strip: usize,
}

/// One line's share of a code block's leading bytes.
struct LineIndent {
    /// What the containers' markers and reserved columns take.
    container: Range<usize>,
    /// The block's own spaces and tabs after them.
    spaces: Range<usize>,
    /// The whole value the parser strips from this line: the containers' reserved
    /// column on a line that writes no marker, plus the block's own spaces.
    value: Range<usize>,
}

/// The bytes a block writes before its opening fence, and the column they hold.
struct Leading {
    /// The bytes themselves, from where the row's cells end to the fence.
    range: Range<usize>,
    /// The column they take left of the card; zero when a container's cell holds it.
    ink: f32,
    /// Whether the block owns them, so the member its source spells is theirs.
    owns: bool,
}

/// The room one row's cells take, in the order the cells stand.
struct RowRoom {
    /// Each cell's width; a cell's width belongs to the construct that writes it.
    widths: Vec<f32>,
    /// Where a quote's bar rounds off; zero for a cell that draws no bar.
    corners: Vec<Corners>,
}

/// A fenced block's card: its corner and side inset, and the size of its head and foot notes.
const CODE_CARD_RADIUS: f32 = 8.0;
const CODE_CARD_PAD: f32 = 12.0;
/// Room between a line's number and the code it numbers.
const CODE_NUMBER_GAP: f32 = 8.0;
/// Room each side of the widest number inside the gutter's band, so the digits stand as far
/// from the card's edge as from the rule.
const CODE_NUMBER_PAD: f32 = CODE_NUMBER_GAP * 0.5;
/// The rule that ends the gutter's band: the band's tint stops there and the code's fill
/// begins.
const CODE_GUTTER_RULE: f32 = 1.0;
/// Room between the head's rule and the first code line, so the text does not touch it.
const CODE_CARD_GAP: f32 = 8.0;
const CODE_CARD_NOTE: f32 = 12.0;
/// How long the copy button's pill takes to fade in or out under the pointer. A reaction
/// is shorter than a transition on purpose: the pill answers the pointer, it does not
/// announce itself.
const CODE_COPY_FADE: AnimSpec = AnimSpec::new(0.1, Easing::Linear);

/// Leading between the lines of one paragraph, as a fraction of the type size: prose reads
/// at about 1.3 times the font's own line box, the ratio mature editors settle on, and it
/// follows the size instead of naming a pixel value per document.
const LINE_GAP_RATIO: f32 = 0.30;

/// A table cell's side and vertical padding, and the rule under the header row.
const TABLE_CELL_PAD: f32 = 6.0;
const TABLE_CELL_PAD_Y: f32 = 3.0;
const TABLE_SEPARATOR: f32 = 1.0;

/// Side of a task item's checkbox; the item's text starts after it.
const TASK_BOX: f32 = 16.0;
/// The space the source spells after `[x]`, kept after the box so the item's text starts
/// where it does with the marker drawn.
const TASK_GAP: f32 = 4.0;

/// A page-supplied highlighter for a fenced block: its language, its source and the base
/// style, answered with runs whose text concatenates to that source. The page owns the
/// grammar and the colours; the control owns where the runs fall.
pub type CodeStyle = fn(Option<&str>, &str, TextStyle) -> Option<Vec<TextRun>>;

/// One registered texture, with the pixel size its aspect ratio comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageTexture {
    pub id: u64,
    pub width: u32,
    pub height: u32,
}

/// A page-supplied texture for an image's source string. The string is the destination
/// exactly as written between the parentheses; resolving a path or fetching is the page's.
pub type ImageSource = fn(&str) -> Option<ImageTexture>;

/// How a formula is edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MathEditing {
    /// The formula gives way to its source and a preview floats beside it.
    Source,
    /// A display formula is its own editable control: a press puts the caret inside the
    /// structure, and its edits rewrite the block's bytes in the document source. An
    /// inline formula still reveals its source, because an atom has no node to focus.
    #[default]
    Structural,
}

/// The room between the formula's view and the preview, and the panel's own frame.
const MATH_PREVIEW_OFFSET: f32 = 6.0;
const MATH_PREVIEW_PAD: f32 = 8.0;
const MATH_PREVIEW_RADIUS: f32 = 4.0;

/// A line break that continues the list or quote the caret is in, and ends it when the
/// caret's line holds no content.
pub const BREAK: ShortcutId = ShortcutId("markdown.break");
/// An indent of the caret's line by the step that nests it; `OUTDENT` takes it back.
pub const INDENT: ShortcutId = ShortcutId("markdown.indent");
pub const OUTDENT: ShortcutId = ShortcutId("markdown.outdent");

/// How a fenced code card reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodeOptions {
    /// Draw each source line's number in a gutter beside the code.
    pub line_numbers: bool,
    /// Break long lines at the card's width. Off, the card scrolls sideways instead.
    pub wrap: bool,
    /// Taller code scrolls inside this height; `None` grows with the code.
    pub max_height: Option<CodeHeight>,
}

impl Default for CodeOptions {
    fn default() -> Self {
        Self {
            line_numbers: false,
            wrap: true,
            max_height: None,
        }
    }
}

/// The height a code card may grow to before its body scrolls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CodeHeight {
    /// A multiple of the code's own line box.
    Lines(u32),
    /// A length in logical pixels.
    Px(f32),
}

/// A Markdown document.
///
/// The look of a construct lives here, not in the caller: a document control that took a
/// per-kind callback would make every caller re-state the same heading scale, and a second
/// control for the same document would state it again.
pub struct Markdown {
    /// The column's own id. A row's id is this mixed with its block's path.
    pub id: Id,
    /// The parsed document, shared: a page keeps one and hands it over rather than
    /// re-reading it every frame. The handle is also what makes this control placeable as
    /// an element, and so cacheable as a whole subtree.
    pub document: std::sync::Arc<Document>,
    /// The projections cut beside the parse, when the app prepared them. A page that keeps
    /// a long document hands them over so a frame positions runs instead of cutting them
    /// again; `None` cuts them here, which is what a short document wants.
    pub prepared: Option<std::sync::Arc<ui_richtext::Prepared>>,
    /// Whether the document can be edited in place. Off, it still renders the same rows
    /// with the same geometry: no syntax is shown, and no caret, selection or view exists.
    pub editable: bool,
    /// How a formula is edited while `editable` is on.
    pub math_editing: MathEditing,
    /// The opaque surface a formula preview sits on. It must not be translucent: the
    /// document behind it would show through the typeset formula.
    pub popover_fill: Color,
    /// The column's width; each row wraps inside it.
    pub width: f32,
    /// Body size. Heading levels and code scale from it.
    pub size: f32,
    pub ink: Color,
    /// Ink for syntax that is showing because the caret is inside it.
    pub marker_ink: Color,
    /// Ink of a link and of an inline formula.
    pub link_ink: Color,
    pub code_ink: Color,
    pub code_fill: Color,
    /// A page-supplied highlighter for a fenced block, if the page has one.
    pub code_style: Option<CodeStyle>,
    /// How a code card reads: its gutter, its wrapping and how tall it may grow.
    pub code: CodeOptions,
    /// A page-supplied texture for a block that is one image, if the page has one.
    pub image: Option<ImageSource>,
    /// Ink of the bullet, the number and the quote bar.
    pub muted: Color,
    pub caret_ink: Color,
    pub selection_fill: Color,
    pub selection_painter: Option<ui_core::SelectionPainterRef>,
}

/// The id of the block at `path`, the chain of child indices from the document root.
fn block_id(root: Id, path: &[usize]) -> Id {
    path.iter().fold(root, |id, index| id.with(*index))
}

/// The buffer bytes one text view draws, and where its caret positions end.
#[derive(Clone)]
struct Window {
    range: Range<usize>,
    /// Whether the caret after the last byte is this view's. A view the row's text follows
    /// ends where that text begins, and the text draws the caret standing between them.
    own_end: bool,
}

impl Window {
    /// A view nothing else on its line follows: the caret after its last byte is its own.
    fn line(range: Range<usize>) -> Self {
        Self {
            range,
            own_end: true,
        }
    }

    /// A view the text beside it follows: the caret after its last byte belongs there.
    fn prefix(range: Range<usize>) -> Self {
        Self {
            range,
            own_end: false,
        }
    }
}

impl Markdown {
    /// Builder: how a formula is edited.
    pub fn math_editing(mut self, mode: MathEditing) -> Self {
        self.math_editing = mode;
        self
    }

    pub fn resolve_in(&self, cx: &crate::Cx<'_, '_>) -> Decl {
        // One input over the whole source: every row draws a window onto it, so a selection,
        // a caret and an edit all live in one coordinate space. Chrome stays outside.
        let mut column = Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // A row is a line box; the document's vertical rhythm is exactly its lines.
            .gap(0.0);
        if self.editable {
            column = column
                .sense(Sense::FOCUSABLE)
                // The rows draw the only ink there is, so the column must declare itself a
                // stop: a request to focus a control that paints nothing is dropped.
                .focus_stop(true)
                .cursor(CursorShape::Text)
                .text_input(std::sync::Arc::clone(&self.document.source))
                .text_input_enter_newline()
                .shortcut(KeyChord::new(Key::Enter, Modifiers::default()), BREAK)
                .shortcut(KeyChord::new(Key::Tab, Modifiers::default()), INDENT)
                .shortcut(
                    KeyChord::new(
                        Key::Tab,
                        Modifiers {
                            shift: true,
                            ..Modifiers::default()
                        },
                    ),
                    OUTDENT,
                );
            self.command(cx);
        }
        let rows = self.scene(cx).rows;
        let rooms = self.cell_rooms(&rows, cx);
        for (row, room) in rows.iter().zip(&rooms) {
            column = column.child(self.row(row, room, cx));
        }
        if let Some(preview) = self.math_preview(cx) {
            column = column.child(preview);
        }
        column
    }

    /// This frame's rows.
    fn scene(&self, cx: &crate::Cx<'_, '_>) -> ui_richtext::Scene {
        let focused = self.editable && cx.core().focused() == Some(self.id);
        // Reveal is cut from the selection a live composition began with: swapping the
        // bytes under a preedit cancels the composition in several input methods.
        let selection = focused
            .then(|| {
                cx.core()
                    .reveal_range_for(self.id)
                    .or_else(|| cx.core().caret_for(self.id).map(|at| at..at))
            })
            .flatten();
        ui_richtext::build_scene(&self.document, self.prepared.as_deref(), selection, focused)
    }

    /// The live formula preview Source mode floats while the caret touches a formula.
    ///
    /// It reads the committed document, never the preedit, so an IME composition does not
    /// rewrite it. Being an overlay, it takes no flow space and opening it moves nothing.
    fn math_preview(&self, cx: &crate::Cx<'_, '_>) -> Option<Decl> {
        if !self.editable || self.math_editing != MathEditing::Source {
            return None;
        }
        if cx.core().focused() != Some(self.id) {
            return None;
        }
        let selection = self.selection_range(cx)?;
        if let Some(mark) = self.document.marks.iter().find(|mark| {
            mark.kind == ui_richtext::MarkKind::InlineMath
                && selection.start <= mark.full.end
                && mark.full.start <= selection.end
        }) {
            // An atom has no node of its own; the caret's x in the row view is the nearest
            // fact that names the formula's place on the line.
            let view = self.row_view_for(mark.full.start, cx)?;
            let anchor = match (cx.core().caret_in_text_for(view), cx.rect_of(view)) {
                (Some(x), Some(rect)) => {
                    Anchor::Point(Point::new(rect.origin.x + x, rect.origin.y + rect.size.h))
                }
                _ => Anchor::Node(view),
            };
            let content = self.document.source.get(mark.range.clone())?;
            let id = self.id.with(("math-preview", mark.full.start));
            return Some(self.preview_panel(id, anchor, content, self.size, cx));
        }
        // A display formula is a row of its own, so its source is what the caret is in and
        // the panel hangs from the row rather than from a caret x.
        let block = self.document.blocks.iter().find(|block| {
            block.kind == BlockKind::Math
                && selection.start <= block.full.end
                && block.full.start <= selection.end
        })?;
        let scene = self.scene(cx);
        let row = scene
            .rows
            .iter()
            .find(|row| row.span.start <= block.span.start && block.span.start <= row.span.end)?;
        let anchor = Anchor::Node(self.row_id(row));
        let content = self.document.source.get(block.span.clone())?;
        let id = self.id.with(("math-preview", block.full.start));
        Some(self.preview_panel(id, anchor, content, self.size * 1.15, cx))
    }

    /// The floating formula panel Source mode hangs beside the caret's formula.
    fn preview_panel(
        &self,
        id: Id,
        anchor: Anchor,
        content: &str,
        size: f32,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let formula =
            crate::math::Math::new(id.with("formula"), content.trim(), size, self.link_ink)
                .resolve_in(cx);
        Decl::container(id, NodeKind::Column)
            .overlay(OverlayPlacement::below(anchor).offset(MATH_PREVIEW_OFFSET))
            .padding(Insets::all(MATH_PREVIEW_PAD))
            .background(self.popover_fill, Corners::all(MATH_PREVIEW_RADIUS))
            .border(self.muted, 1.0)
            .child(formula)
    }

    /// The text node drawing the row that holds `at`, so an overlay can hang from it.
    fn row_view_for(&self, at: usize, cx: &crate::Cx<'_, '_>) -> Option<Id> {
        let scene = self.scene(cx);
        let row = scene
            .rows
            .iter()
            .find(|row| row.span.start <= at && at <= row.span.end)?;
        Some(self.row_id(row).with("text"))
    }

    /// Answer this frame's own shortcut, when one fired on this control.
    ///
    /// The engine applies the edit it asks for as one engine edit at the caret it names, so
    /// the source, the history and the app's copy all take the same bytes.
    fn command(&self, cx: &crate::Cx<'_, '_>) {
        let Some((id, shortcut)) = cx.core().shortcut() else {
            return;
        };
        if id != self.id {
            return;
        }
        // The selection as the key saw it: keys later in the same burst may have extended it,
        // and replacing that would delete text the user selected after pressing this one.
        let (start, end, backwards) = match cx.core().shortcut_selection(self.id) {
            Some(selection) => (
                selection.anchor.min(selection.caret),
                selection.anchor.max(selection.caret),
                selection.anchor > selection.caret,
            ),
            None => match cx.core().caret_for(self.id) {
                Some(at) => (at, at, false),
                None => return,
            },
        };
        // The engine's buffer, not this frame's tree: a character typed into the same burst
        // as this key has not been parsed yet, and the edit must land after it.
        let source = cx
            .core()
            .input_value(self.id)
            .unwrap_or(&*self.document.source);
        let edit = match shortcut {
            BREAK => ui_richtext::line_break(source, start..end),
            INDENT => match ui_richtext::indent(source, start..end, false) {
                Some(edit) => edit,
                None => return,
            },
            OUTDENT => match ui_richtext::indent(source, start..end, true) {
                Some(edit) => edit,
                None => return,
            },
            _ => return,
        };
        match edit.selection {
            Some((anchor, caret)) => {
                // The command sees a normalised range; keep the direction the user's
                // selection had, so Shift+Tab leaves the active end where Shift+Tab found it.
                let (anchor, caret) = if backwards {
                    (caret, anchor)
                } else {
                    (anchor, caret)
                };
                cx.core()
                    .edit_input_selecting(self.id, edit.range, edit.text, (anchor, caret));
            }
            None => cx
                .core()
                .edit_input(self.id, edit.range, edit.text, edit.caret),
        }
    }

    /// One row: a text row draws its whole line in one node, a block row its construct.
    fn row(&self, row: &Row, room: &RowRoom, cx: &crate::Cx<'_, '_>) -> Decl {
        let id = self.row_id(row);
        let taken: f32 = room.widths.iter().sum();
        let width = (self.width - taken).max(0.0);
        let (style, parts) = match &row.kind {
            RowKind::Text { style, parts } => (text_style(self.size, *style), parts),
            RowKind::Empty { parts } => (TextStyle::new(self.size), parts),
            _ => return self.block_row(row, id, room, width, cx),
        };
        let (text, overlays) = self.line_row(row, id, room, style, parts, width, cx);
        if row.cells.is_empty() && overlays.is_empty() {
            return text;
        }
        let mut frame = self.frame(id);
        frame = frame.child(text);
        for overlay in overlays {
            frame = frame.child(overlay);
        }
        frame
    }

    /// The row's frame, for a row that draws more than its text: the stand-ins float over it.
    fn frame(&self, id: Id) -> Decl {
        Decl::container(id.with("frame"), NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .relative()
            .stack_align_axes(
                ui_core::layout::OverlayAlign::Start,
                ui_core::layout::OverlayAlign::Start,
            )
    }

    /// A block row: its card, with the cells its markers stand in beside it.
    fn block_row(
        &self,
        row: &Row,
        id: Id,
        room: &RowRoom,
        width: f32,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let body = self.body(row, id, width, cx);
        if row.cells.is_empty() {
            return body;
        }
        // The frame stretches its children: a cell takes the row's whole height, however
        // many lines the text beside it wraps to, which is what keeps a quote's bar unbroken.
        let mut decl = Decl::container(id.with("frame"), NodeKind::Row).size(SizeClass {
            width: AxisSize::fixed(self.width),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        });
        for (index, cell) in row.cells.iter().enumerate() {
            let width = room.widths[index];
            let corners = room.corners[index];
            let member = match row.kind {
                RowKind::Code { .. } => self.cell_member(row, cell),
                _ => None,
            };
            decl = decl.child(self.cell(row, index, cell, width, corners, member, cx));
        }
        decl.child(body)
    }

    /// The member a code row's cell spells, when it holds the block's own indentation.
    ///
    /// A quote's cell runs to the content's column, so a quoted fence's spaces are its
    /// bytes; the group still starts there, and the fence line is the member it spells.
    fn cell_member(&self, row: &Row, cell: &Cell) -> Option<ui_core::text::Structure> {
        let block = self.block_at(&row.path)?;
        let (open, _) = self.code_fences(block);
        let line = self.card_line_indent(
            &ui_richtext::scene::ancestors_of(&self.document, &row.path),
            open.as_ref()?.start,
        );
        let source = cell.source.as_ref()?;
        let value = &line.value;
        if value.is_empty() || source.start > value.start || value.end > source.end {
            return None;
        }
        Some(ui_core::text::Structure {
            group: Self::code_group(block),
            range: value.start - source.start..value.end - source.start,
            shown: true,
        })
    }

    /// One cell: the marker's own bytes, and the stand-in they render as.
    ///
    /// Both forms share the cell's room, which never changes, so the text beside the marker
    /// keeps its column however the marker is drawn. The bytes stay a view of their own, so
    /// a press in the cell lands the caret in them and the caret can stand in the marker.
    #[allow(clippy::too_many_arguments)]
    fn cell(
        &self,
        row: &Row,
        index: usize,
        cell: &Cell,
        width: f32,
        corners: Corners,
        member: Option<ui_core::text::Structure>,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let id = block_id(self.id, &cell.owner)
            .with(("cell", row.span.start))
            .with(index);
        let style = self.row_text_style(row);
        // The marker sits on the first line's baseline: the same half-leading the body
        // carries above its own first line.
        let half = self.size * LINE_GAP_RATIO * 0.5;
        // Whitespace the source spells at the cell's end is the cell's own room: both forms
        // end at the content edge, so the text beside them starts at one column.
        let gap = self.kept_space(cell, style, cx);
        let inner = (width - gap).max(0.0);
        // The cell stretches over the row, however many lines the text beside it wraps to:
        // that is what keeps a quote's bar unbroken. Its bar is placed against this box.
        let mut decl = Decl::container(id, NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            })
            .padding(Insets {
                right: gap,
                ..Insets::default()
            })
            .relative()
            .stack_align_axes(
                ui_core::layout::OverlayAlign::Start,
                ui_core::layout::OverlayAlign::Start,
            );
        if cell.look == CellLook::Quote && cell.form == Form::Rendered {
            decl = decl.child(self.quote_bar(row, cell, corners));
        }
        if let Some(source) = &cell.source {
            // The bytes show only in source form: rendered, the stand-in paints over them.
            // They keep their room either way, so a vertical step maps a column to the byte
            // under it in both forms and the caret can stand inside the marker.
            let text = self
                .document
                .source
                .get(source.clone())
                .unwrap_or("")
                .to_owned();
            let mut run = TextRun::new(text, style);
            run.visibility = match cell.form {
                Form::Rendered => Visibility::Covered,
                Form::Source => Visibility::Drawn,
            };
            run.ink = Some(self.marker_ink);
            let mut view = self.text_frame(
                id.with("text"),
                Some(Window::prefix(source.clone())),
                vec![run],
                Vec::new(),
                self.marker_ink,
                AxisSize::fixed(inner),
                true,
                cx,
            );
            view.box_model.padding.top += half;
            if let NodeData::Text { block, .. } = &mut view.data {
                // The bytes stand where the stand-in does: a number ends at its dot in both
                // forms, and the leading matches the body's own first line.
                let block = std::rc::Rc::make_mut(block);
                block.align = cell_align(cell.look);
                block.line_gap = self.size * LINE_GAP_RATIO;
            }
            decl = decl.child(with_structures(view, member.into_iter().collect()));
        }
        if cell.form == Form::Rendered && cell.written {
            match cell.look {
                // A bullet stands at the marker's first byte; a number ends at its dot, so
                // a list's numbers line up on their dots instead of starting ragged.
                CellLook::Bullet | CellLook::Ordered => {
                    let label = self.label(id.with("label"), cell, style, inner, half);
                    decl = decl.child(label);
                }
                CellLook::Task { checked } => {
                    decl = decl.child(self.task(cell, checked, half, cx));
                }
                CellLook::Quote | CellLook::Indent => {}
            }
        }
        decl
    }

    /// The label a bullet or a number stands as: text, not a marker, so it holds no position.
    fn label(&self, id: Id, cell: &Cell, style: TextStyle, width: f32, half: f32) -> Decl {
        let mut block = TextBlock::plain(cell.label.clone(), style);
        block.align = cell_align(cell.look);
        block.line_gap = self.size * LINE_GAP_RATIO;
        let mut decl = Decl::container(id, NodeKind::Text)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                top: half,
                ..Insets::default()
            });
        decl.data = NodeData::Text {
            ink: self.muted,
            block: std::rc::Rc::new(block),
            caret: None,
            selection: None,
            composing: None,
            nowrap: true,
        };
        decl
    }

    /// A task item's box, standing in the cell its `[x]` occupies.
    fn task(&self, cell: &Cell, checked: bool, half: f32, cx: &crate::Cx<'_, '_>) -> Decl {
        // The box's id is the flag byte's own offset, so it names the same byte for a
        // reader, an editor and a press.
        let source = &*self.document.source;
        let flag = cell
            .source
            .as_ref()
            .and_then(|range| self.flag_at(range, source));
        let task_id = match flag {
            Some(flag) => self.id.with(("task", flag)),
            None => Id::str("markdown-task"),
        };
        // A press on the box is the box's: it flips its byte and leaves the caret where it
        // was, even in an input the box is not inside.
        if self.editable
            && cx.core().clicked_within(task_id)
            && let Some(flag) = flag
        {
            let to = if checked { " " } else { "x" };
            cx.core().edit_input_detached(self.id, flag..flag + 1, to);
        }
        self.task_box(task_id, checked, half, cx)
    }

    /// The bar a quote's cell draws in rendered form.
    fn quote_bar(&self, row: &Row, cell: &Cell, corners: Corners) -> Decl {
        Decl::container(
            block_id(self.id, &cell.owner).with(("bar", row.span.start)),
            NodeKind::Box,
        )
        // Placed against the cell's box, so it spans the row's whole height: a stack does
        // not stretch a child, and a bar that hugged would be one line tall.
        .size(SizeClass {
            width: AxisSize::fixed(QUOTE_BAR),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .absolute()
        .inset(Insets::new(0.0, 0.0, AUTO, 0.0))
        .background(self.muted, corners)
    }

    /// The room each row's cells take, in the same order as `rows`.
    ///
    /// A cell's width belongs to the construct that writes it, not to the line: every cell
    /// of one list or one quote takes the widest of them, so the text after the markers
    /// starts at one column. A cell is as wide as the source spells it or as wide as its
    /// stand-in is drawn, whichever is wider, plus the separator the source spells after
    /// the marker; that room is the same in both forms.
    fn cell_rooms(&self, rows: &[Row], cx: &crate::Cx<'_, '_>) -> Vec<RowRoom> {
        let mut shared: HashMap<(Vec<usize>, u8), f32> = HashMap::new();
        for row in rows {
            let style = self.row_text_style(row);
            for cell in &row.cells {
                let Some((owner, look)) = cell.share() else {
                    continue;
                };
                if !cell.written {
                    continue;
                }
                let want = self.cell_want(cell, style, cx);
                let entry = shared.entry((owner.to_vec(), look)).or_insert(0.0);
                *entry = entry.max(want);
            }
        }
        rows.iter()
            .enumerate()
            .map(|(index, row)| {
                let style = self.row_text_style(row);
                let widths = row
                    .cells
                    .iter()
                    .map(|cell| match cell.share() {
                        // A line that continues the construct without writing its marker
                        // stands at the same distance from its text as the lines above it.
                        Some((owner, look)) => shared
                            .get(&(owner.to_vec(), look))
                            .copied()
                            .unwrap_or_else(|| self.cell_want(cell, style, cx)),
                        None => self.cell_want(cell, style, cx),
                    })
                    .collect();
                let corners = row
                    .cells
                    .iter()
                    .map(|cell| match cell.look {
                        CellLook::Quote => quote_corners(rows, index, cell),
                        _ => Corners::ZERO,
                    })
                    .collect();
                RowRoom { widths, corners }
            })
            .collect()
    }

    fn cell_want(&self, cell: &Cell, style: TextStyle, cx: &crate::Cx<'_, '_>) -> f32 {
        let source = cell
            .source
            .as_ref()
            .map(|range| self.document.source.get(range.clone()).unwrap_or(""))
            .unwrap_or("");
        // Either form may be the wider one: the source's own bytes, or the label that
        // stands in for them plus the gap they spell.
        self.advance(source, style, cx)
            .max(self.stand_in(cell, style, cx) + self.kept_space(cell, style, cx))
    }

    /// The advance of the whitespace the source spells at a cell's end: the rendered form
    /// draws a label where those bytes stand, and keeps the gap they left beside it.
    fn kept_space(&self, cell: &Cell, style: TextStyle, cx: &crate::Cx<'_, '_>) -> f32 {
        // A task's box and a quote's bar carry their own gap in `stand_in`.
        if matches!(cell.look, CellLook::Task { .. } | CellLook::Quote) {
            return 0.0;
        }
        let source = cell
            .source
            .as_ref()
            .map(|range| self.document.source.get(range.clone()).unwrap_or(""))
            .unwrap_or("");
        (self.advance(source, style, cx) - self.advance(source.trim_end(), style, cx)).max(0.0)
    }

    /// The width the rendered form needs by itself: the label, the box with its gap, or
    /// the bar with its. A label's separator is the cell's own room (`kept_space`).
    fn stand_in(&self, cell: &Cell, style: TextStyle, cx: &crate::Cx<'_, '_>) -> f32 {
        match cell.look {
            CellLook::Bullet | CellLook::Ordered => self.advance(&cell.label, style, cx),
            CellLook::Task { .. } => TASK_BOX + TASK_GAP,
            CellLook::Quote => QUOTE_BAR + QUOTE_GAP,
            CellLook::Indent => 0.0,
        }
    }

    /// The advance of `text` in `style`, or zero where the host has no measurer.
    fn advance(&self, text: &str, style: TextStyle, cx: &crate::Cx<'_, '_>) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        cx.text_size(&TextBlock::plain(text.to_owned(), style), f32::INFINITY)
            .map_or(0.0, |size| size.w)
    }

    /// The style the row's own text draws in; a marker takes the style of the line it starts.
    fn row_text_style(&self, row: &Row) -> TextStyle {
        match &row.kind {
            RowKind::Text { style, .. } => text_style(self.size, *style),
            _ => TextStyle::new(self.size),
        }
    }

    /// The id a row draws under. The first line of a block keeps the block's own id, so a
    /// caller that names a block still finds its first row.
    fn row_id(&self, row: &Row) -> Id {
        if row.path.is_empty() {
            return self.id.with(("blank", row.span.start));
        }
        let base = block_id(self.id, &row.path);
        // Only a text block's later lines need an id of their own: its first line, and the
        // one row a block-level construct draws, are the block itself.
        let first = self.block_at(&row.path).is_some_and(|block| {
            ui_richtext::scene::line_start(&*self.document.source, block.full.start)
                == row.span.start
        });
        if first {
            base
        } else {
            base.with(("row", row.span.start))
        }
    }

    fn body(&self, row: &Row, id: Id, width: f32, cx: &crate::Cx<'_, '_>) -> Decl {
        let mut body = self.kind_body(row, id, width, cx);
        // A row is a line box: half the leading sits above it and half below, so two
        // neighbouring rows meet at exactly one line's leading, whether each holds one
        // line, wraps to three, or holds nothing at all. The row's own frame, when it has
        // one, stretches over this padding, which is what keeps a quote's bar unbroken.
        let half = self.size * LINE_GAP_RATIO * 0.5;
        body.box_model.padding.top += half;
        body.box_model.padding.bottom += half;
        body
    }

    fn kind_body(&self, row: &Row, id: Id, width: f32, cx: &crate::Cx<'_, '_>) -> Decl {
        match &row.kind {
            // A text row draws itself through `line_row`; only a block row comes here.
            RowKind::Text { .. } | RowKind::Empty { .. } => Decl::container(id, NodeKind::Box),
            RowKind::Code { language, form } => {
                let Some(block) = self.block_at(&row.path) else {
                    return Decl::container(id, NodeKind::Box);
                };
                self.code_card(block, row, id, width, language.as_deref(), *form, cx)
            }
            RowKind::Math { form } => {
                let Some(block) = self.block_at(&row.path) else {
                    return Decl::container(id, NodeKind::Box);
                };
                self.math_block(block, id, width, *form, cx)
            }
            RowKind::Image { destination, form } => {
                let Some(block) = self.block_at(&row.path) else {
                    return Decl::container(id, NodeKind::Box);
                };
                self.image_block(block, id, width, destination, *form, cx)
            }
            RowKind::Rule { form } => {
                if *form == ui_richtext::Form::Source {
                    let text = source_text(&self.document, &row.span);
                    return self.text_frame(
                        id.with("text"),
                        Some(Window::line(row.span.clone())),
                        vec![TextRun::new(text, TextStyle::new(self.size))],
                        Vec::new(),
                        self.ink,
                        AxisSize::fixed(width),
                        false,
                        cx,
                    );
                }
                // The line draws no byte, but its bytes still stand as a view: a press on
                // it lands after them, which touches the block and shows the source next
                // frame. Without that, the rule is a stretch of the document no caret can
                // enter and no drag can cover.
                let mut decl = Decl::container(id, NodeKind::Row)
                    .size(SizeClass {
                        width: AxisSize::fixed(width),
                        height: AxisSize::fixed(1.0),
                        grow_x: Grow::No,
                        grow_y: Grow::No,
                    })
                    .background(self.muted, Corners::ZERO);
                if self.editable {
                    let mut run = TextRun::new(
                        source_text(&self.document, &row.span),
                        TextStyle::new(self.size),
                    );
                    run.visibility = Visibility::Hidden;
                    decl.data = NodeData::Text {
                        ink: Color::TRANSPARENT,
                        block: std::rc::Rc::new(TextBlock {
                            runs: vec![run],
                            input_base: Some(row.span.start),
                            ..Default::default()
                        }),
                        caret: None,
                        selection: None,
                        composing: None,
                        nowrap: true,
                    };
                }
                decl
            }
            RowKind::Table { form } => {
                let Some(block) = self.block_at(&row.path) else {
                    return Decl::container(id, NodeKind::Box);
                };
                self.table(block, id, width, *form, cx)
            }
            // Front matter is opaque to layout in a reader, but an editor must show its
            // bytes or the document's metadata would be invisible and uneditable. Source
            // form wins whenever the control is editable, focused or not.
            RowKind::FrontMatter { form } => {
                if *form == ui_richtext::Form::Source || self.editable {
                    let text = source_text(&self.document, &row.span);
                    return self.text_frame(
                        id.with("text"),
                        Some(Window::line(row.span.clone())),
                        vec![TextRun::new(text, TextStyle::new(self.size))],
                        Vec::new(),
                        self.ink,
                        AxisSize::fixed(width),
                        false,
                        cx,
                    );
                }
                Decl::container(id, NodeKind::Box).size(SizeClass {
                    width: AxisSize::fixed(width),
                    height: AxisSize::fixed(0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
            }
        }
    }

    /// One text row drawn as one node: every marker as a room with its stand-in, then the
    /// line's own text. A run is always its source bytes, so an offset inside is an offset
    /// into the document.
    #[allow(clippy::too_many_arguments)]
    fn line_row(
        &self,
        row: &Row,
        id: Id,
        room: &RowRoom,
        style: TextStyle,
        parts: &[Part],
        width: f32,
        cx: &crate::Cx<'_, '_>,
    ) -> (Decl, Vec<Decl>) {
        let source = &*self.document.source;
        let start = row.span.start;
        let half = self.size * LINE_GAP_RATIO * 0.5;
        let mut runs: Vec<TextRun> = Vec::with_capacity(row.cells.len() + parts.len());
        let mut atoms: Vec<AtomBox> = Vec::new();
        let mut overlays: Vec<Decl> = Vec::new();
        // `x` is where the next cell starts, `pending` the room of cells that spell no byte.
        let mut x = 0.0f32;
        let mut pending = 0.0f32;
        let mut last: Option<(usize, bool)> = None;
        for (index, cell) in row.cells.iter().enumerate() {
            let cell_x = x + pending;
            // A lazy continuation line writes no `>`, but its bar still stands: the cell
            // holds no bytes, so nothing in the branches below would draw it.
            if cell.look == CellLook::Quote && cell.form == Form::Rendered && !cell.written {
                overlays.push(self.bar_overlay(row, cell, cell_x, room.corners[index]));
            }
            let Some(range) = &cell.source else {
                pending += room.widths[index];
                continue;
            };
            let at = range.start - start;
            let text = source.get(range.clone()).unwrap_or("").to_owned();
            let here = room.widths[index] + pending;
            pending = 0.0;
            x = cell_x + here;
            let stand_in = cell.form == Form::Rendered && cell.written;
            if stand_in {
                // The bytes stay in the line under the stand-in that paints over them: kept
                // in the flow with their advance, so a caret can stand among them and a drag
                // can take them, while the atom alone draws.
                let mut run = TextRun::new(text.clone(), style);
                run.visibility = Visibility::Covered;
                run.room = Some(Room::new(0.0, TextAlign::Start));
                runs.push(run);
                let atom = match cell.look {
                    CellLook::Bullet | CellLook::Ordered => {
                        self.marker_stand_in(cell, style, here, cx).map(|object| {
                            let mut atom = AtomBox::new(at, object.size())
                                .spanning(text.len())
                                .object(object);
                            atom.size.w = here;
                            atom
                        })
                    }
                    CellLook::Task { checked } => {
                        overlays.push(self.task_overlay(cell, checked, cell_x, half, cx));
                        Some(Self::room_atom(at, text.len(), here, style))
                    }
                    CellLook::Quote => {
                        let corners = room.corners[index];
                        overlays.push(self.bar_overlay(row, cell, cell_x, corners));
                        Some(Self::room_atom(at, text.len(), here, style))
                    }
                    CellLook::Indent => None,
                };
                if let Some(atom) = atom {
                    atoms.push(atom);
                    last = Some((atoms.len() - 1, false));
                }
            } else {
                // Bytes with no stand-in — an indent, a quiet line's room, a revealed
                // marker — keep the room the cell shares. The editor draws the source as
                // written; a page draws the structure its own way and shows none of it.
                let mut run = TextRun::new(text, style);
                if self.editable {
                    run.ink = Some(self.marker_ink);
                    run.room = Some(Room::new(here, cell_align(cell.look)));
                } else {
                    run.visibility = Visibility::Covered;
                    let width = match cell.look {
                        CellLook::Indent => 0.0,
                        _ => here,
                    };
                    run.room = Some(Room::new(width, cell_align(cell.look)));
                }
                runs.push(run);
                last = Some((runs.len() - 1, true));
            }
        }
        // A cell that spells no byte and has nothing after it still keeps its room.
        if pending > 0.0 {
            match last {
                Some((index, true)) => {
                    if let Some(room) = runs[index].room.as_mut() {
                        room.width += pending;
                    }
                }
                Some((index, false)) => atoms[index].size.w += pending,
                None => {}
            }
        }
        for part in parts {
            let at = part.source.start - start;
            let text = source.get(part.source.clone()).unwrap_or("").to_owned();
            let mut run = TextRun::new(text, style);
            match &part.role {
                Role::Content => self.look(&mut run, part.source.clone()),
                Role::Syntax { form } => {
                    run.visibility = match form {
                        Form::Rendered => Visibility::Hidden,
                        Form::Source => Visibility::Drawn,
                    };
                    run.ink = Some(self.marker_ink);
                }
                Role::Object(kind) => {
                    run.visibility = Visibility::Hidden;
                    match kind {
                        ObjectKind::Math => {
                            if let Some(object) = crate::math::Math::new(
                                id.with(("math", at)),
                                part.text.as_str(),
                                self.size,
                                self.link_ink,
                            )
                            .object_in(cx)
                            {
                                atoms.push(
                                    AtomBox::new(at, object.size())
                                        .spanning(run.text.len())
                                        .object(object),
                                );
                            }
                        }
                        ObjectKind::Entity => {
                            if let Some(object) = self.glyph_object(
                                part.text.to_string(),
                                style,
                                run.ink.unwrap_or(self.ink),
                                cx,
                            ) {
                                atoms.push(
                                    AtomBox::new(at, object.size())
                                        .spanning(run.text.len())
                                        .object(object),
                                );
                            }
                        }
                        ObjectKind::Footnote { .. } => {
                            // A raised label: the smaller face sits on the line's top edge,
                            // which is the superscript position without a baseline shift.
                            let raised = TextStyle {
                                size: style.size * 0.75,
                                ..style
                            };
                            if let Some(object) =
                                self.glyph_object(part.text.to_string(), raised, self.link_ink, cx)
                            {
                                let mut atom = AtomBox::new(at, object.size())
                                    .spanning(run.text.len())
                                    .object(object);
                                atom.align = VerticalAlign::Top;
                                atoms.push(atom);
                            }
                        }
                    }
                }
            }
            runs.push(run);
        }
        if runs.is_empty() {
            // An empty row still owns a line box, and the cells' room is its first line's.
            runs.push(TextRun::new("", style));
        }
        let mut decl = self.text_frame(
            id.with("text"),
            Some(Window::line(row.span.clone())),
            runs,
            atoms,
            self.ink,
            AxisSize::fixed(width),
            false,
            cx,
        );
        if let NodeData::Text { block, .. } = &mut decl.data {
            let block = std::rc::Rc::make_mut(block);
            block.line_gap = self.size * LINE_GAP_RATIO;
            // The body column: what a wrapped line clears, and where a row whose cells spell
            // no byte at all starts, since no run or atom carries their room.
            let taken: f32 = room.widths.iter().sum();
            if !row.cells.iter().any(|cell| cell.source.is_some()) {
                block.indent = taken;
            }
            block.hanging = taken;
        }
        decl.box_model.padding.top += half;
        decl.box_model.padding.bottom += half;
        (decl, overlays)
    }

    /// The stand-in a bullet or a number paints: the label, placed where the marker's ink
    /// stands, inside the room its bytes share.
    fn marker_stand_in(
        &self,
        cell: &Cell,
        style: TextStyle,
        room: f32,
        cx: &crate::Cx<'_, '_>,
    ) -> Option<ui_core::math::InlineObject> {
        let mut object = self.glyph_object(cell.label.clone(), style, self.marker_ink, cx)?;
        let gap = self.kept_space(cell, style, cx);
        let offset = match cell_align(cell.look) {
            TextAlign::End => (room - gap - object.size().w).max(0.0),
            _ => 0.0,
        };
        if let ui_core::math::InlineObject::Glyphs { offset: at, .. } = &mut object {
            *at = offset;
        }
        Some(object)
    }

    /// A cell's room with no glyph in it: the bytes stay positions, a shape paints over it.
    fn room_atom(at: usize, len: usize, width: f32, style: TextStyle) -> AtomBox {
        let object = ui_core::math::InlineObject::Glyphs {
            text: String::new(),
            style,
            ink: Color::TRANSPARENT,
            size: Size::new(0.0, 0.0),
            baseline: 0.0,
            offset: 0.0,
        };
        let mut atom = AtomBox::new(at, Size::new(width, 0.0)).spanning(len);
        atom.object = Some(object);
        atom
    }

    fn task_overlay(
        &self,
        cell: &Cell,
        checked: bool,
        x: f32,
        half: f32,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        self.task(cell, checked, half, cx)
            .absolute()
            .inset(Insets::new(x, 0.0, AUTO, 0.0))
    }

    fn bar_overlay(&self, row: &Row, cell: &Cell, x: f32, corners: Corners) -> Decl {
        self.quote_bar(row, cell, corners)
            .inset(Insets::new(x, 0.0, AUTO, 0.0))
    }

    /// Bytes of the line break at `at` in the document, or zero when none starts there.
    fn break_len(&self, at: usize) -> usize {
        ui_core::text::break_len(&*self.document.source, at)
    }

    /// A text node drawing one window of the buffer. `None` draws runs that name no position.
    ///
    /// The caret and the selection belong to the input and are in buffer offsets, while the
    /// node draws offsets local to its own window; both are mapped here, once, so every view
    /// maps them the same way. One position draws one caret: a window that ends where another
    /// begins hands the caret there to the window it starts.
    #[allow(clippy::too_many_arguments)]
    fn text_frame(
        &self,
        id: Id,
        window: Option<Window>,
        mut runs: Vec<TextRun>,
        atoms: Vec<AtomBox>,
        ink: Color,
        width: AxisSize,
        nowrap: bool,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let selection = cx.core().selection_for(self.id);
        // One caret at a time: while a formula inside this document owns the keyboard, the
        // document's caret stands down even though its selection still marks the bytes.
        let caret = (cx.core().focused() == Some(self.id))
            .then(|| cx.core().caret_for(self.id))
            .flatten();
        let preedit = cx.core().preedit_for(self.id);
        let (base, mut caret_at, mut selection_prim) = match &window {
            Some(window) => {
                let range = &window.range;
                // One caret position, one view. A window keeps the position after its last
                // byte only when no drawn byte starts there; in this scene the next view
                // stands either past a line break or on the same line, so the break at the
                // window's end is what tells the two apart. A declared `own_end` alone let
                // two views both claim a boundary and draw two carets at one caret.
                let owns_end = window.own_end
                    && (range.end >= self.document.source.len()
                        || ui_core::text::break_len(&*self.document.source, range.end) > 0);
                let inside = |at: usize| {
                    range.start <= at && (at < range.end || (owns_end && at == range.end))
                };
                let at = caret.filter(|at| inside(*at)).map(|at| at - range.start);
                // The break after this view is source no view draws. A selection that takes
                // it whole already starts at or before it, and reaches one break past the
                // view's last byte so the text pass can mark it; a selection that lies past
                // this view must not stretch back onto its end.
                let gap = self.break_len(range.end);
                let prim = selection
                    .filter(|selection| !selection.is_collapsed())
                    .and_then(|selection| {
                        let selected = selection.range();
                        let takes = gap > 0
                            && selected.start <= range.end
                            && selected.end >= range.end + gap;
                        let end = if takes { range.end + gap } else { range.end };
                        let low = selected.start.clamp(range.start, end);
                        let high = selected.end.clamp(range.start, end);
                        (low < high).then(|| ui_core::SelectionPrim {
                            range: low - range.start..high - range.start,
                            fill: self.selection_fill,
                            painter: self.selection_painter,
                        })
                    });
                (Some(range.start), at, prim)
            }
            None => (None, None, None),
        };

        let mut composing = None;
        if let (Some(preedit), Some(at)) = (preedit.as_ref(), caret_at)
            && !preedit.is_empty()
        {
            // Only the view holding the caret composes; the preedit takes the caret's
            // place in this view's own text.
            let value: String = runs.iter().map(|run| run.text.as_str()).collect();
            let (_, range, mapped) = crate::text::typed_input(&value, Some(at), Some(preedit));
            let style = runs
                .first()
                .map(|run| run.style)
                .unwrap_or(TextStyle::new(self.size));
            runs = crate::text::splice_run(runs, at, TextRun::new(&preedit.text, style));
            caret_at = mapped;
            selection_prim = None;
            composing = range;
        }
        // A frame with no run — an empty line, an empty fence — still occupies a line, and
        // that line is where the caret goes. One empty run gives the view its line box.
        if runs.is_empty() {
            runs.push(TextRun::new("", TextStyle::new(self.size)));
        }

        let mut text = Decl::container(id, NodeKind::Text).size(SizeClass {
            width,
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        });
        // A reader has no caret to place, so the node joins no input: a text node that
        // names a buffer position is a view, and a view belongs to an editing control.
        if self.editable {
            text = text.sense(Sense::CLICK);
        }
        text.data = NodeData::Text {
            ink,
            block: std::rc::Rc::new(TextBlock {
                runs,
                atoms,
                input_base: base.filter(|_| self.editable),
                ..Default::default()
            }),
            caret: caret_at.map(|offset| ui_core::Caret {
                offset,
                color: self.caret_ink,
            }),
            selection: selection_prim,
            composing,
            nowrap,
        };
        text
    }

    /// A fenced block: three stacked bands. The head carries the language and copy, the
    /// middle the code, the foot the line count; each state draws the same three bands, so
    /// the card is the same height whether or not it is being edited.
    #[allow(clippy::too_many_arguments)]
    fn code_card(
        &self,
        block: &Block,
        row: &Row,
        id: Id,
        width: f32,
        language: Option<&str>,
        form: ui_richtext::Form,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let path = row.path.as_slice();
        let style = self.code_text_style();
        let line = self.code_line_height(cx);
        let header = id.with("header");
        let copy = header.with("copy");
        // A press on the copy button also lands the caret in the block. Keep the card up
        // and answer the click here, or the button vanishes before its release names it.
        let copying = cx.core().pressed_within(copy) || cx.core().clicked_within(copy);
        if cx.core().clicked_within(copy) {
            cx.core()
                .copy_text(source_text(&self.document, &block.span));
        }
        let (open, close) = self.code_fences(block);
        let ancestors = ui_richtext::scene::ancestors_of(&self.document, path);
        // The closing fence's line belongs to the foot, containers and indentation included,
        // so the body stops where that line begins.
        let close_line =
            close.map(|close| self.close_line_start(&ancestors, close.start)..close.end);
        let content_end = close_line
            .as_ref()
            .map_or(block.span.end, |close| close.start);
        let (range, lines) = self.code_content(block, content_end);
        // The gutter is as wide as the widest number, so a card of ninety-nine lines and
        // one of nine draw their digits at the same edge.
        let numbered = self.code.line_numbers && lines > 0;
        let gutter = if self.code.line_numbers {
            let digits = lines.max(1).to_string().len();
            self.advance(&"0".repeat(digits), style, cx) + CODE_NUMBER_GAP
        } else {
            0.0
        };
        let indents = CardIndents {
            close: close_line
                .as_ref()
                .map(|close| ui_richtext::scene::code_value(&self.document, &ancestors, close)),
            body: self.card_lines(&ancestors, &range),
            strip: ui_richtext::scene::code_strip(&self.document, &ancestors, block),
        };
        // The bytes before the fence no cell of the row draws. A container's cell holds the
        // column they stand in, so only a block outside one has room of its own there.
        let prefix = self.code_prefix(block, row, &ancestors);
        let spacing = prefix
            .as_ref()
            .map_or(0..0, |leading| leading.range.clone());
        let on_spacing = cx.core().selection_for(self.id).is_some_and(|selection| {
            !spacing.is_empty() && ui_richtext::state::touches(&spacing, &selection.range())
        });
        // A press on the copy button lands the caret in the block, which would swap the
        // card for its source and take the button away before the release names it.
        let source_form = (form == ui_richtext::Form::Source || on_spacing) && !copying;
        // In source form the block stands at its own column: the bytes before its first
        // line are its indentation, so the card starts after them rather than reading
        // through them. The card keeps the same right edge, so nothing around it moves.
        // The bytes stay a view in the rendered form too, taking no room, so a press on
        // them enters the block like a press on the code does.
        let taken = (self.width - width).max(0.0);
        let leading = prefix.map(|mut leading| {
            if leading.owns && source_form {
                // A container's cell already reserves the column the bytes stand in; the
                // block's own indentation has room only when the bytes are the line's own.
                leading.ink = match taken > 0.0 {
                    true => taken,
                    false => self.code_prefix_width(&leading.range, cx),
                };
            }
            leading
        });
        let indent = match (source_form, &leading) {
            (true, Some(leading)) => (leading.ink - taken).max(0.0),
            _ => 0.0,
        };
        let outer = width;
        let width = (width - indent).max(0.0);
        // The code starts at the card's own left edge: the gutter's band is its first
        // column, not an inset inside a padding.
        let code_width = (width - CODE_CARD_PAD).max(0.0);
        // The head and body are two rectangles joined edge to edge, with a rule between
        // them; the tint is the head's own surface, not a band laid over the card.
        let chrome = self.code_fill.lerp_exact(self.ink, 0.06);
        // The card lifts off the page by one step; its own box stays clear, so only the
        // shadow paints, rounded to the card's silhouette.
        let mut card = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background(Color::TRANSPARENT, Corners::all(CODE_CARD_RADIUS))
            .elevation(1);
        card = card
            .child(self.code_head(
                header,
                copy,
                language,
                open.as_ref(),
                leading,
                Self::code_group(block),
                source_form,
                width,
                chrome,
                &style,
                cx,
            ))
            .child(
                Decl::container(id.with("divider"), NodeKind::Box)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::fixed(1.0),
                        grow_x: Grow::Yes,
                        grow_y: Grow::No,
                    })
                    .background(self.code_fill.lerp_exact(self.ink, 0.14), Corners::ZERO),
            )
            .child(self.code_body_area(
                block,
                id,
                language,
                code_width,
                gutter,
                numbered,
                close_line.as_ref(),
                content_end,
                &indents,
                lines,
                source_form,
                width,
                line,
                &style,
                cx,
            ));
        if indent <= 0.0 {
            return card;
        }
        Decl::container(id.with("column"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed(outer),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .child(
                Decl::container(id.with("column").with("indent"), NodeKind::Box).size(SizeClass {
                    width: AxisSize::fixed(indent),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                }),
            )
            .child(card)
    }

    /// The column `range` writes, a tab reaching the next stop of four.
    fn code_prefix_width(&self, range: &Range<usize>, cx: &crate::Cx<'_, '_>) -> f32 {
        let source = &*self.document.source;
        let mut columns = 0usize;
        for byte in source.get(range.clone()).unwrap_or("").bytes() {
            columns += match byte {
                b'\t' => 4 - columns % 4,
                _ => 1,
            };
        }
        match columns {
            0 => 0.0,
            count => self.advance(&" ".repeat(count), self.code_text_style(), cx),
        }
    }

    /// The card's content surface: the code, its foot, and the gutter's own band.
    ///
    /// The band is the body's surface, not a layer over the code: it runs from the card's
    /// edge to the code's first column and over the body's whole height, so the card's inset
    /// never rims it in a second colour. The numbers keep their own box above it.
    #[allow(clippy::too_many_arguments)]
    fn code_body_area(
        &self,
        block: &Block,
        id: Id,
        language: Option<&str>,
        width: f32,
        gutter: f32,
        numbered: bool,
        close: Option<&Range<usize>>,
        content_end: usize,
        indents: &CardIndents,
        lines: usize,
        source_form: bool,
        card_width: f32,
        line: f32,
        style: &TextStyle,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let mut body = Decl::container(id.with("body"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            // The gutter's band and rule are placed against this box, not the card.
            .relative()
            .background(
                self.code_fill,
                Corners {
                    bl: CODE_CARD_RADIUS,
                    br: CODE_CARD_RADIUS,
                    ..Corners::ZERO
                },
            );
        if numbered {
            // The band holds the widest number with the same room on both sides, and stops at
            // the rule: from the rule on, the code's own fill is the background.
            body = body
                .child(
                    Decl::container(id.with("gutter"), NodeKind::Box)
                        .absolute()
                        .inset(Insets {
                            left: 0.0,
                            top: 0.0,
                            right: AUTO,
                            bottom: 0.0,
                        })
                        .size(SizeClass {
                            width: AxisSize::fixed(gutter),
                            height: AxisSize::hug(0.0, 0.0),
                            grow_x: Grow::No,
                            grow_y: Grow::No,
                        })
                        .background(
                            self.code_fill.lerp_exact(self.ink, 0.10),
                            Corners {
                                bl: CODE_CARD_RADIUS,
                                ..Corners::ZERO
                            },
                        ),
                )
                .child(
                    Decl::container(id.with("gutter-rule"), NodeKind::Box)
                        .absolute()
                        .inset(Insets {
                            left: gutter,
                            top: 0.0,
                            right: AUTO,
                            bottom: 0.0,
                        })
                        .size(SizeClass {
                            width: AxisSize::fixed(CODE_GUTTER_RULE),
                            height: AxisSize::hug(0.0, 0.0),
                            grow_x: Grow::No,
                            grow_y: Grow::No,
                        })
                        .background(self.code_fill.lerp_exact(self.ink, 0.14), Corners::ZERO),
                );
        }
        body = body
            .child(
                Decl::container(id.with("code"), NodeKind::Column)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::hug(0.0, 0.0),
                        grow_x: Grow::Yes,
                        grow_y: Grow::No,
                    })
                    .padding(Insets {
                        left: 0.0,
                        top: CODE_CARD_GAP,
                        right: CODE_CARD_PAD,
                        bottom: 0.0,
                    })
                    .child(self.code_bounds(
                        block,
                        id,
                        language,
                        width,
                        gutter,
                        numbered,
                        content_end,
                        &indents.body,
                        indents.strip,
                        line,
                        style,
                        cx,
                    )),
            )
            .child(self.code_foot(
                id.with("footer"),
                close,
                indents.close.as_ref(),
                Self::code_group(block),
                lines,
                source_form,
                card_width,
                line,
                style,
                cx,
            ));
        body
    }

    /// The card's head: an icon and the language on the left, copy on the right, or the
    /// opening fence while the block is edited. The two states share a stack, so the head
    /// takes the taller one's height and entering the block moves nothing; both are laid
    /// out, and the state that is not shown is transparent and inert.
    #[allow(clippy::too_many_arguments)]
    fn code_head(
        &self,
        id: Id,
        copy: Id,
        language: Option<&str>,
        fence: Option<&Range<usize>>,
        leading: Option<Leading>,
        group: u32,
        source_form: bool,
        width: f32,
        fill: Color,
        style: &TextStyle,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        // A fence that writes no marker has none to show, so the chrome stays the head.
        let shows_fence = source_form && fence.is_some();
        let label = language
            .map(ui_richtext::language::language_label)
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| cx.t(Msg::WIDGETS_CODE_PLAIN_TEXT).to_string());
        // A stack lays every child out at its measured size, so the chrome states the head's
        // own width: a fill inside a stack hugs its content and would leave copy mid-head.
        let mut chrome = Decl::container(id.with("chrome"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed((width - CODE_CARD_PAD * 1.5).max(0.0)),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .gap(6.0)
            .child(
                crate::Icon::new(id.with("icon"), "code", CODE_CARD_NOTE + 2.0, self.muted)
                    .resolve(),
            )
            .child(self.note_text(id.with("language"), label, self.muted))
            .child(
                Decl::container(id.with("head-space"), NodeKind::Row).size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::No,
                }),
            )
            .child(self.copy_button(copy, self.muted, cx));
        if shows_fence {
            chrome = chrome.opacity(0.0).disabled(true);
        }
        let mut head = Decl::container(id, NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: CODE_CARD_PAD,
                // The band keeps a little room around the copy button rather than hugging
                // it to its own edges; layout takes the taller state's height on top.
                top: 4.0,
                right: CODE_CARD_PAD * 0.5,
                bottom: 4.0,
            })
            .background(
                fill,
                Corners {
                    tl: CODE_CARD_RADIUS,
                    tr: CODE_CARD_RADIUS,
                    ..Corners::ZERO
                },
            )
            .stack_align_axes(
                ui_core::layout::OverlayAlign::Start,
                ui_core::layout::OverlayAlign::Center,
            );
        if let Some(fence) = fence {
            // Away from the caret the fence text keeps the head's room but names no
            // position, so the head lays out both states while only one can be edited.
            let mut run = TextRun::new(source_text(&self.document, fence), *style);
            if shows_fence {
                run.visibility = Visibility::Drawn;
            } else {
                // The chrome stands over the bytes that spell the fence: the whole head is
                // the fence line's surface, so a drag across it reaches those bytes.
                run.visibility = Visibility::Covered;
                let head = (width - CODE_CARD_PAD * 1.5).max(0.0);
                run.room = Some(Room::new(head, TextAlign::Start));
            }
            let frame = self.text_frame(
                id.with("fence"),
                shows_fence.then(|| Window::line(fence.clone())),
                vec![run],
                Vec::new(),
                self.code_ink,
                AxisSize::hug(0.0, f32::INFINITY),
                true,
                cx,
            );
            // A block that owns its indentation but spells none yet keeps the place the
            // first space typed at the fence would take, so the space is the block's.
            let members = match leading.as_ref() {
                Some(leading) if leading.owns && leading.range.is_empty() => {
                    vec![ui_core::text::Structure {
                        group,
                        range: 0..0,
                        shown: true,
                    }]
                }
                _ => Vec::new(),
            };
            let frame = with_structures(frame, members);
            head = match leading.filter(|leading| !leading.range.is_empty()) {
                // The indentation stands on the fence's own line, left of the card: both
                // are laid in one row, so the text sits at the height the fence has.
                Some(leading) if shows_fence => {
                    let view =
                        self.code_indent_view(id.with("indent"), &leading, group, true, style, cx);
                    head.child(
                        Decl::container(id.with("fence-row"), NodeKind::Row)
                            .size(SizeClass {
                                width: AxisSize::hug(0.0, 0.0),
                                height: AxisSize::hug(0.0, 0.0),
                                grow_x: Grow::No,
                                grow_y: Grow::No,
                            })
                            .child(view)
                            .child(frame),
                    )
                }
                Some(leading) => head
                    .child(self.code_indent_view(
                        id.with("indent"),
                        &leading,
                        group,
                        false,
                        style,
                        cx,
                    ))
                    .child(frame),
                None => head.child(frame),
            };
        }
        head.child(chrome)
    }

    /// The bytes before the fence as a view of their own, laid left of the card.
    ///
    /// The source form puts the block at its own column, so these bytes are text like the
    /// fence beside them: a caret stands in them, a selection highlights them, and the
    /// member the source spells is edited here. The box reaches back over the card's
    /// padding and takes its width back out of the margin, so the fence keeps its place
    /// while the bytes land where the card's surface does not reach. The card form keeps
    /// the view with no room of its own, so a press there still enters the block.
    fn code_indent_view(
        &self,
        id: Id,
        leading: &Leading,
        group: u32,
        visible: bool,
        style: &TextStyle,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let range = &leading.range;
        let mut run = TextRun::new(source_text(&self.document, range), *style);
        run.ink = Some(self.marker_ink);
        // The card stands for the column, so the bytes take none of their own until the
        // source form draws them.
        run.room = Some(Room::new(
            match visible {
                true => leading.ink,
                false => 0.0,
            },
            TextAlign::Start,
        ));
        if !visible {
            run.visibility = Visibility::Covered;
        }
        let window = match visible {
            true => Window::prefix(range.clone()),
            false => Window::line(range.clone()),
        };
        let span = leading.ink + CODE_CARD_PAD;
        let frame = self.text_frame(
            id,
            Some(window),
            vec![run],
            Vec::new(),
            self.code_ink,
            AxisSize::fixed(span),
            true,
            cx,
        );
        let members = if leading.owns {
            vec![ui_core::text::Structure {
                group,
                range: 0..range.len(),
                shown: true,
            }]
        } else {
            Vec::new()
        };
        with_structures(frame, members).margin(Insets {
            left: -span,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        })
    }

    /// The card's copy affordance: the engine's own Copy wording beside the usual mark. At
    /// rest it is bare, so the head reads as the language alone; the pill is a layer the
    /// engine fades in under the pointer and fades out after it.
    fn copy_button(&self, id: Id, ink: Color, cx: &crate::Cx<'_, '_>) -> Decl {
        let label = cx.t(Msg::WIDGETS_MENU_COPY);
        let mut button = Decl::container(id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .sense(Sense::HOVER | Sense::CLICK)
            .cursor(CursorShape::PointingHand)
            // The pill is out of flow and fills the button, so it takes the button's own
            // box as its containing block rather than a slot beside the mark.
            .relative()
            .cross_align(0.0)
            .cross_pos(0.5)
            .gap(4.0)
            .padding(Insets {
                left: 6.0,
                top: 2.0,
                right: 6.0,
                bottom: 2.0,
            });
        if cx.core().hovered_within(id) {
            button = button.child(
                Decl::container(id.with("pill"), NodeKind::Box)
                    .absolute()
                    .inset(Insets::new(0.0, 0.0, 0.0, 0.0))
                    .background(self.code_fill.lerp_exact(self.ink, 0.1), Corners::all(5.0))
                    .enter(Presence::fade(CODE_COPY_FADE, 0.0))
                    .exit(Presence::fade(CODE_COPY_FADE, 0.0)),
            );
        }
        button
            .child(crate::Icon::new(id.with("icon"), "content_copy", 13.0, ink).resolve())
            .child(self.note_text(id.with("label"), label.to_string(), ink))
    }

    /// The runs a fenced block draws, highlighted by the page when it has a grammar.
    fn code_runs(&self, language: Option<&str>, text: &str, style: TextStyle) -> Vec<TextRun> {
        match self.highlight(language, text, style, None) {
            Some(runs) => runs,
            None => vec![TextRun::new(text.to_owned(), style)],
        }
    }

    /// The card's body: the code without the fences. It is the same view in both states —
    /// read, the page's highlighter colours it; editing, the caret and selection land in it.
    #[allow(clippy::too_many_arguments)]
    fn code_body(
        &self,
        block: &Block,
        id: Id,
        language: Option<&str>,
        width: f32,
        content_end: usize,
        indents: &[LineIndent],
        strip: usize,
        style: &TextStyle,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let (range, lines) = self.code_content(block, content_end);
        let text = source_text(&self.document, &range);
        let prefixes = local_prefixes(indents, &range, strip);
        let runs = cover(self.code_runs(language, &text, *style), &prefixes);
        let members = self.code_members(block, indents, &range);
        // The box starts at the code's column, past the gutter, so a broken line starts
        // where the numbers end rather than under them.
        let mut decl = with_structures(
            self.text_frame(
                id,
                Some(Window::line(range)),
                runs,
                Vec::new(),
                self.code_ink,
                match self.code.wrap {
                    true => AxisSize::fixed(width),
                    // Without wrap the line is as wide as it is: the card scrolls sideways.
                    false => AxisSize::hug(0.0, f32::INFINITY),
                },
                !self.code.wrap,
                cx,
            ),
            members,
        );
        // An empty fence has no code line, but a text node still owns one line box; that
        // box would make the card a line taller than the source it replaces.
        if lines == 0 {
            decl.size_class.height = AxisSize::fixed(0.0);
        }
        decl
    }

    /// The number column, laid over the code's left edge and outside its sideways scroll.
    ///
    /// It is the code's own text with every run `Covered`: the same bytes, the same width and
    /// the same style, so its line boxes fall exactly where the code's do and the numbers
    /// line up however a line was broken. Only the numbers paint. Its box starts at the
    /// row's edge, `indent` in from it the code's first column: the numbers stand in that
    /// indent, one number gap before the text.
    #[allow(clippy::too_many_arguments)]
    fn code_gutter(
        &self,
        block: &Block,
        id: Id,
        language: Option<&str>,
        width: f32,
        indent: f32,
        content_end: usize,
        indents: &[LineIndent],
        strip: usize,
        style: &TextStyle,
    ) -> Decl {
        let (range, lines) = self.code_content(block, content_end);
        let content = source_text(&self.document, &range);
        let mut runs = self.code_runs(language, &content, *style);
        for run in &mut runs {
            run.visibility = Visibility::Covered;
        }
        // The same covered indentation as the body's, or a broken line's number would sit
        // three spaces from the line it numbers.
        let prefixes = local_prefixes(indents, &range, strip);
        let mut runs = cover(runs, &prefixes);
        if runs.is_empty() {
            runs.push(TextRun::new(String::new(), *style));
        }
        let mut text = Decl::container(id, NodeKind::Text)
            .absolute()
            .inset(Insets {
                left: 0.0,
                top: 0.0,
                right: AUTO,
                bottom: AUTO,
            })
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: match lines {
                    0 => AxisSize::fixed(0.0),
                    _ => AxisSize::hug(0.0, 0.0),
                },
                grow_x: Grow::No,
                grow_y: Grow::No,
            });
        // A paint-only twin: it names no buffer position, so it joins no input and draws no
        // caret or selection of the document's.
        text.data = NodeData::Text {
            ink: self.muted,
            block: std::rc::Rc::new(TextBlock {
                runs,
                indent,
                hanging: indent,
                line_numbers: Some(LineNumbers {
                    start: 1,
                    ink: self.muted,
                    size: style.size,
                    gap: CODE_NUMBER_GAP,
                }),
                ..Default::default()
            }),
            caret: None,
            selection: None,
            composing: None,
            nowrap: !self.code.wrap,
        };
        text
    }

    /// The body in the frame its options ask for: a gutter beside it, a sideways scroll when
    /// the lines do not wrap, and a capped viewport when the card may not grow past its height.
    #[allow(clippy::too_many_arguments)]
    fn code_bounds(
        &self,
        block: &Block,
        id: Id,
        language: Option<&str>,
        width: f32,
        gutter: f32,
        numbered: bool,
        content_end: usize,
        indents: &[LineIndent],
        strip: usize,
        line: f32,
        style: &TextStyle,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        // Where the code's first column stands in the row: past the band and its rule, with
        // the number gap on the code's side of the rule, or the card's own inset when there
        // are no numbers.
        let text_x = match numbered {
            true => CODE_NUMBER_PAD + gutter,
            false => CODE_CARD_PAD,
        };
        let text_width = (width - text_x).max(0.0);
        let mut body = self.code_body(
            block,
            id.with("text"),
            language,
            text_width,
            content_end,
            indents,
            strip,
            style,
            cx,
        );
        if !self.code.wrap {
            // The line is laid out unbroken, so the strip is what carries the overflow. It
            // takes the wheel itself, as the scroll control's frame does, or the card would
            // clip the line with no way to reach its end.
            let strip_id = id.with("sideways");
            let mut decl = Decl::container(strip_id, NodeKind::Row)
                .size(SizeClass {
                    width: AxisSize::fixed(text_width),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .sense(Sense::HOVER | Sense::SCROLL_OWNED)
                .clip(Clip {
                    x: Overflow::Scroll,
                    y: Overflow::Hidden,
                })
                .child(body);
            // A bar is the only handle the sideways range has. It floats over the last line
            // and the engine shows it only while the pointer or a wheel is on the strip.
            if let Some(geom) = cx.core().scroll_of(strip_id)
                && geom.extent_w > geom.viewport_w
            {
                let thickness = 4.0;
                let (size, start) = crate::scroll::thumb_geometry(
                    geom.viewport_w,
                    geom.extent_w,
                    geom.offset_x,
                    25.0,
                );
                let y = (geom.viewport_h - thickness).max(0.0);
                if let Some(params) = decl.data.params_mut() {
                    params.scrollbar = Some(ui_core::Scrollbar {
                        vertical: None,
                        horizontal: Some(ui_core::ScrollbarBar {
                            thumb: ui_core::geom::Rect::new(start, y, size, thickness),
                            track: ui_core::geom::Rect::new(0.0, y, geom.viewport_w, thickness),
                            track_fill: Color::TRANSPARENT,
                            thumb_fill: Color {
                                a: 0.35,
                                ..self.code_ink
                            },
                            track_corners: Corners::all(thickness * 0.5),
                            thumb_corners: Corners::all(thickness * 0.5),
                        }),
                        fade: true,
                    });
                }
            }
            body = decl;
        }
        let text = Decl::container(id.with("text-column"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: text_x,
                ..Insets::default()
            })
            .child(body);
        let mut bounds = match numbered {
            // The code starts past the ruler's rule, so the strip's clip begins there and no
            // line is ever drawn under the numbers; the twin stays at the row's edge, its own
            // box as wide as the row, so its line boxes fall where the code's do.
            true => Decl::container(id.with("row"), NodeKind::Stack)
                .relative()
                .size(SizeClass {
                    width: AxisSize::fixed(width),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .child(text)
                .child(self.code_gutter(
                    block,
                    id.with("row").with("numbers"),
                    language,
                    width,
                    text_x,
                    content_end,
                    indents,
                    strip,
                    style,
                )),
            false => text,
        };
        if let Some(cap) = self.code.max_height {
            let height = match cap {
                CodeHeight::Lines(count) => line * count as f32,
                CodeHeight::Px(pixels) => pixels,
            };
            // The control's own scroller: content-sized up to the cap, then a viewport with
            // the engine owning the offset and the bar saying how much is below.
            bounds = crate::Scroll::vertical(id.with("capped"), height)
                .child(bounds)
                .resolve_in(cx);
        }
        bounds
    }

    /// The card's foot: the count in an inner rect at the right, or the closing fence while
    /// the block is edited. Both states share a stack, so the foot takes the taller one's
    /// height, never clips the count, and entering the block moves nothing.
    #[allow(clippy::too_many_arguments)]
    fn code_foot(
        &self,
        id: Id,
        close: Option<&Range<usize>>,
        value: Option<&Range<usize>>,
        group: u32,
        lines: usize,
        source_form: bool,
        width: f32,
        line: f32,
        style: &TextStyle,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        // The foot carries no surface of its own: the card's fill runs to its edge, and the
        // band is the room the closing fence or the count needs.
        let shows_fence = source_form && close.is_some();
        let mut chip = Decl::container(id.with("count-row"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed((width - CODE_CARD_PAD * 1.5).max(0.0)),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(
                Decl::container(id.with("foot-space"), NodeKind::Row).size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::No,
                }),
            );
        if lines > 0 {
            let mut args = ui_i18n::Args::new();
            args.set("count", lines as i64);
            let count = cx.t_args(Msg::WIDGETS_CODE_LINE_COUNT, &args).to_string();
            // The count is a note, not a badge: it owns no surface, so it sits at the
            // foot's edge without drawing the eye away from the code.
            chip = chip.child(
                Decl::container(id.with("count-box"), NodeKind::Row)
                    .padding(Insets {
                        left: 6.0,
                        top: 2.0,
                        right: 6.0,
                        bottom: 2.0,
                    })
                    .child(self.note_text(id.with("count"), count, self.muted)),
            );
        }
        if shows_fence {
            chip = chip.opacity(0.0).disabled(true);
        }
        let mut foot = Decl::container(id, NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(line, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: CODE_CARD_PAD,
                top: 4.0,
                right: CODE_CARD_PAD * 0.5,
                bottom: 6.0,
            })
            .stack_align_axes(
                ui_core::layout::OverlayAlign::Start,
                ui_core::layout::OverlayAlign::Center,
            )
            .child(chip);
        if let Some(close) = close {
            // The closing line is never the block's first, so its member only follows the
            // block's own indentation; a change to the opening line's reaches it too.
            // The closing line's share of the value, clamped to the bytes the foot draws:
            // a fence that ends a code line has none, and the whole line is the fence.
            let value = value
                .map(|value| {
                    value.start.max(close.start)..value.end.max(close.start).min(close.end)
                })
                .unwrap_or(close.start..close.start);
            let text = source_text(&self.document, close);
            let form = if shows_fence {
                Visibility::Drawn
            } else {
                Visibility::Covered
            };
            let container = 0..value.start - close.start;
            let mut runs = Vec::with_capacity(3);
            if !container.is_empty() {
                let mut run = TextRun::new(text[container.clone()].to_owned(), *style);
                run.visibility = Visibility::Covered;
                run.room = Some(Room::new(0.0, TextAlign::Start));
                runs.push(run);
            }
            let own = value.start - close.start..value.end - close.start;
            let mut member = TextRun::new(text[own.clone()].to_owned(), *style);
            member.visibility = Visibility::Covered;
            member.room = Some(Room::new(0.0, TextAlign::Start));
            runs.push(member);
            let mut rest = TextRun::new(text[own.end..].to_owned(), *style);
            rest.visibility = form;
            if !shows_fence {
                let foot = (width - CODE_CARD_PAD * 1.5).max(0.0);
                let taken = self.advance(&text[..own.end], *style, cx);
                rest.room = Some(Room::new((foot - taken).max(0.0), TextAlign::Start));
            }
            runs.push(rest);
            let frame = self.text_frame(
                id.with("fence"),
                shows_fence.then(|| Window::line(close.clone())),
                runs,
                Vec::new(),
                self.code_ink,
                AxisSize::hug(0.0, f32::INFINITY),
                true,
                cx,
            );
            foot = foot.child(with_structures(
                frame,
                vec![ui_core::text::Structure {
                    group,
                    range: own,
                    shown: false,
                }],
            ));
        }
        foot
    }

    /// The code's bytes without the final line break, and how many lines they hold.
    ///
    /// The body starts at its first line's beginning, not at the first code byte: the
    /// containers' columns and the block's indentation stand there, covered by the card.
    /// It stops where the closing fence's line starts, because the foot draws that line.
    fn code_content(&self, block: &Block, end: usize) -> (Range<usize>, usize) {
        let range = ui_richtext::scene::line_start(&*self.document.source, block.span.start)
            ..end.min(block.span.end);
        let text = self.document.source.get(range.clone()).unwrap_or("");
        let drawn = without_break(&*self.document.source, &range);
        (drawn, text.lines().count())
    }

    /// Where the closing fence's line starts, when the fence starts that line.
    ///
    /// A fence the source left at the end of the last code line has no line of its own, so
    /// it answers its own offset; a fence after a container's prefix begins the line after
    /// it, and the bytes from there to the fence are the block's indentation.
    fn close_line_start(&self, ancestors: &[&Block], at: usize) -> usize {
        let source = &*self.document.source;
        let start = ui_richtext::scene::line_start(source, at);
        let line = start..ui_richtext::scene::line_end(source, at);
        let container = ui_richtext::scene::container_end(&self.document, ancestors, &line);
        let prefix = source.get(container..at).unwrap_or("");
        match container <= at && prefix.bytes().all(|byte| byte == b' ' || byte == b'\t') {
            true => start,
            false => at,
        }
    }

    /// The bytes before the opening fence no cell of the row draws.
    ///
    /// A container's cell draws the column its marker reserves, so those bytes need no
    /// view of their own. The block owns the value when the line writes no marker: then
    /// the whole run before the fence is its indentation, whether or not a cell reserves
    /// the column. An indented block has no fence and no value the source spells, so it
    /// owns nothing here even when its bytes are unclaimed.
    fn code_prefix(&self, block: &Block, row: &Row, ancestors: &[&Block]) -> Option<Leading> {
        if !matches!(
            block.kind,
            ui_richtext::BlockKind::CodeBlock { fenced: true, .. }
        ) {
            return None;
        }
        let source = &*self.document.source;
        let line_start = ui_richtext::scene::line_start(source, block.full.start);
        let line = line_start..ui_richtext::scene::line_end(source, block.full.start);
        let covered = row
            .cells
            .iter()
            .filter_map(|cell| cell.source.as_ref())
            .map(|range| range.end)
            .max()
            .unwrap_or(line_start)
            .clamp(line_start, block.full.start);
        // Empty at the fence: the block owns the place but spells nothing there yet.
        let owns =
            ui_richtext::scene::code_value(&self.document, ancestors, &line).start <= line_start;
        if covered >= block.full.start && !owns {
            return None;
        }
        Some(Leading {
            range: covered..block.full.start,
            ink: 0.0,
            owns,
        })
    }

    /// Where the line holding `at` begins, when only spaces stand before `at`.
    fn line_begin(&self, at: usize) -> usize {
        let source = &*self.document.source;
        let line = ui_richtext::scene::line_start(source, at);
        let prefix = source.get(line..at).unwrap_or("");
        if prefix.bytes().all(|byte| byte == b' ') {
            line
        } else {
            at
        }
    }

    /// Each card line's containers' prefix, and the block's own spaces and tabs after it.
    fn card_lines(&self, ancestors: &[&Block], range: &Range<usize>) -> Vec<LineIndent> {
        let source = &*self.document.source;
        let mut out = Vec::new();
        let mut start = range.start;
        while start <= range.end && start <= source.len() {
            let end = ui_richtext::scene::line_end(source, start).min(range.end);
            let line = start..end;
            let (container, spaces) =
                ui_richtext::scene::code_line_indent(&self.document, ancestors, &line);
            out.push(LineIndent {
                container,
                spaces,
                value: ui_richtext::scene::code_value(&self.document, ancestors, &line),
            });
            if end >= range.end {
                break;
            }
            start = end + 1;
        }
        out
    }

    /// One card line's split, for the fence lines the body does not cover.
    fn card_line_indent(&self, ancestors: &[&Block], at: usize) -> LineIndent {
        let source = &*self.document.source;
        let line =
            ui_richtext::scene::line_start(source, at)..ui_richtext::scene::line_end(source, at);
        let (container, spaces) =
            ui_richtext::scene::code_line_indent(&self.document, ancestors, &line);
        LineIndent {
            container,
            spaces,
            value: ui_richtext::scene::code_value(&self.document, ancestors, &line),
        }
    }

    /// The members of the block's indentation group, in the body text's own offsets.
    ///
    /// An indented block has no shared value: its four columns are the type's threshold,
    /// not a field the source spells alike on every line, so it carries no member.
    fn code_members(
        &self,
        block: &Block,
        indents: &[LineIndent],
        range: &Range<usize>,
    ) -> Vec<ui_core::text::Structure> {
        if !matches!(
            block.kind,
            ui_richtext::BlockKind::CodeBlock { fenced: true, .. }
        ) {
            return Vec::new();
        }
        let group = Self::code_group(block);
        indents
            .iter()
            .filter(|line| line.value.start >= range.start && line.value.end <= range.end)
            .map(|line| ui_core::text::Structure {
                group,
                range: line.value.start - range.start..line.value.end - range.start,
                shown: false,
            })
            .collect()
    }

    /// The group the lines of `block` share their indentation under.
    fn code_group(block: &Block) -> u32 {
        block.full.start as u32
    }

    /// The opening and closing fence lines of a fenced block, when it has them.
    ///
    /// An indented block has no fences: its first and last lines are code, even when they
    /// read like a fence, because only a line indented at most three columns opens one.
    fn code_fences(&self, block: &Block) -> (Option<Range<usize>>, Option<Range<usize>>) {
        let source = &*self.document.source;
        if !matches!(
            block.kind,
            ui_richtext::BlockKind::CodeBlock { fenced: true, .. }
        ) {
            return (None, None);
        }
        let Some(text) = source.get(block.full.clone()) else {
            return (None, None);
        };
        let opening = text.trim_start_matches([' ', '\t']);
        let Some(fence) = opening.chars().next().filter(|c| matches!(c, '`' | '~')) else {
            return (None, None);
        };
        let open_run = opening.chars().take_while(|c| *c == fence).count();
        let open_end = text.find('\n').unwrap_or(text.len());
        let mut end = text.len();
        while end > 0 && matches!(text.as_bytes().get(end - 1), Some(b'\n' | b'\r')) {
            end -= 1;
        }
        let line_start = text[..end].rfind('\n').map_or(0, |at| at + 1);
        // The opening line is drawn from the fence on: the bytes before it are the block's
        // indentation, a view of their own to the left of the card.
        let open = block.full.start..block.full.start + open_end;
        // A closing fence starts its line, unless the break before it was deleted: then it
        // ends the last code line, and the card still draws it as a fence of its own. The
        // bytes it is made of stay where the source put them, so no byte is drawn twice and
        // no byte is invented for the line it stands on.
        let mut fence_end = end;
        while fence_end > line_start && matches!(text.as_bytes()[fence_end - 1], b' ' | b'\t') {
            fence_end -= 1;
        }
        let mut fence_start = fence_end;
        while fence_start > line_start && text.as_bytes()[fence_start - 1] == fence as u8 {
            fence_start -= 1;
        }
        let run = fence_end - fence_start;
        let spans_line = fence_start == line_start || text.as_bytes()[fence_start - 1] == b' ';
        let closes = fence_start > open_end && run >= open_run && spans_line;
        // The line runs to the block's end, so the spaces after a fence stay a view's bytes,
        // and it begins where its own line does while the fence starts that line.
        let close = closes.then(|| {
            let at = block.full.start + fence_start;
            self.line_begin(at)..block.full.start + end
        });
        (Some(open), close)
    }

    fn code_text_style(&self) -> TextStyle {
        TextStyle::new(self.size * 0.93).family(Family::Generic(GenericFamily::Monospace))
    }

    /// One code line's height, from the same measurer the body lays out with.
    fn code_line_height(&self, cx: &crate::Cx<'_, '_>) -> f32 {
        let block = TextBlock::plain("X", self.code_text_style());
        cx.text_size(&block, f32::INFINITY)
            .map(|size| size.h)
            .unwrap_or(self.size * 0.93 * 1.4)
    }

    /// A head or foot note at the card's own small size, which never wraps.
    fn note_text(&self, id: Id, text: String, ink: Color) -> Decl {
        // A `nowrap` leaf's width is its hint's, so a `hug(0, 0)` note measured zero wide and
        // spilled out of the card; the text control states that rule, so ask it instead.
        crate::Text::new(id, text, CODE_CARD_NOTE, ink)
            .nowrap()
            .resolve()
    }

    /// A display formula: typeset while the caret is away, its own source while inside.
    fn math_block(
        &self,
        block: &Block,
        id: Id,
        width: f32,
        form: ui_richtext::Form,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        // The scene reveals a fence construct the caret touches; a display formula shows
        // its fences then, around the typeset payload rather than instead of it.
        let revealed = form == ui_richtext::Form::Source;
        if self.editable && self.math_editing == MathEditing::Structural {
            // The payload alone: the fences around it stay the document's bytes, and the
            // editor's serialized source replaces exactly those.
            let payload = self.math_payload(block);
            let source = self
                .document
                .source
                .get(payload.clone())
                .unwrap_or_default()
                .to_owned();
            // The payload's last byte is a fence's first when the closing `$$` shares its
            // line: that caret position belongs to the fence's view, the later one, and a
            // formula that also drew it stood twice on one caret. Past a break nobody else
            // draws the position, so the formula keeps it.
            let owns_end = payload.end >= self.document.source.len()
                || ui_core::text::break_len(&*self.document.source, payload.end) > 0;
            let formula =
                crate::math::Math::new(id.with("formula"), source, self.size * 1.15, self.ink)
                    .display()
                    .editable()
                    .in_text(self.id, payload)
                    .caret_ends(owns_end)
                    .resolve_in(cx);
            let mut decl = Decl::container(id, NodeKind::Column).size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            });
            // A display formula typeset alone reads as the whole line; its fences are what
            // say where the bytes the caret is in begin and end. Away from the block they are
            // not drawn, but they still stand as views: a press or a drag past the payload
            // must reach the bytes it does not draw, or the last characters of a document
            // ending in `$$` are out of range until the caret has been inside once.
            decl = decl.child(self.math_fence(id.with("fence-open"), block, true, revealed, cx));
            decl = decl.child(formula);
            decl = decl.child(self.math_fence(id.with("fence-close"), block, false, revealed, cx));
            return decl;
        }
        if revealed {
            // The whole construct, fences included: the payload it typesets starts after
            // the opening `$$` and is no longer the source a reader needs to see.
            let source = without_break(&*self.document.source, &block.full);
            let text = source_text(&self.document, &source);
            let style =
                TextStyle::new(self.size * 1.15).family(Family::Generic(GenericFamily::Monospace));
            return self.text_frame(
                id,
                Some(Window::line(source)),
                vec![TextRun::new(text, style)],
                Vec::new(),
                self.link_ink,
                AxisSize::fixed(width),
                false,
                cx,
            );
        }
        let source: String = block
            .pieces()
            .iter()
            .map(|piece| piece.text.as_ref())
            .collect();
        let mut decl = crate::math::Math::new(id, source.trim(), self.size * 1.15, self.ink)
            .display()
            .resolve_in(cx);
        decl.size_class.width = AxisSize::fixed(width);
        decl
    }

    /// The payload's own bytes: the region the parser hands the formula, without the
    /// whitespace that separates it from the fences. Offsets into it are the editor's, so
    /// the formula's range and its source name the same bytes.
    fn math_payload(&self, block: &Block) -> Range<usize> {
        let source = &*self.document.source;
        let Some(text) = source.get(block.span.clone()) else {
            return block.span.clone();
        };
        let start = block.span.start + (text.len() - text.trim_start().len());
        let end = block.span.end - (text.len() - text.trim_end().len());
        start..end.max(start)
    }

    /// The bytes of one fence, and the byte they start at: the `$$` without the padding
    /// that separates them from the payload.
    fn math_fence_bytes(&self, block: &Block, opening: bool) -> (usize, String) {
        let source = &*self.document.source;
        let payload = self.math_payload(block);
        let range = if opening {
            block.full.start..payload.start
        } else {
            payload.end..block.full.end
        };
        let text = source.get(range.clone()).unwrap_or_default();
        let start = range.start + (text.len() - text.trim_start().len());
        (start, text.trim().to_owned())
    }

    /// One fence of a display formula, drawn where the source writes it.
    ///
    /// The `$$` is a view like every other line of the buffer: the caret can stand on it,
    /// a selection can cover it, and a key typed there edits the document's bytes. Only
    /// the drawn characters name a byte; the padding and the line break around them stay
    /// gaps, which is the same rule every other hidden byte follows.
    ///
    /// While the block draws no fence — the caret is away — the view stays and takes no
    /// space: its bytes are then reachable by a press or a drag, which is what reveals it.
    fn math_fence(
        &self,
        id: Id,
        block: &Block,
        opening: bool,
        drawn: bool,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let (start, text) = self.math_fence_bytes(block, opening);
        let style =
            TextStyle::new(self.size * 1.15).family(Family::Generic(GenericFamily::Monospace));
        let mut run = TextRun::new(text, style);
        run.visibility = match drawn {
            true => Visibility::Drawn,
            false => Visibility::Hidden,
        };
        if !drawn {
            let mut decl = Decl::container(id, NodeKind::Text).size(SizeClass {
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::fixed(0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            });
            if self.editable {
                decl.data = NodeData::Text {
                    ink: Color::TRANSPARENT,
                    block: std::rc::Rc::new(TextBlock {
                        runs: vec![run],
                        input_base: Some(start),
                        ..Default::default()
                    }),
                    caret: None,
                    selection: None,
                    composing: None,
                    nowrap: true,
                };
            }
            return decl;
        }
        self.text_frame(
            id,
            Some(Window::line(start..start + run.text.len())),
            vec![run],
            Vec::new(),
            self.link_ink,
            AxisSize::hug(0.0, f32::INFINITY),
            true,
            cx,
        )
    }

    /// The picture while the caret is away and its source while inside; both take the
    /// scaled height, so entering it never moves the blocks below. No texture means source.
    fn image_block(
        &self,
        block: &Block,
        id: Id,
        width: f32,
        destination: &str,
        form: ui_richtext::Form,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let source = block.span.clone();
        let text = source_text(&self.document, &source);
        let texture = self.image.and_then(|image| image(destination));
        let height =
            texture.map(|texture| width * texture.height as f32 / texture.width.max(1) as f32);
        if let Some(texture) = texture
            && form == ui_richtext::Form::Rendered
        {
            return self.image_view(id, texture, width, height.unwrap_or(0.0), source);
        }
        self.image_source_view(id, source, text, width, height, cx)
    }

    /// The picture itself. In an editor it is also an input view with no drawn byte, so a
    /// press on it lands the caret on the image's first byte and the source takes over.
    fn image_view(
        &self,
        id: Id,
        texture: ImageTexture,
        width: f32,
        height: f32,
        source: Range<usize>,
    ) -> Decl {
        let kind = if self.editable {
            NodeKind::Text
        } else {
            NodeKind::Row
        };
        let mut decl = Decl::container(id, kind)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::fixed(height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background_image(texture.id, ImageFit::Contain);
        if self.editable {
            let mut run = TextRun::new("", TextStyle::new(self.size));
            run.visibility = Visibility::Hidden;
            decl.data = NodeData::Text {
                ink: Color::TRANSPARENT,
                block: std::rc::Rc::new(TextBlock {
                    runs: vec![run],
                    input_base: Some(source.start),
                    ..Default::default()
                }),
                caret: None,
                selection: None,
                composing: None,
                nowrap: false,
            };
        }
        decl
    }

    /// The image's own source line, shown while editing it or when the page has no texture.
    /// A fixed height is the room the picture would take, so the document below stays put.
    fn image_source_view(
        &self,
        id: Id,
        source: Range<usize>,
        text: String,
        width: f32,
        height: Option<f32>,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let run = TextRun::new(text, TextStyle::new(self.size));
        let input = self.editable.then(|| Window::line(source));
        let mut decl = self.text_frame(
            id,
            input,
            vec![run],
            Vec::new(),
            self.ink,
            AxisSize::fixed(width),
            false,
            cx,
        );
        if let Some(height) = height {
            decl.size_class.height = AxisSize::fixed(height);
        }
        decl
    }

    /// A task item's box: a square control of its own, holding down the atom's slot.
    ///
    /// The bytes `[ ]`/`[x]` are not drawn: they belong to the item's marker, which shows
    /// itself when the caret stands on it, so the source stays editable.
    fn task_box(&self, id: Id, checked: bool, top: f32, cx: &crate::Cx<'_, '_>) -> Decl {
        let side = TASK_BOX;
        let (fill, stroke, check) = if checked {
            (self.link_ink, self.link_ink, Some(Color::WHITE))
        } else {
            (Color::TRANSPARENT, self.muted, None)
        };
        let mut square = Decl::container(id.with("box"), NodeKind::Stack)
            .size(SizeClass::fixed(side, side))
            .stack_align(ui_core::layout::OverlayAlign::Center)
            .background(fill, Corners::all(4.0))
            .border(stroke, 1.5);
        if let Some(ink) = check {
            square = square.child(crate::Icon::new(id.with("check"), "check", 13.0, ink).resolve());
        }
        // The box sits on the first line's centre; its own width plus the padding after it
        // is the room the item's text starts from.
        let line = cx
            .text_size(
                &TextBlock::plain("X", TextStyle::new(self.size)),
                f32::INFINITY,
            )
            .map(|size| size.h)
            .unwrap_or(self.size * 1.3)
            .max(side);
        // `top` is the leading above the cell's first line, so the box centres on that line.
        Decl::container(id, NodeKind::Stack)
            .size(SizeClass::fixed(TASK_BOX + TASK_GAP, line + top))
            .padding(Insets {
                top,
                ..Insets::default()
            })
            // A press on the box is the box's, not the paragraph's: no caret moves.
            .sense(if self.editable {
                ui_core::Sense::CLICK
            } else {
                ui_core::Sense::NONE
            })
            // The box stands where the marker's ink did; the gap after it is the space the
            // source spells, so the item's text keeps its column either way.
            .stack_align_axes(
                ui_core::layout::OverlayAlign::Start,
                ui_core::layout::OverlayAlign::Center,
            )
            .child(square)
    }

    /// The byte a task box flips: the flag inside the `[ ]` of its marker.
    fn flag_at(&self, marker: &Range<usize>, source: &str) -> Option<usize> {
        let text = source.get(marker.clone())?;
        let open = text.find('[')?;
        let flag = *text.as_bytes().get(open + 1)?;
        matches!(flag, b' ' | b'x' | b'X').then_some(marker.start + open + 1)
    }

    /// One table: a grid while the caret is away, its whole source while it is inside.
    /// Editing a cell in place is not a v1 feature.
    fn table(
        &self,
        block: &Block,
        id: Id,
        width: f32,
        form: ui_richtext::Form,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let alignments: &[TableAlignment] = match &block.kind {
            BlockKind::Table { alignments } => alignments,
            _ => &[],
        };
        let column = Decl::container(id, NodeKind::Column).size(SizeClass {
            width: AxisSize::fixed(width),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        });
        if form == ui_richtext::Form::Source
            && let Some(projection) = ui_richtext::project(&self.document, block)
        {
            // The source state reads as code: monospace keeps the pipes in columns.
            let style = TextStyle::new(self.size).family(Family::Generic(GenericFamily::Monospace));
            return column.child(self.projected_view(
                id.with("text"),
                &projection,
                width,
                style,
                self.ink,
                cx,
            ));
        }
        column.child(self.table_grid(block, id.with("grid"), width, alignments, cx))
    }

    /// The grid a table reads as: a header row, a rule, then the body rows. Columns keep
    /// their widest cell and share the available width when together they exceed it.
    fn table_grid(
        &self,
        block: &Block,
        id: Id,
        width: f32,
        alignments: &[TableAlignment],
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let rows = block.children();
        let columns = alignments
            .len()
            .max(
                rows.iter()
                    .map(|row| row.children().len())
                    .max()
                    .unwrap_or(0),
            )
            .max(1);
        let header = rows
            .first()
            .is_some_and(|row| matches!(row.kind, BlockKind::TableRow { header: true }));
        // Build each cell's display block once: it is measured for the column, then placed.
        let empty = TextBlock::plain("", TextStyle::new(self.size));
        // A cell's view starts at its own bytes: a press names the byte under the pointer
        // and a vertical step into the grid lands on the row it came through, not on the
        // table's first line every time.
        let cells: Vec<Vec<(TextBlock, usize)>> = rows
            .iter()
            .enumerate()
            .map(|(row_index, row)| {
                let bold = header && row_index == 0;
                let table = block.span.start;
                (0..columns)
                    .map(|column| {
                        row.children()
                            .get(column)
                            .map(|cell| (self.table_cell_text(cell, bold, cx), cell.span.start))
                            .unwrap_or_else(|| (empty.clone(), table))
                    })
                    .collect()
            })
            .collect();
        let mut wants = vec![0.0f32; columns];
        for row in &cells {
            for (column, (text, _)) in row.iter().enumerate() {
                wants[column] = wants[column].max(self.table_cell_want(text, cx));
            }
        }
        // Each column keeps its max-content width; together they may not exceed the width
        // the block was given, so an over-wide table shares what is left proportionally.
        let padding = TABLE_CELL_PAD * 2.0;
        let available = (width - padding * columns as f32).max(0.0);
        let wanted: f32 = wants.iter().sum();
        let scale = if wanted > available && wanted > 0.0 {
            available / wanted
        } else {
            1.0
        };
        let widths: Vec<f32> = wants.iter().map(|want| want * scale + padding).collect();

        // Track order mirrors rows, with one fixed track for the rule after the header.
        let mut row_tracks = Vec::with_capacity(rows.len() + 1);
        let mut grid_row = Vec::with_capacity(rows.len());
        for (index, _) in rows.iter().enumerate() {
            if header && index == 1 {
                row_tracks.push(Track::Fixed(TABLE_SEPARATOR));
            }
            grid_row.push(row_tracks.len() as u32);
            row_tracks.push(Track::Auto);
        }
        let mut grid = Decl::container(id, NodeKind::Grid)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .tracks(TrackList {
                cols: widths.iter().copied().map(Track::Fixed).collect(),
                rows: row_tracks,
                auto_flow: AutoFlow::Row,
            })
            .gap(0.0);
        for (row_index, row) in cells.into_iter().enumerate() {
            for (column, (text, base)) in row.into_iter().enumerate() {
                let cell = self.table_cell(
                    id.with(("cell", row_index, column)),
                    text,
                    widths[column],
                    column_alignment(alignments.get(column)),
                    base,
                );
                grid = grid.child(cell.grid_area(grid_row[row_index], column as u32, 1, 1));
            }
        }
        if header && rows.len() > 1 {
            grid = grid.child(
                Decl::container(id.with("rule"), NodeKind::Box)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::fixed(TABLE_SEPARATOR),
                        grow_x: Grow::No,
                        grow_y: Grow::No,
                    })
                    .background(self.muted, Corners::ZERO)
                    .grid_area(1, 0, 1, columns as u32),
            );
        }
        grid
    }

    /// A cell's text as a reader sees it, with the header row in bold.
    fn table_cell_text(&self, cell: &Block, header: bool, cx: &crate::Cx<'_, '_>) -> TextBlock {
        let base = TextStyle::new(self.size);
        let mut text = ui_richtext::to_text_block(
            &self.document,
            cell,
            base,
            |run, marks| self.look_kinds(run, marks),
            |marks, source| self.inline_object(marks, source, base, cx),
        );
        if header {
            for run in &mut text.runs {
                run.style.weight = Weight::BOLD;
            }
        }
        text
    }

    /// A cell's max-content width, or an estimate when the host has no text measurer.
    fn table_cell_want(&self, text: &TextBlock, cx: &crate::Cx<'_, '_>) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        match cx.text_size(text, f32::INFINITY) {
            Some(size) => size.w,
            None => {
                let flat: String = text.runs.iter().map(|run| run.text.as_str()).collect();
                flat.chars().count() as f32 * self.size * 0.6
            }
        }
    }

    /// One grid cell: a fixed-width text leaf aligned inside its column. Its base is the
    /// table's first byte, so a press on it enters the table's source state.
    fn table_cell(
        &self,
        id: Id,
        mut text: TextBlock,
        width: f32,
        align: TextAlign,
        base: usize,
    ) -> Decl {
        if text.runs.is_empty() {
            text.runs.push(TextRun::new("", TextStyle::new(self.size)));
        }
        text.align = align;
        let mut decl = Decl::container(id, NodeKind::Text)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: TABLE_CELL_PAD,
                top: TABLE_CELL_PAD_Y,
                right: TABLE_CELL_PAD,
                bottom: TABLE_CELL_PAD_Y,
            });
        if self.editable {
            decl = decl.sense(Sense::CLICK);
        }
        decl.data = NodeData::Text {
            ink: self.ink,
            block: std::rc::Rc::new(TextBlock {
                input_base: self.editable.then_some(base),
                ..text
            }),
            caret: None,
            selection: None,
            composing: None,
            nowrap: false,
        };
        decl
    }

    /// The editing view of a projected container (a table's source state).
    fn projected_view(
        &self,
        id: Id,
        projection: &ui_richtext::Projection,
        width: f32,
        style: TextStyle,
        ink: Color,
        cx: &crate::Cx<'_, '_>,
    ) -> Decl {
        let focused = cx.core().focused() == Some(self.id);
        let global = self.selection_range(cx).unwrap_or(0..0);
        let mut runs = Vec::new();
        for shown in projection.shown(&global, focused) {
            // A construct's own bytes are what a form draws or hides; content is what the
            // table's cells read as, so it stays drawn whatever form its fence is in.
            let syntax = shown.run.syntax.is_some();
            let hidden = syntax && shown.form == ui_richtext::Form::Rendered;
            let mut run = TextRun::new(shown.run.text.clone(), style);
            run.visibility = match hidden {
                true => Visibility::Hidden,
                false => Visibility::Drawn,
            };
            if !hidden && syntax {
                run.ink = Some(self.marker_ink);
            }
            runs.push(run);
        }
        self.text_frame(
            id,
            Some(Window::line(projection.span.clone())),
            runs,
            Vec::new(),
            ink,
            AxisSize::fixed(width),
            false,
            cx,
        )
    }

    /// The block at `path`, the chain of child indices from the document root.
    fn block_at(&self, path: &[usize]) -> Option<&Block> {
        let mut blocks = self.document.blocks.as_slice();
        let mut block = None;
        for index in path {
            block = blocks.get(*index);
            blocks = block?.children();
        }
        block
    }

    /// The caret or selection in the document's offsets, or nothing while the editor holds
    /// neither — a caret at zero is not the same as no caret at all.
    fn selection_range(&self, cx: &crate::Cx<'_, '_>) -> Option<Range<usize>> {
        cx.core()
            .selection_for(self.id)
            .map(|selection| selection.range())
            .or_else(|| cx.core().caret_for(self.id).map(|at| at..at))
    }

    /// What each mark looks like: both modes read the same kinds, so a link is a link
    /// whether or not it can be edited.
    fn look(&self, run: &mut TextRun, source: Range<usize>) {
        let marks = self.document.marks_in(source);
        let kinds: Vec<&ui_richtext::MarkKind> = marks.iter().map(|mark| &mark.kind).collect();
        self.look_kinds(run, &kinds);
    }

    fn look_kinds(&self, run: &mut TextRun, marks: &[&ui_richtext::MarkKind]) {
        for mark in marks {
            match mark {
                ui_richtext::MarkKind::Emphasis => run.style.italic = true,
                ui_richtext::MarkKind::Strong => run.style.weight = Weight::BOLD,
                ui_richtext::MarkKind::Strikethrough => run.strikethrough = Decoration::Single,
                ui_richtext::MarkKind::Code => {
                    run.style.family = Family::Generic(GenericFamily::Monospace);
                    run.background = Some(self.code_fill);
                }
                ui_richtext::MarkKind::Link { .. } => {
                    run.ink = Some(self.link_ink);
                    run.underline = Decoration::Single;
                }
                ui_richtext::MarkKind::InlineMath => {
                    run.style.family = Family::Generic(GenericFamily::Monospace);
                    run.ink = Some(self.link_ink);
                }
                // An entity looks like the characters it stands for: the run's own style.
                ui_richtext::MarkKind::Entity => {}
                // The rendered form is an object (a raised label); the source keeps its bytes.
                ui_richtext::MarkKind::FootnoteReference { .. } => {}
                // v1 draws only a whole-paragraph image; an inline image keeps its alt text.
                ui_richtext::MarkKind::Image { .. } => {}
            }
        }
    }

    /// A formula mark becomes a box of its own: the math control lays its source out.
    fn inline_object(
        &self,
        marks: &[&ui_richtext::MarkKind],
        source: &str,
        base: TextStyle,
        cx: &crate::Cx<'_, '_>,
    ) -> Option<ui_core::InlineObject> {
        if marks
            .iter()
            .any(|mark| matches!(mark, ui_richtext::MarkKind::InlineMath))
        {
            // The id belongs to the node a standalone formula would build; inside a line
            // the atom is the identity, so any id does.
            return crate::math::Math::new(
                Id::str("markdown-inline-math"),
                source,
                self.size,
                self.link_ink,
            )
            .object_in(cx);
        }
        if !marks
            .iter()
            .any(|mark| matches!(mark, ui_richtext::MarkKind::Entity))
        {
            return None;
        }
        // The reading path already hands over the decoded character; the run's style is
        // what the character would have been drawn with.
        let mut run = TextRun::new("", base);
        self.look_kinds(&mut run, marks);
        self.glyph_object(
            source.to_owned(),
            run.style,
            run.ink.unwrap_or(self.ink),
            cx,
        )
    }

    /// An entity's character as an inline object measured at its own style, so the atom's
    /// box is exactly what the typesetter would give the text it stands for.
    fn glyph_object(
        &self,
        text: String,
        style: TextStyle,
        ink: Color,
        cx: &crate::Cx<'_, '_>,
    ) -> Option<ui_core::InlineObject> {
        let layout = cx.text_layout(&TextBlock::plain(text.clone(), style), f32::INFINITY)?;
        Some(ui_core::InlineObject::Glyphs {
            text,
            style,
            ink,
            size: layout.size(),
            baseline: layout.baseline,
            offset: 0.0,
        })
    }

    /// A fenced block's runs as the page's highlighter sees them, when it has a grammar
    /// for the language and its runs account for every byte of the content.
    fn highlight(
        &self,
        language: Option<&str>,
        content: &str,
        style: TextStyle,
        fill: Option<Color>,
    ) -> Option<Vec<TextRun>> {
        let mut runs = (self.code_style?)(language, content, style)?;
        let bytes: usize = runs.iter().map(|run| run.text.len()).sum();
        if bytes != content.len() {
            return None;
        }
        for run in &mut runs {
            if run.background.is_none() {
                run.background = fill;
            }
        }
        Some(runs)
    }
}

/// The style a row's text is set in.
fn text_style(size: f32, style: RowStyle) -> TextStyle {
    match style {
        RowStyle::Body => TextStyle::new(size),
        RowStyle::Heading(level) => {
            let scale = match level {
                1 => 1.7,
                2 => 1.35,
                _ => 1.15,
            };
            TextStyle::new(size * scale).weight(Weight::BOLD)
        }
    }
}

/// Where a cell's content stands: a number ends at its dot, so a list's numbers line up on
/// their dots instead of starting from a ragged left; everything else stands at its first
/// byte, where the source spells it.
fn cell_align(look: CellLook) -> TextAlign {
    match look {
        CellLook::Ordered => TextAlign::End,
        CellLook::Bullet | CellLook::Task { .. } | CellLook::Quote | CellLook::Indent => {
            TextAlign::Start
        }
    }
}

/// Where a quote's bar rounds off: only where its run of rows begins or ends. A bar that
/// continues into its neighbour is square, so consecutive rows draw one unbroken bar.
fn quote_corners(rows: &[Row], index: usize, cell: &Cell) -> Corners {
    let up = index
        .checked_sub(1)
        .is_some_and(|at| bar_continues(rows, at, &cell.owner));
    let down = bar_continues(rows, index + 1, &cell.owner);
    Corners {
        tl: if up { 0.0 } else { QUOTE_CAP },
        tr: if up { 0.0 } else { QUOTE_CAP },
        bl: if down { 0.0 } else { QUOTE_CAP },
        br: if down { 0.0 } else { QUOTE_CAP },
        ..Corners::ZERO
    }
}

/// Whether the row at `index` draws the bar of the quote at `owner`.
fn bar_continues(rows: &[Row], index: usize, owner: &[usize]) -> bool {
    rows.get(index).is_some_and(|row| {
        row.cells.iter().any(|cell| {
            cell.look == CellLook::Quote && cell.owner == owner && cell.form == Form::Rendered
        })
    })
}

/// A table column's alignment, or left when the delimiter row names none.
fn column_alignment(alignment: Option<&TableAlignment>) -> TextAlign {
    match alignment {
        Some(TableAlignment::Center) => TextAlign::Center,
        Some(TableAlignment::Right) => TextAlign::End,
        Some(TableAlignment::None | TableAlignment::Left) | None => TextAlign::Start,
    }
}

/// The bytes of a range without its trailing line break.
fn without_break(source: &str, range: &Range<usize>) -> Range<usize> {
    let text = source.get(range.clone()).unwrap_or("");
    let drawn = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    range.start..range.start + drawn.len()
}

/// Cover the bytes CommonMark removed from a code line.
///
/// The containers' columns and the block's own indentation stay in the line as ordinary
/// positions — a caret still names them, a drag still takes them — but take no room, so the
/// card reads flush. `prefixes` are the byte ranges to cover, in the runs' own offsets:
/// the containers' columns and the block's own indentation, which the card stands for.
fn cover(runs: Vec<TextRun>, prefixes: &[(usize, usize)]) -> Vec<TextRun> {
    if runs.is_empty() || prefixes.is_empty() {
        return runs;
    }
    let mut out = Vec::with_capacity(runs.len() + prefixes.len());
    let mut at = 0usize;
    for run in runs {
        let end = at + run.text.len();
        let mut cursor = at;
        for (low, high) in prefixes {
            let (low, high) = ((*low).max(at), (*high).min(end));
            if low >= high {
                continue;
            }
            if cursor < low {
                let mut piece = run.clone();
                piece.text = run.text[cursor - at..low - at].to_owned();
                out.push(piece);
            }
            let mut piece = run.clone();
            piece.text = run.text[low - at..high - at].to_owned();
            // The block's column is the card's own: the bytes stay in the line as positions
            // the caret can name, and take no room of their own.
            piece.visibility = Visibility::Covered;
            piece.room = Some(Room::new(0.0, TextAlign::Start));
            out.push(piece);
            cursor = high;
        }
        if cursor < end {
            let mut piece = run.clone();
            piece.text = run.text[cursor - at..].to_owned();
            out.push(piece);
        }
        at = end;
    }
    out
}

/// The covered ranges of a card's lines, in its text's own offsets.
fn local_prefixes(
    indents: &[LineIndent],
    range: &Range<usize>,
    strip: usize,
) -> Vec<(usize, usize)> {
    indents
        .iter()
        .filter_map(|line| {
            let low = line.container.start.max(range.start);
            let high = (line.spaces.start + (line.spaces.end - line.spaces.start).min(strip))
                .min(range.end);
            (low < high).then(|| (low - range.start, high - range.start))
        })
        .collect()
}

/// A text frame carrying the members of a block value its lines share.
fn with_structures(mut decl: Decl, structures: Vec<ui_core::text::Structure>) -> Decl {
    if let ui_core::NodeData::Text { block, .. } = &mut decl.data {
        std::rc::Rc::make_mut(block).structures = structures;
    }
    decl
}

/// The bytes of the source a range covers, or an empty string when it no longer fits.
fn source_text(document: &Document, source: &Range<usize>) -> String {
    document.source.get(source.clone()).unwrap_or("").to_owned()
}

impl Markdown {
    /// One `u64` over every input `decl` reads that does not come through the context.
    ///
    /// The document is behind an `Arc`, so its bytes cannot change under this handle; hashing
    /// them is what proves the handle, and it costs one pass over the source per frame.
    fn props(&self) -> u64 {
        let cap = match self.code.max_height {
            None => (0u8, 0.0),
            Some(CodeHeight::Lines(lines)) => (1, lines as f32),
            Some(CodeHeight::Px(px)) => (2, px),
        };
        let editing = match self.math_editing {
            MathEditing::Source => 0u8,
            MathEditing::Structural => 1,
        };
        let painter = self
            .selection_painter
            .map(|painter| painter.0 as *const dyn ui_core::SelectionPainter as *const () as usize);
        crate::fingerprint!(
            &*self.document.source,
            self.prepared.is_some(),
            self.width,
            self.size,
            self.ink,
            self.marker_ink,
            self.link_ink,
            self.code_ink,
            self.code_fill,
            self.muted,
            self.caret_ink,
            self.selection_fill,
            self.popover_fill,
            self.editable,
            editing,
            self.code.line_numbers,
            self.code.wrap,
            cap,
            self.code_style.map(|style| style as usize),
            self.image.map(|image| image as usize),
            painter,
        )
    }
}

impl crate::Widget for Markdown {
    fn decl(&self, cx: &crate::Cx<'_, '_>) -> Decl {
        self.resolve_in(cx)
    }

    fn cache_key(&self) -> Option<ui_core::CachedKey> {
        Some(ui_core::CachedKey::new(self.id, self.props()))
    }
}
