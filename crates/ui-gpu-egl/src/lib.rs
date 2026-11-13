//! EGL + gbm + dma-buf: the device and presentation layer.
//!
//! Nothing here knows about layout, text or draw lists; the renderer depends on this,
//! never the other way round.

#![cfg(unix)]

pub mod context;
pub mod ffi;
pub mod node;
pub mod syncobj;
pub mod target;

pub use context::{DmaBufDesc, EglContext, ExportedFence};
pub use node::{candidate_nodes, output_render_node};
pub use syncobj::SyncTimeline;
pub use target::DmaBufTarget;

/// Why a GPU operation failed.
#[derive(Debug)]
pub enum GpuError {
    /// The DRM render node could not be opened.
    OpenNode {
        path: String,
        errno: i32,
    },
    /// `libgbm` or one of its symbols was missing.
    GbmUnavailable(&'static str),
    /// `gbm_create_device` or `gbm_bo_create` returned null.
    Allocation(&'static str),
    Egl(String),
    Gl(String),
    /// A required EGL extension is absent on this display.
    MissingExtension(&'static str),
    /// A DRM syncobj operation failed.
    SyncObj(&'static str),
}

impl core::fmt::Display for GpuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OpenNode { path, errno } => write!(f, "cannot open {path}: errno {errno}"),
            Self::GbmUnavailable(s) => write!(f, "libgbm unavailable: {s}"),
            Self::Allocation(s) => write!(f, "allocation failed: {s}"),
            Self::Egl(s) => write!(f, "EGL: {s}"),
            Self::Gl(s) => write!(f, "GL: {s}"),
            Self::MissingExtension(s) => write!(f, "missing EGL extension {s}"),
            Self::SyncObj(s) => write!(f, "syncobj: {s}"),
        }
    }
}

impl From<String> for GpuError {
    fn from(e: String) -> Self {
        Self::Gl(e)
    }
}

impl std::error::Error for GpuError {}
