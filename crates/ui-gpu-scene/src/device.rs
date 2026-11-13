//! Everything the painter asks of a graphics backend.

use ui_core::draw::Sampling;

use crate::{GpuError, ProgramKey, TargetFormat, TextureFormat, Wrap};

/// A shader program pair the backend owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramId(pub u32);

/// A texture the backend owns.
///
/// The encoding is the backend's: GL uses the texture name, D3D an index into its tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u64);

/// A render target the backend owns, or one presented by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TargetId(pub u64);

/// Where a uniform lives in one program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UniformSlot(pub u32);

/// The operations a painter needs; the backend maps each to its own API.
///
/// Every method is object-safe: callers hold `&mut dyn Device` and the painter is compiled
/// once per backend rather than once per concrete device.
pub trait Device {
    /// Build the program pair named by `key`, with its input layout and required uniforms.
    fn create_program(&mut self, key: ProgramKey) -> Result<ProgramId, GpuError>;

    /// The slot `name` occupies in `program`, or `None` when the shader has no such uniform.
    fn uniform(&self, program: ProgramId, name: &str) -> Option<UniformSlot>;

    /// Allocate a colour target and the texture it renders into.
    fn create_target(
        &mut self,
        width: u32,
        height: u32,
        format: TargetFormat,
    ) -> Result<(TargetId, TextureId), GpuError>;

    /// Reallocate a target's storage, keeping its identity and sampling state.
    fn resize_target(&mut self, target: TargetId, width: u32, height: u32, format: TargetFormat);

    /// Allocate an empty texture; `upload_texture` fills it.
    fn create_texture(
        &mut self,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Result<TextureId, GpuError>;

    /// Replace the texture's whole contents with `data`, reallocating if size or format differ.
    ///
    /// The handle survives the reallocation, so a texture whose contents change wholesale —
    /// an atlas that grew, a picture re-uploaded for a new domain — keeps its identity.
    fn upload_texture(
        &mut self,
        texture: TextureId,
        width: u32,
        height: u32,
        format: TextureFormat,
        data: &[u8],
    );

    /// The texture's address mode on each axis.
    fn texture_wrap(&self, texture: TextureId) -> (Wrap, Wrap);

    /// Set magnification and minification, and whether a mip chain is wanted.
    fn set_sampling(&mut self, texture: TextureId, sampling: Sampling, mips: bool);

    /// Free a texture; a target keeps its own.
    fn destroy_texture(&mut self, texture: TextureId);

    /// Bind the target every subsequent draw and clear writes into, or none.
    fn bind_target(&mut self, target: Option<TargetId>);

    fn set_viewport(&mut self, width: u32, height: u32);

    /// `None` disables the test; `Some` enables it and bounds all writes to `rect`.
    fn set_scissor(&mut self, rect: Option<[i32; 4]>);

    /// Enable or disable premultiplied source-over blending.
    fn set_blend(&mut self, enabled: bool);

    /// The colour a later `clear` writes.
    fn set_clear_color(&mut self, color: [f32; 4]);

    /// Clear the bound target's scissor area to the last colour set.
    fn clear(&mut self);

    /// Make `program` current; uniforms and draws apply to it until the next call.
    fn use_program(&mut self, program: ProgramId);

    /// Bind a texture to a sampler unit; a sampler uniform still names the unit.
    fn bind_texture(&mut self, unit: u32, texture: Option<TextureId>);

    fn set_uniform_f32(&mut self, slot: UniformSlot, value: f32);
    fn set_uniform_i32(&mut self, slot: UniformSlot, value: i32);
    fn set_uniform_vec2(&mut self, slot: UniformSlot, value: [f32; 2]);
    fn set_uniform_vec4(&mut self, slot: UniformSlot, value: [f32; 4]);

    /// Upload consecutive vec4 values, packed as a flat slice.
    fn set_uniform_vec4_slice(&mut self, slot: UniformSlot, values: &[f32]);
    fn set_uniform_f32_slice(&mut self, slot: UniformSlot, values: &[f32]);
    fn set_uniform_mat3(&mut self, slot: UniformSlot, values: &[f32; 9]);
    fn set_uniform_mat4(&mut self, slot: UniformSlot, values: &[f32; 16]);

    /// Draw the unit quad once per instance, reading `instances` in the program's layout.
    fn draw_instanced(
        &mut self,
        program: ProgramId,
        instances: &[u8],
        count: u32,
    ) -> Result<(), GpuError>;

    /// Copy `rect` from `src` into `dst`, with no filtering and no blend.
    fn blit(&mut self, src: TargetId, dst: TargetId, rect: [i32; 4]);

    /// Read the target as top-down premultiplied RGBA8.
    fn read_pixels(&mut self, target: TargetId, width: u32, height: u32) -> Vec<u8>;

    /// Block until the backend has finished writing.
    fn finish(&mut self);
}
