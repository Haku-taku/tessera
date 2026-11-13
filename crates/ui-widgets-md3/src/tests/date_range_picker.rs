//! The MD3 date range picker: the selection rule, the month grid and the band.

use super::*;

use crate::date_picker::{Date, Weekday};
use crate::date_range_picker::{
    DateRangePickerParams, Md3DateRangePicker, grid_of, next_range, row_band,
};
use ui_core::layout::AxisSize;

const PICKER: Id = Id::str("date-range/modal");

fn date(year: i32, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("a real date")
}

fn build() -> Decl {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 600, 800, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let params = DateRangePickerParams::new(PICKER, date(2026, 11, 1), date(2026, 11, 11));
    let control = Md3DateRangePicker::from_pack(crate::pack::pack(), &mut cx, params);
    cx.with_cx(|cx| control.decl(cx))
}

fn find(decl: &Decl, id: Id) -> Option<&Decl> {
    if decl.id == id {
        return Some(decl);
    }
    decl.children.iter().find_map(|child| find(child, id))
}

#[test]
fn a_press_ends_an_open_range_and_restarts_a_closed_one() {
    // Compose's `updateDateSelection`: a completed range is followed by a new start, and a
    // press before the start moves it rather than making an inverted range.
    let first = date(2026, 11, 4);
    assert_eq!(next_range(None, None, first), (first, None));
    assert_eq!(
        next_range(Some(first), None, date(2026, 11, 9)),
        (first, Some(date(2026, 11, 9)))
    );
    assert_eq!(
        next_range(Some(first), None, date(2026, 11, 1)),
        (date(2026, 11, 1), None)
    );
    assert_eq!(
        next_range(Some(first), Some(date(2026, 11, 9)), date(2026, 11, 20)),
        (date(2026, 11, 20), None)
    );
}

#[test]
fn the_band_runs_from_the_start_cell_to_the_end_cell() {
    // November 2026 opens on a Sunday, so a Monday-first grid leaves six blanks before it.
    let grid = grid_of(date(2026, 11, 1), Weekday::Mon);
    assert_eq!(grid[0], None, "five blanks before a Sunday start");
    assert_eq!(grid[6], Some(date(2026, 11, 1)));
    let width = 48.0 * 7.0;
    // Inside one week: from the third day's middle to the fifth day's.
    assert_eq!(
        row_band(
            &grid,
            1,
            Some(date(2026, 11, 3)),
            Some(date(2026, 11, 5)),
            width,
            48.0
        ),
        Some((72.0, 168.0))
    );
    assert_eq!(
        row_band(
            &grid,
            0,
            Some(date(2026, 11, 3)),
            Some(date(2026, 11, 5)),
            width,
            48.0
        ),
        None,
        "a week the range never reaches draws nothing"
    );
    // Across a week boundary: the first row runs to its own edge, the next starts at 0.
    assert_eq!(
        row_band(
            &grid,
            1,
            Some(date(2026, 11, 5)),
            Some(date(2026, 11, 10)),
            width,
            48.0
        ),
        Some((168.0, 336.0))
    );
    assert_eq!(
        row_band(
            &grid,
            2,
            Some(date(2026, 11, 5)),
            Some(date(2026, 11, 10)),
            width,
            48.0
        ),
        Some((0.0, 72.0))
    );
    // A month the range swallows whole fills every row edge to edge.
    assert_eq!(
        row_band(
            &grid,
            3,
            Some(date(2026, 10, 1)),
            Some(date(2026, 12, 31)),
            width,
            48.0
        ),
        Some((0.0, 336.0))
    );
    assert_eq!(
        row_band(&grid, 3, Some(date(2026, 11, 1)), None, width, 48.0),
        None,
        "one date is not a range"
    );
}

#[test]
fn the_range_form_takes_the_modal_pickers_box() {
    // 524 is the modal picker's height and 68 is the range header's 128 less the 60 dp of
    // buttons Compose's header leaves out; a wrong subtraction would move the weekdays.
    let decl = build();
    assert_eq!(decl.id, PICKER, "the widget itself is the scrim");
    let surface = find(&decl, PICKER.with("surface")).expect("a surface of its own");
    assert_eq!(surface.size_class.width, AxisSize::fixed(360.0));
    assert_eq!(surface.size_class.height, AxisSize::fixed(524.0));
    let header = find(&decl, PICKER.with("header")).expect("a header");
    assert_eq!(header.size_class.height, AxisSize::fixed(68.0));
    let scroller = find(&decl, PICKER.with("months")).expect("the month list");
    assert_eq!(scroller.size_class.width, AxisSize::fixed(336.0));
    assert_eq!(scroller.size_class.height, AxisSize::fixed(408.0));
}
