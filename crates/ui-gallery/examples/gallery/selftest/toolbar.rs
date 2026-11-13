// Self-test checks for the toolbar page.

use crate::selftest::*;

pub(crate) fn toolbar(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::Toolbar);

    // A docked bar has no FAB: the page configures one on every sample, so a docked bar
    // that draws it is the control ignoring its own variant rule.
    let docked_fab = rect_of(lp, toolbar_fab_id("docked-standard")).is_none();
    println!(
        "selftest: toolbar docked omits its fab {docked_fab} {}",
        if docked_fab { "✓" } else { "✗" }
    );
    if !docked_fab {
        pass = false;
    }

    // All four bars share one selection.
    click(
        lp,
        size,
        center(rect_of(lp, toolbar_action_id("docked-standard", 2)).expect("toolbar action 2")),
    );
    let picked = lp.app_mut().toolbar_sel;
    println!(
        "selftest: toolbar picked {picked:?} {}",
        if picked == Some(2) { "✓" } else { "✗" }
    );
    if picked != Some(2) {
        pass = false;
    }

    // The floating FAB: 56dp square, its trailing edge inset from the bar's own edge, and
    // a press reports the FAB rather than the member slot under the same point.
    let fab = rect_of(lp, toolbar_fab_id("floating-standard"));
    let bar = rect_of(lp, toolbar_id("floating-standard"));
    let (square, trailing) = match (fab, bar) {
        (Some(f), Some(b)) => (
            (f.size.w - 56.0).abs() < 0.5 && (f.size.h - 56.0).abs() < 0.5,
            ((b.origin.x + b.size.w - 8.0) - (f.origin.x + f.size.w)).abs() <= 0.5,
        ),
        _ => (false, false),
    };
    if let Some(f) = fab {
        click(lp, size, center(f));
    }
    let hits = lp.app_mut().toolbar_fab_hits;
    println!(
        "selftest: toolbar fab square={square} trailing={trailing} hits={hits} {}",
        if square && trailing && hits == 1 {
            "✓"
        } else {
            "✗"
        }
    );
    if !(square && trailing && hits == 1) {
        pass = false;
    }
    pass
}
