//! The process-wide half of the loop on Windows: one message pump, its surfaces, the clipboard.
//!
//! `Host` decides when to wake and what to step; this backend owns the thread's message queue
//! and the pacing point, and turns a portable `ClipboardCopy` into the system clipboard.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_core::draw::ImageEntry;
use ui_platform::clipboard::{ClipboardCommand, ClipboardCopy};
use ui_platform::host::{Backend, OpenOptions, SurfaceHandle, SurfaceStep};
use ui_platform::hotkeys::GlobalHotkeys;
use ui_platform::ipc::Channel;
use ui_platform::jobs::Waker;
use ui_platform::output::OutputInfo;
use ui_platform::session::{HostSession, SessionOptions};
use ui_platform::view::{App, AppLoop};
use ui_platform::window::SurfaceSpec;
use windows::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, DMDO_90, DMDO_180, DMDO_270, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors,
    EnumDisplaySettingsW, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, QS_ALLINPUT,
};
use windows::core::{BOOL, PCWSTR, PWSTR};

use crate::hotkeys::Win32Hotkeys;
use crate::session::Session;
use crate::window::{bounded_millis, pump_messages, set_system_clipboard, wait_for_composition};

/// Surfaces and services on one thread's message queue.
pub struct Win32Backend {
    /// Shared with every surface: something presented this turn, so the pace has work.
    drew: Rc<Cell<bool>>,
    /// Set by a worker to interrupt [`Backend::wait`]; absent when the event could not open.
    wake: Option<Arc<WakeEvent>>,
    /// The system-wide hotkeys, opened on the first request; absent when that failed.
    hotkeys: Option<Win32Hotkeys>,
}

/// The loop's wake channel from the other side: an event any worker can set.
struct WakeEvent(HANDLE);

// A kernel event handle is a process-wide index, not a pointer into thread state.
unsafe impl Send for WakeEvent {}
unsafe impl Sync for WakeEvent {}

impl Drop for WakeEvent {
    fn drop(&mut self) {
        // Closing after the wait is over; a failure here has nothing left to repair.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl Waker for WakeEvent {
    fn wake(&self) {
        // An event already set is still a wake; a refused one has nothing left to repair.
        let _ = unsafe { SetEvent(self.0) };
    }
}

impl Win32Backend {
    /// Open the backend. The message queue belongs to the thread and already exists, so this
    /// only creates the event a worker sets to wake the loop.
    pub fn new() -> Self {
        let wake = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .ok()
            .map(|handle| Arc::new(WakeEvent(handle)));
        Self {
            drew: Rc::new(Cell::new(false)),
            wake,
            hotkeys: None,
        }
    }
}

impl Default for Win32Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for Win32Backend {
    fn open(
        &mut self,
        spec: SurfaceSpec,
        app: Box<dyn App>,
        options: OpenOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let mut lp = AppLoop::with_stack(app, &[]);
        for image in options.images {
            lp.register_image(image);
        }
        for face in options.fonts {
            if !lp.register_font(&face.name, face.data) {
                return Err(format!("the font {} could not be registered", face.name));
            }
        }
        for (chord, id) in options.shortcuts {
            lp.bind_shortcut(chord, id);
        }
        let session = Session::open(
            lp,
            SessionOptions {
                surface: spec,
                uncapped: options.uncapped,
                cpu_presenter: false,
            },
        )?;
        Ok(Box::new(HostedSurface {
            session,
            drew: Rc::clone(&self.drew),
        }))
    }

    fn set_clipboard_copy(&mut self, copy: ClipboardCopy) -> bool {
        // The system clipboard is not a selection protocol: the data is copied in whole and
        // every process reads its own copy, so the serial has nowhere to go.
        set_system_clipboard(copy.content)
    }

    /// Nothing to answer: the data is already on the clipboard, not held on request.
    fn serve_clipboard(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn outputs(&mut self) -> Vec<OutputInfo> {
        let mut found: Vec<OutputInfo> = Vec::new();
        let data = LPARAM((&mut found as *mut Vec<OutputInfo>) as isize);
        // SAFETY: the callback writes through `data` while this call is on the stack, and
        // every monitor it visits outlives the enumeration.
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(collect_output), data);
        }
        found
    }

    fn system_languages(&self) -> Vec<String> {
        preferred_ui_languages().unwrap_or_default()
    }

    fn waker(&mut self) -> Option<Arc<dyn Waker>> {
        self.wake
            .as_ref()
            .map(|wake| Arc::clone(wake) as Arc<dyn Waker>)
    }

    fn listen(&mut self, name: &str) -> Result<Box<dyn Channel>, String> {
        let waker = self.waker();
        Ok(Box::new(crate::ipc::open(name, waker)?))
    }

    fn hotkeys(&mut self) -> Option<&mut (dyn GlobalHotkeys + 'static)> {
        if self.hotkeys.is_none() {
            // A sink that cannot be created is a platform with no hotkeys, which is what the
            // `Option` answer means; the app reports the chord as unsupported.
            self.hotkeys = Win32Hotkeys::open().ok();
        }
        self.hotkeys
            .as_mut()
            .map(|hotkeys| hotkeys as &mut (dyn GlobalHotkeys + 'static))
    }

    fn wait(&mut self, timeout: Option<Duration>) -> Result<(), String> {
        // The queue's own messages and the wake event are what end the sleep.
        let handles = [self.wake.as_ref().map_or(HANDLE::default(), |wake| wake.0)];
        let given = if self.wake.is_some() {
            &handles[..]
        } else {
            &handles[..0]
        };
        let millis = bounded_millis(timeout);
        // SAFETY: `given` is a live slice of handles, or empty.
        unsafe {
            MsgWaitForMultipleObjectsEx(Some(given), millis, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        }
        // Dispatched here as well as per surface: a hotkey arrives at a window of this
        // process that no surface owns, and a loop with no surfaces would never deliver it.
        pump_messages();
        Ok(())
    }

    fn flush(&mut self) {
        // The process's one pacing point: every frame presented this turn is taken by the
        // next composition pass, which is what a Wayland frame callback marks.
        if self.drew.replace(false) {
            wait_for_composition();
        }
    }
}

/// One surface the host opened: the session plus the turn's shared "something drew" flag.
struct HostedSurface {
    session: Session<Box<dyn App>>,
    drew: Rc<Cell<bool>>,
}

impl SurfaceHandle for HostedSurface {
    fn add_image(&mut self, image: ImageEntry) -> Result<(), String> {
        self.session.lp_mut().register_image(image);
        Ok(())
    }

    fn step(&mut self) -> Result<SurfaceStep, String> {
        let tick = self.session.step()?;
        if tick.drew {
            self.drew.set(true);
        }
        Ok(SurfaceStep { drew: tick.drew })
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.session.next_deadline()
    }

    fn request_frame(&mut self) {
        self.session.lp_mut().request_frame();
    }

    fn is_closed(&self) -> bool {
        self.session.finished()
    }

    fn take_copies(&mut self) -> Vec<ClipboardCopy> {
        let mut copies = Vec::new();
        for command in self.session.lp_mut().take_clipboard_commands() {
            match command {
                ClipboardCommand::Copy { content, kind } => copies.push(ClipboardCopy {
                    content,
                    kind,
                    // A copy here is the clipboard itself; there is no serial to serve under.
                    serial: 0,
                }),
                // Windows has no primary selection, so a paste is always the clipboard's.
                ClipboardCommand::RequestPaste { .. } => {
                    let _ = self.session.request_paste();
                }
            }
        }
        copies
    }
}

/// Gather one monitor into the host's output list.
unsafe extern "system" fn collect_output(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: the enumerator passes back the pointer it was given, which `outputs` keeps on
    // the stack for the whole call.
    let found = unsafe { &mut *(data.0 as *mut Vec<OutputInfo>) };
    if let Some(info) = output_of(monitor) {
        found.push(info);
    }
    BOOL(1)
}

fn output_of(monitor: HMONITOR) -> Option<OutputInfo> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) }.as_bool() {
        return None;
    }
    let rect = info.monitorInfo.rcMonitor;
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    let scale =
        match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) } {
            Ok(()) if dpi_x > 0 => dpi_x as f32 / 96.0,
            // A monitor that reports no DPI is at the system's own scale.
            _ => 1.0,
        };
    let (refresh_mhz, rotated) = display_mode(&info);
    let (width, height) = (
        (rect.right - rect.left).max(1) as u32,
        (rect.bottom - rect.top).max(1) as u32,
    );
    Some(OutputInfo {
        name: device_name(&info),
        // Windows reports no make or model without reading the monitor's EDID.
        description: String::new(),
        x: (rect.left as f32 / scale).round() as i32,
        y: (rect.top as f32 / scale).round() as i32,
        width: (width as f32 / scale).round() as u32,
        height: (height as f32 / scale).round() as u32,
        scale,
        refresh_mhz,
        rotated,
    })
}

/// The monitor's device name, such as `\\.\DISPLAY1`.
fn device_name(info: &MONITORINFOEXW) -> String {
    let end = info
        .szDevice
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(info.szDevice.len());
    String::from_utf16_lossy(&info.szDevice[..end])
}

/// The monitor's current refresh rate in millihertz and whether it is turned a quarter turn.
fn display_mode(info: &MONITORINFOEXW) -> (u32, bool) {
    let mut mode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `szDevice` is the name the monitor info just filled, and `mode` is a local.
    let ok = unsafe {
        EnumDisplaySettingsW(
            PCWSTR::from_raw(info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut mode,
        )
    };
    if !ok.as_bool() {
        return (0, false);
    }
    // 0 and 1 are the driver's "hardware default", not a measurable rate.
    let refresh = if mode.dmDisplayFrequency > 1 {
        mode.dmDisplayFrequency * 1000
    } else {
        0
    };
    // SAFETY: the current settings always fill the display union, which is why they were read.
    let orientation = unsafe { mode.Anonymous1.Anonymous2.dmDisplayOrientation };
    let rotated = matches!(orientation, DMDO_90 | DMDO_180 | DMDO_270);
    (refresh, rotated)
}

/// The languages the user's own profile prefers, most preferred first.
fn preferred_ui_languages() -> Option<Vec<String>> {
    use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0u32;
    let mut units = 0u32;
    // The first call asks for the buffer's size; both numbers come back together.
    unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut units) }.ok()?;
    if count == 0 || units == 0 {
        return None;
    }
    let mut buffer = vec![0u16; units as usize];
    // SAFETY: the buffer is the size the first call asked for, and it outlives this call.
    unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut units,
        )
    }
    .ok()?;
    Some(
        buffer
            .split(|&unit| unit == 0)
            .filter(|tag| !tag.is_empty())
            .map(String::from_utf16_lossy)
            .collect(),
    )
}
