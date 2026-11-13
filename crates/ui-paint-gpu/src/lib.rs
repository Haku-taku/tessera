//! GPU paint backend: the OpenGL ES device the scene painter draws through.
//!
//! The scene itself lives in `ui-gpu-scene` and knows nothing about GL; this crate supplies
//! its `Device`. Why not wgpu: it exposes no dma-buf export, so zero-copy presentation is
//! impossible with it. See `docs/gpu-backend.md` §1.

mod gl_device;
pub mod material;
pub mod offscreen;
pub mod shader;

pub use gl_device::GlDevice;
pub use offscreen::OffscreenFbo;
pub use ui_gpu_scene::{
    AtlasUpload, CanvasUpload, Device, GpuError, GpuRenderer, ProgramId, ProgramKey, RenderStats,
    RenderTarget, TargetFormat, TargetId, TextureFormat, TextureId, UniformSlot, Wrap, as_bytes,
    partial_frames,
};
