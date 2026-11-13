// Self-test checks for the navigation bar page.

use crate::selftest::*;

pub(crate) fn nav_bar(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::NavBar);

    // Every bar is the export's 80dp tall.
    let mut heights_ok = true;
    for tag in ["always", "selected", "never"] {
        match rect_of(lp, nav_bar_id(tag)) {
            Some(r) => heights_ok &= (r.size.h - 80.0).abs() < 0.5,
            None => {
                println!("selftest: nav bar {tag} is missing");
                heights_ok = false;
            }
        }
    }
    println!(
        "selftest: nav bars 80dp tall {}",
        if heights_ok { "✓" } else { "✗" }
    );
    if !heights_ok {
        pass = false;
    }

    // One selection shared by all three bars: the second destination in any of them.
    click(
        lp,
        size,
        center(rect_of(lp, nav_item_id("always", 1)).expect("bar item 1")),
    );
    let picked = lp.app_mut().nav_sel;
    println!(
        "selftest: nav bar picked {picked} {}",
        if picked == 1 { "✓" } else { "✗" }
    );
    if picked != 1 {
        pass = false;
    }
    click(
        lp,
        size,
        center(rect_of(lp, nav_item_id("never", 0)).expect("bar item 0")),
    );
    let picked = lp.app_mut().nav_sel;
    println!(
        "selftest: nav bar shares selection {picked} {}",
        if picked == 0 { "✓" } else { "✗" }
    );
    if picked != 0 {
        pass = false;
    }

    // The selected destination resolves its `FILL` axis to 1, the others to 0: the icon
    // source the bar holds is re-resolved per state, not swapped by the app.
    let selected_fill = fill_of(lp, nav_item_id("always", 0));
    let unselected_fill = fill_of(lp, nav_item_id("always", 1));
    let fill_ok = selected_fill == 1.0 && unselected_fill == 0.0;
    println!(
        "selftest: nav bar FILL selected {selected_fill:.1} unselected {unselected_fill:.1} {}",
        if fill_ok { "✓" } else { "✗" }
    );
    pass &= fill_ok;

    // A selected-only bar cross-fades: the outgoing icon slides to the bare position as its
    // label fades, the incoming one rises, and no destination changes height.
    let first = nav_item_id("selected", 0);
    let third = nav_item_id("selected", 2);
    let start_icons = (
        rect_of(lp, first.with("icon")).expect("icon 0").origin.y,
        rect_of(lp, third.with("icon")).expect("icon 2").origin.y,
    );
    let start_h = rect_of(lp, first).expect("bar item 0").size.h;
    let start_alpha = text_ink(lp, first.with("label")).expect("label ink").a;
    let at = center(rect_of(lp, third).expect("bar item 2"));
    tap(lp, size, at, 7);
    let mid_icons = (
        rect_of(lp, first.with("icon")).expect("icon 0").origin.y,
        rect_of(lp, third.with("icon")).expect("icon 2").origin.y,
    );
    let mid_alpha = (
        text_ink(lp, first.with("label")).expect("label ink").a,
        text_ink(lp, third.with("label")).expect("label ink").a,
    );
    let mid_h = rect_of(lp, first).expect("bar item 0").size.h;
    let mid_ok = start_alpha > 0.99
        && mid_icons.0 > start_icons.0 + 0.5
        && mid_icons.1 < start_icons.1 - 0.5
        && mid_alpha.0 < 0.99
        && mid_alpha.1 > 0.01
        && mid_alpha.1 < 0.99
        && (mid_h - start_h).abs() < 0.5;
    frame(lp, size, None);
    let end_alpha = (
        text_ink(lp, first.with("label")).expect("label ink").a,
        text_ink(lp, third.with("label")).expect("label ink").a,
    );
    let cross_fade_ok = mid_ok && end_alpha.0 < 0.01 && end_alpha.1 > 0.99;
    println!(
        "selftest: nav bar label cross-fade mid={mid_ok} settled={} alpha {:?}->{:?} {}",
        end_alpha.0 < 0.01 && end_alpha.1 > 0.99,
        mid_alpha,
        end_alpha,
        if cross_fade_ok { "✓" } else { "✗" }
    );
    if !cross_fade_ok {
        pass = false;
    }
    pass
}

/// The `FILL` axis the bar resolved a destination's icon to, from its text run.
fn fill_of(lp: &AppLoop<Gallery>, id: Id) -> f32 {
    let Some(nid) = lp.tree().find_by_id(id.with("icon")) else {
        return -1.0;
    };
    let Some(node) = lp.tree().get(nid) else {
        return -1.0;
    };
    let ui_core::NodeData::Text { block, .. } = &node.data else {
        return -1.0;
    };
    block
        .runs
        .first()
        .map_or(-1.0, |r| r.style.variations.fill.unwrap_or(0.0))
}
