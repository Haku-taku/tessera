// Self-test checks for the toast page.

use ui_widgets_glass::{TOAST_DISMISS_TRAVEL, TOAST_INSET, TOAST_MAX_WIDTH, TOAST_MIN_HEIGHT};

use crate::selftest::*;

pub(crate) fn toast(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    let mark = |ok: bool| if ok { "✓" } else { "✗" };

    goto(lp, size, Page::Toast);
    let show = rect_of(lp, toast_show_id()).expect("toast trigger");
    click(lp, size, center(show));

    // ── The layer hugs the window's bottom edge at 16 dp, and the pill floats centred in it.
    let layer = rect_of(lp, toast_id());
    let pill = rect_of(lp, toast_id().with("pill"));
    let placed = match (layer, pill) {
        (Some(l), Some(p)) => {
            let edge = (l.origin.x - TOAST_INSET).abs() < 0.5
                && (l.origin.x + l.size.w - (size.0 as f32 - TOAST_INSET)).abs() < 0.5
                && (l.origin.y + l.size.h - (size.1 as f32 - TOAST_INSET)).abs() < 0.5;
            let centred = (p.origin.x + p.size.w * 0.5 - size.0 as f32 * 0.5).abs() < 0.5;
            (edge, centred)
        }
        _ => (false, false),
    };
    println!(
        "selftest: toast edge={} centred={} {}",
        placed.0,
        placed.1,
        mark(placed.0 && placed.1)
    );
    pass &= placed.0 && placed.1;

    // The pill is the reference's 48 dp capsule, never wider than its 400 dp ceiling.
    let shaped = pill.is_some_and(|p| {
        (p.size.h - TOAST_MIN_HEIGHT).abs() < 0.5 && p.size.w <= TOAST_MAX_WIDTH + 0.5
    });
    println!(
        "selftest: toast pill {:?} {}",
        pill.map(|p| (p.size.w, p.size.h)),
        mark(shaped)
    );
    pass &= shaped;

    // The action reports itself and the app closes the toast.
    let before = lp.app_mut().toast_picked;
    let action = center(rect_of(lp, toast_action_id()).expect("toast action"));
    click(lp, size, action);
    let picked = lp.app_mut().toast_picked;
    let closed = rect_of(lp, toast_id()).is_none();
    let acted = picked == before + 1 && closed;
    println!(
        "selftest: toast action {before}->{picked} closed={closed} {}",
        mark(acted)
    );
    pass &= acted;

    // A swipe down past the threshold dismisses it; the app counts the dismissal.
    click(lp, size, center(show));
    let before = lp.app_mut().toast_dismissed;
    let body = center(rect_of(lp, toast_id().with("pill")).expect("toast pill"));
    drag(
        lp,
        size,
        body,
        Point::new(body.x, body.y + TOAST_DISMISS_TRAVEL + 40.0),
    );
    let swiped = lp.app_mut().toast_dismissed;
    let gone = rect_of(lp, toast_id()).is_none();
    let dismissed = swiped == before + 1 && gone;
    println!(
        "selftest: toast swipe {before}->{swiped} closed={gone} {}",
        mark(dismissed)
    );
    pass &= dismissed;

    // The top position mirrors it: the layer starts 16 dp below the window's top edge.
    click(
        lp,
        size,
        center(rect_of(lp, toast_error_id()).expect("error trigger")),
    );
    let top = rect_of(lp, toast_id());
    let anchored = top.is_some_and(|l| (l.origin.y - TOAST_INSET).abs() < 0.5);
    println!("selftest: toast top anchored={anchored} {}", mark(anchored));
    pass &= anchored;
    pass
}
