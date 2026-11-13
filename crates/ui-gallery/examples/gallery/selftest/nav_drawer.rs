// Self-test checks for the navigation drawer page.

use crate::selftest::*;

pub(crate) fn nav_drawer(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::NavDrawer);

    // The drawer is the export's 360dp wide and fills the height its parent gives it.
    let width_ok = rect_of(lp, drawer_id())
        .map(|r| (r.size.w - 360.0).abs() < 0.5 && (r.size.h - 280.0).abs() < 0.5)
        .unwrap_or(false);
    println!(
        "selftest: drawer 360 wide and 280 tall {}",
        if width_ok { "✓" } else { "✗" }
    );
    if !width_ok {
        pass = false;
    }

    click(
        lp,
        size,
        center(rect_of(lp, drawer_item_id(1)).expect("drawer item 1")),
    );
    let picked = lp.app_mut().drawer_sel;
    println!(
        "selftest: drawer picked {picked} {}",
        if picked == 1 { "✓" } else { "✗" }
    );
    if picked != 1 {
        pass = false;
    }

    // The modal sample: a trigger opens a 360dp surface over a window-sized scrim, and a
    // press on the scrim asks the app to close it.
    click(
        lp,
        size,
        center(rect_of(lp, drawer_open_id()).expect("drawer open button")),
    );
    // The surface slides in; a settled frame puts it at rest.
    frame(lp, size, None);
    let opened = lp.app_mut().drawer_open;
    let barrier = rect_of(lp, modal_drawer_id());
    let window = (size.0 as f32, size.1 as f32);
    let barrier_ok = barrier
        .is_some_and(|r| (r.size.w - window.0).abs() < 1.0 && (r.size.h - window.1).abs() < 1.0);
    let surface = rect_of(lp, modal_drawer_id().with("surface"));
    let surface_ok = surface.is_some_and(|r| {
        (r.size.w - 360.0).abs() < 0.5
            && r.origin.x.abs() < 0.5
            && (r.size.h - window.1).abs() < 1.0
    });
    let outside = Point::new(window.0 - 40.0, window.1 * 0.5);
    tap(lp, size, outside, 6);
    let closed = !lp.app_mut().drawer_open && rect_of(lp, modal_drawer_id()).is_none();
    println!(
        "selftest: drawer modal opened={opened} barrier={barrier_ok} surface={surface_ok} \
         scrim-closes={closed}"
    );
    if !(opened && barrier_ok && surface_ok && closed) {
        pass = false;
    }
    pass
}
