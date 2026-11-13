// Self-test checks for the positioning page: geometry, not screenshots.

use crate::selftest::*;

const EPS: f32 = 0.51;

pub(crate) fn positioning(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    // The nav may have been scrolled away by an earlier check, so bring the row back first.
    goto(lp, size, Page::Positioning);
    let mut pass = true;
    let mut good = 0usize;

    // Relative: the slot stays where the flow put it, only the pixels move.
    let mut bad = Vec::new();
    match (
        rect_of(lp, positioning_id("relative", "slot")),
        rect_of(lp, positioning_id("relative", "mover")),
        layout_rect(lp, positioning_id("relative", "mover")),
    ) {
        (Some(slot), Some(mover), Some(mover_layout)) => {
            let want_x = slot.origin.x + slot.size.w + 12.0;
            if (mover_layout.origin.x - want_x).abs() > EPS
                || (mover_layout.origin.y - slot.origin.y).abs() > EPS
            {
                bad.push(format!(
                    "the mover left its slot: layout {mover_layout:?} beside slot {slot:?}"
                ));
            }
            if (mover.origin.x - (mover_layout.origin.x + REL_SHIFT.0)).abs() > EPS
                || (mover.origin.y - (mover_layout.origin.y + REL_SHIFT.1)).abs() > EPS
            {
                bad.push(format!(
                    "visual {mover:?} is not layout {mover_layout:?} plus {REL_SHIFT:?}"
                ));
            }
        }
        _ => bad.push("the relative sample is missing a box".to_string()),
    }
    if bad.is_empty() {
        good += 1;
        println!(
            "selftest: positioning relative mover {:.0},{:.0} → {:.0},{:.0} ✓",
            layout_rect(lp, positioning_id("relative", "mover"))
                .unwrap()
                .origin
                .x,
            layout_rect(lp, positioning_id("relative", "mover"))
                .unwrap()
                .origin
                .y,
            rect_of(lp, positioning_id("relative", "mover"))
                .unwrap()
                .origin
                .x,
            rect_of(lp, positioning_id("relative", "mover"))
                .unwrap()
                .origin
                .y,
        );
    } else {
        for line in &bad {
            println!("selftest: positioning relative: {line}");
        }
        pass = false;
    }

    // Absolute: the badge hangs off the stage's padding box, the box centres by auto margins.
    let mut bad = Vec::new();
    match (
        rect_of(lp, positioning_id("absolute", "stage")),
        rect_of(lp, positioning_id("absolute", "pin")),
        rect_of(lp, positioning_id("absolute", "centre")),
    ) {
        (Some(stage), Some(pin), Some(centred)) => {
            if (pin.origin.x + pin.size.w - (stage.origin.x + stage.size.w)).abs() > EPS
                || (pin.origin.y + pin.size.h - (stage.origin.y + stage.size.h)).abs() > EPS
            {
                bad.push(format!(
                    "pin {pin:?} is not on the padding corner of {stage:?}"
                ));
            }
            let (cx, cy) = (center(centred).x, center(centred).y);
            let (sx, sy) = (center(stage).x, center(stage).y);
            if (cx - sx).abs() > EPS || (cy - sy).abs() > EPS {
                bad.push(format!(
                    "auto-margin box centres at ({cx:.1},{cy:.1}), stage at ({sx:.1},{sy:.1})"
                ));
            }
            println!(
                "selftest: positioning absolute pin {:.0}x{:.0} at {:.0},{:.0}, centre {:.0},{:.0} ✓",
                pin.size.w, pin.size.h, pin.origin.x, pin.origin.y, cx, cy
            );
        }
        _ => bad.push("the absolute sample is missing a box".to_string()),
    }
    if bad.is_empty() {
        good += 1;
    } else {
        for line in &bad {
            println!("selftest: positioning absolute: {line}");
        }
        pass = false;
    }

    // Fixed: the badge pins to the root box, wherever it is declared.
    let mut bad = Vec::new();
    let root_rect = lp
        .tree()
        .root()
        .and_then(|r| lp.tree().get(r))
        .map(|n| n.rect);
    match (root_rect, rect_of(lp, positioning_id("fixed", "badge"))) {
        (Some(root), Some(badge)) => {
            if (badge.origin.x + badge.size.w - (root.origin.x + root.size.w)).abs() > EPS
                || (badge.origin.y + badge.size.h - (root.origin.y + root.size.h)).abs() > EPS
            {
                bad.push(format!(
                    "fixed badge {badge:?} is not on the root corner {root:?}"
                ));
            }
            println!(
                "selftest: positioning fixed badge at {:.0},{:.0} of root {:.0}x{:.0} ✓",
                badge.origin.x, badge.origin.y, root.size.w, root.size.h
            );
        }
        _ => bad.push("the fixed sample is missing its badge".to_string()),
    }

    // `z` decides the paint order, not the declaration order: high is declared first.
    let list = lp.draw_list();
    let painted = |id: Id| -> Option<usize> {
        let nid = lp.tree().find_by_id(id)?;
        list.owners.rects.iter().position(|owner| *owner == nid)
    };
    let walked = |id: Id| -> Option<usize> {
        let nid = lp.tree().find_by_id(id)?;
        lp.tree().walk().iter().position(|n| *n == nid)
    };
    match (
        painted(positioning_id("fixed", "low")),
        painted(positioning_id("fixed", "high")),
        walked(positioning_id("fixed", "low")),
        walked(positioning_id("fixed", "high")),
    ) {
        (Some(low), Some(high), Some(wlow), Some(whigh)) if low < high && wlow < whigh => {
            println!(
                "selftest: positioning z paints low@{low} < high@{high}, walks {wlow} < {whigh} ✓"
            );
        }
        (Some(low), Some(high), Some(wlow), Some(whigh)) => {
            bad.push(format!(
                "z order ignored: paint low@{low}, high@{high}; walk {wlow}, {whigh}"
            ));
        }
        _ => bad.push("a z sample box paints nothing or is missing from the walk".to_string()),
    }
    if bad.is_empty() {
        good += 1;
    } else {
        for line in &bad {
            println!("selftest: positioning fixed: {line}");
        }
        pass = false;
    }

    println!(
        "selftest: positioning samples {good}/3 match their geometry {}",
        if good == 3 { "✓" } else { "✗" }
    );
    pass
}
