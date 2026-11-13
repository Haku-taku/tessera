//! A fixed-height scroll container of rows; the engine owns the wheel offset.

use ui_core::geom::Corners;

use ui_core::{Decl, Id, NodeKind};
use ui_style::{RoleName, Style};

use crate::scroll::Scroll;

pub mod roles {
    pub const FILL: &str = "scrollerFill";
    pub const ROW_TEXT: &str = "rowText";
}

/// One scrollable list; the engine owns the offset and the caller owns the rows.
pub struct List<'a> {
    pub id: Id,
    pub width: f32,
    pub height: f32,
    /// The height of a row whose text fits on one line; a row that wraps grows past it.
    pub row_height: f32,
    /// Rows by index; the caller owns their content.
    pub rows: Vec<String>,
    pub pack: &'a Style,
}

impl<'a> List<'a> {
    pub fn resolve(&self) -> Decl {
        self.build(None).resolve()
    }

    /// Resolve against the frame's context, so the engine-owned scrollbar draws itself.
    ///
    /// Only the rows the viewport shows are declared: a thousand-row list is a scroll
    /// container, not a thousand nodes rebuilt every frame. `heights` is the caller's, because
    /// a row that wraps is only as tall as layout made it and that has to outlive the frame.
    pub fn resolve_in(
        &self,
        cx: &crate::cx::Cx<'_, '_>,
        heights: &mut crate::scroll::RowHeights,
    ) -> Decl {
        let window = heights.window(
            cx.core(),
            self.id,
            self.rows.len(),
            self.row_height * 4.0,
            |row| self.id.with(row),
        );
        self.build(Some(window)).resolve_in(cx)
    }

    /// The scroll container with this list's rows, before any scrollbar decoration.
    ///
    /// `window` narrows the rows to those in view; `None` declares every row, which is the
    /// context-free path a caller with no frame geometry has.
    fn build(&self, window: Option<crate::scroll::RowWindow>) -> Scroll {
        // Same rule as `Button`: the pack's own roles first, then the common plane.
        let fill = self
            .pack
            .roles
            .get(RoleName(roles::FILL))
            .unwrap_or_else(|| {
                self.pack.colour_or(
                    ui_design::CommonColor::Surface,
                    ui_core::geom::Color::TRANSPARENT,
                )
            });
        let text = self
            .pack
            .roles
            .get(RoleName(roles::ROW_TEXT))
            .unwrap_or_else(|| {
                self.pack.colour_or(
                    ui_design::CommonColor::OnSurface,
                    ui_core::geom::Color::WHITE,
                )
            });
        let size = self.pack.type_table.get(ui_style::TextRole::Caption).size;
        let radius = self.pack.metric_or(
            ui_design::CommonMetric::Radius,
            self.pack.metrics.control.radius,
        );

        // The unified scroll container owns the clip, the wheel and the offset; the list
        // only contributes its rows and a background.
        // The default thumb is translucent black, which vanishes on a dark list; it takes
        // the pack's own ink instead.
        let thumb = ui_core::geom::Color {
            a: 0.4,
            ..self.pack.colour_or(
                ui_design::CommonColor::OnSurface,
                ui_core::geom::Color::WHITE,
            )
        };
        let bar = crate::scroll::ScrollbarConf {
            thumb,
            ..crate::scroll::ScrollbarConf::default()
        };
        let mut list = Scroll::fixed(self.id, self.width, self.height)
            .background(fill, Corners::all(radius))
            .with_scrollbar(bar);
        let row_width = self.width
            - self.pack.metrics.control.padding.left
            - self.pack.metrics.control.padding.right;
        let (first, last, above, below) = match window {
            Some(window) => (window.first, window.last, window.above, window.below),
            None => (0, self.rows.len(), 0.0, 0.0),
        };
        if above > 0.0 {
            list = list.child(
                Decl::container(self.id.with("above"), NodeKind::Box)
                    .size(ui_core::SizeClass::fixed(row_width, above)),
            );
        }
        // The row's height is its text's, floored at `row_height`: a wrapped row keeps its
        // lines instead of spilling over the row below it.
        let row_size = ui_core::SizeClass {
            width: ui_core::AxisSize::fixed(row_width),
            height: ui_core::AxisSize::hug(self.row_height, self.row_height),
            grow_x: ui_core::Grow::No,
            grow_y: ui_core::Grow::No,
        };
        for (i, body) in self.rows.iter().enumerate().take(last).skip(first) {
            // Each row needs its own id: the tree keys nodes by `Id`, so duplicates
            // silently collapse into one node and only the last text survives.
            let row_id = self.id.with(i);
            list = list.child(
                Decl::container(row_id, NodeKind::Column)
                    .size(row_size)
                    .child(crate::text::leaf(
                        row_id.with("label"),
                        body.clone(),
                        size,
                        text,
                        ui_core::Weight::REGULAR,
                        row_width,
                        0.0,
                        0.0,
                    )),
            );
        }
        if below > 0.0 {
            list = list.child(
                Decl::container(self.id.with("below"), NodeKind::Box)
                    .size(ui_core::SizeClass::fixed(row_width, below)),
            );
        }
        list
    }
}
