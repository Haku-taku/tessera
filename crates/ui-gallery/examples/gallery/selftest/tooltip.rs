// Self-test checks for the tooltip page.

use crate::app::TIP_HOVER_DELAY;
use crate::selftest::*;

/// An MD3 tooltip waits out the hover delay, opens above its target, and goes away with it.
pub(crate) fn tooltip(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    goto(lp, size, Page::ToolTip);
    let k = 0;
    let target = tip_target_id(MD3_TIPS, k);
    let panel = tip_panel_id(MD3_TIPS, k);
    let Some(rect) = rect_of(lp, target) else {
        println!("selftest: tooltip target missing");
        return false;
    };
    let at = center(rect);
    let frames = (TIP_HOVER_DELAY / 0.016).ceil() as u32;
    // Two frames of hover is not the delay: nothing is declared yet.
    step(lp, size, at, 2);
    let early = rect_of(lp, panel).is_some();
    // Past the delay the panel exists; the exit fade keeps it declared while it runs.
    step(lp, size, at, frames + 4);
    let shown = rect_of(lp, panel);
    let text = painted_text(lp, panel.with("text"));
    let (above, centred, capped) = match shown {
        Some(p) => (
            p.origin.y + p.size.h <= rect.origin.y + 0.5,
            (p.origin.x + p.size.w * 0.5 - (rect.origin.x + rect.size.w * 0.5)).abs() <= 1.0,
            p.size.w <= 200.5,
        ),
        None => (false, false, false),
    };
    step(lp, size, Point::new(8.0, 8.0), 30);
    let gone = rect_of(lp, panel).is_none();
    let labelled = text.as_deref().is_some_and(|t| !t.is_empty());
    println!(
        "selftest: tooltip md3 early={early} shown={} above={above} centred={centred} \
         capped={capped} labelled={labelled} gone={gone}",
        shown.is_some()
    );
    let ok = !early && shown.is_some() && above && centred && capped && labelled && gone;
    if !ok {
        println!("selftest: tooltip text {text:?}");
    }
    ok && rich(lp, size)
}

/// The rich sample: the app opens it from a trigger, its surface caps at 320 dp with the
/// action at the trailing inset, and the action closes it.
fn rich(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let Some(trigger) = rect_of(lp, rich_tip_trigger_id()) else {
        println!("selftest: rich tooltip trigger missing");
        return false;
    };
    click(lp, size, center(trigger));
    frame(lp, size, None);
    let panel = rich_tip_panel_id();
    let shown = rect_of(lp, panel);
    let action = rect_of(lp, rich_tip_action_id());
    let subhead = painted_text(lp, panel.with("subhead-text"));
    let body = painted_text(lp, panel.with("text"));
    let (above, capped, trailing) = match (shown, action) {
        (Some(p), Some(a)) => (
            p.origin.y + p.size.h <= trigger.origin.y + 0.5,
            p.size.w <= 320.5,
            ((p.origin.x + p.size.w - 16.0) - (a.origin.x + a.size.w)).abs() <= 1.0,
        ),
        _ => (false, false, false),
    };
    let labelled = subhead.as_deref().is_some_and(|t| !t.is_empty())
        && body.as_deref().is_some_and(|t| !t.is_empty());
    let closed = match action {
        Some(a) => {
            click(lp, size, center(a));
            // The exit fade keeps the panel mounted for a few frames after the app drops it.
            step(lp, size, Point::new(8.0, 8.0), 30);
            !lp.app_mut().rich_tip_open && rect_of(lp, panel).is_none()
        }
        None => false,
    };
    println!(
        "selftest: tooltip rich shown={} above={above} capped={capped} trailing={trailing} \
         labelled={labelled} action-closes={closed}",
        shown.is_some()
    );
    shown.is_some() && above && capped && trailing && labelled && closed
}
