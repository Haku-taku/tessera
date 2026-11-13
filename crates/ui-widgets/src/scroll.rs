//! A unified scroll container: fixed or filling size, engine-owned offset, optional scrollbar.

use std::rc::Rc;

use ui_core::geom::{Color, Corners, Rect};
use ui_core::{AxisSize, Decl, Grow, Id, NodeKind, Sense, SizeClass};

use crate::cx::Cx;

/// The visible thumb's length and travel for a viewport and content, GPUI's sizing rule:
/// the thumb is the content's visible fraction, floored to `min`, and it travels the
/// leftover track exactly as the offset travels the content's overflow.
pub fn thumb_geometry(viewport: f32, extent: f32, offset: f32, min: f32) -> (f32, f32) {
    let overflow = (extent - viewport).max(0.0);
    if overflow <= 0.0 {
        // Nothing to scroll: a full-length, pinned thumb would be a lie.
        return (0.0, 0.0);
    }
    let size = (viewport * viewport / extent).max(min);
    let travel = (viewport - size).max(0.0);
    let start = (offset / overflow) * travel;
    (size, start)
}

/// When the scrollbar is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarVisibility {
    /// Show while the pointer is over the container or a wheel moved it recently, fade out
    /// after the activity stops. The default, matching GPUI's `Auto`.
    Auto,
    /// Always drawn, even when the content fits.
    Always,
    /// Never drawn; scrolling stays a wheel-only interaction.
    Never,
}

/// The scrollbar's visual treatment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarStyle {
    /// A slim floating thumb over the content. The default, matching GPUI's `Thin`.
    Thin,
    /// A sturdier thumb that reserves its own track.
    Solid,
}

/// Scrollbar appearance and behaviour; the geometry is computed fresh each frame from the
/// engine's scroll state, never stored.
#[derive(Clone, Copy, Debug)]
pub struct ScrollbarConf {
    pub visibility: ScrollbarVisibility,
    pub style: ScrollbarStyle,
    /// The shortest a thumb may be, so a huge content does not shrink it to nothing.
    pub thumb_min: f32,
    /// The thumb and track colours when the pack does not provide roles for them.
    pub thumb: Color,
    pub track: Color,
}

impl Default for ScrollbarConf {
    fn default() -> Self {
        Self {
            visibility: ScrollbarVisibility::Auto,
            style: ScrollbarStyle::Thin,
            thumb_min: 25.0,
            thumb: Color::from_rgba8(0, 0, 0, 64),
            track: Color::TRANSPARENT,
        }
    }
}

impl ScrollbarConf {
    /// A thin floating scrollbar that fades in on interaction (the default).
    pub fn auto() -> Self {
        Self::default()
    }

    /// A scrollbar that is always visible, no matter the scroll state.
    pub fn always() -> Self {
        Self {
            visibility: ScrollbarVisibility::Always,
            ..Self::default()
        }
    }

    /// No scrollbar: wheel-only scrolling.
    pub fn never() -> Self {
        Self {
            visibility: ScrollbarVisibility::Never,
            ..Self::default()
        }
    }

    /// Raise the minimum thumb length; long content otherwise shrinks it to a dot.
    pub fn thumb_min(mut self, min: f32) -> Self {
        self.thumb_min = min;
        self
    }
}

/// The viewport a window falls back to before the scroller has been laid out once.
const UNMEASURED_VIEWPORT: f32 = 800.0;

/// The rows of a long list that a viewport can show, and the space the rest takes.
///
/// A list of ten thousand rows declares ten thousand nodes every frame; a window declares the
/// visible ones between two spacers. The heights given here must be the heights the rows are
/// declared at, or the spacers and the scroll extent disagree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowWindow {
    /// First row in the window.
    pub first: usize,
    /// One past the last row in the window.
    pub last: usize,
    /// Height the rows before the window take.
    pub above: f32,
    /// Height the rows after it take.
    pub below: f32,
}

impl RowWindow {
    /// The rows meeting the scroller's viewport, widened by `overscan` on both sides.
    ///
    /// The scroller's geometry is one frame old, so a fling moves the content under a window
    /// chosen from the offset it had; `overscan` is what hides that. Before the first layout
    /// there is no viewport at all and the window assumes `UNMEASURED_VIEWPORT`.
    pub fn of(
        cx: &ui_core::cx::Cx<'_>,
        scroller: Id,
        count: usize,
        overscan: f32,
        height: impl Fn(usize) -> f32,
    ) -> Self {
        let (offset, viewport) = match cx.scroll_of(scroller) {
            Some(geom) => {
                // A rubber band paints the content past its end for a frame or two; the rows
                // at that end are still the ones on screen, so clamp before looking for them.
                let viewport = geom.viewport_h.max(0.0);
                let end = (geom.extent_h - viewport).max(0.0);
                (geom.offset_y.clamp(0.0, end), viewport)
            }
            None => (0.0, UNMEASURED_VIEWPORT),
        };
        let overscan = overscan.max(0.0);
        let top_edge = (offset - overscan).max(0.0);
        let bottom_edge = offset + viewport + overscan;

        // One pass finds the first row whose bottom passes the top edge, and the first row
        // that starts at or past the bottom one; both are `count` when nothing qualifies.
        let mut top = 0.0;
        let mut above = 0.0;
        let mut first = count;
        let mut last = count;
        for row in 0..count {
            let height = height(row).max(0.0);
            if first == count && top + height > top_edge {
                first = row;
                above = top;
            }
            if last == count && top >= bottom_edge {
                last = row;
            }
            top += height;
        }
        let total = top;
        if first == count {
            // No row reaches the viewport: an empty list, or rows of no height.
            return Self {
                first: count,
                last: count,
                above: total,
                below: 0.0,
            };
        }
        let last = last.max(first);
        let mut shown = 0.0;
        for row in first..last {
            shown += height(row).max(0.0);
        }
        Self {
            first,
            last,
            above,
            below: (total - above - shown).max(0.0),
        }
    }
}

/// The heights a list's rows have been measured at, with an estimate standing in for the rows
/// that have never been laid out.
///
/// A row is measured by being declared: its box is read on the following frame, and a row that
/// has been on screen keeps its true height when scrolled back to. Only a row never seen is
/// estimated, so a jump into the middle of a list lands approximately and settles as the rows
/// around it are measured. A width change invalidates every measurement: start a fresh table.
pub struct RowHeights {
    /// The floor for an unmeasured row: a one-line row's height, as the caller declares it.
    estimate: f32,
    measured: std::collections::HashMap<usize, f32>,
    /// The height unmeasured rows are predicted at: the average of what layout has given the
    /// rows it declared, never below `estimate`.
    ///
    /// A fixed floor is always the shortest row, so a stretch never laid out scrolls past at
    /// the floor's rate — five times too fast for a list whose rows wrap to five lines — and
    /// the rate changes the moment the stretch is measured. The average of the rows already
    /// laid out predicts their neighbours within a line.
    predicted: f32,
    measured_sum: f32,
    /// The rows the last window declared; their boxes are what this frame reads.
    last: Option<(usize, usize)>,
}

impl RowHeights {
    /// A table whose unmeasured rows all take `estimate` until layout measures some.
    pub fn new(estimate: f32) -> Self {
        let estimate = if estimate.is_finite() {
            estimate.max(0.0)
        } else {
            0.0
        };
        Self {
            estimate,
            measured: std::collections::HashMap::new(),
            predicted: estimate,
            measured_sum: 0.0,
            last: None,
        }
    }

    /// The height of `row`: what layout gave it, or what the measured rows predict.
    pub fn height(&self, row: usize) -> f32 {
        self.measured.get(&row).copied().unwrap_or(self.predicted)
    }

    /// The rows meeting `scroller`'s viewport, after learning the heights the last window
    /// declared through `row_id`.
    pub fn window(
        &mut self,
        cx: &ui_core::cx::Cx<'_>,
        scroller: Id,
        count: usize,
        overscan: f32,
        row_id: impl Fn(usize) -> Id,
    ) -> RowWindow {
        if let Some((first, last)) = self.last {
            for row in first..last.min(count) {
                let Some(rect) = cx.rect_of(row_id(row)) else {
                    continue;
                };
                let height = rect.size.h;
                if height.is_finite() && height >= 0.0 && self.measured.get(&row) != Some(&height) {
                    if let Some(previous) = self.measured.insert(row, height) {
                        self.measured_sum -= previous;
                    }
                    self.measured_sum += height;
                    let count = self.measured.len() as f32;
                    self.predicted = (self.measured_sum / count).max(self.estimate);
                }
            }
        }
        let window = RowWindow::of(cx, scroller, count, overscan, |row| self.height(row));
        self.last = Some((window.first, window.last));
        window
    }
}

/// A scroll container. The engine owns the offset: a wheel routes here, the engine
/// accumulates and clamps it, and the app never sees a `Value` change for it.
#[derive(Clone)]
pub struct Scroll {
    id: Id,
    size: SizeClass,
    /// The axis the content stacks along: a strip is a `Row`, everything else a `Column`.
    kind: NodeKind,
    /// Scroll axes enabled; `[x, y]`. Default `[false, true]`.
    axis: [bool; 2],
    children: Vec<Rc<Decl>>,
    scrollbar: ScrollbarConf,
    /// A fill behind the content, so a list need not wrap the container to paint one.
    background: Option<(Color, Corners)>,
    /// Page size per axis for a strip that rests on whole pages; zero leaves the axis free.
    snap: [f32; 2],
}

impl Scroll {
    /// A vertical scroll container sized by its content height up to `max_height`.
    pub fn vertical(id: Id, max_height: f32) -> Self {
        Self {
            id,
            size: SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, max_height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            kind: NodeKind::Column,
            axis: [false, true],
            children: Vec::new(),
            scrollbar: ScrollbarConf::default(),
            background: None,
            snap: [0.0, 0.0],
        }
    }

    /// A fixed-size strip that scrolls sideways only; the upright wheel still moves it.
    pub fn horizontal(id: Id, width: f32, height: f32) -> Self {
        Self {
            kind: NodeKind::Row,
            axis: [true, false],
            ..Self::fixed(id, width, height)
        }
    }

    /// Both axes scrollable, filling the parent on both axes.
    pub fn both(id: Id) -> Self {
        Self {
            id,
            size: SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            kind: NodeKind::Column,
            axis: [true, true],
            children: Vec::new(),
            scrollbar: ScrollbarConf::default(),
            background: None,
            snap: [0.0, 0.0],
        }
    }

    /// A fixed-size scroll container (Qt's `widgetResizable=false`: content is its own size,
    /// the viewport clips it).
    pub fn fixed(id: Id, width: f32, height: f32) -> Self {
        Self {
            id,
            size: SizeClass::fixed(width, height),
            kind: NodeKind::Column,
            axis: [false, true],
            children: Vec::new(),
            scrollbar: ScrollbarConf::default(),
            background: None,
            snap: [0.0, 0.0],
        }
    }

    /// Add a child; call repeatedly for several.
    pub fn child(mut self, child: Decl) -> Self {
        self.children.push(Rc::new(child));
        self
    }

    /// Configure the scrollbar; defaults to an auto-fading thin one.
    pub fn with_scrollbar(mut self, conf: ScrollbarConf) -> Self {
        self.scrollbar = conf;
        self
    }

    /// Hide the scrollbar entirely.
    pub fn no_scrollbar(mut self) -> Self {
        self.scrollbar = ScrollbarConf::never();
        self
    }

    /// Paint a fill behind the content, with the container's own corners.
    pub fn background(mut self, fill: Color, corners: Corners) -> Self {
        self.background = Some((fill, corners));
        self
    }

    /// Come to rest on multiples of `page` on every axis this container scrolls.
    pub fn snap(mut self, page: f32) -> Self {
        self.snap = [
            if self.axis[0] { page.max(0.0) } else { 0.0 },
            if self.axis[1] { page.max(0.0) } else { 0.0 },
        ];
        self
    }

    /// Resolve to this frame's declaration. The node takes the wheel itself. No scrollbar
    /// is drawn: without a context there is no geometry to compute one from.
    pub fn resolve(&self) -> Decl {
        self.base()
    }

    /// Resolve against this frame's context, so a scrollbar can read the engine's scroll
    /// geometry and draw itself. Prefer this in a build.
    pub fn resolve_in(&self, cx: &Cx<'_, '_>) -> Decl {
        let decl = self.base();
        self.decorate(decl, cx)
    }

    /// The container as an element, so a live control can sit inside it. A `Decl` has
    /// nowhere to carry `change()`, so content built as declarations is paint-only.
    pub fn element_in(&self, cx: &Cx<'_, '_>) -> crate::Container {
        crate::Container::from_decl(self.decorate(self.frame(), cx))
    }

    /// The container declaration without any scrollbar decoration.
    fn base(&self) -> Decl {
        let mut decl = self.frame();
        for child in &self.children {
            decl = decl.child(child.clone());
        }
        decl
    }

    /// The container's own declaration, childless: the one definition of what a scroller is.
    fn frame(&self) -> Decl {
        let mut clip = ui_core::layout::Clip {
            x: ui_core::layout::Overflow::Hidden,
            y: ui_core::layout::Overflow::Hidden,
        };
        if self.axis[0] {
            clip.x = ui_core::layout::Overflow::Scroll;
        }
        if self.axis[1] {
            clip.y = ui_core::layout::Overflow::Scroll;
        }
        let mut decl = Decl::container(self.id, self.kind)
            .size(self.size)
            // `HOVER` puts the container on the hover path, which is how the engine knows the
            // pointer is on the scroller and keeps an auto scrollbar shown; it does not change
            // how the wheel routes.
            .sense(Sense::HOVER | Sense::SCROLL_OWNED)
            .clip(clip)
            .snap(self.snap);
        if let Some((fill, corners)) = self.background {
            decl = decl.background(fill, corners);
        }
        decl
    }

    /// Lay the scrollbar over the container as drawn shapes, in the container's own space.
    fn decorate(&self, mut decl: Decl, cx: &Cx<'_, '_>) -> Decl {
        if self.scrollbar.visibility == ScrollbarVisibility::Never {
            return decl;
        }
        let Some(geom) = cx.core().scroll_of(self.id) else {
            return decl;
        };
        let auto = self.scrollbar.visibility == ScrollbarVisibility::Auto;
        let thin = self.scrollbar.style == ScrollbarStyle::Thin;
        let thickness = if thin { 4.0 } else { 8.0 };
        // Content that fits has nothing to scroll, so an automatic bar would be a lie. The
        // engine decides when this bar shows: it owns the activity clock the fade reads, and
        // a widget cannot fade what it stops declaring.
        let wanted = |vertical: bool| {
            let (extent, viewport) = if vertical {
                (geom.extent_h, geom.viewport_h)
            } else {
                (geom.extent_w, geom.viewport_w)
            };
            (if vertical { self.axis[1] } else { self.axis[0] }) && !(auto && extent <= viewport)
        };
        // The thumb travels the track as the offset travels its range, GPUI's rule; its
        // length is the visible fraction of the content, floored to a minimum. The geometry
        // is the previous frame's (see `set_scrolls`), which is the right anchor for the
        // shapes: they land in the container's own space, and the layout shift of scrolled
        // children never touches them.
        let bar = |vertical: bool| {
            let (viewport, extent, offset) = if vertical {
                (geom.viewport_h, geom.extent_h, geom.offset_y)
            } else {
                (geom.viewport_w, geom.extent_w, geom.offset_x)
            };
            let (size, start) = if extent <= viewport {
                // A bar that is always shown still has to say something: a thumb filling the
                // track, with nowhere to travel.
                (viewport, 0.0)
            } else {
                thumb_geometry(viewport, extent, offset, self.scrollbar.thumb_min)
            };
            let (thumb, track) = if vertical {
                let x = if thin {
                    (geom.viewport_w - thickness).max(0.0)
                } else {
                    geom.viewport_w
                };
                (
                    Rect::new(x, start, thickness, size),
                    Rect::new(x, 0.0, thickness, viewport),
                )
            } else {
                let y = if thin {
                    (geom.viewport_h - thickness).max(0.0)
                } else {
                    geom.viewport_h
                };
                (
                    Rect::new(start, y, size, thickness),
                    Rect::new(0.0, y, viewport, thickness),
                )
            };
            ui_core::ScrollbarBar {
                thumb,
                track,
                track_fill: self.scrollbar.track,
                thumb_fill: self.scrollbar.thumb,
                track_corners: Corners::all(thickness * 0.5),
                thumb_corners: Corners::all(thickness * 0.5),
            }
        };
        let vertical = wanted(true).then(|| bar(true));
        let horizontal = wanted(false).then(|| bar(false));
        if vertical.is_none() && horizontal.is_none() {
            return decl;
        }
        if !thin {
            // A solid bar owns its track instead of floating over the content: the inset
            // takes the thickness out of the content box, and the bar then sits in it.
            decl = decl.padding(ui_core::geom::Insets {
                right: if vertical.is_some() { thickness } else { 0.0 },
                bottom: if horizontal.is_some() { thickness } else { 0.0 },
                ..ui_core::geom::Insets::ZERO
            });
        }
        // The bars are handed to the engine rather than drawn as shapes: they have to land
        // above the content, and the same rects are the hit area.
        if let Some(params) = decl.data.params_mut() {
            params.scrollbar = Some(ui_core::Scrollbar {
                vertical,
                horizontal,
                fade: auto,
            });
        }
        decl
    }
}

/// A scroll container is a widget too: `cx.at_scope(..).element(Box::new(scroll))` turns it
/// into an element, and the scrollbar then reads this frame's context when built.
impl crate::widget::Widget for Scroll {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        self.resolve_in(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vertical_scroll_is_engine_owned_and_clips_y() {
        let scroll = Scroll::vertical(Id::str("s"), 200.0);
        let decl = scroll.resolve();
        assert!(
            decl.sense.contains(Sense::SCROLL_OWNED),
            "the engine must own this container's offset"
        );
        let params = decl.data.params();
        assert!(
            params.map(|p| p.clip.scrolls()).unwrap_or(false),
            "a scroll container must declare a scrolling clip"
        );
    }

    #[test]
    fn a_strip_pages_along_the_axis_it_scrolls() {
        let deck = Scroll::horizontal(Id::str("strip"), 360.0, 120.0).snap(360.0);
        let decl = deck.resolve();
        let params = decl.data.params().expect("a container");
        assert_eq!(
            params.snap,
            [360.0, 0.0],
            "a sideways strip pages on x alone"
        );
        assert_eq!(
            (params.clip.x, params.clip.y),
            (
                ui_core::layout::Overflow::Scroll,
                ui_core::layout::Overflow::Hidden
            ),
            "and scrolls no other axis"
        );
        let page = Scroll::vertical(Id::str("page"), 400.0).snap(400.0);
        let decl = page.resolve();
        let params = decl.data.params().expect("a container");
        assert_eq!(
            params.snap,
            [0.0, 400.0],
            "a vertical page pages on y alone"
        );
    }

    #[test]
    fn a_thumb_tracks_the_offset_over_the_overflow_and_shrinks_with_visible_fraction() {
        // GPUI's rule: the thumb is the content's visible fraction of the viewport, floored
        // to a minimum, and it travels the leftover track exactly as the offset travels the
        // overflow. At the top it sits at 0; at the bottom it reaches `viewport - size`.
        // `viewport - size`.
        let (size, start) = thumb_geometry(100.0, 300.0, 0.0, 25.0);
        assert!(
            (size - 33.333).abs() < 0.01,
            "visible fraction of the viewport, got {size}"
        );
        assert!(start == 0.0, "top offset pins the thumb at the top");
        // Half the overflow, half the travel.
        let (_, mid) = thumb_geometry(100.0, 300.0, 100.0, 25.0);
        let travel = 100.0 - size;
        assert!(
            (mid - travel * 0.5).abs() < 0.01,
            "half overflow, half travel, got {mid}"
        );
        // At the bottom, the thumb's tail touches the track's end.
        let (_, bottom) = thumb_geometry(100.0, 300.0, 200.0, 25.0);
        assert!(
            (bottom + size - 100.0).abs() < 0.01,
            "bottom offset bottoms the thumb"
        );
    }

    /// Rows alternate 24 and 40 tall, so a window cannot be an index times a constant.
    fn row_height(row: usize) -> f32 {
        if row % 2 == 0 { 24.0 } else { 40.0 }
    }

    fn window_at(offset: f32, viewport: f32, count: usize, overscan: f32) -> RowWindow {
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        core.set_scrolls(vec![ui_core::cx::ScrollGeom {
            id: Id::str("s"),
            extent_w: 200.0,
            extent_h: slice_height(count),
            viewport_w: 200.0,
            viewport_h: viewport,
            offset_x: 0.0,
            offset_y: offset,
            active: false,
        }]);
        RowWindow::of(&core, Id::str("s"), count, overscan, row_height)
    }

    fn slice_height(count: usize) -> f32 {
        (0..count).map(row_height).sum()
    }

    #[test]
    fn a_window_is_the_rows_the_viewport_shows_and_the_space_around_them() {
        // Unscrolled: the first four rows cover the 100-tall viewport (24+40+24+40 = 128).
        let top = window_at(0.0, 100.0, 10_000, 0.0);
        assert_eq!((top.first, top.last), (0, 4), "the rows in view: {top:?}");
        assert_eq!(top.above, 0.0);
        assert!(
            (top.below - (slice_height(10_000) - slice_height(4))).abs() < 0.01,
            "the rows below keep the extent: {top:?}"
        );

        // Scrolled into the middle: the rows above keep their own heights, which the
        // alternating pattern makes impossible to confuse with an average.
        let middle = window_at(200.0, 100.0, 10_000, 0.0);
        assert_eq!((middle.first, middle.last), (6, 10), "{middle:?}");
        assert!(
            (middle.above - slice_height(6)).abs() < 0.01,
            "the spacer above is the rows before the window: {middle:?}"
        );
        assert!(
            (middle.below - (slice_height(10_000) - slice_height(10))).abs() < 0.01,
            "the spacer below is the rows after it: {middle:?}"
        );

        // The overscan widens the window on both sides.
        let widened = window_at(200.0, 100.0, 10_000, 100.0);
        assert!(widened.first < middle.first && widened.last > middle.last);
    }

    #[test]
    fn an_offset_past_the_end_shows_the_last_rows_rather_than_nothing() {
        // A rubber band paints past the end for a frame or two; the rows at that end are
        // still what the viewport shows, so the window clamps the offset before choosing.
        let past = window_at(10_000_000.0, 100.0, 100, 0.0);
        assert_eq!(past.last, 100, "the window reaches the last row: {past:?}");
        assert!(past.first > 0 && past.below == 0.0, "{past:?}");
    }

    #[test]
    fn a_window_before_the_first_layout_assumes_a_viewport() {
        // No geometry yet: the window must still fill the frame with rows, or the first
        // frame shows an empty list until some event arrives.
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        let first = RowWindow::of(&core, Id::str("s"), 10_000, 0.0, row_height);
        assert_eq!(first.first, 0);
        assert!(
            first.last > 0 && first.above == 0.0,
            "the assumption fills the window: {first:?}"
        );
    }

    #[test]
    fn a_minimum_thumb_survives_a_huge_content() {
        // A 5000-tall content in a 100-tall viewport would shrink the thumb to a dot; the
        // minimum floors it.
        let (size, _) = thumb_geometry(100.0, 5000.0, 0.0, 25.0);
        assert_eq!(size, 25.0, "the minimum thumb length must hold");
    }

    #[test]
    fn an_active_scroller_draws_its_thumb_in_the_containers_space() {
        // The engine fed geometry for the scroller (content 300 in a 100 viewport) and a
        // wheel marked it active: the auto scrollbar must draw one thumb shape, anchored to
        // the viewport's right edge, not to the scrolled content.
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        core.set_scrolls(vec![ui_core::cx::ScrollGeom {
            id: Id::str("s"),
            extent_w: 200.0,
            extent_h: 300.0,
            viewport_w: 200.0,
            viewport_h: 100.0,
            offset_x: 0.0,
            offset_y: 40.0,
            active: true,
        }]);
        let cx = Cx::new(&mut core, ui_style::Env::root());
        let scroll = Scroll::fixed(Id::str("s"), 200.0, 100.0)
            .child(Decl::container(Id::str("c"), NodeKind::Box));
        let decl = scroll.resolve_in(&cx);
        // The bar is the engine's to paint, after the content: as a container shape it would
        // be drawn under the very children it scrolls.
        assert!(
            decl.shapes.is_empty(),
            "the bar is not one of the container's own shapes"
        );
        // The thumb is a capsule hugging the right edge, sized and placed by the geometry.
        let bar = decl
            .data
            .params()
            .and_then(|p| p.scrollbar)
            .and_then(|s| s.vertical)
            .expect("the visible bar is handed to the engine");
        assert_eq!(bar.thumb.size.w, 4.0, "thin thumb");
        assert!(
            (bar.thumb.origin.x - 196.0).abs() < 0.01,
            "hugging the right edge, got {}",
            bar.thumb.origin.x
        );
        assert_eq!(bar.thumb_fill, ScrollbarConf::default().thumb);
        let (want_size, want_start) = thumb_geometry(100.0, 300.0, 40.0, 25.0);
        assert!(
            (bar.thumb.size.h - want_size).abs() < 0.01,
            "thumb size from the geometry"
        );
        assert!(
            (bar.thumb.origin.y - want_start).abs() < 0.01,
            "thumb start from the geometry"
        );
        // The painted bar is also the hit area the engine drags; without it the thumb is
        // decoration the pointer falls straight through.
        assert_eq!(bar.track.size.h, 100.0, "the track spans the viewport");
    }

    #[test]
    fn an_always_bar_shows_a_full_track_when_the_content_fits() {
        // `Always` is literal: a scroller whose content happens to fit still shows where
        // its bar would be, so the user can tell the area scrolls once there is more.
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        core.set_scrolls(vec![ui_core::cx::ScrollGeom {
            id: Id::str("s"),
            extent_w: 200.0,
            extent_h: 100.0,
            viewport_w: 200.0,
            viewport_h: 100.0,
            offset_x: 0.0,
            offset_y: 0.0,
            active: false,
        }]);
        let cx = Cx::new(&mut core, ui_style::Env::root());
        let scroll =
            Scroll::fixed(Id::str("s"), 200.0, 100.0).with_scrollbar(ScrollbarConf::always());
        let decl = scroll.resolve_in(&cx);
        let bar = decl
            .data
            .params()
            .and_then(|p| p.scrollbar)
            .and_then(|s| s.vertical)
            .expect("an always-visible bar is drawn even with nothing to scroll");
        assert_eq!(bar.thumb.size.h, 100.0, "the thumb fills the track");
        assert_eq!(
            bar.thumb.origin.y, 0.0,
            "with nowhere to travel it stays at the top"
        );
    }

    #[test]
    fn a_two_axis_scroller_draws_one_bar_per_axis() {
        // A container that scrolls both ways needs both bars: the vertical one alone says
        // nothing about the sideways range, and there is no other handle on it.
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        core.set_scrolls(vec![ui_core::cx::ScrollGeom {
            id: Id::str("s"),
            extent_w: 600.0,
            extent_h: 300.0,
            viewport_w: 200.0,
            viewport_h: 100.0,
            offset_x: 0.0,
            offset_y: 40.0,
            active: true,
        }]);
        let cx = Cx::new(&mut core, ui_style::Env::root());
        let scroll = Scroll::both(Id::str("s")).with_scrollbar(ScrollbarConf::always());
        let decl = scroll.resolve_in(&cx);
        let bar = decl
            .data
            .params()
            .and_then(|p| p.scrollbar)
            .expect("both axes scroll");
        let vertical = bar.vertical.expect("the vertical axis has a bar");
        let horizontal = bar.horizontal.expect("the horizontal axis has a bar");
        assert_eq!(horizontal.thumb.size.h, 4.0, "a thin horizontal thumb");
        assert!(
            (horizontal.thumb.origin.y - 96.0).abs() < 0.01,
            "hugging the bottom edge, got {}",
            horizontal.thumb.origin.y
        );
        assert_eq!(
            horizontal.track.size.w, 200.0,
            "the track spans the viewport"
        );
        let (want_size, want_start) = thumb_geometry(200.0, 600.0, 0.0, 25.0);
        assert!((horizontal.thumb.size.w - want_size).abs() < 0.01);
        assert!((horizontal.thumb.origin.x - want_start).abs() < 0.01);
        assert_eq!(vertical.thumb.size.w, 4.0);
    }

    #[test]
    fn a_solid_bar_reserves_its_own_track() {
        // `Solid` is the classic scrollbar: the content stops short of the track instead of
        // scrolling under it, so the bar can never cover a row.
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        core.set_scrolls(vec![ui_core::cx::ScrollGeom {
            id: Id::str("s"),
            extent_w: 192.0,
            extent_h: 300.0,
            viewport_w: 192.0,
            viewport_h: 100.0,
            offset_x: 0.0,
            offset_y: 0.0,
            active: true,
        }]);
        let cx = Cx::new(&mut core, ui_style::Env::root());
        let scroll = Scroll::fixed(Id::str("s"), 200.0, 100.0).with_scrollbar(ScrollbarConf {
            style: ScrollbarStyle::Solid,
            ..ScrollbarConf::default()
        });
        let decl = scroll.resolve_in(&cx);
        assert_eq!(
            decl.box_model.padding.right, 8.0,
            "the track takes its width out of the content box"
        );
        let bar = decl
            .data
            .params()
            .and_then(|p| p.scrollbar)
            .and_then(|s| s.vertical)
            .expect("the bar is drawn");
        assert_eq!(bar.thumb.size.w, 8.0, "a solid thumb");
        assert_eq!(
            bar.thumb.origin.x, 192.0,
            "the bar sits in the inset, at the content box's edge"
        );
    }
}
