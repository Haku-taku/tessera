// Self-test checks for the search page.

use crate::selftest::*;

pub(crate) fn search(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // Search bar: the query the engine edits lands on the app's buffer, and the bar's own
    // action names itself when clicked.
    goto(lp, size, Page::Search);
    let mut query_ok = false;
    let mut acted_ok = false;
    if let Some(bar) = rect_of(lp, search_id()) {
        click(lp, size, center(bar));
        for c in "hel".chars() {
            lp.dispatch(ui_core::Event::Text(c.to_string()));
        }
        step(lp, size, center(bar), 2);
        query_ok = lp.app_mut().search_value == "hel";
        if let Some(action) = rect_of(lp, search_action_id()) {
            click(lp, size, center(action));
            acted_ok = lp.app_mut().search_acted.as_deref() == Some("mic");
        }
    }
    let query = lp.app_mut().search_value.clone();
    let acted = lp.app_mut().search_acted.clone();
    println!(
        "selftest: md3 search query \"{query}\" action={acted:?} {}",
        if query_ok && acted_ok { "✓" } else { "✗" }
    );
    if !(query_ok && acted_ok) {
        pass = false;
    }

    // The bar's optional slots: the helper line grows the control's box below a pill that
    // keeps its own 56, and the avatar keeps the export's 30dp inside the pill.
    let mut slots_ok = false;
    if let (Some(root), Some(pill), Some(mark)) = (
        rect_of(lp, Id::str("search/avatar").with("root")),
        rect_of(lp, Id::str("search/avatar")),
        rect_of(lp, Id::str("search/avatar/mark")),
    ) {
        slots_ok = (pill.size.h - ui_widgets_md3::SEARCH_BAR_HEIGHT).abs() < 1.0
            && root.size.h >= pill.size.h + 28.0
            && (mark.size.w - 30.0).abs() < 0.5
            && (mark.size.h - 30.0).abs() < 0.5
            && mark.origin.x > pill.origin.x + pill.size.w * 0.5;
    }
    println!(
        "selftest: md3 search supported bar {}",
        if slots_ok { "✓" } else { "✗" }
    );
    if !slots_ok {
        pass = false;
    }
    pass
}
