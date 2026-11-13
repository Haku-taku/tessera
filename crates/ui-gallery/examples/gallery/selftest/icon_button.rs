// Self-test checks for the icon button page.

use crate::selftest::*;

pub(crate) fn icon_button(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::IconButton);

    // The five sizes: the default width makes each box square, and the height is the token's
    // container height while the width is its mark plus both spaces.
    let sides = [32.0, 40.0, 56.0, 96.0, 136.0];
    let mut sizes_ok = true;
    for (k, side) in sides.iter().enumerate() {
        sizes_ok &= matches!(
            rect_of(lp, icon_button_size_id(k)),
            Some(r) if (r.size.w - side).abs() < 0.5 && (r.size.h - side).abs() < 0.5
        );
    }
    println!(
        "selftest: md3 icon button sizes={sizes_ok} {}",
        if sizes_ok { "✓" } else { "✗" }
    );
    pass &= sizes_ok;

    // The width axis at medium: narrow 48, default 56, wide 72, all 56 tall.
    let widths = [48.0, 56.0, 72.0];
    let mut widths_ok = true;
    for (k, width) in widths.iter().enumerate() {
        widths_ok &= matches!(
            rect_of(lp, icon_button_width_id(k)),
            Some(r) if (r.size.w - width).abs() < 0.5 && (r.size.h - 56.0).abs() < 0.5
        );
    }
    println!(
        "selftest: md3 icon button widths={widths_ok} {}",
        if widths_ok { "✓" } else { "✗" }
    );
    pass &= widths_ok;

    // A toggle reports its activation, and the app owns the state: two presses return it.
    let toggle = icon_button_toggle_id(1);
    let before = lp.app_mut().icon_button_on[1];
    match rect_of(lp, toggle).map(center) {
        Some(at) => click(lp, size, at),
        None => pass = false,
    }
    frame(lp, size, None);
    let checked = lp.app_mut().icon_button_on[1] == !before;
    match rect_of(lp, toggle).map(center) {
        Some(at) => click(lp, size, at),
        None => pass = false,
    }
    frame(lp, size, None);
    let cleared = lp.app_mut().icon_button_on[1] == before;
    println!(
        "selftest: md3 icon button toggle checked={checked} cleared={cleared} {}",
        if checked && cleared { "✓" } else { "✗" }
    );
    pass &= checked && cleared;

    // A disabled button is still laid out at the token's box and takes no press.
    let disabled_ok = matches!(
        rect_of(lp, icon_button_disabled_id()),
        Some(r) if (r.size.w - 56.0).abs() < 0.5 && (r.size.h - 56.0).abs() < 0.5
    );
    println!(
        "selftest: md3 icon button disabled={disabled_ok} {}",
        if disabled_ok { "✓" } else { "✗" }
    );
    pass &= disabled_ok;
    pass
}
