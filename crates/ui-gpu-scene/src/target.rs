//! Render targets the painter allocates, and the target a caller presents into.

use crate::device::{Device, TargetId, TextureId};
use crate::error::GpuError;
use crate::format::TargetFormat;

/// A surface the painter presents into.
///
/// The painter never learns how the target was allocated; the backend that made it owns
/// the meaning of [`TargetId`].
pub trait RenderTarget {
    fn size(&self) -> (u32, u32);
    fn target_id(&self) -> TargetId;
}

/// A backend-owned colour target plus the texture it renders into.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target {
    pub(crate) fbo: TargetId,
    pub(crate) tex: TextureId,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TargetFormat,
}

impl Target {
    /// A slot that has not been allocated yet.
    pub(crate) const EMPTY: Self = Self {
        fbo: TargetId(0),
        tex: TextureId(0),
        width: 0,
        height: 0,
        format: TargetFormat::Rgba8,
    };

    /// Allocate on first use, reallocate when the size or format changes.
    pub(crate) fn ensure(
        &mut self,
        device: &mut dyn Device,
        width: u32,
        height: u32,
        format: TargetFormat,
    ) -> Result<Self, GpuError> {
        let width = width.max(1);
        let height = height.max(1);
        if self.fbo.0 == 0 {
            let (fbo, tex) = device.create_target(width, height, format)?;
            *self = Self {
                fbo,
                tex,
                width,
                height,
                format,
            };
        } else if self.width != width || self.height != height || self.format != format {
            device.resize_target(self.fbo, width, height, format);
            self.width = width;
            self.height = height;
            self.format = format;
        }
        Ok(*self)
    }
}

/// Give `slot` a target of the requested size, allocating or resizing as needed.
///
/// A freshly pushed [`Target::EMPTY`] carries no storage, so the first call allocates it.
pub(crate) fn resize_target(
    slot: &mut Target,
    device: &mut dyn Device,
    width: u32,
    height: u32,
    format: TargetFormat,
) -> Result<(), GpuError> {
    slot.ensure(device, width, height, format).map(|_| ())
}

/// Give `slot` a target of the requested size, allocating or resizing as needed.
pub(crate) fn ensure_target(
    slot: &mut Option<Target>,
    device: &mut dyn Device,
    width: u32,
    height: u32,
    format: TargetFormat,
) -> Result<Target, GpuError> {
    slot.get_or_insert(Target::EMPTY)
        .ensure(device, width, height, format)
}
