//! Markdown source into the block model, with every offset pointing back at the source.
//!
//! The event stream builds the tree with one stack of open blocks; inline attributes are
//! not nested in it, so an emphasis or a link is one range however many blocks it crosses.

use std::ops::Range;

use pulldown_cmark::{
    Alignment as ColumnAlignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use crate::{
    Alignment, Block, BlockBody, BlockKind, Document, Mark, MarkKind, Piece, shifted_range,
};

/// Parse Markdown into blocks and marks. Leaf spans cover the leaf's text; a container's
/// span covers the whole construct, marker and trailing newline included.
pub fn parse(source: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    options.insert(Options::ENABLE_FOOTNOTES);
    let mut builder = Builder::new(source);
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        builder.event(event, range);
    }
    Document {
        source: source.into(),
        blocks: builder.done,
        marks: builder.marks,
    }
}

/// The result of [`Document::reparse`]: the new document and the splice that produced it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reparse {
    pub document: Document,
    /// What was replaced, so a cached projection can be reused by [`crate::Prepared::update`].
    pub splice: Splice,
}

/// How a reparse replaced part of the previous source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Splice {
    /// The source was parsed whole; no earlier work can be reused.
    Whole,
    /// The previous source's `old` range was replaced by the window parse, and every offset
    /// at or after `old.end` moved by `delta`; `blocks` names the new top-level blocks.
    Window {
        old: Range<usize>,
        delta: isize,
        blocks: Range<usize>,
    },
}

impl Document {
    /// Reparse `source` against this document, keeping the blocks and marks away from the
    /// edit. Falls back to [`parse`] when the splice cannot be proved safe.
    ///
    /// The edit is located by the common byte prefix and suffix of the two sources. The
    /// affected interval is widened to the top-level blocks that cover it, plus one block
    /// on each side: a list, quote, fence or Setext heading is a run of lines, and its
    /// meaning depends on the lines around the edit. Everything outside that window keeps
    /// its parse and moves by the length delta; the window is parsed on its own.
    pub fn reparse(&self, source: &str) -> Reparse {
        if &*self.source == source {
            return Reparse {
                document: self.clone(),
                splice: Splice::Whole,
            };
        }
        let old = &*self.source;
        let (prefix, suffix) = common_ends(old, source);
        let changed = prefix..old.len() - suffix;
        let (lo, hi) = window_blocks(&self.blocks, &changed);
        let window_start = match self.blocks.get(lo) {
            Some(block) if lo > 0 => block.full.start,
            _ => 0,
        };
        let window_end = if hi >= self.blocks.len() {
            old.len()
        } else {
            self.blocks
                .get(hi - 1)
                .map_or(old.len(), |block| block.full.end)
        };
        if changed.start < window_start || changed.end > window_end {
            return whole(source);
        }
        let (Ok(old_len), Ok(new_len)) =
            (isize::try_from(old.len()), isize::try_from(source.len()))
        else {
            return whole(source);
        };
        let delta = new_len - old_len;
        let Some(window_end_new) = window_end.checked_add_signed(delta) else {
            return whole(source);
        };
        let Some(window_source) = source.get(window_start..window_end_new) else {
            return whole(source);
        };
        let inner = parse(window_source);
        // A block boundary that does not land on the window's edge means the edit merged
        // or split across it; the retained neighbours can no longer be spliced.
        let left_agrees = lo == 0
            || inner
                .blocks
                .first()
                .is_some_and(|block| block.full.start == 0);
        let right_agrees = hi == self.blocks.len()
            || inner
                .blocks
                .last()
                .is_some_and(|block| block.full.end == window_source.len());
        if !left_agrees || !right_agrees {
            return whole(source);
        }
        let Some(window_shift) = isize::try_from(window_start).ok() else {
            return whole(source);
        };

        let mut blocks = Vec::with_capacity(lo + inner.blocks.len() + self.blocks.len() - hi);
        blocks.extend_from_slice(&self.blocks[..lo]);
        for block in &inner.blocks {
            match shifted_block(block, window_shift) {
                Some(block) => blocks.push(block),
                None => return whole(source),
            }
        }
        for block in &self.blocks[hi..] {
            match shifted_block(block, delta) {
                Some(block) => blocks.push(block),
                None => return whole(source),
            }
        }

        // A mark is a byte range over the whole source; one that crosses the window can
        // belong to neither side, and the splice would have to split it.
        let mut left_marks = Vec::new();
        let mut right_marks = Vec::new();
        for mark in &self.marks {
            if mark.full.end <= window_start {
                left_marks.push(mark.clone());
            } else if mark.full.start >= window_end {
                match shifted_mark(mark, delta) {
                    Some(mark) => right_marks.push(mark),
                    None => return whole(source),
                }
            } else if !(window_start <= mark.full.start && mark.full.end <= window_end) {
                return whole(source);
            }
        }
        let mut marks =
            Vec::with_capacity(left_marks.len() + inner.marks.len() + right_marks.len());
        marks.extend(left_marks);
        for mark in &inner.marks {
            match shifted_mark(mark, window_shift) {
                Some(mark) => marks.push(mark),
                None => return whole(source),
            }
        }
        marks.extend(right_marks);

        let new_blocks = lo..lo + inner.blocks.len();
        Reparse {
            document: Document {
                source: source.into(),
                blocks,
                marks,
            },
            splice: Splice::Window {
                old: window_start..window_end,
                delta,
                blocks: new_blocks,
            },
        }
    }
}

/// A full parse of `source`, with nothing spliceable.
fn whole(source: &str) -> Reparse {
    Reparse {
        document: parse(source),
        splice: Splice::Whole,
    }
}

/// The longest common prefix and suffix in bytes, both cut back to character boundaries.
fn common_ends(old: &str, new: &str) -> (usize, usize) {
    let old = old.as_bytes();
    let new = new.as_bytes();
    // Slice equality is `memcmp`; scanning in blocks keeps a 250 KB edit from costing one
    // branch per byte.
    const BLOCK: usize = 4096;
    let limit = old.len().min(new.len());
    let mut prefix = 0;
    while prefix + BLOCK <= limit && old[prefix..prefix + BLOCK] == new[prefix..prefix + BLOCK] {
        prefix += BLOCK;
    }
    while prefix < limit && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    let room = limit - prefix;
    while suffix + BLOCK <= room
        && old[old.len() - suffix - BLOCK..old.len() - suffix]
            == new[new.len() - suffix - BLOCK..new.len() - suffix]
    {
        suffix += BLOCK;
    }
    while suffix < room && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix] {
        suffix += 1;
    }
    // A byte inside a multi-byte character is not a place a window may start or end;
    // shrinking both ends only widens the change.
    let prefix = floor_boundary_bytes(old, prefix).min(floor_boundary_bytes(new, prefix));
    while suffix > 0
        && (!is_boundary(old, old.len() - suffix) || !is_boundary(new, new.len() - suffix))
    {
        suffix -= 1;
    }
    (prefix, suffix)
}

fn is_boundary(text: &[u8], index: usize) -> bool {
    index == 0 || index >= text.len() || (text[index] & 0xC0) != 0x80
}

fn floor_boundary_bytes(text: &[u8], mut index: usize) -> usize {
    while index > 0 && !is_boundary(text, index) {
        index -= 1;
    }
    index
}

/// The contiguous top-level blocks the splice replaces: the blocks touching `changed`,
/// widened by one on each side; or a block on each side when the edit sits between blocks.
fn window_blocks(blocks: &[Block], changed: &Range<usize>) -> (usize, usize) {
    let len = blocks.len();
    let touched = |block: &Block| {
        if changed.is_empty() {
            block.full.start < changed.start && changed.start < block.full.end
        } else {
            block.full.start < changed.end && changed.start < block.full.end
        }
    };
    let first = blocks.iter().position(&touched);
    let last = blocks.iter().rposition(&touched);
    match (first, last) {
        (Some(first), Some(last)) => (first.saturating_sub(1), (last + 2).min(len)),
        _ => {
            let anchor = if changed.is_empty() {
                changed.start
            } else {
                changed.end
            };
            let pos = blocks.partition_point(|block| block.full.start < anchor);
            (pos.saturating_sub(1), (pos + 1).min(len))
        }
    }
}

fn shifted_block(block: &Block, shift: isize) -> Option<Block> {
    let mut block = block.clone();
    block.span = shifted_range(&block.span, shift)?;
    block.full = shifted_range(&block.full, shift)?;
    match &mut block.body {
        BlockBody::Blocks(children) => {
            for child in children {
                *child = shifted_block(child, shift)?;
            }
        }
        BlockBody::Inline(pieces) => {
            for piece in pieces {
                piece.source = shifted_range(&piece.source, shift)?;
            }
        }
    }
    Some(block)
}

fn shifted_mark(mark: &Mark, shift: isize) -> Option<Mark> {
    Some(Mark {
        range: shifted_range(&mark.range, shift)?,
        full: shifted_range(&mark.full, shift)?,
        kind: mark.kind.clone(),
    })
}

struct Builder<'a> {
    /// The bytes the event ranges index, for the cases that must read them back.
    source: &'a str,
    /// Containers being filled, innermost last.
    open: Vec<Open>,
    /// Marks being read, innermost last.
    reading: Vec<OpenMark>,
    /// Finished marks and top-level blocks.
    marks: Vec<Mark>,
    done: Vec<Block>,
}

struct Open {
    block: Block,
    /// The extent of the inline content seen so far: a leaf's span at close.
    content: Option<Range<usize>>,
    /// Set while a paragraph holds nothing but a display formula, which becomes a block.
    only_math: Option<Range<usize>>,
    /// Inline content a list item owns with no paragraph of its own, as in a tight list.
    loose: Option<Range<usize>>,
    /// The text read so far. A leaf keeps it; an item hands it to the paragraph it grows.
    pieces: Vec<Piece>,
}

struct OpenMark {
    kind: MarkKind,
    /// The whole construct, delimiters included; the start event already spans it.
    full: Range<usize>,
    start: Option<usize>,
    end: usize,
}

impl<'a> Builder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            open: Vec::new(),
            reading: Vec::new(),
            marks: Vec::new(),
            done: Vec::new(),
        }
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        match event {
            Event::Start(Tag::Paragraph) => self.open_block(BlockKind::Paragraph, range),
            Event::Start(Tag::MetadataBlock(_)) => self.open_block(BlockKind::FrontMatter, range),
            Event::Start(Tag::FootnoteDefinition(_)) => {
                self.open_block(BlockKind::FootnoteDefinition, range)
            }
            Event::Start(Tag::Heading { level, .. }) => {
                self.open_block(BlockKind::Heading(level_of(level)), range)
            }
            Event::Start(Tag::BlockQuote(_)) => self.open_block(BlockKind::Quote, range),
            Event::Start(Tag::List(start)) => {
                let kind = match start {
                    Some(start) => BlockKind::OrderedList { start },
                    None => BlockKind::BulletList,
                };
                self.open_block(kind, range);
            }
            Event::Start(Tag::Item) => self.open_block(BlockKind::ListItem { task: None }, range),
            Event::Start(Tag::CodeBlock(info)) => {
                let (language, fenced) = match info {
                    CodeBlockKind::Fenced(info) => {
                        (info.split_whitespace().next().map(str::to_owned), true)
                    }
                    CodeBlockKind::Indented => (None, false),
                };
                self.open_block(BlockKind::CodeBlock { language, fenced }, range);
            }
            Event::Start(Tag::Table(alignments)) => {
                let alignments = alignments.into_iter().map(alignment_of).collect();
                self.open_block(BlockKind::Table { alignments }, range);
            }
            Event::Start(Tag::TableHead) => {
                self.open_block(BlockKind::TableRow { header: true }, range);
            }
            Event::Start(Tag::TableRow) => {
                self.open_block(BlockKind::TableRow { header: false }, range);
            }
            Event::Start(Tag::TableCell) => self.open_block(BlockKind::TableCell, range),
            Event::Start(Tag::Emphasis) => self.open_mark(MarkKind::Emphasis, range),
            Event::Start(Tag::Strong) => self.open_mark(MarkKind::Strong, range),
            Event::Start(Tag::Strikethrough) => self.open_mark(MarkKind::Strikethrough, range),
            Event::Start(Tag::Link { dest_url, .. }) => self.open_mark(
                MarkKind::Link {
                    destination: dest_url.into_string(),
                },
                range,
            ),
            Event::Start(Tag::Image { dest_url, .. }) => self.open_mark(
                MarkKind::Image {
                    destination: dest_url.into_string(),
                },
                range,
            ),
            Event::End(TagEnd::Emphasis)
            | Event::End(TagEnd::Strong)
            | Event::End(TagEnd::Strikethrough)
            | Event::End(TagEnd::Link) => self.close_mark(),
            // An image with an empty alt text has no content event, but its mark still
            // names the construct the caret and the image view need.
            Event::End(TagEnd::Image) => self.close_image(),
            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Heading(_))
            | Event::End(TagEnd::BlockQuote(_))
            | Event::End(TagEnd::List(_))
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::CodeBlock)
            | Event::End(TagEnd::Table)
            | Event::End(TagEnd::TableHead)
            | Event::End(TagEnd::TableRow)
            | Event::End(TagEnd::TableCell)
            | Event::End(TagEnd::MetadataBlock(_))
            | Event::End(TagEnd::FootnoteDefinition) => self.close_block(range),
            Event::Text(text) => self.text_event(range, text.into_string()),
            // An HTML block arrives as its own event with no paragraph around it, so the
            // bytes need a leaf to land in; inline HTML already sits inside one.
            Event::Html(text) => {
                let inline_leaf = self.open.last().is_some_and(|top| {
                    matches!(
                        top.block.kind,
                        BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::TableCell
                    )
                });
                if !inline_leaf {
                    self.open_block(BlockKind::Paragraph, range.clone());
                }
                self.text(range.clone(), text.into_string());
                if !inline_leaf {
                    self.close_block(range);
                }
            }
            Event::InlineHtml(text) => self.text(range, text.into_string()),
            // A soft break is a space: the paragraph flows, and only a hard break ends a
            // line. The source bytes stay the break itself, so an editor reading its own
            // frames still draws the source's lines.
            Event::SoftBreak => self.text(range, " ".to_owned()),
            Event::HardBreak => self.text(range, "\n".to_owned()),
            Event::Code(text) => {
                // The event spans the delimiters; the mark covers the code between them.
                let content = code_content(self.source, range.clone());
                self.text(content.clone(), text.into_string());
                self.marks.push(Mark {
                    range: content,
                    full: range,
                    kind: MarkKind::Code,
                });
            }
            Event::InlineMath(text) => {
                // `$…$`: the formula is the bytes between the markers.
                let content = range.start + 1..range.end.saturating_sub(1);
                self.text(content.clone(), text.into_string());
                self.marks.push(Mark {
                    range: content,
                    full: range,
                    kind: MarkKind::InlineMath,
                });
            }
            Event::DisplayMath(text) => {
                // `$$…$$`: a paragraph holding nothing else is a formula, not text.
                let content = range.start + 2..range.end.saturating_sub(2);
                if let Some(top) = self.open.last_mut()
                    && top.block.kind == BlockKind::Paragraph
                    && top.content.is_none()
                {
                    top.only_math = Some(content.clone());
                }
                self.note(content, text.into_string());
            }
            Event::Rule => {
                self.flush_loose();
                self.attach(Block {
                    kind: BlockKind::Rule,
                    span: range.clone(),
                    full: range,
                    body: BlockBody::Inline(Vec::new()),
                });
            }
            // A GFM task box: its bytes stay in the item's marker gap, so the paragraph's
            // span starts after them and the marker still reaches them for an edit.
            Event::TaskListMarker(checked) => {
                if let Some(top) = self.open.last_mut()
                    && matches!(top.block.kind, BlockKind::ListItem { .. })
                {
                    top.block.kind = BlockKind::ListItem {
                        task: Some(checked),
                    };
                }
            }
            Event::FootnoteReference(name) => {
                let written = self
                    .source
                    .get(range.clone())
                    .map_or_else(|| format!("[^{name}]"), str::to_owned);
                self.text(range.clone(), written);
                self.marks.push(Mark {
                    range: range.clone(),
                    full: range,
                    kind: MarkKind::FootnoteReference {
                        label: name.into_string(),
                    },
                });
            }
            _ => {}
        }
    }

    fn open_block(&mut self, kind: BlockKind, range: Range<usize>) {
        self.flush_loose();
        let body = if is_container(&kind) {
            BlockBody::Blocks(Vec::new())
        } else {
            BlockBody::Inline(Vec::new())
        };
        self.open.push(Open {
            block: Block {
                kind,
                span: range.start..range.start,
                full: range,
                body,
            },
            content: None,
            only_math: None,
            loose: None,
            pieces: Vec::new(),
        });
    }

    /// Plain inline content: it ends any display-formula reading, since the paragraph
    /// now holds more than the formula.
    fn text(&mut self, range: Range<usize>, decoded: String) {
        if let Some(top) = self.open.last_mut() {
            top.only_math = None;
        }
        self.note(range, decoded);
    }

    /// Text that may be an entity: the parser reports it as an event of its own, exactly
    /// over the written form. Code and undecodable forms stay source and take no mark.
    fn text_event(&mut self, range: Range<usize>, decoded: String) {
        let entity = self
            .source
            .get(range.clone())
            .filter(|_| !self.in_code())
            .and_then(crate::entity::decode)
            .is_some_and(|character| character == decoded);
        self.text(range.clone(), decoded);
        if entity {
            self.marks.push(Mark {
                range: range.clone(),
                full: range,
                kind: MarkKind::Entity,
            });
        }
    }

    /// Whether the open leaf is a fenced or indented code block, whose text is literal.
    fn in_code(&self) -> bool {
        self.open
            .last()
            .is_some_and(|open| matches!(open.block.kind, BlockKind::CodeBlock { .. }))
    }

    /// Record content on every reader: the open marks, the leaf, and a loose item.
    fn note(&mut self, range: Range<usize>, decoded: String) {
        for mark in &mut self.reading {
            if mark.start.is_none() {
                mark.start = Some(range.start);
            }
            mark.end = mark.end.max(range.end);
        }
        let Some(top) = self.open.last_mut() else {
            return;
        };
        match &mut top.content {
            Some(content) => content.end = content.end.max(range.end),
            None => top.content = Some(range.clone()),
        }
        if matches!(top.block.kind, BlockKind::ListItem { .. }) {
            match &mut top.loose {
                Some(loose) => loose.end = loose.end.max(range.end),
                None => top.loose = Some(range.clone()),
            }
        }
        top.pieces.push(Piece {
            source: range,
            text: decoded.into(),
        });
    }

    fn open_mark(&mut self, kind: MarkKind, full: Range<usize>) {
        self.reading.push(OpenMark {
            kind,
            full,
            start: None,
            end: 0,
        });
    }

    fn close_mark(&mut self) {
        let Some(mark) = self.reading.pop() else {
            return;
        };
        // A mark with no content between its delimiters paints nothing.
        if let Some(start) = mark.start {
            self.marks.push(Mark {
                range: start..mark.end,
                full: mark.full,
                kind: mark.kind,
            });
        }
    }

    /// Close an image even when its alt text is empty, so the construct keeps a mark.
    fn close_image(&mut self) {
        let Some(mark) = self.reading.pop() else {
            return;
        };
        let range = match mark.start {
            Some(start) => start..mark.end.max(start),
            None => mark.full.start..mark.full.start,
        };
        self.marks.push(Mark {
            range,
            full: mark.full,
            kind: mark.kind,
        });
    }

    /// A paragraph's only image, when the rest of its content is whitespace. v1 renders a
    /// top-level paragraph that is one image; inline images, several images and lists stay source.
    fn only_image(&self, pieces: &[Piece], full: &Range<usize>) -> Option<(Range<usize>, String)> {
        let mut images = self.marks.iter().filter(|mark| {
            matches!(mark.kind, MarkKind::Image { .. })
                && full.start <= mark.full.start
                && mark.full.end <= full.end
        });
        let image = images.next()?;
        if images.next().is_some() {
            return None;
        }
        // The construct's own bytes are not content events, so the paragraph's full range
        // is what says whether anything else was written beside the image.
        let outside = |range: Range<usize>| {
            self.source
                .get(range)
                .is_some_and(|text| text.chars().all(char::is_whitespace))
        };
        if !outside(full.start..image.full.start) || !outside(image.full.end..full.end) {
            return None;
        }
        for piece in pieces {
            let inside =
                image.full.start <= piece.source.start && piece.source.end <= image.full.end;
            if !inside && !piece.text.chars().all(char::is_whitespace) {
                return None;
            }
        }
        let MarkKind::Image { destination } = &image.kind else {
            return None;
        };
        Some((image.full.clone(), destination.clone()))
    }

    fn close_block(&mut self, range: Range<usize>) {
        let Some(mut open) = self.open.pop() else {
            return;
        };
        let mut pieces = std::mem::take(&mut open.pieces);
        // A tight item's own inline content becomes the paragraph it was missing.
        if let Some(loose) = open.loose.take() {
            let body = BlockBody::Inline(std::mem::take(&mut pieces));
            let full = self.wrapped(&loose);
            open.block.push(Block {
                kind: BlockKind::Paragraph,
                span: loose,
                full,
                body,
            });
        }
        match open.block.kind {
            // A leaf's span is its content: markers and trailing newline stay out of it.
            BlockKind::Paragraph => match open.only_math.take() {
                Some(math) => {
                    open.block = Block {
                        kind: BlockKind::Math,
                        span: math,
                        full: open.block.full,
                        body: BlockBody::Inline(pieces),
                    };
                }
                None => {
                    // v1 draws a top-level block only; a list or quote keeps its source.
                    let top_level = self.open.is_empty();
                    let image = top_level
                        .then(|| self.only_image(&pieces, &open.block.full))
                        .flatten();
                    match image {
                        Some((range, destination)) => {
                            open.block.kind = BlockKind::Image { destination };
                            // The source view draws the whole construct, not the alt text
                            // the paragraph's content range would narrow it to.
                            open.block.span = range;
                        }
                        None => {
                            if let Some(content) = open.content.take() {
                                open.block.span = content;
                            }
                        }
                    }
                    open.block.body = BlockBody::Inline(pieces);
                }
            },
            BlockKind::Heading(_) | BlockKind::CodeBlock { .. } | BlockKind::TableCell => {
                if let Some(content) = open.content.take() {
                    open.block.span = content;
                }
                open.block.body = BlockBody::Inline(pieces);
            }
            // A container's span is the whole construct, markers and newlines included.
            _ => open.block.span = open.block.span.start..range.end,
        }
        // An item with neither a block nor inline content still owns the place its text
        // would go. Without a block there is nothing to draw and no view to put a caret
        // in, so a line such as `1. ` would be invisible and unreachable.
        if matches!(open.block.kind, BlockKind::ListItem { .. }) && open.block.children().is_empty()
        {
            let at = item_content_start(self.source, &open.block.span);
            open.block.push(Block {
                kind: BlockKind::Paragraph,
                span: at..at,
                full: at..at,
                body: BlockBody::Inline(Vec::new()),
            });
        }
        self.attach(open.block);
    }

    /// Attach a finished block to its parent, or to the document when no container is open.
    fn attach(&mut self, block: Block) {
        match self.open.last_mut() {
            Some(parent) => parent.block.push(block),
            None => self.done.push(block),
        }
    }

    /// A tight item's inline content becomes a paragraph before anything follows it.
    fn flush_loose(&mut self) {
        let Some(loose) = self.open.last_mut().and_then(|top| top.loose.take()) else {
            return;
        };
        let full = self.wrapped(&loose);
        let Some(top) = self.open.last_mut() else {
            return;
        };
        let pieces = std::mem::take(&mut top.pieces);
        top.block.push(Block {
            kind: BlockKind::Paragraph,
            span: loose,
            full,
            body: BlockBody::Inline(pieces),
        });
    }

    /// The construct a synthesized paragraph came from. A tight list item emits no
    /// paragraph event, so the marks inside its content stand in for one: widen to their
    /// delimiters until no further mark is touched.
    fn wrapped(&self, span: &Range<usize>) -> Range<usize> {
        let mut full = span.clone();
        loop {
            let before = full.clone();
            for mark in &self.marks {
                if mark.full.start < full.end && full.start < mark.full.end {
                    full.start = full.start.min(mark.full.start);
                    full.end = full.end.max(mark.full.end);
                }
            }
            if full == before {
                return full;
            }
        }
    }
}

/// Where a list item's own text starts: past its bullet or number, and the spaces after it.
///
/// The parser reports no event for an item that holds nothing, so this is how the builder
/// finds the byte a caret should sit on in `1. ` or `- `.
fn item_content_start(source: &str, item: &Range<usize>) -> usize {
    let bytes = source.as_bytes();
    let mut at = item.start;
    while at < item.end && bytes.get(at) == Some(&b' ') {
        at += 1;
    }
    if matches!(bytes.get(at), Some(b'-' | b'+' | b'*')) {
        at += 1;
    } else {
        let digits = at;
        while at < item.end && bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if digits == at || !matches!(bytes.get(at), Some(b'.' | b')')) {
            return digits.min(item.end);
        }
        at += 1;
    }
    // CommonMark allows one to four spaces after the marker; a deeper indent belongs to
    // the content, so stop at the first non-space.
    while at < item.end && bytes.get(at) == Some(&b' ') {
        at += 1;
    }
    at.min(item.end)
}

fn is_container(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Quote
            | BlockKind::BulletList
            | BlockKind::OrderedList { .. }
            | BlockKind::ListItem { .. }
            | BlockKind::FootnoteDefinition
            | BlockKind::Table { .. }
            | BlockKind::TableRow { .. }
    )
}

fn alignment_of(alignment: ColumnAlignment) -> Alignment {
    match alignment {
        ColumnAlignment::None => Alignment::None,
        ColumnAlignment::Left => Alignment::Left,
        ColumnAlignment::Center => Alignment::Center,
        ColumnAlignment::Right => Alignment::Right,
    }
}

fn level_of(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// The code between its backtick run, applying CommonMark's one-space padding rule.
fn code_content(source: &str, range: Range<usize>) -> Range<usize> {
    let bytes = source.as_bytes();
    let lead = bytes[range.start.min(bytes.len())..]
        .iter()
        .take_while(|byte| **byte == b'`')
        .count();
    let trail = bytes[..range.end.min(bytes.len())]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'`')
        .count();
    let delimiters = lead.min(trail);
    let mut content = range.start + delimiters..range.end.saturating_sub(delimiters);
    let inner = source.get(content.clone()).unwrap_or("");
    if inner.len() > 2 && inner.starts_with(' ') && inner.ends_with(' ') && !inner.trim().is_empty()
    {
        content = content.start + 1..content.end - 1;
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every span points at the bytes it came from: a leaf's at its text, a container's at
    /// the whole construct, and a mark's at the text between its delimiters.
    #[test]
    fn spans_index_the_source_they_came_from() {
        let source = "# Title\n\nA [link](https://e.com) and *em* and `code`.\n\n- one\n- two\n";
        let document = parse(source);
        let text = |span: &Range<usize>| &source[span.clone()];

        assert_eq!(document.blocks.len(), 3, "heading, paragraph, list");
        assert_eq!(document.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(text(&document.blocks[0].span), "Title");
        assert_eq!(document.blocks[0].pieces()[0].text.as_ref(), "Title");

        assert_eq!(document.blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(
            text(&document.blocks[1].span),
            "A [link](https://e.com) and *em* and `code`."
        );

        let list = &document.blocks[2];
        assert_eq!(list.kind, BlockKind::BulletList);
        assert_eq!(list.children().len(), 2);
        for (item, expected) in list.children().iter().zip(["one", "two"]) {
            assert_eq!(item.kind, BlockKind::ListItem { task: None });
            assert_eq!(
                item.children().len(),
                1,
                "a tight item still holds a paragraph"
            );
            assert_eq!(item.children()[0].kind, BlockKind::Paragraph);
            assert_eq!(text(&item.children()[0].span), expected);
            assert_eq!(item.children()[0].pieces()[0].text.as_ref(), expected);
        }

        assert_eq!(document.marks.len(), 3, "link, emphasis, inline code");
        let link = document
            .marks
            .iter()
            .find(|mark| matches!(mark.kind, MarkKind::Link { .. }))
            .expect("the link");
        assert_eq!(text(&link.range), "link");
        assert_eq!(
            link.kind,
            MarkKind::Link {
                destination: "https://e.com".to_owned()
            }
        );
        let emphasis = document
            .marks
            .iter()
            .find(|mark| mark.kind == MarkKind::Emphasis)
            .expect("the emphasis");
        assert_eq!(text(&emphasis.range), "em");
        let code = document
            .marks
            .iter()
            .find(|mark| mark.kind == MarkKind::Code)
            .expect("the code span");
        assert_eq!(text(&code.range), "code");
    }

    /// A leading `---` metadata block is front matter, not a thematic break followed by a
    /// paragraph: it is opaque to layout and kept for editing, fences included, so a mark
    /// inside it never shows on its own.
    #[test]
    fn a_metadata_block_is_front_matter() {
        let source = "---\ntitle: x\n---\n\nbody\n";
        let document = parse(source);
        assert_eq!(document.blocks[0].kind, BlockKind::FrontMatter);
        assert_eq!(document.blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(
            &source[document.blocks[0].span.clone()],
            "---\ntitle: x\n---",
            "the block owns its fences and the payload between them"
        );
    }

    /// An HTML block arrives as its own event with no paragraph around it; the bytes must
    /// still land in a block, or the text is dropped rather than passed through.
    #[test]
    fn an_html_block_keeps_its_bytes() {
        let source = "<div>hi</div>\n";
        let document = parse(source);
        assert_eq!(document.blocks.len(), 1, "one block holds the HTML bytes");
        assert_eq!(
            document.blocks[0].pieces()[0].text.as_ref(),
            "<div>hi</div>\n"
        );

        // The same block inside a container becomes its child, not the container's text.
        let quoted = parse("> <div>hi</div>\n");
        assert_eq!(quoted.blocks[0].kind, BlockKind::Quote);
        assert_eq!(
            quoted.blocks[0].children()[0].pieces()[0].text.as_ref(),
            "<div>hi</div>\n"
        );
    }

    /// A footnote definition is a container whose paragraph is the note; the reference in
    /// the body keeps its bytes until the model grows a reference object.
    #[test]
    fn a_footnote_definition_holds_its_paragraph() {
        let source = "Body[^1].\n\n[^1]: the note\n";
        let document = parse(source);
        let body: String = document.blocks[0]
            .pieces()
            .iter()
            .map(|piece| piece.text.as_ref())
            .collect();
        assert_eq!(
            body, "Body[^1].",
            "the reference's bytes stay in the paragraph"
        );

        let definition = &document.blocks[1];
        assert_eq!(&source[definition.span.clone()], "[^1]: the note\n");
        assert_eq!(
            definition.children().len(),
            1,
            "the note is the definition's child"
        );
        assert_eq!(definition.children()[0].kind, BlockKind::Paragraph);
        assert_eq!(&source[definition.children()[0].span.clone()], "the note");
    }

    /// A reference is one inline construct: its whole written form is what an edit removes,
    /// and the label is what the rendered form draws.
    #[test]
    fn a_footnote_reference_is_a_mark_over_its_bytes() {
        let source = "Body[^1].\n\n[^1]: the note\n";
        let document = parse(source);
        let reference = document
            .marks
            .iter()
            .find(|mark| matches!(mark.kind, MarkKind::FootnoteReference { .. }))
            .expect("the reference is a mark");
        assert_eq!(&source[reference.range.clone()], "[^1]");
        assert_eq!(&source[reference.full.clone()], "[^1]");
        assert!(matches!(&reference.kind, MarkKind::FootnoteReference { label } if label == "1"));
    }

    /// A soft break is a space, not a line ending: CommonMark parses the two lines as one
    /// paragraph, and a fence may not open mid-line, so the text flows as one run.
    #[test]
    fn a_soft_break_is_a_space() {
        let document = parse("one\ntwo\n\nthree\n");
        let paragraphs = document
            .blocks
            .iter()
            .map(|block| {
                block
                    .pieces()
                    .iter()
                    .map(|piece| piece.text.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert_eq!(paragraphs, ["one two", "three"]);
    }

    /// A quote nests as a child block. A line with fewer markers continues the inner
    /// paragraph instead of ending it — CommonMark's lazy continuation — so the bytes
    /// between the two lines are the inner block's own syntax, and the text reads as one
    /// paragraph rather than as two blocks.
    #[test]
    fn a_quote_nests_and_a_lazier_line_continues_the_inner_paragraph() {
        let source = "> outer\n> > inner\n> back\n";
        let document = parse(source);
        let outer = &document.blocks[0];
        assert_eq!(outer.kind, BlockKind::Quote);
        assert_eq!(
            outer.children().len(),
            2,
            "a paragraph, then a nested quote"
        );
        assert_eq!(outer.children()[0].kind, BlockKind::Paragraph);
        let inner = &outer.children()[1];
        assert_eq!(inner.kind, BlockKind::Quote);
        let paragraph = &inner.children()[0];
        let text: String = paragraph
            .pieces()
            .iter()
            .map(|piece| piece.text.as_ref())
            .collect();
        assert_eq!(text, "inner back");
    }

    /// An entity is one mark over its whole written form, so no caret can land inside it;
    /// code spans and fenced blocks keep the source undecoded and carry no mark.
    #[test]
    fn an_entity_mark_covers_its_whole_written_form_and_code_keeps_source() {
        let source = "AT&amp;T &#65;\n\n`&amp;`\n\n```\n&amp; &#65;\n```\n";
        let document = parse(source);
        let entities: Vec<(Range<usize>, Range<usize>)> = document
            .marks
            .iter()
            .filter(|mark| mark.kind == MarkKind::Entity)
            .map(|mark| (mark.range.clone(), mark.full.clone()))
            .collect();
        assert_eq!(
            entities,
            [(2..7, 2..7), (9..14, 9..14)],
            "each entity is one mark over its bytes, delimiters included"
        );
        assert_eq!(&source[entities[0].0.clone()], "&amp;");
        assert_eq!(&source[entities[1].0.clone()], "&#65;");
        assert_eq!(
            document
                .marks
                .iter()
                .filter(|mark| mark.kind == MarkKind::Entity)
                .count(),
            2,
            "the code span and the fenced block hold no entity mark"
        );
    }

    /// Every representative edit must leave the document exactly what a full parse of the
    /// new source produces: the same blocks, marks and order. The edits are applied in
    /// sequence, so each result is also the previous document for the next edit.
    #[test]
    fn a_reparse_matches_a_full_parse_after_every_representative_edit() {
        const BASE: &str = r#"# Heading one

First paragraph with **bold**, *emphasis*, `code`, ~~strike~~, a [link](https://example.com) and $a+b$.

> quote line
> > nested quote
> lazy line

- alpha
  - nested alpha
  - nested beta
- beta

1. first
2. second

```rust
fn main() {
    let x = 1;
}
```

AT&amp;T and &#65; entity.

last paragraph near a café
"#;
        let mut source = BASE.to_owned();
        let mut previous = parse(&source);
        let mut prepared = crate::Prepared::of(&previous);

        source = replace_once(&source, "First paragraph", "First edited paragraph");
        previous = reparse_matches(&previous, &mut prepared, &source, "insert in a paragraph");

        source = replace_once(&source, ", *emphasis*", "");
        previous = reparse_matches(
            &previous,
            &mut prepared,
            &source,
            "delete part of a paragraph",
        );

        source = replace_once(
            &source,
            "- alpha\n  - nested alpha",
            "- alpha\n  more alpha\n  - nested alpha",
        );
        previous = reparse_matches(
            &previous,
            &mut prepared,
            &source,
            "break inside a list item",
        );

        source = replace_once(
            &source,
            "entity.\n\nlast paragraph",
            "entity.\nlast paragraph",
        );
        previous = reparse_matches(
            &previous,
            &mut prepared,
            &source,
            "delete the blank line between two paragraphs",
        );

        source.insert_str(0, "Intro at the start.\n\n");
        previous = reparse_matches(
            &previous,
            &mut prepared,
            &source,
            "insert at the document start",
        );

        source.push_str("\nAppended final line.\n");
        previous = reparse_matches(
            &previous,
            &mut prepared,
            &source,
            "append at the document end",
        );

        source = replace_once(&source, "let x = 1;", "let answer = 42;");
        previous = reparse_matches(&previous, &mut prepared, &source, "edit inside a fence");

        source = replace_once(
            &source,
            "  - nested beta\n- beta\n\n1. first\n2. second\n",
            "",
        );
        previous = reparse_matches(&previous, &mut prepared, &source, "delete across blocks");

        source = replace_once(
            &source,
            "> quote line\n> > nested quote\n> lazy line\n\n",
            "",
        );
        previous = reparse_matches(
            &previous,
            &mut prepared,
            &source,
            "delete a top-level block",
        );

        source = replace_once(&source, "# Heading one", "## Heading two");
        previous = reparse_matches(&previous, &mut prepared, &source, "edit a heading");

        source = replace_once(
            &source,
            "AT&amp;T and &#65; entity.",
            "AT&T and &#65; entity.",
        );
        previous = reparse_matches(&previous, &mut prepared, &source, "drop an entity");

        source = replace_once(&source, "café", "cafê");
        reparse_matches(
            &previous,
            &mut prepared,
            &source,
            "change a byte inside a multi-byte character",
        );
    }

    fn reparse_matches(
        previous: &Document,
        prepared: &mut crate::Prepared,
        source: &str,
        what: &str,
    ) -> Document {
        let reparsed = previous.reparse(source);
        let full = parse(source);
        assert_eq!(reparsed.document.source, full.source, "{what}: source");
        assert_eq!(reparsed.document.blocks, full.blocks, "{what}: blocks");
        assert_eq!(reparsed.document.marks, full.marks, "{what}: marks");
        let updated = crate::Prepared::update(prepared, &reparsed.document, &reparsed.splice);
        let reprojected = crate::Prepared::of(&reparsed.document);
        assert_eq!(updated, reprojected, "{what}: prepared");
        *prepared = updated;
        reparsed.document
    }

    /// A previous model whose window edge is not a block boundary cannot be spliced; the
    /// result must still be the full parse rather than overlapping retained blocks.
    #[test]
    fn a_window_that_does_not_land_on_a_block_boundary_falls_back_to_a_full_parse() {
        let source = "alpha\n\nbeta\n\ngamma\n";
        let mut previous = parse(source);
        // Corrupt the guard block so the window starts inside the first paragraph and the
        // window parse begins with two newlines.
        previous.blocks[0].full.end = 8;
        previous.blocks[0].span.end = 8;
        previous.blocks[1].full.start = 5;
        previous.blocks[1].span.start = 5;

        let edited = source.replace("gamma", "delta");
        let incremental = previous.reparse(&edited);
        assert_eq!(incremental.document, parse(&edited));
        assert_eq!(incremental.splice, Splice::Whole);
    }

    /// A splice that does not describe the document it is applied to must not reuse a
    /// frame; the result is still the full projection.
    #[test]
    fn an_update_that_cannot_trust_its_splice_falls_back_to_a_full_projection() {
        let source = "alpha\n\nbeta\n\ngamma\n";
        let previous = parse(source);
        let prepared = crate::Prepared::of(&previous);
        let edited = source.replace("gamma", "delta");
        let reparsed = previous.reparse(&edited);
        let expected = crate::Prepared::of(&reparsed.document);

        // The named block range is empty although the document has blocks.
        let misdirected = Splice::Window {
            old: 0..source.len(),
            delta: isize::try_from(edited.len() - source.len()).expect("a small delta"),
            blocks: reparsed.document.blocks.len()..reparsed.document.blocks.len(),
        };
        assert_eq!(
            crate::Prepared::update(&prepared, &reparsed.document, &misdirected),
            expected
        );

        // A projection cut from another source cannot follow this document's offsets.
        let stale = crate::Prepared::of(&parse("something else entirely\n"));
        assert_eq!(
            crate::Prepared::update(&stale, &reparsed.document, &reparsed.splice),
            expected
        );
    }

    fn replace_once(source: &str, from: &str, to: &str) -> String {
        let at = source
            .find(from)
            .unwrap_or_else(|| panic!("the sample does not hold {from:?}"));
        let mut out = String::with_capacity(source.len() + to.len());
        out.push_str(&source[..at]);
        out.push_str(to);
        out.push_str(&source[at + from.len()..]);
        out
    }

    /// How much of a large document one edit costs. Ignored by default because it measures
    /// time; run with `cargo test --release -p ui-richtext -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing measurement"]
    fn a_small_edit_on_a_large_document_costs_less_than_a_full_parse() {
        use std::time::Instant;

        let mut source = String::new();
        for index in 0..2000 {
            source.push_str(&format!(
                "Paragraph {index:04} holds enough words to fill one line of the page, with **bold** text and a [link](https://example.com/{index}).\n\n"
            ));
        }
        let previous = parse(&source);
        let previous_prepared = crate::Prepared::of(&previous);
        let mut edited = source.clone();
        let middle = edited.len() / 2;
        let line = edited[..middle].rfind('\n').map_or(0, |at| at + 1);
        edited.insert_str(line, "edited ");

        let full = parse(&edited);
        let incremental = previous.reparse(&edited);
        assert_eq!(incremental.document.blocks, full.blocks);
        assert_eq!(incremental.document.marks, full.marks);
        assert_eq!(
            crate::Prepared::update(
                &previous_prepared,
                &incremental.document,
                &incremental.splice
            ),
            crate::Prepared::of(&incremental.document)
        );

        // Warm the allocator and the parser, then average enough rounds that one edit's
        // milliseconds are not noise.
        std::hint::black_box(parse(&edited));
        std::hint::black_box(previous.reparse(&edited));
        std::hint::black_box(crate::Prepared::of(&incremental.document));
        std::hint::black_box(crate::Prepared::update(
            &previous_prepared,
            &incremental.document,
            &incremental.splice,
        ));
        let rounds = 20;
        let start = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(parse(&edited));
        }
        let parse_time = start.elapsed() / rounds;
        let start = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(previous.reparse(&edited));
        }
        let reparse_time = start.elapsed() / rounds;
        let start = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(previous.clone());
        }
        let clone_time = start.elapsed() / rounds;
        let start = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(crate::Prepared::of(&incremental.document));
        }
        let prepared_time = start.elapsed() / rounds;
        let start = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(crate::Prepared::update(
                &previous_prepared,
                &incremental.document,
                &incremental.splice,
            ));
        }
        let update_time = start.elapsed() / rounds;
        eprintln!(
            "source {} bytes / 2000 paragraphs: parse {parse_time:?}, reparse {reparse_time:?}, clone {clone_time:?}, prepared {prepared_time:?}, update {update_time:?}",
            edited.len()
        );
    }

    /// An image mark covers the whole `![alt](src)` construct, and a paragraph holding
    /// nothing else becomes an image block; text beside the image keeps it a paragraph.
    #[test]
    fn a_pure_image_paragraph_is_an_image_block_covering_its_source() {
        let source = "![a](b)\n";
        let document = parse(source);
        let images: Vec<&Mark> = document
            .marks
            .iter()
            .filter(|mark| matches!(mark.kind, MarkKind::Image { .. }))
            .collect();
        assert_eq!(images.len(), 1, "the image is one mark");
        assert_eq!(images[0].full, 0..7, "the mark covers every source byte");
        assert_eq!(&source[images[0].full.clone()], "![a](b)");
        assert_eq!(
            &source[images[0].range.clone()],
            "a",
            "its content is the alt text"
        );
        assert_eq!(
            document.blocks[0].kind,
            BlockKind::Image {
                destination: "b".to_owned()
            }
        );
        assert_eq!(
            document.blocks[0].span,
            0..7,
            "the block's span is the whole construct"
        );

        let spaced = parse("  ![a](b)  \n");
        assert_eq!(
            spaced.blocks[0].kind,
            BlockKind::Image {
                destination: "b".to_owned()
            },
            "whitespace around the image does not make it inline"
        );
        assert_eq!(&spaced.source[spaced.blocks[0].span.clone()], "![a](b)");

        let empty = parse("![](b)\n");
        assert_eq!(
            empty.blocks[0].kind,
            BlockKind::Image {
                destination: "b".to_owned()
            },
            "an empty alt text still makes an image block"
        );
        assert_eq!(empty.blocks[0].span, 0..6);

        let mixed = parse("see ![a](b) here\n");
        assert_eq!(
            mixed.blocks[0].kind,
            BlockKind::Paragraph,
            "an image beside text stays source in v1"
        );

        let listed = parse("- ![a](b)\n");
        assert_eq!(
            listed.blocks[0].children()[0].children()[0].kind,
            BlockKind::Paragraph,
            "an image nested in a list stays source in v1"
        );
    }

    /// A paragraph holding only `$$…$$` is a formula block; an inline `$…$` is a mark.
    #[test]
    fn formulas_keep_their_source_without_their_markers() {
        let source = "$$\nx + 1\n$$\n\nsee $a+b$ here\n";
        let document = parse(source);
        assert_eq!(document.blocks.len(), 2);
        assert_eq!(document.blocks[0].kind, BlockKind::Math);
        assert_eq!(&source[document.blocks[0].span.clone()], "\nx + 1\n");
        assert_eq!(document.blocks[1].kind, BlockKind::Paragraph);
        let inline = document
            .marks
            .iter()
            .find(|mark| mark.kind == MarkKind::InlineMath)
            .expect("the inline formula");
        assert_eq!(&source[inline.range.clone()], "a+b");
    }

    /// A GFM table is one block whose rows hold cells, and the delimiter row's colons are
    /// the alignment of the columns. The table's `full` covers every source byte, including
    /// the delimiter row and the line breaks, because the source state edits all of it.
    #[test]
    fn a_gfm_table_is_one_block_with_aligned_columns_and_rows_of_cells() {
        let source = "| a | bb |\n|:--|--:|\n| c | dd |\n";
        let document = parse(source);
        assert_eq!(
            document.blocks.len(),
            1,
            "one table, not paragraphs of pipes"
        );
        let table = &document.blocks[0];
        assert_eq!(
            table.kind,
            BlockKind::Table {
                alignments: vec![Alignment::Left, Alignment::Right],
            }
        );
        assert_eq!(
            &source[table.full.clone()],
            source,
            "`full` covers the table"
        );
        assert_eq!(&source[table.span.clone()], source);

        let rows = table.children();
        assert_eq!(rows.len(), 2, "header and body");
        assert_eq!(rows[0].kind, BlockKind::TableRow { header: true });
        assert_eq!(rows[1].kind, BlockKind::TableRow { header: false });
        for row in rows {
            let cells = row.children();
            assert_eq!(cells.len(), 2);
            assert!(cells.iter().all(|cell| cell.kind == BlockKind::TableCell));
        }
        let text = |block: &Block| source[block.span.clone()].to_owned();
        assert_eq!(
            rows.iter()
                .flat_map(|row| row.children().iter().map(text))
                .collect::<Vec<_>>(),
            ["a", "bb", "c", "dd"]
        );
    }

    /// A GFM task item records its box, and the marker bytes stay with the item: the
    /// paragraph's span starts after `[x] `, while `marker()` still reaches those bytes so
    /// an edit can rewrite them.
    #[test]
    fn a_task_item_records_its_box_and_leaves_the_marker_bytes_reachable() {
        let source = "- [x] done\n- [ ] todo\n- plain\n";
        let document = parse(source);
        let items = document.blocks[0].children();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, BlockKind::ListItem { task: Some(true) });
        assert_eq!(items[1].kind, BlockKind::ListItem { task: Some(false) });
        assert_eq!(items[2].kind, BlockKind::ListItem { task: None });

        let body = |item: &Block| {
            item.children()
                .iter()
                .flat_map(|child| child.pieces().iter().map(|piece| piece.text.as_ref()))
                .collect::<String>()
        };
        assert_eq!(body(&items[0]), "done", "the box is not part of the text");
        assert_eq!(body(&items[1]), "todo");

        let gap = crate::editor::marker(&document, &items[0]).expect("the item keeps a marker");
        assert_eq!(
            &source[gap], "- [x] ",
            "the box and its bullet are the marker"
        );
    }
}
