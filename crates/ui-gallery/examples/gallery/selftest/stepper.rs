// Self-test checks for the stepper page.

use ui_core::Event;

use crate::selftest::*;

pub(crate) fn stepper(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let button = ui_core::PointerButton::Primary;
    let hold = |lp: &mut AppLoop<Gallery>, at: Point, frames: u32| {
        lp.dispatch(Event::PointerMoved { pos: at });
        frame(lp, size, Some(at));
        lp.dispatch(Event::PointerPressed { pos: at, button });
        for _ in 0..frames {
            frame(lp, size, Some(at));
        }
    };
    let release = |lp: &mut AppLoop<Gallery>, at: Point| {
        lp.dispatch(Event::PointerReleased { pos: at, button });
        frame(lp, size, Some(at));
    };

    goto(lp, size, Page::Stepper);

    // ── Geometry: a 94×34 capsule split by a 0.5 dp divider into two equal sides.
    let pill = rect_of(lp, stepper_id());
    let divider = rect_of(lp, stepper_id().with("divider"));
    let minus = rect_of(lp, stepper_id().with("minus"));
    let plus = rect_of(lp, stepper_id().with("plus"));
    let half = (94.0 - 0.5) * 0.5;
    let geo = pill.is_some_and(|p| {
        let d = divider.expect("divider");
        let m = minus.expect("minus side");
        let s = plus.expect("plus side");
        (p.size.w - 94.0).abs() < 0.01
            && (p.size.h - 34.0).abs() < 0.01
            && (d.size.w - 0.5).abs() < 0.01
            && (d.size.h - 34.0).abs() < 0.01
            && (d.origin.x - (p.origin.x + half)).abs() < 0.01
            && (m.size.w - half).abs() < 0.01
            && (s.size.w - half).abs() < 0.01
            && (s.origin.x - (p.origin.x + half + 0.5)).abs() < 0.01
    });
    println!(
        "selftest: stepper geometry {:?} divider {:?} minus {:?} plus {:?} {}",
        pill.map(|r| (r.size.w, r.size.h)),
        divider.map(|r| (r.origin.x, r.size.w)),
        minus.map(|r| (r.origin.x, r.size.w)),
        plus.map(|r| (r.origin.x, r.size.w)),
        mark(geo)
    );
    pass &= geo;

    // ── A press on either side steps once and the app hears the value it reported.
    let before = lp.app_mut().stepper_value;
    let at = center(plus.expect("plus side"));
    click(lp, size, at);
    let up = lp.app_mut().stepper_value == before + 1.0;
    let at = center(minus.expect("minus side"));
    click(lp, size, at);
    let down = lp.app_mut().stepper_value == before;
    println!(
        "selftest: stepper plus→{up} minus→{down} value={} {}",
        lp.app_mut().stepper_value,
        mark(up && down)
    );
    pass &= up && down;

    // ── Holding repeats: one second of held time is six 0.1 s ticks after the 0.5 s
    // delay, so the second frame alone carries six steps, not one.
    let at = center(plus.expect("plus side"));
    hold(lp, at, 2);
    let burst = lp.app_mut().stepper_value;
    release(lp, at);
    // 0..=10 from `before`: the press edge and then the burst land exactly on the top.
    let reached = burst == 10.0 && before + 1.0 + 6.0 == 10.0;
    let presses = lp.app_mut().stepper_presses;
    println!(
        "selftest: stepper hold burst={burst} (want 10) presses={presses} {}",
        mark(reached)
    );
    pass &= reached;

    // ── At the top the plus side is disabled: a press there reports nothing, and the
    // count the app keeps does not move.
    let before_presses = lp.app_mut().stepper_presses;
    click(lp, size, center(plus.expect("plus side")));
    let still =
        lp.app_mut().stepper_presses == before_presses && lp.app_mut().stepper_value == 10.0;
    println!(
        "selftest: stepper top inert={still} presses={} {}",
        lp.app_mut().stepper_presses,
        mark(still)
    );
    pass &= still;

    // ── The wrapping stepper cycles past its end instead of stopping.
    let wrap = rect_of(lp, stepper_wrap_id()).expect("wrapping stepper");
    let at = center(rect_of(lp, stepper_wrap_id().with("plus")).expect("wrap plus"));
    let start = lp.app_mut().stepper_wrap;
    click(lp, size, at);
    let at_top = lp.app_mut().stepper_wrap;
    click(lp, size, at);
    let cycled = lp.app_mut().stepper_wrap;
    // The reference lands one step past the minimum, not on it (`min + (next - max) % span`).
    let wraps_ok = at_top == start + 1.0 && cycled == 1.0;
    println!(
        "selftest: stepper wrap {start}→{at_top}→{cycled} (want 5 then 1) {}",
        mark(wraps_ok)
    );
    pass &= wraps_ok;
    let _ = wrap;

    // ── A stepper built disabled reports nothing at all.
    let before_presses = lp.app_mut().stepper_presses;
    let at = center(rect_of(lp, stepper_disabled_id().with("plus")).expect("disabled plus"));
    click(lp, size, at);
    let inert = lp.app_mut().stepper_presses == before_presses;
    println!("selftest: stepper disabled inert={inert} {}", mark(inert));
    pass &= inert;
    pass
}
