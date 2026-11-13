// The date picker page.

use crate::shell::*;

/// How many pickers the page drives; each keeps its own app-owned facts.
pub(crate) const DATE_PICKERS: usize = 2;

pub(crate) fn date_picker_id(k: usize) -> Id {
    Id::of(("date-picker", k))
}

/// The text-mode sample: one input, one buffer, one action per click.
pub(crate) fn date_input_id() -> Id {
    Id::str("date-input/sample")
}

/// The demo's "today": a calendar must be told which day it is, and the gallery has no clock.
pub(crate) fn date_picker_today() -> ui_widgets_md3::Date {
    ui_widgets_md3::Date::new(2026, 10, 30).expect("a real date")
}

/// The modal sample's facts: whether it is open, what it holds, and what it is looking at.
#[derive(Clone)]
pub(crate) struct DateModalState {
    pub(crate) open: bool,
    pub(crate) value: Option<ui_widgets_md3::Date>,
    pub(crate) shown: ui_widgets_md3::Date,
    pub(crate) view: ui_widgets_md3::DatePickerView,
}

pub(crate) fn date_modal_id() -> Id {
    Id::str("date-picker/modal")
}

pub(crate) fn date_modal_button_id() -> Id {
    Id::str("date-picker/modal/open")
}

/// The live modal, declared at the window root: the engine resolves its target, so the app
/// names the region instead of measuring it.
pub(crate) fn date_modal_picker(state: &DateModalState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("md3"));
    let appearance = ui_style::StyleScope::of(scoped.env()).appearance;
    let pack = ui_widgets_md3::pack_of(appearance);
    let mut params = ui_widgets_md3::DatePickerParams::new(date_modal_id(), date_picker_today());
    params.value = state.value;
    params.shown = state.shown;
    params.view = state.view;
    params.mode = ui_widgets_md3::DatePickerMode::Modal;
    // Compose always draws the second line: the date once there is one, and its own "no date
    // selected" text before that.
    let headline = match state.value {
        Some(date) => date_text(&scoped, Some(date)),
        None => tr(&scoped, Msg::MD3_DATE_PICKER_NO_SELECTION),
    };
    params.headline = Some(headline.into());
    let picker = ui_widgets_md3::Md3DatePicker::from_pack(pack, &mut scoped, params);
    scoped.element(Box::new(picker))
}

/// The range sample's facts: whether the modal is open, both dates and the first month.
#[derive(Clone)]
pub(crate) struct DateRangeState {
    pub(crate) open: bool,
    pub(crate) start: Option<ui_widgets_md3::Date>,
    pub(crate) end: Option<ui_widgets_md3::Date>,
    pub(crate) shown: ui_widgets_md3::Date,
}

pub(crate) fn date_range_id() -> Id {
    Id::str("date-picker/range")
}

pub(crate) fn date_range_button_id() -> Id {
    Id::str("date-picker/range/open")
}

/// The live range modal, declared at the window root like the single picker's.
pub(crate) fn date_range_picker(state: &DateRangeState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("md3"));
    let appearance = ui_style::StyleScope::of(scoped.env()).appearance;
    let pack = ui_widgets_md3::pack_of(appearance);
    let mut params = ui_widgets_md3::DateRangePickerParams::new(
        date_range_id(),
        state.shown,
        date_picker_today(),
    );
    params.start = state.start;
    params.end = state.end;
    // The app owns the locale: the picker draws whatever the dates are written as.
    params.start_text = state
        .start
        .map(|date| date_text(&scoped, Some(date)).into());
    params.end_text = state.end.map(|date| date_text(&scoped, Some(date)).into());
    let picker = ui_widgets_md3::Md3DateRangePicker::from_pack(pack, &mut scoped, params);
    scoped.element(Box::new(picker))
}

/// A date written out for the readout, in the labels the calendar itself uses.
pub(crate) fn date_text(cx: &Cx<'_, '_>, date: Option<ui_widgets_md3::Date>) -> String {
    // The locale owns the field order: "Nov 9, 2026" here, "2026年11月9日" there.
    date.and_then(|d| cx.date(d.year, d.month, d.day))
        .map_or_else(|| "—".to_string(), |text| text.to_string())
}

/// MD3 date pickers: one live calendar, and one build-time variant (Sunday first, a range).
pub(crate) fn date_picker_page(
    values: [Option<ui_widgets_md3::Date>; DATE_PICKERS],
    shown: [ui_widgets_md3::Date; DATE_PICKERS],
    views: [ui_widgets_md3::DatePickerView; DATE_PICKERS],
    range: &DateRangeState,
    input: &DateInputState<'_>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::DatePicker.tag())), NodeKind::Row)
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

    let view = match views[0] {
        ui_widgets_md3::DatePickerView::Days => Msg::GALLERY_DATE_VIEW_DAYS,
        ui_widgets_md3::DatePickerView::Months => Msg::GALLERY_DATE_VIEW_MONTHS,
        ui_widgets_md3::DatePickerView::Years => Msg::GALLERY_DATE_VIEW_YEARS,
    };
    let readout = tf(
        &scoped,
        Msg::GALLERY_DATE_READOUT,
        &[
            ("today", arg(date_text(&scoped, Some(date_picker_today())))),
            ("picked", arg(date_text(&scoped, values[0]))),
            (
                "month",
                arg(ui_widgets_md3::month_label(&scoped, shown[0].month).to_string()),
            ),
            ("year", arg(shown[0].year.to_string())),
            ("view", arg(tr(&scoped, view))),
        ],
    );
    col = col.child_decl(label(
        Id::str("date-picker/readout"),
        readout,
        12.0,
        muted(),
        740.0,
    ));

    // The modal sample leads the column: its button opens the box over the whole window.
    let open_label = tr(&scoped, Msg::GALLERY_DATE_MODAL_OPEN);
    let open = Md3Kit::new().button(
        ui_widgets_md3::ButtonParams::new(date_modal_button_id(), open_label)
            .style(ui_widgets_md3::ButtonStyle::Filled),
        &mut scoped,
    );
    col = col.child(Box::new(
        Container::new(Id::str("date-picker/modal-block"), NodeKind::Column)
            .gap(6.0)
            .child_decl(label(
                Id::str("date-picker/modal-caption"),
                tr(&scoped, Msg::GALLERY_DATE_MODAL_CAPTION),
                11.0,
                muted(),
                740.0,
            ))
            // A row gives the button its own width instead of the column's.
            .child(Box::new(
                Container::new(Id::str("date-picker/modal-row"), NodeKind::Row)
                    .child(scoped.element(open)),
            )),
    ));

    // The range sample: a second modal over the same window, with its own two dates.
    let range_caption = tr(&scoped, Msg::GALLERY_DATE_RANGE_CAPTION);
    let range_readout = tf(
        &scoped,
        Msg::GALLERY_DATE_RANGE_READOUT,
        &[
            ("start", arg(date_text(&scoped, range.start))),
            ("end", arg(date_text(&scoped, range.end))),
        ],
    );
    let range_open = tr(&scoped, Msg::GALLERY_DATE_RANGE_OPEN);
    let range_button = Md3Kit::new().button(
        ui_widgets_md3::ButtonParams::new(date_range_button_id(), range_open)
            .style(ui_widgets_md3::ButtonStyle::Filled),
        &mut scoped,
    );
    col = col.child(Box::new(
        Container::new(Id::str("date-picker/range-block"), NodeKind::Column)
            .gap(6.0)
            .child_decl(label(
                Id::str("date-picker/range-caption"),
                range_caption,
                11.0,
                muted(),
                740.0,
            ))
            .child_decl(label(
                Id::str("date-picker/range-readout"),
                range_readout,
                11.0,
                muted(),
                740.0,
            ))
            .child(Box::new(
                Container::new(Id::str("date-picker/range-row"), NodeKind::Row)
                    .child(scoped.element(range_button)),
            )),
    ));

    let mut samples = Container::new(Id::str("date-picker/samples"), NodeKind::Row).gap(16.0);
    for k in 0..DATE_PICKERS {
        let mut params =
            ui_widgets_md3::DatePickerParams::new(date_picker_id(k), date_picker_today());
        params.value = values[k];
        params.shown = shown[k];
        params.view = views[k];
        if k == 1 {
            // Fixed when the sample is built: neither knob is a running state.
            params.week_start = ui_widgets_md3::Weekday::Sun;
            params.min = ui_widgets_md3::Date::new(2026, 10, 10);
            params.max = ui_widgets_md3::Date::new(2026, 10, 24);
        }
        let caption = if k == 0 {
            Msg::GALLERY_DATE_CAPTION_MON
        } else {
            Msg::GALLERY_DATE_CAPTION_SUN
        };
        let text = tr(&scoped, caption);
        let picker = ui_widgets_md3::Md3DatePicker::from_pack(pack, &mut scoped, params);
        samples = samples.child(Box::new(
            Container::new(Id::of(("date-picker/block", k)), NodeKind::Column)
                .gap(4.0)
                .child_decl(label(
                    Id::of(("date-picker/cap", k)),
                    text,
                    11.0,
                    muted(),
                    360.0,
                ))
                .child(scoped.element(Box::new(picker))),
        ));
    }
    col = col.child(Box::new(samples));

    row = row.child(column_w(
        Page::DatePicker,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        800.0,
        560.0,
        &scoped,
    ));
    // The input replaces the two placeholder columns: at 328 wide it does not fit beside
    // them, and what they said — no date picker in the other packs — is one line here.
    let input_body = date_input_sample(input, &mut scoped);
    row = row.child(column_w(
        Page::DatePicker,
        "md3-input",
        Msg::GALLERY_DATE_INPUT_TITLE,
        input_body,
        COL_W,
        680.0,
        &scoped,
    ));
    Box::new(row)
}

/// The facts the text-mode sample draws: the buffer the engine edits, and the last action.
pub(crate) struct DateInputState<'a> {
    pub(crate) text: &'a str,
    pub(crate) action: Option<ui_widgets_md3::DateInputAction>,
}

/// The text-mode sample: a readout over the 328×512 input surface.
fn date_input_sample(state: &DateInputState<'_>, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    use ui_widgets_md3::DateInputAction;
    let action = match state.action {
        Some(DateInputAction::Calendar) => Msg::GALLERY_DATE_INPUT_CALENDAR,
        Some(DateInputAction::Cancel) => Msg::GALLERY_DATE_INPUT_CANCEL,
        Some(DateInputAction::Confirm) => Msg::GALLERY_DATE_INPUT_CONFIRM,
        None => Msg::GALLERY_DATE_INPUT_NONE,
    };
    let text = if state.text.is_empty() {
        "—"
    } else {
        state.text
    };
    let readout = tf(
        cx,
        Msg::GALLERY_DATE_INPUT_READOUT,
        &[
            ("text", arg(text.to_string())),
            ("action", arg(tr(cx, action))),
        ],
    );
    let supporting = tr(cx, Msg::GALLERY_DATE_INPUT_SUPPORTING);
    let headline = tr(cx, Msg::GALLERY_DATE_INPUT_HEADLINE);
    let label_text = tr(cx, Msg::GALLERY_DATE_INPUT_LABEL);
    let placeholder = tr(cx, Msg::GALLERY_DATE_INPUT_PLACEHOLDER);
    let hint = tr(cx, Msg::GALLERY_DATE_INPUT_HINT);
    let cancel = tr(cx, Msg::GALLERY_DATE_INPUT_CANCEL_LABEL);
    let confirm = tr(cx, Msg::GALLERY_DATE_INPUT_CONFIRM_LABEL);
    let mut params = ui_widgets_md3::DateInputParams::new(date_input_id());
    params.text = state.text;
    params.supporting = &supporting;
    params.headline = &headline;
    params.label = &label_text;
    params.placeholder = &placeholder;
    params.supporting_field = &hint;
    params.cancel = &cancel;
    params.confirm = &confirm;
    let appearance = ui_style::StyleScope::of(cx.env()).appearance;
    let pack = ui_widgets_md3::pack_of(appearance);
    let input = ui_widgets_md3::Md3DateInput::from_pack(pack, cx, params);
    let note = tr(cx, Msg::GALLERY_DATE_INPUT_PENDING);
    let mut body = Container::new(Id::str("date-input/body"), NodeKind::Column).gap(8.0);
    body = body
        .child_decl(label(
            Id::str("date-input/readout"),
            readout,
            12.0,
            muted(),
            COL_W - 24.0,
        ))
        .child(cx.element(Box::new(input)))
        .child_decl(label(
            Id::str("date-input/note"),
            note,
            11.0,
            muted(),
            COL_W - 24.0,
        ));
    Box::new(body)
}
