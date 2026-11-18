//! Storage formats a backend must be able to allocate.

/// Storage precision of an internal render target.
///
/// This is a property of the *pipeline*, not of the presented buffer: an f16 frame keeps
/// gradients and blurred edges from quantising before they reach the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetFormat {
    Rgba8,
    Rgba16F,
}

impl TargetFormat {
    /// Bytes per pixel, for sizing readbacks and expectations in tests.
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            TargetFormat::Rgba8 => 4,
            TargetFormat::Rgba16F => 8,
        }
    }
}

/// Channel layout of an uploaded texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    /// Single-channel coverage, as a glyph atlas stores it.
    R8,
    /// Eight-bit colour, as a picture or a colour atlas stores it in the sRGB domain.
    Rgba8,
    /// Half-float colour: the same sRGB codes in more precision, so values above white
    /// survive. An extended surface's frame and its pictures use it.
    Rgba16F,
}

/// How a sampler addresses texels outside `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    /// Repeat the edge texel; what a sampled picture wants.
    Clamp,
    /// Tile the texture; no painter texture uses it, but a backend must be able to say so.
    Repeat,
}
