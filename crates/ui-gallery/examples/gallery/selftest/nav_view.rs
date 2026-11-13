// Self-test checks for the Fluent navigation view page.

use ui_widgets_fluent::{
    COMPACT_PANE_W, ICON_BOX_W, NAV_ITEM_MARGIN, OPEN_PANE_W, PaneDisplayMode,
};

use crate::selftest::*;

pub(crate) fn nav_view(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let mut pass = true;
    goto(lp, size, Page::NavView);
    // The page arrives open; a previous run's toggle could have left it folded.
    if !lp.app_mut().nav_open
        && let Some(toggle) = rect_of(lp, nav_view_id().with("pane-toggle"))
    {
        click(lp, size, center(toggle));
    }
    let pane = rect_of(lp, nav_view_id().with("pane")).expect("nav pane");
    let content = rect_of(lp, nav_view_id().with("content")).expect("nav content");
    let open_width = (pane.size.w - OPEN_PANE_W).abs() <= 0.5;
    let beside = (content.origin.x - (pane.origin.x + pane.size.w)).abs() <= 0.5;
    println!(
        "selftest: nav-view pane {OPEN_PANE_W} wide {open_width} {}, content beside it {beside} {}",
        mark(open_width),
        mark(beside)
    );
    pass &= open_width && beside;

    // The first destination: 4,2 margin inside the pane, 36 high, icon box 40 with the
    // 16 dp glyph centred in it, label starting 4 after the box.
    let item = rect_of(lp, nav_view_item_id(0)).expect("first destination");
    let glyph = rect_of(lp, nav_view_item_id(0).with("glyph")).expect("first glyph");
    let icon = rect_of(lp, nav_view_item_id(0).with("icon")).expect("first icon box");
    let label = rect_of(lp, nav_view_item_id(0).with("label")).expect("first label");
    let inset = (item.origin.x - pane.origin.x - NAV_ITEM_MARGIN.left).abs() <= 0.5
        && (item.origin.y - pane.origin.y - 44.0 - NAV_ITEM_MARGIN.top).abs() <= 0.5;
    let height = (item.size.h - 36.0).abs() <= 0.5;
    let boxed =
        (icon.size.w - ICON_BOX_W).abs() <= 0.5 && (icon.origin.x - item.origin.x).abs() <= 0.5;
    let glyph_centred = (center(glyph).x - center(icon).x).abs() <= 0.5
        && (center(glyph).y - center(icon).y).abs() <= 0.5;
    let lead = (label.origin.x - (icon.origin.x + icon.size.w) - 4.0).abs() <= 0.5;
    println!(
        "selftest: nav-view item inset {inset} {} (dx {}, dy {}), height 36 {height} {}, icon box {boxed} {}, glyph centred {glyph_centred} {}, label lead {lead} {}",
        mark(inset),
        item.origin.x - pane.origin.x,
        item.origin.y - pane.origin.y,
        mark(height),
        mark(boxed),
        mark(glyph_centred),
        mark(lead)
    );
    pass &= inset && height && boxed && glyph_centred && lead;

    // The selected destination carries the 3×16 indicator at its start edge, centred.
    let bar = rect_of(lp, nav_view_item_id(0).with("bar")).expect("selection indicator");
    let bar_size = (bar.size.w - 3.0).abs() <= 0.5 && (bar.size.h - 16.0).abs() <= 0.5;
    let bar_flush = (bar.origin.x - item.origin.x).abs() <= 0.5;
    let bar_centred = (center(bar).y - center(item).y).abs() <= 0.5;
    let only_selected = rect_of(lp, nav_view_item_id(1).with("bar")).is_none();
    println!(
        "selftest: nav-view indicator {bar_size} {} flush {bar_flush} {}, centred {bar_centred} {}, only on the selection {only_selected} {}",
        mark(bar_size),
        mark(bar_flush),
        mark(bar_centred),
        mark(only_selected)
    );
    pass &= bar_size && bar_flush && bar_centred && only_selected;

    // The toggle folds the pane to 48: labels and the content's left edge go with it.
    if let Some(toggle) = rect_of(lp, nav_view_id().with("pane-toggle")) {
        click(lp, size, center(toggle));
    }
    let folded = !lp.app_mut().nav_open;
    let compact =
        rect_of(lp, nav_view_id().with("pane")).map(|r| (r.size.w - COMPACT_PANE_W).abs() <= 0.5);
    let labels_gone = rect_of(lp, nav_view_item_id(0).with("label")).is_none();
    println!(
        "selftest: nav-view folds {folded} {}, pane {COMPACT_PANE_W} wide {compact:?}, labels gone {labels_gone} {}",
        mark(folded),
        mark(labels_gone)
    );
    pass &= folded && compact == Some(true) && labels_gone;

    // The toggle opens it again, and the labels return.
    if let Some(toggle) = rect_of(lp, nav_view_id().with("pane-toggle")) {
        click(lp, size, center(toggle));
    }
    let reopened = lp.app_mut().nav_open;
    let label_back = rect_of(lp, nav_view_item_id(0).with("label")).is_some();
    println!(
        "selftest: nav-view reopens {reopened} {}, labels back {label_back} {}",
        mark(reopened),
        mark(label_back)
    );
    pass &= reopened && label_back;

    // A destination names what it took; the app stores it and the indicator moves there.
    if let Some(target) = rect_of(lp, nav_view_item_id(2)) {
        click(lp, size, center(target));
    }
    let moved = lp.app_mut().nav_view_sel == 2;
    let bar_moved = rect_of(lp, nav_view_item_id(2).with("bar")).is_some()
        && rect_of(lp, nav_view_item_id(0).with("bar")).is_none();
    println!(
        "selftest: nav-view destination picked {moved} {}, indicator follows {bar_moved} {}",
        mark(moved),
        mark(bar_moved)
    );
    pass &= moved && bar_moved;

    // `Compact` keeps the 48 strip in the flow; opening it covers the content instead of
    // pushing it, which is the whole difference from `Expanded`.
    if let Some(b) = rect_of(lp, nav_mode_id(1)) {
        click(lp, size, center(b));
    }
    let view = rect_of(lp, nav_view_id()).expect("nav view");
    let strip = rect_of(lp, nav_view_id().with("pane")).expect("compact strip");
    let closed_x = rect_of(lp, nav_view_id().with("content")).map(|r| r.origin.x);
    let compact_closed = lp.app_mut().nav_mode == PaneDisplayMode::Compact
        && !lp.app_mut().nav_open
        && (strip.size.w - COMPACT_PANE_W).abs() <= 0.5
        && (closed_x.unwrap_or(0.0) - (view.origin.x + COMPACT_PANE_W)).abs() <= 0.5
        && rect_of(lp, nav_view_item_id(0).with("label")).is_none();
    println!(
        "selftest: nav-view compact keeps the {COMPACT_PANE_W} strip closed {compact_closed} {}",
        mark(compact_closed)
    );
    pass &= compact_closed;

    if let Some(t) = rect_of(lp, nav_view_id().with("pane-toggle")) {
        click(lp, size, center(t));
    }
    let pane = rect_of(lp, nav_view_id().with("overlay"));
    let opened_x = rect_of(lp, nav_view_id().with("content")).map(|r| r.origin.x);
    let overlays = lp.app_mut().nav_open
        && pane.is_some_and(|p| {
            (p.origin.x - view.origin.x).abs() <= 0.5
                && (p.origin.y - view.origin.y).abs() <= 0.5
                && (p.size.w - OPEN_PANE_W).abs() <= 0.5
                && (p.size.h - view.size.h).abs() <= 0.5
        })
        && opened_x == closed_x
        && rect_of(lp, nav_view_item_id(0).with("label")).is_some();
    println!(
        "selftest: nav-view compact opens {OPEN_PANE_W} over the content, it stays put {overlays} {} (pane {pane:?})",
        mark(overlays)
    );
    pass &= overlays;

    // A press beside the pane is the light dismiss every overlay has.
    if let Some(c) = rect_of(lp, nav_view_id().with("content")) {
        click(
            lp,
            size,
            Point::new(c.origin.x + c.size.w - 20.0, c.origin.y + 40.0),
        );
    }
    let dismissed = !lp.app_mut().nav_open && rect_of(lp, nav_view_id().with("overlay")).is_none();
    println!(
        "selftest: nav-view overlay dismisses on a press outside {dismissed} {}",
        mark(dismissed)
    );
    pass &= dismissed;

    // `Minimal` has no strip at all: the toggle sits above the content and the content
    // starts at the view's own edge.
    if let Some(b) = rect_of(lp, nav_mode_id(2)) {
        click(lp, size, center(b));
    }
    let view = rect_of(lp, nav_view_id()).expect("nav view");
    let toggle = rect_of(lp, nav_view_id().with("pane-toggle"));
    let content = rect_of(lp, nav_view_id().with("content"));
    let minimal = lp.app_mut().nav_mode == PaneDisplayMode::Minimal
        && !lp.app_mut().nav_open
        && rect_of(lp, nav_view_id().with("pane")).is_none()
        && content.is_some_and(|c| (c.origin.x - view.origin.x).abs() <= 0.5)
        && toggle
            .zip(content)
            .is_some_and(|(t, c)| t.origin.y + t.size.h <= c.origin.y + 0.5);
    println!(
        "selftest: nav-view minimal drops the strip, toggle above the content {minimal} {}",
        mark(minimal)
    );
    pass &= minimal;

    if let Some(t) = toggle {
        click(lp, size, center(t));
    }
    let pane = rect_of(lp, nav_view_id().with("overlay"));
    let minimal_open = lp.app_mut().nav_open
        && pane.is_some_and(|p| {
            (p.origin.x - view.origin.x).abs() <= 0.5 && (p.size.w - OPEN_PANE_W).abs() <= 0.5
        });
    println!(
        "selftest: nav-view minimal opens the same overlay {minimal_open} {}",
        mark(minimal_open)
    );
    pass &= minimal_open;

    // Escape is bound on the open pane, which holds the keyboard while it is up.
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Escape,
    });
    frame(lp, size, None);
    let escaped = !lp.app_mut().nav_open;
    println!(
        "selftest: nav-view overlay closes on Escape {escaped} {}",
        mark(escaped)
    );
    pass &= escaped;

    // Back to `Expanded`, so the page arrives in the state the earlier checks assume.
    if let Some(b) = rect_of(lp, nav_mode_id(0)) {
        click(lp, size, center(b));
    }
    let back = lp.app_mut().nav_mode == PaneDisplayMode::Expanded
        && lp.app_mut().nav_open
        && rect_of(lp, nav_view_id().with("pane"))
            .is_some_and(|r| (r.size.w - OPEN_PANE_W).abs() <= 0.5);
    println!("selftest: nav-view back to expanded {back} {}", mark(back));
    pass &= back;
    pass
}
