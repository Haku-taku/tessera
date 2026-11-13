// Self-test checks for the modal sheet page.

use ui_widgets_glass::{SHEET_BOTTOM, SHEET_FULL_TOP, SHEET_HALF, SHEET_MARGIN};

use crate::selftest::*;

pub(crate) fn modal_sheet(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let w = size.0 as f32;
    let h = size.1 as f32;
    let half_top = h * (1.0 - SHEET_HALF);
    let full_top = SHEET_FULL_TOP;
    let segment = (half_top - full_top).max(1.0);

    let open = |lp: &mut AppLoop<Gallery>| {
        let trigger = rect_of(lp, modal_sheet_open_id()).expect("modal sheet trigger");
        click(lp, size, center(trigger));
    };
    // The settle spring caps its step at 0.1 s per frame, so a release needs several frames.
    let settle = |lp: &mut AppLoop<Gallery>| {
        for _ in 0..8 {
            frame(lp, size, None);
        }
    };

    goto(lp, size, Page::ModalSheet);
    open(lp);

    // ── At half: 45 % of the window less the 6 dp bottom margin, 5 dp in from each side.
    let sheet = rect_of(lp, modal_sheet_body_id());
    let half = sheet.is_some_and(|r| {
        (r.origin.y - half_top).abs() < 0.5
            && (r.origin.x - SHEET_MARGIN).abs() < 0.5
            && (r.origin.x + r.size.w - (w - SHEET_MARGIN)).abs() < 0.5
            && (r.origin.y + r.size.h - (h - SHEET_BOTTOM)).abs() < 0.5
    });
    println!(
        "selftest: modal sheet half {:?} (want top {half_top}, bottom {}) {}",
        sheet.map(|r| (r.origin.x, r.origin.y, r.size.w, r.size.h)),
        h - SHEET_BOTTOM,
        mark(half)
    );
    pass &= half;

    // ── Dragging past 40 % of the half→full segment settles on full: 90 dp from the top,
    // ── edge to edge, and the app hears which detent it landed on.
    let handle = rect_of(lp, modal_sheet_body_id().with("handle")).expect("drag handle");
    let from = center(handle);
    drag(lp, size, from, Point::new(from.x, from.y - segment * 0.6));
    settle(lp);
    let sheet = rect_of(lp, modal_sheet_body_id());
    let full = sheet.is_some_and(|r| {
        (r.origin.y - full_top).abs() < 0.5
            && r.origin.x.abs() < 0.5
            && (r.origin.x + r.size.w - w).abs() < 0.5
            && (r.origin.y + r.size.h - h).abs() < 0.5
    });
    let heard = lp.app_mut().modal_detent == Some(Msg::GALLERY_MODAL_SHEET_FULL);
    println!(
        "selftest: modal sheet full {:?} (want top {full_top}) heard={heard} {}",
        sheet.map(|r| (r.origin.x, r.origin.y, r.size.w, r.size.h)),
        mark(full && heard)
    );
    pass &= full && heard;

    // ── Dragging back down past 40 % of the same segment returns it to half.
    let handle = rect_of(lp, modal_sheet_body_id().with("handle")).expect("drag handle");
    let from = center(handle);
    drag(lp, size, from, Point::new(from.x, from.y + segment * 0.6));
    settle(lp);
    let back =
        rect_of(lp, modal_sheet_body_id()).is_some_and(|r| (r.origin.y - half_top).abs() < 0.5);
    let heard = lp.app_mut().modal_detent == Some(Msg::GALLERY_MODAL_SHEET_HALF);
    println!(
        "selftest: modal sheet back to half {back} heard={heard} {}",
        mark(back && heard)
    );
    pass &= back && heard;

    // ── A drag down past the snap threshold closes it, and the app counts the dismissal.
    let before = lp.app_mut().modal_dismissed;
    let handle = rect_of(lp, modal_sheet_body_id().with("handle")).expect("drag handle");
    let from = center(handle);
    drag(lp, size, from, Point::new(from.x, from.y + half_top * 0.6));
    let gone = rect_of(lp, modal_sheet_body_id()).is_none();
    let counted = lp.app_mut().modal_dismissed == before + 1;
    println!(
        "selftest: modal sheet swipe closed={gone} counted={counted} {}",
        mark(gone && counted)
    );
    pass &= gone && counted;

    // ── A press on the scrim above the sheet dismisses it too.
    open(lp);
    let before = lp.app_mut().modal_dismissed;
    click(lp, size, Point::new(w * 0.5, half_top * 0.5));
    let gone = rect_of(lp, modal_sheet_body_id()).is_none();
    let counted = lp.app_mut().modal_dismissed == before + 1;
    println!(
        "selftest: modal sheet scrim closed={gone} counted={counted} {}",
        mark(gone && counted)
    );
    pass &= gone && counted;

    // ── Escape closes it while it holds the keyboard.
    open(lp);
    let before = lp.app_mut().modal_dismissed;
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Escape,
    });
    // The change reaches the app after that frame's build, so the sheet leaves on the next.
    frame(lp, size, None);
    frame(lp, size, None);
    let gone = rect_of(lp, modal_sheet_body_id()).is_none();
    let counted = lp.app_mut().modal_dismissed == before + 1;
    println!(
        "selftest: modal sheet escape closed={gone} counted={counted} {}",
        mark(gone && counted)
    );
    pass &= gone && counted;

    // ── A press on the app's own button inside the sheet reaches the app, which closes it.
    open(lp);
    let done = rect_of(lp, modal_sheet_done_id()).expect("done button");
    click(lp, size, center(done));
    let closed = rect_of(lp, modal_sheet_body_id()).is_none() && !lp.app_mut().modal_open;
    println!(
        "selftest: modal sheet content press closed={closed} {}",
        mark(closed)
    );
    pass &= closed;

    // ── The MD3 sheet: 640 wide or the window, against the bottom edge, its handle 22 dp
    // ── below the top; a drag past half the height and a scrim press each close it.
    let trigger = rect_of(lp, md3_sheet_open_id()).expect("md3 sheet trigger");
    let sheet_id = md3_sheet_id().with("sheet");
    let open3 = |lp: &mut AppLoop<Gallery>| {
        let at = rect_of(lp, md3_sheet_open_id()).expect("md3 sheet trigger");
        click(lp, size, center(at));
        for _ in 0..4 {
            frame(lp, size, None);
        }
    };
    click(lp, size, center(trigger));
    for _ in 0..4 {
        frame(lp, size, None);
    }
    let want_w = w.min(640.0);
    let surface = rect_of(lp, sheet_id);
    let geometry = surface.is_some_and(|r| {
        (r.size.w - want_w).abs() < 0.5
            && (r.origin.x - (w - want_w) * 0.5).abs() < 0.5
            && (r.origin.y + r.size.h - h).abs() < 0.5
    });
    let handle = rect_of(lp, sheet_id.with("handle"));
    let handle_ok = match (handle, surface) {
        (Some(r), Some(s)) => {
            (r.size.w - 32.0).abs() < 0.5
                && (r.size.h - 4.0).abs() < 0.5
                && (r.origin.y - s.origin.y - 22.0).abs() < 0.5
                && (center(r).x - center(s).x).abs() < 0.5
        }
        _ => false,
    };
    println!(
        "selftest: md3 bottom sheet {:?} (want {want_w} wide) handle={handle_ok} {:?} {}",
        surface.map(|r| (r.origin.x, r.origin.y, r.size.w, r.size.h)),
        handle.map(|r| (r.origin.x, r.origin.y, r.size.w, r.size.h)),
        mark(geometry && handle_ok)
    );
    pass &= geometry && handle_ok;

    let travel = surface.map_or(0.0, |r| r.size.h) * 0.6;
    let before = lp.app_mut().md3_sheet_dismissed;
    match handle {
        Some(r) => {
            let from = center(r);
            drag(lp, size, from, Point::new(from.x, from.y + travel));
        }
        None => pass = false,
    }
    for _ in 0..4 {
        frame(lp, size, None);
    }
    let dragged = rect_of(lp, sheet_id).is_none();
    let counted = lp.app_mut().md3_sheet_dismissed == before + 1;
    println!(
        "selftest: md3 bottom sheet drag closed={dragged} counted={counted} {}",
        mark(dragged && counted)
    );
    pass &= dragged && counted;

    open3(lp);
    let top = rect_of(lp, sheet_id).map_or(0.0, |r| r.origin.y);
    let before = lp.app_mut().md3_sheet_dismissed;
    click(lp, size, Point::new(w * 0.5, top * 0.5));
    for _ in 0..4 {
        frame(lp, size, None);
    }
    let scrim = rect_of(lp, sheet_id).is_none() && lp.app_mut().md3_sheet_dismissed == before + 1;
    println!(
        "selftest: md3 bottom sheet scrim closed={scrim} {}",
        mark(scrim)
    );
    pass &= scrim;

    open3(lp);
    let before = lp.app_mut().md3_sheet_dismissed;
    let done = rect_of(lp, md3_sheet_done_id()).expect("md3 sheet done");
    click(lp, size, center(done));
    for _ in 0..4 {
        frame(lp, size, None);
    }
    let closed = !lp.app_mut().md3_sheet_open
        && rect_of(lp, sheet_id).is_none()
        && lp.app_mut().md3_sheet_dismissed == before;
    println!(
        "selftest: md3 bottom sheet done closed={closed} {}",
        mark(closed)
    );
    pass &= closed;

    // ── The MD3 side sheet: 256 wide against the trailing edge, full height, its divider
    // ── on the inner edge; the scrim closes it and its action closes it without counting.
    let side_id = md3_side_sheet_id();
    let side_surface = side_id.with("surface");
    let open_side = |lp: &mut AppLoop<Gallery>| {
        let at = rect_of(lp, md3_side_sheet_open_id()).expect("side sheet trigger");
        click(lp, size, center(at));
        for _ in 0..4 {
            frame(lp, size, None);
        }
    };
    open_side(lp);
    let panel = rect_of(lp, side_surface);
    let divider = rect_of(lp, side_surface.with("divider"));
    let headline = rect_of(lp, side_surface.with("headline"));
    let geometry = panel.is_some_and(|r| {
        (r.size.w - 256.0).abs() < 0.5
            && (r.origin.x - (w - 256.0)).abs() < 0.5
            && r.origin.y.abs() < 0.5
            && (r.origin.y + r.size.h - h).abs() < 0.5
    });
    let line_ok = match (divider, panel) {
        (Some(d), Some(p)) => {
            (d.size.w - 1.0).abs() < 0.5
                && (d.origin.x - p.origin.x).abs() < 0.5
                && (d.size.h - p.size.h).abs() < 0.5
        }
        _ => false,
    };
    println!(
        "selftest: md3 side sheet {:?} divider={line_ok} headline={} {}",
        panel.map(|r| (r.origin.x, r.origin.y, r.size.w, r.size.h)),
        headline.is_some(),
        mark(geometry && line_ok && headline.is_some())
    );
    pass &= geometry && line_ok && headline.is_some();

    let body = rect_of(lp, side_id.with("body"));
    match body {
        Some(r) => click(lp, size, center(r)),
        None => pass = false,
    }
    for _ in 0..4 {
        frame(lp, size, None);
    }
    let inside = lp.app_mut().md3_side_sheet_open && rect_of(lp, side_surface).is_some();
    println!(
        "selftest: md3 side sheet content keeps it open={inside} {}",
        mark(inside)
    );
    pass &= inside;

    let before = lp.app_mut().md3_side_sheet_dismissed;
    click(lp, size, Point::new(w * 0.25, h * 0.5));
    for _ in 0..4 {
        frame(lp, size, None);
    }
    let scrim =
        rect_of(lp, side_surface).is_none() && lp.app_mut().md3_side_sheet_dismissed == before + 1;
    println!(
        "selftest: md3 side sheet scrim closed={scrim} {}",
        mark(scrim)
    );
    pass &= scrim;

    open_side(lp);
    let before = lp.app_mut().md3_side_sheet_dismissed;
    let save = rect_of(lp, md3_side_sheet_save_id()).expect("side sheet save");
    click(lp, size, center(save));
    for _ in 0..4 {
        frame(lp, size, None);
    }
    let saved = !lp.app_mut().md3_side_sheet_open
        && rect_of(lp, side_surface).is_none()
        && lp.app_mut().md3_side_sheet_dismissed == before;
    println!(
        "selftest: md3 side sheet action closed={saved} {}",
        mark(saved)
    );
    pass &= saved;
    pass
}
