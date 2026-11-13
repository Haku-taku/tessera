// Self-test checks for the snackbar page.

use crate::selftest::*;

pub(crate) fn snackbar(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::Snackbar);

    // The export's two heights: 48dp for one line, 68dp for two.
    let one = rect_of(lp, snack_id(0)).map(|r| r.size.h);
    let two = rect_of(lp, snack_id(1)).map(|r| r.size.h);
    let heights_ok = matches!((one, two), (Some(a), Some(b)) if (a - 48.0).abs() < 0.5 && (b - 68.0).abs() < 0.5);
    println!(
        "selftest: snackbar heights {one:?} and {two:?} {}",
        if heights_ok { "✓" } else { "✗" }
    );
    if !heights_ok {
        pass = false;
    }

    // A hidden bar paints nothing at all.
    let hidden = shape_rect(lp, snack_id(3), 0).is_none();
    println!(
        "selftest: hidden snackbar paints nothing {}",
        if hidden { "✓" } else { "✗" }
    );
    if !hidden {
        pass = false;
    }

    // Only the action reports, and it reports from the bar's root.
    let before = lp.app_mut().snack_hits;
    click(
        lp,
        size,
        center(rect_of(lp, snack_id(0).with("action")).expect("snackbar action")),
    );
    let after = lp.app_mut().snack_hits;
    println!(
        "selftest: snackbar action {before} -> {after} {}",
        if after > before { "✓" } else { "✗" }
    );
    if after <= before {
        pass = false;
    }

    // The floating sample starts hidden; showing it pins the bar to the window bottom and
    // keeps it out of the flow, so the in-flow samples do not move.
    let overlay = Id::str("snackbar/overlay");
    let in_flow = rect_of(lp, snack_id(0)).map(|r| r.origin.y);
    // Hidden = the app has not shown it and the bar paints nothing (a faded bar stays in
    // the tree, so its node exists but draws no shape).
    let hidden = !lp.app_mut().snack_overlay_visible && shape_rect(lp, overlay, 0).is_none();
    let show = rect_of(lp, snack_overlay_show_id()).expect("the show button");
    click(lp, size, center(show));
    let visible = lp.app_mut().snack_overlay_visible;
    let bar = rect_of(lp, overlay).expect("the floating bar appears");
    let bottom_ok = (bar.origin.y + bar.size.h - (size.1 as f32 - 24.0)).abs() < 1.0;
    let not_flow = rect_of(lp, snack_id(0)).map(|r| r.origin.y == in_flow.unwrap_or(f32::MAX));
    let overlay_ok = hidden && visible && bottom_ok && not_flow.unwrap_or(false);
    println!(
        "selftest: snackbar overlay hidden={hidden} shown={visible} bottom={bottom_ok} \
         out-of-flow={not_flow:?} {}",
        if overlay_ok { "✓" } else { "✗" }
    );
    if !overlay_ok {
        pass = false;
    }
    pass
}
