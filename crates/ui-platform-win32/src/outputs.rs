//! What Windows says about an output: its HDR state and the SDR white it shows.

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SDR_WHITE_LEVEL, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
use windows::Win32::Graphics::Dxgi::{IDXGIFactory1, IDXGIFactory2, IDXGIOutput6};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::core::Interface as _;

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

/// The output behind `monitor`, when Windows is presenting HDR there.
pub(crate) fn hdr_output(factory: &IDXGIFactory2, monitor: HMONITOR) -> Option<HdrOutput> {
    if monitor.is_invalid() {
        return None;
    }
    let factory: IDXGIFactory1 = factory.cast().ok()?;
    for_each_output(&factory, |_, description| {
        if description.Monitor != monitor {
            return None;
        }
        // The desktop composes in PQ only once HDR is switched on for this display; a panel
        // that can reach 600 nits still describes itself as sRGB until then.
        if description.ColorSpace != DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 {
            return None;
        }
        Some(HdrOutput {
            // 80 nits is the configuration's own unit, so it is the honest value when the
            // query that would refine it fails.
            reference_nits: sdr_white_nits(monitor).unwrap_or(80.0),
            min_nits: description.MinLuminance,
            peak_nits: description.MaxLuminance,
        })
    })
}

/// The first output whose description `visit` answers, walking every adapter.
pub(crate) fn for_each_output<T>(
    factory: &IDXGIFactory1,
    mut visit: impl FnMut(&IDXGIOutput6, &windows::Win32::Graphics::Dxgi::DXGI_OUTPUT_DESC1) -> Option<T>,
) -> Option<T> {
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
            if let Some(answer) = visit(&output, &description) {
                return Some(answer);
            }
        }
    }
    None
}

/// The nits this display shows SDR white at, which is what linear 1.0 means in the scene.
///
/// `DXGI_OUTPUT_DESC1` has no field for it: the value belongs to the display configuration,
/// on the path whose GDI device name is the monitor's.
pub(crate) fn sdr_white_nits(monitor: HMONITOR) -> Option<f32> {
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
pub(crate) fn wide_name(name: &[u16]) -> &[u16] {
    let end = name.iter().position(|unit| *unit == 0).unwrap_or(name.len());
    &name[..end]
}
