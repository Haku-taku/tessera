//! Backend-neutral GPU paint scene: what to draw, not how.
//!
//! A backend supplies [`Device`] and its targets; this crate owns batch walking, the layer
//! stack, damage bands, backdrop ordering and the pass sequence. See `docs/src/engine/`.

mod device;
mod effect;
mod error;
mod format;
mod layout;
mod program;
mod renderer;
mod target;

pub use device::{Device, ProgramId, TargetId, TextureId, UniformSlot};
pub use error::GpuError;
pub use format::{TargetFormat, TextureFormat, Wrap};
pub use layout::{MAX_TAPS, UNIT_QUAD};
pub use program::{ProgramKey, ProgramSpec};
pub use renderer::{AtlasUpload, CanvasUpload, GpuRenderer, RenderStats, as_bytes, partial_frames};
pub use target::RenderTarget;
pub use ui_core::draw::Sampling;
