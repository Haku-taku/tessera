//! The painter's [`Device`] on OpenGL ES (desktop GL for the test binaries).
//!
//! Everything GL-shaped lives here: shader compilation, vertex arrays, framebuffers and
//! texture storage. The scene crate above decides what to draw and in which order.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::rc::Rc;

use glow::HasContext;
use ui_core::draw::Sampling;
use ui_gpu_scene::{
    Device, GpuError, ProgramId, ProgramKey, TargetFormat, TargetId, TextureFormat, TextureId,
    UNIT_QUAD, UniformSlot, Wrap,
};

use crate::shader;

/// Handles are GL object names, so a target a platform allocated binds the same way as one
/// the painter allocated: the name is the whole encoding.
fn framebuffer_of(id: TargetId) -> Option<glow::Framebuffer> {
    u32::try_from(id.0)
        .ok()
        .and_then(NonZeroU32::new)
        .map(glow::NativeFramebuffer)
}

fn texture_of(id: TextureId) -> Option<glow::Texture> {
    u32::try_from(id.0)
        .ok()
        .and_then(NonZeroU32::new)
        .map(glow::NativeTexture)
}

/// The uniforms the scene looks up; a name the shader does not declare is simply absent.
const UNIFORMS: &[&str] = &[
    "u_viewport",
    "u_atlas_size",
    "u_atlas",
    "u_color_atlas_size",
    "u_color_atlas",
    "u_image",
    "u_opacity",
    "u_layer",
    "u_clip_rect",
    "u_clip_radius",
    "u_clip_shape",
    "u_clip_on",
    "u_src",
    "u_texel",
    "u_dir",
    "u_taps",
    "u_radius",
    "u_ss",
    "u_capture",
    "u_region",
    "u_weights",
    "u_color",
    "u_scale",
    "u_offset",
    "u_gamut",
    "u_rect",
    "u_exp",
    "u_skip_empty",
    "u_scrim",
    "u_scrim_hole",
    "u_tint",
    "u_luminosity",
    "u_noise",
    "u_body",
    "u_glass",
    "u_light",
    "u_optical",
    "u_blend",
    "u_kit",
    "u_shape[0]",
    "u_shape_radius[0]",
];

struct GlProgram {
    program: glow::Program,
    vao: glow::VertexArray,
    unit_vbo: glow::Buffer,
    instance_vbo: glow::Buffer,
    /// Bytes per instance, so the draw count never outruns what `draw_instanced` uploaded.
    stride: usize,
    slots: HashMap<&'static str, UniformSlot>,
    locations: Vec<glow::UniformLocation>,
}

/// GL objects the painter allocated through this device.
pub struct GlDevice {
    gl: Rc<glow::Context>,
    is_gles: bool,
    programs: Vec<GlProgram>,
    targets: HashMap<u64, TextureId>,
    textures: HashSet<u64>,
    current: Option<usize>,
    bound_target: Option<TargetId>,
}

impl GlDevice {
    pub fn new(gl: Rc<glow::Context>, is_gles: bool) -> Self {
        Self {
            gl,
            is_gles,
            programs: Vec::new(),
            targets: HashMap::new(),
            textures: HashSet::new(),
            current: None,
            bound_target: None,
        }
    }

    fn location(&self, slot: UniformSlot) -> Option<glow::UniformLocation> {
        let index = self.current?;
        let program = self.programs.get(index)?;
        program.locations.get(slot.0 as usize).copied()
    }

    fn draw_framebuffer(&self) -> Option<glow::Framebuffer> {
        self.bound_target.and_then(framebuffer_of)
    }

    fn internal_color(format: TargetFormat) -> (i32, u32) {
        match format {
            TargetFormat::Rgba8 => (glow::RGBA8 as i32, glow::UNSIGNED_BYTE),
            TargetFormat::Rgba16F => (glow::RGBA16F as i32, glow::HALF_FLOAT),
        }
    }
}

/// `(internal, format, type)` for an uploaded texture's storage.
fn upload_format(format: TextureFormat) -> (i32, u32, u32) {
    match format {
        TextureFormat::R8 => (glow::R8 as i32, glow::RED, glow::UNSIGNED_BYTE),
        TextureFormat::Rgba8 => (glow::RGBA8 as i32, glow::RGBA, glow::UNSIGNED_BYTE),
        TextureFormat::Rgba16F => (glow::RGBA16F as i32, glow::RGBA, glow::HALF_FLOAT),
    }
}

fn upload_storage(
    gl: &glow::Context,
    width: u32,
    height: u32,
    format: TextureFormat,
    data: Option<&[u8]>,
) {
    let (internal, channels, ty) = upload_format(format);
    unsafe {
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            internal,
            width as i32,
            height as i32,
            0,
            channels,
            ty,
            glow::PixelUnpackData::Slice(data),
        );
    }
}

/// Compile one shader stage and report the driver's log verbatim.
fn compile_shader(
    gl: &glow::Context,
    kind: u32,
    source: &str,
    header: &str,
) -> Result<glow::Shader, GpuError> {
    unsafe {
        let shader = gl
            .create_shader(kind)
            .map_err(|error| GpuError::Shader(format!("create shader: {error}")))?;
        // `AA` is spliced into every program from one definition, so the vertex expansion
        // and the fragment ramp cannot drift apart.
        gl.shader_source(shader, &format!("{header}{source}"));
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(GpuError::Shader(format!("shader compile: {log}")));
        }
        Ok(shader)
    }
}

fn link_program(
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
        let program = gl
            .create_program()
            .map_err(|error| GpuError::Shader(format!("create program: {error}")))?;
        let mut shaders = Vec::new();
        let mut failure = None;
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            match compile_shader(gl, kind, &shader::splice(source, shader::AA_GLSL), header) {
                Ok(shader) => {
                    gl.attach_shader(program, shader);
                    shaders.push(shader);
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if failure.is_none() {
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                failure = Some(GpuError::Shader(format!(
                    "program link: {}",
                    gl.get_program_info_log(program)
                )));
            }
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        match failure {
            Some(error) => {
                gl.delete_program(program);
                Err(error)
            }
            None => Ok(program),
        }
    }
}

/// The shader pair for one program key; the linear-domain variants differ only by a define.
fn sources(key: ProgramKey) -> (String, String) {
    let layer = || shader::LAYER_VERTEX.to_string();
    match key {
        ProgramKey::Layers => (layer(), shader::layer_fragment()),
        ProgramKey::Effects => (layer(), shader::EFFECT_FRAGMENT.to_string()),
        ProgramKey::Box => (layer(), shader::BOX_FRAGMENT.to_string()),
        ProgramKey::OutputSrgb => (layer(), shader::OUTPUT_FRAGMENT.to_string()),
        ProgramKey::OutputScRgb => (
            layer(),
            format!("#define SCRGB_OUTPUT\n{}", shader::OUTPUT_FRAGMENT),
        ),
        ProgramKey::Resolve => (layer(), shader::RESOLVE_FRAGMENT.to_string()),
        ProgramKey::Capture => (layer(), shader::CAPTURE_FRAGMENT.to_string()),
        ProgramKey::Rects => (shader::RECT_VERTEX.to_string(), shader::rect_fragment()),
        ProgramKey::Sectors => (shader::SECTOR_VERTEX.to_string(), shader::sector_fragment()),
        ProgramKey::Ellipses => (
            shader::ELLIPSE_VERTEX.to_string(),
            shader::ellipse_fragment(),
        ),
        ProgramKey::Lines => (shader::LINE_VERTEX.to_string(), shader::line_fragment()),
        ProgramKey::Glyphs => (shader::GLYPH_VERTEX.to_string(), shader::glyph_fragment()),
        ProgramKey::Shadows => (shader::SHADOW_VERTEX.to_string(), shader::shadow_fragment()),
        ProgramKey::Images => (shader::IMAGE_VERTEX.to_string(), shader::image_fragment()),
        ProgramKey::Acrylic => (layer(), shader::acrylic_fragment()),
        ProgramKey::GlassKit => (layer(), shader::liquid_glass_fragment()),
    }
}

impl Device for GlDevice {
    fn create_program(&mut self, key: ProgramKey) -> Result<ProgramId, GpuError> {
        let spec = key.spec();
        let (vertex, fragment) = sources(key);
        let gl = Rc::clone(&self.gl);
        let program = link_program(&gl, &vertex, &fragment, self.is_gles)?;
        unsafe {
            let vao = gl
                .create_vertex_array()
                .map_err(|error| GpuError::Device(format!("create vertex array: {error}")))?;
            gl.bind_vertex_array(Some(vao));

            let unit_vbo = gl
                .create_buffer()
                .map_err(|error| GpuError::Device(format!("create buffer: {error}")))?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(unit_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                crate::as_bytes(&UNIT_QUAD),
                glow::STATIC_DRAW,
            );
            if let Some(location) = gl.get_attrib_location(program, "a_unit") {
                gl.enable_vertex_attrib_array(location);
                gl.vertex_attrib_pointer_f32(location, 2, glow::FLOAT, false, 8, 0);
            }

            let instance_vbo = gl
                .create_buffer()
                .map_err(|error| GpuError::Device(format!("create buffer: {error}")))?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_vbo));
            let spare = vec![0.0f32; spec.stride() as usize * 64];
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                crate::as_bytes(&spare),
                glow::DYNAMIC_DRAW,
            );
            let mut offset = 0i32;
            for (name, size) in spec.attributes {
                if let Some(location) = gl.get_attrib_location(program, name) {
                    gl.enable_vertex_attrib_array(location);
                    gl.vertex_attrib_pointer_f32(
                        location,
                        *size,
                        glow::FLOAT,
                        false,
                        spec.stride() * 4,
                        offset * 4,
                    );
                    gl.vertex_attrib_divisor(location, 1);
                }
                offset += size;
            }

            let mut slots = HashMap::new();
            let mut locations = Vec::new();
            for name in UNIFORMS {
                if let Some(location) = gl.get_uniform_location(program, name) {
                    slots.insert(*name, UniformSlot(locations.len() as u32));
                    locations.push(location);
                }
            }
            gl.bind_vertex_array(None);

            // A mistyped uniform name yields `None` and then silently does nothing, so a
            // required uniform missing here is a startup error, not a warning.
            for name in spec.required {
                if !slots.contains_key(name) {
                    gl.delete_program(program);
                    return Err(GpuError::Shader(format!(
                        "shader is missing required uniform `{name}`"
                    )));
                }
            }

            self.programs.push(GlProgram {
                program,
                vao,
                unit_vbo,
                instance_vbo,
                stride: spec.stride() as usize * 4,
                slots,
                locations,
            });
        }
        Ok(ProgramId((self.programs.len() - 1) as u32))
    }

    fn uniform(&self, program: ProgramId, name: &str) -> Option<UniformSlot> {
        self.programs
            .get(program.0 as usize)?
            .slots
            .get(name)
            .copied()
    }

    fn create_target(
        &mut self,
        width: u32,
        height: u32,
        format: TargetFormat,
    ) -> Result<(TargetId, TextureId), GpuError> {
        let (width, height) = (width.max(1), height.max(1));
        let (internal, ty) = Self::internal_color(format);
        let gl = Rc::clone(&self.gl);
        let draw = self.draw_framebuffer();
        unsafe {
            let tex = gl
                .create_texture()
                .map_err(|error| GpuError::Device(format!("create texture: {error}")))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            // The type must be one the internal format accepts even with no data: a
            // half-float target rejects bytes and keeps its old storage.
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                ty,
                glow::PixelUnpackData::Slice(None),
            );
            // A target the backdrop path samples: linear, so a warp may land between texels.
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
            gl.bind_texture(glow::TEXTURE_2D, None);

            let fbo = gl
                .create_framebuffer()
                .map_err(|error| GpuError::Device(format!("create framebuffer: {error}")))?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, draw);

            let target = TargetId(u64::from(fbo.0.get()));
            let texture = TextureId(u64::from(tex.0.get()));
            self.targets.insert(target.0, texture);
            Ok((target, texture))
        }
    }

    fn resize_target(&mut self, target: TargetId, width: u32, height: u32, format: TargetFormat) {
        let (width, height) = (width.max(1), height.max(1));
        let (internal, ty) = Self::internal_color(format);
        let gl = Rc::clone(&self.gl);
        let Some(&slot) = self.targets.get(&target.0) else {
            return;
        };
        let Some(tex) = texture_of(slot) else {
            return;
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                ty,
                glow::PixelUnpackData::Slice(None),
            );
            // Re-storage drops the mip chain, and a mipmap filter over a missing chain makes
            // the texture incomplete: every read returns black.
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    fn create_texture(
        &mut self,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Result<TextureId, GpuError> {
        let gl = Rc::clone(&self.gl);
        unsafe {
            let tex = gl
                .create_texture()
                .map_err(|error| GpuError::Device(format!("create texture: {error}")))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            // A source sub-rect never tiles.
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
            upload_storage(&gl, width.max(1), height.max(1), format, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            let id = TextureId(u64::from(tex.0.get()));
            self.textures.insert(id.0);
            Ok(id)
        }
    }

    fn upload_texture(
        &mut self,
        texture: TextureId,
        width: u32,
        height: u32,
        format: TextureFormat,
        data: &[u8],
    ) {
        let Some(tex) = texture_of(texture) else {
            return;
        };
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            upload_storage(&self.gl, width.max(1), height.max(1), format, Some(data));
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    fn set_sampling(&mut self, texture: TextureId, sampling: Sampling, mips: bool) {
        let Some(tex) = texture_of(texture) else {
            return;
        };
        let filter = match sampling {
            Sampling::Smooth => glow::LINEAR,
            Sampling::Pixelated => glow::NEAREST,
        };
        let min = if mips {
            glow::LINEAR_MIPMAP_LINEAR
        } else {
            filter
        };
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            self.gl
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, min as i32);
            self.gl
                .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
            if mips {
                self.gl.generate_mipmap(glow::TEXTURE_2D);
            }
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        if let Some(tex) = texture_of(texture) {
            unsafe { self.gl.delete_texture(tex) };
        }
        self.textures.remove(&texture.0);
    }

    fn texture_wrap(&self, texture: TextureId) -> (Wrap, Wrap) {
        let Some(tex) = texture_of(texture) else {
            return (Wrap::Clamp, Wrap::Clamp);
        };
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            let mode = |axis| match self.gl.get_tex_parameter_i32(glow::TEXTURE_2D, axis) {
                value if value == glow::REPEAT as i32 => Wrap::Repeat,
                _ => Wrap::Clamp,
            };
            let s = mode(glow::TEXTURE_WRAP_S);
            let t = mode(glow::TEXTURE_WRAP_T);
            self.gl.bind_texture(glow::TEXTURE_2D, None);
            (s, t)
        }
    }

    fn bind_target(&mut self, target: Option<TargetId>) {
        self.bound_target = target;
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, target.and_then(framebuffer_of))
        };
    }

    fn set_viewport(&mut self, width: u32, height: u32) {
        unsafe { self.gl.viewport(0, 0, width as i32, height as i32) };
    }

    fn set_scissor(&mut self, rect: Option<[i32; 4]>) {
        unsafe {
            match rect {
                Some([x, y, width, height]) => {
                    self.gl.enable(glow::SCISSOR_TEST);
                    self.gl.scissor(x, y, width, height);
                }
                None => self.gl.disable(glow::SCISSOR_TEST),
            }
        }
    }

    fn set_blend(&mut self, enabled: bool) {
        unsafe {
            if enabled {
                self.gl.enable(glow::BLEND);
                // Premultiplied source over destination.
                self.gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            } else {
                self.gl.disable(glow::BLEND);
            }
        }
    }

    fn set_clear_color(&mut self, color: [f32; 4]) {
        unsafe { self.gl.clear_color(color[0], color[1], color[2], color[3]) };
    }

    fn clear(&mut self) {
        unsafe { self.gl.clear(glow::COLOR_BUFFER_BIT) };
    }

    fn use_program(&mut self, program: ProgramId) {
        let index = program.0 as usize;
        let Some(entry) = self.programs.get(index) else {
            return;
        };
        unsafe { self.gl.use_program(Some(entry.program)) };
        self.current = Some(index);
    }

    fn bind_texture(&mut self, unit: u32, bound: Option<TextureId>) {
        unsafe {
            self.gl.active_texture(glow::TEXTURE0 + unit);
            self.gl
                .bind_texture(glow::TEXTURE_2D, bound.and_then(texture_of));
        }
    }

    fn set_uniform_f32(&mut self, slot: UniformSlot, value: f32) {
        if let Some(location) = self.location(slot) {
            unsafe { self.gl.uniform_1_f32(Some(&location), value) };
        }
    }

    fn set_uniform_i32(&mut self, slot: UniformSlot, value: i32) {
        if let Some(location) = self.location(slot) {
            unsafe { self.gl.uniform_1_i32(Some(&location), value) };
        }
    }

    fn set_uniform_vec2(&mut self, slot: UniformSlot, value: [f32; 2]) {
        if let Some(location) = self.location(slot) {
            unsafe { self.gl.uniform_2_f32(Some(&location), value[0], value[1]) };
        }
    }

    fn set_uniform_vec4(&mut self, slot: UniformSlot, value: [f32; 4]) {
        if let Some(location) = self.location(slot) {
            unsafe {
                self.gl
                    .uniform_4_f32(Some(&location), value[0], value[1], value[2], value[3])
            };
        }
    }

    fn set_uniform_vec4_slice(&mut self, slot: UniformSlot, values: &[f32]) {
        if let Some(location) = self.location(slot) {
            unsafe { self.gl.uniform_4_f32_slice(Some(&location), values) };
        }
    }

    fn set_uniform_f32_slice(&mut self, slot: UniformSlot, values: &[f32]) {
        if let Some(location) = self.location(slot) {
            unsafe { self.gl.uniform_1_f32_slice(Some(&location), values) };
        }
    }

    fn set_uniform_mat3(&mut self, slot: UniformSlot, values: &[f32; 9]) {
        if let Some(location) = self.location(slot) {
            unsafe {
                self.gl
                    .uniform_matrix_3_f32_slice(Some(&location), false, values)
            };
        }
    }

    fn set_uniform_mat4(&mut self, slot: UniformSlot, values: &[f32; 16]) {
        if let Some(location) = self.location(slot) {
            unsafe {
                self.gl
                    .uniform_matrix_4_f32_slice(Some(&location), false, values)
            };
        }
    }

    fn draw_instanced(&mut self, program: ProgramId, instances: &[u8]) -> Result<(), GpuError> {
        let Some(entry) = self.programs.get(program.0 as usize) else {
            return Err(GpuError::Device(format!("no program {}", program.0)));
        };
        let count = instances.len() / entry.stride;
        if count == 0 {
            return Ok(());
        }
        unsafe {
            self.gl.bind_vertex_array(Some(entry.vao));
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(entry.instance_vbo));
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, instances, glow::DYNAMIC_DRAW);
            self.gl
                .draw_arrays_instanced(glow::TRIANGLES, 0, 6, count as i32);
            self.gl.bind_vertex_array(None);
        }
        Ok(())
    }

    fn blit(&mut self, src: TargetId, dst: TargetId, rect: [i32; 4]) {
        let [x, y, width, height] = rect;
        let draw = self.draw_framebuffer();
        unsafe {
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, framebuffer_of(src));
            self.gl
                .bind_framebuffer(glow::DRAW_FRAMEBUFFER, framebuffer_of(dst));
            self.gl.blit_framebuffer(
                x,
                y,
                x + width,
                y + height,
                x,
                y,
                x + width,
                y + height,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw);
        }
    }

    fn read_pixels(&mut self, target: TargetId, width: u32, height: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
        let draw = self.draw_framebuffer();
        unsafe {
            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, framebuffer_of(target));
            self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            self.gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, draw);
        }
        buf
    }

    fn finish(&mut self) {
        unsafe { self.gl.finish() };
    }
}

impl GlDevice {
    /// Free every GL object this device allocated. Must run while `gl` is current.
    ///
    /// Idempotent, so a caller that has to release before the context dies can call it and
    /// let `Drop` find nothing left.
    pub fn release(&mut self) {
        unsafe {
            for program in self.programs.drain(..) {
                self.gl.delete_program(program.program);
                self.gl.delete_vertex_array(program.vao);
                self.gl.delete_buffer(program.unit_vbo);
                self.gl.delete_buffer(program.instance_vbo);
            }
            for (fbo, tex) in self.targets.drain() {
                if let Some(tex) = texture_of(tex) {
                    self.gl.delete_texture(tex);
                }
                if let Some(fbo) = framebuffer_of(TargetId(fbo)) {
                    self.gl.delete_framebuffer(fbo);
                }
            }
            for id in self.textures.drain() {
                if let Some(tex) = texture_of(TextureId(id)) {
                    self.gl.delete_texture(tex);
                }
            }
        }
        self.current = None;
    }

    /// Give up the objects without deleting them.
    ///
    /// An id only names an object inside the context that created it, so a teardown that
    /// cannot make its own context current must neither delete nor touch them.
    pub fn forget(&mut self) {
        self.programs.clear();
        self.targets.clear();
        self.textures.clear();
        self.current = None;
    }
}

impl Drop for GlDevice {
    fn drop(&mut self) {
        self.release();
    }
}
