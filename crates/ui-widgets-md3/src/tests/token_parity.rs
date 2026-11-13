//! Differential check: the generated token table against Flutter's `_SwitchDefaultsM3`.

use crate::spec::{SliderFlags, SliderSize, SwitchFlags, SwitchSpec, slider_spec, switch_spec};

/// (role, alpha) as Flutter's M3 defaults would resolve it. A transparent slot is `("", 0.0)`.
type Ink = (&'static str, f32);

fn thumb(s: &SwitchSpec) -> Ink {
    (s.handle.color, s.handle.opacity)
}

fn track(s: &SwitchSpec) -> Ink {
    (s.track.color, s.track.opacity)
}

fn outline(s: &SwitchSpec) -> Ink {
    if s.track_outline.color.is_empty() {
        return ("", 0.0);
    }
    (s.track_outline.color, s.track.opacity)
}

fn layer(s: &SwitchSpec) -> Ink {
    if s.state_layer.opacity <= 0.0 {
        return ("", 0.0);
    }
    (s.state_layer.color, s.state_layer.opacity)
}

/// Flutter's `thumbColor.resolve`, transcribed from `material/switch.dart`.
fn flutter_thumb(selected: bool, disabled: bool, lit: bool) -> Ink {
    match (disabled, selected, lit) {
        (true, true, _) => ("surface", 1.0),
        (true, false, _) => ("onSurface", 0.38),
        (false, true, true) => ("primaryContainer", 1.0),
        (false, true, false) => ("onPrimary", 1.0),
        (false, false, true) => ("onSurfaceVariant", 1.0),
        (false, false, false) => ("outline", 1.0),
    }
}

/// Flutter's `trackColor.resolve` and `trackOutlineColor.resolve`.
fn flutter_track(selected: bool, disabled: bool) -> Ink {
    match (disabled, selected) {
        (true, true) => ("onSurface", 0.12),
        (true, false) => ("surfaceContainerHighest", 0.12),
        (false, true) => ("primary", 1.0),
        (false, false) => ("surfaceContainerHighest", 1.0),
    }
}

fn flutter_outline(selected: bool, disabled: bool) -> Ink {
    if selected {
        ("", 0.0)
    } else if disabled {
        ("onSurface", 0.12)
    } else {
        ("outline", 1.0)
    }
}

/// Flutter's `overlayColor.resolve`: null at rest, 8% on hover, 10% on focus or press.
fn flutter_layer(selected: bool, hover: bool, focus: bool, press: bool) -> Ink {
    let role = if selected { "primary" } else { "onSurface" };
    if press || focus {
        (role, 0.1)
    } else if hover {
        (role, 0.08)
    } else {
        ("", 0.0)
    }
}

#[test]
fn every_state_combination_matches_flutter_defaults() {
    for selected in [false, true] {
        for disabled in [false, true] {
            for (hover, focus, press) in [
                (false, false, false),
                (true, false, false),
                (false, true, false),
                (false, false, true),
            ] {
                let s = switch_spec(SwitchFlags {
                    selected,
                    hovered: hover,
                    focused: focus,
                    pressed: press,
                    disabled,
                    ..SwitchFlags::NONE
                });
                let lit = hover || focus || press;
                let where_ = (selected, disabled, hover, focus, press);
                assert_eq!(
                    thumb(&s),
                    flutter_thumb(selected, disabled, lit),
                    "thumb {where_:?}"
                );
                assert_eq!(
                    track(&s),
                    flutter_track(selected, disabled),
                    "track {where_:?}"
                );
                assert_eq!(
                    outline(&s),
                    flutter_outline(selected, disabled),
                    "outline {where_:?}"
                );
                assert_eq!(
                    layer(&s),
                    flutter_layer(selected, hover, focus, press),
                    "layer {where_:?}"
                );
            }
        }
    }
}

#[test]
fn the_geometry_matches_the_m3_switch_config() {
    // `_SwitchConfigM3`: track 52x32, thumb 16/24, pressed 28, with icon 24, icon glyph 16.
    let off = switch_spec(SwitchFlags::NONE);
    let on = switch_spec(SwitchFlags {
        selected: true,
        ..SwitchFlags::NONE
    });
    let pressed = switch_spec(SwitchFlags {
        pressed: true,
        ..SwitchFlags::NONE
    });
    let icon = switch_spec(SwitchFlags {
        with_icon: true,
        ..SwitchFlags::NONE
    });
    assert_eq!((off.track.width, off.track.height), (52.0, 32.0));
    assert_eq!((off.handle.width, on.handle.width), (16.0, 24.0));
    assert_eq!((pressed.handle.width, pressed.handle.height), (28.0, 28.0));
    assert_eq!((icon.handle.width, icon.handle.height), (24.0, 24.0));
    assert_eq!((off.icon.size, on.icon.size), (16.0, 16.0));
    assert_eq!(off.state_layer.size, 40.0);
    assert_eq!(off.track_outline.width, 2.0);
}

/// The slider's track and handle against `_SliderDefaultsM3` (material/slider.dart).
#[test]
fn the_slider_track_and_handle_match_flutter_defaults() {
    let cases = [
        // (active, disabled, track role, track alpha, handle role, handle alpha)
        (true, false, "primary", 1.0, "primary", 1.0),
        (false, false, "secondaryContainer", 1.0, "primary", 1.0),
        (true, true, "onSurface", 0.38, "onSurface", 0.38),
        (false, true, "onSurface", 0.12, "onSurface", 0.38),
    ];
    for (active, disabled, track_role, track_alpha, handle_role, handle_alpha) in cases {
        let s = slider_spec(
            SliderFlags {
                active,
                disabled,
                ..SliderFlags::NONE
            },
            SliderSize::Base,
        );
        let where_ = (active, disabled);
        assert_eq!(
            (s.track.color, s.track.opacity),
            (track_role, track_alpha),
            "track {where_:?}"
        );
        assert_eq!(
            (s.handle.color, s.handle.opacity),
            (handle_role, handle_alpha),
            "handle {where_:?}"
        );
    }
}

/// Every size variant keeps the same handle width and only grows the track.
#[test]
fn the_slider_sizes_grow_the_track() {
    let sizes = [
        (SliderSize::Base, 16.0),
        (SliderSize::XSmall, 16.0),
        (SliderSize::Small, 24.0),
        (SliderSize::Medium, 40.0),
        (SliderSize::Large, 56.0),
        (SliderSize::XLarge, 96.0),
    ];
    for (size, height) in sizes {
        let s = slider_spec(
            SliderFlags {
                active: true,
                ..SliderFlags::NONE
            },
            size,
        );
        assert_eq!(s.track.height, height, "{size:?}");
        assert_eq!(s.handle.width, 4.0, "{size:?}");
    }
}
