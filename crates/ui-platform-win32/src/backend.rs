//! The process-wide half of the loop on Windows: one message pump, its surfaces, the clipboard.
//!
//! `Host` decides when to wake and what to step; this backend owns the thread's message queue
//! and the pacing point, and turns a portable `ClipboardCopy` into the system clipboard.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use ui_core::draw::ImageEntry;
use ui_platform::clipboard::{ClipboardCommand, ClipboardCopy};
use ui_platform::host::{Backend, OpenOptions, SurfaceHandle, SurfaceStep};
use ui_platform::hotkeys::GlobalHotkeys;
use ui_platform::ipc::Channel;
use ui_platform::jobs::Waker;
use ui_platform::output::OutputInfo;
use ui_platform::session::{HostSession, SessionOptions};
use ui_platform::transfer::SelectionReader;
use ui_platform::tray::{Icon, TrayEvent, TrayHandle, TrayId};
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
use windows::core::{BOOL, PCWSTR};

use crate::clipboard::ClipboardReader;
use crate::frame_pace::FramePace;
use crate::hotkeys::Win32Hotkeys;
use crate::session::Session;
use crate::tray::{TrayQueue, Win32TrayIcon};
use crate::window::{bounded_millis, pump_messages, set_system_clipboard};

/// Surfaces and services on one thread's message queue.
pub struct Win32Backend {
    /// Set by a worker to interrupt [`Backend::wait`]; absent when the event could not open.
    wake: Option<Arc<WakeEvent>>,
    /// The system-wide hotkeys, opened on the first request; absent when that failed.
    hotkeys: Option<Win32Hotkeys>,
    /// What the tray icons' windows reported; the loop drains it each turn.
    tray_events: TrayQueue,
    /// The system clipboard, read in place: a copy is already complete when it lands.
    reader: ClipboardReader,
    /// Every surface's frame window: the compositor wakes the loop through these. Weak, so
    /// a retired surface takes its entry with it; the wait prunes what is left.
    paces: Vec<Weak<FramePace>>,
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
            wake,
            hotkeys: None,
            tray_events: Rc::new(RefCell::new(Vec::new())),
            reader: ClipboardReader::default(),
            paces: Vec::new(),
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
        // The host's one sleep must end when this surface's render thread finishes a frame;
        // the session keeps the pace alive, so the entry needs no removal on retirement.
        self.paces.push(Arc::downgrade(&session.frame_pace()));
        Ok(Box::new(HostedSurface { session }))
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

    fn selection_reader(&mut self) -> Option<&mut (dyn SelectionReader + 'static)> {
        Some(&mut self.reader)
    }

    fn outputs(&mut self) -> Vec<OutputInfo> {
        outputs_with_monitors()
            .into_iter()
            .map(|(_, information)| information)
            .collect()
    }

    fn system_languages(&self) -> Vec<String> {
        crate::locale::system_languages()
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

    fn create_tray(&mut self, id: TrayId, icon: Icon) -> Result<Box<dyn TrayHandle>, String> {
        // The icon's own window joins this thread's message queue, which is the one the
        // loop already waits on; nothing else has to be woken for a click to arrive.
        let tray = Win32TrayIcon::create(id, icon, Rc::clone(&self.tray_events))?;
        Ok(Box::new(tray))
    }

    fn tray_events(&mut self) -> Vec<(TrayId, TrayEvent)> {
        std::mem::take(&mut *self.tray_events.borrow_mut())
    }

    fn wait(&mut self, timeout: Option<Duration>) -> Result<(), String> {
        // The queue's own messages, the worker's wake event and every surface's frame
        // event end the sleep. A frame event is what makes a surface drawable again:
        // consuming it here is the same act as a Wayland frame callback arriving.
        self.paces.retain(|pace| pace.strong_count() > 0);
        let live: Vec<Arc<FramePace>> = self.paces.iter().filter_map(Weak::upgrade).collect();
        let mut handles: Vec<HANDLE> = Vec::with_capacity(live.len() + 1);
        if let Some(wake) = &self.wake {
            handles.push(wake.0);
        }
        for pace in &live {
            if let Some(handle) = pace.handle() {
                handles.push(handle);
            }
        }
        let millis = bounded_millis(timeout);
        // SAFETY: every handle is live for the duration of the wait: the wake event is
        // owned here, and a pace's event belongs to its session, which the host keeps alive.
        let _ = unsafe {
            MsgWaitForMultipleObjectsEx(Some(handles.as_slice()), millis, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        // The wait consumed the auto-reset object it returned on; any other surface whose
        // render thread finished at the same moment is drained here. A timeout opens
        // nothing: the render thread finishes its frame whatever the compositor does, so a
        // pace that is still closed has a frame genuinely in flight.
        for pace in &live {
            pace.take_signal();
        }
        // Dispatched here as well as per surface: a hotkey arrives at a window of this
        // process that no surface owns, and a loop with no surfaces would never deliver it.
        pump_messages();
        Ok(())
    }

    fn flush(&mut self) {
        // Nothing waits here any more: the frame the turn built is with its render thread,
        // and the next turn waits on that thread's event when it has one to wait for.
    }
}

/// One surface the host opened: the session and nothing else.
struct HostedSurface {
    session: Session<Box<dyn App>>,
}

impl SurfaceHandle for HostedSurface {
    fn add_image(&mut self, image: ImageEntry) -> Result<(), String> {
        self.session.lp_mut().register_image(image);
        Ok(())
    }

    fn step(&mut self) -> Result<SurfaceStep, String> {
        let tick = self.session.step()?;
        Ok(SurfaceStep { drew: tick.drew })
    }

    fn moving(&self) -> bool {
        self.session.moving()
    }

    fn pump_moved(&mut self) -> bool {
        self.session.pump_moved()
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

/// Every monitor Windows reports, with the output description the host lists it as.
///
/// The order is the enumerator's, which is the order `Backend::outputs` answers in; a monitor
/// whose description cannot be read is left out of both, so a name still finds its monitor.
pub(crate) fn outputs_with_monitors() -> Vec<(HMONITOR, OutputInfo)> {
    let mut found: Vec<(HMONITOR, OutputInfo)> = Vec::new();
    let data = LPARAM((&mut found as *mut Vec<(HMONITOR, OutputInfo)>) as isize);
    // SAFETY: the callback writes through `data` while this call is on the stack, and every
    // monitor it visits outlives the enumeration.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect_output), data);
    }
    found
}

/// The monitor an output name refers to.
pub(crate) fn monitor_named(name: &str) -> Option<HMONITOR> {
    outputs_with_monitors()
        .into_iter()
        .find(|(_, information)| information.name == name)
        .map(|(monitor, _)| monitor)
}

/// Gather one monitor into the host's output list.
unsafe extern "system" fn collect_output(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: the enumerator passes back the pointer it was given, which the caller keeps on
    // the stack for the whole call.
    let found = unsafe { &mut *(data.0 as *mut Vec<(HMONITOR, OutputInfo)>) };
    if let Some(info) = output_of(monitor) {
        found.push((monitor, info));
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

