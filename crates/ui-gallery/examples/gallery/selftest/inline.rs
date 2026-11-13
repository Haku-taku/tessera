// Self-test checks for the inline page: atoms sit inside their line, never under glyphs.

use crate::selftest::*;

const EPS: f32 = 0.51;

pub(crate) fn inline(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    // The nav may have been scrolled away by an earlier check, so bring the row back first.
    goto(lp, size, Page::Inline);
    let mut pass = true;
    let mut good = 0usize;
    for (tag, width) in INLINE_SAMPLES {
        let mut bad = Vec::new();
        let id = inline_text_id(tag);
        let Some(nid) = lp.tree().find_by_id(id) else {
            println!("selftest: inline {tag} missing");
            pass = false;
            continue;
        };
        let Some(node) = lp.tree().get(nid) else {
            println!("selftest: inline {tag} has no node");
            pass = false;
            continue;
        };
        let Some(layout) = node.inline.clone() else {
            println!("selftest: inline {tag} has no layout");
            pass = false;
            continue;
        };
        let origin = node.visual_rect().origin;
        let mut atoms = 0usize;
        let mut own_row = false;
        for (index, line) in layout.lines.iter().enumerate() {
            for atom in &line.atoms {
                atoms += 1;
                // Inside its line box, on both axes.
                if atom.x < -EPS
                    || atom.x + atom.size.w > line.width + EPS
                    || atom.y < -EPS
                    || atom.y + atom.size.h > line.height + EPS
                {
                    bad.push(format!(
                        "atom at {} sticks out of line {index} ({:.1}x{:.1})",
                        atom.at, line.width, line.height
                    ));
                }
                if index > 0 && atom.x.abs() < EPS {
                    own_row = true;
                }
                // No glyph of this block may paint over the atom's box.
                let rect = atom.rect(ui_core::geom::Point::new(origin.x, origin.y + line.y));
                let list = lp.draw_list();
                for (g, owner) in list.prims.glyphs.iter().zip(&list.owners.glyphs) {
                    if *owner != nid {
                        continue;
                    }
                    let hit_x = rect.origin.x + 1.0 < g.rect.origin.x + g.rect.size.w
                        && g.rect.origin.x + 1.0 < rect.origin.x + rect.size.w;
                    let hit_y = rect.origin.y + 1.0 < g.rect.origin.y + g.rect.size.h
                        && g.rect.origin.y + 1.0 < rect.origin.y + rect.size.h;
                    if hit_x && hit_y {
                        bad.push(format!(
                            "a glyph at ({:.1},{:.1}) paints into the atom at {}",
                            g.rect.origin.x, g.rect.origin.y, atom.at
                        ));
                    }
                }
            }
        }
        if atoms == 0 {
            bad.push("no atom on any line".to_string());
        }
        // The narrow sample must push its atom onto a line of its own.
        if tag == "narrow" && !own_row {
            bad.push("the wide atom shares a line instead of starting one".to_string());
        }
        if layout.width > width + EPS {
            bad.push(format!(
                "the block is {:.1} wide, over its {width:.1} box",
                layout.width
            ));
        }
        for line in &bad {
            println!("selftest: inline {tag}: {line}");
        }
        if bad.is_empty() {
            good += 1;
            let xs: Vec<String> = layout
                .lines
                .iter()
                .flat_map(|l| {
                    l.atoms
                        .iter()
                        .map(|a| format!("{:.1}@{:.1}/{:.1}", a.x, a.y, a.size.h))
                })
                .collect();
            let rows = layout
                .lines
                .iter()
                .enumerate()
                .map(|(i, l)| format!("[{i}] w{:.0} y{:.0} h{:.0}", l.width, l.y, l.height))
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "selftest: inline {tag} atoms={atoms} {} | rows {rows}",
                xs.join(" ")
            );
        } else {
            pass = false;
        }
    }
    // A baseline row holds its items on one line whatever their size. The mark has no
    // baseline of its own, so it counts by its bottom edge, the way the engine synthesises
    // one for a box.
    let mut line = Vec::new();
    for tag in ["small", "large"] {
        let id = Id::of(("inline-row-text", Page::Inline.tag(), tag));
        let found = lp
            .tree()
            .find_by_id(id)
            .and_then(|nid| lp.tree().get(nid).map(|n| (n.visual_rect(), n.baseline)));
        match found {
            Some((rect, baseline)) => line.push((tag, rect.origin.y + baseline)),
            None => {
                println!("selftest: inline row text {tag} missing");
                pass = false;
            }
        }
    }
    let mark = Id::of(("inline-row-mark", Page::Inline.tag()));
    match lp
        .tree()
        .find_by_id(mark)
        .and_then(|nid| lp.tree().get(nid))
    {
        Some(node) => {
            let rect = node.visual_rect();
            line.push(("mark", rect.origin.y + rect.size.h));
        }
        None => {
            println!("selftest: inline row mark missing");
            pass = false;
        }
    }
    let cross = line
        .first()
        .is_some_and(|(_, first)| line.iter().all(|(_, y)| (y - first).abs() <= EPS));
    if !cross {
        for (tag, y) in &line {
            println!("selftest: inline row {tag} baseline at {y:.2}");
        }
        pass = false;
    }
    println!(
        "selftest: inline cross baseline {cross} {}",
        line.iter()
            .map(|(tag, y)| format!("{tag} {y:.1}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Each rule style fills the same band its own way: one line, two, a row of dots, a
    // wave. The sample gives each style its own ink, so the paint can be counted.
    let inks = [
        (
            "single",
            ui_core::geom::Color::from_rgba8(255, 120, 140, 255),
        ),
        (
            "double",
            ui_core::geom::Color::from_rgba8(255, 200, 90, 255),
        ),
        (
            "dotted",
            ui_core::geom::Color::from_rgba8(120, 220, 140, 255),
        ),
        ("wavy", ui_core::geom::Color::from_rgba8(130, 190, 255, 255)),
    ];
    let list = lp.draw_list();
    let rules: Vec<(String, Vec<ui_core::geom::Rect>)> = inks
        .iter()
        .map(|(tag, ink)| {
            let rects = list
                .prims
                .rects
                .iter()
                .filter(|r| r.fill == *ink && r.rect.size.h <= 4.0)
                .map(|r| r.rect)
                .collect::<Vec<_>>();
            (tag.to_string(), rects)
        })
        .collect();
    let count = |tag: &str| {
        rules
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, r)| r.len())
            .unwrap_or(0)
    };
    let spread = |tag: &str| {
        let ys: Vec<f32> = rules
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, rects)| rects.iter().map(|r| r.origin.y).collect())
            .unwrap_or_default();
        if ys.is_empty() {
            return 0.0;
        }
        let high = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let low = ys.iter().copied().fold(f32::INFINITY, f32::min);
        high - low
    };
    let single = count("single") == 1;
    let double = count("double") == 2;
    let dotted = count("dotted") >= 5;
    let wavy = count("wavy") >= 10 && spread("wavy") > 0.5;
    if !(single && double && dotted && wavy) {
        pass = false;
    }
    println!(
        "selftest: inline rules single {single} double {double} dotted {} wavy {} spread {:.2}",
        count("dotted"),
        count("wavy"),
        spread("wavy")
    );

    // A justified paragraph stretches every full line to the block's edge and leaves its
    // last line at its own width.
    let mut justify = true;
    let id = Id::of(("inline-justify", Page::Inline.tag()));
    let found = lp.tree().find_by_id(id).and_then(|nid| {
        lp.tree()
            .get(nid)
            .map(|node| (nid, node.visual_rect(), node.inline.clone()))
    });
    match found {
        Some((nid, rect, layout)) => {
            let lines = layout.map(|l| l.lines.clone()).unwrap_or_default();
            let list = lp.draw_list();
            let right = rect.origin.x + rect.size.w;
            let mut edges = Vec::new();
            for (index, line) in lines.iter().enumerate() {
                let top = rect.origin.y + line.y;
                let bottom = top + line.height;
                let mut hi = f32::NEG_INFINITY;
                for (glyph, owner) in list.prims.glyphs.iter().zip(&list.owners.glyphs) {
                    if *owner != nid {
                        continue;
                    }
                    let centre = glyph.rect.origin.y + glyph.rect.size.h * 0.5;
                    if centre >= top && centre <= bottom {
                        hi = hi.max(glyph.rect.origin.x + glyph.rect.size.w);
                    }
                }
                edges.push((index, hi));
            }
            if lines.len() < 2 {
                justify = false;
            }
            for (index, hi) in &edges {
                // Ink stops short of the advance by the last glyph's own side bearing. The
                // sample's one hard break closes the line before the tail, and CSS leaves
                // that line flush along with the last one.
                let flush = *index + 1 == edges.len() || *index + 2 == edges.len();
                let ok = if flush {
                    *hi < right - 10.0
                } else {
                    *hi > right - 10.0 && *hi < right + 2.0
                };
                justify &= ok;
            }
            println!(
                "selftest: inline justify edges {} right {right:.1} {}",
                edges
                    .iter()
                    .map(|(i, hi)| format!("[{i}] {hi:.1}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                if justify { "✓" } else { "✗" }
            );
        }
        None => {
            println!("selftest: inline justify block missing");
            justify = false;
        }
    }
    pass &= justify;

    // A widget inside a sentence: the atom holds a child node, and the node lands on the
    // atom without covering the words around it.
    let holder_ok;
    let text_id = Id::of(("inline-holder", Page::Inline.tag()));
    let chip_id = Id::of(("inline-holder-chip", Page::Inline.tag()));
    let found = lp
        .tree()
        .find_by_id(text_id)
        .and_then(|nid| lp.tree().get(nid).map(|n| (nid, n.visual_rect())));
    let chip = lp
        .tree()
        .find_by_id(chip_id)
        .and_then(|nid| lp.tree().get(nid).map(|n| n.visual_rect()));
    match (found, chip) {
        (Some((nid, text_rect)), Some(chip_rect)) => {
            let list = lp.draw_list();
            let mut overlaps = 0usize;
            let mut after = 0usize;
            for (glyph, owner) in list.prims.glyphs.iter().zip(&list.owners.glyphs) {
                if *owner != nid {
                    continue;
                }
                let g = glyph.rect;
                let hit_x = g.origin.x + 1.0 < chip_rect.origin.x + chip_rect.size.w
                    && chip_rect.origin.x + 1.0 < g.origin.x + g.size.w;
                let hit_y = g.origin.y + 1.0 < chip_rect.origin.y + chip_rect.size.h
                    && chip_rect.origin.y + 1.0 < g.origin.y + g.size.h;
                if hit_x && hit_y {
                    overlaps += 1;
                }
                if g.origin.x >= chip_rect.origin.x + chip_rect.size.w - 1.0 {
                    after += 1;
                }
            }
            let inside = chip_rect.origin.y >= text_rect.origin.y - 1.0
                && chip_rect.origin.y + chip_rect.size.h
                    <= text_rect.origin.y + text_rect.size.h + 1.0
                && chip_rect.origin.x >= text_rect.origin.x - 1.0
                && chip_rect.origin.x + chip_rect.size.w
                    <= text_rect.origin.x + text_rect.size.w + 1.0;
            // The widget is an ordinary control: a press on it takes focus, and a press
            // on the words beside it is not the control's press.
            let middle = center(chip_rect);
            click(lp, size, middle);
            let focused = lp.focused() == Some(chip_id);
            let beside = Point::new(chip_rect.origin.x - 8.0, middle.y);
            click(lp, size, beside);
            let cleared = lp.focused() != Some(chip_id);
            holder_ok = overlaps == 0 && after > 0 && inside && focused && cleared;
            println!(
                "selftest: inline holder chip {:.1},{:.1} {:.0}x{:.0} overlaps {overlaps} after {after} inside {inside} focus {focused} cleared {cleared} {}",
                chip_rect.origin.x,
                chip_rect.origin.y,
                chip_rect.size.w,
                chip_rect.size.h,
                if holder_ok { "✓" } else { "✗" }
            );
        }
        _ => {
            println!("selftest: inline holder block or chip missing");
            holder_ok = false;
        }
    }
    pass &= holder_ok;

    println!(
        "selftest: inline samples {good}/{} keep atoms in their lines {}",
        INLINE_SAMPLES.len(),
        if good == INLINE_SAMPLES.len() {
            "✓"
        } else {
            "✗"
        }
    );
    pass
}
