// Self-test checks for the lists page.

use crate::selftest::*;

pub(crate) fn lists(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;

    // Lists: the three line counts give 56/72/88, and a row's selection is the app's.
    goto(lp, size, Page::Lists);
    let heights: Vec<f32> = (1..=3)
        .map(|k| {
            rect_of(lp, list_id("lines", k))
                .map(|r| r.size.h)
                .unwrap_or(-1.0)
        })
        .collect();
    let heights_ok = heights
        .iter()
        .zip([56.0, 72.0, 88.0])
        .all(|(got, want)| (got - want).abs() < 0.5);
    let before = lp.app_mut().list_sel[0];
    click(
        lp,
        size,
        center(rect_of(lp, list_id("selection", 1)).expect("list row")),
    );
    let after = lp.app_mut().list_sel[0];
    println!(
        "selftest: md3 list heights {heights:?} (want [56, 72, 88]) {}, selection {before}->{after} {}",
        if heights_ok { "✓" } else { "✗" },
        if after != before { "✓" } else { "✗" }
    );
    if !(heights_ok && after != before) {
        pass = false;
    }

    // Badges sit in the trailing slot at their token sizes, and the divider is one hairline.
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let pill_row = rect_of(lp, ornament_id("pill").with("row")).expect("pill row");
    let pill = rect_of(lp, ornament_id("pill")).expect("pill badge");
    let dot = rect_of(lp, ornament_id("dot")).expect("dot badge");
    let centred = (center(pill).y - center(pill_row).y).abs() < 0.5;
    let pill_ok = (pill.size.h - 16.0).abs() < 0.5 && pill.size.w >= 15.5 && centred;
    let dot_ok = (dot.size.w - 6.0).abs() < 0.5 && (dot.size.h - 6.0).abs() < 0.5;
    println!(
        "selftest: badge pill {:.0}x{:.0} (centred {}), dot {:.0}x{:.0} {}",
        pill.size.w,
        pill.size.h,
        mark(centred),
        dot.size.w,
        dot.size.h,
        mark(dot_ok)
    );
    pass &= pill_ok && dot_ok;

    // The plain divider spans the row; the inset one starts 16 in and loses that 16 of width.
    let row = rect_of(lp, ornament_id("dot").with("row")).expect("dot row");
    let full = rect_of(lp, ornament_id("divider-full")).expect("full divider");
    let line = rect_of(lp, ornament_id("divider-inset").with("line")).expect("inset line");
    let full_ok = (full.size.h - 1.0).abs() < 0.5 && (full.size.w - row.size.w).abs() < 0.5;
    let inset_ok = (line.size.h - 1.0).abs() < 0.5
        && (line.origin.x - row.origin.x - 16.0).abs() < 0.5
        && (line.size.w - (row.size.w - 16.0)).abs() < 0.5;
    println!(
        "selftest: divider full {:.0}x{:.1} {}, inset dx {:.1} w {:.1} {}",
        full.size.w,
        full.size.h,
        mark(full_ok),
        line.origin.x - row.origin.x,
        line.size.w,
        mark(inset_ok)
    );
    pass &= full_ok && inset_ok;

    // ── Glass rows: the 16/12 inset, the 56 dp text edge (16 + 28 slot + 12), the 20 dp
    // trailing glyph 16 from the right edge, and the separator that starts at the text edge.
    let tile = rect_of(lp, glass_tile_id("wifi")).expect("glass row");
    let title = rect_of(lp, glass_tile_id("wifi").with("title")).expect("glass title");
    let lead = rect_of(lp, glass_tile_id("wifi").with("leading")).expect("glass leading");
    let trail = rect_of(lp, glass_tile_id("wifi").with("trailing")).expect("glass trailing");
    let inset = (title.origin.x - tile.origin.x - 56.0).abs() < 0.5
        && (title.origin.y - tile.origin.y - 12.0).abs() < 0.5
        && (lead.origin.x - tile.origin.x - 16.0).abs() < 0.5
        && (lead.size.w - 28.0).abs() < 0.5;
    let right = (tile.origin.x + tile.size.w - trail.origin.x - trail.size.w - 16.0).abs() < 0.5
        && (trail.size.w - 20.0).abs() < 0.5;
    let band = rect_of(lp, glass_tile_id("divider").with(1usize)).expect("glass divider");
    let line =
        rect_of(lp, glass_tile_id("divider").with(1usize).with("line")).expect("divider line");
    let sep = (band.size.h - 1.0).abs() < 0.5
        && (line.size.h - 0.5).abs() < 0.5
        && (line.origin.x - tile.origin.x - 56.0).abs() < 0.5;
    println!(
        "selftest: glass row inset {} (title dx {:.1}, dy {:.1}), trailing right {} (dx {:.1}), separator {} (h {:.1}, x {:.1})",
        mark(inset),
        title.origin.x - tile.origin.x,
        title.origin.y - tile.origin.y,
        mark(right),
        tile.origin.x + tile.size.w - trail.origin.x - trail.size.w,
        mark(sep),
        line.size.h,
        line.origin.x - tile.origin.x
    );
    pass &= inset && right && sep;

    // A row with an action reports; a still row is not a target at all.
    let hits = lp.app_mut().glass_list_hits;
    click(lp, size, center(tile));
    let acted = lp.app_mut().glass_list_hits;
    let still = rect_of(lp, glass_tile_id("still")).expect("still row");
    click(lp, size, center(still));
    let quiet = lp.app_mut().glass_list_hits;
    let acts = acted == hits + 1 && quiet == acted;
    println!(
        "selftest: glass row activation {hits}->{acted}->{quiet} (still must not count) {}",
        mark(acts)
    );
    pass &= acts;

    // Holding the row raises the 8 % wash at once; letting go fades it out.
    let button = ui_core::PointerButton::Primary;
    let at = center(tile);
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    frame(lp, size, Some(at));
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    frame(lp, size, Some(at));
    // The row stacks its face first and the wash second, so the wash is drawn shape 1.
    let held = shape_fill(lp, glass_tile_id("wifi"), 1).map_or(-1.0, |c| c.a);
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    for _ in 0..3 {
        frame(lp, size, Some(at));
    }
    let gone = shape_fill(lp, glass_tile_id("wifi"), 1).map_or(-1.0, |c| c.a);
    // The reference's own number, not the constant: asserting a constant against itself
    // would pass even if the wash were never painted.
    let wash = (held - 0.08).abs() < 0.01 && gone < 0.001;
    println!(
        "selftest: glass row wash held {held:.3} released {gone:.3} {}",
        mark(wash)
    );
    pass &= wash;
    pass
}
