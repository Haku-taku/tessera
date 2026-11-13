// Self-test checks for the math page: pack ink, the structural editor, and a refused source.

use crate::selftest::*;

pub(crate) fn math(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    goto(lp, size, Page::Math);
    let appearance = appearance_of(lp.app_mut().dark);
    let packs = [
        ("md3", ui_widgets_md3::pack_of(appearance).ink("onSurface")),
        (
            "fluent",
            ui_widgets_fluent::pack_of(appearance).ink("TextFillColorPrimaryBrush"),
        ),
        (
            "glass",
            ui_widgets_glass::pack_of(appearance).ink("textFillColorPrimary"),
        ),
    ];
    let mut good = 0usize;
    for (tag, want) in packs {
        let Some(want) = want else {
            println!("selftest: math {tag} pack has no text ink");
            continue;
        };
        let id = Id::of(("math-formula", tag, "display"));
        match ink_of(lp, id) {
            Some(ink) if ink == want => good += 1,
            Some(ink) => println!("selftest: math {tag} formula ink {ink:?} is not {want:?}"),
            None => println!("selftest: math {tag} formula is missing"),
        }
    }
    let mark = if good == packs.len() { "✓" } else { "✗" };
    println!(
        "selftest: math formula ink {good}/{} match the pack {mark}",
        packs.len()
    );

    // ── The structural editor: a click focuses the formula, the typed character reaches the
    // app's source through `Change::Text`, and the control keeps drawing a formula.
    let edited = math_edit_id("md3");
    let card = rect_of(lp, edited).expect("md3 structured formula");
    click(lp, size, center(card));
    let before = lp.app_mut().math_edit_src[0].clone();
    type_text(lp, size, "3");
    let after = lp.app_mut().math_edit_src[0].clone();
    let typed = after.matches('3').count() == before.matches('3').count() + 1;
    let still_formula = is_math(lp, edited);
    println!("selftest: math edit {before:?} -> {after:?}, typed {typed}, formula {still_formula}");

    // ── A refused source keeps the structure around it: the fraction still renders, its
    // denominator's bad run is a hover target inside the formula, and the reason hangs from
    // the target while the pointer is over it.
    let refused_id = math_refused_id();
    let formula = refused_id.with("formula");
    // The error counter is `usize` in the widget; an `i32` literal would hash a shorter salt.
    let target = refused_id.with(("error", 0usize));
    let hint = target.with("hint");
    let formula_math = is_math(lp, formula);
    let Some(target_rect) = rect_of(lp, target) else {
        println!("selftest: math error target missing");
        return false;
    };
    let formula_rect = rect_of(lp, formula);
    let height = formula_rect.map(|formula| formula.size.h);
    let structure = height.is_some_and(|height| height > 26.0);
    // The target is padded past the run, so it may cross the formula's edge; its centre is
    // what says the run is inside the formula, and below the baseline that it is in the
    // denominator rather than on the line.
    let (marked, inside, denominator) = match (math_metrics(lp, formula), formula_rect) {
        (Some((ascent, errors)), Some(formula)) => {
            let center = center(target_rect);
            (
                errors == 1,
                center.x >= formula.origin.x
                    && center.x <= formula.origin.x + formula.size.w
                    && center.y >= formula.origin.y
                    && center.y <= formula.origin.y + formula.size.h,
                center.y > formula.origin.y + ascent,
            )
        }
        _ => (false, false, false),
    };
    let hidden_before = lp.tree().find_by_id(hint).is_none();
    let at = center(target_rect);
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    // The hover path is read from the pointer a frame later, so the panel mounts on the second.
    frame(lp, size, Some(at));
    frame(lp, size, Some(at));
    let present = lp.tree().find_by_id(hint).is_some();
    let below = rect_of(lp, hint)
        .is_some_and(|hint| hint.origin.y >= target_rect.origin.y + target_rect.size.h);
    println!(
        "selftest: math refused: formula {formula_math} height {height:?} marked {structure} one {marked}, target inside {inside} below baseline {denominator}, hint hidden {hidden_before}, present {present}, below {below}"
    );

    good == packs.len()
        && typed
        && still_formula
        && formula_math
        && structure
        && marked
        && inside
        && denominator
        && hidden_before
        && present
        && below
}

/// A formula's ascent and how many runs the parser refused inside it.
fn math_metrics(lp: &AppLoop<Gallery>, id: Id) -> Option<(f32, usize)> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Math { formula, .. } => Some((formula.ascent, formula.errors.len())),
        _ => None,
    }
}

/// Whether `id` names a node the host laid out as a formula.
fn is_math(lp: &AppLoop<Gallery>, id: Id) -> bool {
    lp.tree()
        .find_by_id(id)
        .and_then(|nid| lp.tree().get(nid))
        .is_some_and(|node| matches!(node.data, ui_core::NodeData::Math { .. }))
}

/// The ink a formula node asks for, whether it drew a formula or fell back to its source.
fn ink_of(lp: &AppLoop<Gallery>, id: Id) -> Option<ui_core::geom::Color> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Math { ink, .. } => Some(*ink),
        ui_core::NodeData::Text { ink, .. } => Some(*ink),
        _ => None,
    }
}
