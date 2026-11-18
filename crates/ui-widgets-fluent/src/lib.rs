//! Fluent (WinUI 3) controls, reproduced from the reference's own tokens.
//!
//! One file per control; sizes, paint helpers and the label shape live beside them.

pub mod material;
pub mod pack;
pub use pack::{pack, pack_of};

mod button;
mod checkbox;
mod combo;
mod dialog;
pub mod fluent_codepoints;
pub mod glyphs;
mod infobar;
mod math;
mod menu_flyout;
mod menu_style;
mod navigation;
mod paint;
mod popup;
mod progress;
mod progress_ring;
mod radio;
mod scaffold;
mod slider;
pub mod spec;
mod style;
mod tab_view;
mod text;
mod textbox;
mod toggle;
mod tooltip;

pub use button::{ButtonParams, ButtonVariant, FluentButton};
pub use checkbox::{CheckBoxParams, CheckState, FluentCheckBox};
pub use combo::*;
pub use dialog::*;
pub use infobar::*;
pub use math::*;
pub use menu_flyout::*;
pub use menu_style::*;
pub use navigation::*;
pub use popup::*;
pub use progress::*;
pub use progress_ring::*;
pub use radio::{FluentRadio, RadioParams};
pub use scaffold::*;
pub use slider::*;
pub use style::*;
pub use tab_view::*;
pub use textbox::*;
pub use toggle::*;
pub use tooltip::*;

#[cfg(test)]
mod tests;
