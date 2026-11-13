// Self-test checks for the text selection page.

use crate::selftest::*;

pub(crate) fn selection(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Selection: a drag over a paragraph must put each pack's wash in the draw list, and
    // Fluent must also invert the glyphs — the painter, not the engine, decides both.
    goto(lp, size, Page::Selection);
    let appearance = appearance_of(lp.app_mut().dark);
    let packs = [
        ("md3", ui_widgets_md3::pack_of(appearance)),
        ("fluent", ui_widgets_fluent::pack_of(appearance)),
        ("glass", ui_widgets_glass::pack_of(appearance)),
    ];
    let mut sel_ok = 0;
    for (tag, pack) in &packs {
        let id = Id::of(("selection-text", *tag, 0));
        let Some(rect) = rect_of(lp, id) else {
            println!("selftest: selection {tag} paragraph missing");
            continue;
        };
        let from = Point::new(rect.origin.x + 3.0, rect.origin.y + rect.size.h * 0.5);
        let to = Point::new(rect.origin.x + rect.size.w * 0.7, from.y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: from });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: to,
            button: ui_core::PointerButton::Primary,
        });
        frame(lp, size, None);
        let wash = pack
            .roles
            .get(ui_style::RoleName("textSelection"))
            .is_some_and(|want| lp.draw_list().prims.rects.iter().any(|r| r.fill == want));
        let inverted = *tag != "fluent"
            || lp
                .draw_list()
                .prims
                .glyphs
                .iter()
                .any(|g| g.color == Color::WHITE);
        println!(
            "selftest: selection {tag} wash={wash} inverted={inverted} {}",
            if wash && inverted { "✓" } else { "✗" }
        );
        if wash && inverted {
            sel_ok += 1;
        }
    }
    println!("selftest: selection {sel_ok}/3 ✓");
    if sel_ok != 3 {
        pass = false;
    }
    // A glyph prim is a raster cell, which overhangs the line box by up to a pixel of padding
    // plus device-pixel binning; a box short of a whole line is an order of magnitude more.
    const INK_SLACK: f32 = 2.0;
    // Stacking: the three paragraphs in a column must not overlap, and each box must cover
    // every line it paints — a shorter box stacks the next paragraph on top of those lines.
    let mut fit_ok = 0;
    for (tag, _) in &packs {
        let mut boxes = Vec::new();
        let mut bad = Vec::new();
        for k in 0..3 {
            let id = Id::of(("selection-text", *tag, k));
            let Some(rect) = rect_of(lp, id) else {
                bad.push(format!("paragraph {k} missing"));
                continue;
            };
            boxes.push(rect);
            let Some(nid) = lp.tree().find_by_id(id) else {
                bad.push(format!("paragraph {k} has no node"));
                continue;
            };
            let list = lp.draw_list();
            let painted = list
                .prims
                .glyphs
                .iter()
                .zip(&list.owners.glyphs)
                .filter(|(_, owner)| **owner == nid)
                .fold(f32::NEG_INFINITY, |acc, (g, _)| {
                    acc.max(g.rect.origin.y + g.rect.size.h)
                });
            let bottom = rect.origin.y + rect.size.h;
            if painted.is_finite() && painted > bottom + INK_SLACK {
                bad.push(format!(
                    "paragraph {k} paints to {painted} but its box ends at {bottom}"
                ));
            }
        }
        for i in 0..boxes.len() {
            for j in (i + 1)..boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                if a.origin.y < b.origin.y + b.size.h - 0.5
                    && b.origin.y < a.origin.y + a.size.h - 0.5
                {
                    bad.push(format!("paragraphs {i} and {j} overlap: {a:?} {b:?}"));
                }
            }
        }
        for line in &bad {
            println!("selftest: selection {tag}: {line}");
        }
        if bad.is_empty() {
            fit_ok += 1;
        } else {
            pass = false;
        }
    }
    println!(
        "selftest: selection paragraphs {fit_ok}/3 fit their boxes {}",
        if fit_ok == 3 { "✓" } else { "✗" }
    );
    if fit_ok != 3 {
        pass = false;
    }
    // Three paragraphs are three text nodes: one drag from the first into the third must
    // cover all of them, and the copy must carry one line per paragraph.
    let mut crossed = false;
    let mut covered = false;
    let first = rect_of(lp, Id::of(("selection-text", "md3", 0)));
    let third = rect_of(lp, Id::of(("selection-text", "md3", 2)));
    let middle_rect = rect_of(lp, Id::of(("selection-text", "md3", 1)));
    let middle = painted_text(lp, Id::of(("selection-text", "md3", 1)));
    if let (Some(first), Some(third), Some(middle_rect), Some(middle)) =
        (first, third, middle_rect, middle)
    {
        let from = Point::new(
            first.origin.x + first.size.w * 0.3,
            first.origin.y + first.size.h * 0.5,
        );
        let to = Point::new(
            third.origin.x + third.size.w * 0.5,
            third.origin.y + third.size.h * 0.5,
        );
        lp.dispatch(ui_core::Event::PointerMoved { pos: from });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: to,
            button: ui_core::PointerButton::Primary,
        });
        frame(lp, size, None);
        // The middle paragraph owns no end of the drag, yet it must paint its own wash.
        covered = ui_widgets_md3::pack_of(appearance_of(lp.app_mut().dark))
            .roles
            .get(ui_style::RoleName("textSelection"))
            .is_some_and(|want| {
                lp.draw_list().prims.rects.iter().any(|r| {
                    r.fill == want
                        && r.rect.origin.y < middle_rect.origin.y + middle_rect.size.h
                        && middle_rect.origin.y < r.rect.origin.y + r.rect.size.h
                })
            });
        lp.take_clipboard_commands();
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            ctrl: true,
            ..ui_core::Modifiers::default()
        }));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char('c'),
        });
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
        crossed = lp.take_clipboard_commands().iter().any(|command| {
            let ui_platform::clipboard::ClipboardCommand::Copy { content, .. } = command else {
                return false;
            };
            let ui_core::clipboard::ClipboardContent::Text(text) = content else {
                return false;
            };
            let lines: Vec<&str> = text.split('\n').collect();
            lines.len() == 3 && lines[1] == middle
        });
    }
    println!(
        "selftest: selection cross-paragraph copy={crossed} covered={covered} {}",
        if crossed && covered { "✓" } else { "✗" }
    );
    if !(crossed && covered) {
        pass = false;
    }
    pass
}
