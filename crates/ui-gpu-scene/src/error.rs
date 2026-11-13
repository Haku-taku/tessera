//! Why a GPU operation failed.

/// A failure in the painter or in the backend behind it.
#[derive(Debug)]
pub enum GpuError {
    /// A buffer, texture or target could not be created.
    Allocation(&'static str),
    /// The backend refused a call; the string says which and why.
    Device(String),
    /// A shader failed to build or lacks a required uniform.
    Shader(String),
    /// The backend does not implement this operation.
    Unsupported(&'static str),
}

impl core::fmt::Display for GpuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Allocation(s) => write!(f, "allocation failed: {s}"),
            Self::Device(s) => write!(f, "{s}"),
            Self::Shader(s) => write!(f, "shader: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
        }
    }
}

impl From<String> for GpuError {
    fn from(message: String) -> Self {
        Self::Device(message)
    }
}

impl From<&str> for GpuError {
    fn from(message: &str) -> Self {
        Self::Device(message.to_string())
    }
}

impl std::error::Error for GpuError {}
