//! MD3 time input: the 12-hour arithmetic, and the one action a frame reports.

use crate::time_picker::Period;

#[test]
fn twelve_am_is_midnight_and_twelve_pm_is_noon() {
    // The two hours every hand-rolled converter gets wrong: noon and midnight are both 12
    // on the clock face but eleven apart in the day.
    assert_eq!(Period::Am.hour24(12), 0);
    assert_eq!(Period::Pm.hour24(12), 12);
    assert_eq!(Period::Am.hour24(1), 1);
    assert_eq!(Period::Pm.hour24(1), 13);
    assert_eq!(Period::Am.hour24(11), 11);
    assert_eq!(Period::Pm.hour24(11), 23);
}
