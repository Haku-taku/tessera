//! The display list a laid-out formula hands to layout and paint.
//!
//! `ui-math` produces it from LaTeX; the engine stores and draws it without knowing the
//! source format, and the types live here so a node can carry one without depending on
//! the layout library.

use crate::geom::{Color, Size};
use crate::text::TextStyle;

/// Why a formula has no display list.
///
/// Both cases are the host's to fix, not the author's: a missing face is an installation
/// problem, and a source the chain cannot read at all is a library limit. Anything the author
/// can fix comes back as a formula with [`MathErrorMark`]s instead.
#[derive(Clone, Debug, PartialEq)]
pub enum MathError {
    /// The host has no face with a MATH table for the requested family.
    NoFace,
    /// The layout chain failed even after recovering from the source's errors.
    Failed(String),
}

/// How the runs a formula could not read are drawn.
///
/// Part of the layout contract because the run is painted from the display list, which is
/// built before any control sees it; the layout cache keys on these colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MathErrorStyle {
    pub border: Color,
    pub panel: Color,
    pub panel_ink: Color,
}

/// A run the parser could not read, in the coordinates of the formula it sits in.
///
/// The run stays in the display list, so the structure around it — a fraction, a radical, a
/// script — keeps its layout. This records where it landed so a control can hang a reason
/// from it; `y` grows downward from the baseline, as [`MathItem`] positions do.
#[derive(Clone, Debug, PartialEq)]
pub struct MathErrorMark {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The parser's diagnostic, without its terminal-oriented source context.
    pub message: String,
}

/// A formula laid out at one size, with the baseline metrics inline content needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub width: f32,
    /// Distance from the top of the formula to its baseline.
    pub ascent: f32,
    /// Distance from the baseline to its bottom; a fraction's denominator reaches below.
    pub descent: f32,
    pub items: Vec<MathItem>,
    /// The runs the parser refused, in reading order; empty when the source parsed whole.
    pub errors: Vec<MathErrorMark>,
}

impl Formula {
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// Something drawn at a text atom's rect, in place of its U+FFFC.
///
/// The atom reserves the box; the object fills it. A formula is the first case; an image
/// or an icon inside a sentence would be the next, and would not touch the layout.
#[derive(Clone, Debug, PartialEq)]
pub enum InlineObject {
    Math {
        formula: Formula,
        /// The face the formula was laid out with; its glyph ids mean nothing in another.
        family: String,
        ink: Color,
    },
    /// A run of text drawn where its bytes are not the bytes it shows: an entity whose
    /// source is several bytes and whose character is one, measured by the caller.
    Glyphs {
        text: String,
        style: TextStyle,
        ink: Color,
        size: Size,
        /// Distance from the top of `size` to the text's baseline.
        baseline: f32,
        /// Where the glyphs start inside a wider atom, from its left edge.
        offset: f32,
    },
}

impl InlineObject {
    /// The box the atom reserves: the object's own extents.
    pub fn size(&self) -> crate::geom::Size {
        match self {
            Self::Math { formula, .. } => crate::geom::Size::new(formula.width, formula.height()),
            Self::Glyphs { size, .. } => *size,
        }
    }

    /// The object's own baseline, from the top of its box.
    pub fn baseline(&self) -> f32 {
        match self {
            Self::Math { formula, .. } => formula.ascent,
            Self::Glyphs { baseline, .. } => *baseline,
        }
    }
}

/// One drawable of a formula, in painter's order.
#[derive(Clone, Debug, PartialEq)]
pub enum MathItem {
    /// A glyph of the face the formula was laid out with. `(x, y)` is its baseline origin,
    /// `y` growing downward; `size` is its own size, smaller inside scripts.
    Glyph {
        /// Glyph index in that face — not a character, and meaningless in another font.
        id: u16,
        /// The face slot the layout's shaper named; 0 is the formula's own face. A
        /// formula's text may fall back to another face, and its glyphs mean nothing
        /// in the first one.
        face: u16,
        x: f32,
        y: f32,
        size: f32,
        advance: f32,
        /// `None` draws the formula's ink.
        color: Option<Color>,
        /// Draw the glyph flipped about `x + advance / 2`; right-to-left math asks for it.
        mirrored: bool,
    },
    /// A filled rectangle: a fraction bar, a radical overbar, an error border.
    Rule {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Option<Color>,
    },
    /// A stroked segment: `\cancel` and its relatives draw one across the body.
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        thickness: f32,
        color: Option<Color>,
    },
    /// A `mathbackground` fill, emitted before the items it sits behind.
    Background {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
}

/// The caret a structural math editor reports, in the formula's own space: `top` and
/// `bottom` are offsets from the baseline, so a control paints it beside the display list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MathCaret {
    pub x: f32,
    pub top: f32,
    pub bottom: f32,
}

/// A rectangle in a formula's space, `y` measured down from the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MathEditRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A key the structural editor understands. Characters arrive one at a time, so typing a
/// command name, a script mark or a fence is the same door as an arrow key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Character(char),
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Escape,
}

/// One frame's request to keep a formula's editor alive, and the facts it lays out with.
#[derive(Clone, Copy, Debug)]
pub struct MathAttach<'a> {
    pub id: crate::Id,
    /// The math face, by family name.
    pub family: &'a str,
    /// The source the app holds.
    pub source: &'a str,
    pub size: f32,
    pub display: bool,
    /// The frame asking; an editor not attached for a frame is dropped.
    pub frame: u64,
    /// The text input whose buffer this formula is part of, and the byte range it occupies.
    pub owner: Option<(crate::Id, usize, usize)>,
}

/// The host's structural formula editing, beside its [`MathLayout`](crate::MathLayout).
///
/// A control attaches an editor for its id while it is mounted; the host keeps the model,
/// caret, selection and history across frames and lays out again on every edit. The same
/// face that paints the formula edits it, so glyph positions and caret geometry agree.
pub trait MathEdit {
    /// Keep an editor for `id` alive this frame, starting from `source` when there is none.
    ///
    /// A source the app changed on its own is an external edit: the caret keeps its reading
    /// position and history is dropped, as with any engine-owned input. Returns whether an
    /// editor exists for the id.
    fn attach(&mut self, request: MathAttach<'_>) -> bool;

    /// Whether `id` has an editor, which is what makes a press focus it.
    fn is_editing(&mut self, id: crate::Id) -> bool;

    /// The text input whose buffer `id`'s formula is part of, and the byte range the
    /// formula occupies there. An edit then rewrites that range instead of reporting on
    /// the formula's own id, so a document that embeds a formula stays one buffer.
    fn owner(&mut self, id: crate::Id) -> Option<(crate::Id, usize, usize)>;

    /// The formula embedded in the text input `owner` whose source occupies `offset`.
    ///
    /// The engine asks this to know that a caret in its buffer has entered a formula and
    /// that the structural editor, not the buffer, owns the next keystroke.
    fn formula_at(&mut self, owner: crate::Id, offset: usize) -> Option<crate::Id>;

    fn key(&mut self, id: crate::Id, key: EditKey, shift: bool) -> bool;

    /// Insert a whole LaTeX snippet, as a toolbar or a paste would.
    fn insert(&mut self, id: crate::Id, latex: &str) -> bool;

    /// Put the caret at `(x, y)` in formula space.
    fn click(&mut self, id: crate::Id, x: f32, y: f32, extend: bool) -> bool;

    /// The formula as currently edited, marked in `style`.
    fn formula(&mut self, id: crate::Id, style: Option<MathErrorStyle>) -> Option<Formula>;

    fn source(&mut self, id: crate::Id) -> Option<String>;

    fn caret(&mut self, id: crate::Id) -> Option<MathCaret>;

    fn selection(&mut self, id: crate::Id) -> Vec<MathEditRect>;

    /// The selection as `(anchor, caret)` byte offsets into [`source`](Self::source).
    fn selection_offsets(&mut self, id: crate::Id) -> Option<(usize, usize)>;

    /// Place the caret and selection from byte offsets into [`source`](Self::source),
    /// snapping to the nearest boundary the structure offers. A host that owns the buffer
    /// hands in the part of its own selection that falls inside this formula.
    fn select_offsets(&mut self, id: crate::Id, anchor: usize, caret: usize) -> bool;

    /// The byte offset into [`source`](Self::source) nearest `(x, y)` in formula space,
    /// which is where a press inside the formula lands in the host's buffer.
    fn offset_at(&mut self, id: crate::Id, x: f32, y: f32) -> Option<usize>;

    /// The required slots that are empty, which a control paints as placeholders.
    fn placeholders(&mut self, id: crate::Id) -> Vec<MathEditRect>;

    fn set_preedit(&mut self, id: crate::Id, text: &str) -> bool;
    fn commit_preedit(&mut self, id: crate::Id, text: &str) -> bool;
    fn cancel_preedit(&mut self, id: crate::Id) -> bool;

    fn undo(&mut self, id: crate::Id) -> bool;
    fn redo(&mut self, id: crate::Id) -> bool;
    fn select_all(&mut self, id: crate::Id) -> bool;
    fn selection_latex(&mut self, id: crate::Id) -> Option<String>;
    fn cut(&mut self, id: crate::Id) -> Option<String>;
}
