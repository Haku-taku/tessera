// Self-test checks for the buttons page and cursor.

use crate::selftest::*;

pub(crate) fn buttons(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Buttons: the five size heights, and a toggle whose state the app owns.
    goto(lp, size, Page::Buttons);
    let want_h: [f32; 5] = [32.0, 40.0, 56.0, 96.0, 136.0];
    let tags = ["xs", "sm", "md", "lg", "xl"];
    let got_h: Vec<f32> = tags
        .iter()
        .map(|tag| {
            rect_of(lp, Id::of(("btn", Page::Buttons.tag(), *tag)))
                .map(|r| r.size.h)
                .unwrap_or(-1.0)
        })
        .collect();
    let sizes_ok = got_h
        .iter()
        .zip(&want_h)
        .all(|(got, want)| (got - want).abs() < 0.5);
    println!(
        "selftest: md3 button heights {got_h:?} (want {want_h:?}) {}",
        if sizes_ok { "✓" } else { "✗" }
    );
    let before = lp.app_mut().btn_sel[0];
    click(
        lp,
        size,
        center(rect_of(lp, btn_toggle_id(0)).expect("toggle button")),
    );
    let after = lp.app_mut().btn_sel[0];
    println!(
        "selftest: md3 button toggle {before}->{after} {}",
        if after != before { "✓" } else { "✗" }
    );
    if !(sizes_ok && after != before) {
        pass = false;
    }

    // The split button: both halves take the size's height, and each half reports itself.
    let split_heights: Vec<f32> = (0..5)
        .map(|k| {
            rect_of(lp, btn_split_sample_id(k).with("leading"))
                .map(|r| r.size.h)
                .unwrap_or(-1.0)
        })
        .collect();
    let split_sizes = split_heights
        .iter()
        .zip(&want_h)
        .all(|(got, want)| (got - want).abs() < 0.5);
    let open_before = lp.app_mut().split_open;
    click(
        lp,
        size,
        center(rect_of(lp, btn_split_id().with("trailing")).expect("split trailing half")),
    );
    let open_after = lp.app_mut().split_open;
    let presses_before = lp.app_mut().split_presses;
    click(
        lp,
        size,
        center(rect_of(lp, btn_split_id().with("leading")).expect("split leading half")),
    );
    let presses_after = lp.app_mut().split_presses;
    // The leading half's mark is the filled button table's icon, 24 dp at medium.
    let mark = rect_of(lp, btn_split_id().with("leading").with("icon")).map(|r| r.size.w);
    let mark_ok = mark.is_some_and(|w| (w - 24.0).abs() < 0.5);
    let split_ok = mark_ok
        && split_sizes
        && open_after != open_before
        && presses_after == presses_before + 1
        && lp.app_mut().split_open == open_after;
    println!(
        "selftest: split button halves {split_heights:?} (want {want_h:?}), mark {mark:?}, menu {open_before}->{open_after}, presses {presses_before}->{presses_after} {}",
        if split_ok { "✓" } else { "✗" }
    );
    if !split_ok {
        pass = false;
    }

    // The glass capsules: 56dp squares, and a labelled one that grows past its height.
    let icon = rect_of(lp, glass_btn_id(0));
    let labelled = rect_of(lp, glass_label_btn_id(0));
    let shapes_ok = icon
        .is_some_and(|r| (r.size.w - 56.0).abs() < 0.5 && (r.size.h - 56.0).abs() < 0.5)
        && labelled.is_some_and(|r| r.size.w > r.size.h && (r.size.h - 44.0).abs() < 0.5);
    println!(
        "selftest: glass buttons icon {:?}, labelled {:?} {}",
        icon.map(|r| (r.size.w, r.size.h)),
        labelled.map(|r| (r.size.w, r.size.h)),
        if shapes_ok { "✓" } else { "✗" }
    );
    if !shapes_ok {
        pass = false;
    }

    // A press on a glass capsule reaches the app; a disabled one stays silent.
    let before = lp.app_mut().glass_press;
    click(
        lp,
        size,
        center(rect_of(lp, glass_btn_id(0)).expect("glass icon button")),
    );
    let pressed = lp.app_mut().glass_press;
    let disabled = Id::of(("btn", Page::Buttons.tag(), "glass", "disabled"));
    click(
        lp,
        size,
        center(rect_of(lp, disabled).expect("disabled glass button")),
    );
    let after = lp.app_mut().glass_press;
    let press_ok = pressed == before + 1 && after == pressed;
    println!(
        "selftest: glass button presses {before}->{pressed}, disabled stays {after} {}",
        if press_ok { "✓" } else { "✗" }
    );
    if !press_ok {
        pass = false;
    }

    // Held down, the face inflates by a fixed 17 dp on its longest side and comes back.
    let face = glass_btn_id(0).with("face");
    let rest_w = rect_of(lp, face).map_or(0.0, |r| r.size.w);
    let at = center(rect_of(lp, glass_btn_id(0)).expect("glass icon button"));
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    step(lp, size, at, 1);
    lp.dispatch(ui_core::Event::PointerPressed {
        pos: at,
        button: ui_core::PointerButton::Primary,
    });
    step(lp, size, at, 30);
    let pressed_w = rect_of(lp, face).map_or(0.0, |r| r.size.w);
    lp.dispatch(ui_core::Event::PointerReleased {
        pos: at,
        button: ui_core::PointerButton::Primary,
    });
    step(lp, size, at, 60);
    let back_w = rect_of(lp, face).map_or(0.0, |r| r.size.w);
    let inflate_ok =
        (rest_w - 56.0).abs() < 0.5 && pressed_w > rest_w + 12.0 && (back_w - rest_w).abs() < 0.5;
    println!(
        "selftest: glass button face {rest_w:.1} -> {pressed_w:.1} held -> {back_w:.1} released {}",
        if inflate_ok { "✓" } else { "✗" }
    );
    if !inflate_ok {
        pass = false;
    }

    // Held down, the highlight sits on the finger and follows it; released, it is gone.
    // Its centre is what a wrong offset moves, so the rect is read rather than the pixels.
    let glare = glass_btn_id(0).with("glare");
    let at = center(rect_of(lp, glass_btn_id(0)).expect("glass icon button"));
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    step(lp, size, at, 1);
    lp.dispatch(ui_core::Event::PointerPressed {
        pos: at,
        button: ui_core::PointerButton::Primary,
    });
    step(lp, size, at, 30);
    fn miss(lp: &AppLoop<Gallery>, glare: Id, at: Point) -> f32 {
        rect_of(lp, glare).map_or(f32::INFINITY, |r| {
            let c = center(r);
            (c.x - at.x).abs().max((c.y - at.y).abs())
        })
    }
    let held = miss(lp, glare, at);
    let moved = Point::new(at.x + 10.0, at.y - 6.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos: moved });
    step(lp, size, moved, 2);
    let followed = miss(lp, glare, moved);
    lp.dispatch(ui_core::Event::PointerReleased {
        pos: moved,
        button: ui_core::PointerButton::Primary,
    });
    step(lp, size, moved, 60);
    let gone = rect_of(lp, glare).is_none();
    let glare_ok = held < 1.0 && followed < 1.0 && gone;
    println!(
        "selftest: glass button glare held off by {held:.1}, moved off by {followed:.1}, gone {gone} {}",
        if glare_ok { "✓" } else { "✗" }
    );
    if !glare_ok {
        pass = false;
    }
    pass
}

pub(crate) fn cursor(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The cursor is a per-frame output resolved from the hover path: a control that declares a
    // hand must yield one, and bare ground must fall back to the arrow. No pixel carries this,
    // so nothing else can catch a shape that is produced but never applied.
    goto(lp, size, Page::Buttons);
    let over = center(rect_of(lp, btn_toggle_id(0)).expect("toggle button"));
    lp.dispatch(ui_core::Event::PointerMoved { pos: over });
    let on_control = lp.build_with_dt(size, Some(over), 1.0).cursor;
    let ground = Point::new(size.0 as f32 - 4.0, size.1 as f32 - 4.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos: ground });
    let on_ground = lp.build_with_dt(size, Some(ground), 1.0).cursor;
    let cursor_ok = on_control == ui_core::CursorShape::PointingHand
        && on_ground == ui_core::CursorShape::Arrow;
    println!(
        "selftest: cursor over a button {on_control:?}, over bare ground {on_ground:?} {}",
        if cursor_ok { "✓" } else { "✗" }
    );
    if !cursor_ok {
        pass = false;
    }
    pass
}
