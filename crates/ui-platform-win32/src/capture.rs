//! Screen capture through DXGI Desktop Duplication.
//!
//! The duplication hands over the desktop's own texture, on the adapter that drives the
//! output — a device on any other adapter duplicates a black frame on a hybrid laptop. One
//! output at a time: the caller names it or takes the primary.

use std::time::{Duration, Instant};

use ui_core::colour::{ContentColour, TransferFunction};
use windows::Win32::Foundation::{HMODULE, POINT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_MODE_ROTATION,
    DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_ROTATE90, DXGI_MODE_ROTATION_ROTATE270,
    DXGI_MODE_ROTATION_UNSPECIFIED,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_ACCESS_DENIED, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT,
    DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter, IDXGIFactory1, IDXGIOutput, IDXGIOutput1, IDXGIOutput5,
    IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::Interface as _;

use crate::outputs::wide_name;

/// One grabbed desktop frame, top-down, rows packed tight.
pub struct ScreenFrame {
    pub width: u32,
    pub height: u32,
    /// Straight red, green, blue, alpha, four bytes per pixel, already rotated upright.
    pub pixels: Vec<u8>,
    /// What the pixels mean. A float desktop is converted to these sRGB codes, which clips
    /// light past SDR white, so an HDR scene arrives without its highlights.
    pub colour: ContentColour,
    /// The output's GDI device name, such as `\\.\DISPLAY1`.
    pub output: String,
    /// The output's device pixels per logical point.
    pub scale: f32,
}

impl ScreenFrame {
    /// Bytes between rows in `pixels`, which are tight.
    pub fn stride(&self) -> u32 {
        self.width * 4
    }
}

/// A duplicated output, kept open to grab more than one frame.
pub struct ScreenCapture {
    duplication: IDXGIOutputDuplication,
    context: ID3D11DeviceContext,
    /// The copy the CPU reads; recreated when the desktop's size or format changes.
    staging: Option<ID3D11Texture2D>,
    output: String,
    scale: f32,
    /// The nits this display shows SDR white at: the desktop's linear float surface is
    /// referenced to 80 nits, so this is the scale between the two.
    reference_nits: f32,
    rotation: DXGI_MODE_ROTATION,
}

impl ScreenCapture {
    /// Duplicate `output` by GDI device name, or the primary output when none is named.
    pub fn open(output: Option<&str>) -> Result<Self, String> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| format!("CreateDXGIFactory1: {error}"))?;
        let (adapter, output, name) = find_output(&factory, output)?;
        let monitor = unsafe { output.GetDesc() }
            .map_err(|error| format!("output description: {error}"))?
            .Monitor;
        let scale = monitor_scale(monitor);
        let (device, context) = device_on(&adapter)?;
        let duplication = duplicate(&output, &device, &name)?;
        let description = unsafe { duplication.GetDesc() };
        let reference_nits = crate::outputs::sdr_white_nits(monitor).unwrap_or(80.0);
        Ok(Self {
            duplication,
            context,
            staging: None,
            output: name,
            scale,
            reference_nits,
            rotation: description.Rotation,
        })
    }

    /// The GDI device name of the output being duplicated.
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Device pixels per logical point on that output.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Grab the next desktop frame, waiting at most `timeout` for the desktop to change.
    pub fn grab(&mut self, timeout: Duration) -> Result<ScreenFrame, String> {
        let started = Instant::now();
        loop {
            let waited = started.elapsed();
            if waited >= timeout {
                return Err(String::from(
                    "the desktop did not change before the deadline",
                ));
            }
            let remaining = timeout - waited;
            let milliseconds = u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX);
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            let acquired = unsafe {
                self.duplication
                    .AcquireNextFrame(milliseconds, &mut info, &mut resource)
            };
            if let Err(error) = acquired {
                if error.code() == DXGI_ERROR_WAIT_TIMEOUT {
                    continue;
                }
                return Err(if error.code() == DXGI_ERROR_ACCESS_LOST {
                    String::from("the duplication was lost: the desktop's mode or session changed")
                } else if error.code() == DXGI_ERROR_ACCESS_DENIED {
                    String::from("another application is holding the duplication")
                } else {
                    format!("AcquireNextFrame: {error}")
                });
            }
            // A frame whose present time is zero carries the pointer and nothing else; its
            // image is unchanged, so the first one after `DuplicateOutput` is the empty frame.
            let taken = match (info.LastPresentTime, resource) {
                (0, _) => Ok(None),
                (_, None) => Err(String::from("the duplication returned no frame")),
                (_, Some(resource)) => self.copy(&resource).map(Some),
            };
            // Every acquired frame is owed back, or the next acquire fails.
            let _ = unsafe { self.duplication.ReleaseFrame() };
            let Some((width, height, pixels)) = taken? else {
                continue;
            };
            let (width, height, pixels) = rotate(self.rotation, width, height, pixels);
            return Ok(ScreenFrame {
                width,
                height,
                pixels,
                colour: ContentColour::SDR,
                output: self.output.clone(),
                scale: self.scale,
            });
        }
    }

    /// Copy the desktop texture into an SDR frame the CPU can read.
    fn copy(&mut self, resource: &IDXGIResource) -> Result<(u32, u32, Vec<u8>), String> {
        let texture: ID3D11Texture2D = resource
            .cast()
            .map_err(|error| format!("desktop texture: {error}"))?;
        let mut description = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut description) };
        let float = match description.Format {
            DXGI_FORMAT_B8G8R8A8_UNORM => false,
            // The surface an advanced colour desktop composes in: linear light in sRGB
            // primaries, where 1.0 is 80 nits.
            DXGI_FORMAT_R16G16B16A16_FLOAT => true,
            other => {
                return Err(format!(
                    "the desktop is in {other:?}, which is neither 8-bit sRGB nor half-float"
                ));
            }
        };
        let staging = match &self.staging {
            Some(staging) if self.matches(staging, &description) => staging.clone(),
            _ => {
                let staging = self.create_staging(&description)?;
                self.staging = Some(staging.clone());
                staging
            }
        };
        unsafe { self.context.CopyResource(&staging, &texture) };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|error| format!("map the desktop frame: {error}"))?;
        let tight = if float {
            self.read_float_rows(&mapped, &description)
        } else {
            self.read_rows(&mapped, &description)
        };
        unsafe { self.context.Unmap(&staging, 0) };
        Ok((description.Width, description.Height, tight))
    }

    /// Whether the cached copy already has the desktop's size and packing.
    fn matches(&self, staging: &ID3D11Texture2D, wanted: &D3D11_TEXTURE2D_DESC) -> bool {
        let mut current = D3D11_TEXTURE2D_DESC::default();
        unsafe { staging.GetDesc(&mut current) };
        current.Width == wanted.Width
            && current.Height == wanted.Height
            && current.Format == wanted.Format
    }

    fn create_staging(&self, wanted: &D3D11_TEXTURE2D_DESC) -> Result<ID3D11Texture2D, String> {
        let description = D3D11_TEXTURE2D_DESC {
            Width: wanted.Width,
            Height: wanted.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: wanted.Format,
            SampleDesc: wanted.SampleDesc,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging: Option<ID3D11Texture2D> = None;
        // The context, not the device: both belong to the device the duplication was made on.
        let device = unsafe { self.context.GetDevice() }
            .map_err(|error| format!("the duplication's device: {error}"))?;
        unsafe { device.CreateTexture2D(&description, None, Some(&mut staging)) }
            .map_err(|error| format!("staging texture: {error}"))?;
        staging.ok_or_else(|| String::from("CreateTexture2D returned no texture"))
    }

    /// The float desktop as sRGB codes the content had.
    ///
    /// Its linear light is referenced to 80 nits and SDR white to the display's own, so
    /// dividing by that scale returns the codes a window drew. Light past SDR white clips.
    fn read_float_rows(
        &self,
        mapped: &D3D11_MAPPED_SUBRESOURCE,
        size: &D3D11_TEXTURE2D_DESC,
    ) -> Vec<u8> {
        let width = size.Width as usize;
        let rows = size.Height as usize;
        let to_white = 80.0 / self.reference_nits;
        let mut pixels = Vec::with_capacity(width * rows * 4);
        let pitch = mapped.RowPitch as usize;
        for row in 0..rows {
            let source = unsafe { (mapped.pData as *const u8).add(row * pitch) };
            // SAFETY: as in `read_rows`, the row holds the texture's texels for its width.
            let row_bytes = unsafe { std::slice::from_raw_parts(source, width * 8) };
            for texel in row_bytes.as_chunks::<8>().0 {
                for channel in 0..3 {
                    let at = channel * 2;
                    let bits = u16::from_le_bytes([texel[at], texel[at + 1]]);
                    let linear = (from_half(bits) * to_white).clamp(0.0, 1.0);
                    let code = TransferFunction::Srgb.encode(linear) * 255.0;
                    pixels.push(code.round() as u8);
                }
                pixels.push(255);
            }
        }
        pixels
    }

    /// The mapped rows as straight RGBA, each row copied out of its own pitch.
    fn read_rows(&self, mapped: &D3D11_MAPPED_SUBRESOURCE, size: &D3D11_TEXTURE2D_DESC) -> Vec<u8> {
        let width = size.Width as usize;
        let rows = size.Height as usize;
        let mut pixels = Vec::with_capacity(width * rows * 4);
        let pitch = mapped.RowPitch as usize;
        for row in 0..rows {
            let source = unsafe { (mapped.pData as *const u8).add(row * pitch) };
            // SAFETY: D3D11 guarantees a mapped texture is readable for `RowPitch` bytes per
            // row, and the row holds four bytes per texel of the width it was created for.
            let row_pixels = unsafe { std::slice::from_raw_parts(source, width * 4) };
            for texel in row_pixels.as_chunks::<4>().0 {
                pixels.extend_from_slice(&[texel[2], texel[1], texel[0], texel[3]]);
            }
        }
        pixels
    }
}

/// The adapter and output a grab reads, with the output's GDI device name.
fn find_output(
    factory: &IDXGIFactory1,
    wanted: Option<&str>,
) -> Result<(IDXGIAdapter, IDXGIOutput, String), String> {
    let primary = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut seen = Vec::new();
    let mut index = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        let mut number = 0;
        while let Ok(output) = unsafe { adapter.EnumOutputs(number) } {
            number += 1;
            let Ok(description) = (unsafe { output.GetDesc() }) else {
                continue;
            };
            if !description.AttachedToDesktop.as_bool() {
                continue;
            }
            let name = String::from_utf16_lossy(wide_name(&description.DeviceName));
            let chosen = match wanted {
                Some(wanted) => name.eq_ignore_ascii_case(wanted),
                None => description.Monitor == primary,
            };
            if chosen {
                let adapter = adapter
                    .cast()
                    .map_err(|error| format!("IDXGIAdapter: {error}"))?;
                return Ok((adapter, output, name));
            }
            seen.push(name);
        }
    }
    Err(match wanted {
        Some(wanted) => format!("no output named {wanted}; present: {}", seen.join(", ")),
        None => String::from("no output is attached to the desktop"),
    })
}

/// The duplication, asking for the float surface an advanced colour desktop composes in.
///
/// `DuplicateOutput` converts that surface to BGRA8 by clipping, so on an auto colour managed
/// display every SDR window comes back over-exposed; only `DuplicateOutput1` asks for floats.
fn duplicate(
    output: &IDXGIOutput,
    device: &ID3D11Device,
    name: &str,
) -> Result<IDXGIOutputDuplication, String> {
    if let Ok(output5) = output.cast::<IDXGIOutput5>() {
        let formats = [DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_B8G8R8A8_UNORM];
        if let Ok(duplication) = unsafe { output5.DuplicateOutput1(device, 0, &formats) } {
            return Ok(duplication);
        }
    }
    let output1: IDXGIOutput1 = output
        .cast()
        .map_err(|error| format!("IDXGIOutput1: {error}"))?;
    unsafe { output1.DuplicateOutput(device) }.map_err(|error| {
        // Another duplicator, a protected window, or a driver that only allows a few
        // sessions: all of them arrive as the same call failing.
        format!("DuplicateOutput on {name}: {error}")
    })
}

/// A binary16 value as a float, which is the only half-float this crate reads.
fn from_half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = (bits & 0x3ff) as f32;
    match exponent {
        // Subnormals scale from 2^-14 with no implicit leading bit.
        0 => sign * mantissa * (1.0 / 16_777_216.0),
        0x1f if mantissa == 0.0 => sign * f32::INFINITY,
        0x1f => f32::NAN,
        _ => sign * (1.0 + mantissa / 1024.0) * f32::exp2(exponent as f32 - 15.0),
    }
}

/// A D3D11 device on `adapter`, which duplication requires.
fn device_on(adapter: &IDXGIAdapter) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let levels = [
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    ];
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    let created = unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    };
    created.map_err(|error| format!("D3D11CreateDevice on the output's adapter: {error}"))?;
    match (device, context) {
        (Some(device), Some(context)) => Ok((device, context)),
        _ => Err(String::from("D3D11CreateDevice returned no device")),
    }
}

/// Device pixels per logical point for `monitor`, 1.0 when Windows will not say.
fn monitor_scale(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> f32 {
    let (mut x, mut y) = (96u32, 96u32);
    let asked = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    if asked.is_err() || x == 0 {
        return 1.0;
    }
    x as f32 / 96.0
}

/// Turn the unrotated desktop image upright.
///
/// Duplication reports the desktop in the panel's own orientation: a portrait display
/// rotates the image rather than resizing it, so the pixels arrive sideways.
fn rotate(
    rotation: DXGI_MODE_ROTATION,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> (u32, u32, Vec<u8>) {
    if rotation == DXGI_MODE_ROTATION_IDENTITY
        || rotation == DXGI_MODE_ROTATION_UNSPECIFIED
        || pixels.len() != (width as usize) * (height as usize) * 4
    {
        return (width, height, pixels);
    }
    let (across, down) = (width as usize, height as usize);
    let (out_width, out_height) = match rotation {
        DXGI_MODE_ROTATION_ROTATE90 | DXGI_MODE_ROTATION_ROTATE270 => (down, across),
        _ => (across, down),
    };
    let mut upright = vec![0u8; pixels.len()];
    for y in 0..out_height {
        for x in 0..out_width {
            let (from_x, from_y) = match rotation {
                // A quarter turn clockwise: the desktop's last row becomes the first column.
                DXGI_MODE_ROTATION_ROTATE90 => (y, down - 1 - x),
                DXGI_MODE_ROTATION_ROTATE270 => (across - 1 - y, x),
                _ => (across - 1 - x, down - 1 - y),
            };
            let from = (from_y * across + from_x) * 4;
            let to = (y * out_width + x) * 4;
            upright[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
        }
    }
    (out_width as u32, out_height as u32, upright)
}
