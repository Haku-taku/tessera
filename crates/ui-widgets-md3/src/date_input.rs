//! MD3 date input: the text face of the date picker — a header band, one date field, two actions.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{Change, CursorShape, Decl, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::{Cx, IconSet, IconSource, Text, Widget, WidgetState};

use crate::paint::spec_ink;
use crate::spec::date_input_spec;
use crate::text_field::{Md3TextField, TextFieldParams, TextFieldVariant};
use crate::{ButtonParams, ButtonSize, ButtonStyle, Md3Button, Md3Options, typescale};

/// `container.width`: the surface's box, as the export sizes it.
pub const DATE_INPUT_WIDTH: f32 = 328.0;
/// `container.height`: the surface's box height; the actions sit at its bottom edge.
pub const DATE_INPUT_HEIGHT: f32 = 512.0;
/// `header.container.height`: the band holding the supporting text and the headline.
pub const DATE_INPUT_HEADER: f32 = 120.0;
/// Side inset of the header and the field, from Compose's `DatePickerTitlePadding`.
pub const DATE_INPUT_SIDE: f32 = 24.0;
/// The surface's corner: MD3's extra-large shape, shared with the dialog and the picker.
pub const DATE_INPUT_RADIUS: f32 = 28.0;

/// `DatePickerTitlePadding.top`: the supporting line hangs this far under the top edge.
const TITLE_TOP: f32 = 16.0;
/// `DatePickerHeadlinePadding`: the end inset, and the gap under the headline.
const HEADLINE_END: f32 = 12.0;
const HEADLINE_BOTTOM: f32 = 12.0;
/// `InputTextFieldPadding.top`: the field hangs this far under the header band.
const FIELD_TOP: f32 = 10.0;
/// The mode toggle's square; MD3's icon button is 40dp, a touch target inside the row.
const TOGGLE: f32 = 40.0;
/// Top inset of the actions band, which Compose leaves to the dialog's own padding.
const ACTIONS_TOP: f32 = 8.0;

/// One of the three things the input asks for; each owns its node id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateInputAction {
    /// Ask for the calendar face; the buffer is untouched.
    Calendar,
    Cancel,
    /// Take the buffer as the chosen date; validating it is the app's.
    Confirm,
}

impl DateInputAction {
    /// The node this action reports as, derived from the input's own id.
    pub fn id(self, input: Id) -> Id {
        input.with(match self {
            Self::Calendar => "calendar",
            Self::Cancel => "cancel",
            Self::Confirm => "confirm",
        })
    }
}

/// The field's own id: the node the engine edits, so the app can follow the buffer.
pub fn date_input_field_id(input: Id) -> Id {
    input.with("field")
}

/// What the input asks its app to do. Text edits arrive separately, as `Change::Text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateInputChange(pub DateInputAction);

/// Everything the input is built with; every string is the app's.
pub struct DateInputParams<'a> {
    pub id: Id,
    /// The buffer the app holds; the engine edits it and hands the new text back.
    pub text: &'a str,
    /// The 32sp line: the chosen date, or the spec's "No date selected".
    pub headline: &'a str,
    /// The 14sp line above it: what this dialog is for.
    pub supporting: &'a str,
    pub label: &'a str,
    /// The format the field expects, e.g. `MM/DD/YYYY`.
    pub placeholder: &'a str,
    /// Helper or error line under the field; empty means none.
    pub supporting_field: &'a str,
    pub error: bool,
    pub cancel: &'a str,
    pub confirm: &'a str,
    pub disabled: bool,
    /// Draw the calendar button beside the headline; a host that has no calendar leaves it off.
    pub calendar: bool,
}

impl<'a> DateInputParams<'a> {
    /// A bare input: one empty buffer, no label, hint or supporting lines.
    pub fn new(id: Id) -> Self {
        Self {
            id,
            text: "",
            headline: "",
            supporting: "",
            label: "",
            placeholder: "",
            supporting_field: "",
            error: false,
            cancel: "",
            confirm: "",
            disabled: false,
            calendar: true,
        }
    }
}

/// A text-mode MD3 date picker; the app owns the buffer, the headline and the actions' words.
pub struct Md3DateInput {
    id: Id,
    surface: Color,
    shadow: Option<ShadowSpec>,
    headline: String,
    headline_style: typescale::TypeStyle,
    headline_ink: Color,
    supporting: String,
    supporting_style: typescale::TypeStyle,
    supporting_ink: Color,
    toggle_ink: Color,
    calendar: bool,
    field: Md3TextField,
    actions: Vec<(DateInputAction, Md3Button)>,
    fired: Option<DateInputAction>,
    disabled: bool,
}

impl Md3DateInput {
    /// Resolve this frame's input: the surface's inks, the field, and the one action taken.
    pub fn from_pack(pack: &Style, cx: &mut Cx<'_, '_>, params: DateInputParams<'_>) -> Self {
        let DateInputParams {
            id,
            text,
            headline,
            supporting,
            label,
            placeholder,
            supporting_field,
            error,
            cancel,
            confirm,
            disabled,
            calendar,
        } = params;
        let spec = date_input_spec();
        let state = cx.at(id).state();
        let disabled = disabled || state.disabled;
        // The export carries no disabled colours for this surface, so it fades as a whole the
        // way every MD3 dialog does; the field resolves its own.
        let fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };
        // Only the actions this input actually offers: a host without a calendar gets no
        // button, so it must not be able to report that action either.
        let offered = if calendar {
            &[
                DateInputAction::Calendar,
                DateInputAction::Cancel,
                DateInputAction::Confirm,
            ][..]
        } else {
            &[DateInputAction::Cancel, DateInputAction::Confirm][..]
        };
        let fired = offered
            .iter()
            .find(|action| cx.state(action.id(id)).activation())
            .copied();
        let field = Md3TextField::new_with(
            Md3Options {
                disabled,
                ..Md3Options::default()
            },
            TextFieldParams::new(
                date_input_field_id(id),
                TextFieldVariant::Outlined,
                DATE_INPUT_WIDTH - 2.0 * DATE_INPUT_SIDE,
            )
            .text(text)
            .label(label)
            .placeholder(placeholder)
            .supporting(supporting_field)
            .error(error)
            .disabled(disabled),
            cx,
            pack,
        );
        // Text buttons, as every MD3 dialog lays out its actions.
        let options = Md3Options {
            disabled,
            ..Md3Options::default()
        };
        let actions = [
            (DateInputAction::Cancel, cancel),
            (DateInputAction::Confirm, confirm),
        ]
        .into_iter()
        .map(|(action, label)| {
            let button = Md3Button::new_with(
                options,
                ButtonParams::new(action.id(id), label)
                    .style(ButtonStyle::Text)
                    .size(ButtonSize::Base),
                cx,
                pack,
            );
            (action, button)
        })
        .collect();
        Self {
            id,
            surface: spec_ink(pack, spec.container.color, 1.0),
            shadow: Some(ShadowSpec::elevation(
                spec.container.elevation.clamp(0.0, 8.0) as u8,
            )),
            headline: headline.to_string(),
            headline_style: typescale::named(spec.header_headline.type_),
            headline_ink: spec_ink(pack, spec.header_headline.color, fade),
            supporting: supporting.to_string(),
            supporting_style: typescale::named(spec.header_supporting_text.type_),
            supporting_ink: spec_ink(pack, spec.header_supporting_text.color, fade),
            toggle_ink: spec_ink(pack, spec.header_headline.color, fade),
            calendar,
            field,
            actions,
            fired,
            disabled,
        }
    }

    /// The field's node id, so a caller can drive it without rebuilding the derivation.
    pub fn field_id(&self) -> Id {
        date_input_field_id(self.id)
    }

    /// The supporting line above the headline, measured for its own row.
    fn supporting_decl(&self) -> Decl {
        let line = measure_line(self.supporting_style.size);
        Decl::container(self.id.with("header/supporting"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(line),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: DATE_INPUT_SIDE,
                top: TITLE_TOP,
                right: HEADLINE_END,
                bottom: 0.0,
            })
            .child(
                Text::new(
                    self.id.with("header/supporting-text"),
                    self.supporting.clone(),
                    self.supporting_style.size,
                    self.supporting_ink,
                )
                .weight(self.supporting_style.weight)
                .resolve(),
            )
    }

    /// The headline row: the date on the left, the calendar toggle at the far end.
    fn headline_decl(&self) -> Decl {
        let line = measure_line(self.headline_style.size);
        let mut row = Decl::container(self.id.with("header/headline"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                // A long headline wraps and the band grows; clamping the row would clip it.
                height: AxisSize::hug(line + HEADLINE_BOTTOM, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: DATE_INPUT_SIDE,
                top: 0.0,
                right: HEADLINE_END,
                bottom: HEADLINE_BOTTOM,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(
                Decl::container(self.id.with("header/headline-text"), NodeKind::Column)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::hug(0.0, 0.0),
                        grow_x: Grow::Yes,
                        grow_y: Grow::No,
                    })
                    .cross_align(0.0)
                    .cross_pos(0.5)
                    .child(
                        Text::new(
                            self.id.with("header/headline-label"),
                            self.headline.clone(),
                            self.headline_style.size,
                            self.headline_ink,
                        )
                        .weight(self.headline_style.weight)
                        .resolve(),
                    ),
            );
        if self.calendar {
            // A glyph slot that subscribes: the same shape the search bar's actions use.
            let action = DateInputAction::Calendar;
            row = row.child(
                Decl::container(action.id(self.id), NodeKind::Column)
                    .size(SizeClass::fixed(TOGGLE, TOGGLE))
                    .padding(Insets {
                        left: 0.0,
                        top: (TOGGLE - ui_widgets::icon_extent(24.0)) * 0.5,
                        right: 0.0,
                        bottom: 0.0,
                    })
                    .cross_align(0.0)
                    .cross_pos(0.5)
                    .sense(Sense::CLICK | Sense::FOCUSABLE)
                    .cursor(CursorShape::PointingHand)
                    .disabled(self.disabled)
                    .child(
                        IconSource::glyph("calendar_today", IconSet::Outlined)
                            .resolve(action.id(self.id).with("icon"), self.toggle_ink),
                    ),
            );
        }
        row
    }

    /// The header band: SpaceBetween, as Compose arranges the two lines.
    fn header_decl(&self) -> Decl {
        // The export's 120 is a floor, as Compose takes it (`defaultMinSize`): a headline
        // that needs two lines grows the band instead of being cut off.
        Decl::container(self.id.with("header"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(DATE_INPUT_HEADER, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .child(self.supporting_decl())
            .child(grower(self.id.with("header/gap")))
            .child(self.headline_decl())
    }

    /// The actions band: text buttons at the far end, Cancel before Confirm.
    fn actions_decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let mut row = Decl::container(self.id.with("actions"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: DATE_INPUT_SIDE,
                top: ACTIONS_TOP,
                right: DATE_INPUT_SIDE,
                bottom: DATE_INPUT_SIDE,
            })
            .cross_align(0.0)
            .cross_pos(1.0)
            .gap(crate::dialog::ACTION_GAP)
            // A row has no main-axis position, so a grower carries the buttons to the end.
            .child(grower(self.id.with("actions/gap")));
        for (_, button) in &self.actions {
            row = row.child(button.decl(cx));
        }
        row
    }
}

/// A zero-basis grower: it takes the spare space on both axes and draws nothing.
fn grower(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::Yes,
        grow_y: Grow::Yes,
    })
}

/// The line box a text of `size` measures at, floored the way layout floors a text leaf.
fn measure_line(size: f32) -> f32 {
    Text::new(
        Id::str("date-input/probe"),
        "0".to_string(),
        size,
        Color::WHITE,
    )
    .resolve()
    .size_class
    .height
    .hint
}

impl Widget for Md3DateInput {
    fn handles_disabled(&self) -> bool {
        // The surface's fade is resolved per line, so the adapter must not dim it again.
        true
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        Decl::container(self.id, NodeKind::Column)
            .size(SizeClass::fixed(DATE_INPUT_WIDTH, DATE_INPUT_HEIGHT))
            .cross_align(0.0)
            .disabled(self.disabled)
            .shapes(vec![Shape::Box {
                corners: Corners::all(DATE_INPUT_RADIUS),
                fill: self.surface,
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }])
            .child(self.header_decl())
            .child(
                Decl::container(self.id.with("field-band"), NodeKind::Column)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::hug(0.0, 0.0),
                        grow_x: Grow::No,
                        grow_y: Grow::No,
                    })
                    .padding(Insets {
                        left: DATE_INPUT_SIDE,
                        top: FIELD_TOP,
                        right: DATE_INPUT_SIDE,
                        bottom: 0.0,
                    })
                    .child(self.field.decl(cx)),
            )
            .child(grower(self.id.with("gap")))
            .child(self.actions_decl(cx))
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        Some(Change::Data(Box::new(DateInputChange(self.fired?))))
    }
}
