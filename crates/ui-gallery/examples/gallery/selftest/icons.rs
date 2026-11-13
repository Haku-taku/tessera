// Self-test checks for the icons page.

use crate::selftest::*;

pub(crate) fn icons(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The animated icon: a text node never takes input, so the tap must be the container
    // around it. Clicking has to reach the app and flip the target the `FILL` axis follows.
    goto(lp, size, Page::Icons);
    let before = lp.app_mut().icon_filled;
    if let Some(heart) = rect_of(lp, icon_toggle_id()) {
        click(lp, size, center(heart));
    }
    let after = lp.app_mut().icon_filled;
    let icon_ok = after != before;
    println!(
        "selftest: icon FILL target {before}->{after} {}",
        if icon_ok { "✓" } else { "✗" }
    );
    if !icon_ok {
        pass = false;
    }
    pass
}
