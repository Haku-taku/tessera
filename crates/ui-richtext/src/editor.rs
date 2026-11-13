//! A leaf block as the source an in-place editor shows.
//!
//! The block's `full` range is the whole construct, markers included, while its pieces
//! are the text layout draws. The projection cuts that range into runs, so every byte is
//! accounted for once and a caret offset stays a source offset however much syntax hides.

use std::ops::Range;

use crate::{Block, BlockBody, Document, MarkKind, Splice, shifted_range};

/// Every leaf block's projection, cut once for a source rather than once per frame.
///
/// A control redraws its whole document every frame and a projection is linear in the
/// block, so an app that parsed a large document would cut it again sixty times a second.
/// Building this beside the parse moves that work to the edit that changed the source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    /// Sorted by the block's own range, which no two blocks share.
    frames: Vec<(Range<usize>, Projection)>,
    /// The source length these frames were cut from, so [`Prepared::update`] can reject a
    /// cache that does not belong to the document it is given.
    source_len: usize,
}

impl Prepared {
    pub fn of(document: &Document) -> Self {
        let mut frames = Vec::new();
        collect(document, &document.blocks, &mut frames);
        frames.sort_unstable_by_key(|(range, _)| (range.start, range.end));
        Self {
            frames,
            source_len: document.source.len(),
        }
    }

    /// Reuse the frames a splice did not touch; only the window's blocks are projected anew.
    /// Falls back to [`Prepared::of`] when the splice does not line up with `document`.
    pub fn update(old: &Prepared, document: &Document, splice: &Splice) -> Self {
        let Splice::Window {
            old: window,
            delta,
            blocks,
        } = splice
        else {
            return Self::of(document);
        };
        if old.source_len.checked_add_signed(*delta) != Some(document.source.len()) {
            return Self::of(document);
        }
        let Some(window_end_new) = window.end.checked_add_signed(*delta) else {
            return Self::of(document);
        };
        let Some(window_blocks) = document.blocks.get(blocks.clone()) else {
            return Self::of(document);
        };
        if window_blocks.is_empty() {
            if !document.blocks.is_empty() {
                return Self::of(document);
            }
        } else {
            let inside =
                window_blocks
                    .first()
                    .zip(window_blocks.last())
                    .is_some_and(|(first, last)| {
                        window.start <= first.full.start && last.full.end <= window_end_new
                    });
            if !inside {
                return Self::of(document);
            }
        }

        let mut frames = Vec::with_capacity(old.frames.len() + window_blocks.len());
        for (range, projection) in &old.frames {
            if range.start < window.start && range.end <= window.start {
                frames.push((range.clone(), projection.clone()));
            } else if range.start >= window.end {
                let Some(projection) = shifted_projection(projection, *delta) else {
                    return Self::of(document);
                };
                let Some(range) = shifted_range(range, *delta) else {
                    return Self::of(document);
                };
                frames.push((range, projection));
            }
            // A frame inside the window was replaced by the window parse.
        }
        collect(document, window_blocks, &mut frames);
        frames.sort_unstable_by_key(|(range, _)| (range.start, range.end));
        Self {
            frames,
            source_len: document.source.len(),
        }
    }

    pub fn get(&self, block: &Block) -> Option<&Projection> {
        let key = (block.full.start, block.full.end);
        let at = self
            .frames
            .binary_search_by_key(&key, |(range, _)| (range.start, range.end))
            .ok()?;
        self.frames.get(at).map(|(_, projection)| projection)
    }

    /// The blocks prepared, for a caller that wants a count and not a walk.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

fn collect(document: &Document, blocks: &[Block], frames: &mut Vec<(Range<usize>, Projection)>) {
    for block in blocks {
        if let Some(projection) = project(document, block) {
            frames.push((block.full.clone(), projection));
            // A projected container's frame covers its children, and no view asks a cell for
            // one: cutting them would only repeat the work on every keystroke.
            if matches!(block.kind, crate::BlockKind::Table { .. }) {
                continue;
            }
        }
        collect(document, block.children(), frames);
    }
}

/// A frame's runs and constructs moved by `shift`, with their text unchanged.
fn shifted_projection(projection: &Projection, shift: isize) -> Option<Projection> {
    let runs = projection
        .runs
        .iter()
        .map(|run| {
            Some(Run {
                source: shifted_range(&run.source, shift)?,
                text: run.text.clone(),
                syntax: run.syntax,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let constructs = projection
        .constructs
        .iter()
        .map(|construct| {
            Some(Construct {
                kind: construct.kind,
                full: shifted_range(&construct.full, shift)?,
                content: match &construct.content {
                    Some(content) => Some(shifted_range(content, shift)?),
                    None => None,
                },
                marker: match &construct.marker {
                    Some(marker) => Some(shifted_range(marker, shift)?),
                    None => None,
                },
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Projection {
        span: shifted_range(&projection.span, shift)?,
        runs,
        constructs,
    })
}

/// A leaf block cut into runs that cover its whole source construct, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Projection {
    /// The bytes an editor shows: `Block::full` without its trailing line break.
    pub span: Range<usize>,
    /// Every byte of `span`, exactly once, in order.
    pub runs: Vec<Run>,
    /// The constructs that may hide their delimiters, in source order.
    pub constructs: Vec<Construct>,
}

/// One run of the block's source.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The bytes this run covers.
    pub source: Range<usize>,
    /// Those bytes as text. This is the source verbatim, never a decoded form: layout may
    /// draw something else for it later, but an offset in it is always a source offset.
    pub text: String,
    /// The construct whose delimiter this run is, or `None` for content.
    pub syntax: Option<usize>,
}

/// One construct of the source, with the state model's class attached to its kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Construct {
    pub kind: ConstructKind,
    /// The whole construct, delimiters included. For a marker this is the bytes the line
    /// writes before its text, a trailing line break included.
    pub full: Range<usize>,
    /// The construct's payload — a mark's text, a fence's code — or `None` when the whole
    /// construct is syntax, as a heading's `# ` is.
    pub content: Option<Range<usize>>,
    /// The bytes this construct's own line writes, when the construct belongs to a line.
    pub marker: Option<Range<usize>>,
}

impl Construct {
    /// The class whose predicate decides this construct's form.
    pub fn scope(&self) -> crate::Scope {
        self.kind.scope()
    }

    /// How this construct is drawn while `selection` stands where it does.
    pub fn form(&self, selection: &Range<usize>) -> crate::Form {
        self.scope()
            .form(&self.full, self.marker.as_ref(), selection)
    }
}

/// What a construct is: its syntax, and the class its state is decided by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructKind {
    Emphasis,
    Strong,
    Strikethrough,
    Code,
    Link,
    /// An image's `![`, `](` and destination; its alt text is the content.
    Image,
    InlineMath,
    /// An entity's whole written form, which has no delimiters of its own.
    Entity,
    /// A footnote reference's whole written form, like an entity's.
    Footnote,
    /// A line's own marker: a heading's `# `, an item's bullet, a quote's `> `.
    Marker,
    /// A construct that is one unit across all its lines: a fence, a display formula, a
    /// table's frame.
    Fence,
}

impl ConstructKind {
    pub fn scope(self) -> crate::Scope {
        match self {
            Self::Emphasis
            | Self::Strong
            | Self::Strikethrough
            | Self::Code
            | Self::Link
            | Self::Image
            | Self::InlineMath
            | Self::Entity
            | Self::Footnote => crate::Scope::Inline,
            Self::Marker => crate::Scope::Marker,
            Self::Fence => crate::Scope::Fence,
        }
    }
}

/// A run and the form of the construct that owns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown<'a> {
    pub run: &'a Run,
    /// For syntax, its own construct's form. For content, the form of the mark this run is
    /// the payload of — the innermost one — so an object knows whether it is an object yet.
    pub form: crate::Form,
}

/// The bytes a container's own marker takes on its first line.
///
/// A list item's bullet and the space after it come before its first child, so no leaf
/// frame holds them. An editor draws them as text of their own — the marker an item is
/// written with, and exactly the space the source spells — which keeps the caret able to
/// reach them. The range stops at the first line break, so a nested block's indentation is
/// not read as part of the marker; an item with no child — a break's own new line — writes
/// nothing else, so its line is that marker.
pub fn marker(document: &Document, block: &Block) -> Option<Range<usize>> {
    let start = block.span.start.min(block.full.start);
    let end = match block.children().first() {
        Some(first) => first.full.start.min(block.full.end),
        None => block.full.end,
    };
    let line = document.source.get(start..end)?;
    let line_end = start + line.find('\n').unwrap_or(line.len());
    let ink = start + (line.len() - line.trim_start().len());
    (line_end > ink).then_some(ink..line_end)
}

/// The projection of a leaf block, or nothing for a container or an empty construct.
///
/// A table is the one container that projects: its source state edits the whole construct.
pub fn project(document: &Document, block: &Block) -> Option<Projection> {
    if matches!(block.kind, crate::BlockKind::Table { .. }) {
        return project_table(document, block);
    }
    let BlockBody::Inline(_) = &block.body else {
        return None;
    };
    let source = &*document.source;
    // An empty frame still names a position: an empty item or fence line has to be drawn
    // for the caret to be able to enter it, even though it holds no run.
    let span = trimmed(source, &block.full);
    if span.end > source.len() {
        return None;
    }

    let mut content: Vec<Range<usize>> = block
        .pieces()
        .iter()
        .filter_map(|piece| {
            let start = piece.source.start.max(span.start);
            let end = piece.source.end.min(span.end);
            (start < end).then_some(start..end)
        })
        .collect();
    content.sort_unstable_by_key(|range| range.start);

    // A leaf that draws itself as one parsed form — a fenced block, a display formula, an
    // image, the front matter — owns everything a mark would have claimed. Its fences and
    // the payload between them are one unit, so no mark inside it is shown on its own.
    let fence = matches!(
        block.kind,
        crate::BlockKind::CodeBlock { .. }
            | crate::BlockKind::Math
            | crate::BlockKind::Image { .. }
            | crate::BlockKind::FrontMatter
    );
    let mut constructs = Vec::new();
    if fence {
        let payload = block.span.start.max(span.start)..block.span.end.min(span.end);
        constructs.push(Construct {
            kind: ConstructKind::Fence,
            full: span.clone(),
            content: Some(payload).filter(|payload| !payload.is_empty()),
            marker: None,
        });
    } else {
        // A mark's delimiters are the construct minus its content. Only a mark whose whole
        // construct sits inside the frame owns delimiters here; a mark crossing a block
        // boundary would hide the block's own text.
        for mark in &document.marks {
            if mark.full.start < span.start || span.end < mark.full.end || mark.full.is_empty() {
                continue;
            }
            let start = mark.range.start.max(span.start);
            let end = mark.range.end.min(span.end);
            if start >= end {
                continue;
            }
            constructs.push(Construct {
                kind: kind_of(&mark.kind),
                full: mark.full.clone(),
                content: Some(start..end),
                marker: None,
            });
        }
    }
    let fence_index = fence.then_some(0usize);
    let inline_count = constructs.len();

    // Edges cut at every delimiter and at every line start: a line's unclaimed syntax must
    // never merge with the next line's, because a marker belongs to one line.
    let mut edges = vec![span.start, span.end];
    for range in &content {
        edges.extend([range.start, range.end]);
    }
    for construct in &constructs {
        edges.extend([construct.full.start, construct.full.end]);
        if let Some(content) = &construct.content {
            edges.extend([content.start, content.end]);
        }
    }
    let line_starts: Vec<usize> = line_ranges(source, &span)
        .into_iter()
        .map(|line| line.start)
        .collect();
    edges.extend(line_starts.iter().copied());
    edges.sort_unstable();
    edges.dedup();

    let mut classes = Vec::with_capacity(edges.len().saturating_sub(1));
    let mut markers: Vec<Construct> = Vec::new();
    let mut open_marker: Option<usize> = None;
    // A line's marker is its prefix; whitespace after the line has begun is the line.
    let mut begun = false;
    for pair in edges.windows(2) {
        let at = pair[0]..pair[1];
        if line_starts.contains(&at.start) {
            begun = false;
        }
        if content
            .iter()
            .any(|range| range.start <= at.start && at.end <= range.end)
        {
            open_marker = None;
            begun = true;
            classes.push(Class::Content);
            continue;
        }
        if let Some(index) = constructs
            .iter()
            .position(|construct| delimiter(construct, &at))
        {
            open_marker = None;
            begun = true;
            classes.push(Class::Syntax(index));
            continue;
        }
        // A fence owns every byte its marks did not claim.
        if let Some(index) = fence_index {
            classes.push(Class::Syntax(index));
            continue;
        }
        // Spaces at a line's end belong to the line: no construct claimed them, and a row
        // shows its source as written, so they keep their room and their caret bytes.
        if begun
            && source
                .get(at.clone())
                .is_some_and(|bytes| bytes.chars().all(char::is_whitespace))
        {
            open_marker = None;
            classes.push(Class::Content);
            continue;
        }
        // Syntax no mark claimed: a heading's `# `, the `> ` a quote writes on a
        // continuation line. Consecutive bytes on one line are that line's marker.
        let index = match open_marker {
            Some(index) => index,
            None => {
                markers.push(Construct {
                    kind: ConstructKind::Marker,
                    full: at.clone(),
                    content: None,
                    marker: Some(marker_bytes(source, &at)),
                });
                markers.len() - 1
            }
        };
        if let Some(marker) = markers.get_mut(index) {
            marker.full.end = at.end;
            marker.marker = Some(marker_bytes(source, &marker.full));
        }
        open_marker = Some(index);
        classes.push(Class::Marker(index));
    }

    // Runs name constructs by index; the markers sit after the marks, so order the whole
    // set by source once and point both ends of the mapping at the new positions.
    constructs.extend(markers);
    let mut order: Vec<usize> = (0..constructs.len()).collect();
    order.sort_by_key(|index| {
        constructs
            .get(*index)
            .map_or(0, |construct| construct.full.start)
    });
    let mut new_index = vec![0usize; order.len()];
    for (position, old) in order.iter().enumerate() {
        new_index[*old] = position;
    }

    let mut runs = Vec::with_capacity(classes.len());
    for (pair, class) in edges.windows(2).zip(classes) {
        let at = pair[0]..pair[1];
        let syntax = match class {
            Class::Content => None,
            Class::Syntax(index) => Some(new_index[index]),
            Class::Marker(index) => Some(new_index[inline_count + index]),
        };
        let text = source.get(at.clone()).map(str::to_owned)?;
        runs.push(Run {
            source: at,
            text,
            syntax,
        });
    }

    let constructs = order
        .iter()
        .filter_map(|old| constructs.get(*old).cloned())
        .collect();
    Some(Projection {
        span,
        runs,
        constructs,
    })
}

/// Whether `at` is one of `construct`'s delimiters rather than its payload.
fn delimiter(construct: &Construct, at: &Range<usize>) -> bool {
    match &construct.content {
        Some(content) => {
            covers(&(construct.full.start..content.start), at)
                || covers(&(content.end..construct.full.end), at)
        }
        None => covers(&construct.full, at),
    }
}

/// The bytes a line writes as its marker: the run without the line break that ends it.
fn marker_bytes(source: &str, run: &Range<usize>) -> Range<usize> {
    let breaks = run.end > run.start && source.as_bytes().get(run.end - 1) == Some(&b'\n');
    let end = if breaks { run.end - 1 } else { run.end };
    run.start..end
}

/// Every line of a range, cut at its breaks.
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

/// How one interval of the frame is drawn.
enum Class {
    Content,
    /// A construct's syntax, by index into the constructs.
    Syntax(usize),
    /// One line's marker, by index into the markers.
    Marker(usize),
}

impl Projection {
    /// The runs to lay out, each naming the form its construct is in.
    ///
    /// `selection` is in the document's offsets, like the construct spans. A read-only
    /// control asks for no source at all: with nothing focused every construct is rendered,
    /// which is what makes a reading card and an editor share one scene builder.
    pub fn shown(&self, selection: &Range<usize>, focused: bool) -> Vec<Shown<'_>> {
        let forms: Vec<crate::Form> = self
            .constructs
            .iter()
            .map(|construct| match focused {
                true => construct.form(selection),
                false => crate::Form::Rendered,
            })
            .collect();
        self.runs
            .iter()
            .map(|run| Shown {
                run,
                form: match run.syntax {
                    Some(index) => forms.get(index).copied().unwrap_or(crate::Form::Rendered),
                    // The innermost construct whose payload this run is: constructs are in
                    // source order, so a nested mark — the one that starts later — wins.
                    None => forms
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(index, _)| {
                            self.constructs.get(*index).is_some_and(|construct| {
                                construct.content.as_ref().is_some_and(|content| {
                                    content.start <= run.source.start
                                        && run.source.end <= content.end
                                })
                            })
                        })
                        .map_or(crate::Form::Rendered, |(_, form)| *form),
                },
            })
            .collect()
    }
}

/// A table's frame: cell text is content, and every other byte — pipes, padding, the
/// delimiter row, the line breaks — is one fence construct, shown as a whole when the caret
/// is anywhere inside and never drawn by the grid.
fn project_table(document: &Document, block: &Block) -> Option<Projection> {
    let source = &*document.source;
    let span = trimmed(source, &block.full);
    let mut content = Vec::new();
    for row in block.children() {
        for cell in row.children() {
            let start = cell.span.start.max(span.start);
            let end = cell.span.end.min(span.end);
            if start < end {
                content.push(start..end);
            }
        }
    }
    content.sort_unstable_by_key(|range| range.start);

    let mut edges = vec![span.start, span.end];
    for range in &content {
        edges.extend([range.start, range.end]);
    }
    edges.sort_unstable();
    edges.dedup();

    let mut runs = Vec::with_capacity(edges.len().saturating_sub(1));
    for pair in edges.windows(2) {
        let at = pair[0]..pair[1];
        let text = source.get(at.clone()).map(str::to_owned)?;
        let content = content
            .iter()
            .any(|range| range.start <= at.start && at.end <= range.end);
        runs.push(Run {
            source: at,
            text,
            syntax: (!content).then_some(0usize),
        });
    }
    Some(Projection {
        span: span.clone(),
        runs,
        constructs: vec![Construct {
            kind: ConstructKind::Fence,
            full: span,
            content: None,
            marker: None,
        }],
    })
}

/// The frame without the line break that ends it.
fn trimmed(source: &str, full: &Range<usize>) -> Range<usize> {
    let mut end = full.end.min(source.len());
    let start = full.start.min(end);
    let bytes = source.as_bytes();
    while end > start && matches!(bytes.get(end - 1), Some(b'\n' | b'\r')) {
        end -= 1;
    }
    start..end
}

fn covers(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end && !outer.is_empty()
}

fn kind_of(kind: &MarkKind) -> ConstructKind {
    match kind {
        MarkKind::Emphasis => ConstructKind::Emphasis,
        MarkKind::Strong => ConstructKind::Strong,
        MarkKind::Strikethrough => ConstructKind::Strikethrough,
        MarkKind::Code => ConstructKind::Code,
        MarkKind::InlineMath => ConstructKind::InlineMath,
        MarkKind::Entity => ConstructKind::Entity,
        MarkKind::FootnoteReference { .. } => ConstructKind::Footnote,
        MarkKind::Link { .. } => ConstructKind::Link,
        MarkKind::Image { .. } => ConstructKind::Image,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockKind, parse};

    /// The prepared frames are a cache of `project`, so every leaf must answer alike: a
    /// miss would draw a block with no runs, and a wrong hit would draw another block's.
    #[test]
    fn a_prepared_document_answers_what_the_projection_answers() {
        let source = "# Title\n\n- item one\n- item two\n\n> quoted **bold**\n\n```rust\nfn main() {}\n```\n";
        let document = parse(source);
        let prepared = Prepared::of(&document);
        let mut leaves = 0;
        fn walk(document: &Document, blocks: &[Block], prepared: &Prepared, leaves: &mut usize) {
            for block in blocks {
                match project(document, block) {
                    Some(projection) => {
                        *leaves += 1;
                        assert_eq!(prepared.get(block), Some(&projection));
                    }
                    None => assert_eq!(prepared.get(block), None),
                }
                walk(document, block.children(), prepared, leaves);
            }
        }
        walk(&document, &document.blocks, &prepared, &mut leaves);
        assert!(
            leaves >= 5,
            "the sample holds a heading, paragraphs, items and code"
        );
    }

    fn leaf<'a>(document: &'a Document, kind: &BlockKind) -> &'a Block {
        fn walk<'a>(blocks: &'a [Block], kind: &BlockKind) -> Option<&'a Block> {
            for block in blocks {
                if &block.kind == kind {
                    return Some(block);
                }
                if let Some(found) = walk(block.children(), kind) {
                    return Some(found);
                }
            }
            None
        }
        walk(&document.blocks, kind).expect("the document has such a block")
    }

    /// The bytes a reader sees: content always, syntax only while it is drawn as source.
    fn visible(projection: &Projection, selection: Range<usize>, focused: bool) -> String {
        projection
            .shown(&selection, focused)
            .into_iter()
            .filter(|shown| shown.run.syntax.is_none() || shown.form == crate::Form::Source)
            .map(|shown| shown.run.text.clone())
            .collect()
    }

    fn shown(projection: &Projection, selection: Range<usize>) -> Vec<String> {
        visible(projection, selection, true)
            .chars()
            .map(String::from)
            .collect()
    }

    /// Hiding syntax does not remove bytes: the runs cover the frame exactly, and each
    /// run's text is its own source bytes, so a caret offset is the source offset.
    #[test]
    fn every_byte_of_the_frame_is_one_run_of_its_own_source() {
        let document = parse("**bold** and *it*\n");
        let projection = project(&document, leaf(&document, &BlockKind::Paragraph))
            .expect("a paragraph projects");
        assert_eq!(projection.span, 0..17);
        assert_eq!(
            projection
                .runs
                .iter()
                .map(|run| run.source.clone())
                .collect::<Vec<_>>(),
            [0..2, 2..6, 6..8, 8..13, 13..14, 14..16, 16..17]
        );
        for run in &projection.runs {
            assert_eq!(run.text.as_str(), &document.source[run.source.clone()]);
        }
        let texts: Vec<&str> = projection
            .runs
            .iter()
            .map(|run| run.text.as_str())
            .collect();
        assert_eq!(texts, ["**", "bold", "**", " and ", "*", "it", "*"]);
    }

    /// A caret next to or inside a construct shows both of its delimiters; a caret away
    /// from it shows none, and the neighbours stay hidden.
    #[test]
    fn the_construct_under_the_caret_shows_both_delimiters() {
        let document = parse("**bold** and *it*\n");
        let projection = project(&document, leaf(&document, &BlockKind::Paragraph))
            .expect("a paragraph projects");
        assert_eq!(shown(&projection, 0..0).join(""), "**bold** and it");
        assert_eq!(shown(&projection, 3..3).join(""), "**bold** and it");
        assert_eq!(shown(&projection, 6..6).join(""), "**bold** and it");
        assert_eq!(shown(&projection, 8..8).join(""), "**bold** and it");
        assert_eq!(shown(&projection, 11..11).join(""), "bold and it");
        assert_eq!(shown(&projection, 14..14).join(""), "bold and *it*");
    }

    /// Delimiters belong to the innermost construct, so a caret in the outer text does
    /// not reveal an inner emphasis, and the outer markers stay hidden inside it.
    #[test]
    fn nested_delimiters_belong_to_the_innermost_construct() {
        let document = parse("**a *b* c**\n");
        let projection = project(&document, leaf(&document, &BlockKind::Paragraph))
            .expect("a paragraph projects");
        assert_eq!(shown(&projection, 2..2).join(""), "**a b c**");
        assert_eq!(shown(&projection, 5..5).join(""), "**a *b* c**");
        assert_eq!(shown(&projection, 6..6).join(""), "**a *b* c**");
    }

    /// A fence is one unit: a caret anywhere inside shows the whole construct as source, so
    /// the opening and closing fences appear together and the code never sits between one
    /// shown fence and one hidden one.
    #[test]
    fn a_fence_shows_as_one_unit_while_the_caret_is_inside_it() {
        let source = "```rust\nlet x = 1;\n```\n";
        let document = parse(source);
        let code = leaf(
            &document,
            &BlockKind::CodeBlock {
                language: Some("rust".to_owned()),
                fenced: true,
            },
        );
        let projection = project(&document, code).expect("a fenced block projects");
        assert_eq!(&source[projection.span.clone()], "```rust\nlet x = 1;\n```");
        assert_eq!(
            shown(&projection, 12..12).join(""),
            "```rust\nlet x = 1;\n```"
        );
        assert_eq!(
            shown(&projection, 1..1).join(""),
            "```rust\nlet x = 1;\n```"
        );
        assert_eq!(
            shown(&projection, 20..20).join(""),
            "```rust\nlet x = 1;\n```"
        );
        assert_eq!(visible(&projection, 12..12, false), "let x = 1;\n");
    }

    /// A marker belongs to the line it is written on: in a quote, where every line carries
    /// `> `, the caret's line shows its prefix and the others stay hidden.
    #[test]
    fn a_block_marker_shows_on_its_own_line_only() {
        let document = parse("> a\n> b\n");
        let quote = leaf(&document, &BlockKind::Quote);
        let paragraph = quote
            .children()
            .first()
            .expect("the quote holds a paragraph");
        let projection = project(&document, paragraph).expect("the quote's text projects");
        assert_eq!(&document.source[projection.span.clone()], "a\n> b");
        assert_eq!(shown(&projection, 2..2).join(""), "a\nb");
        assert_eq!(shown(&projection, 6..6).join(""), "a\n> b");
        // A selection reaching both lines speaks for both of them.
        assert_eq!(shown(&projection, 2..6).join(""), "a\n> b");
    }

    /// A tight list item has no paragraph event; its frame starts after the bullet, and
    /// the marks inside it still widen the frame to their delimiters.
    #[test]
    fn a_tight_list_item_keeps_the_bullet_out_of_its_frame() {
        let document = parse("- item **a**\n");
        let item = leaf(&document, &BlockKind::ListItem { task: None });
        let paragraph = item.children().first().expect("the item holds a paragraph");
        let projection = project(&document, paragraph).expect("the item's text projects");
        assert_eq!(projection.span, 2..12);
        assert_eq!(
            projection
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<Vec<_>>(),
            ["item ", "**", "a", "**"]
        );
    }

    /// A marker belongs to its own bytes: the caret must touch the `# ` to show it, so the
    /// title it introduces — and every other line — leaves it hidden. This is the rule that
    /// keeps a document-wide input from revealing every heading at once.
    #[test]
    fn a_marker_shows_only_while_the_caret_touches_it() {
        let document = parse("## Head\n## Other\n");
        let first = &document.blocks[0];
        let projection = project(&document, first).expect("a heading projects");
        assert_eq!(projection.span, 0..7);
        assert_eq!(projection.constructs[0].kind, ConstructKind::Marker);
        assert_eq!(projection.constructs[0].marker, Some(0..3));
        assert_eq!(shown(&projection, 0..0).join(""), "## Head");
        assert_eq!(
            shown(&projection, 3..3).join(""),
            "## Head",
            "a caret resting against the marker's end is a caret on it"
        );
        assert_eq!(
            shown(&projection, 5..5).join(""),
            "Head",
            "a caret in the title leaves the marker hidden"
        );
        assert_eq!(
            shown(&projection, 12..12).join(""),
            "Head",
            "and so does a caret on another line"
        );
        assert_eq!(visible(&projection, 5..5, false), "Head");
    }

    /// A container's marker is the bytes before its first child, cut at the line break:
    /// what an editor draws as text of its own, so the caret can step onto it.
    #[test]
    fn a_marker_is_the_source_before_the_first_child_on_its_line() {
        let document = parse("- one\n- two\n\n  1. nested\n");
        let text =
            |range: Option<Range<usize>>| range.map(|range| document.source[range].to_owned());
        let list = &document.blocks[0];
        assert_eq!(
            text(marker(&document, &list.children()[0])),
            Some("- ".to_owned())
        );
        assert_eq!(
            text(marker(&document, &list.children()[1])),
            Some("- ".to_owned())
        );
        let nested = &list.children()[1].children()[1].children()[0];
        assert_eq!(text(marker(&document, nested)), Some("1. ".to_owned()));

        // An item whose text starts on the next line: the marker stops at the break, so
        // the indentation of what follows is not read as part of it.
        let split = parse("- \n  - sub\n");
        let text = |range: Option<Range<usize>>| range.map(|range| split.source[range].to_owned());
        assert_eq!(
            text(marker(&split, &split.blocks[0].children()[0])),
            Some("- ".to_owned())
        );
    }

    /// A block holding nothing is still drawn: the item's paragraph sits where its text
    /// would go, and a fence line with nothing under it is a line of its own. Either is a
    /// place the caret enters, so neither frame may be dropped for holding no text.
    #[test]
    fn a_block_with_no_content_keeps_a_place_for_its_text() {
        let document = parse("1. \n");
        let item = leaf(&document, &BlockKind::ListItem { task: None });
        let paragraph = item
            .children()
            .first()
            .expect("the item keeps a place for its text");
        assert_eq!(paragraph.span, 3..3);
        let projection = project(&document, paragraph).expect("an empty frame projects");
        assert_eq!(projection.span, 3..3);
        assert!(projection.runs.is_empty());

        // A fence must begin a line, so `aaaaa```rust` is a paragraph and the closing
        // fence opens an empty block of its own (cmark and cmark-gfm agree).
        let source = "aaaaa```rust\nlet runs = 1;\n```\n";
        let document = parse(source);
        let fence = leaf(
            &document,
            &BlockKind::CodeBlock {
                language: None,
                fenced: true,
            },
        );
        assert_eq!(&source[fence.span.clone()], "");
        let projection = project(&document, fence).expect("the fence line projects");
        assert_eq!(&source[projection.span.clone()], "```");
        assert_eq!(projection.runs.len(), 1);
        assert_eq!(projection.runs[0].text.as_str(), "```");
        assert_eq!(
            projection.constructs[projection.runs[0].syntax.expect("the fence is syntax")].kind,
            ConstructKind::Fence
        );
    }

    /// A table's frame covers every byte of the table exactly once. Away from the caret the
    /// pipes, padding, delimiter row and line breaks are hidden runs, so the frame reads as
    /// its cells; with the caret inside, every run is drawn and the source state shows.
    #[test]
    fn a_table_frame_covers_every_byte_and_shows_it_in_the_source_state() {
        let source = "| a | bb |\n|:--|--:|\n| c | dd |\n";
        let document = parse(source);
        let table = &document.blocks[0];
        let projection = project(&document, table).expect("a table projects");
        assert_eq!(
            &source[projection.span.clone()],
            &source[..source.len() - 1]
        );

        let mut at = projection.span.start;
        for run in &projection.runs {
            assert_eq!(run.source.start, at, "runs are contiguous and in order");
            assert_eq!(run.text.as_str(), &source[run.source.clone()]);
            at = run.source.end;
        }
        assert_eq!(at, projection.span.end, "the runs cover the frame exactly");

        assert_eq!(
            visible(&projection, 0..0, false),
            "abbcdd",
            "the grid reads only the cell text"
        );
        assert_eq!(
            visible(&projection, 0..0, true),
            &source[projection.span.clone()],
            "the source state shows every byte"
        );
    }
}
