//! Windows Graphics Capture: the compositor's shared surface, one display at a time.
//!
//! A capture session holds nothing exclusively and hands over a frame while the desktop is
//! still, which is what one screenshot needs. The pool is free-threaded because the caller has
//! no dispatcher queue, and an advanced-colour display is captured in scRGB half floats, which
//! travel unconverted, exactly as a duplication's float surface does.

use std::time::{Duration, Instant};

use ui_core::colour::ContentColour;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::E_POINTER;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1, IDXGIFactory2,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::Interface as _;

use crate::capture::{
    ScreenFrame, device_on, find_output, monitor_scale, read_half_rows, read_rows, staging_for,
};
use crate::outputs::{HdrOutput, hdr_output};

/// A capture session over one output, alive until it drops.
pub struct GraphicsCapture {
    pool: Direct3D11CaptureFramePool,
    /// Held because releasing the session stops the capture, whatever the pool still holds.
    _session: GraphicsCaptureSession,
    context: ID3D11DeviceContext,
    /// The frame's texture belongs to this device; the field keeps it alive with the context.
    _device: ID3D11Device,
    /// The copy the CPU reads; recreated when the size or packing changes.
    staging: Option<ID3D11Texture2D>,
    /// Present when the display is in advanced colour, which is what the pool was sized for.
    hdr: Option<HdrOutput>,
    output: String,
    scale: f32,
}

impl GraphicsCapture {
    /// Whether this Windows offers the API at all: Windows 10 1809 and later register the class.
    pub fn available() -> bool {
        // Activation needs an apartment; one the caller already made, of either kind, will do.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().is_ok()
    }

    /// Capture `output` by GDI device name, or the primary output when none is named.
    pub fn open(output: Option<&str>) -> Result<Self, String> {
        if !Self::available() {
            return Err(String::from(
                "Windows Graphics Capture is not available on this system",
            ));
        }
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| format!("CreateDXGIFactory1: {error}"))?;
        let (adapter, dxgi_output, name) = find_output(&factory, output)?;
        let monitor = unsafe { dxgi_output.GetDesc() }
            .map_err(|error| format!("output description: {error}"))?
            .Monitor;
        let scale = monitor_scale(monitor);
        // Every DXGI factory since Windows 8 is a factory2; without that cast there is simply
        // no HDR answer, which reads as an SDR display and is what the pool then assumes.
        let hdr = factory
            .cast::<IDXGIFactory2>()
            .ok()
            .and_then(|factory| hdr_output(&factory, monitor));
        let (device, context) = device_on(&adapter)?;
        let direct = direct_device(&device)?;
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
                .map_err(|error| format!("the capture interop: {error}"))?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(monitor) }
            .map_err(|error| format!("a capture item for {name}: {error}"))?;
        let size = item
            .Size()
            .map_err(|error| format!("the capture item's size on {name}: {error}"))?;
        let format = match hdr {
            Some(_) => DirectXPixelFormat::R16G16B16A16Float,
            None => DirectXPixelFormat::B8G8R8A8UIntNormalized,
        };
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&direct, format, 2, size)
            .map_err(|error| format!("a capture frame pool for {name}: {error}"))?;
        let session = pool
            .CreateCaptureSession(&item)
            .map_err(|error| format!("a capture session for {name}: {error}"))?;
        // The rungs must photograph the same desktop: a duplication never draws a pointer, and
        // the border is a Windows 11 affordance nobody asked for. Both settings are newer than
        // the API itself, so a refusal means the frame carries them and nothing else.
        let _ = session.SetIsCursorCaptureEnabled(false);
        let _ = session.SetIsBorderRequired(false);
        session
            .StartCapture()
            .map_err(|error| format!("StartCapture on {name}: {error}"))?;
        Ok(Self {
            pool,
            _session: session,
            context,
            _device: device,
            staging: None,
            hdr,
            output: name,
            scale,
        })
    }

    /// The GDI device name of the output being captured.
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Device pixels per logical point on that output.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Grab one frame, waiting at most `timeout` for the compositor to hand one over.
    pub fn grab(&mut self, timeout: Duration) -> Result<ScreenFrame, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(frame) = self.next()? {
                return Ok(frame);
            }
            if Instant::now() >= deadline {
                return Err(String::from(
                    "the compositor handed over no frame before the deadline",
                ));
            }
            // The pool's own wait belongs to a dispatcher queue; a millisecond keeps this
            // thread from spinning while the first frame is composed.
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The next frame the pool holds, if it holds one yet.
    fn next(&mut self) -> Result<Option<ScreenFrame>, String> {
        let frame = match self.pool.TryGetNextFrame() {
            Ok(frame) => frame,
            // "Nothing yet" is a null return, which windows-rs reports as an error carrying a
            // success code (or E_POINTER for the null itself); neither is a failure here.
            Err(error) if error.code().is_ok() || error.code() == E_POINTER => return Ok(None),
            Err(error) => return Err(format!("TryGetNextFrame: {error}")),
        };
        let taken = self.copy(&frame);
        // The pool will not hand over the next frame until this one is closed; a copy that
        // already failed has nothing further to report from here.
        let _ = frame.Close();
        taken.map(Some)
    }

    fn copy(&mut self, frame: &Direct3D11CaptureFrame) -> Result<ScreenFrame, String> {
        let surface = frame
            .Surface()
            .map_err(|error| format!("frame surface: {error}"))?;
        let access: IDirect3DDxgiInterfaceAccess = surface
            .cast()
            .map_err(|error| format!("the frame's DXGI interface: {error}"))?;
        let texture: ID3D11Texture2D = unsafe { access.GetInterface() }
            .map_err(|error| format!("the frame's texture: {error}"))?;
        self.read(&texture)
    }

    /// The captured texture as a frame the caller can draw.
    fn read(&mut self, texture: &ID3D11Texture2D) -> Result<ScreenFrame, String> {
        let mut description = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut description) };
        let float = match description.Format {
            DXGI_FORMAT_B8G8R8A8_UNORM => false,
            DXGI_FORMAT_R16G16B16A16_FLOAT => true,
            other => {
                return Err(format!(
                    "the capture session is in {other:?}, which is neither 8-bit BGRA nor half-float"
                ));
            }
        };
        let staging = staging_for(&self.context, &mut self.staging, &description)?;
        unsafe { self.context.CopyResource(&staging, texture) };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|error| format!("map the captured frame: {error}"))?;
        let (pixels, colour) = if float {
            (
                read_half_rows(&mapped, &description),
                ContentColour::scrgb(self.hdr.map_or(0.0, |hdr| hdr.peak_nits)),
            )
        } else {
            (read_rows(&mapped, &description), ContentColour::SDR)
        };
        unsafe { self.context.Unmap(&staging, 0) };
        Ok(ScreenFrame {
            width: description.Width,
            height: description.Height,
            pixels,
            colour,
            output: self.output.clone(),
            scale: self.scale,
        })
    }
}

/// The WinRT wrapper a frame pool is created from.
fn direct_device(device: &ID3D11Device) -> Result<IDirect3DDevice, String> {
    let dxgi: IDXGIDevice = device
        .cast()
        .map_err(|error| format!("IDXGIDevice: {error}"))?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
        .map_err(|error| format!("CreateDirect3D11DeviceFromDXGIDevice: {error}"))?;
    inspectable
        .cast()
        .map_err(|error| format!("IDirect3DDevice: {error}"))
}
