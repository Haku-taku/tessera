//! Context-menu items a node declares, and the engine shows.

use crate::id::Id;
use crate::shortcut::KeyChord;

/// One row of a context menu.
///
/// The label is the app's: the engine never invents text, only lays it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// Stable id, reported back when the row is chosen.
    pub id: Id,
    pub label: String,
    /// The chord the row runs, shown at its trailing edge, already in the app's words.
    pub shortcut: Option<String>,
    /// The same chord in machine form, so pressing it while the menu is open picks the row.
    pub accelerator: Option<KeyChord>,
    /// A greyed row that cannot be chosen, e.g. Copy with nothing selected.
    pub enabled: bool,
}

impl MenuItem {
    pub fn new(id: Id, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            shortcut: None,
            accelerator: None,
            enabled: true,
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
}
