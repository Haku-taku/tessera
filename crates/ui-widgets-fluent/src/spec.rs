//! WinUI 3 control geometry, transcribed from the theme resources.
//!
//! Authority: `microsoft-ui-xaml` @ `868fef9a`, `*_themeresources.xaml`; the resource key
//! each number came from is named in the row's comment.

use ui_core::geom::Insets;
use ui_style::{ControlKind, ControlMetrics, ControlMetricsTable, TextRole};

/// The `CommonStates` column order every WinUI template's brush array follows:
/// `Rest`, `PointerOver`, `Pressed`, `Disabled`.
pub(crate) const REST: usize = 0;
pub(crate) const POINTER_OVER: usize = 1;
pub(crate) const PRESSED: usize = 2;
pub(crate) const DISABLED: usize = 3;

const fn m(
    min_width: f32,
    min_height: f32,
    padding: Insets,
    border: f32,
    radius: f32,
    focus: Insets,
    icon: f32,
    font: TextRole,
) -> ControlMetrics {
    ControlMetrics {
        min_width,
        min_height,
        padding,
        border,
        radius,
        focus_margin: focus,
        icon_size: icon,
        font,
    }
}

/// The WinUI controls sized so far; every other row stays `EMPTY` until its control lands.
pub fn winui_controls() -> ControlMetricsTable {
    let mut rows = [ControlMetrics::EMPTY; ControlKind::COUNT];
    // ButtonPadding / ControlCornerRadius / ButtonBorderThemeThickness / FocusVisualMargin -3.
    rows[ControlKind::Button as usize] = m(
        0.0,
        32.0,
        Insets::new(11.0, 5.0, 11.0, 6.0),
        1.0,
        4.0,
        Insets::all(-3.0),
        0.0,
        TextRole::Body,
    );
    // CheckBoxSize 20 / CheckBoxGlyphSize 12 / CheckBoxHeight 32 / CheckBoxMinWidth 120.
    rows[ControlKind::CheckBox as usize] = m(
        120.0,
        32.0,
        Insets::new(8.0, 5.0, 0.0, 0.0),
        1.0,
        4.0,
        Insets::new(-7.0, -3.0, -7.0, -3.0),
        12.0,
        TextRole::Body,
    );
    // RadioButton MinWidth 120 / glyph 12 / 20 pt ellipses / padding 8,6,0,0.
    rows[ControlKind::RadioButton as usize] = m(
        120.0,
        32.0,
        Insets::new(8.0, 6.0, 0.0, 0.0),
        1.0,
        4.0,
        Insets::new(-7.0, -3.0, -7.0, -3.0),
        12.0,
        TextRole::Body,
    );
    // ToggleSwitchThemeMinWidth 154; the switch column is 10 + 20 + 10 tall.
    rows[ControlKind::ToggleSwitch as usize] = m(
        154.0,
        40.0,
        Insets::ZERO,
        0.0,
        4.0,
        Insets::new(-7.0, -3.0, -7.0, -3.0),
        0.0,
        TextRole::Body,
    );
    // SliderHorizontalHeight 32 / SliderTrackCornerRadius 2 / FocusVisualMargin -7,0,-7,0.
    rows[ControlKind::Slider as usize] = m(
        0.0,
        32.0,
        Insets::ZERO,
        0.0,
        2.0,
        Insets::new(-7.0, 0.0, -7.0, 0.0),
        0.0,
        TextRole::Body,
    );
    // ComboBoxThemeMinWidth 64 / ComboBoxMinHeight 32 / ComboBoxPadding 12,5,0,7 /
    // ComboBoxBorderThemeThickness 1 / ControlCornerRadius 4 / glyph 12 / FocusVisualMargin -4.
    rows[ControlKind::ComboBox as usize] = m(
        64.0,
        32.0,
        Insets::new(12.0, 5.0, 0.0, 7.0),
        1.0,
        4.0,
        Insets::all(-4.0),
        12.0,
        TextRole::Body,
    );
    // NavigationViewItemOnLeftMinHeight 36 / OverlayCornerRadius 8 / NavigationViewItemBorderThickness 1.
    rows[ControlKind::NavigationViewItem as usize] = m(
        0.0,
        36.0,
        Insets::ZERO,
        1.0,
        8.0,
        Insets::ZERO,
        16.0,
        TextRole::Body,
    );
    // MenuFlyoutItemThemePadding 11,8,11,9 / ControlCornerRadius 4 / IconRoot 16.
    rows[ControlKind::MenuFlyoutItem as usize] = m(
        0.0,
        0.0,
        Insets::new(11.0, 8.0, 11.0, 9.0),
        0.0,
        4.0,
        Insets::ZERO,
        16.0,
        TextRole::Body,
    );
    ControlMetricsTable::from_standard(rows)
}
