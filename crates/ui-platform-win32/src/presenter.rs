//! The swap chain and the scene that fills it: the engine's [`SceneTarget`] on Windows.

use ui_core::Damage;
use ui_core::draw::DrawList;
use ui_gpu_d3d11::D3d11Device;
use ui_gpu_scene::{
    AtlasUpload, CanvasUpload, Device, GpuRenderer, RenderTarget, TargetFormat, TargetId,
};
use ui_platform::present::{CanvasFrame, PresentStats, SceneTarget};
use ui_platform::text_pass::TextPass;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_UNSPECIFIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory2, IDXGIOutput, IDXGISwapChain1,
};

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
        let renderer = GpuRenderer::new(&mut device)
            .map_err(|error| WindowError::Gpu(format!("renderer: {error}")))?;
        let (chain, format) = create_chain(device.raw_device(), window.hwnd(), size)?;
        let mut this = Self {
            device,
            renderer,
            chain,
            format,
            buffer: TargetId(0),
            frame: TargetId(0),
            size,
            capture: false,
            captured: None,
            atlas_version: u64::MAX,
            color_atlas_version: 0,
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
        if self.frame.0 == 0 {
            let (target, _sampled) = self
                .device
                .create_target(self.size.0, self.size.1, TargetFormat::Rgba8)
                .map_err(|error| WindowError::Gpu(format!("frame target: {error}")))?;
            self.frame = target;
        } else {
            self.device
                .resize_target(self.frame, self.size.0, self.size.1, TargetFormat::Rgba8);
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

/// A flip-model chain at the window's own format; `B8G8R8A8` is the one DXGI guarantees.
fn create_chain(
    device: &ID3D11Device,
    hwnd: HWND,
    size: (u32, u32),
) -> Result<(IDXGISwapChain1, DXGI_FORMAT), WindowError> {
    let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }
        .map_err(|error| WindowError::Gpu(format!("CreateDXGIFactory2: {error}")))?;
    let format = DXGI_FORMAT_B8G8R8A8_UNORM;
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
    Ok((chain, format))
}
