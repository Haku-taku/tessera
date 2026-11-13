//! The text control: the one place a widget states *what* it is drawing and *in what font*.

use ui_core::geom::{Color, Insets};
use ui_core::layout::{AxisSize, Grow};
use ui_core::{
    Decl, Family, Features, Id, NodeData, NodeKind, SizeClass, TextAlign, TextBlock, TextRun,
    TextStyle, Variations, Weight,
};

/// A text node's height per em of type size: the box its glyphs are centred in.
pub const LINE_HINT: f32 = 1.4;

/// The box an icon of `size` occupies: its em square, so a caller centres the box.
pub fn icon_extent(size: f32) -> f32 {
    size
}

/// The role names this control reads from a pack.
pub mod roles {
    /// The wash behind selected text.
    pub const SELECTION: &str = "textSelection";
}

/// A run of text as a control: body plus every font-bearing setting it takes.
///
/// Ink is here, not on the type ramp: the widget that owns the surface knows its background
/// and so chooses the role; the text control only carries that choice down to the prim.
pub struct Text {
    pub id: Id,
    pub body: String,
    pub size: f32,
    pub weight: Weight,
    /// Family, variable axes and features: everything a font request carries besides size.
    pub family: Family,
    pub variations: Variations,
    pub features: Features,
    /// An outline drawn around every glyph, and its ink.
    pub outline: ui_core::GlyphOutline,
    pub ink: Color,
    /// Wrap width: the most this text may take before it breaks a line.
    pub max_width: f32,
    /// One line, however narrow the box: the node overflows and the caller scrolls it.
    pub nowrap: bool,
    /// Stop after this many lines; the painter ends the last one with an ellipsis.
    pub max_lines: Option<u32>,
    pub line_gap: f32,
    pub indent: f32,
    /// How lines sit inside the wrap width; the painter honours it via `aligned_x`.
    pub align: TextAlign,
    /// Draw an insertion caret at this byte offset, in this ink.
    pub caret: Option<ui_core::Caret>,
    /// Pointer selection, when this text takes it.
    pub selectable: Option<Selectable>,
    /// A selection to paint without taking pointer selection: an input's own selection.
    pub painted: Option<ui_core::SelectionPrim>,
    /// Bytes the input method is composing, underlined by the text pass.
    pub composing: Option<std::ops::Range<usize>>,
}

/// A text control that takes pointer selection, and how its style draws it.
#[derive(Clone, Copy, Debug)]
pub struct Selectable {
    pub fill: Color,
    pub painter: Option<ui_core::SelectionPainterRef>,
}

impl Text {
    pub fn new(id: Id, body: impl Into<String>, size: f32, ink: Color) -> Self {
        Self {
            id,
            body: body.into(),
            size,
            weight: Weight::REGULAR,
            family: Family::default(),
            variations: Variations::NONE,
            features: Features::NONE,
            outline: ui_core::GlyphOutline::NONE,
            ink,
            max_width: f32::INFINITY,
            nowrap: false,
            max_lines: None,
            line_gap: 0.0,
            indent: 0.0,
            align: TextAlign::Start,
            caret: None,
            selectable: None,
            painted: None,
            composing: None,
        }
    }

    /// Builder: this text asks for weight `w`.
    pub fn weight(mut self, w: Weight) -> Self {
        self.weight = w;
        self
    }

    /// Builder: this text asks for `family`.
    pub fn family(mut self, family: Family) -> Self {
        self.family = family;
        self
    }

    /// Builder: variable-font axes beyond weight.
    pub fn variations(mut self, variations: Variations) -> Self {
        self.variations = variations;
        self
    }

    /// Builder: OpenType features.
    pub fn features(mut self, features: Features) -> Self {
        self.features = features;
        self
    }

    /// Builder: draw an outline of `width` in `ink` around every glyph.
    pub fn outline(mut self, width: f32, ink: Color) -> Self {
        self.outline = ui_core::GlyphOutline { width, color: ink };
        self
    }

    /// Builder: wrap at `width` logical units.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = width;
        self
    }

    /// Builder: lay this text out on one line, overflowing instead of wrapping.
    pub fn nowrap(mut self) -> Self {
        self.nowrap = true;
        self
    }

    /// Builder: stop after `lines`, ending the last one with an ellipsis.
    pub fn max_lines(mut self, lines: u32) -> Self {
        self.max_lines = Some(lines);
        self
    }

    /// Builder: extra leading between lines.
    pub fn line_gap(mut self, gap: f32) -> Self {
        self.line_gap = gap;
        self
    }

    /// Builder: first-line indent.
    pub fn indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }

    /// Builder: how lines sit inside the wrap width.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Builder: draw the insertion caret at byte `offset` in `ink`.
    pub fn caret(mut self, offset: usize, ink: Color) -> Self {
        self.caret = Some(ui_core::Caret { offset, color: ink });
        self
    }

    /// Builder: take pointer selection, drawn by `painter` in `fill`.
    pub fn selectable(
        mut self,
        fill: Color,
        painter: Option<ui_core::SelectionPainterRef>,
    ) -> Self {
        self.selectable = Some(Selectable { fill, painter });
        self
    }

    /// Builder: take pointer selection, with the wash and painter `pack` resolved.
    pub fn selectable_in(self, pack: &ui_style::Style) -> Self {
        let fill = pack
            .ink(roles::SELECTION)
            .expect("pack has no textSelection role");
        self.selectable(fill, pack.selection)
    }

    /// Builder: underline this byte range as input-method composition.
    pub fn composing(mut self, range: Option<std::ops::Range<usize>>) -> Self {
        self.composing = range;
        self
    }

    /// Builder: paint `selection` under this text, without taking pointer selection.
    pub fn painting(mut self, selection: Option<ui_core::SelectionPrim>) -> Self {
        self.painted = selection;
        self
    }

    /// The declaration for a frame, with this node's selection read from the engine.
    pub fn resolve_in(&self, cx: &crate::cx::Cx<'_, '_>) -> Decl {
        let selection = self.selectable.as_ref().and_then(|style| {
            cx.core()
                .selection_for(self.id)
                .filter(|selection| !selection.is_collapsed())
                .map(|selection| ui_core::SelectionPrim {
                    range: selection.range(),
                    fill: style.fill,
                    painter: style.painter,
                })
        });
        self.build(selection)
    }

    pub fn resolve(&self) -> Decl {
        self.build(None)
    }

    /// The box this text takes on one line, measured as layout will measure it.
    ///
    /// `None` when the host supplied no measurer: a caller that needs a number must then
    /// fall back to geometry it already has, never to a guessed advance.
    pub fn size_in(&self, cx: &crate::cx::Cx<'_, '_>) -> Option<ui_core::geom::Size> {
        let decl = self.build(None);
        let ui_core::NodeData::Text { block, .. } = &decl.data else {
            return None;
        };
        cx.text_size(block, self.max_width)
    }

    /// The width this text takes on one line.
    pub fn width_in(&self, cx: &crate::cx::Cx<'_, '_>) -> Option<f32> {
        self.size_in(cx).map(|size| size.w)
    }

    fn build(&self, selection: Option<ui_core::SelectionPrim>) -> Decl {
        Decl {
            id: self.id,
            kind: NodeKind::Text,
            shapes: Vec::new(),
            data: NodeData::Text {
                ink: self.ink,
                block: std::rc::Rc::new(TextBlock {
                    runs: vec![TextRun::new(
                        self.body.clone(),
                        TextStyle::new(self.size)
                            .weight(self.weight)
                            .family(self.family)
                            .variations(self.variations)
                            .features(self.features)
                            .outline(self.outline.width, self.outline.color),
                    )],
                    align: self.align,
                    max_lines: self.max_lines,
                    line_gap: self.line_gap,
                    indent: self.indent,
                    hanging: 0.0,
                    atoms: Vec::new(),
                    input_base: None,
                    line_numbers: None,
                    structures: Vec::new(),
                }),
                caret: self.caret,
                selection: selection.or_else(|| self.painted.clone()),
                composing: self.composing.clone(),
                nowrap: self.nowrap,
            },
            size_class: SizeClass {
                width: AxisSize::hug(0.0, self.max_width),
                height: AxisSize::hug(self.size, self.size * LINE_HINT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            ink_margin: Insets::ZERO,
            box_model: Default::default(),
            offset: ui_core::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: if self.selectable.is_some() {
                ui_core::Sense::SELECT
            } else {
                ui_core::Sense::NONE
            },
            disabled: false,
            grid: Default::default(),
            placement: Default::default(),
            env: None,
            children: Vec::new(),
        }
    }
}

/// Thin positional wrapper kept for callers that built one line inline; delegates to `Text`.
pub fn leaf(
    id: Id,
    body: String,
    size: f32,
    color: Color,
    weight: Weight,
    max_width: f32,
    line_gap: f32,
    indent: f32,
) -> Decl {
    Text::new(id, body, size, color)
        .weight(weight)
        .max_width(max_width)
        .line_gap(line_gap)
        .indent(indent)
        .resolve()
}

/// What an editor draws: the buffer with the composition spliced in at the caret, plus the
/// composed range and where the caret sits in the shown text.
///
/// An editor that keeps its own view over a buffer uses this so the preedit draws where
/// the input method is composing, not at the committed caret.
pub fn typed_input(
    text: &str,
    caret: Option<usize>,
    preedit: Option<&ui_core::Preedit>,
) -> (String, Option<std::ops::Range<usize>>, Option<usize>) {
    match preedit {
        Some(p) if !p.is_empty() => {
            let (shown, at) =
                ui_core::text_input::composed(text, &p.text, caret.unwrap_or(text.len()));
            (
                shown,
                Some(at..at + p.text.len()),
                Some(at + p.cursor.min(p.text.len())),
            )
        }
        _ => (text.to_string(), None, caret),
    }
}

/// Insert `run` at byte `at`, splitting the run it lands inside so every other run keeps
/// its own bytes. Used where an editor draws a buffer as styled runs and must show an
/// input method's composition without losing the ink around it.
pub fn splice_run(runs: Vec<TextRun>, at: usize, run: TextRun) -> Vec<TextRun> {
    let mut out = Vec::with_capacity(runs.len() + 1);
    let mut seen = 0usize;
    let mut placed = false;
    for existing in runs {
        let end = seen + existing.text.len();
        if !placed && at <= seen {
            out.push(run.clone());
            placed = true;
        }
        if at > seen && at < end {
            let (head, tail) = existing.text.split_at(at - seen);
            if !head.is_empty() {
                out.push(TextRun {
                    text: head.to_string(),
                    ..existing.clone()
                });
            }
            out.push(run.clone());
            if !tail.is_empty() {
                out.push(TextRun {
                    text: tail.to_string(),
                    ..existing
                });
            }
            placed = true;
        } else {
            seen = end;
            out.push(existing);
            continue;
        }
        seen = end;
    }
    if !placed {
        out.push(run);
    }
    out
}

/// How far a single-line editor must shift left so its caret clears the right edge.
pub fn caret_scroll(
    cx: &crate::Cx<'_, '_>,
    text_id: Id,
    width: f32,
    caret_width: f32,
    slack: f32,
) -> f32 {
    cx.core()
        .caret_in_text_for(text_id)
        .map(|x| (x + caret_width + slack - width).max(0.0))
        .unwrap_or(0.0)
}

/// Everything one editable line draws; the surface, label and icons belong to the field.
pub struct LineEdit<'a> {
    pub id: Id,
    pub text: &'a str,
    pub caret: Option<usize>,
    pub preedit: Option<&'a ui_core::Preedit>,
    /// The selection, painted by the pack's own painter.
    pub painted: Option<ui_core::SelectionPrim>,
    pub placeholder: &'a str,
    /// Draw the line as marks. They keep the buffer's byte layout, so the caret and the
    /// selection stay real offsets.
    pub obscure: bool,
    /// Cupertino shows the hint whenever the buffer is empty; Material only while focused.
    pub hint_always: bool,
    pub focused: bool,
    pub size: f32,
    pub weight: Weight,
    pub ink: Color,
    pub placeholder_ink: Color,
    pub caret_ink: Color,
    pub caret_width: f32,
    pub slack: f32,
    /// The text box's width between the field's insets.
    pub width: f32,
    pub left_inset: f32,
    pub right_inset: f32,
}

impl LineEdit<'_> {
    /// The line: one nowrap run scrolled just far enough, plus the hint and caret when empty.
    pub fn resolve(&self, cx: &crate::Cx<'_, '_>) -> Decl {
        let (shown, composing, caret_at) = typed_input(self.text, self.caret, self.preedit);
        // Masking after the composition is spliced in covers the preedit too: an input
        // method's candidate text is as secret as the buffer it will land in.
        let shown = match self.obscure {
            true => ui_core::text::masked(&shown),
            false => shown,
        };
        let mut input = Text::new(self.id.with("input"), shown.clone(), self.size, self.ink)
            .weight(self.weight)
            .nowrap()
            .max_width(self.width)
            .painting(self.painted.clone())
            .composing(composing);
        if let Some(offset) = caret_at.filter(|_| !shown.is_empty()) {
            input = input.caret(offset, self.caret_ink);
        }
        let scroll = caret_scroll(
            cx,
            self.id.with("input"),
            self.width,
            self.caret_width,
            self.slack,
        );
        let input = input.resolve().offset(-scroll, 0.0);
        // A nowrap line runs past the field; cutting it here keeps the overflow out of sight.
        let mut content = Decl::container(self.id.with("content"), NodeKind::Stack)
            .size(SizeClass {
                width: AxisSize::fixed(self.width + self.left_inset + self.right_inset),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .clip(ui_core::layout::Clip {
                x: ui_core::layout::Overflow::Hidden,
                y: ui_core::layout::Overflow::Visible,
            })
            .padding(Insets {
                left: self.left_inset,
                top: 0.0,
                right: self.right_inset,
                bottom: 0.0,
            })
            .child(input);
        let empty = self.text.is_empty() && self.preedit.is_none();
        if (self.focused || self.hint_always) && empty && !self.placeholder.is_empty() {
            content = content.child(
                Text::new(
                    self.id.with("hint"),
                    self.placeholder.to_string(),
                    self.size,
                    self.placeholder_ink,
                )
                .weight(self.weight)
                .max_width(self.width)
                .resolve(),
            );
        }
        if self.focused && empty {
            // An empty buffer has no span to shape, so its caret is a plain box at the origin.
            content = content.child(
                Decl::container(self.id.with("caret"), NodeKind::Box)
                    .size(SizeClass::fixed(self.caret_width, self.size * LINE_HINT))
                    .background(self.caret_ink, ui_core::geom::Corners::ZERO),
            );
        }
        content
    }
}
