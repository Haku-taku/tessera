//! The document as the rows an editor draws.
//!
//! A row is one source line. Every byte belongs to exactly one row, and a row never spans
//! a line break, so vertical rhythm is the source's own: n blank lines are n rows, and no
//! block gap invents a line the source did not spell. Syntax is not removed from a row; it
//! is a run the row hides, which keeps every caret offset a source offset.
//!
//! This is the decoration model CodeMirror and ProseMirror use: the text is untouched and
//! a pure pass answers what each range looks like this frame. The widget above only turns
//! rows into declarations.

use std::ops::Range;

use crate::editor::{ConstructKind, Projection, Run, marker, project};
use crate::{Block, BlockKind, Document, MarkKind, Prepared};

/// The size a row's text is set at; the widget maps it to a point size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowStyle {
    Body,
    Heading(u8),
}

/// The document, cut into rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub rows: Vec<Row>,
}

/// One source line.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Child indices from the document root to the block this row draws; empty for a line
    /// no block owns.
    pub path: Vec<usize>,
    /// The line's bytes, its break excluded.
    pub span: Range<usize>,
    /// The bytes this row's own text draws: the line without its marker cells.
    pub content: Range<usize>,
    /// The markers standing at the line's start, outermost first.
    pub cells: Vec<Cell>,
    pub kind: RowKind,
}

/// One container marker at the start of a row, drawn in room of its own.
///
/// A cell belongs to the construct that writes it — a list's items share one cell width, a
/// quote's lines another — so the text after the cells starts at the same x on every line
/// of that construct. The marker's bytes stay a view of their own: the cell draws them in
/// source form and the stand-in of `look` in rendered form, and the width does not change
/// between the two, so nothing beside the marker moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// The construct whose cell this is: a quote's path, or the path of the list an item
    /// belongs to. Cells of one owner and one look share a width.
    pub owner: Vec<usize>,
    /// The bytes the cell shows. A line that continues the construct without writing its
    /// marker has none here, or holds the indentation the source spells in its place.
    pub source: Option<Range<usize>>,
    /// Whether this line writes the marker; a stand-in is drawn only where it does.
    pub written: bool,
    pub look: CellLook,
    /// What the rendered form draws as text: a bullet, a number. Empty for a shape.
    pub label: String,
    pub form: crate::Form,
}

impl Cell {
    /// The width-sharing key of this cell's owner, or none for a cell no construct owns.
    pub fn share(&self) -> Option<(&[usize], u8)> {
        match self.look {
            CellLook::Indent => None,
            CellLook::Bullet => Some((&self.owner, 0)),
            CellLook::Ordered => Some((&self.owner, 1)),
            CellLook::Task { .. } => Some((&self.owner, 2)),
            CellLook::Quote => Some((&self.owner, 3)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CellLook {
    /// A bullet stands in for the bytes.
    Bullet,
    /// The number as written; it aligns by its dot, so a list's numbers line up.
    Ordered,
    /// A box stands in for a task item's `[x]`.
    Task { checked: bool },
    /// A bar stands in for a quote's `> `, on every line of the quote.
    Quote,
    /// Indentation no container owns: room, and whatever bytes the source spells.
    Indent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    /// A leaf's inline content. One text node, syntax runs included.
    Text {
        style: RowStyle,
        parts: Vec<Part>,
    },
    /// A line no block owns: a blank line, a `>`-only line, the line after the last break.
    Empty {
        parts: Vec<Part>,
    },
    /// A fenced block, drawn as a card.
    Code {
        language: Option<String>,
        form: crate::Form,
    },
    /// A display formula.
    Math {
        form: crate::Form,
    },
    /// A paragraph holding one image.
    Image {
        destination: String,
        form: crate::Form,
    },
    Rule {
        form: crate::Form,
    },
    Table {
        form: crate::Form,
    },
    FrontMatter {
        form: crate::Form,
    },
}

/// One run of a row.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// The bytes this part covers.
    pub source: Range<usize>,
    /// The bytes as written. Always the source verbatim, so an offset in it is a source
    /// offset even when the part draws something else.
    pub text: String,
    pub role: Role,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// Text the document says; marks over it decide the look.
    Content,
    /// Syntax of a construct. In its source form it draws its bytes; rendered, it is the
    /// part of the construct the parsed form does not write, and takes no width.
    Syntax { form: crate::Form },
    /// Drawn as an object instead of its glyphs. The part's `text` is what the object
    /// draws: TeX, a decoded entity, a bullet, or nothing for a box.
    Object(ObjectKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    /// An inline formula.
    Math,
    /// An HTML entity's character.
    Entity,
    /// A footnote reference, drawn as its label.
    Footnote { label: String },
}

/// One block's place in the walk.
struct Entry<'a> {
    block: &'a Block,
    path: Vec<usize>,
    /// The enclosing containers, outermost first.
    ancestors: Vec<&'a Block>,
}

/// Cut `document` into rows.
///
/// `selection` is in document offsets; `focused` says whether the editor holds it. With
/// neither, the scene is what a reader sees: no syntax anywhere.
pub fn build(
    document: &Document,
    prepared: Option<&Prepared>,
    selection: Option<Range<usize>>,
    focused: bool,
) -> Scene {
    let source = &*document.source;
    let selection = selection.unwrap_or(0..0);
    let mut entries = Vec::new();
    collect(
        &document.blocks,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut entries,
    );

    let mut rows = Vec::new();
    let mut cursor = 0usize;
    for entry in entries {
        if entry.block.full.start > cursor {
            gap_rows(
                document,
                cursor..entry.block.full.start,
                &selection,
                &mut rows,
            );
        }
        row_of(document, prepared, &entry, &selection, focused, &mut rows);
        cursor = cursor.max(next_line_start(source, entry.block.full.end));
    }
    if cursor < source.len() {
        gap_rows(document, cursor..source.len(), &selection, &mut rows);
    }
    // A source that ends in a break leaves one more line; no block draws it, and the caret
    // after the final break stands there.
    if source.is_empty() || source.ends_with('\n') {
        let at = source.len();
        let cells = spacer_cells(document, at..at);
        rows.push(Row {
            path: Vec::new(),
            span: at..at,
            content: at..at,
            cells,
            kind: RowKind::Empty { parts: Vec::new() },
        });
    }
    Scene { rows }
}

/// Every block a row can come from, in source order.
fn collect<'a>(
    blocks: &'a [Block],
    path: &mut Vec<usize>,
    ancestors: &mut Vec<&'a Block>,
    out: &mut Vec<Entry<'a>>,
) {
    for (index, block) in blocks.iter().enumerate() {
        path.push(index);
        match &block.kind {
            BlockKind::Quote
            | BlockKind::BulletList
            | BlockKind::OrderedList { .. }
            | BlockKind::ListItem { .. }
            | BlockKind::FootnoteDefinition
            | BlockKind::TableRow { .. } => {
                ancestors.push(block);
                collect(block.children(), path, ancestors, out);
                ancestors.pop();
            }
            // Cells are drawn by their table; a cell that escaped one is text of its own.
            BlockKind::TableCell => {}
            _ => out.push(Entry {
                block,
                path: path.clone(),
                ancestors: ancestors.clone(),
            }),
        }
        path.pop();
    }
}

#[allow(clippy::too_many_arguments)]
fn row_of(
    document: &Document,
    prepared: Option<&Prepared>,
    entry: &Entry<'_>,
    selection: &Range<usize>,
    focused: bool,
    out: &mut Vec<Row>,
) {
    let block = entry.block;
    let source = &*document.source;
    // A rule's line is source like any other: the bytes before and after the marker stay
    // in its row, so a caret can reach them and a drag can take them.
    let span = match block.kind {
        BlockKind::Rule => line_start(source, block.full.start)..line_end(source, block.full.start),
        _ => trimmed(source, &block.full),
    };
    // An inline leaf may hold hard breaks; it is one text node per source line, so the
    // line is the unit everywhere and no row spans a break.
    let style = match &block.kind {
        BlockKind::Paragraph => Some(RowStyle::Body),
        BlockKind::Heading(level) => Some(RowStyle::Heading(*level)),
        _ => None,
    };
    if let Some(style) = style {
        let source = &*document.source;
        let projection = projection_of(document, prepared, block);
        for (index, line) in line_ranges(source, &span).into_iter().enumerate() {
            let first = if index == 0 {
                line_start(source, block.full.start)
            } else {
                line.start
            };
            // The row is the whole line, its prefix included: a block's first line starts at
            // its content, so the line itself begins earlier — and its end is the source
            // line's, not the frame's, because the spaces it trails are source too.
            let line_range = first..line_end(source, line.end);
            // Where the line's own text begins: the parser records it for the block's first
            // line, and a continuation line carries its prefix in the projection's marker run.
            let content_start = match index {
                0 => block.full.start.clamp(line_range.start, line_range.end),
                _ => projection.as_ref().map_or(line.start, |projection| {
                    prefix_end(document, entry, projection, &line)
                }),
            };
            // A continuation line's whole prefix is one marker construct, so every cell of
            // the line takes that construct's form.
            let line_form = match index {
                0 => None,
                _ => projection
                    .as_ref()
                    .and_then(|projection| marker_form(projection, line.start, selection, focused)),
            };
            let (cells, content) = line_cells(
                document,
                entry,
                &line_range,
                content_start,
                line_form,
                selection,
                focused,
            );
            let parts = projection
                .as_ref()
                .map(|projection| {
                    line_parts(
                        document,
                        projection,
                        &(content.start..line_range.end),
                        selection,
                        focused,
                    )
                })
                .unwrap_or_default();
            out.push(Row {
                path: entry.path.clone(),
                span: first..line_range.end,
                content,
                cells,
                kind: RowKind::Text { style, parts },
            });
        }
        return;
    }
    let form = block_form(document, prepared, block, selection, focused);
    let kind = match &block.kind {
        BlockKind::Paragraph | BlockKind::Heading(_) => unreachable!("handled above"),
        BlockKind::CodeBlock { language, .. } => RowKind::Code {
            language: language.clone(),
            form,
        },
        BlockKind::Math => RowKind::Math { form },
        BlockKind::Image { destination } => RowKind::Image {
            destination: destination.clone(),
            form,
        },
        BlockKind::Rule => RowKind::Rule { form },
        BlockKind::Table { .. } => RowKind::Table { form },
        BlockKind::FrontMatter => RowKind::FrontMatter { form },
        BlockKind::Quote
        | BlockKind::BulletList
        | BlockKind::OrderedList { .. }
        | BlockKind::ListItem { .. }
        | BlockKind::FootnoteDefinition
        | BlockKind::TableRow { .. }
        | BlockKind::TableCell => return,
    };
    out.push(Row {
        path: entry.path.clone(),
        span: span.clone(),
        content: span.clone(),
        cells: block_cells(document, entry, span.start),
        kind,
    });
}

/// How a block-level row is drawn: the form of the construct it projects to.
///
/// The first construct is the outermost one, which for a fence leaf and a table covers
/// the whole block; a thematic break projects to one marker and nothing else. A block
/// with no projection — one this editor does not model — stays rendered.
fn block_form(
    document: &Document,
    prepared: Option<&Prepared>,
    block: &Block,
    selection: &Range<usize>,
    focused: bool,
) -> crate::Form {
    if !focused {
        return crate::Form::Rendered;
    }
    let Some(projection) = projection_of(document, prepared, block) else {
        return crate::Form::Rendered;
    };
    projection
        .constructs
        .first()
        .map_or(crate::Form::Rendered, |construct| construct.form(selection))
}

/// One line of a leaf's projection, clipped to the line.
///
/// A run crossing the line's end is cut, because its bytes after the break belong to the
/// next row; the break itself is drawn by neither row, since a row is a line box of its
/// own. Clipping by byte offset is safe: every run text is its own source bytes, and both
/// the run edges and the line edges fall on character boundaries.
fn line_parts(
    document: &Document,
    projection: &Projection,
    line: &Range<usize>,
    selection: &Range<usize>,
    focused: bool,
) -> Vec<Part> {
    let runs = projection
        .shown(selection, focused)
        .into_iter()
        .filter_map(|shown| {
            let start = shown.run.source.start.max(line.start);
            let end = shown.run.source.end.min(line.end);
            if start >= end {
                return None;
            }
            let from = start - shown.run.source.start;
            let to = from + (end - start);
            let text = shown.run.text.get(from..to).unwrap_or("");
            let mut part = projection_part(document, shown.run, shown.form);
            part.source = start..end;
            if part.role == Role::Content || matches!(part.role, Role::Syntax { .. }) {
                part.text = text.into();
            }
            Some(part)
        });
    // A frame can stop before its line does: an item's frame ends at its text, and the
    // spaces the line trails are still source, so they belong to the row as content.
    let mut parts = Vec::new();
    let mut at = line.start;
    for part in runs {
        if part.source.start > at {
            parts.push(source_part(document, at..part.source.start));
        }
        at = at.max(part.source.end);
        parts.push(part);
    }
    if at < line.end {
        parts.push(source_part(document, at..line.end));
    }
    parts
}

/// Source bytes no construct claimed, shown as the author wrote them.
fn source_part(document: &Document, range: Range<usize>) -> Part {
    Part {
        text: document.source.get(range.clone()).unwrap_or("").into(),
        source: range,
        role: Role::Content,
    }
}

/// The end of the source line `at` sits on, its break excluded.
pub fn line_end(source: &str, at: usize) -> usize {
    let at = at.min(source.len());
    let rest = source.get(at..).unwrap_or("");
    let end = at + rest.find('\n').unwrap_or(rest.len());
    match source.as_bytes().get(end.saturating_sub(1)) {
        Some(b'\r') if end > at => end - 1,
        _ => end,
    }
}

/// Every line of a range, its break excluded.
fn line_ranges(source: &str, span: &Range<usize>) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = span.start;
    while start <= span.end {
        let end = source[start..span.end]
            .find('\n')
            .map_or(span.end, |offset| start + offset);
        lines.push(start..end);
        if end >= span.end {
            break;
        }
        start = end + 1;
    }
    lines
}

/// The block's projection: borrowed when the prepared pass already built it.
///
/// The clone this used to make copied every run's text, once per block per frame — for a
/// document that is already projected, the whole text was duplicated to draw it.
fn projection_of<'a>(
    document: &Document,
    prepared: Option<&'a Prepared>,
    block: &Block,
) -> Option<std::borrow::Cow<'a, Projection>> {
    match prepared.and_then(|prepared| prepared.get(block)) {
        Some(projection) => Some(std::borrow::Cow::Borrowed(projection)),
        None => project(document, block).map(std::borrow::Cow::Owned),
    }
}

/// One run of a leaf's projection.
fn projection_part(document: &Document, run: &Run, form: crate::Form) -> Part {
    let text = run.text.clone();
    let source = run.source.clone();
    if run.syntax.is_some() {
        return Part {
            source,
            text,
            role: Role::Syntax { form },
        };
    }
    // The payload draws as an object exactly while its construct is rendered; the source
    // form writes the bytes the object stands for.
    if form == crate::Form::Rendered
        && let Some((mark, kind)) = object_mark(document, &run.source)
    {
        let drawn = match &kind {
            ObjectKind::Math => document.source[mark.range.clone()].to_owned(),
            ObjectKind::Footnote { label } => label.clone(),
            _ => crate::entity::decode(&run.text).unwrap_or_else(|| run.text.clone()),
        };
        return Part {
            source,
            text: drawn.into(),
            role: Role::Object(kind),
        };
    }
    Part {
        source,
        text,
        role: Role::Content,
    }
}

/// The mark a content run is the whole of, when it draws an object.
fn object_mark<'a>(
    document: &'a Document,
    run: &Range<usize>,
) -> Option<(&'a crate::Mark, ObjectKind)> {
    document.marks.iter().find_map(|mark| {
        if &mark.range != run {
            return None;
        }
        match &mark.kind {
            MarkKind::InlineMath => Some((mark, ObjectKind::Math)),
            MarkKind::Entity => Some((mark, ObjectKind::Entity)),
            MarkKind::FootnoteReference { label } => Some((
                mark,
                ObjectKind::Footnote {
                    label: label.clone(),
                },
            )),
            _ => None,
        }
    })
}

/// The markers at the start of a line, and the range its own text draws.
///
/// A cell is `written` where the line spells the marker. A line that continues a construct
/// without repeating its marker still gets the construct's room, so the text after the
/// cells keeps one x down the construct. Bytes no cell claims are the indentation the
/// source spells, and they join the innermost cell with room for them.
#[allow(clippy::too_many_arguments)]
fn line_cells(
    document: &Document,
    entry: &Entry<'_>,
    line: &Range<usize>,
    content_start: usize,
    line_form: Option<crate::Form>,
    selection: &Range<usize>,
    focused: bool,
) -> (Vec<Cell>, Range<usize>) {
    let source = &*document.source;
    let mut cells: Vec<Cell> = Vec::new();
    let mut at = line.start;
    for (depth, ancestor) in entry.ancestors.iter().enumerate() {
        let Some(look) = ancestor_look(entry, depth) else {
            continue;
        };
        // A nested item's marker starts past its line's first byte. A marker that starts
        // before `at` belongs to an earlier line, and one reaching past the content is
        // another construct's.
        let recorded = marker(document, ancestor).filter(|range| {
            range.start >= at && range.end <= content_start && range.end > range.start
        });
        // A quote writes its marker on every line, so a line that has none is the lazy
        // continuation CommonMark allows; a list item writes its marker once.
        let written = match ancestor.kind {
            BlockKind::Quote => recorded.or_else(|| scan_quote(source, at, content_start)),
            _ => recorded,
        };
        if let Some(range) = &written
            && range.start > at
        {
            // The source spells the column an enclosing construct already reserves, so the
            // bytes join that cell: a nested marker stands at the content column, not one
            // column past it. Only a marker after a written one indents past its construct.
            match cells.last_mut() {
                Some(cell) if !cell.written => {
                    cell.source = Some(match cell.source.take() {
                        Some(own) => own.start..range.start,
                        None => at..range.start,
                    });
                }
                _ => cells.push(indent_cell(at..range.start)),
            }
            at = range.start;
        }
        let form = cell_form(line_form, ancestor, written.as_ref(), selection, focused);
        match (&ancestor.kind, written) {
            (BlockKind::ListItem { task }, Some(range)) => {
                let owner = entry.path[..depth].to_vec();
                let bracket = task.and_then(|_| task_bracket(source, &range));
                let split = bracket.map_or(range.end, |(open, _)| range.start + open);
                cells.push(Cell {
                    owner: owner.clone(),
                    label: label_of(source.get(range.start..split).unwrap_or("")),
                    source: Some(range.start..split),
                    written: true,
                    look,
                    form,
                });
                if let (Some(checked), Some((open, _))) = (task, bracket) {
                    cells.push(Cell {
                        owner,
                        source: Some(range.start + open..range.end),
                        written: true,
                        look: CellLook::Task { checked: *checked },
                        label: String::new(),
                        form,
                    });
                }
                at = range.end;
            }
            (_, Some(range)) => {
                cells.push(Cell {
                    owner: cell_owner(entry, depth),
                    label: label_of(source.get(range.clone()).unwrap_or("")),
                    source: Some(range.clone()),
                    written: true,
                    look,
                    form,
                });
                at = range.end;
            }
            (_, None) => cells.push(Cell {
                owner: cell_owner(entry, depth),
                source: None,
                written: false,
                look,
                label: String::new(),
                form,
            }),
        }
    }
    if at < content_start {
        match cells.iter_mut().rev().find(|cell| !cell.written) {
            Some(cell) => cell.source = Some(at..content_start),
            None => cells.push(indent_cell(at..content_start)),
        }
    }
    (cells, content_start..line.end)
}

/// The look a container's cell draws, or none for a container that writes no marker.
fn ancestor_look(entry: &Entry<'_>, depth: usize) -> Option<CellLook> {
    match entry.ancestors.get(depth)?.kind {
        BlockKind::Quote => Some(CellLook::Quote),
        BlockKind::ListItem { .. } => {
            Some(match entry.ancestors.get(depth.checked_sub(1)?)?.kind {
                BlockKind::OrderedList { .. } => CellLook::Ordered,
                _ => CellLook::Bullet,
            })
        }
        _ => None,
    }
}

/// The construct a cell's width is shared by: a quote itself, or the list an item is in.
fn cell_owner(entry: &Entry<'_>, depth: usize) -> Vec<usize> {
    match entry.ancestors.get(depth).map(|ancestor| &ancestor.kind) {
        Some(BlockKind::ListItem { .. }) => entry.path[..depth].to_vec(),
        _ => entry.path[..=depth].to_vec(),
    }
}

fn cell_form(
    line_form: Option<crate::Form>,
    ancestor: &Block,
    written: Option<&Range<usize>>,
    selection: &Range<usize>,
    focused: bool,
) -> crate::Form {
    if let Some(form) = line_form {
        return form;
    }
    match written {
        Some(range) if focused => crate::Scope::Marker.form(&ancestor.full, Some(range), selection),
        _ => crate::Form::Rendered,
    }
}

fn indent_cell(range: Range<usize>) -> Cell {
    Cell {
        owner: Vec::new(),
        source: Some(range),
        written: false,
        look: CellLook::Indent,
        label: String::new(),
        form: crate::Form::Source,
    }
}

/// How many bytes of the block's own indentation each of its lines writes: a fence's N,
/// or an indented block's four columns.
pub fn code_strip(document: &Document, ancestors: &[&Block], block: &Block) -> usize {
    let source = &*document.source;
    let fence_line = line_start(source, block.full.start)..line_end(source, block.full.start);
    block
        .full
        .start
        .saturating_sub(container_end(document, ancestors, &fence_line))
}

/// The bytes a code block's line writes before its first character, split by owner.
///
/// `container` is what the containers' markers and columns take; `spaces` is the whole
/// run of spaces and tabs after it — the block's own indentation as this line spells it,
/// which the parser strips up to [`code_strip`] of and keeps the rest of as code.
pub fn code_line_indent(
    document: &Document,
    ancestors: &[&Block],
    line: &Range<usize>,
) -> (Range<usize>, Range<usize>) {
    let source = &*document.source;
    let container = container_end(document, ancestors, line);
    let mut end = container;
    while end < line.end && matches!(source.as_bytes().get(end), Some(b' ' | b'\t')) {
        end += 1;
    }
    (line.start..container, container..end)
}

/// The bytes `line` spells as its part of the block's indentation.
///
/// A line that writes a container's marker starts after it. A line that writes none —
/// a list item's continuation line — has no marker there, so the column its container
/// reserves is part of the block's indentation on that line, and an edit to it moves
/// every line alike.
pub fn code_value(document: &Document, ancestors: &[&Block], line: &Range<usize>) -> Range<usize> {
    let source = &*document.source;
    let mut at = line.start;
    for ancestor in ancestors {
        let written = match ancestor.kind {
            BlockKind::Quote => scan_quote(source, at, line.end).or_else(|| {
                marker(document, ancestor)
                    .filter(|range| range.start >= at && range.end <= line.end)
            }),
            _ => marker(document, ancestor)
                .filter(|range| range.start >= at && range.end <= line.end),
        };
        if let Some(range) = written {
            at = range.end;
        }
    }
    let mut end = at;
    while end < line.end && matches!(source.as_bytes().get(end), Some(b' ' | b'\t')) {
        end += 1;
    }
    at..end
}

/// The ancestors of the block at `path`, outermost first.
pub fn ancestors_of<'a>(document: &'a Document, path: &[usize]) -> Vec<&'a Block> {
    let mut out = Vec::new();
    let mut blocks = document.blocks.as_slice();
    for index in path {
        let Some(block) = blocks.get(*index) else {
            break;
        };
        out.push(block);
        blocks = block.children();
    }
    out.pop();
    out
}

/// Where the containers' prefix ends at the start of `line`.
///
/// This is where the parser resumes: the containers' markers and the columns they reserve,
/// without the block's own indentation. A quote writes `>` after up to three spaces and may
/// take one space after it; a list indents its lines by up to three spaces; an item writes
/// its marker once and reserves marker width plus one to four spaces on every other line.
pub fn container_end(document: &Document, ancestors: &[&Block], line: &Range<usize>) -> usize {
    let source = &*document.source;
    let mut at = line.start;
    for (index, ancestor) in ancestors.iter().enumerate() {
        match &ancestor.kind {
            BlockKind::Quote => {
                at = scan_quote(source, at, line.end).map_or(at, |marker| marker.end);
            }
            BlockKind::BulletList | BlockKind::OrderedList { .. } => {
                at = (at + list_indent(document, &ancestors[..index], ancestor)).min(line.end);
            }
            BlockKind::ListItem { .. } => {
                at = (at + item_column(document, ancestor)).min(line.end);
            }
            _ => {}
        }
    }
    at
}

/// The columns a list adds before its items' markers.
fn list_indent(document: &Document, ancestors: &[&Block], list: &Block) -> usize {
    let Some(first) = list.children().first() else {
        return 0;
    };
    let Some(ink) = marker(document, first).map(|marker| marker.start) else {
        return 0;
    };
    let source = &*document.source;
    let line = line_start(source, ink)..line_end(source, ink);
    ink.saturating_sub(container_end(document, ancestors, &line))
}

/// The columns an item reserves: its marker's width plus the one to four spaces after it.
fn item_column(document: &Document, item: &Block) -> usize {
    let Some(marker) = marker(document, item) else {
        return 0;
    };
    let Some(text) = document.source.get(marker) else {
        return 0;
    };
    let width = text
        .bytes()
        .take_while(|byte| !matches!(byte, b' ' | b'\t'))
        .count();
    let spaces = text.len() - width;
    let after = if spaces == 0 || spaces > 4 { 1 } else { spaces };
    width + after
}

/// A quote's marker at `at`: up to three spaces, `>`, and the one space after it.
fn scan_quote(source: &str, at: usize, limit: usize) -> Option<Range<usize>> {
    let bytes = source.as_bytes();
    let mut cursor = at;
    let mut columns = 0usize;
    while cursor < limit && columns < 3 {
        match bytes.get(cursor) {
            Some(b' ') => {
                cursor += 1;
                columns += 1;
            }
            Some(b'\t') => {
                cursor += 1;
                columns = (columns / 4 + 1) * 4;
                if columns > 3 {
                    return None;
                }
            }
            _ => break,
        }
    }
    if cursor >= limit || bytes.get(cursor) != Some(&b'>') {
        return None;
    }
    cursor += 1;
    if cursor < limit && matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
        cursor += 1;
    }
    Some(at..cursor)
}

/// A task item's `[x]` inside its marker: its offset, and whether the box is checked.
fn task_bracket(source: &str, range: &Range<usize>) -> Option<(usize, bool)> {
    let text = source.get(range.clone())?;
    let open = text.find('[')?;
    let checked = match text.as_bytes().get(open + 1)? {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };
    (text.as_bytes().get(open + 2) == Some(&b']')).then_some((open, checked))
}

/// Where a continuation line's own text begins: past the marker run it opens with.
///
/// The run starts with the columns its containers reserve — a setext heading's underline is
/// written after them — so the run's first byte past those columns begins the line's own
/// syntax. A run of spaces alone is indentation, not syntax.
fn prefix_end(
    document: &Document,
    entry: &Entry<'_>,
    projection: &Projection,
    line: &Range<usize>,
) -> usize {
    let end = leading_marker_end(projection, line);
    let source = &*document.source;
    let from = container_end(document, &entry.ancestors, line).clamp(line.start, end);
    source
        .get(from..end)
        .and_then(|bytes| bytes.find(|c: char| !c.is_whitespace()))
        .map_or(end, |offset| from + offset)
}

/// The end of the marker run the line opens with, or the line's start when it opens with none.
fn leading_marker_end(projection: &Projection, line: &Range<usize>) -> usize {
    for run in &projection.runs {
        if run.source.end <= line.start {
            continue;
        }
        if run.source.start >= line.end {
            break;
        }
        let marker = run.syntax.is_some_and(|index| {
            projection
                .constructs
                .get(index)
                .is_some_and(|construct| construct.kind == ConstructKind::Marker)
        });
        if marker && run.source.start <= line.start {
            return run.source.end.min(line.end);
        }
        break;
    }
    line.start
}

/// The form of the marker a line opens with, when the line writes one.
fn marker_form(
    projection: &Projection,
    at: usize,
    selection: &Range<usize>,
    focused: bool,
) -> Option<crate::Form> {
    let run = projection.runs.iter().find(|run| run.source.start == at)?;
    let construct = projection.constructs.get(run.syntax?)?;
    if construct.kind != ConstructKind::Marker {
        return None;
    }
    Some(match focused {
        true => construct.form(selection),
        false => crate::Form::Rendered,
    })
}

/// The room the enclosing containers take at the start of a block-level row.
fn block_cells(document: &Document, entry: &Entry<'_>, body_start: usize) -> Vec<Cell> {
    let source = &*document.source;
    let line = line_start(source, body_start);
    let mut cells = Vec::new();
    for (depth, ancestor) in entry.ancestors.iter().enumerate() {
        let Some(look) = ancestor_look(entry, depth) else {
            continue;
        };
        let range = marker(document, ancestor)
            .filter(|range| range.start >= line && range.end <= body_start);
        cells.push(Cell {
            owner: cell_owner(entry, depth),
            written: range.is_some(),
            label: range.as_ref().map_or_else(String::new, |range| {
                label_of(source.get(range.clone()).unwrap_or(""))
            }),
            form: crate::Form::Rendered,
            source: range,
            look,
        });
    }
    cells
}

/// The room a line takes from its containers without writing any of their markers.
fn spacer_cells(document: &Document, line: Range<usize>) -> Vec<Cell> {
    containers_at(document, line)
        .into_iter()
        .map(|(owner, look, _)| Cell {
            owner,
            source: None,
            written: false,
            look,
            label: String::new(),
            form: crate::Form::Rendered,
        })
        .collect()
}

/// The containers a line stands in, outermost first, each with the marker it writes there.
///
/// A line stands in a construct only while the construct's bytes still reach it, and a line
/// that writes no marker belongs to it only while it still has a block below: a quote that
/// ended above spells `>` at this cursor too, so the marker alone cannot claim the line.
fn containers_at(
    document: &Document,
    line: Range<usize>,
) -> Vec<(Vec<usize>, CellLook, Option<Range<usize>>)> {
    fn walk(
        source: &str,
        blocks: &[Block],
        path: &mut Vec<usize>,
        line: &Range<usize>,
        at: &mut usize,
        out: &mut Vec<(Vec<usize>, CellLook, Option<Range<usize>>)>,
    ) {
        for (index, block) in blocks.iter().enumerate() {
            // The line's first byte must stand inside the container's own bytes. An earlier
            // quote of the same document spells `>` at this cursor too, so the marker alone
            // cannot say which construct holds the line.
            if block.full.start > line.start || line.start >= block.full.end {
                continue;
            }
            let look = match &block.kind {
                BlockKind::Quote => Some(CellLook::Quote),
                BlockKind::BulletList => Some(CellLook::Bullet),
                BlockKind::OrderedList { .. } => Some(CellLook::Ordered),
                // An item writes its list's cell, which the list itself already named.
                _ => None,
            };
            let written = match look {
                Some(CellLook::Quote) => scan_quote(source, *at, line.end),
                _ => None,
            };
            if written.is_none() && !content_after(block, line.end) {
                continue;
            }
            if let Some(range) = &written {
                *at = range.end;
            }
            path.push(index);
            if let Some(look) = look {
                out.push((path.clone(), look, written));
            }
            walk(source, block.children(), path, line, at, out);
            path.pop();
        }
    }
    let mut out = Vec::new();
    let mut at = line.start;
    walk(
        &*document.source,
        &document.blocks,
        &mut Vec::new(),
        &line,
        &mut at,
        &mut out,
    );
    out
}

/// Whether `block` holds a block that begins on a line after `line_end`.
///
/// A block's span carries the blank lines after it — a list that ends above still ends at
/// the next block's start, and that tail can hold a sibling's marker — so containment asks
/// about the blocks below the line, not about the span's bytes.
fn content_after(block: &Block, line_end: usize) -> bool {
    block
        .children()
        .iter()
        .any(|child| child.span.start >= line_end || content_after(child, line_end))
}

fn block_at<'a>(document: &'a Document, path: &[usize]) -> Option<&'a Block> {
    let mut blocks = document.blocks.as_slice();
    let mut block = None;
    for index in path {
        block = blocks.get(*index);
        blocks = block?.children();
    }
    block
}

/// What a hidden marker draws: a bullet for any of CommonMark's three bullet characters,
/// otherwise the number or letter as written.
///
/// The space the source spells after the marker is not part of the label: the cell keeps
/// that room itself (`Markdown::kept_space`), so the label and the bytes beside it end on
/// the same edge.
fn label_of(marker: &str) -> String {
    let rest = marker.trim_start();
    match rest.chars().next() {
        Some('-' | '+' | '*') => format!("•{}", &rest[1..]).trim_end().to_owned(),
        _ => marker.trim_end().to_owned(),
    }
}

/// One row per line a block does not own.
fn gap_rows(
    document: &Document,
    range: Range<usize>,
    selection: &Range<usize>,
    out: &mut Vec<Row>,
) {
    let source = &*document.source;
    let mut start = range.start;
    while start < range.end {
        let Some(offset) = source[start..range.end].find('\n') else {
            // The tail sits on the next block's line; that row's prefix covers those bytes.
            break;
        };
        let end = start + offset;
        // A line no block owns still stands inside its containers: the markers it writes on
        // this line are cells, and one that writes none keeps the container's room, so the
        // caret on a blank line starts at the column its neighbours do.
        let mut cells = Vec::new();
        let mut at = start;
        for (owner, look, written) in containers_at(document, start..end) {
            let form = written
                .as_ref()
                .zip(block_at(document, &owner))
                .map_or(crate::Form::Rendered, |(range, block)| {
                    crate::Scope::Marker.form(&block.full, Some(range), selection)
                });
            if let Some(range) = &written {
                at = range.end;
            }
            cells.push(Cell {
                owner,
                written: written.is_some(),
                source: written,
                look,
                label: String::new(),
                form,
            });
        }
        let parts = if at < end {
            vec![Part {
                source: at..end,
                text: source[at..end].into(),
                role: Role::Syntax {
                    form: crate::Scope::Marker.form(&(start..end), Some(&(start..end)), selection),
                },
            }]
        } else {
            Vec::new()
        };
        out.push(Row {
            path: Vec::new(),
            span: start..end,
            content: at..end,
            cells,
            kind: RowKind::Empty { parts },
        });
        start = end + 1;
    }
}

/// The first byte of the line after the one holding `at`; `at` itself when it already
/// stands at a line start.
pub fn next_line_start(source: &str, at: usize) -> usize {
    let at = at.min(source.len());
    if at == 0 || source.as_bytes().get(at - 1) == Some(&b'\n') {
        return at;
    }
    source.as_bytes()[at..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |offset| at + offset + 1)
}

/// The first byte of the line holding `at`.
pub fn line_start(source: &str, at: usize) -> usize {
    let at = at.min(source.len());
    source.as_bytes()[..at]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1)
}

/// The block's bytes without the break that ends it.
fn trimmed(source: &str, full: &Range<usize>) -> Range<usize> {
    let mut end = full.end.min(source.len());
    let start = full.start.min(end);
    let bytes = source.as_bytes();
    while end > start && matches!(bytes.get(end - 1), Some(b'\n' | b'\r')) {
        end -= 1;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn scene(source: &str) -> Scene {
        build(&parse(source), None, None, false)
    }

    fn parts(scene: &Scene, row: usize) -> &[Part] {
        match &scene.rows[row].kind {
            RowKind::Text { parts, .. } | RowKind::Empty { parts } => parts,
            other => panic!("row {row} is not text: {other:?}"),
        }
    }

    /// The first invariant: the rows tile the source in order, and no text row spans a
    /// line break — a code card is the only row that covers several lines.
    #[test]
    fn every_source_line_is_exactly_one_row() {
        let source = "a\n\nb\n\n\n```rust\nx\n```\n";
        let scene = scene(source);
        let lines: Vec<&str> = scene
            .rows
            .iter()
            .map(|row| &source[row.span.clone()])
            .collect();
        assert_eq!(lines, ["a", "", "b", "", "", "```rust\nx\n```", ""]);
        for row in &scene.rows {
            if matches!(row.kind, RowKind::Text { .. } | RowKind::Empty { .. }) {
                assert!(
                    !source[row.span.clone()].contains('\n'),
                    "a text row must not span a break"
                );
            }
        }
    }

    /// A footnote definition is a container: its marker gets the line it writes and its
    /// note gets the next, rather than the definition collapsing into one paragraph.
    #[test]
    fn a_footnote_definition_keeps_its_marker_and_note_on_their_lines() {
        let source = "Body[^1].\n\n[^1]: the note\n";
        let scene = scene(source);
        let lines: Vec<&str> = scene
            .rows
            .iter()
            .map(|row| &source[row.span.clone()])
            .collect();
        assert_eq!(lines, ["Body[^1].", "", "[^1]: the note", ""]);

        let note = &scene.rows[2];
        assert_eq!(&source[note.content.clone()], "the note");
        let marker = note.cells.first().expect("the marker keeps its bytes");
        assert_eq!(marker.look, CellLook::Indent);
        assert_eq!(
            &source[marker.source.clone().expect("the marker is written")],
            "[^1]: "
        );
    }

    /// A reference is an object while the reader looks at it and its bytes while the caret
    /// is inside, the same split an inline formula takes.
    #[test]
    fn a_footnote_reference_draws_its_label_until_the_caret_reaches_it() {
        let source = "Body[^1].\n\n[^1]: the note\n";
        let document = parse(source);
        let away = build(&document, None, Some(0..0), true);
        let row = parts(&away, 0);
        let reference = row
            .iter()
            .find(|part| matches!(part.role, Role::Object(ObjectKind::Footnote { .. })))
            .expect("the reference is an object");
        assert_eq!(reference.text.as_str(), "1");

        let near = build(&document, None, Some(6..6), true);
        let row = parts(&near, 0);
        assert!(
            !row.iter()
                .any(|part| matches!(part.role, Role::Object(ObjectKind::Footnote { .. }))),
            "the caret inside shows the reference as written"
        );
    }

    /// A line's trailing spaces are its own bytes: an item's frame stops at its text, and
    /// hiding them would take away the caret positions that stand inside them.
    #[test]
    fn a_row_keeps_the_spaces_the_line_ends_with() {
        let source = "- a  \n\n> q  \n";
        let scene = scene(source);
        let list = &scene.rows[0];
        assert_eq!(
            &source[list.span.clone()],
            "- a  ",
            "the row is its source line"
        );
        let quote = &scene.rows[2];
        assert_eq!(&source[quote.span.clone()], "> q  ", "a quote's row too");
        for row in [list, quote] {
            let parts = match &row.kind {
                RowKind::Text { parts, .. } => parts,
                other => panic!("expected a text row, got {other:?}"),
            };
            let last = parts.last().expect("a line trail is a part");
            assert_eq!(
                &source[last.source.clone()],
                "  ",
                "the spaces are the row's tail"
            );
            assert_eq!(last.role, Role::Content, "a line is shown as it is written");
        }
    }

    /// Front matter is opaque to layout but kept for editing: focused, it must show its
    /// source like a fence, or a document's metadata would vanish from the editor.
    #[test]
    fn a_focused_front_matter_row_shows_its_source() {
        let document = parse("---\ntitle: x\n---\n");
        let scene = build(&document, None, Some(0..0), true);
        assert!(
            matches!(
                scene.rows.first().map(|row| &row.kind),
                Some(RowKind::FrontMatter {
                    form: crate::Form::Source
                })
            ),
            "a focused front matter row is source, got {:?}",
            scene.rows.first().map(|row| &row.kind)
        );
        let unfocused = build(&document, None, None, false);
        assert!(matches!(
            unfocused.rows.first().map(|row| &row.kind),
            Some(RowKind::FrontMatter {
                form: crate::Form::Rendered
            })
        ));
    }

    /// Heading style reaches the row rather than the widget guessing from an id.
    #[test]
    fn a_heading_row_carries_its_level() {
        let scene = scene("## Title\n");
        assert!(matches!(
            scene.rows.first().map(|row| &row.kind),
            Some(RowKind::Text {
                style: RowStyle::Heading(2),
                ..
            })
        ));
    }

    /// A list item's marker is a cell of its own: its bytes stay a view, and the text after
    /// them starts at the cell's far edge.
    #[test]
    fn a_list_marker_is_a_cell_of_its_own() {
        let scene = scene("- one\n");
        let row = &scene.rows[0];
        assert_eq!(row.cells.len(), 1);
        let cell = &row.cells[0];
        assert_eq!(cell.look, CellLook::Bullet);
        assert!(cell.written);
        assert_eq!(cell.label, "•");
        assert_eq!(cell.source, Some(0..2));
        assert_eq!(row.content, 2..5);
        assert_eq!(parts(&scene, 0)[0].text.as_str(), "one");
    }

    /// With the caret on the line the cell draws its own bytes, so they can be deleted.
    #[test]
    fn a_marker_under_the_caret_shows_as_source() {
        let document = parse("- one\n");
        let scene = build(&document, None, Some(1..1), true);
        let cell = &scene.rows[0].cells[0];
        assert_eq!(cell.form, crate::Form::Source);
        assert_eq!(cell.source, Some(0..2));
    }

    /// A quote writes its marker on every line; a marker shows only while the caret touches
    /// its own bytes, so a caret in the quoted text leaves the prefix rendered.
    #[test]
    fn a_quote_prefix_shows_only_while_the_caret_touches_it() {
        let document = parse("> a\n> b\n");
        let in_text = build(&document, None, Some(3..3), true);
        assert_eq!(in_text.rows[0].cells[0].form, crate::Form::Rendered);
        let scene = build(&document, None, Some(2..2), true);
        assert_eq!(scene.rows[0].cells[0].form, crate::Form::Source);
        assert_eq!(scene.rows[1].cells[0].form, crate::Form::Rendered);
    }

    /// Every blank source line draws a row of its own, so the caret can stand on each.
    #[test]
    fn blank_lines_between_paragraphs_each_get_a_row() {
        let scene = scene("a\n\n\n\nb\n");
        let spans: Vec<Range<usize>> = scene.rows.iter().map(|row| row.span.clone()).collect();
        assert_eq!(spans, [0..1, 2..2, 3..3, 4..4, 5..6, 7..7]);
    }

    /// A thematic break is a line-level construct like a marker: its bytes show while the
    /// caret touches them, and the divider stands otherwise.
    #[test]
    fn a_rule_shows_its_bytes_while_the_caret_touches_it() {
        let document = parse("---\n");
        let away = build(&document, None, Some(4..4), true);
        assert!(matches!(
            away.rows[0].kind,
            RowKind::Rule {
                form: crate::Form::Rendered
            }
        ));
        let on = build(&document, None, Some(1..1), true);
        assert!(matches!(
            on.rows[0].kind,
            RowKind::Rule {
                form: crate::Form::Source
            }
        ));
    }

    /// A setext heading's underline is the line's own syntax, not a column a container
    /// reserves: the spaces before it stay the item's cell, and the underline hides.
    #[test]
    fn a_setext_underline_is_syntax_past_the_containers_column() {
        let listed = scene("- x\n  -\n");
        let row = &listed.rows[1];
        assert_eq!(row.cells.len(), 1);
        assert_eq!(row.cells[0].look, CellLook::Bullet);
        assert!(!row.cells[0].written);
        assert_eq!(
            row.cells[0].source,
            Some(4..6),
            "the item's column, not the underline"
        );
        let line = parts(&listed, 1);
        assert_eq!(line.len(), 1);
        assert_eq!(line[0].text.as_str(), "-");
        assert_eq!(line[0].source, 6..7);
        assert_eq!(
            line[0].role,
            Role::Syntax {
                form: crate::Form::Rendered
            }
        );
        // With no container, the whole run is the line's syntax and no cell stands there.
        let bare = scene("Title\n=====\n");
        assert!(bare.rows[1].cells.is_empty());
        assert_eq!(parts(&bare, 1)[0].source, 6..11);
    }

    /// A nested item writes its marker past the indentation the source spells, and those
    /// bytes join the construct whose column they spell: the nested marker stands at the
    /// parent's content column rather than one column past it, and still draws its number.
    #[test]
    fn a_nested_item_stands_at_its_parents_content_column() {
        let scene = scene("- a\n\n  1. one\n");
        let row = scene
            .rows
            .iter()
            .find(|row| row.cells.iter().any(|cell| cell.look == CellLook::Ordered))
            .expect("the ordered item's row");
        assert_eq!(row.cells.len(), 2, "the indentation is the parent's cell");
        assert_eq!(row.cells[0].look, CellLook::Bullet);
        assert!(!row.cells[0].written);
        assert_eq!(row.cells[0].source, Some(5..7));
        let ordered = &row.cells[1];
        assert!(ordered.written);
        assert_eq!(ordered.label, "1.");
        assert_eq!(ordered.source, Some(7..10));
        assert_eq!(row.content, 10..13);
    }

    /// A blank line inside a list writes no marker and still stands at the list's column:
    /// the cell is the container's room alone, which is what puts the caret there where the
    /// item's text starts.
    #[test]
    fn a_blank_line_inside_a_list_keeps_its_column() {
        let scene = scene("- a\n\n  b\n");
        let row = &scene.rows[1];
        assert_eq!(row.content, 4..4);
        assert_eq!(row.cells.len(), 1);
        assert_eq!(row.cells[0].look, CellLook::Bullet);
        assert!(!row.cells[0].written);
        // The line after the last break follows the list, so it keeps no room from it.
        let tail = scene.rows.last().expect("the line after the last break");
        assert!(tail.content.is_empty());
        assert!(tail.cells.is_empty());
    }

    /// A blank line after a list is not a list line: the list's last byte is above it, and
    /// only the blank lines a construct still has content after keep its room.
    #[test]
    fn a_blank_line_after_a_list_belongs_to_no_container() {
        let listed = scene("- a\n\nb\n");
        let row = &listed.rows[1];
        assert_eq!(row.content, 4..4);
        assert!(row.cells.is_empty(), "the list ended on the line above");
        // The same for a quote that ends before the blank line.
        let quoted = scene("> a\n\nb\n");
        assert!(quoted.rows[1].cells.is_empty());
        // A quote's own blank line keeps the quote's room while the list above it ended.
        let nested = scene("> - a\n>\n> b\n");
        let row = &nested.rows[1];
        assert_eq!(row.cells.len(), 1, "the list ended on the line above");
        assert_eq!(row.cells[0].look, CellLook::Quote);
    }

    /// A nested quote stands in two cells, so it draws two bars.
    #[test]
    fn a_nested_quote_stands_in_two_cells() {
        let scene = scene("> > deep\n");
        let cells = &scene.rows[0].cells;
        assert_eq!(cells.len(), 2);
        assert!(cells.iter().all(|cell| cell.look == CellLook::Quote));
        assert_eq!(cells[0].source, Some(0..2));
        assert_eq!(cells[1].source, Some(2..4));
    }

    /// A task item's marker is two cells — the bullet and the box — so the flag stays a byte
    /// a press can flip, and the box keeps the room the source spells.
    #[test]
    fn a_task_marker_is_a_bullet_cell_and_a_box_cell() {
        let scene = scene("- [x] done\n");
        let cells = &scene.rows[0].cells;
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].look, CellLook::Bullet);
        assert_eq!(cells[0].source, Some(0..2));
        assert_eq!(cells[1].look, CellLook::Task { checked: true });
        assert_eq!(cells[1].source, Some(2..6));
        assert_eq!(scene.rows[0].content, 6..10);
    }

    /// A line that repeats no marker still holds its construct's room, so the nested item's
    /// second line starts under its first rather than under the quote's bar.
    #[test]
    fn a_quiet_line_keeps_the_room_of_every_container() {
        let scene = scene("> - a\n>   b\n");
        let second = &scene.rows[1];
        assert_eq!(second.cells.len(), 2);
        assert_eq!(second.cells[0].look, CellLook::Quote);
        assert!(second.cells[0].written);
        assert_eq!(second.cells[0].source, Some(6..8));
        assert_eq!(second.cells[1].look, CellLook::Bullet);
        assert!(!second.cells[1].written, "an item writes its marker once");
        assert_eq!(
            second.cells[1].source,
            Some(8..10),
            "the indentation is the item's room"
        );
        assert_eq!(second.content, 10..11);
    }

    /// A marker-only line is its quotes' cells and nothing else.
    #[test]
    fn a_marker_only_line_is_one_quote_cell() {
        let scene = scene("> a\n>\n> b\n");
        let row = &scene.rows[1];
        assert_eq!(row.cells.len(), 1);
        assert_eq!(row.cells[0].look, CellLook::Quote);
        assert_eq!(row.cells[0].source, Some(4..5));
        assert_eq!(row.content, 5..5);
    }

    /// A formula away from the caret is an object carrying its TeX; under the caret it is
    /// the source again.
    #[test]
    fn an_inline_formula_is_an_object_until_the_caret_reaches_it() {
        let document = parse("x $a^2$ y\n");
        let away = build(&document, None, Some(0..0), true);
        let row = parts(&away, 0);
        let math = row
            .iter()
            .find(|part| matches!(part.role, Role::Object(ObjectKind::Math)))
            .expect("the formula is an object");
        assert_eq!(math.text.as_str(), "a^2");

        let near = build(&document, None, Some(4..4), true);
        let row = parts(&near, 0);
        assert!(
            !row.iter()
                .any(|part| matches!(part.role, Role::Object(ObjectKind::Math))),
            "the caret inside shows the TeX as written"
        );
    }
}
