//! The retained tree: persistent structure, immediate declarations.

use crate::collections::FastMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::geom::{Corners, Insets, Rect, Size};
use crate::id::{Id, NodeId};
use crate::layout::{BoxModel, Constraints, SizeClass};

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// Horizontal flow.
    Row,
    /// Vertical flow.
    Column,
    /// Children stacked on top of each other.
    Stack,
    /// Absolutely positioned relative to the parent.
    Overlay,
    /// Leaf that paints a filled, possibly rounded box.
    Box,
    /// Leaf that paints text.
    Text,
    /// Leaf that paints a laid-out formula.
    Math,
    /// Two-dimensional track layout, see [`crate::layout::TrackList`].
    Grid,
}

/// Grid placement and alignment a child declares for its grid parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridItem {
    /// `None` auto-flows into the next free cell.
    pub area: Option<crate::layout::GridArea>,
    /// Cross-axis alignment: vertical in a grid cell, and the container's cross axis in a
    /// `Column` or `Row`, where it overrides the line's own alignment. `None` follows it.
    pub align: Option<crate::layout::Align>,
    /// Horizontal alignment in a grid cell. A flow item has no main-axis slack to place
    /// itself in, so this is ignored there — as CSS ignores `justify-self` in flex.
    pub justify: Option<crate::layout::Align>,
    /// Cells an auto-flowed item occupies; explicit areas carry their own spans.
    pub span: (u32, u32),
}

impl Default for GridItem {
    fn default() -> Self {
        Self {
            area: None,
            align: None,
            justify: None,
            span: (1, 1),
        }
    }
}

/// Backdrop blur parameters: kernel radius in pixels and its Gaussian sigma.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropBlur {
    pub radius: u32,
    pub sigma: f32,
    /// How the blurred result is shaded; `Plain` keeps it untouched.
    pub material: crate::draw::BackdropMaterial,
}

impl BackdropBlur {
    /// Radius derived from a CSS-style `blur(Npx)` value, as `⌈3σ⌉`.
    pub fn css(sigma: f32) -> Self {
        Self {
            radius: (3.0 * sigma).ceil().max(1.0) as u32,
            sigma,
            material: crate::draw::BackdropMaterial::PLAIN,
        }
    }

    /// Shade the blur with a named material instead of writing it back untouched.
    pub fn with_material(mut self, material: crate::draw::BackdropMaterial) -> Self {
        self.material = material;
        self
    }
}

/// How a texture is fitted into a node's box.
///
/// There is deliberately no `Repeat`: a source smaller than the box is cropped or
/// letterboxed, never tiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    /// Scale to cover the box, cropping the overflow.
    Cover,
    /// Scale to fit inside the box, leaving the fill visible around it.
    Contain,
    /// Stretch, ignoring the aspect ratio.
    Fill,
}

impl ImageFit {
    /// Destination rect and source rect (in texture pixels) for one box.
    ///
    /// The source never leaves the texture: `Cover` crops, `Contain` letterboxes, `Fill`
    /// stretches. Tiling would need a source outside it, so it cannot happen by accident.
    pub fn place(self, box_: Rect, texture: (u32, u32)) -> (Rect, Rect) {
        let (tw, th) = (texture.0.max(1) as f32, texture.1.max(1) as f32);
        let whole = Rect::new(0.0, 0.0, tw, th);
        if box_.size.w <= 0.0 || box_.size.h <= 0.0 {
            return (box_, whole);
        }
        match self {
            ImageFit::Fill => (box_, whole),
            ImageFit::Cover => {
                let s = (box_.size.w / tw).max(box_.size.h / th);
                let (sw, sh) = (box_.size.w / s, box_.size.h / s);
                let src = Rect::new((tw - sw) * 0.5, (th - sh) * 0.5, sw, sh);
                (box_, src)
            }
            ImageFit::Contain => {
                let s = (box_.size.w / tw).min(box_.size.h / th);
                let (dw, dh) = (tw * s, th * s);
                let dest = Rect::new(
                    box_.origin.x + (box_.size.w - dw) * 0.5,
                    box_.origin.y + (box_.size.h - dh) * 0.5,
                    dw,
                    dh,
                );
                (dest, whole)
            }
        }
    }
}

/// A soft shadow: what colour it casts, how far it fades and how deep it sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    /// The cast colour; its alpha is the peak alpha at the shape's edge.
    pub colour: crate::geom::Color,
    /// Falloff width in logical units.
    pub blur: f32,
    /// How far the cast shape grows before blurring, as CSS `box-shadow` spread.
    pub spread: f32,
    /// Vertical offset in logical units.
    pub dy: f32,
    /// Paints within the surface's own edge instead of outside it: CSS's inset box-shadow.
    pub inset: bool,
}

impl ShadowSpec {
    /// A black shadow at peak `alpha`: the elevation every WinUI and MD surface casts.
    pub const fn black(blur: f32, alpha: f32, dy: f32) -> Self {
        Self {
            colour: crate::geom::Color::rgba(0.0, 0.0, 0.0, alpha),
            blur,
            spread: 0.0,
            dy,
            inset: false,
        }
    }

    /// A coloured halo that grows `spread` beyond the shape, centred on it.
    pub const fn glow(colour: crate::geom::Color, blur: f32, spread: f32) -> Self {
        Self {
            colour,
            blur,
            spread,
            dy: 0.0,
            inset: false,
        }
    }

    /// WinUI's elevation ramp (`ThemeShadow`): deeper panels fade wider and sit lower.
    pub const fn elevation(level: u8) -> Self {
        // Two linear ramps keep the table honest: no hand-tuned per-level magic.
        let blur = 2.0 + 4.0 * level as f32;
        let dy = 1.0 + 2.0 * level as f32;
        Self::black(blur, 0.18, dy)
    }
}

/// A texture drawn into a node's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageSpec {
    /// Stable texture identity, registered on the loop.
    pub id: u64,
    pub fit: ImageFit,
    /// How the sampler reads between texels. It belongs to the drawing, not the picture: one
    /// file drawn enlarged in two pins may want both.
    pub sampling: crate::draw::Sampling,
}

/// What a modal surface covers, and therefore what it dims and blocks.
///
/// The two choices are the ones every declarative toolkit converges on: Flutter's
/// `OverlayChildLocation { rootOverlay, nearestOverlay }` and UIKit's `currentContext`
/// against the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalTarget {
    /// The whole window: what `showDialog` does by default.
    Global,
    /// The nearest ancestor declared [`ContainerParams::modal_host`], falling back to the
    /// window when there is none.
    Proximate,
}

/// Values a node is painted from at the far end of an `enter` or `exit` transition.
///
/// The values are stated, never inferred: a node that did not exist last frame has no
/// previous state to travel from (CSS says the same with `@starting-style`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presence {
    pub spec: crate::anim::AnimSpec,
    /// The scale runs on its own clock when the two differ, as WinUI's dialog does: its
    /// opacity lands in 83 ms while its scale travels for 250.
    pub scale_spec: Option<crate::anim::AnimSpec>,
    pub opacity: f32,
    /// Visual displacement at that end, in logical units; layout ignores it.
    pub offset: crate::geom::Point,
    /// Scale at that end about the node's own centre; 1 is its laid-out size.
    pub scale: f32,
}

impl Presence {
    /// Fade from/to `opacity`, in place.
    pub fn fade(spec: crate::anim::AnimSpec, opacity: f32) -> Self {
        Self {
            spec,
            scale_spec: None,
            opacity,
            offset: crate::geom::Point::ZERO,
            scale: 1.0,
        }
    }

    /// Fade while displaced: `offset` is where that end sits relative to the final one.
    pub fn slide(spec: crate::anim::AnimSpec, opacity: f32, offset: crate::geom::Point) -> Self {
        Self {
            spec,
            scale_spec: None,
            opacity,
            offset,
            scale: 1.0,
        }
    }

    /// The same transition at `scale` instead of full size.
    pub const fn scaled(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// The same end scale, travelled on `spec` instead of the presence's own clock.
    pub fn scaled_over(mut self, scale: f32, spec: crate::anim::AnimSpec) -> Self {
        self.scale = scale;
        self.scale_spec = Some(spec);
        self
    }

    /// What a node shows `value` of the way from this end to its declaration.
    ///
    /// The far values are interpolated, not multiplied: a slide that keeps opacity at 1
    /// stays opaque while it travels. A landed scale snaps to exactly 1, or every glyph in
    /// the subtree stays on the transformed path (`scale_value` runs its own clock).
    pub fn values_at(&self, value: f32, scale_value: f32) -> PresenceValues {
        PresenceValues {
            alpha: self.opacity + value * (1.0 - self.opacity),
            shift: self.offset * (1.0 - value),
            scale: if scale_value >= 1.0 {
                1.0
            } else {
                self.scale + scale_value * (1.0 - self.scale)
            },
        }
    }
}

/// The paint values one presence node shows at a given progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresenceValues {
    pub alpha: f32,
    pub shift: crate::geom::Point,
    pub scale: f32,
}

/// What happens to the slot a leaving node held, while its exit runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExitSpace {
    /// The slot goes on the first exit frame: siblings reflow at once, only the paint waits.
    #[default]
    OutOfFlow,
    /// The slot stays at the size the node last had; siblings reflow when the exit lands.
    KeepSpace,
    /// The slot shrinks with the exit, so the siblings travel with the node as it leaves.
    Collapse,
}

/// Spacing and alignment parameters carried by container nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerParams {
    pub gap: f32,
    /// For a `Row`: stack along the column axis when the children no longer fit side by side.
    pub switch_axis: Option<crate::layout::AxisSwitch>,
    /// Share of the spare cross-axis space a child takes: 0 hugs, 1 stretches.
    pub cross_align: f32,
    /// Where a child sits in the cross-axis space it did not take: 0 = start, 0.5 = centre.
    pub cross_pos: f32,
    /// Align the children of a `Row` on one shared baseline instead of `cross_align`/
    /// `cross_pos`. A child with no baseline of its own sits on the line by its bottom
    /// margin edge, the way CSS synthesises one for a box. A `Column` ignores this: its
    /// cross axis is horizontal, where a baseline has no meaning.
    pub cross_baseline: bool,
    /// Where a `Stack`'s children sit in its content box, per axis. A stack has no main
    /// axis, so this replaces `cross_align`/`cross_pos` for it; the default is the origin.
    pub stack_align: (crate::layout::OverlayAlign, crate::layout::OverlayAlign),
    /// Group opacity for this subtree, 0..=1. Below 1 it composites as one unit.
    pub opacity: f32,
    /// A paint-only scale of this node and its subtree, about the node's own top-left.
    ///
    /// Layout is untouched: the box keeps the size it was laid out at, and the hit box is the
    /// scaled rect. Text is shaped at the scaled size rather than stretched, which is what a
    /// zoomable pin needs; `1.0` paints as laid out.
    pub scale: f32,
    /// Effect chain applied to the whole subtree, in order.
    pub effects: Vec<crate::effect::Effect>,
    /// Travel for this node's *fixed* size when the declared value changes; `None` jumps.
    pub size_transition: Option<crate::anim::AnimSpec>,
    /// Start values this node is painted from when it first appears; `None` appears settled.
    pub enter: Option<Presence>,
    /// Values this node is painted toward when it leaves; `None` disappears at once.
    ///
    /// Leaving means the declaration stopped mentioning it, so the engine keeps the node
    /// alive and out of layout until it lands — the app never holds a dead subtree.
    pub exit: Option<Presence>,
    /// Whether the slot the node held survives its exit, and whether it shrinks with it.
    pub exit_space: ExitSpace,
    /// Paint this subtree into its own target that survives frames, so an unchanged
    /// subtree is neither rebuilt into vertices nor drawn again.
    pub offscreen: bool,
    /// Blurs what is painted behind this node. Unlike `effects`, filters the backdrop.
    pub backdrop_blur: Option<BackdropBlur>,
    /// Track template when this node is a `Grid`; `None` lays the items out in one column.
    pub tracks: Option<Box<crate::layout::TrackList>>,
    /// What happens to content that does not fit the box, per axis.
    ///
    /// A `Scroll` main axis also stops the line from squeezing its children.
    pub clip: crate::layout::Clip,
    /// How far the content is scrolled left, in logical units.
    ///
    /// Same contract as [`Self::scroll_y`], for a horizontal strip.
    pub scroll_x: f32,
    /// How far the content is scrolled up, in logical units.
    ///
    /// A view value, not state: the offset belongs to the app, which passes it in each
    /// frame; layout shifts the children, never the viewport.
    pub scroll_y: f32,
    /// The engine is stretching this scroller past its ends right now (a pan's rubber
    /// band or the spring returning it), so layout must not clamp the offset back.
    pub scroll_elastic: bool,
    /// Page size per axis for a scroller that comes to rest on multiples of it; zero
    /// leaves the axis free. A wheel burst turns one page, a release lands on the nearest.
    pub snap: [f32; 2],
    /// Press ink the engine splats where a press lands on this node or its descendants.
    pub press_ink: Option<crate::draw::PressInk>,
    /// The focus adorner to draw around this node while it holds visible focus.
    ///
    /// Drawn after the subtree, so it is never hidden by children, and outside the box,
    /// which is why it lives here rather than among the shapes.
    pub focus_visual: Option<crate::draw::FocusVisual>,
    /// Whether this node is a Tab stop even though it paints no ink of its own.
    ///
    /// A control that draws its ring on a child still has to say it takes focus.
    pub focus_stop: bool,
    /// The node a popup opens onto, when it should not be the first focusable one.
    ///
    /// Read once, when the popup takes the keyboard; a hint outside the popup is ignored.
    pub focus_initial: Option<crate::Id>,
    /// What this node is modal *over*, when it is a modal surface; the layout places it on
    /// that rect instead of where the tree put it.
    ///
    /// A modal declared inside the panel it dims still covers the whole panel: the target
    /// is resolved after layout, so no rect has to be passed in from outside.
    pub modal: Option<ModalTarget>,
    /// Whether this container can be the target of a `Proximate` modal.
    pub modal_host: bool,
    /// The title a window surface carries when this node is presented as one. The app's
    /// text, because a window title is localized like any other label.
    pub window_title: Option<String>,
    /// The node's own curve when the frame does not paint one: a text button is a pill for
    /// its ripple and for its clip, yet has no container at rest.
    ///
    /// It outranks the first box shape wherever the engine infers a curve, since a shape
    /// that comes and goes (a hover layer) cannot decide a curve that must not move.
    pub corners: Option<crate::geom::Corners>,
    /// The buffer this node edits, when it accepts text; the engine reports edits as
    /// `Change::Text` and remembers the caret. The app still owns the value.
    pub text_input: Option<Arc<str>>,
    /// Whether this input takes hard line breaks: `Shift+Enter` inserts `\n` instead of
    /// activating. A single-line field never sees a `\n` in its buffer.
    pub text_input_multiline: bool,
    /// Whether a plain `Enter` also inserts `\n`. A document editor sets this: a break in
    /// the text is a block split after the app reparses, not a command.
    pub text_input_enter_newline: bool,
    /// Whether the buffer is secret: a mask on screen, no copy or cut, and an input method
    /// told not to learn from it. The app still sees the real value in `Change::Text`.
    pub text_input_hidden: bool,
    /// What the input is for, so the input method can offer the right actions.
    pub text_input_purpose: crate::ime::ContentPurpose,
    /// Shortcuts this node answers while it or a descendant holds focus.
    ///
    /// The engine resolves the chord and reports `Change::Shortcut`; the node never sees a key.
    pub shortcuts: Vec<(crate::shortcut::KeyChord, crate::shortcut::ShortcutId)>,
    /// Rows a secondary click on this node opens, most specific node first.
    pub menu: Vec<crate::menu::MenuItem>,
    /// Where the scrollbar sits in this node's own space, when it draws one; the engine
    /// turns a press there into a thumb drag instead of a content press.
    pub scrollbar: Option<Scrollbar>,
}

/// A scrollbar in its container's own space: what it looks like and where a press lands.
///
/// The engine paints it after the content, so an opaque child can never hide a bar that
/// still takes the press at its edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scrollbar {
    /// The bar along the vertical axis, when this container draws one.
    pub vertical: Option<ScrollbarBar>,
    /// The bar along the horizontal axis, when this container draws one.
    pub horizontal: Option<ScrollbarBar>,
    /// The engine fades this bar with activity: it appears while the pointer is over the
    /// scroller or a wheel moved it, and leaves after a short hold. A bar that is not
    /// faded is drawn at full strength whenever it is declared.
    pub fade: bool,
}

impl Scrollbar {
    /// Both declared bars, paired with the axis each one drags.
    pub fn bars(&self) -> impl Iterator<Item = (ScrollAxis, ScrollbarBar)> {
        [
            (ScrollAxis::Vertical, self.vertical),
            (ScrollAxis::Horizontal, self.horizontal),
        ]
        .into_iter()
        .filter_map(|(axis, bar)| bar.map(|bar| (axis, bar)))
    }
}

/// Which way a scrollbar runs, and therefore which offset a press on it drags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
}

/// One bar's geometry and colours. A track with a transparent fill is not painted, so the
/// thumb floats over the content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarBar {
    /// The thumb's rect, which a press may grab.
    pub thumb: crate::geom::Rect,
    /// The whole track; a press outside the thumb jumps the thumb there.
    pub track: crate::geom::Rect,
    /// Track colour; a transparent track is not painted.
    pub track_fill: crate::geom::Color,
    pub thumb_fill: crate::geom::Color,
    pub track_corners: crate::geom::Corners,
    pub thumb_corners: crate::geom::Corners,
}

impl Default for ContainerParams {
    fn default() -> Self {
        Self {
            gap: 0.0,
            switch_axis: None,
            cross_align: 1.0,
            cross_pos: 0.0,
            cross_baseline: false,
            stack_align: (
                crate::layout::OverlayAlign::Start,
                crate::layout::OverlayAlign::Start,
            ),
            opacity: 1.0,
            scale: 1.0,
            effects: Vec::new(),
            size_transition: None,
            enter: None,
            exit: None,
            exit_space: ExitSpace::OutOfFlow,
            offscreen: false,
            backdrop_blur: None,
            tracks: None,
            clip: crate::layout::Clip::VISIBLE,
            scroll_x: 0.0,
            scroll_y: 0.0,
            scroll_elastic: false,
            snap: [0.0, 0.0],
            press_ink: None,
            focus_visual: None,
            focus_stop: false,
            focus_initial: None,
            corners: None,
            modal: None,
            modal_host: false,
            window_title: None,
            text_input: None,
            text_input_multiline: false,
            text_input_enter_newline: false,
            text_input_hidden: false,
            text_input_purpose: crate::ime::ContentPurpose::Normal,
            shortcuts: Vec::new(),
            menu: Vec::new(),
            scrollbar: None,
        }
    }
}

/// Per-kind payload.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeData {
    Container(ContainerParams),
    /// Styled runs; rich text grows by adding runs, not fields.
    /// Text carries its ink because the component chose the role, not the type ramp.
    Text {
        /// Shared with the draw list, which a presenter may draw on another thread.
        block: std::sync::Arc<crate::text::TextBlock>,
        ink: crate::geom::Color,
        /// Insertion caret at a byte offset, drawn by the text pass.
        caret: Option<crate::draw::Caret>,
        /// Selected bytes and how this style draws them, when the node is selectable.
        selection: Option<crate::selection::SelectionPrim>,
        /// Bytes the input method is composing; drawn with an underline.
        composing: Option<std::ops::Range<usize>>,
        /// Lay this text out on one line however narrow the box is; a single-line editor
        /// scrolls it instead of wrapping.
        nowrap: bool,
    },
    /// A formula laid out by `ui-math`: a display list the text pass rasterizes.
    Math {
        formula: std::sync::Arc<crate::math::Formula>,
        /// Family the formula was laid out with. Glyph ids are per face, so the painter must
        /// use this face and no other.
        family: std::sync::Arc<str>,
        ink: crate::geom::Color,
    },
}

impl NodeData {
    pub fn is_container(&self) -> bool {
        matches!(self, NodeData::Container(_))
    }

    /// Container parameters, if this node lays out children.
    pub fn params(&self) -> Option<&ContainerParams> {
        match self {
            NodeData::Container(p) => Some(p),
            _ => None,
        }
    }

    /// Mutable container parameters.
    pub fn params_mut(&mut self) -> Option<&mut ContainerParams> {
        match self {
            NodeData::Container(p) => Some(p),
            _ => None,
        }
    }
}

/// One node in the retained tree.
#[derive(Debug)]
pub struct Node {
    pub id: Id,
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
    pub data: NodeData,
    /// Layout demand, recomputed when props change.
    pub size_class: SizeClass,
    /// Size this frame's declaration asked for; a size transition overwrites `size_class`
    /// with the interpolated value, so the target has to live somewhere else.
    pub declared_size: SizeClass,
    /// Declared box edges, owned by the node rather than buried in container params.
    pub box_model: BoxModel,
    /// Narrowest size the content allows, from the last measure pass.
    pub min_content: Size,
    /// Size the content wants when nothing constrains it.
    pub max_content: Size,
    /// Size of the content inside a scroll container, for its scrollbars.
    ///
    /// `None` until the first placement computes it: an empty content and an unmeasured one
    /// need different answers, and both would read as a zero size.
    pub scroll_extent: Option<Size>,
    /// Result of the last layout pass.
    pub rect: Rect,
    /// Whether the last measure stacked this flow container along its cross axis.
    pub flow_vertical: bool,
    /// Visual-only displacement; layout and `rect` ignore it.
    ///
    /// Dragging must not reflow: a per-frame relayout puts the hit box one frame behind
    /// the pixels, which reads as jitter at speed. This is the folded one: the declaration's
    /// own displacement plus every ancestor's.
    pub offset: crate::geom::Point,
    /// The displacement the declaration alone asks for. `accumulate_offsets` folds from
    /// this, never from `offset`, so a subtree the cache skipped cannot compound it.
    pub local_offset: crate::geom::Point,
    /// How far this node has arrived, 0 at its `enter` start and 1 once settled.
    pub presence: f32,
    /// Group opacity factor of a node still entering, 1 once settled.
    pub presence_alpha: f32,
    /// Painted displacement of a node still entering; folded into [`Node::visual_rect`].
    pub presence_shift: crate::geom::Point,
    /// Painted scale about this node's centre; 1 when settled.
    pub presence_scale: f32,
    /// This node's own declared cursor shape; the resolved one walks up the ancestors.
    pub cursor: Option<crate::cursor::CursorShape>,
    /// What interactions this node subscribes to, as declared this frame.
    pub sense: crate::sense::Sense,
    /// What a drag that starts on this node carries, when it declares a payload.
    pub drag: Option<Rc<crate::dnd::DragData>>,
    /// Start an operating-system drag so another window can take the payload.
    pub drag_out: bool,
    /// MIME patterns this node takes as a drop target; `None` takes anything it is offered.
    pub drop_types: Option<Rc<Vec<String>>>,
    /// Inert, folded from this declaration and every ancestor's.
    pub disabled: bool,
    /// Grid placement and cell alignment declared by this node.
    pub grid: GridItem,
    /// Positioning scheme, insets and stacking order.
    pub placement: crate::layout::Placement,
    /// Environment in force at this node: the nearest declared one on the path from root.
    pub env: Option<Arc<dyn std::any::Any + Send + Sync>>,
    /// Clip inherited from the clipping ancestors; a visible container adds nothing.
    pub mask: Rect,
    /// The same clip with scroll viewports left out: they translate content, not hide it.
    pub focus_mask: Rect,
    /// How far this node may paint outside `rect` (halo/shadow/grips).
    pub ink_margin: Insets,
    /// The box layout measured text in; paint positions lines in the same box.
    ///
    /// A `nowrap` line never wraps, but the box is still what its alignment stands in.
    pub wrap_width: f32,
    /// Lines layout wrapped this text node into; paint draws exactly these.
    pub inline: Option<std::sync::Arc<crate::text::InlineLayout>>,
    /// First baseline from the node's top edge; 0 for anything without text on its first line.
    pub baseline: f32,
    /// True when layout must be redone for this subtree.
    pub dirty: bool,
    /// The last measurement, reused while `dirty` is false and the constraints repeat.
    pub last_measure: Option<crate::layout::LastMeasure>,
    /// Paint-only shapes, in this node's own space.
    pub shapes: Vec<crate::draw::Shape>,
    /// Position among its siblings in the declaration, which is also the paint order.
    pub order: u32,
    /// True when the node existed last frame too.
    pub reused: bool,
    /// Kept alive only to leave: out of the live tree, painted above it.
    pub held: bool,
    /// The box the node last laid out in, so a kept or collapsed slot knows what it held.
    pub exit_size: crate::geom::Size,
    /// The declaration this node was last applied from, kept to detect a cached subtree.
    pub src: Option<Rc<Decl>>,
}

impl Node {
    /// Rect grown by `ink_margin`: the area this node can actually affect.
    pub fn ink_rect(&self) -> Rect {
        let base = self
            .ink_margin
            .expand(self.rect)
            .translate(self.paint_offset());
        // A shape may paint outside the node's own box; damage must cover it too.
        let box_rect = self.visual_rect();
        self.shapes
            .iter()
            .fold(base, |acc, s| acc.union(s.bounds(box_rect)))
    }

    /// Displacement applied at paint and hit time only, never to layout.
    pub fn paint_offset(&self) -> crate::geom::Point {
        self.offset + self.presence_shift
    }

    /// The rect as drawn and hit tested, i.e. with the visual offset applied.
    pub fn visual_rect(&self) -> Rect {
        self.rect.translate(self.paint_offset())
    }

    /// Whether this node takes a drag offering `types`.
    ///
    /// A target that named no patterns takes anything, which is what a plain
    /// `Sense::DROP` container means.
    pub fn accepts_drop<'a>(&self, types: impl Iterator<Item = &'a str>) -> bool {
        if !self.sense.contains(crate::sense::Sense::DROP) {
            return false;
        }
        match &self.drop_types {
            None => true,
            Some(accepted) => crate::dnd::offered_matches(accepted, types),
        }
    }
}

/// The static scale a node declares; a leaf paints as laid out.
fn declared_scale(node: &Node) -> f32 {
    node.data.params().map_or(1.0, |params| params.scale)
}

/// A declaration produced by user code each frame.
///
/// Equality is by identity, not by address: two declarations that carry the same env are
/// equal even though `dyn Any` has no `PartialEq` of its own.
/// `PartialEq`.
#[derive(Clone, Debug)]
pub struct Decl {
    pub id: Id,
    pub kind: NodeKind,
    pub data: NodeData,
    /// Box edges of any node, leaf included: a text run takes a margin like a container does.
    pub box_model: crate::layout::BoxModel,
    pub size_class: SizeClass,
    /// Visible extent beyond `rect`. Non-zero for shadows, focus rings, halos.
    pub ink_margin: Insets,
    /// Visual-only displacement applied at paint and hit time; layout ignores it.
    pub offset: crate::geom::Point,
    /// Shape shown while hovered; `None` defers to an ancestor. Explicit `Arrow` means "stop here".
    pub cursor: Option<crate::cursor::CursorShape>,
    /// What interactions this node subscribes to; the router reads only this.
    pub sense: crate::sense::Sense,
    /// What a drag that starts here carries. A node that declares one is a drag source.
    pub drag: Option<Rc<crate::dnd::DragData>>,
    /// Start an operating-system drag at the threshold, so the payload can leave this app.
    /// Ignored without [`drag`](Self::drag).
    pub drag_out: bool,
    /// MIME patterns this node takes as a drop target; `None` takes anything offered.
    /// A node that sets this accepts drops even without [`Sense::DROP`](crate::Sense::DROP).
    pub drop_types: Option<Rc<Vec<String>>>,
    /// Inert: neither this node nor its subtree takes input, focus or hover.
    ///
    /// Inherited downward, like a disabled fieldset: a child cannot re-enable itself.
    pub disabled: bool,
    /// Where this item sits in a grid parent, and how it aligns inside its cell.
    pub grid: GridItem,
    /// Positioning scheme, insets and stacking order; `static` by default.
    pub placement: crate::layout::Placement,
    /// Opaque environment for this subtree; a `scoped` node replaces it, children inherit.
    ///
    /// `ui-core` never interprets it: only the style layer knows what is inside. It lives on
    /// the declaration so the engine can hand it to every control without the app threading
    /// it by hand.
    pub env: Option<Arc<dyn std::any::Any + Send + Sync>>,
    /// Paint-only shapes, in this node's own space; see [`crate::draw::Shape`].
    pub shapes: Vec<crate::draw::Shape>,
    /// Shared so a cached subtree can be spliced into a new parent without copying it.
    pub children: Vec<Rc<Decl>>,
}

impl Decl {
    /// Everything this declaration says about itself, leaving the children out: the shell a
    /// container builds from app input, which the engine has no fact to replay against.
    ///
    /// Stricter than [`PartialEq`], which skips `disabled`: for reuse a false "same" is
    /// a stale subtree, so every field that can move a pixel has to match.
    pub fn own_eq(&self, other: &Self) -> bool {
        let same_env = match (&self.env, &other.env) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        same_env
            && self.id == other.id
            && self.kind == other.kind
            && self.data == other.data
            && self.box_model == other.box_model
            && self.size_class == other.size_class
            && self.ink_margin == other.ink_margin
            && self.offset == other.offset
            && self.cursor == other.cursor
            && self.sense == other.sense
            && self.drag == other.drag
            && self.drag_out == other.drag_out
            && self.drop_types == other.drop_types
            && self.disabled == other.disabled
            && self.grid == other.grid
            && self.placement == other.placement
            && self.shapes == other.shapes
    }
}

impl PartialEq for Decl {
    fn eq(&self, other: &Self) -> bool {
        let same_env = match (&self.env, &other.env) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        same_env
            && self.id == other.id
            && self.kind == other.kind
            && self.data == other.data
            && self.box_model == other.box_model
            && self.size_class == other.size_class
            && self.ink_margin == other.ink_margin
            && self.offset == other.offset
            && self.cursor == other.cursor
            && self.sense == other.sense
            && self.drag == other.drag
            && self.drag_out == other.drag_out
            && self.drop_types == other.drop_types
            && self.grid == other.grid
            && self.placement == other.placement
            && self.shapes == other.shapes
            && self.children.len() == other.children.len()
            // Pointer equality short-circuits the deep compare for cached subtrees.
            && self
                .children
                .iter()
                .zip(other.children.iter())
                .all(|(a, b)| Rc::ptr_eq(a, b) || a == b)
    }
}

impl Decl {
    pub fn container(id: Id, kind: NodeKind) -> Self {
        Self {
            id,
            kind,
            data: NodeData::Container(ContainerParams::default()),
            box_model: crate::layout::BoxModel::default(),
            shapes: Vec::new(),
            size_class: SizeClass::hug(0.0, 0.0),
            ink_margin: Insets::ZERO,
            offset: crate::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: crate::sense::Sense::NONE,
            drag: None,
            drag_out: false,
            drop_types: None,
            disabled: false,
            grid: GridItem::default(),
            placement: crate::layout::Placement::default(),
            env: None,
            children: Vec::new(),
        }
    }

    /// Take this node out of the flow and place it against its containing block.
    pub fn absolute(mut self) -> Self {
        self.placement.position = crate::layout::Position::Absolute;
        self
    }

    /// Keep this node's slot and shift only its paint and hit by the insets.
    pub fn relative(mut self) -> Self {
        self.placement.position = crate::layout::Position::Relative;
        self
    }

    /// Like `absolute`, but the containing block is always the root box.
    pub fn fixed(mut self) -> Self {
        self.placement.position = crate::layout::Position::Fixed;
        self
    }

    /// Declare the inset sides; an [`Insets::AUTO`] side is left to the solver.
    ///
    /// Pixel insets convert; a [`LenInsets`](crate::layout::LenInsets) may also carry
    /// percentages, resolved against the containing block.
    pub fn inset(mut self, inset: impl Into<crate::layout::LenInsets>) -> Self {
        self.placement.inset = inset.into();
        self
    }

    /// Stacking order among positioned siblings; higher paints later and hits first.
    pub fn z(mut self, z: i32) -> Self {
        self.placement.z = z;
        self
    }

    /// Anchor this node as an overlay: the engine places it from `placement`, paints it above
    /// the tree and lets it escape every clipping ancestor. Insets are ignored.
    pub fn overlay(mut self, placement: crate::layout::OverlayPlacement) -> Self {
        self.placement.position = crate::layout::Position::Absolute;
        self.placement.overlay = Some(placement);
        self
    }

    /// Lift this node: it keeps its layout slot but paints at the top layer, unclipped.
    pub fn lifted(mut self) -> Self {
        self.placement.lift = true;
        self
    }

    /// Place this item in its grid parent; spans default to one cell.
    pub fn grid_area(mut self, row: u32, col: u32, row_span: u32, col_span: u32) -> Self {
        self.grid.area = Some(crate::layout::GridArea {
            row,
            col,
            row_span: row_span.max(1),
            col_span: col_span.max(1),
        });
        self
    }

    /// Cells an auto-flowed item takes; the default is one.
    pub fn grid_span(mut self, row_span: u32, col_span: u32) -> Self {
        self.grid.span = (row_span.max(1), col_span.max(1));
        self
    }

    /// Cross-axis alignment: the cell's vertical axis, or the line's cross axis in a flow.
    pub fn align_self(mut self, align: crate::layout::Align) -> Self {
        self.grid.align = Some(align);
        self
    }

    /// Horizontal alignment inside a grid cell; a flow item ignores it.
    pub fn justify_self(mut self, align: crate::layout::Align) -> Self {
        self.grid.justify = Some(align);
        self
    }

    /// Give a grid container its track template.
    pub fn tracks(mut self, tracks: crate::layout::TrackList) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.tracks = Some(Box::new(tracks));
        }
        self
    }

    /// Displace this node visually without reflowing layout.
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = crate::geom::Point::new(x, y);
        self
    }

    /// Shape the pointer takes while this node is hovered.
    pub fn cursor(mut self, shape: crate::cursor::CursorShape) -> Self {
        self.cursor = Some(shape);
        self
    }

    /// Put this declaration's subtree inside an opaque environment.
    ///
    /// Children inherit it because the engine folds the nearest one down the tree.
    pub fn env(mut self, env: Arc<dyn std::any::Any + Send + Sync>) -> Self {
        self.env = Some(env);
        self
    }

    /// Subscribe to interactions; compile the set with `|` where the node is declared.
    pub fn sense(mut self, sense: crate::sense::Sense) -> Self {
        self.sense = sense;
        self
    }

    /// Declare the payload a drag that starts on this node carries.
    ///
    /// Also subscribes to [`Sense::DRAG`](crate::Sense::DRAG): a node with a payload is a
    /// drag source by construction, and asking for it twice is the easy mistake.
    pub fn drag_data(mut self, data: impl Into<Rc<crate::dnd::DragData>>) -> Self {
        self.drag = Some(data.into());
        self.sense |= crate::sense::Sense::DRAG;
        self
    }

    /// Begin an operating-system drag at the threshold instead of an in-app one, so the
    /// payload can be taken by another window. Meaningless without
    /// [`drag_data`](Self::drag_data).
    pub fn drag_out(mut self) -> Self {
        self.drag_out = true;
        self
    }

    /// Take drops whose offered type matches one of `types`; `text/*` and `*` are patterns.
    ///
    /// Also subscribes to [`Sense::DROP`](crate::Sense::DROP): declaring what you take is
    /// what makes a node a drop target.
    pub fn accept_drop(mut self, types: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.drop_types = Some(Rc::new(types.into_iter().map(Into::into).collect()));
        self.sense |= crate::sense::Sense::DROP;
        self
    }

    /// Make this node and its whole subtree inert.
    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }

    pub fn padding(mut self, p: Insets) -> Self {
        self.box_model.padding = p;
        self
    }

    /// Outside the border box; a non-finite side is `auto`, meaningful only out of flow.
    pub fn margin(mut self, m: Insets) -> Self {
        self.box_model.margin = m;
        self
    }

    /// The margin a flow parent uses when it stacked this child instead of lining it up.
    pub fn margin_alt(mut self, m: Insets) -> Self {
        self.box_model.margin_alt = Some(m);
        self
    }

    /// Make this `Row` stack along the column axis when its children no longer fit.
    pub fn switch_axis(mut self, switch: crate::layout::AxisSwitch) -> Self {
        if let Some(c) = self.data.params_mut() {
            c.switch_axis = Some(switch);
        }
        self
    }

    /// Cut the content at the box on both axes, and let the wheel scroll the main one.
    ///
    /// Sugar for main-axis [`Overflow::Scroll`] plus cross-axis `Hidden`: `Row` scrolls `x`,
    /// anything else `y`. Like CSS, the cross axis still clips.
    pub fn scrollable(mut self) -> Self {
        let horizontal = self.kind == NodeKind::Row;
        if let NodeData::Container(p) = &mut self.data {
            if horizontal {
                p.clip.x = crate::layout::Overflow::Scroll;
                p.clip.y = crate::layout::Overflow::Hidden;
            } else {
                p.clip.y = crate::layout::Overflow::Scroll;
                p.clip.x = crate::layout::Overflow::Hidden;
            }
        }
        self
    }

    /// Declare per-axis overflow policy; clipping is opt-in.
    pub fn clip(mut self, clip: crate::layout::Clip) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.clip = clip;
        }
        self
    }

    /// Scroll the content left by `x` logical units; negative scrolls right.
    pub fn scroll_x(mut self, x: f32) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.scroll_x = if x.is_finite() { x } else { 0.0 };
        }
        self
    }

    /// Scroll the content up by `y` logical units; negative scrolls down.
    pub fn scroll_y(mut self, y: f32) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.scroll_y = if y.is_finite() { y } else { 0.0 };
        }
        self
    }

    /// Come to rest on multiples of `page` logical units, per axis; zero leaves an axis free.
    ///
    /// The engine moves a snapping axis by whole pages: a wheel burst turns the page it
    /// travelled toward, and a release lands on the page nearest where the fling would stop.
    pub fn snap(mut self, page: [f32; 2]) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.snap = page.map(|step| if step.is_finite() { step.max(0.0) } else { 0.0 });
        }
        self
    }

    /// Paint this subtree at `scale` about its own top-left; layout keeps its size.
    ///
    /// A non-finite or negative value paints as laid out: a scale is a look, not a place, and
    /// nothing downstream should have to agree on what half a node means.
    pub fn scale(mut self, scale: f32) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.scale = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
        }
        self
    }

    /// Group opacity for the subtree, clamped to 0..=1.
    pub fn opacity(mut self, o: f32) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.opacity = if o.is_finite() {
                o.clamp(0.0, 1.0)
            } else {
                1.0
            };
        }
        self
    }

    /// Travel to a new *fixed* size instead of jumping; the engine interpolates before layout.
    pub fn transition_size(mut self, spec: crate::anim::AnimSpec) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.size_transition = Some(spec);
        }
        self
    }

    /// Paint this node from `enter`'s values up to its declared ones when it first appears.
    pub fn enter(mut self, enter: Presence) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.enter = Some(enter);
        }
        self
    }

    /// Keep this node alive and painted while it travels to `exit` after it is undeclared.
    pub fn exit(mut self, exit: Presence) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.exit = Some(exit);
        }
        self
    }

    /// What the node's slot does while it leaves: kept, collapsed, or gone at once.
    pub fn exit_space(mut self, space: ExitSpace) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.exit_space = space;
        }
        self
    }

    /// Keep this subtree's pixels in a target of its own across frames; it repaints only
    /// when a change lands inside it.
    pub fn offscreen(mut self) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.offscreen = true;
        }
        self
    }

    /// Splat press ink where a press lands on this node or its descendants.
    pub fn press_ink(mut self, ink: Option<crate::draw::PressInk>) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.press_ink = ink;
        }
        self
    }

    /// Ask the engine to draw a focus adorner around this node while it holds visible focus.
    pub fn focus_visual(mut self, visual: Option<crate::draw::FocusVisual>) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.focus_visual = visual;
        }
        self
    }

    /// Declare this node a Tab stop although its ring is painted by a child.
    pub fn focus_stop(mut self, stop: bool) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.focus_stop = stop;
        }
        self
    }

    /// Name the descendant a popup should focus when it opens, instead of its first stop.
    pub fn focus_initial(mut self, id: crate::Id) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.focus_initial = Some(id);
        }
        self
    }

    /// State this node's own curve, which then outranks whatever it paints this frame.
    pub fn corners(mut self, corners: crate::geom::Corners) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.corners = Some(corners);
        }
        self
    }

    /// Lay this text out on one line, overflowing its box instead of wrapping.
    pub fn nowrap(mut self) -> Self {
        if let NodeData::Text { nowrap, .. } = &mut self.data {
            *nowrap = true;
        }
        self
    }

    /// Make this node a modal surface over `target`, whatever position the tree gives it.
    pub fn modal(mut self, target: ModalTarget) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.modal = Some(target);
        }
        self
    }

    /// Let a `Proximate` modal resolve to this container.
    pub fn modal_host(mut self) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.modal_host = true;
        }
        self
    }

    /// The title this node's window carries when it is presented as one.
    pub fn window_title(mut self, title: impl Into<String>) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.window_title = Some(title.into());
        }
        self
    }

    /// Bind a chord on this node; the engine reports `Change::Shortcut` when it fires.
    pub fn shortcut(
        mut self,
        chord: crate::shortcut::KeyChord,
        id: crate::shortcut::ShortcutId,
    ) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.shortcuts.push((chord, id));
        }
        self
    }

    /// Rows this node offers on a secondary click; the engine shows them and reports the pick.
    pub fn menu(mut self, items: Vec<crate::menu::MenuItem>) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.menu = items;
        }
        self
    }

    /// Make this node a text input holding `value`; the engine reports edits, the app keeps
    /// the value and passes the new one back next frame. The buffer lives on a container's
    /// parameters, so a text leaf cannot be the input: it is one of the input's views.
    pub fn text_input(mut self, value: impl Into<Arc<str>>) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.text_input = Some(value.into());
        }
        self
    }

    /// Declare this input's `Shift+Enter` as a hard line break; without it the chord
    /// activates, like Enter does. Only an input with a buffer is affected.
    pub fn text_input_multiline(mut self, multiline: bool) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.text_input_multiline = multiline;
        }
        self
    }

    /// Let a plain `Enter` insert a line break too, the way a document editor splits its
    /// text. Implies multiline for the break itself.
    pub fn text_input_enter_newline(mut self) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.text_input_multiline = true;
            p.text_input_enter_newline = true;
        }
        self
    }

    /// Mark this input's buffer as secret. The control still draws the marks; the engine's
    /// half is what stops a copy and keeps the input method from learning the value.
    pub fn text_input_hidden(mut self) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.text_input_hidden = true;
        }
        self
    }

    /// Tell the input method what the field is for; a secret buffer keeps its own purpose.
    pub fn text_input_purpose(mut self, purpose: crate::ime::ContentPurpose) -> Self {
        if let Some(p) = self.data.params_mut() {
            p.text_input_purpose = purpose;
        }
        self
    }

    /// Effect chain for the whole subtree; any effect forces an offscreen layer.
    /// Fill colour of the node's own box shape, if it has one.
    pub fn fill(&self) -> Option<crate::geom::Color> {
        self.shapes.iter().find_map(|s| match s {
            crate::draw::Shape::Box { fill, .. } => Some(*fill),
            _ => None,
        })
    }

    /// The node's own box shape, created on first use and always painted first.
    fn box_shape(&mut self) -> &mut crate::draw::Shape {
        let at = self
            .shapes
            .iter()
            .position(|s| matches!(s, crate::draw::Shape::Box { .. }));
        let at = at.unwrap_or_else(|| {
            self.shapes.insert(
                0,
                crate::draw::Shape::Box {
                    corners: Corners::ZERO,
                    fill: crate::geom::Color::TRANSPARENT,
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                },
            );
            0
        });
        &mut self.shapes[at]
    }

    /// Attach paint-only shapes, appended in paint order; the box shape always stays first.
    pub fn shapes(mut self, shapes: Vec<crate::draw::Shape>) -> Self {
        self.shapes.extend(shapes);
        self
    }

    pub fn effects(mut self, chain: Vec<crate::effect::Effect>) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.effects = chain;
        }
        self
    }

    pub fn blur(self, radius: u32, sigma: f32) -> Self {
        self.effects(vec![crate::effect::Effect::blur(radius, sigma)])
    }

    /// Blur what lies behind this node, so its translucent fill reads as frosted glass.
    pub fn backdrop_blur(mut self, sigma: f32) -> Self {
        if let NodeData::Container(p) = &mut self.data {
            p.backdrop_blur = Some(BackdropBlur::css(sigma));
        }
        self
    }

    /// Cast a soft shadow; `level` follows WinUI's elevation ramp.
    pub fn elevation(mut self, level: u8) -> Self {
        if let crate::draw::Shape::Box { shadow, .. } = self.box_shape() {
            *shadow = Some(ShadowSpec::elevation(level));
        }
        self
    }

    /// Draw a registered texture into this node's box, under its children.
    pub fn background_image(mut self, id: u64, fit: ImageFit) -> Self {
        if let crate::draw::Shape::Box { image, .. } = self.box_shape() {
            *image = Some(ImageSpec {
                id,
                fit,
                sampling: crate::draw::Sampling::Smooth,
            });
        }
        self
    }

    /// Read the background texture's texels as blocks rather than blending between them.
    pub fn image_sampling(mut self, sampling: crate::draw::Sampling) -> Self {
        if let crate::draw::Shape::Box {
            image: Some(spec), ..
        } = self.box_shape()
        {
            spec.sampling = sampling;
        }
        self
    }

    /// Name the material that shades this node's blurred backdrop.
    pub fn backdrop_material(mut self, material: crate::draw::BackdropMaterial) -> Self {
        if let NodeData::Container(p) = &mut self.data
            && let Some(blur) = p.backdrop_blur.as_mut()
        {
            blur.material = material;
        }
        self
    }

    pub fn gap(mut self, g: f32) -> Self {
        if let Some(c) = self.data.params_mut() {
            c.gap = g;
        }
        self
    }

    /// How much cross-axis space this container hands each child: 0 hugs, 1 stretches.
    pub fn cross_align(mut self, a: f32) -> Self {
        if let Some(c) = self.data.params_mut() {
            c.cross_align = a;
        }
        self
    }

    /// Where this container parks the children in their cross-axis slack; 0.5 centres.
    pub fn cross_pos(mut self, a: f32) -> Self {
        if let Some(c) = self.data.params_mut() {
            c.cross_pos = a;
        }
        self
    }

    /// Park every child of a `Row` on one shared baseline, whatever its size.
    ///
    /// This replaces `cross_align`/`cross_pos`: a child keeps its own cross size and moves
    /// until its baseline meets the line's.
    pub fn cross_baseline(mut self) -> Self {
        if let Some(c) = self.data.params_mut() {
            c.cross_baseline = true;
        }
        self
    }

    /// Centre a `Stack`'s children in its content box on both axes: how an icon sits in a
    /// slot larger than itself, without padding arithmetic.
    pub fn stack_align(mut self, align: crate::layout::OverlayAlign) -> Self {
        if let Some(c) = self.data.params_mut() {
            c.stack_align = (align, align);
        }
        self
    }

    /// The same per axis: a bar sits at the bottom centre without offset arithmetic.
    pub fn stack_align_axes(
        mut self,
        x: crate::layout::OverlayAlign,
        y: crate::layout::OverlayAlign,
    ) -> Self {
        if let Some(c) = self.data.params_mut() {
            c.stack_align = (x, y);
        }
        self
    }

    /// Give a container a background box.
    /// Attach a gradient to this box; the solid fill stays as the fallback.
    pub fn gradient(
        mut self,
        from: crate::geom::Color,
        to: crate::geom::Color,
        axis: crate::draw::GradientAxis,
    ) -> Self {
        if let crate::draw::Shape::Box { gradient, .. } = self.box_shape() {
            *gradient = Some(crate::draw::Gradient::new(from, to, axis));
        }
        self
    }

    pub fn background(mut self, fill: crate::geom::Color, radius: Corners) -> Self {
        if let crate::draw::Shape::Box {
            fill: f,
            corners: c,
            ..
        } = self.box_shape()
        {
            *f = fill;
            *c = radius;
        }
        self
    }

    /// Cast this node's own box up: the shadow an elevated surface sits on.
    ///
    /// Only the box's shape carries it, so a node with no background casts nothing.
    pub fn shadow(mut self, spec: crate::tree::ShadowSpec) -> Self {
        if let crate::draw::Shape::Box { shadow, .. } = self.box_shape() {
            *shadow = Some(spec);
        }
        self
    }

    /// Draw the stroke as a gradient across the box; the near stop is also its fallback.
    pub fn border_gradient(mut self, gradient: crate::draw::Gradient, width: f32) -> Self {
        let corners = match self.box_shape() {
            crate::draw::Shape::Box { corners, .. } => *corners,
            _ => Corners::ZERO,
        };
        self.shapes.push(crate::draw::Shape::Outline {
            corners,
            colour: gradient.from,
            gradient: Some(gradient),
            width,
            gap: None,
        });
        self
    }

    /// Add a border to the background box.
    pub fn border(mut self, color: crate::geom::Color, width: f32) -> Self {
        if let crate::draw::Shape::Box { border, .. } = self.box_shape() {
            *border = Some((color, width));
        }
        self
    }

    pub fn ink(mut self, m: Insets) -> Self {
        self.ink_margin = m;
        self
    }

    pub fn size(mut self, sc: SizeClass) -> Self {
        self.size_class = sc;
        self
    }

    pub fn child(mut self, c: impl Into<Rc<Decl>>) -> Self {
        self.children.push(c.into());
        self
    }

    pub fn children(mut self, cs: impl IntoIterator<Item = impl Into<Rc<Decl>>>) -> Self {
        self.children.extend(cs.into_iter().map(Into::into));
        self
    }

    /// The node id a named slot takes under this container.
    pub fn slot_id(&self, name: crate::slot::SlotName) -> Id {
        crate::slot::slot_id(self.id, name)
    }

    /// Fill one named slot with a bare node; a second fill of the same name replaces it.
    pub fn slot(self, name: crate::slot::SlotName, content: impl Into<Rc<Decl>>) -> Self {
        self.slot_with(
            name,
            Decl::container(Id::default(), NodeKind::Stack),
            content,
        )
    }

    /// Fill one named slot inside `frame`, the container's own wrapper.
    ///
    /// The frame's id is replaced by the derived slot id, so a slot's identity never follows
    /// its content; the content is appended inside the frame's own children.
    pub fn slot_with(
        mut self,
        name: crate::slot::SlotName,
        mut frame: Decl,
        content: impl Into<Rc<Decl>>,
    ) -> Self {
        frame.id = self.slot_id(name);
        frame.children.push(content.into());
        let at = self.children.iter().position(|c| c.id == frame.id);
        let frame = Rc::new(frame);
        match at {
            Some(i) => self.children[i] = frame,
            None => self.children.push(frame),
        }
        self
    }
}

/// Difference produced by applying a declaration tree.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ApplyStats {
    pub created: usize,
    pub reused: usize,
    pub retired: usize,
}

/// One parent's children: declaration order for painting, and a map for sibling lookup.
#[derive(Debug, Default)]
struct Siblings {
    /// Sorted by each node's declaration position at the end of every apply.
    order: Vec<NodeId>,
    /// The same children by `(z, declaration order)`, sorted once per apply so the walks,
    /// hit rebuild and paint can borrow the answer instead of sorting a copy each.
    paint: Vec<NodeId>,
    by_id: FastMap<Id, NodeId>,
}

/// Effective stacking order among siblings: only a positioned node carries its `z`.
fn z_of(nodes: &[Option<Node>], nid: NodeId) -> i32 {
    nodes
        .get(nid.index())
        .and_then(|slot| slot.as_ref())
        .map(|n| {
            if n.placement.position == crate::layout::Position::Static {
                0
            } else {
                n.placement.z
            }
        })
        .unwrap_or(0)
}

/// The retained tree.
#[derive(Debug, Default)]
pub struct Tree {
    nodes: Vec<Option<Node>>,
    /// Generation per slot; bumped on retire so stale handles read as absent.
    generations: Vec<u32>,
    free: Vec<u32>,
    live: Vec<NodeId>,
    /// `Id` -> node, scoped per parent to keep sibling lookups cheap.
    by_parent: FastMap<Option<NodeId>, Siblings>,
    /// `Id` -> node over the whole tree, rebuilt once per apply: the general lookup runs
    /// for every element every frame, so it must not scan the sibling maps.
    by_id: FastMap<Id, NodeId>,
    root: Option<NodeId>,
    /// Marks the whole tree as needing layout.
    layout_dirty: bool,
    /// Nodes whose values the engine interpolates this frame, in apply order.
    animated: Vec<NodeId>,
    /// Nodes that appeared this frame declaring `enter`, in apply order.
    appeared: Vec<NodeId>,
    /// Roots of subtrees kept alive past their declaration so they can leave visibly.
    held: Vec<NodeId>,
    /// Panels the last layout pass measured for a surface of the platform's own.
    platform_panels: Vec<crate::layout::PlatformPanel>,
    /// Whether the window this tree is laid out for has a surface to show a panel in.
    panel_surfaces: bool,
    /// The output a platform panel must fit, in this tree's own coordinates.
    output_bounds: Option<crate::geom::Rect>,
}

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Nodes whose time-dependent values the engine drives this frame.
    pub fn animated(&self) -> &[NodeId] {
        &self.animated
    }

    /// Nodes that appeared this frame with an `enter` declaration.
    pub fn appeared(&self) -> &[NodeId] {
        &self.appeared
    }

    /// Subtrees the engine is keeping alive while they leave, painted above the content.
    pub fn held(&self) -> &[NodeId] {
        &self.held
    }

    /// Panels the last layout pass laid out for the platform to present.
    pub fn platform_panels(&self) -> &[crate::layout::PlatformPanel] {
        &self.platform_panels
    }

    /// Whether the window has a surface a panel of its own may be shown in.
    ///
    /// False until a host says otherwise, so a loop with no host paints its panels in the
    /// window: the declaration is a request, and a request nobody can answer must not leave
    /// the panel unpainted.
    pub(crate) fn panel_surfaces(&self) -> bool {
        self.panel_surfaces
    }

    /// Say whether the window this tree is laid out for presents panels.
    pub fn set_panel_surfaces(&mut self, presents: bool) {
        self.panel_surfaces = presents;
    }

    /// The output a platform panel must stay inside, in this tree's coordinates.
    ///
    /// A surface presents a panel where the tree placed it, so the tree is the one that
    /// constrains the panel: a surface moved after layout would map the input its grab
    /// routes back into the tree to the wrong places.
    pub fn output_bounds(&self) -> Option<crate::geom::Rect> {
        self.output_bounds
    }

    /// Name the output the platform panels are placed against; `None` leaves them at their
    /// preferred spot. A change is a layout change, so the next pass re-places them.
    pub fn set_output_bounds(&mut self, bounds: Option<crate::geom::Rect>) {
        if self.output_bounds != bounds {
            self.output_bounds = bounds;
            self.layout_dirty = true;
        }
    }

    /// The layout pass is about to record this frame's platform panels.
    pub(crate) fn clear_platform_panels(&mut self) {
        self.platform_panels.clear();
    }

    pub(crate) fn record_platform_panel(&mut self, panel: crate::layout::PlatformPanel) {
        self.platform_panels.push(panel);
    }

    /// Whether anything is painting at a scale other than 1: a presence animation or a
    /// declared paint scale.
    pub fn paints_scaled(&self) -> bool {
        self.live.iter().any(|nid| {
            self.slot(*nid)
                .is_some_and(|n| (n.presence_scale - 1.0).abs() > 1e-6 || declared_scale(n) != 1.0)
        })
    }

    /// The rect this node is painted in, after every scale above and on it.
    pub fn scaled_visual_rect(&self, nid: NodeId) -> Rect {
        self.scaled(nid, |n| n.visual_rect())
    }

    /// The box this node's ink occupies, after every scale above and on it. A scaled
    /// subtree paints its shapes outside the rect layout gave it.
    pub fn scaled_ink_rect(&self, nid: NodeId) -> Rect {
        self.scaled(nid, |n| n.ink_rect())
    }

    /// The box this node's focus adorner paints; `None` when it declares none.
    ///
    /// XAML writes a negative margin when the ring reaches outside the control, so this box
    /// is not the node's rect. Whether the ring is showing is the caller's question.
    pub fn scaled_adorner_rect(&self, nid: NodeId) -> Option<Rect> {
        let visual = self.get(nid)?.data.params()?.focus_visual?;
        Some(self.scaled(nid, |n| {
            let r = n.visual_rect();
            Rect::new(
                r.origin.x + visual.margin.left,
                r.origin.y + visual.margin.top,
                r.size.w - visual.margin.left - visual.margin.right,
                r.size.h - visual.margin.top - visual.margin.bottom,
            )
        }))
    }

    /// Apply the scales from `nid` up to the root to the rect `base` picks.
    fn scaled(&self, nid: NodeId, base: impl Fn(&Node) -> Rect) -> Rect {
        let Some(node) = self.get(nid) else {
            return Rect::ZERO;
        };
        // In application order, innermost first, exactly as paint composes them: a node's
        // declared scale is about its own top-left and its presence scale about its centre.
        let mut transforms = Vec::new();
        let mut current = Some(nid);
        while let Some(p) = current {
            let Some(step) = self.slot(p) else {
                break;
            };
            let declared = declared_scale(step);
            if declared != 1.0 {
                transforms.push(crate::draw::Transform::about(
                    step.visual_rect().origin,
                    declared,
                ));
            }
            if (step.presence_scale - 1.0).abs() > 1e-6 {
                transforms.push(crate::draw::Transform::about(
                    step.visual_rect().center(),
                    step.presence_scale,
                ));
            }
            current = step.parent;
        }
        let mut rect = base(node);
        for transform in transforms {
            rect = transform.apply_rect(rect);
        }
        rect
    }

    /// Drop a held subtree; the exit has landed and nothing of it should remain.
    pub fn release(&mut self, nid: NodeId) {
        self.held.retain(|n| *n != nid);
        let parent = self.get(nid).and_then(|n| n.parent);
        self.retire_subtree(nid);
        if let Some(p) = parent {
            self.mark_dirty_up(p);
        }
        self.layout_dirty = true;
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Fetch a node, or `None` for a stale handle.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes
            .get(id.index())
            .and_then(|s| s.as_ref())
            .filter(|n| n.id != Id::default() || id.generation() == 0)
            .or_else(|| self.slot(id))
    }

    fn slot(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index()).and_then(|s| s.as_ref())
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.index()).and_then(|s| s.as_mut())
    }

    pub fn find_by_id(&self, id: Id) -> Option<NodeId> {
        self.by_id.get(&id).copied()
    }

    pub fn children_of(&self, parent: Option<NodeId>) -> &[NodeId] {
        // Already in declaration order: `rebuild_index` sorts it once per apply. A child added
        // later would otherwise take a later arena slot and paint above its earlier siblings.
        self.by_parent
            .get(&parent)
            .map(|s| s.order.as_slice())
            .unwrap_or(&[])
    }

    /// One child by position, for callers that must mutate the tree while walking it.
    pub fn child_at(&self, parent: Option<NodeId>, index: usize) -> Option<NodeId> {
        self.by_parent
            .get(&parent)
            .and_then(|s| s.order.get(index).copied())
    }

    pub fn mark_dirty(&mut self, id: NodeId) {
        self.mark_dirty_up(id);
    }

    /// Mark `id` and every ancestor dirty: a cached measurement only holds while its whole
    /// subtree is unchanged, so a leaf edit invalidates the chain above it.
    pub fn mark_dirty_up(&mut self, id: NodeId) {
        self.layout_dirty = true;
        let mut cur = Some(id);
        while let Some(nid) = cur {
            cur = match self.get_mut(nid) {
                Some(n) => {
                    n.dirty = true;
                    n.parent
                }
                None => None,
            };
        }
    }

    /// Replace the tree contents with `decl`, reusing nodes whose `Id` matches.
    pub fn apply(&mut self, decl: &Decl) -> ApplyStats {
        let mut stats = ApplyStats::default();
        self.animated.clear();
        self.appeared.clear();
        // Mark everything as "not seen yet"; anything left unseen is retired.
        let live_snapshot = self.live.clone();
        for nid in live_snapshot {
            if let Some(n) = self.get_mut(nid) {
                n.reused = false;
            }
        }
        let root = self.apply_decl(decl, None, None, 0, &mut stats);
        self.root = Some(root);
        // Everything built while applying this tree was built under exactly one scope, the
        // one the caller handed to its build context. A declaration that opened a narrower
        // scope did so after its contents were already resolved, so the two cannot disagree.
        // Retire unseen nodes (their ids disappeared from the declaration).
        let unseen: Vec<NodeId> = self
            .live
            .iter()
            .cloned()
            .filter(|nid| {
                self.slot(*nid)
                    .map(|n| !n.reused && !n.held)
                    .unwrap_or(false)
            })
            .collect();
        for nid in unseen {
            // An ancestor that is already leaving holds this node too; retiring it would
            // tear a hole in the subtree that is still being painted.
            if self.slot(nid).is_some_and(|n| n.held) {
                continue;
            }
            // A removed child changes what its parent measures, so read the parent first.
            let parent = self.get(nid).and_then(|n| n.parent);
            // A node that declares an exit is kept alive and painted until it lands.
            if self.hold(nid) {
                if let Some(p) = parent {
                    self.mark_dirty_up(p);
                }
                self.layout_dirty = true;
                continue;
            }
            stats.retired += self.retire_subtree(nid);
            if let Some(p) = parent {
                self.mark_dirty_up(p);
            }
        }

        // Normalise the sibling map to only live nodes.
        self.rebuild_index();
        self.detach_held();

        if stats.created > 0 || stats.retired > 0 {
            self.layout_dirty = true;
        }
        stats
    }

    fn apply_decl(
        &mut self,
        decl: &Decl,
        src: Option<&Rc<Decl>>,
        parent: Option<NodeId>,
        order: u32,
        stats: &mut ApplyStats,
    ) -> NodeId {
        let existing = self
            .by_parent
            .get(&parent)
            .and_then(|s| s.by_id.get(&decl.id))
            .copied();
        // Same `Rc` as last frame means the payload cannot have changed.
        let same_src = existing.is_some_and(|nid| {
            src.is_some_and(|rc| {
                self.slot(nid)
                    .and_then(|n| n.src.as_ref())
                    .is_some_and(|s| Rc::ptr_eq(s, rc))
            })
        });
        // The scope in force here: this declaration's own, else the parent's. Folding it
        // during apply keeps it available before layout, and parents are visited first.
        let inherited_env = parent.and_then(|p| self.get(p)).and_then(|n| n.env.clone());

        // Set when this node's own payload changed, so the ancestors' caches go with it.
        let mut changed_here = false;
        // Set when the reuse path changed the node's own payload or its declared
        // displacement; applied after the node borrow ends.
        let mut payload_moved = false;
        let mut offset_moved = false;
        let nid = match existing {
            Some(nid) => {
                let kind_changed = self.slot(nid).map(|n| n.kind != decl.kind).unwrap_or(true);
                if kind_changed {
                    // Kind change discards the subtree; identity no longer means anything.
                    self.retire_subtree(nid);
                    stats.created += 1;
                    let spawned = self.spawn(decl, parent);
                    if let Some(env) = decl.env.clone().or_else(|| inherited_env.clone())
                        && let Some(n) = self.get_mut(spawned)
                    {
                        n.env = Some(env);
                    }
                    spawned
                } else {
                    // Read the inherited flag before taking the mutable borrow of the node.
                    let inherited_disabled = parent
                        .and_then(|p| self.get(p))
                        .map(|p| p.disabled)
                        .unwrap_or(false);
                    let n = self.get_mut(nid).expect("live node");
                    n.reused = true;
                    n.parent = parent;
                    n.ink_margin = decl.ink_margin;
                    n.cursor = decl.cursor;
                    // A node property, copied every frame like `cursor`: it follows the
                    // declaration instead of sticking to whatever it was first set to.
                    n.sense = decl.sense;
                    n.drag = decl.drag.clone();
                    n.drag_out = decl.drag_out;
                    n.drop_types = decl.drop_types.clone();
                    // Same folding on the reuse path: inherited disabled wins over own false.
                    n.disabled = decl.disabled || inherited_disabled;
                    // `None` means "inherit": the folded value is only replaced when this
                    // declaration actually opens a scope.
                    n.env = decl.env.clone().or_else(|| inherited_env.clone());
                    // The declared size is always the target, whatever a transition left in
                    // `size_class`; a cached subtree keeps the one it was applied with.
                    n.declared_size = decl.size_class;
                    if !same_src {
                        let changed = n.data != decl.data
                            || n.size_class != decl.size_class
                            || n.grid != decl.grid
                            || n.placement != decl.placement;
                        n.data = decl.data.clone();
                        n.size_class = decl.size_class;
                        n.grid = decl.grid;
                        n.placement = decl.placement;
                        n.box_model = decl.box_model;
                        if changed {
                            n.dirty = true;
                            payload_moved = true;
                        }
                    }
                    // Only the declared displacement lands here: `accumulate_offsets` folds
                    // it with the ancestors' afresh, so the folded copy may lag until the
                    // next pass but can never compound.
                    offset_moved = n.local_offset != decl.offset;
                    if offset_moved {
                        n.local_offset = decl.offset;
                        n.dirty = true;
                    }
                    stats.reused += 1;
                    nid
                }
            }
            None => {
                stats.created += 1;
                let spawned = self.spawn(decl, parent);
                if let Some(env) = decl.env.clone().or_else(|| inherited_env.clone())
                    && let Some(n) = self.get_mut(spawned)
                {
                    n.env = Some(env);
                }
                spawned
            }
        };
        if payload_moved || offset_moved {
            self.layout_dirty = true;
            changed_here = true;
        }

        // Time-dependent nodes are collected even when the declaration is cached: a subtree
        // that is skipped whole would otherwise stop driving its own transition.
        let entering = self.slot(nid).is_some_and(|n| n.presence < 1.0);
        if decl
            .data
            .params()
            .is_some_and(|p| p.size_transition.is_some() || (p.enter.is_some() && entering))
        {
            self.animated.push(nid);
        }

        // The same declaration object in the same place: nothing under it can have changed,
        // so the whole subtree is seen and the walk stops here.
        if same_src
            && self
                .slot(nid)
                .is_some_and(|n| n.parent == parent && n.order == order)
        {
            self.mark_reused_subtree(nid);
            return nid;
        }

        // Siblings are keyed by `Id`, so a duplicate silently overwrites its twin: the
        // second declaration reuses the first node and the first one's content is lost.
        #[cfg(debug_assertions)]
        {
            use crate::collections::FastSet;
            let mut seen =
                FastSet::with_capacity_and_hasher(decl.children.len(), Default::default());
            for child in decl.children.iter() {
                assert!(
                    seen.insert(child.id),
                    "duplicate sibling Id {:?} under {:?}: the tree keys by Id, so one node \
                     silently replaces the other; give repeated items derived ids (Id::with / Id::of)",
                    child.id,
                    decl.id
                );
                assert!(
                    child.id != decl.id,
                    "a child reuses its parent's Id {:?}: the tree keys by Id, so the child's \
                     declaration collapses into the wrapper's; give the wrapper its own id",
                    decl.id
                );
            }
        }

        // Paint-only shapes and the sibling position come straight from the declaration.
        if let Some(n) = self.get_mut(nid) {
            // A sibling that moved takes a different slot in the flow, so it is a change.
            if n.order != order {
                n.order = order;
                changed_here = true;
            }
            n.src = src.cloned();
            if !same_src && n.shapes != decl.shapes {
                n.shapes = decl.shapes.clone();
            }
        }
        if changed_here {
            self.mark_dirty_up(nid);
        }
        for (i, child) in decl.children.iter().enumerate() {
            self.apply_decl(child, Some(child), Some(nid), i as u32, stats);
        }
        nid
    }

    /// Mark a subtree as still present this frame, without rebuilding any of it.
    ///
    /// Recursion depth is the tree's depth; no allocation, and the nodes' content is
    /// already correct because the declaration they came from is the same object.
    fn mark_reused_subtree(&mut self, nid: NodeId) {
        let mut animated = false;
        if let Some(n) = self.get_mut(nid) {
            n.reused = true;
            let settled = n.presence >= 1.0;
            animated = n
                .data
                .params()
                .is_some_and(|p| p.size_transition.is_some() || (p.enter.is_some() && !settled));
        }
        if animated {
            self.animated.push(nid);
        }
        let count = self.children_of(Some(nid)).len();
        for i in 0..count {
            if let Some(kid) = self.child_at(Some(nid), i) {
                self.mark_reused_subtree(kid);
            }
        }
    }

    fn spawn(&mut self, decl: &Decl, parent: Option<NodeId>) -> NodeId {
        let index = match self.free.pop() {
            Some(i) => i,
            None => {
                self.nodes.push(None);
                self.generations.push(0);
                (self.nodes.len() - 1) as u32
            }
        };
        let generation = *self.generations.get(index as usize).unwrap_or(&0);
        // A slot handed out while still occupied would overwrite a live node, and the tree
        // would silently lose it; `free` must never hold an index twice.
        debug_assert!(
            self.nodes
                .get(index as usize)
                .map(Option::is_none)
                .unwrap_or(true),
            "free list handed out a live slot"
        );
        let nid = NodeId { index, generation };
        let entering = decl.data.params().is_some_and(|p| p.enter.is_some());
        self.nodes[index as usize] = Some(Node {
            id: decl.id,
            parent,
            kind: decl.kind,
            data: decl.data.clone(),
            shapes: decl.shapes.clone(),
            order: 0,
            size_class: decl.size_class,
            declared_size: decl.size_class,
            box_model: decl.box_model,
            min_content: Size::ZERO,
            max_content: Size::ZERO,
            scroll_extent: None,
            rect: Rect::ZERO,
            flow_vertical: false,
            offset: decl.offset,
            local_offset: decl.offset,
            // A node declaring `enter` is born at its start point, not at its final look.
            presence: if entering { 0.0 } else { 1.0 },
            presence_alpha: 1.0,
            presence_shift: crate::geom::Point::ZERO,
            presence_scale: 1.0,
            cursor: decl.cursor,
            sense: decl.sense,
            drag: decl.drag.clone(),
            drag_out: decl.drag_out,
            drop_types: decl.drop_types.clone(),
            // A disabled ancestor disables everything below it, so the flag is folded here.
            disabled: decl.disabled
                || parent
                    .and_then(|p| self.get(p))
                    .map(|p| p.disabled)
                    .unwrap_or(false),
            grid: decl.grid,
            placement: decl.placement,
            // The node starts with its own declaration; `apply` folds the inherited one in.
            env: decl.env.clone(),
            mask: Rect::ZERO,
            focus_mask: Rect::ZERO,
            ink_margin: decl.ink_margin,
            wrap_width: f32::INFINITY,
            inline: None,
            baseline: 0.0,
            dirty: true,
            last_measure: None,
            reused: true,
            held: false,
            exit_size: crate::geom::Size::ZERO,
            src: None,
        });
        self.live.push(nid);
        if entering {
            self.appeared.push(nid);
        }
        let sibs = self.by_parent.entry(parent).or_default();
        sibs.by_id.insert(decl.id, nid);
        sibs.order.push(nid);
        self.layout_dirty = true;
        // A new child changes what its ancestors measure, so their caches are stale too.
        if let Some(p) = parent {
            self.mark_dirty_up(p);
        }
        nid
    }

    /// Retire `nid` and everything below it; returns how many nodes were still alive.
    fn retire_subtree(&mut self, nid: NodeId) -> usize {
        let mut retired = 0;
        let mut stack = vec![nid];
        while let Some(cur) = stack.pop() {
            // A subtree removal also lists the descendants an ancestor already freed, and a
            // stale handle may name a slot that has since been handed out again. Freeing
            // either would put one index in `free` twice, and two nodes would share it.
            if self.generations.get(cur.index()).copied() != Some(cur.generation()) {
                continue;
            }
            stack.extend_from_slice(self.children_of(Some(cur)));
            self.by_parent.remove(&Some(cur));
            let Some(node) = self.nodes.get_mut(cur.index()).and_then(|s| s.take()) else {
                continue;
            };
            if let Some(sibs) = self.by_parent.get_mut(&node.parent) {
                sibs.by_id.remove(&node.id);
                sibs.order.retain(|n| *n != cur);
            }
            self.live.retain(|n| *n != cur);
            self.free.push(cur.index() as u32);
            if let Some(g) = self.generations.get_mut(cur.index()) {
                *g = g.wrapping_add(1);
            }
            retired += 1;
        }
        retired
    }

    /// Keep an undeclared subtree alive for its exit; false when it has none.
    fn hold(&mut self, nid: NodeId) -> bool {
        let declared = self
            .slot(nid)
            .and_then(|n| Some((n.held, n.data.params()?.exit.is_some())));
        let Some((already_held, has_exit)) = declared else {
            return false;
        };
        if already_held || !has_exit {
            return false;
        }
        let mut stack = vec![nid];
        while let Some(cur) = stack.pop() {
            stack.extend_from_slice(self.children_of(Some(cur)));
            if let Some(n) = self.get_mut(cur) {
                n.held = true;
                n.exit_size = n.rect.size;
                // Leaving starts from fully present; the first exit leg rewrites these.
                n.presence = 1.0;
                n.presence_alpha = 1.0;
                n.presence_shift = crate::geom::Point::ZERO;
                n.presence_scale = 1.0;
            }
        }
        self.held.push(nid);
        true
    }

    /// Take held subtrees out of every sibling list and the id map `rebuild_index` refilled.
    fn detach_held(&mut self) {
        if self.held.is_empty() {
            return;
        }
        let roots = self.held.clone();
        let mut ghosts: Vec<(Id, NodeId)> = Vec::new();
        for root in roots {
            let Some((parent, id)) = self.slot(root).map(|n| (n.parent, n.id)) else {
                continue;
            };
            // A kept or collapsing slot stays in the flow so its siblings do not reflow on
            // the first exit frame; only the paint and id tables drop it.
            let space = self
                .slot(root)
                .and_then(|n| n.data.params())
                .map(|p| p.exit_space)
                .unwrap_or_default();
            if let Some(sibs) = self.by_parent.get_mut(&parent) {
                if space == ExitSpace::OutOfFlow {
                    sibs.order.retain(|n| *n != root);
                }
                // Paint reads its own sorted copy; leaving the root there paints it twice,
                // once from the parent and once from `held()`, doubling a translucent alpha.
                sibs.paint.retain(|n| *n != root);
                if sibs.by_id.get(&id) == Some(&root) {
                    sibs.by_id.remove(&id);
                }
            }
            let mut stack = vec![root];
            while let Some(cur) = stack.pop() {
                stack.extend_from_slice(self.children_of(Some(cur)));
                if let Some(n) = self.slot(cur) {
                    ghosts.push((n.id, cur));
                }
            }
        }
        // A reopened surface must find the live node, never the one that is leaving.
        for (id, nid) in ghosts {
            if self.by_id.get(&id) == Some(&nid) {
                self.by_id.remove(&id);
            }
        }
    }

    fn rebuild_index(&mut self) {
        for sibs in self.by_parent.values_mut() {
            sibs.order.clear();
            sibs.by_id.clear();
        }
        self.by_id.clear();
        let live = self.live.clone();
        self.by_id.reserve(live.len());
        for nid in live {
            if let Some(n) = self.slot(nid) {
                let (parent, id) = (n.parent, n.id);
                let sibs = self.by_parent.entry(parent).or_default();
                sibs.by_id.insert(id, nid);
                sibs.order.push(nid);
                self.by_id.insert(id, nid);
            }
        }
        let nodes = &self.nodes;
        for sibs in self.by_parent.values_mut() {
            sibs.order.sort_by_key(|n| {
                let order = nodes[n.index()].as_ref().map(|x| x.order).unwrap_or(0);
                (order, n.index())
            });
            // Paint order is a second view of the same children, sorted here once per apply:
            // walks, hit rebuild and paint then borrow it instead of copying and sorting it.
            let mut paint = std::mem::take(&mut sibs.paint);
            paint.clear();
            paint.extend_from_slice(&sibs.order);
            paint.sort_by_key(|k| {
                (
                    z_of(nodes, *k),
                    nodes[k.index()].as_ref().map(|x| x.order).unwrap_or(0),
                )
            });
            sibs.paint = paint;
        }
    }

    /// Walk the tree depth-first, parents before children.
    pub fn walk(&self) -> Vec<NodeId> {
        let mut out = Vec::with_capacity(self.live.len());
        let Some(root) = self.root else {
            return out;
        };
        // A modal, overlay or lifted subtree comes last: it floats above what it covers,
        // and paint order is walk order. Later ones land after earlier ones.
        let mut deferred: Vec<NodeId> = Vec::new();
        let mut stack = vec![root];
        while let Some(nid) = stack.pop() {
            if self.is_modal(nid) || self.is_overlay(nid) || self.is_lifted(nid) {
                deferred.push(nid);
                continue;
            }
            out.push(nid);
            // Positioned siblings paint by `z`; ties keep declaration order.
            stack.extend(self.children_in_paint_order(nid).iter().rev().copied());
        }
        for node in deferred {
            self.walk_subtree(node, &mut out);
        }
        out
    }

    /// Whether `nid` is a modal surface root, which the walk defers.
    pub fn is_modal(&self, nid: NodeId) -> bool {
        self.get(nid)
            .and_then(|n| n.data.params())
            .is_some_and(|p| p.modal.is_some())
    }

    /// Whether `nid` is an anchored overlay, which paints above everything else too.
    pub fn is_overlay(&self, nid: NodeId) -> bool {
        self.get(nid).is_some_and(|n| n.placement.overlay.is_some())
    }

    /// Whether `nid` is lifted: painted above the tree while keeping its layout slot.
    pub fn is_lifted(&self, nid: NodeId) -> bool {
        self.get(nid).is_some_and(|n| n.placement.lift)
    }

    /// Whether `nid` floats: no slot in its parent's flow, and it paints above it.
    pub fn floats(&self, nid: NodeId) -> bool {
        self.get(nid)
            .is_some_and(|n| n.kind == NodeKind::Overlay || self.is_modal(nid))
    }

    /// Whether `nid` is taken out of the flow by its positioning scheme.
    pub fn out_of_flow(&self, nid: NodeId) -> bool {
        self.get(nid).is_some_and(|n| {
            matches!(
                n.placement.position,
                crate::layout::Position::Absolute | crate::layout::Position::Fixed
            )
        })
    }

    /// Effective stacking order among siblings: only a positioned node carries its `z`.
    pub fn effective_z(&self, nid: NodeId) -> i32 {
        z_of(&self.nodes, nid)
    }

    /// Children in paint order: `(z, declaration order)`, stable. Paint and hit both use it.
    pub fn children_in_paint_order(&self, nid: NodeId) -> &[NodeId] {
        // Sorted once per apply: a stable sort keeps declaration order wherever `z` ties.
        self.by_parent
            .get(&Some(nid))
            .map(|s| s.paint.as_slice())
            .unwrap_or(&[])
    }

    /// One paint-ordered child by position, for callers that must mutate while walking.
    pub fn paint_child_at(&self, nid: NodeId, index: usize) -> Option<NodeId> {
        self.by_parent
            .get(&Some(nid))
            .and_then(|s| s.paint.get(index).copied())
    }

    /// The modal that owns the keyboard: the one painted last, which is the one on top.
    pub fn modal_root(&self) -> Option<NodeId> {
        self.walk()
            .into_iter()
            .rev()
            .find(|nid| self.is_modal(*nid))
    }

    /// The topmost overlay that takes the keyboard — a menu or a picker panel, not a tooltip.
    ///
    /// Asking for focus is the overlay's own declaration (`Sense::FOCUSABLE`), so a panel
    /// that is only read aloud stays out of the keyboard's way without a second flag.
    pub fn popup_root(&self) -> Option<NodeId> {
        self.walk().into_iter().rev().find(|nid| {
            self.is_overlay(*nid)
                && self
                    .get(*nid)
                    .is_some_and(|n| n.sense.contains(crate::Sense::FOCUSABLE))
        })
    }

    /// Whether `nid` is `root` or one of its descendants.
    pub fn within(&self, nid: NodeId, root: NodeId) -> bool {
        self.path_to(nid).contains(&root)
    }

    /// Append one subtree in pre-order, deferring the floats nested inside it.
    ///
    /// A panel declared inside another panel is still a float: paint defers it out of the
    /// subtree, so the walk — hit order, the keyboard scope, the focus stops — must defer it
    /// the same way, or a press lands on the sibling it covers.
    fn walk_subtree(&self, nid: NodeId, out: &mut Vec<NodeId>) {
        let mut deferred: Vec<NodeId> = Vec::new();
        let mut stack = vec![nid];
        while let Some(cur) = stack.pop() {
            if cur != nid && (self.is_modal(cur) || self.is_overlay(cur) || self.is_lifted(cur)) {
                deferred.push(cur);
                continue;
            }
            out.push(cur);
            stack.extend(self.children_in_paint_order(cur).iter().rev().copied());
        }
        for node in deferred {
            self.walk_subtree(node, out);
        }
    }

    /// Ancestor chain from root down to `nid`, inclusive.
    pub fn path_to(&self, nid: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut cur = Some(nid);
        while let Some(c) = cur {
            path.push(c);
            cur = self.slot(c).and_then(|n| n.parent);
        }
        path.reverse();
        path
    }

    pub fn constraints_for_root(&self, size: Size) -> Constraints {
        Constraints::loose(size)
    }

    pub fn set_layout_clean(&mut self) {
        self.layout_dirty = false;
        for nid in self.live.clone() {
            if let Some(n) = self.get_mut(nid) {
                n.dirty = false;
            }
        }
    }

    pub fn layout_is_dirty(&self) -> bool {
        self.layout_dirty
    }
}

#[cfg(test)]
mod tests;
