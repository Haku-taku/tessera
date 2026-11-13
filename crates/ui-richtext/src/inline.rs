//! A leaf block as a paragraph: its text split where marks begin and end.
//!
//! The document states semantics — emphasis, a link — and never a look; the caller's
//! `look` decides what each mark means for the paragraph it is building.

use ui_core::{AtomBox, InlineObject, OBJECT, TextBlock, TextRun, TextStyle};

use crate::{Block, BlockBody, Document, MarkKind, Piece};

/// Layout input for one leaf block.
///
/// `look` is called once per run with the marks covering it, and may set ink, weight,
/// decoration or anything else a run carries. `object` may replace a mark's text with an
/// inline object — a formula, later an image — which occupies one U+FFFC and reserves its
/// own box; returning `None` keeps the text. A block that is not a leaf yields an empty
/// paragraph.
pub fn to_text_block(
    document: &Document,
    block: &Block,
    base: TextStyle,
    mut look: impl FnMut(&mut TextRun, &[&MarkKind]),
    mut object: impl FnMut(&[&MarkKind], &str) -> Option<InlineObject>,
) -> TextBlock {
    let BlockBody::Inline(pieces) = &block.body else {
        return TextBlock::default();
    };
    // Marks are clipped to the leaf already, so every boundary below is inside it.
    let marks = document.marks_in(block.span.clone());
    let mut runs = Vec::new();
    let mut atoms = Vec::new();
    // The flattened offset the next run starts at, which is where an atom's U+FFFC sits.
    let mut at = 0usize;
    for piece in pieces {
        for (start, end) in cuts(piece, &marks) {
            let Some(from) = decoded_offset(piece, start) else {
                continue;
            };
            let Some(to) = decoded_offset(piece, end) else {
                continue;
            };
            let Some(text) = piece.text.get(from..to) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let kinds: Vec<&MarkKind> = marks
                .iter()
                .filter(|mark| mark.range.start <= start && end <= mark.range.end)
                .map(|mark| &mark.kind)
                .collect();
            if let Some(object) = object(&kinds, text) {
                // The mark's source range still covers the formula, so an edit slices the
                // source; only what layout draws changed.
                atoms.push(AtomBox::new(at, object.size()).object(object));
                let mut run = TextRun::new(String::from(OBJECT), base);
                look(&mut run, &kinds);
                at += run.text.len();
                runs.push(run);
                continue;
            }
            let mut run = TextRun::new(text, base);
            look(&mut run, &kinds);
            at += run.text.len();
            runs.push(run);
        }
    }
    TextBlock {
        runs,
        atoms,
        ..Default::default()
    }
}

/// Where to cut one piece in source bytes: its own edges, plus every mark boundary inside.
///
/// A piece whose text is shorter than its source spelled an entity, and byte arithmetic
/// no longer maps one offset to the other; such a piece stays whole, because a delimiter
/// never sits inside an entity, so no mark boundary can land inside it.
fn cuts(piece: &Piece, marks: &[crate::Mark]) -> Vec<(usize, usize)> {
    let mut edges = vec![piece.source.start, piece.source.end];
    if piece.text.len() == piece.source.len() {
        for mark in marks {
            for edge in [mark.range.start, mark.range.end] {
                if piece.source.start < edge && edge < piece.source.end {
                    edges.push(edge);
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
    }
    edges.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

/// Where a source offset sits in the decoded text.
fn decoded_offset(piece: &Piece, source: usize) -> Option<usize> {
    if piece.text.len() == piece.source.len() {
        return Some(source - piece.source.start);
    }
    if source == piece.source.start {
        Some(0)
    } else if source == piece.source.end {
        Some(piece.text.len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockKind, Mark};

    fn paragraph(span: std::ops::Range<usize>, text: &str) -> Block {
        Block {
            kind: BlockKind::Paragraph,
            span: span.clone(),
            full: span.clone(),
            body: BlockBody::Inline(vec![Piece {
                source: span,
                text: text.into(),
            }]),
        }
    }

    /// A mark splits the text it covers out of the run around it, and only that part
    /// carries the mark.
    #[test]
    fn a_mark_cuts_its_text_out_of_the_run_around_it() {
        let document = Document {
            source: "abcdef".into(),
            blocks: Vec::new(),
            marks: vec![Mark {
                range: 2..4,
                full: 1..5,
                kind: MarkKind::Emphasis,
            }],
        };
        let block = paragraph(0..6, "abcdef");
        let mut seen: Vec<(String, Vec<MarkKind>)> = Vec::new();
        let text = to_text_block(
            &document,
            &block,
            TextStyle::new(16.0),
            |run, marks| {
                seen.push((
                    run.text.to_string(),
                    marks.iter().map(|kind| (*kind).clone()).collect(),
                ));
            },
            |_, _| None,
        );
        let texts: Vec<&str> = text.runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(texts, ["ab", "cd", "ef"]);
        assert_eq!(seen[0].1.len(), 0);
        assert_eq!(seen[1].1, [MarkKind::Emphasis]);
        assert_eq!(seen[2].1.len(), 0);
    }

    /// A mark the caller turns into an object leaves one U+FFFC in the run and an atom at
    /// that offset, sized by the object; the same mark with no object keeps its text.
    #[test]
    fn an_object_replaces_its_mark_with_one_atom() {
        // The parser keeps the delimiters out of the text, so the mark covers the formula.
        let document = Document {
            source: "ab$xy$cd".into(),
            blocks: Vec::new(),
            marks: vec![Mark {
                range: 2..4,
                full: 1..5,
                kind: MarkKind::InlineMath,
            }],
        };
        let block = paragraph(0..6, "abxycd");
        let mut asked = Vec::new();
        let text = to_text_block(
            &document,
            &block,
            TextStyle::new(16.0),
            |_run, _marks| {},
            |marks, source| {
                let inline = marks
                    .iter()
                    .any(|kind| matches!(kind, MarkKind::InlineMath));
                asked.push((inline, source.to_owned()));
                inline.then(|| InlineObject::Math {
                    formula: ui_core::Formula {
                        width: 30.0,
                        ascent: 9.0,
                        descent: 3.0,
                        items: Vec::new(),
                        errors: Vec::new(),
                    },
                    family: "Test Math".to_owned(),
                    ink: ui_core::Color::WHITE,
                })
            },
        );
        assert_eq!(
            asked,
            [
                (false, "ab".to_owned()),
                (true, "xy".to_owned()),
                (false, "cd".to_owned())
            ]
        );
        let joined: String = text.runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(joined, format!("ab{OBJECT}cd"));
        assert_eq!(text.atoms.len(), 1);
        assert_eq!(
            text.atoms[0].at, 2,
            "the atom sits where the mark's text began"
        );
        assert_eq!(text.atoms[0].size, ui_core::Size::new(30.0, 12.0));

        let kept = to_text_block(
            &document,
            &block,
            TextStyle::new(16.0),
            |_run, _marks| {},
            |_, _| None,
        );
        let joined: String = kept.runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(
            joined, "abxycd",
            "no object keeps the mark's source in the run"
        );
        assert!(kept.atoms.is_empty());
    }

    /// The paragraph lays out the decoded text, not the escapes the source spelled.
    #[test]
    fn the_paragraph_reads_decoded_text() {
        let source = "AT&amp;T and *em*\n";
        let document = crate::parse(source);
        let block = &document.blocks[0];
        let text = to_text_block(
            &document,
            block,
            TextStyle::new(16.0),
            |_run, marks| {
                let _ = marks;
            },
            |_, _| None,
        );
        let joined: String = text.runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(joined, "AT&T and em");
        assert!(
            !joined.contains("&amp;"),
            "the entity must not reach layout as source"
        );
    }
}
