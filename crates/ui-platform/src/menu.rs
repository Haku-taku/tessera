//! The engine's context menu: what a click collects, and the state a drawn panel reads.
//!
//! A design system replaces the look through [`MenuStyle`]; the menu itself is a declared
//! panel like any other, so layout, input and the platform's popup surface are the engine's
//! ordinary ones instead of a second painter.

use ui_core::Id;
use ui_core::geom::{Color, Point};
use ui_core::menu::{MenuIcon, MenuItem, MenuRow};
use ui_core::text::{TextBlock, TextStyle, Variations, Weight};

/// How the menu looks.
#[derive(Clone, Debug)]
pub struct MenuStyle {
    pub surface: Color,
    pub ink: Color,
    /// Rows that cannot be chosen and the second line that says why.
    pub muted: Color,
    pub hover: Color,
    /// `None` paints no border; MD3's menu is bounded by its shadow alone.
    pub border: Option<(Color, f32)>,
    pub font_size: f32,
    /// The ink, size and weight of the chord shown at a row's trailing edge.
    pub shortcut_ink: Color,
    pub shortcut_size: f32,
    pub shortcut_weight: Weight,
    /// Space between a label and its shortcut, and the inset from the trailing edge.
    pub shortcut_gap: f32,
    pub trailing_space: f32,
    pub row_height: f32,
    /// A label's inset from the container's edge; a chord mirrors it on the trailing side.
    pub pad_x: f32,
    pub pad_y: f32,
    /// The horizontal inset of a row's own shape: its hover fill and hit box shrink by this
    /// much on each side, so a row reads as a shape inside the container.
    pub row_inset: f32,
    pub radius: f32,
    pub min_width: f32,
    /// The widest the menu gets; a row that would be wider has its label ellipsized. MD3
    /// caps it.
    pub max_width: f32,
    pub shadow: f32,
    /// A frost behind the surface, which then needs a translucent fill to show it.
    pub frost: Option<ui_core::BackdropBlur>,
    /// The strip of icon-over-label cells: its height, the icon and label sizes drawn in a
    /// cell, the cell's side padding and the gap between its icon and its label.
    pub compact_height: f32,
    pub compact_icon_size: f32,
    pub compact_label_size: f32,
    pub compact_pad_x: f32,
    pub compact_gap: f32,
}

impl Default for MenuStyle {
    fn default() -> Self {
        let ink = Color::from_rgba8(232, 232, 236, 255);
        Self {
            surface: Color::from_rgba8(44, 46, 54, 255),
            ink,
            muted: Color::from_rgba8(140, 142, 150, 255),
            hover: Color::from_rgba8(255, 255, 255, 18),
            border: Some((Color::from_rgba8(255, 255, 255, 30), 1.0)),
            font_size: 14.0,
            // The chord is the row's own ink at reduced emphasis, so it follows whatever
            // theme supplied that ink instead of carrying a colour of its own.
            shortcut_ink: Color {
                a: ink.a * 0.7,
                ..ink
            },
            shortcut_size: 12.0,
            shortcut_weight: Weight::MEDIUM,
            shortcut_gap: 16.0,
            trailing_space: 12.0,
            row_height: 34.0,
            pad_x: 14.0,
            pad_y: 6.0,
            row_inset: 4.0,
            radius: 8.0,
            min_width: 180.0,
            max_width: f32::INFINITY,
            shadow: 10.0,
            frost: None,
            compact_height: 52.0,
            // The compact strip is Win11's shape, not MD3's: a small mark over a small label.
            compact_icon_size: 20.0,
            compact_label_size: 12.0,
            compact_pad_x: 6.0,
            compact_gap: 2.0,
        }
    }
}

impl MenuStyle {
    /// The text block one row's label is shaped as.
    pub fn block(&self, label: &str) -> TextBlock {
        self.label_block(label, false)
    }

    /// A label block, bold for the menu's default row.
    pub fn label_block(&self, label: &str, bold: bool) -> TextBlock {
        let style = TextStyle::new(self.font_size);
        let style = match bold {
            true => style.weight(Weight::BOLD),
            false => style,
        };
        TextBlock::plain(label, style)
    }

    /// The text block one row's shortcut is shaped as.
    pub fn shortcut_block(&self, shortcut: &str) -> TextBlock {
        TextBlock::plain(
            shortcut,
            TextStyle::new(self.shortcut_size).weight(self.shortcut_weight),
        )
    }

    /// The text block a disabled row's reason is shaped as.
    pub fn reason_block(&self, reason: &str) -> TextBlock {
        TextBlock::plain(reason, TextStyle::new(self.shortcut_size))
    }

    /// The mark a checked row draws in its leading column.
    pub fn check_block(&self) -> TextBlock {
        TextBlock::plain(
            "\u{2713}",
            TextStyle::new(self.font_size * 0.9).weight(Weight::MEDIUM),
        )
    }

    /// The mark a submenu header draws at its trailing edge, so a cascade is visible.
    pub fn chevron_block(&self) -> TextBlock {
        TextBlock::plain(SUBMENU_MARK, TextStyle::new(self.font_size))
    }

    /// The label one strip cell draws under its icon.
    pub fn compact_label_block(&self, label: &str, bold: bool) -> TextBlock {
        let style = TextStyle::new(self.compact_label_size);
        let style = match bold {
            true => style.weight(Weight::BOLD),
            false => style,
        };
        TextBlock::plain(label, style)
    }

    /// The glyph one strip cell draws over its label.
    pub fn icon_block(&self, icon: MenuIcon) -> TextBlock {
        // The optical size axis follows the drawn size, as the icon slot resolves it.
        let style = TextStyle::new(self.compact_icon_size)
            .family(icon.family)
            .variations(
                Variations::NONE.optical_size(self.compact_icon_size),
            );
        TextBlock::plain(icon.glyph.to_string(), style)
    }
}

/// One built-in text row's words: what it says and, if it has one, the chord it runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuRowText {
    pub label: String,
    /// Already in the app's words; the engine only right-aligns it.
    pub shortcut: Option<String>,
}

/// The app's words for the engine's built-in text rows.
///
/// The engine owns *which* rows exist and whether each is enabled; an app owns every word.
/// Until an app supplies these, the engine contributes no text rows at all — it never
/// invents text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextMenuLabels {
    pub select_all: MenuRowText,
    pub copy: MenuRowText,
    pub cut: MenuRowText,
    pub paste: MenuRowText,
}

/// A divider is a thin line between groups, not a row a pointer can aim at.
pub(crate) const SEPARATOR_HEIGHT: f32 = 9.0;
/// The mark a submenu header carries; the UI font has it, an icon face would have to be
/// registered for a menu that otherwise draws only text.
pub(crate) const SUBMENU_MARK: &str = "\u{203A}";
/// The leading column a checked row's mark sits in.
pub(crate) const CHECK_GAP: f32 = 6.0;
/// Space between a disabled row's label and the reason below it.
pub(crate) const REASON_GAP: f32 = 2.0;

/// WinUI's `ControlFastAnimationDuration`, decelerating on (0,0,0,1).
pub(crate) const ENTRANCE: ui_core::anim::AnimSpec =
    ui_core::anim::AnimSpec::new(0.167, ui_core::anim::Easing::Cubic(0.0, 0.0, 0.0, 1.0));

/// What Enter did on an open menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuEnter {
    Opened,
    Pick(Id),
    Nothing,
}

/// One open menu level: its rows, the row the user is on, and the flyout that row opened.
#[derive(Clone, Debug)]
pub(crate) struct Menu {
    /// The node whose menu this is; every pick and every dismissal is reported to it.
    pub owner: Id,
    pub items: Vec<MenuItem>,
    /// The point the root level was asked for, which is where its panel anchors.
    pub at: Point,
    /// The row the keys and the pointer agree is current.
    pub hovered: Option<usize>,
    /// The row the pointer was last seen over, so a resting pointer cannot undo a key move.
    pub pointer_hovered: Option<usize>,
    /// The one child flyout open at a time, and the row that opened it.
    pub child: Option<Box<Menu>>,
    /// The row of the level above that heads this level, and what its panel anchors to; the
    /// root level has none. It lives here so a flyout always knows what it hangs off.
    pub header: Option<usize>,
}

impl Menu {
    /// One open level. Its width is a layout fact and is measured when the frame declares
    /// it, because a menu that outlives a font load would otherwise keep the old metrics.
    pub fn open(owner: Id, items: Vec<MenuItem>, at: Point) -> Self {
        Self {
            owner,
            items,
            at,
            hovered: None,
            pointer_hovered: None,
            child: None,
            header: None,
        }
    }

    /// The id a level's panel is declared under; the platform reports its dismissal to it.
    pub fn panel_id(depth: usize) -> Id {
        Id::of(&("ui-platform/menu", depth))
    }

    /// The id one command row is declared under, which is what a click reports.
    pub fn row_id(depth: usize, index: usize) -> Id {
        Id::of(&("ui-platform/menu/row", depth, index))
    }

    /// How many flyouts are open below the root level.
    pub fn depth(&self) -> usize {
        match &self.child {
            Some(child) => child.depth() + 1,
            None => 0,
        }
    }

    /// Swap a level's rows in place: same panel, same point, no second entrance.
    ///
    /// A flyout is closed, because the rows it was opened from may not be the same rows now;
    /// the hover stays on its index while that index is still a row the user can be on.
    pub fn replace_items(&mut self, items: Vec<MenuItem>) -> bool {
        if self.items == items {
            return false;
        }
        self.items = items;
        if self.hovered.is_some_and(|index| !self.hoverable(index)) {
            self.hovered = None;
        }
        self.pointer_hovered = None;
        self.child = None;
        true
    }

    /// Whether any row of this level carries a mark, so every label makes room for one.
    pub fn has_check(&self) -> bool {
        self.items.iter().any(|item| item.checked)
    }

    /// The level `depth` flyouts down, when it is open.
    pub fn level_mut(&mut self, depth: usize) -> Option<&mut Menu> {
        let mut level = self;
        for _ in 0..depth {
            level = level.child.as_mut().map(|child| &mut **child)?;
        }
        Some(level)
    }

    /// The row `id` names, as its level and index.
    pub fn locate(&self, id: Id) -> Option<(usize, usize)> {
        let mut level = self;
        for depth in 0..=self.depth() {
            for index in 0..level.items.len() {
                if Self::row_id(depth, index) == id {
                    return Some((depth, index));
                }
            }
            level = level.child.as_deref()?;
        }
        None
    }

    /// Whether a row can be hovered or opened: a command or a submenu header, enabled.
    pub fn hoverable(&self, index: usize) -> bool {
        self.items.get(index).is_some_and(|item| {
            item.enabled && !matches!(item.kind, MenuRow::Separator)
        })
    }

    fn next_row(&self, from: Option<usize>) -> Option<usize> {
        (from.map_or(0, |index| index + 1)..self.items.len()).find(|index| self.hoverable(*index))
    }

    fn previous_row(&self, from: Option<usize>) -> Option<usize> {
        let end = from.unwrap_or(self.items.len()).min(self.items.len());
        (0..end).rev().find(|index| self.hoverable(*index))
    }

    /// Run `f` over the deepest open level: the keys act there.
    fn with_deepest<R>(&mut self, f: impl FnOnce(&mut Menu) -> R) -> R {
        if let Some(child) = self.child.as_mut() {
            return child.with_deepest(f);
        }
        f(self)
    }

    fn default_row(&self) -> Option<usize> {
        self.items.iter().position(|item| {
            item.enabled && item.is_default && matches!(item.kind, MenuRow::Item)
        })
    }

    /// Move the hover one row, clamped at either end.
    pub fn move_hover(&mut self, forward: bool) -> bool {
        self.with_deepest(|level| {
            let moved = match forward {
                true => level.next_row(level.hovered),
                false => level.previous_row(level.hovered),
            };
            // Clamping at the ends keeps a held key from making the highlight disappear.
            let next = moved.or(level.hovered);
            let changed = level.hovered != next;
            level.hovered = next;
            changed
        })
    }

    /// Move the hover to the first or last hoverable row.
    pub fn move_hover_edge(&mut self, last: bool) -> bool {
        self.with_deepest(|level| {
            let next = match last {
                true => level.previous_row(Some(level.items.len())),
                false => level.next_row(None),
            };
            let changed = level.hovered != next;
            level.hovered = next;
            changed
        })
    }

    /// Take the pointer's verdict for this level, as the build saw it.
    ///
    /// The observation is only a move when it differs from the last one: a pointer at rest
    /// over a row would otherwise drag the highlight back from where the keys put it, and a
    /// pointer that was never over the level must not clear a keyboard move either.
    pub fn pointer_over(&mut self, hovered: Option<usize>) -> bool {
        if hovered == self.pointer_hovered {
            return false;
        }
        let was_over = self.pointer_hovered.is_some();
        self.pointer_hovered = hovered;
        let Some(index) = hovered else {
            // A pointer that leaves the row it was on drops the highlight it left behind. A
            // level it never entered keeps the keys' hover, and an open flyout keeps its
            // header lit; a press outside is a dismissal the platform reports separately.
            if was_over && self.child.is_none() && self.hovered.take().is_some() {
                return true;
            }
            return false;
        };
        let changed = self.hovered != Some(index);
        self.hovered = Some(index);
        let is_submenu = self
            .items
            .get(index)
            .is_some_and(|item| item.enabled && matches!(item.kind, MenuRow::Submenu(_)));
        if is_submenu {
            changed | self.open_child(index)
        } else if self.child.take().is_some() {
            true
        } else {
            changed
        }
    }

    /// Open the hovered submenu header's flyout, when the hover is one.
    pub fn open_hovered_submenu(&mut self) -> bool {
        self.with_deepest(|level| {
            let Some(index) = level.hovered else {
                return false;
            };
            if level.child.as_ref().is_some_and(|child| child.header == Some(index)) {
                return false;
            }
            level.open_child(index)
        })
    }

    /// Close the deepest open flyout; false when none was open.
    pub fn close_deepest_submenu(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        if child.child.is_some() {
            return child.close_deepest_submenu();
        }
        self.child = None;
        true
    }

    /// Activate the hovered row, or the default row when nothing is hovered.
    pub fn enter(&mut self) -> MenuEnter {
        self.with_deepest(|level| {
            let Some(index) = level.hovered.or_else(|| level.default_row()) else {
                return MenuEnter::Nothing;
            };
            let (enabled, is_submenu, is_separator, id) = match level.items.get(index) {
                Some(item) => (
                    item.enabled,
                    matches!(item.kind, MenuRow::Submenu(_)),
                    matches!(item.kind, MenuRow::Separator),
                    item.id,
                ),
                None => return MenuEnter::Nothing,
            };
            if !enabled || is_separator {
                return MenuEnter::Nothing;
            }
            if is_submenu {
                return match level.open_child(index) {
                    true => MenuEnter::Opened,
                    false => MenuEnter::Nothing,
                };
            }
            MenuEnter::Pick(id)
        })
    }

    /// The enabled command row whose chord was pressed, in this level only.
    ///
    /// A row that names every modifier it needs outranks one that leaves a modifier free,
    /// as the binding tables resolve the same press.
    pub fn accelerator_at(&self, key: ui_core::Key, mods: ui_core::Modifiers) -> Option<usize> {
        let mut best: Option<(u32, usize)> = None;
        for (index, item) in self.items.iter().enumerate() {
            if !item.enabled || !matches!(item.kind, MenuRow::Item) {
                continue;
            }
            let Some(chord) = item.accelerator.filter(|chord| chord.matches(key, mods)) else {
                continue;
            };
            if best.is_none_or(|(seen, _)| chord.freedom() < seen) {
                best = Some((chord.freedom(), index));
            }
        }
        best.map(|(_, index)| index)
    }

    /// The command whose chord was pressed, searching an open flyout before this level.
    pub fn accelerator_item(&self, key: ui_core::Key, mods: ui_core::Modifiers) -> Option<Id> {
        if let Some(child) = &self.child
            && let Some(id) = child.accelerator_item(key, mods)
        {
            return Some(id);
        }
        self.accelerator_at(key, mods)
            .and_then(|index| self.items.get(index))
            .map(|item| item.id)
    }

    /// Open the flyout `index` heads, when it is an enabled submenu row.
    pub fn open_row(&mut self, index: usize) -> bool {
        self.open_child(index)
    }

    /// Open a child flyout beside `index`, which the child records as the row it hangs off.
    fn open_child(&mut self, index: usize) -> bool {
        let Some(item) = self.items.get(index) else {
            return false;
        };
        if !item.enabled {
            return false;
        }
        let MenuRow::Submenu(items) = &item.kind else {
            return false;
        };
        if items.is_empty() {
            return false;
        }
        let items = items.clone();
        self.child = Some(Box::new(Menu {
            owner: self.owner,
            items,
            at: self.at,
            hovered: None,
            pointer_hovered: None,
            child: None,
            header: Some(index),
        }));
        true
    }
}

/// Whether `item` stands in a strip of quick actions rather than as a row of its own.
pub(crate) fn in_strip(item: &MenuItem) -> bool {
    item.compact && matches!(item.kind, MenuRow::Item)
}

/// The end of the strip run starting at `start`, which is `start` when no run starts there.
pub(crate) fn strip_end(items: &[MenuItem], start: usize) -> usize {
    let mut end = start;
    while end < items.len() && in_strip(&items[end]) {
        end += 1;
    }
    end
}

/// The height a level's rows take, with every strip run counting as one cell row.
pub(crate) fn level_height(style: &MenuStyle, items: &[MenuItem]) -> f32 {
    let mut total = 0.0;
    let mut index = 0;
    while index < items.len() {
        let end = strip_end(items, index);
        if end > index {
            total += style.compact_height;
            index = end;
        } else {
            total += row_height(style, &items[index]);
            index += 1;
        }
    }
    total
}

/// The room a row's trailing marks take: its chord and, for a header, its cascade mark.
pub(crate) fn trailing_width(
    style: &MenuStyle,
    item: &MenuItem,
    width_of: &mut impl FnMut(&TextBlock) -> f32,
) -> f32 {
    let chord = item.shortcut.as_deref().map_or(0.0, |shortcut| {
        style.shortcut_gap + width_of(&style.shortcut_block(shortcut))
    });
    match item.kind {
        MenuRow::Submenu(_) => chord + style.shortcut_gap + width_of(&style.chevron_block()),
        _ => chord,
    }
}

/// A row's height: a divider is thin, and a disabled row carrying its reason grows a line.
pub(crate) fn row_height(style: &MenuStyle, item: &MenuItem) -> f32 {
    match &item.kind {
        MenuRow::Separator => SEPARATOR_HEIGHT,
        MenuRow::Item | MenuRow::Submenu(_) if in_strip(item) => style.compact_height,
        MenuRow::Item | MenuRow::Submenu(_) => match reason_of(item) {
            Some(_) => style.row_height + REASON_GAP + reason_line(style),
            None => style.row_height,
        },
    }
}

/// The reason a row carries when it is disabled and says why.
pub(crate) fn reason_of(item: &MenuItem) -> Option<&str> {
    item.reason
        .as_deref()
        .filter(|reason| !item.enabled && !reason.is_empty())
}

/// The line height a row's reason is laid out at.
pub(crate) fn reason_line(style: &MenuStyle) -> f32 {
    style.shortcut_size * 1.25
}

/// The leading column reserved for check marks, so checked and unchecked labels align.
pub(crate) fn check_column(style: &MenuStyle) -> f32 {
    style.font_size * 0.9 + CHECK_GAP
}

/// The width a level's rows share: the widest row, measured from shaped text.
pub(crate) fn level_width(
    style: &MenuStyle,
    items: &[MenuItem],
    width_of: &mut impl FnMut(&TextBlock) -> f32,
) -> f32 {
    let check = match items.iter().any(|item| item.checked) {
        true => check_column(style),
        false => 0.0,
    };
    let trailing = style.pad_x.max(style.trailing_space);
    // Each row's box is inset from the panel's edge, and that inset is not part of the
    // label's room: the widest row has to fit inside what the panel leaves it.
    let margin = 2.0 * style.row_inset;
    let mut widest = style.min_width;
    let mut index = 0;
    while index < items.len() {
        let end = strip_end(items, index);
        if end > index {
            // A strip's cells share its width equally, so its widest cell sets all of them.
            let mut cell: f32 = 0.0;
            for item in &items[index..end] {
                let label = width_of(&style.compact_label_block(&item.label, item.is_default));
                cell = cell.max(label.max(style.compact_icon_size) + 2.0 * style.compact_pad_x);
            }
            widest = widest.max(margin + cell * (end - index) as f32);
            index = end;
            continue;
        }
        let item = &items[index];
        index += 1;
        if matches!(item.kind, MenuRow::Separator) {
            continue;
        }
        let label = width_of(&style.label_block(&item.label, item.is_default));
        let mut row = margin
            + check
            + label
            + trailing_width(style, item, width_of)
            + style.pad_x
            + trailing;
        if let Some(reason) = reason_of(item) {
            row = row.max(
                margin + check + width_of(&style.reason_block(reason)) + style.pad_x + trailing,
            );
        }
        widest = widest.max(row);
    }
    widest.min(style.max_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::menu::MenuItem;

    /// A pointer that leaves the row it was on drops the highlight it left behind; a level
    /// the pointer never entered keeps the keyboard's, and an open flyout keeps its header.
    #[test]
    fn a_pointer_leaving_a_level_drops_its_hover_but_never_a_keyboards() {
        let mut level = Menu::open(
            Id::str("owner"),
            vec![
                MenuItem::new(Id::str("one"), "One"),
                MenuItem::new(Id::str("two"), "Two"),
            ],
            Point::ZERO,
        );
        assert!(level.move_hover(true));
        assert_eq!(level.hovered, Some(0));
        assert!(
            !level.pointer_over(None),
            "a pointer that was never over the level must not undo the keys"
        );
        assert_eq!(level.hovered, Some(0));

        assert!(level.pointer_over(Some(1)));
        assert_eq!(level.hovered, Some(1));
        assert!(level.pointer_over(None), "the row it left is unlit");
        assert_eq!(level.hovered, None, "no row keeps a highlight the pointer left");

        let mut level = Menu::open(
            Id::str("owner"),
            vec![MenuItem::submenu(
                Id::str("more"),
                "More",
                vec![MenuItem::new(Id::str("a"), "A")],
            )],
            Point::ZERO,
        );
        assert!(level.pointer_over(Some(0)), "the header opens its flyout");
        assert!(level.child.is_some());
        assert!(
            !level.pointer_over(None),
            "a flyout still open keeps the header that heads it lit"
        );
        assert_eq!(level.hovered, Some(0));
    }
}
