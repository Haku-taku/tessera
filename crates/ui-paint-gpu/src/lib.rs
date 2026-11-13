//! GPU paint backend: EGL + OpenGL ES rendering into dma-buf backed targets.
//!
//! Why not wgpu: it exposes no dma-buf export, so zero-copy presentation is
//! impossible with it. See `docs/gpu-backend.md` §1.

mod effect_pass;
pub mod material;
pub mod node;
pub mod offscreen;
pub mod renderer;
pub mod shader;
pub use ui_gpu_egl::{DmaBufTarget as GpuTarget, GpuSurface, SyncTimeline};

pub use ui_gpu_egl::EglContext;
/// Compatibility path: existing `crate::context::…` references keep working.
pub mod context {
    pub use ui_gpu_egl::context::*;
}
/// Same for the gbm bindings.
pub mod ffi {
    pub use ui_gpu_egl::ffi::*;
}
/// Storage precision of internal render targets.
pub use effect_pass::TargetFormat;
pub use material::{Acrylic, BackdropMaterial, LiquidGlass};
pub use node::{candidate_nodes, output_render_node};
pub use offscreen::OffscreenFbo;
pub use renderer::partial_frames;
pub use renderer::{AtlasUpload, CanvasUpload, GpuRenderer, RenderStats};
pub use ui_gpu_egl::GpuError;
