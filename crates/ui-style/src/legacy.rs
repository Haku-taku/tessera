//! The look the examples drew with literal colours before any pack existed. Data here,
//! not in the widgets, is what makes a style switch possible.

use ui_core::Weight;
use ui_core::geom::{Color, CornerShape, Insets};

use crate::Style;
use crate::capability::Capability;
use crate::motion::MotionTable;
use crate::roles::{RoleDict, RoleName};
use crate::types::{MetricsSet, TextStyleToken, TypeTable, WidgetMetrics};

fn c(v: [u8; 4]) -> Color {
    Color::from_rgba8(v[0], v[1], v[2], v[3])
}

/// The colours and metrics lifted from `window_gpu`'s inline literals.
pub fn legacy_pack() -> Style {
    // Values are the example's own literals: the pill's three states, the card's blue
    // with its lighter hover lift, the scroller's fill, the panel's tint and its dark
    // stand-in. Nothing here invents a shade the example never drew.
    let mut roles = RoleDict::new();
    roles.set(RoleName("buttonFill"), c([255, 255, 255, 255]));
    roles.set(RoleName("buttonFillHover"), c([255, 240, 170, 255]));
    roles.set(RoleName("buttonFillPressed"), c([255, 170, 90, 255]));
    roles.set(RoleName("buttonText"), c([20, 30, 50, 255]));
    roles.set(RoleName("buttonBorder"), Color::TRANSPARENT);
    roles.set(RoleName("focusStroke"), c([20, 30, 50, 255]));
    roles.set(RoleName("cardFill"), c([60, 130, 220, 255]));
    roles.set(RoleName("cardBorder"), c([210, 225, 245, 255]));
    roles.set(RoleName("scrollerFill"), c([30, 60, 110, 255]));
    roles.set(RoleName("rowText"), c([210, 225, 250, 255]));
    roles.set(RoleName("glassFallback"), c([18, 20, 26, 255]));
    roles.set(RoleName("glassBorder"), c([255, 255, 255, 90]));

    let style = |size: f32, weight: Weight, _color: Color| TextStyleToken {
        size,
        weight,
        line_gap: 0.0,
        indent: 0.0,
    };
    // The five roles the widgets actually request. The example's other texts (a CJK
    // probe, an RTL probe, a blurred group) are diagnostics, not widgets, and stay inline.
    let type_table = TypeTable::new([
        style(18.0, Weight::BOLD, Color::WHITE),
        style(16.0, Weight::BOLD, c([255, 255, 255, 240])),
        style(12.0, Weight::REGULAR, c([225, 235, 250, 255])),
        style(13.0, Weight::REGULAR, c([210, 225, 250, 255])),
        style(12.0, Weight::REGULAR, c([235, 240, 255, 210])),
    ]);

    let surface = WidgetMetrics {
        radius: 10.0,
        corner: CornerShape::SQUIRCLE,
        border: 2.0,
        padding: Insets {
            left: 12.0,
            right: 12.0,
            top: 10.0,
            bottom: 10.0,
        },
        elevation: 1,
    };
    let overlay = WidgetMetrics {
        radius: 14.0,
        corner: CornerShape::SQUIRCLE,
        border: 1.0,
        padding: Insets {
            left: 14.0,
            right: 14.0,
            top: 10.0,
            bottom: 10.0,
        },
        // A floating panel sits above the surface it covers.
        elevation: 3,
    };
    let control = WidgetMetrics {
        radius: 6.0,
        corner: CornerShape::ROUND,
        border: 2.0,
        padding: Insets {
            left: 8.0,
            right: 8.0,
            top: 4.0,
            bottom: 4.0,
        },
        elevation: 0,
    };

    let tint = c([255, 255, 255, 90]);
    Style {
        common: None,
        roles,
        motion: MotionTable::legacy(),
        metrics: MetricsSet {
            control,
            surface,
            overlay,
        },
        controls: crate::ControlMetricsTable::EMPTY,
        type_table,
        material: Some(crate::material::Material::Frost {
            sigma: 14.0,
            tint,
            fallback: c([18, 20, 26, 255]),
        }),
        device: Capability::GPU_BASELINE,
        palette: None,
        selection: None,
    }
}
