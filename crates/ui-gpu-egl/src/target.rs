//! A dma-buf backed target: allocated here, handed to the compositor here.

use std::ffi::c_void;

use glow::HasContext;

use crate::GpuError;
use crate::context::EglContext;
use crate::ffi::{GBM_BO_USE_RENDERING, GBM_FORMAT_ARGB8888, GbmBuffer};

/// A buffer the compositor can import, plus the GL objects that render into it.
pub struct DmaBufTarget {
    width: u32,
    height: u32,
    fourcc: u32,
    buffer: GbmBuffer,
    image: *mut c_void,
    texture: glow::Texture,
    fbo: glow::Framebuffer,
}

impl DmaBufTarget {
    /// Allocate a buffer of `width` x `height` and make it renderable.
    pub fn new(ctx: &EglContext, width: u32, height: u32) -> Result<Self, GpuError> {
        Self::new_with_fourcc(ctx, width, height, GBM_FORMAT_ARGB8888)
    }

    /// Allocate with an explicit DRM fourcc; the caller decides the buffer format.
    pub fn new_with_fourcc(
        ctx: &EglContext,
        width: u32,
        height: u32,
        fourcc: u32,
    ) -> Result<Self, GpuError> {
        let width = width.max(1);
        let height = height.max(1);
        let buffer = ctx
            .gbm()
            .create_buffer(width, height, fourcc, GBM_BO_USE_RENDERING)?;
        let desc = crate::DmaBufDesc {
            fd: buffer.fd,
            width: buffer.width,
            height: buffer.height,
            stride: buffer.stride,
            offset: buffer.offset,
            format: fourcc,
            modifier: buffer.modifier,
        };
        let (texture, fbo, image) = ctx.create_dmabuf_texture(&desc)?;
        Ok(Self {
            width,
            height,
            fourcc,
            buffer,
            image,
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

    pub fn desc(&self) -> crate::DmaBufDesc {
        crate::DmaBufDesc {
            fd: self.buffer.fd,
            width: self.width,
            height: self.height,
            stride: self.buffer.stride,
            offset: self.buffer.offset,
            format: self.fourcc,
            modifier: self.buffer.modifier,
        }
    }

    /// Duplicate the dma-buf fd, because the compositor closes what it receives.
    pub fn dup_fd(&self) -> Result<std::os::fd::OwnedFd, GpuError> {
        use std::os::fd::FromRawFd;
        let fd = unsafe { libc::fcntl(self.buffer.fd, libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            return Err(GpuError::Allocation("dup dma-buf fd"));
        }
        Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
    }

    /// Free the GL objects. Must be called while `ctx` is current.
    pub fn release(&self, ctx: &EglContext) {
        let gl = ctx.gl();
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.texture);
        }
        ctx.destroy_image(self.image);
    }
}

impl ui_gpu_scene::RenderTarget for DmaBufTarget {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn target_id(&self) -> ui_gpu_scene::TargetId {
        ui_gpu_scene::TargetId(u64::from(self.fbo.0.get()))
    }
}

impl core::fmt::Debug for DmaBufTarget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DmaBufTarget")
            .field("size", &(self.width, self.height))
            .field("stride", &self.buffer.stride)
            .field("modifier", &format_args!("{:#x}", self.buffer.modifier))
            .finish()
    }
}
