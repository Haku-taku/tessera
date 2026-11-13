//! The swap chain and the scene that fills it: the engine's [`SceneTarget`] on Windows.

use ui_core::Damage;
use ui_core::colour::{
    Chromaticities, Destination, Domain, LinearEncoding, ToneMapping, WhiteAdaptation,
    gamut_matrix_adapted,
};
use ui_core::draw::DrawList;
use ui_gpu_d3d11::D3d11Device;
use ui_gpu_scene::{AtlasUpload, CanvasUpload, Device, GpuRenderer, RenderTarget, TargetId};
use ui_platform::present::{CanvasFrame, PresentStats, SceneTarget};
use ui_platform::text_pass::TextPass;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_ALPHA_MODE_UNSPECIFIED,
    DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGIOutput, IDXGISwapChain1,
    IDXGISwapChain3,
};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::core::Interface as _;

use crate::outputs::HdrOutput;
use crate::window::Win32Window;
use crate::WindowError;

/// Buffers the flip model needs to overlap the next frame with the one on screen.
const BUFFERS: u32 = 2;

/// A window's swap chain and the renderer that fills it.
///
/// The painter draws into a target of its own and the frame is copied into the back buffer.
/// D3D11 hides a flip chain's rotation: buffer 0 is always the buffer to draw into, even
/// though its storage changes with every `Present`. A persistent target keeps the painter's
/// damage bands honest — they assume the target they hand over holds the previous frame —
/// and the copy writes every pixel, so a discarded back buffer never shows through.
pub struct Win32Presenter {
    device: D3d11Device,
    renderer: GpuRenderer,
    chain: IDXGISwapChain1,
    format: DXGI_FORMAT,
    /// The output the chain declares HDR for, with how that light is encoded; `None` on an
    /// 8-bit SDR surface.
    hdr: Option<(HdrOutput, LinearEncoding)>,
    /// Held for the life of the surface: composed frames stop showing when it drops.
    _composition: Option<Composition>,
    /// The back buffer as a target; released before the chain resizes.
    buffer: TargetId,
    /// The persistent target the painter draws into.
    frame: TargetId,
    /// Surface extent in physical pixels, also the frame target's.
    size: (u32, u32),
    /// Set for the next frame: read the back buffer before presenting it.
    capture: bool,
    /// The last captured frame, top-down RGBA8.
    captured: Option<Vec<u8>>,
    atlas_version: u64,
    color_atlas_version: u64,
    /// Frames presented since the loop last read the count.
    presented: u32,
}

impl Win32Presenter {
    /// Build the device and open a swap chain on `window`.
    pub fn new(window: &Win32Window) -> Result<Self, WindowError> {
        let size = window.client_size();
        if size.0 == 0 || size.1 == 0 {
            return Err(WindowError::Win32(String::from(
                "the window has no client area to present into",
            )));
        }
        let mut device = D3d11Device::new()
            .map_err(|error| WindowError::Gpu(format!("D3D11 device: {error}")))?;
        let mut renderer = GpuRenderer::new(&mut device)
            .map_err(|error| WindowError::Gpu(format!("renderer: {error}")))?;
        let opened = open_chain(
            device.raw_device(),
            window.visual_hwnd(),
            size,
            window.composited(),
        )?;
        // Before the frame target exists: its format follows the destination.
        if let Some((output, encoding)) = opened.hdr {
            aim_at_hdr(&mut renderer, output, encoding);
        }
        let mut this = Self {
            device,
            renderer,
            chain: opened.chain,
            format: opened.format,
            hdr: opened.hdr,
            _composition: opened.composition,
            buffer: TargetId(0),
            frame: TargetId(0),
            size,
            capture: false,
            captured: None,
            atlas_version: u64::MAX,
            color_atlas_version: 0,
            presented: 0,
        };
        this.allocate_targets()?;
        Ok(this)
    }

    /// Follow a new client extent and DPI scale; a no-op when neither changed.
    pub fn resize(&mut self, size: (u32, u32), scale: f32) -> Result<(), WindowError> {
        self.renderer.set_scale(scale);
        if size.0 == 0 || size.1 == 0 || size == self.size {
            return Ok(());
        }
        // Every reference to the old back buffer has to go before DXGI will resize, including
        // the one the command stream may still hold.
        self.device.finish();
        self.device.bind_target(None);
        self.device.release_target(self.buffer);
        self.buffer = TargetId(0);
        unsafe {
            self.chain
                .ResizeBuffers(BUFFERS, size.0, size.1, self.format, DXGI_SWAP_CHAIN_FLAG(0))
        }
        .map_err(|error| WindowError::Gpu(format!("ResizeBuffers: {error}")))?;
        self.size = size;
        self.captured = None;
        self.allocate_targets()
    }

    /// Surface extent in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The HDR the swap chain declares; `None` when it presents an SDR surface.
    pub fn hdr(&self) -> Option<HdrOutput> {
        self.hdr.map(|(output, _)| output)
    }

    /// How the chain encodes linear light, which decides what its buffer holds.
    pub fn hdr_encoding(&self) -> Option<LinearEncoding> {
        self.hdr.map(|(_, encoding)| encoding)
    }

    /// The scene renderer, for knobs only the scene knows about such as supersampling.
    pub fn renderer_mut(&mut self) -> &mut GpuRenderer {
        &mut self.renderer
    }

    /// Drain the GPU: every command submitted so far has finished when this returns.
    pub fn finish(&mut self) {
        self.device.finish();
    }

    /// Frames presented since the last call.
    pub fn take_presented(&mut self) -> u32 {
        std::mem::take(&mut self.presented)
    }

    /// Write the last composited frame as a PPM, top-down RGBA8 sampled into RGB.
    ///
    /// The frame target keeps the scene after the copy into the back buffer, so this works
    /// without asking the next present to capture anything.
    pub fn dump_frame_ppm(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.device.finish();
        let pixels = self.device.read_pixels(self.frame, self.size.0, self.size.1);
        write_ppm(path, self.size, &pixels)
    }

    /// Ask the next `present_scene` to read the back buffer back before presenting it.
    pub fn capture_next_frame(&mut self) {
        self.capture = true;
    }

    /// The frame captured by the last `present_scene`, top-down RGBA8.
    pub fn take_capture(&mut self) -> Option<Vec<u8>> {
        self.captured.take()
    }

    /// Adopt the back buffer and give the frame target its storage.
    fn allocate_targets(&mut self) -> Result<(), WindowError> {
        let texture: ID3D11Texture2D = unsafe { self.chain.GetBuffer(0) }
            .map_err(|error| WindowError::Gpu(format!("GetBuffer: {error}")))?;
        let (width, height, format) = (self.size.0, self.size.1, self.format);
        self.buffer = if self.buffer.0 == 0 {
            self.device
                .adopt_target(&texture, width, height, format)
                .map_err(|error| WindowError::Gpu(format!("adopt back buffer: {error}")))?
        } else {
            self.device
                .retarget(self.buffer, &texture, width, height, format)
                .map_err(|error| WindowError::Gpu(format!("retarget: {error}")))?;
            self.buffer
        };
        let format = self.renderer.frame_format();
        if self.frame.0 == 0 {
            let (target, _sampled) = self
                .device
                .create_target(self.size.0, self.size.1, format)
                .map_err(|error| WindowError::Gpu(format!("frame target: {error}")))?;
            self.frame = target;
        } else {
            self.device
                .resize_target(self.frame, self.size.0, self.size.1, format);
        }
        Ok(())
    }

    /// Upload the glyph atlases if the text pass changed them.
    fn sync_atlas(&mut self, pass: &TextPass) -> Result<(), WindowError> {
        if pass.version() != self.atlas_version {
            self.atlas_version = pass.version();
            let (width, height) = pass.dimensions();
            self.renderer
                .set_atlas(
                    &mut self.device,
                    AtlasUpload {
                        width,
                        height,
                        data: pass.atlas().to_vec(),
                    },
                )
                .map_err(|error| WindowError::Gpu(format!("atlas upload: {error}")))?;
        }
        // The colour page is tracked separately: a new text glyph must not re-upload it.
        if pass.color_version() != self.color_atlas_version {
            self.color_atlas_version = pass.color_version();
            if !pass.color_atlas().is_empty() {
                let (width, height) = pass.color_dimensions();
                self.renderer
                    .set_color_atlas(
                        &mut self.device,
                        AtlasUpload {
                            width,
                            height,
                            data: pass.color_atlas().to_vec(),
                        },
                    )
                    .map_err(|error| {
                        WindowError::Gpu(format!("colour atlas upload: {error}"))
                    })?;
            }
        }
        Ok(())
    }
}

impl SceneTarget for Win32Presenter {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn present_scene(
        &mut self,
        scene: &DrawList,
        text: &TextPass,
        damage: &Damage,
        canvases: &[CanvasFrame],
    ) -> Result<PresentStats, String> {
        self.sync_atlas(text).map_err(|error| error.to_string())?;
        let uploads: Vec<CanvasUpload<'_>> = canvases
            .iter()
            .map(|canvas| CanvasUpload {
                id: canvas.id,
                size: canvas.size,
                list: canvas.changed.then_some(&canvas.list),
            })
            .collect();
        let frame = Frame {
            target: self.frame,
            size: self.size,
        };
        let stats = self
            .renderer
            .render_frame_with_canvases(
                &mut self.device,
                &frame,
                scene,
                damage,
                &[],
                &uploads,
            )
            .map_err(|error| format!("render: {error}"))?;
        self.device
            .present_copy(self.frame, self.buffer)
            .map_err(|error| format!("present copy: {error}"))?;
        // Before `Present`: D3D11 remaps buffer 0 to the next storage as it flips, so the
        // frame about to be shown is only readable right here.
        if self.capture {
            self.capture = false;
            self.captured = Some(self.device.read_pixels(self.buffer, self.size.0, self.size.1));
        }
        unsafe { self.chain.Present(1, DXGI_PRESENT(0)) }
            .ok()
            .map_err(|error| format!("Present: {error}"))?;
        if self.presented == 0 {
            // The window is on screen; the programs it did not draw compile from here, so the
            // one that first draws glass does not wait for it.
            ui_gpu_d3d11::precompile_remaining();
        }
        self.presented += 1;
        Ok(PresentStats {
            draw_calls: stats.draw_calls,
            band: stats.band,
        })
    }
}

/// The persistent target's extent, as the renderer asks for it.
struct Frame {
    target: TargetId,
    size: (u32, u32),
}

impl RenderTarget for Frame {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn target_id(&self) -> TargetId {
        self.target
    }
}

/// A window's chain, and the compositor when the surface's alpha is meant to be seen.
struct Chain {
    chain: IDXGISwapChain1,
    format: DXGI_FORMAT,
    hdr: Option<(HdrOutput, LinearEncoding)>,
    composition: Option<Composition>,
}

/// The DirectComposition stack a transparent surface presents through.
///
/// An HWND swap chain is shown opaque: whatever the frame leaves transparent is black on the
/// desktop. A layer has to be composed with `DXGI_ALPHA_MODE_PREMULTIPLIED` instead, which
/// DXGI only offers off a window of its own, hence the visual holding the chain.
struct Composition {
    _device: IDCompositionDevice,
    _target: IDCompositionTarget,
}

/// The chain the window presents through, plus the HDR it declares.
///
/// Only one chain exists at a time: the format is decided from the output before a chain is
/// made, and a chain the driver will declare neither PQ nor scRGB is dropped before the SDR
/// one replaces it.
fn open_chain(
    device: &ID3D11Device,
    hwnd: HWND,
    size: (u32, u32),
    composited: bool,
) -> Result<Chain, WindowError> {
    let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }
        .map_err(|error| WindowError::Gpu(format!("CreateDXGIFactory2: {error}")))?;
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if let Some(output) = crate::outputs::hdr_output(&factory, monitor) {
        let format = DXGI_FORMAT_R16G16B16A16_FLOAT;
        let (chain, composition) = create_chain(&factory, device, hwnd, size, format, composited)?;
        if let Some(encoding) = declare_hdr(&chain) {
            return Ok(Chain {
                chain,
                format,
                hdr: Some((output, encoding)),
                composition,
            });
        }
        drop(chain);
    }
    let format = DXGI_FORMAT_B8G8R8A8_UNORM;
    let (chain, composition) = create_chain(&factory, device, hwnd, size, format, composited)?;
    Ok(Chain {
        chain,
        format,
        hdr: None,
        composition,
    })
}

/// Declare the chain in the first HDR space it will present, or `None` when it takes none.
///
/// PQ is what a colour-managed compositor asks for, and a float chain on an advanced-colour
/// display often refuses it while accepting scRGB — which is what the desktop itself
/// composites in, and what Chromium declares for HDR.
fn declare_hdr(chain: &IDXGISwapChain1) -> Option<LinearEncoding> {
    let chain = chain.cast::<IDXGISwapChain3>().ok()?;
    // `DXGI_COLOR_SPACE_SUPPORT_FLAG_PRESENT`, the flag the bindings do not name.
    const SUPPORTED_FOR_PRESENT: u32 = 0x1;
    for (space, encoding) in [
        (
            DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
            LinearEncoding::Pq,
        ),
        (DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, LinearEncoding::ScRgb),
    ] {
        let presentable = unsafe { chain.CheckColorSpaceSupport(space) }
            .is_ok_and(|flags| flags & SUPPORTED_FOR_PRESENT != 0);
        if presentable && unsafe { chain.SetColorSpace1(space) }.is_ok() {
            return Some(encoding);
        }
    }
    None
}

/// Point the renderer at the display: light tone mapped into the panel's own range, in the
/// gamut and encoding the chain declared.
fn aim_at_hdr(renderer: &mut GpuRenderer, output: HdrOutput, encoding: LinearEncoding) {
    renderer.set_destination(Destination {
        domain: Domain::Linear {
            reference_nits: output.reference_nits,
            min_nits: output.min_nits,
            peak_nits: output.peak_nits,
            encoding,
        },
        tone: ToneMapping::Bt2390,
    });
    renderer.set_gamut(match encoding {
        // A PQ surface carries BT.2020.
        LinearEncoding::Pq => gamut_matrix_adapted(
            Chromaticities::SRGB,
            Chromaticities::BT2020,
            WhiteAdaptation::Bradford,
        ),
        // scRGB's primaries are the working space's own, so nothing is converted.
        LinearEncoding::ScRgb => gamut_matrix_adapted(
            Chromaticities::SRGB,
            Chromaticities::SRGB,
            WhiteAdaptation::Bradford,
        ),
    });
}

/// A flip-model chain at `format`, composed when the surface's alpha is meant to be seen.
fn create_chain(
    factory: &IDXGIFactory2,
    device: &ID3D11Device,
    hwnd: HWND,
    size: (u32, u32),
    format: DXGI_FORMAT,
    composited: bool,
) -> Result<(IDXGISwapChain1, Option<Composition>), WindowError> {
    let descriptor = DXGI_SWAP_CHAIN_DESC1 {
        Width: size.0,
        Height: size.1,
        Format: format,
        Stereo: false.into(),
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: BUFFERS,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        AlphaMode: if composited {
            DXGI_ALPHA_MODE_PREMULTIPLIED
        } else {
            DXGI_ALPHA_MODE_UNSPECIFIED
        },
        Flags: 0,
    };
    if composited {
        let chain = unsafe {
            factory.CreateSwapChainForComposition(device, &descriptor, None::<&IDXGIOutput>)
        }
        .map_err(|error| WindowError::Gpu(format!("CreateSwapChainForComposition: {error}")))?;
        let composition = compose(device, hwnd, &chain)?;
        return Ok((chain, Some(composition)));
    }
    let chain =
        unsafe { factory.CreateSwapChainForHwnd(device, hwnd, &descriptor, None, None::<&IDXGIOutput>) }
            .map_err(|error| WindowError::Gpu(format!("CreateSwapChainForHwnd: {error}")))?;
    Ok((chain, None))
}

/// Put `chain` on screen through a composition visual rooted in `hwnd`.
///
/// The window keeps its place in the stacking order and its messages; only the pixels come
/// from the visual, which is why the window is created without a redirection surface.
fn compose(
    device: &ID3D11Device,
    hwnd: HWND,
    chain: &IDXGISwapChain1,
) -> Result<Composition, WindowError> {
    let dxgi: IDXGIDevice = device
        .cast()
        .map_err(|error| WindowError::Gpu(format!("IDXGIDevice: {error}")))?;
    let composer: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi) }
        .map_err(|error| WindowError::Gpu(format!("DCompositionCreateDevice: {error}")))?;
    let visual: IDCompositionVisual = unsafe { composer.CreateVisual() }
        .map_err(|error| WindowError::Gpu(format!("CreateVisual: {error}")))?;
    let target = unsafe { composer.CreateTargetForHwnd(hwnd, true) }
        .map_err(|error| WindowError::Gpu(format!("CreateTargetForHwnd: {error}")))?;
    unsafe { visual.SetContent(chain) }
        .map_err(|error| WindowError::Gpu(format!("SetContent: {error}")))?;
    unsafe { target.SetRoot(&visual) }
        .map_err(|error| WindowError::Gpu(format!("SetRoot: {error}")))?;
    unsafe { composer.Commit() }
        .map_err(|error| WindowError::Gpu(format!("Commit: {error}")))?;
    Ok(Composition {
        _device: composer,
        _target: target,
    })
}

/// Write a PPM: the one image format a probe can read back without a decoder.
fn write_ppm(path: &std::path::Path, size: (u32, u32), pixels: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let describe = |error: std::io::Error| format!("{}: {error}", path.display());
    let mut file = std::fs::File::create(path).map_err(describe)?;
    write!(file, "P6\n{} {}\n255\n", size.0, size.1).map_err(describe)?;
    for texel in pixels.as_chunks::<4>().0 {
        file.write_all(&texel[..3]).map_err(describe)?;
    }
    Ok(())
}
