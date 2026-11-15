//! The Fluent combo box: a 32 dp field with a chevron over an acrylic drop-down of rows.

use ui_core::draw::{Acrylic, BackdropMaterial, FocusVisual};
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{
    Anchor, AxisSize, Grow, OverlayAlign, OverlayPlacement, Presentation, Side, SizeClass,
};
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::{
    BackdropBlur, Change, CursorShape, Decl, Id, Key, Modifiers, NodeKind, Sense, Weight,
};
use ui_style::{ControlKind, Density, Style};
use ui_widgets::text::Text;
use ui_widgets::{Cx, Icon, Widget, WidgetState};

use crate::glyphs;
use crate::material::{AcrylicRecipe, acrylic_dark, acrylic_light};
use crate::paint::{Stroke, role_of, stroke_of};
use crate::popup::{PopupMotion, popup_motion, popup_window};

/// `ComboBoxThemeMinWidth` and `ComboBoxPopupThemeMinWidth`.
pub const COMBO_MIN_WIDTH: f32 = 64.0;
pub const POPUP_MIN_WIDTH: f32 = 80.0;
/// The drop-down glyph's `Width`/`Height` 12, its 38 dp column and `Margin 0,0,14,0`.
pub const CHEVRON_SIZE: f32 = 12.0;
pub const CHEVRON_COLUMN: f32 = 38.0;
pub const CHEVRON_MARGIN: f32 = 14.0;
/// `OverlayCornerRadius`, `ComboBoxDropdownBorderThickness`, `ComboBoxDropdownContentMargin`.
pub const POPUP_RADIUS: f32 = 8.0;
pub const POPUP_BORDER: f32 = 1.0;
pub const POPUP_CONTENT_PAD: Insets = Insets::new(0.0, 4.0, 0.0, 4.0);
/// The item template's `Margin="5,2,5,2"`, `ComboBoxItemThemePadding`,
/// `ComboBoxItemCornerRadius`, and the selected pill's `ComboBoxItemPill*` tokens.
pub const COMBO_ITEM_MARGIN: Insets = Insets::new(5.0, 2.0, 5.0, 2.0);
pub const COMBO_ITEM_PAD: Insets = Insets::new(11.0, 5.0, 11.0, 7.0);
pub const COMBO_ITEM_RADIUS: f32 = 3.0;
pub const PILL_W: f32 = 3.0;
pub const PILL_H: f32 = 16.0;
pub const PILL_RADIUS: f32 = 1.5;
/// The field's own pill sits at `Margin="1,0,0,0"`; `ComboBoxTopHeaderMargin` is 0,0,0,8.
pub const PILL_LEFT: f32 = 1.0;
pub const HEADER_GAP: f32 = 8.0;
/// `ComboBoxPlaceholderTextThemeFontWeight` is `SemiLight`.
pub const PLACEHOLDER_WEIGHT: Weight = Weight(350);

/// Escape closes the drop-down; while it is closed the arrows move the selection.
pub const COMBO_DISMISS: ShortcutId = ShortcutId("combo.dismiss");
pub const COMBO_TOGGLE: ShortcutId = ShortcutId("combo.toggle");
pub const COMBO_NEXT: ShortcutId = ShortcutId("combo.next");
pub const COMBO_PREV: ShortcutId = ShortcutId("combo.prev");
pub const COMBO_FIRST: ShortcutId = ShortcutId("combo.first");
pub const COMBO_LAST: ShortcutId = ShortcutId("combo.last");

/// One entry of the list: `ComboBoxItem`'s content and its `IsEnabled`.
#[derive(Clone, Debug)]
pub struct ComboBoxItem {
    pub id: Id,
    pub label: String,
    pub disabled: bool,
}

impl ComboBoxItem {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            disabled: false,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// Everything the field is built with, named after the properties the reference exposes.
pub struct ComboBoxParams<'a> {
    pub id: Id,
    pub items: &'a [ComboBoxItem],
    /// `SelectedIndex`; out of range means nothing is selected.
    pub selected: Option<usize>,
    /// `IsDropDownOpen`; a closed field builds no panel at all.
    pub open: bool,
    /// `PlaceholderText`, shown while nothing is selected.
    pub placeholder: String,
    /// `Header`; the caption above the field.
    pub header: Option<String>,
    pub disabled: bool,
    /// `Width`; `None` hugs the widest row down to `ComboBoxThemeMinWidth`.
    pub width: Option<f32>,
}

impl<'a> ComboBoxParams<'a> {
    pub fn new(id: Id, items: &'a [ComboBoxItem]) -> Self {
        Self {
            id,
            items,
            selected: None,
            open: false,
            placeholder: String::new(),
            header: None,
            disabled: false,
            width: None,
        }
    }

    pub fn selected(mut self, index: Option<usize>) -> Self {
        self.selected = index;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn header(mut self, text: impl Into<String>) -> Self {
        self.header = Some(text.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
}

use crate::spec::{DISABLED, POINTER_OVER, PRESSED, REST};

/// The brush families one item state column swaps in.
struct ItemBrushes {
    fill: [&'static str; 4],
    ink: [&'static str; 4],
}

const ITEM_PLAIN: ItemBrushes = ItemBrushes {
    fill: [
        "ComboBoxItemBackground",
        "ComboBoxItemBackgroundPointerOver",
        "ComboBoxItemBackgroundPressed",
        "ComboBoxItemBackgroundDisabled",
    ],
    ink: [
        "ComboBoxItemForeground",
        "ComboBoxItemForegroundPointerOver",
        "ComboBoxItemForegroundPressed",
        "ComboBoxItemForegroundDisabled",
    ],
};

const ITEM_SELECTED: ItemBrushes = ItemBrushes {
    fill: [
        "ComboBoxItemBackgroundSelected",
        "ComboBoxItemBackgroundSelectedPointerOver",
        "ComboBoxItemBackgroundSelectedPressed",
        "ComboBoxItemBackgroundSelectedDisabled",
    ],
    ink: [
        "ComboBoxItemForegroundSelected",
        "ComboBoxItemForegroundSelectedPointerOver",
        "ComboBoxItemForegroundSelectedPressed",
        "ComboBoxItemForegroundSelectedDisabled",
    ],
};

const ITEM_SELECTED_UNFOCUSED: ItemBrushes = ItemBrushes {
    fill: [
        "ComboBoxItemBackgroundSelectedUnfocused",
        "ComboBoxItemBackgroundSelectedPointerOver",
        "ComboBoxItemBackgroundSelectedPressed",
        "ComboBoxItemBackgroundSelectedDisabled",
    ],
    ink: [
        "ComboBoxItemForegroundSelectedUnfocused",
        "ComboBoxItemForegroundSelectedPointerOver",
        "ComboBoxItemForegroundSelectedPressed",
        "ComboBoxItemForegroundSelectedDisabled",
    ],
};

/// One row, with the facts its decl needs.
struct Row {
    id: Id,
    label: String,
    disabled: bool,
    selected: bool,
    /// Which of the three brush tables this row reads.
    brushes: usize,
    /// Which column of that table this frame's state picks.
    slot: usize,
}

/// A combo box, resolved against the pack in force where it was built.
pub struct FluentComboBox {
    id: Id,
    rows: Vec<Row>,
    header: Option<String>,
    header_ink: Color,
    header_size: f32,
    placeholder: String,
    selected: Option<usize>,
    open: bool,
    disabled: bool,
    width: Option<f32>,
    min_width: f32,
    min_height: f32,
    pad: Insets,
    corners: Corners,
    border_w: f32,
    /// The field's `CommonStates` column and whether its `FocusStates` column is in force.
    slot: usize,
    focused: bool,
    pressed: bool,
    fill: [Color; 4],
    ink: [Color; 4],
    placeholder_ink: [Color; 4],
    glyph: [Color; 4],
    stroke: [Stroke; 4],
    /// The `FocusStates` column, which outranks the common one while the field has keys.
    ink_focused: Color,
    ink_focused_pressed: Color,
    placeholder_focused: Color,
    placeholder_focused_pressed: Color,
    glyph_focused: Color,
    glyph_focused_pressed: Color,
    focus: FocusVisual,
    pill: Color,
    label_size: f32,
    label_weight: Weight,
    /// The drop-down panel: acrylic inside `OverlayCornerRadius`, with its flyout stroke.
    dropdown_stroke: Color,
    acrylic: Acrylic,
    blur_sigma: f32,
    item_fill: [[Color; 4]; 3],
    item_ink: [[Color; 4]; 3],
    item_focus: FocusVisual,
    /// The panel's entrance: presence, travel, and whether it is still on stage.
    motion: PopupMotion,
    /// This frame's input facts, read here because `change` sees only the root's state.
    fired: Option<Id>,
    dismissed: bool,
    toggled: bool,
    step: Option<usize>,
}

impl FluentComboBox {
    pub fn new(params: ComboBoxParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let ComboBoxParams {
            id,
            items,
            selected,
            open,
            placeholder,
            header,
            disabled: off,
            width,
        } = params;
        let m = pack.controls.get(ControlKind::ComboBox, Density::Standard);
        // The state the field draws with is the box's own, not the root's: the box is what
        // takes the pointer and the keyboard.
        let box_state = cx.state(id.with("box"));
        let root_state = cx.state(id);
        let disabled = off || box_state.disabled;
        let slot = if disabled {
            DISABLED
        } else if box_state.pressed {
            PRESSED
        } else if box_state.hovered {
            POINTER_OVER
        } else {
            REST
        };
        // `Focused`/`FocusedPressed` only exist while the field has keys. An open panel keeps
        // the highlight: the keyboard is inside the control, just not on the box any more.
        let focused = !disabled && (box_state.focus_visible || open);
        let selected = selected.filter(|k| *k < items.len());
        let rows: Vec<Row> = items
            .iter()
            .enumerate()
            .map(|(k, item)| {
                let st = cx.state(item.id);
                let live = !item.disabled && !st.disabled;
                let selected = selected == Some(k);
                let column = if !live {
                    DISABLED
                } else if st.pressed {
                    PRESSED
                } else if st.hovered {
                    POINTER_OVER
                } else {
                    REST
                };
                // `Selected` is the live look; `SelectedUnfocused` drops the border and the
                // darker wash, which is what a row shows with the keyboard elsewhere.
                let brushes = match selected {
                    false => 0,
                    true if live && (st.hovered || st.focused) => 1,
                    true => 2,
                };
                Row {
                    id: item.id,
                    label: item.label.clone(),
                    disabled: item.disabled,
                    selected,
                    brushes,
                    slot: column,
                }
            })
            .collect();
        let fired = rows
            .iter()
            .find(|r| {
                let st = cx.state(r.id);
                !r.disabled && st.activation()
            })
            .map(|r| r.id);
        // A press outside the panel, or Escape, closes it. Both are read here because the
        // change reports from the root, while the panel is the node they land on.
        let dismissed =
            cx.state(id.with("popup")).dismissed || root_state.shortcut == Some(COMBO_DISMISS);
        let step = match root_state.shortcut {
            Some(COMBO_NEXT) => neighbour(items, selected, true),
            Some(COMBO_PREV) => neighbour(items, selected, false),
            Some(COMBO_FIRST) => items.iter().position(|i| !i.disabled),
            Some(COMBO_LAST) => items.iter().rposition(|i| !i.disabled),
            _ => None,
        };
        let toggled = !dismissed
            && fired.is_none()
            && (box_state.activation() || root_state.shortcut == Some(COMBO_TOGGLE));
        // The panel unfolds toward the field; the window keeps it clipped to its final rect.
        let motion = popup_motion(
            cx,
            id.with("popup"),
            Anchor::Node(id.with("box")),
            Side::Below,
            open,
        );
        let recipe: AcrylicRecipe = match pack.appearance() {
            Some(ui_design::Appearance::Dark) => acrylic_dark(),
            _ => acrylic_light(),
        };
        // `FocusVisualMargin -4` around the 4 dp field: the adorner's curve grows by as much
        // to reach the reference's `ComboBoxHiglightBorderCornerRadius` 7.
        // `ComboBoxHiglightBorderCornerRadius` 7.
        let focus = FocusVisual {
            outer: role_of(pack, "FocusStrokeColorOuterBrush"),
            outer_width: 2.0,
            inner: Color::TRANSPARENT,
            // The reveal focus of the reference is one 2 dp ring, with no inner line.
            inner_width: 0.0,
            margin: Insets::all(m.focus_margin.left),
            radius: 7.0 - m.radius,
        };
        let item_focus = FocusVisual {
            outer: role_of(pack, "FocusStrokeColorOuterBrush"),
            outer_width: 2.0,
            inner: role_of(pack, "FocusStrokeColorInnerBrush"),
            inner_width: 1.0,
            // `FocusVisualMargin -3` on the item.
            margin: Insets::all(-3.0),
            radius: COMBO_ITEM_RADIUS + 3.0,
        };
        let header_ink = if disabled {
            role_of(pack, "ComboBoxHeaderForegroundDisabled")
        } else {
            role_of(pack, "ComboBoxHeaderForeground")
        };
        let role = pack.type_table.get(m.font);
        Self {
            id,
            rows,
            header,
            header_ink,
            header_size: role.size,
            placeholder,
            selected,
            open,
            disabled,
            width,
            min_width: m.min_width.max(COMBO_MIN_WIDTH),
            min_height: m.min_height,
            pad: m.padding,
            corners: Corners::all(m.radius).with_shape(pack.metrics.control.corner),
            border_w: m.border,
            slot,
            focused,
            pressed: box_state.pressed,
            fill: [
                "ComboBoxBackground",
                "ComboBoxBackgroundPointerOver",
                "ComboBoxBackgroundPressed",
                "ComboBoxBackgroundDisabled",
            ]
            .map(|n| role_of(pack, n)),
            ink: [
                "ComboBoxForeground",
                "ComboBoxForegroundPointerOver",
                "ComboBoxForegroundPressed",
                "ComboBoxForegroundDisabled",
            ]
            .map(|n| role_of(pack, n)),
            placeholder_ink: [
                "ComboBoxPlaceHolderForeground",
                "ComboBoxPlaceHolderForegroundPointerOver",
                "ComboBoxPlaceHolderForegroundPressed",
                "ComboBoxPlaceHolderForegroundDisabled",
            ]
            .map(|n| role_of(pack, n)),
            glyph: [
                "ComboBoxDropDownGlyphForeground",
                "ComboBoxDropDownGlyphForeground",
                "ComboBoxDropDownGlyphForeground",
                "ComboBoxDropDownGlyphForegroundDisabled",
            ]
            .map(|n| role_of(pack, n)),
            stroke: [
                "ComboBoxBorderBrush",
                "ComboBoxBorderBrushPointerOver",
                "ComboBoxBorderBrushPressed",
                "ComboBoxBorderBrushDisabled",
            ]
            .map(|n| stroke_of(pack, n)),
            ink_focused: role_of(pack, "ComboBoxForegroundFocused"),
            ink_focused_pressed: role_of(pack, "ComboBoxForegroundFocusedPressed"),
            placeholder_focused: role_of(pack, "ComboBoxPlaceHolderForegroundFocused"),
            placeholder_focused_pressed: role_of(
                pack,
                "ComboBoxPlaceHolderForegroundFocusedPressed",
            ),
            glyph_focused: role_of(pack, "ComboBoxDropDownGlyphForegroundFocused"),
            // Its own key names a legacy alias the corpus never defines; that alias equals
            // TextFillColorPrimary in both themes, which is exactly this field's pressed ink.
            glyph_focused_pressed: role_of(pack, "ComboBoxForegroundFocusedPressed"),
            focus,
            pill: role_of(pack, "ComboBoxItemPillFillBrush"),
            label_size: role.size,
            label_weight: role.weight,
            dropdown_stroke: role_of(pack, "ComboBoxDropDownBorderBrush"),
            acrylic: Acrylic {
                tint: recipe.tint,
                luminosity: recipe.luminosity,
                noise: recipe.noise,
            },
            blur_sigma: recipe.blur_sigma,
            item_fill: [
                ITEM_PLAIN.fill,
                ITEM_SELECTED.fill,
                ITEM_SELECTED_UNFOCUSED.fill,
            ]
            .map(|names| names.map(|n| role_of(pack, n))),
            item_ink: [
                ITEM_PLAIN.ink,
                ITEM_SELECTED.ink,
                ITEM_SELECTED_UNFOCUSED.ink,
            ]
            .map(|names| names.map(|n| role_of(pack, n))),
            item_focus,
            motion,
            fired,
            dismissed,
            toggled,
            step,
        }
    }

    /// The text the field shows, and whether it is the placeholder.
    fn value(&self) -> (String, bool) {
        match self.selected.and_then(|k| self.rows.get(k)) {
            Some(row) => (row.label.clone(), false),
            None => (self.placeholder.clone(), true),
        }
    }

    /// The field itself: its paint, the pill when focused, its text column and its chevron.
    fn box_decl(&self) -> Decl {
        let id = self.id;
        let slot = self.slot;
        let focused = self.focused;
        let mut row = Decl::container(id.with("box"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(self.min_height, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            // WinUI combo boxes keep the arrow: the drop-down glyph is not a hyperlink.
            .cursor(CursorShape::Arrow)
            .disabled(self.disabled)
            .cross_align(0.0)
            .cross_pos(0.5)
            .corners(self.corners)
            .focus_visual(Some(self.focus))
            .background(self.fill[slot], self.corners);
        row = self.stroke[slot].on(row, self.border_w);
        let (text, placeholder) = self.value();
        let ink = match (placeholder, focused) {
            (false, true) if self.pressed => self.ink_focused_pressed,
            (false, true) => self.ink_focused,
            (false, false) => self.ink[slot],
            (true, true) if self.pressed => self.placeholder_focused_pressed,
            (true, true) => self.placeholder_focused,
            (true, false) => self.placeholder_ink[slot],
        };
        // The `Pill` at x=1 exists only while the field has focus; it shares the content's
        // 12 dp lead-in instead of taking a slot the unfocused field would also pay for.
        let lead = match focused {
            true => self.pad.left - PILL_LEFT - PILL_W,
            false => self.pad.left,
        };
        if focused {
            row = row.child(self.field_pill_decl());
        }
        let mut content = Decl::container(id.with("content"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: lead,
                top: self.pad.top,
                right: self.pad.right,
                bottom: self.pad.bottom,
            })
            .cross_align(0.0)
            .cross_pos(0.5);
        if !text.is_empty() {
            content = content.child(
                Text::new(id.with("value"), text, self.label_size, ink)
                    .weight(match placeholder {
                        true => PLACEHOLDER_WEIGHT,
                        false => self.label_weight,
                    })
                    .nowrap()
                    .resolve(),
            );
        }
        row = row.child(content);
        row = row.child(self.chevron_decl(focused, slot));
        row
    }

    /// The focused pill: `ComboBoxItemPillFillBrush`, 3×16 at x=1, vertically centred.
    fn field_pill_decl(&self) -> Decl {
        Decl::container(self.id.with("pill"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(PILL_LEFT + PILL_W),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: PILL_LEFT,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(
                Decl::container(self.id.with("pill-bar"), NodeKind::Box)
                    .size(SizeClass::fixed(PILL_W, PILL_H))
                    .background(self.pill, Corners::all(PILL_RADIUS)),
            )
    }

    /// The chevron's 38 dp column: the 12 dp glyph, 14 dp in from the field's right edge.
    ///
    /// The reference rotates the glyph while open; the engine has no rotation, so its
    /// up-pointing twin stands in.
    fn chevron_decl(&self, focused: bool, slot: usize) -> Decl {
        let ink = match (focused, self.pressed) {
            (true, true) => self.glyph_focused_pressed,
            (true, false) => self.glyph_focused,
            (false, _) => self.glyph[slot],
        };
        let mut column = Decl::container(self.id.with("chevron"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(CHEVRON_COLUMN),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: 0.0,
                top: 0.0,
                right: CHEVRON_MARGIN,
                bottom: 0.0,
            })
            .cross_align(0.0)
            .cross_pos(1.0);
        let name = match self.open {
            true => "chevron_up_12_regular",
            false => "chevron_down_12_regular",
        };
        if let Some(source) = glyphs::icon(name) {
            column = column.child(
                Icon {
                    id: self.id.with("chevron-glyph"),
                    source,
                    size: CHEVRON_SIZE,
                    ink,
                }
                .resolve(),
            );
        }
        column
    }

    /// One row of the drop-down: its wash, its pill when selected, and its label.
    fn row_decl(&self, row: &Row) -> Decl {
        let mut line = Decl::container(row.id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .margin(COMBO_ITEM_MARGIN)
            .background(
                self.item_fill[row.brushes][row.slot],
                Corners::all(COMBO_ITEM_RADIUS),
            )
            // A closing panel takes nothing: the row is inert while the surface fades out.
            .sense(match self.open {
                true => Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE,
                false => Sense::NONE,
            })
            .cursor(CursorShape::Arrow)
            .disabled(row.disabled)
            .focus_visual(Some(self.item_focus))
            .cross_align(0.0)
            .cross_pos(0.5);
        if row.selected {
            // The pill takes the row's 11 dp lead-in: 3 wide with 8 after it leaves the
            // label at exactly the x an unselected row's padding puts it.
            line = line.child(
                Decl::container(row.id.with("pill"), NodeKind::Column)
                    .size(SizeClass {
                        width: AxisSize::fixed(PILL_W),
                        height: AxisSize::hug(0.0, 0.0),
                        grow_x: Grow::No,
                        grow_y: Grow::No,
                    })
                    .margin(Insets {
                        left: 0.0,
                        top: 0.0,
                        right: COMBO_ITEM_PAD.left - PILL_W,
                        bottom: 0.0,
                    })
                    .cross_align(0.0)
                    .cross_pos(0.5)
                    .child(
                        Decl::container(row.id.with("pill-bar"), NodeKind::Box)
                            .size(SizeClass::fixed(PILL_W, PILL_H))
                            .background(self.pill, Corners::all(PILL_RADIUS)),
                    ),
            );
        }
        // The item's padding rides on the content, as in the reference, so the pill is centred
        // against the whole row rather than the text's inset box.
        line.child(
            Decl::container(row.id.with("content"), NodeKind::Row)
                .size(SizeClass {
                    width: AxisSize::hug(0.0, 0.0),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::No,
                })
                .padding(Insets {
                    left: match row.selected {
                        true => 0.0,
                        false => COMBO_ITEM_PAD.left,
                    },
                    top: COMBO_ITEM_PAD.top,
                    right: COMBO_ITEM_PAD.right,
                    bottom: COMBO_ITEM_PAD.bottom,
                })
                .cross_align(0.0)
                .cross_pos(0.5)
                .child(
                    Text::new(
                        row.id.with("label"),
                        row.label.clone(),
                        self.label_size,
                        self.item_ink[row.brushes][row.slot],
                    )
                    .weight(self.label_weight)
                    .nowrap()
                    .resolve(),
                ),
        )
    }

    /// The drop-down: one acrylic panel under the field, at the field's own width.
    fn dropdown_decl(&self) -> Decl {
        let id = self.id.with("dropdown");
        let placement = OverlayPlacement {
            anchor: Anchor::Node(self.id.with("box")),
            side: Side::Below,
            align: OverlayAlign::Start,
            // WinUI pulls the panel's border half a point under the field.
            offset: -0.5,
            flip: true,
            match_anchor_width: true,
            match_anchor_height: false,
            min_width: POPUP_MIN_WIDTH,
            presentation: Presentation::Parent,
        };
        let corners = Corners::all(POPUP_RADIUS);
        // The window is the one the field's width is applied to, so the panel fills it.
        let mut panel = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .corners(corners)
            // `ComboBoxDropDownBackground` is the in-app acrylic brush, which the backdrop
            // pass paints; the box itself only carries the shape.
            .background(Color::TRANSPARENT, corners)
            .border(self.dropdown_stroke, POPUP_BORDER);
        // A combo box opens onto its selected row; the window is the popup the engine asks.
        let initial = self
            .selected
            .and_then(|k| self.rows.get(k))
            .map(|row| row.id);
        if let Some(p) = panel.data.params_mut() {
            p.backdrop_blur = Some(BackdropBlur {
                radius: (3.0_f32 * self.blur_sigma).ceil().max(1.0) as u32,
                sigma: self.blur_sigma,
                material: BackdropMaterial::Acrylic(self.acrylic),
            });
        }
        let mut list = Decl::container(self.id.with("dropdown-list"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(POPUP_CONTENT_PAD);
        for row in &self.rows {
            list = list.child(self.row_decl(row));
        }
        // The window takes the keyboard while the panel is open, like a flyout's.
        let mut window = popup_window(self.id.with("popup"))
            .overlay(placement)
            .sense(match self.open {
                true => Sense::FOCUSABLE | Sense::HOVER,
                false => Sense::NONE,
            });
        if let Some(id) = initial {
            window = window.focus_initial(id);
        }
        window.child(self.motion.apply(panel.child(list)))
    }
}

impl Widget for FluentComboBox {
    fn handles_disabled(&self) -> bool {
        // The disabled brushes are painted here; the adapter must not fade a second time.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        // A row took the click or Enter: that is the pick, whatever the field's own state.
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        // Closing outranks opening: a press on the field while the panel is open is the
        // outside press the engine reports as a dismissal.
        if self.dismissed {
            return Some(Change::Dismissed);
        }
        if let Some(k) = self.step {
            // Stepping onto the row already selected is not a change.
            return (Some(k) != self.selected).then(|| Change::Selected(Some(self.rows[k].id)));
        }
        self.toggled.then_some(Change::Activated)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let id = self.id;
        let mut root = Decl::container(id, NodeKind::Column)
            .size(SizeClass {
                width: match self.width {
                    Some(w) => AxisSize::fixed(w),
                    None => AxisSize::hug(self.min_width, 0.0),
                },
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .gap(0.0);
        // The chords live on the root: it is the one ancestor of both the field and the
        // panel, so they fire wherever the keyboard sits inside the control.
        root = match self.open {
            true => root
                .shortcut(KeyChord::new(Key::Escape, Modifiers::NONE), COMBO_DISMISS)
                .shortcut(alt(Key::Up), COMBO_DISMISS),
            // Alt+Down opens; the bare arrows walk the list while the panel is shut. While
            // it is open they belong to the focus ring inside the panel instead.
            false => root
                .shortcut(alt(Key::Down), COMBO_TOGGLE)
                .shortcut(KeyChord::new(Key::Down, Modifiers::NONE), COMBO_NEXT)
                .shortcut(KeyChord::new(Key::Up, Modifiers::NONE), COMBO_PREV)
                .shortcut(KeyChord::new(Key::Home, Modifiers::NONE), COMBO_FIRST)
                .shortcut(KeyChord::new(Key::End, Modifiers::NONE), COMBO_LAST),
        };
        if let Some(header) = &self.header {
            root = root.child(
                Text::new(
                    id.with("header"),
                    header.clone(),
                    self.header_size,
                    self.header_ink,
                )
                .nowrap()
                .resolve()
                .margin(Insets {
                    left: 0.0,
                    top: 0.0,
                    right: 0.0,
                    bottom: HEADER_GAP,
                }),
            );
        }
        root = root.child(self.box_decl());
        if self.motion.alive {
            root = root.child(self.dropdown_decl());
        }
        root
    }
}

/// Alt plus `key`: the reference's own open/close chords.
fn alt(key: Key) -> KeyChord {
    KeyChord::new(
        key,
        Modifiers {
            alt: true,
            ..Modifiers::NONE
        },
    )
}

/// The next enabled row in `forward` order, wrapping; `None` when every row is disabled.
pub(crate) fn neighbour(
    items: &[ComboBoxItem],
    selected: Option<usize>,
    forward: bool,
) -> Option<usize> {
    let n = items.len();
    if n == 0 {
        return None;
    }
    let start = match selected {
        Some(k) => k as isize,
        None if forward => -1,
        None => 0,
    };
    for step in 1..=n {
        let step = step as isize;
        let delta = match forward {
            true => step,
            false => -step,
        };
        let k = (start + delta).rem_euclid(n as isize) as usize;
        if !items[k].disabled {
            return Some(k);
        }
    }
    None
}
