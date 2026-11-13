// The slider page.

use crate::shell::*;

pub(crate) fn slider_id(page: Page, tag: &'static str) -> Id {
    Id::of(("slider", tag, page.tag()))
}

/// The MD3 slider's five expressive sizes; each is a construction-time variant.
pub(crate) const SLIDER_SIZES: [ui_widgets_md3::spec::SliderSize; 5] = [
    ui_widgets_md3::spec::SliderSize::XSmall,
    ui_widgets_md3::spec::SliderSize::Small,
    ui_widgets_md3::spec::SliderSize::Medium,
    ui_widgets_md3::spec::SliderSize::Large,
    ui_widgets_md3::spec::SliderSize::XLarge,
];

/// The width every MD3 sample slider is built at; the selftest reads the real rect instead.
pub(crate) const SLIDER_SAMPLE_W: f32 = 320.0;

pub(crate) fn slider_size_id(k: usize) -> Id {
    Id::of(("slider-size", k))
}

/// The MD3 page has one slider per variant, all reporting into the same app value, so they
/// all count as the first column.
pub(crate) fn slider_column_of(on: Id) -> Option<usize> {
    if let Some(i) = STYLES
        .iter()
        .position(|(tag, _)| slider_id(Page::Slider, tag) == on)
    {
        return Some(i);
    }
    let md3 =
        (0..SLIDER_SIZES.len()).any(|k| slider_size_id(k) == on) || on == slider_disabled_id();
    md3.then_some(0)
}

pub(crate) fn slider_disabled_id() -> Id {
    Id::str("slider/disabled")
}

/// The stepped sample, the range sample, and the stepped range sample.
pub(crate) fn slider_step_id() -> Id {
    Id::str("slider/step")
}

pub(crate) fn slider_range_id() -> Id {
    Id::str("slider/range")
}

pub(crate) fn slider_range_step_id() -> Id {
    Id::str("slider/range-step")
}

/// The stop count both stepped samples use.
pub(crate) const SLIDER_STOPS: f32 = 4.0;

/// The MD3 slider's construction-time variants: the five expressive sizes plus a disabled
/// sample. The value is live and shared, so dragging any row moves all of them.
pub(crate) fn slider_page(
    styles: &[(&dyn StyleKit, &'static str, Msg); 3],
    value: f32,
    step_value: f32,
    range: (f32, f32),
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Slider.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(10.0);
    for (k, size) in SLIDER_SIZES.iter().enumerate() {
        let id = if k == 0 {
            slider_id(Page::Slider, "md3")
        } else {
            slider_size_id(k)
        };
        let spec = ui_widgets_md3::spec::slider_spec(
            ui_widgets_md3::spec::SliderFlags {
                active: true,
                ..ui_widgets_md3::spec::SliderFlags::NONE
            },
            *size,
        );
        let mut scoped = cx.at_scope(style_env("md3"));
        let control = style.slider_sized(id, value, SLIDER_SAMPLE_W, *size, &mut scoped);
        let caption = tf(
            &scoped,
            Msg::GALLERY_SLIDER_SIZE_CAPTION,
            &[
                ("size", arg(format!("{size:?}"))),
                ("track", arg(format!("{:.0}", spec.track.height))),
                ("handle", arg(format!("{:.0}", spec.handle.height))),
            ],
        );
        col = col.child(Box::new(captioned_row(
            id,
            scoped.element(control),
            caption,
        )));
    }
    // `disabled` is a build-time knob: the state layer and the inks both come from the tokens.
    let mut params = ui_widgets_md3::Md3Options::default();
    params.disabled = true;
    let disabled_style = Md3Kit::with_scope(ui_style::StyleScope::default(), params);
    let mut scoped = cx.at_scope(Env::with(Some(root_env()), params));
    let control = disabled_style.slider_sized(
        slider_disabled_id(),
        value,
        SLIDER_SAMPLE_W,
        ui_widgets_md3::spec::SliderSize::XSmall,
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        slider_disabled_id(),
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_SLIDER_DISABLED_CAPTION),
    )));
    // Discrete and range are construction-time too: the stop count and the second handle
    // are decided when the control is built, not by any runtime state.
    let mut scoped = cx.at_scope(style_env("md3"));
    let step = ui_widgets_md3::SliderOpts {
        step: Some(1.0 / SLIDER_STOPS),
        ticks: true,
        end: None,
    };
    let control = style.slider_opts(
        slider_step_id(),
        step_value,
        SLIDER_SAMPLE_W,
        ui_widgets_md3::spec::SliderSize::Small,
        step,
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        slider_step_id(),
        scoped.element(control),
        tf(
            &scoped,
            Msg::GALLERY_SLIDER_STEPPED_CAPTION,
            &[("stops", arg(SLIDER_STOPS as i32 + 1))],
        ),
    )));
    let mut scoped = cx.at_scope(style_env("md3"));
    let control = style.slider_opts(
        slider_range_id(),
        range.0,
        SLIDER_SAMPLE_W,
        ui_widgets_md3::spec::SliderSize::Small,
        ui_widgets_md3::SliderOpts {
            end: Some(range.1),
            ..Default::default()
        },
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        slider_range_id(),
        scoped.element(control),
        tf(
            &scoped,
            Msg::GALLERY_SLIDER_RANGE_CAPTION,
            &[
                ("start", arg(format!("{:.0}", range.0 * 100.0))),
                ("end", arg(format!("{:.0}", range.1 * 100.0))),
            ],
        ),
    )));
    let mut scoped = cx.at_scope(style_env("md3"));
    let control = style.slider_opts(
        slider_range_step_id(),
        range.0,
        SLIDER_SAMPLE_W,
        ui_widgets_md3::spec::SliderSize::Small,
        ui_widgets_md3::SliderOpts {
            step: Some(1.0 / SLIDER_STOPS),
            ticks: true,
            end: Some(range.1),
        },
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        slider_range_step_id(),
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_SLIDER_RANGE_STEP_CAPTION),
    )));
    col = col.child_decl(label(
        Id::str("slider/readout"),
        tf(
            &scoped,
            Msg::GALLERY_SLIDER_READOUT,
            &[("value", arg(format!("{:.1}", value * 100.0)))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Slider,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        &scoped,
    ));
    for (style, tag, title) in styles.iter().filter(|(_, tag, _)| *tag != "md3") {
        let mut scoped = cx.at_scope(style_env(tag));
        let control = style.slider(
            &SliderSpec::new(slider_id(Page::Slider, tag), value, ""),
            &mut scoped,
        );
        let body = Container::new(Id::of(("body", *tag)), NodeKind::Column)
            .gap(10.0)
            .child(scoped.element(control))
            .child_decl(label(
                Id::of(("readout", *tag)),
                tf(
                    &scoped,
                    Msg::GALLERY_SLIDER_VALUE,
                    &[("value", arg(format!("{:.1}", value * 100.0)))],
                ),
                12.0,
                muted(),
                300.0,
            ));
        row = row.child(column(
            Page::Slider,
            tag,
            *title,
            Box::new(body),
            COL_CTRL_H,
            &scoped,
        ));
    }
    Box::new(row)
}
