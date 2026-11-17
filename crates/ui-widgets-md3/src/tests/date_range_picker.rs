//! The MD3 date range picker: the selection rule, the month grid and the band.

use crate::date_picker::{Date, Weekday};
use crate::date_range_picker::{grid_of, next_range, row_band};

fn date(year: i32, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("a real date")
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

