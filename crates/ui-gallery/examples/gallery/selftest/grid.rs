// Self-test checks for the grid page.

use crate::selftest::*;

const EPS: f32 = 0.51;

/// Every sample's items must tile their grid: no overlap, gap-sized steps along a row,
/// and the rightmost edge on the content box wherever an `fr` track fills it.
pub(crate) fn grid(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    // The nav may have been scrolled away by an earlier check, so bring the row back first.
    goto(lp, size, Page::Grid);
    let mut pass = true;
    let mut good = 0usize;
    for (tag, count, filled) in GRID_SAMPLES {
        let mut bad = Vec::new();
        let Some(grid) = rect_of(lp, grid_id(tag)) else {
            println!("selftest: grid {tag} missing");
            pass = false;
            continue;
        };
        let rects: Vec<Rect> = (0..count)
            .filter_map(|i| rect_of(lp, grid_item_id(tag, i)))
            .collect();
        if rects.len() != count {
            println!("selftest: grid {tag} has {} of {count} items", rects.len());
            pass = false;
            continue;
        }
        for (i, r) in rects.iter().enumerate() {
            if r.origin.x < grid.origin.x - EPS
                || r.origin.y < grid.origin.y - EPS
                || r.origin.x + r.size.w > grid.origin.x + grid.size.w + EPS
                || r.origin.y + r.size.h > grid.origin.y + grid.size.h + EPS
            {
                bad.push(format!("item {i} {r:?} outside {grid:?}"));
            }
        }
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let (a, b) = (rects[i], rects[j]);
                let overlap_x = a.origin.x < b.origin.x + b.size.w - EPS
                    && b.origin.x < a.origin.x + a.size.w - EPS;
                let overlap_y = a.origin.y < b.origin.y + b.size.h - EPS
                    && b.origin.y < a.origin.y + a.size.h - EPS;
                if overlap_x && overlap_y {
                    bad.push(format!("items {i} and {j} overlap: {a:?} {b:?}"));
                }
            }
        }
        // Row-mates step by exactly one gap between their edges.
        let mut rows: Vec<Vec<Rect>> = Vec::new();
        for r in &rects {
            match rows
                .iter_mut()
                .find(|row| (row[0].origin.y - r.origin.y).abs() < EPS)
            {
                Some(row) => row.push(*r),
                None => rows.push(vec![*r]),
            }
        }
        for (n, row) in rows.iter_mut().enumerate() {
            row.sort_by(|a, b| a.origin.x.partial_cmp(&b.origin.x).unwrap());
            for pair in row.windows(2) {
                let step = pair[1].origin.x - (pair[0].origin.x + pair[0].size.w);
                if (step - GRID_GAP).abs() > EPS {
                    bad.push(format!("row {n} step is {step}, want {GRID_GAP}"));
                }
            }
        }
        if filled {
            let right = rects
                .iter()
                .map(|r| r.origin.x + r.size.w)
                .fold(f32::NEG_INFINITY, f32::max);
            if (right - (grid.origin.x + grid.size.w)).abs() > EPS {
                bad.push(format!(
                    "tracks end at {right}, content box ends at {}",
                    grid.origin.x + grid.size.w
                ));
            }
        }
        match tag {
            // Auto tracks follow their own content: longer label, wider column.
            "fluent" => {
                if !(rects[0].size.w < rects[1].size.w - EPS
                    && rects[1].size.w > rects[2].size.w + EPS)
                {
                    bad.push(format!(
                        "auto columns ignore content: {:.1} {:.1} {:.1}",
                        rects[0].size.w, rects[1].size.w, rects[2].size.w
                    ));
                }
            }
            // Span geometry: a two-column span equals the two track widths plus one gap,
            // and the two-row span equals the two row heights plus one gap.
            "glass" => {
                let two_cols = rects[2].size.w + GRID_GAP + rects[3].size.w;
                if (rects[0].size.w - two_cols).abs() > EPS {
                    bad.push(format!(
                        "col span is {:.1}, two tracks plus gap are {:.1}",
                        rects[0].size.w, two_cols
                    ));
                }
                let two_rows = rects[3].size.h + GRID_GAP + rects[5].size.h;
                if (rects[2].size.h - two_rows).abs() > EPS {
                    bad.push(format!(
                        "row span is {:.1}, two tracks plus gap are {:.1}",
                        rects[2].size.h, two_rows
                    ));
                }
                // A fixed-size item centres instead of stretching.
                let centre = rects[7].origin.x + rects[7].size.w * 0.5;
                let want = grid.origin.x + grid.size.w * 0.5;
                if (rects[7].size.w - 80.0).abs() > EPS || (centre - want).abs() > EPS {
                    bad.push(format!(
                        "centred item is {:?}, grid centre is {want}",
                        rects[7]
                    ));
                }
            }
            _ => {}
        }
        if bad.is_empty() {
            good += 1;
            println!(
                "selftest: grid {tag} {}x{} items={} ✓",
                grid.size.w as i32,
                grid.size.h as i32,
                rects.len()
            );
        } else {
            for line in &bad {
                println!("selftest: grid {tag}: {line}");
            }
            pass = false;
        }
    }
    println!(
        "selftest: grid samples {good}/{} tile their tracks {}",
        GRID_SAMPLES.len(),
        if good == GRID_SAMPLES.len() {
            "✓"
        } else {
            "✗"
        }
    );

    if !dense(lp) {
        pass = false;
    }
    pass
}

/// L1 geometry: the auto-filled track count, `minmax` bounds, and dense backfilling.
fn dense(lp: &AppLoop<Gallery>) -> bool {
    let mut bad = Vec::new();
    let Some(grid) = rect_of(lp, grid_id("dense")) else {
        println!("selftest: grid dense sample missing");
        return false;
    };
    let cols = dense_columns(grid.size.w);
    let track = DENSE_MIN;
    let rects: Vec<Rect> = (0..DENSE_ITEMS)
        .filter_map(|i| rect_of(lp, grid_item_id("dense", i)))
        .collect();
    if rects.len() != DENSE_ITEMS {
        bad.push(format!("{} of {DENSE_ITEMS} items", rects.len()));
    }
    for (i, r) in rects.iter().enumerate() {
        // A span-1 item fills its track; the two-column item fills two plus one gap.
        let want = if i == 1 {
            track * 2.0 + GRID_GAP
        } else {
            track
        };
        if (r.size.w - want).abs() > EPS {
            bad.push(format!("item {i} is {:.1} wide, want {want:.1}", r.size.w));
        }
        if r.size.w < DENSE_MIN - EPS || r.size.w > DENSE_MAX * 2.0 + GRID_GAP + EPS {
            bad.push(format!("item {i} is {:.1} wide, outside minmax", r.size.w));
        }
    }
    // The auto-filled count and the right edge of the last track it made.
    let right = rects
        .iter()
        .filter(|r| (r.origin.y - grid.origin.y).abs() < EPS)
        .map(|r| r.origin.x + r.size.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let want_right = grid.origin.x + cols * track + (cols - 1.0) * GRID_GAP;
    if (right - want_right).abs() > EPS {
        bad.push(format!(
            "row 0 ends at {right}, {cols} tracks end at {want_right}"
        ));
    }
    // Dense flow fills the hole the pinned and two-column items left at column 0.
    match rects.get(2) {
        Some(r)
            if (r.origin.x - grid.origin.x).abs() < EPS
                && (r.origin.y - grid.origin.y).abs() < EPS =>
        {
            println!(
                "selftest: grid dense cols={cols} row0 right={:.0} first auto at {:.0},{:.0} ✓",
                right, r.origin.x, r.origin.y
            );
        }
        Some(r) => bad.push(format!(
            "dense flow did not backfill (0,0): the first auto item sits at {:?}",
            r.origin
        )),
        None => bad.push("the first auto item is missing".to_string()),
    }
    for line in &bad {
        println!("selftest: grid dense: {line}");
    }
    bad.is_empty()
}
