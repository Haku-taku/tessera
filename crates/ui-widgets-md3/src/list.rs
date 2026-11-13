//! MD3 list items: one row of the list, with one to three lines of text.

use std::rc::Rc;

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::slot::{SlotFills, SlotName, slot_id};
use ui_core::{CursorShape, Decl, Id, InkRadius, NodeKind, PressInk, Sense, Weight};
use ui_style::Style;
use ui_widgets::IconSource;
use ui_widgets::build::param;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink;
use crate::paint::{role, spec_ink};
use crate::spec::{
    ListFlags, ListLayout, ListLines, ListSelection, ListVariant, ListVideo, list_spec,
};

use crate::motion::REACTION;

/// Segmented items are separated by a 2dp gap and read as pills; a plain item fills the row.
pub const LIST_RADIUS_SEGMENTED: f32 = 12.0;
/// The gap between the label and its trailing text inside one line.
pub const LIST_TRAILING_GAP: f32 = 8.0;

/// How many text lines the item carries; the export gives one height per count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListLineCount {
    One,
    Two,
    Three,
}

/// What sits before the text column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListLeading {
    None,
    Icon,
    Avatar,
    Image,
    Video,
}

/// The named slots a list item exposes; the vocabulary is fixed when the item is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListItemSlot {
    /// Before the text column.
    Leading,
    /// After the text column.
    Trailing,
}

impl ListItemSlot {
    /// The vocabulary in layout order; a fill naming anything else is rejected.
    pub const ALL: [SlotName; 2] = [Self::Leading.slot(), Self::Trailing.slot()];

    /// The engine-side name this slot is keyed by.
    pub const fn slot(self) -> SlotName {
        match self {
            Self::Leading => SlotName::new("leading"),
            Self::Trailing => SlotName::new("trailing"),
        }
    }
}

/// Everything about one item that is fixed when it is built.
pub struct ListItemParams {
    pub id: Id,
    pub label: String,
    pub overline: Option<String>,
    pub supporting: Option<String>,
    pub trailing_text: Option<String>,
    pub lines: ListLineCount,
    pub leading: ListLeading,
    /// Draws the leading mark at the size it is given, in the ink it is given.
    pub mark: Option<fn(Id, Color, f32) -> Decl>,
    /// The trailing mark, as a source the slot resolves.
    pub trailing: Option<IconSource>,
    pub selected: bool,
    pub segmented: bool,
    pub expressive: bool,
    /// Only meaningful for `ListLeading::Video`.
    pub video_large: bool,
    /// The app's own content for the item's named slots; a fill outranks the built-in mark.
    pub slots: SlotFills,
}

impl ListItemParams {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            overline: None,
            supporting: None,
            trailing_text: None,
            lines: ListLineCount::One,
            leading: ListLeading::None,
            mark: None,
            trailing: None,
            selected: false,
            segmented: false,
            expressive: false,
            video_large: false,
            slots: SlotFills::new(),
        }
    }

    pub fn overline(mut self, text: impl Into<String>) -> Self {
        self.overline = Some(text.into());
        self
    }

    pub fn supporting(mut self, text: impl Into<String>) -> Self {
        self.supporting = Some(text.into());
        self
    }

    pub fn trailing_text(mut self, text: impl Into<String>) -> Self {
        self.trailing_text = Some(text.into());
        self
    }

    pub fn lines(mut self, lines: ListLineCount) -> Self {
        self.lines = lines;
        self
    }

    pub fn leading(mut self, leading: ListLeading) -> Self {
        self.leading = leading;
        self
    }

    pub fn mark(mut self, mark: fn(Id, Color, f32) -> Decl) -> Self {
        self.mark = Some(mark);
        self
    }

    pub fn trailing(mut self, trailing: IconSource) -> Self {
        self.trailing = Some(trailing);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn segmented(mut self, segmented: bool) -> Self {
        self.segmented = segmented;
        self
    }

    pub fn expressive(mut self, expressive: bool) -> Self {
        self.expressive = expressive;
        self
    }

    pub fn video_large(mut self, large: bool) -> Self {
        self.video_large = large;
        self
    }

    /// Fill one of the item's named slots with the app's own content.
    pub fn slot(mut self, name: ListItemSlot, content: impl Into<Rc<Decl>>) -> Self {
        self.slots.insert(name.slot(), content);
        self
    }
}

/// The leading slot's size for a leading kind and the resolved token sizes.
fn leading_size_of(leading: ListLeading, icon: f32, avatar: f32, image: (f32, f32)) -> (f32, f32) {
    match leading {
        ListLeading::None => (0.0, 0.0),
        // The frame is the mark's own em box; a line-box height would leave it hanging high.
        ListLeading::Icon => (icon, ui_widgets::icon_extent(icon)),
        ListLeading::Avatar => (avatar, avatar),
        ListLeading::Image | ListLeading::Video => image,
    }
}

fn lines_of(count: ListLineCount) -> ListLines {
    match count {
        ListLineCount::One => ListLines::OneLine,
        ListLineCount::Two => ListLines::TwoLine,
        ListLineCount::Three => ListLines::ThreeLine,
    }
}

fn selection_of(selected: bool) -> ListSelection {
    if selected {
        ListSelection::Selected
    } else {
        ListSelection::Unselected
    }
}

/// The divider's own inset, which the export ties to the text column rather than the row.
pub fn divider_insets() -> (f32, f32) {
    let spec = base_spec();
    (spec.divider.leading_space, spec.divider.trailing_space)
}

fn base_spec() -> crate::spec::ListSpec {
    list_spec(
        ListFlags::NONE,
        ListLines::OneLine,
        ListSelection::Unselected,
        ListLayout::Base,
        ListVariant::Base,
        ListVideo::Base,
    )
}

/// One row of a list.
pub struct Md3ListItem {
    id: Id,
    label: String,
    overline: Option<String>,
    supporting: Option<String>,
    trailing_text: Option<String>,
    leading: ListLeading,
    slots: SlotFills,
    height: f32,
    pad_l: f32,
    pad_r: f32,
    gap: f32,
    radius: f32,
    fill: Color,
    layer: Option<Color>,
    /// The press ink the engine splats at the pointer.
    press: Option<PressInk>,
    ring: Option<(Color, f32)>,
    label_ink: Color,
    label_style: (f32, Weight),
    overline_ink: Color,
    overline_style: (f32, Weight),
    supporting_ink: Color,
    supporting_style: (f32, Weight),
    trailing_text_ink: Color,
    trailing_text_style: (f32, Weight),
    icon_size: f32,
    avatar_ink: Color,
    avatar_size: f32,
    image: (f32, f32),
    disabled: bool,
}

impl Md3ListItem {
    /// Build one item. Every axis here is chosen when it is built, not while it runs.
    pub fn new_with(
        params: Md3Options,
        item: ListItemParams,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let id = item.id;
        let mut node = cx.at(id);
        let state = node.state();
        let disabled = params.disabled || state.disabled;
        let video = if item.video_large {
            ListVideo::Large
        } else {
            ListVideo::Small
        };
        let layout = if item.segmented {
            ListLayout::Segmented
        } else {
            ListLayout::Base
        };
        let variant = if item.expressive {
            ListVariant::Expressive
        } else {
            ListVariant::Base
        };
        let lines = lines_of(item.lines);
        let rest = list_spec(
            ListFlags {
                hovered: state.hovered,
                focused: state.focus_visible,
                pressed: state.pressed,
                disabled,
                ..ListFlags::NONE
            },
            lines,
            selection_of(item.selected),
            layout,
            variant,
            video,
        );
        let hover = list_spec(
            ListFlags {
                hovered: true,
                disabled,
                ..ListFlags::NONE
            },
            lines,
            selection_of(item.selected),
            layout,
            variant,
            video,
        );
        let focus = list_spec(
            ListFlags {
                focused: true,
                disabled,
                ..ListFlags::NONE
            },
            lines,
            selection_of(item.selected),
            layout,
            variant,
            video,
        );
        let press = list_spec(
            ListFlags {
                pressed: true,
                disabled,
                ..ListFlags::NONE
            },
            lines,
            selection_of(item.selected),
            layout,
            variant,
            video,
        );
        let hover_t = node.animate(id.with(param::HOVER), f32::from(state.hovered), REACTION);
        let focus_t = node.animate(
            id.with(param::FOCUS),
            f32::from(state.focus_visible),
            REACTION,
        );
        let layer_ink = role(pack, hover.state_layer.color);
        let strength =
            (hover_t * hover.state_layer.opacity).max(focus_t * focus.state_layer.opacity);
        let layer = (strength > 0.0 && !disabled).then_some(Color {
            a: layer_ink.a * strength,
            ..layer_ink
        });
        // A press is a splat from the pointer, not another rise of the flat layer.
        let ink = (!disabled).then(|| {
            press_ink(
                Color {
                    a: layer_ink.a * press.state_layer.opacity,
                    ..layer_ink
                },
                InkRadius::Cover,
            )
        });
        let ring_t = node.animate(
            id.with(param::THICKNESS),
            f32::from(state.focus_visible),
            REACTION,
        );
        let ring_ink = spec_ink(pack, rest.focus_indicator.color, 1.0);
        let ring = (ring_t > 0.0 && !disabled).then_some((
            Color {
                a: ring_ink.a * ring_t,
                ..ring_ink
            },
            rest.focus_indicator.thickness,
        ));
        let weight = |w: f32| Weight(w.round().clamp(0.0, 1000.0) as u16);
        let icon_ink = spec_ink(pack, rest.leading_icon.color, rest.leading_icon.opacity);
        let icon_size = rest.leading_icon.size;
        let avatar_ink = spec_ink(pack, rest.leading_avatar.color, 1.0);
        let avatar_label_ink = spec_ink(pack, rest.leading_avatar_label.color, 1.0);
        let avatar_size = rest.leading_avatar.size;
        let image = (rest.leading_image.width, rest.leading_image.height);
        let leading_size = leading_size_of(item.leading, icon_size, avatar_size, image);
        // The built-in mark and icon are default fills: an app fill always outranks them.
        let mut slots = item.slots;
        let leading = ListItemSlot::Leading.slot();
        let trailing = ListItemSlot::Trailing.slot();
        if slots.get(leading).is_none()
            && let (Some(mark), (w, h)) = (item.mark, leading_size)
            && w > 0.0
            && h > 0.0
        {
            let ink = match item.leading {
                ListLeading::Avatar => avatar_label_ink,
                _ => icon_ink,
            };
            slots.insert(
                leading,
                mark(slot_id(id, leading).with("mark"), ink, w.min(h)),
            );
        }
        if slots.get(trailing).is_none()
            && let Some(source) = item.trailing
        {
            slots.insert(
                trailing,
                source.resolve_with_size(slot_id(id, trailing).with("trail"), icon_ink, icon_size),
            );
        }
        Self {
            id,
            label: item.label,
            overline: item.overline,
            supporting: item.supporting,
            trailing_text: item.trailing_text,
            leading: item.leading,
            slots,
            height: rest.container.height,
            pad_l: rest.root.leading_space,
            pad_r: rest.root.trailing_space,
            gap: rest.root.between_space,
            radius: if item.segmented {
                LIST_RADIUS_SEGMENTED
            } else {
                0.0
            },
            fill: spec_ink(pack, rest.container.color, rest.container.opacity),
            layer,
            press: ink,
            ring,
            label_ink: spec_ink(pack, rest.label_text.color, rest.label_text.opacity),
            label_style: (rest.label_text.size, weight(rest.label_text.weight)),
            overline_ink: spec_ink(pack, rest.overline.color, rest.overline.opacity),
            overline_style: (rest.overline.size, weight(rest.overline.weight)),
            supporting_ink: spec_ink(
                pack,
                rest.supporting_text.color,
                rest.supporting_text.opacity,
            ),
            supporting_style: (
                rest.supporting_text.size,
                weight(rest.supporting_text.weight),
            ),
            trailing_text_ink: spec_ink(
                pack,
                rest.trailing_supporting_text.color,
                rest.trailing_supporting_text.opacity,
            ),
            trailing_text_style: (
                rest.trailing_supporting_text.size,
                weight(rest.trailing_supporting_text.weight),
            ),
            icon_size,
            avatar_ink,
            avatar_size,
            image,
            disabled,
        }
    }

    /// The row's height, which the export fixes per line count.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// The leading slot's own size; zero when the item has none.
    pub fn leading_size(&self) -> (f32, f32) {
        leading_size_of(self.leading, self.icon_size, self.avatar_size, self.image)
    }

    fn corner(&self) -> Corners {
        Corners::all(self.radius)
    }

    fn text(&self, id: Id, body: &str, style: (f32, Weight), ink: Color) -> Decl {
        Text::new(id, body.to_string(), style.0, ink)
            .weight(style.1)
            .resolve()
    }

    /// The leading slot's frame: the export's sized box, painted with the avatar's fill.
    ///
    /// The frame is the mark's em box, so its cross-axis slack must centre the mark rather
    /// than leave it at the top.
    fn leading_frame(&self) -> Decl {
        let (w, h) = self.leading_size();
        let frame = if w > 0.0 && h > 0.0 {
            Decl::container(Id::default(), NodeKind::Row)
                .size(SizeClass::fixed(w, h))
                .cross_align(0.0)
                .cross_pos(0.5)
        } else {
            Decl::container(Id::default(), NodeKind::Box)
        };
        match self.leading {
            ListLeading::Avatar => frame.shapes(vec![Shape::Box {
                corners: Corners::all(w.min(h) * 0.5),
                fill: self.avatar_ink,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }]),
            ListLeading::Image | ListLeading::Video => {
                // The export names the shape without sizing it; 8dp keeps the corner in proportion.
                frame.shapes(vec![Shape::Box {
                    corners: Corners::all(8.0),
                    fill: self.avatar_ink,
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                }])
            }
            _ => frame,
        }
    }

    fn text_column(&self) -> Decl {
        let id = self.id.with("text");
        let mut column = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .gap(0.0);
        if let Some(overline) = &self.overline {
            column = column.child(self.text(
                id.with("overline"),
                overline,
                self.overline_style,
                self.overline_ink,
            ));
        }
        let mut line = Decl::container(id.with("line"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .gap(LIST_TRAILING_GAP);
        line = line.child(self.text(
            id.with("label"),
            &self.label,
            self.label_style,
            self.label_ink,
        ));
        if let Some(text) = &self.trailing_text {
            line = line.child(self.text(
                id.with("trailing-text"),
                text,
                self.trailing_text_style,
                self.trailing_text_ink,
            ));
        }
        column = column.child(line);
        if let Some(supporting) = &self.supporting {
            column = column.child(self.text(
                id.with("supporting"),
                supporting,
                self.supporting_style,
                self.supporting_ink,
            ));
        }
        column
    }
}

impl Widget for Md3ListItem {
    fn handles_disabled(&self) -> bool {
        // The export carries its own 38% disabled content.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<ui_core::Change> {
        (!self.disabled && state.activation()).then_some(ui_core::Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let mut shapes = Vec::with_capacity(3);
        shapes.push(Shape::Box {
            corners: self.corner(),
            fill: self.fill,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        });
        if let Some(layer) = self.layer {
            shapes.push(Shape::Box {
                corners: self.corner(),
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        // The export insets the ring by `focus.indicator.outline.offset` (-3dp, i.e. inside);
        // it is drawn on the item's own edge instead, since a decl cannot know the final rect.
        if let Some((ink, width)) = self.ring {
            shapes.push(Shape::Box {
                corners: self.corner(),
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((ink, width)),
                shadow: None,
                image: None,
            });
        }
        let mut row = Decl::container(self.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: self.pad_l,
                right: self.pad_r,
                top: 0.0,
                bottom: 0.0,
            })
            .gap(self.gap)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(self.press)
            .shapes(shapes);
        // Slots are placed where the item's own layout wants them, not in fill order: the
        // leading slot goes before the text column and the trailing one after it.
        let leading = ListItemSlot::Leading.slot();
        let trailing = ListItemSlot::Trailing.slot();
        debug_assert_eq!(
            self.slots.outside(&ListItemSlot::ALL),
            None,
            "a fill named a slot this item does not declare"
        );
        if let Some(content) = self.slots.get(leading) {
            row = row.slot_with(leading, self.leading_frame(), Rc::clone(content));
        }
        row = row.child(self.text_column());
        if let Some(content) = self.slots.get(trailing) {
            row = row.slot(trailing, Rc::clone(content));
        }
        row
    }
}
