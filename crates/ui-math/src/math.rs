//! Formula layout: LaTeX becomes a model tree, and that tree becomes a glyph display list.
//!
//! This crate owns the layout libraries so nothing above it sees their types, and it does no
//! I/O: the caller brings the font bytes and decides where they live.

use std::fmt;

use math_core::{
    Color as MathColor, Diagnostic, DiagnosticKind, Geometry, Item, Layout, Model, NodeId,
    Rect as MathRect, Style, TextShaper, layout_with_geometry, layout_with_shaper_and_geometry,
};
use math_font::Face;
use math_syntax::parse;
use ui_core::geom::Color;

// The display list lives in the engine's vocabulary, so a node can carry one without
// depending on this crate; the crate only fills it in.
pub use ui_core::math::{Formula, MathErrorMark, MathErrorStyle, MathItem};

mod editor;
pub use editor::{Caret, EditRect, MathEditor, MathKey};

/// Error boxes are drawn with a hairline, like the formula's own rules.
const MARK_BORDER: f32 = 1.0;

/// Why a formula could not be produced.
#[derive(Debug)]
pub enum Error {
    /// The LaTeX could not be parsed or laid out.
    Latex(String),
    /// Kept for callers that match this variant; mathstack's parser has no
    /// separate MathML stage to fail.
    Mathml(String),
    /// The bytes are not a math font: unparseable, or no MATH table.
    Font(String),
    /// The layout library produced an item this version does not draw.
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Latex(message) => write!(f, "LaTeX could not be laid out: {message}"),
            Self::Mathml(message) => write!(f, "MathML could not be laid out: {message}"),
            Self::Font(message) => write!(f, "not a math font: {message}"),
            Self::Unsupported(what) => write!(f, "unsupported formula item: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// A parsed math face: the MATH table is read once here, not on every formula.
pub struct MathFace<'a> {
    font: Face<'a>,
}

impl<'a> MathFace<'a> {
    /// Parse `bytes` at `index` (the face's position inside a collection), or fail when the
    /// font has no MATH table.
    pub fn new(bytes: &'a [u8], index: u32) -> Result<Self, Error> {
        let font = Face::parse(bytes, index).map_err(|error| Error::Font(error.to_string()))?;
        if !font.has_math() {
            return Err(Error::Font("the face has no MATH table".to_owned()));
        }
        Ok(Self { font })
    }

    /// Lay `source` out at `size`, in display or inline style; every error is fatal.
    pub fn layout(&self, source: &str, size: f32, display: bool) -> Result<Formula, Error> {
        let document = parse(source);
        if let Some(diagnostic) = document.diagnostics.first() {
            return Err(Error::Latex(diagnostic.message.clone()));
        }
        let (layout, _, diagnostics) = layout_with_geometry(
            &self.font,
            &document.model,
            document.root(),
            size,
            math_style(display),
        );
        if let Some(diagnostic) = diagnostics.first() {
            return Err(Error::Latex(diagnostic.message.clone()));
        }
        Ok(formula(&layout))
    }

    /// Lay `source` out, keeping the structure around the runs the parser refuses.
    ///
    /// A formula someone is still typing is usually incomplete or has a mistyped command;
    /// dropping to the source text on either loses the frame they are working in. The
    /// parser's model keeps the surrounding structure and an `Error` node where it gave
    /// up, so the layout still produces the fraction or radical and this pass marks the
    /// refused run. An `Incomplete` diagnostic is the next keystroke's input, not an
    /// error, so it gets no mark.
    pub fn layout_recovering(
        &self,
        source: &str,
        size: f32,
        display: bool,
        style: Option<MathErrorStyle>,
    ) -> Result<Formula, Error> {
        self.layout_recovering_with(source, size, display, style, None)
    }

    /// Lay `source` out with the host's text shaper, so `\text` and operator names are
    /// measured by the same stack as the document's own text.
    pub fn layout_recovering_with(
        &self,
        source: &str,
        size: f32,
        display: bool,
        style: Option<MathErrorStyle>,
        shaper: Option<&dyn TextShaper>,
    ) -> Result<Formula, Error> {
        let document = parse(source);
        let default_shaper;
        let shaper: &dyn TextShaper = match shaper {
            Some(shaper) => shaper,
            None => {
                default_shaper = math_core::FontShaper::new(&self.font);
                &default_shaper
            }
        };
        let (layout, geometry, layout_diagnostics) = layout_with_shaper_and_geometry(
            &self.font,
            shaper,
            &document.model,
            document.root(),
            size,
            math_style(display),
        );
        let diagnostics: Vec<Diagnostic> = document
            .diagnostics
            .iter()
            .chain(layout_diagnostics.iter())
            .cloned()
            .collect();
        Ok(formula_with_marks(
            &document.model,
            &layout,
            &geometry,
            &diagnostics,
            style,
        ))
    }

    /// The parsed face, so the editor module can lay out with the same bytes.
    pub(crate) fn font(&self) -> &Face<'a> {
        &self.font
    }

    /// This face's parsed bytes, for an editor the host keeps across frames.
    pub fn editor(&self, source: &str, size: f32, display: bool) -> MathEditor {
        MathEditor::new(self, source, size, display)
    }
}

pub(crate) fn math_style(display: bool) -> Style {
    if display { Style::DISPLAY } else { Style::TEXT }
}

fn formula(layout: &Layout) -> Formula {
    let mut items = Vec::with_capacity(layout.items.len());
    for item in &layout.items {
        let item = match *item {
            Item::Background(background) => MathItem::Background {
                x: background.x,
                y: background.y - layout.ascent,
                w: background.width,
                h: background.height,
                color: ink(background.color),
            },
            Item::Glyph(glyph) => MathItem::Glyph {
                id: glyph.glyph.0,
                face: glyph.face,
                x: glyph.x,
                y: glyph.baseline - layout.ascent,
                size: glyph.size,
                advance: 0.0,
                color: glyph.color.map(ink),
                mirrored: false,
            },
            Item::Rule(rule) => MathItem::Rule {
                x: rule.x,
                y: rule.y - layout.ascent,
                w: rule.width,
                h: rule.height,
                color: rule.color.map(ink),
            },
            Item::Line(line) => MathItem::Line {
                x1: line.x1,
                y1: line.y1 - layout.ascent,
                x2: line.x2,
                y2: line.y2 - layout.ascent,
                thickness: line.thickness,
                color: line.color.map(ink),
            },
        };
        items.push(item);
    }
    Formula {
        width: layout.width,
        ascent: layout.ascent,
        descent: layout.descent,
        items,
        errors: Vec::new(),
    }
}

/// The layout, with a mark over every refused run. `Incomplete` is the next keystroke's
/// input, not an error, so it gets no mark.
pub(crate) fn formula_with_marks(
    model: &Model,
    layout: &Layout,
    geometry: &Geometry,
    diagnostics: &[Diagnostic],
    style: Option<MathErrorStyle>,
) -> Formula {
    let mut formula = formula(layout);
    for diagnostic in diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind != DiagnosticKind::Incomplete)
    {
        let Some(rect) = mark_rect(model, geometry, diagnostic.span) else {
            continue;
        };
        formula.errors.push(MathErrorMark {
            x: rect.x,
            y: rect.y - layout.ascent,
            w: rect.width,
            h: rect.height,
            message: diagnostic.message.clone(),
        });
        mark_items(rect, layout.ascent, style, &mut formula.items);
    }
    formula
}

/// The smallest laid-out node covering `span`: an error node when the parser made one,
/// and otherwise the construct the diagnostic is anchored to.
fn mark_rect(model: &Model, geometry: &Geometry, span: math_core::Span) -> Option<MathRect> {
    fn walk(
        model: &Model,
        geometry: &Geometry,
        id: NodeId,
        span: math_core::Span,
        best: &mut Option<(usize, MathRect)>,
    ) {
        let node_span = model.span(id);
        if node_span.start > span.start || node_span.end < span.end {
            return;
        }
        if let Some(rect) = geometry.rect_of(id) {
            let extent = node_span.end.saturating_sub(node_span.start);
            if best.is_none_or(|(smallest, _)| extent < smallest) {
                *best = Some((extent, rect));
            }
        }
        for child in model.kind(id).children() {
            walk(model, geometry, child.id, span, best);
        }
    }

    let mut best = None;
    walk(model, geometry, model.root(), span, &mut best);
    best.map(|(_, rect)| rect)
}

/// Draw the panel and border an error mark carries, above the formula's own item order.
fn mark_items(rect: MathRect, ascent: f32, style: Option<MathErrorStyle>, out: &mut Vec<MathItem>) {
    let y = rect.y - ascent;
    if let Some(style) = style {
        out.push(MathItem::Background {
            x: rect.x,
            y,
            w: rect.width,
            h: rect.height,
            color: style.panel,
        });
    }
    // Without a style the border takes the formula's own ink, so a read-only formula still
    // marks the run without borrowing a pack's colours.
    let border = style.map(|style| style.border);
    for (x, y, w, h) in [
        (rect.x, y, rect.width, MARK_BORDER),
        (
            rect.x,
            y + rect.height - MARK_BORDER,
            rect.width,
            MARK_BORDER,
        ),
        (rect.x, y, MARK_BORDER, rect.height),
        (
            rect.x + rect.width - MARK_BORDER,
            y,
            MARK_BORDER,
            rect.height,
        ),
    ] {
        out.push(MathItem::Rule {
            x,
            y,
            w,
            h,
            color: border,
        });
    }
}

fn ink(color: MathColor) -> Color {
    Color::from_rgba8(color.red, color.green, color.blue, color.alpha)
}

/// Whether `bytes` can lay out math. Reads the table directory only, so a font list can be
/// scanned with it.
pub fn probe(bytes: &[u8], index: u32) -> bool {
    Face::parse(bytes, index).is_ok_and(|face| face.has_math())
}
