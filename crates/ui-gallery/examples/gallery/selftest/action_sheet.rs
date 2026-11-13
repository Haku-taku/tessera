// Self-test checks for the action sheet page.

use crate::selftest::*;

pub(crate) fn action_sheet(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    let mark = |ok: bool| if ok { "✓" } else { "✗" };

    goto(lp, size, Page::ActionSheet);
    let trigger = rect_of(lp, sheet_open_id()).expect("sheet trigger");
    click(lp, size, center(trigger));

    // ── The sheet is anchored to the bottom edge: cards inset 8 dp on both sides and at the
    // bottom, separated by 8 dp. No offsets by hand — the spacer above it does the work.
    let actions = rect_of(lp, sheet_id().with("actions"));
    let cancel = rect_of(lp, sheet_id().with("cancel"));
    let placed = match (actions, cancel) {
        (Some(a), Some(c)) => {
            let left = (a.origin.x - 8.0).abs() < 0.5;
            let right = (a.origin.x + a.size.w - (size.0 as f32 - 8.0)).abs() < 0.5;
            let bottom = (c.origin.y + c.size.h - (size.1 as f32 - 8.0)).abs() < 0.5;
            let gap = (c.origin.y - (a.origin.y + a.size.h) - 8.0).abs() < 0.5;
            (left, right, bottom, gap)
        }
        _ => (false, false, false, false),
    };
    println!(
        "selftest: action sheet inset left={} right={} bottom={} gap={} {}",
        placed.0,
        placed.1,
        placed.2,
        placed.3,
        mark(placed.0 && placed.1 && placed.2 && placed.3)
    );
    pass &= placed.0 && placed.1 && placed.2 && placed.3;

    // Every action row is the reference's 16 dp padding over one 17 pt line, and all equal.
    let rows: Vec<f32> = (0..SHEET_ACTIONS.len())
        .map(|k| rect_of(lp, sheet_id().with(("action", k))).map_or(-1.0, |r| r.size.h))
        .collect();
    let first = rows.first().copied().unwrap_or(-1.0);
    let rows_ok = first > 48.0 && rows.iter().all(|h| (h - first).abs() < 0.5);
    println!("selftest: action sheet rows {rows:?} {}", mark(rows_ok));
    pass &= rows_ok;

    // Picking an action reports it and the app closes the sheet.
    let share = center(rect_of(lp, sheet_id().with(("action", 1usize))).expect("share row"));
    click(lp, size, share);
    let picked = lp.app_mut().sheet_picked;
    let closed = rect_of(lp, sheet_id()).is_none();
    let chose = picked == Some(SHEET_ACTIONS[1]) && closed;
    println!(
        "selftest: action sheet picked {picked:?} closed={closed} {}",
        mark(chose)
    );
    pass &= chose;

    // The cancel card reports a dismissal, not a choice.
    click(lp, size, center(trigger));
    let before = lp.app_mut().sheet_dismissed;
    let cancel_row = center(rect_of(lp, sheet_id().with("cancel")).expect("cancel row"));
    click(lp, size, cancel_row);
    let after_cancel = lp.app_mut().sheet_dismissed;
    let still_closed = rect_of(lp, sheet_id()).is_none();
    let cancelled = after_cancel == before + 1 && still_closed;
    println!(
        "selftest: action sheet cancel {before}->{after_cancel} closed={still_closed} {}",
        mark(cancelled)
    );
    pass &= cancelled;

    // A press on the scrim outside the sheet dismisses it too.
    click(lp, size, center(trigger));
    click(lp, size, Point::new(size.0 as f32 * 0.5, 40.0));
    let after_scrim = lp.app_mut().sheet_dismissed;
    let scrim_closed = rect_of(lp, sheet_id()).is_none();
    let dismissed = after_scrim == after_cancel + 1 && scrim_closed;
    println!(
        "selftest: action sheet scrim {after_cancel}->{after_scrim} closed={scrim_closed} {}",
        mark(dismissed)
    );
    pass &= dismissed;
    pass
}
