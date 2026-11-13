// Self-test checks for the navigation rail page.

use crate::selftest::*;

pub(crate) fn nav_rail(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::NavRail);

    // The rail is the export's 80dp wide, whatever its label type.
    let mut widths_ok = true;
    for tag in ["all", "selected", "none"] {
        match rect_of(lp, rail_id(tag)) {
            Some(r) => widths_ok &= (r.size.w - 80.0).abs() < 0.5,
            None => {
                println!("selftest: rail {tag} is missing");
                widths_ok = false;
            }
        }
    }
    println!(
        "selftest: nav rails 80dp wide {}",
        if widths_ok { "✓" } else { "✗" }
    );
    if !widths_ok {
        pass = false;
    }

    click(
        lp,
        size,
        center(rect_of(lp, nav_item_id("all", 2)).expect("rail item 2")),
    );
    let picked = lp.app_mut().nav_sel;
    println!(
        "selftest: nav rail picked {picked} {}",
        if picked == 2 { "✓" } else { "✗" }
    );
    if picked != 2 {
        pass = false;
    }

    // The selected destination resolves its `FILL` axis to 1, the others to 0.
    let selected_fill = fill_of(lp, nav_item_id("all", 2));
    let unselected_fill = fill_of(lp, nav_item_id("all", 0));
    let fill_ok = selected_fill == 1.0 && unselected_fill == 0.0;
    println!(
        "selftest: nav rail FILL selected {selected_fill:.1} unselected {unselected_fill:.1} {}",
        if fill_ok { "✓" } else { "✗" }
    );
    pass &= fill_ok;

    // Baseline M3 gives the rail content an 8dp margin, so the first labelled capsule never
    // sits on the rail's edge.
    let mut margin_ok = true;
    for tag in ["all", "selected"] {
        let rail = rect_of(lp, rail_id(tag)).expect("rail");
        let first = rect_of(lp, nav_item_id(tag, 0)).expect("first destination");
        margin_ok &= (first.origin.y - rail.origin.y - 8.0).abs() < 0.5;
    }
    println!(
        "selftest: nav rail content margin {}",
        if margin_ok { "✓" } else { "✗" }
    );
    if !margin_ok {
        pass = false;
    }

    // `Selected` cross-fades the label: it grows its own room while the destinations below
    // move down, instead of appearing and shoving them in one frame.
    let labelled_h = rect_of(lp, nav_item_id("all", 0))
        .expect("all rail item")
        .size
        .h;
    let first = nav_item_id("selected", 0);
    let third = nav_item_id("selected", 2);
    // Measured inside the rail: the page centres it, so its own origin moves as it grows.
    let rail = rail_id("selected");
    let inside = |lp: &AppLoop<Gallery>, id: Id| {
        rect_of(lp, id).expect("rail destination").origin.y
            - rect_of(lp, rail).expect("rail").origin.y
    };
    let start_h = rect_of(lp, first).expect("rail item 0").size.h;
    let start_y = inside(lp, third);
    let at = center(rect_of(lp, first).expect("rail item 0"));
    tap(lp, size, at, 7);
    let mid_h = rect_of(lp, first).expect("rail item 0").size.h;
    let mid_y = inside(lp, third);
    let mid_alpha = (
        text_ink(lp, first.with("label")).expect("label ink").a,
        text_ink(lp, third.with("label")).expect("label ink").a,
    );
    let mid_ok = start_h < labelled_h - 0.5
        && mid_h > start_h + 0.5
        && mid_h < labelled_h - 0.5
        && mid_y > start_y + 0.5
        && mid_alpha.0 > 0.01
        && mid_alpha.0 < 0.99
        && mid_alpha.1 > 0.01
        && mid_alpha.1 < 0.99;
    frame(lp, size, None);
    let end_h = rect_of(lp, first).expect("rail item 0").size.h;
    let end_alpha = (
        text_ink(lp, first.with("label")).expect("label ink").a,
        text_ink(lp, third.with("label")).expect("label ink").a,
    );
    let label_ok =
        mid_ok && (end_h - labelled_h).abs() < 0.5 && end_alpha.0 > 0.99 && end_alpha.1 < 0.01;
    println!(
        "selftest: nav rail label grow mid={mid_ok} settled={} height {start_h}/{mid_h}/{end_h} \
         labelled={labelled_h} y {start_y}/{mid_y} alpha {:?}->{:?} {}",
        (end_h - labelled_h).abs() < 0.5 && end_alpha.0 > 0.99 && end_alpha.1 < 0.01,
        mid_alpha,
        end_alpha,
        if label_ok { "✓" } else { "✗" }
    );
    if !label_ok {
        pass = false;
    }
    pass
}

/// The `FILL` axis a destination's icon resolved to, from its text run.
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
