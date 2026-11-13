// Self-test checks for the tabs page.

use crate::selftest::*;

pub(crate) fn tabs(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Tabs: equal shares of the bar, the two heights, and a click that selects.
    goto(lp, size, Page::Tabs);
    let bar = rect_of(lp, tab_bar_id(0)).expect("text tab bar");
    let share = bar.size.w / TAB_COUNT[0] as f32;
    let shares: Vec<f32> = (0..TAB_COUNT[0])
        .map(|k| {
            rect_of(lp, tab_item_id(0, k))
                .map(|r| r.size.w)
                .unwrap_or(-1.0)
        })
        .collect();
    let even = shares.iter().all(|w| (w - share).abs() < 0.5);
    println!(
        "selftest: md3 tab shares {shares:?} (want {share:.1}) {}",
        if even { "✓" } else { "✗" }
    );
    let band = ui_widgets_md3::spec::secondary_navigation_tab_spec(
        ui_widgets_md3::spec::SecondaryNavigationTabFlags::NONE,
    );
    let want_h = [
        ui_widgets_md3::TAB_HEIGHT + band.active_indicator.height + ui_widgets_md3::TAB_DIVIDER,
        ui_widgets_md3::TAB_ICON_HEIGHT
            + band.active_indicator.height
            + ui_widgets_md3::TAB_DIVIDER,
    ];
    let got_h = [
        rect_of(lp, tab_bar_id(0)).map(|r| r.size.h).unwrap_or(-1.0),
        rect_of(lp, tab_bar_id(1)).map(|r| r.size.h).unwrap_or(-1.0),
    ];
    let tall_ok = got_h
        .iter()
        .zip(&want_h)
        .all(|(got, want)| (got - want).abs() < 0.5);
    println!(
        "selftest: md3 tab bar heights {got_h:?} (want {want_h:?}) {}",
        if tall_ok { "✓" } else { "✗" }
    );
    let last = TAB_COUNT[0] - 1;
    click(
        lp,
        size,
        center(rect_of(lp, tab_item_id(0, last)).expect("last tab")),
    );
    let picked = lp.app_mut().tab_sel[0] == last;
    println!(
        "selftest: md3 tab click selected {} {}",
        lp.app_mut().tab_sel[0],
        if picked { "✓" } else { "✗" }
    );
    if !(even && tall_ok && picked) {
        pass = false;
    }
    // One underline serves the bar; after the click it must sit under the tab just picked.
    let underline = rect_of(lp, tab_bar_id(0).with("indicator"));
    let want_x = bar.origin.x + share * last as f32;
    let slid = underline
        .map(|u| (u.origin.x - want_x).abs() < 0.5 && (u.size.w - share).abs() < 0.5)
        .unwrap_or(false);
    println!(
        "selftest: md3 tab underline at {:?} (want x {want_x:.0} w {share:.0}) {}",
        underline.map(|u| (u.origin.x, u.size.w)),
        if slid { "✓" } else { "✗" }
    );
    if !slid {
        pass = false;
    }
    // The Fluent strip: a header owns its own overlays, its close glyph centres in the slot,
    // and the close button lights only while the pointer is on it.
    let slots = lp.app_mut().fluent_tabs.clone();
    let closable = slots.iter().copied().find(|s| *s != 0);
    if let (Some(&first), Some(close_slot)) = (slots.first(), closable) {
        let tab = rect_of(lp, fluent_tab_id(first));
        let sep = rect_of(lp, fluent_tab_id(first).with("sep"));
        // A static header leaves the strip as the containing block, so the separator lands on
        // the strip's far edge instead of this header's.
        let owned = matches!((tab, sep), (Some(t), Some(s))
            if (s.origin.x - (t.origin.x + t.size.w - 1.0)).abs() < 1.0
                && (s.origin.y - (t.origin.y + (t.size.h - ui_widgets_fluent::SEPARATOR_HEIGHT) * 0.5)).abs() < 1.0);
        println!(
            "selftest: fluent separator at {:?} of header {:?} {}",
            sep.map(|s| (s.origin.x, s.origin.y)),
            tab.map(|t| (t.origin.x, t.size.w)),
            if owned { "✓" } else { "✗" }
        );
        if !owned {
            pass = false;
        }
        let close_id = fluent_tab_id(close_slot).with("close");
        let boxed = rect_of(lp, close_id);
        let glyph = rect_of(lp, fluent_tab_id(close_slot).with("close-glyph"));
        // A Column would park the glyph at the slot's top; both axes must centre.
        let centred = matches!((boxed, glyph), (Some(b), Some(g))
            if (center(b).y - center(g).y).abs() < 0.6 && (center(b).x - center(g).x).abs() < 0.6);
        println!(
            "selftest: fluent close glyph centred {centred} (box {:?}, glyph {:?}) {}",
            boxed.map(center),
            glyph.map(center),
            if centred { "✓" } else { "✗" }
        );
        if !centred {
            pass = false;
        }
        let rest = shape_fill(lp, close_id, 0).map(|c| c.a).unwrap_or(1.0);
        if let Some(b) = boxed {
            let at = center(b);
            frame(lp, size, Some(at));
            frame(lp, size, Some(at));
            let lit = shape_fill(lp, close_id, 0).map(|c| c.a).unwrap_or(0.0);
            // `TabViewItemHeaderCloseButtonBackgroundPointerOver` is a visible wash; the
            // header's own hover must not be what lights it.
            let glows = rest < 0.01 && lit > 0.02;
            println!(
                "selftest: fluent close button hover fill {rest:.3}->{lit:.3} {}",
                if glows { "✓" } else { "✗" }
            );
            if !glows {
                pass = false;
            }
        }
        let away = rect_of(lp, fluent_tabs_id())
            .map(center)
            .unwrap_or(Point::new(0.0, 0.0));
        frame(lp, size, Some(Point::new(away.x, away.y + 60.0)));
    }
    // Dragging a header across its neighbours reorders the strip: the crossed headers slide
    // over by the dragged one's width, and the release names the slot it lands in.
    let order = lp.app_mut().fluent_tabs.clone();
    if order.len() >= 3 {
        let (from, to) = (0usize, 2usize);
        let source = rect_of(lp, fluent_tab_id(order[from])).expect("dragged header");
        let slot = rect_of(lp, fluent_tab_id(order[to])).expect("landing header");
        let press = center(source);
        let drop_at = Point::new(center(slot).x, press.y);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerMoved { pos: press });
        frame(lp, size, Some(press));
        lp.dispatch(ui_core::Event::PointerPressed { pos: press, button });
        frame(lp, size, Some(press));
        for pos in [Point::new(press.x + 8.0, press.y), drop_at] {
            lp.dispatch(ui_core::Event::PointerMoved { pos });
            step(lp, size, pos, 2);
        }
        let slid = rect_of(lp, fluent_tab_id(order[from + 1]))
            .map(|r| (r.origin.x - source.origin.x).abs() < 1.0)
            .unwrap_or(false);
        println!(
            "selftest: fluent drag slides the crossed header {slid} {}",
            if slid { "✓" } else { "✗" }
        );
        if !slid {
            pass = false;
        }
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: drop_at,
            button,
        });
        frame(lp, size, Some(drop_at));
        let after = lp.app_mut().fluent_tabs.clone();
        let mut want = order.clone();
        let moved = want.remove(from);
        want.insert(to, moved);
        let reordered = after == want;
        println!(
            "selftest: fluent drag reorders {after:?} (want {want:?}) {reordered} {}",
            if reordered { "✓" } else { "✗" }
        );
        if !reordered {
            pass = false;
        }
    }
    // The strip's arrows sit beside the list rather than inside it, so their request has to
    // name it: one press steps 50 dp, and holding repeats on the template's cadence.
    let list = fluent_tabs_id().with("tabs");
    let offset = |lp: &AppLoop<Gallery>| lp.scroll_offset(list).map(|g| g.x).unwrap_or(0.0);
    if let Some(right) = rect_of(lp, fluent_tabs_id().with("scroll-right")) {
        let at = center(right);
        let before = offset(lp);
        click(lp, size, at);
        let stepped = offset(lp) - before;
        let one = (stepped - ui_widgets_fluent::SCROLL_STEP).abs() <= 2.0;
        println!(
            "selftest: fluent strip arrow steps {stepped:.1} (want 50) {one} {}",
            if one { "✓" } else { "✗" }
        );
        if !one {
            pass = false;
        }
        let before = offset(lp);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        frame(lp, size, Some(at));
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        step(lp, size, at, 30);
        let held = offset(lp) - before;
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        frame(lp, size, Some(at));
        // 30 frames are 480 ms: the press, then repeats at 50, 150, 250, 350 and 450 ms.
        let repeats = held > ui_widgets_fluent::SCROLL_STEP * 2.0;
        println!(
            "selftest: fluent strip arrow repeats {held:.1} over 480 ms {repeats} {}",
            if repeats { "✓" } else { "✗" }
        );
        if !repeats {
            pass = false;
        }
    }
    pass
}
