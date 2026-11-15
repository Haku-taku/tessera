//! Layout sizes and the pluggable layout engine trait.

use crate::geom::{Insets, Rect};

/// An inset side that is not declared; the solver derives it.
pub const AUTO: f32 = f32::INFINITY;

/// An author-specified length along one axis.
///
/// `Pct` is relative to the containing block on the same axis; when that is unknown it
/// reads as `Auto`. Percentage heights against an indefinite parent are not errors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Len {
    Auto,
    Px(f32),
    Pct(f32),
}

/// How an axis decides its size, independent of context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    /// An exact value (CSS `width`), stored as a length so `%` can be expressed.
    Definite(Len),
    /// Shrink to content: `clamp(min_content, avail, max_content)`, CSS fit-content / hug.
    Fit,
    /// Wants the space the parent offers, CSS stretch / fill. The node cannot take it by
    /// itself: a flow parent's cross rule (stretch by default) gives it the cross axis, and
    /// `Grow` shares the main axis. Until something offers space it measures like content,
    /// exactly as `Auto` does — the difference is the declaration, not the computation.
    Stretch,
    /// Decided by the container algorithm (flex main axis, grid item, abspos auto axis).
    Auto,
}

/// The context-free form of one axis: how it sizes, plus explicit bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisSizes {
    pub size: Sizing,
    /// Starts at 0, like CSS `min-width: 0`.
    pub min: Len,
    /// Starts unbounded, like CSS `max-width: none`.
    pub max: Len,
}

/// A node's size preference along one axis, in the shape of Qt's
/// `(min, sizeHint, max, expanding)` tuple; `sizes()` translates it to [`AxisSizes`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisSize {
    /// Below this the content breaks (e.g. text would be truncated).
    pub min: f32,
    /// Comfortable size; "hug content" reports the measured extent here.
    pub hint: f32,
    /// Above this a node gains nothing; "fixed" sets min == hint == max.
    pub max: f32,
    /// Which constructor made this value; the numbers alone cannot tell hug from fill.
    pub sizing: Sizing,
}

impl AxisSize {
    /// A fixed size: min == hint == max.
    pub const fn fixed(v: f32) -> Self {
        Self {
            min: v,
            hint: v,
            max: v,
            sizing: Sizing::Definite(Len::Px(v)),
        }
    }

    /// Hug content: a lower bound plus the measured extent.
    pub const fn hug(min: f32, hint: f32) -> Self {
        Self {
            min,
            hint,
            max: f32::INFINITY,
            sizing: Sizing::Fit,
        }
    }

    /// Hug content, pinned to `v` for now: still the hug sizing a parent reads, but the
    /// axis' own extent is clamped to `v`. The engine writes this while a size transition
    /// walks a hug axis, so the node never snaps back to its measured extent mid-leg.
    pub const fn hug_pinned(v: f32) -> Self {
        Self {
            min: v,
            hint: v,
            max: v,
            sizing: Sizing::Fit,
        }
    }

    /// Filling: measured like [`hug`](Self::hug), and filled by the parent's cross-axis
    /// rule or by `Grow` on the main axis; a hint stays a basis, never a cap.
    pub const fn fill(min: f32, hint: f32) -> Self {
        Self {
            min,
            hint,
            max: f32::INFINITY,
            sizing: Sizing::Stretch,
        }
    }

    /// Translate to the context-free vocabulary. `hint` is a comfort ceiling only where the
    /// author asked to fit content; a `fill` wants the space itself, so its hint stays a basis.
    pub fn sizes(self) -> AxisSizes {
        let capped = self.sizing == Sizing::Fit && self.hint.is_finite() && self.hint > self.min;
        let max = if capped {
            Len::Px(self.hint)
        } else if self.max.is_finite() {
            Len::Px(self.max)
        } else {
            Len::Auto
        };
        AxisSizes {
            size: self.sizing,
            min: Len::Px(self.min),
            max,
        }
    }

    pub fn clamp_to(self, lo: f32, hi: f32) -> Self {
        Self {
            min: self.min.clamp(lo, hi),
            hint: self.hint.clamp(lo, hi),
            max: self.max.clamp(lo, hi),
            sizing: self.sizing,
        }
    }
}

impl From<AxisSize> for AxisSizes {
    fn from(axis: AxisSize) -> Self {
        axis.sizes()
    }
}

/// How a node takes spare space on an axis. Covers "fill parent" and "flex weight".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grow {
    /// Never grows beyond `hint`, yet still shrinks like every item (a declared floor stops it).
    #[default]
    No,
    /// Takes all spare space (when it is the only grower) or shares equally.
    Yes,
    /// Takes spare space proportional to weight.
    Flex(u16),
    /// Grows from a zero basis, so equal weights give equal sizes whatever the content is
    /// (CSS `flex: 1 1 0`); `Yes` and `Flex` grow *on top of* the content instead.
    Split(u16),
}

/// Per-axis layout demand of one node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeClass {
    pub width: AxisSize,
    pub height: AxisSize,
    pub grow_x: Grow,
    pub grow_y: Grow,
}

impl SizeClass {
    pub const fn fixed(w: f32, h: f32) -> Self {
        Self {
            width: AxisSize::fixed(w),
            height: AxisSize::fixed(h),
            grow_x: Grow::No,
            grow_y: Grow::No,
        }
    }

    pub const fn hug(w: f32, h: f32) -> Self {
        Self {
            width: AxisSize::hug(0.0, w),
            height: AxisSize::hug(0.0, h),
            grow_x: Grow::No,
            grow_y: Grow::No,
        }
    }

    /// Both axes in the context-free vocabulary, leaving the declared fields untouched.
    pub fn axes(self) -> (AxisSizes, AxisSizes) {
        (self.width.into(), self.height.into())
    }
}

/// How one grid item sits inside the cell it was given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Fill the cell, or the line in a flow; a definite declared size still wins.
    #[default]
    Stretch,
    Start,
    Center,
    End,
}

/// Positioning scheme; `Sticky` is not modelled yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    /// Takes its slot in the flow; insets are ignored.
    #[default]
    Static,
    /// Keeps its slot, and shifts paint and hit by the insets.
    Relative,
    /// Leaves the flow; placed against its containing block.
    Absolute,
    /// Like `Absolute`, but the containing block is always the root box.
    Fixed,
}

/// Placement insets in author lengths, so `left: 50%` means half the containing block.
///
/// CSS resolves `left`/`right` against the containing block's width and `top`/`bottom`
/// against its height; [`resolve`](Self::resolve) does the same.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LenInsets {
    pub left: Len,
    pub top: Len,
    pub right: Len,
    pub bottom: Len,
}

impl LenInsets {
    /// No side declared.
    pub const AUTO: Self = Self {
        left: Len::Auto,
        top: Len::Auto,
        right: Len::Auto,
        bottom: Len::Auto,
    };

    pub const fn new(left: Len, top: Len, right: Len, bottom: Len) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Resolve against a containing block size; an automatic or unresolvable edge is
    pub fn resolve(self, cb: crate::geom::Size) -> Insets {
        fn one(len: Len, avail: f32) -> f32 {
            match len {
                Len::Auto => AUTO,
                Len::Px(v) => v,
                Len::Pct(p) if avail.is_finite() => avail * p / 100.0,
                Len::Pct(_) => AUTO,
            }
        }
        Insets::new(
            one(self.left, cb.w),
            one(self.top, cb.h),
            one(self.right, cb.w),
            one(self.bottom, cb.h),
        )
    }
}

impl From<Insets> for LenInsets {
    /// A pixel inset keeps its numbers; an [`AUTO`] side stays undeclared.
    fn from(inset: Insets) -> Self {
        fn one(v: f32) -> Len {
            if v.is_finite() { Len::Px(v) } else { Len::Auto }
        }
        Self::new(
            one(inset.left),
            one(inset.top),
            one(inset.right),
            one(inset.bottom),
        )
    }
}

/// Where a node sits: its scheme, insets and stacking order.
///
/// An inset side of [`Len::Auto`] is not declared, so the solver derives it from the
/// opposite side or from the static position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: Position,
    pub inset: LenInsets,
    /// Only a positioned node has an effective `z`; static children use 0.
    pub z: i32,
    /// Set for an anchored overlay; the engine then places it from its anchor instead of
    /// from insets, and lets it escape every clipping ancestor.
    pub overlay: Option<OverlayPlacement>,
    /// Painted at the top layer and free of ancestor clips, but still holding its layout slot.
    pub lift: bool,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            position: Position::Static,
            inset: LenInsets::AUTO,
            z: 0,
            overlay: None,
            lift: false,
        }
    }
}

/// When a row stops being a row: it stacks along its cross axis instead.
///
/// The decision is made while measuring, from the children's own sizes, so the container
/// still lays out in one pass. A stacked child reads its alternate margin, if it declared one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisSwitch {
    /// Stack once a child is taller than this, cross margin included: a line that already
    /// wrapped has broken the row, even if its wrapped width still fits.
    pub max_cross: Option<f32>,
}

/// What an overlay hangs from: a node's box, or a point in the root's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Node(crate::Id),
    Point(crate::geom::Point),
}

/// Which edge of the anchor the panel unfolds from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Below,
    Above,
    Left,
    Right,
}

/// How the panel lines up with the anchor along the other axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayAlign {
    Start,
    Center,
    End,
}

/// Which surface paints the panel an overlay declares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presentation {
    /// The parent's surface, laid out against it and clamped into its viewport.
    #[default]
    Parent,
    /// A surface of the platform's own: measured here as content, positioned there
    /// against an output this tree knows nothing about.
    Platform,
}

/// An anchored overlay: where it unfolds from, how far off, and what to do at an edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayPlacement {
    pub anchor: Anchor,
    pub side: Side,
    pub align: OverlayAlign,
    /// Gap between the anchor and the panel.
    pub offset: f32,
    /// Flip to the opposite side when the panel would leave the viewport.
    pub flip: bool,
    /// Panel width follows the anchor's (menus, combo boxes).
    pub match_anchor_width: bool,
    /// Panel height follows the anchor's (drawers, side panes that span their host).
    pub match_anchor_height: bool,
    /// The narrowest the panel may become when it matches the anchor (WinUI's
    /// `ComboBoxPopupThemeMinWidth`): a short trigger still gets a usable panel.
    pub min_width: f32,
    /// Painted by its parent, or presented by the platform.
    pub presentation: Presentation,
}

/// A panel the layout pass measured for a surface of the platform's own.
///
/// The rect is where the panel would be if its parent painted it: the platform may put
/// the surface somewhere else entirely, so this is a request, not a position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlatformPanel {
    /// The panel subtree's root, laid out in the parent tree.
    pub node: crate::id::NodeId,
    /// The anchor's rect, in the parent presentation's coordinates.
    pub anchor: Rect,
    /// The panel's own rect in those same coordinates, before any platform adjustment.
    pub rect: Rect,
    pub side: Side,
    pub align: OverlayAlign,
    pub offset: f32,
}

impl OverlayPlacement {
    /// A panel under `anchor`, left-aligned, no gap: the common menu shape.
    pub fn below(anchor: Anchor) -> Self {
        Self {
            anchor,
            side: Side::Below,
            align: OverlayAlign::Start,
            offset: 0.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Parent,
        }
    }

    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    pub fn align(mut self, align: OverlayAlign) -> Self {
        self.align = align;
        self
    }

    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    pub fn match_anchor_width(mut self, on: bool) -> Self {
        self.match_anchor_width = on;
        self
    }

    pub fn flip(mut self, on: bool) -> Self {
        self.flip = on;
        self
    }

    pub fn match_anchor_height(mut self, on: bool) -> Self {
        self.match_anchor_height = on;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn presentation(mut self, presentation: Presentation) -> Self {
        self.presentation = presentation;
        self
    }
}

/// One grid track template.
#[derive(Clone, Debug, PartialEq)]
pub enum Track {
    /// Exact logical units.
    Fixed(f32),
    /// The largest max-content of the items on this track.
    Auto,
    /// Share of what fixed and auto tracks leave over.
    Fr(f32),
    /// Content contribution clamped into `[min, max]`; `Auto` defers to the content and
    /// an `Fr` upper bound takes a share of the space the other tracks leave over.
    MinMax(TrackBound, TrackBound),
    /// The largest min-content of the items on this track.
    MinContent,
    /// The largest max-content of the items on this track.
    MaxContent,
    /// The inner track repeated; an auto repeat counts how many fit the space.
    Repeat(Repeat, Box<Track>),
}

/// One bound of a `minmax()` track: a length or a share of the free space.
///
/// A separate type because `Len` also sizes boxes and insets, where `fr` means nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackBound {
    Len(Len),
    Fr(f32),
}

/// How many times a `Track::Repeat` repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    /// A fixed count.
    Count(u16),
    /// As many as fit the space, keeping the empty ones.
    AutoFill,
    /// Like `AutoFill`, then the tracks no item landed in collapse.
    AutoFit,
}

/// How items without a `grid_area` take free cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoFlow {
    /// Fill row by row, left to right.
    #[default]
    Row,
    /// Fill column by column, top to bottom.
    Column,
    /// Row flow that backfills the first free cell instead of moving on.
    RowDense,
    /// Column flow that backfills the first free cell instead of moving on.
    ColumnDense,
}

impl AutoFlow {
    /// Whether the primary axis is the row axis.
    pub fn rows_first(self) -> bool {
        matches!(self, AutoFlow::Row | AutoFlow::RowDense)
    }

    /// Whether an item scans from the grid origin instead of the cursor.
    pub fn dense(self) -> bool {
        matches!(self, AutoFlow::RowDense | AutoFlow::ColumnDense)
    }
}

/// A grid container's track template.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackList {
    pub cols: Vec<Track>,
    pub rows: Vec<Track>,
    pub auto_flow: AutoFlow,
}

/// Where an item sits in its parent grid: 0-based row and column plus spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridArea {
    pub row: u32,
    pub col: u32,
    pub row_span: u32,
    pub col_span: u32,
}

/// Box edges a node reserves around its content.
///
/// A declared size is a border box, unlike CSS's default `content-box`; a border paints inside
/// that box as a stroke, so it takes no layout space. There is no border or box-sizing field.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxModel {
    /// Outside the border box; may be negative. Only shifts placement.
    pub margin: crate::geom::Insets,
    /// The margin a flow parent uses when it stacked along its cross axis instead.
    pub margin_alt: Option<crate::geom::Insets>,
    pub padding: crate::geom::Insets,
}

/// What one container does with content that does not fit its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    /// Content paints outside the box; this container contributes no clip.
    #[default]
    Visible,
    /// Content is cut at the box.
    Hidden,
    /// Like `Hidden`, and the wheel may move the content inside the box.
    Scroll,
}

/// Per-axis overflow policy of one container; clipping is opt-in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clip {
    pub x: Overflow,
    pub y: Overflow,
}

impl Clip {
    pub const VISIBLE: Self = Self {
        x: Overflow::Visible,
        y: Overflow::Visible,
    };

    /// Whether either axis cuts content.
    pub fn clips(self) -> bool {
        self.x != Overflow::Visible || self.y != Overflow::Visible
    }

    /// Whether the wheel may move this container's content.
    pub fn scrolls(self) -> bool {
        self.x == Overflow::Scroll || self.y == Overflow::Scroll
    }
}

/// Constraints handed down by a parent: `min` is a hard lower bound and `max` is the
/// offered space; content larger than `max` overflows instead of shrinking below `min`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min: crate::geom::Size,
    pub max: crate::geom::Size,
}

impl Constraints {
    /// Exactly `size`: `min == max == size`.
    pub const fn tight(size: crate::geom::Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    /// At most `max`: the floor is zero, so any smaller size is allowed.
    pub const fn loose(max: crate::geom::Size) -> Self {
        Self {
            min: crate::geom::Size::ZERO,
            max,
        }
    }
}

/// What one measurement call reports: the used size plus its intrinsic contributions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeasureResult {
    pub size: crate::geom::Size,
    /// The narrowest the node can get without breaking its content.
    pub min_content: crate::geom::Size,
    /// The size the node takes when nothing constrains it.
    pub max_content: crate::geom::Size,
    /// What the content itself measured on each axis, before the axis sizing clamped it;
    /// a `Fit` (hug) axis settles on this clamped by the declared bounds.
    pub content: crate::geom::Size,
    /// First baseline; 0 until the inline pass reports one (P4).
    pub baseline: f32,
}

/// A node's cached measurement, valid only while its constraints and subtree stay put.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LastMeasure {
    pub constraints: Constraints,
    pub result: MeasureResult,
}

/// Statistics returned by a layout pass, used by tests and diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutStats {
    /// How many nodes were measured this pass.
    pub measured: usize,
    /// How many of them were actually measured instead of served from the cache.
    pub remeasured: usize,
}

/// Pluggable layout engine.
///
/// The built-in engine is a single deterministic measure pass (no second pass,
/// no `request_discard`-style retry). A taffy-backed engine can be added later
/// behind a feature flag.
pub trait LayoutEngine {
    /// Solve the tree's layout, writing each node's rect.
    fn solve(
        &mut self,
        tree: &mut crate::tree::Tree,
        root: NodeIdLocal,
        constraints: Constraints,
    ) -> LayoutStats;

    /// Whether this engine can solve only dirty subtrees.
    fn supports_incremental(&self) -> bool {
        false
    }
}

// Local alias so this module does not force a tree import in doc examples.
pub use crate::id::NodeId as NodeIdLocal;

/// Helper: the rect a child gets inside a parent's content box.
pub fn place(parent_content: Rect, size: crate::geom::Size) -> Rect {
    Rect::from_origin_size(parent_content.origin, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hug and fill must no longer be byte-identical: one fits content, one stretches.
    #[test]
    fn hug_and_fill_choose_different_sizing() {
        let hug: AxisSizes = AxisSize::hug(0.0, 0.0).into();
        let fill: AxisSizes = AxisSize::fill(0.0, 0.0).into();
        assert_eq!(hug.size, Sizing::Fit);
        assert_eq!(fill.size, Sizing::Stretch);
        assert_ne!(hug, fill);
    }
}
