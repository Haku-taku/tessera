//! Built-in widgets: state machines and semantics, resolved into `Decl` through a
//! style pack. No colour, duration or radius literal may appear in this crate.

pub mod build;
pub mod button;
pub mod cx;
pub mod fingerprint;
pub mod group;
pub mod icon;
mod icon_codepoints;
pub mod list;
pub mod markdown;
pub mod math;
pub mod scaffold;
pub mod scroll;
mod sf_symbol_codepoints;
pub mod shortcut;
pub mod state;
pub mod style;
pub mod styled;
pub mod surface;
pub mod text;
pub mod widget;

pub use button::Button;
pub use cx::{Cx, NodeCx};
pub use group::GroupPosition;
pub use icon::{Icon, IconSet, IconSource, SF_SYMBOLS_FAMILY};
pub use list::List;
pub use markdown::{
    CodeHeight, CodeOptions, CodeStyle, ImageSource, ImageTexture, Markdown, MathEditing,
};
pub use math::{Math, MathErrorStyle};
pub use scaffold::ScaffoldSpec;
pub use scroll::{RowHeights, RowWindow, Scroll};
pub use shortcut::{modifiers_label, shortcut_label, shortcut_label_in};
pub use state::WidgetState;
pub use style::StyleKit;
pub use styled::Styled;
pub use surface::{Card, Panel};
pub use text::{LINE_HINT, Text, icon_extent, leaf};
pub use ui_core::change::Change;
pub use ui_platform::menu::{MenuRowText, MenuStyle, TextMenuLabels};
pub use ui_platform::{Container, Element, from_decl};
pub use widget::{Widget, decl_only};
