//! An offscreen render target that never leaves the GPU.
//!
//! Readback tests need a framebuffer, not a shareable buffer; keeping the two apart is
//! what lets this crate stop depending on gbm and dma-buf.

use glow::HasContext;

use crate::GpuError;
use ui_gpu_egl::GpuSurface;

/// A plain texture + framebuffer pair.
pub struct OffscreenFbo {
    width: u32,
    height: u32,
    texture: glow::Texture,
    fbo: glow::Framebuffer,
}

impl OffscreenFbo {
    /// Allocate a `width` x `height` RGBA8 target.
    pub fn new(gl: &glow::Context, width: u32, height: u32) -> Result<Self, GpuError> {
        let width = width.max(1);
        let height = height.max(1);
        let texture = unsafe {
            let tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            // Same filtering as the dma-buf target, so both paths sample identically.
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            tex
        };

        let fbo = unsafe {
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(GpuError::Gl(format!("FBO incomplete: {status:#x}")));
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            fbo
        };

        Ok(Self {
            width,
            height,
            texture,
            fbo,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn release(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.texture);
        }
    }
}

impl GpuSurface for OffscreenFbo {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.viewport(0, 0, self.width as i32, self.height as i32);
        }
    }
}
