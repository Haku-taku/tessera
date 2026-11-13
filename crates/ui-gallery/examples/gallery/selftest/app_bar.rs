// Self-test checks for the app bar page.

use crate::selftest::*;

pub(crate) fn app_bar(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The App bar page: six bars at their exported heights, an action click names its slot,
    // and the collapse flag shrinks only the flexible variants.
    goto(lp, size, Page::AppBar);
    let mut heights = Vec::new();
    for (k, sample) in APP_BAR_SAMPLES.iter().enumerate() {
        let want = sample.variant.expanded_height(sample.subtitle.is_some());
        let got = rect_of(lp, app_bar_bar_id(k))
            .map(|r| r.size.h)
            .unwrap_or(-1.0);
        heights.push((got, want));
    }
    let heights_ok = heights.iter().all(|(got, want)| (got - want).abs() < 0.5);
    // Every slot centres its mark on both axes. `Text`'s line box would hang it 4.8 dp high,
    // which is what a fixed inset used to compensate for.
    let mut centred = true;
    let mut marks = 0;
    for (k, sample) in APP_BAR_SAMPLES.iter().enumerate() {
        let slots = std::iter::once(app_bar_lead_id(k))
            .chain((0..sample.actions.len()).map(|j| app_bar_action_id(k, j)));
        for slot in slots {
            let (Some(s), Some(icon)) = (rect_of(lp, slot), rect_of(lp, slot.with("icon"))) else {
                continue;
            };
            marks += 1;
            let dx = (s.origin.x + s.size.w * 0.5) - (icon.origin.x + icon.size.w * 0.5);
            let dy = (s.origin.y + s.size.h * 0.5) - (icon.origin.y + icon.size.h * 0.5);
            if dx.abs() > 0.5 || dy.abs() > 0.5 {
                centred = false;
                println!("selftest: app bar slot {slot:?} mark off centre by {dx:.2},{dy:.2}");
            }
        }
    }
    let mut named = false;
    if let Some(action) = rect_of(lp, app_bar_action_id(0, 0)) {
        click(lp, size, center(action));
        named = lp.app_mut().app_bar_fired == Some(APP_BAR_SAMPLES[0].actions[0].0);
    }
    if let Some(toggle) = rect_of(lp, app_bar_collapse_id()) {
        click(lp, size, center(toggle));
    }
    for _ in 0..20 {
        frame(lp, size, None);
    }
    let collapsed = lp.app_mut().app_bar_collapsed;
    let mut shrunk = 0;
    let mut kept = 0;
    for (k, sample) in APP_BAR_SAMPLES.iter().enumerate() {
        let h = rect_of(lp, app_bar_bar_id(k))
            .map(|r| r.size.h)
            .unwrap_or(-1.0);
        if sample.variant.flexible() {
            shrunk += usize::from((h - 64.0).abs() < 0.5);
        } else {
            kept += usize::from((h - heights[k].1).abs() < 0.5);
        }
    }
    let flexible_count = APP_BAR_SAMPLES
        .iter()
        .filter(|s| s.variant.flexible())
        .count();
    let app_bar_ok = heights_ok && centred && named && collapsed && shrunk == flexible_count;
    println!(
        "selftest: md3 app bar heights={heights:?} at-token={heights_ok} marks={marks} \
         centred={centred} slot={named} collapsed={collapsed} shrunk={shrunk}/{flexible_count} \
         kept={kept} {}",
        if app_bar_ok { "✓" } else { "✗" }
    );
    if !app_bar_ok {
        pass = false;
    }
    pass
}
