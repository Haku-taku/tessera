//! Selection: where it is, and how a style draws it.

use crate::draw::RectPrim;
use crate::geom::{Color, Rect};

/// A selection in a text buffer, as two byte offsets.
///
/// The anchor stays where the gesture began and the caret follows the pointer or keys;
/// which one is smaller is not the reader's business, so `range` normalises them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub caret: usize,
}

impl TextSelection {
    pub const fn new(anchor: usize, caret: usize) -> Self {
        Self { anchor, caret }
    }

    pub const fn collapsed(at: usize) -> Self {
        Self {
            anchor: at,
            caret: at,
        }
    }

    /// The selected bytes, start first.
    pub fn range(&self) -> std::ops::Range<usize> {
        if self.anchor <= self.caret {
            self.anchor..self.caret
        } else {
            self.caret..self.anchor
        }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.caret
    }

    /// Whether a click at `index` belongs to this selection: inside it or on either edge.
    pub fn covers(&self, index: usize) -> bool {
        let range = self.range();
        index >= range.start && index <= range.end
    }
}

/// One visual fragment of a selection: a run of selected glyphs on one line, or the
/// block standing for a selected line break.
///
/// A wrapped or bidirectional selection has one fragment per visual line piece, so a
/// painter never has to re-break the text to know where the highlight goes. A line break
/// draws no glyph of its own, so a selection that covers one carries a fragment of its
/// own for it: a small block past the line's ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionFragment {
    /// Where it sits on the surface; one line box tall, or a small block for a break.
    pub rect: Rect,
    /// Line index inside the block.
    pub line: usize,
    /// The selection's first fragment. A break's block is never first or last.
    pub first: bool,
    /// The selection's last fragment.
    pub last: bool,
    /// This fragment reads right to left.
    pub rtl: bool,
    /// This fragment marks a selected line break rather than selected glyphs.
    pub line_break: bool,
}

/// What a painter is given: geometry plus the colours in force.
#[derive(Clone, Copy, Debug)]
pub struct SelectionGeometry<'a> {
    pub fragments: &'a [SelectionFragment],
    /// The selection colour the style resolved; a painter may use it or ignore it.
    pub fill: Color,
    /// The text's own ink.
    pub ink: Color,
}

/// What a painter returns: rects in surface coordinates, plus an ink override.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionPaint {
    /// Painted beneath the glyphs, in order.
    pub rects: Vec<RectPrim>,
    /// Draw the selected glyphs in this ink; `None` keeps the text's own.
    pub ink: Option<Color>,
}

/// A selection's drawing policy, chosen by the style and carried by the declaration.
///
/// The engine owns the geometry and the paint order; a system that wants a pill, a
/// hairline or inverted text supplies a painter and never touches the text pass.
/// Painters travel inside `Style`, so they must cross threads with it.
pub trait SelectionPainter: std::fmt::Debug + Send + Sync {
    fn paint(&self, geometry: &SelectionGeometry<'_>) -> SelectionPaint;
}

/// A painter carried by a declaration, compared by identity.
///
/// `&'static` keeps the declaration copyable and the comparison honest: `ptr::eq` on a
/// trait object compares the vtable too, so two different zero-sized painters never
/// collapse into one.
#[derive(Clone, Copy)]
pub struct SelectionPainterRef(pub &'static dyn SelectionPainter);

impl PartialEq for SelectionPainterRef {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl std::fmt::Debug for SelectionPainterRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelectionPainter")
    }
}

/// A text node's selection: the byte range, the colour and who draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionPrim {
    pub range: std::ops::Range<usize>,
    pub fill: Color,
    pub painter: Option<SelectionPainterRef>,
}

/// The engine's own look, for a style that names no painter: a plain rect in `fill`.
pub fn flat_selection(geometry: &SelectionGeometry<'_>) -> SelectionPaint {
    SelectionPaint {
        rects: geometry
            .fragments
            .iter()
            .map(|f| RectPrim {
                rect: f.rect,
                radius: crate::geom::Corners::ZERO,
                fill: geometry.fill,
                gradient: None,
                border: None,
                border_gradient: None,
                notch: None,
            })
            .collect(),
        ink: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_normalises_a_backwards_selection() {
        // Dragging right to left is normal; the range must still start first.
        let sel = TextSelection::new(9, 3);
        assert_eq!(sel.range(), 3..9);
        assert_eq!(TextSelection::collapsed(4).range(), 4..4);
        assert!(TextSelection::collapsed(4).is_collapsed());
    }
}
