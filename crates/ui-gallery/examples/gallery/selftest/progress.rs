// Self-test checks for the progress indicator page.

use ui_widgets_glass::PROGRESS_MIN_W;

use crate::selftest::*;

pub(crate) fn progress(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::Progress);
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let id = progress_id(k);
        let Some(bar) = rect_of(lp, id) else {
            println!("selftest: progress bar {k} is missing");
            pass = false;
            continue;
        };
        // The exported 4dp thickness is the bar's whole height.
        let h_ok = (bar.size.h - 4.0).abs() < 0.5;
        // The active bar is the second shape; at zero it is not painted at all.
        let w_ok = match shape_rect(lp, id, 1) {
            None => *value == 0.0,
            Some(active) => (active.size.w - bar.size.w * value).abs() < 0.5,
        };
        println!(
            "selftest: progress {value:.2} height {:.1} active {} {}",
            bar.size.h,
            if w_ok { "matches" } else { "does not match" },
            if h_ok && w_ok { "✓" } else { "✗" }
        );
        if !(h_ok && w_ok) {
            pass = false;
        }
    }

    // ── Indeterminate: two bars sweep, every segment stays inside the track, and the
    // pattern is somewhere else one second later.
    let sweep = progress_id(PROGRESS_VALUES.len());
    let read = |lp: &AppLoop<Gallery>| {
        let mut segs = Vec::new();
        for k in 0..8 {
            match shape_rect(lp, sweep, k) {
                Some(r) => segs.push(r),
                None => break,
            }
        }
        segs
    };
    let bar = rect_of(lp, sweep).expect("indeterminate bar");
    let before = read(lp);
    frame(lp, size, None);
    let after = read(lp);
    let inside = |segs: &[Rect]| {
        segs.iter().all(|r| {
            r.origin.x >= bar.origin.x - 0.5
                && r.origin.x + r.size.w <= bar.origin.x + bar.size.w + 0.5
                && (r.size.h - 4.0).abs() < 0.5
        })
    };
    let sweeping = before.len() >= 2 && inside(&before) && inside(&after) && before != after;
    println!(
        "selftest: progress sweep segments {}→{} {}",
        before.len(),
        after.len(),
        if sweeping { "✓" } else { "✗" }
    );
    pass &= sweeping;

    // ── The expressive form: 10 dp tall, a wave over 60% of the track, a stop dot at the tail.
    let wavy_id = progress_wavy_id();
    let wavy = rect_of(lp, wavy_id).expect("wavy bar");
    let lines = shape_lines(lp, wavy_id);
    let (mut low, mut high, mut left, mut right) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for line in &lines {
        low = low.min(line.from.y).min(line.to.y);
        high = high.max(line.from.y).max(line.to.y);
        left = left.min(line.from.x).min(line.to.x);
        right = right.max(line.from.x).max(line.to.x);
    }
    let inside = lines.iter().all(|line| {
        line.from.y >= wavy.origin.y - 0.5 && line.to.y <= wavy.origin.y + wavy.size.h + 0.5
    });
    let dot = shape_rects(lp, wavy_id).into_iter().find(|rect| {
        (rect.size.w - 4.0).abs() < 0.5
            && (rect.size.h - 4.0).abs() < 0.5
            && rect.origin.x > wavy.origin.x + wavy.size.w - 12.0
    });
    let value_end = wavy.origin.x + wavy.size.w * 0.6;
    let first = lines.clone();
    step(lp, size, center(wavy), 2);
    let later = shape_lines(lp, wavy_id);
    let travels = !first.is_empty() && first != later;
    let wavy_ok = (wavy.size.h - 10.0).abs() < 0.5
        && lines.len() > 20
        && high - low > 4.0
        && inside
        && (left - wavy.origin.x).abs() < 4.0
        && (right - value_end).abs() < 6.0
        && dot.is_some()
        && travels;
    println!(
        "selftest: progress wavy {:.0} dp, {} samples over {:.0}..{:.0}, dot {}, travels {} {}",
        wavy.size.h,
        lines.len(),
        left - wavy.origin.x,
        right - wavy.origin.x,
        dot.is_some(),
        travels,
        if wavy_ok { "✓" } else { "✗" }
    );
    pass &= wavy_ok;

    // ── Its sweeping twin: two waves that move between frames and stay in the box.
    let wavy_sweep = progress_wavy_sweep_id();
    let box_ = rect_of(lp, wavy_sweep).expect("wavy sweep");
    let mut previous = shape_lines(lp, wavy_sweep);
    let mut samples = previous.len();
    let mut moved = false;
    for _ in 0..30 {
        step(lp, size, center(box_), 1);
        let current = shape_lines(lp, wavy_sweep);
        samples = samples.max(current.len());
        moved |= current != previous;
        previous = current;
    }
    let stays = previous.iter().all(|line| {
        line.from.y >= box_.origin.y - 0.5 && line.to.y <= box_.origin.y + box_.size.h + 0.5
    });
    let sweeping = (box_.size.h - 10.0).abs() < 0.5 && samples > 10 && stays && moved;
    println!(
        "selftest: progress wavy sweep {:.0} dp, {} samples, moved {}, in box {} {}",
        box_.size.h,
        samples,
        moved,
        stays,
        if sweeping { "✓" } else { "✗" }
    );
    pass &= sweeping;

    // ── The ring: a 40 dp circle whose arc grows to 270° and travels, never a full ring.
    let ring_id = progress_ring_id();
    let ring = rect_of(lp, ring_id).expect("indeterminate ring");
    let sweep_of = |s: &ui_core::draw::SectorPrim| s.sector.sweep();
    let mut widest: f32 = 0.0;
    let mut starts = std::collections::BTreeSet::new();
    for _ in 0..90 {
        step(lp, size, center(ring), 1);
        if let Some(s) = shape_sector(lp, ring_id, 0) {
            widest = widest.max(sweep_of(&s));
            starts.insert((s.sector.trim.0 * 720.0).round() as i32);
        }
    }
    let ring_ok = (ring.size.w - 40.0).abs() < 0.5
        && (ring.size.h - 40.0).abs() < 0.5
        && (0.74..0.76).contains(&widest)
        && starts.len() > 20;
    println!(
        "selftest: progress ring {:.0} dp widest {:.2} turns over {} starts {}",
        ring.size.w,
        widest,
        starts.len(),
        if ring_ok { "✓" } else { "✗" }
    );
    pass &= ring_ok;

    // ── Rings with a value: the arc is the value, and each end owns one span only.
    let mut rings_ok = true;
    for (k, value) in PROGRESS_VALUES.iter().enumerate() {
        let id = progress_ring_value_id(k);
        let size_ok = rect_of(lp, id)
            .is_some_and(|r| (r.size.w - 40.0).abs() < 0.5 && (r.size.h - 40.0).abs() < 0.5);
        let track = shape_sector(lp, id, 0).map(|s| s.sector.sweep());
        let arc = shape_sector(lp, id, 1).map(|s| s.sector.sweep());
        // Both ends leave one span: the track at 0%, the arc at 100%.
        let spans_ok = if *value <= 0.0 || *value >= 1.0 {
            track.is_some_and(|t| (t - 1.0).abs() < 0.01) && arc.is_none()
        } else {
            track.is_some() && arc.is_some_and(|a| (a - value).abs() < 0.01)
        };
        println!(
            "selftest: progress ring {value:.2} spans {:?} {:?} {}",
            track,
            arc,
            if size_ok && spans_ok { "✓" } else { "✗" }
        );
        rings_ok &= size_ok && spans_ok;
    }
    pass &= rings_ok;

    // ── Glass: the ring keeps the reference's 20 dp diameter (and 40 when asked), the bar
    // is 4 dp tall and no narrower than 200 dp, and its fill is the value's fraction.
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    let ring = rect_of(lp, progress_glass_ring_id(0)).expect("glass ring");
    let big = rect_of(lp, progress_glass_big_id(0)).expect("glass big ring");
    let ring_ok = (ring.size.w - 20.0).abs() < 0.5
        && (ring.size.h - 20.0).abs() < 0.5
        && (big.size.w - 40.0).abs() < 0.5
        && (big.size.h - 40.0).abs() < 0.5;
    let track = rect_of(lp, progress_glass_bar_id(1)).expect("glass bar");
    let fill = rect_of(lp, progress_glass_bar_id(1).with("value")).expect("glass bar fill");
    let full = rect_of(lp, progress_glass_bar_id(3).with("value")).expect("glass full bar");
    let zero_paints_nothing = rect_of(lp, progress_glass_bar_id(0).with("value")).is_none();
    let bar_ok = (track.size.h - 4.0).abs() < 0.5
        && track.size.w >= PROGRESS_MIN_W - 0.5
        && zero_paints_nothing
        && (fill.size.w - track.size.w * 0.25).abs() < 1.0
        && (fill.origin.x - track.origin.x).abs() < 0.5
        && (full.size.w - track.size.w).abs() < 1.0;
    println!(
        "selftest: glass progress rings {:.0}/{:.0} dp {}, bar {:.0}×{:.1} fill {:.1}/{:.1} {}",
        ring.size.w,
        big.size.w,
        mark(ring_ok),
        track.size.w,
        track.size.h,
        fill.size.w,
        track.size.w * 0.25,
        mark(bar_ok)
    );
    pass &= ring_ok && bar_ok;
    pass
}
