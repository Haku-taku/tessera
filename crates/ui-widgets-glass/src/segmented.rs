//! The glass segmented control: equal cells in a translucent track under a glass lens.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, OverlayAlign, SizeClass};
use ui_core::{AnimSpec, CursorShape, Decl, Easing, Id, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::{IconSource, Widget};

use crate::HigOptions;
use crate::paint::*;
use crate::shared::*;
use crate::text::LabelText;

/// One segment: the id it reports, and a label, an icon, or both.
#[derive(Clone, Debug)]
pub struct GlassSegment {
    pub id: Id,
    pub label: Option<String>,
    pub icon: Option<IconSource>,
    pub enabled: bool,
}

impl GlassSegment {
    pub fn label(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: Some(label.into()),
            icon: None,
            enabled: true,
        }
    }

    pub fn icon(id: Id, icon: IconSource) -> Self {
        Self {
            id,
            label: None,
            icon: Some(icon),
            enabled: true,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// One segmented control; every spring advances in `new`, `decl()` is pure.
pub struct GlassSegmentedControl {
    id: Id,
    segments: Vec<GlassSegment>,
    labels: Vec<Option<LabelText>>,
    selected: usize,
    /// Width in dp; the reference reads it from constraints.
    width: f32,
    align: f32,
    thickness: f32,
    jelly: (f32, f32),
    /// Per-segment selection crossfade, 0..=1.
    sel: Vec<f32>,
    ink: Color,
    /// The selection's accent: a wash over the whole indicator.
    accent: Color,
    track: Color,
    rest: Color,
    lens_backdrop: Option<ui_core::BackdropBlur>,
}

impl GlassSegmentedControl {
    pub fn new(
        id: Id,
        segments: Vec<GlassSegment>,
        selected: usize,
        width: f32,
        cx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(
            HigOptions::default(),
            id,
            segments,
            selected,
            width,
            cx,
            pack,
        )
    }

    pub fn new_with(
        _params: HigOptions,
        id: Id,
        segments: Vec<GlassSegment>,
        selected: usize,
        width: f32,
        cx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let n = segments.len().max(1);
        let selected = selected.min(n - 1);
        let ink = role_of(pack, "textFillColorPrimary");
        let accent = role_of(pack, "glassSelectionAccent");
        let motion = lens_motion(id, selected, n, width, &cx.state(), cx, LENS_SPRINGS);

        // ── Labels: 13 pt, w500 at 60 % unselected and w600 full ink selected
        // (`segmented_control_internal.dart:281-291`), the change tweened over 200 ms.
        let cell_w = (width - SEGMENT_INSET * 2.0) / n as f32;
        let labels: Vec<Option<LabelText>> = segments
            .iter()
            .map(|seg| {
                let text = seg.label.as_deref()?;
                LabelText::new(pack, ui_style::TextRole::Caption, text, ink, cell_w)
                    .map(|l| l.restyled(SEGMENT_FONT, Weight::MEDIUM))
            })
            .collect();
        let sel = segments
            .iter()
            .enumerate()
            .map(|(k, seg)| {
                cx.animate(
                    seg.id.with("selected"),
                    f32::from(k == selected),
                    SELECTION_TWEEN,
                )
            })
            .collect();

        Self {
            id,
            segments,
            labels,
            selected,
            width,
            align: motion.align,
            thickness: motion.thickness,
            jelly: motion.jelly,
            sel,
            ink,
            accent,
            track: role_of(pack, "glassSegmentTrack"),
            rest: role_of(pack, "glassSegmentRest"),
            lens_backdrop: glass_backdrop(
                &{
                    // The accent belongs to the resting pill: the lens carries it only while
                    // that pill is still there, and fades to clear as the glass blooms.
                    let mut glass =
                        crate::material::glass_recipe(pack, crate::material::Control::SegmentLens);
                    glass.tint = Color {
                        a: SEGMENT_LENS_TINT * rest_fade(motion.thickness),
                        ..accent
                    };
                    glass
                },
                motion.thickness.clamp(0.0, 1.0),
            ),
        }
    }
}

impl Widget for GlassSegmentedControl {
    fn handles_disabled(&self) -> bool {
        // Per-segment fades are painted here; the adapter must not fade the whole strip.
        true
    }

    fn change(
        &self,
        state: &ui_widgets::WidgetState,
        _bounds: ui_core::geom::Rect,
    ) -> Option<ui_core::Change> {
        if state.disabled {
            return None;
        }
        let n = self.segments.len().max(1);
        // Released: the drag lands on the segment it came to rest over.
        let index = if state.dropped {
            let rel = (self.align + 1.0) * 0.5;
            (rel * (n - 1) as f32).round().clamp(0.0, (n - 1) as f32) as usize
        } else if state.dragging || state.pressed || state.clicked {
            // The reference commits on press-down, so the lens starts moving immediately.
            let x = state.local.map_or(self.width * 0.5, |p| p.x);
            index_at_x(x, self.width, n)
        } else {
            return None;
        };
        let segment = self.segments.get(index)?;
        // A disabled segment never takes the selection, by tap or by drag.
        if index == self.selected || !segment.enabled {
            return None;
        }
        Some(ui_core::Change::Selected(Some(segment.id)))
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let n = self.segments.len().max(1);
        let inner_w = self.width - SEGMENT_INSET * 2.0;
        let cell_w = inner_w / n as f32;
        let cell_h = SEGMENT_H - SEGMENT_INSET * 2.0;

        // ── The track: the reference's 8 % black in light, 12 % white in dark.
        let track = Decl::container(self.id.with("track"), NodeKind::Box)
            .size(SizeClass::fixed(self.width, SEGMENT_H))
            .background(self.track, Corners::all(SEGMENT_H * 0.5));

        // ── The resting pill, faded out as the glass lens takes over.
        let rest_a = rest_fade(self.thickness);
        let geo = lens_geometry(
            self.align,
            self.thickness,
            self.jelly,
            inner_w,
            cell_w,
            SEGMENT_INSET,
            SEGMENT_H,
            (SEGMENT_EXPAND_H, SEGMENT_EXPAND_V),
        );
        let rest = Decl::container(self.id.with("rest"), NodeKind::Box)
            .size(SizeClass::fixed(geo.rest.size.w, geo.rest.size.h))
            .offset(geo.rest.origin.x, geo.rest.origin.y)
            .background(
                Color {
                    a: self.rest.a * rest_a,
                    ..self.rest
                },
                Corners::all(geo.rest.size.h * 0.5),
            );
        // The whole indicator wears the accent, not its label: a wash over the resting pill,
        // fading with it as the glass lens takes over.
        let accent = Decl::container(self.id.with("rest-accent"), NodeKind::Box)
            .size(SizeClass::fixed(geo.rest.size.w, geo.rest.size.h))
            .offset(geo.rest.origin.x, geo.rest.origin.y)
            .background(
                Color {
                    a: SEGMENT_ACCENT * rest_a,
                    ..self.accent
                },
                Corners::all(geo.rest.size.h * 0.5),
            );

        // ── Cells: equal shares, each centring its content in a Stack.
        let mut cells = Decl::container(self.id.with("cells"), NodeKind::Row)
            .size(SizeClass::fixed(self.width, SEGMENT_H))
            .padding(Insets {
                left: SEGMENT_INSET,
                top: SEGMENT_INSET,
                right: SEGMENT_INSET,
                bottom: SEGMENT_INSET,
            });
        for (k, segment) in self.segments.iter().enumerate() {
            let mut content = Decl::container(segment.id.with("content"), NodeKind::Column)
                .gap(SEGMENT_ICON_GAP)
                .cross_align(0.0)
                .cross_pos(0.5);
            let ink = self.label_ink(k);
            if let Some(icon) = segment.icon {
                let size = if segment.label.is_some() {
                    SEGMENT_ICON_WITH_LABEL
                } else {
                    SEGMENT_ICON
                };
                content = content.child(
                    ui_widgets::Icon {
                        id: segment.id.with("icon"),
                        source: icon,
                        size,
                        ink,
                    }
                    .resolve(),
                );
            }
            if let Some(label) = self.labels.get(k).and_then(|l| l.as_ref()) {
                content = content.child(self.styled_label(label, k).decl(segment.id.with("label")));
            }
            // The cell carries the segment's own id: that is what a click reports and what
            // a caller measures.
            let mut cell = Decl::container(segment.id, NodeKind::Stack)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fixed(cell_h),
                    grow_x: Grow::Split(1),
                    grow_y: Grow::No,
                })
                .stack_align(OverlayAlign::Center)
                .child(content);
            if !segment.enabled {
                cell = cell.opacity(SEGMENT_DISABLED);
            }
            cells = cells.child(cell);
        }

        // ── The lens: 12 dp per side and 8 dp up and down at full bloom.
        let mut lens = Decl::container(self.id.with("lens"), NodeKind::Box)
            .size(SizeClass::fixed(geo.lens.size.w, geo.lens.size.h))
            .offset(geo.lens.origin.x, geo.lens.origin.y)
            .background(Color::TRANSPARENT, Corners::all(geo.lens.size.h * 0.5));
        if let Some(backdrop) = self.lens_backdrop
            && let Some(p) = lens.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }

        let stack = Decl::container(self.id.with("stack"), NodeKind::Stack)
            .size(SizeClass::fixed(self.width, SEGMENT_H))
            .child(track)
            .child(rest)
            .child(accent)
            .child(cells)
            .child(lens);

        Decl::container(self.id, NodeKind::Box)
            .size(SizeClass::fixed(self.width, SEGMENT_H))
            .sense(Sense::HOVER | Sense::CLICK | Sense::DRAG | Sense::FOCUSABLE)
            .cursor(CursorShape::PointingHand)
            .child(stack)
    }
}

impl GlassSegmentedControl {
    /// The label ink at segment `k`: 60 % unselected fading to full selected.
    fn label_ink(&self, k: usize) -> Color {
        let sel = self.sel.get(k).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        Color {
            a: self.ink.a * (0.6 + 0.4 * sel),
            ..self.ink
        }
    }

    /// The label at segment `k`, with the selected weight once the tween passes halfway.
    fn styled_label(&self, label: &LabelText, k: usize) -> LabelText {
        let sel = self.sel.get(k).copied().unwrap_or(0.0);
        let weight = if sel > 0.5 {
            Weight::SEMIBOLD
        } else {
            Weight::MEDIUM
        };
        label
            .restyled(SEGMENT_FONT, weight)
            .inked(self.label_ink(k))
    }
}

/// The control's height, the reference's `GlassDefaults.heightControl`.
pub const SEGMENT_H: f32 = 32.0;
/// The track's padding, `EdgeInsets.all(2)`.
pub const SEGMENT_INSET: f32 = 2.0;
/// `indicatorExpansion`: the lens reaches 12 dp per side and 8 dp up and down at full bloom.
pub const SEGMENT_EXPAND_H: f32 = 12.0;
pub const SEGMENT_EXPAND_V: f32 = 8.0;
/// Both label styles are 13 pt; only the weight and the ink's alpha change.
pub const SEGMENT_FONT: f32 = 13.0;
/// `iconSize` 20 for an icon alone, 16 beside a label, 2 dp apart.
pub const SEGMENT_ICON: f32 = 20.0;
pub const SEGMENT_ICON_WITH_LABEL: f32 = 16.0;
pub const SEGMENT_ICON_GAP: f32 = 2.0;
/// A disabled segment's content, the reference's `Opacity(0.38)`.
pub const SEGMENT_DISABLED: f32 = 0.38;
/// How strongly the accent washes the resting indicator, as a straight-alpha mix.
pub const SEGMENT_ACCENT: f32 = 0.28;
/// The same wash on the glass lens, which carries it as the material's tint.
pub const SEGMENT_LENS_TINT: f32 = 0.22;

/// `AnimatedDefaultTextStyle(duration: 200ms)`: the label style tween.
const SELECTION_TWEEN: AnimSpec = AnimSpec::new(0.200, Easing::Linear);
