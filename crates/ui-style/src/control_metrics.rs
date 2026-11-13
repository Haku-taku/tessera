//! Per-control geometry: one row per control kind, two densities.
//!
//! Only the envelope every control shares lives here. Control-specific shapes
//! (checkbox box, slider thumb, switch track) stay in the control's own spec.

use crate::types::TextRole;
use ui_core::geom::Insets;

/// The control kinds a pack may size; the index is the table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ControlKind {
    Button = 0,
    CheckBox,
    RadioButton,
    ToggleSwitch,
    Slider,
    TextBox,
    ComboBox,
    ListViewItem,
    NavigationViewItem,
    TabViewItem,
    MenuFlyoutItem,
    InfoBar,
    ContentDialog,
    ToolTip,
    ProgressBar,
    ProgressRing,
}

impl ControlKind {
    pub const COUNT: usize = 16;

    pub const ALL: [Self; Self::COUNT] = [
        Self::Button,
        Self::CheckBox,
        Self::RadioButton,
        Self::ToggleSwitch,
        Self::Slider,
        Self::TextBox,
        Self::ComboBox,
        Self::ListViewItem,
        Self::NavigationViewItem,
        Self::TabViewItem,
        Self::MenuFlyoutItem,
        Self::InfoBar,
        Self::ContentDialog,
        Self::ToolTip,
        Self::ProgressBar,
        Self::ProgressRing,
    ];
}

/// Touch density. WinUI 3 ships no density dictionary; a pack that has one fills `Compact`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Standard,
    Compact,
}

/// The size envelope of one control: floor, insets, outline, focus reach, glyph, type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlMetrics {
    pub min_width: f32,
    pub min_height: f32,
    pub padding: Insets,
    /// Uniform border width; per-side thicknesses live in the control's spec.
    pub border: f32,
    pub radius: f32,
    /// How far the focus adorner reaches outside the box, as XAML `FocusVisualMargin`.
    pub focus_margin: Insets,
    /// Glyph box side; `0` when the control carries no glyph of its own.
    pub icon_size: f32,
    pub font: TextRole,
}

impl ControlMetrics {
    /// No floor, no inset, no outline: the row of a control nobody has sized yet.
    pub const EMPTY: Self = Self {
        min_width: 0.0,
        min_height: 0.0,
        padding: Insets::ZERO,
        border: 0.0,
        radius: 0.0,
        focus_margin: Insets::ZERO,
        icon_size: 0.0,
        font: TextRole::Body,
    };
}

/// Standard and compact rows for every `ControlKind`, indexed by the kind's discriminant.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlMetricsTable {
    standard: [ControlMetrics; ControlKind::COUNT],
    compact: [ControlMetrics; ControlKind::COUNT],
}

impl ControlMetricsTable {
    pub const EMPTY: Self = Self {
        standard: [ControlMetrics::EMPTY; ControlKind::COUNT],
        compact: [ControlMetrics::EMPTY; ControlKind::COUNT],
    };

    /// A pack with one density: compact reads the same rows.
    pub const fn from_standard(standard: [ControlMetrics; ControlKind::COUNT]) -> Self {
        Self {
            standard,
            compact: standard,
        }
    }

    pub fn get(&self, kind: ControlKind, density: Density) -> &ControlMetrics {
        match density {
            Density::Standard => &self.standard[kind as usize],
            Density::Compact => &self.compact[kind as usize],
        }
    }

    /// Fill one compact row, leaving every other kind on its standard row.
    pub fn set_compact(&mut self, kind: ControlKind, metrics: ControlMetrics) {
        self.compact[kind as usize] = metrics;
    }
}

impl Default for ControlMetricsTable {
    fn default() -> Self {
        Self::EMPTY
    }
}
