//! MD3 outlined segmented button: a pill of equal segments that filter a choice.
//!
//! The app owns the selection set; the control only reports which segment was clicked, so
//! it serves single- and multi-select with no mode flag.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Point, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Change, CursorShape, Decl, Id, InkRadius, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, GroupPosition, IconSource, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{role, spec_ink};
use crate::spec::{OutlinedSegmentedButtonFlags, outlined_segmented_button_spec};

use crate::motion::REACTION;
/// Inset of one segment's content from its edges, when the neighbour is not there to share it.
const SEGMENT_PAD: f32 = 16.0;

/// One segment: the id the app knows it by, its label, and an optional 18dp leading mark.
pub struct Md3SegmentItem<'a> {
    pub id: Id,
    pub label: &'a str,
    /// `Some(src)` draws the mark in the segment's own ink before the label.
    pub icon: Option<IconSource>,
}

impl<'a> Md3SegmentItem<'a> {
    /// A text-only segment.
    pub fn text(id: Id, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
        }
    }

    /// A segment with a leading mark.
    pub fn with_icon(id: Id, label: &'a str, icon: IconSource) -> Self {
        Self {
            id,
            label,
            icon: Some(icon),
        }
    }
}

/// Everything the caller states about one segmented button.
pub struct SegmentedParams<'a> {
    pub id: Id,
    pub items: Vec<Md3SegmentItem<'a>>,
    /// Selected segments, in any order. Single-select is the app replacing it; multi is toggling.
    pub selected: Vec<Id>,
    pub disabled: bool,
}

/// One segment's resolved paint data.
struct Segment {
    id: Id,
    label: String,
    label_ink: Color,
    icon: Option<(IconSource, Color)>,
    corners: Corners,
    /// The selected fill; `None` when the segment's clear state shows the group's surface.
    fill: Option<Color>,
    /// Draw the seam on this segment's left edge; the first segment has none.
    seam: bool,
    layer: Option<Color>,
    press: Option<ui_core::PressInk>,
    ring: Option<(Color, f32)>,
    label_size: f32,
    label_weight: Weight,
}

/// An MD3 outlined segmented button.
pub struct Md3SegmentedButton {
    id: Id,
    height: f32,
    segments: Vec<Segment>,
    /// The group's outline: one ring around every segment, plus the seams between them.
    outline: Option<(Color, f32)>,
    /// The clicked segment, resolved at build time so `change` carries no state.
    clicked: Option<Id>,
    disabled: bool,
}

impl Md3SegmentedButton {
    pub fn new_with(
        options: Md3Options,
        params: SegmentedParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let SegmentedParams {
            id,
            items,
            selected,
            disabled,
        } = params;
        let with_icon = items.iter().any(|s| s.icon.is_some());
        let rest = outlined_segmented_button_spec(OutlinedSegmentedButtonFlags::NONE);
        let disabled = options.disabled || disabled || cx.state(id).disabled;
        let height = rest.container.height;
        let ring_colour = spec_ink(pack, rest.focus_indicator.color, 1.0);
        let ring_width = rest.focus_indicator.thickness;
        // One ring for the whole group and one seam between neighbours: each segment drawing
        // its own ring would put two lines in every seam.
        let outline = (rest.outline.width > 0.0).then(|| {
            (
                spec_ink(pack, rest.outline.color, rest.outline.opacity),
                rest.outline.width,
            )
        });
        let len = items.len();
        let mut segments = Vec::with_capacity(len);
        let mut clicked = None;
        for (k, item) in items.iter().enumerate() {
            let on = selected.contains(&item.id);
            let mut node = cx.at(item.id);
            let state = node.state();
            if state.activation() && !disabled {
                clicked = Some(item.id);
            }
            let spec = outlined_segmented_button_spec(OutlinedSegmentedButtonFlags {
                selected: on,
                with_icon,
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
                disabled,
            });
            // The layer's ink follows the segment's own selection: `onSurface` clear,
            // `onSecondaryContainer` when filled. Read it off the hovered set.
            let layer_ink = role(
                pack,
                outlined_segmented_button_spec(OutlinedSegmentedButtonFlags {
                    selected: on,
                    with_icon,
                    hovered: true,
                    disabled,
                    ..OutlinedSegmentedButtonFlags::NONE
                })
                .state_layer
                .color,
            );
            let fill = on.then(|| spec_ink(pack, spec.container.color, 1.0));
            // Press is a splat at the pointer, so only hover and focus stay in the flat layer.
            let hover_layer = outlined_segmented_button_spec(OutlinedSegmentedButtonFlags {
                selected: on,
                with_icon,
                hovered: true,
                disabled,
                ..OutlinedSegmentedButtonFlags::NONE
            })
            .state_layer
            .opacity;
            let focus_layer = outlined_segmented_button_spec(OutlinedSegmentedButtonFlags {
                selected: on,
                with_icon,
                focused: true,
                disabled,
                ..OutlinedSegmentedButtonFlags::NONE
            })
            .state_layer
            .opacity;
            let press_layer = outlined_segmented_button_spec(OutlinedSegmentedButtonFlags {
                selected: on,
                with_icon,
                pressed: true,
                disabled,
                ..OutlinedSegmentedButtonFlags::NONE
            })
            .state_layer
            .opacity;
            let hover = node.animate(
                item.id.with(param::HOVER),
                f32::from(state.hovered),
                REACTION,
            );
            let focus = node.animate(
                item.id.with(param::FOCUS),
                f32::from(state.focus_visible),
                REACTION,
            );
            let alpha = (hover * hover_layer).max(focus * focus_layer);
            let layer = (alpha > 0.0 && !disabled).then_some(Color {
                a: layer_ink.a * alpha,
                ..layer_ink
            });
            // Declared every frame, not only while held: the engine owns the splat and fades
            // it out after release, so withdrawing the declaration would cut the ripple short.
            let press = (!disabled).then(|| {
                press_ink(
                    Color {
                        a: layer_ink.a * press_layer,
                        ..layer_ink
                    },
                    InkRadius::Cover,
                )
            });
            let ring = (!disabled && state.focus_visible).then(|| {
                let t = node.animate(item.id.with(param::THICKNESS), 1.0, REACTION);
                (
                    Color {
                        a: ring_colour.a * t,
                        ..ring_colour
                    },
                    ring_width,
                )
            });
            let corners = GroupPosition::new(k, len).corners(height * 0.5, 0.0);
            segments.push(Segment {
                id: item.id,
                label: item.label.to_string(),
                label_ink: spec_ink(pack, spec.label_text.color, spec.label_text.opacity),
                icon: item
                    .icon
                    .map(|f| (f, spec_ink(pack, spec.icon.color, spec.icon.opacity))),
                corners,
                fill,
                seam: k > 0,
                layer,
                press,
                ring,
                label_size: spec.label_text.size,
                label_weight: Weight(spec.label_text.weight as u16),
            });
        }
        Self {
            id,
            height,
            segments,
            outline,
            clicked,
            disabled,
        }
    }

    /// The bar's height, so a caller can align what sits beside it.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Which segment a point in the bar's space belongs to; segments are equal shares.
    pub fn segment_at(&self, local: Point, width: f32) -> Option<usize> {
        let n = self.segments.len();
        if n == 0 || width <= 0.0 || local.y < 0.0 || local.y > self.height {
            return None;
        }
        let share = width / n as f32;
        Some(((local.x / share).floor().max(0.0) as usize).min(n - 1))
    }
}

impl Widget for Md3SegmentedButton {
    fn handles_disabled(&self) -> bool {
        // The export carries 38% content and 12% outline fades, already painted per segment.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        self.clicked.map(|id| Change::Selected(Some(id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let corners = Corners::all(self.height * 0.5);
        let mut row = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .corners(corners)
            // The ring is positioned against this row, so it can cover the whole bar.
            .relative()
            .disabled(self.disabled);
        for segment in &self.segments {
            row = row.child(self.segment_decl(segment));
        }
        if let Some((ink, width)) = self.outline {
            // One ring for the whole group, painted after the segments: a selected fill would
            // otherwise cover the border, which is what makes the selection read as outlined.
            row = row.child(
                Decl::container(self.id.with("outline"), NodeKind::Box)
                    .size(SizeClass::hug(0.0, 0.0))
                    .inset(Insets::ZERO)
                    .absolute()
                    // Paint-only: it covers every segment, so it must not take their presses.
                    .sense(Sense::PASS_THROUGH)
                    .shapes(vec![Shape::Box {
                        corners,
                        fill: Color::TRANSPARENT,
                        gradient: None,
                        border: Some((ink, width)),
                        shadow: None,
                        image: None,
                    }]),
            );
        }
        row
    }
}

impl Md3SegmentedButton {
    /// One segment: full ring or fill, its state layer, and the centred icon-and-label block.
    fn segment_decl(&self, segment: &Segment) -> Decl {
        let mut shapes = Vec::with_capacity(4);
        // The clear segment's surface is transparent over the group's pill; a fill means selected.
        if let Some(fill) = segment.fill {
            shapes.push(Shape::Box {
                corners: segment.corners,
                fill,
                gradient: None,
                // The group's ring and the seams already frame this segment; a third outline
                // would double every edge and need the selection state to draw itself.
                border: None,
                shadow: None,
                image: None,
            });
        }
        if let Some(layer) = segment.layer {
            shapes.push(Shape::Box {
                corners: segment.corners,
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        // One seam on this segment's left edge, not a ring: the group's own outline already
        // covers the perimeter, and two rings would draw every seam twice.
        if segment.seam
            && let Some((ink, width)) = self.outline
        {
            shapes.push(Shape::Rect {
                rect: Rect::new(0.0, 0.0, width, self.height),
                corners: Corners::ZERO,
                fill: ink,
                gradient: None,
                border: None,
            });
        }
        if let Some((ink, width)) = segment.ring {
            shapes.push(Shape::Box {
                corners: segment.corners,
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((ink, width)),
                shadow: None,
                image: None,
            });
        }
        let label = Text::new(
            segment.id.with("label"),
            segment.label.clone(),
            segment.label_size,
            segment.label_ink,
        )
        .weight(segment.label_weight)
        .resolve();
        let mut content = Decl::container(segment.id.with("content"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: SEGMENT_PAD,
                right: SEGMENT_PAD,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(8.0)
            .cross_align(0.0)
            .cross_pos(0.5);
        if let Some((source, ink)) = segment.icon {
            content = content.child(source.resolve(segment.id.with("icon"), ink));
        }
        content = content.child(label);
        Decl::container(segment.id, NodeKind::Column)
            .size(SizeClass {
                // Equal shares: a zero-basis grower splits the bar whatever the labels are.
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::Split(1),
                grow_y: Grow::No,
            })
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(segment.press)
            // The segment's own curve, so the ripple is cut to the end segments' rounding.
            .corners(segment.corners)
            .shapes(shapes)
            .child(content)
    }
}
