//! The MD3 date picker: the calendar's arithmetic, and the one action a frame reports.

use super::*;

use crate::date_picker::Date;

use ui_core::NodeData;
use ui_core::layout::AxisSize;

const PICKER: Id = Id::str("date-picker/modal");

fn build(mode: DatePickerMode) -> Decl {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 600, 800, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let mut params = DatePickerParams::new(PICKER, Date::new(2026, 10, 30).unwrap());
    params.mode = mode;
    params.headline = Some("Oct 24, 2026".into());
    let control = Md3DatePicker::from_pack(crate::pack::pack(), &mut cx, params);
    cx.with_cx(|cx| control.decl(cx))
}

fn find(decl: &Decl, id: Id) -> Option<&Decl> {
    if decl.id == id {
        return Some(decl);
    }
    decl.children.iter().find_map(|child| find(child, id))
}

#[test]
fn the_modal_form_reads_the_modal_table() {
    // The docked export is 456 tall with a 64 dp header; a modal built from it would draw a
    // docked box over the dialog's scrim. These numbers are the form switch itself.
    let modal = build(DatePickerMode::Modal);
    assert_eq!(modal.id, PICKER, "the widget itself is the scrim");
    let surface = find(&modal, PICKER.with("surface")).expect("a surface of its own");
    assert_eq!(surface.size_class.width, AxisSize::fixed(360.0));
    assert_eq!(surface.size_class.height, AxisSize::fixed(524.0));
    let header = find(&modal, PICKER.with("header")).expect("a header");
    assert_eq!(header.size_class.height, AxisSize::fixed(120.0));
    let row = find(&modal, PICKER.with("month-row")).expect("a month row");
    assert_eq!(row.size_class.height, AxisSize::fixed(56.0));
    // The second line is the modal table's 32/40 headline, not the docked label.
    let headline = find(&modal, PICKER.with("header/headline")).expect("the picked date");
    let NodeData::Text { block, .. } = &headline.data else {
        panic!("the headline is a text line");
    };
    assert_eq!(block.runs[0].style.size, 32.0);

    // Docked keeps its own box and its own id: it is the widget, not a surface of the scrim.
    let docked = build(DatePickerMode::Docked);
    assert_eq!(docked.id, PICKER);
    assert_eq!(docked.size_class.height, AxisSize::fixed(456.0));
    let header = find(&docked, PICKER.with("header")).expect("a header");
    assert_eq!(header.size_class.height, AxisSize::fixed(64.0));
}

#[test]
fn february_follows_the_gregorian_leap_rule() {
    // The four-year rule alone is wrong for century years, which is the one mistake every
    // hand-rolled calendar makes: 1900 and 2100 are not leap, 2000 is.
    assert_eq!(Date::days_in_month(2024, 2), 29, "a leap year");
    assert_eq!(Date::days_in_month(2025, 2), 28, "a common year");
    assert_eq!(Date::days_in_month(2000, 2), 29, "a 400-year leap");
    assert_eq!(Date::days_in_month(1900, 2), 28, "a 100-year non-leap");
    assert_eq!(
        Date::days_in_month(2100, 2),
        28,
        "another 100-year non-leap"
    );
    assert!(Date::new(2023, 2, 29).is_none(), "no 29 February in 2023");
}

#[test]
fn stepping_a_month_clamps_the_day_into_the_target() {
    // 31 January cannot become 31 February; the app must get a date that exists.
    let jan31 = Date::new(2024, 1, 31).unwrap();
    assert_eq!(
        jan31.add_months(1),
        Date::new(2024, 2, 29).unwrap(),
        "a leap February"
    );
    assert_eq!(
        Date::new(2023, 1, 31).unwrap().add_months(1),
        Date::new(2023, 2, 28).unwrap(),
        "a common February"
    );
    assert_eq!(
        Date::new(2026, 1, 15).unwrap().add_months(-1),
        Date::new(2025, 12, 15).unwrap(),
        "backwards over a year end"
    );
}
