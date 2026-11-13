//! The MD3 slider: gapped track geometry, the stop dot, the handle's press narrowing and the
//! press-to-value mapping.

use super::*;

use crate::slider::{Md3Slider, SliderOpts};
use crate::spec::{SliderFlags, SliderSize, slider_spec};
use ui_core::cx::Routing;
use ui_core::geom::{Point, Rect};

const ID: Id = Id::str("s");
const WIDTH: f32 = 240.0;

fn build(
    store: &mut AnimStore,
    routing: Routing,
    now: f64,
    value: f32,
    size: SliderSize,
    params: Md3Options,
) -> (Md3Slider, Decl, ui_widgets::WidgetState) {
    build_opts(
        store,
        routing,
        now,
        value,
        size,
        params,
        SliderOpts::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_opts(
    store: &mut AnimStore,
    routing: Routing,
    now: f64,
    value: f32,
    size: SliderSize,
    params: Md3Options,
    opts: SliderOpts,
) -> (Md3Slider, Decl, ui_widgets::WidgetState) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = {
        let mut node = cx.at(ID);
        Md3Slider::with_opts(
            ID,
            value,
            WIDTH,
            size,
            params,
            opts,
            Some(&mut node),
            &crate::pack::pack(),
        )
    };
    let state = cx.state(ID);
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl, state)
}

/// The export splits the two track insets across the active and inactive states; they agree.
fn insets(size: SliderSize) -> (f32, f32) {
    let on = slider_spec(
        SliderFlags {
            active: true,
            ..SliderFlags::NONE
        },
        size,
    );
    let off = slider_spec(SliderFlags::NONE, size);
    assert_eq!(
        on.track_shape.leading, off.track_shape.trailing,
        "the two halves agree"
    );
    (on.track_shape.leading, off.track_shape.trailing)
}

#[test]
fn a_pressed_slider_reports_the_value_under_the_finger() {
    // The slider maps the press point itself, in its own space; the app never sees a
    // coordinate, and a wrong offset here puts the handle beside the finger.
    let mut store = AnimStore::new();
    let (control, _, _) = build(
        &mut store,
        Routing::default(),
        0.0,
        0.25,
        SliderSize::XSmall,
        Md3Options::default(),
    );
    let (inset_l, inset_r) = insets(SliderSize::XSmall);
    let travel = WIDTH - inset_l - inset_r;
    let mut state = ui_widgets::WidgetState::default();
    state.pressed = true;
    state.local = Some(Point::new(inset_l + travel * 0.75, 16.0));
    match control.change(&state, Rect::ZERO) {
        Some(ui_core::Change::Value(v)) => {
            assert!(
                (v - 0.75).abs() < 1e-4,
                "three quarters along asks 0.75, got {v}"
            )
        }
        other => panic!("expected Value(0.75), got {other:?}"),
    }
}
