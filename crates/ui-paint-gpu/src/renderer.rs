//! Draws a `ui_core::DrawList` with OpenGL ES.
//!
//! Rectangles use an instanced signed-distance shader; glyphs sample a coverage
//! atlas. Both emit premultiplied alpha so one blend equation covers both.

use glow::HasContext;
use std::collections::{HashMap, HashSet};
use ui_core::draw::{DrawList, GlyphPrim, ImageEntry, ImagePrim, PrimKind, RectPrim, ShadowPrim};
use ui_core::effect::Effect;
use ui_core::geom::Color;
use ui_core::timed;

use crate::material::BackdropMaterial;

/// Which program shades a blurred backdrop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shade {
    Plain,
    Acrylic,
    /// LiquidGlassKit's material: the one glass program.
    Liquid,
}

/// Which material a backdrop entry is shaded with: its own if it names one, else the
/// renderer-wide default.
pub(crate) fn resolve(
    entry: BackdropMaterial,
    default: Option<BackdropMaterial>,
) -> Option<BackdropMaterial> {
    match entry {
        BackdropMaterial::Plain => default,
        named => Some(named),
    }
}

impl Shade {
    /// The plain blit is the fallback for "no material" and for an explicit `Plain`.
    pub(crate) fn of(material: Option<BackdropMaterial>) -> Self {
        match material {
            None | Some(BackdropMaterial::Plain) => Shade::Plain,
            Some(BackdropMaterial::Acrylic(_)) => Shade::Acrylic,
            Some(BackdropMaterial::LiquidGlass(_)) => Shade::Liquid,
        }
    }
}
/// Whether a backdrop entry has to run the composite pass. A plain blit only changes the
/// frame when the blur did; a shaded material is all there is to draw.
pub(crate) fn composites(material: Option<BackdropMaterial>, blurred: bool) -> bool {
    blurred || !matches!(material, None | Some(BackdropMaterial::Plain))
}

use crate::GpuError;
use crate::shader;
use ui_gpu_egl::GpuSurface;

/// Fused shapes arrive in the node's logical space; the shader works in device pixels, so
/// each is offset by the node's origin and scaled once.
fn upload_shapes(
    gl: &glow::Context,
    p: &Program,
    blend: &[ui_core::draw::BlendShape],
    count: u8,
    device: ui_core::geom::Rect,
    s: f32,
) {
    if count == 0 {
        return;
    }
    let mut boxes = [0.0f32; 16];
    let mut radii = [0.0f32; 16];
    for (i, shape) in blend.iter().take(count as usize).enumerate() {
        let hw = shape.rect.size.w.abs() * 0.5 * s;
        let hh = shape.rect.size.h.abs() * 0.5 * s;
        boxes[i * 4] = device.origin.x + shape.rect.origin.x * s + hw;
        boxes[i * 4 + 1] = device.origin.y + shape.rect.origin.y * s + hh;
        boxes[i * 4 + 2] = hw;
        boxes[i * 4 + 3] = hh;
        let cap = hw.min(hh);
        radii[i * 4] = (shape.corners.tl * s).min(cap);
        radii[i * 4 + 1] = (shape.corners.tr * s).min(cap);
        radii[i * 4 + 2] = (shape.corners.br * s).min(cap);
        radii[i * 4 + 3] = (shape.corners.bl * s).min(cap);
    }
    if let Some(u) = p.uniforms.shapes {
        unsafe { gl.uniform_4_f32_slice(Some(&u), &boxes) };
    }
    if let Some(u) = p.uniforms.shape_radius {
        unsafe { gl.uniform_4_f32_slice(Some(&u), &radii) };
    }
}

/// Counters for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    pub rects: u32,
    pub glyphs: u32,
    pub draw_calls: u32,
    /// Frosts executed this frame.
    pub frosts: u32,
    /// Backdrop passes that also shaded a material (acrylic or glass) after the blur.
    pub material_passes: u32,
    /// Image textures uploaded this frame; a cached `ImageEntry::id` does not count.
    pub image_uploads: u32,
    /// Soft shadows drawn this frame.
    pub shadows_drawn: u32,
    /// Image primitives drawn this frame.
    pub images_drawn: u32,
    /// Batches skipped because the cached subtree they belong to did not change.
    pub cached_skips: u32,
    /// Batches skipped because the redraw band stays clear of their layer's bounds.
    pub band_skips: u32,
    /// Batches skipped because they miss the redraw band and could write no pixel.
    pub band_culls: u32,
    /// The device rect this frame redrew, or `None` when it redrew everything.
    pub band: Option<[i32; 4]>,
}

/// A coverage atlas uploaded to the GPU.
pub struct AtlasUpload {
    pub width: u32,
    pub height: u32,
    /// Single-channel coverage.
    pub data: Vec<u8>,
}

pub(crate) struct Program {
    pub(crate) program: glow::Program,
    pub(crate) vao: glow::VertexArray,
    unit_vbo: glow::Buffer,
    pub(crate) instance_vbo: glow::Buffer,
    pub(crate) uniforms: Uniforms,
}

pub(crate) struct Uniforms {
    pub(crate) viewport: Option<glow::UniformLocation>,
    atlas_size: Option<glow::UniformLocation>,
    atlas: Option<glow::UniformLocation>,
    color_atlas_size: Option<glow::UniformLocation>,
    color_atlas: Option<glow::UniformLocation>,
    image: Option<glow::UniformLocation>,
    opacity: Option<glow::UniformLocation>,
    /// Sampler name differs per program: glyphs use `u_atlas`, layers `u_layer`.
    layer_tex: Option<glow::UniformLocation>,
    clip_rect: Option<glow::UniformLocation>,
    clip_radius: Option<glow::UniformLocation>,
    clip_shape: Option<glow::UniformLocation>,
    clip_on: Option<glow::UniformLocation>,
    /// Effect pass inputs; `None` for programs that do not run one.
    pub(crate) src: Option<glow::UniformLocation>,
    pub(crate) texel: Option<glow::UniformLocation>,
    pub(crate) dir: Option<glow::UniformLocation>,
    pub(crate) taps: Option<glow::UniformLocation>,
    /// Sliding-window radius, used by the box program instead of `u_weights`.
    pub(crate) radius: Option<glow::UniformLocation>,
    /// Supersample factor, used by the resolve program.
    pub(crate) supersample: Option<glow::UniformLocation>,
    /// Whether the resolve pass must encode linear light back into sRGB bytes.
    pub(crate) encode: Option<glow::UniformLocation>,
    /// The lens capture in device pixels, read by the kit shader; the capture pass takes the
    /// same region in frame uv under `u_region`.
    pub(crate) capture: Option<glow::UniformLocation>,
    pub(crate) region: Option<glow::UniformLocation>,
    pub(crate) weights: Option<glow::UniformLocation>,
    pub(crate) color: Option<glow::UniformLocation>,
    /// Output transform inputs; `None` for programs that do not encode.
    pub(crate) scale: Option<glow::UniformLocation>,
    pub(crate) offset: Option<glow::UniformLocation>,
    pub(crate) gamut: Option<glow::UniformLocation>,
    layer_rect: Option<glow::UniformLocation>,
    layer_radius: Option<glow::UniformLocation>,
    layer_exp: Option<glow::UniformLocation>,
    layer_skip_empty: Option<glow::UniformLocation>,
    /// Material uniforms; `None` for programs that shade no backdrop.
    tint: Option<glow::UniformLocation>,
    luminosity: Option<glow::UniformLocation>,
    noise: Option<glow::UniformLocation>,
    /// Glass: the neutral body transfer.
    body: Option<glow::UniformLocation>,
    glass: Option<glow::UniformLocation>,
    light: Option<glow::UniformLocation>,
    /// Glass: refractive index, dispersion, thickness, capture density.
    optical: Option<glow::UniformLocation>,
    /// Blend group: live shape count and smooth-union radius.
    blend: Option<glow::UniformLocation>,
    /// Kit lens: Fresnel intensity, distance, sharpness, glare convergence.
    kit: Option<glow::UniformLocation>,
    /// Blend group: each shape's centre and half size, device pixels.
    shapes: Option<glow::UniformLocation>,
    /// Blend group: each shape's four corner radii, device pixels.
    shape_radius: Option<glow::UniformLocation>,
}

/// GPU painter.
/// One offscreen render target for a layer, reused across frames by depth.
pub(crate) struct LayerTarget {
    pub(crate) fbo: glow::Framebuffer,
    pub(crate) tex: glow::Texture,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Storage precision, so a format change reallocates instead of silently reusing.
    pub(crate) format: crate::effect_pass::TargetFormat,
}

/// Where a batch is drawn: the render target's extent and the band it may touch.
#[derive(Clone, Copy)]
struct Pass {
    viewport: (f32, f32),
    band: Option<[i32; 4]>,
}

/// What every layer push has to know about the frame being drawn.
struct FrameTargets<'a> {
    /// Regions that changed; a cached subtree outside them keeps its pixels.
    damage: &'a ui_core::damage::Damage,
    /// The redraw band in render pixels; `None` means the whole frame was cleared.
    band: Option<[i32; 4]>,
    viewport: (f32, f32),
}

/// What a popped layer needs to know to finish this frame.
#[derive(Clone, Copy)]
struct Finish {
    /// The subtree target to keep the pixels in, if this layer has one.
    cache: Option<u64>,
    /// Nothing inside the layer was drawn: it kept the pixels of its last composite.
    clean: bool,
    /// The parent region was repainted, so the layer has to be composited into it.
    composite: bool,
}

/// The active target chain, and per level whether this frame needs it at all.
#[derive(Default)]
struct Chain {
    stack: Vec<u32>,
    /// `true` when the level kept its pixels and neither it nor its parent needs work.
    clean: Vec<bool>,
    /// `true` when `clean` holds because the band stays clear of the level's bounds.
    band_clear: Vec<bool>,
}

/// A device-space band as a rect.
fn rect_of([x, y, w, h]: [i32; 4]) -> ui_core::geom::Rect {
    ui_core::geom::Rect::new(x as f32, y as f32, w as f32, h as f32)
}

/// The band a frame may cull batches by.
///
/// A layer being redrawn is cleared first and, when it carries a cache key, is stored and
/// composited as a whole: culling a batch out of it would cache a layer with holes and paint
/// them back on the next frame that does not redraw it.
fn culling_band(
    band: Option<[i32; 4]>,
    layers: &[ui_core::draw::LayerEntry],
    stack: &[u32],
) -> Option<[i32; 4]> {
    let any_cached = stack.iter().any(|index| {
        layers
            .get(*index as usize)
            .is_some_and(|l| l.cache_key.is_some())
    });
    if any_cached { None } else { band }
}

/// Whether the frame's band stays clear of a device rect, so nothing inside it changed.
fn band_clear_of(band: Option<[i32; 4]>, bounds: ui_core::geom::Rect) -> bool {
    band_rect(band).is_some_and(|b| b.intersection(bounds).is_none())
}

/// The band as a device-space rect, for comparing against layer bounds.
fn band_rect(band: Option<[i32; 4]>) -> Option<ui_core::geom::Rect> {
    band.map(rect_of)
}

/// Whether this frame cleared the frame target over a device-space `bounds`.
fn frame_cleared_over(band: Option<[i32; 4]>, bounds: Option<ui_core::geom::Rect>) -> bool {
    match band_rect(band) {
        None => true,
        Some(b) => bounds.is_none_or(|r| b.intersects(r)),
    }
}

/// A subtree target that outlives the frame, keyed by `LayerEntry::cache_key`.
pub(crate) struct CachedTarget {
    pub(crate) target: LayerTarget,
    /// Whether `target` holds a finished composite of the subtree.
    pub(crate) valid: bool,
    /// Frame number this target was last asked for, so vanished canvases are freed.
    pub(crate) last_used: u64,
}

/// A lens capture kept for reuse, one per size. Two glass cards of different heights
/// alternate within a frame, so a single slot would re-specify a texture the queue still
/// samples twice per frame — which stalls the driver.
struct CaptureTarget {
    target: LayerTarget,
    /// Frame number this capture was last written, so the pool can re-form a stale slot.
    last_used: u64,
}

/// Distinct lens sizes to keep before the oldest slot is re-formed instead of added to.
const CAPTURE_TARGETS: usize = 4;

/// One cached image texture; `id` and size decide whether the upload is reused.
struct ImageTexture {
    tex: glow::Texture,
    width: u32,
    height: u32,
    /// The buffer this texture was uploaded from. Holding it keeps the address unique, so
    /// a later frame can tell "the same picture" from "a new one behind the same id".
    data: std::sync::Arc<Vec<u8>>,
    /// What the buffer held: a different description means a different conversion.
    colour: ui_core::colour::ContentColour,
    /// The conversion the texture was built with; a switch re-uploads.
    destination: ui_core::colour::Destination,
}

/// The texture one `ImagePrim` samples, resolved for the current frame.
#[derive(Clone, Copy)]
struct FrameImage {
    tex: glow::Texture,
    width: u32,
    height: u32,
    /// The filter this draw must set: a canvas texture serves every sampling, so its state
    /// cannot be baked in at upload the way a picture's is.
    sampling: Option<ui_core::draw::Sampling>,
}

/// One canvas this frame draws: its pixels, or only the target it already holds.
pub struct CanvasUpload<'a> {
    pub id: ui_core::draw::CanvasId,
    pub size: (u32, u32),
    /// `Some` when the app's version changed: paint it before the frame samples it.
    pub list: Option<&'a DrawList>,
}

/// A canvas's persistent target, kept until a frame stops naming it.
struct CanvasTarget {
    target: Option<LayerTarget>,
    /// The frame that last named this canvas.
    last_used: u64,
}

pub struct GpuRenderer {
    rects: Program,
    /// Stroked circular arcs, shaped analytically in the fragment.
    sectors: Program,
    /// Filled and stroked ellipses, shaped analytically in the fragment.
    ellipses: Program,
    /// Stroked segments, shaped analytically in the fragment.
    lines: Program,
    glyphs: Program,
    /// Rounded-rect texture sampling, one draw per run of primitives sharing a texture.
    images: Program,
    /// Analytic soft shadows; no blur pass, so a shadow costs one draw.
    shadows: Program,
    layers: Program,
    /// Full-screen pass shared by every effect in the chain.
    effects: Program,
    /// Output transform into PQ, used only when HDR is declared: encoded and linear
    /// variants of the same source, picked by the working domain.
    output_srgb: Program,
    output_linear: Program,
    /// Box average from the supersampled frame down to the presented buffer.
    resolve: Program,
    resolve_scratch: Option<LayerTarget>,
    /// Resample one region of the frame into a lens's own capture.
    capture: Program,
    capture_targets: Vec<CaptureTarget>,
    /// Rendering resolution multiplier: every internal target is `ss²` times the surface.
    supersample: u32,
    /// (scale, offset) for the PQ output transform; `None` keeps the buffer as sRGB.
    hdr: Option<[f32; 2]>,
    /// What this renderer draws onto: the domain, its white and peak, and the tone curve.
    /// Content is converted for it, so a change here invalidates every image texture.
    destination: ui_core::colour::Destination,
    /// Working domain: encoded sRGB for SDR, linear light for HDR.
    encode: Encode,
    /// Internal storage precision for the frame and every layer target.
    target_format: crate::effect_pass::TargetFormat,
    /// sRGB -> output gamut, row-major; identity until a description is read.
    gamut: [[f32; 3]; 3],
    output_scratch: Option<LayerTarget>,
    /// Device-space bounds per nesting depth: the only region worth filtering or compositing.
    layer_bounds: Vec<ui_core::geom::Rect>,
    /// Device-space corner radii per depth, mirroring `layer_bounds`.
    layer_radius: Vec<ui_core::geom::Corners>,
    /// Persistent frame buffer: the target rotates, so the accumulated frame lives here.
    frame: Option<LayerTarget>,
    /// Root of the active layer chain; `end_layer` composites depth 0 here.
    root_fbo: Option<glow::Framebuffer>,
    /// Ping-pong scratch per nesting depth. These hold no meaningful content between
    /// frames: each frame recomputes every stage from the live scene.
    effect_scratch: Vec<crate::effect_pass::Scratch>,
    /// Sliding-window box pass, used for blurs whose radius exceeds the kernel array.
    boxpass: Program,
    /// Acrylic composite over the blurred backdrop.
    acrylic: Program,
    /// The same in the linear domain, where the encoded-space maths needs a crossing.
    acrylic_linear: Program,
    /// LiquidGlassKit's material: the captured face and the thin-prism rim.
    glass_kit: Program,
    glass_kit_linear: Program,
    /// Style-level backdrop material; `None` keeps the plain blur blit.
    material: Option<BackdropMaterial>,
    /// Scratch for backdrop blurs; kept apart from the layer scratch so a frost and a
    /// layer effect in the same frame cannot overwrite each other's intermediates.
    backdrop_scratch: crate::effect_pass::Scratch,
    /// Offscreen targets indexed by nesting depth.
    layer_targets: Vec<LayerTarget>,
    /// Subtree targets that persist across frames, keyed by the layer's cache key.
    cached_targets: HashMap<u64, CachedTarget>,
    /// Frame counter, so a canvas absent this frame can be freed.
    frame_serial: u64,
    atlas_tex: Option<glow::Texture>,
    atlas_size: (u32, u32),
    color_atlas_tex: Option<glow::Texture>,
    color_atlas_size: (u32, u32),
    /// Uploaded image textures keyed by picture and sampling; keys absent from a frame are
    /// freed. One picture drawn both ways holds two textures, because the filter is a
    /// property of the texture object.
    image_textures: HashMap<(u64, ui_core::draw::Sampling), ImageTexture>,
    /// Canvas targets this frame draws, resolved to their textures before the main list.
    /// The filter is set at the draw, not at the texture, since one canvas serves both
    /// samplings.
    canvas_images: HashMap<ui_core::draw::CanvasId, FrameImage>,
    /// Canvas targets by id; one absent from a frame is freed with its texture.
    canvas_targets: HashMap<ui_core::draw::CanvasId, CanvasTarget>,
    /// Image cache keys named anywhere this frame — the main list and every canvas list.
    /// Uploads are freed against this union, so a picture a canvas samples is not evicted
    /// by the frame that never names it.
    frame_keys: HashSet<(u64, ui_core::draw::Sampling)>,
    /// Textures resolved for the frame being drawn, parallel to `DrawList::images`.
    frame_images: Vec<FrameImage>,
    scale: f32,
    stats: RenderStats,
}

impl GpuRenderer {
    /// Compile both programs and set up vertex arrays.
    pub fn new(gl: &glow::Context, is_gles: bool) -> Result<Self, GpuError> {
        let layers = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &shader::layer_fragment(),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_opacity",
                "u_layer",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_skip_empty",
            ],
        )?;
        let effects = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::EFFECT_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_texel",
                "u_dir",
                "u_taps",
                "u_weights",
                "u_color",
            ],
        )?;
        let boxpass = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::BOX_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_texel",
                "u_dir",
                "u_radius",
                "u_color",
            ],
        )?;
        // One source, two programs: the branch sits at the encoding boundary only.
        let output_srgb = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::OUTPUT_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &["u_viewport", "u_src", "u_scale", "u_offset", "u_gamut"],
        )?;
        let output_linear = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &format!("#define LINEAR_DOMAIN\n{}", shader::OUTPUT_FRAGMENT),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &["u_viewport", "u_src", "u_scale", "u_offset", "u_gamut"],
        )?;
        let resolve = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::RESOLVE_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &["u_viewport", "u_src", "u_ss", "u_encode"],
        )?;
        let capture = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            shader::CAPTURE_FRAGMENT,
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &["u_viewport", "u_src", "u_region"],
        )?;
        let rects = Program::new(
            gl,
            is_gles,
            shader::RECT_VERTEX,
            &shader::rect_fragment(),
            shader::RECT_STRIDE,
            &[
                ("a_rect", 4),
                ("a_radius", 4),
                ("a_fill", 4),
                ("a_border", 4),
                ("a_border2", 4),
                ("a_border_g", 1),
                ("a_shape", 1),
                ("a_fill2", 4),
                ("a_grad", 2),
                ("a_stops", 4),
                ("a_extent", 2),
                ("a_border_w", 1),
                ("a_notch", 2),
                ("a_flip", 2),
            ],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let sectors = Program::new(
            gl,
            is_gles,
            shader::SECTOR_VERTEX,
            &shader::sector_fragment(),
            shader::SECTOR_STRIDE,
            &[("a_centre", 2), ("a_geom", 4), ("a_fill", 4), ("a_caps", 1)],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let ellipses = Program::new(
            gl,
            is_gles,
            shader::ELLIPSE_VERTEX,
            &shader::ellipse_fragment(),
            shader::ELLIPSE_STRIDE,
            &[
                ("a_centre", 2),
                ("a_radii", 2),
                ("a_width", 1),
                ("a_fill", 4),
                ("a_stroke", 4),
            ],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let lines = Program::new(
            gl,
            is_gles,
            shader::LINE_VERTEX,
            &shader::line_fragment(),
            shader::LINE_STRIDE,
            &[("a_from", 2), ("a_to", 2), ("a_width", 1), ("a_fill", 4)],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let glyphs = Program::new(
            gl,
            is_gles,
            shader::GLYPH_VERTEX,
            &shader::glyph_fragment(),
            shader::GLYPH_STRIDE,
            &[("a_rect", 4), ("a_atlas", 4), ("a_color", 4), ("a_kind", 1)],
            &[
                "u_viewport",
                "u_atlas_size",
                "u_atlas",
                "u_color_atlas_size",
                "u_color_atlas",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let shadows = Program::new(
            gl,
            is_gles,
            shader::SHADOW_VERTEX,
            &shader::shadow_fragment(),
            shader::SHADOW_STRIDE,
            &[
                ("a_rect", 4),
                ("a_radius", 4),
                ("a_colour", 4),
                ("a_soft", 2),
                ("a_shape", 1),
                ("a_ring", 1),
                ("a_cut", 4),
                ("a_inset", 1),
            ],
            &[
                "u_viewport",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let images = Program::new(
            gl,
            is_gles,
            shader::IMAGE_VERTEX,
            &shader::image_fragment(),
            shader::IMAGE_STRIDE,
            &[("a_rect", 4), ("a_radius", 4), ("a_uv", 4), ("a_shape", 1)],
            &[
                "u_viewport",
                "u_image",
                "u_clip_rect",
                "u_clip_radius",
                "u_clip_shape",
                "u_clip_on",
            ],
        )?;
        let acrylic = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &shader::acrylic_fragment(),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_tint",
                "u_luminosity",
                "u_noise",
            ],
        )?;
        let acrylic_linear = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &format!("#define LINEAR_DOMAIN\n{}", shader::acrylic_fragment()),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_tint",
                "u_luminosity",
                "u_noise",
            ],
        )?;
        let glass_kit_linear = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &format!("#define LINEAR_DOMAIN\n{}", shader::liquid_glass_fragment()),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_glass",
                "u_optical",
                "u_light",
                "u_kit",
                "u_blend",
                "u_capture",
                "u_shape[0]",
                "u_shape_radius[0]",
            ],
        )?;
        let glass_kit = Program::new(
            gl,
            is_gles,
            shader::LAYER_VERTEX,
            &shader::liquid_glass_fragment(),
            shader::RECT_STRIDE,
            &[("a_rect", 4)],
            &[
                "u_viewport",
                "u_src",
                "u_rect",
                "u_radius",
                "u_exp",
                "u_glass",
                "u_optical",
                "u_light",
                "u_kit",
                "u_blend",
                "u_capture",
                "u_shape[0]",
                "u_shape_radius[0]",
            ],
        )?;
        Ok(Self {
            rects,
            sectors,
            ellipses,
            lines,
            glyphs,
            images,
            shadows,
            layers,
            effects,
            boxpass,
            acrylic,
            acrylic_linear,
            glass_kit,
            glass_kit_linear,
            material: None,
            output_srgb,
            output_linear,
            resolve,
            resolve_scratch: None,
            capture,
            capture_targets: Vec::new(),
            supersample: 1,
            layer_bounds: Vec::new(),
            layer_radius: Vec::new(),
            hdr: None,
            destination: ui_core::colour::Destination::SDR,
            encode: Encode::Srgb,
            target_format: crate::effect_pass::TargetFormat::Rgba8,
            gamut: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            output_scratch: None,
            frame: None,
            root_fbo: None,
            effect_scratch: Vec::new(),
            backdrop_scratch: Default::default(),
            layer_targets: Vec::new(),
            cached_targets: HashMap::new(),
            frame_serial: 0,
            atlas_tex: None,
            atlas_size: (1, 1),
            color_atlas_tex: None,
            color_atlas_size: (1, 1),
            image_textures: HashMap::new(),
            canvas_images: HashMap::new(),
            canvas_targets: HashMap::new(),
            frame_keys: HashSet::new(),
            frame_images: Vec::new(),
            scale: 1.0,
            stats: RenderStats::default(),
        })
    }

    /// Every frame now repaints the whole surface, so a presenter must treat the damage
    /// as full. This reports it explicitly instead of letting callers guess.
    pub fn repaints_everything(&self) -> bool {
        true
    }

    /// Device pixels per logical point.
    /// Turn on the PQ output transform; `None` writes sRGB straight through.
    ///
    /// The domain and the frame precision are one choice: linear light in eight bits bands
    /// in the shadows far worse than sRGB does, so HDR always stores f16.
    /// Draw for `destination` from the next frame on.
    pub fn set_destination(&mut self, destination: ui_core::colour::Destination) {
        self.destination = destination;
        self.hdr = match destination.domain {
            ui_core::colour::Domain::Encoded => None,
            ui_core::colour::Domain::Linear {
                reference_nits,
                min_nits,
                ..
            } => Some([
                ui_core::colour::reference_scale(reference_nits),
                ui_core::colour::min_offset(min_nits),
            ]),
        };
        self.encode = Encode::of(destination);
        self.target_format = match self.encode {
            Encode::Srgb => crate::effect_pass::TargetFormat::Rgba8,
            Encode::Linear => crate::effect_pass::TargetFormat::Rgba16F,
        };
    }

    /// What this renderer draws onto; a change needs every image texture rebuilt.
    pub fn destination(&self) -> ui_core::colour::Destination {
        self.destination
    }

    /// The frame's storage precision, which follows the working domain.
    pub fn frame_format(&self) -> crate::effect_pass::TargetFormat {
        self.target_format
    }

    /// Set the sRGB -> output gamut matrix, row-major.
    pub fn set_gamut(&mut self, gamut: [[f32; 3]; 3]) {
        self.gamut = gamut;
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Render `ss` times finer than the surface, then box-average down on present: the lens
    /// magnifies what earlier batches painted, and a rasterized 1× frame holds no more detail.
    pub fn set_supersample(&mut self, ss: u32) {
        self.supersample = ss.clamp(1, 4);
    }

    pub fn supersample(&self) -> u32 {
        self.supersample
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Install the style's backdrop material; `None` keeps the plain blur blit.
    pub fn set_backdrop_material(&mut self, material: Option<BackdropMaterial>) {
        self.material = material;
    }

    /// Upload (or replace) the coverage atlas.
    pub fn set_atlas(&mut self, gl: &glow::Context, atlas: AtlasUpload) -> Result<(), GpuError> {
        upload_atlas(
            gl,
            &mut self.atlas_tex,
            atlas.width,
            atlas.height,
            glow::R8,
            glow::RED,
            glow::UNSIGNED_BYTE,
            &atlas.data,
        )?;
        self.atlas_size = (atlas.width, atlas.height);
        Ok(())
    }

    /// Upload (or replace) the colour atlas: premultiplied RGBA8, converted to the domain.
    pub fn set_color_atlas(
        &mut self,
        gl: &glow::Context,
        atlas: AtlasUpload,
    ) -> Result<(), GpuError> {
        let linear = match self.encode {
            Encode::Srgb => None,
            Encode::Linear => Some(premultiplied_to_linear(&atlas.data)),
        };
        let (internal, kind, data) = match &linear {
            None => (glow::RGBA8, glow::UNSIGNED_BYTE, atlas.data.as_slice()),
            Some(bytes) => (glow::RGBA16F, glow::HALF_FLOAT, bytes.as_slice()),
        };
        upload_atlas(
            gl,
            &mut self.color_atlas_tex,
            atlas.width,
            atlas.height,
            internal,
            glow::RGBA,
            kind,
            data,
        )?;
        self.color_atlas_size = (atlas.width, atlas.height);
        Ok(())
    }

    /// Clear `target` to `color` (straight alpha, 0..=1).
    pub fn clear(&mut self, gl: &glow::Context, target: &dyn GpuSurface, color: [f32; 4]) {
        target.bind(gl);
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(color[0], color[1], color[2], color[3]);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    /// Render `list` into `target`.
    ///
    /// Every frame clears the persistent frame buffer and repaints the whole scene. That is
    /// what Flutter, Skia-backed toolkits and Haze all do, and it is the reason they do not
    /// have the class of bug we kept hitting: no stage can read a pixel that nobody wrote
    /// this frame. Cross-frame reuse of filtered pixels was tried and removed — see
    /// `docs/effect-cache-plan.md` §11.
    pub fn render(
        &mut self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
        list: &DrawList,
    ) -> Result<RenderStats, GpuError> {
        self.render_frame(gl, target, list, &ui_core::damage::Damage::Full)
    }

    /// Render only `damage` into the persistent frame, then hand that band to `target`.
    ///
    /// The band is one rect: every pixel inside it is cleared and redrawn, so no blend
    /// surface is drawn twice; a frost widens the band over itself instead.
    pub fn render_frame(
        &mut self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
        list: &DrawList,
        damage: &ui_core::damage::Damage,
    ) -> Result<RenderStats, GpuError> {
        self.render_frame_into(gl, target, list, damage, &[])
    }

    /// The same, also copying `carry` to `target`: the rects it is still missing.
    ///
    /// A buffer that is reused round-robin holds the bands drawn on the frames it was
    /// assigned to, so everything drawn since then has to travel with this frame's band.
    pub fn render_frame_into(
        &mut self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
        list: &DrawList,
        damage: &ui_core::damage::Damage,
        carry: &[[i32; 4]],
    ) -> Result<RenderStats, GpuError> {
        self.render_frame_with_canvases(gl, target, list, damage, carry, &[])
    }

    /// The same, painting `canvases` before the frame that samples them.
    ///
    /// A canvas is drawn in its own pixels at scale 1, so the frame's damage band and the
    /// surface scale never reach it; the frame then samples its target as one picture.
    pub fn render_frame_with_canvases(
        &mut self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
        list: &DrawList,
        damage: &ui_core::damage::Damage,
        carry: &[[i32; 4]],
        canvases: &[CanvasUpload<'_>],
    ) -> Result<RenderStats, GpuError> {
        self.stats = RenderStats::default();
        // The frame's identity starts here: every image key and every canvas target this
        // frame touches is stamped with it, and anything older is freed at the end.
        self.frame_serial += 1;
        self.frame_keys.clear();
        self.paint_canvases(gl, canvases)?;
        // Everything below is measured in render pixels: the surface times `ss`. Geometry
        // and scissor conversion ride the same factor, so the scene is simply drawn finer.
        let surface = (target.size().0, target.size().1);
        let ss = self.supersample;
        let size = (surface.0 * ss, surface.1 * ss);
        let scale = self.scale;
        self.scale = scale * ss as f32;
        let (frame_fbo, frame_tex) = crate::effect_pass::ensure_target(
            &mut self.frame,
            gl,
            size.0,
            size.1,
            self.target_format,
        )?;
        // Only the redraw band is cleared: everything outside it still holds the pixels the
        // previous frame left there, and that is exactly what the band promises.
        let band = timed!("gpu_band", self.damage_band(damage, surface, list));
        self.stats.band = band;
        match band {
            Some(_) => PARTIAL_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            None => FULL_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        };
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(frame_fbo));
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            match band {
                Some(b) => {
                    gl.enable(glow::SCISSOR_TEST);
                    gl.scissor(b[0], b[1], b[2], b[3]);
                }
                None => gl.disable(glow::SCISSOR_TEST),
            }
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.root_fbo = Some(frame_fbo);
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(frame_fbo));
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
        }
        let viewport = (size.0 as f32, size.1 as f32);
        unsafe {
            gl.enable(glow::BLEND);
            // Premultiplied source over destination.
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::SCISSOR_TEST);
        }

        let mut rect_data: Vec<f32> = Vec::new();
        let mut glyph_data: Vec<f32> = Vec::new();
        // Active layer chain; depth `d` renders into `layer_targets[d]`.
        let mut chain = Chain::default();
        let frame = FrameTargets {
            damage,
            band,
            viewport,
        };

        // A layer starts empty, so a frosted surface inside one samples its parent: the
        // layer is composited back over that same parent, so it is what the surface covers.
        let backdrops = list.backdrop_order();
        let mut done = 0usize;
        timed!("gpu_upload", self.sync_images(gl, list))?;
        // Probe: `CRABSNAP_NO_CULL=1` draws every batch, so an A/B can price the culling.
        let cull = std::env::var_os("CRABSNAP_NO_CULL").is_none();

        for batch in list.plan_batches() {
            timed!(
                "gpu_layers",
                self.sync_layers(gl, list, &frame, &mut chain, &batch.layers)
            )?;
            // A cached subtree that did not change keeps the pixels the frame already has;
            // rebuilding its vertices and submitting them would write nothing new.
            if chain.clean.last() == Some(&true) {
                if chain.band_clear.last() == Some(&true) {
                    self.stats.band_skips += 1;
                } else {
                    self.stats.cached_skips += 1;
                }
                continue;
            }
            // A cached layer is cleared, drawn and stored as a whole: clipping this batch to
            // the frame's band would leave the rest of the layer empty in its cache.
            let band = culling_band(band, &list.layers, &chain.stack);
            while done < backdrops.len() && backdrops[done].0 <= batch.epoch {
                let (_, entry) = backdrops[done];
                done += 1;
                // Run here, before the frost node's own fill: everything the kernel reads
                // was painted by earlier batches, and running later would smear its text.
                if std::env::var("CRABSNAP_NO_BACKDROP").is_err() {
                    let (src, dst) = self.backdrop_pair(frame_tex, frame_fbo, &chain.stack);
                    timed!(
                        "gpu_backdrop",
                        self.apply_backdrop(gl, size, dst, src, entry, band)
                    )?;
                }
            }
            // A batch that misses the band can write no pixel: its scissor would be empty.
            // Skipping it saves the draw call, which the driver prices per call, not per pixel.
            if let Some(b) = band.filter(|_| cull) {
                let s = self.scale.max(f32::EPSILON);
                let area = ui_core::geom::Rect::new(
                    b[0] as f32 / s,
                    b[1] as f32 / s,
                    b[2] as f32 / s,
                    b[3] as f32 / s,
                );
                let hits = batch.bounds.is_some_and(|r| area.intersection(r).is_some());
                if !hits {
                    self.stats.band_culls += 1;
                    continue;
                }
            }
            self.draw_batch(
                gl,
                list,
                &batch,
                Pass { viewport, band },
                &mut rect_data,
                &mut glyph_data,
            )?;
        }

        // A backdrop with no batch after it would otherwise never run: the loop only
        // advances the queue when it sees a later epoch.
        while done < backdrops.len() {
            let (_, entry) = backdrops[done];
            done += 1;
            if std::env::var("CRABSNAP_NO_BACKDROP").is_err() {
                let (src, dst) = self.backdrop_pair(frame_tex, frame_fbo, &chain.stack);
                self.apply_backdrop(gl, size, dst, src, entry, band)?;
            }
        }
        while let Some(index) = chain.stack.pop() {
            let depth = chain.stack.len();
            let was_clean = chain.clean.pop().unwrap_or(false);
            chain.band_clear.pop();
            let entry = list.layers.get(index as usize);
            let effects = entry.map_or(&[][..], |l| &l.effects[..]);
            let cache = entry.and_then(|l| l.cache_key);
            // A layer is composited only into a parent this frame repainted: compositing
            // again over pixels that already show it would blend a translucent layer twice.
            let composite = match chain.clean.last() {
                Some(&parent_clean) => !parent_clean,
                None => frame_cleared_over(band, self.layer_bounds.get(depth).copied()),
            };
            self.end_layer(
                gl,
                depth,
                effects,
                viewport,
                band,
                Finish {
                    cache,
                    clean: was_clean,
                    composite,
                },
            )?;
        }
        self.cached_targets
            .retain(|_, c| c.last_used == self.frame_serial);
        self.evict_images(gl);
        // The target must end up equal to the frame: this frame's band plus whatever it
        // missed on the frames it was not handed. A full band copies everything anyway.
        let copy = band.map(|b| carry.iter().fold(b, |acc, c| union_device(acc, *c, size)));
        let blitted = timed!(
            "gpu_blit",
            self.blit_frame(gl, target, surface, frame_fbo, frame_tex, copy)
        );
        self.scale = scale;
        blitted?;
        Ok(self.stats)
    }

    /// Bind the target that layer `depth` renders into, creating it on demand.
    fn begin_layer(
        &mut self,
        gl: &glow::Context,
        depth: usize,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        while self.layer_targets.len() <= depth {
            self.layer_targets.push(crate::effect_pass::new_target(gl)?);
        }

        let slot = &mut self.layer_targets[depth];
        if slot.width != width || slot.height != height || slot.format != self.target_format {
            crate::effect_pass::resize_target(gl, slot, width, height, self.target_format);
        }

        let fbo = slot.fbo;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, width as i32, height as i32);
            // A layer starts fully transparent; leftovers would leak through.
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::SCISSOR_TEST);
        }
        Ok(())
    }

    /// Composite layer `depth` into its parent with `opacity`.
    /// The device rect this frame redraws, or `None` for the whole surface.
    fn damage_band(
        &self,
        damage: &ui_core::damage::Damage,
        surface: (u32, u32),
        list: &DrawList,
    ) -> Option<[i32; 4]> {
        if std::env::var_os("CRABSNAP_NO_PARTIAL").is_some() {
            return None;
        }
        if self.supersample > 1 || self.hdr.is_some() {
            return None;
        }
        let full = ui_core::geom::Rect::new(
            0.0,
            0.0,
            surface.0 as f32 / self.scale.max(f32::EPSILON),
            surface.1 as f32 / self.scale.max(f32::EPSILON),
        );
        let rects = damage.rects(full);
        let (&first, rest) = rects.split_first()?;
        let bbox = rest.iter().fold(first, |acc, r| acc.union(*r));
        let full_dev = ui_core::geom::Rect::new(0.0, 0.0, surface.0 as f32, surface.1 as f32);
        // A frost re-reads the frame around its box, so a band reaching it must repaint it too;
        // `cover_readers` grows the band over the chain of readers that follows.
        let frosts: Vec<(ui_core::geom::Rect, ui_core::geom::Rect)> = list
            .backdrop_order()
            .iter()
            .filter_map(|(_, e)| {
                let vis = self.backdrop_visible(e)?.intersection(full_dev)?;
                Some((self.backdrop_read(e), vis))
            })
            .collect();
        let effects = self.effect_readers(list);
        let b = cover_readers(device_scissor(bbox, self.scale), &frosts, &effects);
        let area = b[2] as f64 * b[3] as f64;
        let all = surface.0 as f64 * surface.1 as f64;
        // Past this share the band is most of the frame, and a full frame is cheaper.
        (area < all * 0.7).then_some(b)
    }

    /// Device bounds of every layer whose chain reads its neighbours: a change inside one
    /// alters pixels the damage rect never named.
    fn effect_readers(&self, list: &DrawList) -> Vec<ui_core::geom::Rect> {
        let s = self.scale;
        list.layers
            .iter()
            .filter(|l| l.effects.iter().any(|e| !e.kernel.is_identity()))
            .map(|l| {
                ui_core::geom::Rect::new(
                    l.bounds.origin.x * s,
                    l.bounds.origin.y * s,
                    l.bounds.size.w * s,
                    l.bounds.size.h * s,
                )
            })
            .collect()
    }

    /// Hand the persistent frame to the rotating target, applying the output transform.
    ///
    /// Both the static path and the redraw path go through here: skipping it on a static
    /// frame leaves sRGB values in a surface declared as PQ, which reads as washed out.
    fn blit_frame(
        &mut self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
        size: (u32, u32),
        frame_fbo: glow::Framebuffer,
        frame_tex: glow::Texture,
        band: Option<[i32; 4]>,
    ) -> Result<(), GpuError> {
        // Resolve before the output transform: the transform must encode averaged pixels,
        // not one sample per destination pixel taken from the supersampled frame.
        let mut from = frame_fbo;
        let mut source = frame_tex;
        if self.supersample > 1 {
            let scratch = crate::effect_pass::ensure_target(
                &mut self.resolve_scratch,
                gl,
                size.0,
                size.1,
                self.target_format,
            )?;
            self.resolve_frame(gl, scratch.0, source, size, false)?;
            from = scratch.0;
            source = scratch.1;
        }
        if let Some([scale, offset]) = self.hdr {
            let scratch = crate::effect_pass::ensure_target(
                &mut self.output_scratch,
                gl,
                size.0,
                size.1,
                self.target_format,
            )?;
            let p = match self.encode {
                Encode::Srgb => &self.output_srgb,
                Encode::Linear => &self.output_linear,
            };
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(scratch.0));
                gl.viewport(0, 0, size.0 as i32, size.1 as i32);
                gl.disable(glow::BLEND);
                gl.disable(glow::SCISSOR_TEST);
                gl.use_program(Some(p.program));
                if let Some(u) = &p.uniforms.viewport {
                    gl.uniform_2_f32(Some(u), size.0 as f32, size.1 as f32);
                }
                if let Some(u) = &p.uniforms.src {
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(source));
                    gl.uniform_1_i32(Some(u), 0);
                }
                if let Some(u) = &p.uniforms.scale {
                    gl.uniform_1_f32(Some(u), scale);
                }
                if let Some(u) = &p.uniforms.offset {
                    gl.uniform_1_f32(Some(u), offset);
                }
                if let Some(u) = &p.uniforms.gamut {
                    // GLSL reads mat3 column-major, so transpose on the way out.
                    let g = &self.gamut;
                    let cols = [
                        g[0][0], g[1][0], g[2][0], g[0][1], g[1][1], g[2][1], g[0][2], g[1][2],
                        g[2][2],
                    ];
                    gl.uniform_matrix_3_f32_slice(Some(u), false, &cols);
                }
                gl.bind_vertex_array(Some(p.vao));
                let quad: [f32; 4] = [0.0, 0.0, size.0 as f32, size.1 as f32];
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&quad), glow::DYNAMIC_DRAW);
                gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, 1);
                gl.enable(glow::BLEND);
            }
            from = scratch.0;
        }
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            target.bind(gl);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(from));
            let [x, y, w, h] = band.unwrap_or([0, 0, size.0 as i32, size.1 as i32]);
            gl.blit_framebuffer(
                x,
                y,
                x + w,
                y + h,
                x,
                y,
                x + w,
                y + h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        }
        Ok(())
    }

    /// Box-average the supersampled frame into `dest`, one destination pixel per quad pixel.
    fn resolve_frame(
        &self,
        gl: &glow::Context,
        dest: glow::Framebuffer,
        src: glow::Texture,
        size: (u32, u32),
        encode: bool,
    ) -> Result<(), GpuError> {
        let p = &self.resolve;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dest));
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.use_program(Some(p.program));
            if let Some(u) = &p.uniforms.viewport {
                gl.uniform_2_f32(Some(u), size.0 as f32, size.1 as f32);
            }
            if let Some(u) = &p.uniforms.src {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(src));
                gl.uniform_1_i32(Some(u), 0);
            }
            if let Some(u) = &p.uniforms.supersample {
                gl.uniform_1_i32(Some(u), self.supersample as i32);
            }
            if let Some(u) = &p.uniforms.encode {
                gl.uniform_1_i32(Some(u), i32::from(encode));
            }
            gl.bind_vertex_array(Some(p.vao));
            let quad: [f32; 4] = [0.0, 0.0, size.0 as f32, size.1 as f32];
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&quad), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, 1);
            gl.enable(glow::BLEND);
        }
        Ok(())
    }

    fn end_layer(
        &mut self,
        gl: &glow::Context,
        depth: usize,
        effects: &[Effect],
        viewport: (f32, f32),
        band: Option<[i32; 4]>,
        finish: Finish,
    ) -> Result<(), GpuError> {
        let Finish {
            cache,
            clean,
            composite,
        } = finish;
        // Depth 0 lands in the frame, which this frame cleared only inside the band.
        let cleared = if depth == 0 { band_rect(band) } else { None };

        // A cached layer nobody changed still has its last composite, and its depth target may
        // never have been allocated: this runs before the lookup and draws nothing.
        if clean {
            if let (true, Some(key)) = (composite, cache)
                && let Some(cached) = self.cached_targets.get(&key)
            {
                let tex = cached.target.tex;
                self.composite_layer(gl, depth, tex, viewport, cleared);
            } else if !composite {
                self.bind_parent(gl, depth, viewport);
            }
            return Ok(());
        }

        let Some(slot) = self.layer_targets.get(depth) else {
            return Ok(());
        };
        let (layer_w, layer_h, layer_tex) = (slot.width, slot.height, slot.tex);

        // Effects and compositing both stop at the layer's bounds; the rest is transparent.
        let full = ui_core::geom::Rect::new(0.0, 0.0, viewport.0, viewport.1);
        let bounds = self.layer_bounds.get(depth).copied().unwrap_or(full);

        // The layer was painted in full this frame, so its effects are recomputed in full:
        // nothing here may depend on what a previous frame left in these textures.
        if self.effect_scratch.len() <= depth {
            self.effect_scratch.resize_with(depth + 1, Default::default);
        }
        // Kernels are authored per logical dp; the layer grid is `scale` times finer.
        let scaled: Vec<Effect> = effects
            .iter()
            .cloned()
            .map(|e| ui_core::effect::on_grid(e, self.scale))
            .collect();
        let tex = crate::effect_pass::run(
            &mut self.effect_scratch[depth],
            gl,
            &self.effects,
            &self.boxpass,
            layer_tex,
            layer_w,
            layer_h,
            bounds,
            self.target_format,
            &scaled,
            false,
        )?;

        // Keep the finished pixels for the frames this subtree does not change in.
        if let Some(key) = cache {
            self.store_cache(gl, key, tex, viewport)?;
        }

        if composite {
            self.composite_layer(gl, depth, tex, viewport, cleared);
        } else {
            self.bind_parent(gl, depth, viewport);
        }
        Ok(())
    }

    /// Bind the target a layer at `depth` composites into: the frame at depth 0, else its
    /// outer layer.
    fn bind_parent(&self, gl: &glow::Context, depth: usize, viewport: (f32, f32)) {
        if depth == 0 {
            if let Some(root) = self.root_fbo {
                unsafe {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(root));
                    gl.viewport(0, 0, viewport.0 as i32, viewport.1 as i32);
                }
            }
        } else if let Some(parent) = self.layer_targets.get(depth - 1) {
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(parent.fbo));
                gl.viewport(0, 0, parent.width as i32, parent.height as i32);
            }
        }
    }

    /// Composite layer `depth` from `tex` into its parent, inside `cleared` (the region the
    /// parent repainted; `None` = all of it), clipped and rounded to the layer.
    fn composite_layer(
        &mut self,
        gl: &glow::Context,
        depth: usize,
        tex: glow::Texture,
        viewport: (f32, f32),
        cleared: Option<ui_core::geom::Rect>,
    ) {
        let full = ui_core::geom::Rect::new(0.0, 0.0, viewport.0, viewport.1);
        let bounds = self.layer_bounds.get(depth).copied().unwrap_or(full);
        self.bind_parent(gl, depth, viewport);
        let cut = bounds.intersection(full).and_then(|b| match cleared {
            Some(c) => b.intersection(c),
            None => Some(b),
        });
        let Some(cut) = cut else {
            return;
        };
        let scissor = device_scissor(cut, 1.0);
        let corners = self
            .layer_radius
            .get(depth)
            .copied()
            .unwrap_or(ui_core::geom::Corners::all(0.0));
        unsafe {
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
        }
        self.draw_layer_quad(
            gl,
            tex,
            viewport,
            [
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.w,
                bounds.size.h,
            ],
            corners,
        );
        self.stats.draw_calls += 1;
    }

    /// Draw `tex` as one quad with the layer program: a layer composited into its parent, and
    /// the copy that keeps a cached layer's pixels.
    fn draw_layer_quad(
        &self,
        gl: &glow::Context,
        tex: glow::Texture,
        viewport: (f32, f32),
        rect: [f32; 4],
        corners: ui_core::geom::Corners,
    ) {
        unsafe {
            let p = &self.layers;
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            if let Some(u) = p.uniforms.opacity {
                gl.uniform_1_f32(Some(&u), 1.0);
            }
            // Rounded corners are a property of the layer, not of this pass.
            if let Some(u) = &p.uniforms.layer_rect {
                gl.uniform_4_f32(Some(u), rect[0], rect[1], rect[2], rect[3]);
            }
            if let Some(u) = &p.uniforms.layer_radius {
                gl.uniform_4_f32(Some(u), corners.tl, corners.tr, corners.br, corners.bl);
            }
            if let Some(u) = &p.uniforms.layer_exp {
                gl.uniform_1_f32(Some(u), corners.shape.exponent());
            }
            if let Some(u) = &p.uniforms.layer_skip_empty {
                gl.uniform_1_f32(Some(u), 0.0);
            }
            if let Some(u) = p.uniforms.layer_tex {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.uniform_1_i32(Some(&u), 0);
            }
            let data = [0.0f32, 0.0, viewport.0, viewport.1];
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, shader::UNIT_QUAD.len() as i32 / 2, 1);
            gl.enable(glow::SCISSOR_TEST);
        }
    }

    /// Copy a finished layer into its cache target, unmasked: the mask is the layer's, and it
    /// is reapplied whenever the cache is composited.
    fn store_cache(
        &mut self,
        gl: &glow::Context,
        key: u64,
        tex: glow::Texture,
        viewport: (f32, f32),
    ) -> Result<(), GpuError> {
        let (w, h) = (viewport.0 as u32, viewport.1 as u32);
        if !self.cached_targets.contains_key(&key) {
            let target = crate::effect_pass::new_target(gl)?;
            self.cached_targets.insert(
                key,
                CachedTarget {
                    target,
                    valid: false,
                    last_used: self.frame_serial,
                },
            );
        }
        let Some(slot) = self.cached_targets.get_mut(&key) else {
            return Ok(());
        };
        slot.last_used = self.frame_serial;
        if slot.target.width != w
            || slot.target.height != h
            || slot.target.format != self.target_format
        {
            crate::effect_pass::resize_target(gl, &mut slot.target, w, h, self.target_format);
        }
        slot.valid = true;
        let fbo = slot.target.fbo;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, w as i32, h as i32);
            gl.disable(glow::SCISSOR_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.draw_layer_quad(
            gl,
            tex,
            viewport,
            [0.0, 0.0, viewport.0, viewport.1],
            ui_core::geom::Corners::all(0.0),
        );
        self.stats.draw_calls += 1;
        Ok(())
    }

    /// Blur what is already painted inside `entry`, in place on the frame target.
    ///
    /// The filter reads the live frame and runs over all of it: everything the kernel needs
    /// was painted by earlier batches this frame, so no tap can reach a pixel from an earlier
    /// frame, and edge taps meet real neighbours instead of a cropped copy's transparent
    /// border. The result is written back over the node's own rect with corner coverage,
    /// which is what keeps the frost's rounded shape.
    /// The texture a backdrop reads and the target it lands in, for the layer in force.
    fn backdrop_pair(
        &self,
        frame_tex: glow::Texture,
        frame_fbo: glow::Framebuffer,
        stack: &[u32],
    ) -> (glow::Texture, glow::Framebuffer) {
        match stack.len() {
            0 => (frame_tex, frame_fbo),
            n => {
                let src = if n >= 2 {
                    self.layer_targets[n - 2].tex
                } else {
                    frame_tex
                };
                (src, self.layer_targets[n - 1].fbo)
            }
        }
    }

    /// A dp rect on the render grid.
    fn on_device(&self, r: ui_core::geom::Rect) -> ui_core::geom::Rect {
        let s = self.scale;
        ui_core::geom::Rect::new(r.origin.x * s, r.origin.y * s, r.size.w * s, r.size.h * s)
    }

    /// The device area a backdrop may write: its box narrowed by the node's own clip.
    fn backdrop_visible(
        &self,
        entry: &ui_core::draw::BackdropEntry,
    ) -> Option<ui_core::geom::Rect> {
        self.on_device(entry.rect)
            .intersection(self.on_device(entry.clip))
    }

    /// The device box a backdrop reads: its own rect grown by the kernel's reach.
    fn backdrop_read(&self, entry: &ui_core::draw::BackdropEntry) -> ui_core::geom::Rect {
        let s = self.scale;
        let device = self.on_device(entry.rect);
        let effect =
            ui_core::effect::on_grid(ui_core::effect::Effect::box_blur(entry.sigma), self.scale);
        // The kit lens shows a margin beyond its box; a plain blur reads only its kernel.
        let margin = match resolve(entry.material, self.material) {
            Some(ui_core::draw::BackdropMaterial::LiquidGlass(k)) => {
                (k.capture_size - 1.0).max(0.0) * 0.5 * device.size.w.max(device.size.h)
            }
            _ => 0.0,
        };
        let reach = effect.kernel.radius as f32 * 3.0 + margin + 2.0 * s;
        ui_core::geom::Rect::new(
            device.origin.x - reach,
            device.origin.y - reach,
            device.size.w + reach * 2.0,
            device.size.h + reach * 2.0,
        )
    }

    fn apply_backdrop(
        &mut self,
        gl: &glow::Context,
        size: (u32, u32),
        dst_fbo: glow::Framebuffer,
        src_tex: glow::Texture,
        entry: ui_core::draw::BackdropEntry,
        band: Option<[i32; 4]>,
    ) -> Result<(), GpuError> {
        let full = ui_core::geom::Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32);
        let device = self.on_device(entry.rect);
        // Only the region this frame cleared may be composited: blending over the frost's
        // own output would apply it twice, and `damage_band` already grew the band over
        // every frost it reaches, so a frost outside it has nothing to redraw.
        let Some(clip) = self
            .backdrop_visible(&entry)
            .and_then(|r| r.intersection(full))
        else {
            return Ok(());
        };
        let Some(clip) = (match band_rect(band) {
            Some(b) => clip.intersection(b),
            None => Some(clip),
        }) else {
            return Ok(());
        };
        let scissor = device_scissor(clip, 1.0);
        if scissor[2] <= 0 || scissor[3] <= 0 {
            return Ok(());
        }

        let material = resolve(entry.material, self.material);
        // The kit lens carries its own mip chain on its capture, so no material reads this
        // frame below level 0 any more; every one of them samples it at level 0.
        // Sigma is a visual radius: on the finer grid it must widen with the whole grid,
        // which is the display scale times the supersample.
        let effect =
            ui_core::effect::on_grid(ui_core::effect::Effect::box_blur(entry.sigma), self.scale);
        let bounds = self
            .backdrop_read(&entry)
            .intersection(full)
            .unwrap_or(ui_core::geom::Rect::ZERO);
        let filtered = timed!(
            "gpu_blur",
            crate::effect_pass::run(
                &mut self.backdrop_scratch,
                gl,
                &self.effects,
                &self.boxpass,
                src_tex,
                size.0,
                size.1,
                bounds,
                self.target_format,
                &[effect],
                false,
            )
        )?;
        if !composites(material, filtered != src_tex) {
            return Ok(());
        }
        self.stats.frosts += 1;
        timed!(
            "gpu_composite",
            self.composite_backdrop(gl, size, dst_fbo, device, scissor, entry, filtered)
        )
    }

    /// Resample the region a lens reads into a texture of its own: `size` times the node's
    /// area at `scale` texels per dp. The shader addresses it by position, not by stretching
    /// the node over it, so the margin is headroom the rim may read — not a zoom.
    /// The capture target for exactly `width`×`height`, reusing one of this size.
    fn capture_target(
        &mut self,
        gl: &glow::Context,
        width: u32,
        height: u32,
    ) -> Result<(glow::Framebuffer, glow::Texture), GpuError> {
        let (format, serial) = (self.target_format, self.frame_serial);
        if let Some(slot) = self.capture_targets.iter_mut().find(|slot| {
            slot.target.width == width
                && slot.target.height == height
                && slot.target.format == format
        }) {
            slot.last_used = serial;
            return Ok((slot.target.fbo, slot.target.tex));
        }
        let target = if self.capture_targets.len() < CAPTURE_TARGETS {
            self.capture_targets.push(CaptureTarget {
                target: crate::effect_pass::new_target(gl)?,
                last_used: serial,
            });
            let slot = self.capture_targets.last_mut().expect("just pushed");
            &mut slot.target
        } else {
            let slot = self
                .capture_targets
                .iter_mut()
                .min_by_key(|slot| slot.last_used)
                .expect("the pool is at its cap");
            slot.last_used = serial;
            &mut slot.target
        };
        crate::effect_pass::resize_target(gl, target, width, height, format);
        Ok((target.fbo, target.tex))
    }

    fn lens_capture(
        &mut self,
        gl: &glow::Context,
        src: glow::Texture,
        viewport: (u32, u32),
        device: ui_core::geom::Rect,
        size: f32,
        scale: f32,
    ) -> Result<(glow::Texture, ui_core::geom::Rect), GpuError> {
        let size = size.clamp(0.1, 4.0);
        let scale = scale.clamp(0.02, 4.0);
        let region = ui_core::geom::Rect::new(
            device.origin.x + device.size.w * (1.0 - size) * 0.5,
            device.origin.y + device.size.h * (1.0 - size) * 0.5,
            device.size.w * size,
            device.size.h * size,
        );
        let w = (region.size.w * scale).round().max(1.0) as u32;
        let h = (region.size.h * scale).round().max(1.0) as u32;
        let (fbo, tex) = timed!("gpu_lens_alloc", self.capture_target(gl, w, h))?;
        let p = &self.capture;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, w as i32, h as i32);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.use_program(Some(p.program));
            if let Some(u) = &p.uniforms.viewport {
                gl.uniform_2_f32(Some(u), w as f32, h as f32);
            }
            if let Some(u) = &p.uniforms.src {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(src));
                gl.uniform_1_i32(Some(u), 0);
            }
            if let Some(u) = &p.uniforms.region {
                gl.uniform_4_f32(
                    Some(u),
                    region.origin.x / viewport.0 as f32,
                    region.origin.y / viewport.1 as f32,
                    region.size.w / viewport.0 as f32,
                    region.size.h / viewport.1 as f32,
                );
            }
            gl.bind_vertex_array(Some(p.vao));
            let quad: [f32; 4] = [0.0, 0.0, w as f32, h as f32];
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&quad), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, 1);
            // The rim compresses many capture texels into one pixel, so the capture carries a
            // chain: the face still reads level 0, where the tap is already exact.
            // The chain is built with the capture's framebuffer released: GL leaves the result
            // undefined while the texture is still attached to the bound draw framebuffer.
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            timed!("gpu_lens_mip", {
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR_MIPMAP_LINEAR as i32,
                );
                gl.generate_mipmap(glow::TEXTURE_2D);
            });
        }
        Ok((tex, region))
    }

    /// Write the filtered backdrop back over the node's rect, shaded as the style asks.
    fn composite_backdrop(
        &mut self,
        gl: &glow::Context,
        size: (u32, u32),
        dst_fbo: glow::Framebuffer,
        device: ui_core::geom::Rect,
        scissor: [i32; 4],
        entry: ui_core::draw::BackdropEntry,
        filtered: glow::Texture,
    ) -> Result<(), GpuError> {
        let s = self.scale;
        // A node may name its own material; the renderer-wide one is the default for nodes
        // that only asked for a blur.
        let material = resolve(entry.material, self.material);
        let shade = Shade::of(material);
        // Without a material this is the plain blit: one texture read with corner coverage.
        // The glass reads its own capture, never the live frame: the region it shows,
        // rendered at the density it shows it.
        let lens = match material {
            Some(BackdropMaterial::LiquidGlass(k)) => Some(timed!(
                "gpu_lens",
                self.lens_capture(gl, filtered, size, device, k.capture_size, k.capture_scale,)
            )?),
            _ => None,
        };
        let p = match (shade, self.encode) {
            (Shade::Plain, _) => &self.layers,
            (Shade::Acrylic, Encode::Srgb) => &self.acrylic,
            (Shade::Acrylic, Encode::Linear) => &self.acrylic_linear,
            (Shade::Liquid, Encode::Srgb) => &self.glass_kit,
            (Shade::Liquid, Encode::Linear) => &self.glass_kit_linear,
        };
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dst_fbo));
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
            // Premultiplied blend against the frame: corner coverage is below 1 there, and
            // overwriting would replace opaque pixels with partially transparent ones.
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), size.0 as f32, size.1 as f32);
            }
            if let Some(u) = p.uniforms.opacity {
                gl.uniform_1_f32(Some(&u), 1.0);
            }
            if let Some(u) = &p.uniforms.layer_rect {
                gl.uniform_4_f32(
                    Some(u),
                    device.origin.x,
                    device.origin.y,
                    device.size.w,
                    device.size.h,
                );
            }
            if let Some(u) = &p.uniforms.layer_radius {
                gl.uniform_4_f32(
                    Some(u),
                    entry.corners.tl * s,
                    entry.corners.tr * s,
                    entry.corners.br * s,
                    entry.corners.bl * s,
                );
            }
            if let Some(u) = &p.uniforms.layer_exp {
                gl.uniform_1_f32(Some(u), entry.corners.shape.exponent());
            }
            if let Some(u) = &p.uniforms.layer_skip_empty {
                gl.uniform_1_f32(Some(u), 0.0);
            }
            if let Some(u) = p.uniforms.layer_tex {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(filtered));
                gl.uniform_1_i32(Some(&u), 0);
            }
            if let Some(u) = p.uniforms.src {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(filtered));
                gl.uniform_1_i32(Some(&u), 0);
            }
            match material {
                None | Some(BackdropMaterial::Plain) => {}
                Some(BackdropMaterial::Acrylic(a)) => {
                    if let Some(u) = p.uniforms.tint {
                        gl.uniform_4_f32(Some(&u), a.tint.r, a.tint.g, a.tint.b, a.tint.a);
                    }
                    if let Some(u) = p.uniforms.luminosity {
                        gl.uniform_4_f32(
                            Some(&u),
                            a.luminosity.r,
                            a.luminosity.g,
                            a.luminosity.b,
                            a.luminosity.a,
                        );
                    }
                    if let Some(u) = p.uniforms.noise {
                        gl.uniform_1_f32(Some(&u), a.noise);
                    }
                }
                Some(BackdropMaterial::LiquidGlass(k)) => {
                    if let Some(u) = p.uniforms.body {
                        gl.uniform_4_f32(
                            Some(&u),
                            k.body.mix,
                            k.body.pole,
                            k.body.chroma,
                            k.body.tone,
                        );
                    }
                    if let Some(u) = p.uniforms.glass {
                        gl.uniform_4_f32(Some(&u), k.tint.r, k.tint.g, k.tint.b, k.tint.a);
                    }
                    // The glass measures its thickness in dp, as the reference does.
                    if let Some(u) = p.uniforms.optical {
                        gl.uniform_4_f32(
                            Some(&u),
                            k.refractive_index,
                            k.dispersion,
                            k.thickness,
                            k.magnification,
                        );
                    }
                    if let Some(u) = p.uniforms.light {
                        gl.uniform_4_f32(
                            Some(&u),
                            k.glare_intensity,
                            k.glare_distance,
                            k.glare_sharpness,
                            k.glare_offset,
                        );
                    }
                    if let Some(u) = p.uniforms.kit {
                        gl.uniform_4_f32(
                            Some(&u),
                            k.fresnel_intensity,
                            k.fresnel_distance,
                            k.fresnel_sharpness,
                            k.glare_convergence,
                        );
                    }
                    if let Some(u) = p.uniforms.blend {
                        gl.uniform_4_f32(
                            Some(&u),
                            k.blend_count as f32,
                            k.blend_k * s,
                            k.glare_bias,
                            s,
                        );
                    }
                    if let Some((tex, rect)) = lens {
                        if let Some(u) = p.uniforms.src {
                            gl.active_texture(glow::TEXTURE0);
                            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                            gl.uniform_1_i32(Some(&u), 0);
                        }
                        if let Some(u) = p.uniforms.capture {
                            gl.uniform_4_f32(
                                Some(&u),
                                rect.origin.x,
                                rect.origin.y,
                                rect.size.w,
                                rect.size.h,
                            );
                        }
                    }
                    upload_shapes(gl, p, &k.blend, k.blend_count, device, s);
                }
            }
            // `a_rect.y` is the top in device space, and the quad runs down from there, so
            // the instance must cover the whole frame; coverage() then cuts the node's shape.
            let data = [0.0f32, 0.0, size.0 as f32, size.1 as f32];
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(&data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                crate::shader::UNIT_QUAD.len() as i32 / 2,
                1,
            );
        }
        self.stats.draw_calls += 1;
        if material.is_some() {
            self.stats.material_passes += 1;
        }
        Ok(())
    }

    /// Pop and push layers so the active chain equals `want`.
    fn sync_layers(
        &mut self,
        gl: &glow::Context,
        list: &DrawList,
        frame: &FrameTargets<'_>,
        chain: &mut Chain,
        want: &[u32],
    ) -> Result<(), GpuError> {
        let viewport = frame.viewport;
        let stack = &mut chain.stack;
        let mut common = 0;
        while common < stack.len() && common < want.len() && stack[common] == want[common] {
            common += 1;
        }
        while stack.len() > common {
            let index = stack.pop().expect("non-empty");
            let depth = stack.len();
            let was_clean = chain.clean.pop().unwrap_or(false);
            chain.band_clear.pop();
            let entry = list.layers.get(index as usize);
            let effects = entry.map_or(&[][..], |l| &l.effects[..]);
            let cache = entry.and_then(|l| l.cache_key);
            let composite = match chain.clean.last() {
                Some(&parent_clean) => !parent_clean,
                None => frame_cleared_over(frame.band, self.layer_bounds.get(depth).copied()),
            };
            self.end_layer(
                gl,
                depth,
                effects,
                frame.viewport,
                frame.band,
                Finish {
                    cache,
                    clean: was_clean,
                    composite,
                },
            )?;
        }
        for &index in &want[common..] {
            let depth = stack.len();
            let (w, h) = (viewport.0 as u32, viewport.1 as u32);
            let entry = list.layers.get(index as usize);
            let cache = entry.and_then(|l| l.cache_key);
            let bounds = entry.map_or(ui_core::geom::Rect::new(0.0, 0.0, 0.0, 0.0), |l| l.bounds);
            let corners = entry.map_or(ui_core::geom::Corners::all(0.0), |l| l.radius);
            let s = self.scale;
            // Record the layer's device-space bounds: effects and compositing stop there.
            let device = ui_core::geom::Rect::new(
                bounds.origin.x * s,
                bounds.origin.y * s,
                bounds.size.w * s,
                bounds.size.h * s,
            );
            if self.layer_bounds.len() <= depth {
                self.layer_bounds
                    .resize(depth + 1, ui_core::geom::Rect::new(0.0, 0.0, 0.0, 0.0));
            }
            self.layer_bounds[depth] = device;
            if self.layer_radius.len() <= depth {
                self.layer_radius
                    .resize(depth + 1, ui_core::geom::Corners::all(0.0));
            }
            self.layer_radius[depth] = ui_core::geom::Corners {
                tl: corners.tl * s,
                tr: corners.tr * s,
                br: corners.br * s,
                bl: corners.bl * s,
                ..corners
            }
            // A layer is clipped to its own bounds, so the same half-side limit applies.
            .clamp_to(device.size);

            // A layer beneath a skipped one is skipped too: its parent holds no pixels this
            // frame, so redrawing it would only land in a target nobody composites.
            let parent_clean = chain.clean.last().copied().unwrap_or(false);
            // Asked unconditionally: the lookup also marks the target as used this frame, and
            // a skipped one would otherwise be freed while its canvas is still in the tree.
            let own_clean =
                cache.is_some_and(|key| self.cached_is_clean(gl, key, frame.damage, device, w, h));
            // A layer the band stays clear of has no pixel changed, so neither drawing it nor
            // compositing it could alter what the frame shows: keep its pixels as they are.
            let band_clear = chain.band_clear.last().copied().unwrap_or(false)
                || (cache.is_none() && band_clear_of(frame.band, device));
            let clean = parent_clean || own_clean || band_clear;
            if !clean {
                self.begin_layer(gl, depth, w, h)?;
            }
            chain.clean.push(clean);
            chain.band_clear.push(band_clear);
            stack.push(index);
        }
        Ok(())
    }

    /// Whether the cached target under `key` still holds the pixels `bounds` should show:
    /// it exists, was finished, and no damage landed inside it.
    fn cached_is_clean(
        &mut self,
        gl: &glow::Context,
        key: u64,
        damage: &ui_core::damage::Damage,
        bounds: ui_core::geom::Rect,
        width: u32,
        height: u32,
    ) -> bool {
        if !self.cached_targets.contains_key(&key) {
            let Ok(target) = crate::effect_pass::new_target(gl) else {
                return false;
            };
            self.cached_targets.insert(
                key,
                CachedTarget {
                    target,
                    valid: false,
                    last_used: self.frame_serial,
                },
            );
        }
        let Some(slot) = self.cached_targets.get_mut(&key) else {
            return false;
        };
        slot.last_used = self.frame_serial;
        if slot.target.width != width
            || slot.target.height != height
            || slot.target.format != self.target_format
        {
            crate::effect_pass::resize_target(
                gl,
                &mut slot.target,
                width,
                height,
                self.target_format,
            );
            slot.valid = false;
        }
        if !slot.valid {
            return false;
        }
        !damage_reaches(damage, bounds, self.scale)
    }

    fn draw_rects(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.rects;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::RECT_STRIDE as usize) as i32,
            );
        }
        self.stats.rects += (data.len() / shader::RECT_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_sectors(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.sectors;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::SECTOR_STRIDE as usize) as i32,
            );
        }
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_ellipses(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.ellipses;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::ELLIPSE_STRIDE as usize) as i32,
            );
        }
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_lines(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.lines;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::LINE_STRIDE as usize) as i32,
            );
        }
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_glyphs(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let Some(atlas_tex) = self.atlas_tex else {
            return Ok(());
        };
        let p = &self.glyphs;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            if let Some(u) = p.uniforms.atlas_size {
                gl.uniform_2_f32(Some(&u), self.atlas_size.0 as f32, self.atlas_size.1 as f32);
            }
            if let Some(u) = p.uniforms.color_atlas_size {
                gl.uniform_2_f32(
                    Some(&u),
                    self.color_atlas_size.0 as f32,
                    self.color_atlas_size.1 as f32,
                );
            }
            if let Some(u) = p.uniforms.atlas {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(atlas_tex));
                gl.uniform_1_i32(Some(&u), 0);
            }
            if let Some(u) = p.uniforms.color_atlas {
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(
                    glow::TEXTURE_2D,
                    Some(self.color_atlas_tex.unwrap_or(atlas_tex)),
                );
                gl.uniform_1_i32(Some(&u), 1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::GLYPH_STRIDE as usize) as i32,
            );
        }
        self.stats.glyphs += (data.len() / shader::GLYPH_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    /// Draw one batch's primitives into the bound target.
    ///
    /// The frame and a canvas draw the same kinds; only the target, viewport and band differ.
    fn draw_batch(
        &mut self,
        gl: &glow::Context,
        list: &DrawList,
        batch: &ui_core::draw::Batch,
        pass: Pass,
        rect_data: &mut Vec<f32>,
        glyph_data: &mut Vec<f32>,
    ) -> Result<(), GpuError> {
        let Pass { viewport, band } = pass;
        match batch.kind {
            PrimKind::Shadow => {
                // One analytic draw for the whole run: no pass, no temporary texture.
                let mut data: Vec<f32> = Vec::new();
                for i in batch.range.start..batch.range.end {
                    if let Some(p) = list.prims.shadows.get(i as usize) {
                        push_shadow(&mut data, p, self.scale, self.encode);
                    }
                }
                if !data.is_empty() {
                    set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport, band);
                    self.draw_shadows(gl, viewport, &data, batch.clip)?;
                }
            }
            PrimKind::Image => {
                // Consecutive primitives on one texture share a draw; a batch may mix them.
                let mut start = batch.range.start;
                while start < batch.range.end {
                    let Some(first) = list.prims.images.get(start as usize) else {
                        return Err(GpuError::from(format!(
                            "image batch names prim {start} of {}",
                            list.prims.images.len()
                        )));
                    };
                    let mut end = start + 1;
                    while end < batch.range.end
                        && list
                            .prims
                            .images
                            .get(end as usize)
                            .is_some_and(|p| p.texture == first.texture)
                    {
                        end += 1;
                    }
                    let Some(img) = self.frame_images.get(first.texture as usize).copied() else {
                        return Err(GpuError::from(format!(
                            "image prim {start} samples texture {} of {}",
                            first.texture,
                            self.frame_images.len()
                        )));
                    };
                    let mut data: Vec<f32> = Vec::new();
                    for i in start..end {
                        if let Some(p) = list.prims.images.get(i as usize) {
                            push_image(&mut data, p, self.scale, (img.width, img.height));
                        }
                    }
                    if !data.is_empty() {
                        set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport, band);
                        self.draw_images(gl, viewport, &data, img.tex, img.sampling, batch.clip)?;
                    }
                    start = end;
                }
            }
            PrimKind::Rect => {
                for i in batch.range.start..batch.range.end {
                    if let Some(p) = list.prims.rects.get(i as usize) {
                        push_rect(rect_data, p, self.scale, self.encode);
                    }
                }
                if !rect_data.is_empty() {
                    set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport, band);
                    self.draw_rects(gl, viewport, rect_data, batch.clip)?;
                    rect_data.clear();
                }
            }
            PrimKind::Sector => {
                let mut data: Vec<f32> = Vec::new();
                for i in batch.range.start..batch.range.end {
                    if let Some(p) = list.prims.sectors.get(i as usize) {
                        push_sector(&mut data, p, self.scale, self.encode);
                    }
                }
                if !data.is_empty() {
                    set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport, band);
                    self.draw_sectors(gl, viewport, &data, batch.clip)?;
                }
            }
            PrimKind::Ellipse => {
                let mut data: Vec<f32> = Vec::new();
                for i in batch.range.start..batch.range.end {
                    if let Some(p) = list.prims.ellipses.get(i as usize) {
                        push_ellipse(&mut data, p, self.scale, self.encode);
                    }
                }
                if !data.is_empty() {
                    set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport, band);
                    self.draw_ellipses(gl, viewport, &data, batch.clip)?;
                }
            }
            PrimKind::Line => {
                let mut data: Vec<f32> = Vec::new();
                for i in batch.range.start..batch.range.end {
                    if let Some(p) = list.prims.lines.get(i as usize) {
                        push_line(&mut data, p, self.scale, self.encode);
                    }
                }
                if !data.is_empty() {
                    set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport, band);
                    self.draw_lines(gl, viewport, &data, batch.clip)?;
                }
            }
            PrimKind::Glyph => {
                // A glyph wholly outside the batch's clip would only be scissored away;
                // dropping it here leaves a long line in a narrow field as layout only.
                let scissor = batch.clip.map(|c| device_scissor(c.rect, self.scale));
                for i in batch.range.start..batch.range.end {
                    if let Some(p) = list.prims.glyphs.get(i as usize) {
                        if scissor.is_some_and(|s| !glyph_hits_scissor(p, self.scale, s)) {
                            continue;
                        }
                        push_glyph(glyph_data, p, self.scale, self.encode);
                    }
                }
                if !glyph_data.is_empty() {
                    set_scissor(gl, batch.clip.map(|c| c.rect), self.scale, viewport, band);
                    self.draw_glyphs(gl, viewport, glyph_data, batch.clip)?;
                    glyph_data.clear();
                }
            }
            // Raw text and formulas without a pre-rasterized atlas have no GPU path yet.
            PrimKind::Text | PrimKind::Math => {}
        }
        Ok(())
    }

    /// Paint every changed canvas into its own target; resolve the unchanged ones.
    ///
    /// Nothing here sees the surface: a canvas is drawn at scale 1 into a target the size of
    /// its pixels, and the frame that samples it is what applies zoom.
    fn paint_canvases(
        &mut self,
        gl: &glow::Context,
        canvases: &[CanvasUpload<'_>],
    ) -> Result<(), GpuError> {
        self.canvas_images.clear();
        let saved_scale = self.scale;
        let saved_root = self.root_fbo;
        let saved_bounds = std::mem::take(&mut self.layer_bounds);
        let saved_radius = std::mem::take(&mut self.layer_radius);
        let result = self.paint_canvases_inner(gl, canvases);
        self.scale = saved_scale;
        self.root_fbo = saved_root;
        self.layer_bounds = saved_bounds;
        self.layer_radius = saved_radius;
        result?;
        self.canvas_targets
            .retain(|_, target| target.last_used == self.frame_serial);
        Ok(())
    }

    fn paint_canvases_inner(
        &mut self,
        gl: &glow::Context,
        canvases: &[CanvasUpload<'_>],
    ) -> Result<(), GpuError> {
        for canvas in canvases {
            let mut slot = self.canvas_targets.remove(&canvas.id);
            let painted = match canvas.list {
                Some(list) => {
                    let size = (canvas.size.0.max(1), canvas.size.1.max(1));
                    let mut target = slot.take().unwrap_or(CanvasTarget {
                        target: None,
                        last_used: 0,
                    });
                    target.last_used = self.frame_serial;
                    let (fbo, tex) = crate::effect_pass::ensure_target(
                        &mut target.target,
                        gl,
                        size.0,
                        size.1,
                        self.target_format,
                    )?;
                    unsafe {
                        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                        gl.viewport(0, 0, size.0 as i32, size.1 as i32);
                        gl.disable(glow::SCISSOR_TEST);
                        gl.clear_color(0.0, 0.0, 0.0, 0.0);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                        gl.enable(glow::BLEND);
                        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                        gl.disable(glow::DEPTH_TEST);
                    }
                    self.scale = 1.0;
                    self.root_fbo = Some(fbo);
                    self.paint_list(gl, list, (size.0 as f32, size.1 as f32))?;
                    self.canvas_targets.insert(canvas.id, target);
                    FrameImage {
                        tex,
                        width: size.0,
                        height: size.1,
                        sampling: None,
                    }
                }
                None => {
                    let Some(mut target) = slot.take() else {
                        return Err(GpuError::from(format!(
                            "canvas {:?} is sampled but was never painted",
                            canvas.id
                        )));
                    };
                    let Some(layer) = target.target.as_ref() else {
                        return Err(GpuError::from(format!(
                            "canvas {:?} is sampled but holds no target",
                            canvas.id
                        )));
                    };
                    let image = FrameImage {
                        tex: layer.tex,
                        width: layer.width,
                        height: layer.height,
                        sampling: None,
                    };
                    // Sampling counts as use: the end of this pass frees what the frame did
                    // not touch, and an unchanged canvas is touched exactly here.
                    target.last_used = self.frame_serial;
                    self.canvas_targets.insert(canvas.id, target);
                    image
                }
            };
            self.canvas_images.insert(canvas.id, painted);
        }
        Ok(())
    }

    /// Draw a canvas list into the target bound as the root.
    fn paint_list(
        &mut self,
        gl: &glow::Context,
        list: &DrawList,
        viewport: (f32, f32),
    ) -> Result<(), GpuError> {
        if !list.layers.is_empty() {
            return Err(GpuError::from(String::from(
                "a canvas list holds a layer effect, which has no canvas pass",
            )));
        }
        self.sync_images(gl, list)?;
        let mut rect_data: Vec<f32> = Vec::new();
        let mut glyph_data: Vec<f32> = Vec::new();
        for batch in list.plan_batches() {
            if !batch.layers.is_empty() {
                return Err(GpuError::from(String::from(
                    "a canvas list holds a layer effect, which has no canvas pass",
                )));
            }
            let pass = Pass {
                viewport,
                band: None,
            };
            self.draw_batch(gl, list, &batch, pass, &mut rect_data, &mut glyph_data)?;
        }
        Ok(())
    }

    /// Upload the frame's textures, reusing one GL texture per picture and sampling.
    fn sync_images(&mut self, gl: &glow::Context, list: &DrawList) -> Result<(), GpuError> {
        self.frame_images.clear();
        for entry in &list.images {
            // A canvas is already a texture: the canvas pass rendered it, and its sampling
            // is a property of this draw rather than of an upload, so it has no cache key.
            if let ui_core::draw::ImageSource::Canvas(id) = &entry.source {
                let img = self.canvas_images.get(id).copied().ok_or_else(|| {
                    GpuError::from(format!(
                        "canvas {id:?} is drawn but has no target this frame"
                    ))
                })?;
                self.frame_images.push(FrameImage {
                    sampling: Some(entry.sampling),
                    ..img
                });
                continue;
            }
            self.frame_keys.insert((entry.id, entry.sampling));
            let key = (entry.id, entry.sampling);
            let cached = self.image_textures.get(&key);
            let identity = cached.map(|c| ImageIdentity {
                width: c.width,
                height: c.height,
                data: c.data.as_ptr() as usize,
                colour: c.colour,
                destination: c.destination,
            });
            let existing = cached.map(|c| c.tex);
            let img = match existing {
                Some(tex) if !needs_upload(identity, entry, self.destination) => FrameImage {
                    tex,
                    width: entry.width,
                    height: entry.height,
                    sampling: None,
                },
                other => {
                    let tex = match other {
                        Some(tex) => tex,
                        None => unsafe { gl.create_texture()? },
                    };
                    upload_image(gl, tex, entry, self.destination)?;
                    // A canvas entry above never reaches this branch, so the picture here
                    // always has pixels.
                    let data = entry.pixels().cloned().unwrap_or_default();
                    self.image_textures.insert(
                        key,
                        ImageTexture {
                            tex,
                            width: entry.width,
                            height: entry.height,
                            data,
                            colour: entry.colour,
                            destination: self.destination,
                        },
                    );
                    self.stats.image_uploads += 1;
                    FrameImage {
                        tex,
                        width: entry.width,
                        height: entry.height,
                        sampling: None,
                    }
                }
            };
            self.frame_images.push(img);
        }
        Ok(())
    }

    /// Free uploads no list named this frame: an app that mints an id per frame must not leak.
    ///
    /// The union spans the main list and every canvas list, so a picture only a canvas
    /// samples survives the frame that never names it.
    fn evict_images(&mut self, gl: &glow::Context) {
        let live = std::mem::take(&mut self.frame_keys);
        self.image_textures.retain(|key, texture| {
            let keep = live.contains(key);
            if !keep {
                unsafe { gl.delete_texture(texture.tex) };
            }
            keep
        });
    }

    fn draw_images(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        texture: glow::Texture,
        sampling: Option<ui_core::draw::Sampling>,
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.images;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(sampling) = sampling {
                let filter = match sampling {
                    ui_core::draw::Sampling::Smooth => glow::LINEAR,
                    ui_core::draw::Sampling::Pixelated => glow::NEAREST,
                };
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
            }
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            if let Some(u) = p.uniforms.image {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.uniform_1_i32(Some(&u), 0);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::IMAGE_STRIDE as usize) as i32,
            );
        }
        self.stats.images_drawn += (data.len() / shader::IMAGE_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    fn draw_shadows(
        &mut self,
        gl: &glow::Context,
        viewport: (f32, f32),
        data: &[f32],
        clip: Option<ui_core::draw::ClipEntry>,
    ) -> Result<(), GpuError> {
        let p = &self.shadows;
        unsafe {
            gl.use_program(Some(p.program));
            if let Some(u) = p.uniforms.viewport {
                gl.uniform_2_f32(Some(&u), viewport.0, viewport.1);
            }
            set_clip_uniforms(gl, p, clip, self.scale);
            gl.bind_vertex_array(Some(p.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(p.instance_vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(data), glow::DYNAMIC_DRAW);
            gl.draw_arrays_instanced(
                glow::TRIANGLES,
                0,
                shader::UNIT_QUAD.len() as i32 / 2,
                (data.len() / shader::SHADOW_STRIDE as usize) as i32,
            );
        }
        self.stats.shadows_drawn += (data.len() / shader::SHADOW_STRIDE as usize) as u32;
        self.stats.draw_calls += 1;
        Ok(())
    }

    /// Wrap mode of a cached texture for `id`; `None` when it is not resident. The picture
    /// may be resident under either sampling, so the smooth one answers first.
    pub fn image_wrap(&self, gl: &glow::Context, id: u64) -> Option<(i32, i32)> {
        let t = self
            .image_textures
            .get(&(id, ui_core::draw::Sampling::Smooth))
            .or_else(|| {
                self.image_textures
                    .get(&(id, ui_core::draw::Sampling::Pixelated))
            })?;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(t.tex));
            let s = gl.get_tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S);
            let v = gl.get_tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T);
            gl.bind_texture(glow::TEXTURE_2D, None);
            Some((s, v))
        }
    }

    /// Read layer `depth` back, for diagnosing compositing.
    pub fn read_layer(&self, gl: &glow::Context, depth: usize) -> Option<(u32, u32, Vec<u8>)> {
        let slot = self.layer_targets.get(depth)?;
        let (w, h) = (slot.width, slot.height);
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(slot.fbo));
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Some((w, h, buf))
    }

    /// The persistent frame's raw pixels, for comparing one frame against the next.
    pub fn frame_pixels(&self, gl: &glow::Context) -> Option<(u32, u32, Vec<u8>)> {
        let frame = self.frame.as_ref()?;
        let (w, h) = (frame.width, frame.height);
        Some((w, h, read_framebuffer_rgba8(gl, frame.fbo, w, h)))
    }

    /// Pixels where the persistent frame and `target` disagree, with their bounding box.
    ///
    /// A presented buffer promises the whole surface (`Damage::Full`), so it must match.
    pub fn diff_frame(
        &self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
    ) -> Option<(u64, [u32; 4])> {
        let frame = self.frame.as_ref()?;
        let (w, h) = target.size();
        if (frame.width, frame.height) != (w, h) {
            return None;
        }
        let bytes = (w as usize) * (h as usize) * 4;
        let mut want = vec![0u8; bytes];
        let mut got = vec![0u8; bytes];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(frame.fbo));
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut want)),
            );
            target.bind(gl);
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut got)),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        let mut count = 0u64;
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for (i, (a, b)) in want
            .as_chunks::<4>()
            .0
            .iter()
            .zip(got.as_chunks::<4>().0)
            .enumerate()
        {
            if a != b {
                count += 1;
                let (x, y) = (i as u32 % w, i as u32 / w);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        Some((count, [x0, y0, x1, y1]))
    }

    /// Block until the GPU has finished writing to the target.
    ///
    /// A dma-buf handed to another process needs the writes completed first;
    /// without this the compositor can sample the buffer before the GPU is done.
    pub fn finish(&self, gl: &glow::Context) {
        unsafe { gl.finish() };
    }

    /// Convert straight-alpha RGBA8 into the premultiplied BGRA that `ui_paint::Pixels` holds.
    ///
    /// `read_pixels` and `Pixels` disagree on *both* channel order and alpha convention;
    /// this function exists so callers stop re-deriving it wrongly.
    pub fn to_premultiplied_bgra(straight_rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(straight_rgba.len());
        for p in straight_rgba.as_chunks::<4>().0 {
            let a = p[3] as u32;
            let pm = |c: u8| ((c as u32 * a + 127) / 255) as u8;
            out.extend_from_slice(&[pm(p[2]), pm(p[1]), pm(p[0]), p[3]]);
        }
        out
    }

    /// Read the target back as top-down straight-alpha RGBA8.
    ///
    /// The bytes are whatever the target stores: after a present they carry the output
    /// transfer, so an HDR buffer reads as PQ codes. A picture of the interface is
    /// [`GpuRenderer::capture`] instead.
    pub fn read_pixels(
        &self,
        gl: &glow::Context,
        target: &dyn GpuSurface,
    ) -> Result<Vec<u8>, GpuError> {
        target.bind(gl);
        let (w, h) = target.size();
        let mut buf = read_bound_rgba8(gl, w, h);
        un_premultiply(&mut buf);
        Ok(buf)
    }

    /// The interface as an SDR picture: straight-alpha RGBA8, resolved from a supersampled
    /// render. Read before the present pass, so a capture does not depend on the display's
    /// transfer function or on the buffer being 8-bit.
    pub fn capture(&mut self, gl: &glow::Context) -> Option<(u32, u32, Vec<u8>)> {
        use crate::effect_pass::TargetFormat;
        let (fbo, tex, format, width, height) = {
            let frame = self.frame.as_ref()?;
            (
                frame.fbo,
                frame.tex,
                frame.format,
                frame.width,
                frame.height,
            )
        };
        let ss = self.supersample.max(1);
        let (w, h) = (width / ss, height / ss);
        // An f16 frame cannot be read back as bytes on every driver, and the picture is
        // 8-bit either way: resolve into bytes first, averaging the samples per pixel.
        let fbo = if ss > 1 || format != TargetFormat::Rgba8 {
            let (scratch_fbo, _) = crate::effect_pass::ensure_target(
                &mut self.resolve_scratch,
                gl,
                w,
                h,
                TargetFormat::Rgba8,
            )
            .ok()?;
            self.resolve_frame(gl, scratch_fbo, tex, (w, h), self.encode == Encode::Linear)
                .ok()?;
            scratch_fbo
        } else {
            fbo
        };
        let mut buf = read_framebuffer_rgba8(gl, fbo, w, h);
        un_premultiply(&mut buf);
        Some((w, h, buf))
    }

    /// Free GL objects. Must be called while `ctx` is current.
    pub fn release(&self, gl: &glow::Context) {
        unsafe {
            for p in [
                &self.rects,
                &self.sectors,
                &self.ellipses,
                &self.lines,
                &self.glyphs,
                &self.images,
                &self.shadows,
            ] {
                gl.delete_program(p.program);
                gl.delete_vertex_array(p.vao);
                gl.delete_buffer(p.unit_vbo);
                gl.delete_buffer(p.instance_vbo);
            }
            for t in self.image_textures.values() {
                gl.delete_texture(t.tex);
            }
            if let Some(t) = self.atlas_tex {
                gl.delete_texture(t);
            }
            if let Some(t) = self.color_atlas_tex {
                gl.delete_texture(t);
            }
        }
    }
}

impl Program {
    fn new(
        gl: &glow::Context,
        is_gles: bool,
        vertex: &str,
        fragment: &str,
        stride: i32,
        instance_attribs: &[(&str, i32)],
        required_uniforms: &[&str],
    ) -> Result<Self, GpuError> {
        let program = compile_program(gl, vertex, fragment, is_gles)?;
        unsafe {
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));

            let unit_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(unit_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                as_bytes(&shader::UNIT_QUAD),
                glow::STATIC_DRAW,
            );
            let unit_loc = gl.get_attrib_location(program, "a_unit");
            if let Some(loc) = unit_loc {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(loc, 2, glow::FLOAT, false, 8, 0);
            }

            let instance_vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                as_bytes(&vec![0.0f32; stride as usize * 64]),
                glow::DYNAMIC_DRAW,
            );
            let mut offset = 0i32;
            for (name, size) in instance_attribs {
                if let Some(loc) = gl.get_attrib_location(program, name) {
                    gl.enable_vertex_attrib_array(loc);
                    gl.vertex_attrib_pointer_f32(
                        loc,
                        *size,
                        glow::FLOAT,
                        false,
                        stride * 4,
                        offset * 4,
                    );
                    gl.vertex_attrib_divisor(loc, 1);
                }
                offset += size;
            }

            // A mistyped uniform name yields `None` and then silently does nothing,
            // so a required uniform missing here is a startup error, not a warning.
            for name in required_uniforms {
                if gl.get_uniform_location(program, name).is_none() {
                    return Err(GpuError::from(format!(
                        "shader is missing required uniform `{name}`"
                    )));
                }
            }

            let uniforms = Uniforms {
                viewport: gl.get_uniform_location(program, "u_viewport"),
                atlas_size: gl.get_uniform_location(program, "u_atlas_size"),
                atlas: gl.get_uniform_location(program, "u_atlas"),
                color_atlas_size: gl.get_uniform_location(program, "u_color_atlas_size"),
                color_atlas: gl.get_uniform_location(program, "u_color_atlas"),
                image: gl.get_uniform_location(program, "u_image"),
                opacity: gl.get_uniform_location(program, "u_opacity"),
                layer_tex: gl.get_uniform_location(program, "u_layer"),
                clip_rect: gl.get_uniform_location(program, "u_clip_rect"),
                clip_radius: gl.get_uniform_location(program, "u_clip_radius"),
                clip_shape: gl.get_uniform_location(program, "u_clip_shape"),
                clip_on: gl.get_uniform_location(program, "u_clip_on"),
                src: gl.get_uniform_location(program, "u_src"),
                texel: gl.get_uniform_location(program, "u_texel"),
                dir: gl.get_uniform_location(program, "u_dir"),
                taps: gl.get_uniform_location(program, "u_taps"),
                radius: gl.get_uniform_location(program, "u_radius"),
                supersample: gl.get_uniform_location(program, "u_ss"),
                encode: gl.get_uniform_location(program, "u_encode"),
                capture: gl.get_uniform_location(program, "u_capture"),
                region: gl.get_uniform_location(program, "u_region"),
                weights: gl.get_uniform_location(program, "u_weights"),
                color: gl.get_uniform_location(program, "u_color"),
                scale: gl.get_uniform_location(program, "u_scale"),
                offset: gl.get_uniform_location(program, "u_offset"),
                gamut: gl.get_uniform_location(program, "u_gamut"),
                layer_rect: gl.get_uniform_location(program, "u_rect"),
                layer_radius: gl.get_uniform_location(program, "u_radius"),
                layer_exp: gl.get_uniform_location(program, "u_exp"),
                layer_skip_empty: gl.get_uniform_location(program, "u_skip_empty"),
                tint: gl.get_uniform_location(program, "u_tint"),
                luminosity: gl.get_uniform_location(program, "u_luminosity"),
                noise: gl.get_uniform_location(program, "u_noise"),
                body: gl.get_uniform_location(program, "u_body"),
                glass: gl.get_uniform_location(program, "u_glass"),
                light: gl.get_uniform_location(program, "u_light"),
                optical: gl.get_uniform_location(program, "u_optical"),
                blend: gl.get_uniform_location(program, "u_blend"),
                kit: gl.get_uniform_location(program, "u_kit"),
                shapes: gl.get_uniform_location(program, "u_shape[0]"),
                shape_radius: gl.get_uniform_location(program, "u_shape_radius[0]"),
            };
            gl.bind_vertex_array(None);
            Ok(Self {
                program,
                vao,
                unit_vbo,
                instance_vbo,
                uniforms,
            })
        }
    }
}

/// Pack one shadow: rect, radius, colour, blur + dy, shape, surface edge.
fn push_shadow(out: &mut Vec<f32>, p: &ShadowPrim, scale: f32, encode: Encode) {
    let r = p.rect;
    let (rgb, a) = encode.split(p.colour);
    let rad = [
        p.corners.tl * scale,
        p.corners.tr * scale,
        p.corners.br * scale,
        p.corners.bl * scale,
    ];
    // Corners may not exceed half the shorter side, or the SDF folds.
    let half = (r.size.w * scale).min(r.size.h * scale) * 0.5;
    let rad: Vec<f32> = rad.iter().map(|v| v.min(half).max(0.0)).collect();
    let (edge, inset) = match p.bound {
        Some(bound) => (
            bound.rect,
            matches!(bound.side, ui_core::draw::ShadowSide::Inset),
        ),
        None => (ui_core::geom::Rect::ZERO, false),
    };
    out.extend_from_slice(&[
        r.origin.x * scale,
        r.origin.y * scale,
        r.size.w * scale,
        r.size.h * scale,
        rad[0],
        rad[1],
        rad[2],
        rad[3],
        rgb[0],
        rgb[1],
        rgb[2],
        a,
        (p.blur * scale).max(0.5),
        p.dy * scale,
        p.corners.shape.exponent(),
        p.ring * scale,
        edge.origin.x * scale,
        edge.origin.y * scale,
        edge.size.w * scale,
        edge.size.h * scale,
        if inset { 1.0 } else { 0.0 },
    ]);
}

fn push_rect(out: &mut Vec<f32>, p: &RectPrim, scale: f32, encode: Encode) {
    let r = p.rect;
    // Same clamp as the CPU path: a radius over half the short side folds the SDF.
    let radius = p.radius.clamp_to(r.size);
    let (fill, alpha) = encode.split(p.fill);
    // Gradient: the far stop plus (axis, enabled). Backends without the shader support
    // fall back to `fill`, which is why `fill` is always the near stop.
    let (far, axis, enabled) = match p.gradient {
        Some(g) => {
            let (c, a) = encode.split(g.to);
            let axis = match g.axis {
                ui_core::draw::GradientAxis::Vertical => 0.0,
                ui_core::draw::GradientAxis::Horizontal => 1.0,
            };
            ([c[0], c[1], c[2], a], axis, 1.0)
        }
        None => ([0.0; 4], 0.0, 0.0),
    };
    let (border, width) = match p.border {
        Some((c, w)) => {
            let (br, ba) = encode.split(c);
            ([br[0], br[1], br[2], ba], w * scale)
        }
        None => ([0.0; 4], 0.0),
    };
    // A stroke gradient reuses the fill's axis: every WinUI elevation border is vertical.
    let (border_far, border_grad) = match p.border_gradient {
        Some(g) => {
            let (c, a) = encode.split(g.to);
            ([c[0], c[1], c[2], a], 1.0)
        }
        None => ([0.0; 4], 0.0),
    };
    let fill_stops = p.gradient.map(|g| g.stops).unwrap_or((0.0, 1.0));
    let border_stops = p.border_gradient.map(|g| g.stops).unwrap_or((0.0, 1.0));
    let fill_extent = p
        .gradient
        .and_then(|g| g.extent)
        .map(|e| e * scale)
        .unwrap_or(0.0);
    let border_extent = p
        .border_gradient
        .and_then(|g| g.extent)
        .map(|e| e * scale)
        .unwrap_or(0.0);
    // An empty range (`x >= y`) is how the shader sees "no notch", so a missing one sends 0,0.
    let (notch_lo, notch_hi) = match p.notch {
        Some((from, to)) => (from * scale, to * scale),
        None => (0.0, 0.0),
    };
    // Light-theme elevation borders are mirrored; each brush states its own flip.
    let (fill_flip, border_flip) = (
        p.gradient.map(|g| f32::from(g.flip)).unwrap_or(0.0),
        p.border_gradient.map(|g| f32::from(g.flip)).unwrap_or(0.0),
    );
    out.extend_from_slice(&[
        r.origin.x * scale,
        r.origin.y * scale,
        r.size.w * scale,
        r.size.h * scale,
        radius.tl * scale,
        radius.tr * scale,
        radius.br * scale,
        radius.bl * scale,
        fill[0],
        fill[1],
        fill[2],
        alpha,
        border[0],
        border[1],
        border[2],
        border[3],
        border_far[0],
        border_far[1],
        border_far[2],
        border_far[3],
        border_grad,
        radius.shape.exponent(),
        far[0],
        far[1],
        far[2],
        far[3],
        axis,
        enabled,
        fill_stops.0,
        fill_stops.1,
        border_stops.0,
        border_stops.1,
        fill_extent,
        border_extent,
        width,
        notch_lo,
        notch_hi,
        fill_flip,
        border_flip,
    ]);
}

fn push_sector(out: &mut Vec<f32>, p: &ui_core::draw::SectorPrim, scale: f32, encode: Encode) {
    let s = &p.sector;
    let (rgb, alpha) = encode.split(p.colour);
    // The sweep is normalised here so the shader needs no negative-angle handling.
    let sweep = s.sweep();
    out.extend_from_slice(&[
        s.centre.x * scale,
        s.centre.y * scale,
        s.radius * scale,
        (s.radius - s.thickness).max(0.0) * scale,
        s.trim.0,
        sweep,
        rgb[0],
        rgb[1],
        rgb[2],
        alpha,
        f32::from(s.round_caps),
    ]);
}

fn push_ellipse(out: &mut Vec<f32>, p: &ui_core::draw::EllipsePrim, scale: f32, encode: Encode) {
    let (fill, fill_alpha) = encode.split(p.fill);
    let (rgb, alpha) = encode.split(p.colour);
    out.extend_from_slice(&[
        p.centre.x * scale,
        p.centre.y * scale,
        p.radii.0 * scale,
        p.radii.1 * scale,
        p.thickness * scale,
        fill[0],
        fill[1],
        fill[2],
        fill_alpha,
        rgb[0],
        rgb[1],
        rgb[2],
        alpha,
    ]);
}

fn push_line(out: &mut Vec<f32>, p: &ui_core::draw::LinePrim, scale: f32, encode: Encode) {
    let (rgb, alpha) = encode.split(p.colour);
    out.extend_from_slice(&[
        p.from.x * scale,
        p.from.y * scale,
        p.to.x * scale,
        p.to.y * scale,
        p.thickness * scale,
        rgb[0],
        rgb[1],
        rgb[2],
        alpha,
    ]);
}

fn upload_atlas(
    gl: &glow::Context,
    slot: &mut Option<glow::Texture>,
    width: u32,
    height: u32,
    internal: u32,
    format: u32,
    kind: u32,
    data: &[u8],
) -> Result<(), GpuError> {
    unsafe {
        let tex = match *slot {
            Some(t) => t,
            None => {
                let t = gl.create_texture()?;
                *slot = Some(t);
                t
            }
        };
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            internal as i32,
            width as i32,
            height as i32,
            0,
            format,
            kind,
            glow::PixelUnpackData::Slice(Some(data)),
        );
        // LINEAR, not NEAREST: a glyph's destination carries the quarter pixel its vertical
        // phase did not take, and a 1:1 tile still samples exact texels because fragment
        // centres land on texel centres. Each tile has a pixel of empty padding, so the blend
        // at its edge reaches only transparency.
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
    }
    Ok(())
}

fn push_glyph(out: &mut Vec<f32>, p: &GlyphPrim, scale: f32, encode: Encode) {
    let (rgb, alpha) = encode.split(p.color);
    out.extend_from_slice(&[
        p.rect.origin.x * scale,
        p.rect.origin.y * scale,
        p.rect.size.w * scale,
        p.rect.size.h * scale,
        p.atlas.origin.x,
        p.atlas.origin.y,
        p.atlas.size.w,
        p.atlas.size.h,
        rgb[0],
        rgb[1],
        rgb[2],
        alpha * p.alpha,
        match p.kind {
            ui_core::draw::GlyphKind::Coverage => 0.0,
            ui_core::draw::GlyphKind::Color => 1.0,
        },
    ]);
}

/// Whether a glyph's device rect touches a device scissor `[x, y, w, h]`.
fn glyph_hits_scissor(p: &GlyphPrim, scale: f32, scissor: [i32; 4]) -> bool {
    let x0 = (p.rect.origin.x * scale).floor();
    let y0 = (p.rect.origin.y * scale).floor();
    let x1 = ((p.rect.origin.x + p.rect.size.w) * scale).ceil();
    let y1 = ((p.rect.origin.y + p.rect.size.h) * scale).ceil();
    x1 > scissor[0] as f32
        && y1 > scissor[1] as f32
        && x0 < (scissor[0] + scissor[2]) as f32
        && y0 < (scissor[1] + scissor[3]) as f32
}

fn push_image(out: &mut Vec<f32>, p: &ImagePrim, scale: f32, tex: (u32, u32)) {
    let r = p.rect;
    let corners = p.corners.clamp_to(r.size);
    // Normalised source rect; texels are never scaled, only the destination is.
    let w = tex.0.max(1) as f32;
    let h = tex.1.max(1) as f32;
    let u0 = p.src.origin.x / w;
    let v0 = p.src.origin.y / h;
    let u1 = (p.src.origin.x + p.src.size.w) / w;
    let v1 = (p.src.origin.y + p.src.size.h) / h;
    out.extend_from_slice(&[
        r.origin.x * scale,
        r.origin.y * scale,
        r.size.w * scale,
        r.size.h * scale,
        corners.tl * scale,
        corners.tr * scale,
        corners.br * scale,
        corners.bl * scale,
        u0,
        v0,
        u1,
        v1,
        p.corners.shape.exponent(),
    ]);
}

/// What an uploaded texture was built from.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ImageIdentity {
    width: u32,
    height: u32,
    data: usize,
    colour: ui_core::colour::ContentColour,
    destination: ui_core::colour::Destination,
}

/// Whether the uploaded texture still holds `entry`'s picture as this frame needs it.
///
/// An id names a texture slot, not a picture: an app that fills a canvas every frame keeps
/// its id and hands over a new buffer. The buffer's identity is compared rather than its
/// pixels, because hashing the contents every frame would cost more than the upload. The
/// description and the destination are part of the identity: the same bytes convert
/// differently for each.
fn needs_upload(
    cached: Option<ImageIdentity>,
    entry: &ImageEntry,
    destination: ui_core::colour::Destination,
) -> bool {
    match cached {
        Some(cached) => {
            cached.width != entry.width
                || cached.height != entry.height
                || cached.data != entry.picture_id()
                || cached.colour != entry.colour
                || cached.destination != destination
        }
        None => true,
    }
}

/// Premultiplied sRGB bytes to premultiplied linear half floats.
///
/// Premultiplication is undone before the decode: `decode(a*c)` is not `a*decode(c)`.
fn premultiplied_to_linear(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() * 2);
    for texel in rgba.as_chunks::<4>().0 {
        let alpha = texel[3] as f32 / 255.0;
        for &c in &texel[..3] {
            let straight = ui_core::colour::TransferFunction::Srgb
                .decode((c as f32 / 255.0) / alpha.max(1e-6));
            out.extend_from_slice(&half_bits((straight * alpha).clamp(0.0, 1.0)).to_le_bytes());
        }
        out.extend_from_slice(&half_bits(alpha).to_le_bytes());
    }
    out
}

/// IEEE-754 binary32 to binary16, rounding at the 10-bit mantissa.
///
/// Only linear light in the range a texture carries reaches this, so subnormals are the
/// only edge; infinities and NaNs are passed through rather than clamped.
fn half_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        return sign | 0x7c00 | if mantissa != 0 { 0x0200 } else { 0 };
    }
    let mut e = exponent - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mantissa | 0x80_0000) >> (1 - e);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    let mut m = mantissa + 0x1000;
    if m & 0x80_0000 != 0 {
        m = 0;
        e += 1;
        if e >= 0x1f {
            return sign | 0x7c00;
        }
    }
    sign | ((e as u16) << 10) | ((m >> 13) as u16)
}

/// Upload one image, converted for `destination` and premultiplied, with `CLAMP_TO_EDGE`
/// so a source sub-rect never tiles.
fn upload_image(
    gl: &glow::Context,
    tex: glow::Texture,
    entry: &ImageEntry,
    destination: ui_core::colour::Destination,
) -> Result<(), GpuError> {
    let needed = (entry.width as usize) * (entry.height as usize) * 4;
    let Some(data) = entry.pixels() else {
        return Err(GpuError::from(format!(
            "image {} is a canvas; its target supplies the texture, not an upload",
            entry.id
        )));
    };
    if entry.width == 0 || entry.height == 0 || data.len() < needed {
        return Err(GpuError::from(format!(
            "image {} is {}x{} but carries {} bytes (want {needed})",
            entry.id,
            entry.width,
            entry.height,
            data.len()
        )));
    }
    let encode = Encode::of(destination);
    let placement = entry.colour.placement(destination);
    // The sampler filters the stored channels, so straight alpha would let a transparent
    // texel's colour reach across an edge. Premultiplied here is what filtering must see,
    // and it happens after the conversion: decode(a*c) is not a*decode(c).
    let premultiplied: Vec<u8> = if placement.is_identity() {
        let mut bytes = data[..needed].to_vec();
        for texel in bytes.chunks_exact_mut(4) {
            let alpha = texel[3] as u32;
            for channel in &mut texel[..3] {
                *channel = ((*channel as u32 * alpha + 127) / 255) as u8;
            }
        }
        bytes
    } else {
        // Tables only pay off on a whole texture, and only after the conversion is known
        // to be needed.
        let placement = placement.tabulated();
        data[..needed]
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|texel| {
                let [r, g, b, alpha] = placement.texel(texel);
                let mut out = Vec::with_capacity(4);
                match encode {
                    Encode::Srgb => {
                        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
                        out.extend_from_slice(&[
                            byte(r * alpha),
                            byte(g * alpha),
                            byte(b * alpha),
                            byte(alpha),
                        ]);
                    }
                    Encode::Linear => {
                        for value in [r * alpha, g * alpha, b * alpha, alpha] {
                            out.extend_from_slice(&half_bits(value).to_le_bytes());
                        }
                    }
                }
                out
            })
            .collect()
    };
    let (internal, kind, bytes): (u32, u32, &[u8]) = match encode {
        Encode::Srgb => (glow::RGBA8, glow::UNSIGNED_BYTE, &premultiplied),
        Encode::Linear => (glow::RGBA16F, glow::HALF_FLOAT, &premultiplied),
    };
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            internal as i32,
            entry.width as i32,
            entry.height as i32,
            0,
            glow::RGBA,
            kind,
            glow::PixelUnpackData::Slice(Some(bytes)),
        );
        let filter = match entry.sampling {
            ui_core::draw::Sampling::Smooth => glow::LINEAR,
            ui_core::draw::Sampling::Pixelated => glow::NEAREST,
        };
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
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
    }
    Ok(())
}

/// How colour is written into instance data and textures.
///
/// SDR keeps sRGB-encoded values, which is the look every control was authored with and
/// matches the CPU rasteriser. HDR converts to linear light first: blending, blurs and
/// coverage then average physical light, and values above SDR white stay meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encode {
    Srgb,
    Linear,
}

impl Encode {
    /// The domain a destination blends in.
    fn of(destination: ui_core::colour::Destination) -> Self {
        match destination.domain {
            ui_core::colour::Domain::Encoded => Self::Srgb,
            ui_core::colour::Domain::Linear { .. } => Self::Linear,
        }
    }

    /// Straight-alpha colour as this domain stores it, before premultiplication.
    fn split(self, c: Color) -> ([f32; 3], f32) {
        match self {
            Self::Srgb => ([c.r, c.g, c.b], c.a),
            Self::Linear => (
                [
                    ui_core::colour::TransferFunction::Srgb.decode(c.r),
                    ui_core::colour::TransferFunction::Srgb.decode(c.g),
                    ui_core::colour::TransferFunction::Srgb.decode(c.b),
                ],
                c.a,
            ),
        }
    }
}

/// Apply a clip: scissor bounds the rect cheaply, the shader handles the rounded shape.
fn set_clip_uniforms(
    gl: &glow::Context,
    program: &Program,
    clip: Option<ui_core::draw::ClipEntry>,
    scale: f32,
) {
    unsafe {
        // Must run with `program` current: setting a uniform with no program bound is
        // GL_INVALID_OPERATION, and the error then surfaces at an unrelated call.
        let rounded = clip.is_some_and(|c| !c.corners.is_zero());
        if let Some(u) = program.uniforms.clip_on {
            gl.uniform_1_f32(Some(&u), if rounded { 1.0 } else { 0.0 });
        }
        let Some(c) = clip.filter(|_| rounded) else {
            return;
        };
        // The corners follow the uncut box (`shape`); the scissor still bounds `rect`.
        let clamped = c.corners.clamp_to(c.shape.size);
        if let Some(u) = program.uniforms.clip_rect {
            gl.uniform_4_f32(
                Some(&u),
                c.shape.origin.x * scale,
                c.shape.origin.y * scale,
                c.shape.size.w * scale,
                c.shape.size.h * scale,
            );
        }
        if let Some(u) = program.uniforms.clip_radius {
            gl.uniform_4_f32(
                Some(&u),
                clamped.tl * scale,
                clamped.tr * scale,
                clamped.br * scale,
                clamped.bl * scale,
            );
        }
        if let Some(u) = program.uniforms.clip_shape {
            gl.uniform_1_f32(Some(&u), clamped.shape.exponent());
        }
    }
}

/// Read the bound framebuffer as top-down premultiplied RGBA8.
///
/// An f16 frame converts down through the same call: the picture is 8-bit anyway, and a
/// float readback is not universally supported for a float attachment.
fn read_bound_rgba8(gl: &glow::Context, w: u32, h: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    unsafe {
        gl.read_pixels(
            0,
            0,
            w as i32,
            h as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut buf)),
        );
    }
    buf
}

/// Bind `fbo`, read it as RGBA8, and leave nothing bound.
fn read_framebuffer_rgba8(gl: &glow::Context, fbo: glow::Framebuffer, w: u32, h: u32) -> Vec<u8> {
    unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo)) };
    let buf = read_bound_rgba8(gl, w, h);
    unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, None) };
    buf
}

/// Undo premultiplication in place, so callers see straight alpha.
fn un_premultiply(buf: &mut [u8]) {
    for p in buf.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        if a == 0 || a == 255 {
            continue;
        }
        for channel in &mut p[..3] {
            *channel = ((*channel as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

static PARTIAL_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static FULL_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many frames took the partial path versus the full one.
pub fn partial_frames() -> (u64, u64) {
    (
        PARTIAL_FRAMES.load(std::sync::atomic::Ordering::Relaxed),
        FULL_FRAMES.load(std::sync::atomic::Ordering::Relaxed),
    )
}

fn set_scissor(
    gl: &glow::Context,
    clip: Option<ui_core::geom::Rect>,
    scale: f32,
    viewport: (f32, f32),
    band: Option<[i32; 4]>,
) {
    match batch_scissor(clip.map(|r| device_scissor(r, scale)), band) {
        Scissor::Off => unsafe { gl.disable(glow::SCISSOR_TEST) },
        Scissor::Empty => unsafe {
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(0, 0, 0, 0);
        },
        Scissor::Box(r) => {
            // No Y flip: this renderer maps device y=0 to GL's bottom, which is the same
            // memory row the compositor reads as the image top (see the vertex shader).
            let _ = viewport;
            unsafe {
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(r[0], r[1], r[2], r[3]);
            }
        }
    }
}

/// A logical rect as a device scissor: round the *far edge* out, not the size, or a fractional
/// origin drops the last row the rect covers (CPU `clip_to` and Skia's `roundOut` do the same).
/// Two device scissors as one, or `None` when they miss each other.
/// What a batch's scissor box has to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scissor {
    /// Clip to nothing.
    Off,
    /// Clip to this device box.
    Box([i32; 4]),
    /// Draw nothing: the batch's clip and the redraw band do not overlap.
    Empty,
}

/// Clip ∩ band. An empty intersection must mean "nothing visible", not "unclipped".
pub(crate) fn batch_scissor(clip: Option<[i32; 4]>, band: Option<[i32; 4]>) -> Scissor {
    match (clip, band) {
        (Some(c), Some(b)) => match intersect_scissor(c, b) {
            Some(r) => Scissor::Box(r),
            None => Scissor::Empty,
        },
        (Some(c), None) => Scissor::Box(c),
        (None, Some(b)) => Scissor::Box(b),
        (None, None) => Scissor::Off,
    }
}

fn intersect_scissor(a: [i32; 4], b: [i32; 4]) -> Option<[i32; 4]> {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    (x1 > x0 && y1 > y0).then(|| [x0, y0, x1 - x0, y1 - y0])
}

/// Grow a repaint band until it covers every reader whose reach it touches: a frost re-reads
/// the frame and a layer kernel re-reads its layer, so a change near either moves more pixels.
fn cover_readers(
    mut b: [i32; 4],
    frosts: &[(ui_core::geom::Rect, ui_core::geom::Rect)],
    effects: &[ui_core::geom::Rect],
) -> [i32; 4] {
    let passes = frosts.len() + effects.len();
    for _ in 0..=passes {
        let mut grew = false;
        for (read, vis) in frosts {
            let band = rect_of(b);
            if !read.intersects(band) {
                continue;
            }
            let merged = device_scissor(band.union(*vis), 1.0);
            if merged != b {
                b = merged;
                grew = true;
            }
        }
        // Any change within a kernel's reach alters pixels outside the damage, and the layer
        // is recomputed whole anyway: its bounds are the reader's own damaged region.
        for bounds in effects {
            let band = rect_of(b);
            if !bounds.intersects(band) {
                continue;
            }
            let merged = device_scissor(band.union(*bounds), 1.0);
            if merged != b {
                b = merged;
                grew = true;
            }
        }
        if !grew {
            return b;
        }
    }
    // Every union reads the band as it stands: one built from an earlier snapshot can be
    // smaller, so two readers reaching each other would never settle.
    debug_assert!(false, "a band must settle within one pass per reader");
    b
}

/// Whether any damaged area falls inside `bounds`.
///
/// Damage arrives in logical units and a layer's bounds are the target's device pixels, so
/// the two must be compared in one space.
fn damage_reaches(
    damage: &ui_core::damage::Damage,
    bounds: ui_core::geom::Rect,
    scale: f32,
) -> bool {
    match damage {
        ui_core::damage::Damage::None => false,
        ui_core::damage::Damage::Full => true,
        ui_core::damage::Damage::Rects(rects) => rects.iter().any(|r| {
            ui_core::geom::Rect::new(
                r.origin.x * scale,
                r.origin.y * scale,
                r.size.w * scale,
                r.size.h * scale,
            )
            .intersects(bounds)
        }),
    }
}

pub(crate) fn device_scissor(r: ui_core::geom::Rect, scale: f32) -> [i32; 4] {
    let x0 = (r.origin.x * scale).floor().max(0.0);
    let y0 = (r.origin.y * scale).floor().max(0.0);
    let x1 = ((r.origin.x + r.size.w) * scale).ceil().max(0.0);
    let y1 = ((r.origin.y + r.size.h) * scale).ceil().max(0.0);
    [x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32]
}

/// Union of two device rects, clamped to `size`.
pub(crate) fn union_device(a: [i32; 4], b: [i32; 4], size: (u32, u32)) -> [i32; 4] {
    let x0 = a[0].min(b[0]);
    let y0 = a[1].min(b[1]);
    let x1 = (a[0] + a[2]).max(b[0] + b[2]).min(size.0 as i32);
    let y1 = (a[1] + a[3]).max(b[1] + b[3]).min(size.1 as i32);
    [x0, y0, (x1 - x0).max(0), (y1 - y0).max(0)]
}

pub(crate) fn as_bytes(data: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
}

fn compile_program(
    gl: &glow::Context,
    vertex: &str,
    fragment: &str,
    is_gles: bool,
) -> Result<glow::Program, GpuError> {
    let header = if is_gles {
        "#version 300 es\n"
    } else {
        "#version 330\n"
    };
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let shader = gl.create_shader(kind)?;
            // `AA` is spliced into every program from one definition, so the vertex
            // expansion and the fragment ramp cannot drift apart.
            gl.shader_source(
                shader,
                &format!("{header}{}", shader::splice(src, shader::AA_GLSL)),
            );
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let log = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                return Err(GpuError::Gl(format!("shader compile: {log}")));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            return Err(GpuError::Gl(format!("program link: {log}")));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        Ok(program)
    }
}

#[cfg(test)]
mod tests {

    /// Two frosts that reach each other must settle, not pull the band back and forth.
    #[test]
    fn a_band_shared_by_two_reaching_frosts_settles() {
        let left = (
            ui_core::geom::Rect::new(722.0, 462.0, 350.0, 256.0),
            ui_core::geom::Rect::new(772.0, 512.0, 250.0, 156.0),
        );
        let right = (
            ui_core::geom::Rect::new(1020.0, 486.0, 302.0, 208.0),
            ui_core::geom::Rect::new(1046.0, 512.0, 250.0, 156.0),
        );
        assert_eq!(
            cover_readers([1046, 512, 522, 342], &[left, right], &[]),
            [772, 512, 796, 342]
        );
    }

    /// A change inside an effect layer alters pixels outside the damage through its kernel.
    #[test]
    fn a_band_touching_a_layer_kernel_grows_over_that_layer() {
        let layer = Rect::new(26.0, 16.0, 142.0, 28.0);
        assert_eq!(
            cover_readers([30, 20, 22, 22], &[], &[layer]),
            [26, 16, 142, 28],
            "the blurred tail past the damage must be repainted too"
        );
        assert_eq!(
            cover_readers([0, 60, 10, 10], &[], &[layer]),
            [0, 60, 10, 10],
            "a band clear of the layer leaves it alone"
        );
    }
    use super::*;
    use ui_core::geom::{Color, Corners, Rect};

    fn rect_prim(fill: Color) -> RectPrim {
        RectPrim {
            rect: Rect::new(10.0, 20.0, 30.0, 40.0),
            radius: Corners::all(4.0),
            fill,
            gradient: None,
            border: None,
            border_gradient: None,
            notch: None,
        }
    }

    #[test]
    fn rect_instance_fields_land_at_the_shader_offsets() {
        let mut out = Vec::new();
        push_rect(
            &mut out,
            &rect_prim(Color::from_rgba8(51, 102, 153, 204)),
            1.0,
            Encode::Srgb,
        );
        // Layout: rect(0..4), radius(4..8), fill(8..12), border(12..16).
        assert_eq!(&out[0..4], &[10.0, 20.0, 30.0, 40.0]);
        assert_eq!(&out[4..8], &[4.0, 4.0, 4.0, 4.0]);
        let c = Color::from_rgba8(51, 102, 153, 204);
        assert_eq!(&out[8..12], &[c.r, c.g, c.b, c.a]);
    }

    #[test]
    fn a_new_buffer_behind_a_known_id_is_uploaded_again() {
        // A canvas or a video keeps its id and hands over new pixels at the same size: a
        // size-only check would keep the first picture on the GPU for the life of the run.
        let data = std::sync::Arc::new(vec![1u8, 2, 3, 255]);
        let entry = |data: &std::sync::Arc<Vec<u8>>| ui_core::draw::ImageEntry {
            id: 5,
            width: 1,
            height: 1,
            source: ui_core::draw::ImageSource::Pixels(data.clone()),
            colour: ui_core::colour::ContentColour::SDR,
            sampling: ui_core::draw::Sampling::Smooth,
        };
        let first = entry(&data);
        let uploaded = ImageIdentity {
            width: 1,
            height: 1,
            data: data.as_ptr() as usize,
            colour: ui_core::colour::ContentColour::SDR,
            destination: ui_core::colour::Destination::SDR,
        };
        let sdr = ui_core::colour::Destination::SDR;
        assert!(
            !needs_upload(Some(uploaded), &first, sdr),
            "the same buffer is not uploaded twice"
        );
        assert!(
            needs_upload(None, &first, sdr),
            "a texture nothing uploaded yet is uploaded"
        );
        let resized = ImageIdentity {
            width: 2,
            ..uploaded
        };
        assert!(
            needs_upload(Some(resized), &first, sdr),
            "a resized texture is uploaded again"
        );
        let fresh = std::sync::Arc::new(vec![9u8, 9, 9, 255]);
        assert!(
            needs_upload(Some(uploaded), &entry(&fresh), sdr),
            "new pixels behind a known id must reach the GPU"
        );
        // The same bytes mean different colours under another description or in another
        // domain, so neither may leave the old texture in place.
        let pq = ui_core::colour::ContentColour {
            storage: ui_core::colour::PixelStorage::Packed10,
            ..ui_core::colour::ContentColour::SDR
        };
        let described = ui_core::draw::ImageEntry {
            colour: pq,
            ..first.clone()
        };
        assert!(
            needs_upload(Some(uploaded), &described, sdr),
            "a new description must reach the GPU"
        );
        let hdr = ui_core::colour::Destination {
            domain: ui_core::colour::Domain::Linear {
                reference_nits: 203.0,
                min_nits: 0.0,
                peak_nits: 417.0,
            },
            tone: ui_core::colour::ToneMapping::Bt2390,
        };
        assert!(
            needs_upload(Some(uploaded), &first, hdr),
            "the same picture in another domain must be converted again"
        );
    }

    #[test]
    fn a_cached_layer_is_redrawn_whole() {
        // The cull may only skip work that cannot write into the damaged band. A cached
        // layer is cleared, redrawn and stored as a whole, so a culled batch leaves a hole
        // that the next frame reusing the cache composites back onto the screen.
        let layer = |cache_key| ui_core::draw::LayerEntry {
            bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
            effects: Vec::new(),
            radius: Corners::all(0.0),
            cache_key,
        };
        let layers = [layer(None), layer(Some(7))];
        let band = Some([10, 10, 20, 20]);
        assert_eq!(culling_band(band, &layers, &[]), band);
        assert_eq!(
            culling_band(band, &layers, &[0]),
            band,
            "an uncached layer still culls"
        );
        assert_eq!(
            culling_band(band, &layers, &[0, 1]),
            None,
            "a cached layer must be drawn in full"
        );
    }

    #[test]
    fn a_layer_the_band_misses_keeps_its_pixels() {
        // A band clear of a layer's bounds means nothing in it was damaged, so neither its
        // drawing nor its composite can change a pixel the frame shows.
        let bounds = Rect::new(20.0, 10.0, 60.0, 60.0);
        assert!(
            band_clear_of(Some([0, 0, 10, 10]), bounds),
            "the band is elsewhere"
        );
        assert!(
            !band_clear_of(Some([0, 0, 30, 20]), bounds),
            "a band overlapping the layer redraws it"
        );
        assert!(
            !band_clear_of(None, bounds),
            "a full frame redraws everything"
        );
    }

    #[test]
    fn a_scaled_layer_sees_damage_inside_its_device_bounds() {
        // Bounds are the cached target's device pixels; damage is logical. At scale 2 a
        // logical rect at (120, 60) lands at device (240, 120), inside a layer whose logical
        // box is (100, 50, 50, 50): comparing the two without converting says "clean", and
        // the cached layer keeps showing the content it was first drawn with.
        let bounds = Rect::new(200.0, 100.0, 100.0, 100.0);
        let inside = ui_core::damage::Damage::Rects(vec![Rect::new(120.0, 60.0, 10.0, 10.0)]);
        assert!(
            damage_reaches(&inside, bounds, 2.0),
            "device (240,120) is inside"
        );
        let outside = ui_core::damage::Damage::Rects(vec![Rect::new(10.0, 10.0, 10.0, 10.0)]);
        assert!(
            !damage_reaches(&outside, bounds, 2.0),
            "device (20,20) is outside"
        );
        assert!(damage_reaches(&ui_core::damage::Damage::Full, bounds, 2.0));
        assert!(!damage_reaches(&ui_core::damage::Damage::None, bounds, 2.0));
    }

    #[test]
    fn glyph_culling_keeps_every_glyph_that_touches_the_clip() {
        // Culling an overflowing line must never eat a visible glyph: the predicate may
        // reject only what is wholly outside, including a glyph straddling the edge.
        let glyph = |x: f32| GlyphPrim {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            atlas: Rect::new(0.0, 0.0, 10.0, 10.0),
            alpha: 1.0,
            color: Color::BLACK,
            kind: ui_core::draw::GlyphKind::Coverage,
        };
        let scissor = device_scissor(Rect::new(10.0, 0.0, 20.0, 20.0), 2.0);
        assert_eq!(scissor, [20, 0, 40, 40]);
        assert!(
            !glyph_hits_scissor(&glyph(0.0), 2.0, scissor),
            "entirely left"
        );
        assert!(
            !glyph_hits_scissor(&glyph(40.0), 2.0, scissor),
            "entirely right"
        );
        assert!(
            glyph_hits_scissor(&glyph(5.0), 2.0, scissor),
            "straddling the left edge"
        );
        assert!(glyph_hits_scissor(&glyph(20.0), 2.0, scissor), "inside");
    }

    fn image_prim(src: Rect, corners: Corners) -> ImagePrim {
        ImagePrim {
            rect: Rect::new(10.0, 20.0, 30.0, 40.0),
            corners,
            texture: 0,
            src,
        }
    }

    #[test]
    fn image_uv_covers_the_source_rect_rather_than_the_whole_texture() {
        // Sampling the whole texture instead of `src` is how a small source sub-rect ends up
        // tiled or showing the wrong quadrant; the uv rect must come from `src` alone.
        let mut out = Vec::new();
        push_image(
            &mut out,
            &image_prim(Rect::new(16.0, 8.0, 16.0, 16.0), Corners::ZERO),
            1.0,
            (64, 32),
        );
        assert_eq!(&out[8..12], &[0.25, 0.25, 0.5, 0.75]);
    }

    #[test]
    fn a_scissor_keeps_the_row_its_far_edge_covers() {
        // 207.8..256.8 covers device row 256; `ceil(size)` from a floored origin stops at 255 and
        // silently drops the row, which is how a 1dp divider under a fractional box disappears.
        assert_eq!(
            device_scissor(Rect::new(0.0, 207.8, 10.0, 49.0), 1.0),
            [0, 207, 10, 50]
        );
        // The far edge rounds out in device space, not in logical space.
        assert_eq!(
            device_scissor(Rect::new(0.0, 10.1, 4.0, 2.0), 1.5),
            [0, 15, 6, 4]
        );
        // An integral rect is untouched.
        assert_eq!(
            device_scissor(Rect::new(3.0, 4.0, 5.0, 6.0), 1.0),
            [3, 4, 5, 6]
        );
    }

    #[test]
    fn a_clip_that_misses_the_band_draws_nothing_instead_of_everything() {
        // A disjoint clip used to disable the scissor, painting the batch over the whole frame.
        let band = Some([100, 100, 50, 50]);
        assert_eq!(batch_scissor(Some([0, 0, 40, 40]), band), Scissor::Empty);
        assert_eq!(
            batch_scissor(Some([120, 120, 10, 10]), band),
            Scissor::Box([120, 120, 10, 10])
        );
        // Without a band, a clip still clips — and nothing at all means no clipping.
        assert_eq!(
            batch_scissor(Some([0, 0, 40, 40]), None),
            Scissor::Box([0, 0, 40, 40])
        );
        assert_eq!(batch_scissor(None, None), Scissor::Off);
    }
}
