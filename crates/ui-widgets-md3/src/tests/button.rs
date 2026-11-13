//! The MD3 button: five styles, five sizes, one pill with a centred icon and label.

use super::*;

use crate::spec::{ButtonSize, ButtonStyle, ButtonToggle};
use ui_core::cx::Routing;
use ui_core::draw::Shape;
use ui_core::geom::Rect;

const BTN: Id = Id::str("btn");

/// A 24dp mark in the button's ink: the icon variant's stand-in for a real glyph.
fn dot(id: Id, ink: Color) -> Decl {
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::fixed(24.0, 24.0))
        .shapes(vec![Shape::Box {
            corners: ui_core::Corners::ZERO,
            fill: ink,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }])
}

fn build(
    store: &mut AnimStore,
    routing: Routing,
    now: f64,
    style: ButtonStyle,
    size: ButtonSize,
    toggle: ButtonToggle,
    icon: bool,
    params: Md3Options,
) -> (Md3Button, Decl) {
    let mut core = ui_core::cx::Cx::new(store, 0, 400, 200, now, 0.0, 1.0, None);
    core.route(routing);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let mark = icon.then_some(ui_widgets::IconSource::Custom(dot));
    let spec = ButtonParams::new(BTN, "Label")
        .style(style)
        .size(size)
        .toggle(toggle);
    let spec = match mark {
        Some(m) => spec.icon(m),
        None => spec,
    };
    let control = Md3Button::new_with(params, spec, &mut cx, &crate::pack::pack());
    let decl = cx.with_cx(|cx| control.decl(cx));
    (control, decl)
}

fn plain(style: ButtonStyle, size: ButtonSize) -> (Md3Button, Decl) {
    let mut store = AnimStore::new();
    build(
        &mut store,
        Routing::default(),
        0.0,
        style,
        size,
        ButtonToggle::Base,
        false,
        Md3Options::default(),
    )
}

#[test]
fn a_click_reports_activation_and_a_disabled_click_reports_nothing() {
    // A button's semantic is activation, not a value; reporting on a disabled control would
    // let the app act on a press the UI said it would ignore.
    let (control, _) = plain(ButtonStyle::Filled, ButtonSize::Small);
    let bounds = Rect::new(0.0, 0.0, 100.0, control.height());
    let click = ui_widgets::WidgetState {
        clicked: true,
        ..Default::default()
    };
    assert!(matches!(
        control.change(&click, bounds),
        Some(ui_core::Change::Activated)
    ));
    let hover = ui_widgets::WidgetState {
        local: Some(ui_core::geom::Point::new(1.0, 1.0)),
        ..Default::default()
    };
    assert!(
        control.change(&hover, bounds).is_none(),
        "hover alone is not activation"
    );
    let params = Md3Options {
        disabled: true,
        ..Md3Options::default()
    };
    let mut store = AnimStore::new();
    let (disabled, _) = build(
        &mut store,
        Routing::default(),
        0.0,
        ButtonStyle::Filled,
        ButtonSize::Small,
        ButtonToggle::Base,
        false,
        params,
    );
    assert!(disabled.change(&click, bounds).is_none());
}
