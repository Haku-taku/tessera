//! A block document: the tree a Markdown source becomes before layout.
//!
//! Blocks carry source byte spans and nest; inline attributes are a flat list of marks
//! over the source, not part of the tree, so a link or a comment may cross a block.

use std::ops::Range;

pub mod blank;
pub mod editor;
pub mod entity;
pub mod inline;
pub mod language;
pub mod markdown;
pub mod ops;
pub mod scene;
pub mod state;

pub use editor::{Construct, ConstructKind, Prepared, Projection, Run, Shown, marker, project};
pub use inline::to_text_block;
pub use markdown::{Reparse, Splice, parse};
pub use ops::{Edit, indent, line_break};
pub use scene::{
    Cell, CellLook, ObjectKind, Part, Role, Row, RowKind, RowStyle, Scene, build as build_scene,
};
pub use state::{Form, Scope};

/// A range moved by a signed byte offset, or nothing when an end would wrap.
pub(crate) fn shifted_range(range: &Range<usize>, shift: isize) -> Option<Range<usize>> {
    Some(range.start.checked_add_signed(shift)?..range.end.checked_add_signed(shift)?)
}

/// A document, with every node pinpointed by byte offsets into `source`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    /// The bytes the offsets refer to. An editor slices this to apply an edit, while
    /// layout reads the decoded text the parser produced.
    pub source: std::sync::Arc<str>,
    /// Top-level blocks, in reading order.
    pub blocks: Vec<Block>,
    /// Inline attributes over source bytes, in no particular order.
    pub marks: Vec<Mark>,
}

/// One block: a leaf such as a paragraph, or a container such as a quote.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    /// The bytes this block owns: a leaf's text, with its markers and trailing newline
    /// excluded, or a container's whole construct, markers included.
    pub span: Range<usize>,
    /// The whole source construct, markers and trailing newline included, as the parser
    /// reported it. Layout reads `span`; an editor edits `full` and hides what `span`
    /// excludes.
    pub full: Range<usize>,
    pub body: BlockBody,
}

/// What a block holds. Nothing here assumes a leaf is text alone: an inline object such
/// as a formula draws beside the pieces rather than as their glyphs.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockBody {
    /// Nested blocks: a quote, a list item, a table cell.
    Blocks(Vec<Block>),
    /// The leaf's own text, in source order.
    Inline(Vec<Piece>),
}

/// A run of decoded text and the source bytes it came from.
///
/// The two differ wherever the source spelled a character as an entity or an escape: the
/// entity is 5 bytes, the character it becomes is 1. Both are kept, because layout reads
/// the text and an edit, a mark or a caret indexes the source.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub source: Range<usize>,
    pub text: std::sync::Arc<str>,
}

impl Block {
    /// Attach a nested block. A leaf has no children, so the call is a no-op there.
    pub fn push(&mut self, block: Block) {
        if let BlockBody::Blocks(blocks) = &mut self.body {
            blocks.push(block);
        }
    }

    /// The nested blocks, or nothing for a leaf.
    pub fn children(&self) -> &[Block] {
        match &self.body {
            BlockBody::Blocks(blocks) => blocks,
            BlockBody::Inline(_) => &[],
        }
    }

    /// The leaf's text, or nothing for a container.
    pub fn pieces(&self) -> &[Piece] {
        match &self.body {
            BlockBody::Inline(pieces) => pieces,
            BlockBody::Blocks(_) => &[],
        }
    }
}

/// What a block is. The container kinds nest; the rest are leaves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// Heading level, 1 to 6.
    Heading(u8),
    /// Fenced or indented code; `language` is the fence's info string.
    CodeBlock {
        language: Option<String>,
        /// Whether the source wrote a fence. An indented block has none, so its first and
        /// last lines are code rather than delimiters.
        fenced: bool,
    },
    Quote,
    BulletList,
    OrderedList {
        start: u64,
    },
    /// One list item; `task` is the GFM task-list box it opens with, if any.
    ListItem {
        task: Option<bool>,
    },
    /// A thematic break; its span is the marker only.
    Rule,
    /// A display formula, opaque to layout until a math engine lays it out.
    Math,
    /// A paragraph holding nothing but one image; `destination` is the source in its
    /// parentheses. The paragraph's bytes stay its `span`, so an edit still sees them.
    Image {
        destination: String,
    },
    /// A leading metadata block, opaque to layout and kept for editing.
    FrontMatter,
    /// A footnote definition: `[^label]:` and the blocks it holds. Its span covers the
    /// whole construct, marker included, because the marker is the note's only name.
    FootnoteDefinition,
    /// A GFM table. Its children are [`BlockKind::TableRow`]s, each holding
    /// [`BlockKind::TableCell`]s, and `alignments` names each column's text alignment.
    Table {
        alignments: Vec<Alignment>,
    },
    /// One table row; `header` marks the row the delimiter row names.
    TableRow {
        header: bool,
    },
    /// One table cell; its `span` is the text between the cell's padding, while `full`
    /// covers that padding too.
    TableCell,
}

/// How a GFM table column aligns its cells, from the delimiter row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Alignment {
    /// No colon: the source puts no alignment, which reads as left.
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// An attribute over a byte range of the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The bytes the attribute covers, delimiters excluded.
    pub range: Range<usize>,
    /// The whole construct, delimiters included: `**a**` is `range` over `a` and `full`
    /// over all five bytes. An editor hides `full` minus `range`.
    pub full: Range<usize>,
    pub kind: MarkKind,
}

/// The inline attributes a document can carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// `*text*`
    Emphasis,
    /// `**text**`
    Strong,
    /// Inline code: `` `text` ``.
    Code,
    /// `~~text~~`
    Strikethrough,
    /// `$…$`, as the formula's own source bytes.
    InlineMath,
    /// An HTML entity over its whole written form, so a caret cannot sit inside its bytes;
    /// [`crate::entity::decode`] is the reader's pure decoding step.
    Entity,
    /// A link's text; the destination is resolved by the parser, not by layout.
    Link { destination: String },
    /// An image: `range` is its alt text and `full` the whole `![alt](src)`.
    Image { destination: String },
    /// `[^label]` in the body. The range is the whole reference, because its bytes are
    /// what an edit removes; the rendered form draws the label instead.
    FootnoteReference { label: String },
}

impl Document {
    /// The marks overlapping `range`, each content range clipped to it.
    ///
    /// Clipping is what lets a mark cross blocks: each block asks for its own slice and
    /// gets a range it can turn into runs without knowing about the other blocks. `full`
    /// stays the whole construct, delimiters included, because a caller asking "is the
    /// caret inside this mark?" needs the `$` around the slice as much as the slice.
    pub fn marks_in(&self, range: Range<usize>) -> Vec<Mark> {
        if range.start >= range.end {
            return Vec::new();
        }
        self.marks
            .iter()
            .filter_map(|mark| {
                let start = mark.range.start.max(range.start);
                let end = mark.range.end.min(range.end);
                (start < end).then(|| Mark {
                    range: start..end,
                    full: mark.full.clone(),
                    kind: mark.kind.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A link may start in one paragraph and end in the next: the mark lives over the
    /// source, and each block sees only its own slice.
    #[test]
    fn a_link_spanning_two_paragraphs_resolves_inside_each() {
        let document = Document {
            source: "intro\ntarget text\n".into(),
            blocks: vec![
                Block {
                    kind: BlockKind::Paragraph,
                    span: 0..5,
                    full: 0..6,
                    body: BlockBody::Inline(vec![Piece {
                        source: 0..5,
                        text: "intro".into(),
                    }]),
                },
                Block {
                    kind: BlockKind::Paragraph,
                    span: 6..17,
                    full: 6..18,
                    body: BlockBody::Inline(vec![Piece {
                        source: 6..17,
                        text: "target text".into(),
                    }]),
                },
            ],
            marks: vec![Mark {
                range: 3..12,
                full: 0..18,
                kind: MarkKind::Link {
                    destination: "https://example.com".to_owned(),
                },
            }],
        };

        let first = document.marks_in(document.blocks[0].span.clone());
        assert_eq!(first.len(), 1, "the first paragraph holds the link's tail");
        assert_eq!(first[0].range, 3..5);
        let second = document.marks_in(document.blocks[1].span.clone());
        assert_eq!(second.len(), 1, "and the second paragraph its head");
        assert_eq!(second[0].range, 6..12);
        assert!(
            document.marks_in(13..17).is_empty(),
            "a block past the mark's end carries nothing"
        );
    }
}
