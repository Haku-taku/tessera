// Self-test checks for the Fluent menu flyout page.

use crate::selftest::*;

pub(crate) fn menu_flyout(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let mut pass = true;
    goto(lp, size, Page::MenuFlyout);

    // The MD3 menu: 44 dp rows that grow for a supporting line, a 24 dp section label, and a
    // click that names the row while the disabled one stays silent.
    let surface = rect_of(lp, md3_menu_id()).expect("md3 menu");
    let plain = rect_of(lp, md3_menu_item_id(0)).expect("md3 plain row");
    let two_line = rect_of(lp, md3_menu_item_id(2)).expect("md3 supporting row");
    let section = rect_of(lp, md3_menu_id().with(("section", 0usize))).expect("md3 section label");
    let md3_rows = (plain.size.h - 44.0).abs() < 0.5
        && (two_line.size.h - 52.0).abs() < 0.5
        && (section.size.h - 24.0).abs() < 0.5
        && surface.size.w >= 112.0;
    click(
        lp,
        size,
        Point::new(
            two_line.origin.x + two_line.size.w * 0.5,
            two_line.origin.y + two_line.size.h * 0.5,
        ),
    );
    let md3_picked = lp.app_mut().md3_menu_picked;
    let disabled = rect_of(lp, md3_menu_item_id(4)).expect("md3 disabled row");
    click(
        lp,
        size,
        Point::new(
            disabled.origin.x + disabled.size.w * 0.5,
            disabled.origin.y + disabled.size.h * 0.5,
        ),
    );
    let md3_after = lp.app_mut().md3_menu_picked;
    println!(
        "selftest: md3 menu rows {:.0}/{:.0}/{:.0}, width {:.0} {}; picked {md3_picked:?} then {md3_after:?} {}",
        plain.size.h,
        two_line.size.h,
        section.size.h,
        surface.size.w,
        mark(md3_rows),
        mark(md3_picked == Some(2) && md3_after == Some(2))
    );
    pass &= md3_rows && md3_picked == Some(2) && md3_after == Some(2);
    // A press on another page may already have light-dismissed the panel, so the test opens
    // it from its anchor; that the anchor opens it is what this page has to promise.
    if !lp.app_mut().menu_open {
        let anchor = rect_of(lp, menu_anchor_id()).expect("menu anchor");
        click(lp, size, center(anchor));
    }
    let opened = lp.app_mut().menu_open;
    let elsewhere = rect_of(lp, Id::of(("bodyrow", Page::MenuFlyout.tag()))).expect("page row");
    click(
        lp,
        size,
        Point::new(elsewhere.origin.x + 4.0, elsewhere.origin.y + 4.0),
    );
    let dismissed = !lp.app_mut().menu_open;
    let anchor = rect_of(lp, menu_anchor_id()).expect("menu anchor");
    click(lp, size, center(anchor));
    let reopened = lp.app_mut().menu_open;
    println!(
        "selftest: fluent menu opens from its anchor {opened} {}, an outside press dismisses it {dismissed} {}, anchor reopens it {reopened} {}",
        mark(opened),
        mark(dismissed),
        mark(reopened)
    );
    pass &= opened && dismissed && reopened;

    // The panel unfolds below the anchor and start-aligned; rows keep the template's margins:
    // 4 dp outside, 11 dp inside, and the 28 dp icon column before every label.
    let panel = rect_of(lp, menu_flyout_id()).expect("menu panel");
    let row = rect_of(lp, menu_item_id(0)).expect("first row");
    let label = rect_of(lp, menu_item_id(0).with("label")).expect("first label");
    let below = (panel.origin.y - (anchor.origin.y + anchor.size.h)).abs() < 0.5;
    let aligned = (panel.origin.x - anchor.origin.x).abs() < 0.5;
    let margins = (row.origin.x - panel.origin.x - 4.0).abs() < 0.5
        && (panel.size.w - row.size.w - 8.0).abs() < 0.5;
    let indent = label.origin.x - row.origin.x;
    println!(
        "selftest: fluent menu below {below} {}, start-aligned {aligned} {}, row margins {margins} {}, label indent {indent:.1} (want 39) {}",
        mark(below),
        mark(aligned),
        mark(margins),
        mark((indent - 39.0).abs() < 0.5)
    );
    pass &= below && aligned && margins && (indent - 39.0).abs() < 0.5;

    // The open panel owns the keyboard: the arrows walk its rows, and Escape closes it.
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Down,
    });
    frame(lp, size, None);
    let walked = lp.focused() == Some(menu_item_id(1));
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Escape,
    });
    frame(lp, size, None);
    let escaped = !lp.app_mut().menu_open;
    println!(
        "selftest: fluent menu arrow walks rows {walked} {}, Escape closes {escaped} {}",
        mark(walked),
        mark(escaped)
    );
    pass &= walked && escaped;

    // A pick names the command and closes the menu; the app keeps both facts.
    if let Some(anchor) = rect_of(lp, menu_anchor_id()) {
        click(lp, size, center(anchor));
    }
    if let Some(row) = rect_of(lp, menu_item_id(1)) {
        click(lp, size, center(row));
    }
    let picked = lp.app_mut().menu_picked == Some(1) && !lp.app_mut().menu_open;
    println!(
        "selftest: fluent menu item picked {picked} {}",
        mark(picked)
    );
    pass &= picked;

    // ── The glass menu: a 200 dp frosted panel whose 44 dp rows sit 2 dp apart, start-aligned
    // under the anchor, with the title at the leading edge after a 20 dp icon.
    if !lp.app_mut().gmenu_open
        && let Some(anchor) = rect_of(lp, glass_menu_anchor_id())
    {
        click(lp, size, center(anchor));
    }
    let anchor = rect_of(lp, glass_menu_anchor_id()).expect("glass menu anchor");
    let panel = rect_of(lp, glass_menu_id()).expect("glass menu panel");
    let row0 = rect_of(lp, glass_menu_item_id(0)).expect("glass row 0");
    let row1 = rect_of(lp, glass_menu_item_id(1)).expect("glass row 1");
    let icon = rect_of(lp, glass_menu_item_id(0).with("icon")).expect("glass row icon");
    let label = rect_of(lp, glass_menu_item_id(0).with("label")).expect("glass row label");
    let width = (panel.size.w - 200.0).abs() < 0.5;
    let below = panel.origin.y >= anchor.origin.y + anchor.size.h - 0.5;
    let aligned = (panel.origin.x - anchor.origin.x).abs() < 0.5;
    let inset =
        (row0.origin.x - panel.origin.x - 12.0).abs() < 0.5 && (row0.size.h - 44.0).abs() < 0.5;
    let gap = (row1.origin.y - row0.origin.y - 46.0).abs() < 0.5;
    let leading = (icon.origin.x - row0.origin.x - 16.0).abs() < 0.5
        && (label.origin.x - row0.origin.x - 48.0).abs() < 0.5;
    println!(
        "selftest: glass menu width {width} {}, below {below} {}, start-aligned {aligned} {}, row inset {inset} {}, row gap {gap} {}, leading content {leading} {}",
        mark(width),
        mark(below),
        mark(aligned),
        mark(inset),
        mark(gap),
        mark(leading)
    );
    pass &= width && below && aligned && inset && gap && leading;

    // A pick names the row and closes the panel; Escape closes a reopened one.
    click(lp, size, center(row1));
    let glass_picked = lp.app_mut().gmenu_picked == Some(1) && !lp.app_mut().gmenu_open;
    if let Some(anchor) = rect_of(lp, glass_menu_anchor_id()) {
        click(lp, size, center(anchor));
    }
    let reopened = lp.app_mut().gmenu_open;
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Escape,
    });
    frame(lp, size, None);
    let escaped = !lp.app_mut().gmenu_open;
    println!(
        "selftest: glass menu item picked {glass_picked} {}, anchor reopens {reopened} {}, Escape closes {escaped} {}",
        mark(glass_picked),
        mark(reopened),
        mark(escaped)
    );
    pass &= glass_picked && reopened && escaped;

    // ── The glass popover: the same sheet, holding the app's own blocks — 280 dp wide,
    // start-aligned under its anchor, its blocks inside the sheet at the 16 dp inset.
    if !lp.app_mut().gpop_open
        && let Some(anchor) = rect_of(lp, glass_popover_anchor_id())
    {
        click(lp, size, center(anchor));
    }
    let opened = lp.app_mut().gpop_open;
    let anchor = rect_of(lp, glass_popover_anchor_id()).expect("popover anchor");
    let panel = rect_of(lp, glass_popover_id()).expect("popover sheet");
    let title = rect_of(lp, Id::of(("gpop", "title"))).expect("popover title");
    let body = rect_of(lp, Id::of(("gpop", "body"))).expect("popover body");
    let done = rect_of(lp, glass_popover_done_id()).expect("popover done");
    let width = (panel.size.w - 280.0).abs() < 0.5;
    let below = panel.origin.y >= anchor.origin.y + anchor.size.h - 0.5;
    let inset = (title.origin.x - panel.origin.x - 16.0).abs() < 0.5;
    let inside = [title, body, done].iter().all(|r| {
        r.origin.x >= panel.origin.x - 0.5
            && r.origin.x + r.size.w <= panel.origin.x + panel.size.w + 0.5
            && r.origin.y + r.size.h <= panel.origin.y + panel.size.h + 0.5
    });
    let order = title.origin.y < body.origin.y && body.origin.y < done.origin.y;
    println!(
        "selftest: glass popover opened {opened} {}, width {width} {}, below {below} {}, \
         inset {inset} {}, blocks inside {inside} {}, in order {order} {}",
        mark(opened),
        mark(width),
        mark(below),
        mark(inset),
        mark(inside),
        mark(order)
    );
    pass &= opened && width && below && inset && inside && order;

    // Its own "Done" reports like any control, and the app closes the sheet; a reopened
    // sheet is closed by Escape.
    let before = lp.app_mut().gpop_done;
    click(lp, size, center(done));
    let acted = lp.app_mut().gpop_done == before + 1 && !lp.app_mut().gpop_open;
    if let Some(anchor) = rect_of(lp, glass_popover_anchor_id()) {
        click(lp, size, center(anchor));
    }
    let reopened = lp.app_mut().gpop_open;
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Escape,
    });
    frame(lp, size, None);
    let escaped = !lp.app_mut().gpop_open;
    println!(
        "selftest: glass popover done {acted} {}, anchor reopens {reopened} {}, Escape closes {escaped} {}",
        mark(acted),
        mark(reopened),
        mark(escaped)
    );
    pass &= acted && reopened && escaped;
    pass
}
