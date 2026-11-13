//! GLSL sources. The instance layouts they read are `ui_gpu_scene::layout`, which is also
//! what the painter packs against, so the two cannot describe different buffers.

/// The SDF feather, in device pixels: coverage ramps over this width, and every quad reaches
/// this far past its shape so that no covered pixel centre falls outside it.
pub const AA_GLSL: &str = include_str!("shaders/aa.glsl");

/// Clip uniforms and the coverage test shared by every fragment program.
///
/// Composed into the sources rather than duplicated, so the two programs cannot drift.
pub const CLIP_GLSL: &str = include_str!("shaders/clip.glsl");

/// Rounded-shape coverage plus the uniforms it reads, shared by every backdrop program.
pub const COVERAGE_GLSL: &str = include_str!("shaders/coverage.glsl");

/// Rounded-box signed distance, shared by the rect and the glass programs.
pub const SDF_GLSL: &str = include_str!("shaders/sdf.glsl");

pub const SHADOW_VERTEX: &str = include_str!("shaders/shadow.vert.glsl");

/// Flat ±100 px shift for diagnosis: a displaced wash proves the quad is drawn at all.
pub const SHADOW_FRAGMENT: &str = include_str!("shaders/shadow.frag.glsl");

pub const RECT_VERTEX: &str = include_str!("shaders/rect.vert.glsl");

pub const RECT_FRAGMENT: &str = include_str!("shaders/rect.frag.glsl");

pub const GLYPH_VERTEX: &str = include_str!("shaders/glyph.vert.glsl");

pub const GLYPH_FRAGMENT: &str = include_str!("shaders/glyph.frag.glsl");

/// Samples one RGBA8 texture through a rounded rect; the instance layout is the key's `ProgramSpec`.
pub const IMAGE_VERTEX: &str = include_str!("shaders/image.vert.glsl");

pub const IMAGE_FRAGMENT: &str = include_str!("shaders/image.frag.glsl");

/// Instanced quad for compositing a layer texture; the instance layout is the key's `ProgramSpec`.
pub const LAYER_VERTEX: &str = include_str!("shaders/layer.vert.glsl");

pub const LAYER_FRAGMENT: &str = include_str!("shaders/layer.frag.glsl");

/// Composite a blurred backdrop as WinUI acrylic: luminosity, tint, then grain.
pub const ACRYLIC_FRAGMENT: &str = include_str!("shaders/acrylic.frag.glsl");

/// LiquidGlassKit's material, ported whole (`LiquidGlassFragment.metal:435-540`): the face
/// shows a capture of the backdrop, the rim bends by a thin prism, then Fresnel and glare
/// are added in LCH.
pub const LIQUID_GLASS_FRAGMENT: &str = include_str!("shaders/liquid_glass.frag.glsl");

/// Full-screen pass: one separable convolution tap row plus a colour matrix.
///
/// `u_dir` picks the axis, so the same program runs the horizontal and the vertical pass;
/// `u_color` is the category's second operation, applied after the convolution.
pub const EFFECT_FRAGMENT: &str = include_str!("shaders/effect.frag.glsl");

/// Full-screen pass: one sliding-window box average along `u_dir`.
///
/// GLSL cannot carry a running sum across pixels, so the window is summed per fragment.
/// The cost is therefore O(radius) in the shader rather than O(1); three such passes still
/// beat an explicit 85-tap kernel and, more importantly, are not limited by `u_weights`.
pub const BOX_FRAGMENT: &str = include_str!("shaders/box.frag.glsl");

/// Fragment source with the shared clip block spliced in after `precision`.
///
/// `precision` must come first: GLSL ES fragment shaders have no default float precision.
pub(crate) fn splice(body: &str, block: &str) -> String {
    body.replacen(
        "precision highp float;",
        &format!("precision highp float;\n{block}"),
        1,
    )
}

fn with_clip(body: &str) -> String {
    splice(body, CLIP_GLSL)
}

fn with_coverage(body: &str) -> String {
    splice(body, COVERAGE_GLSL)
}

fn with_sdf(body: &str) -> String {
    splice(body, SDF_GLSL)
}

pub fn rect_fragment() -> String {
    with_sdf(&with_clip(RECT_FRAGMENT))
}

pub const SECTOR_VERTEX: &str = include_str!("shaders/sector.vert.glsl");

pub const SECTOR_FRAGMENT: &str = include_str!("shaders/sector.frag.glsl");

pub fn sector_fragment() -> String {
    with_clip(SECTOR_FRAGMENT)
}

pub const ELLIPSE_VERTEX: &str = include_str!("shaders/ellipse.vert.glsl");

pub const ELLIPSE_FRAGMENT: &str = include_str!("shaders/ellipse.frag.glsl");

pub fn ellipse_fragment() -> String {
    with_clip(ELLIPSE_FRAGMENT)
}

pub const LINE_VERTEX: &str = include_str!("shaders/line.vert.glsl");

pub const LINE_FRAGMENT: &str = include_str!("shaders/line.frag.glsl");

pub fn line_fragment() -> String {
    with_clip(LINE_FRAGMENT)
}

pub fn glyph_fragment() -> String {
    with_clip(GLYPH_FRAGMENT)
}

pub fn shadow_fragment() -> String {
    with_sdf(&with_clip(SHADOW_FRAGMENT))
}

pub fn image_fragment() -> String {
    with_sdf(&with_clip(IMAGE_FRAGMENT))
}

pub fn layer_fragment() -> String {
    with_coverage(LAYER_FRAGMENT)
}

pub fn acrylic_fragment() -> String {
    with_coverage(ACRYLIC_FRAGMENT)
}

pub fn liquid_glass_fragment() -> String {
    with_coverage(&with_sdf(LIQUID_GLASS_FRAGMENT))
}

/// Output transform: sRGB-encoded premultiplied in, PQ-encoded premultiplied out.
pub const OUTPUT_FRAGMENT: &str = include_str!("shaders/output.frag.glsl");

/// Supersample resolve: the N×N box average one destination pixel covers.
pub const RESOLVE_FRAGMENT: &str = include_str!("shaders/resolve.frag.glsl");

/// The lens's own backdrop capture: one region of the frame, resampled into its own texture.
pub const CAPTURE_FRAGMENT: &str = include_str!("shaders/capture.frag.glsl");
