//! Font stack: platform-native enumeration with an explicit fallback chain.
//!
//! The engine never names a concrete font. It asks for a `FontRequest` and
//! receives a resolved face, so swapping the provider (native, bundled, test)
//! changes nothing above this module.

pub mod bidi;
pub mod emoji;
pub mod fallback;
pub mod linebreak;
pub mod provider;
pub mod subpixel;

pub use bidi::{Direction, LevelRun, itemize, paragraph_direction, visual_order, visual_runs};
pub use fallback::{FontStack, ResolvedFont};
pub use linebreak::{
    ELLIPSIS, Line, break_lines, justify_gaps, min_content_width, wrap, wrap_indented,
};
pub use provider::{
    FaceId, FaceMetrics, Features, FontBlob, FontProvider, FontRequest, GenericFamily, Script,
    StaticProvider, Variations, Weight,
};
pub use subpixel::{Phases, SUBPIXEL_PHASES, SubpixelBin, VERTICAL_PHASES, VerticalBin};
