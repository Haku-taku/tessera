//! The swap chain and the scene that fills it: the engine's [`SceneTarget`] on Windows.

use ui_core::Damage;
use ui_core::colour::{
    Chromaticities, Destination, Domain, ToneMapping, WhiteAdaptation, gamut_matrix_adapted,
};
use ui_core::draw::DrawList;
use ui_gpu_d3d11::D3d11Device;
use ui_gpu_scene::{AtlasUpload, CanvasUpload, Device, GpuRenderer, RenderTarget, TargetId};
use ui_platform::present::{CanvasFrame, PresentStats, SceneTarget};
use ui_platform::text_pass::TextPass;
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SDR_WHITE_LEVEL, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_UNSPECIFIED, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, DXGI_FORMAT,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory1, IDXGIFactory2, IDXGIOutput, IDXGIOutput6,
    IDXGISwapChain1, IDXGISwapChain3,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow,
};
use windows::core::Interface as _;

use crate::window::Win32Window;
use crate::WindowError;

/// Buffers the flip model needs to overlap the next frame with the one on screen.
const BUFFERS: u32 = 2;

/// What an HDR output says about its own luminance, in nits.
///
/// `reference_nits` is the display's SDR white: linear 1.0 in the scene is shown at it, which
/// is what keeps a window on an HDR desktop as bright as the rest of the desktop.
#[derive(Clone, Copy, Debug)]
pub struct HdrOutput {
    pub reference_nits: f32,
    pub min_nits: f32,
    pub peak_nits: f32,
}

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
    /// Set when the chain carries PQ in BT.2020 rather than an 8-bit SDR surface.
    hdr: Option<HdrOutput>,
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
    /// Build the device, compile every program, and open a swap chain on `window`.
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
        let (chain, format, hdr) = open_chain(device.raw_device(), window.hwnd(), size)?;
        // Before the frame target exists: its format follows the destination.
        if let Some(output) = hdr {
            aim_at_hdr(&mut renderer, output);
        }
        let mut this = Self {
            device,
            renderer,
            chain,
            format,
            hdr,
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
        self.hdr
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

/// The chain the window presents through, plus the HDR it declares.
///
/// Only one chain exists at a time: the format is decided from the output before a chain is
/// made, and a chain the driver will not declare PQ is dropped before the SDR one replaces it.
fn open_chain(
    device: &ID3D11Device,
    hwnd: HWND,
    size: (u32, u32),
) -> Result<(IDXGISwapChain1, DXGI_FORMAT, Option<HdrOutput>), WindowError> {
    let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }
        .map_err(|error| WindowError::Gpu(format!("CreateDXGIFactory2: {error}")))?;
    if let Some(output) = hdr_output(&factory, hwnd) {
        let format = DXGI_FORMAT_R16G16B16A16_FLOAT;
        let chain = create_chain(&factory, device, hwnd, size, format)?;
        if declare_hdr(&chain) {
            return Ok((chain, format, Some(output)));
        }
        drop(chain);
    }
    let format = DXGI_FORMAT_B8G8R8A8_UNORM;
    let chain = create_chain(&factory, device, hwnd, size, format)?;
    Ok((chain, format, None))
}

/// The output the window is on, when Windows is presenting HDR there.
fn hdr_output(factory: &IDXGIFactory2, hwnd: HWND) -> Option<HdrOutput> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_invalid() {
        return None;
    }
    let factory: IDXGIFactory1 = factory.cast().ok()?;
    let mut index = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        let mut number = 0;
        while let Ok(output) = unsafe { adapter.EnumOutputs(number) } {
            number += 1;
            let Ok(output) = output.cast::<IDXGIOutput6>() else {
                continue;
            };
            let Ok(description) = (unsafe { output.GetDesc1() }) else {
                continue;
            };
            if description.Monitor != monitor {
                continue;
            }
            // The desktop composes in PQ only once HDR is switched on for this display; a
            // panel that can reach 600 nits still describes itself as sRGB until then.
            if description.ColorSpace != DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 {
                return None;
            }
            return Some(HdrOutput {
                // 80 nits is the configuration's own unit, so it is the honest value when the
                // query that would refine it fails.
                reference_nits: sdr_white_nits(hwnd).unwrap_or(80.0),
                min_nits: description.MinLuminance,
                peak_nits: description.MaxLuminance,
            });
        }
    }
    None
}

/// Declare the chain PQ in BT.2020; false when the output will not present that.
fn declare_hdr(chain: &IDXGISwapChain1) -> bool {
    let Ok(chain) = chain.cast::<IDXGISwapChain3>() else {
        return false;
    };
    // `DXGI_COLOR_SPACE_SUPPORT_FLAG_PRESENT`, the flag the bindings do not name.
    const SUPPORTED_FOR_PRESENT: u32 = 0x1;
    let space = DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
    let supported = unsafe { chain.CheckColorSpaceSupport(space) }
        .is_ok_and(|flags| flags & SUPPORTED_FOR_PRESENT != 0);
    supported && unsafe { chain.SetColorSpace1(space) }.is_ok()
}

/// Point the renderer at the display: linear light tone mapped into the panel's own range,
/// and BT.2020 because that is the gamut a PQ surface carries.
fn aim_at_hdr(renderer: &mut GpuRenderer, output: HdrOutput) {
    renderer.set_destination(Destination {
        domain: Domain::Linear {
            reference_nits: output.reference_nits,
            min_nits: output.min_nits,
            peak_nits: output.peak_nits,
        },
        tone: ToneMapping::Bt2390,
    });
    renderer.set_gamut(gamut_matrix_adapted(
        Chromaticities::SRGB,
        Chromaticities::BT2020,
        WhiteAdaptation::Bradford,
    ));
}

/// The nits this display shows SDR white at, which is what linear 1.0 means in the scene.
///
/// `DXGI_OUTPUT_DESC1` has no field for it: the value belongs to the display configuration,
/// on the path whose GDI device name is the monitor's.
fn sdr_white_nits(hwnd: HWND) -> Option<f32> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let described =
        unsafe { GetMonitorInfoW(monitor, std::ptr::from_mut(&mut info).cast::<MONITORINFO>()) };
    if !described.as_bool() {
        return None;
    }
    let device = wide_name(&info.szDevice);
    let (mut paths, mut modes) = (0u32, 0u32);
    let sized = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes) };
    if sized.is_err() {
        return None;
    }
    let mut path_list = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
    let mut mode_list = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
    let queried = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut paths,
            path_list.as_mut_ptr(),
            &mut modes,
            mode_list.as_mut_ptr(),
            None,
        )
    };
    if queried.is_err() {
        return None;
    }
    let path = path_list
        .iter()
        .take(paths as usize)
        .find(|path| wide_name(&source_device(path)) == device)?;
    let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
            size: std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        SDRWhiteLevel: 0,
    };
    // Returns an error code rather than setting the last error, so zero is success.
    let status = unsafe { DisplayConfigGetDeviceInfo(&mut white.header) };
    if status != 0 {
        return None;
    }
    // The value is in thousandths of 80 nits: 1000 is the reference white of an SDR display.
    Some(white.SDRWhiteLevel as f32 * 80.0 / 1000.0)
}

/// The GDI device name of a display path's source, such as `\\.\DISPLAY1`.
fn source_device(path: &DISPLAYCONFIG_PATH_INFO) -> [u16; 32] {
    let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        viewGdiDeviceName: [0; 32],
    };
    if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
        return [0; 32];
    }
    source.viewGdiDeviceName
}

/// A fixed-width wide name up to its terminator.
fn wide_name(name: &[u16]) -> &[u16] {
    let end = name.iter().position(|unit| *unit == 0).unwrap_or(name.len());
    &name[..end]
}

/// A flip-model chain at `format`.
fn create_chain(
    factory: &IDXGIFactory2,
    device: &ID3D11Device,
    hwnd: HWND,
    size: (u32, u32),
    format: DXGI_FORMAT,
) -> Result<IDXGISwapChain1, WindowError> {
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
        AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
        Flags: 0,
    };
    let chain =
        unsafe { factory.CreateSwapChainForHwnd(device, hwnd, &descriptor, None, None::<&IDXGIOutput>) }
            .map_err(|error| WindowError::Gpu(format!("CreateSwapChainForHwnd: {error}")))?;
    Ok(chain)
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
