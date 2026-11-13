// Self-test checks for the date picker page.

use crate::selftest::*;

pub(crate) fn date_picker(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::DatePicker);
    let picker = date_picker_id(0);

    // Date picker: the arrow moves the month the app owns, and a day reports the date its
    // cell stands for — the two facts the calendar cannot supply itself.
    let month_before = lp.app_mut().dp_shown[0];
    click(
        lp,
        size,
        center(rect_of(lp, picker.with("next")).expect("next arrow")),
    );
    let stepped = lp.app_mut().dp_shown[0];
    let wanted = ui_widgets_md3::Date::new(stepped.year, stepped.month, 14).expect("a real date");
    click(
        lp,
        size,
        center(rect_of(lp, ui_widgets_md3::day_cell_id(picker, wanted)).expect("day 14")),
    );
    let picked_date = lp.app_mut().dp_value[0];
    let next_month = if month_before.month == 12 {
        (month_before.year + 1, 1)
    } else {
        (month_before.year, month_before.month + 1)
    };
    let stepped_ok = (stepped.year, stepped.month) == next_month;
    println!(
        "selftest: md3 date picker month {}->{} stepped={stepped_ok} picked={picked_date:?} {}",
        month_before.month,
        stepped.month,
        if stepped_ok && picked_date == Some(wanted) {
            "✓"
        } else {
            "✗"
        }
    );
    if !(stepped_ok && picked_date == Some(wanted)) {
        pass = false;
    }

    // The grid answers the arrow keys: focus walks one day right and one week down, and
    // Enter picks the focused day — the same change a click reports.
    let cell_of = |d: u8| {
        ui_widgets_md3::day_cell_id(
            picker,
            ui_widgets_md3::Date::new(stepped.year, stepped.month, d).expect("a real date"),
        )
    };
    let (day14_cell, day15_cell, day22_cell) = (cell_of(14), cell_of(15), cell_of(22));
    tap(
        lp,
        size,
        center(rect_of(lp, day14_cell).expect("day 14")),
        2,
    );
    let landed_ok = lp.focused() == Some(day14_cell);
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Right,
    });
    frame(lp, size, None);
    let right_ok = lp.focused() == Some(day15_cell);
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Down,
    });
    frame(lp, size, None);
    let down_ok = lp.focused() == Some(day22_cell);
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Enter,
    });
    frame(lp, size, None);
    let entered = lp.app_mut().dp_value[0]
        == Some(ui_widgets_md3::Date::new(stepped.year, stepped.month, 22).expect("a real date"));
    println!(
        "selftest: md3 date picker arrows land={landed_ok} right={right_ok} down={down_ok} \
         enter={entered} {}",
        if landed_ok && right_ok && down_ok && entered {
            "✓"
        } else {
            "✗"
        }
    );
    pass &= landed_ok && right_ok && down_ok && entered;

    // Every element's state layer fills the same background its ripple is clipped to: the
    // 40dp disc inside a day's 48dp cell, and the whole stadium of a month cell or a button.
    let day15 = ui_widgets_md3::day_cell_id(
        picker,
        ui_widgets_md3::Date::new(stepped.year, stepped.month, 15).unwrap(),
    );
    let day_rect = rect_of(lp, day15).expect("day 15");
    step(lp, size, center(day_rect), 24);
    let day_layer = shape_rect(lp, day15, 0).expect("day state layer");
    let want_disc = Rect::new(day_rect.origin.x + 4.0, day_rect.origin.y + 4.0, 40.0, 40.0);
    let day_ok = close_rect(day_layer, want_disc);

    // The chevron's em box must sit concentric with the 40dp button, not at its main-axis
    // start, which is where a single-child row leaves it.
    let arrow = picker.with("next");
    let arrow_rect = rect_of(lp, arrow).expect("next arrow");
    step(lp, size, center(arrow_rect), 24);
    let arrow_layer = shape_rect(lp, arrow, 0).expect("arrow state layer");
    let arrow_ok = close_rect(arrow_layer, arrow_rect);
    let mark = rect_of(lp, arrow.with("icon")).expect("chevron");
    let mark_ok = (center(mark).x - center(arrow_rect).x).abs() <= 0.5
        && (center(mark).y - center(arrow_rect).y).abs() <= 0.5;

    lp.app_mut().dp_view[0] = ui_widgets_md3::DatePickerView::Months;
    frame(lp, size, None);
    let month_cell = ui_widgets_md3::month_cell_id(picker, 3);
    let month_rect = rect_of(lp, month_cell).expect("month cell");
    step(lp, size, center(month_rect), 24);
    let month_layer = shape_rect(lp, month_cell, 0).expect("month state layer");
    let month_ok = close_rect(month_layer, month_rect);

    println!(
        "selftest: md3 date picker layers day={day_ok} month={month_ok} arrow={arrow_ok} \
         chevron-centred={mark_ok} {}",
        if day_ok && month_ok && arrow_ok && mark_ok {
            "✓"
        } else {
            "✗"
        }
    );
    if !(day_ok && month_ok && arrow_ok && mark_ok) {
        pass = false;
    }

    // Paging: each menu button toggles its list, the arrows stand down while the month grid
    // is up, and they page a whole twelve-year block when the year grid is up.
    lp.app_mut().dp_view[0] = ui_widgets_md3::DatePickerView::Days;
    frame(lp, size, None);
    let month_btn = picker.with("month");
    let year_btn = picker.with("year");
    click(
        lp,
        size,
        center(rect_of(lp, month_btn).expect("month button")),
    );
    let month_open = lp.app_mut().dp_view[0] == ui_widgets_md3::DatePickerView::Months;
    let frozen = lp.app_mut().dp_shown[0];
    let arrow_mark = picker.with("next").with("icon");
    let dimmed = glyph_ink(lp, arrow_mark).expect("arrow mark");
    click(
        lp,
        size,
        center(rect_of(lp, picker.with("next")).expect("next arrow")),
    );
    let stood_down = lp.app_mut().dp_shown[0] == frozen;
    click(
        lp,
        size,
        center(rect_of(lp, month_btn).expect("month button")),
    );
    let month_closed = lp.app_mut().dp_view[0] == ui_widgets_md3::DatePickerView::Days;
    let lit = glyph_ink(lp, arrow_mark).expect("arrow mark");
    let arrow_dimmed = dimmed.a < lit.a * 0.5;
    click(
        lp,
        size,
        center(rect_of(lp, year_btn).expect("year button")),
    );
    let year_open = lp.app_mut().dp_view[0] == ui_widgets_md3::DatePickerView::Years;
    let year_before = lp.app_mut().dp_shown[0].year;
    click(
        lp,
        size,
        center(rect_of(lp, picker.with("next")).expect("next arrow")),
    );
    let year_paged = lp.app_mut().dp_shown[0].year == year_before + 12
        && lp.app_mut().dp_view[0] == ui_widgets_md3::DatePickerView::Years;
    click(
        lp,
        size,
        center(rect_of(lp, year_btn).expect("year button")),
    );
    let year_closed = lp.app_mut().dp_view[0] == ui_widgets_md3::DatePickerView::Days;
    let toggle_ok = month_open
        && month_closed
        && year_open
        && year_closed
        && stood_down
        && year_paged
        && arrow_dimmed;
    println!(
        "selftest: md3 date picker toggle month={month_open}/{month_closed} \
         year={year_open}/{year_closed} arrows-stand-down={stood_down} year-page={year_paged} \
         arrow-dimmed={arrow_dimmed} {}",
        if toggle_ok { "✓" } else { "✗" }
    );
    if !toggle_ok {
        pass = false;
    }

    // A ranged calendar keeps two independent channels: the role says which month a day is
    // in, the alpha says whether it can be picked, so no two categories share a look.
    lp.app_mut().dp_shown[0] = date_picker_today();
    lp.app_mut().dp_view[0] = ui_widgets_md3::DatePickerView::Days;
    lp.app_mut().dp_value[0] = None;
    frame(lp, size, None);
    let ink = |lp: &AppLoop<Gallery>, k: usize, d: ui_widgets_md3::Date| {
        let cell = ui_widgets_md3::day_cell_id(date_picker_id(k), d);
        glyph_ink(lp, cell.with("label")).expect("day label ink")
    };
    let date = |y, m, d| ui_widgets_md3::Date::new(y, m, d).unwrap();
    let this_month = ink(lp, 0, date(2026, 10, 15));
    let other_month = ink(lp, 0, date(2026, 9, 30));
    let this_unpicked = ink(lp, 1, date(2026, 10, 5));
    let other_unpicked = ink(lp, 1, date(2026, 9, 27));
    let same_role = |a: ui_core::geom::Color, b: ui_core::geom::Color| {
        (a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01
    };
    let same_strength = |a: ui_core::geom::Color, b: ui_core::geom::Color| (a.a - b.a).abs() < 0.02;
    let roles_ok = !same_role(this_month, other_month)
        && same_role(this_month, this_unpicked)
        && same_role(other_month, other_unpicked);
    let alphas_ok = same_strength(this_month, other_month)
        && this_unpicked.a < this_month.a * 0.5
        && same_strength(this_unpicked, other_unpicked);
    println!(
        "selftest: md3 date picker range roles={roles_ok} alphas={alphas_ok} \
         inks {:?} {:?} {:?} {:?} {}",
        this_month.a,
        other_month.a,
        this_unpicked.a,
        other_unpicked.a,
        if roles_ok && alphas_ok { "✓" } else { "✗" }
    );
    if !(roles_ok && alphas_ok) {
        pass = false;
    }

    // ── The text face: the export's 328×512 box, a 120 band, a field the insets leave 280
    // wide — and the buffer and the three actions actually reaching the app.
    let input = date_input_id();
    let surface = rect_of(lp, input);
    let header = rect_of(lp, input.with("header"));
    let field = rect_of(lp, ui_widgets_md3::date_input_field_id(input).with("root"));
    let geometry_ok = matches!(
        (surface, header, field),
        (Some(s), Some(h), Some(f))
            if (s.size.w - 328.0).abs() < 0.5
                && (s.size.h - 512.0).abs() < 0.5
                && h.size.h >= 119.5
                && (h.origin.y - s.origin.y).abs() < 0.5
                && (f.size.w - 280.0).abs() < 0.5
                && (f.origin.x - s.origin.x - 24.0).abs() < 0.5
    );
    println!(
        "selftest: md3 date input surface {:?} header {:?} field {:?} {}",
        surface.map(|r| (r.size.w, r.size.h)),
        header.map(|r| (r.origin.y, r.size.h)),
        field.map(|r| (r.origin.x, r.size.w)),
        if geometry_ok { "✓" } else { "✗" }
    );
    pass &= geometry_ok;

    let mut typed_ok = false;
    if let Some(f) = field {
        // The field's own box is one label band below its root, as every outlined field is.
        click(lp, size, center(f));
        lp.dispatch(ui_core::Event::Text("10/30/2026".to_string()));
        for _ in 0..2 {
            frame(lp, size, None);
        }
        typed_ok = lp.app_mut().di_text == "10/30/2026";
    }
    let mut actions_ok = true;
    for (action, rect) in [
        (
            ui_widgets_md3::DateInputAction::Calendar,
            rect_of(lp, ui_widgets_md3::DateInputAction::Calendar.id(input)),
        ),
        (
            ui_widgets_md3::DateInputAction::Cancel,
            rect_of(lp, ui_widgets_md3::DateInputAction::Cancel.id(input)),
        ),
        (
            ui_widgets_md3::DateInputAction::Confirm,
            rect_of(lp, ui_widgets_md3::DateInputAction::Confirm.id(input)),
        ),
    ] {
        let reported = match rect {
            Some(r) => {
                click(lp, size, center(r));
                lp.app_mut().di_action == Some(action)
            }
            None => false,
        };
        actions_ok &= reported;
    }
    let (buffer, last) = {
        let app = lp.app_mut();
        (app.di_text.clone(), app.di_action)
    };
    println!(
        "selftest: md3 date input typing={typed_ok} actions={actions_ok} buffer {buffer:?} \
         last {last:?} {}",
        if typed_ok && actions_ok { "✓" } else { "✗" }
    );
    pass &= typed_ok && actions_ok;

    // ── The modal face: the page's button declares it over the whole window, the surface is
    // the export's 360×524 centred there, and a day and the scrim each close it.
    let button = date_modal_button_id();
    let open_button = |lp: &AppLoop<Gallery>| rect_of(lp, button).map(center);
    match open_button(lp) {
        Some(at) => click(lp, size, at),
        None => pass = false,
    }
    frame(lp, size, None);
    let modal = date_modal_id();
    let window = Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32);
    let barrier_ok = rect_of(lp, modal).is_some_and(|b| close_rect(b, window));
    let surface = rect_of(lp, modal.with("surface"));
    let header = rect_of(lp, modal.with("header"));
    let headline = rect_of(lp, modal.with("header/headline"));
    let row = rect_of(lp, modal.with("month-row"));
    let geometry_ok = matches!((surface, header, headline, row),
        (Some(s), Some(h), Some(t), Some(r))
            if (s.size.w - 360.0).abs() < 0.5
                && (s.size.h - 524.0).abs() < 0.5
                && ((s.origin.x + s.size.w * 0.5) - window.size.w * 0.5).abs() < 0.5
                && ((s.origin.y + s.size.h * 0.5) - window.size.h * 0.5).abs() < 0.5
                && (h.size.h - 120.0).abs() < 0.5
                && (h.origin.y - s.origin.y).abs() < 0.5
                && t.size.w > 0.0
                && (r.size.h - 56.0).abs() < 0.5
    );
    println!(
        "selftest: md3 date picker modal barrier={barrier_ok} surface {:?} header {:?} \
         headline {:?} row {:?} {}",
        surface.map(|r| (r.origin.x, r.origin.y, r.size.w, r.size.h)),
        header.map(|r| (r.origin.y, r.size.h)),
        headline.map(|r| (r.origin.y, r.size.h)),
        row.map(|r| r.size.h),
        if barrier_ok && geometry_ok {
            "✓"
        } else {
            "✗"
        }
    );
    pass &= barrier_ok && geometry_ok;

    // A press beside the surface reaches the scrim, not the page under it.
    let outside = surface.map(|s| Point::new(s.origin.x - 24.0, s.origin.y + 24.0));
    match outside {
        Some(at) => click(lp, size, at),
        None => pass = false,
    }
    frame(lp, size, None);
    let scrim_closed = !lp.app_mut().dp_modal.open && rect_of(lp, modal).is_none();
    println!(
        "selftest: md3 date picker modal scrim dismiss={scrim_closed} {}",
        if scrim_closed { "✓" } else { "✗" }
    );
    pass &= scrim_closed;

    // Reopened, a day press is the same `Picked` the docked form reports, and the app
    // closes the dialog on it.
    if let Some(at) = open_button(lp) {
        click(lp, size, at);
    }
    frame(lp, size, None);
    let wanted = ui_widgets_md3::Date::new(2026, 10, 15).expect("a real date");
    let picked = match rect_of(lp, ui_widgets_md3::day_cell_id(modal, wanted)) {
        Some(r) => {
            click(lp, size, center(r));
            true
        }
        None => false,
    };
    frame(lp, size, None);
    let pick_ok =
        picked && lp.app_mut().dp_modal.value == Some(wanted) && !lp.app_mut().dp_modal.open;
    println!(
        "selftest: md3 date picker modal picked={pick_ok} {}",
        if pick_ok { "✓" } else { "✗" }
    );
    pass &= pick_ok;

    // ── The range form: its own modal over the same window, the shared 360×524 box with a
    // 68 dp header, and two presses that the selection rule turns into a range.
    match rect_of(lp, date_range_button_id()).map(center) {
        Some(at) => click(lp, size, at),
        None => pass = false,
    }
    frame(lp, size, None);
    let range = date_range_id();
    let range_barrier = rect_of(lp, range).is_some_and(|b| close_rect(b, window));
    let range_surface = rect_of(lp, range.with("surface"));
    let range_header = rect_of(lp, range.with("header"));
    let range_weekdays = rect_of(lp, range.with("weekdays"));
    let range_list = rect_of(lp, range.with("months"));
    let range_geometry = matches!(
        (range_surface, range_header, range_weekdays, range_list),
        (Some(s), Some(h), Some(w), Some(l))
            if (s.size.w - 360.0).abs() < 0.5
                && (s.size.h - 524.0).abs() < 0.5
                && ((s.origin.x + s.size.w * 0.5) - window.size.w * 0.5).abs() < 0.5
                && ((s.origin.y + s.size.h * 0.5) - window.size.h * 0.5).abs() < 0.5
                && (h.size.h - 68.0).abs() < 0.5
                && (h.origin.y - s.origin.y).abs() < 0.5
                && (w.size.h - 48.0).abs() < 0.5
                && (l.size.w - 336.0).abs() < 0.5
                && (l.size.h - 408.0).abs() < 0.5
    );
    println!(
        "selftest: md3 date range barrier={range_barrier} surface {:?} header {:?} \
         list {:?} {}",
        range_surface.map(|r| (r.origin.x, r.origin.y, r.size.w, r.size.h)),
        range_header.map(|r| r.size.h),
        range_list.map(|r| (r.size.w, r.size.h)),
        if range_barrier && range_geometry {
            "✓"
        } else {
            "✗"
        }
    );
    pass &= range_barrier && range_geometry;

    // Two presses: the first sets the start, the second ends the range after it.
    let first = ui_widgets_md3::Date::new(2026, 10, 4).expect("a real date");
    let last = ui_widgets_md3::Date::new(2026, 10, 9).expect("a real date");
    for day in [first, last] {
        match rect_of(lp, ui_widgets_md3::range_day_id(range, day)) {
            Some(r) => click(lp, size, center(r)),
            None => pass = false,
        }
        frame(lp, size, None);
    }
    let range_ok =
        lp.app_mut().dp_range.start == Some(first) && lp.app_mut().dp_range.end == Some(last);
    println!(
        "selftest: md3 date range selection={range_ok} {}",
        if range_ok { "✓" } else { "✗" }
    );
    pass &= range_ok;

    // The scrim still closes the picker, leaving the selection the app kept.
    match range_surface.map(|s| Point::new(s.origin.x - 24.0, s.origin.y + 24.0)) {
        Some(at) => click(lp, size, at),
        None => pass = false,
    }
    frame(lp, size, None);
    let range_closed = !lp.app_mut().dp_range.open && rect_of(lp, range).is_none();
    println!(
        "selftest: md3 date range scrim dismiss={range_closed} {}",
        if range_closed { "✓" } else { "✗" }
    );
    pass &= range_closed;
    pass
}
