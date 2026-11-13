// Self-test checks for the FAB menu page.

use crate::selftest::*;

pub(crate) fn fab_menu(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // FAB menu: the anchor is the FAB's own slot, and picking an item closes the menu.
    goto(lp, size, Page::FabMenu);
    let want_h = 3.0 * 56.0 + 2.0 * 4.0 + 8.0 + 56.0;
    let got_h = rect_of(lp, fab_menu_id(0))
        .map(|r| r.size.h)
        .unwrap_or(-1.0);
    let was_open = lp.app_mut().fab_open[0];
    click(
        lp,
        size,
        center(rect_of(lp, fab_item_id(0, 1)).expect("fab item")),
    );
    let picked = lp.app_mut().fab_picked;
    let closed = !lp.app_mut().fab_open[0];
    println!(
        "selftest: md3 fab menu height {got_h} (want {want_h}) {}, item picked {picked:?}, open {was_open}->{} {}",
        if (got_h - want_h).abs() < 0.5 {
            "✓"
        } else {
            "✗"
        },
        !closed,
        if picked == Some((0, 1)) && was_open && closed {
            "✓"
        } else {
            "✗"
        }
    );
    if !((got_h - want_h).abs() < 0.5 && picked == Some((0, 1)) && was_open && closed) {
        pass = false;
    }
    // The screen pins the anchor to its bottom-right corner with a 16dp margin, which is where
    // MD3 floats a FAB; without the cross-axis position it drifts to the leading edge.
    let placed = match (
        rect_of(lp, fab_screen_id(0)),
        rect_of(lp, fab_menu_id(0).with("close")),
    ) {
        (Some(screen), Some(anchor)) => {
            let right = (screen.origin.x + screen.size.w) - (anchor.origin.x + anchor.size.w);
            let bottom = (screen.origin.y + screen.size.h) - (anchor.origin.y + anchor.size.h);
            println!("selftest: md3 fab anchor inset right {right} bottom {bottom} (want 16)");
            (right - 16.0).abs() < 0.5 && (bottom - 16.0).abs() < 0.5
        }
        _ => {
            println!("selftest: md3 fab screen or anchor missing");
            false
        }
    };
    if !placed {
        pass = false;
    }

    // ── The plain FAB: four sizes from the token table, the disabled one inert, the rest
    // ── reporting one activation each.
    let sizes_ok = [40.0, 56.0, 80.0, 96.0]
        .into_iter()
        .enumerate()
        .all(|(k, want)| {
            rect_of(lp, fab_sample_id(k))
                .is_some_and(|r| (r.size.w - want).abs() < 0.5 && (r.size.h - want).abs() < 0.5)
        });
    println!(
        "selftest: md3 fab sizes 40/56/80/96 {} {}",
        sizes_ok,
        if sizes_ok { "✓" } else { "✗" }
    );
    if !sizes_ok {
        pass = false;
    }
    let before = lp.app_mut().fab_presses;
    if let Some(r) = rect_of(lp, fab_sample_id(4)) {
        click(lp, size, center(r));
    }
    for _ in 0..2 {
        frame(lp, size, None);
    }
    let counted = lp.app_mut().fab_presses == before + 1;
    if let Some(r) = rect_of(lp, fab_sample_id(7)) {
        click(lp, size, center(r));
    }
    for _ in 0..2 {
        frame(lp, size, None);
    }
    let inert = lp.app_mut().fab_presses == before + 1;
    println!(
        "selftest: md3 fab activation counted={counted} disabled-inert={inert} {}",
        if counted && inert { "✓" } else { "✗" }
    );
    pass &= counted && inert;
    pass
}

pub(crate) fn fab_page(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The FAB menu page: the anchor toggles a collapsed menu, and an item click names it and
    // closes the menu. The control reports from its root, so the anchor is matched by root.
    // The nav may have been scrolled away by an earlier check, so bring the row back first.
    goto(lp, size, Page::FabMenu);
    const SHUT: usize = 3;
    let was_open = lp.app_mut().fab_open[SHUT];
    if let Some(anchor) = rect_of(lp, fab_menu_id(SHUT).with("close")) {
        click(lp, size, center(anchor));
    }
    let opened = !was_open && lp.app_mut().fab_open[SHUT];
    // Items travel while the menu opens, so let it settle before aiming at one.
    for _ in 0..12 {
        frame(lp, size, None);
    }
    let mut picked = false;
    if opened && let Some(item) = rect_of(lp, fab_item_id(SHUT, 1)) {
        click(lp, size, center(item));
        picked = lp.app_mut().fab_picked == Some((SHUT, 1)) && !lp.app_mut().fab_open[SHUT];
    }
    let fab_ok = opened && picked;
    println!(
        "selftest: md3 fab menu opens from closed={opened} picks after opening={picked} {}",
        if fab_ok { "✓" } else { "✗" }
    );
    if !fab_ok {
        pass = false;
    }
    pass
}
