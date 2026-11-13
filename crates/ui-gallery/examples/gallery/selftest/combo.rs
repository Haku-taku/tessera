// Self-test checks for the Fluent combo box page.

use ui_widgets_fluent::{COMBO_ITEM_MARGIN, COMBO_ITEM_PAD, PILL_H, PILL_W};

use crate::selftest::*;

pub(crate) fn combo(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let mut pass = true;
    goto(lp, size, Page::Combo);
    // The page arrives with the panel open; the field closes it if it somehow did not.
    if !lp.app_mut().combo_open
        && let Some(field) = rect_of(lp, combo_id().with("box"))
    {
        click(lp, size, center(field));
    }
    let field = rect_of(lp, combo_id().with("box")).expect("combo field");
    let panel = rect_of(lp, combo_id().with("dropdown")).expect("combo panel");
    let below = (panel.origin.y - (field.origin.y + field.size.h)).abs() <= 0.5;
    let same_width = (panel.size.w - field.size.w).abs() <= 0.5;
    let aligned = (panel.origin.x - field.origin.x).abs() <= 0.5;
    println!(
        "selftest: combo panel below {below} {}, follows the field's width {same_width} {}, start-aligned {aligned} {}",
        mark(below),
        mark(same_width),
        mark(aligned)
    );
    pass &= below && same_width && aligned;

    // Rows keep the item's 5,2,5,2 margin and 11 dp lead-in; the selected pill is 3×16,
    // flush with the row's start edge and centred in the row.
    let row = rect_of(lp, combo_item_id(0)).expect("first row");
    let label = rect_of(lp, combo_item_id(0).with("label")).expect("first label");
    let margin = (row.origin.x - panel.origin.x - COMBO_ITEM_MARGIN.left).abs() <= 0.5;
    let lead = (label.origin.x - row.origin.x - COMBO_ITEM_PAD.left).abs() <= 0.5;
    let pill = rect_of(lp, combo_item_id(1).with("pill-bar")).expect("selected pill");
    let pill_row = rect_of(lp, combo_item_id(1)).expect("selected row");
    let size_ok = (pill.size.w - PILL_W).abs() <= 0.5 && (pill.size.h - PILL_H).abs() <= 0.5;
    let flush = (pill.origin.x - pill_row.origin.x).abs() <= 0.5;
    let centred = (center(pill).y - center(pill_row).y).abs() <= 0.5;
    println!(
        "selftest: combo row margin {margin} {}, label lead {lead} {}, pill {size_ok} {} flush {flush} {}, centred {centred} {}",
        mark(margin),
        mark(lead),
        mark(size_ok),
        mark(flush),
        mark(centred)
    );
    pass &= margin && lead && size_ok && flush && centred;

    // The open panel owns the keyboard: it opens onto the selected row, the arrows walk
    // from there, and Escape closes it.
    let landed = lp.focused() == Some(combo_item_id(1));
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Down,
    });
    frame(lp, size, None);
    let walked = lp.focused() == Some(combo_item_id(2));
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Escape,
    });
    frame(lp, size, None);
    let escaped = !lp.app_mut().combo_open;
    println!(
        "selftest: combo opens onto the selection {landed} {}, arrows walk {walked} {}, Escape closes {escaped} {}",
        mark(landed),
        mark(walked),
        mark(escaped)
    );
    pass &= landed && walked && escaped;

    // A row names what it took; the app stores it and the panel closes.
    if let Some(field) = rect_of(lp, combo_id().with("box")) {
        click(lp, size, center(field));
    }
    if let Some(row) = rect_of(lp, combo_item_id(3)) {
        click(lp, size, center(row));
    }
    let picked = lp.app_mut().combo_sel == Some(3) && !lp.app_mut().combo_open;
    println!("selftest: combo row picked {picked} {}", mark(picked));
    pass &= picked;

    // A press outside the panel is a light dismiss, as a flyout's is.
    if let Some(field) = rect_of(lp, combo_id().with("box")) {
        click(lp, size, center(field));
    }
    let reopened = lp.app_mut().combo_open;
    // The field takes the light-dismiss tap whole: pressing it again closes the list
    // instead of closing and reopening it in one gesture.
    if let Some(field) = rect_of(lp, combo_id().with("box")) {
        click(lp, size, center(field));
    }
    let toggled_shut = !lp.app_mut().combo_open;
    println!(
        "selftest: combo field toggles shut {toggled_shut} {}",
        mark(toggled_shut)
    );
    pass &= toggled_shut;
    if let Some(field) = rect_of(lp, combo_id().with("box")) {
        click(lp, size, center(field));
    }
    if let Some(panel) = rect_of(lp, combo_id().with("dropdown")) {
        click(
            lp,
            size,
            Point::new(panel.origin.x - 20.0, panel.origin.y - 20.0),
        );
    }
    let dismissed = !lp.app_mut().combo_open;
    println!(
        "selftest: combo reopens {reopened} {}, outside press dismisses {dismissed} {}",
        mark(reopened),
        mark(dismissed)
    );
    pass &= reopened && dismissed;
    pass
}
