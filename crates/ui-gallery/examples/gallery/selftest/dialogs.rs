// Self-test checks for the dialogs page.

use ui_widgets_glass::{
    DIALOG_ACTION_GAP, DIALOG_ACTION_H, DIALOG_MESSAGE_ALPHA, DIALOG_PAD, DIALOG_WIDTH,
};

use crate::pages::dialogs_fluent::{FLUENT_PREVIEWS, fluent_preview_id};
use crate::selftest::*;

pub(crate) fn dialogs(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The Dialogs page: every build-time variant present and inside its column, a click on an
    // action names it, and a click on the modal's scrim cancels the modal.
    goto(lp, size, Page::Dialogs);
    // WinUI's `CommandSpace` gives every visible action an equal `*` column with an 8 pt gap
    // between them, and a lone action takes the right half. A stretched button centres its
    // label in that column, as `HorizontalContentAlignment` does.
    let mut fluent_ok = true;
    for (k, preview) in FLUENT_PREVIEWS.iter().enumerate() {
        let base = fluent_preview_id(k);
        let Some(commands) = rect_of(lp, base.with("commands")) else {
            continue;
        };
        let visible: Vec<&str> = [
            ("primary", preview.primary),
            ("secondary", preview.secondary),
            ("close", preview.close),
        ]
        .iter()
        .filter(|(_, text)| text.is_some())
        .map(|(tag, _)| *tag)
        .collect();
        let band_x = commands.origin.x + ui_widgets_fluent::DIALOG_PAD;
        let band_w = commands.size.w - 2.0 * ui_widgets_fluent::DIALOG_PAD;
        let gap = ui_widgets_fluent::DIALOG_BUTTON_SPACING;
        let n = visible.len() as f32;
        for (i, tag) in visible.iter().enumerate() {
            let Some(button) = rect_of(lp, base.with(tag)) else {
                fluent_ok = false;
                println!("selftest: fluent dialog {k} action {tag} has no box");
                continue;
            };
            let (want_x, want_w) = if n == 1.0 {
                // The empty primary column keeps its share, so the action sits in the right half.
                let w = (band_w - gap) * 0.5;
                (band_x + band_w - w, w)
            } else {
                let w = (band_w - gap * (n - 1.0)) / n;
                (band_x + i as f32 * (w + gap), w)
            };
            let placed =
                (button.origin.x - want_x).abs() < 0.5 && (button.size.w - want_w).abs() < 0.5;
            let label = rect_of(lp, base.with(tag).with("label"));
            let centred = label
                .map(|l| {
                    let b = button.origin.x + button.size.w * 0.5;
                    let c = l.origin.x + l.size.w * 0.5;
                    (b - c).abs() < 0.5
                })
                .unwrap_or(false);
            if !(placed && centred) {
                fluent_ok = false;
            }
            println!(
                "selftest: fluent dialog {k} action {tag} at {:.1} w {:.1} (want {want_x:.1} + {want_w:.1}), label centred={centred}",
                button.origin.x, button.size.w
            );
        }
    }
    if !fluent_ok {
        pass = false;
    }
    let col = rect_of(lp, col_id(Page::Dialogs, "md3")).expect("dialogs column");
    let mut previews = 0;
    let mut inside = true;
    for k in 0..DIALOG_PREVIEWS.len() {
        if let Some(r) = rect_of(lp, dialog_preview_id(k)) {
            previews += 1;
            if r.origin.y + r.size.h > col.origin.y + col.size.h + 0.01 {
                inside = false;
                println!("selftest: dialog preview {k} spills out of its column");
            }
        }
    }
    // An action row wider than its band overflows the surface: with no measurement at decl
    // time the app has to keep the labels short enough, so the demo must show it can be done.
    for (k, row) in DIALOG_PREVIEWS.iter().enumerate() {
        let Some(surface) = rect_of(lp, dialog_preview_id(k)) else {
            continue;
        };
        for i in 0..row.3.len() {
            if let Some(a) = rect_of(lp, dialog_action_id(k, i))
                && a.origin.x + a.size.w > surface.origin.x + surface.size.w + 0.01
            {
                inside = false;
                println!("selftest: dialog preview {k} action {i} spills past its surface");
            }
        }
    }
    if let Some(a) = rect_of(lp, dialog_action_id(0, 1)) {
        click(lp, size, center(a));
    }
    let named = lp.app_mut().dialog_picked == Some(DIALOG_PREVIEWS[0].3[1]);
    let dismissed_before = lp.app_mut().dialog_dismissed;
    if let Some(b) = rect_of(lp, dialog_open_id()) {
        click(lp, size, center(b));
    }
    let opened = lp.app_mut().dialog_open;
    for _ in 0..12 {
        frame(lp, size, None);
    }
    // A modal confined to the column would dim that column only and cover the samples it
    // is supposed to float above; the scrim must span the window.
    let covers_window = rect_of(lp, dialog_modal_id())
        .map(|r| {
            r.origin.x.abs() < 0.5
                && r.origin.y.abs() < 0.5
                && (r.size.w - size.0 as f32).abs() < 0.5
                && (r.size.h - size.1 as f32).abs() < 0.5
        })
        .unwrap_or(false);
    let mut dismissed_after = dismissed_before;
    if let Some(r) = rect_of(lp, dialog_modal_id()) {
        // The surface centres, so the scrim's own corner is far outside it.
        click(lp, size, Point::new(r.origin.x + 6.0, r.origin.y + 6.0));
        dismissed_after = lp.app_mut().dialog_dismissed;
    }
    let closed = !lp.app_mut().dialog_open;
    // A modal that is opened, dismissed and opened again must come back whole: the first
    // pass leaves nothing behind for the second to inherit.
    let mut reopened = 0;
    for _ in 0..2 {
        if let Some(b) = rect_of(lp, dialog_open_id()) {
            click(lp, size, center(b));
        }
        let surface = rect_of(lp, dialog_modal_id().with("surface"));
        if lp.app_mut().dialog_open && surface.map(|r| r.size.w > 0.0).unwrap_or(false) {
            reopened += 1;
        }
        if let Some(r) = rect_of(lp, dialog_modal_id()) {
            click(lp, size, Point::new(r.origin.x + 6.0, r.origin.y + 6.0));
        }
    }
    let reopens_whole = reopened == 2 && !lp.app_mut().dialog_open;
    // A proximate modal must cover the host panel and centre its surface there, while the
    // rest of the window stays undimmed — the whole point of naming a target.
    let host = rect_of(lp, Id::str("dialog-panel")).expect("host panel");
    if let Some(b) = rect_of(lp, dialog_open_panel_id()) {
        click(lp, size, center(b));
    }
    for _ in 0..12 {
        frame(lp, size, None);
    }
    let proximate = lp.app_mut().dialog_target == ModalTarget::Proximate;
    let covers_host = rect_of(lp, dialog_modal_id())
        .map(|r| {
            (r.origin.x - host.origin.x).abs() < 0.5
                && (r.size.w - host.size.w).abs() < 0.5
                && (r.size.h - host.size.h).abs() < 0.5
        })
        .unwrap_or(false);
    let surface_in_host = rect_of(lp, dialog_modal_id().with("surface"))
        .map(|r| {
            r.origin.x >= host.origin.x
                && r.origin.x + r.size.w <= host.origin.x + host.size.w + 0.01
        })
        .unwrap_or(false);
    if let Some(r) = rect_of(lp, dialog_modal_id()) {
        click(lp, size, Point::new(r.origin.x + 4.0, r.origin.y + 4.0));
    }
    let host_closed = !lp.app_mut().dialog_open;
    let dialogs_ok = previews == DIALOG_PREVIEWS.len()
        && inside
        && named
        && opened
        && covers_window
        && dismissed_after == dismissed_before + 1
        && closed
        && reopens_whole
        && proximate
        && covers_host
        && surface_in_host
        && host_closed;
    println!(
        "selftest: md3 dialogs {previews}/{} in-column={inside} action={named} opened={opened} \
         window-scrim={covers_window} dismissed={} closed={closed} reopens={reopened}/2 \
         panel-scrim={covers_host}+{surface_in_host} closed={host_closed} {}",
        DIALOG_PREVIEWS.len(),
        dismissed_after - dismissed_before,
        if dialogs_ok { "✓" } else { "✗" }
    );
    if !dialogs_ok {
        pass = false;
    }
    if !glass_dialogs(lp, size) {
        pass = false;
    }
    pass
}

/// The glass alerts: a fixed 280 dp surface, bands that stretch so their lines centre, two
/// actions sharing one row, three stacking, and a press that names its action.
fn glass_dialogs(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut ok = true;
    let Some(col) = rect_of(lp, col_id(Page::Dialogs, "glass")) else {
        println!("selftest: glass dialog column missing");
        return false;
    };
    let inner = DIALOG_WIDTH - 2.0 * DIALOG_PAD;
    let half = (inner - DIALOG_ACTION_GAP) / 2.0;
    for k in 0..GLASS_DIALOG_PREVIEWS.len() {
        let base = glass_dialog_preview_id(k);
        let Some(surface) = rect_of(lp, base) else {
            println!("selftest: glass dialog {k} has no box");
            ok = false;
            continue;
        };
        let sized = (surface.size.w - DIALOG_WIDTH).abs() < 0.5;
        let in_column = surface.origin.x >= col.origin.x - 0.01
            && surface.origin.x + surface.size.w <= col.origin.x + col.size.w + 0.01;
        // A centred line can only centre inside a full-width box: the band must stretch.
        let stretched = rect_of(lp, base.with("surface").with("title"))
            .map(|t| {
                (t.origin.x - (surface.origin.x + DIALOG_PAD)).abs() < 0.5
                    && (t.size.w - inner).abs() < 0.5
            })
            .unwrap_or(true);
        if !(sized && in_column && stretched) {
            println!(
                "selftest: glass dialog {k} at {:.1} w {:.1} in-column={in_column} stretched={stretched}",
                surface.origin.x, surface.size.w
            );
            ok = false;
        }
    }
    // The inner box an action must stay inside: the surface minus its inset on both sides.
    let band = |k: usize| {
        rect_of(lp, glass_dialog_preview_id(k)).map(|s| {
            let left = s.origin.x + DIALOG_PAD;
            (left, left + inner)
        })
    };
    // Two actions share the strip; three stack, each taking the whole inner width.
    let pair = match (
        rect_of(lp, glass_dialog_action_id(0, 0)),
        rect_of(lp, glass_dialog_action_id(0, 1)),
        band(0),
    ) {
        (Some(a), Some(b), Some((left, right))) => {
            let spaced = (b.origin.x - (a.origin.x + a.size.w + DIALOG_ACTION_GAP)).abs() < 0.5;
            let same_row = (a.origin.y - b.origin.y).abs() < 0.5;
            let width = (a.size.w - half).abs() < 0.5 && (b.size.w - half).abs() < 0.5;
            let height = (a.size.h - DIALOG_ACTION_H).abs() < 0.5;
            // Whatever the gap, the strip may not spill out of the surface.
            let contained = a.origin.x >= left - 0.5 && b.origin.x + b.size.w <= right + 0.5;
            if !(spaced && same_row && width && height && contained) {
                println!(
                    "selftest: glass dialog pair a={:.1}+{:.1} b={:.1}+{:.1} row={same_row} \
                     contained={contained}",
                    a.origin.x, a.size.w, b.origin.x, b.size.w
                );
            }
            spaced && same_row && width && height && contained
        }
        _ => false,
    };
    let mut stacked = true;
    let mut previous: Option<Rect> = None;
    for i in 0..3 {
        let (Some(r), Some((left, right))) = (rect_of(lp, glass_dialog_action_id(1, i)), band(1))
        else {
            stacked = false;
            continue;
        };
        let full = (r.size.w - inner).abs() < 0.5;
        let below = previous
            .map(|p| (r.origin.y - (p.origin.y + p.size.h + DIALOG_ACTION_GAP)).abs() < 0.5)
            .unwrap_or(true);
        let contained = r.origin.x >= left - 0.5 && r.origin.x + r.size.w <= right + 0.5;
        if !(full && below && contained) {
            println!(
                "selftest: glass dialog stacked action {i} at {:.1}+{:.1} y {:.1}",
                r.origin.x, r.size.w, r.origin.y
            );
            stacked = false;
        }
        previous = Some(r);
    }
    let lone = match (rect_of(lp, glass_dialog_action_id(2, 0)), band(2)) {
        (Some(r), Some((left, right))) => {
            (r.size.w - inner).abs() < 0.5
                && r.origin.x >= left - 0.5
                && r.origin.x + r.size.w <= right + 0.5
        }
        _ => false,
    };
    // The message keeps the label's own ink at the secondary alpha.
    let surface = glass_dialog_preview_id(0).with("surface");
    let inked = match (
        text_ink(lp, surface.with("title")),
        text_ink(lp, surface.with("message")),
    ) {
        (Some(title), Some(message)) => {
            (message.a - DIALOG_MESSAGE_ALPHA).abs() < 0.01
                && message.r == title.r
                && message.g == title.g
                && message.b == title.b
        }
        _ => false,
    };
    // A press reports the action it landed on; the dialog never decides what that means.
    if let Some(a) = rect_of(lp, glass_dialog_action_id(0, 1)) {
        click(lp, size, center(a));
    }
    let named = lp.app_mut().dialog_picked == Some(GLASS_DIALOG_PREVIEWS[0].3[1].0);
    let glass_ok = ok && pair && stacked && lone && inked && named;
    println!(
        "selftest: glass dialogs bands={ok} pair={pair} stacked={stacked} lone={lone} \
         message-ink={inked} named={named} {}",
        if glass_ok { "✓" } else { "✗" }
    );
    glass_ok
}
