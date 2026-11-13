//! The eighteen programs, their instance layouts and their uniform slots.

use crate::device::{Device, ProgramId, UniformSlot};
use crate::error::GpuError;
use crate::layout;

/// One program pair a backend must supply shader sources for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramKey {
    Layers,
    Effects,
    Box,
    OutputSrgb,
    OutputLinear,
    Resolve,
    Capture,
    Rects,
    Sectors,
    Ellipses,
    Lines,
    Glyphs,
    Shadows,
    Images,
    Acrylic,
    AcrylicLinear,
    GlassKit,
    GlassKitLinear,
}

/// What the painter and the backend must agree on for one program.
pub struct ProgramSpec {
    /// Floats per instance.
    pub stride: i32,
    /// Per-instance vertex attributes, in buffer order.
    pub attributes: &'static [(&'static str, i32)],
    /// Uniforms the shader must declare; a missing one is a startup error.
    pub required: &'static [&'static str],
}

const CLIP: &[&str] = &[
    "u_viewport",
    "u_clip_rect",
    "u_clip_radius",
    "u_clip_shape",
    "u_clip_on",
];

const LAYER: &[(&str, i32)] = &[("a_rect", 4)];
const RECT_ATTRS: &[(&str, i32)] = &[
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
];

impl ProgramKey {
    pub fn spec(self) -> ProgramSpec {
        match self {
            Self::Layers => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &[
                    "u_viewport",
                    "u_opacity",
                    "u_layer",
                    "u_rect",
                    "u_radius",
                    "u_exp",
                    "u_skip_empty",
                ],
            },
            Self::Effects => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &[
                    "u_viewport",
                    "u_src",
                    "u_texel",
                    "u_dir",
                    "u_taps",
                    "u_weights",
                    "u_color",
                ],
            },
            Self::Box => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &[
                    "u_viewport",
                    "u_src",
                    "u_texel",
                    "u_dir",
                    "u_radius",
                    "u_color",
                ],
            },
            Self::OutputSrgb | Self::OutputLinear => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &["u_viewport", "u_src", "u_scale", "u_offset", "u_gamut"],
            },
            Self::Resolve => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &["u_viewport", "u_src", "u_ss", "u_encode"],
            },
            Self::Capture => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &["u_viewport", "u_src", "u_region"],
            },
            Self::Rects => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: RECT_ATTRS,
                required: CLIP,
            },
            Self::Sectors => ProgramSpec {
                stride: layout::SECTOR_STRIDE,
                attributes: &[("a_centre", 2), ("a_geom", 4), ("a_fill", 4), ("a_caps", 1)],
                required: CLIP,
            },
            Self::Ellipses => ProgramSpec {
                stride: layout::ELLIPSE_STRIDE,
                attributes: &[
                    ("a_centre", 2),
                    ("a_radii", 2),
                    ("a_width", 1),
                    ("a_fill", 4),
                    ("a_stroke", 4),
                ],
                required: CLIP,
            },
            Self::Lines => ProgramSpec {
                stride: layout::LINE_STRIDE,
                attributes: &[("a_from", 2), ("a_to", 2), ("a_width", 1), ("a_fill", 4)],
                required: CLIP,
            },
            Self::Glyphs => ProgramSpec {
                stride: layout::GLYPH_STRIDE,
                attributes: &[("a_rect", 4), ("a_atlas", 4), ("a_color", 4), ("a_kind", 1)],
                required: &[
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
            },
            Self::Shadows => ProgramSpec {
                stride: layout::SHADOW_STRIDE,
                attributes: &[
                    ("a_rect", 4),
                    ("a_radius", 4),
                    ("a_colour", 4),
                    ("a_soft", 2),
                    ("a_shape", 1),
                    ("a_ring", 1),
                    ("a_cut", 4),
                    ("a_inset", 1),
                ],
                required: CLIP,
            },
            Self::Images => ProgramSpec {
                stride: layout::IMAGE_STRIDE,
                attributes: &[("a_rect", 4), ("a_radius", 4), ("a_uv", 4), ("a_shape", 1)],
                required: &[
                    "u_viewport",
                    "u_image",
                    "u_clip_rect",
                    "u_clip_radius",
                    "u_clip_shape",
                    "u_clip_on",
                ],
            },
            Self::Acrylic | Self::AcrylicLinear => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &[
                    "u_viewport",
                    "u_src",
                    "u_rect",
                    "u_radius",
                    "u_exp",
                    "u_tint",
                    "u_luminosity",
                    "u_noise",
                ],
            },
            Self::GlassKit | Self::GlassKitLinear => ProgramSpec {
                stride: layout::RECT_STRIDE,
                attributes: LAYER,
                required: &[
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
            },
        }
    }
}

pub(crate) struct Program {
    pub(crate) id: ProgramId,
    pub(crate) uniforms: Uniforms,
}

/// Where each uniform the painter sets lives in this program; `None` if it is absent.
pub(crate) struct Uniforms {
    pub(crate) viewport: Option<UniformSlot>,
    pub(crate) atlas_size: Option<UniformSlot>,
    pub(crate) atlas: Option<UniformSlot>,
    pub(crate) color_atlas_size: Option<UniformSlot>,
    pub(crate) color_atlas: Option<UniformSlot>,
    pub(crate) image: Option<UniformSlot>,
    pub(crate) opacity: Option<UniformSlot>,
    pub(crate) layer_tex: Option<UniformSlot>,
    pub(crate) clip_rect: Option<UniformSlot>,
    pub(crate) clip_radius: Option<UniformSlot>,
    pub(crate) clip_shape: Option<UniformSlot>,
    pub(crate) clip_on: Option<UniformSlot>,
    pub(crate) src: Option<UniformSlot>,
    pub(crate) texel: Option<UniformSlot>,
    pub(crate) dir: Option<UniformSlot>,
    pub(crate) taps: Option<UniformSlot>,
    pub(crate) radius: Option<UniformSlot>,
    pub(crate) supersample: Option<UniformSlot>,
    pub(crate) encode: Option<UniformSlot>,
    pub(crate) capture: Option<UniformSlot>,
    pub(crate) region: Option<UniformSlot>,
    pub(crate) weights: Option<UniformSlot>,
    pub(crate) color: Option<UniformSlot>,
    pub(crate) scale: Option<UniformSlot>,
    pub(crate) offset: Option<UniformSlot>,
    pub(crate) gamut: Option<UniformSlot>,
    pub(crate) layer_rect: Option<UniformSlot>,
    pub(crate) layer_radius: Option<UniformSlot>,
    pub(crate) layer_exp: Option<UniformSlot>,
    pub(crate) layer_skip_empty: Option<UniformSlot>,
    pub(crate) tint: Option<UniformSlot>,
    pub(crate) luminosity: Option<UniformSlot>,
    pub(crate) noise: Option<UniformSlot>,
    pub(crate) body: Option<UniformSlot>,
    pub(crate) glass: Option<UniformSlot>,
    pub(crate) light: Option<UniformSlot>,
    pub(crate) optical: Option<UniformSlot>,
    pub(crate) blend: Option<UniformSlot>,
    pub(crate) kit: Option<UniformSlot>,
    pub(crate) shapes: Option<UniformSlot>,
    pub(crate) shape_radius: Option<UniformSlot>,
}

impl Uniforms {
    fn query(device: &dyn Device, program: ProgramId) -> Self {
        let slot = |name: &str| device.uniform(program, name);
        Self {
            viewport: slot("u_viewport"),
            atlas_size: slot("u_atlas_size"),
            atlas: slot("u_atlas"),
            color_atlas_size: slot("u_color_atlas_size"),
            color_atlas: slot("u_color_atlas"),
            image: slot("u_image"),
            opacity: slot("u_opacity"),
            layer_tex: slot("u_layer"),
            clip_rect: slot("u_clip_rect"),
            clip_radius: slot("u_clip_radius"),
            clip_shape: slot("u_clip_shape"),
            clip_on: slot("u_clip_on"),
            src: slot("u_src"),
            texel: slot("u_texel"),
            dir: slot("u_dir"),
            taps: slot("u_taps"),
            radius: slot("u_radius"),
            supersample: slot("u_ss"),
            encode: slot("u_encode"),
            capture: slot("u_capture"),
            region: slot("u_region"),
            weights: slot("u_weights"),
            color: slot("u_color"),
            scale: slot("u_scale"),
            offset: slot("u_offset"),
            gamut: slot("u_gamut"),
            layer_rect: slot("u_rect"),
            layer_radius: slot("u_radius"),
            layer_exp: slot("u_exp"),
            layer_skip_empty: slot("u_skip_empty"),
            tint: slot("u_tint"),
            luminosity: slot("u_luminosity"),
            noise: slot("u_noise"),
            body: slot("u_body"),
            glass: slot("u_glass"),
            light: slot("u_light"),
            optical: slot("u_optical"),
            blend: slot("u_blend"),
            kit: slot("u_kit"),
            shapes: slot("u_shape[0]"),
            shape_radius: slot("u_shape_radius[0]"),
        }
    }
}

pub(crate) fn new_program(device: &mut dyn Device, key: ProgramKey) -> Result<Program, GpuError> {
    let id = device.create_program(key)?;
    Ok(Program {
        id,
        uniforms: Uniforms::query(device, id),
    })
}
