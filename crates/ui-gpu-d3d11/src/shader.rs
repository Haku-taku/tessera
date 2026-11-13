//! HLSL sources, the constant buffer each program declares, and its instance layout.
//!
//! The constant buffer is generated here and its byte offsets come back with the sources, so
//! the shader's view of a uniform and the device's cannot describe different layouts.

use std::fmt::Write;

use ui_gpu_scene::{MAX_TAPS, ProgramKey};

/// The SDF feather, in device pixels: coverage ramps over this width, and every quad reaches
/// this far past its shape so that no covered pixel centre falls outside it.
const AA: &str = include_str!("shaders/aa.hlsl");

/// Clip uniforms and the coverage test shared by every fragment program.
const CLIP: &str = include_str!("shaders/clip.hlsl");

/// Rounded-shape coverage plus the uniforms it reads, shared by every backdrop program.
const COVERAGE: &str = include_str!("shaders/coverage.hlsl");

/// Rounded-box signed distance, shared by the rect, shadow, image and glass programs.
const SDF: &str = include_str!("shaders/sdf.hlsl");

/// Device pixels to clip space; the only place the y direction is flipped.
const COMMON: &str = include_str!("shaders/common.hlsl");

const LAYER_VERTEX: &str = include_str!("shaders/layer.vert.hlsl");
const LAYER_FRAGMENT: &str = include_str!("shaders/layer.frag.hlsl");
const EFFECT_FRAGMENT: &str = include_str!("shaders/effect.frag.hlsl");
const BOX_FRAGMENT: &str = include_str!("shaders/box.frag.hlsl");
const OUTPUT_FRAGMENT: &str = include_str!("shaders/output.frag.hlsl");
const RESOLVE_FRAGMENT: &str = include_str!("shaders/resolve.frag.hlsl");
const CAPTURE_FRAGMENT: &str = include_str!("shaders/capture.frag.hlsl");
const RECT_VERTEX: &str = include_str!("shaders/rect.vert.hlsl");
const RECT_FRAGMENT: &str = include_str!("shaders/rect.frag.hlsl");
const SECTOR_VERTEX: &str = include_str!("shaders/sector.vert.hlsl");
const SECTOR_FRAGMENT: &str = include_str!("shaders/sector.frag.hlsl");
const ELLIPSE_VERTEX: &str = include_str!("shaders/ellipse.vert.hlsl");
const ELLIPSE_FRAGMENT: &str = include_str!("shaders/ellipse.frag.hlsl");
const LINE_VERTEX: &str = include_str!("shaders/line.vert.hlsl");
const LINE_FRAGMENT: &str = include_str!("shaders/line.frag.hlsl");
const GLYPH_VERTEX: &str = include_str!("shaders/glyph.vert.hlsl");
const GLYPH_FRAGMENT: &str = include_str!("shaders/glyph.frag.hlsl");
const SHADOW_VERTEX: &str = include_str!("shaders/shadow.vert.hlsl");
const SHADOW_FRAGMENT: &str = include_str!("shaders/shadow.frag.hlsl");
const IMAGE_VERTEX: &str = include_str!("shaders/image.vert.hlsl");
const IMAGE_FRAGMENT: &str = include_str!("shaders/image.frag.hlsl");
const ACRYLIC_FRAGMENT: &str = include_str!("shaders/acrylic.frag.hlsl");
const LIQUID_GLASS_FRAGMENT: &str = include_str!("shaders/liquid_glass.frag.hlsl");

/// A scalar array takes one register per element in a D3D11 constant buffer, so the tap
/// weights are declared as `float4` and read through here; the bytes still arrive four to a
/// register, exactly as [`Kind::Weights`] lays them out.
const WEIGHTS: &str = "\
float tap_weight(int index) {\n\
    return u_weights[index / 4][index % 4];\n\
}\n";

/// Texture units the fragment programs can sample, and the fetch that picks one.
///
/// D3D11 wants a literal sampler subscript, so a unit chosen by a uniform becomes a two-way
/// select instead of `u_textures[unit]`.
const TEXTURES: &str = "\
Texture2D u_textures[2] : register(t0);\n\
SamplerState u_samplers[2] : register(s0);\n\
float4 sample_unit(int unit, float2 uv) {\n\
    if (unit == 0) {\n\
        return u_textures[0].Sample(u_samplers[0], uv);\n\
    }\n\
    return u_textures[1].Sample(u_samplers[1], uv);\n\
}\n\
float4 sample_unit_grad(int unit, float2 uv, float2 dx, float2 dy) {\n\
    if (unit == 0) {\n\
        return u_textures[0].SampleGrad(u_samplers[0], uv, dx, dy);\n\
    }\n\
    return u_textures[1].SampleGrad(u_samplers[1], uv, dx, dy);\n\
}\n";

/// One member of a program's constant buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Float,
    Int,
    Vec2,
    Vec4,
    Mat3,
    Mat4,
    /// `MAX_TAPS` weights of one separable convolution.
    Weights,
    /// Four fused shapes: centre and half extent, or their four corner radii.
    Shapes,
}

impl Kind {
    fn declaration(self) -> (&'static str, String) {
        match self {
            Self::Float => ("float", String::new()),
            Self::Int => ("int", String::new()),
            Self::Vec2 => ("float2", String::new()),
            Self::Vec4 => ("float4", String::new()),
            Self::Mat3 => ("float3x3", String::new()),
            Self::Mat4 => ("float4x4", String::new()),
            Self::Weights => ("float4", format!("[{}]", MAX_TAPS.div_ceil(4))),
            Self::Shapes => ("float4", "[4]".to_string()),
        }
    }

    /// Registers the member occupies: one per vector or scalar, one per matrix row, one per
    /// array element.
    fn registers(self) -> u32 {
        match self {
            Self::Float | Self::Int | Self::Vec2 | Self::Vec4 => 1,
            Self::Mat3 => 3,
            Self::Mat4 | Self::Shapes => 4,
            Self::Weights => MAX_TAPS.div_ceil(4),
        }
    }
}

/// A uniform the painter may set on a program.
#[derive(Clone, Copy, Debug)]
pub struct Uniform {
    pub name: &'static str,
    pub kind: Kind,
}

/// A program's sources and where its uniforms live.
pub struct Program {
    pub vertex: String,
    pub fragment: String,
    /// Uniform name, as the painter asks for it, to byte offset in the constant buffer.
    pub offsets: Vec<(String, u32)>,
    /// Constant buffer size in bytes.
    pub size: u32,
}

const fn uniform(name: &'static str, kind: Kind) -> Uniform {
    Uniform { name, kind }
}

const RECT: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_rect", Kind::Vec4),
    uniform("u_radius", Kind::Vec4),
    uniform("u_exp", Kind::Float),
    uniform("u_opacity", Kind::Float),
    uniform("u_layer", Kind::Int),
    uniform("u_skip_empty", Kind::Float),
];

const CLIP_UNIFORMS: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_clip_rect", Kind::Vec4),
    uniform("u_clip_radius", Kind::Vec4),
    uniform("u_clip_shape", Kind::Float),
    uniform("u_clip_on", Kind::Float),
];

const EFFECT: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_texel", Kind::Vec2),
    uniform("u_dir", Kind::Vec2),
    uniform("u_src", Kind::Int),
    uniform("u_taps", Kind::Int),
    uniform("u_weights", Kind::Weights),
    uniform("u_color", Kind::Mat4),
];

const BOX: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_texel", Kind::Vec2),
    uniform("u_dir", Kind::Vec2),
    uniform("u_src", Kind::Int),
    uniform("u_radius", Kind::Int),
    uniform("u_color", Kind::Mat4),
];

const OUTPUT: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_src", Kind::Int),
    uniform("u_scale", Kind::Float),
    uniform("u_offset", Kind::Float),
    uniform("u_gamut", Kind::Mat3),
];

const RESOLVE: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_src", Kind::Int),
    uniform("u_ss", Kind::Int),
    uniform("u_encode", Kind::Int),
];

const CAPTURE: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_src", Kind::Int),
    uniform("u_region", Kind::Vec4),
];

const GLYPH: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_atlas_size", Kind::Vec2),
    uniform("u_color_atlas_size", Kind::Vec2),
    uniform("u_atlas", Kind::Int),
    uniform("u_color_atlas", Kind::Int),
    uniform("u_clip_rect", Kind::Vec4),
    uniform("u_clip_radius", Kind::Vec4),
    uniform("u_clip_shape", Kind::Float),
    uniform("u_clip_on", Kind::Float),
];

const IMAGE: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_image", Kind::Int),
    uniform("u_clip_rect", Kind::Vec4),
    uniform("u_clip_radius", Kind::Vec4),
    uniform("u_clip_shape", Kind::Float),
    uniform("u_clip_on", Kind::Float),
];

const ACRYLIC: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_src", Kind::Int),
    uniform("u_rect", Kind::Vec4),
    uniform("u_radius", Kind::Vec4),
    uniform("u_exp", Kind::Float),
    uniform("u_tint", Kind::Vec4),
    uniform("u_luminosity", Kind::Vec4),
    uniform("u_noise", Kind::Float),
];

const GLASS_KIT: &[Uniform] = &[
    uniform("u_viewport", Kind::Vec2),
    uniform("u_src", Kind::Int),
    uniform("u_rect", Kind::Vec4),
    uniform("u_radius", Kind::Vec4),
    uniform("u_exp", Kind::Float),
    uniform("u_body", Kind::Vec4),
    uniform("u_glass", Kind::Vec4),
    uniform("u_optical", Kind::Vec4),
    uniform("u_light", Kind::Vec4),
    uniform("u_kit", Kind::Vec4),
    uniform("u_blend", Kind::Vec4),
    uniform("u_capture", Kind::Vec4),
    uniform("u_shape", Kind::Shapes),
    uniform("u_shape_radius", Kind::Shapes),
];

fn uniforms(key: ProgramKey) -> &'static [Uniform] {
    match key {
        ProgramKey::Layers => RECT,
        ProgramKey::Effects => EFFECT,
        ProgramKey::Box => BOX,
        ProgramKey::OutputSrgb | ProgramKey::OutputLinear | ProgramKey::OutputScRgb => OUTPUT,
        ProgramKey::Resolve => RESOLVE,
        ProgramKey::Capture => CAPTURE,
        ProgramKey::Rects
        | ProgramKey::Sectors
        | ProgramKey::Ellipses
        | ProgramKey::Lines
        | ProgramKey::Shadows => CLIP_UNIFORMS,
        ProgramKey::Glyphs => GLYPH,
        ProgramKey::Images => IMAGE,
        ProgramKey::Acrylic | ProgramKey::AcrylicLinear => ACRYLIC,
        ProgramKey::GlassKit | ProgramKey::GlassKitLinear => GLASS_KIT,
    }
}

/// What one program's vertex shader hands its fragment shader, in semantic order.
const SCREEN: &[(&str, u32)] = &[("v_uv", 2), ("v_device", 2)];
const RECT_VARYINGS: &[(&str, u32)] = &[
    ("v_local", 2),
    ("v_half", 2),
    ("v_radius", 4),
    ("v_fill", 4),
    ("v_border", 4),
    ("v_border2", 4),
    ("v_border_g", 1),
    ("v_border_w", 1),
    ("v_shape", 1),
    ("v_device", 2),
    ("v_fill2", 4),
    ("v_grad", 2),
    ("v_stops", 4),
    ("v_extent", 2),
    ("v_notch", 2),
    ("v_flip", 2),
];
const SECTOR_VARYINGS: &[(&str, u32)] = &[
    ("v_device", 2),
    ("v_centre", 2),
    ("v_geom", 4),
    ("v_fill", 4),
    ("v_caps", 1),
];
const ELLIPSE_VARYINGS: &[(&str, u32)] = &[
    ("v_device", 2),
    ("v_centre", 2),
    ("v_radii", 2),
    ("v_width", 1),
    ("v_fill", 4),
    ("v_stroke", 4),
];
const LINE_VARYINGS: &[(&str, u32)] = &[
    ("v_device", 2),
    ("v_from", 2),
    ("v_to", 2),
    ("v_width", 1),
    ("v_fill", 4),
];
const GLYPH_VARYINGS: &[(&str, u32)] =
    &[("v_uv", 2), ("v_color", 4), ("v_kind", 1), ("v_device", 2)];
const SHADOW_VARYINGS: &[(&str, u32)] = &[
    ("v_local", 2),
    ("v_half", 2),
    ("v_radius", 4),
    ("v_colour", 4),
    ("v_soft", 2),
    ("v_shape", 1),
    ("v_ring", 1),
    ("v_cut_local", 2),
    ("v_cut_half", 2),
    ("v_inset", 1),
    ("v_device", 2),
];
const IMAGE_VARYINGS: &[(&str, u32)] = &[
    ("v_local", 2),
    ("v_half", 2),
    ("v_radius", 4),
    ("v_shape", 1),
    ("v_uv", 2),
    ("v_device", 2),
];

fn varyings(key: ProgramKey) -> &'static [(&'static str, u32)] {
    match key {
        ProgramKey::Rects => RECT_VARYINGS,
        ProgramKey::Sectors => SECTOR_VARYINGS,
        ProgramKey::Ellipses => ELLIPSE_VARYINGS,
        ProgramKey::Lines => LINE_VARYINGS,
        ProgramKey::Glyphs => GLYPH_VARYINGS,
        ProgramKey::Shadows => SHADOW_VARYINGS,
        ProgramKey::Images => IMAGE_VARYINGS,
        _ => SCREEN,
    }
}

/// The shader pair, and whether the linear-domain variant is wanted.
fn bodies(key: ProgramKey) -> (&'static str, &'static str, bool) {
    match key {
        ProgramKey::Layers => (LAYER_VERTEX, LAYER_FRAGMENT, false),
        ProgramKey::Effects => (LAYER_VERTEX, EFFECT_FRAGMENT, false),
        ProgramKey::Box => (LAYER_VERTEX, BOX_FRAGMENT, false),
        ProgramKey::OutputSrgb => (LAYER_VERTEX, OUTPUT_FRAGMENT, false),
        ProgramKey::OutputLinear => (LAYER_VERTEX, OUTPUT_FRAGMENT, true),
        ProgramKey::OutputScRgb => (LAYER_VERTEX, OUTPUT_FRAGMENT, true),
        ProgramKey::Resolve => (LAYER_VERTEX, RESOLVE_FRAGMENT, false),
        ProgramKey::Capture => (LAYER_VERTEX, CAPTURE_FRAGMENT, false),
        ProgramKey::Rects => (RECT_VERTEX, RECT_FRAGMENT, false),
        ProgramKey::Sectors => (SECTOR_VERTEX, SECTOR_FRAGMENT, false),
        ProgramKey::Ellipses => (ELLIPSE_VERTEX, ELLIPSE_FRAGMENT, false),
        ProgramKey::Lines => (LINE_VERTEX, LINE_FRAGMENT, false),
        ProgramKey::Glyphs => (GLYPH_VERTEX, GLYPH_FRAGMENT, false),
        ProgramKey::Shadows => (SHADOW_VERTEX, SHADOW_FRAGMENT, false),
        ProgramKey::Images => (IMAGE_VERTEX, IMAGE_FRAGMENT, false),
        ProgramKey::Acrylic => (LAYER_VERTEX, ACRYLIC_FRAGMENT, false),
        ProgramKey::AcrylicLinear => (LAYER_VERTEX, ACRYLIC_FRAGMENT, true),
        ProgramKey::GlassKit => (LAYER_VERTEX, LIQUID_GLASS_FRAGMENT, false),
        ProgramKey::GlassKitLinear => (LAYER_VERTEX, LIQUID_GLASS_FRAGMENT, true),
    }
}

/// The shared blocks a program's fragment shader needs.
fn fragment_blocks(key: ProgramKey) -> String {
    use ProgramKey::*;
    match key {
        Rects | Shadows | Images => format!("{CLIP}\n{SDF}\n"),
        Sectors | Ellipses | Lines | Glyphs => format!("{CLIP}\n"),
        Layers | Acrylic | AcrylicLinear => format!("{COVERAGE}\n"),
        GlassKit | GlassKitLinear => format!("{COVERAGE}\n{SDF}\n"),
        _ => String::new(),
    }
}

fn float_type(size: u32) -> &'static str {
    match size {
        1 => "float",
        2 => "float2",
        3 => "float3",
        _ => "float4",
    }
}

/// One struct of float fields, each its own semantic index.
fn structure(name: &str, fields: &[(&str, u32)], position: bool) -> String {
    let mut out = format!("struct {name} {{\n");
    if position {
        out.push_str("    float4 position : SV_POSITION;\n");
    }
    for (index, (field, size)) in fields.iter().enumerate() {
        let _ = writeln!(out, "    {} {field} : TEXCOORD{index};", float_type(*size));
    }
    out.push_str("};\n");
    out
}

/// Build both stages of `key` with the constant buffer they share.
pub fn build(key: ProgramKey) -> Program {
    let uniforms = uniforms(key);
    let mut offsets = Vec::with_capacity(uniforms.len());
    let mut buffer = String::from("cbuffer Uniforms : register(b0) {\n");
    let mut cursor = 0;
    for uniform in uniforms {
        let register = cursor;
        cursor += uniform.kind.registers();
        let (ty, suffix) = uniform.kind.declaration();
        // An array uniform is named by its first element in the painter, which is also where
        // the upload starts.
        let slot = if suffix.is_empty() {
            uniform.name.to_string()
        } else {
            format!("{}[0]", uniform.name)
        };
        offsets.push((slot, register * 16));
        let _ = writeln!(
            buffer,
            "    {ty} {}{suffix} : packoffset(c{register});",
            uniform.name
        );
    }
    buffer.push_str("};\n");
    buffer.push_str(TEXTURES);
    buffer.push_str(AA);
    if uniforms.iter().any(|uniform| uniform.kind == Kind::Weights) {
        buffer.push_str(WEIGHTS);
    }

    let (vertex_body, fragment_body, linear) = bodies(key);
    let spec = key.spec();
    let mut inputs: Vec<(&str, u32)> = vec![("a_unit", 2)];
    inputs.extend(
        spec.attributes
            .iter()
            .map(|(name, size)| (*name, *size as u32)),
    );
    let varyings = varyings(key);

    let header = match (linear, key) {
        (true, ProgramKey::OutputScRgb) => "#define LINEAR_DOMAIN 1\n#define SCRGB_OUTPUT 1\n",
        (true, _) => "#define LINEAR_DOMAIN 1\n",
        (false, ProgramKey::OutputScRgb) => "#define SCRGB_OUTPUT 1\n",
        (false, _) => "",
    };
    let vertex = format!(
        "{header}{buffer}{COMMON}\n{}{}{vertex_body}",
        structure("VsIn", &inputs, false),
        structure("VsOut", varyings, true),
    );
    // `PsIn` opens with `SV_POSITION` as well: the stages must agree on every register, or
    // the rasteriser feeds each input from the neighbouring one instead of renumbering.
    let fragment = format!(
        "{header}{buffer}{}\n{}{fragment_body}",
        fragment_blocks(key),
        structure("PsIn", varyings, true),
    );

    Program {
        vertex,
        fragment,
        offsets,
        size: cursor * 16,
    }
}
