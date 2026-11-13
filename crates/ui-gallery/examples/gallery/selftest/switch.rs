// Self-test checks for the switch page and app state.

use crate::selftest::*;

pub(crate) fn switch_state(
    lp: &mut AppLoop<Gallery>,
    size: (u32, u32),
    dragged: f32,
    drag_ok: bool,
) -> bool {
    let mut pass = true;
    let button = ui_core::PointerButton::Primary;

    // Toggle on the Switch page, then walk through other sections and back.
    goto(lp, size, Page::Switch);

    let toggled = lp.app_mut().on;
    for page in [Page::Layouts, Page::Materials, Page::Switch] {
        goto(lp, size, page);
    }
    let back = lp.app_mut();
    let value_kept = (back.value - dragged).abs() < 1e-6;
    let on_kept = back.on == toggled;
    let page_kept = back.page == Page::Switch;
    println!(
        "selftest: value {dragged:.3} kept={value_kept}, on {toggled} kept={on_kept}, page kept={page_kept}"
    );
    if !(drag_ok && value_kept && on_kept && page_kept) {
        pass = false;
    }

    // The engine clock moves the control. Sample the whole chain (switch, track, thumb)
    // before and after frames of scripted time, then report what actually changed.
    // Drive the first MD3 variant (enabled, with icon, seeded off): clicking flips its own
    // on-value, so the transition runs and the clock carries the thumb across the track.
    goto(lp, size, Page::Switch);
    frame(lp, size, None);
    let root = md3_variant_id(0);
    let at = center(rect_of(lp, root).expect("md3 variant 0"));
    let chain = |lp: &AppLoop<Gallery>| {
        (
            rect_of(lp, root),
            shape_rect(lp, root, 0),
            biggest_shape_rect(lp, root),
        )
    };
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    let _ = lp.build_with_dt(size, Some(at), 1.0);
    let before = chain(lp);
    let was_on = lp.app_mut().variant_on[0];
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    let _ = lp.build_with_dt(size, Some(at), 0.016);
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    // Missing nodes report no motion rather than panicking: the gallery's page churn is
    // already known to retire nodes for a frame or two.
    let d = |a: Option<Rect>, b: Option<Rect>| -> f32 {
        match (a, b) {
            (Some(a), Some(b)) => (a.origin.x - b.origin.x)
                .abs()
                .max((a.size.w - b.size.w).abs())
                .max((a.size.h - b.size.h).abs()),
            _ => 0.0,
        }
    };
    let mut biggest = 0.0_f32;
    for _ in 0..20 {
        let _ = lp.build_with_dt(size, Some(at), 0.016);
        let now = chain(lp);
        biggest = biggest
            .max(d(now.0, before.0))
            .max(d(now.1, before.1))
            .max(d(now.2, before.2));
    }
    let after = chain(lp);
    let flipped = lp.app_mut().variant_on[0] != was_on;
    let moved = biggest.max(d(after.2, before.2));
    let animated = moved > 0.5 && flipped;
    println!(
        "selftest: engine-clock click moved the MD3 control {moved:.1}px {} (on={}, switch x {:.1}->{:.1}, track x {:.1}->{:.1}, thumb x {:.1}->{:.1} w {:.1}->{:.1})",
        if animated { "✓" } else { "✗" },
        lp.app_mut().on,
        before.0.map(|r| r.origin.x).unwrap_or(f32::NAN),
        after.0.map(|r| r.origin.x).unwrap_or(f32::NAN),
        before.1.map(|r| r.origin.x).unwrap_or(f32::NAN),
        after.1.map(|r| r.origin.x).unwrap_or(f32::NAN),
        before.2.map(|r| r.origin.x).unwrap_or(f32::NAN),
        after.2.map(|r| r.origin.x).unwrap_or(f32::NAN),
        before.2.map(|r| r.size.w).unwrap_or(f32::NAN),
        after.2.map(|r| r.size.w).unwrap_or(f32::NAN)
    );
    if !animated {
        pass = false;
    }

    // Settle the previous animation first, then hold the clock still: ten frames must not
    // move anything.  Letting it settle is the point — this measures the clock, not the
    // travel that is still in flight.
    let _ = lp.build_with_dt(size, Some(at), 1.0);
    let _ = lp.build_with_dt(size, Some(at), 1.0);
    let flip_from = lp.app_mut().on;
    lp.app_mut().on = !flip_from;
    // The frame that carries the flip lands on the near end; every frame after it must
    // hold still, because the clock is not moving.
    // `dt == 0` means "use the wall clock", so a frozen clock needs a step that is positive
    // but negligible; otherwise the check measures real elapsed time instead of the clock.
    let frozen = 1e-9;
    // The flip frame itself may differ from the next by a sub-pixel amount; what this check
    // measures is drift over the frames that follow, so the baseline starts one frame later.
    let _ = lp.build_with_dt(size, Some(at), frozen);
    let _ = lp.build_with_dt(size, Some(at), frozen);
    let frozen_before = last_shape_rect(lp, root).expect("thumb").origin.x;
    for _ in 0..10 {
        let _ = lp.build_with_dt(size, Some(at), frozen);
    }
    let frozen_after = last_shape_rect(lp, root).expect("thumb").origin.x;
    let _ = flip_from;
    // What moves here is an instance snapping the last fraction of a pixel onto its target,
    // not the clock: the bug this guards moved the thumb 23.9 px in ten frames.
    let frozen = (frozen_after - frozen_before).abs() < 0.1;
    println!(
        "selftest: frozen clock left the MD3 thumb at {:.3}px moved {}",
        (frozen_after - frozen_before).abs(),
        if frozen { "✓" } else { "✗" }
    );
    if !frozen {
        pass = false;
    }
    // The toolbar's appearance pill must actually flip the gallery between light and dark.
    let before = lp.app_mut().dark;
    if let Some(pill) = rect_of(lp, Id::str("toolbar/appearance")) {
        click(lp, size, center(pill));
        let after = lp.app_mut().dark;
        if after != before {
            println!("selftest: appearance toggled {before}->{after} ✓");
        } else {
            println!("selftest: appearance pill did not toggle (still {after})");
            pass = false;
        }
        // Flip back so later assertions see the default appearance.
        click(lp, size, center(pill));
    } else {
        println!("selftest: appearance pill is missing");
        pass = false;
    }

    // On the Switch page, the MD3 column must show all four spec variants.
    goto(lp, size, Page::Switch);
    let mut variants = 0;
    for k in 0..4 {
        if rect_of(lp, md3_variant_id(k)).is_some() {
            variants += 1;
        }
    }
    if variants == 4 {
        println!("selftest: md3 switch variants 4/4 ✓");
    } else {
        println!("selftest: md3 switch variants {variants}/4");
        pass = false;
    }

    // A press must not carry the Fluent knob anywhere: it is the drag that does that. The
    // knob's centre is the measure, since WinUI swells it in place while pressed.
    let fid = switch_id(Page::Switch, "fluent");
    let knob_centre =
        |lp: &AppLoop<Gallery>| rect_of(lp, fid.with("thumb")).map(|r| r.origin.x + r.size.w * 0.5);
    let root = rect_of(lp, fid).expect("fluent switch");
    let at = Point::new(root.origin.x + 20.0, root.origin.y + root.size.h * 0.5);
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    frame(lp, size, Some(at));
    let rest = knob_centre(lp).expect("fluent knob");
    let was = lp.app_mut().on;
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    frame(lp, size, Some(at));
    let held = knob_centre(lp).expect("fluent knob");
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    frame(lp, size, Some(at));
    // The change is applied by the frame after the one that reports it, and the tree shows it
    // one frame after that.
    frame(lp, size, Some(at));
    frame(lp, size, Some(at));
    let flipped_to = lp.app_mut().on;
    let landed = knob_centre(lp).expect("fluent knob");
    let ok = (held - rest).abs() < 0.1 && flipped_to != was && (landed - rest).abs() > 10.0;
    println!(
        "selftest: fluent switch press held the knob at {held:.1} (rest {rest:.1}), click flipped to {flipped_to} and landed at {landed:.1} {}",
        if ok { "✓" } else { "✗" }
    );
    if !ok {
        pass = false;
    }

    // A drag that ends outside the 40 dp track still commits by the side it reached: the
    // release lands past the right edge, where there is no pointer left to read.
    let was = lp.app_mut().on;
    let root = rect_of(lp, fid).expect("fluent switch");
    let start = Point::new(root.origin.x + 8.0, root.origin.y + root.size.h * 0.5);
    lp.dispatch(ui_core::Event::PointerMoved { pos: start });
    frame(lp, size, Some(start));
    lp.dispatch(ui_core::Event::PointerPressed { pos: start, button });
    frame(lp, size, Some(start));
    let mut to = start;
    for step in [10.0f32, 20.0, 30.0] {
        to = Point::new(start.x + step, start.y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        frame(lp, size, Some(to));
    }
    lp.dispatch(ui_core::Event::PointerReleased { pos: to, button });
    for _ in 0..4 {
        frame(lp, size, Some(to));
    }
    let dragged_on = lp.app_mut().on;
    let ok = dragged_on != was;
    println!(
        "selftest: fluent switch drag past the end flipped {was}->{dragged_on} {}",
        if ok { "✓" } else { "✗" }
    );
    if !ok {
        pass = false;
    }

    // The glass switch: a press blooms the capsule around the knob and commits nothing; only
    // the release flips it. The knob's centre is what must never move.
    let gid = switch_id(Page::Switch, "glass");
    let g = rect_of(lp, gid).expect("glass switch");
    if !lp.app_mut().on {
        click(lp, size, center(g));
        frame(lp, size, Some(center(g)));
    }
    let shell = |lp: &AppLoop<Gallery>| rect_of(lp, gid.with("thumb"));
    // The white material is the thumb's only shape; there is no drop shadow under it.
    let material = |lp: &AppLoop<Gallery>| shape_rect(lp, gid.with("thumb"), 0);
    let knob = |lp: &AppLoop<Gallery>| material(lp).map(|r| r.origin.x + r.size.w * 0.5);
    let at = Point::new(g.origin.x + 10.0, g.origin.y + g.size.h * 0.5);
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    frame(lp, size, Some(at));
    let rest = knob(lp).expect("glass knob");
    let rest_shell = shell(lp).expect("glass shell");
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    // The bloom is committed by the frame it starts on; the next one shows it grown.
    frame(lp, size, Some(at));
    frame(lp, size, Some(at));
    let held = knob(lp).expect("glass knob");
    let held_shell = shell(lp).expect("glass shell");
    // The glass swells around the knob: the knob's centre holds while the shell grows on both
    // sides. An edge-anchored bloom would carry the centre and spill to one side only.
    let rides = (held - rest).abs() < 0.2
        && held_shell.size.h > rest_shell.size.h + 2.0
        && (held_shell.origin.x - (held - held_shell.size.w * 0.5)).abs() < 0.2;
    let during = lp.app_mut().on;
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    for _ in 0..3 {
        frame(lp, size, Some(at));
    }
    let flipped_to = lp.app_mut().on;
    let landed = knob(lp).expect("glass knob");
    let ok = during && !flipped_to && (landed - rest).abs() > 10.0 && rides;
    println!(
        "selftest: glass switch press held the knob at {held:.1} (rest {rest:.1}) and swelled the shell to {:.1}x{:.1} (rest {:.1}x{:.1}) {}, click flipped to {flipped_to} and landed at {landed:.1}",
        held_shell.size.w,
        held_shell.size.h,
        rest_shell.size.w,
        rest_shell.size.h,
        if rides { "✓" } else { "✗" },
    );
    if !ok {
        pass = false;
    }

    // A drag carries the thumb under the finger and commits by the side it lands on.
    let from = Point::new(g.origin.x + 8.0, g.origin.y + g.size.h * 0.5);
    lp.dispatch(ui_core::Event::PointerMoved { pos: from });
    frame(lp, size, Some(from));
    lp.dispatch(ui_core::Event::PointerPressed { pos: from, button });
    frame(lp, size, Some(from));
    let mut to = from;
    for step in [10.0f32, 20.0, 30.0] {
        to = Point::new(from.x + step, from.y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        frame(lp, size, Some(to));
    }
    lp.dispatch(ui_core::Event::PointerReleased { pos: to, button });
    // The drag pulled past the end; the release has to spring that back before it rests.
    for _ in 0..8 {
        frame(lp, size, Some(to));
    }
    let dragged_on = lp.app_mut().on;
    let dragged_x = knob(lp).expect("glass knob");
    let ok = dragged_on && (dragged_x - rest).abs() < 1.0;
    println!(
        "selftest: glass switch drag turned it on={dragged_on} and left the knob at {dragged_x:.1} (want {rest:.1}) {}",
        if ok { "✓" } else { "✗" }
    );
    if !ok {
        pass = false;
    }

    // A click opens the lens, carries the knob across, then closes it: the shell must reach
    // the reference's 58×38.333 and come back to 37×24. The press here is one frame long.
    let at = center(g);
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    frame(lp, size, Some(at));
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    let _ = lp.build_with_dt(size, Some(at), 0.016);
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    let mut heights = Vec::new();
    let mut centres = Vec::new();
    // 60 frames: the 0.2 s hold, the 0.4 s expansion and the 0.6 s contraction.
    for _ in 0..60 {
        let _ = lp.build_with_dt(size, Some(at), 0.016);
        heights.push(shell(lp).map(|r| r.size.h).unwrap_or(0.0));
        centres.push(knob(lp).unwrap_or(0.0));
    }
    let peak = heights.iter().enumerate().fold(
        (0usize, 0.0f32),
        |b, (i, &h)| if h > b.1 { (i, h) } else { b },
    );
    let rest_h = rest_shell.size.h;
    // The damping ratio is 0.6, so the spring overshoots the expanded height by a few percent;
    // the jelly stretches it further while the knob flies — both are elasticity, not defects.
    let jelly_stretch = 1.0 + ui_widgets_glass::THUMB_JELLY_DISTORTION * 0.3;
    let swells = peak.0 >= 2
        && peak.1 >= ui_widgets_glass::EXPANDED_H * 0.98
        && peak.1 < ui_widgets_glass::EXPANDED_H * 1.2 * jelly_stretch;
    let rounds_up = heights.last().copied().unwrap_or(99.0) < rest_h + 0.5;
    let flew = (centres[0] - centres.last().copied().unwrap_or(0.0)).abs() > 10.0;
    println!(
        "selftest: glass switch click peaked {:.1}px tall at frame {} of 60 (expanded {:.1}) and ended at {:.1} (rest {rest_h:.1}), knob flew {:.1}px {}",
        peak.1,
        peak.0,
        ui_widgets_glass::EXPANDED_H,
        heights.last().copied().unwrap_or(f32::NAN),
        (centres[0] - centres.last().copied().unwrap_or(0.0)).abs(),
        if swells && rounds_up && flew {
            "✓"
        } else {
            "✗"
        },
    );
    if !(swells && rounds_up && flew) {
        pass = false;
    }

    pass
}
