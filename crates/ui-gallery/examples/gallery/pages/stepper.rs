// The stepper page.

use ui_widgets_glass::{GlassStepper, GlassStepperParams};

use crate::shell::*;

/// The stepping stepper, the wrapping one, the disabled one, and the label beside them.
pub(crate) fn stepper_id() -> Id {
    Id::of(("stepper", Page::Stepper.tag()))
}

pub(crate) fn stepper_wrap_id() -> Id {
    stepper_id().with("wrap")
}

pub(crate) fn stepper_disabled_id() -> Id {
    stepper_id().with("disabled")
}

pub(crate) fn stepper_value_id() -> Id {
    stepper_id().with("value")
}

/// One glass stepper: the reference's capsule at its own range.
fn stepper(
    id: Id,
    value: f64,
    min: f64,
    max: f64,
    wraps: bool,
    disabled: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("glass"));
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut params = GlassStepperParams::new(id, value)
        .range(min, max)
        .wraps(wraps);
    if disabled {
        params = params.disabled();
    }
    let control = GlassStepper::new(params, &mut scoped, &pack);
    scoped.element(Box::new(control))
}

/// The page: the value in the app's own label, and three steppers with different ranges.
pub(crate) fn stepper_page(
    value: f64,
    wrap: f64,
    presses: u32,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Stepper.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    // MD3 and Fluent have no stepper in this repository yet.
    for tag in ["md3", "fluent"] {
        row = row.child(pending_column(Page::Stepper, tag, COL_W, COL_TALL_H, cx));
    }

    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let ink = pack
        .ink("textFillColorPrimary")
        .unwrap_or(ui_core::geom::Color::BLACK);
    let mut col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);

    // ── The value lives in the app's label, as iOS 26's `UIStepper` leaves it to the app.
    let mut pair = Container::new(stepper_id().with("pair"), NodeKind::Row)
        .gap(16.0)
        .cross_align(0.0)
        .cross_pos(0.5);
    pair = pair.child_decl(label(
        stepper_value_id(),
        format!("{value:.0}"),
        22.0,
        ink,
        60.0,
    ));
    pair = pair.child(stepper(stepper_id(), value, 0.0, 10.0, false, false, cx));
    col = col.child(Box::new(captioned_row(
        stepper_id().with("cap"),
        Box::new(pair),
        tr(cx, Msg::GALLERY_STEPPER_CAPTION),
    )));

    col = col.child(Box::new(captioned_row(
        stepper_wrap_id(),
        stepper(stepper_wrap_id(), wrap, 0.0, 5.0, true, false, cx),
        tr(cx, Msg::GALLERY_STEPPER_CAPTION_WRAP),
    )));
    col = col.child(Box::new(captioned_row(
        stepper_disabled_id(),
        stepper(stepper_disabled_id(), 3.0, 0.0, 10.0, false, true, cx),
        tr(cx, Msg::GALLERY_STEPPER_CAPTION_DISABLED),
    )));
    col = col.child_decl(label(
        Id::str("stepper/readout"),
        tf(
            cx,
            Msg::GALLERY_STEPPER_READOUT,
            &[
                ("value", arg(format!("{value:.0}"))),
                ("wrap", arg(format!("{wrap:.0}"))),
                ("count", arg(presses as i64)),
            ],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Stepper,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
