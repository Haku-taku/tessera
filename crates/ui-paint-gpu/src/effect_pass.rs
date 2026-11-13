//! Full-screen passes that turn an effect chain into pixels.
//!
//! The renderer has no `match` on effect kinds: a pass is a separable convolution plus a
//! colour matrix, and every effect is expressed with those two.

use glow::HasContext;
use ui_core::effect::{ColorMatrix, Effect};

use crate::GpuError;
use crate::renderer::{LayerTarget, Program};
use crate::shader;

/// Create or resize one ping-pong slot, returning its framebuffer and texture.
/// Storage precision of an internal render target.
///
/// This is a property of the *pipeline*, not of the presented buffer: an f16 frame
/// keeps gradients and blurred edges from quantising before they reach the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetFormat {
    Rgba8,
    Rgba16F,
}

impl TargetFormat {
    pub(crate) fn internal(self) -> i32 {
        match self {
            TargetFormat::Rgba8 => glow::RGBA8 as i32,
            TargetFormat::Rgba16F => glow::RGBA16F as i32,
        }
    }
    /// Bytes per pixel, for sizing readbacks and expectations in tests.
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            TargetFormat::Rgba8 => 4,
            TargetFormat::Rgba16F => 8,
        }
    }
}

pub(crate) fn ensure_target(
    slot: &mut Option<LayerTarget>,
    gl: &glow::Context,
    width: u32,
    height: u32,
    format: TargetFormat,
) -> Result<(glow::Framebuffer, glow::Texture), GpuError> {
    if slot.is_none() {
        // A new target carries no storage; the resize branch below allocates it.
        *slot = Some(new_target(gl)?);
    }
    let target = slot.as_mut().expect("just created");
    if target.width != width || target.height != height || target.format != format {
        resize_target(gl, target, width, height, format);
    }
    Ok((target.fbo, target.tex))
}

/// A render target the backdrop path will sample: linear, so a warp may land between texels.
pub(crate) fn new_target(gl: &glow::Context) -> Result<LayerTarget, GpuError> {
    let tex = unsafe { gl.create_texture()? };
    let fbo = unsafe { gl.create_framebuffer()? };
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
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
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(tex),
            0,
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }
    Ok(LayerTarget {
        fbo,
        tex,
        width: 0,
        height: 0,
        format: TargetFormat::Rgba8,
    })
}

/// (Re)allocate storage; the filtering `new_target` set outlives it, as the spec requires.
pub(crate) fn resize_target(
    gl: &glow::Context,
    target: &mut LayerTarget,
    width: u32,
    height: u32,
    format: TargetFormat,
) {
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(target.tex));
        // The type must be one the internal format accepts even with no data: a half-float
        // target rejects bytes with INVALID_OPERATION and keeps its old storage, so the
        // target silently never resizes or never becomes float at all.
        let (channels, ty) = match format {
            TargetFormat::Rgba8 => (glow::RGBA, glow::UNSIGNED_BYTE),
            TargetFormat::Rgba16F => (glow::RGBA, glow::HALF_FLOAT),
        };
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            format.internal(),
            width as i32,
            height as i32,
            0,
            channels,
            ty,
            glow::PixelUnpackData::Slice(None),
        );
        // The reallocation drops the mip chain, and a mipmap filter over a missing chain makes
        // the texture incomplete: every read returns black.
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
    }
    target.width = width;
    target.height = height;
    target.format = format;
}

/// Run one sliding-window box average from `src` into `dest`.
#[allow(clippy::too_many_arguments)]
fn box_pass(
    gl: &glow::Context,
    program: &Program,
    dest: (glow::Framebuffer, glow::Texture),
    src: glow::Texture,
    width: u32,
    height: u32,
    dir: [f32; 2],
    radius: u32,
    scissor: [i32; 4],
) -> Result<(), GpuError> {
    let m = ColorMatrix::IDENTITY.0;
    let mut column_major = [0.0f32; 16];
    for row in 0..4 {
        for col in 0..4 {
            column_major[col * 4 + row] = m[row * 4 + col];
        }
    }
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dest.0));
        gl.viewport(0, 0, width as i32, height as i32);
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
        gl.use_program(Some(program.program));
        if let Some(u) = program.uniforms.viewport {
            gl.uniform_2_f32(Some(&u), width as f32, height as f32);
        }
        if let Some(u) = program.uniforms.src {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src));
            gl.uniform_1_i32(Some(&u), 0);
        }
        if let Some(u) = program.uniforms.texel {
            gl.uniform_2_f32(Some(&u), 1.0 / width as f32, 1.0 / height as f32);
        }
        if let Some(u) = program.uniforms.dir {
            gl.uniform_2_f32(Some(&u), dir[0], dir[1]);
        }
        if let Some(u) = program.uniforms.radius {
            gl.uniform_1_i32(Some(&u), radius as i32);
        }
        if let Some(u) = program.uniforms.color {
            gl.uniform_matrix_4_f32_slice(Some(&u), false, &column_major);
        }
        let rect = [0.0f32, 0.0, width as f32, height as f32];
        gl.bind_vertex_array(Some(program.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(program.instance_vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            crate::renderer::as_bytes(&rect),
            glow::DYNAMIC_DRAW,
        );
        gl.draw_arrays_instanced(glow::TRIANGLES, 0, shader::UNIT_QUAD.len() as i32 / 2, 1);
    }
    Ok(())
}

/// Run one full-screen convolution/colour pass from `src` into `dest`.
#[allow(clippy::too_many_arguments)]
fn effect_pass(
    gl: &glow::Context,
    program: &Program,
    dest: (glow::Framebuffer, glow::Texture),
    src: glow::Texture,
    width: u32,
    height: u32,
    dir: [f32; 2],
    weights: &[f32],
    color: &ColorMatrix,
    scissor: [i32; 4],
) -> Result<(), GpuError> {
    // Row-major on our side, column-major in GLSL: transpose here rather than rely on
    // `transpose = true`, which GLES rejects.
    let m = color.0;
    let mut column_major = [0.0f32; 16];
    for row in 0..4 {
        for col in 0..4 {
            column_major[col * 4 + row] = m[row * 4 + col];
        }
    }

    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dest.0));
        gl.viewport(0, 0, width as i32, height as i32);
        // Only the layer's own region is worth filtering; the rest is transparent.
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
        gl.use_program(Some(program.program));
        if let Some(u) = program.uniforms.viewport {
            gl.uniform_2_f32(Some(&u), width as f32, height as f32);
        }
        if let Some(u) = program.uniforms.src {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(src));
            gl.uniform_1_i32(Some(&u), 0);
        }
        if let Some(u) = program.uniforms.texel {
            gl.uniform_2_f32(Some(&u), 1.0 / width as f32, 1.0 / height as f32);
        }
        if let Some(u) = program.uniforms.dir {
            gl.uniform_2_f32(Some(&u), dir[0], dir[1]);
        }
        // The shader declares `u_weights[33]`; uploading more silently drops the tail and
        // leaves the kernel un-normalized, which dims the image instead of blurring it.
        let taps = weights.len().min(crate::shader::MAX_TAPS as usize);
        let weights = &weights[..taps];
        if let Some(u) = program.uniforms.taps {
            gl.uniform_1_i32(Some(&u), weights.len() as i32);
        }
        if let Some(u) = program.uniforms.weights {
            gl.uniform_1_f32_slice(Some(&u), weights);
        }
        if let Some(u) = program.uniforms.color {
            gl.uniform_matrix_4_f32_slice(Some(&u), false, &column_major);
        }
        let rect = [0.0f32, 0.0, width as f32, height as f32];
        gl.bind_vertex_array(Some(program.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(program.instance_vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            crate::renderer::as_bytes(&rect),
            glow::DYNAMIC_DRAW,
        );
        gl.draw_arrays_instanced(glow::TRIANGLES, 0, shader::UNIT_QUAD.len() as i32 / 2, 1);
    }
    Ok(())
}

/// Targets kept alive across frames purely as scratch space: the ping-pong textures the
/// passes write into. They hold nothing meaningful between frames — every call recomputes
/// `src -> output` in full, which is what Flutter/Skia/Haze all do. Reusing filtered pixels
/// across frames was tried and removed (docs/effect-cache-plan.md §11): every bookkeeping
/// variant produced a new flavour of "edges show an old frame".
#[derive(Default)]
pub(crate) struct Scratch {
    /// Filtered image; compositing reads this.
    pub output: Option<LayerTarget>,
    /// Intermediate of the horizontal pass only.
    pub scratch: Option<LayerTarget>,
    /// Second intermediate, so a multi-pass chain never reads a target it writes.
    pub band: Option<LayerTarget>,
}

/// Give `tex` a mip chain and let it be filtered across levels: the lens compresses tens of
/// texels into one pixel, and only the chain can average that footprint.
fn enable_mips(gl: &glow::Context, tex: glow::Texture) {
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl.generate_mipmap(glow::TEXTURE_2D);
    }
}

/// Drop back to single-level filtering: target textures are shared between effects, so one
/// that stops needing mips must not keep reading a chain nobody regenerates.
fn disable_mips(gl: &glow::Context, tex: glow::Texture) {
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
    }
}

/// Apply the whole `effects` chain over `src` into `scratch`, recomputing every pixel.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    scratch: &mut Scratch,
    gl: &glow::Context,
    program: &Program,
    boxpass: &Program,
    src: glow::Texture,
    width: u32,
    height: u32,
    bounds: ui_core::geom::Rect,
    format: TargetFormat,
    effects: &[Effect],
    mips: bool,
) -> Result<glow::Texture, GpuError> {
    if effects.is_empty() {
        return Ok(src);
    }
    // A zero-radius blur is a chain of full-frame copies that change nothing; a lens that
    // frosts nothing must not pay for six passes over the frame.
    if !mips && effects.iter().all(|e| e.is_noop()) {
        return Ok(src);
    }
    let dest = ensure_target(&mut scratch.output, gl, width, height, format)?;
    let to_i32 = |r: ui_core::geom::Rect| crate::renderer::device_scissor(r, 1.0);
    // Filtering and compositing stop at the layer bounds; the rest must stay transparent.
    // The destination is cleared first: a shrunk layer must not keep texels outside `bounds`.
    let area = to_i32(bounds);
    unsafe {
        gl.disable(glow::BLEND);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dest.0));
        gl.enable(glow::SCISSOR_TEST);
        gl.scissor(area[0], area[1], area[2], area[3]);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
    }
    const UNIT: [f32; 1] = [1.0];
    for effect in effects {
        if effect.is_box() {
            // Six passes h,v,h,v,h,v alternate two targets, so no pass samples its output.
            // Each pass covers the whole bounds: with full recomputation there is no
            // "region no earlier pass wrote this frame" for a pass to read.
            let passes = effect.kernel.boxes.max(1) as usize * 2;
            let mut from = src;
            for pass in 0..passes {
                let horizontal = pass % 2 == 0;
                let dir = if horizontal { [1.0, 0.0] } else { [0.0, 1.0] };
                let last = pass + 1 == passes;
                // Alternate two intermediates: a pass must never read the target it writes.
                let slot = if pass % 2 == 0 {
                    &mut scratch.scratch
                } else {
                    &mut scratch.band
                };
                let out = if last {
                    dest
                } else {
                    ensure_target(slot, gl, width, height, format)?
                };
                // Writing into the last destination must blend nothing: the pass replaces it.
                box_pass(
                    gl,
                    boxpass,
                    out,
                    from,
                    width,
                    height,
                    dir,
                    effect.kernel.radius,
                    area,
                )?;
                from = out.1;
            }
        } else if !effect.kernel.is_identity() {
            let mid = ensure_target(&mut scratch.scratch, gl, width, height, format)?;
            effect_pass(
                gl,
                program,
                mid,
                src,
                width,
                height,
                [1.0, 0.0],
                &effect.kernel.weights[..],
                &ColorMatrix::IDENTITY,
                area,
            )?;
            effect_pass(
                gl,
                program,
                dest,
                mid.1,
                width,
                height,
                [0.0, 1.0],
                &effect.kernel.weights[..],
                &effect.color,
                area,
            )?;
        } else if !effect.color.is_identity() {
            effect_pass(
                gl,
                program,
                dest,
                src,
                width,
                height,
                [0.0, 0.0],
                &UNIT[..],
                &effect.color,
                area,
            )?;
        }
    }
    unsafe { gl.enable(glow::BLEND) };
    if mips {
        enable_mips(gl, dest.1);
    } else {
        disable_mips(gl, dest.1);
    }
    Ok(dest.1)
}
