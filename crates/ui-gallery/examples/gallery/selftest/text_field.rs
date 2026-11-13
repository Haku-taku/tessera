// Self-test checks for the text field page.

use crate::selftest::*;

pub(crate) fn text_field(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::TextField);
    let mut typed = false;
    let mut banded = false;
    let mut floated = false;
    if let Some(field) = rect_of(lp, text_field_id(1)) {
        click(lp, size, center(field));
        lp.dispatch(ui_core::Event::Text("hi".to_string()));
        for _ in 0..2 {
            frame(lp, size, None);
        }
        typed = lp.app_mut().tf_value[1] == "hi";
        // The outlined field's own box sits one label band below its root.
        banded = match (
            rect_of(lp, text_field_id(1).with("root")),
            rect_of(lp, text_field_id(1)),
        ) {
            (Some(root), Some(box_)) => (box_.origin.y - root.origin.y - 8.0).abs() < 0.5,
            _ => false,
        };
        // Text in the buffer shrinks the label to the populated size (16sp → 12sp).
        floated = rect_of(lp, text_field_id(1).with("label-text"))
            .map(|l| l.size.h < 20.0)
            .unwrap_or(false);
    }
    // The icon slot centres its mark both ways: the slot is 56 tall and the mark 24, so an
    // inset derived from `Text`'s line box would hang it 4.8 dp high.
    let marker = Id::of(("tf/field", 3)).with("leading");
    let icons_centred = match (rect_of(lp, marker), rect_of(lp, marker.with("icon"))) {
        (Some(slot), Some(icon)) => {
            let dx = (slot.origin.x + slot.size.w * 0.5) - (icon.origin.x + icon.size.w * 0.5);
            let dy = (slot.origin.y + slot.size.h * 0.5) - (icon.origin.y + icon.size.h * 0.5);
            dx.abs() < 0.5 && dy.abs() < 0.5
        }
        _ => false,
    };
    let tf_ok = typed && banded && floated && icons_centred;
    println!(
        "selftest: md3 text fields typed={typed} height={banded} samples={floated} \
         icon-centred={icons_centred} {}",
        if tf_ok { "✓" } else { "✗" }
    );
    if !tf_ok {
        pass = false;
    }
    pass
}

/// A secret field draws marks over the real buffer, and the engine copies none of it.
pub(crate) fn text_field_secure(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::TextField);
    let field = glass_secure_field_id();
    scroll_into_view(lp, size, field);
    let mut typed = false;
    let mut marked = false;
    if let Some(rect) = rect_of(lp, field) {
        click(lp, size, center(rect));
        type_text(lp, size, "hunter2");
        typed = lp.app_mut().tf_secure == "hunter2";
        // The app holds the password; the screen holds one mark per character.
        marked = painted_text(lp, field.with("input")).is_some_and(|shown| {
            !shown.is_empty()
                && shown != "hunter2"
                && shown
                    .chars()
                    .all(|mark| matches!(mark, '*' | '\u{b7}' | '\u{2022}'))
        });
    }
    // Select all and copy: a secret buffer leaves by no engine path, so nothing is queued.
    lp.take_clipboard_commands();
    let ctrl = ui_core::Modifiers {
        ctrl: true,
        ..ui_core::Modifiers::default()
    };
    lp.dispatch(ui_core::Event::Modifiers(ctrl));
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Char('a'),
    });
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Char('c'),
    });
    lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
    frame(lp, size, None);
    let copied = !lp.take_clipboard_commands().is_empty();
    println!(
        "selftest: glass secure field typed={typed} marked={marked} copied={copied} {}",
        if typed && marked && !copied {
            "✓"
        } else {
            "✗"
        }
    );
    if !(typed && marked && !copied) {
        pass = false;
    }
    pass
}

pub(crate) fn text_field_caret(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Click-to-caret and in-field selection: pressing near the left edge puts the caret
    // near the start, and a drag leaves a wash the pack's own painter drew.
    let mut caret_ok = false;
    let mut field_sel = false;
    if let Some(input) = rect_of(lp, text_field_id(1).with("input")) {
        let y = input.origin.y + input.size.h * 0.5;
        let from = Point::new(input.origin.x + 2.0, y);
        click(lp, size, from);
        caret_ok = lp
            .caret()
            .is_some_and(|(_, at)| at < lp.app_mut().tf_value[1].len().max(1));
        let to = Point::new(input.origin.x + input.size.w - 2.0, y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: from });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: to,
            button: ui_core::PointerButton::Primary,
        });
        frame(lp, size, None);
        field_sel = ui_widgets_md3::pack_of(appearance_of(lp.app_mut().dark))
            .ink("textSelection")
            .is_some_and(|want| lp.draw_list().prims.rects.iter().any(|r| r.fill == want));
    }
    println!(
        "selftest: md3 text field caret={caret_ok} selection={field_sel} {}",
        if caret_ok && field_sel { "✓" } else { "✗" }
    );
    if !(caret_ok && field_sel) {
        pass = false;
    }
    pass
}

pub(crate) fn text_field_ime(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Composition: a live preedit must underline in place and never touch the app's buffer.
    goto(lp, size, Page::TextField);
    let mut composed = false;
    if let Some(input) = rect_of(lp, text_field_id(1).with("input")) {
        let y = input.origin.y + input.size.h * 0.5;
        click(lp, size, Point::new(input.origin.x + 2.0, y));
        let before = lp.app_mut().tf_value[1].clone();
        lp.dispatch(ui_core::Event::Preedit(Some(ui_core::Preedit::new(
            "にほ", 6,
        ))));
        frame(lp, size, None);
        let underlined = lp.draw_list().prims.rects.iter().any(|r| {
            r.rect.size.h <= 1.5
                && r.rect.size.w > 4.0
                && r.rect.origin.x >= input.origin.x - 1.0
                && r.rect.origin.x <= input.origin.x + input.size.w + 40.0
        });
        composed = underlined && lp.app_mut().tf_value[1] == before;
    }
    println!(
        "selftest: ime composition underlined={composed} {}",
        if composed { "✓" } else { "✗" }
    );
    if !composed {
        pass = false;
    }
    pass
}

pub(crate) fn glass_text_field(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::TextField);
    let mut placed = false;
    let mut search_sized = false;
    if let Some(field) = rect_of(lp, glass_text_field_id()) {
        // 16 dp inset, a 20 dp icon slot, then the 12 dp gap the line starts after.
        let want_x = field.origin.x + 16.0 + 20.0 + 12.0;
        let icon_ok = match (
            rect_of(lp, glass_text_field_id().with("leading")),
            rect_of(lp, glass_text_field_id().with("leading").with("glyph")),
        ) {
            (Some(slot), Some(icon)) => {
                (slot.origin.x - (field.origin.x + 16.0)).abs() < 0.5
                    && (slot.origin.y + slot.size.h * 0.5 - (icon.origin.y + icon.size.h * 0.5))
                        .abs()
                        < 0.5
            }
            _ => false,
        };
        let line_ok = match rect_of(lp, glass_text_field_id().with("input")) {
            Some(line) => {
                (line.origin.x - want_x).abs() < 0.5
                    && (line.origin.y + line.size.h * 0.5 - (field.origin.y + field.size.h * 0.5))
                        .abs()
                        < 0.5
            }
            None => false,
        };
        // An empty field shows its hint without focus; Cupertino's placeholder is not a
        // focus effect.
        let hinted = rect_of(lp, glass_text_field_id().with("hint")).is_some();
        placed = icon_ok && line_ok && hinted;
        let search_line_ok = match (
            rect_of(lp, glass_search_field_id()),
            rect_of(lp, glass_search_field_id().with("input")),
        ) {
            (Some(search), Some(line)) => {
                // 12 dp inset, 20 dp slot, 8 dp gap: the compact variant's own numbers.
                (search.size.h - 44.0).abs() < 0.5
                    && (line.origin.x - (search.origin.x + 12.0 + 20.0 + 8.0)).abs() < 0.5
                    && (line.origin.y + line.size.h * 0.5 - (search.origin.y + search.size.h * 0.5))
                        .abs()
                        < 0.5
            }
            _ => false,
        };
        search_sized = search_line_ok;
    }
    let mut typed = false;
    if let Some(field) = rect_of(lp, glass_text_field_id()) {
        click(lp, size, center(field));
        lp.dispatch(ui_core::Event::Text("glass".to_string()));
        for _ in 0..2 {
            frame(lp, size, None);
        }
        typed = lp.app_mut().tf_glass == "glass";
    }
    // The disabled sample must not take focus even when clicked, and its text is faded
    // relative to the live field's — compared, not checked against a copied constant.
    let mut still_quiet = true;
    let mut faded = false;
    if let Some(still) = rect_of(lp, glass_still_field_id()) {
        click(lp, size, center(still));
        frame(lp, size, None);
        still_quiet = lp.caret().map(|(id, _)| id) != Some(glass_still_field_id());
        faded = match (
            text_ink(lp, glass_still_field_id().with("input")),
            text_ink(lp, glass_text_field_id().with("input")),
        ) {
            (Some(still_ink), Some(live_ink)) => still_ink.a < live_ink.a,
            _ => false,
        };
    }
    let ok = placed && search_sized && typed && still_quiet && faded;
    println!(
        "selftest: glass text field placed={placed} search={search_sized} typed={typed} \
         disabled-quiet={still_quiet} faded={faded} {}",
        if ok { "✓" } else { "✗" }
    );
    if !ok {
        pass = false;
    }
    pass
}

pub(crate) fn glass_search_bar(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    goto(lp, size, Page::Search);
    // Let the cancel button finish its 250 ms slide-in before measuring.
    for _ in 0..20 {
        frame(lp, size, None);
    }
    let mut shaped = false;
    if let (Some(bar), Some(slot), Some(cancel), Some(field)) = (
        rect_of(lp, glass_search_id()),
        rect_of(lp, glass_search_id().with("cancel-slot")),
        rect_of(lp, glass_search_id().with("cancel")),
        rect_of(lp, glass_search_id().with("field")),
    ) {
        shaped = (bar.size.h - 44.0).abs() < 0.5
            && (cancel.size.w - 44.0).abs() < 0.5
            && (slot.origin.x + slot.size.w - (bar.origin.x + bar.size.w)).abs() < 0.5
            // The field gives up exactly the cancel button plus its 10 dp gap.
            && (field.size.w - (bar.size.w - 44.0 - 10.0)).abs() < 0.5;
    }
    // Nothing to clear while the query is empty: the × is absent, not merely invisible.
    let quiet_clear = text_ink(
        lp,
        glass_search_id()
            .with("field")
            .with("trailing")
            .with("glyph"),
    )
    .map(|ink| ink.a < 0.01)
    .unwrap_or(true);
    let mut typed = false;
    let mut cleared = false;
    if let Some(field) = rect_of(lp, glass_search_id().with("field")) {
        click(lp, size, center(field));
        lp.dispatch(ui_core::Event::Text("abc".to_string()));
        for _ in 0..14 {
            frame(lp, size, None);
        }
        typed = lp.app_mut().glass_search == "abc";
        // The × cross-fades in over 180 ms, then acts.
        if let Some(clear) = rect_of(lp, glass_search_id().with("field").with("trailing")) {
            click(lp, size, center(clear));
            for _ in 0..2 {
                frame(lp, size, None);
            }
            cleared = lp.app_mut().glass_search.is_empty()
                && lp.app_mut().glass_search_acted.as_deref() == Some("clear");
        }
    }
    let mut cancelled = false;
    if let Some(cancel) = rect_of(lp, glass_search_id().with("cancel")) {
        click(lp, size, center(cancel));
        for _ in 0..2 {
            frame(lp, size, None);
        }
        cancelled = lp.app_mut().glass_search_acted.as_deref() == Some("cancel");
    }
    let ok = shaped && quiet_clear && typed && cleared && cancelled;
    println!(
        "selftest: glass search bar shaped={shaped} quiet-clear={quiet_clear} typed={typed} \
         cleared={cleared} cancelled={cancelled} {}",
        if ok { "✓" } else { "✗" }
    );
    if !ok {
        pass = false;
    }
    pass
}
