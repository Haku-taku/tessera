//! The painter's [`Device`] on Direct3D 11.
//!
//! Shaders are compiled at runtime by d3dcompiler, so a shader edit needs no build step and
//! the driver's own compiler is the one that judges them.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::OnceLock;

use ui_gpu_scene::{
    Device, GpuError, MAX_TAPS, ProgramId, ProgramKey, Sampling, TargetFormat, TargetId,
    TextureFormat, TextureId, UNIT_QUAD, UniformSlot, Wrap, as_bytes,
};
use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0,
    D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, ID3DBlob,
};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R32G32_FLOAT,
    DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::core::{Interface, s};

use crate::shader;

/// The one pass that is not the scene's: a finished frame into a swap chain buffer.
const PRESENT_VERTEX: &str = include_str!("shaders/present.vert.hlsl");
const PRESENT_FRAGMENT: &str = include_str!("shaders/present.frag.hlsl");

/// Instances the buffer holds before it has to grow.
const MIN_INSTANCES: u32 = 64;

fn format_of(format: TargetFormat) -> DXGI_FORMAT {
    match format {
        TargetFormat::Rgba8 => DXGI_FORMAT_R8G8B8A8_UNORM,
        TargetFormat::Rgba16F => DXGI_FORMAT_R16G16B16A16_FLOAT,
    }
}

fn upload_format(format: TextureFormat) -> DXGI_FORMAT {
    match format {
        TextureFormat::R8 => DXGI_FORMAT_R8_UNORM,
        TextureFormat::Rgba8 => DXGI_FORMAT_R8G8B8A8_UNORM,
        TextureFormat::Rgba16F => DXGI_FORMAT_R16G16B16A16_FLOAT,
    }
}

/// Bytes per texel of an upload format; the scene packs rows without padding.
fn texel_bytes(format: TextureFormat) -> u32 {
    match format {
        TextureFormat::R8 => 1,
        TextureFormat::Rgba8 => 4,
        TextureFormat::Rgba16F => 8,
    }
}

fn view_format(size: u32) -> DXGI_FORMAT {
    match size {
        1 => DXGI_FORMAT_R32_FLOAT,
        2 => DXGI_FORMAT_R32G32_FLOAT,
        3 => DXGI_FORMAT_R32G32B32_FLOAT,
        _ => DXGI_FORMAT_R32G32B32A32_FLOAT,
    }
}

/// Report a `Create*` call's own failure, or the missing object when it did not fail.
fn created<T>(
    result: windows::core::Result<()>,
    value: Option<T>,
    what: &str,
) -> Result<T, GpuError> {
    result.map_err(|error| GpuError::Device(format!("{what}: {error}")))?;
    value.ok_or_else(|| GpuError::Device(format!("{what}: no object returned")))
}

/// One compiled program: both stages, the instance layout, and the constants it reads.
struct Program {
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    unit: ID3D11Buffer,
    instances: ID3D11Buffer,
    /// Instances `instances` can hold.
    capacity: u32,
    stride: u32,
    constants: ID3D11Buffer,
    /// CPU mirror of `constants`, written by the uniform setters and uploaded per draw.
    data: Vec<u8>,
    slots: Vec<(String, u32)>,
}

/// The storage format of a sampled texture, kept so a re-upload can compare.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Storage {
    Target(TargetFormat),
    Texture(TextureFormat),
}

impl Storage {
    fn dxgi(self) -> DXGI_FORMAT {
        match self {
            Self::Target(format) => format_of(format),
            Self::Texture(format) => upload_format(format),
        }
    }
}

/// A texture together with the view that samples it.
struct Sampled {
    texture: ID3D11Texture2D,
    srv: ID3D11ShaderResourceView,
    width: u32,
    height: u32,
    storage: Storage,
    sampling: Sampling,
    mips: bool,
    wrap: (Wrap, Wrap),
}

struct Target {
    texture: ID3D11Texture2D,
    view: ID3D11RenderTargetView,
    width: u32,
    height: u32,
    /// `None` for a texture the caller owns: the painter draws into it but cannot resize it.
    format: Option<TargetFormat>,
    /// The storage format; a swap chain buffer has no `TargetFormat` of its own.
    dxgi: DXGI_FORMAT,
    /// The id this target's texture is sampled under, when the texture is sampleable.
    sampled: Option<TextureId>,
}

/// The pass that copies a finished frame into the presented buffer.
///
/// A swap chain's back buffer is usually `B8G8R8A8`, and a raw copy would leave red and blue
/// exchanged; drawing through a render target view lets the view's format order the channels.
struct PresentCopy {
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SamplerKey {
    sampling: Sampling,
    mips: bool,
    /// Bit 0 tiles on u, bit 1 on v.
    repeat: u8,
}

pub struct D3d11Device {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    /// The same context as D3D11.1, whose `ClearView` is the only scissored clear there is.
    enhanced: Option<ID3D11DeviceContext1>,
    programs: Vec<Program>,
    targets: HashMap<u64, Target>,
    sampled: HashMap<u64, Sampled>,
    samplers: HashMap<SamplerKey, ID3D11SamplerState>,
    next_texture: u64,
    next_target: u64,
    current: Option<usize>,
    bound: Option<u64>,
    viewport: (u32, u32),
    scissor: Option<[i32; 4]>,
    clear_color: [f32; 4],
    raster: ID3D11RasterizerState,
    blend_on: ID3D11BlendState,
    blend_off: ID3D11BlendState,
    /// A 1x1 default texture and its staging copy, used to wait for the GPU in `finish`.
    sync: Option<(ID3D11Texture2D, ID3D11Texture2D)>,
    /// Built on the first present; most devices never present through this one.
    present: Option<PresentCopy>,
    /// A failure inside a `Device` method whose signature has no `Result`.
    pending: Option<String>,
}

impl D3d11Device {
    pub fn new() -> Result<Self, GpuError> {
        let (device, context) = create_device()?;
        let raster = create_raster(&device)?;
        let blend_on = create_blend(&device, true)?;
        let blend_off = create_blend(&device, false)?;
        let enhanced = context.cast::<ID3D11DeviceContext1>().ok();
        let mut this = Self {
            device,
            context,
            enhanced,
            programs: Vec::new(),
            targets: HashMap::new(),
            sampled: HashMap::new(),
            samplers: HashMap::new(),
            next_texture: 1,
            next_target: 1,
            current: None,
            bound: None,
            viewport: (0, 0),
            scissor: None,
            clear_color: [0.0; 4],
            raster,
            blend_on,
            blend_off,
            sync: None,
            present: None,
            pending: None,
        };
        unsafe {
            let context = &this.context;
            context.OMSetBlendState(&this.blend_on, None, u32::MAX);
            context.RSSetState(&this.raster);
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        }
        this.apply_scissor();
        Ok(this)
    }

    fn fail(&mut self, error: GpuError) {
        self.pending = Some(error.to_string());
    }

    /// Report a failure that happened inside a method whose signature has no `Result`.
    fn take_pending(&mut self) -> Result<(), GpuError> {
        match self.pending.take() {
            Some(message) => Err(GpuError::Device(message)),
            None => Ok(()),
        }
    }

    fn target_size(&self) -> (u32, u32) {
        self.bound
            .and_then(|id| self.targets.get(&id))
            .map(|target| (target.width, target.height))
            .unwrap_or(self.viewport)
    }

    fn apply_scissor(&mut self) {
        let rect = match self.scissor {
            Some([x, y, width, height]) => RECT {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            },
            None => {
                let (width, height) = self.target_size();
                RECT {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                }
            }
        };
        unsafe { self.context.RSSetScissorRects(Some(&[rect])) };
    }

    fn texture(
        &self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<ID3D11Texture2D, GpuError> {
        // `GENERATE_MIPS` requires both bindings in D3D11, and every texture carries a full
        // chain: `set_sampling` may ask for mips on any of them, and a resource without the
        // chain makes `GenerateMips` a no-op.
        let bind = (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width.max(1),
            Height: height.max(1),
            MipLevels: 0,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        };
        let mut texture = None;
        let result = unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) };
        created(result, texture, "texture")
    }

    fn sampled_view(
        &self,
        width: u32,
        height: u32,
        storage: Storage,
    ) -> Result<(ID3D11Texture2D, ID3D11ShaderResourceView), GpuError> {
        let texture = self.texture(width, height, storage.dxgi())?;
        let mut srv = None;
        let result = unsafe {
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut srv))
        };
        Ok((texture, created(result, srv, "shader resource view")?))
    }

    fn buffer(
        &self,
        desc: &D3D11_BUFFER_DESC,
        data: Option<&[u8]>,
    ) -> Result<ID3D11Buffer, GpuError> {
        let subresource = data.map(|bytes| D3D11_SUBRESOURCE_DATA {
            pSysMem: bytes.as_ptr() as *const c_void,
            SysMemPitch: 0,
            SysMemSlicePitch: 0,
        });
        let mut buffer = None;
        let result = unsafe {
            self.device.CreateBuffer(
                desc,
                subresource.as_ref().map(|data| data as *const _),
                Some(&mut buffer),
            )
        };
        created(result, buffer, "buffer")
    }

    fn dynamic_buffer(&self, bytes: u32, bind: u32) -> Result<ID3D11Buffer, GpuError> {
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: bytes.max(16).div_ceil(16) * 16,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: bind,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        self.buffer(&desc, None)
    }

    fn sampler(&mut self, key: SamplerKey) -> Result<ID3D11SamplerState, GpuError> {
        if let Some(state) = self.samplers.get(&key) {
            return Ok(state.clone());
        }
        let filter = match (key.sampling, key.mips) {
            (Sampling::Smooth, true) => D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            (Sampling::Smooth, false) => D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            (Sampling::Pixelated, true) => D3D11_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR,
            (Sampling::Pixelated, false) => D3D11_FILTER_MIN_MAG_MIP_POINT,
        };
        let address = |tiles: bool| {
            if tiles {
                D3D11_TEXTURE_ADDRESS_WRAP
            } else {
                D3D11_TEXTURE_ADDRESS_CLAMP
            }
        };
        let desc = D3D11_SAMPLER_DESC {
            Filter: filter,
            AddressU: address(key.repeat & 1 != 0),
            AddressV: address(key.repeat & 2 != 0),
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            // Every texture here carries a full chain, but only a caller that asked for mips
            // writes the lower levels; sampling further would read a level nobody generated.
            MaxLOD: if key.mips { f32::MAX } else { 0.0 },
        };
        let mut state = None;
        let result = unsafe { self.device.CreateSamplerState(&desc, Some(&mut state)) };
        let state = created(result, state, "sampler state")?;
        self.samplers.insert(key, state.clone());
        Ok(state)
    }

    fn write(&mut self, slot: UniformSlot, bytes: &[u8]) {
        let Some(index) = self.current else { return };
        let Some(program) = self.programs.get_mut(index) else {
            return;
        };
        let Some(&(_, offset)) = program.slots.get(slot.0 as usize) else {
            return;
        };
        let start = offset as usize;
        let Some(destination) = program.data.get_mut(start..start + bytes.len()) else {
            return;
        };
        destination.copy_from_slice(bytes);
    }

    fn sync_pair(&mut self) -> Option<(ID3D11Texture2D, ID3D11Texture2D)> {
        if let Some(pair) = &self.sync {
            return Some(pair.clone());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: 1,
            Height: 1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: 0,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        let result = unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) };
        let texture = match created(result, texture, "sync texture") {
            Ok(texture) => texture,
            Err(error) => {
                self.fail(error);
                return None;
            }
        };
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..desc
        };
        let mut staging = None;
        let result = unsafe {
            self.device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))
        };
        let staging = match created(result, staging, "sync staging texture") {
            Ok(staging) => staging,
            Err(error) => {
                self.fail(error);
                return None;
            }
        };
        self.sync = Some((texture.clone(), staging.clone()));
        Some((texture, staging))
    }

    /// The device itself, for a presenter that has to create its own swap chain.
    pub fn raw_device(&self) -> &ID3D11Device {
        &self.device
    }

    /// Register a texture the caller owns — a swap chain's back buffer — as a target.
    ///
    /// The painter may draw into it and read it back, but never resize it: the owner
    /// re-points it with [`Self::retarget`] after its own resize.
    pub fn adopt_target(
        &mut self,
        texture: &ID3D11Texture2D,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<TargetId, GpuError> {
        let (width, height) = (width.max(1), height.max(1));
        let view = render_target_view(&self.device, texture)?;
        let target = TargetId(self.next_target);
        self.next_target += 1;
        self.targets.insert(
            target.0,
            Target {
                texture: texture.clone(),
                view,
                width,
                height,
                format: None,
                dxgi: format,
                sampled: None,
            },
        );
        Ok(target)
    }

    /// Point an adopted target at a replacement texture, keeping the same handle.
    ///
    /// A flip-model swap chain hands a different buffer to each frame, and a resize creates
    /// new ones; the target id survives both so a presenter can keep one field.
    pub fn retarget(
        &mut self,
        target: TargetId,
        texture: &ID3D11Texture2D,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<(), GpuError> {
        let (width, height) = (width.max(1), height.max(1));
        let view = render_target_view(&self.device, texture)?;
        let Some(entry) = self.targets.get_mut(&target.0) else {
            return Err(GpuError::Device(format!("no target {}", target.0)));
        };
        if entry.format.is_some() {
            return Err(GpuError::Device(String::from(
                "retarget names a target the painter allocated",
            )));
        }
        entry.texture = texture.clone();
        entry.view = view;
        entry.width = width;
        entry.height = height;
        entry.dxgi = format;
        Ok(())
    }

    /// Release an adopted target and every reference it holds.
    ///
    /// A swap chain cannot resize while a back buffer is still referenced, so its owner
    /// calls this for each buffer before `ResizeBuffers`.
    pub fn release_target(&mut self, target: TargetId) {
        if self.bound == Some(target.0) {
            self.bind_target(None);
        }
        self.targets.remove(&target.0);
    }

    /// Draw `src` into `dst` through the pipeline, so the destination's own format decides
    /// channel order. This is how a finished frame reaches a `B8G8R8A8` swap chain buffer.
    ///
    /// `dst` is left unbound: the swap chain must hold no reference when it resizes.
    pub fn present_copy(&mut self, src: TargetId, dst: TargetId) -> Result<(), GpuError> {
        self.take_pending()?;
        let (view, width, height) = match self.targets.get(&dst.0) {
            Some(entry) => (entry.view.clone(), entry.width, entry.height),
            None => {
                return Err(GpuError::Device(format!(
                    "no present target {}",
                    dst.0
                )));
            }
        };
        let srv = self
            .targets
            .get(&src.0)
            .and_then(|entry| entry.sampled)
            .and_then(|id| self.sampled.get(&id.0))
            .map(|entry| entry.srv.clone())
            .ok_or_else(|| {
                GpuError::Device(String::from("the present source is not sampleable"))
            })?;
        let program = self.present_program()?;
        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        let rect = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        // SAFETY: the view, srv and program objects live in this device, the viewport and
        // scissor name the target's own extent, and the draw reads no vertex buffer.
        unsafe {
            let context = &self.context;
            context.OMSetRenderTargets(Some(&[Some(view)]), None);
            context.RSSetViewports(Some(&[viewport]));
            context.RSSetScissorRects(Some(&[rect]));
            context.OMSetBlendState(&self.blend_off, None, u32::MAX);
            context.IASetInputLayout(None::<&ID3D11InputLayout>);
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&program.vertex, None);
            context.PSSetShader(&program.pixel, None);
            context.PSSetShaderResources(0, Some(&[Some(srv)]));
            context.PSSetSamplers(0, Some(&[Some(program.sampler)]));
            context.Draw(3, 0);
            context.PSSetShaderResources(0, Some(&[None]));
            context.OMSetRenderTargets(None, None);
        }
        self.bound = None;
        self.scissor = None;
        self.apply_scissor();
        // The painter sets its own blend state per pass, but the copy must not leave the
        // opaque one armed for a frame that assumes source-over.
        unsafe { self.context.OMSetBlendState(&self.blend_on, None, u32::MAX) };
        Ok(())
    }

    /// Compile the present copy once; every presented frame reuses it.
    fn present_program(&mut self) -> Result<PresentCopy, GpuError> {
        if let Some(program) = &self.present {
            return Ok(PresentCopy {
                vertex: program.vertex.clone(),
                pixel: program.pixel.clone(),
                sampler: program.sampler.clone(),
            });
        }
        let vertex_code = compile("present.vert", PRESENT_VERTEX, s!("main"), s!("vs_5_0"))?;
        let pixel_code = compile("present.frag", PRESENT_FRAGMENT, s!("main"), s!("ps_5_0"))?;
        let mut vertex = None;
        let result = unsafe {
            self.device
                .CreateVertexShader(&vertex_code, None, Some(&mut vertex))
        };
        let vertex = created(result, vertex, "present vertex shader")?;
        let mut pixel = None;
        let result = unsafe {
            self.device
                .CreatePixelShader(&pixel_code, None, Some(&mut pixel))
        };
        let pixel = created(result, pixel, "present pixel shader")?;
        // A 1:1 copy: linear filtering is exact at this scale and mip 0 is what it picks.
        let sampler = self.sampler(SamplerKey {
            sampling: Sampling::Smooth,
            mips: true,
            repeat: 0,
        })?;
        self.present = Some(PresentCopy {
            vertex: vertex.clone(),
            pixel: pixel.clone(),
            sampler: sampler.clone(),
        });
        Ok(PresentCopy {
            vertex,
            pixel,
            sampler,
        })
    }
}

impl Device for D3d11Device {
    fn create_program(&mut self, key: ProgramKey) -> Result<ProgramId, GpuError> {
        let spec = key.spec();
        // The reflection half only: the sources go through the process-wide cache.
        let source = shader::build(key);
        let (vertex_code, pixel_code) = bytecode(key)
            .as_ref()
            .map_err(|message| GpuError::Shader(message.clone()))?;

        let mut elements = vec![D3D11_INPUT_ELEMENT_DESC {
            SemanticName: s!("TEXCOORD"),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        }];
        let mut offset = 0u32;
        for (index, (_, size)) in spec.attributes.iter().enumerate() {
            elements.push(D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: index as u32 + 1,
                Format: view_format(*size as u32),
                InputSlot: 1,
                AlignedByteOffset: offset,
                InputSlotClass: D3D11_INPUT_PER_INSTANCE_DATA,
                InstanceDataStepRate: 1,
            });
            offset += *size as u32 * 4;
        }

        let mut vertex = None;
        let result = unsafe {
            self.device
                .CreateVertexShader(vertex_code, None, Some(&mut vertex))
        };
        let vertex = created(result, vertex, "vertex shader")?;
        let mut pixel = None;
        let result = unsafe {
            self.device
                .CreatePixelShader(pixel_code, None, Some(&mut pixel))
        };
        let pixel = created(result, pixel, "pixel shader")?;
        let mut layout = None;
        let result = unsafe {
            self.device
                .CreateInputLayout(&elements, vertex_code, Some(&mut layout))
        };
        let layout = created(result, layout, "input layout")?;

        let unit = self.buffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of_val(&UNIT_QUAD) as u32,
                Usage: D3D11_USAGE_IMMUTABLE,
                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
                StructureByteStride: 0,
            },
            Some(as_bytes(&UNIT_QUAD)),
        )?;
        let stride = spec.stride as u32 * 4;
        let instances =
            self.dynamic_buffer(MIN_INSTANCES * stride, D3D11_BIND_VERTEX_BUFFER.0 as u32)?;
        let constants = self.dynamic_buffer(source.size, D3D11_BIND_CONSTANT_BUFFER.0 as u32)?;

        // A mistyped uniform name yields `None` and then silently does nothing, so a
        // required uniform missing here is a startup error, not a warning.
        for name in spec.required {
            if slot_index(&source.offsets, name).is_none() {
                return Err(GpuError::Shader(format!(
                    "shader is missing required uniform `{name}`"
                )));
            }
        }

        self.programs.push(Program {
            vertex,
            pixel,
            layout,
            unit,
            instances,
            capacity: MIN_INSTANCES,
            stride,
            constants,
            data: vec![0; source.size as usize],
            slots: source.offsets,
        });
        Ok(ProgramId((self.programs.len() - 1) as u32))
    }

    fn uniform(&self, program: ProgramId, name: &str) -> Option<UniformSlot> {
        let program = self.programs.get(program.0 as usize)?;
        let index = slot_index(&program.slots, name)?;
        Some(UniformSlot(index as u32))
    }

    fn create_target(
        &mut self,
        width: u32,
        height: u32,
        format: TargetFormat,
    ) -> Result<(TargetId, TextureId), GpuError> {
        let (width, height) = (width.max(1), height.max(1));
        let storage = Storage::Target(format);
        let texture = self.texture(width, height, storage.dxgi())?;
        let mut view = None;
        let result = unsafe {
            self.device
                .CreateRenderTargetView(&texture, None, Some(&mut view))
        };
        let view = created(result, view, "render target view")?;
        let mut srv = None;
        let result = unsafe {
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut srv))
        };
        let srv = created(result, srv, "shader resource view")?;

        let sampled = TextureId(self.next_texture);
        self.next_texture += 1;
        self.sampled.insert(
            sampled.0,
            Sampled {
                texture: texture.clone(),
                srv,
                width,
                height,
                storage,
                sampling: Sampling::Smooth,
                mips: true,
                wrap: (Wrap::Clamp, Wrap::Clamp),
            },
        );
        let target = TargetId(self.next_target);
        self.next_target += 1;
        self.targets.insert(
            target.0,
            Target {
                texture,
                view,
                width,
                height,
                format: Some(format),
                dxgi: format_of(format),
                sampled: Some(sampled),
            },
        );
        Ok((target, sampled))
    }

    fn resize_target(&mut self, target: TargetId, width: u32, height: u32, format: TargetFormat) {
        let (width, height) = (width.max(1), height.max(1));
        let Some(sampled) = self.targets.get(&target.0).and_then(|entry| entry.sampled) else {
            return;
        };
        let texture = match self.texture(width, height, format_of(format)) {
            Ok(texture) => texture,
            Err(error) => return self.fail(error),
        };
        let mut view = None;
        let result = unsafe {
            self.device
                .CreateRenderTargetView(&texture, None, Some(&mut view))
        };
        let view = match created(result, view, "render target view") {
            Ok(view) => view,
            Err(error) => return self.fail(error),
        };
        let mut srv = None;
        let result = unsafe {
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut srv))
        };
        let srv = match created(result, srv, "shader resource view") {
            Ok(srv) => srv,
            Err(error) => return self.fail(error),
        };
        if let Some(entry) = self.targets.get_mut(&target.0) {
            entry.texture = texture.clone();
            entry.view = view;
            entry.width = width;
            entry.height = height;
            entry.format = Some(format);
            entry.dxgi = format_of(format);
        }
        if let Some(entry) = self.sampled.get_mut(&sampled.0) {
            entry.texture = texture;
            entry.srv = srv;
            entry.width = width;
            entry.height = height;
            entry.storage = Storage::Target(format);
        }
    }

    fn create_texture(
        &mut self,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Result<TextureId, GpuError> {
        let (width, height) = (width.max(1), height.max(1));
        let storage = Storage::Texture(format);
        let (texture, srv) = self.sampled_view(width, height, storage)?;
        let id = TextureId(self.next_texture);
        self.next_texture += 1;
        self.sampled.insert(
            id.0,
            Sampled {
                texture,
                srv,
                width,
                height,
                storage,
                sampling: Sampling::Smooth,
                mips: false,
                wrap: (Wrap::Clamp, Wrap::Clamp),
            },
        );
        Ok(id)
    }

    fn upload_texture(
        &mut self,
        texture: TextureId,
        width: u32,
        height: u32,
        format: TextureFormat,
        data: &[u8],
    ) {
        let (width, height) = (width.max(1), height.max(1));
        let storage = Storage::Texture(format);
        let row = width * texel_bytes(format);
        if data.len() < (row * height) as usize {
            return self.fail(GpuError::Device(format!(
                "upload of {width}x{height} needs {} bytes, {} given",
                row * height,
                data.len()
            )));
        }
        let grow = match self.sampled.get(&texture.0) {
            Some(entry) => {
                entry.width != width || entry.height != height || entry.storage != storage
            }
            None => return,
        };
        if grow {
            let (replacement, srv) = match self.sampled_view(width, height, storage) {
                Ok(pair) => pair,
                Err(error) => return self.fail(error),
            };
            if let Some(entry) = self.sampled.get_mut(&texture.0) {
                entry.texture = replacement;
                entry.srv = srv;
                entry.width = width;
                entry.height = height;
                entry.storage = storage;
            }
        }
        let Some(entry) = self.sampled.get(&texture.0) else {
            return;
        };
        let mips = entry.mips;
        let srv = entry.srv.clone();
        // SAFETY: the length check above covers every row this upload reads.
        unsafe {
            self.context.UpdateSubresource(
                &entry.texture,
                0,
                None,
                data.as_ptr() as *const c_void,
                row,
                0,
            );
            if mips {
                self.context.GenerateMips(&srv);
            }
        }
    }

    fn texture_wrap(&self, texture: TextureId) -> (Wrap, Wrap) {
        self.sampled
            .get(&texture.0)
            .map(|entry| entry.wrap)
            .unwrap_or((Wrap::Clamp, Wrap::Clamp))
    }

    fn set_sampling(&mut self, texture: TextureId, sampling: Sampling, mips: bool) {
        let srv = match self.sampled.get_mut(&texture.0) {
            Some(entry) => {
                entry.sampling = sampling;
                entry.mips = mips;
                entry.srv.clone()
            }
            None => return,
        };
        if mips {
            unsafe { self.context.GenerateMips(&srv) };
        }
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        self.sampled.remove(&texture.0);
    }

    fn bind_target(&mut self, target: Option<TargetId>) {
        self.bound = target.map(|id| id.0);
        let view = target
            .and_then(|id| self.targets.get(&id.0))
            .map(|entry| entry.view.clone());
        match view {
            Some(view) => unsafe { self.context.OMSetRenderTargets(Some(&[Some(view)]), None) },
            None => unsafe { self.context.OMSetRenderTargets(None, None) },
        }
        self.apply_scissor();
    }

    fn set_viewport(&mut self, width: u32, height: u32) {
        self.viewport = (width, height);
        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        unsafe { self.context.RSSetViewports(Some(&[viewport])) };
    }

    fn set_scissor(&mut self, rect: Option<[i32; 4]>) {
        self.scissor = rect;
        self.apply_scissor();
    }

    fn set_blend(&mut self, enabled: bool) {
        let state = if enabled {
            &self.blend_on
        } else {
            &self.blend_off
        };
        unsafe { self.context.OMSetBlendState(state, None, u32::MAX) };
    }

    fn set_clear_color(&mut self, color: [f32; 4]) {
        self.clear_color = color;
    }

    fn clear(&mut self) {
        let Some(target) = self.bound.and_then(|id| self.targets.get(&id)) else {
            return;
        };
        let color = self.clear_color;
        // A plain clear ignores the scissor, but the damage band depends on it: only
        // `ClearView` bounds a clear to a rect.
        if let (Some([x, y, width, height]), Some(context)) = (self.scissor, &self.enhanced)
            && let Ok(view) = target.view.cast::<ID3D11View>()
        {
            let rect = RECT {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            unsafe { context.ClearView(&view, &color, Some(&[rect])) };
            return;
        }
        unsafe { self.context.ClearRenderTargetView(&target.view, &color) };
    }

    fn use_program(&mut self, program: ProgramId) {
        let index = program.0 as usize;
        let Some(entry) = self.programs.get(index) else {
            return;
        };
        unsafe {
            let context = &self.context;
            context.IASetInputLayout(&entry.layout);
            context.VSSetShader(&entry.vertex, None);
            context.PSSetShader(&entry.pixel, None);
            context.VSSetConstantBuffers(0, Some(&[Some(entry.constants.clone())]));
            context.PSSetConstantBuffers(0, Some(&[Some(entry.constants.clone())]));
        }
        self.current = Some(index);
    }

    fn bind_texture(&mut self, unit: u32, texture: Option<TextureId>) {
        let Some(texture) = texture else {
            unsafe {
                let context = &self.context;
                context.PSSetShaderResources(unit, Some(&[None]));
                context.PSSetSamplers(unit, Some(&[None]));
            }
            return;
        };
        let key = match self.sampled.get(&texture.0) {
            Some(entry) => SamplerKey {
                sampling: entry.sampling,
                mips: entry.mips,
                repeat: u8::from(entry.wrap.0 == Wrap::Repeat)
                    | (u8::from(entry.wrap.1 == Wrap::Repeat) << 1),
            },
            None => return,
        };
        let sampler = match self.sampler(key) {
            Ok(sampler) => sampler,
            Err(error) => return self.fail(error),
        };
        let Some(entry) = self.sampled.get(&texture.0) else {
            return;
        };
        let srv = entry.srv.clone();
        unsafe {
            let context = &self.context;
            context.PSSetShaderResources(unit, Some(&[Some(srv)]));
            context.PSSetSamplers(unit, Some(&[Some(sampler)]));
        }
    }

    fn set_uniform_f32(&mut self, slot: UniformSlot, value: f32) {
        self.write(slot, &value.to_ne_bytes());
    }

    fn set_uniform_i32(&mut self, slot: UniformSlot, value: i32) {
        self.write(slot, &value.to_ne_bytes());
    }

    fn set_uniform_vec2(&mut self, slot: UniformSlot, value: [f32; 2]) {
        self.write(slot, as_bytes(&value));
    }

    fn set_uniform_vec4(&mut self, slot: UniformSlot, value: [f32; 4]) {
        self.write(slot, as_bytes(&value));
    }

    fn set_uniform_vec4_slice(&mut self, slot: UniformSlot, values: &[f32]) {
        self.write(slot, as_bytes(values));
    }

    fn set_uniform_f32_slice(&mut self, slot: UniformSlot, values: &[f32]) {
        // The member is `MAX_TAPS` floats wide; a longer kernel would run past its registers.
        let values = &values[..values.len().min(MAX_TAPS as usize)];
        self.write(slot, as_bytes(values));
    }

    fn set_uniform_mat3(&mut self, slot: UniformSlot, values: &[f32; 9]) {
        // One register per column, each padded to four floats: HLSL packs cbuffer matrices
        // column-major and the painter hands them over in that order.
        let mut packed = [0.0f32; 12];
        for column in 0..3 {
            packed[column * 4..column * 4 + 3].copy_from_slice(&values[column * 3..column * 3 + 3]);
        }
        self.write(slot, as_bytes(&packed));
    }

    fn set_uniform_mat4(&mut self, slot: UniformSlot, values: &[f32; 16]) {
        // Four columns of four, which is exactly what column-major packing wants.
        self.write(slot, as_bytes(values));
    }

    fn draw_instanced(
        &mut self,
        program: ProgramId,
        instances: &[u8],
        count: u32,
    ) -> Result<(), GpuError> {
        self.take_pending()?;
        let index = program.0 as usize;
        let Some(entry) = self.programs.get(index) else {
            return Err(GpuError::Device(format!("no program {}", program.0)));
        };
        let stride = entry.stride;
        let capacity = entry.capacity;
        if count > capacity {
            let capacity = capacity.max(count).next_power_of_two();
            let buffer =
                self.dynamic_buffer(capacity * stride, D3D11_BIND_VERTEX_BUFFER.0 as u32)?;
            let Some(entry) = self.programs.get_mut(index) else {
                return Err(GpuError::Device(format!("no program {}", program.0)));
            };
            entry.instances = buffer;
            entry.capacity = capacity;
        }

        let Some(entry) = self.programs.get_mut(index) else {
            return Err(GpuError::Device(format!("no program {}", program.0)));
        };
        let bytes = instances.len().min(count as usize * stride as usize);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: both buffers are DYNAMIC with CPU write access, are unmapped again below,
        // and the copies stay inside the sizes their descriptors declared.
        unsafe {
            let context = &self.context;
            context
                .Map(
                    &entry.instances,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped),
                )
                .map_err(|error| GpuError::Device(format!("map instances: {error}")))?;
            std::ptr::copy_nonoverlapping(instances.as_ptr(), mapped.pData as *mut u8, bytes);
            context.Unmap(&entry.instances, 0);

            context
                .Map(
                    &entry.constants,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped),
                )
                .map_err(|error| GpuError::Device(format!("map constants: {error}")))?;
            std::ptr::copy_nonoverlapping(
                entry.data.as_ptr(),
                mapped.pData as *mut u8,
                entry.data.len(),
            );
            context.Unmap(&entry.constants, 0);

            context.IASetVertexBuffers(
                0,
                1,
                Some(&Some(entry.unit.clone())),
                Some(&8u32),
                Some(&0u32),
            );
            context.IASetVertexBuffers(
                1,
                1,
                Some(&Some(entry.instances.clone())),
                Some(&stride),
                Some(&0u32),
            );
            context.DrawInstanced(6, count, 0, 0);
        }
        Ok(())
    }

    fn blit(&mut self, src: TargetId, dst: TargetId, rect: [i32; 4]) {
        // A raw copy moves bits: it cannot reorder a B8G8R8A8 buffer into an RGBA one.
        let compatible = match (self.targets.get(&src.0), self.targets.get(&dst.0)) {
            (Some(source), Some(destination)) => source.dxgi == destination.dxgi,
            _ => return,
        };
        if !compatible {
            self.fail(GpuError::Device(String::from(
                "blit needs matching target formats; present a frame with `present_copy`",
            )));
            return;
        }
        let (Some(source), Some(destination)) =
            (self.targets.get(&src.0), self.targets.get(&dst.0))
        else {
            return;
        };
        let [x, y, width, height] = rect;
        let bounds = D3D11_BOX {
            left: x.max(0) as u32,
            top: y.max(0) as u32,
            front: 0,
            right: (x + width).max(0) as u32,
            bottom: (y + height).max(0) as u32,
            back: 1,
        };
        // A copy cannot read a resource still bound for sampling, nor write one still bound
        // as a target; the caller's next use rebinds what it needs.
        let restore = self.bound;
        // SAFETY: both targets exist and share a format, and the box is inside the source.
        unsafe {
            let context = &self.context;
            context.PSSetShaderResources(0, Some(&[None, None]));
            context.OMSetRenderTargets(None, None);
            context.CopySubresourceRegion(
                &destination.texture,
                0,
                bounds.left,
                bounds.top,
                0,
                &source.texture,
                0,
                Some(&bounds),
            );
        }
        self.bound = None;
        if let Some(id) = restore {
            self.bind_target(Some(TargetId(id)));
        }
    }

    fn read_pixels(&mut self, target: TargetId, width: u32, height: u32) -> Vec<u8> {
        let empty = || vec![0u8; width as usize * height as usize * 4];
        let Some(entry) = self.targets.get(&target.0) else {
            return empty();
        };
        let (width, height, format) = (entry.width, entry.height, entry.dxgi);
        let source = entry.texture.clone();
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging = None;
        let result = unsafe {
            self.device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))
        };
        let staging = match created(result, staging, "readback texture") {
            Ok(staging) => staging,
            Err(error) => {
                self.fail(error);
                return empty();
            }
        };
        // Only mip 0 is read back: the source carries a full chain and `CopyResource` wants
        // both sides to have the same levels, while a subresource copy does not.
        unsafe {
            let context = &self.context;
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &source, 0, None);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if let Err(error) = context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) {
                self.fail(GpuError::Device(format!("map readback: {error}")));
                return empty();
            }
            let mut out = vec![0u8; width as usize * height as usize * 4];
            let half = format == DXGI_FORMAT_R16G16B16A16_FLOAT;
            // A presented buffer is BGRA on almost every Windows machine; the readback
            // contract is RGBA, so its bytes are reordered on the way out.
            let bgra = format == DXGI_FORMAT_B8G8R8A8_UNORM;
            for row in 0..height as usize {
                let row_start = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
                let span = width as usize * 4;
                let destination = &mut out[row * span..(row + 1) * span];
                if bgra {
                    let texels = std::slice::from_raw_parts(row_start, span);
                    for (texel, chunk) in texels.chunks(4).zip(destination.as_chunks_mut::<4>().0) {
                        *chunk = [texel[2], texel[1], texel[0], texel[3]];
                    }
                } else if half {
                    let texels =
                        std::slice::from_raw_parts(row_start as *const u16, width as usize * 4);
                    for (texel, chunk) in texels.chunks(4).zip(destination.as_chunks_mut::<4>().0) {
                        for channel in 0..4 {
                            let value = half_to_float(texel[channel]).clamp(0.0, 1.0);
                            chunk[channel] = (value * 255.0 + 0.5) as u8;
                        }
                    }
                } else {
                    std::ptr::copy_nonoverlapping(row_start, destination.as_mut_ptr(), span);
                }
            }
            context.Unmap(&staging, 0);
            out
        }
    }

    fn finish(&mut self) {
        let Some((texture, staging)) = self.sync_pair() else {
            return;
        };
        // `Map` on a staging resource waits for every command queued before the copy, which
        // is the only portable way to know the frame has landed.
        unsafe {
            let context = &self.context;
            context.Flush();
            context.CopyResource(&staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .is_ok()
            {
                context.Unmap(&staging, 0);
            }
        }
    }
}

fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), GpuError> {
    let levels = [D3D_FEATURE_LEVEL_11_0];
    let mut last = String::from("no driver tried");
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let mut device = None;
        let mut context = None;
        let mut level = D3D_FEATURE_LEVEL(0);
        // SAFETY: every out parameter points at a local, and the adapter is chosen by the
        // driver type rather than passed in.
        let result = unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut level),
                Some(&mut context),
            )
        };
        match (result, device, context) {
            (Ok(()), Some(device), Some(context)) => return Ok((device, context)),
            (Err(error), _, _) => last = error.to_string(),
            _ => last = String::from("no device returned"),
        }
    }
    Err(GpuError::Device(format!("D3D11: {last}")))
}

fn render_target_view(
    device: &ID3D11Device,
    texture: &ID3D11Texture2D,
) -> Result<ID3D11RenderTargetView, GpuError> {
    let mut view = None;
    let result = unsafe { device.CreateRenderTargetView(texture, None, Some(&mut view)) };
    created(result, view, "render target view")
}

fn create_raster(device: &ID3D11Device) -> Result<ID3D11RasterizerState, GpuError> {
    let desc = D3D11_RASTERIZER_DESC {
        FillMode: D3D11_FILL_SOLID,
        CullMode: D3D11_CULL_NONE,
        FrontCounterClockwise: false.into(),
        DepthBias: 0,
        DepthBiasClamp: 0.0,
        SlopeScaledDepthBias: 0.0,
        DepthClipEnable: true.into(),
        // Always on: `set_scissor(None)` widens the rect to the whole target instead.
        ScissorEnable: true.into(),
        MultisampleEnable: false.into(),
        AntialiasedLineEnable: false.into(),
    };
    let mut state = None;
    let result = unsafe { device.CreateRasterizerState(&desc, Some(&mut state)) };
    created(result, state, "rasterizer state")
}

fn create_blend(device: &ID3D11Device, enabled: bool) -> Result<ID3D11BlendState, GpuError> {
    let mut desc = D3D11_BLEND_DESC::default();
    desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: enabled.into(),
        SrcBlend: D3D11_BLEND_ONE,
        DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: D3D11_BLEND_ONE,
        DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    let mut state = None;
    let result = unsafe { device.CreateBlendState(&desc, Some(&mut state)) };
    created(result, state, "blend state")
}

/// Where a uniform lives, accepting either spelling of an array: the painter names one array
/// by its base and another by its first element, as GLSL accepted both.
fn slot_index(offsets: &[(String, u32)], name: &str) -> Option<usize> {
    if let Some(index) = offsets.iter().position(|(each, _)| each.as_str() == name) {
        return Some(index);
    }
    let alternate = match name.strip_suffix("[0]") {
        Some(base) => format!("{base}[0]"),
        None => format!("{name}[0]"),
    };
    offsets.iter().position(|(each, _)| each.as_str() == alternate)
}

/// The bytecode of one program: vertex first, pixel second.
type Bytecode = Result<(Vec<u8>, Vec<u8>), String>;

/// The compiled programs of this process, built on first request.
///
/// Compiling is what a surface pays to start, and it needs no device: a process compiles each
/// program once, and every device it makes afterwards only builds its own shader objects.
fn bytecode(key: ProgramKey) -> &'static Bytecode {
    static BYTECODE: [OnceLock<Bytecode>; ProgramKey::ALL.len()] =
        [const { OnceLock::new() }; ProgramKey::ALL.len()];
    BYTECODE[key.index()].get_or_init(|| compile_program(key))
}

fn compile_program(key: ProgramKey) -> Bytecode {
    let source = shader::build(key);
    let vertex = compile(
        &format!("{key:?}.vert"),
        &source.vertex,
        s!("main"),
        s!("vs_5_0"),
    )
    .map_err(|error| error.to_string())?;
    let pixel = compile(
        &format!("{key:?}.frag"),
        &source.fragment,
        s!("main"),
        s!("ps_5_0"),
    )
    .map_err(|error| error.to_string())?;
    Ok((vertex, pixel))
}

/// Compile every program the frame has not asked for yet, on a worker.
///
/// Called once the first frame is on screen: starting this alongside that frame only puts the
/// window's own programs behind the compiler's queue.
pub fn precompile_remaining() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::spawn(|| {
        for key in ProgramKey::ALL {
            if let Err(message) = bytecode(key) {
                eprintln!("shader {key:?}: {message}");
            }
        }
    });
}

/// Compile one stage, reporting the compiler's log verbatim.
///
/// The label becomes the source name, so a diagnostic says which program and stage it came
/// from instead of pointing at an anonymous `Shader`.
fn compile(
    label: &str,
    source: &str,
    entry: windows::core::PCSTR,
    target: windows::core::PCSTR,
) -> Result<Vec<u8>, GpuError> {
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    let name = format!("{label}\0");
    // SAFETY: the source and the name both outlive the call and both blob out parameters are
    // locals.
    let result = unsafe {
        D3DCompile(
            source.as_ptr() as *const c_void,
            source.len(),
            windows::core::PCSTR::from_raw(name.as_ptr()),
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    match result {
        Ok(()) => match code {
            Some(blob) => Ok(unsafe { blob_bytes(&blob) }),
            None => Err(GpuError::Shader(String::from("no bytecode returned"))),
        },
        Err(error) => {
            let log = errors
                .as_ref()
                .map(|blob| unsafe { blob_text(blob) })
                .unwrap_or_default();
            Err(GpuError::Shader(format!("{label}: {error}\n{log}")))
        }
    }
}

unsafe fn blob_bytes(blob: &ID3DBlob) -> Vec<u8> {
    // SAFETY: the blob owns a buffer of exactly the size it reports.
    let size = unsafe { blob.GetBufferSize() };
    let pointer = unsafe { blob.GetBufferPointer() } as *const u8;
    unsafe { std::slice::from_raw_parts(pointer, size) }.to_vec()
}

unsafe fn blob_text(blob: &ID3DBlob) -> String {
    let bytes = unsafe { blob_bytes(blob) };
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// IEEE half to single; only the half-float targets are ever read back as halves.
fn half_to_float(bits: u16) -> f32 {
    let sign = (bits >> 15) & 1;
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = (bits & 0x3ff) as f32;
    let value = match exponent {
        0 => mantissa * 2f32.powi(-24),
        31 => {
            if mantissa == 0.0 {
                f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => (1.0 + mantissa / 1024.0) * 2f32.powi(i32::from(exponent) - 15),
    };
    if sign == 1 { -value } else { value }
}
