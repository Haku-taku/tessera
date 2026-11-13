//! Structural editing: the caret, selection, empty slots and undo over the same model tree
//! the formula is drawn from.
//!
//! The editor holds no font; every call that changes the model takes the face, because a
//! host keeps one parsed face and edits against it. Rects come back in the formula's own
//! space — x from the left, y from the baseline — so a control paints them beside the
//! display list without knowing about layout coordinates.

use math_core::{Point, Rect};
use math_editor::Editor;

use crate::{Formula, MathErrorStyle, MathFace, formula_with_marks, math_style};

pub use math_editor::Key as MathKey;

/// The caret in formula space: `top` and `bottom` are offsets from the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub x: f32,
    pub top: f32,
    pub bottom: f32,
}

/// A rectangle in formula space, y measured down from the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A formula being edited by structure.
///
/// The host owns the instance across frames and asks for a `Formula` each build, so the
/// display list and the caret always come from one layout.
pub struct MathEditor {
    editor: Editor,
    size: f32,
    display: bool,
}

impl MathEditor {
    pub(crate) fn new(face: &MathFace<'_>, source: &str, size: f32, display: bool) -> Self {
        Self {
            editor: Editor::new(source, face.font(), size, math_style(display)),
            size,
            display,
        }
    }

    /// The formula as written back to LaTeX.
    pub fn source(&self) -> String {
        self.editor.source()
    }

    /// The display list, with the runs the parser refuses marked in `style`.
    pub fn formula(&self, style: Option<MathErrorStyle>) -> Formula {
        formula_with_marks(
            self.editor.model(),
            self.editor.layout(),
            self.editor.geometry(),
            self.editor.diagnostics(),
            style,
        )
    }

    /// Where the caret stands, or `None` when no path in the layout holds it.
    pub fn caret(&self) -> Option<Caret> {
        let ascent = self.editor.layout().ascent;
        self.editor.caret_rect().map(|rect| Caret {
            x: rect.x,
            top: rect.top - ascent,
            bottom: rect.bottom - ascent,
        })
    }

    /// The caret's byte offset in `source()`, so a host that owns the buffer can place its
    /// own caret on the formula's boundary.
    pub fn caret_offset(&self) -> Option<usize> {
        self.editor.caret_offset()
    }

    /// The anchor's byte offset in `source()`, when something is selected.
    pub fn anchor_offset(&self) -> Option<usize> {
        self.editor.anchor_offset()
    }

    /// The selection as `(anchor, caret)` byte offsets.
    pub fn selection_offsets(&self) -> Option<(usize, usize)> {
        self.editor.selection_offsets()
    }

    /// Place the caret and selection from byte offsets of `source()`, snapping to the
    /// nearest boundary the structure offers. This is how a host hands in a selection that
    /// starts or ends outside the formula.
    pub fn set_selection_offsets(&mut self, anchor: usize, caret: usize) -> bool {
        self.editor.set_selection_offsets(anchor, caret)
    }

    /// The byte offset in `source()` nearest `(x, y)` in formula space, which is where a
    /// press inside the formula lands in the host's buffer.
    pub fn offset_at(&self, x: f32, y: f32) -> Option<usize> {
        let ascent = self.editor.layout().ascent;
        self.editor.offset_at(Point { x, y: y + ascent })
    }

    /// The selected runs, in formula space.
    pub fn selection(&self) -> Vec<EditRect> {
        let ascent = self.editor.layout().ascent;
        self.editor
            .selection()
            .into_iter()
            .map(|rect| shifted(rect, ascent))
            .collect()
    }

    /// The required slots that are empty, in formula space: the host paints a placeholder.
    pub fn placeholders(&self) -> Vec<EditRect> {
        let ascent = self.editor.layout().ascent;
        self.editor
            .placeholders()
            .into_iter()
            .map(|(_, rect)| shifted(rect, ascent))
            .collect()
    }

    pub fn message(&self) -> Option<&str> {
        self.editor.message()
    }

    /// Apply one key. `shift` extends the selection; the return says whether the display
    /// changed.
    pub fn key(&mut self, face: &MathFace<'_>, key: MathKey, shift: bool) -> bool {
        self.editor.key(key, shift, face.font())
    }

    /// Insert a whole LaTeX snippet at the caret, the way a toolbar would.
    pub fn insert_latex(&mut self, face: &MathFace<'_>, source: &str) -> bool {
        self.editor.insert_latex(source, face.font())
    }

    pub fn insert_superscript(&mut self, face: &MathFace<'_>) -> bool {
        self.editor.insert_superscript(face.font())
    }

    pub fn insert_subscript(&mut self, face: &MathFace<'_>) -> bool {
        self.editor.insert_subscript(face.font())
    }

    /// Put the caret at `(x, y)` in formula space; `extend` moves the selection's far end.
    pub fn click(&mut self, x: f32, y: f32, extend: bool) -> bool {
        let ascent = self.editor.layout().ascent;
        self.editor.click(Point { x, y: y + ascent }, extend)
    }

    pub fn select_all(&mut self) -> bool {
        self.editor.select_all()
    }

    pub fn selection_latex(&self) -> Option<String> {
        self.editor.selection_latex()
    }

    pub fn cut_selection(&mut self, face: &MathFace<'_>) -> Option<String> {
        self.editor.cut_selection(face.font())
    }

    pub fn undo(&mut self, face: &MathFace<'_>) -> bool {
        self.editor.undo(face.font())
    }

    pub fn redo(&mut self, face: &MathFace<'_>) -> bool {
        self.editor.redo(face.font())
    }

    pub fn set_source(&mut self, face: &MathFace<'_>, source: &str) {
        self.editor.set_source(source, face.font());
    }

    pub fn set_preedit(&mut self, face: &MathFace<'_>, text: &str) -> bool {
        self.editor.set_preedit(text, face.font())
    }

    pub fn commit_preedit(&mut self, face: &MathFace<'_>, text: &str) -> bool {
        self.editor.commit_preedit(text, face.font())
    }

    pub fn cancel_preedit(&mut self, face: &MathFace<'_>) -> bool {
        self.editor.cancel_preedit(face.font())
    }

    /// Install the host's text shaper: `\text` and operator names are then measured by
    /// the same stack as the document's own text. The editor lays out again on install.
    pub fn set_shaper(
        &mut self,
        face: &MathFace<'_>,
        shaper: std::sync::Arc<dyn math_core::TextShaper>,
    ) {
        self.editor.set_shaper(shaper, face.font());
    }

    /// Change the set size; the caret and selection survive the relayout.
    pub fn set_size(&mut self, face: &MathFace<'_>, size: f32) {
        if size != self.size {
            self.size = size;
            self.editor.set_size(size, face.font());
        }
    }

    pub fn set_display(&mut self, face: &MathFace<'_>, display: bool) {
        if display != self.display {
            self.display = display;
            self.editor.set_style(math_style(display), face.font());
        }
    }
}

fn shifted(rect: Rect, ascent: f32) -> EditRect {
    EditRect {
        x: rect.x,
        y: rect.y - ascent,
        w: rect.width,
        h: rect.height,
    }
}
