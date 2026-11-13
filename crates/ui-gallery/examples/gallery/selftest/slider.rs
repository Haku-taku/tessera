// Self-test checks for the slider page.

use crate::selftest::*;

pub(crate) fn slider(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> (bool, f32, bool) {
    let mut pass = true;
    let button = ui_core::PointerButton::Primary;
    // Drag the MD3 slider, then leave and come back: the value must be the same.
    goto(lp, size, Page::Slider);
    // The five size variants must reach layout as five different heights.
    let mut heights = Vec::new();
    for k in 0..SLIDER_SIZES.len() {
        let id = if k == 0 {
            slider_id(Page::Slider, "md3")
        } else {
            slider_size_id(k)
        };
        let want = ui_widgets_md3::spec::slider_spec(
            ui_widgets_md3::spec::SliderFlags {
                active: true,
                ..ui_widgets_md3::spec::SliderFlags::NONE
            },
            SLIDER_SIZES[k],
        )
        .handle
        .height
        .max(ui_widgets_md3::SLIDER_TOUCH);
        let got = rect_of(lp, id).map(|r| r.size.h).unwrap_or(-1.0);
        heights.push((got, want));
    }
    let sizes_ok = heights.iter().all(|(got, want)| (got - want).abs() < 0.5);
    println!(
        "selftest: md3 slider sizes {:?} {}",
        heights,
        if sizes_ok { "✓" } else { "✗" }
    );
    if !sizes_ok {
        pass = false;
    }

    // A press commits nothing: only a tap's release or a real drag sets the value. Committing
    // on the press would send the thumb off the moment it is touched.
    for (tag, bar_tag, map) in [
        (
            "glass",
            "stack",
            ui_widgets_glass::slider_value_at as fn(f32) -> f32,
        ),
        ("fluent", "bar", ui_widgets_fluent::slider_value_at),
    ] {
        let id = slider_id(Page::Slider, tag);
        let bar = rect_of(lp, id.with(bar_tag)).expect("slider bar");
        let at = Point::new(bar.origin.x + 200.0, bar.origin.y + bar.size.h * 0.5);
        let before = lp.app_mut().value;
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        frame(lp, size, Some(at));
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        frame(lp, size, Some(at));
        let held = lp.app_mut().value;
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        frame(lp, size, Some(at));
        let tapped = lp.app_mut().value;
        let want = map(200.0);
        let ok = held == before && (tapped - want).abs() < 1e-3;
        println!(
            "selftest: {tag} slider press holds {held:.3} (was {before:.3}), tap {tapped:.3} (want {want:.3}) {}",
            if ok { "✓" } else { "✗" }
        );
        if !ok {
            pass = false;
        }

        // A drag must move the value while the finger is still down, not only when it lifts.
        if tag == "fluent" {
            // The thumb rides the track's centre line; a Box would flow it a track below.
            let mid = |id: ui_core::Id| rect_of(lp, id).map(|r| r.origin.y + r.size.h * 0.5);
            let track_mid = mid(id.with("track")).unwrap_or(f32::NAN);
            let thumb_mid = mid(id.with("thumb")).unwrap_or(f32::NAN);
            let centred = (thumb_mid - track_mid).abs() < 0.01;
            println!(
                "selftest: fluent slider thumb centre {thumb_mid:.2} on track centre {track_mid:.2} {}",
                if centred { "✓" } else { "✗" }
            );
            if !centred {
                pass = false;
            }
        }
        let from = Point::new(bar.origin.x + 100.0, bar.origin.y + bar.size.h * 0.5);
        // Let the tap's own glide land first: a press banks where the thumb stands.
        lp.dispatch(ui_core::Event::PointerMoved { pos: from });
        for _ in 0..20 {
            let _ = lp.build_with_dt(size, Some(from), 0.016);
        }
        let settled = lp.app_mut().value;
        lp.dispatch(ui_core::Event::PointerPressed { pos: from, button });
        frame(lp, size, Some(from));
        let mut to = from;
        for step in [20.0f32, 40.0, 60.0] {
            to = Point::new(from.x + step, from.y);
            lp.dispatch(ui_core::Event::PointerMoved { pos: to });
            frame(lp, size, Some(to));
        }
        // The app applies a reported change on the frame after it is reported.
        frame(lp, size, Some(to));
        let mid = lp.app_mut().value;
        let follows = (mid - settled).abs() > 0.01;
        println!(
            "selftest: {tag} slider drag moved the value {settled:.3}->{mid:.3} before release {}",
            if follows { "✓" } else { "✗" }
        );
        if !follows {
            pass = false;
        }
        // Letting go must keep the thumb where the finger left it: that frame arrives before
        // the app has applied the value, so a thumb that reads it rewinds to the drag's start.
        let centre = |lp: &AppLoop<Gallery>| {
            rect_of(lp, id.with("thumb")).map(|r| r.origin.x + r.size.w * 0.5)
        };
        let under_finger = centre(lp).unwrap_or(f32::NAN);
        lp.dispatch(ui_core::Event::PointerReleased { pos: to, button });
        frame(lp, size, Some(to));
        let after_release = centre(lp).unwrap_or(f32::NAN);
        let stays = (after_release - under_finger).abs() < 1.0;
        println!(
            "selftest: {tag} slider release keeps the thumb at {under_finger:.2}, now {after_release:.2} {}",
            if stays { "✓" } else { "✗" }
        );
        if tag == "fluent" && !stays {
            pass = false;
        }
    }

    // A press must melt the glass thumb into the reference's 58×38.333 lens about the value's
    // centre, and the release must bring it back to 37×24. The ids differ from the switch's,
    // so its own check does not cover this wiring.
    let gid = slider_id(Page::Slider, "glass");
    let stack = rect_of(lp, gid.with("stack")).expect("glass slider");
    let thumb = |lp: &AppLoop<Gallery>| rect_of(lp, gid.with("thumb"));
    let at = Point::new(stack.origin.x + 200.0, stack.origin.y + stack.size.h * 0.5);
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    // Let the tap's own spring land before pressing: a press banks the thumb where it stands,
    // so an unfinished flight would be measured as if the press had moved it.
    // 0.64 s covers the 0.2 s tap hold plus the 0.6 s contraction.
    for _ in 0..40 {
        let _ = lp.build_with_dt(size, Some(at), 0.016);
    }
    let rest = thumb(lp).expect("glass thumb");
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    let mut peak = 0.0f32;
    for _ in 0..20 {
        let _ = lp.build_with_dt(size, Some(at), 0.016);
        peak = peak.max(thumb(lp).map(|r| r.size.h).unwrap_or(0.0));
    }
    let held = thumb(lp).expect("glass thumb");
    // The press holds the thumb where the value put it: the shared value belongs to the other
    // sliders on the page too, so the pointer is not where the thumb stands.
    let rest_centre = rest.origin.x + rest.size.w * 0.5;
    let held_centre = held.origin.x + held.size.w * 0.5;
    let centre = held_centre - rest_centre;
    let swells = (ui_widgets_glass::SLIDER_EXPANDED_H * 0.98
        ..ui_widgets_glass::SLIDER_EXPANDED_H * 1.2)
        .contains(&peak)
        && centre.abs() < 0.2;
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    for _ in 0..60 {
        let _ = lp.build_with_dt(size, Some(at), 0.016);
    }
    let back = thumb(lp).expect("glass thumb");
    let rounds_down =
        (back.size.h - rest.size.h).abs() < 0.5 && (back.size.w - rest.size.w).abs() < 0.5;
    println!(
        "selftest: glass slider press peaked {peak:.1}px tall (expanded {:.1}, rest {:.1}), centre moved {centre:.2}px, released to {:.1}x{:.1} {}",
        ui_widgets_glass::SLIDER_EXPANDED_H,
        rest.size.h,
        back.size.w,
        back.size.h,
        if swells && rounds_down { "✓" } else { "✗" }
    );
    if !(swells && rounds_down) {
        pass = false;
    }

    let slider = rect_of(lp, slider_id(Page::Slider, "md3")).expect("md3 slider");
    let inset = ui_widgets_md3::spec::slider_spec(
        ui_widgets_md3::spec::SliderFlags {
            active: true,
            ..ui_widgets_md3::spec::SliderFlags::NONE
        },
        ui_widgets_md3::spec::SliderSize::XSmall,
    )
    .track_shape
    .leading;
    let travel = slider.size.w - 2.0 * inset;
    let start = Point::new(
        slider.origin.x + inset + travel * 0.5,
        slider.origin.y + slider.size.h * 0.5,
    );
    lp.dispatch(ui_core::Event::PointerMoved { pos: start });
    frame(lp, size, Some(start));
    lp.dispatch(ui_core::Event::PointerPressed { pos: start, button });
    frame(lp, size, Some(start));
    let mut last = start;
    for step in [20.0f32, 40.0] {
        last = Point::new(start.x + step, start.y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: last });
        frame(lp, size, Some(last));
    }
    lp.dispatch(ui_core::Event::PointerReleased { pos: last, button });
    frame(lp, size, Some(last));
    let dragged = lp.app_mut().value;
    let want = (0.5 + 40.0 / travel).min(1.0);
    let drag_ok = (dragged - want).abs() < 0.02;
    println!(
        "selftest: slider drag value {dragged:.3} (want {want:.3}) {}",
        if drag_ok { "✓" } else { "✗" }
    );
    if !drag_ok {
        pass = false;
    }

    // A stepped slider snaps to its stops: 0.62 along is nearer 0.5 than 0.75.
    let stepped = rect_of(lp, slider_step_id()).expect("stepped slider");
    let step_inset = ui_widgets_md3::spec::slider_spec(
        ui_widgets_md3::spec::SliderFlags {
            active: true,
            ..ui_widgets_md3::spec::SliderFlags::NONE
        },
        ui_widgets_md3::spec::SliderSize::Small,
    )
    .track_shape
    .leading;
    let step_travel = stepped.size.w - 2.0 * step_inset;
    click(
        lp,
        size,
        Point::new(
            stepped.origin.x + step_inset + step_travel * 0.62,
            stepped.origin.y + stepped.size.h * 0.5,
        ),
    );
    let snapped = lp.app_mut().step_value;
    let snap_ok = (snapped - 0.5).abs() < 1e-3;
    println!(
        "selftest: stepped slider snapped to {snapped:.3} (want 0.500) {}",
        if snap_ok { "✓" } else { "✗" }
    );
    if !snap_ok {
        pass = false;
    }

    // A range slider moves the nearer handle only: a click near the trailing one at 0.9
    // leaves the leading one at 0.2.
    let ranged = rect_of(lp, slider_range_id()).expect("range slider");
    let range_inset = step_inset;
    let range_travel = ranged.size.w - 2.0 * range_inset;
    click(
        lp,
        size,
        Point::new(
            ranged.origin.x + range_inset + range_travel * 0.9,
            ranged.origin.y + ranged.size.h * 0.5,
        ),
    );
    let (rs, re) = lp.app_mut().range;
    let range_ok = (rs - 0.2).abs() < 0.02 && (re - 0.9).abs() < 0.02;
    println!(
        "selftest: range slider {rs:.3}..{re:.3} (want 0.200..0.900) {}",
        if range_ok { "✓" } else { "✗" }
    );
    if !range_ok {
        pass = false;
    }
    (pass, dragged, drag_ok)
}
