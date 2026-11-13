//! Materials that shade the pixels behind a surface: Fluent acrylic and glass.
//!
//! The blur amount stays on the node (`BackdropEntry`); a material only says how to
//! colour the blurred result, so changing style never changes geometry or cost shape.

pub use ui_core::draw::{Acrylic, BackdropMaterial, LiquidGlass};
