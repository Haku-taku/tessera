//! Single-pass deterministic layout.

use crate::geom::{Insets, Rect, Size};
use crate::id::NodeId;
use crate::layout::{Align, Constraints, LayoutStats, OverlayAlign, SizeClass};
use crate::tree::{NodeData, NodeKind, Tree};

/// The offset a box of `slack` spare space takes for one axis's alignment.
fn align_offset(align: OverlayAlign, slack: f32) -> f32 {
    match align {
        OverlayAlign::Start => 0.0,
        OverlayAlign::Center => slack * 0.5,
        OverlayAlign::End => slack,
    }
}

/// Slack for the I1 check: float arithmetic must not turn an exact fit into a violation.
const I1_EPS: f32 = 1e-3;

/// Text measurement, injected so layout stays pure.
pub trait TextMeasure {
    /// Wrap `block` at `max_width`: the one result layout sizes and paint draws from.
    ///
    /// Returns a shared handle because both sides keep it for the frame; wrapping twice
    /// is what makes a caret and its painted line disagree.
    ///
    /// Takes the whole block so per-run styles reach the measurer: rich text measures
    /// each span at its own size.
    fn layout(
        &mut self,
        block: &crate::text::TextBlock,
        max_width: f32,
    ) -> std::sync::Arc<crate::text::InlineLayout>;

    /// Size of `block` wrapped at `max_width`, honouring its own `max_lines`.
    ///
    /// Derived from [`TextMeasure::layout`], so the two can never disagree.
    fn measure(&mut self, block: &crate::text::TextBlock, max_width: f32) -> Size {
        self.layout(block, max_width).size()
    }

    /// Smallest and largest size the block can take: min-content and max-content.
    ///
    /// The default treats the whole block as unbreakable; a real measurer refines it.
    fn intrinsic(&mut self, block: &crate::text::TextBlock) -> (Size, Size) {
        let max = self.measure(block, f32::INFINITY);
        (max, max)
    }
}

/// A text measurer that assumes a fixed advance per character.
///
/// Deterministic and dependency-free: good enough for layout tests and the
/// first window example. A real one lives in `ui-text`.
#[derive(Clone, Copy, Debug)]
pub struct MonospaceMeasure {
    /// Advance as a fraction of font size.
    pub advance: f32,
    /// Line height as a fraction of font size.
    pub line_height: f32,
}

impl Default for MonospaceMeasure {
    fn default() -> Self {
        Self {
            advance: 0.6,
            line_height: 1.3,
        }
    }
}
/// Lets an engine be swapped between measurers at runtime.
impl TextMeasure for Box<dyn TextMeasure> {
    fn layout(
        &mut self,
        block: &crate::text::TextBlock,
        max_width: f32,
    ) -> std::sync::Arc<crate::text::InlineLayout> {
        (**self).layout(block, max_width)
    }

    fn intrinsic(&mut self, block: &crate::text::TextBlock) -> (Size, Size) {
        (**self).intrinsic(block)
    }
}

impl TextMeasure for MonospaceMeasure {
    fn layout(
        &mut self,
        block: &crate::text::TextBlock,
        max_width: f32,
    ) -> std::sync::Arc<crate::text::InlineLayout> {
        use crate::text::{AtomBox, InlineAtom, InlineLayout, InlineLine};
        let (text, spans) = block.flatten();
        if spans.is_empty() {
            let line_h = crate::text::TextStyle::default().size * self.line_height;
            // No bidi algorithm lives in this portable fallback, so the block reads
            // left-to-right here; the real measurer sets the paragraph's own direction.
            return std::sync::Arc::new(InlineLayout {
                rtl: false,
                lines: vec![InlineLine {
                    range: 0..0,
                    y: 0.0,
                    height: line_h,
                    baseline: 0.0,
                    width: 0.0,
                    ellipsis: false,
                    runs: Vec::new(),
                    atoms: Vec::new(),
                }],
                width: 0.0,
                height: line_h,
                baseline: 0.0,
            });
        }
        // Per character: byte offset, byte length, advance, ascent, descent, line box, atom.
        //
        // The estimator has no face, so it assumes an ascent of 1em and a descent of 0.3em;
        // an atom takes its own size and never the room left on the line.
        struct Piece {
            start: usize,
            len: usize,
            w: f32,
            ascent: f32,
            descent: f32,
            line_h: f32,
            atom: Option<AtomBox>,
        }
        let mut chars: Vec<Piece> = Vec::new();
        for span in &spans {
            let size = span.style.size;
            // Tracking widens every cluster, the last one included; see `TextStyle`.
            let advance = (size * self.advance).max(1.0) + span.style.letter_spacing;
            let ascent = size;
            let descent = size * 0.3;
            let line_h = (size * self.line_height).max(ascent + descent);
            // A room is atomic: it takes its own width and never breaks, like an atom.
            if let Some(room) = span.room {
                chars.push(Piece {
                    start: span.range.start,
                    len: span.range.len(),
                    w: room.width,
                    ascent,
                    descent,
                    line_h,
                    atom: None,
                });
                continue;
            }
            for (offset, ch) in text[span.range.clone()].char_indices() {
                let start = span.range.start + offset;
                let (w, a, d, h, atom) = match block.atom_at(start) {
                    Some(atom) if ch == crate::text::OBJECT => {
                        let (a, d) = atom.align.extents(atom.size, atom.baseline);
                        (atom.size.w, a, d, atom.size.h, Some(atom.clone()))
                    }
                    _ => (advance, ascent, descent, line_h, None),
                };
                chars.push(Piece {
                    start,
                    len: ch.len_utf8(),
                    // Collapsed syntax keeps its byte and takes no advance, as in the shaper.
                    w: if !span.visibility.takes_room() && atom.is_none() {
                        0.0
                    } else {
                        w
                    },
                    ascent: a,
                    descent: d,
                    line_h: h,
                    atom,
                });
            }
        }
        // Greedy fill: an atom as wide as the block starts a line of its own. The first
        // line gives up the indent and every later one the hanging indent.
        let wrap = max_width.is_finite() && max_width > 0.0;
        let edges = block.edges();
        let first_limit = (max_width - edges.first.max(0.0)).max(0.0);
        let body_limit = (max_width - edges.body.max(0.0)).max(0.0);
        let mut chunks: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        let mut used = 0.0f32;
        for (index, piece) in chars.iter().enumerate() {
            let limit = if chunks.is_empty() {
                first_limit
            } else {
                body_limit
            };
            if wrap && !current.is_empty() && used + piece.w > limit + 0.01 {
                chunks.push(std::mem::take(&mut current));
                used = 0.0;
            }
            used += piece.w;
            current.push(index);
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        // `max_lines` keeps the first lines and marks the last one ellipsized.
        let raw = chunks.len();
        let cap = block
            .max_lines
            .map_or(raw, |c| (c.max(1) as usize).min(raw));
        let truncated = cap < raw;
        chunks.truncate(cap);

        let mut lines = Vec::with_capacity(chunks.len());
        let mut y = 0.0f32;
        for (n, chunk) in chunks.iter().enumerate() {
            let mut text_ascent = 0.0f32;
            let mut text_descent = 0.0f32;
            let mut text_box = 0.0f32;
            let mut width = 0.0f32;
            for &i in chunk {
                let piece = &chars[i];
                width += piece.w;
                if piece.atom.is_none() {
                    text_ascent = text_ascent.max(piece.ascent);
                    text_descent = text_descent.max(piece.descent);
                    text_box = text_box.max(piece.line_h);
                }
            }
            let mut ascent = text_ascent;
            let mut descent = text_descent;
            for &i in chunk {
                if let Some(atom) = &chars[i].atom {
                    let (above, below) = atom.align.extents(atom.size, atom.baseline);
                    ascent = ascent.max(above);
                    descent = descent.max(below);
                }
            }
            let height = text_box.max(ascent + descent).max(1.0);
            let mut atoms = Vec::new();
            let mut pen = 0.0f32;
            for &i in chunk {
                let piece = &chars[i];
                if let Some(atom) = &piece.atom {
                    atoms.push(InlineAtom {
                        at: piece.start,
                        len: atom.len,
                        x: pen,
                        y: atom.align.top(atom.size, atom.baseline, ascent, height),
                        size: atom.size,
                        align: atom.align,
                    });
                }
                pen += piece.w;
            }
            let first = &chars[chunk[0]];
            let last = &chars[*chunk.last().unwrap()];
            lines.push(InlineLine {
                range: first.start..last.start + last.len,
                y,
                height,
                baseline: ascent,
                width,
                ellipsis: truncated && n + 1 == chunks.len(),
                runs: crate::text::runs_in(block, &spans, first.start..last.start + last.len),
                atoms,
            });
            // A gap sits between lines only; the last line adds none.
            y += height + block.line_gap;
        }
        let height = (y - block.line_gap).max(0.0);
        let baseline = lines.first().map_or(0.0, |l| l.baseline);
        std::sync::Arc::new(InlineLayout {
            rtl: false,
            width: lines.iter().enumerate().fold(0.0f32, |acc, (index, line)| {
                acc.max(line.width + edges.at(index))
            }),
            lines,
            height,
            baseline,
        })
    }

    fn intrinsic(&mut self, block: &crate::text::TextBlock) -> (Size, Size) {
        let max = self.measure(block, f32::INFINITY);
        let (text, spans) = block.flatten();
        if spans.is_empty() {
            return (Size::new(0.0, max.h), max);
        }
        let mut widest = 0.0f32;
        let mut line_h = 0.0f32;
        for span in &spans {
            let char_w = (span.style.size * self.advance).max(1.0);
            line_h = line_h.max(span.style.size * self.line_height);
            // Breaks happen at whitespace; the longest run fixes the narrowest line.
            for word in text[span.range.clone()].split_whitespace() {
                widest = widest.max(char_w * word.chars().count() as f32);
            }
        }
        // An atom is unbreakable: its own width, never its U+FFFC's advance, floors the line.
        for atom in &block.atoms {
            widest = widest.max(atom.size.w);
            line_h = line_h.max(atom.size.h);
        }
        (Size::new(widest, line_h), max)
    }
}

/// The built-in layout engine.
pub struct BuiltinLayout<M: TextMeasure> {
    measure: M,
    /// Nodes measured since the current solve began; reported as `LayoutStats::remeasured`.
    remeasured: usize,
}

/// Resolve a `Len` upper bound; `Auto` is unbounded and an unresolvable `Pct` acts as one.
fn max_len(len: crate::layout::Len, avail: f32) -> f32 {
    match len {
        crate::layout::Len::Px(v) => v,
        crate::layout::Len::Pct(p) if avail.is_finite() => avail * p / 100.0,
        _ => f32::INFINITY,
    }
}

/// The size class layout works with: a text leaf's height hint is its line box, so it is a
/// floor for one line and never a ceiling on the block.
fn layout_size_class(kind: NodeKind, sc: SizeClass) -> SizeClass {
    let h = sc.height;
    if kind != NodeKind::Text
        || h.sizing != crate::layout::Sizing::Fit
        || !h.hint.is_finite()
        || h.hint <= h.min
    {
        return sc;
    }
    // Making the hint the minimum leaves the derived maximum open (`sizes()` only caps a
    // hint it can shrink to).
    SizeClass {
        height: crate::layout::AxisSize { min: h.hint, ..h },
        ..sc
    }
}

/// Resolve a `Len` lower bound; `Auto` means zero.
fn min_len(len: crate::layout::Len, avail: f32) -> f32 {
    match len {
        crate::layout::Len::Px(v) => v.max(0.0),
        crate::layout::Len::Pct(p) if avail.is_finite() => (avail * p / 100.0).max(0.0),
        _ => 0.0,
    }
}

/// One axis' used size: the tentative size clamped into `[floor, ceiling]`.
///
/// `floor` is the content minimum clamped by the declared bounds (CSS clamps a size's
/// min-content contribution by its own max); `ceiling` never falls below it.
/// P1 reads `Stretch` as content.
fn used_axis(
    sizing: crate::layout::Sizing,
    avail: f32,
    min_content: f32,
    max_content: f32,
    measured: f32,
    decl_min: f32,
    decl_max: f32,
) -> f32 {
    use crate::layout::{Len, Sizing};
    let tentative = match sizing {
        Sizing::Definite(Len::Px(v)) => v,
        Sizing::Definite(Len::Pct(p)) if avail.is_finite() => avail * p / 100.0,
        // Fit / Stretch / Auto all start from the content measured under `avail`.
        _ => measured.max(min_content).min(max_content.max(min_content)),
    };
    let (floor, ceiling) = content_bounds(min_content, decl_min, decl_max);
    tentative.max(floor).min(ceiling)
}

/// The usable `[floor, ceiling]` for a node: content floor clamped by its declared bounds.
fn content_bounds(min_content: f32, decl_min: f32, decl_max: f32) -> (f32, f32) {
    let ceiling = decl_max.max(decl_min).max(0.0);
    let floor = decl_min.max(min_content.min(ceiling)).max(0.0);
    (floor, ceiling)
}

/// The extent a `Fit` (hug) axis settles on, from what the last measure saw.
///
/// It runs the same clamp `used_axis` does, so a transition's target is the size hug would
/// have taken: the declared bounds against the space the node was offered, over the content
/// extent layout measured before its own axis sizing overwrote it. `None` before the first
/// measure, or when the axis is not a hug.
pub fn hug_extent(node: &crate::tree::Node, vertical: bool) -> Option<f32> {
    let last = node.last_measure?;
    let declared = layout_size_class(node.kind, node.declared_size);
    let axis = if vertical {
        declared.height
    } else {
        declared.width
    };
    if axis.sizing != crate::layout::Sizing::Fit {
        return None;
    }
    let sizes = axis.sizes();
    let (avail, min_c, content) = if vertical {
        (
            last.constraints.max.h,
            node.min_content.h,
            last.result.content.h,
        )
    } else {
        (
            last.constraints.max.w,
            node.min_content.w,
            last.result.content.w,
        )
    };
    let (floor, ceiling) =
        content_bounds(min_c, min_len(sizes.min, avail), max_len(sizes.max, avail));
    Some(content.max(floor).min(ceiling))
}

/// In-flow margins: finite edges only. `auto` has no free-space meaning inside the flow yet.
/// A stacked parent reads the child's alternate margin, which is how one child lines up two ways.
fn flow_margin(tree: &Tree, kid: NodeId, stacked: bool) -> crate::geom::Insets {
    use crate::geom::Insets;
    let m = tree
        .get(kid)
        .map(|n| match (stacked, n.box_model.margin_alt) {
            (true, Some(alt)) => alt,
            _ => n.box_model.margin,
        })
        .unwrap_or(Insets::ZERO);
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    Insets {
        top: finite(m.top),
        right: finite(m.right),
        bottom: finite(m.bottom),
        left: finite(m.left),
    }
}

/// One axis reading of a flow container while it measures: extents and intrinsics.
#[derive(Clone, Copy, Default)]
struct FlowSum {
    main: f32,
    cross: f32,
    main_min: f32,
    main_max: f32,
    cross_min: f32,
    cross_max: f32,
}

/// What one measuring pass over a flow container's children produced.
#[derive(Clone, Copy, Default)]
struct FlowAgg {
    row: FlowSum,
    col: FlowSum,
    /// How far the tallest child rises above the shared baseline, and how far the deepest
    /// descends below it: together the height of a baseline-aligned line, which differs
    /// from the tallest child whenever the children's baselines disagree.
    above: f32,
    below: f32,
    widest: f32,
    tallest: f32,
    axis_min: [f32; 2],
    axis_max: [f32; 2],
    flow: usize,
}

/// The size placement will give a child along one axis, from its own size class.
fn placed_along(
    sizes: crate::layout::AxisSizes,
    hint: f32,
    min_c: f32,
    inner: f32,
    grow: crate::layout::Grow,
    measured: f32,
) -> f32 {
    let (floor, ceiling) =
        content_bounds(min_c, min_len(sizes.min, inner), max_len(sizes.max, inner));
    match (grow, sizes.size) {
        // A zero-basis grower's size comes from the line, not content.
        (crate::layout::Grow::Split(_), _) => measured,
        (_, crate::layout::Sizing::Definite(crate::layout::Len::Px(v))) => v,
        // Fitting content already measured itself against its hint cap.
        (_, crate::layout::Sizing::Fit) => measured,
        _ => measured.max(preferred(hint)),
    }
    .max(floor)
    .min(ceiling)
}

/// The flex items one line solves over, plus each item's content contribution.
///
/// A zero-basis grower contributes none of its content to `basis`, so a scrolling line —
/// which is exactly the sum of what its items want — has to read the contents separately.
fn line_items(
    tree: &Tree,
    line: &[NodeId],
    horizontal: bool,
    box_main: f32,
) -> (Vec<crate::flex::Item>, Vec<f32>) {
    line.iter()
        .map(|kid| {
            let n = tree.get(*kid);
            let sc = n
                .map(|n| layout_size_class(n.kind, n.size_class))
                .unwrap_or(SizeClass::fixed(0.0, 0.0));
            let axes = sc.axes();
            let axis_sizes = if horizontal { axes.0 } else { axes.1 };
            let (min_c, max_c, measured) = n
                .map(|n| {
                    if horizontal {
                        (n.min_content.w, n.max_content.w, n.rect.size.w)
                    } else {
                        (n.min_content.h, n.max_content.h, n.rect.size.h)
                    }
                })
                .unwrap_or((0.0, 0.0, 0.0));
            let grow = if horizontal { sc.grow_x } else { sc.grow_y };
            // Every item shrinks by default, as CSS `flex-shrink: 1` does; a declared floor
            // (a `fixed` size) is what stops it.
            let (grow_f, shrink_f) = match grow {
                crate::layout::Grow::No => (0.0, 1.0),
                crate::layout::Grow::Yes => (1.0, 1.0),
                crate::layout::Grow::Flex(w) => (w as f32, 1.0),
                crate::layout::Grow::Split(w) => (w as f32, 1.0),
            };
            // The basis is the declared size, else content clamped into the item's own
            // bounds; a zero-basis grower (`Split`) starts at 0.
            let hint = if horizontal {
                sc.width.hint
            } else {
                sc.height.hint
            };
            let preferred = if hint.is_finite() && hint > 0.0 {
                hint
            } else {
                0.0
            };
            let content = if max_c.is_finite() { max_c } else { measured };
            let decl_min = min_len(axis_sizes.min, box_main);
            let decl_max = max_len(axis_sizes.max, box_main);
            let (floor, ceiling) = content_bounds(min_c, decl_min, decl_max);
            let basis = match (grow, axis_sizes.size) {
                (crate::layout::Grow::Split(_), _) => 0.0,
                (_, crate::layout::Sizing::Definite(crate::layout::Len::Px(v))) => v,
                // A fitting item hugs content; its hint caps it, it is no floor.
                (_, crate::layout::Sizing::Fit) => content.clamp(floor, ceiling),
                _ => content.max(preferred),
            };
            // What this item wants before free space: its own size, or its content when it
            // grows from a zero basis.
            let contribution = match (grow, axis_sizes.size) {
                (crate::layout::Grow::Split(_), _) => content.clamp(floor, ceiling),
                _ => basis,
            };
            (
                crate::flex::Item {
                    basis,
                    grow: grow_f,
                    shrink: shrink_f,
                    min: floor,
                    max: ceiling.max(floor),
                    hypothetical: basis.max(floor).min(ceiling.max(floor)),
                },
                contribution,
            )
        })
        .unzip()
}

/// Total in-flow margin the line has to subtract before it can solve.
fn flow_margins_total(tree: &Tree, line: &[NodeId], horizontal: bool, stacked: bool) -> f32 {
    line.iter()
        .map(|k| {
            let m = flow_margin(tree, *k, stacked);
            if horizontal {
                m.left + m.right
            } else {
                m.top + m.bottom
            }
        })
        .sum()
}

/// Per-child main-axis caps for the children a line squeezes below their measured size.
fn squeezed_limits(
    tree: &Tree,
    parent: NodeId,
    child_count: usize,
    horizontal: bool,
    inner: Size,
    gap: f32,
    stacked: bool,
) -> Option<Vec<f32>> {
    let line: Vec<NodeId> = (0..child_count)
        .filter_map(|index| tree.child_at(Some(parent), index))
        .filter(|kid| !tree.floats(*kid) && !tree.out_of_flow(*kid))
        .collect();
    if line.is_empty() {
        return None;
    }
    let box_main = if horizontal { inner.w } else { inner.h };
    let (items, _) = line_items(tree, &line, horizontal, box_main);
    let margins = flow_margins_total(tree, &line, horizontal, stacked);
    let mains = crate::flex::solve_line(&items, (box_main - margins).max(0.0), gap);
    let mut caps = Vec::with_capacity(line.len());
    let mut squeezed = false;
    for (i, kid) in line.iter().enumerate() {
        let measured = tree
            .get(*kid)
            .map(|n| {
                if horizontal {
                    n.rect.size.w
                } else {
                    n.rect.size.h
                }
            })
            .unwrap_or(0.0);
        let main = mains.get(i).copied().unwrap_or(measured);
        squeezed |= main + I1_EPS < measured;
        caps.push(if main + I1_EPS < measured {
            main
        } else {
            f32::INFINITY
        });
    }
    squeezed.then_some(caps)
}

/// Whether the last measure stacked this flow container along its cross axis.
fn stacked_axis(tree: &Tree, nid: NodeId) -> bool {
    tree.get(nid).map(|n| n.flow_vertical).unwrap_or(false)
}

/// Whether the wheel may move this container along its own main axis.
/// Whether this axis cuts content, which drops the item's automatic minimum to zero.
fn main_clips(horizontal: bool, clip: crate::layout::Clip) -> bool {
    let overflow = if horizontal { clip.x } else { clip.y };
    overflow != crate::layout::Overflow::Visible
}

fn main_scrolls(horizontal: bool, clip: crate::layout::Clip) -> bool {
    if horizontal {
        clip.x == crate::layout::Overflow::Scroll
    } else {
        clip.y == crate::layout::Overflow::Scroll
    }
}

/// Intersect `mask` with `rect` only on the axes this clip actually cuts.
fn apply_clip(mask: Rect, rect: Rect, clip: crate::layout::Clip) -> Rect {
    use crate::layout::Overflow;
    let (x0, x1) = if clip.x == Overflow::Visible {
        (mask.min_x(), mask.max_x())
    } else {
        (
            mask.min_x().max(rect.min_x()),
            mask.max_x().min(rect.max_x()),
        )
    };
    let (y0, y1) = if clip.y == Overflow::Visible {
        (mask.min_y(), mask.max_y())
    } else {
        (
            mask.min_y().max(rect.min_y()),
            mask.max_y().min(rect.max_y()),
        )
    };
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// The clip a keyboard cares about: a scroll viewport translates content, it does not hide it.
fn focus_clip(clip: crate::layout::Clip) -> crate::layout::Clip {
    use crate::layout::Overflow;
    let unclip = |o: Overflow| {
        if o == Overflow::Scroll {
            Overflow::Visible
        } else {
            o
        }
    };
    crate::layout::Clip {
        x: unclip(clip.x),
        y: unclip(clip.y),
    }
}

/// The template, or a single auto column when the author declared none.
fn grid_tracks_of(params: &crate::tree::ContainerParams) -> crate::layout::TrackList {
    params
        .tracks
        .as_deref()
        .cloned()
        .unwrap_or(crate::layout::TrackList {
            cols: vec![crate::layout::Track::Auto],
            rows: Vec::new(),
            auto_flow: crate::layout::AutoFlow::Row,
        })
}

/// Place the items and size both axes; `use_min` takes min-content for the floor pass.
/// Where every item lands and which tracks exist, before any track is sized.
///
/// Auto placement and the template expansion depend only on the items and the offered
/// space, so measure's two passes and placement's one share a single computation.
struct GridCells {
    cells: Vec<crate::grid::Cell>,
    cols: Vec<crate::layout::Track>,
    rows: Vec<crate::layout::Track>,
}

/// Expand the template and place every item; no track size depends on the content yet.
fn grid_cells(
    tree: &Tree,
    kids: &[NodeId],
    params: &crate::tree::ContainerParams,
    available: Size,
) -> GridCells {
    let tracks = grid_tracks_of(params);
    let areas: Vec<Option<crate::layout::GridArea>> = kids
        .iter()
        .map(|k| tree.get(*k).and_then(|n| n.grid.area))
        .collect();
    // An auto repeat counts the tracks that fit, so the template is spelled out before
    // anything is placed; `auto-fit` collapses its empty tracks only afterwards.
    let mut cols_t = crate::grid::expand_repeat(&tracks.cols, available.w, params.gap);
    let mut rows_t = crate::grid::expand_repeat(&tracks.rows, available.h, params.gap);
    let spans_of = |kid: &NodeId| -> (usize, usize) {
        tree.get(*kid)
            .map(|n| (n.grid.span.0 as usize, n.grid.span.1 as usize))
            .unwrap_or((1, 1))
    };
    let auto_spans: Vec<(usize, usize)> = kids.iter().map(spans_of).collect();
    let (mut cells, mut ncols, nrows) = crate::grid::place_items(
        &areas,
        &auto_spans,
        cols_t.tracks.len(),
        rows_t.tracks.len(),
        tracks.auto_flow,
    );
    if !cols_t.auto_fit.is_empty() {
        ncols =
            crate::grid::collapse_auto_fit(&mut cells, &mut cols_t.tracks, &cols_t.auto_fit, true);
    }
    if !rows_t.auto_fit.is_empty() {
        crate::grid::collapse_auto_fit(&mut cells, &mut rows_t.tracks, &rows_t.auto_fit, false);
    }
    GridCells {
        cells,
        cols: crate::grid::expand(&cols_t.tracks, ncols),
        rows: crate::grid::expand(&rows_t.tracks, nrows),
    }
}

/// Size the tracks of a placed grid; the cells decide where every item goes.
fn grid_layout(
    tree: &Tree,
    kids: &[NodeId],
    params: &crate::tree::ContainerParams,
    available: Size,
    placed: &GridCells,
    definite: (bool, bool),
    use_min: bool,
) -> (Vec<f32>, Vec<f32>) {
    let cols = &placed.cols;
    let rows = &placed.rows;
    let cells = &placed.cells;
    let spans = |axis: usize| -> Vec<crate::grid::Span> {
        let avail = if axis == 0 { available.w } else { available.h };
        cells
            .iter()
            .zip(kids)
            .map(|(cell, kid)| {
                // A definite preferred size *is* the item's contribution, as in CSS: the
                // content is not consulted, so a `fixed(20)` item cannot widen its track.
                let declared = tree.get(*kid).and_then(|n| {
                    let axes = n.size_class.axes();
                    let axis_sizes = if axis == 0 { axes.0 } else { axes.1 };
                    match axis_sizes.size {
                        crate::layout::Sizing::Definite(crate::layout::Len::Px(v)) => {
                            Some(v.max(0.0))
                        }
                        crate::layout::Sizing::Definite(crate::layout::Len::Pct(p))
                            if avail.is_finite() =>
                        {
                            Some((avail * p / 100.0).max(0.0))
                        }
                        _ => None,
                    }
                });
                let (min, max) = match (tree.get(*kid), declared) {
                    (_, Some(d)) => (d, d),
                    (Some(n), None) if axis == 0 => (n.min_content.w, n.max_content.w),
                    (Some(n), None) => (n.min_content.h, n.max_content.h),
                    (None, None) => (0.0, 0.0),
                };
                // A margin is outside the item, so the track must hold it too.
                let m = flow_margin(tree, *kid, false);
                let margin = if axis == 0 {
                    m.left + m.right
                } else {
                    m.top + m.bottom
                };
                crate::grid::Span {
                    start: if axis == 0 { cell.col } else { cell.row },
                    span: if axis == 0 {
                        cell.col_span
                    } else {
                        cell.row_span
                    },
                    min: min + margin,
                    max: max + margin,
                }
            })
            .collect()
    };
    let widths = crate::grid::track_sizes(
        cols,
        &spans(0),
        available.w,
        params.gap,
        definite.0,
        use_min,
    );
    let heights = crate::grid::track_sizes(
        rows,
        &spans(1),
        available.h,
        params.gap,
        definite.1,
        use_min,
    );
    (widths, heights)
}

/// Size and offset one item inside its cell; stretch fills, the rest keep their own size.
fn place_in_cell(
    cell: f32,
    align: Option<crate::layout::Align>,
    axis: crate::layout::AxisSize,
    measured: f32,
) -> (f32, f32) {
    use crate::layout::{Align, Len, Sizing};
    let definite = match axis.sizing {
        Sizing::Definite(Len::Px(v)) => Some(v),
        Sizing::Definite(Len::Pct(p)) => Some(cell * p / 100.0),
        _ => None,
    };
    // The item's own bounds hold in a cell exactly as they do in a row: stretch fills the
    // cell and is then clamped, and a definite or measured size is clamped the same way.
    let bounds = axis.sizes();
    let floor = min_len(bounds.min, cell);
    let ceiling = max_len(bounds.max, cell).max(floor);
    let clamp = |v: f32| v.max(floor).min(ceiling).max(0.0);
    let own = clamp(definite.unwrap_or(measured));
    match align.unwrap_or(Align::Stretch) {
        Align::Stretch if definite.is_none() => (clamp(cell), 0.0),
        Align::Stretch | Align::Start => (own, 0.0),
        Align::Center => (own, ((cell - own) * 0.5).max(0.0)),
        Align::End => (own, (cell - own).max(0.0)),
    }
}

/// The author's "comfortable size" for one axis, or 0 when none was declared.
fn preferred(hint: f32) -> f32 {
    if hint.is_finite() && hint > 0.0 {
        hint
    } else {
        0.0
    }
}

/// The hint a parent may treat as a floor: a fitting item's hint caps it instead.
fn hint_floor(axis: crate::layout::AxisSize) -> f32 {
    if axis.sizing == crate::layout::Sizing::Fit {
        0.0
    } else {
        preferred(axis.hint)
    }
}

/// What one child contributes to its parent's intrinsic range on one axis.
///
/// An explicit size wins over content; the lower bound always folds in the content floor.
fn contribution(sizes: crate::layout::AxisSizes, min_c: f32, max_c: f32, avail: f32) -> (f32, f32) {
    use crate::layout::{Len, Sizing};
    let (lo, ceiling) = content_bounds(min_c, min_len(sizes.min, avail), max_len(sizes.max, avail));
    let declared = match sizes.size {
        Sizing::Definite(Len::Px(v)) => v,
        Sizing::Definite(Len::Pct(p)) if avail.is_finite() => avail * p / 100.0,
        _ => max_c,
    };
    (lo, declared.max(lo).min(ceiling))
}

/// The content box of a node's parent: what a relative node's percentages resolve against.
fn parent_content_box(tree: &Tree, nid: NodeId) -> crate::geom::Size {
    let Some(parent) = tree
        .get(nid)
        .and_then(|n| n.parent)
        .and_then(|parent| tree.get(parent))
    else {
        return crate::geom::Size::ZERO;
    };
    let pad = parent.box_model.padding;
    crate::geom::Size::new(
        (parent.rect.size.w - pad.left - pad.right).max(0.0),
        (parent.rect.size.h - pad.top - pad.bottom).max(0.0),
    )
}

/// Move every descendant of `nid` by `delta`, leaving the node's own box where it is.
fn shift_subtree(tree: &mut Tree, nid: NodeId, delta: crate::geom::Point) {
    // The walk mutates the tree it reads, so a borrowed child slice cannot outlive the
    // first move; positions are all it changes, so an index walk sees the same children.
    let child_count = tree.children_of(Some(nid)).len();
    for index in 0..child_count {
        let Some(kid) = tree.child_at(Some(nid), index) else {
            continue;
        };
        if let Some(n) = tree.get_mut(kid) {
            n.rect.origin.x += delta.x;
            n.rect.origin.y += delta.y;
        }
        shift_subtree(tree, kid, delta);
    }
}

/// The rect a modal node covers: its nearest `modal_host` ancestor, else the window.
fn modal_target_rect(tree: &Tree, nid: NodeId, target: crate::tree::ModalTarget) -> Option<Rect> {
    let root = tree.root()?;
    // The root is being placed right now, so its stored rect is last frame's or nothing.
    if nid == root {
        return None;
    }
    let window = tree.get(root)?.rect;
    if target == crate::tree::ModalTarget::Global {
        return Some(window);
    }
    let mut cur = tree.get(nid)?.parent;
    while let Some(host) = cur {
        let node = tree.get(host)?;
        if node.data.params().is_some_and(|p| p.modal_host) {
            return Some(node.rect);
        }
        cur = node.parent;
    }
    Some(window)
}

/// The visual displacement a relative node's insets declare.
///
/// Over-constrained pairs follow CSS: `left` wins horizontally, `top` vertically.
fn relative_shift(inset: crate::layout::LenInsets, cb: crate::geom::Size) -> crate::geom::Point {
    let inset = inset.resolve(cb);
    let x = if inset.left.is_finite() {
        inset.left
    } else if inset.right.is_finite() {
        -inset.right
    } else {
        0.0
    };
    let y = if inset.top.is_finite() {
        inset.top
    } else if inset.bottom.is_finite() {
        -inset.bottom
    } else {
        0.0
    };
    crate::geom::Point::new(x, y)
}

/// The paint offset a node will carry once [`accumulate_offsets`] runs: its own declared
/// offset and every ancestor's, summed without touching the tree.
fn pending_offset(tree: &Tree, nid: NodeId) -> crate::geom::Point {
    let mut sum = crate::geom::Point::ZERO;
    let mut next = Some(nid);
    while let Some(current) = next {
        let Some(node) = tree.get(current) else {
            break;
        };
        let shift = if node.placement.position == crate::layout::Position::Relative {
            relative_shift(node.placement.inset, parent_content_box(tree, current))
        } else {
            crate::geom::Point::ZERO
        };
        sum = crate::geom::Point::new(
            sum.x + node.local_offset.x + shift.x,
            sum.y + node.local_offset.y + shift.y,
        );
        next = node.parent;
    }
    sum
}

/// Fold each node's absolute offset and the nearest declared environment down the tree.
fn accumulate_offsets(
    tree: &mut Tree,
    nid: NodeId,
    inherited: crate::geom::Point,
    inherited_env: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
) {
    let own = {
        let Some(node) = tree.get(nid) else {
            return;
        };
        // `offset` and a relative inset meet here, in the one place that adds them. A
        // relative node keeps its flow slot, so its percentages are of its parent's content
        // box; only a relative node pays for finding that box.
        let shift = if node.placement.position == crate::layout::Position::Relative {
            relative_shift(node.placement.inset, parent_content_box(tree, nid))
        } else {
            crate::geom::Point::ZERO
        };
        crate::geom::Point::new(
            inherited.x + node.local_offset.x + shift.x,
            inherited.y + node.local_offset.y + shift.y,
        )
    };
    let env = inherited_env;
    if let Some(node) = tree.get_mut(nid) {
        node.offset = own;
    }
    // Indexed rather than iterated: the child walk mutates the tree it is reading.
    let count = tree.children_of(Some(nid)).len();
    for i in 0..count {
        let Some(child) = tree.child_at(Some(nid), i) else {
            continue;
        };
        accumulate_offsets(tree, child, own, env.clone());
    }
}

/// The containing block of an out-of-flow node: the padding box of its nearest positioned
/// ancestor, else the root box.
fn containing_block(tree: &Tree, nid: NodeId) -> Rect {
    let mut cur = tree.get(nid).and_then(|n| n.parent);
    while let Some(a) = cur {
        let Some(node) = tree.get(a) else {
            break;
        };
        if node.placement.position != crate::layout::Position::Static {
            // A border is ink inside the box, so the padding box is the whole rect.
            return node.rect;
        }
        cur = node.parent;
    }
    tree.root()
        .and_then(|r| tree.get(r))
        .map(|n| n.rect)
        .unwrap_or(Rect::ZERO)
}

/// Room an axis has once its declared insets are taken out.
fn inset_avail(cb: f32, a: f32, b: f32) -> f32 {
    let a = if a.is_finite() { a } else { 0.0 };
    let b = if b.is_finite() { b } else { 0.0 };
    (cb - a - b).max(0.0)
}

/// Solve one axis of an out-of-flow box: its size, and where its margin edge starts.
///
/// `None` for the offset means neither inset is declared, so the caller uses the static
/// position. A non-finite margin is `auto`, split evenly when both sides are.
fn solve_oof_axis(
    cb: f32,
    a: f32,
    b: f32,
    size_auto: bool,
    measured: f32,
    ma: f32,
    mb: f32,
    align: crate::layout::Align,
) -> (f32, Option<f32>) {
    use crate::layout::Align;
    let a = a.is_finite().then_some(a);
    let b = b.is_finite().then_some(b);
    let ma_auto = !ma.is_finite();
    let mb_auto = !mb.is_finite();
    let ma0 = if ma_auto { 0.0 } else { ma };
    let mb0 = if mb_auto { 0.0 } else { mb };
    let mut size = measured;
    // Stretch-fit: both insets fix the edges, so an auto size is whatever is left.
    if size_auto && let (Some(a), Some(b)) = (a, b) {
        size = (cb - a - b - ma0 - mb0).max(0.0);
    }
    let pos = match (a, b) {
        (Some(a), Some(b)) => {
            let leftover = cb - a - b - size - ma0 - mb0;
            let extra = if ma_auto && mb_auto {
                leftover * 0.5
            } else if ma_auto {
                leftover
            } else if mb_auto {
                0.0
            } else {
                // Over-constrained: self-alignment decides where the spare space goes.
                match align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => leftover * 0.5,
                    Align::End => leftover,
                }
            };
            Some(a + ma0 + extra)
        }
        (Some(a), None) => Some(a + ma0),
        (None, Some(b)) => Some(cb - b - size - mb0),
        (None, None) => None,
    };
    (size, pos)
}

/// Recompute every mask from the visual boxes, after offsets are folded in.
///
/// Hit and paint both test against the mask, so it has to live in the same space the
/// pixels do; computing it during placement would leave a displaced clip behind.
fn compute_masks(
    tree: &mut Tree,
    nid: NodeId,
    parent_mask: Rect,
    parent_focus: Rect,
    viewport: Rect,
) {
    let Some(node) = tree.get(nid) else {
        return;
    };
    let visual = node.visual_rect();
    let clip = node
        .data
        .params()
        .map(|p| p.clip)
        .unwrap_or(crate::layout::Clip::VISIBLE);
    // A modal covers its target, an overlay hangs off its anchor and a lifted node floats
    // above its layout: none of them sits inside the clip of wherever it is declared.
    let detached =
        node.data.params().and_then(|p| p.modal).is_some() || node.placement.overlay.is_some();
    let lifted = node.placement.lift;
    let base = if detached {
        visual
    } else if lifted {
        viewport
    } else {
        parent_mask
    };
    let child_count = tree.children_of(Some(nid)).len();
    let mask = apply_clip(base, visual, clip);
    let focus_base = if detached {
        visual
    } else if lifted {
        viewport
    } else {
        parent_focus
    };
    let focus_mask = apply_clip(focus_base, visual, focus_clip(clip));
    if let Some(n) = tree.get_mut(nid) {
        n.mask = mask;
        n.focus_mask = focus_mask;
    }
    for i in 0..child_count {
        let Some(child) = tree.child_at(Some(nid), i) else {
            continue;
        };
        compute_masks(tree, child, mask, focus_mask, viewport);
    }
}

impl<M: TextMeasure> crate::layout::LayoutEngine for BuiltinLayout<M> {
    fn solve(&mut self, tree: &mut Tree, root: NodeId, constraints: Constraints) -> LayoutStats {
        let max = constraints.max;
        self.remeasured = 0;
        tree.clear_platform_panels();
        // Bottom-up demand, then top-down placement: one pass each, no retry.
        //
        // The root fills the available space unless it declares a fixed size or an
        // explicit grow policy; otherwise nothing inside it could ever grow.
        let (root_res, _) = self.measure_node(tree, root, constraints);
        let root_grows = tree
            .get(root)
            .map(|n| {
                let axes = n.size_class.axes();
                n.size_class.grow_x != crate::layout::Grow::No
                    || n.size_class.grow_y != crate::layout::Grow::No
                    || max_len(axes.0.max, max.w).is_infinite()
                    || max_len(axes.1.max, max.h).is_infinite()
            })
            .unwrap_or(false);
        let size = if root_grows {
            max
        } else {
            Size::new(
                root_res.size.w.min(max.w).max(0.0),
                root_res.size.h.min(max.h).max(0.0),
            )
        };
        let mut stats = LayoutStats::default();
        self.place_node(
            tree,
            root,
            Rect::from_origin_size(crate::geom::Point::ZERO, size),
            &mut stats,
        );
        // Out-of-flow boxes come second: they are solved against boxes that are already
        // down, so a static ancestor's size never waits on them.
        self.place_out_of_flow(tree, root, &mut stats);
        // Offsets are inherited: a displaced container must carry its whole subtree, or
        // its background moves while its children stay behind.
        accumulate_offsets(tree, root, crate::geom::Point::ZERO, None);
        // Masks follow the pixels, so they are derived after the offsets are folded in.
        let root_visual = tree
            .get(root)
            .map(|n| n.visual_rect())
            .unwrap_or(Rect::ZERO);
        compute_masks(tree, root, root_visual, root_visual, root_visual);
        stats.remeasured = self.remeasured;
        tree.set_layout_clean();
        stats
    }
}

impl<M: TextMeasure> BuiltinLayout<M> {
    pub fn new(measure: M) -> Self {
        Self {
            measure,
            remeasured: 0,
        }
    }

    /// The cached measurement, when the node is clean and was last asked the same question.
    ///
    /// A clean subtree answers from its cache; a dirty one invalidates the whole chain above
    /// it, so a hit here means nothing below changed either.
    fn measure_node(
        &mut self,
        tree: &mut Tree,
        nid: NodeId,
        constraints: Constraints,
    ) -> (crate::layout::MeasureResult, usize) {
        let cached = tree.get(nid).and_then(|n| {
            if n.dirty {
                return None;
            }
            n.last_measure
                .filter(|m| m.constraints == constraints)
                .map(|m| m.result)
        });
        if let Some(result) = cached {
            return (result, 0);
        }
        self.remeasured += 1;
        let (result, count) = self.measure_node_uncached(tree, nid, constraints);
        if let Some(n) = tree.get_mut(nid) {
            n.last_measure = Some(crate::layout::LastMeasure {
                constraints,
                result,
            });
        }
        (result, count)
    }

    /// One pass over a flow container's children. `limits` caps each in-flow child's main
    /// axis at the size the line will actually give it, so its content reflows there.
    fn measure_flow_children(
        &mut self,
        tree: &mut Tree,
        parent: NodeId,
        child_count: usize,
        switchable: bool,
        gap: f32,
        kid_avail: Size,
        limits: Option<(&[f32], bool)>,
        count: &mut usize,
    ) -> FlowAgg {
        let mut agg = FlowAgg::default();
        let mut flow = 0usize;
        for index in 0..child_count {
            let Some(kid) = tree.child_at(Some(parent), index) else {
                continue;
            };
            let floating = tree.floats(kid) || tree.out_of_flow(kid);
            let mut avail = kid_avail;
            if !floating
                && let Some((caps, horizontal)) = limits
                && let Some(cap) = caps.get(flow)
            {
                if horizontal {
                    avail.w = avail.w.min(*cap);
                } else {
                    avail.h = avail.h.min(*cap);
                }
            }
            let (res, c) = self.measure_node(tree, kid, Constraints::loose(avail));
            *count += c;
            // Remember the measured size; placement only assigns origins.
            if let Some(n) = tree.get_mut(kid) {
                n.rect.size = res.size;
                n.min_content = res.min_content;
                n.max_content = res.max_content;
            }
            // A floating child contributes nothing to its parent's content size: it floats
            // above the flow instead of taking a slot; out-of-flow is sized by its containing
            if floating {
                continue;
            }
            if flow > 0 {
                agg.row.main += gap;
                agg.col.main += gap;
            }
            flow += 1;
            // Margins sit outside the border box, so they enlarge the parent's content
            let m = flow_margin(tree, kid, false);
            // Only a switchable row has a second reading; any other parent would be reading
            // its own margins twice under another name.
            let m_alt = if switchable {
                flow_margin(tree, kid, true)
            } else {
                m
            };
            agg.widest = agg.widest.max(res.size.w + m.left + m.right);
            agg.tallest = agg.tallest.max(res.size.h + m.top + m.bottom);
            // The main axis counts the size placement will give the child, not only the
            // measured one, or a declared comfortable size overflows its parent.
            let kid_sc = tree
                .get(kid)
                .map(|n| layout_size_class(n.kind, n.size_class))
                .unwrap_or(SizeClass::fixed(0.0, 0.0));
            let axes = kid_sc.axes();
            let placed = placed_along(
                axes.0,
                kid_sc.width.hint,
                res.min_content.w,
                kid_avail.w,
                kid_sc.grow_x,
                res.size.w,
            );
            let placed_v = placed_along(
                axes.1,
                kid_sc.height.hint,
                res.min_content.h,
                kid_avail.h,
                kid_sc.grow_y,
                res.size.h,
            );
            agg.row.main += placed + m.left + m.right;
            agg.row.cross = agg.row.cross.max(res.size.h + m.top + m.bottom);
            agg.col.main += placed_v + m_alt.top + m_alt.bottom;
            agg.col.cross = agg.col.cross.max(res.size.w + m_alt.left + m_alt.right);
            // A baseline a child does not report is synthesised from its bottom margin
            // edge, so a box beside text still has a place on the line.
            let own = tree.get(kid).map(|n| n.baseline).unwrap_or(0.0);
            let own = if own > 0.0 {
                own
            } else {
                res.size.h + m.bottom
            };
            agg.above = agg.above.max(m.top + own);
            agg.below = agg.below.max(res.size.h + m.bottom - own);
            // An explicit size wins over content; the content floor bounds both ends.
            let (wlo0, whi0) = contribution(
                axes.0,
                res.min_content.w,
                res.max_content.w.max(hint_floor(kid_sc.width)),
                kid_avail.w,
            );
            let (hlo0, hhi0) = contribution(
                axes.1,
                res.min_content.h,
                res.max_content.h.max(hint_floor(kid_sc.height)),
                kid_avail.h,
            );
            let (wlo, whi) = (wlo0 + m.left + m.right, whi0 + m.left + m.right);
            let (hlo, hhi) = (hlo0 + m.top + m.bottom, hhi0 + m.top + m.bottom);
            let (wlo_alt, whi_alt) = (
                wlo0 + m_alt.left + m_alt.right,
                whi0 + m_alt.left + m_alt.right,
            );
            let (hlo_alt, hhi_alt) = (
                hlo0 + m_alt.top + m_alt.bottom,
                hhi0 + m_alt.top + m_alt.bottom,
            );
            agg.axis_min[0] = agg.axis_min[0].max(wlo);
            agg.axis_min[1] = agg.axis_min[1].max(hlo);
            agg.axis_max[0] = agg.axis_max[0].max(whi);
            agg.axis_max[1] = agg.axis_max[1].max(hhi);
            agg.row.main_min += wlo;
            agg.row.main_max += whi;
            agg.row.cross_min = agg.row.cross_min.max(hlo);
            agg.row.cross_max = agg.row.cross_max.max(hhi);
            agg.col.main_min += hlo_alt;
            agg.col.main_max += hhi_alt;
            agg.col.cross_min = agg.col.cross_min.max(wlo_alt);
            agg.col.cross_max = agg.col.cross_max.max(whi_alt);
        }
        agg.flow = flow;
        agg
    }

    /// Bottom-up: compute a node's demanded size from its children, storing each
    /// child's own size in `child.rect.size` so the top-down pass only has to place.
    fn measure_node_uncached(
        &mut self,
        tree: &mut Tree,
        nid: NodeId,
        constraints: Constraints,
    ) -> (crate::layout::MeasureResult, usize) {
        let Some(node) = tree.get(nid) else {
            return (crate::layout::MeasureResult::default(), 0);
        };
        let available = constraints.max;
        // I2: a parent may only tighten, so the constraint it hands down must be well formed.
        debug_assert!(
            constraints.min.w <= constraints.max.w + I1_EPS
                && constraints.min.h <= constraints.max.h + I1_EPS,
            "I2 violated at {nid:?}: {constraints:?}"
        );
        let node_id = node.id;
        let kind = node.kind;
        let data = node.data.clone();
        let sc = layout_size_class(kind, node.size_class);
        let box_model = node.box_model;
        let sizes = sc.axes();
        // Declared bounds resolved against the offered space, read once and used everywhere.
        let decl_max = Size::new(
            max_len(sizes.0.max, available.w),
            max_len(sizes.1.max, available.h),
        );
        let decl_min = Size::new(
            min_len(sizes.0.min, available.w),
            min_len(sizes.1.min, available.h),
        );
        let mut count = 1usize;

        // A grid has to measure its children before it can size a track, so it takes its
        // own pass; every other kind answers inside the match below.
        let grid_out = (kind == NodeKind::Grid && data.params().is_some())
            .then(|| self.measure_grid(tree, nid, &data, available, &mut count));
        // First baseline: only a text or math leaf has one until the inline pass places runs.
        let mut baseline = 0.0f32;
        // Which axis a flow container settled on; placement must not re-decide it.
        let mut flow_axis_vertical = false;
        let (used, min_content, max_content, content) = match grid_out {
            Some(out) => out,
            None => match &data {
                NodeData::Text { block, nowrap, .. } => {
                    // The box's padding belongs to the box, not to the text: a line only gets the
                    // room left inside it, and the painter starts at the content origin.
                    let pad = box_model.padding;
                    let pad_w = pad.left + pad.right;
                    let pad_h = pad.top + pad.bottom;
                    // Wrap at the width this node can actually occupy; the painter reuses it.
                    // A `nowrap` node ignores the room it was given: a single-line editor scrolls.
                    let inner = (available.w.min(decl_max.w) - pad_w).max(0.0);
                    let wrap = if *nowrap { f32::INFINITY } else { inner };
                    // A holder's measured size is the box its atom reserves, so it is measured
                    // before the line breaks. The atom keeps the answer in the node the painter
                    // reads, which also makes the size part of the measurer's cache key.
                    let mut held = false;
                    for (index, atom) in block.atoms.iter().enumerate() {
                        let Some(id) = atom.holder else { continue };
                        let Some(child) = tree.find_by_id(id) else {
                            continue;
                        };
                        if tree.get(child).map(|n| n.parent) != Some(Some(nid)) {
                            continue;
                        }
                        let (res, c) = self.measure_node(
                            tree,
                            child,
                            Constraints::loose(Size::new(wrap, f32::INFINITY)),
                        );
                        count += c;
                        held = true;
                        if let Some(n) = tree.get_mut(nid)
                            && let NodeData::Text { block, .. } = &mut n.data
                            && let Some(atom) = std::sync::Arc::make_mut(block).atoms.get_mut(index)
                        {
                            atom.size = res.size;
                            // A container reports no baseline; CSS gives it its bottom edge.
                            atom.baseline = if res.baseline > 0.0 {
                                res.baseline
                            } else {
                                res.size.h
                            };
                        }
                    }
                    // Read the stamped block back, so the wrap sees what the painter will.
                    let block = match tree.get(nid).map(|n| &n.data) {
                        Some(NodeData::Text { block, .. }) if held => block,
                        _ => block,
                    };
                    let layout = self.measure.layout(block, wrap);
                    let m = layout.size();
                    // A one-line node still reports the box it declared, or a wide line would be
                    // centred on its narrow parent and start off-screen.
                    let m = if *nowrap {
                        Size::new(m.w.min((sc.width.hint - pad_w).max(0.0)).min(inner), m.h)
                    } else {
                        m
                    };
                    let (min_c, max_c) = self.measure.intrinsic(block);
                    // A nowrap line cannot be broken: its floor is the capped line itself.
                    let (mut min_c, mut max_c) = if *nowrap {
                        (Size::new(m.w, min_c.h), Size::new(m.w, max_c.h))
                    } else {
                        (min_c, max_c)
                    };
                    // The declared height hint is the author's line box; glyphs sit inside it.
                    let line_hint = preferred(sc.height.hint);
                    // The block is as tall as the lines it wrapped into: min/max-content heights
                    // belong to the axis that picks the breaks, not to the height axis.
                    let height = m.h.max(line_hint);
                    let content = Size::new(m.w + pad_w, height + pad_h);
                    min_c.w += pad_w;
                    max_c.w += pad_w;
                    min_c.h = content.h;
                    max_c.h = content.h;
                    // Hand the painter the very lines layout sized, instead of a second wrap.
                    baseline = pad.top + layout.baseline;
                    let used = Size::new(
                        used_axis(
                            sizes.0.size,
                            available.w,
                            min_c.w,
                            max_c.w,
                            content.w,
                            decl_min.w,
                            decl_max.w,
                        ),
                        used_axis(
                            sizes.1.size,
                            available.h,
                            min_c.h,
                            max_c.h,
                            content.h,
                            decl_min.h,
                            decl_max.h,
                        ),
                    );
                    if let Some(node) = tree.get_mut(nid) {
                        // A nowrap line never breaks, but it still has a box to sit in: `wrap`
                        // is infinite because the line is unbreakable, not because the box is.
                        // Alignment reads the content width, so an end-aligned marker ends at
                        // the cell's far edge instead of starting at its near one.
                        node.wrap_width = if *nowrap {
                            (used.w - pad_w).max(0.0)
                        } else {
                            wrap
                        };
                        node.inline = Some(layout);
                    }
                    (used, min_c, max_c, content)
                }
                NodeData::Math { formula, .. } => {
                    // A formula cannot reflow: its width is what it is, so min and max coincide
                    // and a narrow parent does not ask it to wrap.
                    let pad = box_model.padding;
                    let content = Size::new(
                        formula.width + pad.left + pad.right,
                        formula.height() + pad.top + pad.bottom,
                    );
                    let (min_c, max_c) = (content, content);
                    baseline = pad.top + formula.ascent;
                    let used = Size::new(
                        used_axis(
                            sizes.0.size,
                            available.w,
                            min_c.w,
                            max_c.w,
                            content.w,
                            decl_min.w,
                            decl_max.w,
                        ),
                        used_axis(
                            sizes.1.size,
                            available.h,
                            min_c.h,
                            max_c.h,
                            content.h,
                            decl_min.h,
                            decl_max.h,
                        ),
                    );
                    (used, min_c, max_c, content)
                }
                NodeData::Container(_) => {
                    let params = data.params().cloned().unwrap_or_default();
                    let pad = box_model.padding;
                    // A capped box cannot hand its children more room than it will itself take;
                    // otherwise a `fixed(380)` column measures its text against the whole window.
                    let own_avail =
                        Size::new(available.w.min(decl_max.w), available.h.min(decl_max.h));
                    let inner_avail = Size::new(
                        (own_avail.w - pad.left - pad.right).max(0.0),
                        (own_avail.h - pad.top - pad.bottom).max(0.0),
                    );
                    // A scrolling axis hands children no bound at all: the content is measured
                    // at its own size and the viewport scrolls over it.
                    let flows = matches!(kind, NodeKind::Row | NodeKind::Column);
                    let kid_avail = if flows && main_scrolls(kind == NodeKind::Row, params.clip) {
                        if kind == NodeKind::Row {
                            Size::new(f32::INFINITY, inner_avail.h)
                        } else {
                            Size::new(inner_avail.w, f32::INFINITY)
                        }
                    } else {
                        inner_avail
                    };
                    // Ask children first. The pass mutates the tree it walks, so it walks by
                    // index rather than over a copied child list.
                    let child_count = tree.children_of(Some(nid)).len();
                    // A row that may stack keeps both readings at once: one measure pass feeds
                    // the row and the column interpretation, so the switch costs no second pass.
                    let switchable = kind == NodeKind::Row && params.switch_axis.is_some();
                    let mut agg = self.measure_flow_children(
                        tree,
                        nid,
                        child_count,
                        switchable,
                        params.gap,
                        kid_avail,
                        None,
                        &mut count,
                    );
                    let decide = |agg: &FlowAgg| {
                        let stacked = kind == NodeKind::Row
                            && params.switch_axis.is_some_and(|sw| {
                                agg.flow == 1
                                    || agg.row.main > inner_avail.w + 0.01
                                    || sw.max_cross.is_some_and(|h| agg.row.cross > h)
                            });
                        (stacked, kind == NodeKind::Row && !stacked)
                    };
                    // A line can hand a child less than it measured at. Its own content then
                    // has to reflow at the size it will actually occupy, or a text keeps the
                    let (stacked, mut horizontal) = decide(&agg);
                    if matches!(kind, NodeKind::Row | NodeKind::Column)
                        && !main_scrolls(horizontal, params.clip)
                        && let Some(limits) = squeezed_limits(
                            tree,
                            nid,
                            child_count,
                            horizontal,
                            inner_avail,
                            params.gap,
                            stacked,
                        )
                    {
                        agg = self.measure_flow_children(
                            tree,
                            nid,
                            child_count,
                            switchable,
                            params.gap,
                            kid_avail,
                            Some((&limits, horizontal)),
                            &mut count,
                        );
                        horizontal = decide(&agg).1;
                    }
                    // A baseline-aligned row is as tall as both sides of the shared baseline:
                    // the tallest child's box alone would crop the deepest descender.
                    if params.cross_baseline && horizontal {
                        // The line needs both sides of the baseline, so it is the line's
                        // min-content height too: a parent that squeezes the row must not get
                        // the tallest child's box back and crop the descenders.
                        let extent = (agg.above + agg.below).max(agg.row.cross_min);
                        agg.row.cross = extent;
                        agg.row.cross_min = extent;
                        agg.row.cross_max = agg.row.cross_max.max(extent);
                    }
                    let FlowAgg {
                        mut row,
                        mut col,
                        widest,
                        tallest,
                        axis_min,
                        axis_max,
                        flow,
                        ..
                    } = agg;
                    let gaps = params.gap * (flow.saturating_sub(1)) as f32;
                    // Main-axis intrinsics include the gaps between flow children.
                    row.main_min += gaps;
                    row.main_max += gaps;
                    col.main_min += gaps;
                    col.main_max += gaps;
                    flow_axis_vertical =
                        matches!(kind, NodeKind::Row | NodeKind::Column) && !horizontal;
                    let sum = if horizontal { row } else { col };
                    let content = match kind {
                        NodeKind::Row | NodeKind::Column => {
                            if horizontal {
                                Size::new(sum.main, sum.cross)
                            } else {
                                Size::new(sum.cross, sum.main)
                            }
                        }
                        NodeKind::Stack => Size::new(widest, tallest),
                        _ => Size::new(sum.cross, sum.main),
                    };
                    let pad_w = pad.left + pad.right;
                    let pad_h = pad.top + pad.bottom;
                    // The content size is the box's own, so the padding around the children is
                    // part of it; leaving it out sizes a hugging box narrower than its content.
                    let content = Size::new(content.w + pad_w, content.h + pad_h);
                    let (min_c, max_c) = match kind {
                        NodeKind::Row | NodeKind::Column => {
                            if horizontal {
                                (
                                    Size::new(sum.main_min + pad_w, sum.cross_min + pad_h),
                                    Size::new(sum.main_max + pad_w, sum.cross_max + pad_h),
                                )
                            } else {
                                (
                                    Size::new(sum.cross_min + pad_w, sum.main_min + pad_h),
                                    Size::new(sum.cross_max + pad_w, sum.main_max + pad_h),
                                )
                            }
                        }
                        NodeKind::Stack => (
                            Size::new(axis_min[0] + pad_w, axis_min[1] + pad_h),
                            Size::new(axis_max[0] + pad_w, axis_max[1] + pad_h),
                        ),
                        _ => (
                            Size::new(axis_min[0] + pad_w, sum.main_min + pad_h),
                            Size::new(axis_max[0] + pad_w, sum.main_max + pad_h),
                        ),
                    };
                    // A scroll viewport never demands its content on the scrolling axis: the
                    // content overflows by design and the app scrolls it.
                    let (min_c, max_c) = if main_clips(horizontal, params.clip) {
                        if horizontal {
                            (Size::new(0.0, min_c.h), max_c)
                        } else {
                            (Size::new(min_c.w, 0.0), max_c)
                        }
                    } else {
                        (min_c, max_c)
                    };
                    let used = Size::new(
                        used_axis(
                            sizes.0.size,
                            own_avail.w,
                            min_c.w,
                            max_c.w,
                            content.w,
                            decl_min.w,
                            decl_max.w,
                        ),
                        used_axis(
                            sizes.1.size,
                            own_avail.h,
                            min_c.h,
                            max_c.h,
                            content.h,
                            decl_min.h,
                            decl_max.h,
                        ),
                    );
                    (used, min_c, max_c, content)
                }
            },
        };

        // Store this node's answer; the placement pass reads sizes and intrinsics here.
        if let Some(n) = tree.get_mut(nid) {
            n.rect.size = used;
            n.min_content = min_content;
            n.max_content = max_content;
            n.baseline = baseline;
            // Placement reads the axis this measure settled on, not the declared kind.
            n.flow_vertical = flow_axis_vertical;
        }
        // I1: the answer stays between the content floor and the declared ceiling.
        let (floor_w, ceiling_w) = content_bounds(min_content.w, decl_min.w, decl_max.w);
        let (floor_h, ceiling_h) = content_bounds(min_content.h, decl_min.h, decl_max.h);
        let floor = Size::new(floor_w, floor_h);
        let ceiling = Size::new(ceiling_w, ceiling_h);
        debug_assert!(
            used.w + I1_EPS >= floor.w
                && used.h + I1_EPS >= floor.h
                && used.w <= ceiling.w + I1_EPS
                && used.h <= ceiling.h + I1_EPS,
            "I1 violated at {node_id:?}: used {used:?}, floor {floor:?}, ceiling {ceiling:?}, {constraints:?}"
        );
        (
            crate::layout::MeasureResult {
                size: used,
                min_content,
                max_content,
                content,
                baseline,
            },
            count,
        )
    }

    /// Put each held node at the rect its atom reserved inside the placed text block.
    ///
    /// The origin comes from `InlineLayout::line_origin`, the same call the painter uses, so
    /// a widget lands exactly where the glyphs and the caret think its line starts.
    fn place_inline_holders(
        &mut self,
        tree: &mut Tree,
        nid: NodeId,
        rect: Rect,
        box_model: crate::layout::BoxModel,
        stats: &mut LayoutStats,
    ) {
        let Some(node) = tree.get(nid) else { return };
        let Some(layout) = node.inline.clone() else {
            return;
        };
        let NodeData::Text { block, .. } = &node.data else {
            return;
        };
        let (wrap_width, edges, align) = (node.wrap_width, block.edges(), block.align);
        let origin = crate::geom::Point::new(
            rect.origin.x + box_model.padding.left,
            rect.origin.y + box_model.padding.top,
        );
        // Collected first: placing a child borrows the tree mutably.
        let mut jobs = Vec::new();
        for (index, line) in layout.lines.iter().enumerate() {
            if line.atoms.is_empty() {
                continue;
            }
            let line_origin =
                layout.line_origin(index, origin, rect.size.h, wrap_width, edges, align);
            for atom in &line.atoms {
                // Identity stays on the block, not on the layout: the measurer shares one
                // layout between nodes whose text and geometry match, and those nodes hold
                // different children.
                let Some(id) = block.atom_at(atom.at).and_then(|atom| atom.holder) else {
                    continue;
                };
                if let Some(child) = tree.find_by_id(id) {
                    jobs.push((child, atom.rect(line_origin)));
                }
            }
        }
        for (child, rect) in jobs {
            self.place_node(tree, child, rect, stats);
        }
    }

    /// Top-down: assign origins and recurse. Sizes come from the measure pass.
    fn place_node(&mut self, tree: &mut Tree, nid: NodeId, rect: Rect, stats: &mut LayoutStats) {
        stats.measured += 1;
        // Placement runs for every node on every frame; only the container parameters are
        // read, so a text node's runs are not copied here.
        let (kind, params, box_model) = match tree.get(nid) {
            Some(n) => (n.kind, n.data.params().cloned(), n.box_model),
            None => return,
        };
        // A modal covers its target, not the place the tree happened to put it: the target
        // is an ancestor, already placed, so this needs no rect from outside and follows
        // every resize in the same frame.
        let rect = match params.as_ref().and_then(|p| p.modal) {
            Some(target) => modal_target_rect(tree, nid, target).unwrap_or(rect),
            None => rect,
        };
        if let Some(n) = tree.get_mut(nid) {
            n.rect.origin = rect.origin;
            n.rect.size = rect.size;
        }

        // A text leaf places the nodes its atoms hold. A leaf has no flow items, so nothing
        // else in this pass would ever give them a rect.
        if kind == NodeKind::Text {
            self.place_inline_holders(tree, nid, rect, box_model, stats);
            return;
        }

        if let Some(params) = params {
            let pad: Insets = box_model.padding;
            let content_w = (rect.size.w - pad.left - pad.right).max(0.0);
            let content_h = (rect.size.h - pad.top - pad.bottom).max(0.0);
            // The wheel may not move content past its own end; the extent is the one the last
            // placement wrote, so the clamp only bites at the boundary.
            let extent = tree.get(nid).and_then(|n| n.scroll_extent);
            // An elastic frame carries the engine's own overscroll and must reach the paint;
            // clamping it would make the rubber band and the spring return invisibly. A
            // container that has never been placed has no extent to clamp against yet.
            let clamp = |offset: f32, extent: Option<f32>, view: f32| match extent {
                Some(extent) if !params.scroll_elastic => {
                    offset.clamp(0.0, (extent - view).max(0.0))
                }
                _ => offset,
            };
            // Scrolling shifts the content box; the container's own box stays put, so
            // children scrolled outside are clipped by the mask it already has.
            let content_origin = crate::geom::Point::new(
                rect.origin.x + pad.left - clamp(params.scroll_x, extent.map(|e| e.w), content_w),
                rect.origin.y + pad.top - clamp(params.scroll_y, extent.map(|e| e.h), content_h),
            );
            // Placement mutates the tree it walks, so it walks by index.
            let child_count = tree.children_of(Some(nid)).len();
            // The measure pass settled the axis; a switchable row may have stacked.
            // Read once: placement asks per child, and a row that stacked must answer the
            // same way for the line and for every margin under it.
            let stacked = stacked_axis(tree, nid);
            let horizontal = kind == NodeKind::Row && !stacked;
            if kind == NodeKind::Grid {
                self.place_grid(
                    tree,
                    nid,
                    &params,
                    content_origin,
                    Size::new(content_w, content_h),
                    stats,
                );
                return;
            }
            // A stack gives every child the content origin and its own measured size, so
            // layers overlap instead of taking turns down the box.
            if kind == NodeKind::Stack {
                let (align_x, align_y) = params.stack_align;
                // A stack has no line to measure, so its content extent is the union of the
                // boxes it lays down. Skipping it left a scrolling stack with no extent at
                // all: no wheel travel, no bar, and `bring_into_view` could never move it.
                let mut bounds = Size::ZERO;
                for index in 0..child_count {
                    let Some(kid) = tree.child_at(Some(nid), index) else {
                        continue;
                    };
                    // An out-of-flow layer is placed by the OOF pass, not stacked here.
                    if tree.out_of_flow(kid) {
                        continue;
                    }
                    // A stack has no line to grow in, so `fill` here takes the content box
                    // itself, the way an absolutely positioned box fills its container.
                    let size = match tree.get(kid) {
                        Some(node) => {
                            let declared = node.declared_size;
                            let mut size = node.rect.size;
                            if declared.width.sizing == crate::layout::Sizing::Stretch {
                                let bounds = declared.width.sizes();
                                size.w = content_w
                                    .max(min_len(bounds.min, content_w))
                                    .min(max_len(bounds.max, content_w));
                            }
                            if declared.height.sizing == crate::layout::Sizing::Stretch {
                                let bounds = declared.height.sizes();
                                size.h = content_h
                                    .max(min_len(bounds.min, content_h))
                                    .min(max_len(bounds.max, content_h));
                            }
                            size
                        }
                        None => Size::ZERO,
                    };
                    // A stack has no main axis, so both axes take the same rule.
                    let origin = crate::geom::Point::new(
                        content_origin.x + align_offset(align_x, content_w - size.w),
                        content_origin.y + align_offset(align_y, content_h - size.h),
                    );
                    let rect = Rect::from_origin_size(origin, size);
                    bounds.w = bounds.w.max(rect.max_x() - content_origin.x);
                    bounds.h = bounds.h.max(rect.max_y() - content_origin.y);
                    self.place_node(tree, kid, rect, stats);
                }
                Self::settle_scroll_extent(tree, nid, bounds);
                return;
            }

            // Resolve the line with the flex algorithm: basis, grow and scaled shrink,
            // freezing an item once it reaches its own bound.
            // Floating children take no slot, so they take no gap either.
            let flow: Vec<NodeId> = (0..child_count)
                .filter_map(|index| tree.child_at(Some(nid), index))
                .filter(|kid| !tree.floats(*kid) && !tree.out_of_flow(*kid))
                .collect();
            let gap_total = params.gap * (flow.len().saturating_sub(1)) as f32;
            let box_main = if horizontal {
                content_w
            } else {
                (rect.size.h - pad.top - pad.bottom).max(0.0)
            };
            let (items, contents) = line_items(tree, &flow, horizontal, box_main);
            // A scrolling container is sized by its content: hand the line the sum of what
            // its items want, so nothing is squeezed and nothing has spare space to grow
            // into. A zero-basis grower wants its content, not its basis.
            let margins_total = flow_margins_total(tree, &flow, horizontal, stacked);
            let main_avail = if main_scrolls(horizontal, params.clip) {
                contents.iter().sum::<f32>() + gap_total + margins_total
            } else {
                (box_main - margins_total).max(0.0)
            };
            let mains = crate::flex::solve_line(&items, main_avail, params.gap);
            let mut next_main = 0usize;
            // The line's baseline is the highest point any child's own sits at; every child
            // below it moves up to meet it. Measured before placement so the first child
            // already knows where the line ends up.
            let baseline_line = if params.cross_baseline && horizontal {
                flow.iter()
                    .map(|kid| {
                        let m = flow_margin(tree, *kid, stacked);
                        let size = tree.get(*kid).map(|n| n.rect.size).unwrap_or(Size::ZERO);
                        let own = tree.get(*kid).map(|n| n.baseline).unwrap_or(0.0);
                        let own = if own > 0.0 { own } else { size.h + m.bottom };
                        m.top + own
                    })
                    .fold(0.0, f32::max)
            } else {
                0.0
            };

            // The content extent is what a scrollbar needs; it is not the viewport.
            let mut main_end = 0.0_f32;
            let mut cross_max = 0.0_f32;
            let mut cursor = 0.0_f32;
            for i in 0..child_count {
                let Some(kid) = tree.child_at(Some(nid), i) else {
                    continue;
                };
                // An out-of-flow child is placed against its containing block afterwards.
                if tree.out_of_flow(kid) {
                    continue;
                }
                // A floating child keeps its own measured size and sits at the content
                // origin; it neither consumes a slot nor receives spare space.
                if tree.floats(kid) {
                    let size = tree.get(kid).map(|n| n.rect.size).unwrap_or(Size::ZERO);
                    let rect = Rect::from_origin_size(content_origin, size);
                    self.place_node(tree, kid, rect, stats);
                    continue;
                }
                let kid_size = tree.get(kid).map(|n| n.rect.size).unwrap_or(Size::ZERO);
                // In-flow margins offset the child and consume the line's space.
                let m = flow_margin(tree, kid, stacked);
                let (m_main_start, m_main_end) = if horizontal {
                    (m.left, m.right)
                } else {
                    (m.top, m.bottom)
                };
                let (m_cross_start, m_cross_end) = if horizontal {
                    (m.top, m.bottom)
                } else {
                    (m.left, m.right)
                };
                cursor += m_main_start;
                // `mains` no longer has one entry per kid: overlays were filtered out.
                let slot = next_main;
                next_main += 1;
                let main = mains.get(slot).copied().unwrap_or(if horizontal {
                    kid_size.w
                } else {
                    kid_size.h
                });
                // Cross axis: `cross_align` is the fraction of the spare cross space the
                // child takes (0 = hug, 1 = stretch to fill); a deficit shrinks it too.
                let cross_avail = (if horizontal {
                    (rect.size.h - pad.top - pad.bottom).max(0.0)
                } else {
                    content_w
                } - m_cross_start
                    - m_cross_end)
                    .max(0.0);
                let cross_measured = if horizontal { kid_size.h } else { kid_size.w };
                // Stretching is bounded by the child's own cross-axis bounds: `fixed` and a
                // declared max are hard caps, an unbounded `hug`/`fill` may stretch.
                // The same resolution measure used: a text's height hint is a line-box floor,
                // and reading the raw class here would cap it at one line.
                let (cross_sizes, cross_min_c) = tree
                    .get(kid)
                    .map(|n| {
                        let axes = layout_size_class(n.kind, n.size_class).axes();
                        (
                            if horizontal { axes.1 } else { axes.0 },
                            if horizontal {
                                n.min_content.h
                            } else {
                                n.min_content.w
                            },
                        )
                    })
                    .unwrap_or((
                        crate::layout::AxisSizes {
                            size: crate::layout::Sizing::Fit,
                            min: crate::layout::Len::Px(0.0),
                            max: crate::layout::Len::Auto,
                        },
                        0.0,
                    ));
                let (floor, ceiling) = content_bounds(
                    cross_min_c,
                    min_len(cross_sizes.min, cross_avail),
                    max_len(cross_sizes.max, cross_avail),
                );
                // An item's own alignment wins over the line's: CSS `align-self` is the
                // cross axis, whichever way the container runs.
                let self_align = tree.get(kid).and_then(|n| n.grid.align);
                // A baseline-aligned child keeps its own cross size and moves so that its
                // baseline meets the line's, so there is no slack left for `cross_pos`.
                let (cross, slack) = if let Some(align) = self_align {
                    let own = cross_measured.max(floor).min(ceiling);
                    let spare = (cross_avail - own - m_cross_start - m_cross_end).max(0.0);
                    let cross = match align {
                        Align::Stretch => cross_avail.max(floor).min(ceiling),
                        _ => own,
                    };
                    let slack = match align {
                        Align::Stretch | Align::Start => m_cross_start,
                        Align::Center => m_cross_start + spare * 0.5,
                        Align::End => m_cross_start + spare,
                    };
                    (cross, slack)
                } else if params.cross_baseline && horizontal {
                    let own = tree.get(kid).map(|n| n.baseline).unwrap_or(0.0);
                    let own = if own > 0.0 {
                        own
                    } else {
                        cross_measured + m_cross_end
                    };
                    (
                        cross_measured.max(floor).min(ceiling),
                        baseline_line - m_cross_start - own,
                    )
                } else {
                    let stretched =
                        cross_measured + (cross_avail - cross_measured) * params.cross_align;
                    let cross = stretched.max(floor).min(ceiling);
                    // `cross_pos` places what the child did not take: 0 hugs, 0.5 centres.
                    let slack = (cross_avail - cross).max(0.0) * params.cross_pos + m_cross_start;
                    (cross, slack)
                };
                let (w, h) = if horizontal {
                    (main, cross)
                } else {
                    (cross, main)
                };
                let _ = content_w;
                let origin = if horizontal {
                    crate::geom::Point::new(content_origin.x + cursor, content_origin.y + slack)
                } else {
                    crate::geom::Point::new(content_origin.x + slack, content_origin.y + cursor)
                };
                let child_rect = Rect::from_origin_size(origin, Size::new(w, h));
                self.place_node(tree, kid, child_rect, stats);
                cursor += (if horizontal { w } else { h }) + m_main_end;
                main_end = cursor;
                cross_max =
                    cross_max.max((if horizontal { h } else { w }) + m_cross_start + m_cross_end);
                if i + 1 < child_count {
                    cursor += params.gap;
                }
            }
            Self::settle_scroll_extent(
                tree,
                nid,
                if horizontal {
                    Size::new(main_end, cross_max)
                } else {
                    Size::new(cross_max, main_end)
                },
            );
        }
    }

    /// Write the content extent this placement produced, and pull a shrunken content back
    /// inside it.
    ///
    /// The content origin came from the previous frame's extent, because the current one is
    /// only known once the children are placed. A list that just got shorter was therefore
    /// laid out shifted too far by an offset that no longer exists: one frame showed empty
    /// space, then the next clamp put it right. Flutter closes the same window with
    /// `applyContentDimensions` and a pixel correction; this is that correction.
    fn settle_scroll_extent(tree: &mut Tree, nid: NodeId, extent: Size) {
        // Copied out of the node: the writes below mutate the tree this reads.
        let state = tree.get(nid).and_then(|node| {
            let params = node.data.params()?;
            let pad = node.box_model.padding;
            Some((
                node.scroll_extent,
                Size::new(
                    (node.rect.size.w - pad.left - pad.right).max(0.0),
                    (node.rect.size.h - pad.top - pad.bottom).max(0.0),
                ),
                params.scroll_x,
                params.scroll_y,
                params.scroll_elastic,
            ))
        });
        let Some((previous, content, scroll_x, scroll_y, elastic)) = state else {
            if let Some(n) = tree.get_mut(nid) {
                n.scroll_extent = Some(extent);
            }
            return;
        };
        let length = |offset: f32, extent: Option<f32>, view: f32| match extent {
            Some(extent) => offset.clamp(0.0, (extent - view).max(0.0)),
            None => offset,
        };
        let used = crate::geom::Point::new(
            length(scroll_x, previous.map(|e| e.w), content.w),
            length(scroll_y, previous.map(|e| e.h), content.h),
        );
        let want = crate::geom::Point::new(
            length(scroll_x, Some(extent.w), content.w),
            length(scroll_y, Some(extent.h), content.h),
        );
        if let Some(n) = tree.get_mut(nid) {
            n.scroll_extent = Some(extent);
            if !elastic && let Some(params) = n.data.params_mut() {
                params.scroll_x = want.x;
                params.scroll_y = want.y;
            }
        }
        // An elastic frame is the engine stretching the container itself: it must reach the
        // paint unclamped, or the rubber band and the spring return are invisible.
        let shift = crate::geom::Point::new(used.x - want.x, used.y - want.y);
        if !elastic && (shift.x.abs() > 0.01 || shift.y.abs() > 0.01) {
            shift_subtree(tree, nid, shift);
        }
    }

    /// Place every out-of-flow node, parents before children, after the flow is down.
    fn place_out_of_flow(&mut self, tree: &mut Tree, nid: NodeId, stats: &mut LayoutStats) {
        let count = tree.children_of(Some(nid)).len();
        for i in 0..count {
            let Some(kid) = tree.child_at(Some(nid), i) else {
                continue;
            };
            if tree.out_of_flow(kid) {
                self.place_out_of_flow_node(tree, kid, stats);
            }
            // A nested OOF node hangs off its own containing block, so it comes after.
            self.place_out_of_flow(tree, kid, stats);
        }
    }

    /// Resolve one out-of-flow node against its containing block, then place its subtree.
    fn place_out_of_flow_node(&mut self, tree: &mut Tree, nid: NodeId, stats: &mut LayoutStats) {
        use crate::layout::Position;
        let Some(node) = tree.get(nid) else {
            return;
        };
        let placement = node.placement;
        let box_model = node.box_model;
        let size_class = node.size_class;
        let grid = node.grid;
        let parent = node.parent;
        // An overlay answers to its anchor, not to a containing block or to insets.
        if let Some(overlay) = placement.overlay {
            self.place_overlay(tree, nid, overlay, stats);
            return;
        }
        // Fixed pins to the initial containing block: the root's border box.
        let cb = match placement.position {
            Position::Fixed => tree
                .root()
                .and_then(|r| tree.get(r))
                .map(|n| n.rect)
                .unwrap_or(Rect::ZERO),
            _ => containing_block(tree, nid),
        };
        // The static position is the parent's content origin, where the node would have sat.
        let static_origin = parent
            .and_then(|p| tree.get(p))
            .map(|p| {
                let pad = p.box_model.padding;
                let (sx, sy) = p
                    .data
                    .params()
                    .map(|q| (q.scroll_x, q.scroll_y))
                    .unwrap_or((0.0, 0.0));
                crate::geom::Point::new(
                    p.rect.origin.x + pad.left - sx,
                    p.rect.origin.y + pad.top - sy,
                )
            })
            .unwrap_or(cb.origin);
        // Percent insets were declared against this same containing block.
        let inset = placement.inset.resolve(cb.size);
        let margin = box_model.margin;
        // Auto margins are written as non-finite values; only an OOF node reads them.
        let margin_left = if margin.left.is_finite() {
            margin.left
        } else {
            crate::layout::AUTO
        };
        let margin_right = if margin.right.is_finite() {
            margin.right
        } else {
            crate::layout::AUTO
        };
        let margin_top = if margin.top.is_finite() {
            margin.top
        } else {
            crate::layout::AUTO
        };
        let margin_bottom = if margin.bottom.is_finite() {
            margin.bottom
        } else {
            crate::layout::AUTO
        };
        // The inset-modified containing block bounds the measurement.
        let avail_w = inset_avail(cb.size.w, inset.left, inset.right);
        let avail_h = inset_avail(cb.size.h, inset.top, inset.bottom);
        let (res, c) =
            self.measure_node(tree, nid, Constraints::loose(Size::new(avail_w, avail_h)));
        stats.measured += c;
        if let Some(n) = tree.get_mut(nid) {
            n.min_content = res.min_content;
            n.max_content = res.max_content;
        }
        let width_auto = matches!(size_class.width.sizing, crate::layout::Sizing::Fit);
        let height_auto = matches!(size_class.height.sizing, crate::layout::Sizing::Fit);
        let justify = grid.justify.unwrap_or(crate::layout::Align::Stretch);
        let align = grid.align.unwrap_or(crate::layout::Align::Stretch);
        let mut measured = res.size;
        let (w, x_off) = solve_oof_axis(
            cb.size.w,
            inset.left,
            inset.right,
            width_auto,
            measured.w,
            margin_left,
            margin_right,
            justify,
        );
        // Text wraps to the width it is given, so a stretched width must be measured again.
        if width_auto && (w - measured.w).abs() > 0.01 {
            let (again, c) =
                self.measure_node(tree, nid, Constraints::loose(Size::new(w, avail_h)));
            stats.measured += c;
            measured = again.size;
            if let Some(n) = tree.get_mut(nid) {
                n.min_content = again.min_content;
                n.max_content = again.max_content;
            }
        }
        let (h, y_off) = solve_oof_axis(
            cb.size.h,
            inset.top,
            inset.bottom,
            height_auto,
            measured.h,
            margin_top,
            margin_bottom,
            align,
        );
        // An axis with no inset falls back to the static position.
        let x = x_off
            .map(|off| cb.origin.x + off)
            .unwrap_or(static_origin.x);
        let y = y_off
            .map(|off| cb.origin.y + off)
            .unwrap_or(static_origin.y);
        self.place_node(
            tree,
            nid,
            Rect::from_origin_size(crate::geom::Point::new(x, y), Size::new(w, h)),
            stats,
        );
    }

    /// Place an anchored overlay: measure against the viewport, unfold from the anchor,
    /// flip at an edge, then clamp inside the viewport.
    fn place_overlay(
        &mut self,
        tree: &mut Tree,
        nid: NodeId,
        overlay: crate::layout::OverlayPlacement,
        stats: &mut LayoutStats,
    ) {
        use crate::geom::Point;
        use crate::layout::{Anchor, OverlayAlign, PlatformPanel, Presentation, Side};
        // A panel gets a surface of its own only where the window has one. The request is
        // read against that capability once, here, so the fallback is not a second placement:
        // a panel with no surface is measured, placed, clipped and hit as its parent's own.
        let presentation = match overlay.presentation {
            Presentation::Platform | Presentation::Window if !tree.panel_surfaces() => {
                Presentation::Parent
            }
            presentation => presentation,
        };
        // An anchor may hang off any branch of the tree, and offsets are only folded into
        // the paint offset after this pass. So both sides are spoken in the surface's own
        // space here: the anchor's rect is lifted by the offsets it will carry, and the
        // panel's result is put back into the space its branch will add them to.
        let viewport = tree
            .root()
            .and_then(|root| {
                tree.get(root)
                    .map(|n| n.rect.translate(pending_offset(tree, root)))
            })
            .unwrap_or(Rect::ZERO);
        let anchor = match overlay.anchor {
            Anchor::Node(id) => tree.find_by_id(id).map(|a| {
                tree.get(a)
                    .map(|n| n.rect.translate(pending_offset(tree, a)))
                    .unwrap_or(Rect::ZERO)
            }),
            Anchor::Point(p) => Some(Rect::from_origin_size(p, Size::ZERO)),
        };
        // A missing anchor still gets a panel: it unfolds from the viewport's top-left.
        let anchor = anchor.unwrap_or(Rect::from_origin_size(viewport.origin, Size::ZERO));
        // A platform panel is measured as content: the surface it will live in belongs to
        // the platform, and a menu must be allowed to grow past a window smaller than it.
        let room = match presentation {
            Presentation::Parent => viewport.size,
            Presentation::Platform | Presentation::Window => {
                Size::new(f32::INFINITY, f32::INFINITY)
            }
        };
        let (res, c) = self.measure_node(
            tree,
            nid,
            Constraints::loose(Size::new(room.w, room.h)),
        );
        stats.measured += c;
        if let Some(n) = tree.get_mut(nid) {
            n.min_content = res.min_content;
            n.max_content = res.max_content;
        }
        let mut size = res.size;
        // A panel told to span its anchor is re-measured inside the anchor's box, so its
        // text wraps and its fill children stretch to that box, not to the viewport.
        let span_w = (overlay.match_anchor_width && anchor.size.w > 0.0)
            .then(|| anchor.size.w.max(overlay.min_width));
        let span_h = (overlay.match_anchor_height && anchor.size.h > 0.0).then_some(anchor.size.h);
        if span_w.is_some() || span_h.is_some() {
            let width = span_w.unwrap_or(size.w);
            // The other axis keeps its ceiling: a hugging panel may grow taller when its
            // text rewraps at the anchor's width.
            let height = span_h.unwrap_or(room.h);
            let (again, c) =
                self.measure_node(tree, nid, Constraints::loose(Size::new(width, height)));
            stats.measured += c;
            // A spanned axis takes the anchor's extent outright: the panel's own content
            // rarely asks for it, and a background has to reach the host's edge anyway.
            size = Size::new(width, span_h.unwrap_or(again.size.h));
        }
        let fits = |r: Rect| {
            r.min_x() >= viewport.min_x() - 0.01
                && r.min_y() >= viewport.min_y() - 0.01
                && r.max_x() <= viewport.max_x() + 0.01
                && r.max_y() <= viewport.max_y() + 0.01
        };
        let at = |side: Side| {
            let (mut x, mut y) = match side {
                Side::Below => (0.0, anchor.max_y() + overlay.offset),
                Side::Above => (0.0, anchor.min_y() - overlay.offset - size.h),
                Side::Right => (anchor.max_x() + overlay.offset, 0.0),
                Side::Left => (anchor.min_x() - overlay.offset - size.w, 0.0),
            };
            let horizontal = matches!(side, Side::Below | Side::Above);
            let (start, span, own) = if horizontal {
                (anchor.min_x(), anchor.size.w, size.w)
            } else {
                (anchor.min_y(), anchor.size.h, size.h)
            };
            let aligned = match overlay.align {
                OverlayAlign::Start => start,
                OverlayAlign::Center => start + (span - own) * 0.5,
                OverlayAlign::End => start + span - own,
            };
            if horizontal {
                x = aligned;
            } else {
                y = aligned;
            }
            Rect::from_origin_size(Point::new(x, y), size)
        };
        let mut rect = at(overlay.side);
        // A platform panel is placed against the output, not this viewport: a menu near the
        // edge of a small window must be allowed past it. And it is placed here for good:
        // the surface stands where this rect says, or the input its grab delivers outside
        // the panel would be mapped into places the tree does not name.
        if presentation != Presentation::Parent
            && let Some(bounds) = tree.output_bounds()
        {
            rect = crate::layout::place_anchored(
                anchor,
                size,
                overlay.side,
                overlay.align,
                overlay.offset,
                crate::layout::Fit::FLIP_AND_SLIDE,
                bounds,
            );
        }
        if presentation == Presentation::Parent {
            if overlay.flip && !fits(rect) {
                let opposite = match overlay.side {
                    Side::Below => Side::Above,
                    Side::Above => Side::Below,
                    Side::Left => Side::Right,
                    Side::Right => Side::Left,
                };
                let flipped = at(opposite);
                if fits(flipped) {
                    rect = flipped;
                }
            }
            // The viewport wins over the anchor: a panel taller than the window still
            // starts at its top edge, so its first line stays readable.
            rect.origin.x = rect.origin.x.clamp(
                viewport.min_x(),
                (viewport.max_x() - size.w).max(viewport.min_x()),
            );
            rect.origin.y = rect.origin.y.clamp(
                viewport.min_y(),
                (viewport.max_y() - size.h).max(viewport.min_y()),
            );
        }
        let own = pending_offset(tree, nid);
        self.place_node(
            tree,
            nid,
            Rect::from_origin_size(Point::new(rect.origin.x - own.x, rect.origin.y - own.y), size),
            stats,
        );
        if presentation != Presentation::Parent {
            let title = tree
                .get(nid)
                .and_then(|n| n.data.params())
                .and_then(|p| p.window_title.clone());
            tree.record_platform_panel(PlatformPanel {
                node: nid,
                anchor,
                rect,
                side: overlay.side,
                align: overlay.align,
                offset: overlay.offset,
                window: presentation == Presentation::Window,
                title,
            });
        }
    }

    /// Bottom-up grid answer: measure the items, size the tracks, return the box demand.
    fn measure_grid(
        &mut self,
        tree: &mut Tree,
        nid: NodeId,
        data: &NodeData,
        available: Size,
        count: &mut usize,
    ) -> (Size, Size, Size, Size) {
        let Some(node) = tree.get(nid) else {
            return (Size::ZERO, Size::ZERO, Size::ZERO, Size::ZERO);
        };
        let sc = node.size_class;
        let pad = node.box_model.padding;
        let sizes = sc.axes();
        let params = data.params().cloned().unwrap_or_default();
        // A capped box cannot offer its items more room than it will take itself.
        let own_avail = Size::new(
            available.w.min(max_len(sizes.0.max, available.w)),
            available.h.min(max_len(sizes.1.max, available.h)),
        );
        let inner = Size::new(
            (own_avail.w - pad.left - pad.right).max(0.0),
            (own_avail.h - pad.top - pad.bottom).max(0.0),
        );
        let decl_min = Size::new(
            min_len(sizes.0.min, available.w),
            min_len(sizes.1.min, available.h),
        );
        let decl_max = Size::new(
            max_len(sizes.0.max, available.w),
            max_len(sizes.1.max, available.h),
        );
        let layout = self.measure_grid_items(tree, nid, &params, inner, count);
        // A flexible track only divides space the grid can occupy: an unbounded offer (a
        // scrolling parent) leaves the extent indefinite, so the track sizes to content.
        let definite = (
            !matches!(sizes.0.size, crate::layout::Sizing::Fit) && own_avail.w.is_finite(),
            !matches!(sizes.1.size, crate::layout::Sizing::Fit) && own_avail.h.is_finite(),
        );
        // Placement is the same for both passes: only the track sizing differs.
        let placed = grid_cells(tree, &layout, &params, inner);
        // A track's width comes from intrinsic sizes, but an item's height does not: text
        // measured against the whole grid keeps the lines it wrapped at that width. Size the
        // columns first, let each item reflow into the cell it will occupy, then size the rows.
        let (column_widths, _) =
            grid_layout(tree, &layout, &params, inner, &placed, definite, false);
        self.reflow_grid_items(
            tree,
            &layout,
            &placed,
            &column_widths,
            params.gap,
            inner,
            count,
        );
        let (floor_w, floor_h) =
            grid_layout(tree, &layout, &params, inner, &placed, (false, false), true);
        let (max_w, max_h) = grid_layout(tree, &layout, &params, inner, &placed, definite, false);
        let gap = params.gap;
        let extent = |v: &[f32]| v.iter().sum::<f32>() + gap * v.len().saturating_sub(1) as f32;
        let content = Size::new(extent(&max_w), extent(&max_h));
        let min_c = Size::new(
            extent(&floor_w) + pad.left + pad.right,
            extent(&floor_h) + pad.top + pad.bottom,
        );
        let max_c = Size::new(
            content.w + pad.left + pad.right,
            content.h + pad.top + pad.bottom,
        );
        let used = Size::new(
            used_axis(
                sizes.0.size,
                own_avail.w,
                min_c.w,
                max_c.w,
                content.w,
                decl_min.w,
                decl_max.w,
            ),
            used_axis(
                sizes.1.size,
                own_avail.h,
                min_c.h,
                max_c.h,
                content.h,
                decl_min.h,
                decl_max.h,
            ),
        );
        (used, min_c, max_c, content)
    }

    /// Measure every in-flow item of a grid with the viewport as its space.
    fn measure_grid_items(
        &mut self,
        tree: &mut Tree,
        nid: NodeId,
        _params: &crate::tree::ContainerParams,
        inner: Size,
        count: &mut usize,
    ) -> Vec<NodeId> {
        let mut kids = Vec::new();
        let child_count = tree.children_of(Some(nid)).len();
        for i in 0..child_count {
            let Some(kid) = tree.child_at(Some(nid), i) else {
                continue;
            };
            let (res, c) = self.measure_node(tree, kid, Constraints::loose(inner));
            *count += c;
            if let Some(n) = tree.get_mut(kid) {
                n.rect.size = res.size;
                n.min_content = res.min_content;
                n.max_content = res.max_content;
            }
            // Measured, but they own no cell: floating and out-of-flow items alike.
            if tree.floats(kid) || tree.out_of_flow(kid) {
                continue;
            }
            kids.push(kid);
        }
        kids
    }

    /// Re-measure the items a narrower cell squeezes, so their content reflows at the width
    /// it will actually sit in.
    ///
    /// The intrinsic widths go back afterwards: they are what sized the tracks, and placement
    /// sizes them again from the same numbers — a re-wrapped item must not also narrow the
    /// column it was told to fit into.
    fn reflow_grid_items(
        &mut self,
        tree: &mut Tree,
        kids: &[NodeId],
        placed: &GridCells,
        widths: &[f32],
        gap: f32,
        inner: Size,
        count: &mut usize,
    ) {
        for (i, kid) in kids.iter().enumerate() {
            let Some(cell) = placed.cells.get(i).copied() else {
                continue;
            };
            let cell_w = widths[cell.col..(cell.col + cell.col_span).min(widths.len())]
                .iter()
                .sum::<f32>()
                + gap * cell.col_span.saturating_sub(1) as f32;
            let margin = flow_margin(tree, *kid, false);
            let room = (cell_w - margin.left - margin.right).max(0.0);
            let Some(node) = tree.get(*kid) else {
                continue;
            };
            // A declared pixel width already answered this for the author.
            if matches!(
                node.size_class.width.sizing,
                crate::layout::Sizing::Definite(crate::layout::Len::Px(_))
            ) {
                continue;
            }
            if node.rect.size.w <= room + 0.01 {
                continue;
            }
            let intrinsic = (node.min_content.w, node.max_content.w);
            let (res, c) =
                self.measure_node(tree, *kid, Constraints::loose(Size::new(room, inner.h)));
            *count += c;
            if let Some(n) = tree.get_mut(*kid) {
                n.rect.size = res.size;
                n.min_content = res.min_content;
                n.max_content = res.max_content;
                n.min_content.w = intrinsic.0;
                n.max_content.w = intrinsic.1;
            }
        }
    }

    /// Top-down grid placement: assign every item its cell, then align it inside.
    fn place_grid(
        &mut self,
        tree: &mut Tree,
        nid: NodeId,
        params: &crate::tree::ContainerParams,
        origin: crate::geom::Point,
        content: Size,
        stats: &mut LayoutStats,
    ) {
        let mut kids = Vec::new();
        let count = tree.children_of(Some(nid)).len();
        for i in 0..count {
            let Some(kid) = tree.child_at(Some(nid), i) else {
                continue;
            };
            if tree.out_of_flow(kid) {
                // Placed by the OOF pass, which runs after every in-flow box is down.
                continue;
            }
            if tree.floats(kid) {
                let size = tree.get(kid).map(|n| n.rect.size).unwrap_or(Size::ZERO);
                self.place_node(tree, kid, Rect::from_origin_size(origin, size), stats);
            } else {
                kids.push(kid);
            }
        }
        let sizes = tree
            .get(nid)
            .map(|n| n.size_class.axes())
            .unwrap_or_else(|| SizeClass::fixed(0.0, 0.0).axes());
        let definite = (
            !matches!(sizes.0.size, crate::layout::Sizing::Fit),
            !matches!(sizes.1.size, crate::layout::Sizing::Fit),
        );
        let placed = grid_cells(tree, &kids, params, content);
        let (widths, heights) = grid_layout(tree, &kids, params, content, &placed, definite, false);
        let gap = params.gap;
        let starts = |v: &[f32]| {
            let mut out = Vec::with_capacity(v.len());
            let mut acc = 0.0;
            for s in v {
                out.push(acc);
                acc += s + gap;
            }
            out
        };
        let xs = starts(&widths);
        let ys = starts(&heights);
        for (i, kid) in kids.iter().enumerate() {
            let Some(cell) = placed.cells.get(i).copied() else {
                continue;
            };
            let span = |v: &[f32], start: usize, span: usize| -> f32 {
                v[start..(start + span).min(v.len())].iter().sum::<f32>()
                    + gap * span.saturating_sub(1) as f32
            };
            let cell_w = span(&widths, cell.col, cell.col_span);
            let cell_h = span(&heights, cell.row, cell.row_span);
            let Some(n) = tree.get(*kid) else { continue };
            // The same resolution measure used: reading the raw class here would put a text's
            // line-box hint back as a one-line ceiling on the cell it is stretched into.
            let sc = layout_size_class(n.kind, n.size_class);
            let (grid, measured) = (n.grid, n.rect.size);
            // The margin sits outside the item: it offsets it and shrinks the cell it aligns in.
            let m = flow_margin(tree, *kid, false);
            let (w, dx) = place_in_cell(
                (cell_w - m.left - m.right).max(0.0),
                grid.justify,
                sc.width,
                measured.w,
            );
            let (h, dy) = place_in_cell(
                (cell_h - m.top - m.bottom).max(0.0),
                grid.align,
                sc.height,
                measured.h,
            );
            let rect = Rect::from_origin_size(
                crate::geom::Point::new(
                    origin.x + xs[cell.col] + m.left + dx,
                    origin.y + ys[cell.row] + m.top + dy,
                ),
                Size::new(w, h),
            );
            self.place_node(tree, *kid, rect, stats);
        }
        Self::settle_scroll_extent(
            tree,
            nid,
            Size::new(
                widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32,
                heights.iter().sum::<f32>() + gap * heights.len().saturating_sub(1) as f32,
            ),
        );
    }
}

#[cfg(test)]
mod tests;
