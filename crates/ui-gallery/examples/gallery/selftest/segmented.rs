// Self-test checks for the segmented buttons page.

use crate::selftest::*;

pub(crate) fn segmented(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::Segmented);

    // Equal shares of the export's 40dp height: the segments split the group.
    let mut widths = [0.0f32; SEGMENTS];
    let mut geometry_ok = true;
    for k in 0..SEGMENTS {
        match rect_of(lp, seg_single_id(k)) {
            Some(r) => {
                widths[k] = r.size.w;
                geometry_ok &= (r.size.h - 40.0).abs() < 0.5;
            }
            None => {
                println!("selftest: segment {k} is missing");
                geometry_ok = false;
            }
        }
    }
    let widest = widths.iter().copied().fold(f32::MIN, f32::max);
    let narrowest = widths.iter().copied().fold(f32::MAX, f32::min);
    let spread = widest - narrowest;
    println!(
        "selftest: segmented 40dp tall, shares {spread:.1} apart {}",
        if geometry_ok && spread < 0.5 {
            "✓"
        } else {
            "✗"
        }
    );
    if !(geometry_ok && spread < 0.5) {
        pass = false;
    }

    // A click on the third segment replaces the single choice.
    click(
        lp,
        size,
        center(rect_of(lp, seg_single_id(2)).expect("segment 2")),
    );
    let picked = lp.app_mut().seg_single;
    println!(
        "selftest: segmented single-select picked {picked} {}",
        if picked == 2 { "✓" } else { "✗" }
    );
    if picked != 2 {
        pass = false;
    }

    // A click on a multi segment toggles it.
    let before = lp.app_mut().seg_multi[1];
    click(
        lp,
        size,
        center(rect_of(lp, seg_multi_id(1)).expect("multi segment 1")),
    );
    let after = lp.app_mut().seg_multi[1];
    println!(
        "selftest: segmented multi-select {before} -> {after} {}",
        if after != before { "✓" } else { "✗" }
    );
    if after == before {
        pass = false;
    }

    // The disabled group reports nothing.
    let before = lp.app_mut().seg_single;
    click(
        lp,
        size,
        center(rect_of(lp, seg_disabled_id(1)).expect("disabled segment")),
    );
    let after = lp.app_mut().seg_single;
    println!(
        "selftest: segmented disabled stays {after} {}",
        if after == before { "✓" } else { "✗" }
    );
    if after != before {
        pass = false;
    }

    // The glass control reports its own selection and its lens travels to that cell.
    let lens = glass_group_id().with("lens");
    let target = center(rect_of(lp, seg_glass_id(2)).expect("glass segment 2"));
    click(lp, size, target);
    let picked = lp.app_mut().seg_glass;
    step(lp, size, target, 40);
    let lens_x = rect_of(lp, lens).map(|r| r.origin.x + r.size.w * 0.5);
    let cell_x = rect_of(lp, seg_glass_id(2)).map(|r| r.origin.x + r.size.w * 0.5);
    let landed = lens_x.zip(cell_x).is_some_and(|(l, c)| (l - c).abs() < 1.0);
    println!(
        "selftest: segmented glass picked {picked}, lens at {lens_x:?} vs cell {cell_x:?} {}",
        if picked == 2 && landed { "✓" } else { "✗" }
    );
    if picked != 2 || !landed {
        pass = false;
    }
    pass
}
