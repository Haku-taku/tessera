// The time picker page.

use crate::shell::*;

/// How many time inputs the page drives; each keeps its own buffers.
pub(crate) const TIME_PICKERS: usize = 2;

pub(crate) fn time_picker_id(k: usize) -> Id {
    Id::of(("time-picker", k))
}

/// MD3 time inputs: one live 12-hour clock and one built disabled.
pub(crate) fn time_picker_page(
    hours: &[String; TIME_PICKERS],
    minutes: &[String; TIME_PICKERS],
    periods: [ui_widgets_md3::Period; TIME_PICKERS],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::TimePicker.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let appearance = ui_style::StyleScope::of(cx.env()).appearance;
    let pack = ui_widgets_md3::pack_of(appearance);
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(8.0);

    // The 24-hour value is the one fact the widget never sees; deriving it here shows both.
    let hour24 = periods[0].hour24(ui_widgets_md3::parse_hour(&hours[0]).unwrap_or(12));
    let readout = format!(
        "typed {}:{} → 24-hour {hour24:02}:{} · period: {:?}",
        hours[0],
        minutes[0],
        ui_widgets_md3::parse_minute(&minutes[0]).unwrap_or(0),
        periods[0]
    );
    col = col.child_decl(label(
        Id::str("time-picker/readout"),
        readout,
        12.0,
        muted(),
        740.0,
    ));

    let mut samples = Container::new(Id::str("time-picker/samples"), NodeKind::Row).gap(16.0);
    for k in 0..TIME_PICKERS {
        let mut params = ui_widgets_md3::TimePickerParams::new(
            time_picker_id(k),
            &hours[k],
            &minutes[k],
            periods[k],
        );
        // Fixed when the sample is built: the second one never takes input.
        params.disabled = k == 1;
        let caption = if k == 0 {
            Msg::GALLERY_TIME_CAPTION_LIVE
        } else {
            Msg::GALLERY_TIME_CAPTION_DISABLED
        };
        let text = tr(&scoped, caption);
        let picker = ui_widgets_md3::Md3TimeInput::from_pack(&pack, &mut scoped, params);
        samples = samples.child(Box::new(
            Container::new(Id::of(("time-picker/block", k)), NodeKind::Column)
                .gap(4.0)
                .child_decl(label(
                    Id::of(("time-picker/cap", k)),
                    text,
                    11.0,
                    muted(),
                    340.0,
                ))
                .child(scoped.element(Box::new(picker))),
        ));
    }
    col = col.child(Box::new(samples));

    row = row.child(column_w(
        Page::TimePicker,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        800.0,
        320.0,
        &scoped,
    ));
    for tag in ["fluent", "glass"] {
        row = row.child(pending_column(
            Page::TimePicker,
            tag,
            190.0,
            COL_CTRL_H,
            &scoped,
        ));
    }
    Box::new(row)
}
