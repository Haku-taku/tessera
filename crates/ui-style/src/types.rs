//! Text and metric tokens: what size a title is, what padding a card carries.

use ui_core::Weight;
use ui_core::geom::{CornerShape, Insets};

/// The named text styles widgets may ask for. Fluent ships eight of these
/// (`typography.dart:42-78`); five covers what the built-in widgets need today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    /// Largest heading a surface carries.
    Title,
    /// Secondary heading, still bold.
    Subtitle,
    /// Running text, possibly wrapped.
    Body,
    /// Compact text inside a control.
    Caption,
    /// Smallest supporting text.
    Hint,
}

/// One resolved text style: metrics only. Ink is the widget's role choice, not a ramp field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyleToken {
    pub size: f32,
    pub weight: Weight,
    /// Extra leading between lines; MD3 carries it per style, not per widget.
    pub line_gap: f32,
    /// First-line indent, CSS `text-indent` in logical units.
    pub indent: f32,
}

/// The five styles for one brightness of one pack.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeTable {
    entries: [TextStyleToken; 5],
}

impl TypeTable {
    pub fn new(entries: [TextStyleToken; 5]) -> Self {
        Self { entries }
    }

    pub fn get(&self, role: TextRole) -> &TextStyleToken {
        &self.entries[role as usize]
    }
}

/// The envelope one depth level draws with: radius, corner curve, border, padding.
///
/// Grouped so a widget cannot take a radius from one tier and padding from another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetMetrics {
    pub radius: f32,
    pub corner: CornerShape,
    pub border: f32,
    pub padding: Insets,
    /// Elevation level, `0` = flat. Overlays sit above controls.
    pub elevation: u8,
}

impl WidgetMetrics {
    pub fn flat(radius: f32, corner: CornerShape) -> Self {
        Self {
            radius,
            corner,
            border: 0.0,
            padding: Insets::ZERO,
            elevation: 0,
        }
    }
}

/// Control / surface / overlay: the three depths a pack styles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricsSet {
    pub control: WidgetMetrics,
    pub surface: WidgetMetrics,
    pub overlay: WidgetMetrics,
}

impl MetricsSet {
    /// Square corners with no borders; a starting point for real packs.
    pub const NONE: Self = Self {
        control: WidgetMetrics {
            radius: 0.0,
            corner: CornerShape::ROUND,
            border: 0.0,
            padding: Insets::ZERO,
            elevation: 0,
        },
        surface: WidgetMetrics {
            radius: 0.0,
            corner: CornerShape::ROUND,
            border: 0.0,
            padding: Insets::ZERO,
            elevation: 0,
        },
        overlay: WidgetMetrics {
            radius: 0.0,
            corner: CornerShape::ROUND,
            border: 0.0,
            padding: Insets::ZERO,
            elevation: 0,
        },
    };
}

impl Default for MetricsSet {
    fn default() -> Self {
        Self::NONE
    }
}
