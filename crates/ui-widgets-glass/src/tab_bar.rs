//! The glass tab bar: a 64 dp capsule of icon+label tabs under a draggable glass lens.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::{CursorShape, Decl, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::{IconSource, Widget};

use crate::HigOptions;
use crate::paint::*;
use crate::shared::*;
use crate::text::LabelText;

/// One destination: the id it reports, its glyph and its label.
#[derive(Clone, Debug)]
pub struct GlassTab {
    pub id: Id,
    pub label: String,
    pub icon: IconSource,
}

/// One glass tab bar; every spring advances in `new`, `decl()` is pure.
pub struct GlassTabBar {
    id: Id,
    tabs: Vec<GlassTab>,
    labels: Vec<Option<LabelText>>,
    selected: usize,
    /// Width of the bar's capsule in dp; the reference reads it from constraints.
    width: f32,
    /// Indicator alignment, -1..=1, and its velocity in alignment units per second.
    align: f32,
    /// Lens bloom, 0..=1.
    thickness: f32,
    /// Jelly scale on the lens, x and y.
    jelly: (f32, f32),
    ink: Color,
    /// The ink of a tab seen through the lens: the product accent.
    accent: Color,
    rest: Color,
    bar_backdrop: Option<ui_core::BackdropBlur>,
    lens_backdrop: Option<ui_core::BackdropBlur>,
}

impl GlassTabBar {
    pub fn new(
        id: Id,
        tabs: Vec<GlassTab>,
        selected: usize,
        width: f32,
        cx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        Self::new_with(HigOptions::default(), id, tabs, selected, width, cx, pack)
    }

    pub fn new_with(
        _params: HigOptions,
        id: Id,
        tabs: Vec<GlassTab>,
        selected: usize,
        width: f32,
        cx: &mut ui_widgets::NodeCx<'_>,
        pack: &Style,
    ) -> Self {
        let n = tabs.len().max(1);
        let selected = selected.min(n - 1);
        let ink = role_of(pack, "textFillColorPrimary");
        let state = cx.state();
        let motion = lens_motion(id, selected, n, width, &state, cx, LENS_SPRINGS);
        let (align, thickness, jelly) = (motion.align, motion.thickness, motion.jelly);

        let cell_w = (width - INSET * 2.0) / n as f32;
        let labels = tabs
            .iter()
            .map(|tab| LabelText::new(pack, ui_style::TextRole::Caption, &tab.label, ink, cell_w))
            .collect();
        let lens_recipe = crate::material::glass_recipe(pack, crate::material::Control::TabLens);
        Self {
            id,
            tabs,
            labels,
            selected,
            width,
            align,
            thickness,
            jelly,
            ink,
            accent: role_of(pack, "glassSelectionAccent"),
            rest: role_of(pack, "glassTabRest"),
            bar_backdrop: glass_backdrop(
                &crate::material::glass_recipe(pack, crate::material::Control::TabBar),
                1.0,
            ),
            lens_backdrop: glass_backdrop(&lens_recipe, thickness.clamp(0.0, 1.0)),
        }
    }
}

impl Widget for GlassTabBar {
    fn handles_disabled(&self) -> bool {
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
        let n = self.tabs.len().max(1);
        // Released: the drag lands on the slot it came to rest over.
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
        let tab = self.tabs.get(index)?;
        (index != self.selected).then(|| ui_core::Change::Selected(Some(tab.id)))
    }

    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let n = self.tabs.len().max(1);
        let inner_w = self.width - INSET * 2.0;
        let cell_w = inner_w / n as f32;

        // ── The capsule: one glass surface at the bar's recipe.
        let mut bar = Decl::container(self.id.with("pill"), NodeKind::Box)
            .size(SizeClass::fixed(self.width, BAR_H))
            .background(Color::TRANSPARENT, Corners::all(BAR_H * 0.5));
        if let Some(backdrop) = self.bar_backdrop
            && let Some(p) = bar.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }

        // ── Stack: resting pill, tabs, the accent copy cut to the lens, then the glass.
        let rest_a = rest_fade(self.thickness);
        let geo = lens_geometry(
            self.align,
            self.thickness,
            self.jelly,
            inner_w,
            cell_w,
            INSET,
            BAR_H,
            (EXPAND_H, EXPAND_V),
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

        let icons = self.tabs_decl(self.ink, false);

        // The tab the glass covers is drawn again in the accent ink, cut to the lens: it is the
        // lens's input only, so it must never show outside the glass.
        // A Stack, not a Box: the copy keeps its own width and is only cut by the clip, so a
        // moving lens does not re-wrap the labels at a new width on every frame.
        let accent = Decl::container(self.id.with("accent"), NodeKind::Stack)
            .size(SizeClass::fixed(geo.lens.size.w, geo.lens.size.h))
            .offset(geo.lens.origin.x, geo.lens.origin.y)
            .background(Color::TRANSPARENT, Corners::all(geo.lens.size.h * 0.5))
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .child(
                self.tabs_decl(self.accent, true)
                    .offset(-geo.lens.origin.x, -geo.lens.origin.y),
            );

        // The lens grows out of the resting pill: 12 dp per side, 8 dp up and down.
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
            .size(SizeClass::fixed(self.width, BAR_H))
            // The bar's glass goes first: a backdrop pass runs where its node paints, so a
            // bar painted last would blur the icons it is supposed to sit under.
            .child(bar)
            .child(rest)
            .child(icons)
            .child(accent)
            .child(lens);

        let mut root = Decl::container(self.id, NodeKind::Box)
            .size(SizeClass::fixed(self.width, BAR_H))
            .sense(Sense::HOVER | Sense::CLICK | Sense::DRAG | Sense::FOCUSABLE)
            .cursor(CursorShape::PointingHand)
            .child(stack);
        root = root.padding(Insets {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        });
        root
    }
}

impl GlassTabBar {
    /// The icon+label row: equal shares, every tab the same size.
    ///
    /// `accent` builds the copy the lens shows; its ids carry their own salt so the two rows
    /// stay distinct nodes instead of merging by id.
    fn tabs_decl(&self, ink: Color, accent: bool) -> Decl {
        let row_id = match accent {
            true => self.id.with("accent-row"),
            false => self.id.with("icons"),
        };
        let mut row = Decl::container(row_id, NodeKind::Row)
            .size(SizeClass::fixed(self.width, BAR_H))
            .padding(Insets {
                left: INSET,
                top: 0.0,
                right: INSET,
                bottom: 0.0,
            });
        for (k, tab) in self.tabs.iter().enumerate() {
            // A tab keeps its size while the lens passes: the glass moves, the content does not.
            let base = match accent {
                true => tab.id.with("accent"),
                false => tab.id,
            };
            let icon = ui_widgets::Icon {
                id: base.with("icon"),
                source: tab.icon,
                size: ICON_SIZE,
                ink,
            };
            let mut content = Decl::container(base.with("content"), NodeKind::Column)
                .gap(ICON_LABEL_GAP)
                .cross_align(0.0)
                .cross_pos(0.5)
                .child(icon.resolve());
            if let Some(label) = self.labels.get(k).and_then(|l| l.as_ref()) {
                content = content.child(label.inked(ink).decl(base.with("label")));
            }
            // Icon and label are one block; centre that block in the cell instead of hanging
            // it from the capsule's top edge.
            let label_h = self
                .labels
                .get(k)
                .and_then(|l| l.as_ref())
                .map_or(0.0, |l| l.height());
            let block_h = ui_widgets::icon_extent(ICON_SIZE)
                + if label_h > 0.0 {
                    ICON_LABEL_GAP + label_h
                } else {
                    0.0
                };
            let cell = Decl::container(base.with("cell"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fixed(BAR_H),
                    grow_x: Grow::Split(1),
                    grow_y: Grow::No,
                })
                .padding(Insets {
                    left: 0.0,
                    right: 0.0,
                    top: ((BAR_H - block_h) * 0.5).max(0.0),
                    bottom: 0.0,
                })
                .cross_align(0.0)
                .cross_pos(0.5)
                .child(content);
            row = row.child(cell);
        }
        row
    }
}

/// The bar's capsule height, the reference's `barHeight`.
pub const BAR_H: f32 = 64.0;
/// The pill sits this far inside the capsule on every side.
pub const INSET: f32 = 4.0;
/// How far the lens reaches past its cell at full bloom, per side.
pub const EXPAND_H: f32 = 12.0;
pub const EXPAND_V: f32 = 8.0;
pub const ICON_SIZE: f32 = 24.0;
pub const ICON_LABEL_GAP: f32 = 4.0;
