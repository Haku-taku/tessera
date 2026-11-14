//! The formula control: LaTeX in, a math node out, laid out by the host's math face.

use ui_core::geom::{Corners, Insets};
use ui_core::layout::{AUTO, Anchor, AxisSize, Grow, OverlayPlacement};
pub use ui_core::math::MathErrorStyle;
use ui_core::math::{MathEditRect, MathError, MathItem};
use ui_core::{Decl, Family, GenericFamily, Id, NodeData, NodeKind, Sense, SizeClass};

use crate::Text;

/// The popover's padding, corner, text size and width, as the systems' tooltips use them.
const HINT_PAD: f32 = 8.0;
const HINT_RADIUS: f32 = 4.0;
const HINT_SIZE: f32 = 12.0;
const HINT_MAX_WIDTH: f32 = 320.0;
/// Gap between the marked run and the panel that hangs from it.
const HINT_OFFSET: f32 = 6.0;
/// How far the hover target grows past the marked run, so a hairline box is still reachable.
const MARK_HOVER_PAD: f32 = 4.0;

/// The editing caret: one visible bar, set beside the glyphs rather than in them.
const CARET_WIDTH: f32 = 1.5;
/// An empty required slot is a faint outline, so the structure is visible before it is filled.
const SLOT_BORDER: f32 = 1.0;
const SLOT_ALPHA: f32 = 0.35;
/// A selected run is a wash under the glyphs, not a second display list.
const SELECTION_ALPHA: f32 = 0.25;

/// A formula as a control.
///
/// The host lays it out: the build asks the engine context for a display list, because only
/// the host knows which face is installed and which one the painter will rasterize.
pub struct Math {
    pub id: Id,
    /// LaTeX, as the document model stores it.
    pub source: String,
    pub size: f32,
    pub ink: ui_core::geom::Color,
    /// The math face, by family name; `None` asks the host for its default.
    pub family: Option<String>,
    /// Display style: fractions stacked, limits above and below the operator.
    pub display: bool,
    /// How runs the parser refuses are drawn. `None` marks them in the formula's own ink,
    /// with no reason on hover.
    pub error: Option<MathErrorStyle>,
    /// Edit the formula by structure rather than reading it: the host keeps a model, a
    /// caret, a selection and history per control, and the app still owns the source.
    pub editable: bool,
    /// The text input this formula is embedded in, and the byte range its source occupies
    /// there. An edit rewrites that range, so a document with formulas stays one buffer.
    pub owner: Option<(Id, usize, usize)>,
    /// Whether the caret at the source's last byte is this formula's. It is not when the
    /// document writes another view on that byte, as a fence on the formula's own line:
    /// one caret position belongs to one view, and that view is the later one.
    pub caret_ends: bool,
}

impl Math {
    pub fn new(id: Id, source: impl Into<String>, size: f32, ink: ui_core::geom::Color) -> Self {
        Self {
            id,
            source: source.into(),
            size,
            ink,
            family: None,
            display: false,
            error: None,
            editable: false,
            owner: None,
            caret_ends: true,
        }
    }

    /// Builder: draw refused runs in `style` and hang the parser's reason from each.
    pub fn error(mut self, style: MathErrorStyle) -> Self {
        self.error = Some(style);
        self
    }

    /// Builder: display style rather than inline style.
    pub fn display(mut self) -> Self {
        self.display = true;
        self
    }

    /// Builder: this formula asks for `family`, by name.
    pub fn family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    /// Builder: edit this formula by structure.
    pub fn editable(mut self) -> Self {
        self.editable = true;
        self
    }

    /// Builder: this formula's source is the bytes `range` of `input`'s buffer.
    pub fn in_text(mut self, input: Id, range: std::ops::Range<usize>) -> Self {
        self.owner = Some((input, range.start, range.end));
        self
    }

    /// Builder: whether the caret at the source's last byte draws here.
    pub fn caret_ends(mut self, own: bool) -> Self {
        self.caret_ends = own;
        self
    }

    /// This frame's declaration: the formula, or — when the host cannot lay it out — its
    /// source.
    ///
    /// A source with errors still lays out: the runs around each error keep the structure
    /// they sit in, and only the refused run is boxed. A missing face is not the author's
    /// mistake, so it shows the bare source instead of an empty box.
    pub fn resolve_in(&self, cx: &crate::cx::Cx<'_, '_>) -> Decl {
        match self.laid_out(cx) {
            Ok((formula, family)) => {
                if let Some(edited) = self.edited_formula(cx, &family) {
                    return self.editable_decl(edited, family);
                }
                match self.error {
                    Some(style) if !formula.errors.is_empty() => {
                        self.marked_decl(formula, family, style, cx)
                    }
                    _ => self.formula_decl(self.id, formula, family),
                }
            }
            Err(MathError::NoFace | MathError::Failed(_)) => self.source_decl(),
        }
    }

    /// The editor's own display list for this frame, with the caret, selection and empty
    /// slots painted into it. `None` when this control is read-only or the host cannot
    /// edit it, and the read-only formula is then the answer.
    fn edited_formula(&self, cx: &crate::cx::Cx<'_, '_>, family: &str) -> Option<ui_core::Formula> {
        if !self.editable {
            return None;
        }
        let frame = cx.core().frame();
        let request = ui_core::MathAttach {
            id: self.id,
            family,
            source: &self.source,
            size: self.size,
            display: self.display,
            frame,
            owner: self.owner,
        };
        let attached = cx.core().with_math_edit(|edit| edit.attach(request))?;
        if !attached {
            return None;
        }
        // A formula inside a document does not keep a caret of its own: the document's
        // selection is the only one, and the structure mirrors the part of it that falls
        // inside this formula, clamped to its ends.
        if let Some((owner, start, end)) = self.owner
            && let Some(selection) = cx.core().selection_for(owner)
        {
            let local = |at: usize| at.clamp(start, end) - start;
            let _ = cx.core().with_math_edit(|edit| {
                edit.select_offsets(self.id, local(selection.anchor), local(selection.caret))
            });
        }
        let mut formula = cx
            .core()
            .with_math_edit(|edit| edit.formula(self.id, self.error))??;
        let selection = cx
            .core()
            .with_math_edit(|edit| edit.selection(self.id))
            .unwrap_or_default();
        let slots = cx
            .core()
            .with_math_edit(|edit| edit.placeholders(self.id))
            .unwrap_or_default();
        // Only the control the caret is in draws it: the focused one when it stands alone,
        // and inside a document the one whose bytes hold the document's caret.
        let caret = if self.shows_caret(cx) {
            cx.core()
                .with_math_edit(|edit| edit.caret(self.id))
                .flatten()
        } else {
            None
        };
        // Painter's order: selection under everything, slot outlines over the structure but
        // under the caret, and the caret last.
        for rect in selection {
            formula.items.push(MathItem::Background {
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
                color: ui_core::geom::Color {
                    a: self.ink.a * SELECTION_ALPHA,
                    ..self.ink
                },
            });
        }
        for slot in slots {
            slot_outline(slot, self.ink, &mut formula.items);
        }
        if let Some(caret) = caret {
            formula.items.push(MathItem::Rule {
                x: caret.x - CARET_WIDTH * 0.5,
                y: caret.top,
                w: CARET_WIDTH,
                h: (caret.bottom - caret.top).max(1.0),
                color: Some(self.ink),
            });
        }
        Some(formula)
    }

    /// The edited formula's node.
    ///
    /// A standalone formula takes focus and clicks itself, so a press moves its caret into
    /// the structure the way a press into text does. One embedded in a document does not:
    /// the document owns the caret, and the formula is a view of bytes in it.
    fn editable_decl(&self, formula: ui_core::Formula, family: String) -> Decl {
        let mut decl = self.formula_decl(self.id, formula, family);
        if self.owner.is_none() {
            decl = decl.sense(Sense::CLICK | Sense::FOCUSABLE);
        }
        decl
    }

    /// Whether this frame's caret bar is this formula's.
    fn shows_caret(&self, cx: &crate::cx::Cx<'_, '_>) -> bool {
        if cx.core().focused() == Some(self.id) {
            return true;
        }
        let Some((owner, start, end)) = self.owner else {
            return false;
        };
        cx.core()
            .caret_for(owner)
            .is_some_and(|at| start <= at && (at < end || (self.caret_ends && at == end)))
    }

    /// This formula as an inline object for a paragraph, or `None` when the host cannot lay
    /// it out — the caller then keeps the source text in the run.
    pub fn object_in(&self, cx: &crate::cx::Cx<'_, '_>) -> Option<ui_core::InlineObject> {
        let (formula, family) = self.laid_out(cx).ok()?;
        Some(ui_core::InlineObject::Math {
            formula,
            family,
            ink: self.ink,
        })
    }

    /// The display list and the face it belongs to, from the host's math layout.
    fn laid_out(
        &self,
        cx: &crate::cx::Cx<'_, '_>,
    ) -> Result<(ui_core::Formula, String), MathError> {
        let core = cx.core();
        let family = match &self.family {
            Some(name) => Some(name.clone()),
            None => core.math_family(),
        }
        .ok_or(MathError::NoFace)?;
        let formula =
            core.math_layout(&family, &self.source, self.size, self.display, self.error)?;
        Ok((formula, family))
    }

    fn formula_decl(&self, id: Id, formula: ui_core::Formula, family: String) -> Decl {
        let mut decl = Decl::container(id, NodeKind::Math);
        decl.data = NodeData::Math {
            formula: std::sync::Arc::new(formula),
            family: family.into(),
            ink: self.ink,
        };
        decl.size_class = SizeClass {
            width: AxisSize::hug(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        };
        decl
    }

    fn source_decl(&self) -> Decl {
        Text::new(self.id, self.source.clone(), self.size, self.ink)
            .family(Family::Generic(GenericFamily::Monospace))
            .resolve()
    }

    /// The formula with a hover target over every refused run.
    ///
    /// A formula is one leaf, so the run inside it has no node of its own; the target is an
    /// empty box placed over the run's recorded rect. It sits outside the flow, so it does
    /// not shift the formula, and it senses hover itself, because an unsensed node is never
    /// hovered and the reason would never mount.
    fn marked_decl(
        &self,
        formula: ui_core::Formula,
        family: String,
        style: MathErrorStyle,
        cx: &crate::cx::Cx<'_, '_>,
    ) -> Decl {
        let ascent = formula.ascent;
        let marks = formula.errors.clone();
        let mut decl = Decl::container(self.id, NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // The absolute targets below are placed against this box.
            .relative()
            .child(self.formula_decl(self.id.with("formula"), formula, family));
        for (index, mark) in marks.iter().enumerate() {
            let id = self.id.with(("error", index));
            let mut target = Decl::container(id, NodeKind::Box)
                .size(SizeClass::fixed(
                    mark.w + 2.0 * MARK_HOVER_PAD,
                    mark.h + 2.0 * MARK_HOVER_PAD,
                ))
                .absolute()
                .inset(Insets::new(
                    mark.x - MARK_HOVER_PAD,
                    ascent + mark.y - MARK_HOVER_PAD,
                    AUTO,
                    AUTO,
                ))
                .sense(Sense::HOVER);
            if cx.core().hovered_within(id) {
                target = target.child(
                    Decl::container(id.with("hint"), NodeKind::Box)
                        .overlay(OverlayPlacement::below(Anchor::Node(id)).offset(HINT_OFFSET))
                        .padding(Insets::all(HINT_PAD))
                        .shapes(vec![ui_core::draw::Shape::Box {
                            corners: Corners::all(HINT_RADIUS),
                            fill: style.panel,
                            gradient: None,
                            border: None,
                            shadow: None,
                            image: None,
                        }])
                        .child(
                            Text::new(
                                id.with("hint-text"),
                                mark.message.clone(),
                                HINT_SIZE,
                                style.panel_ink,
                            )
                            .max_width(HINT_MAX_WIDTH)
                            .resolve(),
                        ),
                );
            }
            decl = decl.child(target);
        }
        decl
    }
}

impl crate::Widget for Math {
    fn decl(&self, cx: &crate::cx::Cx<'_, '_>) -> Decl {
        self.resolve_in(cx)
    }
}

/// Four hairline rules around an empty required slot.
fn slot_outline(rect: MathEditRect, ink: ui_core::geom::Color, out: &mut Vec<MathItem>) {
    let color = Some(ui_core::geom::Color {
        a: ink.a * SLOT_ALPHA,
        ..ink
    });
    for (x, y, w, h) in [
        (rect.x, rect.y, rect.w, SLOT_BORDER),
        (rect.x, rect.y + rect.h - SLOT_BORDER, rect.w, SLOT_BORDER),
        (rect.x, rect.y, SLOT_BORDER, rect.h),
        (rect.x + rect.w - SLOT_BORDER, rect.y, SLOT_BORDER, rect.h),
    ] {
        out.push(MathItem::Rule { x, y, w, h, color });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Widget;
    use ui_core::cx::MathLayout;

    /// A host with one math face and one formula shape.
    struct Host;

    impl MathLayout for Host {
        fn default_family(&mut self) -> Option<String> {
            Some("Test Math".to_owned())
        }

        fn layout(
            &mut self,
            family: &str,
            source: &str,
            size: f32,
            display: bool,
            _style: Option<MathErrorStyle>,
        ) -> Result<ui_core::Formula, ui_core::math::MathError> {
            if family == "Test Math" && source == "x^2" && size == 16.0 && !display {
                Ok(ui_core::Formula {
                    width: 40.0,
                    ascent: 10.0,
                    descent: 3.0,
                    items: Vec::new(),
                    errors: Vec::new(),
                })
            } else {
                Err(ui_core::math::MathError::Failed("test failure".to_owned()))
            }
        }
    }

    fn with_host<R>(host: Option<Host>, f: impl FnOnce(&crate::cx::Cx<'_, '_>) -> R) -> R {
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        if let Some(host) = host {
            core.attach_math(std::rc::Rc::new(std::cell::RefCell::new(host)));
        }
        let cx = crate::cx::Cx::new(&mut core, ui_style::Env::root());
        f(&cx)
    }

    /// A formula draws its display list when the host can lay it out, and its source when
    /// the host has no math face: an empty box would hide that it never rendered.
    #[test]
    fn a_formula_draws_its_display_list_or_falls_back_to_its_source() {
        let math = Math::new(Id::str("m"), "x^2", 16.0, ui_core::Color::WHITE);
        with_host(Some(Host), |cx| {
            let decl = math.decl(cx);
            assert_eq!(
                decl.kind,
                NodeKind::Math,
                "the host's formula becomes a math node"
            );
            let NodeData::Math {
                formula, family, ..
            } = &decl.data
            else {
                panic!("a math node carries the display list");
            };
            assert_eq!(
                family.as_ref(),
                "Test Math",
                "the node names the face it was laid out with"
            );
            assert_eq!(formula.width, 40.0);
        });
        with_host(None, |cx| {
            let decl = math.decl(cx);
            assert_eq!(
                decl.kind,
                NodeKind::Text,
                "without a math face the source is drawn"
            );
            let NodeData::Text { block, .. } = &decl.data else {
                panic!("the fallback is a text node");
            };
            let (text, _) = block.flatten();
            assert_eq!(text, "x^2");
        });
    }
}
