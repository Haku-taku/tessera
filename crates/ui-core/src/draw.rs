//! Draw primitives, the order log, and batch planning.
//!
//! Order and data are stored separately, so a backend can upload a whole typed
//! slice at once. A single `enum Cmd` would force a match per command.

use crate::geom::{Color, Corners, Insets, Rect};
use crate::id::{Id, NodeId};

/// Direction a linear gradient runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientAxis {
    /// Top to bottom.
    Vertical,
    /// Left to right.
    Horizontal,
}

/// A two-stop linear gradient in the primitive's own rect.
///
/// XAML states the stops separately from the brush's extent, and WinUI's elevation borders
/// use both: a 3-unit run at offset 0.33 rather than a ramp across the whole control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gradient {
    pub from: Color,
    pub to: Color,
    pub axis: GradientAxis,
    /// Where the near and far stops sit in the gradient's own 0..1 space.
    pub stops: (f32, f32),
    /// `None` spans the whole box; `Some(len)` covers only the first `len` units on the axis.
    pub extent: Option<f32>,
    /// Measure from the far edge inward: WinUI's light theme mirrors its elevation borders.
    pub flip: bool,
}

impl Gradient {
    /// A full-box gradient, near stop at the start and far stop at the end.
    pub const fn new(from: Color, to: Color, axis: GradientAxis) -> Self {
        Self {
            from,
            to,
            axis,
            stops: (0.0, 1.0),
            extent: None,
            flip: false,
        }
    }

    /// Anchor the run at the box's far edge instead of its near edge.
    pub const fn flipped(mut self) -> Self {
        self.flip = true;
        self
    }

    /// Move the two stops inside the gradient's own space.
    pub const fn with_stops(mut self, near: f32, far: f32) -> Self {
        self.stops = (near, far);
        self
    }

    /// Cover only the first `len` units along the axis, holding the far colour after.
    pub const fn over(mut self, len: f32) -> Self {
        self.extent = Some(len);
        self
    }

    /// Colour at `u`, the gradient's own 0..=1 coordinate.
    pub fn at(&self, u: f32) -> Color {
        let (near, far) = self.stops;
        let span = if far > near { far - near } else { 1.0 };
        self.from.lerp(self.to, ((u - near) / span).clamp(0.0, 1.0))
    }

    /// Colour at `pos` units along the axis, in a box `extent` units long.
    pub fn sample(&self, pos: f32, extent: f32) -> Color {
        // A flip mirrors the whole box before the run is measured into it.
        let pos = if self.flip { extent - pos } else { pos };
        let len = self.extent.unwrap_or(extent);
        let u = if len > 0.0 { pos / len } else { 0.0 };
        self.at(u)
    }
}

/// How far a press splat grows, and whether it drifts to the node's centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkRadius {
    /// A fixed radius around the press point; the splat never drifts.
    Fixed(f32),
    /// Covers the node: the splat drifts to the centre and grows to half the diagonal.
    Cover,
}

/// Where a ripple takes its rect and curve from: its effective background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkTarget {
    /// The node's own box and its declared (or inferred) curve.
    Node,
    /// An explicit rect and curve in the node's own space: the control's effective
    /// background, whether or not this frame paints one.
    Rect {
        rect: crate::geom::Rect,
        corners: crate::geom::Corners,
    },
    /// The nth shape the node paints, in declaration order: that shape's rect and curve.
    Shape(usize),
    /// A descendant node's box and curve, as Flutter's `getRectCallback`.
    Descendant(Id),
}

/// Press ink the engine splats where a press lands and fades out after release.
///
/// The engine owns the splat's origin and lifetime: `Widget::decl` is a pure function and
/// cannot hold the press point across frames, so a control only states colour and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressInk {
    /// Ink colour; its alpha is the strength the state layer would have.
    pub colour: Color,
    pub radius: InkRadius,
    /// The ink's effective background; the ripple sizes to it and is clipped to it.
    pub target: InkTarget,
    /// Opacity ramp while held; Flutter's `_kFadeInDuration`.
    pub fade_in: crate::anim::AnimSpec,
    /// Radius and drift; Flutter's `_kRadiusDuration`.
    pub grow: crate::anim::AnimSpec,
    /// Fade after the press ends.
    pub fade: crate::anim::AnimSpec,
}

impl PressInk {
    /// Flutter `InkRipple`: 75ms fade-in, 225ms radius on `Curves.ease`, then 375ms from the
    /// release with the fade in its last 150ms — a quick tap still shows the whole splash.
    pub const fn new(colour: Color, radius: InkRadius) -> Self {
        Self::with_target(colour, radius, InkTarget::Node)
    }

    /// Ink sized to and clipped by `target`, its effective background.
    pub const fn with_target(colour: Color, radius: InkRadius, target: InkTarget) -> Self {
        use crate::anim::{AnimSpec, Easing};
        Self {
            colour,
            radius,
            target,
            fade_in: AnimSpec::new(0.075, Easing::Linear),
            grow: AnimSpec::new(0.225, Easing::Cubic(0.25, 0.1, 0.25, 1.0)),
            fade: AnimSpec::new(0.15, Easing::Linear).delayed(0.225),
        }
    }
}

/// One filled (optionally rounded) box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectPrim {
    pub rect: Rect,
    pub radius: Corners,
    /// Solid colour; also the fallback where `gradient` is unsupported.
    pub fill: Color,
    /// Optional gradient across `rect`; backends that cannot draw it use `fill`.
    pub gradient: Option<Gradient>,
    pub border: Option<(Color, f32)>,
    /// A gradient across the stroke; `border` stays the near-stop fallback.
    pub border_gradient: Option<Gradient>,
    /// Omit the border over this x-range of its top edge, measured from `rect`'s left.
    ///
    /// An outlined field's label cuts this gap; it is a property of the stroke, so it
    /// lives on the primitive rather than in a clip.
    pub notch: Option<(f32, f32)>,
}

/// A filled and/or stroked ellipse, in logical space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipsePrim {
    /// Painted extent: the ellipse's rect grown by the stroke's half width.
    pub bounds: Rect,
    /// Centre, in logical space like the sector's.
    pub centre: crate::geom::Point,
    /// Semi-axes along x and y. A circle is an ellipse whose radii are equal.
    pub radii: (f32, f32),
    /// Stroke width, centred on the curve; zero strokes nothing.
    pub thickness: f32,
    /// Interior colour; transparent leaves the inside empty.
    pub fill: Color,
    /// Stroke colour.
    pub colour: Color,
}

/// One stroked circular arc, with its bounding box for clipping and damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorPrim {
    pub bounds: Rect,
    /// Stroke colour.
    pub colour: Color,
    pub sector: Sector,
}

/// A stroked circular arc, measured in turns clockwise from 3 o'clock.
///
/// Turns rather than radians because the reference (Lottie's `TrimEnd`) expresses arcs
/// that way; a quarter turn is `0.25`, a full ring `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector {
    /// Centre, relative to `rect`'s origin.
    pub centre: crate::geom::Point,
    /// Radius of the stroke's outer edge.
    pub radius: f32,
    /// Stroke width, growing inwards.
    pub thickness: f32,
    /// Start and end in turns; the sweep runs clockwise from start to end.
    pub trim: (f32, f32),
    /// Round caps at both ends, as WinUI's `StrokeDashCap="Round"` draws them.
    pub round_caps: bool,
}

impl Sector {
    /// The sweep in turns; a span of one full turn or more is the whole ring.
    ///
    /// `rem_euclid` alone would fold a full turn back to zero, which is how a 100% ring
    /// would vanish.
    pub fn sweep(&self) -> f32 {
        let span = self.trim.1 - self.trim.0;
        if span >= 1.0 {
            1.0
        } else {
            span.rem_euclid(1.0)
        }
    }

    /// The bounding box of the stroke, in the same space as the primitive's `rect`.
    ///
    /// Damage and the vertex quad both round this out to the pixel grid, so the feather's
    /// half pixel is covered without the box carrying a margin of its own.
    pub fn bounds(&self) -> Rect {
        let r = self.radius;
        Rect::new(self.centre.x - r, self.centre.y - r, 2.0 * r, 2.0 * r)
    }
}

/// One stroked segment, in the same logical space as the formula that emitted it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinePrim {
    pub from: crate::geom::Point,
    pub to: crate::geom::Point,
    pub thickness: f32,
    pub colour: Color,
}

impl LinePrim {
    /// The segment's painted extent, including the stroke's half width.
    pub fn bounds(&self) -> Rect {
        let half = self.thickness * 0.5;
        let x = self.from.x.min(self.to.x) - half;
        let y = self.from.y.min(self.to.y) - half;
        let right = self.from.x.max(self.to.x) + half;
        let bottom = self.from.y.max(self.to.y) + half;
        Rect::new(x, y, right - x, bottom - y)
    }
}

impl RectPrim {
    /// Colour at a point: the gradient if present, else the solid fill.
    pub fn colour_at(&self, px: f32, py: f32) -> Color {
        match &self.gradient {
            Some(g) => {
                let (pos, extent) = match g.axis {
                    GradientAxis::Vertical => (py - self.rect.origin.y, self.rect.size.h),
                    GradientAxis::Horizontal => (px - self.rect.origin.x, self.rect.size.w),
                };
                g.sample(pos, extent)
            }
            None => self.fill,
        }
    }

    /// The stroke's colour at a point: its gradient if present, else the solid.
    pub fn border_colour_at(&self, px: f32, py: f32) -> Color {
        let Some((solid, _)) = self.border else {
            return Color::TRANSPARENT;
        };
        match &self.border_gradient {
            Some(g) => {
                let (pos, extent) = match g.axis {
                    GradientAxis::Vertical => (py - self.rect.origin.y, self.rect.size.h),
                    GradientAxis::Horizontal => (px - self.rect.origin.x, self.rect.size.w),
                };
                g.sample(pos, extent)
            }
            None => solid,
        }
    }

    /// Whether a point, relative to the rect's origin, falls in the stroke's top-edge gap.
    ///
    /// The band is the stroke plus half a pixel: the antialiased skirt reaches past the
    /// nominal width, and leaving it behind draws a hairline under a floating label.
    pub fn in_notch(&self, x: f32, y: f32, width: f32, skirt: f32) -> bool {
        let Some((from, to)) = self.notch else {
            return false;
        };
        x >= from && x <= to && y <= width + 0.5 + skirt.max(0.0)
    }
}

/// Build a flat draw list from a laid-out tree: backgrounds then text, in tree order.
///
/// Every primitive is placed at `visual_rect`, never `rect`: an offset lives only in the
/// visual rect, so painting the layout rect pins every offset child to its parent origin.
pub fn list_from_tree(tree: &crate::tree::Tree, viewport: Rect) -> DrawList {
    let mut list = DrawList::new(viewport);
    for nid in tree.walk() {
        let Some(node) = tree.get(nid) else { continue };
        let visual = node.visual_rect();
        push_shapes(&mut list, nid, node, visual, |_, _| None);
        if let crate::tree::NodeData::Text {
            block,
            ink,
            caret,
            selection,
            composing,
            ..
        } = &node.data
        {
            list.push_text(
                nid,
                TextPrim {
                    block: block.clone(),
                    origin: visual.origin,
                    max_width: visual.size.w,
                    layout: node.inline.clone(),
                    height: visual.size.h,
                    ink: *ink,
                    caret: *caret,
                    selection: selection.clone(),
                    composing: composing.clone(),
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
        }
        if let crate::tree::NodeData::Math {
            formula,
            family,
            ink,
        } = &node.data
        {
            list.push_math(
                nid,
                MathPrim {
                    formula: formula.clone(),
                    family: family.clone(),
                    origin: visual.origin,
                    ink: *ink,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
        }
    }
    list
}

/// Emit one node's shapes in paint order; `image` resolves a texture slot for box images.
pub fn push_shapes(
    list: &mut DrawList,
    owner: NodeId,
    node: &crate::tree::Node,
    visual: Rect,
    mut image: impl FnMut(&crate::tree::ImageSpec, Rect) -> Option<ImagePrim>,
) {
    let clip = list.clip_now();
    let transform = list.tf();
    for shape in &node.shapes {
        // A shape no clip can show would only be scissored away, and a long document pays
        // that decision on every frame it spends outside the viewport.
        if let Some(clip) = clip {
            let reach = shape.bounds(visual);
            let reach = transform.map_or(reach, |t| t.apply_rect(reach));
            if !reach
                .intersection(clip)
                .is_some_and(|seen| !seen.is_empty())
            {
                continue;
            }
        }
        match shape {
            Shape::Box {
                corners,
                fill,
                gradient,
                border,
                shadow,
                image: spec,
            } => {
                if let Some(sh) = shadow
                    && !sh.inset
                {
                    list.push_shadow(
                        owner,
                        ShadowPrim {
                            rect: visual.inflate(sh.spread),
                            corners: *corners,
                            colour: sh.colour,
                            blur: sh.blur,
                            dy: sh.dy,
                            ring: 0.0,
                            bound: Some(ShadowBound {
                                rect: visual,
                                side: ShadowSide::Outer,
                            }),
                        },
                    );
                }
                list.push_rect(
                    owner,
                    RectPrim {
                        rect: visual,
                        radius: *corners,
                        fill: *fill,
                        gradient: *gradient,
                        border: *border,
                        border_gradient: None,
                        notch: None,
                    },
                );
                // CSS paints an inset shadow above the background and below the content.
                if let Some(sh) = shadow
                    && sh.inset
                {
                    let inner = visual.inflate(-sh.spread);
                    if !inner.is_empty() {
                        list.push_shadow(
                            owner,
                            ShadowPrim {
                                rect: inner,
                                corners: *corners,
                                colour: sh.colour,
                                blur: sh.blur,
                                dy: sh.dy,
                                ring: 0.0,
                                bound: Some(ShadowBound {
                                    rect: visual,
                                    side: ShadowSide::Inset,
                                }),
                            },
                        );
                    }
                }
                if let Some(spec) = spec
                    && let Some(prim) = image(spec, visual)
                {
                    list.push_image_prim(owner, prim);
                }
            }
            Shape::Rect {
                rect,
                corners,
                fill,
                gradient,
                border,
            } => {
                list.push_rect(
                    owner,
                    RectPrim {
                        rect: rect.translate(visual.origin),
                        radius: *corners,
                        fill: *fill,
                        gradient: *gradient,
                        border: *border,
                        border_gradient: None,
                        notch: None,
                    },
                );
            }
            Shape::Shadow {
                rect,
                corners,
                colour,
                blur,
                dy,
                ring,
            } => {
                list.push_shadow(
                    owner,
                    ShadowPrim {
                        rect: rect.translate(visual.origin),
                        corners: *corners,
                        colour: *colour,
                        blur: *blur,
                        dy: *dy,
                        ring: *ring,
                        bound: None,
                    },
                );
            }
            Shape::Outline {
                corners,
                colour,
                gradient,
                width,
                gap,
            } => {
                list.push_rect(
                    owner,
                    RectPrim {
                        rect: visual,
                        radius: *corners,
                        fill: Color::TRANSPARENT,
                        gradient: None,
                        border: Some((*colour, *width)),
                        border_gradient: *gradient,
                        notch: *gap,
                    },
                );
            }
            Shape::Ellipse {
                rect,
                thickness,
                fill,
                colour,
            } => {
                let rect = rect.translate(visual.origin);
                let half = thickness * 0.5;
                list.push_ellipse(
                    owner,
                    EllipsePrim {
                        bounds: rect.inflate(half),
                        centre: rect.origin
                            + crate::geom::Point::new(rect.size.w, rect.size.h) * 0.5,
                        radii: (rect.size.w * 0.5, rect.size.h * 0.5),
                        thickness: *thickness,
                        fill: *fill,
                        colour: *colour,
                    },
                );
            }
            Shape::Line {
                from,
                to,
                thickness,
                colour,
            } => {
                list.push_line(
                    owner,
                    LinePrim {
                        from: *from + visual.origin,
                        to: *to + visual.origin,
                        thickness: *thickness,
                        colour: *colour,
                    },
                );
            }
            Shape::Sector {
                centre,
                radius,
                thickness,
                trim,
                round_caps,
                colour,
            } => {
                let sector = Sector {
                    centre: crate::geom::Point::new(
                        visual.origin.x + centre.x,
                        visual.origin.y + centre.y,
                    ),
                    radius: *radius,
                    thickness: *thickness,
                    trim: *trim,
                    round_caps: *round_caps,
                };
                list.push_sector(
                    owner,
                    SectorPrim {
                        bounds: sector.bounds(),
                        colour: *colour,
                        sector,
                    },
                );
            }
        }
    }
}

/// The focus adorner a node asks the engine to draw around itself.
///
/// It is a *post-layout* drawing: the rect is only known after layout, and the adorner
/// deliberately extends past the node's box (`margin` is negative, as WinUI writes it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusVisual {
    /// The outer stroke and its width. WinUI: `FocusVisualPrimaryBrush`, 2 px.
    pub outer: Color,
    pub outer_width: f32,
    /// The inner stroke drawn just inside the outer one. WinUI: `FocusVisualSecondaryBrush`, 1 px.
    pub inner: Color,
    pub inner_width: f32,
    /// How far the adorner reaches outside the node on each side; negative, as XAML writes it.
    pub margin: crate::geom::Insets,
    /// Extra radius over the node's own, so the two curves stay concentric.
    pub radius: f32,
}

/// A paint-only shape in the node's own space: no identity, no layout slot, no hit target.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// The node's own box: its fill, gradient, border, shadow and image.
    Box {
        corners: Corners,
        fill: Color,
        gradient: Option<Gradient>,
        border: Option<(Color, f32)>,
        shadow: Option<crate::tree::ShadowSpec>,
        image: Option<crate::tree::ImageSpec>,
    },
    /// An explicit rect: circles, pills and thumbnails inside the node's box.
    Rect {
        rect: Rect,
        corners: Corners,
        fill: Color,
        gradient: Option<Gradient>,
        border: Option<(Color, f32)>,
    },
    /// A soft shadow under an explicit rect.
    Shadow {
        rect: Rect,
        corners: Corners,
        colour: Color,
        blur: f32,
        dy: f32,
        /// How deep the casting band is: 0 shadows the whole shape, a positive depth shadows
        /// only its rim, leaving the interior clear for a translucent surface above it.
        ring: f32,
    },
    /// The node's own box drawn as a border only, its top edge interrupted over `gap`:
    /// an outlined field's label notch, measured from the node's left edge.
    Outline {
        corners: Corners,
        colour: Color,
        /// A gradient along the stroke; `colour` stays the near-stop fallback.
        gradient: Option<Gradient>,
        width: f32,
        gap: Option<(f32, f32)>,
    },
    /// A filled and/or stroked ellipse; the stroke is centred on the curve.
    Ellipse {
        rect: Rect,
        thickness: f32,
        fill: Color,
        colour: Color,
    },
    /// A stroked segment: annotation rules and arrows.
    Line {
        from: crate::geom::Point,
        to: crate::geom::Point,
        thickness: f32,
        colour: Color,
    },
    /// A stroked circular arc: rings and progress sweeps.
    Sector {
        /// Centre in the node's own space.
        centre: crate::geom::Point,
        radius: f32,
        thickness: f32,
        trim: (f32, f32),
        round_caps: bool,
        colour: Color,
    },
}

impl Shape {
    /// The area this shape can touch, given the box of the node that owns it.
    pub fn bounds(&self, node: Rect) -> Rect {
        match self {
            // Same extent as the prim it becomes: spread goes into the rect, the rest pads it.
            Shape::Box { shadow, .. } => match shadow {
                Some(s) => shadow_painted(node.inflate(s.spread), s.blur, s.dy, 0.0),
                None => node,
            },
            // Sub-shapes are declared in the node's own space, so lift them into absolute.
            Shape::Rect { rect, .. } => rect.translate(node.origin),
            Shape::Shadow {
                rect,
                blur,
                dy,
                ring,
                ..
            } => shadow_painted(rect.translate(node.origin), *blur, *dy, *ring),
            Shape::Outline { .. } => node,
            // Sub-shapes and strokes are declared in the node's space too.
            Shape::Ellipse {
                rect, thickness, ..
            } => rect.translate(node.origin).inflate(thickness * 0.5),
            Shape::Line {
                from,
                to,
                thickness,
                ..
            } => LinePrim {
                from: *from + node.origin,
                to: *to + node.origin,
                thickness: *thickness,
                colour: Color::TRANSPARENT,
            }
            .bounds(),
            // The arc is declared in the node's space too.
            Shape::Sector { centre, radius, .. } => Rect::new(
                node.origin.x + centre.x - radius,
                node.origin.y + centre.y - radius,
                2.0 * radius,
                2.0 * radius,
            ),
        }
    }
}

/// A uniform scale, applied to everything a subtree paints.
///
/// Affine rather than scale-about-a-point: two scales about different centres compose to a
/// translation, which a fixed point cannot express; nesting would drift.
///
/// Paint-only: layout never sees it. Non-uniform and rotational transforms are a different,
/// later problem; a scale is what presence animations and the slider lens need.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub scale: f32,
    /// Added after scaling: `p * scale + offset`.
    pub offset: crate::geom::Point,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        scale: 1.0,
        offset: crate::geom::Point::ZERO,
    };

    /// The scale that leaves `origin` where it is.
    pub fn about(origin: crate::geom::Point, scale: f32) -> Self {
        Self {
            scale,
            offset: origin - origin * scale,
        }
    }

    pub fn is_identity(self) -> bool {
        (self.scale - 1.0).abs() < 1e-6 && self.offset.x.abs() < 1e-6 && self.offset.y.abs() < 1e-6
    }

    pub fn apply_point(self, p: crate::geom::Point) -> crate::geom::Point {
        p * self.scale + self.offset
    }

    pub fn apply_rect(self, r: Rect) -> Rect {
        let origin = self.apply_point(r.origin);
        Rect::new(
            origin.x,
            origin.y,
            r.size.w * self.scale,
            r.size.h * self.scale,
        )
    }

    pub fn apply_corners(self, c: crate::geom::Corners) -> crate::geom::Corners {
        crate::geom::Corners {
            tl: c.tl * self.scale,
            tr: c.tr * self.scale,
            br: c.br * self.scale,
            bl: c.bl * self.scale,
            shape: c.shape,
        }
    }

    /// Compose: `self` wraps `inner`, so the inner scale happens first.
    pub fn then(self, inner: Self) -> Self {
        Self {
            scale: self.scale * inner.scale,
            offset: self.apply_point(inner.offset),
        }
    }
}

/// One run of text.
///
/// The block and its layout are shared with the tree that declared them, so they are reached
/// through `Arc`: a draw list is the frame contract, and a presenter may draw it on a thread
/// of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPrim {
    /// Styled runs plus paragraph properties; rich text adds runs here.
    pub block: std::sync::Arc<crate::text::TextBlock>,
    pub origin: crate::geom::Point,
    /// Wrap width in logical units; `f32::INFINITY` means one line.
    pub max_width: f32,
    /// The lines layout wrapped this block into; `None` makes the painter wrap it itself.
    pub layout: Option<std::sync::Arc<crate::text::InlineLayout>>,
    /// The node's own box height; `0.0` when the caller does not know it.
    ///
    /// A symbol is centred by its ink inside this box; text ignores it.
    pub height: f32,
    /// Ink this text is drawn in; the component picks the role, the type ramp never does.
    pub ink: Color,
    /// Draw the insertion caret at this byte offset.
    pub caret: Option<Caret>,
    /// Selected bytes and how this style draws them; `None` when nothing is selected.
    pub selection: Option<crate::selection::SelectionPrim>,
    /// Bytes the input method is composing; drawn with an underline.
    pub composing: Option<std::ops::Range<usize>>,
    /// The scale in force where this run was pushed; shaping happens first, then this applies.
    ///
    /// It sits here because glyphs exist only after shaping, which happens after painting.
    pub zoom: Option<Transform>,
    /// The static part of that scale — a declared paint scale, not a presence animation.
    ///
    /// Glyphs are rasterized at this, so a zoomed card is as sharp as text laid out that
    /// large, while a presence animation still stretches its glyphs instead of rasterizing a
    /// new size on every frame of the tween.
    pub shape_scale: f32,
}

impl TextPrim {
    /// Equality with a pointer fast path: a frame that rebuilt nothing hands over the same
    /// paragraph, and comparing its runs byte by byte is the cost this sharing exists to save.
    pub fn same_as(&self, other: &Self) -> bool {
        (std::sync::Arc::ptr_eq(&self.block, &other.block) || self.block == other.block)
            && self.origin == other.origin
            && self.max_width == other.max_width
            && match (&self.layout, &other.layout) {
                (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b) || a == b,
                (a, b) => a == b,
            }
            && self.height == other.height
            && self.ink == other.ink
            && self.caret == other.caret
            && self.selection == other.selection
            && self.composing == other.composing
            && self.zoom == other.zoom
            && self.shape_scale == other.shape_scale
    }
}

/// A laid-out formula, waiting for the text pass to rasterize its glyphs.
#[derive(Clone, Debug, PartialEq)]
pub struct MathPrim {
    pub formula: std::sync::Arc<crate::math::Formula>,
    /// The face the formula's glyph ids belong to; another face would draw other glyphs.
    pub family: std::sync::Arc<str>,
    /// Top-left of the formula's box, so its baseline sits at `origin.y + formula.ascent`.
    pub origin: crate::geom::Point,
    /// Ink for items that carry no colour of their own.
    pub ink: Color,
    /// The scale in force where this was pushed; applied when glyphs are rasterized.
    pub zoom: Option<Transform>,
    /// The static part of that scale; glyphs are rasterized at it. See `TextPrim`.
    pub shape_scale: f32,
}

/// A text input's insertion caret: where it sits and what colour it takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub offset: usize,
    pub color: Color,
}

/// Which atlas a glyph primitive samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphKind {
    /// One coverage byte per pixel, tinted by `GlyphPrim::color`.
    #[default]
    Coverage,
    /// Premultiplied RGBA per pixel, used as-is; `color` only scales alpha.
    Color,
}

/// One pre-rasterized glyph blitted from an atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphPrim {
    /// Destination rect on screen.
    pub rect: Rect,
    /// Atlas slot origin and size.
    pub atlas: Rect,
    /// Linear multiplier applied to coverage or alpha (0..=1).
    pub alpha: f32,
    pub color: Color,
    /// Whether `atlas` indexes the coverage atlas or the colour atlas.
    pub kind: GlyphKind,
}

/// One RGBA8 texture drawn into a rounded rect, sampling a source sub-rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImagePrim {
    /// Destination in logical units.
    pub rect: Rect,
    pub corners: Corners,
    /// Index into `DrawList::images`.
    pub texture: u32,
    /// Source rect in texture pixels, already resolved by the node's fit.
    pub src: Rect,
}

/// Which side of a surface's edge a shadow occupies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowSide {
    /// Beyond the edge: CSS's outer box-shadow. The surface stays clear of its own cast.
    Outer,
    /// Within the edge: CSS's inset box-shadow.
    Inset,
}

/// The surface whose edge bounds a shadow, so it never paints through the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowBound {
    /// The surface's box; the bound shares the shadow's corner radii.
    pub rect: Rect,
    pub side: ShadowSide,
}

/// A soft shadow under a rounded rect. The falloff is analytic, so there is no blur pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowPrim {
    /// The shape being shadowed; the shadow extends `blur` beyond it.
    pub rect: Rect,
    pub corners: Corners,
    /// Colour and peak alpha.
    pub colour: Color,
    /// Falloff width in logical units: the shadow fades over roughly `2 * blur`.
    pub blur: f32,
    /// Vertical offset; WinUI's elevation shadows sit slightly below their surface.
    pub dy: f32,
    /// Depth of the casting band, in logical units; 0 shadows the whole shape.
    pub ring: f32,
    /// The surface edge that stops the cast. `None` casts freely, as the glass rim does.
    pub bound: Option<ShadowBound>,
}

/// The area a shadow cast inside `rect` can paint: the shader's own quad extent.
fn shadow_painted(rect: Rect, blur: f32, dy: f32, ring: f32) -> Rect {
    rect.inflate(ring * 0.5 + blur * 1.5 + dy.abs())
}

impl ShadowPrim {
    /// The area this shadow can paint, for damage and clipping.
    pub fn painted(&self) -> Rect {
        let area = match self.bound {
            Some(bound) => self.rect.union(bound.rect),
            None => self.rect,
        };
        shadow_painted(area, self.blur, self.dy, self.ring)
    }
}

/// How a sampler reads between texel centres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sampling {
    /// Blend the neighbouring texels: the right choice whenever the destination is smaller
    /// than the source, where picking one texel would alias.
    #[default]
    Smooth,
    /// Take the nearest texel: an enlarged picture shows its own blocks instead of a blur.
    Pixelated,
}

/// A picture's identity: pixels on the CPU, or a canvas the frame paints into a target.
///
/// The engine never reads a canvas back: a renderer resolves the id to the target it owns,
/// so the content crosses neither the bus nor a copy.
#[derive(Clone, Debug)]
pub enum ImageSource {
    /// Straight alpha, row-major, packed as `colour.storage` says. This `Arc` is the
    /// picture's identity: keep it while the pixels are unchanged, hand over a new one when
    /// they change.
    Pixels(std::sync::Arc<Vec<u8>>),
    /// A target declared by a [`Canvas`] this frame; the drawing node names it like a
    /// texture, and the renderer samples it with the node's `Sampling`.
    Canvas(CanvasId),
}

/// A canvas's stable identity, so its target survives frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanvasId(pub u64);

/// A render target painted from primitives in its own pixels.
///
/// A canvas is not a subtree: it has no layout, no hit index and no events. The app draws
/// it at its content's resolution and the engine repaints it only when `version` or `size`
/// changes, so a zoom or a pan costs one textured quad.
#[derive(Debug)]
pub struct Canvas {
    pub id: CanvasId,
    pub size: (u32, u32),
    /// The app's content revision, bumped by the edit that changes the picture. Style
    /// changes count: a restyle moves no bounds but changes the pixels.
    pub version: u64,
    pub list: DrawList,
}

/// One texture the frame refers to. `id` is stable across frames so a renderer can cache.
#[derive(Clone, Debug)]
pub struct ImageEntry {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// The picture itself, pixels or a canvas.
    pub source: ImageSource,
    /// What the pixels mean. A renderer converts for the domain it draws in and caches the
    /// result; the pixels themselves are never rewritten.
    pub colour: crate::colour::ContentColour,
    /// How this drawing reads the texels; a texture cache keys on it, because one file drawn
    /// both ways needs two GPU textures.
    pub sampling: Sampling,
}

impl ImageEntry {
    /// The pixel buffer, when this entry holds pixels rather than a canvas.
    pub fn pixels(&self) -> Option<&std::sync::Arc<Vec<u8>>> {
        match &self.source {
            ImageSource::Pixels(data) => Some(data),
            ImageSource::Canvas(_) => None,
        }
    }

    /// Identity of the picture for damage hashing: where the buffer lives, which changes
    /// when the app hands over new pixels behind the same id. A canvas answers 0; its
    /// version is hashed instead.
    pub fn picture_id(&self) -> usize {
        self.pixels().map_or(0, |data| data.as_ptr() as usize)
    }

    /// The same entry read with `sampling`, for the slot a drawing node declares.
    pub fn sampled(&self, sampling: Sampling) -> Self {
        Self {
            sampling,
            ..self.clone()
        }
    }
}

/// Primitive kinds, ordered so that `PrimKind as usize` indexes the bank array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum PrimKind {
    Rect = 0,
    Text = 1,
    Glyph = 2,
    Image = 3,
    Shadow = 4,
    Sector = 5,
    Math = 6,
    Line = 7,
    Ellipse = 8,
}

impl PrimKind {
    pub const COUNT: usize = 9;

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Why a datum is in the list, and where it lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimRef {
    pub kind: PrimKind,
    pub index: u32,
}

/// One entry in the paint-order log.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// Draw the referenced primitive.
    Prim(PrimRef),
    /// Push a clip rect (index into `clips`).
    PushClip(u32),
    /// Pop the innermost clip.
    PopClip,
    /// Push an offscreen layer (index into `layers`).
    PushLayer(u32),
    /// Pop the innermost layer.
    PopLayer,
    /// Blur whatever is already painted inside a rect (index into `backdrops`).
    Backdrop(u32),
}

/// WinUI acrylic: a luminosity wash, a colour-blended tint, then a grain layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Acrylic {
    /// Tint colour; `a` is `TintOpacity` times the tint-opacity modifier.
    pub tint: Color,
    /// Luminosity colour; `a` is `TintLuminosityOpacity`.
    pub luminosity: Color,
    /// Grain layer opacity; the reference tiles a 2% noise image here.
    pub noise: f32,
}

/// What glass does to its capture before any colour: a neutral luminance transfer plus a
/// chroma scale. Apple's regular glass lifts luminance toward white and keeps the hue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassBody {
    /// Share of the way the luminance moves toward `pole`.
    pub mix: f32,
    /// The luminance `mix` heads toward.
    pub pole: f32,
    /// Chroma scale about that luminance; `1` keeps the backdrop's saturation.
    pub chroma: f32,
    /// Strength of the dark appearance's shadow-lifting tone curve, `0..=1.5`.
    pub tone: f32,
}

impl GlassBody {
    /// No body: the capture reaches the eye untouched.
    pub const CLEAR: Self = Self {
        mix: 0.0,
        pole: 1.0,
        chroma: 1.0,
        tone: 0.0,
    };
}

/// LiquidGlassKit's glass material (`LiquidGlassView.swift:56-78`): the backdrop is captured
/// at the material's own density, the rim bends by a thin prism, then Fresnel and glare.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidGlass {
    /// The neutral body; it carries no hue, so an untinted pane never casts one.
    pub body: GlassBody,
    /// A semantic brand colour laid over the body, at `a`; transparent means untinted.
    pub tint: Color,
    /// `refractiveIndex`: how hard the rim's thin prism bends.
    pub refractive_index: f32,
    /// `glassThickness` in dp: how deep the rim bend reaches.
    pub thickness: f32,
    /// `backgroundTextureSizeCoefficient`: how much wider than the node the capture is.
    pub capture_size: f32,
    /// `backgroundTextureScaleCoefficient`: capture density against the node's device size.
    pub capture_scale: f32,
    /// Content scale under the glass: `1` shows 1:1, `0.75` shows a wider field (shrinks).
    pub magnification: f32,
    /// How far the backdrop is blurred before it is captured, in dp; `0` keeps it sharp.
    pub frost: f32,
    /// `dispersionStrength`: per-channel prism split, scaled by 0.02 in the shader.
    pub dispersion: f32,
    /// `fresnelIntensity`: how much the grazing angle lifts the rim, `0..=1`.
    pub fresnel_intensity: f32,
    /// `fresnelDistanceRange` in dp: how far from the edge Fresnel builds.
    pub fresnel_distance: f32,
    /// `fresnelEdgeSharpness`: power of the Fresnel falloff.
    pub fresnel_sharpness: f32,
    /// `glareIntensity`: strength of the specular streak.
    pub glare_intensity: f32,
    /// `glareDistanceRange` in dp: how far the streak reaches inward.
    pub glare_distance: f32,
    /// `glareEdgeSharpness`: power of the streak's falloff.
    pub glare_sharpness: f32,
    /// `glareAngleConvergence`: focuses the streak toward the light axis, `0..=π`.
    pub glare_convergence: f32,
    /// `glareOppositeSideBias`: `>1` amplifies the far-side highlight.
    pub glare_bias: f32,
    /// `glareDirectionOffset` in radians: tilts the streak's asymmetry.
    pub glare_offset: f32,
    /// Extra shapes fused with the node's own rect, which is always shape zero.
    pub blend: [BlendShape; BLEND_SHAPES],
    /// How many entries of `blend` are live; `0` shades the node's rect alone.
    pub blend_count: u8,
    /// The smooth-union radius in logical px: how far a neck reaches between shapes.
    pub blend_k: f32,
}

/// How many extra shapes one glass surface may fuse with.
pub const BLEND_SHAPES: usize = 4;

/// One member of a blend group, in the node's own logical coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendShape {
    pub rect: Rect,
    pub corners: Corners,
}

impl BlendShape {
    /// A shape that contributes nothing; the slot's default.
    pub const NONE: Self = Self {
        rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        corners: Corners::ZERO,
    };
}

/// How a blurred backdrop is shaded before it is written back over the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackdropMaterial {
    /// Plain blit of the blurred pixels; the node's own translucent fill supplies the tint.
    Plain,
    Acrylic(Acrylic),
    LiquidGlass(LiquidGlass),
}

impl BackdropMaterial {
    /// The material that leaves the blur untouched.
    pub const PLAIN: Self = Self::Plain;
}

/// A backdrop blur recorded by `Op::Backdrop`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropEntry {
    /// The shape whose backdrop is read; its corners belong to this box.
    pub rect: Rect,
    /// The clip the box is cut by, in visual space: the frost is cut the same way.
    pub clip: Rect,
    pub corners: Corners,
    pub radius: u32,
    pub sigma: f32,
    /// How the blurred result is shaded; `PLAIN` keeps the tinted blur.
    pub material: BackdropMaterial,
}

/// A clip region recorded by `Op::PushClip`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipEntry {
    pub rect: Rect,
    /// Corner shape of the clip; `Corners::ZERO` means a plain rectangle.
    pub corners: Corners,
    /// The uncut box the corners belong to; equals `rect` unless an outer clip cut it.
    pub shape: Rect,
}

impl ClipEntry {
    pub fn rect(rect: Rect) -> Self {
        Self {
            rect,
            corners: Corners::ZERO,
            shape: rect,
        }
    }
}

/// A layer recorded by `Op::PushLayer`.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerEntry {
    pub bounds: Rect,
    /// Applied in order to the layer's pixels before compositing; empty means none.
    pub effects: Vec<crate::effect::Effect>,
    pub radius: Corners,
    /// When set, the target outlives the frame under this key and is reused while nothing
    /// inside the layer changes; `None` renders the layer afresh every frame.
    pub cache_key: Option<u64>,
}

impl LayerEntry {
    /// Uniform fade when the chain is exactly one opacity matrix.
    pub fn opacity(&self) -> Option<f32> {
        match self.effects.as_slice() {
            [] => Some(1.0),
            [e] if e.kernel.is_identity() => e.color.as_opacity(),
            _ => None,
        }
    }
}

/// Typed primitive banks.
#[derive(Clone, Default, Debug)]
pub struct Primitives {
    pub rects: Vec<RectPrim>,
    pub texts: Vec<TextPrim>,
    pub glyphs: Vec<GlyphPrim>,
    pub images: Vec<ImagePrim>,
    pub shadows: Vec<ShadowPrim>,
    pub sectors: Vec<SectorPrim>,
    pub maths: Vec<MathPrim>,
    pub lines: Vec<LinePrim>,
    pub ellipses: Vec<EllipsePrim>,
}

impl Primitives {
    pub fn len_of(&self, kind: PrimKind) -> usize {
        match kind {
            PrimKind::Rect => self.rects.len(),
            PrimKind::Text => self.texts.len(),
            PrimKind::Glyph => self.glyphs.len(),
            PrimKind::Image => self.images.len(),
            PrimKind::Shadow => self.shadows.len(),
            PrimKind::Sector => self.sectors.len(),
            PrimKind::Math => self.maths.len(),
            PrimKind::Line => self.lines.len(),
            PrimKind::Ellipse => self.ellipses.len(),
        }
    }
}

/// A contiguous run of same-kind primitives under one clip, one layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub kind: PrimKind,
    /// Range into the typed bank.
    pub range: core::ops::Range<u32>,
    /// Clip in force for this batch (`None` = unlimited).
    pub clip: Option<ClipEntry>,
    /// Layer chain from outermost to innermost; empty means the root target.
    pub layers: Vec<u32>,
    /// Backdrop generation; a batch never spans two different values.
    pub epoch: u32,
    /// Union of the prims' bounds: a backend may skip a batch that misses the redraw band.
    pub bounds: Option<Rect>,
}

/// The per-frame draw list.
///
/// `Clone` exists for a presenter that draws on a thread of its own: the loop hands over a
/// copy and keeps building while the renderer works.
#[derive(Clone, Debug)]
pub struct DrawList {
    pub ops: Vec<Op>,
    pub prims: Primitives,
    pub clips: Vec<ClipEntry>,
    pub layers: Vec<LayerEntry>,
    pub backdrops: Vec<BackdropEntry>,
    /// Textures this frame samples, indexed by `ImagePrim::texture`.
    pub images: Vec<ImageEntry>,
    /// Owning node per primitive, for per-node damage diffing.
    pub owners: Owners,
    /// Canvas size, for batching decisions.
    pub canvas: Rect,
    /// Clip in force for pushes right now, intersected down the stack and already transformed.
    ///
    /// Paint reads it to drop a primitive no clip can show; the text pass derives the same
    /// stack from the ops, because it culls after the list is built.
    clip_now: Option<Rect>,
    /// What `clip_now` held before each push, restored on pop.
    clip_before: Vec<Option<Rect>>,
    /// Scale applied to every primitive pushed from now on; `None` paints as laid out.
    ///
    /// The painter sets it for the span of one subtree, so a subtree that scales is scaled
    /// whole — shapes, clips, layers and glyphs alike — without touching layout.
    pub transform: Option<Transform>,
    /// The part of that scale the app declared rather than an animation, for glyph shaping.
    ///
    /// Text is rasterized at the static scale so a zoomed card is sharp; a presence tween
    /// only stretches the shaped glyphs, which keeps the atlas from filling with one raster
    /// per frame of the animation.
    pub shape_scale: f32,
}

/// Per-kind ownership tables, parallel to `Primitives`.
#[derive(Clone, Default, Debug)]
pub struct Owners {
    pub rects: Vec<NodeId>,
    pub texts: Vec<NodeId>,
    pub glyphs: Vec<NodeId>,
    pub images: Vec<NodeId>,
    pub shadows: Vec<NodeId>,
    pub sectors: Vec<NodeId>,
    pub maths: Vec<NodeId>,
    pub lines: Vec<NodeId>,
    pub ellipses: Vec<NodeId>,
}

impl Owners {
    pub fn push(&mut self, kind: PrimKind, node: NodeId) {
        match kind {
            PrimKind::Rect => self.rects.push(node),
            PrimKind::Text => self.texts.push(node),
            PrimKind::Glyph => self.glyphs.push(node),
            PrimKind::Image => self.images.push(node),
            PrimKind::Shadow => self.shadows.push(node),
            PrimKind::Sector => self.sectors.push(node),
            PrimKind::Math => self.maths.push(node),
            PrimKind::Line => self.lines.push(node),
            PrimKind::Ellipse => self.ellipses.push(node),
        }
    }

    pub fn of(&self, kind: PrimKind, index: usize) -> Option<NodeId> {
        match kind {
            PrimKind::Rect => self.rects.get(index).copied(),
            PrimKind::Text => self.texts.get(index).copied(),
            PrimKind::Glyph => self.glyphs.get(index).copied(),
            PrimKind::Image => self.images.get(index).copied(),
            PrimKind::Shadow => self.shadows.get(index).copied(),
            PrimKind::Sector => self.sectors.get(index).copied(),
            PrimKind::Math => self.maths.get(index).copied(),
            PrimKind::Line => self.lines.get(index).copied(),
            PrimKind::Ellipse => self.ellipses.get(index).copied(),
        }
    }
}

impl Default for DrawList {
    fn default() -> Self {
        Self {
            ops: Vec::new(),
            prims: Primitives::default(),
            clips: Vec::new(),
            layers: Vec::new(),
            backdrops: Vec::new(),
            images: Vec::new(),
            owners: Owners::default(),
            canvas: Rect::ZERO,
            clip_now: None,
            clip_before: Vec::new(),
            transform: None,
            // A list that scales nothing shapes text at the surface scale alone.
            shape_scale: 1.0,
        }
    }
}

impl DrawList {
    pub fn new(canvas: Rect) -> Self {
        Self {
            canvas,
            ..Default::default()
        }
    }

    /// The owner a canvas's primitives carry.
    ///
    /// A canvas has no node: nothing hit-tests its content, and its damage comes from the
    /// version that painted it. Every push still names an owner, so canvas content names
    /// this one.
    pub fn canvas_owner() -> crate::id::NodeId {
        // A slot the arena never hands out, so a stray lookup cannot land on the root.
        crate::id::NodeId::from_raw(u32::MAX, u32::MAX)
    }

    pub fn clear(&mut self) {
        self.clip_now = None;
        self.clip_before.clear();
        self.ops.clear();
        self.prims.rects.clear();
        self.prims.texts.clear();
        self.prims.glyphs.clear();
        self.prims.images.clear();
        self.prims.shadows.clear();
        self.prims.sectors.clear();
        self.prims.maths.clear();
        self.prims.lines.clear();
        self.prims.ellipses.clear();
        self.clips.clear();
        self.layers.clear();
        self.backdrops.clear();
        self.images.clear();
        self.owners.rects.clear();
        self.owners.texts.clear();
        self.owners.glyphs.clear();
        self.owners.images.clear();
        self.owners.shadows.clear();
        self.owners.sectors.clear();
        self.owners.maths.clear();
        self.owners.lines.clear();
        self.owners.ellipses.clear();
    }

    /// The transform in force for pushes right now, when it would change anything.
    fn tf(&self) -> Option<Transform> {
        self.transform.filter(|t| !t.is_identity())
    }

    pub fn push_rect(&mut self, owner: NodeId, mut prim: RectPrim) -> u32 {
        if let Some(t) = self.tf() {
            prim.rect = t.apply_rect(prim.rect);
            prim.radius = t.apply_corners(prim.radius);
            if let Some((_, width)) = prim.border.as_mut() {
                *width *= t.scale;
            }
            prim.notch = prim.notch.map(|(a, b)| (a * t.scale, b * t.scale));
            if let Some(g) = prim.gradient.as_mut() {
                g.extent = g.extent.map(|len| len * t.scale);
            }
            if let Some(g) = prim.border_gradient.as_mut() {
                g.extent = g.extent.map(|len| len * t.scale);
            }
        }
        let index = self.prims.rects.len() as u32;
        self.prims.rects.push(prim);
        self.owners.push(PrimKind::Rect, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Rect,
            index,
        }));
        index
    }

    /// Register a texture for this frame and return its index.
    pub fn push_image(&mut self, entry: ImageEntry) -> u32 {
        let index = self.images.len() as u32;
        self.images.push(entry);
        index
    }

    /// Add one stroked arc; its bounds ride along for damage and culling.
    pub fn push_sector(&mut self, owner: NodeId, mut prim: SectorPrim) -> u32 {
        if let Some(t) = self.tf() {
            // The centre is relative to the bounds' origin, so it scales on its own.
            prim.bounds = t.apply_rect(prim.bounds);
            prim.sector.centre = prim.sector.centre * t.scale;
            prim.sector.radius *= t.scale;
            prim.sector.thickness *= t.scale;
        }
        let index = self.prims.sectors.len() as u32;
        self.prims.sectors.push(prim);
        self.owners.push(PrimKind::Sector, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Sector,
            index,
        }));
        index
    }

    /// Add one ellipse; a paint-only transform scales its radii with it.
    pub fn push_ellipse(&mut self, owner: NodeId, mut prim: EllipsePrim) -> u32 {
        if let Some(t) = self.tf() {
            prim.bounds = t.apply_rect(prim.bounds);
            prim.centre = prim.centre * t.scale;
            prim.radii = (prim.radii.0 * t.scale, prim.radii.1 * t.scale);
            prim.thickness *= t.scale;
        }
        let index = self.prims.ellipses.len() as u32;
        self.prims.ellipses.push(prim);
        self.owners.push(PrimKind::Ellipse, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Ellipse,
            index,
        }));
        index
    }

    /// Add one stroked segment; its endpoints scale with the current transform.
    pub fn push_line(&mut self, owner: NodeId, mut prim: LinePrim) -> u32 {
        if let Some(t) = self.tf() {
            prim.from = t.apply_point(prim.from);
            prim.to = t.apply_point(prim.to);
            prim.thickness *= t.scale;
        }
        let index = self.prims.lines.len() as u32;
        self.prims.lines.push(prim);
        self.owners.push(PrimKind::Line, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Line,
            index,
        }));
        index
    }

    pub fn push_shadow(&mut self, owner: NodeId, mut prim: ShadowPrim) -> u32 {
        if let Some(t) = self.tf() {
            prim.rect = t.apply_rect(prim.rect);
            prim.corners = t.apply_corners(prim.corners);
            prim.blur *= t.scale;
            prim.dy *= t.scale;
            prim.ring *= t.scale;
            if let Some(bound) = prim.bound.as_mut() {
                bound.rect = t.apply_rect(bound.rect);
            }
        }
        let index = self.prims.shadows.len() as u32;
        self.prims.shadows.push(prim);
        self.owners.push(PrimKind::Shadow, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Shadow,
            index,
        }));
        index
    }

    pub fn push_image_prim(&mut self, owner: NodeId, mut prim: ImagePrim) -> u32 {
        if let Some(t) = self.tf() {
            prim.rect = t.apply_rect(prim.rect);
            prim.corners = t.apply_corners(prim.corners);
        }
        let index = self.prims.images.len() as u32;
        self.prims.images.push(prim);
        self.owners.push(PrimKind::Image, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Image,
            index,
        }));
        index
    }

    pub fn push_glyph(&mut self, owner: NodeId, mut prim: GlyphPrim) -> u32 {
        if let Some(t) = self.tf() {
            // Scaling the destination quad scales the glyph; the atlas rect stays as it is.
            prim.rect = t.apply_rect(prim.rect);
        }
        let index = self.prims.glyphs.len() as u32;
        self.prims.glyphs.push(prim);
        self.owners.push(PrimKind::Glyph, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Glyph,
            index,
        }));
        index
    }

    pub fn push_text(&mut self, owner: NodeId, mut prim: TextPrim) -> u32 {
        // Glyphs do not exist yet, so the scale rides along and the pass applies it later.
        if let Some(t) = self.tf() {
            prim.zoom = Some(match prim.zoom {
                Some(inner) => t.then(inner),
                None => t,
            });
        }
        prim.shape_scale *= self.shape_scale;
        let index = self.prims.texts.len() as u32;
        self.prims.texts.push(prim);
        self.owners.push(PrimKind::Text, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Text,
            index,
        }));
        index
    }

    pub fn push_math(&mut self, owner: NodeId, mut prim: MathPrim) -> u32 {
        // Glyphs exist only after the pass rasterizes the face, so the scale rides along.
        if let Some(t) = self.tf() {
            prim.zoom = Some(match prim.zoom {
                Some(inner) => t.then(inner),
                None => t,
            });
        }
        prim.shape_scale *= self.shape_scale;
        let index = self.prims.maths.len() as u32;
        self.prims.maths.push(prim);
        self.owners.push(PrimKind::Math, owner);
        self.ops.push(Op::Prim(PrimRef {
            kind: PrimKind::Math,
            index,
        }));
        index
    }

    pub fn push_clip(&mut self, rect: Rect, corners: Corners) {
        self.push_clip_shaped(rect, corners, rect);
    }

    /// Push a clip whose corners belong to `shape`, an uncut box that an outer clip narrowed
    /// to `rect`: the cut edge stays square while the node's own corners stay round.
    pub fn push_clip_shaped(&mut self, rect: Rect, corners: Corners, shape: Rect) {
        let (rect, corners, shape) = match self.tf() {
            Some(t) => (
                t.apply_rect(rect),
                t.apply_corners(corners),
                t.apply_rect(shape),
            ),
            None => (rect, corners, shape),
        };
        self.clip_before.push(self.clip_now);
        self.clip_now = Some(match self.clip_now {
            Some(outer) => outer.intersection(rect).unwrap_or(Rect::ZERO),
            None => rect,
        });
        let index = self.clips.len() as u32;
        self.clips.push(ClipEntry {
            rect,
            corners,
            shape,
        });
        self.ops.push(Op::PushClip(index));
    }

    pub fn pop_clip(&mut self) {
        self.clip_now = self.clip_before.pop().flatten();
        self.ops.push(Op::PopClip);
    }

    /// The clip in force for pushes right now; `None` while nothing has clipped.
    pub(crate) fn clip_now(&self) -> Option<Rect> {
        self.clip_now
    }

    /// Push a layer that is rebuilt every frame; returns its index in `layers`.
    pub fn push_layer(
        &mut self,
        bounds: Rect,
        effects: Vec<crate::effect::Effect>,
        radius: Corners,
    ) -> u32 {
        self.push_layer_inner(bounds, effects, radius, None)
    }

    /// Push a layer whose target is cached across frames under `key`; returns its index.
    pub fn push_cached_layer(
        &mut self,
        key: u64,
        bounds: Rect,
        effects: Vec<crate::effect::Effect>,
        radius: Corners,
    ) -> u32 {
        self.push_layer_inner(bounds, effects, radius, Some(key))
    }

    fn push_layer_inner(
        &mut self,
        bounds: Rect,
        effects: Vec<crate::effect::Effect>,
        radius: Corners,
        cache_key: Option<u64>,
    ) -> u32 {
        // A scaled subtree's layer covers its scaled pixels, so its target scales too.
        let (bounds, radius) = match self.tf() {
            Some(t) => (t.apply_rect(bounds), t.apply_corners(radius)),
            None => (bounds, radius),
        };
        let index = self.layers.len() as u32;
        self.layers.push(LayerEntry {
            bounds,
            effects,
            radius,
            cache_key,
        });
        self.ops.push(Op::PushLayer(index));
        index
    }

    pub fn pop_layer(&mut self) {
        self.ops.push(Op::PopLayer);
    }

    pub fn push_backdrop(&mut self, mut entry: BackdropEntry) {
        if let Some(t) = self.tf() {
            entry.rect = t.apply_rect(entry.rect);
            entry.clip = t.apply_rect(entry.clip);
            entry.corners = t.apply_corners(entry.corners);
            entry.sigma *= t.scale;
        }
        let index = self.backdrops.len() as u32;
        self.backdrops.push(entry);
        self.ops.push(Op::Backdrop(index));
    }

    /// Backdrops in paint order, paired with the epoch after which each must run.
    pub fn backdrop_order(&self) -> Vec<(u32, BackdropEntry)> {
        let mut out = Vec::new();
        let mut epoch = 0u32;
        for op in &self.ops {
            if let Op::Backdrop(i) = *op {
                epoch += 1;
                if let Some(e) = self.backdrops.get(i as usize) {
                    out.push((epoch, *e));
                }
            }
        }
        out
    }

    /// Rect a primitive occupies, for damage and culling.
    pub fn prim_bounds(&self, pr: PrimRef) -> Rect {
        match pr.kind {
            PrimKind::Rect => self
                .prims
                .rects
                .get(pr.index as usize)
                .map(|p| p.rect)
                .unwrap_or(Rect::ZERO),
            PrimKind::Glyph => self
                .prims
                .glyphs
                .get(pr.index as usize)
                .map(|p| p.rect)
                .unwrap_or(Rect::ZERO),
            PrimKind::Image => self
                .prims
                .images
                .get(pr.index as usize)
                .map(|p| p.rect)
                .unwrap_or(Rect::ZERO),
            PrimKind::Shadow => self
                .prims
                .shadows
                .get(pr.index as usize)
                .map(|p| p.painted())
                .unwrap_or(Rect::ZERO),
            PrimKind::Sector => self
                .prims
                .sectors
                .get(pr.index as usize)
                .map(|p| p.bounds)
                .unwrap_or(Rect::ZERO),
            PrimKind::Math => self
                .prims
                .maths
                .get(pr.index as usize)
                .map(|p| Rect::new(p.origin.x, p.origin.y, p.formula.width, p.formula.height()))
                .unwrap_or(Rect::ZERO),
            PrimKind::Line => self
                .prims
                .lines
                .get(pr.index as usize)
                .map(LinePrim::bounds)
                .unwrap_or(Rect::ZERO),
            PrimKind::Ellipse => self
                .prims
                .ellipses
                .get(pr.index as usize)
                .map(|p| p.bounds)
                .unwrap_or(Rect::ZERO),
            PrimKind::Text => {
                let t = self.prims.texts.get(pr.index as usize);
                match t {
                    Some(t) => {
                        // Approximation: width unknown here, so report the origin and a
                        // generous box; text damage is refined by the owning node rect. The
                        // runs answer the length without building the span table, which a
                        // bounds query must not pay for on every text of a page.
                        let chars: usize =
                            t.block.runs.iter().map(|r| r.text.chars().count()).sum();
                        let size = t.block.runs.first().map_or(14.0, |r| r.style.size);
                        Rect::new(
                            t.origin.x,
                            t.origin.y,
                            (chars as f32 * size * 0.6).max(1.0),
                            size * 1.3,
                        )
                    }
                    None => Rect::ZERO,
                }
            }
        }
    }

    /// Group the order log into batches usable by a backend.
    ///
    /// A batch breaks when the kind, clip, or layer changes. Layers switch the
    /// render target, so primitives across them must never merge; this is why the
    /// plan is not a plain k-way merge of the typed banks.
    pub fn plan_batches(&self) -> Vec<Batch> {
        let mut out: Vec<Batch> = Vec::new();
        let mut clip_stack: Vec<ClipEntry> = Vec::new();
        let mut layer_stack: Vec<u32> = Vec::new();
        // A backdrop reads and rewrites the target, so nothing may batch across it.
        let mut backdrop_epoch = 0u32;

        for op in &self.ops {
            match *op {
                Op::PushClip(i) => {
                    if let Some(c) = self.clips.get(i as usize) {
                        clip_stack.push(*c);
                    }
                }
                Op::PopClip => {
                    clip_stack.pop();
                }
                Op::Backdrop(_) => backdrop_epoch += 1,
                Op::PushLayer(i) => layer_stack.push(i),
                Op::PopLayer => {
                    layer_stack.pop();
                }
                Op::Prim(pr) => {
                    // Rects intersect; the shape comes from the clip that still bounds the
                    // intersection, since a narrower square clip must not inherit outer curves.
                    let clip = clip_stack.split_first().map(|(first, rest)| {
                        let rect = rest.iter().fold(first.rect, |acc, c| {
                            acc.intersection(c.rect).unwrap_or(Rect::ZERO)
                        });
                        let shaped = clip_stack.iter().rev().find(|c| !c.corners.is_zero());
                        combined_clip(rect, shaped)
                    });
                    let layers = &layer_stack;
                    // Bounds are what the renderer culls by: narrower than the paint would
                    // drop that paint from every frame whose band misses the rest.
                    let bounds = self.prim_bounds(pr);
                    match out.last_mut() {
                        Some(last)
                            if last.kind == pr.kind
                                && last.clip == clip
                                && last.layers == *layers
                                && last.epoch == backdrop_epoch =>
                        {
                            last.range.end += 1;
                            last.bounds = Some(last.bounds.map_or(bounds, |u| u.union(bounds)));
                        }
                        _ => out.push(Batch {
                            kind: pr.kind,
                            range: pr.index..pr.index + 1,
                            clip,
                            layers: layers.clone(),
                            epoch: backdrop_epoch,
                            bounds: Some(bounds),
                        }),
                    }
                }
            }
        }
        out
    }
}

impl DrawList {
    /// The part of this list painted by `root` and its descendants, and the box it covers.
    ///
    /// Every primitive carries its owning node, so one node's painting can be taken out of
    /// a finished frame without asking the app to build again. Clips, layers and textures
    /// travel unchanged (the ops still index them the same way); only primitives of other
    /// nodes are left behind. Backdrops are left behind too: a frost reads the surface it
    /// sits on, and the surface this painting is taken out of is exactly what the caller is
    /// separating it from. This is what a drag image is made of: the element alone.
    pub fn subtree(&self, tree: &crate::tree::Tree, root: crate::id::NodeId) -> (DrawList, Rect) {
        let mut out = DrawList::new(self.canvas);
        out.clips.clone_from(&self.clips);
        out.layers.clone_from(&self.layers);
        out.backdrops.clone_from(&self.backdrops);
        out.images.clone_from(&self.images);
        let mut bounds: Option<Rect> = None;
        // Ancestry is asked once per node rather than once per primitive: `within` walks a
        // path back to the root, and a frame can paint dozens of prims for one node.
        let mut inside: crate::collections::FastMap<crate::id::NodeId, bool> =
            crate::collections::FastMap::default();
        for op in &self.ops {
            if matches!(op, Op::Backdrop(_)) {
                continue;
            }
            let Op::Prim(prim) = *op else {
                out.ops.push(*op);
                continue;
            };
            let index = prim.index as usize;
            let Some(owner) = self.owners.of(prim.kind, index) else {
                continue;
            };
            let member = *inside
                .entry(owner)
                .or_insert_with(|| tree.within(owner, root));
            if !member {
                continue;
            }
            match prim.kind {
                PrimKind::Rect => {
                    out.push_rect(owner, self.prims.rects[index]);
                }
                PrimKind::Text => {
                    out.push_text(owner, self.prims.texts[index].clone());
                }
                PrimKind::Glyph => {
                    out.push_glyph(owner, self.prims.glyphs[index]);
                }
                PrimKind::Image => {
                    out.push_image_prim(owner, self.prims.images[index]);
                }
                PrimKind::Shadow => {
                    out.push_shadow(owner, self.prims.shadows[index]);
                }
                PrimKind::Sector => {
                    out.push_sector(owner, self.prims.sectors[index]);
                }
                PrimKind::Math => {
                    out.push_math(owner, self.prims.maths[index].clone());
                }
                PrimKind::Line => {
                    out.push_line(owner, self.prims.lines[index]);
                }
                PrimKind::Ellipse => {
                    out.push_ellipse(owner, self.prims.ellipses[index]);
                }
            }
            let painted = self.prim_bounds(prim);
            bounds = Some(bounds.map_or(painted, |union| union.union(painted)));
        }
        (out, bounds.unwrap_or(Rect::ZERO))
    }
}

/// Area a node affects, including ink margin.
pub fn node_ink_rect(rect: Rect, ink: Insets) -> Rect {
    ink.expand(rect)
}

/// Convenience for tests and examples.
pub fn prim_ref(kind: PrimKind, index: u32) -> PrimRef {
    PrimRef { kind, index }
}

/// Owner of the primitive with this id, if any (debug helper).
pub fn owner_of(list: &DrawList, id: Id) -> Option<NodeId> {
    let _ = (list, id);
    None
}

/// The clip one batch is drawn under: the stack's intersection, with a corner kept round
/// only where the innermost rounded clip still owns it.
///
/// A card scrolled halfway out of its viewport keeps the corners that are still visible and
/// squares the edge the viewport cut through. Falling back to a square everywhere would let a
/// child's square fill bleed past the card's visible curve.
fn combined_clip(rect: Rect, shaped: Option<&ClipEntry>) -> ClipEntry {
    let Some(s) = shaped else {
        return ClipEntry {
            rect,
            corners: Corners::ZERO,
            shape: rect,
        };
    };
    let keep = |own: bool, radius: f32| if own { radius } else { 0.0 };
    let corners = Corners {
        tl: keep(
            s.rect.min_x() == rect.min_x() && s.rect.min_y() == rect.min_y(),
            s.corners.tl,
        ),
        tr: keep(
            s.rect.max_x() == rect.max_x() && s.rect.min_y() == rect.min_y(),
            s.corners.tr,
        ),
        br: keep(
            s.rect.max_x() == rect.max_x() && s.rect.max_y() == rect.max_y(),
            s.corners.br,
        ),
        bl: keep(
            s.rect.min_x() == rect.min_x() && s.rect.max_y() == rect.max_y(),
            s.corners.bl,
        ),
        shape: s.corners.shape,
    };
    ClipEntry {
        rect,
        corners,
        shape: s.shape,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::NodeId;

    fn nid(i: u32) -> NodeId {
        NodeId {
            index: i,
            generation: 0,
        }
    }

    fn rect_prim(w: f32) -> RectPrim {
        RectPrim {
            rect: Rect::new(0.0, 0.0, w, w),
            radius: Corners::ZERO,
            fill: Color::WHITE,
            gradient: None,
            border: None,
            border_gradient: None,
            notch: None,
        }
    }

    #[test]
    fn a_rounded_clip_keeps_the_corners_a_square_clip_did_not_cut() {
        // A card scrolled halfway out of its viewport: the cut edge is straight and the
        // corners still on screen stay round. A square everywhere would let a child's fill
        // bleed past the card's visible curve.
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 200.0, 200.0));
        list.push_clip(Rect::new(0.0, 20.0, 200.0, 200.0), Corners::ZERO);
        list.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0), Corners::all(12.0));
        list.push_rect(nid(0), rect_prim(20.0));
        list.pop_clip();
        list.pop_clip();
        let clip = list.plan_batches()[0].clip.expect("a clip is in force");
        assert_eq!(clip.rect, Rect::new(0.0, 20.0, 100.0, 80.0));
        assert_eq!(
            (clip.corners.tl, clip.corners.tr),
            (0.0, 0.0),
            "the edge the viewport cut through is straight"
        );
        assert_eq!(
            (clip.corners.bl, clip.corners.br),
            (12.0, 12.0),
            "the corners still on screen keep the card's curve"
        );

        // A square clip that merely coincides with the rounded one takes nothing away: the
        // intersection is still the rounded region.
        let mut inner = DrawList::new(Rect::new(0.0, 0.0, 200.0, 200.0));
        inner.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0), Corners::all(12.0));
        inner.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0), Corners::ZERO);
        inner.push_rect(nid(0), rect_prim(20.0));
        inner.pop_clip();
        inner.pop_clip();
        let clip = inner.plan_batches()[0].clip.expect("a clip is in force");
        assert_eq!(clip.corners.tl, 12.0, "the card's own corner stays round");
    }

    #[test]
    fn a_transform_scales_about_its_origin_and_composes_inside_out() {
        // The origin is the fixed point; composing two scales must equal applying them in
        // turn, or a nested scaled subtree would drift towards the wrong centre.
        let t = Transform::about(crate::geom::Point::new(20.0, 20.0), 0.5);
        let r = t.apply_rect(Rect::new(10.0, 10.0, 20.0, 20.0));
        assert_eq!(r, Rect::new(15.0, 15.0, 10.0, 10.0));
        assert_eq!(t.apply_corners(Corners::all(8.0)).tl, 4.0);
        assert_eq!(
            t.apply_point(crate::geom::Point::new(20.0, 20.0)),
            crate::geom::Point::new(20.0, 20.0)
        );

        let outer = Transform::about(crate::geom::Point::ZERO, 2.0);
        let composed = outer.then(t);
        let p = crate::geom::Point::new(3.0, 7.0);
        assert_eq!(composed.apply_point(p), outer.apply_point(t.apply_point(p)));
        // Two scales about different centres are not the identity when their factors cancel:
        // they leave a translation behind, which is why the offset is part of the type.
        assert_eq!(composed.scale, 1.0);
        assert_ne!(composed.offset, crate::geom::Point::ZERO);
    }

    #[test]
    fn a_batch_bounds_cover_the_whole_paint_of_every_primitive() {
        // The renderer skips a batch whose bounds miss the redraw band. Bounds narrower
        // than the paint drop it from every frame that band leaves alone — silently.
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 200.0, 200.0));
        let at = |x: f32, y: f32| RectPrim {
            rect: Rect::new(x, y, 20.0, 20.0),
            ..rect_prim(20.0)
        };
        list.push_rect(nid(0), at(10.0, 10.0));
        list.push_rect(nid(1), at(150.0, 150.0));
        let shadow = ShadowPrim {
            rect: Rect::new(60.0, 60.0, 20.0, 20.0),
            corners: Corners::ZERO,
            colour: Color::WHITE,
            blur: 8.0,
            dy: 0.0,
            ring: 0.0,
            bound: None,
        };
        list.push_shadow(nid(2), shadow);

        let batches = list.plan_batches();

        assert_eq!(batches.len(), 2, "a kind change splits the run");
        assert_eq!(
            batches[0].bounds,
            Some(Rect::new(10.0, 10.0, 160.0, 160.0)),
            "the union must span both rects"
        );
        assert_eq!(
            batches[1].bounds,
            Some(shadow.painted()),
            "a shadow's falloff paints outside its own rect"
        );
    }

    #[test]
    fn a_backdrop_breaks_the_batch_so_nothing_paints_across_it() {
        // A backdrop rewrites the target in place; a batch spanning it would draw rects
        // that were meant to land after the blur, or blur pixels already drawn over.
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_rect(nid(0), rect_prim(10.0));
        list.push_backdrop(BackdropEntry {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            clip: Rect::new(0.0, 0.0, 50.0, 50.0),
            corners: Corners::ZERO,
            radius: 6,
            sigma: 2.0,
            material: BackdropMaterial::PLAIN,
        });
        list.push_rect(nid(1), rect_prim(10.0));

        let batches = list.plan_batches();
        assert_eq!(batches.len(), 2, "the two rects must not share a batch");
        assert!(batches[0].epoch < batches[1].epoch);
    }
}
