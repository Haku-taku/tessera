//! Context-menu items a node declares, and the engine shows.

use crate::id::Id;
use crate::shortcut::KeyChord;
use crate::text::Family;

/// The mark a quick-action row draws above its label: one glyph in one family.
///
/// The app names the same mark it draws elsewhere; the menu shapes the glyph itself rather
/// than carrying an icon table the app would have to keep in step with its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuIcon {
    pub family: Family,
    pub glyph: char,
}

impl MenuIcon {
    pub const fn new(family: Family, glyph: char) -> Self {
        Self { family, glyph }
    }
}

/// What one menu row is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuRow {
    /// A command the menu reports when chosen.
    Item,
    /// A divider between groups; it is never hovered or chosen.
    Separator,
    /// A header that opens its own rows beside the menu.
    Submenu(Vec<MenuItem>),
}

/// One row of a context menu.
///
/// The label is the app's: the engine never invents text, only lays it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// Stable id, reported back when the row is chosen.
    pub id: Id,
    pub label: String,
    /// The chord the row runs, shown at its trailing edge, already in the app's words. A
    /// strip cell keeps the binding but draws no chord text.
    pub shortcut: Option<String>,
    /// The same chord in machine form, so pressing it while the menu is open picks the row.
    pub accelerator: Option<KeyChord>,
    /// A greyed row that cannot be chosen, e.g. Copy with nothing selected.
    pub enabled: bool,
    pub kind: MenuRow,
    /// Draws a check mark in the leading column.
    pub checked: bool,
    /// Draws the label boldly, as a dialog's default action is drawn.
    pub is_default: bool,
    /// Why the row is disabled; painted as a second, muted line when present. A strip cell
    /// has no room for the line and draws only the muted ink.
    pub reason: Option<String>,
    /// The glyph drawn over this row's label when it stands in a quick-action strip.
    pub icon: Option<MenuIcon>,
    /// Stand this row beside its neighbours in the menu's strip of icon-over-label cells
    /// rather than as a full-width row of its own.
    pub compact: bool,
}

impl MenuItem {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            shortcut: None,
            accelerator: None,
            enabled: true,
            kind: MenuRow::Item,
            checked: false,
            is_default: false,
            reason: None,
            icon: None,
            compact: false,
        }
    }

    /// A divider between groups of rows.
    pub fn separator() -> Self {
        Self {
            kind: MenuRow::Separator,
            ..Self::new(Id::default(), String::new())
        }
    }

    /// A header whose `items` open beside the menu when it is hovered.
    pub fn submenu(id: Id, label: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            kind: MenuRow::Submenu(items),
            ..Self::new(id, label)
        }
    }

    /// Show `shortcut` at the row's trailing edge.
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Pick this row when `chord` is pressed while the menu is open.
    pub fn with_accelerator(mut self, chord: KeyChord) -> Self {
        self.accelerator = Some(chord);
        self
    }

    /// Bind the key that picks this row while the menu is open; `shortcut` is how the row
    /// spells it, so the label and the press can never disagree.
    pub fn with_chord(self, chord: KeyChord, shortcut: impl Into<String>) -> Self {
        self.with_accelerator(chord).with_shortcut(shortcut)
    }

    /// A row that is shown but cannot be chosen.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// A disabled row that also says why, in the app's words.
    pub fn disabled_because(mut self, reason: impl Into<String>) -> Self {
        self.enabled = false;
        self.reason = Some(reason.into());
        self
    }

    /// Draw a check mark in the row's leading column.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Draw this row as the menu's default action.
    pub fn default_row(mut self) -> Self {
        self.is_default = true;
        self
    }

    /// Draw this row as a cell of the menu's quick-action strip, with `icon` over its label.
    pub fn compact(mut self, icon: MenuIcon) -> Self {
        self.compact = true;
        self.icon = Some(icon);
        self
    }
}
