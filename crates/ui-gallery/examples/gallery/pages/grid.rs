// The grid page: three L0 track configurations under real geometry.

use crate::shell::*;
use ui_core::layout::{Align, AutoFlow, Len, Repeat, Track, TrackBound, TrackList};

/// Every sample uses the same gap, so the self-test can check it without reading the tree.
pub(crate) const GRID_GAP: f32 = 8.0;

/// tag, item count, whether the tracks fill the box (an `fr` track exists).
pub(crate) const GRID_SAMPLES: [(&str, usize, bool); 3] =
    [("md3", 5, true), ("fluent", 6, false), ("glass", 8, true)];

/// The dense sample's `minmax` bounds; the self-test reads the same numbers.
pub(crate) const DENSE_MIN: f32 = 72.0;
pub(crate) const DENSE_MAX: f32 = 120.0;

/// Items in the dense sample: one pinned, one two-column, the rest auto-flowed.
pub(crate) const DENSE_ITEMS: usize = 14;

/// How many `repeat(auto-fill)` tracks fit `width` (the sample's content box).
pub(crate) fn dense_columns(width: f32) -> f32 {
    ((width + GRID_GAP) / (DENSE_MAX + GRID_GAP)).floor()
}

pub(crate) fn grid_id(tag: &'static str) -> Id {
    Id::of(("grid", Page::Grid.tag(), tag))
}

pub(crate) fn grid_item_id(tag: &'static str, index: usize) -> Id {
    grid_id(tag).with(index)
}

/// A grid item: a padded label. `hug` leaves it stretchable to the cell.
fn tile(tag: &'static str, index: usize, text: &str, fill: Color) -> Decl {
    let id = grid_item_id(tag, index);
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::hug(0.0, 0.0))
        .padding(Insets::all(8.0))
        .background(fill, Corners::all(6.0))
        .child(label(
            id.with("label"),
            text.to_string(),
            12.0,
            Color::from_rgba8(245, 247, 252, 255),
            // No wrap hint: an `auto` track must follow the content, not a declared width.
            0.0,
        ))
}

fn sample(tag: &'static str, cols: Vec<Track>, rows: Vec<Track>, items: Vec<Decl>) -> Decl {
    let mut grid = Decl::container(grid_id(tag), NodeKind::Grid)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .tracks(TrackList {
            cols,
            rows,
            auto_flow: AutoFlow::Row,
        })
        .gap(GRID_GAP);
    for item in items {
        grid = grid.child(item);
    }
    grid
}

/// `minmax` + `repeat(auto-fill)` + dense flow: pinned cells leave holes, and the
/// auto-flowed items come back to fill them instead of moving on.
fn dense_sample(width: f32, pinned: &str, two: &str) -> Decl {
    let mut grid = Decl::container(grid_id("dense"), NodeKind::Grid)
        .size(SizeClass {
            width: AxisSize::fill(0.0, width),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .tracks(TrackList {
            cols: vec![Track::Repeat(
                Repeat::AutoFill,
                Box::new(Track::MinMax(
                    TrackBound::Len(Len::Px(DENSE_MIN)),
                    TrackBound::Len(Len::Px(DENSE_MAX)),
                )),
            )],
            // Explicit rows: an empty implicit row has no intrinsic height.
            rows: vec![Track::Fixed(40.0), Track::Fixed(40.0)],
            auto_flow: AutoFlow::RowDense,
        })
        .gap(GRID_GAP);
    grid = grid.child(tile("dense", 0, pinned, amber()).grid_area(0, 1, 1, 1));
    grid = grid.child(tile("dense", 1, two, teal()).grid_span(1, 2));
    for i in 2..DENSE_ITEMS {
        grid = grid.child(tile("dense", i, "t", purple()));
    }
    grid
}

fn blue() -> Color {
    Color::from_rgba8(56, 92, 175, 255)
}

fn teal() -> Color {
    Color::from_rgba8(36, 130, 120, 255)
}

fn purple() -> Color {
    Color::from_rgba8(92, 70, 150, 255)
}

fn amber() -> Color {
    Color::from_rgba8(150, 96, 48, 255)
}

pub(crate) fn grid_page(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Grid.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .gap(20.0);

    // Fixed plus fr: the fixed track keeps 64, the fr pair splits the rest.
    let fixed = sample(
        "md3",
        vec![Track::Fixed(64.0), Track::Fr(1.0), Track::Fr(2.0)],
        vec![Track::Fixed(56.0), Track::Fixed(56.0)],
        vec![
            tile("md3", 0, "64", blue()),
            tile("md3", 1, "1fr", teal()),
            tile("md3", 2, "2fr", purple()),
            tile("md3", 3, &tr(cx, Msg::GALLERY_GRID_TILE_ROW2), amber()),
            tile(
                "md3",
                4,
                &tr(cx, Msg::GALLERY_GRID_TILE_SPAN),
                Color::from_rgba8(58, 130, 70, 255),
            )
            .grid_area(1, 1, 1, 2),
        ],
    );
    row = row.child(column(
        Page::Grid,
        "md3",
        Msg::GALLERY_GRID_COLUMN_MD3,
        from_decl(fixed),
        COL_CTRL_H,
        cx,
    ));

    // Auto tracks: each column is as wide as its widest label.
    let auto = sample(
        "fluent",
        vec![Track::Auto, Track::Auto, Track::Auto],
        vec![Track::Auto, Track::Auto],
        vec![
            tile("fluent", 0, &tr(cx, Msg::GALLERY_GRID_TILE_ONE), blue()),
            tile(
                "fluent",
                1,
                &tr(cx, Msg::GALLERY_GRID_TILE_LONG_LABEL),
                teal(),
            ),
            tile("fluent", 2, &tr(cx, Msg::GALLERY_GRID_TILE_MID), purple()),
            tile("fluent", 3, &tr(cx, Msg::GALLERY_GRID_TILE_SECOND), amber()),
            tile("fluent", 4, &tr(cx, Msg::GALLERY_GRID_TILE_ROW), blue()),
            tile("fluent", 5, &tr(cx, Msg::GALLERY_GRID_TILE_HERE), teal()),
        ],
    );
    row = row.child(column(
        Page::Grid,
        "fluent",
        Msg::GALLERY_GRID_COLUMN_FLUENT,
        from_decl(auto),
        COL_CTRL_H,
        cx,
    ));

    // Spans and per-item alignment: a two-column span, a two-row span, a centred tile.
    let spans = sample(
        "glass",
        vec![Track::Fr(1.0), Track::Fr(1.0), Track::Fr(1.0)],
        vec![
            Track::Fixed(44.0),
            Track::Fixed(44.0),
            Track::Fixed(44.0),
            Track::Fixed(44.0),
        ],
        vec![
            tile(
                "glass",
                0,
                &tr(cx, Msg::GALLERY_GRID_TILE_TWO_COLUMNS),
                blue(),
            )
            .grid_area(0, 0, 1, 2),
            tile("glass", 1, "c", teal()).grid_area(0, 2, 1, 1),
            tile(
                "glass",
                2,
                &tr(cx, Msg::GALLERY_GRID_TILE_TWO_ROWS),
                purple(),
            )
            .grid_area(1, 0, 2, 1),
            tile("glass", 3, "b", amber()).grid_area(1, 1, 1, 1),
            tile("glass", 4, "c", blue()).grid_area(1, 2, 1, 1),
            tile("glass", 5, "b", teal()).grid_area(2, 1, 1, 1),
            tile("glass", 6, "c", purple()).grid_area(2, 2, 1, 1),
            Decl::container(grid_item_id("glass", 7), NodeKind::Box)
                .size(SizeClass::fixed(80.0, 24.0))
                .background(Color::from_rgba8(120, 140, 210, 255), Corners::all(12.0))
                .justify_self(Align::Center)
                .align_self(Align::Center)
                .grid_area(3, 0, 1, 3)
                .child(label(
                    grid_item_id("glass", 7).with("label"),
                    tr(cx, Msg::GALLERY_GRID_TILE_CENTRED),
                    11.0,
                    Color::from_rgba8(20, 30, 50, 255),
                    80.0,
                )),
        ],
    );
    row = row.child(column(
        Page::Grid,
        "glass",
        Msg::GALLERY_GRID_COLUMN_GLASS,
        from_decl(spans),
        COL_CTRL_H,
        cx,
    ));

    // A second row: the L1 sample needs the full width to show how many tracks fit.
    let page = Container::new(Id::of(("pagecol", Page::Grid.tag())), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0)
        .child(Box::new(row))
        .child(column_w(
            Page::Grid,
            "dense",
            Msg::GALLERY_GRID_COLUMN_DENSE,
            from_decl(dense_sample(
                1176.0,
                &tr(cx, Msg::GALLERY_GRID_TILE_PINNED),
                &tr(cx, Msg::GALLERY_GRID_TILE_TWO_SPAN),
            )),
            1200.0,
            COL_CTRL_H,
            cx,
        ));
    Box::new(page)
}
