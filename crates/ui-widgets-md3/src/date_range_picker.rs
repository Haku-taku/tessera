//! MD3 date range picker: a modal calendar whose month list scrolls and joins two days.

use std::sync::Arc;

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::{ModalTarget, Presence};
use ui_core::{Change, CursorShape, Decl, Id, InkRadius, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::scroll::{RowWindow, Scroll};
use ui_widgets::text::Text;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::date_picker::{
    CalendarLabels, DATE_PICKER_CELL, DATE_PICKER_DISC, DATE_PICKER_RADIUS, DATE_PICKER_SIDE,
    DATE_PICKER_WEEKS, DATE_PICKER_WIDTH, Date, Weekday, day_cell_id, measure_line,
};
use crate::dialog::{ENTER, LEAVE, SCRIM_OPACITY};
use crate::layer::Background;
use crate::motion::REACTION;
use crate::paint::spec_ink;
use crate::spec::{
    DatePickerFlags, RangeSelectionFlags, date_picker_modal_spec, date_picker_spec,
    range_selection_spec,
};
use crate::typescale::TypeStyle;

/// The modal picker's height: the range form shares its box.
pub const DATE_RANGE_PICKER_HEIGHT: f32 = 524.0;
/// `header.container.height` (128) less the 60 dp of buttons Compose's header leaves out.
pub const DATE_RANGE_HEADER: f32 = 68.0;
/// One month block: the subhead's 20 + 20 + 8 and its six 48 dp weeks.
pub const DATE_RANGE_MONTH: f32 = 48.0 + DATE_PICKER_CELL * DATE_PICKER_WEEKS as f32;
/// How many months the list offers when the app names none.
pub const DATE_RANGE_MONTHS: u32 = 12;
/// The title's inset and the headline line under it.
const HEADER_TITLE_TOP: f32 = 8.0;
const HEADER_HEADLINE_TOP: f32 = 28.0;
const HEADER_SIDE: f32 = 24.0;
const HEADER_END: f32 = 12.0;
/// `CalendarMonthSubheadPadding`.
const SUBHEAD_LEFT: f32 = 24.0;
const SUBHEAD_TOP: f32 = 20.0;
/// One screen of months kept beyond the viewport, so a wheel step never shows a gap.
const OVERSCAN: f32 = 524.0;
/// `date.disabled`: the strength a cell the range rules out keeps.
const DISABLED_OPACITY: f32 = 0.38;

/// The id of the day cell for `date` inside the range picker `picker`.
pub fn range_day_id(picker: Id, date: Date) -> Id {
    day_cell_id(picker, date)
}

/// The id of the month block at list position `row`.
pub fn range_month_id(picker: Id, row: usize) -> Id {
    Id::of((&picker, "range-month", row))
}

/// Everything the picker is built with; the app owns the selection and the dates.
pub struct DateRangePickerParams {
    pub id: Id,
    /// The first day of the selection, drawn as a filled disc.
    pub start: Option<Date>,
    /// The second day; no band is drawn until both are set.
    pub end: Option<Date>,
    /// The first month of the list, and therefore the month it opens on.
    pub shown: Date,
    pub today: Date,
    /// How many months follow `shown`.
    pub months: u32,
    /// Earliest selectable day; cells before it are inert.
    pub min: Option<Date>,
    pub max: Option<Date>,
    pub week_start: Weekday,
    /// Labels to draw; `None` reads them from the catalog in force.
    pub labels: Option<CalendarLabels>,
    /// The header's title; `None` reads the catalog's.
    pub title: Option<Arc<str>>,
    /// The two headline halves; `None` draws the catalog's placeholders.
    pub start_text: Option<Arc<str>>,
    pub end_text: Option<Arc<str>>,
}

impl DateRangePickerParams {
    /// A picker opening on `shown`'s month with nothing chosen.
    pub fn new(id: Id, shown: Date, today: Date) -> Self {
        Self {
            id,
            start: None,
            end: None,
            shown: shown.first_of_month(),
            today,
            months: DATE_RANGE_MONTHS,
            min: None,
            max: None,
            week_start: Weekday::Mon,
            labels: None,
            title: None,
            start_text: None,
            end_text: None,
        }
    }
}

/// What the picker asks its app to remember.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateRangeChange {
    /// The selection after a press: Compose's `updateDateSelection` applied.
    Selected { start: Date, end: Option<Date> },
    /// A press on the scrim; closing it is the app's decision.
    Dismissed,
}

/// The selection a press produces: the rule Compose states in `updateDateSelection`.
///
/// A completed range starts over at the pressed day; while only the start is set, a day at
/// or after it ends the range and an earlier day restarts it.
pub fn next_range(start: Option<Date>, end: Option<Date>, day: Date) -> (Date, Option<Date>) {
    match (start, end) {
        (Some(first), None) if day >= first => (first, Some(day)),
        _ => (day, None),
    }
}

/// The inks and styles this frame resolves to, so the decl builders repeat no lookups.
struct Palette {
    surface: Color,
    scrim: Color,
    on_surface: Color,
    primary: Color,
    on_primary: Color,
    band: Color,
    in_range: Color,
    header_ink: Color,
    subhead_ink: Color,
    hover_ink: Color,
    hover_opacity: f32,
    press_ink: Color,
    press_opacity: f32,
    in_range_hover_ink: Color,
    in_range_hover_opacity: f32,
    in_range_press_ink: Color,
    in_range_press_opacity: f32,
    cell: f32,
    disc: f32,
    band_height: f32,
    day_label: TypeStyle,
    weekday_label: TypeStyle,
    title: TypeStyle,
    headline: TypeStyle,
    subhead: TypeStyle,
}

/// The measured height of one line at each size the picker uses.
#[derive(Clone, Copy)]
struct LineHeights {
    day: f32,
    weekday: f32,
    title: f32,
}

/// One day of a month grid.
struct DayCell {
    id: Id,
    text: String,
    selected: bool,
    today: bool,
    in_range: bool,
    enabled: bool,
    ink: Color,
    layer: Option<Color>,
    ring: Option<Color>,
}

/// One month of the list, resolved: its subhead, its 42 cells and one band per row.
struct MonthBlock {
    row: usize,
    subhead: String,
    cells: Vec<Option<DayCell>>,
    bands: Vec<Option<(f32, f32)>>,
}

/// A modal MD3 date range picker; the app owns both dates and the first month shown.
pub struct Md3DateRangePicker {
    id: Id,
    surface_id: Id,
    dismissed: bool,
    fired: Option<(Date, Option<Date>)>,
    disabled: bool,
    title: Arc<str>,
    start_text: Arc<str>,
    end_text: Arc<str>,
    weekdays: Vec<(Id, String)>,
    blocks: Vec<MonthBlock>,
    above: f32,
    below: f32,
    palette: Palette,
    line: LineHeights,
}

impl Md3DateRangePicker {
    /// Resolve this frame's picker: its palette, the months in the window, and any press.
    pub fn from_pack(pack: &Style, cx: &mut Cx<'_, '_>, params: DateRangePickerParams) -> Self {
        let id = params.id;
        let surface_id = id.with("surface");
        let scroller_id = id.with("months");
        let modal = date_picker_modal_spec();
        let base = date_picker_spec(DatePickerFlags::NONE);
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
        let range = range_selection_spec(RangeSelectionFlags {
            in_range: true,
            ..RangeSelectionFlags::NONE
        });
        let range_hover = range_selection_spec(RangeSelectionFlags {
            in_range: true,
            hovered: true,
            ..RangeSelectionFlags::NONE
        });
        let range_press = range_selection_spec(RangeSelectionFlags {
            in_range: true,
            pressed: true,
            ..RangeSelectionFlags::NONE
        });
        let palette = Palette {
            surface: spec_ink(pack, modal.container.color, 1.0),
            scrim: spec_ink(pack, "scrim", SCRIM_OPACITY),
            on_surface: spec_ink(pack, base.date_label_text.color, 1.0),
            primary: spec_ink(pack, selected.date_container.color, 1.0),
            on_primary: spec_ink(pack, selected.date_label_text.color, 1.0),
            band: spec_ink(pack, range.active_indicator_container.color, 1.0),
            in_range: spec_ink(pack, range.date_label_text.color, 1.0),
            header_ink: spec_ink(pack, modal.header_headline.color, 1.0),
            subhead_ink: spec_ink(pack, range.month_subhead.color, 1.0),
            hover_ink: spec_ink(pack, hover.date_state_layer.color, 1.0),
            hover_opacity: hover.date_state_layer.opacity,
            press_ink: spec_ink(pack, press.date_state_layer.color, 1.0),
            press_opacity: press.date_state_layer.opacity,
            in_range_hover_ink: spec_ink(pack, range_hover.date_state_layer.color, 1.0),
            in_range_hover_opacity: range_hover.date_state_layer.opacity,
            in_range_press_ink: spec_ink(pack, range_press.date_state_layer.color, 1.0),
            in_range_press_opacity: range_press.date_state_layer.opacity,
            cell: base.date_container.width.max(DATE_PICKER_CELL),
            disc: base.date_state_layer.width.max(DATE_PICKER_DISC),
            band_height: range
                .active_indicator_container
                .height
                .max(DATE_PICKER_DISC),
            day_label: TypeStyle {
                size: base.date_label_text.size,
                weight: Weight(base.date_label_text.weight as u16),
            },
            weekday_label: TypeStyle {
                size: base.weekdays_label_text.size,
                weight: Weight(base.weekdays_label_text.weight as u16),
            },
            title: TypeStyle {
                size: modal.header_supporting_text.size,
                weight: Weight(modal.header_supporting_text.weight as u16),
            },
            headline: TypeStyle {
                size: range.header_headline.size,
                weight: Weight(range.header_headline.weight as u16),
            },
            subhead: TypeStyle {
                size: range.month_subhead.size,
                weight: Weight(range.month_subhead.weight as u16),
            },
        };
        let line = LineHeights {
            day: measure_line(palette.day_label.size),
            weekday: measure_line(palette.weekday_label.size),
            title: measure_line(palette.title.size),
        };
        let labels = params
            .labels
            .clone()
            .unwrap_or_else(|| CalendarLabels::from_cx(cx));
        let disabled = cx.state(id).disabled;
        let dismissed = cx.state(id).clicked && !cx.state(surface_id).clicked;

        let count = params.months.max(1) as usize;
        let window = RowWindow::of(cx.core(), scroller_id, count, OVERSCAN, |_| {
            DATE_RANGE_MONTH
        });
        let mut fired = None;
        let mut blocks = Vec::with_capacity(window.last.saturating_sub(window.first));
        for row in window.first..window.last {
            let month = params.shown.first_of_month().add_months(row as i32);
            let grid = grid_of(month, params.week_start);
            let cells = grid
                .iter()
                .map(|slot| {
                    slot.and_then(|date| {
                        Self::day(cx, &palette, disabled, date, &params, &mut fired)
                    })
                })
                .collect();
            blocks.push(MonthBlock {
                row,
                subhead: format!("{} {}", labels.month(month.month), month.year),
                cells,
                bands: (0..DATE_PICKER_WEEKS)
                    .map(|line| {
                        row_band(
                            &grid,
                            line,
                            params.start,
                            params.end,
                            palette.cell * 7.0,
                            palette.cell,
                        )
                    })
                    .collect(),
            });
        }
        Self {
            id,
            surface_id,
            dismissed,
            fired,
            disabled,
            title: params
                .title
                .clone()
                .unwrap_or_else(|| cx.t(ui_i18n::Msg::MD3_DATE_RANGE_PICKER_TITLE)),
            start_text: params
                .start_text
                .clone()
                .unwrap_or_else(|| cx.t(ui_i18n::Msg::MD3_DATE_RANGE_PICKER_START)),
            end_text: params
                .end_text
                .clone()
                .unwrap_or_else(|| cx.t(ui_i18n::Msg::MD3_DATE_RANGE_PICKER_END)),
            weekdays: (0..7u8)
                .map(|i| {
                    let weekday = Weekday::ALL[((params.week_start.index() + i) % 7) as usize];
                    (
                        Id::of((&id, "weekday", i)),
                        labels.weekdays[weekday.index() as usize].to_string(),
                    )
                })
                .collect(),
            blocks,
            above: window.above,
            below: window.below,
            palette,
            line,
        }
    }

    /// The scroller, so a schedule or a self-test can read its geometry.
    pub fn scroller_id(&self) -> Id {
        self.id.with("months")
    }

    /// Resolve one day cell: its paint, and the press it reports when activated.
    fn day(
        cx: &mut Cx<'_, '_>,
        palette: &Palette,
        disabled: bool,
        date: Date,
        params: &DateRangePickerParams,
        fired: &mut Option<(Date, Option<Date>)>,
    ) -> Option<DayCell> {
        let cell_id = day_cell_id(params.id, date);
        let enabled = !disabled
            && params.min.is_none_or(|min| date >= min)
            && params.max.is_none_or(|max| date <= max);
        let selected = params.start == Some(date) || params.end == Some(date);
        let today = date == params.today;
        let in_range = match (params.start, params.end) {
            (Some(start), Some(end)) => date > start && date < end,
            _ => false,
        };
        let ink = if selected {
            palette.on_primary
        } else if in_range {
            palette.in_range
        } else if !enabled {
            Color {
                a: palette.on_surface.a * DISABLED_OPACITY,
                ..palette.on_surface
            }
        } else if today {
            palette.primary
        } else {
            palette.on_surface
        };
        let layer = if enabled {
            Self::layer(cx, palette, disabled, cell_id, in_range)
        } else {
            None
        };
        let state = cx.state(cell_id);
        if state.activation() && enabled {
            *fired = Some(next_range(params.start, params.end, date));
        }
        Some(DayCell {
            id: cell_id,
            text: date.day.to_string(),
            selected,
            today,
            in_range,
            enabled,
            ink,
            layer,
            ring: state.focus_visible.then_some(palette.on_surface),
        })
    }

    /// The state layer's ink, or nothing when no pointer is on the cell.
    fn layer(
        cx: &mut Cx<'_, '_>,
        palette: &Palette,
        disabled: bool,
        id: Id,
        in_range: bool,
    ) -> Option<Color> {
        let state = cx.state(id);
        if disabled || !(state.hovered || state.pressed || state.focus_visible) {
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
        let (ink, hover_opacity, press_opacity) = if in_range {
            (
                palette.in_range_hover_ink,
                palette.in_range_hover_opacity,
                palette.in_range_press_opacity,
            )
        } else {
            (
                palette.hover_ink,
                palette.hover_opacity,
                palette.press_opacity,
            )
        };
        let fade = if state.pressed {
            press_opacity
        } else {
            (hover * hover_opacity).max(press * press_opacity)
        };
        (fade > 0.0).then_some(Color {
            a: ink.a * fade,
            ..ink
        })
    }

    /// The header: the title, then the two dates or their placeholders.
    fn header_decl(&self) -> Decl {
        let p = &self.palette;
        let title = Text::new(
            self.id.with("header/title"),
            self.title.to_string(),
            p.title.size,
            p.header_ink,
        )
        .weight(p.title.weight)
        .resolve();
        let gap = (HEADER_HEADLINE_TOP - HEADER_TITLE_TOP - self.line.title).max(0.0);
        let headline = self.headline_decl().margin(Insets {
            left: 0.0,
            top: gap,
            right: 0.0,
            bottom: 0.0,
        });
        Decl::container(self.id.with("header"), NodeKind::Column)
            .size(size_class(
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(DATE_RANGE_HEADER),
            ))
            .padding(Insets {
                left: HEADER_SIDE,
                top: HEADER_TITLE_TOP,
                right: HEADER_END,
                bottom: 0.0,
            })
            .cross_align(1.0)
            .child(title)
            .child(headline)
    }

    /// One half of the headline: the date, or the catalog's placeholder for it.
    fn headline_text(&self, id: Id, text: &str) -> Decl {
        Text::new(
            id,
            text.to_string(),
            self.palette.headline.size,
            self.palette.header_ink,
        )
        .weight(self.palette.headline.weight)
        .resolve()
    }

    /// The headline row: start, a dash, end, four dp apart as Compose spaces them.
    fn headline_decl(&self) -> Decl {
        Decl::container(self.id.with("header/headline"), NodeKind::Row)
            .gap(4.0)
            .cross_align(0.0)
            .child(self.headline_text(self.id.with("header/start"), &self.start_text))
            .child(self.headline_text(self.id.with("header/dash"), "-"))
            .child(self.headline_text(self.id.with("header/end"), &self.end_text))
    }

    /// The seven weekday initials, aligned with the grid below.
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
                    .size(size_class(AxisSize::fixed(p.cell), AxisSize::fixed(p.cell)))
                    .padding(Insets {
                        left: 0.0,
                        top: ((p.cell - self.line.weekday) * 0.5).max(0.0),
                        right: 0.0,
                        bottom: 0.0,
                    })
                    .cross_align(0.0)
                    .cross_pos(0.5)
                    .child(label)
            })
            .collect();
        Decl::container(self.id.with("weekdays"), NodeKind::Row)
            .size(size_class(
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(p.cell),
            ))
            .padding(Insets {
                left: DATE_PICKER_SIDE,
                top: 0.0,
                right: DATE_PICKER_SIDE,
                bottom: 0.0,
            })
            .children(cells)
    }

    /// The scroller over the months, inset so the grid lines up with the weekdays.
    fn body_decl(&self) -> Decl {
        let p = &self.palette;
        let mut content = Decl::container(self.id.with("months/content"), NodeKind::Column).size(
            size_class(AxisSize::fill(0.0, 0.0), AxisSize::hug(0.0, 0.0)),
        );
        if self.above > 0.0 {
            content = content.child(spacer(self.id.with("months/above"), self.above));
        }
        for block in &self.blocks {
            content = content.child(self.month_decl(block));
        }
        if self.below > 0.0 {
            content = content.child(spacer(self.id.with("months/below"), self.below));
        }
        let height = DATE_RANGE_PICKER_HEIGHT - DATE_RANGE_HEADER - p.cell;
        let scroller = Scroll::fixed(self.scroller_id(), p.cell * 7.0, height)
            .no_scrollbar()
            .child(content)
            .resolve();
        Decl::container(self.id.with("body"), NodeKind::Column)
            .size(size_class(
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(height),
            ))
            .padding(Insets {
                left: DATE_PICKER_SIDE,
                top: 0.0,
                right: DATE_PICKER_SIDE,
                bottom: 0.0,
            })
            .child(scroller)
    }

    /// One month: its subhead over six rows of cells.
    fn month_decl(&self, block: &MonthBlock) -> Decl {
        let p = &self.palette;
        let subhead = Text::new(
            Id::of((&self.id, "range-subhead", block.row)),
            block.subhead.clone(),
            p.subhead.size,
            p.subhead_ink,
        )
        .weight(p.subhead.weight)
        .resolve();
        let subhead = Decl::container(
            Id::of((&self.id, "range-subhead-box", block.row)),
            NodeKind::Column,
        )
        .size(size_class(AxisSize::fill(0.0, 0.0), AxisSize::fixed(48.0)))
        .padding(Insets {
            left: SUBHEAD_LEFT,
            top: SUBHEAD_TOP,
            right: HEADER_END,
            bottom: 0.0,
        })
        .child(subhead);
        let mut month = Decl::container(range_month_id(self.id, block.row), NodeKind::Column)
            .size(size_class(
                AxisSize::fill(0.0, 0.0),
                AxisSize::fixed(DATE_RANGE_MONTH),
            ))
            .child(subhead);
        for line in 0..DATE_PICKER_WEEKS {
            month = month.child(self.row_decl(block, line));
        }
        month
    }

    /// One week: the band behind it and its seven cells.
    fn row_decl(&self, block: &MonthBlock, line: usize) -> Decl {
        let p = &self.palette;
        let mut shapes = Vec::new();
        if let Some((x0, x1)) = block.bands[line] {
            let top = (p.cell - p.band_height) * 0.5;
            shapes.push(Shape::Rect {
                rect: Rect::new(x0, top, (x1 - x0).max(0.0), p.band_height),
                corners: Corners::ZERO,
                fill: p.band,
                gradient: None,
                border: None,
            });
        }
        let mut row = Decl::container(
            Id::of((&self.id, "range-row", block.row, line)),
            NodeKind::Row,
        )
        .size(size_class(
            AxisSize::fill(0.0, 0.0),
            AxisSize::fixed(p.cell),
        ))
        .shapes(shapes);
        for column in 0..7 {
            row = row.child(match &block.cells[line * 7 + column] {
                Some(cell) => self.day_decl(cell),
                None => Decl::container(
                    Id::of((&self.id, "range-blank", block.row, line, column)),
                    NodeKind::Column,
                )
                .size(size_class(AxisSize::fixed(p.cell), AxisSize::fixed(p.cell))),
            });
        }
        row
    }

    /// One day cell: the 40 dp disc inside the 48 dp touch cell.
    fn day_decl(&self, cell: &DayCell) -> Decl {
        let p = &self.palette;
        let side = p.cell;
        let inset = (side - p.disc) * 0.5;
        let radius = p.disc * 0.5;
        let background = Background::new(
            Rect::new(inset, inset, p.disc, p.disc),
            Corners::all(radius),
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
            shapes.push(ring(Corners::all(side * 0.5 - 2.0), ink));
        }
        let label = Text::new(
            cell.id.with("label"),
            cell.text.clone(),
            p.day_label.size,
            cell.ink,
        )
        .weight(p.day_label.weight)
        .resolve();
        let press = if cell.selected {
            p.on_primary
        } else if cell.in_range {
            p.in_range_press_ink
        } else {
            p.press_ink
        };
        Decl::container(cell.id, NodeKind::Column)
            .size(size_class(AxisSize::fixed(side), AxisSize::fixed(side)))
            .padding(Insets {
                left: 0.0,
                top: ((side - self.line.day) * 0.5).max(0.0),
                right: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .focus_stop(true)
            .cursor(if cell.enabled {
                CursorShape::PointingHand
            } else {
                CursorShape::Arrow
            })
            .disabled(!cell.enabled)
            .press_ink(cell.enabled.then(|| {
                background.press(
                    Color {
                        a: press.a * p.press_opacity,
                        ..press
                    },
                    InkRadius::Cover,
                )
            }))
            .corners(Corners::all(side * 0.5))
            .shapes(shapes)
            .child(label)
    }

    /// The surface: header, weekdays and the month list in one rounded, elevated box.
    fn surface_decl(&self) -> Decl {
        let p = &self.palette;
        Decl::container(self.surface_id, NodeKind::Column)
            .size(SizeClass::fixed(
                DATE_PICKER_WIDTH,
                DATE_RANGE_PICKER_HEIGHT,
            ))
            .cross_align(1.0)
            .shapes(vec![Shape::Box {
                corners: Corners::all(DATE_PICKER_RADIUS),
                fill: p.surface,
                gradient: None,
                border: None,
                // The range table's container elevation is 0: the scrim sets it apart.
                shadow: None,
                image: None,
            }])
            .child(self.header_decl())
            .child(self.weekdays_decl())
            .child(self.body_decl())
    }
}

/// The month's cells in a 6×7 grid, blank before the first day and after the last.
pub(crate) fn grid_of(month: Date, week_start: Weekday) -> Vec<Option<Date>> {
    let first = month.first_of_month();
    let lead = first.weekday().column(week_start) as usize;
    let days = Date::days_in_month(first.year, first.month) as usize;
    let mut out = vec![None; DATE_PICKER_WEEKS * 7];
    for day in 0..days {
        let index = lead + day;
        if let Some(slot) = out.get_mut(index) {
            *slot = Some(Date {
                day: day as u8 + 1,
                ..first
            });
        }
    }
    out
}

/// The band a row paints, in the grid's own space: from the start cell's middle to the end
/// cell's, widened to the row's edges while the range continues past it.
pub(crate) fn row_band(
    grid: &[Option<Date>],
    row: usize,
    start: Option<Date>,
    end: Option<Date>,
    width: f32,
    cell: f32,
) -> Option<(f32, f32)> {
    let (Some(start), Some(end)) = (start, end) else {
        return None;
    };
    let week = grid.get(row * 7..row * 7 + 7)?;
    let columns: Vec<usize> = week
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.is_some_and(|date| date >= start && date <= end))
        .map(|(column, _)| column)
        .collect();
    let first = *columns.first()?;
    let last = *columns.last()?;
    let half = cell * 0.5;
    let x0 = if week.contains(&Some(start)) {
        first as f32 * cell + half
    } else {
        0.0
    };
    let x1 = if week.contains(&Some(end)) {
        last as f32 * cell + half
    } else {
        width
    };
    Some((x0, x1))
}

/// A box with no growth in either axis.
fn size_class(width: AxisSize, height: AxisSize) -> SizeClass {
    SizeClass {
        width,
        height,
        grow_x: Grow::No,
        grow_y: Grow::No,
    }
}

/// A zero-width block of the given height, so the scroller's content keeps its full extent.
fn spacer(id: Id, height: f32) -> Decl {
    Decl::container(id, NodeKind::Column).size(size_class(
        AxisSize::fill(0.0, 0.0),
        AxisSize::fixed(height),
    ))
}

/// A ring drawn on the node's own edge.
fn ring(corners: Corners, ink: Color) -> Shape {
    Shape::Box {
        corners,
        fill: Color::TRANSPARENT,
        border: Some((ink, 2.0)),
        gradient: None,
        shadow: None,
        image: None,
    }
}

impl Widget for Md3DateRangePicker {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let surface = self
            .surface_decl()
            .enter(Presence::fade(ENTER, 0.0).scaled(0.8))
            .exit(Presence::fade(LEAVE, 0.0));
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
                fill: self.palette.scrim,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }])
            .child(spacer_grower(self.id.with("pad-top")))
            .child(surface)
            .child(spacer_grower(self.id.with("pad-bottom")))
            // Only the scrim fades on its own; the surface it holds already scales.
            .enter(Presence::fade(ENTER, 0.0))
            .exit(Presence::fade(LEAVE, 0.0))
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if self.dismissed {
            return Some(Change::Data(Box::new(DateRangeChange::Dismissed)));
        }
        self.fired
            .map(|(start, end)| Change::Data(Box::new(DateRangeChange::Selected { start, end })))
    }

    fn handles_disabled(&self) -> bool {
        true
    }
}

/// A zero-basis grower: it takes a share of the spare space and draws nothing.
fn spacer_grower(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}
