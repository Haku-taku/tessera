//! MD3 date picker: inline on the page, or centred over a scrim as a modal dialog.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, OverlayAlign, SizeClass};
use ui_core::tree::{ModalTarget, Presence, ShadowSpec};
use ui_core::{Change, CursorShape, Decl, Id, InkRadius, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Icon, Widget, WidgetState};

use crate::dialog::{ENTER, LEAVE, SCRIM_OPACITY};
use crate::layer::{Background, press_ink};
use crate::paint::spec_ink;
use crate::spec::{DatePickerFlags, date_picker_modal_spec, date_picker_spec, divider_spec};
use crate::typescale::{self, TypeStyle};

/// `container.width`: the docked surface's box width.
pub const DATE_PICKER_WIDTH: f32 = 360.0;
/// `container.height`: the docked surface's box height.
pub const DATE_PICKER_HEIGHT: f32 = 456.0;
/// `header.height`.
pub const DATE_PICKER_HEADER: f32 = 64.0;
/// `menu-button.container.height`: the month, year and arrow buttons.
pub const DATE_PICKER_BUTTON: f32 = 40.0;
/// `date.container.width`: one day cell.
pub const DATE_PICKER_CELL: f32 = 48.0;
/// `date.state-layer.width`: the disc inside a day cell.
pub const DATE_PICKER_DISC: f32 = 40.0;
/// The surface's corner: MD3's extra-large shape.
pub const DATE_PICKER_RADIUS: f32 = 28.0;
/// Side inset of the grid and of the weekday row.
pub const DATE_PICKER_SIDE: f32 = 12.0;
/// The grid is six weeks tall in every month, so no month resizes the surface.
pub const DATE_PICKER_WEEKS: usize = 6;
/// `date.disabled` has no token in the export; 38% is MD3's disabled label strength. The
/// export dims outside-month days to the same 38%, so this build keeps their role instead.
pub const DATE_PICKER_DISABLED_OPACITY: f32 = 0.38;

/// Compose's `MonthYearHeight`: the modal month row, taller than the docked one.
pub const DATE_PICKER_MODAL_ROW: f32 = 56.0;
/// The modal header's 120 dp: 16 top, 20 title, 32 gap, 40 headline, 11 gap and a 1 dp line.
const MODAL_TITLE_TOP: f32 = 16.0;
const MODAL_HEADLINE_TOP: f32 = 68.0;
const MODAL_BOTTOM: f32 = 12.0;
const MODAL_SIDE: f32 = 24.0;
const MODAL_END: f32 = 12.0;

use crate::motion::REACTION;

/// The id of the cell standing for `date` inside the picker `picker`: the derivation lives
/// here, so a caller that only has the two ids can address the cell without guessing.
pub fn day_cell_id(picker: Id, date: Date) -> Id {
    Id::of((&picker, "day", date.year, date.month, date.day))
}

/// The id of the month cell for `month` (1–12) in the picker `picker`.
pub fn month_cell_id(picker: Id, month: u8) -> Id {
    Id::of((&picker, "month-cell", month))
}

/// The id of the year cell for `year` in the picker `picker`.
pub fn year_cell_id(picker: Id, year: i32) -> Id {
    Id::of((&picker, "year-cell", year))
}

/// A civil date: no time, no zone, no clock. Months and days are 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    /// 1–12.
    pub month: u8,
    /// 1–31, bounded by the month.
    pub day: u8,
}

impl Date {
    /// A date, or `None` when that month or day cannot exist.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        ((1..=12).contains(&month) && day >= 1 && day <= Self::days_in_month(year, month))
            .then_some(Self { year, month, day })
    }

    /// The Gregorian length of a month, leap years included.
    pub fn days_in_month(year: i32, month: u8) -> u8 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
            2 => 28,
            _ => 0,
        }
    }

    /// Days since 1970-01-01, by Howard Hinnant's `days_from_civil`.
    fn days_since_epoch(self) -> i64 {
        let y = if self.month <= 2 {
            self.year - 1
        } else {
            self.year
        } as i64;
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (self.month as i64 + 9) % 12;
        let doy = (153 * mp + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The weekday this date falls on.
    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday, which is index 3 counted from Monday.
        Weekday::ALL[(((self.days_since_epoch() + 3) % 7 + 7) % 7) as usize]
    }

    /// The same month, on its first day.
    pub fn first_of_month(self) -> Date {
        Date { day: 1, ..self }
    }

    /// The month `delta` months away, the day clamped into whatever the target month allows.
    pub fn add_months(self, delta: i32) -> Date {
        let zero = self.year * 12 + self.month as i32 - 1 + delta;
        let year = zero.div_euclid(12);
        let month = zero.rem_euclid(12) as u8 + 1;
        let day = self.day.min(Self::days_in_month(year, month));
        Date { year, month, day }
    }
}

/// A day of the week; the calendar's columns start at whichever one the app picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    /// Monday first, which is also the index order.
    pub const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    /// The 0-based index of `self` counted from Monday.
    pub fn index(self) -> u8 {
        Weekday::ALL.iter().position(|w| *w == self).unwrap_or(0) as u8
    }

    /// This weekday's 0-based column when the week starts at `start`.
    pub fn column(self, start: Weekday) -> u8 {
        (self.index() + 7 - start.index()) % 7
    }
}

/// What the body of the picker shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePickerView {
    /// The 6×7 day grid.
    Days,
    /// Twelve month names.
    Months,
    /// Twelve years.
    Years,
}

/// Where the picker sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DatePickerMode {
    /// Inline: the surface is the widget.
    #[default]
    Docked,
    /// In a dialog: a scrim across the window, the surface centred above it.
    Modal,
}

/// The words a calendar cannot invent: month names, weekday initials and the title.
///
/// The app owns them so they can come from Fluent; the widget only draws what it is given.
#[derive(Clone, Debug)]
pub struct CalendarLabels {
    /// January first.
    pub months: [std::sync::Arc<str>; 12],
    /// Monday first, matching [`Weekday::ALL`].
    pub weekdays: [std::sync::Arc<str>; 7],
    pub header: std::sync::Arc<str>,
}

impl CalendarLabels {
    /// Read every label from the catalog in force.
    pub fn from_cx(cx: &Cx<'_, '_>) -> Self {
        Self {
            months: [
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_JANUARY),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_FEBRUARY),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_MARCH),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_APRIL),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_MAY),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_JUNE),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_JULY),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_AUGUST),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_SEPTEMBER),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_OCTOBER),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_NOVEMBER),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_MONTH_DECEMBER),
            ],
            weekdays: [
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_WEEKDAY_MON),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_WEEKDAY_TUE),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_WEEKDAY_WED),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_WEEKDAY_THU),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_WEEKDAY_FRI),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_WEEKDAY_SAT),
                cx.t(ui_i18n::Msg::MD3_DATE_PICKER_WEEKDAY_SUN),
            ],
            header: cx.t(ui_i18n::Msg::MD3_DATE_PICKER_HEADER),
        }
    }

    /// The name of a 1-based month.
    pub fn month(&self, month: u8) -> &str {
        &self.months[(month.clamp(1, 12) - 1) as usize]
    }
}

/// The catalog's name for a 1-based month, for readouts beside a picker.
pub fn month_label(cx: &Cx<'_, '_>, month: u8) -> std::sync::Arc<str> {
    cx.t(month_msg(month))
}

fn month_msg(month: u8) -> ui_i18n::Msg {
    use ui_i18n::Msg;
    match month.clamp(1, 12) {
        1 => Msg::MD3_DATE_PICKER_MONTH_JANUARY,
        2 => Msg::MD3_DATE_PICKER_MONTH_FEBRUARY,
        3 => Msg::MD3_DATE_PICKER_MONTH_MARCH,
        4 => Msg::MD3_DATE_PICKER_MONTH_APRIL,
        5 => Msg::MD3_DATE_PICKER_MONTH_MAY,
        6 => Msg::MD3_DATE_PICKER_MONTH_JUNE,
        7 => Msg::MD3_DATE_PICKER_MONTH_JULY,
        8 => Msg::MD3_DATE_PICKER_MONTH_AUGUST,
        9 => Msg::MD3_DATE_PICKER_MONTH_SEPTEMBER,
        10 => Msg::MD3_DATE_PICKER_MONTH_OCTOBER,
        11 => Msg::MD3_DATE_PICKER_MONTH_NOVEMBER,
        _ => Msg::MD3_DATE_PICKER_MONTH_DECEMBER,
    }
}

/// Everything the picker is built with; all of it is the app's, none of it is the widget's.
pub struct DatePickerParams {
    pub id: Id,
    /// The chosen date, drawn filled.
    pub value: Option<Date>,
    /// The month on screen.
    pub shown: Date,
    /// The day drawn with an outline.
    pub today: Date,
    pub view: DatePickerView,
    /// Labels to draw; `None` reads them from the catalog in force.
    pub labels: Option<CalendarLabels>,
    /// Earliest selectable day; cells before it are inert.
    pub min: Option<Date>,
    pub max: Option<Date>,
    /// The column the week starts on.
    pub week_start: Weekday,
    /// Inline on the page or in a dialog over it.
    pub mode: DatePickerMode,
    /// The date written out for the modal header's second line; the app owns the locale.
    pub headline: Option<std::sync::Arc<str>>,
}

impl DatePickerParams {
    /// A picker showing `today`'s month with nothing chosen.
    pub fn new(id: Id, today: Date) -> Self {
        Self {
            id,
            value: None,
            shown: today.first_of_month(),
            today,
            view: DatePickerView::Days,
            labels: None,
            min: None,
            max: None,
            week_start: Weekday::Mon,
            mode: DatePickerMode::Docked,
            headline: None,
        }
    }
}

/// What the picker asks its app to remember.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePickerChange {
    /// A day was chosen.
    Picked(Date),
    /// The month on screen moved, and the body may switch view in the same step.
    Shown { date: Date, view: DatePickerView },
    /// The body switched view without moving the month.
    View(DatePickerView),
    /// A modal picker's scrim was pressed; closing it is the app's decision.
    Dismissed,
}

/// One action the user took this frame, found at build time from the cells' own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Day(Date),
    Prev,
    Next,
    MonthButton,
    YearButton,
    Month(u8),
    Year(i32),
}

/// The inks this frame resolves to, so assembling the decl repeats no token lookups.
#[derive(Clone, Copy)]
struct Palette {
    surface: Color,
    on_surface: Color,
    on_surface_variant: Color,
    primary: Color,
    on_primary: Color,
    /// The strength an unselectable day's label keeps.
    disabled_opacity: f32,
    /// `date.hover.state-layer.opacity`.
    hover_opacity: f32,
    /// `date.pressed.state-layer.opacity`.
    press_opacity: f32,
    elevation: f32,
    cell: f32,
    disc: f32,
    button: f32,
    day_label: TypeStyle,
    menu_label: TypeStyle,
    weekday_label: TypeStyle,
    /// The modal form's own box, header and hairline; `None` paints the docked form.
    modal: Option<ModalChrome>,
}

/// What the modal form adds over the docked one: a taller box and a two-line header.
#[derive(Clone, Copy)]
struct ModalChrome {
    width: f32,
    height: f32,
    surface: Color,
    elevation: f32,
    scrim: Color,
    header: f32,
    row: f32,
    body_top: f32,
    body_bottom: f32,
    ink: Color,
    title: TypeStyle,
    headline: TypeStyle,
    title_line: f32,
    headline_line: f32,
    divider: (Color, f32),
}

/// One day cell, resolved.
struct DayCell {
    id: Id,
    text: String,
    selected: bool,
    today: bool,
    enabled: bool,
    ink: Color,
    layer: Option<Color>,
    ring: Option<Color>,
}

/// One cell of the month or year grid, resolved.
struct ChoiceCell {
    id: Id,
    text: String,
    chosen: bool,
    width: f32,
    layer: Option<Color>,
    ring: Option<Color>,
}

/// One of the 40dp buttons, resolved.
struct Button {
    id: Id,
    text: Option<String>,
    icon: Option<&'static str>,
    /// Already dimmed when the view stands the button down.
    ink: Color,
    enabled: bool,
    layer: Option<Color>,
    ring: Option<Color>,
}

/// A docked MD3 date picker; the app owns the chosen date, the shown month and the view.
pub struct Md3DatePicker {
    id: Id,
    /// The picker's own box: the widget itself when docked, a child of the scrim when modal.
    surface_id: Id,
    headline: Option<std::sync::Arc<str>>,
    dismissed: bool,
    shown: Date,
    view: DatePickerView,
    labels: CalendarLabels,
    week_start: Weekday,
    fired: Option<Action>,
    disabled: bool,
    palette: Palette,
    line: LineHeights,
    weekdays: Vec<(Id, String)>,
    days: Vec<DayCell>,
    months: Vec<ChoiceCell>,
    years: Vec<ChoiceCell>,
    month_button: Button,
    year_button: Button,
    prev: Button,
    next: Button,
}

/// The measured height of one line at each size the picker uses, for vertical centring.
#[derive(Clone, Copy)]
struct LineHeights {
    day: f32,
    menu: f32,
    weekday: f32,
    header: f32,
}

impl Md3DatePicker {
    /// Resolve this frame's control: palette, every cell's paint, and the one action taken.
    pub fn from_pack(pack: &Style, cx: &mut Cx<'_, '_>, params: DatePickerParams) -> Self {
        let base = date_picker_spec(DatePickerFlags::NONE);
        let id = params.id;
        let modal = (params.mode == DatePickerMode::Modal).then(|| {
            let m = date_picker_modal_spec();
            let divider = divider_spec();
            let title = TypeStyle {
                size: m.header_supporting_text.size,
                weight: Weight(m.header_supporting_text.weight as u16),
            };
            let headline = TypeStyle {
                size: m.header_headline.size,
                weight: Weight(m.header_headline.weight as u16),
            };
            ModalChrome {
                width: m.container.width,
                height: m.container.height,
                surface: spec_ink(pack, m.container.color, 1.0),
                elevation: m.container.elevation,
                // `showDialog`'s barrier is `Colors.black54`; the dialog uses the same dim.
                scrim: spec_ink(pack, "scrim", SCRIM_OPACITY),
                header: m.header_container.height,
                row: DATE_PICKER_MODAL_ROW,
                body_top: 0.0,
                body_bottom: MODAL_BOTTOM,
                ink: spec_ink(pack, m.header_headline.color, 1.0),
                title,
                headline,
                title_line: measure_line(title.size),
                headline_line: measure_line(headline.size),
                divider: (
                    spec_ink(pack, divider.root.color, 1.0),
                    divider.root.thickness,
                ),
            }
        });
        let surface_id = if modal.is_some() {
            id.with("surface")
        } else {
            id
        };
        let selected = date_picker_spec(DatePickerFlags {
            selected: true,
            ..DatePickerFlags::NONE
        });
        let hover = date_picker_spec(DatePickerFlags {
            hovered: true,
            ..DatePickerFlags::NONE
        });
        let press = date_picker_spec(DatePickerFlags {
            pressed: true,
            ..DatePickerFlags::NONE
        });
        let palette = Palette {
            surface: modal.map_or_else(|| spec_ink(pack, base.container.color, 1.0), |m| m.surface),
            on_surface: spec_ink(pack, base.date_label_text.color, 1.0),
            on_surface_variant: spec_ink(pack, hover.date_state_layer.color, 1.0),
            primary: spec_ink(pack, selected.date_container.color, 1.0),
            on_primary: spec_ink(pack, selected.date_label_text.color, 1.0),
            disabled_opacity: DATE_PICKER_DISABLED_OPACITY,
            hover_opacity: hover.date_state_layer.opacity,
            press_opacity: press.date_state_layer.opacity,
            elevation: modal.map_or(base.container.elevation, |m| m.elevation),
            cell: base.date_container.width.max(DATE_PICKER_CELL),
            disc: base.date_state_layer.width.max(DATE_PICKER_DISC),
            button: base.menu_button_container.height.max(DATE_PICKER_BUTTON),
            day_label: TypeStyle {
                size: base.date_label_text.size,
                weight: Weight(base.date_label_text.weight as u16),
            },
            menu_label: TypeStyle {
                size: base.menu_button_label_text.size,
                weight: Weight(base.menu_button_label_text.weight as u16),
            },
            weekday_label: TypeStyle {
                size: base.weekdays_label_text.size,
                weight: Weight(base.weekdays_label_text.weight as u16),
            },
            modal,
        };
        let line = LineHeights {
            day: measure_line(palette.day_label.size),
            menu: measure_line(palette.menu_label.size),
            weekday: measure_line(palette.weekday_label.size),
            header: measure_line(typescale::LABEL_MEDIUM.size),
        };
        let labels = params
            .labels
            .clone()
            .unwrap_or_else(|| CalendarLabels::from_cx(cx));
        // A press is a one-frame fact: read it here, since `change()` sees no context. Only a
        // press outside the surface counts, and only the modal form has anything outside it.
        let dismissed = modal.is_some() && cx.state(id).clicked && !cx.state(surface_id).clicked;
        let mut me = Self {
            id,
            surface_id,
            headline: params.headline.clone(),
            dismissed,
            shown: params.shown.first_of_month(),
            view: params.view,
            labels,
            week_start: params.week_start,
            fired: None,
            disabled: cx.state(id).disabled,
            palette,
            line,
            weekdays: Vec::with_capacity(7),
            days: Vec::with_capacity(DATE_PICKER_WEEKS * 7),
            months: Vec::with_capacity(12),
            years: Vec::with_capacity(12),
            month_button: Button::placeholder(id.with("month")),
            year_button: Button::placeholder(id.with("year")),
            prev: Button::placeholder(id.with("prev")),
            next: Button::placeholder(id.with("next")),
        };
        me.resolve(cx, &params);
        me
    }

    /// The ids a caller (or a test) can address.
    pub fn day_id(&self, date: Date) -> Id {
        day_cell_id(self.id, date)
    }

    pub fn prev_id(&self) -> Id {
        self.prev.id
    }

    pub fn next_id(&self) -> Id {
        self.next.id
    }

    pub fn month_button_id(&self) -> Id {
        self.month_button.id
    }

    pub fn year_button_id(&self) -> Id {
        self.year_button.id
    }

    pub fn month_cell_id(&self, month: u8) -> Id {
        month_cell_id(self.id, month)
    }

    pub fn year_cell_id(&self, year: i32) -> Id {
        year_cell_id(self.id, year)
    }

    /// The month on screen, as the widget resolved it.
    pub fn shown(&self) -> Date {
        self.shown
    }

    /// Whether a day may be chosen at all.
    fn selectable(date: Date, min: Option<Date>, max: Option<Date>) -> bool {
        min.is_none_or(|m| date >= m) && max.is_none_or(|m| date <= m)
    }

    /// The 42 cells of the grid: the shown month, the tail of the previous month and the head
    /// of the next, so the surface keeps one height.
    pub(crate) fn grid(&self) -> Vec<(Date, bool)> {
        let first = self.shown.first_of_month();
        let lead = first.weekday().column(self.week_start) as u32;
        let days = Date::days_in_month(first.year, first.month) as u32;
        let prev = first.add_months(-1);
        let prev_days = Date::days_in_month(prev.year, prev.month) as u32;
        let next = first.add_months(1);
        let total = (DATE_PICKER_WEEKS * 7) as u32;
        let mut out = Vec::with_capacity(total as usize);
        for i in 0..lead {
            let day = (prev_days - lead + i + 1) as u8;
            out.push((
                Date {
                    year: prev.year,
                    month: prev.month,
                    day,
                },
                false,
            ));
        }
        for day in 1..=days {
            out.push((
                Date {
                    year: first.year,
                    month: first.month,
                    day: day as u8,
                },
                true,
            ));
        }
        let mut day = 1;
        while (out.len() as u32) < total {
            out.push((
                Date {
                    year: next.year,
                    month: next.month,
                    day,
                },
                false,
            ));
            day += 1;
        }
        out
    }

    /// The twelve years the year view offers: one fixed block, so picking one does not move it.
    fn year_block(&self) -> i32 {
        self.shown.year.div_euclid(12) * 12
    }

    /// How far the arrows move in this view: one month, or one block of twelve years.
    fn step(&self) -> i32 {
        match self.view {
            DatePickerView::Days => 1,
            // The arrows stand down while the month grid is shown, so this is never used.
            DatePickerView::Months => 0,
            DatePickerView::Years => 12 * 12,
        }
    }

    /// The state layer's ink, or nothing when no pointer is on the cell; the shape it fills
    /// is the cell's own effective background.
    fn layer(&self, cx: &mut Cx<'_, '_>, id: Id, ink: Color) -> Option<Color> {
        let p = self.palette;
        let state = cx.state(id);
        if self.disabled || !(state.hovered || state.pressed || state.focus_visible) {
            return None;
        }
        let mut node = cx.at(id);
        let hover = node.animate(
            id.with(param::HOVER),
            if state.hovered { 1.0 } else { 0.0 },
            REACTION,
        );
        let press = node.animate(
            id.with(param::PRESS),
            if state.pressed { 1.0 } else { 0.0 },
            REACTION,
        );
        let fade = if state.pressed {
            p.press_opacity
        } else {
            (hover * p.hover_opacity).max(press * p.press_opacity)
        };
        (fade > 0.0).then_some(Color {
            a: ink.a * fade,
            ..ink
        })
    }

    /// Every resolved piece of the frame, and the one action taken.
    fn resolve(&mut self, cx: &mut Cx<'_, '_>, params: &DatePickerParams) {
        let p = self.palette;
        let id = self.id;
        let (min, max) = (params.min, params.max);
        let value = params.value;

        for (date, inside) in self.grid() {
            let cell_id = self.day_id(date);
            let selected = value == Some(date);
            let today = date == params.today;
            let enabled = Self::selectable(date, min, max) && !self.disabled;
            let layer_ink = if selected {
                p.on_primary
            } else if today {
                p.primary
            } else {
                p.on_surface_variant
            };
            // Two independent channels: the role says which month a day belongs to, the
            // alpha says whether it can be picked — so no category borrows another's look.
            let role = if inside {
                p.on_surface
            } else {
                p.on_surface_variant
            };
            let ink = if selected {
                p.on_primary
            } else if !enabled {
                Color {
                    a: role.a * p.disabled_opacity,
                    ..role
                }
            } else if today {
                p.primary
            } else {
                role
            };
            let layer = if enabled {
                self.layer(cx, cell_id, layer_ink)
            } else {
                None
            };
            let state = cx.state(cell_id);
            if state.activation() && enabled {
                self.fired = Some(Action::Day(date));
            }
            self.days.push(DayCell {
                id: cell_id,
                text: date.day.to_string(),
                selected,
                today,
                enabled,
                ink,
                layer,
                ring: state.focus_visible.then_some(p.on_surface),
            });
        }

        for i in 0..7u8 {
            let weekday = Weekday::ALL[((self.week_start.index() + i) % 7) as usize];
            let text = self.labels.weekdays[weekday.index() as usize].to_string();
            self.weekdays.push((Id::of((&id, "weekday", i)), text));
        }

        let wide = (DATE_PICKER_WIDTH - 2.0 * DATE_PICKER_SIDE) / 3.0;
        for month in 1..=12u8 {
            let cell_id = self.month_cell_id(month);
            let chosen = value.is_some_and(|v| v.year == self.shown.year && v.month == month);
            let layer_ink = if chosen {
                p.on_primary
            } else {
                p.on_surface_variant
            };
            let layer = self.layer(cx, cell_id, layer_ink);
            let state = cx.state(cell_id);
            if state.activation() && !self.disabled {
                self.fired = Some(Action::Month(month));
            }
            self.months.push(ChoiceCell {
                id: cell_id,
                text: self.labels.month(month).to_string(),
                chosen,
                width: wide,
                layer,
                ring: state.focus_visible.then_some(p.on_surface),
            });
        }

        let first_year = self.year_block();
        for year in first_year..first_year + 12 {
            let cell_id = self.year_cell_id(year);
            let chosen = value.is_some_and(|v| v.year == year);
            let layer_ink = if chosen {
                p.on_primary
            } else {
                p.on_surface_variant
            };
            let layer = self.layer(cx, cell_id, layer_ink);
            let state = cx.state(cell_id);
            if state.activation() && !self.disabled {
                self.fired = Some(Action::Year(year));
            }
            self.years.push(ChoiceCell {
                id: cell_id,
                text: year.to_string(),
                chosen,
                width: wide,
                layer,
                ring: state.focus_visible.then_some(p.on_surface),
            });
        }

        let month_text = self.labels.month(self.shown.month).to_string();
        let year_text = self.shown.year.to_string();
        // A month grid spans a whole year, so there is no month to page to and the arrows
        // stand down until the day grid is back; the year grid pages by its own block.
        let arrows = self.view != DatePickerView::Months;
        self.month_button = self.button(
            cx,
            id.with("month"),
            Some(month_text),
            None,
            Action::MonthButton,
            true,
        );
        self.year_button = self.button(
            cx,
            id.with("year"),
            Some(year_text),
            None,
            Action::YearButton,
            true,
        );
        self.prev = self.button(
            cx,
            id.with("prev"),
            None,
            Some("chevron_left"),
            Action::Prev,
            arrows,
        );
        self.next = self.button(
            cx,
            id.with("next"),
            None,
            Some("chevron_right"),
            Action::Next,
            arrows,
        );
    }

    /// Resolve one 40dp button: its state layer, its focus ring and the action it fires.
    fn button(
        &mut self,
        cx: &mut Cx<'_, '_>,
        id: Id,
        text: Option<String>,
        icon: Option<&'static str>,
        action: Action,
        enabled: bool,
    ) -> Button {
        let enabled = enabled && !self.disabled;
        let ink = if enabled {
            self.palette.on_surface_variant
        } else {
            // `menu-button.disabled`: onSurface at 38%.
            let base = self.palette.on_surface;
            Color {
                a: base.a * DATE_PICKER_DISABLED_OPACITY,
                ..base
            }
        };
        let layer = if enabled {
            self.layer(cx, id, ink)
        } else {
            None
        };
        let state = cx.state(id);
        if state.activation() && enabled {
            self.fired = Some(action);
        }
        Button {
            id,
            text,
            icon,
            ink,
            enabled,
            layer,
            ring: (enabled && state.focus_visible).then_some(self.palette.on_surface),
        }
    }

    /// A ring drawn on the node's own edge.
    fn ring(corners: Corners, ink: Color) -> Shape {
        Shape::Box {
            corners,
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((ink, 2.0)),
            shadow: None,
            image: None,
        }
    }

    /// Vertical padding that centres one line of `line` in a box of `height`.
    fn centre(height: f32, line: f32) -> Insets {
        Insets {
            left: 0.0,
            top: ((height - line) * 0.5).max(0.0),
            right: 0.0,
            bottom: 0.0,
        }
    }

    /// One day cell.
    fn day_decl(&self, cell: &DayCell) -> Decl {
        let p = &self.palette;
        let side = p.cell;
        // The effective background is the 40dp disc inside the 48dp touch cell: selection,
        // state layer and ripple all use it, never the square or the touch box around it.
        let inset = (side - p.disc) * 0.5;
        let background = Background::new(
            Rect::new(inset, inset, p.disc, p.disc),
            Corners::all(p.disc * 0.5),
        );
        let mut shapes = Vec::with_capacity(4);
        if cell.selected {
            shapes.push(background.fill(p.primary));
        }
        if let Some(ink) = cell.layer {
            shapes.push(background.fill(ink));
        }
        // Today keeps its outline only while it can be picked, so the ring never advertises
        // a day the range rules out.
        if cell.today && !cell.selected && cell.enabled {
            shapes.push(background.ring(p.primary, 1.0));
        }
        if let Some(ink) = cell.ring {
            shapes.push(Self::ring(Corners::all(side * 0.5 - 2.0), ink));
        }
        let label = Text::new(
            cell.id.with("label"),
            cell.text.clone(),
            p.day_label.size,
            cell.ink,
        )
        .weight(p.day_label.weight)
        .resolve();
        Decl::container(cell.id, NodeKind::Column)
            .size(SizeClass::fixed(side, side))
            .padding(Self::centre(side, self.line.day))
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            // A day paints nothing at rest, and a focus stop is usually inferred from ink:
            // without this the arrows walk past every unselected day.
            .focus_stop(true)
            .cursor(if cell.enabled {
                CursorShape::PointingHand
            } else {
                CursorShape::Arrow
            })
            .disabled(!cell.enabled)
            .press_ink(cell.enabled.then(|| {
                let ink = if cell.selected {
                    p.on_primary
                } else if cell.today {
                    p.primary
                } else {
                    p.on_surface_variant
                };
                background.press(
                    Color {
                        a: ink.a * p.press_opacity,
                        ..ink
                    },
                    InkRadius::Cover,
                )
            }))
            // The touch target stays the 48dp cell, larger than the disc it ink-splats into.
            .corners(Corners::all(side * 0.5))
            .shapes(shapes)
            .child(label)
    }

    /// One cell of the month or year grid.
    fn choice_decl(&self, cell: &ChoiceCell) -> Decl {
        let p = &self.palette;
        let (w, h) = (cell.width, p.cell);
        let corners = Corners::all(h * 0.5);
        // The item *is* the background: its selection and state layer fill the stadium the
        // ripple is clipped to, not a disc floating in its middle.
        let background = Background::stadium(w, h);
        let mut shapes = Vec::with_capacity(3);
        if cell.chosen {
            shapes.push(background.fill(p.primary));
        }
        if let Some(ink) = cell.layer {
            shapes.push(background.fill(ink));
        }
        if let Some(ink) = cell.ring {
            shapes.push(Self::ring(Corners::all(h * 0.5 - 2.0), ink));
        }
        let ink = if cell.chosen {
            p.on_primary
        } else {
            p.on_surface
        };
        let label = Text::new(
            cell.id.with("label"),
            cell.text.clone(),
            p.weekday_label.size,
            ink,
        )
        .weight(p.weekday_label.weight)
        .resolve();
        Decl::container(cell.id, NodeKind::Column)
            .size(SizeClass::fixed(w, h))
            .padding(Self::centre(h, self.line.weekday))
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .focus_stop(true)
            .cursor(CursorShape::PointingHand)
            .disabled(self.disabled)
            .press_ink(Some(background.press(
                Color {
                    a: p.on_surface_variant.a * p.press_opacity,
                    ..p.on_surface_variant
                },
                InkRadius::Cover,
            )))
            .corners(corners)
            .shapes(shapes)
            .child(label)
    }

    /// One 40dp button: a label with a caret, or a mark alone.
    fn button_decl(&self, button: &Button, icon_only: bool) -> Decl {
        let p = &self.palette;
        let h = p.button;
        let corners = Corners::all(h * 0.5);
        let ink = button.ink;
        let mut shapes = Vec::with_capacity(2);
        // The button is its own background: a TextButton-style pill (a circle when icon-only),
        // so the state layer covers the whole button instead of a disc at its left edge.
        if let Some(layer) = button.layer {
            shapes.push(Shape::Box {
                corners,
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        if let Some(ring) = button.ring {
            shapes.push(Self::ring(Corners::all(h * 0.5 - 2.0), ring));
        }
        // An icon-only button stacks its mark, so the glyph's em box centres on both axes;
        // a row would leave it at the main-axis start.
        let mut decl = if icon_only {
            Decl::container(button.id, NodeKind::Stack)
                .size(fixed(AxisSize::fixed(h), AxisSize::fixed(h)))
                .stack_align(OverlayAlign::Center)
        } else {
            Decl::container(button.id, NodeKind::Row)
                .size(fixed(AxisSize::hug(0.0, 0.0), AxisSize::fixed(h)))
                .padding(Insets {
                    left: 12.0,
                    top: ((h - self.line.menu) * 0.5).max(0.0),
                    right: 8.0,
                    bottom: 0.0,
                })
                .cross_align(0.0)
                .cross_pos(0.0)
                .gap(2.0)
        }
        .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
        .cursor(if button.enabled {
            CursorShape::PointingHand
        } else {
            CursorShape::Arrow
        })
        .disabled(!button.enabled)
        .press_ink(button.enabled.then(|| {
            press_ink(
                Color {
                    a: p.on_surface_variant.a * p.press_opacity,
                    ..p.on_surface_variant
                },
                InkRadius::Cover,
            )
        }))
        .corners(corners)
        .shapes(shapes);
        if let Some(name) = button.icon {
            decl = decl.child(Icon::new(button.id.with("icon"), name, 24.0, ink).resolve());
        }
        if let Some(text) = &button.text {
            decl = decl
                .child(
                    Text::new(
                        button.id.with("label"),
                        text.clone(),
                        p.menu_label.size,
                        ink,
                    )
                    .weight(p.menu_label.weight)
                    .resolve(),
                )
                .child(Icon::new(button.id.with("caret"), "arrow_drop_down", 18.0, ink).resolve());
        }
        decl
    }

    /// The header band: the calendar's title, or the modal form's two lines and hairline.
    fn header_decl(&self) -> Decl {
        if let Some(m) = self.palette.modal {
            return self.modal_header_decl(m);
        }
        let p = &self.palette;
        let label = Text::new(
            self.id.with("header/label"),
            self.labels.header.to_string(),
            typescale::LABEL_MEDIUM.size,
            p.on_surface_variant,
        )
        .weight(typescale::LABEL_MEDIUM.weight)
        .resolve();
        Decl::container(self.id.with("header"), NodeKind::Row)
            .size(fixed(
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(DATE_PICKER_HEADER),
            ))
            .padding(Insets {
                left: 16.0,
                top: ((DATE_PICKER_HEADER - self.line.header) * 0.5).max(0.0),
                right: 16.0,
                bottom: 0.0,
            })
            .child(label)
    }

    /// The modal header: a small label above the chosen date, closed by a hairline.
    ///
    /// Compose uses a `SpaceBetween` column here; without main-axis placement the two gaps
    /// are fixed, which is the same arithmetic written out.
    fn modal_header_decl(&self, m: ModalChrome) -> Decl {
        let mut text = Decl::container(self.id.with("header/text"), NodeKind::Column)
            .size(fixed(AxisSize::fill(0.0, 0.0), AxisSize::hug(0.0, 0.0)))
            .padding(Insets {
                left: MODAL_SIDE,
                top: MODAL_TITLE_TOP,
                right: MODAL_END,
                bottom: 0.0,
            });
        text = text.child(
            Text::new(
                self.id.with("header/label"),
                self.labels.header.to_string(),
                m.title.size,
                m.ink,
            )
            .weight(m.title.weight)
            .resolve(),
        );
        let mut used = MODAL_TITLE_TOP + m.title_line;
        if let Some(headline) = &self.headline {
            let gap = (MODAL_HEADLINE_TOP - used).max(0.0);
            used = MODAL_HEADLINE_TOP + m.headline_line;
            text = text.child(
                Text::new(
                    self.id.with("header/headline"),
                    headline.to_string(),
                    m.headline.size,
                    m.ink,
                )
                .weight(m.headline.weight)
                .resolve()
                .margin(Insets {
                    left: 0.0,
                    top: gap,
                    right: 0.0,
                    bottom: 0.0,
                }),
            );
        }
        let divider = Decl::container(self.id.with("header/divider"), NodeKind::Box)
            .size(fixed(
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(m.divider.1),
            ))
            .margin(Insets {
                left: 0.0,
                top: (m.header - used - m.divider.1).max(0.0),
                right: 0.0,
                bottom: 0.0,
            })
            .background(m.divider.0, Corners::ZERO);
        Decl::container(self.id.with("header"), NodeKind::Column)
            .size(fixed(AxisSize::fill(0.0, 0.0), AxisSize::fixed(m.header)))
            .cross_align(1.0)
            .child(text)
            .child(divider)
    }

    /// The month and year buttons with the arrows at the far end.
    fn month_row_decl(&self) -> Decl {
        let spacer =
            Decl::container(self.id.with("month-row/spacer"), NodeKind::Column).size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            });
        let (height, cross_align) = match self.palette.modal {
            Some(m) => (m.row, 0.5),
            None => (self.palette.button, 0.0),
        };
        Decl::container(self.id.with("month-row"), NodeKind::Row)
            .size(fixed(AxisSize::fill(0.0, 0.0), AxisSize::fixed(height)))
            .padding(Insets {
                left: DATE_PICKER_SIDE,
                top: 0.0,
                right: DATE_PICKER_SIDE,
                bottom: 0.0,
            })
            .cross_align(cross_align)
            .gap(4.0)
            .child(self.button_decl(&self.month_button, false))
            .child(self.button_decl(&self.year_button, false))
            .child(spacer)
            .child(self.button_decl(&self.prev, true))
            .child(self.button_decl(&self.next, true))
    }

    /// The seven weekday initials.
    fn weekdays_decl(&self) -> Decl {
        let p = &self.palette;
        let cells: Vec<Decl> = self
            .weekdays
            .iter()
            .map(|(id, text)| {
                let label = Text::new(
                    id.with("label"),
                    text.clone(),
                    p.weekday_label.size,
                    p.on_surface,
                )
                .weight(p.weekday_label.weight)
                .resolve();
                Decl::container(*id, NodeKind::Column)
                    .size(SizeClass::fixed(p.cell, p.cell))
                    .padding(Self::centre(p.cell, self.line.weekday))
                    .cross_align(0.0)
                    .cross_pos(0.5)
                    .child(label)
            })
            .collect();
        Decl::container(self.id.with("weekdays"), NodeKind::Row)
            .size(fixed(AxisSize::fill(0.0, 0.0), AxisSize::fixed(p.cell)))
            .children(cells)
    }

    /// Six rows of seven day cells.
    fn grid_decl(&self) -> Decl {
        let p = &self.palette;
        let rows: Vec<Decl> = self
            .days
            .chunks(7)
            .enumerate()
            .map(|(r, week)| {
                Decl::container(Id::of((&self.id, "week", r)), NodeKind::Row)
                    .size(fixed(AxisSize::fill(0.0, 0.0), AxisSize::fixed(p.cell)))
                    .children(
                        week.iter()
                            .map(|cell| self.day_decl(cell))
                            .collect::<Vec<_>>(),
                    )
            })
            .collect();
        Decl::container(self.id.with("grid"), NodeKind::Column)
            .size(fixed(
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(p.cell * DATE_PICKER_WEEKS as f32),
            ))
            .children(rows)
    }

    /// Three columns of four month or year cells.
    fn choices_decl(&self, cells: &[ChoiceCell], tag: &str) -> Decl {
        let rows: Vec<Decl> = cells
            .chunks(3)
            .enumerate()
            .map(|(r, line)| {
                Decl::container(Id::of((&self.id, tag, r)), NodeKind::Row)
                    .size(fixed(
                        AxisSize::fill(0.0, 0.0),
                        AxisSize::fixed(self.palette.cell),
                    ))
                    .children(line.iter().map(|c| self.choice_decl(c)).collect::<Vec<_>>())
            })
            .collect();
        Decl::container(self.id.with(tag), NodeKind::Column)
            .size(fixed(AxisSize::fill(0.0, 0.0), AxisSize::hug(0.0, 0.0)))
            .children(rows)
    }

    /// The area under the month row: whichever grid the view asks for.
    fn body_decl(&self) -> Decl {
        let (top, bottom) = match self.palette.modal {
            Some(m) => (m.body_top, m.body_bottom),
            None => (8.0, 8.0),
        };
        let body = Decl::container(self.id.with("body"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            })
            .padding(Insets {
                left: DATE_PICKER_SIDE,
                top,
                right: DATE_PICKER_SIDE,
                bottom,
            });
        match self.view {
            DatePickerView::Days => body.child(self.weekdays_decl()).child(self.grid_decl()),
            DatePickerView::Months => body.child(self.choices_decl(&self.months, "month-grid")),
            DatePickerView::Years => body.child(self.choices_decl(&self.years, "year-grid")),
        }
    }
}

impl Button {
    fn placeholder(id: Id) -> Self {
        Self {
            id,
            text: None,
            icon: None,
            ink: Color::TRANSPARENT,
            enabled: true,
            layer: None,
            ring: None,
        }
    }
}

/// A box with no growth in either axis.
fn fixed(width: AxisSize, height: AxisSize) -> SizeClass {
    SizeClass {
        width,
        height,
        grow_x: Grow::No,
        grow_y: Grow::No,
    }
}

/// The height one line occupies at `size`, as the text engine measures it.
pub(crate) fn measure_line(size: f32) -> f32 {
    Text::new(
        Id::str("date-picker/probe"),
        "0".to_string(),
        size,
        Color::WHITE,
    )
    .resolve()
    .size_class
    .height
    .hint
}

impl Md3DatePicker {
    /// The surface itself: the three bands inside one rounded, elevated box.
    fn surface_decl(&self) -> Decl {
        let p = &self.palette;
        let (width, height) = p
            .modal
            .map_or((DATE_PICKER_WIDTH, DATE_PICKER_HEIGHT), |m| {
                (m.width, m.height)
            });
        Decl::container(self.surface_id, NodeKind::Column)
            .size(SizeClass::fixed(width, height))
            .cross_align(1.0)
            .shapes(vec![Shape::Box {
                corners: Corners::all(DATE_PICKER_RADIUS),
                fill: p.surface,
                gradient: None,
                border: None,
                shadow: Some(ShadowSpec::elevation(p.elevation.clamp(0.0, 8.0) as u8)),
                image: None,
            }])
            .child(self.header_decl())
            .child(self.month_row_decl())
            .child(self.body_decl())
    }
}

/// A zero-basis grower: it takes a share of the spare space and draws nothing.
fn spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

impl Widget for Md3DatePicker {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let surface = self
            .surface_decl()
            .enter(Presence::fade(ENTER, 0.0).scaled(0.8))
            .exit(Presence::fade(LEAVE, 0.0));
        let Some(m) = self.palette.modal else {
            return surface;
        };
        // The scrim subscribes: a press outside the surface must reach `change()`, and a
        // modal picker blocks the page behind it instead of letting it take the press.
        Decl::container(self.id, NodeKind::Column)
            // Measured as its own content; the layout then places it on the window.
            .size(SizeClass::hug(0.0, 0.0))
            .modal(ModalTarget::Global)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::CLICK)
            .cursor(CursorShape::Arrow)
            .disabled(self.disabled)
            .shapes(vec![Shape::Box {
                corners: Corners::ZERO,
                fill: m.scrim,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }])
            // Two equal growers centre the surface: the engine has no main-axis position.
            .child(spacer(self.id.with("pad-top")))
            .child(surface)
            .child(spacer(self.id.with("pad-bottom")))
            // Only the scrim fades on its own; the surface it holds already scales.
            .enter(Presence::fade(ENTER, 0.0))
            .exit(Presence::fade(LEAVE, 0.0))
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if self.dismissed {
            return Some(Change::Data(Box::new(DatePickerChange::Dismissed)));
        }
        let action = self.fired?;
        Some(Change::Data(Box::new(match action {
            Action::Day(date) => DatePickerChange::Picked(date),
            Action::Prev => DatePickerChange::Shown {
                date: self.shown.add_months(-self.step()),
                view: self.view,
            },
            Action::Next => DatePickerChange::Shown {
                date: self.shown.add_months(self.step()),
                view: self.view,
            },
            // The two menu buttons toggle: a second tap on the open list returns to the days.
            Action::MonthButton => DatePickerChange::View(if self.view == DatePickerView::Months {
                DatePickerView::Days
            } else {
                DatePickerView::Months
            }),
            Action::YearButton => DatePickerChange::View(if self.view == DatePickerView::Years {
                DatePickerView::Days
            } else {
                DatePickerView::Years
            }),
            Action::Month(month) => DatePickerChange::Shown {
                date: Date {
                    month,
                    ..self.shown
                },
                view: DatePickerView::Days,
            },
            Action::Year(year) => DatePickerChange::Shown {
                date: Date { year, ..self.shown },
                view: DatePickerView::Months,
            },
        })))
    }

    fn handles_disabled(&self) -> bool {
        true
    }
}
