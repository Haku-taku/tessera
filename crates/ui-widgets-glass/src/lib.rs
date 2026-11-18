//! Apple Liquid Glass controls: frosted, refractive, and animated by light.
//!
//! One file per control; sizes, paint helpers and the label shape live beside them.

pub mod material;
pub mod pack;
pub mod tuning;
pub use pack::{pack, pack_of};

mod action_sheet;
mod button;
mod card;
mod chip;
mod dialog;
mod divider;
mod list_tile;
mod math;
mod menu;
mod menu_style;
mod modal_sheet;
mod morph;
mod overlay;
mod paint;
mod popover;
mod progress;
mod scaffold;
mod search_bar;
mod segmented;
mod shared;
mod slider;
mod stepper;
mod style;
mod switch;
mod tab_bar;
mod text;
mod text_field;
mod toast;

pub use action_sheet::{
    GlassActionSheet, GlassActionSheetParams, GlassSheetAction, SHEET_CARD_ALPHA, SHEET_GAP,
    SHEET_HIGHLIGHT, SHEET_ICON, SHEET_ICON_GAP, SHEET_INSET, SHEET_LABEL, SHEET_MESSAGE,
    SHEET_RADIUS, SHEET_ROW_PAD, SHEET_SCRIM, SHEET_TITLE,
};
pub use button::{
    AMBIENT_DARK, AMBIENT_FOCUS, AMBIENT_HOVER, AMBIENT_LIGHT, BUTTON_DAMPING, BUTTON_H,
    BUTTON_ICON, BUTTON_INTENSITY, BUTTON_LABEL_GAP, BUTTON_PAD, BUTTON_PRESS_GROWTH,
    BUTTON_RESISTANCE, BUTTON_SIZE, BUTTON_SQUASH, BUTTON_STRETCH, ButtonFace, GlassButton,
    GlassButtonParams, GlassButtonStyle, button_face, resisted,
};
pub use card::{
    CARD_PAD, CARD_RADIUS, CONTAINER_RADIUS, GlassCard, GlassCardParams, GlassSurface,
    SOLID_CARD_RADIUS,
};
pub use chip::{
    CHIP_DELETE, CHIP_FONT, CHIP_GAP, CHIP_ICON, CHIP_PAD_X, CHIP_PAD_Y, CHIP_PRESS_SCALE,
    CHIP_TINT_DARK, CHIP_TINT_LIGHT, GlassChip, GlassChipParams,
};
pub use dialog::{
    DIALOG_ACTION_FONT, DIALOG_ACTION_GAP, DIALOG_ACTION_H, DIALOG_ACTION_RADIUS,
    DIALOG_ACTIONS_GAP, DIALOG_CONTENT_GAP, DIALOG_GAP, DIALOG_MESSAGE_ALPHA,
    DIALOG_MESSAGE_LEADING, DIALOG_MESSAGE_SIZE, DIALOG_PAD, DIALOG_RADIUS, DIALOG_SCRIM,
    DIALOG_TITLE_SIZE, DIALOG_WIDTH, GlassActionKind, GlassDialog, GlassDialogAction,
    GlassDialogParams,
};
pub use divider::{
    DIVIDER_ALPHA_DARK, DIVIDER_ALPHA_LIGHT, DIVIDER_BAND, DIVIDER_THICKNESS, GlassDivider,
};
pub use list_tile::{
    GlassListTile, GlassListTileParams, LIST_GAP, LIST_HIGHLIGHT, LIST_LEADING, LIST_LEADING_MIN,
    LIST_PAD_X, LIST_PAD_Y, LIST_RADIUS, LIST_SUBTITLE, LIST_SUBTITLE_ALPHA, LIST_TEXT_GAP,
    LIST_TITLE, LIST_TRAILING, LIST_TRAILING_ALPHA, LIST_TRAILING_GAP,
};
pub use math::*;
pub use menu::{
    GlassMenu, GlassMenuEntry, GlassMenuParams, MENU_DISMISS_SHORTCUT, MENU_DIVIDER_H, MENU_FONT,
    MENU_ICON, MENU_ICON_GAP, MENU_ITEM_RADIUS, MENU_PAD, MENU_RADIUS, MENU_ROW_GAP, MENU_ROW_H,
    MENU_ROW_PAD_X, MENU_ROW_PAD_Y, MENU_W, menu_row_fill,
};
pub use menu_style::*;
pub use popover::{
    GlassPopover, GlassPopoverParams, POPOVER_DISMISS_SHORTCUT, POPOVER_PAD, POPOVER_RADIUS,
    POPOVER_W,
};
pub use progress::{
    GlassProgress, GlassProgressParams, PROGRESS_ARC, PROGRESS_BAR_H, PROGRESS_CYCLE,
    PROGRESS_GLOW_ALPHA, PROGRESS_GLOW_GROWTH, PROGRESS_MIN_W, PROGRESS_SIZE, PROGRESS_STROKE,
    PROGRESS_SWEEP, PROGRESS_TRACK_ALPHA, ProgressKind,
};
pub use scaffold::*;
pub use search_bar::{
    CANCEL_GAP, CANCEL_GROW, CANCEL_ICON, CLEAR_ICON, GlassSearchBar, GlassSearchBarParams,
};
pub use segmented::{
    GlassSegment, GlassSegmentedControl, SEGMENT_DISABLED, SEGMENT_EXPAND_H, SEGMENT_EXPAND_V,
    SEGMENT_FONT, SEGMENT_H, SEGMENT_ICON, SEGMENT_ICON_GAP, SEGMENT_ICON_WITH_LABEL,
    SEGMENT_INSET,
};
pub use shared::*;
// Explicit lists: switch and slider share geometry names (`WIDTH`, `THUMB_W`, …) that a
pub use modal_sheet::{
    GlassModalSheet, GlassModalSheetParams, GlassSheetDetent, SHEET_BOTTOM, SHEET_DIM,
    SHEET_DISMISS_SHORTCUT, SHEET_FULL_TOP, SHEET_HALF, SHEET_HANDLE_DARK, SHEET_HANDLE_GAP,
    SHEET_HANDLE_H, SHEET_HANDLE_LIGHT, SHEET_HANDLE_TOP, SHEET_HANDLE_W, SHEET_MARGIN,
    SHEET_RESISTANCE, SHEET_SNAP, SHEET_TOP_RADIUS, SHEET_TOP_RADIUS_FULL, detent_id_of,
};
pub use morph::{GlassMorph, MorphState, back_out};
pub use slider::{
    GlassSlider, SLIDER_BOX_H, SLIDER_EXPANDED_H, SLIDER_EXPANDED_W, SLIDER_THUMB_H,
    SLIDER_THUMB_W, SLIDER_TRACK_H, SLIDER_WIDTH, SliderThumb, fill_rect, raw_value_at,
    rubber_band, rubber_band_offset, slider_thumb, slider_thumb_x, slider_travel, slider_value_at,
    thumb_centre, track_rect,
};
pub use stepper::{
    GlassStepper, GlassStepperParams, STEPPER_DISABLED_ALPHA, STEPPER_DIVIDER,
    STEPPER_DIVIDER_ALPHA, STEPPER_FADE_TIME, STEPPER_H, STEPPER_ICON, STEPPER_PRESS_SCALE,
    STEPPER_PRESS_TIME, STEPPER_REPEAT_DELAY, STEPPER_REPEAT_INTERVAL, STEPPER_W, stepped,
};
pub use style::*;
pub use switch::{
    EDGE_TOGGLE, EXPANDED_H, EXPANDED_W, GlassSwitch, HEIGHT, INSET, SwitchThumb, TAP_HOLD,
    THUMB_H, THUMB_W, WIDTH, switch_thumb, travel,
};
pub use tab_bar::{BAR_H, EXPAND_H, EXPAND_V, GlassTab, GlassTabBar, ICON_LABEL_GAP, ICON_SIZE};
pub use text_field::{
    FIELD_CARET_W, FIELD_DISABLED_ALPHA, FIELD_FONT, FIELD_ICON, FIELD_ICON_GAP, FIELD_INK_ALPHA,
    FIELD_PAD_X, FIELD_PAD_Y, FIELD_PLACEHOLDER_ALPHA, FIELD_RADIUS, FIELD_SCROLL_SLACK,
    FIELD_SEARCH_H, FIELD_SEARCH_ICON_GAP, FIELD_SEARCH_PAD_X, FIELD_SEARCH_PAD_Y,
    FIELD_SEARCH_RADIUS, GlassTextField, GlassTextFieldParams,
};
pub use toast::{
    GlassToast, GlassToastAction, GlassToastKind, GlassToastParams, GlassToastPosition,
    TOAST_ACTION_LABEL, TOAST_ACTION_MIN, TOAST_ACTION_PAD, TOAST_DISMISS_TRAVEL, TOAST_FACE_ALPHA,
    TOAST_GAP, TOAST_GLOW_ALPHA, TOAST_GLOW_BLUR, TOAST_GLOW_SPREAD, TOAST_ICON, TOAST_INSET,
    TOAST_LABEL, TOAST_MAX_WIDTH, TOAST_MIN_HEIGHT, TOAST_PAD_H, TOAST_PAD_V, TOAST_PAD_V_ACTION,
    TOAST_RADIUS, TOAST_RIM, TOAST_RIM_ALPHA,
};

#[cfg(test)]
mod tests;
