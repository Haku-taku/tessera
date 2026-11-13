// Self-test checks for the time picker page.

use crate::selftest::*;

pub(crate) fn time_picker(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Time input: the period tile that was hit is what the app is told, and nothing else.
    goto(lp, size, Page::TimePicker);
    let tp = time_picker_id(0);
    let period_before = lp.app_mut().tp_period[0];
    click(
        lp,
        size,
        center(rect_of(lp, tp.with("pm")).expect("pm tile")),
    );
    let period_after = lp.app_mut().tp_period[0];
    let switched =
        period_after == ui_widgets_md3::Period::Pm && period_before == ui_widgets_md3::Period::Am;
    println!(
        "selftest: md3 time picker period {period_before:?}->{period_after:?} {}",
        if switched { "✓" } else { "✗" }
    );
    if !switched {
        pass = false;
    }
    pass
}
