// Self-test checks for the checkbox page.

use crate::selftest::*;

pub(crate) fn checkbox(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    // The Checkbox page must show every variant, and a click must flip only the box it hit.
    goto(lp, size, Page::Checkbox);
    let mut boxes = 0;
    for k in 0..CHECKBOXES {
        if rect_of(lp, checkbox_id(k)).is_some() {
            boxes += 1;
        }
    }
    let was = lp.app_mut().checked;
    if let Some(second) = rect_of(lp, checkbox_id(1)) {
        click(lp, size, center(second));
    }
    let now = lp.app_mut().checked;
    // Box 1 starts mixed, and a mixed box reports "on", so the click must land it on `On`.
    let flipped =
        was[1] == ui_widgets_md3::CheckState::Mixed && now[1] == ui_widgets_md3::CheckState::On;
    let others = (0..CHECKBOXES)
        .filter(|k| *k != 1)
        .all(|k| now[k] == was[k]);
    if boxes == CHECKBOXES && flipped && others {
        println!("selftest: md3 checkbox variants {boxes}/{CHECKBOXES}, mixed click went on ✓");
    } else {
        println!(
            "selftest: md3 checkbox variants {boxes}/{CHECKBOXES}, flipped={flipped}, others untouched={others}"
        );
        pass = false;
    }
    // The group buttons are the only door to the undetermined state, so a click on "all mixed"
    // must write every box; without it the state exists in the API but never on screen.
    if let Some(b) = rect_of(lp, checkbox_group_id("mixed")) {
        click(lp, size, center(b));
    }
    let all_mixed = lp
        .app_mut()
        .checked
        .iter()
        .all(|v| *v == ui_widgets_md3::CheckState::Mixed);
    if all_mixed {
        println!("selftest: md3 checkbox group wrote mixed to all {CHECKBOXES} ✓");
    } else {
        println!("selftest: md3 checkbox group did not reach mixed");
        pass = false;
    }
    // The Fluent `IsThreeState` box is the only one whose click can *reach* indeterminate:
    // it starts mixed, and three clicks must walk it unchecked → checked → mixed.
    if let Some(b) = rect_of(lp, fluent_checkbox_id(4)) {
        let before = lp.app_mut().ftri;
        click(lp, size, center(b));
        let one = lp.app_mut().ftri;
        click(lp, size, center(b));
        let two = lp.app_mut().ftri;
        click(lp, size, center(b));
        let three = lp.app_mut().ftri;
        let tri = before == ui_widgets_fluent::CheckState::Indeterminate
            && one == ui_widgets_fluent::CheckState::Unchecked
            && two == ui_widgets_fluent::CheckState::Checked
            && three == ui_widgets_fluent::CheckState::Indeterminate;
        println!(
            "selftest: fluent three-state box walks off→on→mixed {before:?}→{one:?}→{two:?}→{three:?} {tri} {}",
            if tri { "✓" } else { "✗" }
        );
        if !tri {
            pass = false;
        }
    }
    pass
}
