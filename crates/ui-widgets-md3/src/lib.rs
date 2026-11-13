//! MD3 controls, reproduced from `flutter/material/*.dart` defaults.
//!
//! One file per control; the shared sizes, curves and glyph strokes live beside them.

pub mod pack;
/// Generated token lookups from the official design-system export (`tokens/*.json`).
pub mod spec;
pub use pack::{pack, pack_of};
pub use spec::{ButtonSize, ButtonStyle, ButtonToggle};
pub use ui_design::Appearance;

/// MD3 disabled content opacity: `on-surface` at 38% (label, icon, handle, active track).
pub const DISABLED_CONTENT_OPACITY: f32 = 0.38;
/// MD3 disabled container opacity: `on-surface` at 12% (inactive track, outline).
pub const DISABLED_CONTAINER_OPACITY: f32 = 0.12;

mod badge;
mod bottom_sheet;
mod button;
mod button_group;
mod card;
mod checkbox;
mod chip;
mod date_input;
mod date_picker;
mod date_range_picker;
mod dialog;
mod divider;
mod fab;
mod fab_menu;
mod glyph;
mod icon_button;
mod layer;
mod list;
mod math;
mod menu;
mod menu_style;
mod motion;
mod nav_bar;
mod nav_drawer;
mod nav_rail;
mod paint;
mod progress;
mod radio;
mod scaffold;
mod search_bar;
mod search_view;
mod segmented_button;
mod shortcut_field;
mod side_sheet;
mod slider;
mod snackbar;
mod split_button;
mod style;
mod tab_bar;
mod text_field;
mod time_picker;
mod toggle;
mod toolbar;
mod tooltip;
pub mod typescale;

mod app_bar;
mod carousel;

pub use app_bar::*;
pub use badge::*;
pub use bottom_sheet::*;
pub use button::*;
pub use button_group::*;
pub use card::*;
pub use carousel::*;
pub use checkbox::*;
pub use chip::*;
pub use date_input::*;
pub use date_picker::*;
pub use date_range_picker::*;
pub use dialog::*;
pub use divider::*;
pub use fab::*;
pub use fab_menu::*;
pub use icon_button::*;
pub use list::*;
pub use math::*;
pub use menu::*;
pub use menu_style::*;
pub use nav_bar::*;
pub use nav_drawer::*;
pub use nav_rail::*;
pub use progress::*;
pub use radio::*;
pub use scaffold::*;
pub use search_bar::*;
pub use search_view::*;
pub use segmented_button::*;
pub use shortcut_field::*;
pub use side_sheet::*;
pub use slider::*;
pub use snackbar::*;
pub use split_button::*;
pub use style::*;
pub use tab_bar::*;
pub use text_field::*;
pub use time_picker::*;
pub use toggle::*;
pub use toolbar::*;
pub use tooltip::*;

#[cfg(test)]
mod tests;
