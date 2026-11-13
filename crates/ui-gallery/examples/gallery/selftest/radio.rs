// Self-test checks for the radio page.

use crate::selftest::*;

pub(crate) fn radio(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The Radio page: a click on an option moves the group's selection to it.
    goto(lp, size, Page::Radio);
    let mut rings = 0;
    for k in 0..RADIOS {
        if rect_of(lp, radio_id(k)).is_some() {
            rings += 1;
        }
    }
    let was = lp.app_mut().radio_sel;
    if let Some(third) = rect_of(lp, radio_id(2)) {
        click(lp, size, center(third));
    }
    let now = lp.app_mut().radio_sel;
    let moved = now == 2 && was != 2;
    if rings == RADIOS && moved {
        println!(
            "selftest: md3 radio options {rings}/{RADIOS}, click moved the selection {was}->{now} ✓"
        );
    } else {
        println!("selftest: md3 radio options {rings}/{RADIOS}, selection {was}->{now}");
        pass = false;
    }
    pass
}
