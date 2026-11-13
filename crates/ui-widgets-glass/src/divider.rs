//! The glass divider: an iOS hairline inside a one-dp band.

use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind};
use ui_style::Style;
use ui_widgets::{Cx, Widget};

/// A separator; the band gives it a touch of padding, the line itself is 0.5 dp.
pub struct GlassDivider {
    id: Id,
    vertical: bool,
    thickness: f32,
    indent: f32,
    end_indent: f32,
    band: f32,
    ink: Color,
}

impl GlassDivider {
    /// A full-width horizontal hairline (`GlassDivider`).
    pub fn horizontal(id: Id, pack: &Style) -> Self {
        Self::new(id, false, pack)
    }

    /// A full-height vertical hairline (`GlassDivider.vertical`).
    pub fn vertical(id: Id, pack: &Style) -> Self {
        Self::new(id, true, pack)
    }

    fn new(id: Id, vertical: bool, pack: &Style) -> Self {
        let dark = pack.appearance() == Some(ui_design::Appearance::Dark);
        Self {
            id,
            vertical,
            thickness: DIVIDER_THICKNESS,
            indent: 0.0,
            end_indent: 0.0,
            band: DIVIDER_BAND,
            ink: Color {
                a: if dark {
                    DIVIDER_ALPHA_DARK
                } else {
                    DIVIDER_ALPHA_LIGHT
                },
                ..if dark { Color::WHITE } else { Color::BLACK }
            },
        }
    }

    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    pub fn indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }

    pub fn end_indent(mut self, end_indent: f32) -> Self {
        self.end_indent = end_indent;
        self
    }

    pub fn band(mut self, band: f32) -> Self {
        self.band = band;
        self
    }

    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = ink;
        self
    }
}

impl Widget for GlassDivider {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        // The band is a line container: the hairline grows along its main axis and is
        // centred across it by the cross-axis rule, so no offset is hardcoded.
        let (kind, size, padding, line_size) = if self.vertical {
            (
                NodeKind::Column,
                SizeClass {
                    width: AxisSize::fixed(self.band),
                    height: AxisSize::fill(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                },
                Insets {
                    top: self.indent,
                    bottom: self.end_indent,
                    ..Insets::ZERO
                },
                SizeClass {
                    width: AxisSize::fixed(self.thickness),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::Yes,
                },
            )
        } else {
            (
                NodeKind::Row,
                SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fixed(self.band),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                },
                Insets {
                    left: self.indent,
                    right: self.end_indent,
                    ..Insets::ZERO
                },
                SizeClass {
                    width: AxisSize::hug(0.0, 0.0),
                    height: AxisSize::fixed(self.thickness),
                    grow_x: Grow::Yes,
                    grow_y: Grow::No,
                },
            )
        };
        Decl::container(self.id, kind)
            .size(size)
            .padding(padding)
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(
                Decl::container(self.id.with("line"), NodeKind::Box)
                    .size(line_size)
                    .background(self.ink, Corners::ZERO),
            )
    }
}

/// iOS 26's hairline separator (`glass_divider.dart:38-40`).
pub const DIVIDER_THICKNESS: f32 = 0.5;
/// The band the line sits in (`height: 1.0`).
pub const DIVIDER_BAND: f32 = 1.0;
/// Adaptive ink: 10 % black in light, 20 % white in dark (`glass_divider.dart:107-110`).
pub const DIVIDER_ALPHA_LIGHT: f32 = 0.10;
pub const DIVIDER_ALPHA_DARK: f32 = 0.20;
