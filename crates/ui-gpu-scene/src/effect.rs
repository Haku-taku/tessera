//! Full-screen passes that turn an effect chain into pixels.
//!
//! The renderer has no `match` on effect kinds: a pass is a separable convolution plus a
//! colour matrix, and every effect is expressed with those two.

use ui_core::draw::Sampling;
use ui_core::effect::{ColorMatrix, Effect};

use crate::device::{Device, TextureId};
use crate::error::GpuError;
use crate::format::TargetFormat;
use crate::program::Program;
use crate::renderer::{as_bytes, device_scissor};
use crate::target::{Target, ensure_target};

/// Run one sliding-window box average from `src` into `dest`.
#[allow(clippy::too_many_arguments)]
fn box_pass(
    device: &mut dyn Device,
    program: &Program,
    dest: Target,
    src: TextureId,
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
    device.bind_target(Some(dest.fbo));
    device.set_viewport(width, height);
    device.set_scissor(Some(scissor));
    device.use_program(program.id);
    if let Some(u) = program.uniforms.viewport {
        device.set_uniform_vec2(u, [width as f32, height as f32]);
    }
    if let Some(u) = program.uniforms.src {
        device.bind_texture(0, Some(src));
        device.set_uniform_i32(u, 0);
    }
    if let Some(u) = program.uniforms.texel {
        device.set_uniform_vec2(u, [1.0 / width as f32, 1.0 / height as f32]);
    }
    if let Some(u) = program.uniforms.dir {
        device.set_uniform_vec2(u, dir);
    }
    if let Some(u) = program.uniforms.radius {
        device.set_uniform_i32(u, radius as i32);
    }
    if let Some(u) = program.uniforms.color {
        device.set_uniform_mat4(u, &column_major);
    }
    let rect = [0.0f32, 0.0, width as f32, height as f32];
    device.draw_instanced(program.id, as_bytes(&rect))
}

/// Run one full-screen convolution/colour pass from `src` into `dest`.
#[allow(clippy::too_many_arguments)]
fn effect_pass(
    device: &mut dyn Device,
    program: &Program,
    dest: Target,
    src: TextureId,
    width: u32,
    height: u32,
    dir: [f32; 2],
    weights: &[f32],
    color: &ColorMatrix,
    scissor: [i32; 4],
) -> Result<(), GpuError> {
    // Row-major on our side, column-major in the shader: transpose here rather than rely
    // on a transpose flag the backend may not have.
    let m = color.0;
    let mut column_major = [0.0f32; 16];
    for row in 0..4 {
        for col in 0..4 {
            column_major[col * 4 + row] = m[row * 4 + col];
        }
    }

    device.bind_target(Some(dest.fbo));
    device.set_viewport(width, height);
    // Only the layer's own region is worth filtering; the rest is transparent.
    device.set_scissor(Some(scissor));
    device.use_program(program.id);
    if let Some(u) = program.uniforms.viewport {
        device.set_uniform_vec2(u, [width as f32, height as f32]);
    }
    if let Some(u) = program.uniforms.src {
        device.bind_texture(0, Some(src));
        device.set_uniform_i32(u, 0);
    }
    if let Some(u) = program.uniforms.texel {
        device.set_uniform_vec2(u, [1.0 / width as f32, 1.0 / height as f32]);
    }
    if let Some(u) = program.uniforms.dir {
        device.set_uniform_vec2(u, dir);
    }
    // The shader declares `u_weights[33]`; uploading more silently drops the tail and
    // leaves the kernel un-normalized, which dims the image instead of blurring it.
    let taps = weights.len().min(crate::layout::MAX_TAPS as usize);
    let weights = &weights[..taps];
    if let Some(u) = program.uniforms.taps {
        device.set_uniform_i32(u, weights.len() as i32);
    }
    if let Some(u) = program.uniforms.weights {
        device.set_uniform_f32_slice(u, weights);
    }
    if let Some(u) = program.uniforms.color {
        device.set_uniform_mat4(u, &column_major);
    }
    let rect = [0.0f32, 0.0, width as f32, height as f32];
    device.draw_instanced(program.id, as_bytes(&rect))
}

/// Targets kept alive across frames purely as scratch space: the ping-pong textures the
/// passes write into. They hold nothing meaningful between frames — every call recomputes
/// `src -> output` in full, which is what Flutter/Skia/Haze all do. Reusing filtered pixels
/// across frames was tried and removed (docs/effect-cache-plan.md §11): every bookkeeping
/// variant produced a new flavour of "edges show an old frame".
#[derive(Default)]
pub(crate) struct Scratch {
    /// Filtered image; compositing reads this.
    pub(crate) output: Option<Target>,
    /// Intermediate of the horizontal pass only.
    pub(crate) scratch: Option<Target>,
    /// Second intermediate, so a multi-pass chain never reads a target it writes.
    pub(crate) band: Option<Target>,
}

/// Copy the region into the scratch output, leaving the rest of the target clear: a material
/// that filters nothing still cannot read the frame while that frame is bound for writing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn copy(
    scratch: &mut Scratch,
    device: &mut dyn Device,
    program: &Program,
    src: TextureId,
    width: u32,
    height: u32,
    bounds: ui_core::geom::Rect,
    format: TargetFormat,
) -> Result<TextureId, GpuError> {
    let dest = ensure_target(&mut scratch.output, device, width, height, format)?;
    let area = device_scissor(bounds, 1.0);
    device.set_blend(false);
    device.bind_target(Some(dest.fbo));
    device.set_scissor(Some(area));
    device.set_clear_color([0.0, 0.0, 0.0, 0.0]);
    device.clear();
    const UNIT: [f32; 1] = [1.0];
    effect_pass(
        device,
        program,
        dest,
        src,
        width,
        height,
        [0.0, 0.0],
        &UNIT[..],
        &ColorMatrix::IDENTITY,
        area,
    )?;
    device.set_blend(true);
    device.set_sampling(dest.tex, Sampling::Smooth, false);
    Ok(dest.tex)
}

/// Apply the whole `effects` chain over `src` into `scratch`, recomputing every pixel.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    scratch: &mut Scratch,
    device: &mut dyn Device,
    program: &Program,
    boxpass: &Program,
    src: TextureId,
    width: u32,
    height: u32,
    bounds: ui_core::geom::Rect,
    format: TargetFormat,
    effects: &[Effect],
    mips: bool,
) -> Result<TextureId, GpuError> {
    if effects.is_empty() {
        return Ok(src);
    }
    // A zero-radius blur is a chain of full-frame copies that change nothing; a lens that
    // frosts nothing must not pay for six passes over the frame.
    if !mips && effects.iter().all(|e| e.is_noop()) {
        return Ok(src);
    }
    let dest = ensure_target(&mut scratch.output, device, width, height, format)?;
    let to_i32 = |r: ui_core::geom::Rect| device_scissor(r, 1.0);
    // Filtering and compositing stop at the layer bounds; the rest must stay transparent.
    // The destination is cleared first: a shrunk layer must not keep texels outside `bounds`.
    let area = to_i32(bounds);
    device.set_blend(false);
    device.bind_target(Some(dest.fbo));
    device.set_scissor(Some(area));
    device.set_clear_color([0.0, 0.0, 0.0, 0.0]);
    device.clear();
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
                    ensure_target(slot, device, width, height, format)?
                };
                // Writing into the last destination must blend nothing: the pass replaces it.
                box_pass(
                    device,
                    boxpass,
                    out,
                    from,
                    width,
                    height,
                    dir,
                    effect.kernel.radius,
                    area,
                )?;
                from = out.tex;
            }
        } else if !effect.kernel.is_identity() {
            let mid = ensure_target(&mut scratch.scratch, device, width, height, format)?;
            effect_pass(
                device,
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
                device,
                program,
                dest,
                mid.tex,
                width,
                height,
                [0.0, 1.0],
                &effect.kernel.weights[..],
                &effect.color,
                area,
            )?;
        } else if !effect.color.is_identity() {
            effect_pass(
                device,
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
    device.set_blend(true);
    // A chain that asked for mips needs the chain rebuilt after the last write; one that
    // did not must drop back to single-level filtering, since targets are shared.
    device.set_sampling(dest.tex, Sampling::Smooth, mips);
    Ok(dest.tex)
}
