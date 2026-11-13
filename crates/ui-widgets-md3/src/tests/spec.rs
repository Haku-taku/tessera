//! The generated state function overlays states by priority; colours are pack roles.

use crate::spec::{SheetForm, SwitchFlags, sheet_spec, switch_spec};

#[test]
fn a_pressed_handle_overrides_the_selected_size() {
    // Pressed is a later state than `selected`, so 28dp wins over the 24dp handle.
    let s = switch_spec(SwitchFlags {
        selected: true,
        pressed: true,
        ..SwitchFlags::NONE
    });
    assert_eq!((s.handle.width, s.handle.height), (28.0, 28.0));
    assert_eq!(s.state_layer.opacity, 0.1);
}

#[test]
fn disabled_is_applied_last_and_keeps_the_track_visible() {
    // Disabled wins over hover, yet the track colour still comes from the disabled branch.
    let s = switch_spec(SwitchFlags {
        selected: true,
        hovered: true,
        pressed: true,
        disabled: true,
        ..SwitchFlags::NONE
    });
    assert_eq!(s.track.color, "onSurface");
    assert_eq!(s.track.opacity, 0.12);
    assert_eq!(s.handle.color, "surface");
    assert_eq!(s.handle.opacity, 1.0);
}

#[test]
fn the_sheet_takes_its_elevation_from_the_form_axis() {
    // `modal` is a variant value, not a boolean: a table whose overlay went missing would
    // paint the sheet with no shadow, which is invisible until a scrim sits behind it.
    let modal = sheet_spec(SheetForm::Modal);
    assert_eq!(modal.container.elevation, 1.0);
    assert_eq!(modal.container.color, "surfaceContainerLow");
    assert_eq!(modal.drag_handle.color, "onSurfaceVariant");
    assert_eq!(
        (modal.drag_handle.width, modal.drag_handle.height),
        (32.0, 4.0)
    );
    // The base has no elevation of its own; `standard` is the other value that carries one.
    assert_eq!(sheet_spec(SheetForm::Base).container.elevation, 0.0);
    assert_eq!(sheet_spec(SheetForm::Standard).container.elevation, 1.0);
}
