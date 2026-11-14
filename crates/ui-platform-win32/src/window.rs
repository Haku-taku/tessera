//! A Win32 window: class registration, creation, the message pump, input and the extent.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use ui_core::clipboard::ClipboardContent;
use ui_core::geom::{Point, Rect};
use ui_core::{CursorShape, Event};
use ui_platform::ThemeWatcher as _;
use ui_platform::driver::FrameHost;
use ui_platform::present::ImeState;
use ui_platform::window::{
    AutoHide, KeyboardMode, Layer, LayerSpec, SurfaceRole, SurfaceSpec,
};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_USE_IMMERSIVE_DARK_MODE, DwmFlush, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CombineRgn, CreateRectRgn, DEVMODEW, DeleteObject, ENUM_CURRENT_SETTINGS,
    EnumDisplaySettingsW, GetMonitorInfoW, HBRUSH, HGDIOBJ, HMONITOR, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
    MonitorFromRect, MonitorFromWindow, PAINTSTRUCT, RGN_OR, ScreenToClient, SetWindowRgn,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
    GetDpiForSystem, GetDpiForWindow, MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_ESCAPE,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETSTATE, ABM_GETTASKBARPOS, ABS_AUTOHIDE,
    APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWL_EXSTYLE, GWL_STYLE, GWLP_USERDATA, GetClientRect, GetCursorPos, GetForegroundWindow,
    GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT,
    HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, IDC_CROSS, IDC_HAND,
    IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IsWindow,
    LoadCursorW, MA_ACTIVATE, MA_NOACTIVATE, MINMAXINFO, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PM_NOREMOVE, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    RegisterClassExW, SM_CXMINTRACK, SM_CYMINTRACK, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOZORDER, SetCursor, SetCursorPos, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CAPTURECHANGED,
    WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND,
    WM_GETMINMAXINFO,
    WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION,
    WM_KEYDOWN, WM_KEYUP,
    WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEACTIVATE,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WM_PAINT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_OVERLAPPEDWINDOW, WS_POPUP,
};
use windows::core::PCWSTR;

use crate::{WindowError, input};

/// The one message this backend uses that the syscall-binding module does not declare:
/// `WM_MOUSELEAVE` lives with the common controls, whose whole API surface is not worth a
/// feature flag for one constant.
const WM_MOUSELEAVE: u32 = 0x02A3;

/// The strip an auto-hidden taskbar needs left free, in physical pixels.
///
/// The shell will not reveal a hidden taskbar while a window covers the monitor edge it hides
/// against, so every toolkit that fills the screen — Chromium, Windows Terminal — keeps this
/// much of that edge for the desktop.
const AUTO_HIDE_STRIP: i32 = 2;

/// What the window procedure has collected since the loop last looked.
///
/// Windows hands a window its messages on the thread that created it, so this state is
/// confined to that thread and shared with the procedure by raw pointer, not by `Arc`.
#[derive(Default)]
struct WindowState {
    events: Vec<Event>,
    pointer: Option<Point>,
    modifiers: ui_core::Modifiers,
    close_requested: bool,
    cursor: Option<CursorShape>,
    /// A `WM_MOUSELEAVE` is armed; Windows sends it once per entry.
    tracking_leave: bool,
    /// Focus, extent or scale changed without an `Event`; a frame is still owed.
    silent_change: bool,
    /// The display mode or topology changed: what the output presents has to be re-read.
    display_changed: bool,
    /// Physical pixels per logical unit, in whole DPI.
    dpi: u32,
    /// The smallest client area the spec allows, in logical units.
    min_size: Option<(u32, u32)>,
    /// A layer's keyboard request; `None` for a window the manager places, which has no
    /// shape and no activation rule of its own.
    keyboard: Option<KeyboardMode>,
    /// The input-method state the last frame published, so one change applies once.
    ime: Option<ImeState>,
    /// Whether the input context is open, which is the last state that reached it.
    ime_open: bool,
    /// The buffer is secret: the input method's own candidate list is hidden too.
    ime_hidden: bool,
    /// The TSF document this window hands an input method, once a text field has asked.
    tsf: Option<Rc<RefCell<crate::tsf::Session>>>,
    /// TSF could not be brought up: the window keeps IMM32 for the rest of its life.
    tsf_failed: bool,
    /// The areas the last frame's tree consumes, surface-local logical units; `None` until a
    /// frame publishes one, which leaves the whole surface consuming input.
    input: Option<Vec<Rect>>,
    /// The window that held the foreground when the pointer arrived on a consumed area.
    previous: Option<HWND>,
    /// A sizing drag this window runs itself; see `begin_sizing`.
    sizing: Option<Sizing>,
}

/// A border drag: the hit test that began it, and where the pointer and window were then.
#[derive(Clone, Copy)]
struct Sizing {
    edge: u32,
    /// Screen pixels, which is what a non-client press carries.
    cursor: POINT,
    /// The window's rectangle on screen, restored when the drag is cancelled.
    rect: RECT,
}

/// A window the window manager places, decorates and stacks.
///
/// Dropping it destroys the window, so a presenter that draws into it must be dropped first.
pub struct Win32Window {
    /// The window that takes messages: focus, the input shape, the IME and the clipboard.
    hwnd: HWND,
    /// The window the swap chain presents through. A layer draws in a second window, held
    /// above this one and skipped by every hit test, so that what is painted and what is
    /// clicked can be shaped differently.
    visual: HWND,
    state: Rc<RefCell<WindowState>>,
    /// What OLE's drop target posts while a drag crosses this window.
    drag: Rc<crate::dnd::DragQueue>,
    /// The registered drop target; withdrawal must precede the window's destruction.
    drop_target: Option<windows::Win32::System::Ole::IDropTarget>,
    /// The surface reaches the desktop through a compositor, so the frame's own alpha
    /// decides what is seen: a layer, not a decorated window.
    composited: bool,
}

impl Win32Window {
    /// Open the window `spec` asks for: a window the manager places, or a layer this backend
    /// approximates with an undecorated popup held at the level the spec names.
    pub fn new(spec: SurfaceSpec) -> Result<Self, WindowError> {
        // A manifest may already have fixed the process's awareness; a refusal means it did.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let app_id = if spec.app_id.is_empty() {
            "crabsnap"
        } else {
            spec.app_id.as_str()
        };
        let class = wide(app_id);
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|error| WindowError::Win32(format!("GetModuleHandleW: {error}")))?;
        let instance = HINSTANCE(instance.0);
        register_class(&class, instance)?;
        let opening = match &spec.role {
            SurfaceRole::Window { size } => opening_window(*size)?,
            SurfaceRole::Layer(layer) => opening_layer(layer)?,
        };
        let title = wide(&spec.name);
        let state = Rc::new(RefCell::new(WindowState {
            dpi: opening.dpi,
            keyboard: opening.keyboard,
            min_size: spec.min_size,
            ..WindowState::default()
        }));
        // The procedure needs the state from `WM_NCCREATE` on, before `CreateWindowExW`
        // returns; the extra count is released in `WM_NCDESTROY`, which runs even on failure
        // of a later step in creation.
        let raw = Rc::as_ptr(&state);
        unsafe { Rc::increment_strong_count(raw) };
        let hwnd = unsafe {
            CreateWindowExW(
                opening.ex_style,
                PCWSTR::from_raw(class.as_ptr()),
                PCWSTR::from_raw(title.as_ptr()),
                opening.style,
                opening.x,
                opening.y,
                opening.width,
                opening.height,
                None,
                None,
                Some(instance),
                Some(raw as *const core::ffi::c_void),
            )
        };
        let hwnd = match hwnd {
            Ok(hwnd) => hwnd,
            Err(error) => {
                // No window exists to end, so the count this function added is still owed.
                drop(unsafe { Rc::from_raw(raw) });
                return Err(WindowError::Win32(format!("CreateWindowExW: {error}")));
            }
        };
        // A layer's pixels are a second window: owned, so it stays above and dies with this
        // one, and styled to be invisible to the hit test, which this window answers.
        let visual = if opening.keyboard.is_some() {
            unsafe {
                CreateWindowExW(
                    opening.ex_style | WS_EX_LAYERED | WS_EX_TRANSPARENT,
                    PCWSTR::from_raw(class.as_ptr()),
                    PCWSTR::from_raw(title.as_ptr()),
                    WS_POPUP,
                    opening.x,
                    opening.y,
                    opening.width,
                    opening.height,
                    Some(hwnd),
                    None,
                    Some(instance),
                    None,
                )
            }
        } else {
            Ok(hwnd)
        };
        let visual = match visual {
            Ok(visual) => visual,
            Err(error) => {
                let _ = unsafe { DestroyWindow(hwnd) };
                return Err(WindowError::Win32(format!("CreateWindowExW: {error}")));
            }
        };
        // The return value says whether the window was previously visible, which is not
        // an outcome this backend has a use for.
        unsafe {
            let command = if opening.keyboard.is_some() {
                // Showing a layer must not take the keyboard away from the desktop; only an
                // exclusive one asks for it, and even then as a separate step.
                SW_SHOWNOACTIVATE
            } else {
                SW_SHOW
            };
            let _ = ShowWindow(hwnd, command);
            if visual != hwnd {
                let _ = ShowWindow(visual, SW_SHOWNOACTIVATE);
            }
            if opening.keyboard == Some(KeyboardMode::Exclusive) {
                // A hotkey press is what usually makes this succeed; the shell may refuse,
                // and the click still activates the surface through `WM_MOUSEACTIVATE`.
                let _ = SetForegroundWindow(hwnd);
            }
        }
        if opening.keyboard.is_none() {
            // Only a window the manager frames has a caption; a layer is a popup.
            theme_caption(hwnd);
        }
        // OLE hands every drag over this window to the target; its callbacks only queue
        // events, so nothing here needs the engine to exist yet.
        let drag = Rc::new(crate::dnd::DragQueue::new(hwnd));
        let drop_target = crate::dnd::register(hwnd, Rc::clone(&drag));
        Ok(Self {
            hwnd,
            visual,
            state,
            drag,
            drop_target,
            composited: opening.composited,
        })
    }

    /// The window handle a swap chain presents through.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// The window the frame is composed into.
    pub fn visual_hwnd(&self) -> HWND {
        self.visual
    }

    /// Whether the frame is composed over the desktop rather than shown opaque.
    pub fn composited(&self) -> bool {
        self.composited
    }

    /// Whether the window still exists; closing it destroys it and ends the loop.
    pub fn alive(&self) -> bool {
        unsafe { IsWindow(Some(self.hwnd)).as_bool() }
    }

    /// The client area in physical pixels; `(0, 0)` while minimized.
    pub fn client_size(&self) -> (u32, u32) {
        let mut rect = RECT::default();
        match unsafe { GetClientRect(self.hwnd, &mut rect) } {
            Ok(()) => (
                (rect.right - rect.left).max(0) as u32,
                (rect.bottom - rect.top).max(0) as u32,
            ),
            Err(_) => (0, 0),
        }
    }

    /// The client area in logical units, which is what layout and input use.
    pub fn logical_size(&self) -> (u32, u32) {
        let (width, height) = self.client_size();
        let scale = self.scale();
        (
            (width as f32 / scale).round() as u32,
            (height as f32 / scale).round() as u32,
        )
    }

    /// Ask the window manager for a client area of `size` physical pixels; the compositor's
    /// own resize is what the frame loop sees afterwards.
    pub fn set_client_size(&self, size: (u32, u32)) -> Result<(), WindowError> {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: size.0.max(1) as i32,
            bottom: size.1.max(1) as i32,
        };
        unsafe {
            AdjustWindowRectExForDpi(
                &mut frame,
                WS_OVERLAPPEDWINDOW,
                false,
                WINDOW_EX_STYLE(0),
                dpi,
            )
        }
        .map_err(|error| WindowError::Win32(format!("AdjustWindowRectExForDpi: {error}")))?;
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                frame.right - frame.left,
                frame.bottom - frame.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .map_err(|error| WindowError::Win32(format!("SetWindowPos: {error}")))
    }

    /// The output's refresh rate in hertz, when the mode reports one.
    ///
    /// This is the pacing ground truth: a frame wait says how long the loop waited, not
    /// what the display could have shown.
    pub fn refresh_hz(&self) -> Option<f32> {
        unsafe {
            let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let info_pointer = (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>();
            if !GetMonitorInfoW(monitor, info_pointer).as_bool() {
                return None;
            }
            let mut mode = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..DEVMODEW::default()
            };
            let device = PCWSTR::from_raw(info.szDevice.as_ptr());
            if !EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut mode).as_bool() {
                return None;
            }
            // 0 and 1 are the driver's "hardware default", not a measurable rate.
            (mode.dmDisplayFrequency > 1).then_some(mode.dmDisplayFrequency as f32)
        }
    }

    /// Physical pixels per logical unit.
    pub fn scale(&self) -> f32 {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
    }

    /// The pointer in logical units, once it has entered the client area.
    pub fn pointer(&self) -> Option<Point> {
        self.state.borrow().pointer
    }

    /// Whether the user or the app asked to close; the loop is what ends.
    pub fn should_close(&self) -> bool {
        self.state.borrow().close_requested || !self.alive()
    }

    /// Close the window the way its title-bar button does, for scripted exit tests.
    pub fn request_close(&mut self) {
        self.state.borrow_mut().close_requested = true;
    }

    /// Take every message waiting for this thread; true when there was one.
    pub fn pump(&mut self) -> bool {
        pump_messages()
    }

    /// Events since the last call, in arrival order.
    pub fn take_events(&mut self) -> Vec<Event> {
        self.pump();
        self.drain_tsf();
        let mut events = std::mem::take(&mut self.state.borrow_mut().events);
        // OLE callbacks run inside the pump above and only queue; appending keeps the
        // pointer events of the turn ahead of the drag state they produced.
        events.extend(self.drag.take());
        events
    }

    /// Publish what dropping the drag under the pointer would do, for OLE's own answer.
    pub fn set_drop_effect(&self, effect: Option<ui_core::DropEffect>) {
        self.drag.set_effect(effect);
    }

    /// Start a drag this app carries, so another window can take the payload.
    pub fn begin_drag(&mut self, drag: ui_platform::view::NativeDrag) -> bool {
        crate::dnd::begin(self.hwnd, drag, &self.drag)
    }

    /// Whether focus, extent or scale changed without an `Event`.
    pub fn take_silent_change(&mut self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().silent_change)
    }

    /// Whether the display's mode or topology changed since the last call.
    pub fn take_display_change(&mut self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().display_changed)
    }

    /// Block until a message arrives or `timeout` passes; true when messages are waiting.
    ///
    /// Bounded even with no deadline given: an app-owned deadline (a lifetime, a scripted
    /// step) can only be noticed between turns, and an unbounded wait would swallow it
    /// until unrelated input arrived.
    pub fn wait_until(&mut self, timeout: Option<Duration>) -> bool {
        let millis = bounded_millis(timeout);
        unsafe { MsgWaitForMultipleObjectsEx(None, millis, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
        // Peeked, not removed: `take_events` is what drains and dispatches.
        let mut message = MSG::default();
        unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) }.as_bool()
    }

    /// Block until a message arrives.
    pub fn wait(&mut self) {
        let _ = self.wait_until(None);
    }

    /// Wait for the desktop compositor to take the next frame.
    ///
    /// A Win32 window has no frame callback; `DwmFlush` blocks until the composition pass
    /// that will show what was presented, which is the same pacing point.
    pub fn wait_for_frame(&mut self) {
        wait_for_composition();
    }

    /// Apply the pointer shape this frame resolved; false when it was already that shape.
    pub fn set_cursor(&mut self, shape: CursorShape) -> bool {
        let mut state = self.state.borrow_mut();
        if state.cursor == Some(shape) {
            return false;
        }
        state.cursor = Some(shape);
        drop(state);
        // Applied here as well as in `WM_SETCURSOR`: a still pointer produces no message,
        // so the shape would otherwise arrive one movement late.
        unsafe {
            SetCursor(Some(load_cursor(shape)));
        }
        true
    }

    /// Publish where this frame's tree consumes input, in surface-local logical units.
    ///
    /// Only a layer has a shape of its own to publish: a window the manager places consumes
    /// its whole client area, which is already what the default hit test gives it.
    pub fn set_input_region(&mut self, areas: &[Rect]) {
        let mut state = self.state.borrow_mut();
        if state.keyboard.is_none() || state.input.as_deref() == Some(areas) {
            return;
        }
        state.input = Some(areas.to_vec());
        let scale = state.dpi.max(96) as f32 / 96.0;
        drop(state);
        shape(self.hwnd, areas, scale);
    }

    /// Apply this frame's input-method state: open or close the context, and keep the IME's
    /// composition and candidate windows at the caret.
    ///
    /// A context change sends messages back to this thread's window procedure, so nothing is
    /// borrowed while the IME calls run.
    /// A focused field that is not secret hands the input to TSF, which is the only Windows
    /// path that carries the surrounding text and a request to delete it. Every other state
    /// — no field, a secret one — keeps the IMM32 path.
    pub fn set_ime(&mut self, state: ImeState) {
        let (open, dpi) = {
            let mut window = self.state.borrow_mut();
            if window.ime.as_ref() == Some(&state) {
                return;
            }
            window.ime = Some(state.clone());
            window.ime_hidden = state.hidden;
            (window.ime_open, window.dpi)
        };
        let wants_tsf = state.enabled && !state.hidden;
        let session = self.tsf(wants_tsf);
        if let Some(session) = session {
            let attached = {
                let mut session = session.borrow_mut();
                if !wants_tsf {
                    session.set_attached(false);
                }
                let attached = wants_tsf && session.set_attached(true);
                if attached {
                    let mut origin = POINT::default();
                    // The store's rectangles are in screen pixels; the caret arrives in
                    // surface-local logical units.
                    unsafe {
                        let _ = ClientToScreen(self.hwnd, &mut origin);
                    }
                    session.publish(&state, dpi, origin);
                }
                attached
            };
            self.drain_tsf();
            if attached {
                return;
            }
        }
        if crate::ime::set_open(self.hwnd, state.enabled, open) {
            self.state.borrow_mut().ime_open = state.enabled;
        }
        if state.enabled
            && let Some(caret) = state.cursor
        {
            crate::ime::place(self.hwnd, dpi, caret);
        }
    }

    /// The window's TSF document, started on first need; `None` once TSF refused to start.
    fn tsf(&mut self, start: bool) -> Option<Rc<RefCell<crate::tsf::Session>>> {
        if let Some(session) = self.state.borrow().tsf.clone() {
            return Some(session);
        }
        if !start || self.state.borrow().tsf_failed {
            return None;
        }
        // TSF would only take the keyboard away here: an out-of-process input method serves
        // this window through IMM32, which an associated document switches off.
        if !crate::tsf::serves_input() {
            log::info!(
                "tsf: the active input method is out of process, so this window keeps IMM32"
            );
            self.state.borrow_mut().tsf_failed = true;
            return None;
        }
        match crate::tsf::Session::attach(self.hwnd) {
            Ok(session) => {
                let session = Rc::new(RefCell::new(session));
                self.state.borrow_mut().tsf = Some(session.clone());
                Some(session)
            }
            Err(error) => {
                log::warn!("tsf: not available, keeping IMM32: {error}");
                self.state.borrow_mut().tsf_failed = true;
                None
            }
        }
    }

    /// Move the events a TSF edit produced into the window's queue.
    fn drain_tsf(&mut self) {
        let Some(session) = self.state.borrow().tsf.clone() else {
            return;
        };
        let events = session.borrow().take_events();
        if !events.is_empty() {
            self.state.borrow_mut().events.extend(events);
        }
    }

    /// Move the pointer by a logical offset; false when the platform refused.
    pub fn nudge_pointer(&mut self, dx: f32, dy: f32) -> bool {
        let mut point = POINT::default();
        if unsafe { GetCursorPos(&mut point) }.is_err() {
            return false;
        }
        let scale = self.scale();
        unsafe {
            SetCursorPos(
                point.x + (dx * scale).round() as i32,
                point.y + (dy * scale).round() as i32,
            )
        }
        .is_ok()
    }

    /// Put `content` on the clipboard; false when Windows has no format for the payload.
    pub fn write_clipboard(&mut self, content: ClipboardContent) -> bool {
        crate::clipboard::write(Some(self.hwnd), &content)
    }

    /// Read the clipboard into a paste event; false when it holds no text.
    pub fn request_paste(&mut self) -> bool {
        let Some(text) = crate::clipboard::read_text(Some(self.hwnd)) else {
            return false;
        };
        self.state.borrow_mut().events.push(Event::Paste(text));
        true
    }
}

/// What `CreateWindowExW` is asked for, worked out from the surface's role.
struct Opening {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    /// The DPI of the monitor the window opens on, which its first scale reflects.
    dpi: u32,
    /// A layer's keyboard request; `None` for a window the manager places.
    keyboard: Option<KeyboardMode>,
    /// Whether the desktop should see through the frame where it is transparent.
    composited: bool,
}

/// An overlapped window at the manager's default position, sized for `size` logical pixels.
fn opening_window(size: (u32, u32)) -> Result<Opening, WindowError> {
    let dpi = unsafe { GetDpiForSystem() }.max(96);
    let scale = dpi as f32 / 96.0;
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: (size.0 as f32 * scale).round().max(1.0) as i32,
        bottom: (size.1 as f32 * scale).round().max(1.0) as i32,
    };
    unsafe {
        AdjustWindowRectExForDpi(
            &mut frame,
            WS_OVERLAPPEDWINDOW,
            false,
            WINDOW_EX_STYLE(0),
            dpi,
        )
    }
    .map_err(|error| WindowError::Win32(format!("AdjustWindowRectExForDpi: {error}")))?;
    Ok(Opening {
        x: CW_USEDEFAULT,
        y: CW_USEDEFAULT,
        width: frame.right - frame.left,
        height: frame.bottom - frame.top,
        style: WS_OVERLAPPEDWINDOW,
        ex_style: WINDOW_EX_STYLE(0),
        dpi,
        keyboard: None,
        composited: false,
    })
}

/// A layer as the closest thing Win32 has: a borderless popup on one monitor.
///
/// A spec that names an output lands on that monitor; one that names none, or names an output
/// that is gone, lands on the primary one, which is what the shell would pick anyway. The popup
/// is placed by arithmetic on the monitor's rectangle; nothing in the shell anchors it after.
fn opening_layer(layer: &LayerSpec) -> Result<Opening, WindowError> {
    let monitor = layer
        .output
        .as_deref()
        .and_then(crate::backend::monitor_named)
        .unwrap_or_else(primary_monitor);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(WindowError::Win32(String::from(
            "GetMonitorInfoW: the monitor's rectangle was refused",
        )));
    }
    let (mut dpi_x, mut dpi_y) = (0, 0);
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
        .map_err(|error| WindowError::Win32(format!("GetDpiForMonitor: {error}")))?;
    let scale = dpi_x.max(96) as f32 / 96.0;
    let rect = info.rcMonitor;
    let (mut x, mut width) = Span {
        start: rect.left,
        end: rect.right,
        near: layer.anchor.left,
        far: layer.anchor.right,
        near_margin: layer.margin.left,
        far_margin: layer.margin.right,
    }
    .place(layer.size.map(|size| size.0), scale);
    let (mut y, mut height) = Span {
        start: rect.top,
        end: rect.bottom,
        near: layer.anchor.top,
        far: layer.anchor.bottom,
        near_margin: layer.margin.top,
        far_margin: layer.margin.bottom,
    }
    .place(layer.size.map(|size| size.1), scale);
    // A surface that reaches the edge an auto-hidden taskbar hides against leaves that strip
    // to the desktop: covering it is what makes the taskbar unreachable by the mouse. A
    // surface whose picture is the screen asks for the edge instead; two rows short would
    // squeeze its whole picture, and a pointer would name a pixel it is not over.
    if layer.auto_hide == AutoHide::Yield
        && let Some(edge) = auto_hide_edge(monitor)
    {
        match edge {
            ABE_TOP if y <= rect.top => {
                let strip = AUTO_HIDE_STRIP.min(height - 1);
                y += strip;
                height -= strip;
            }
            ABE_BOTTOM if y + height >= rect.bottom => {
                height -= AUTO_HIDE_STRIP.min(height - 1);
            }
            ABE_LEFT if x <= rect.left => {
                let strip = AUTO_HIDE_STRIP.min(width - 1);
                x += strip;
                width -= strip;
            }
            ABE_RIGHT if x + width >= rect.right => {
                width -= AUTO_HIDE_STRIP.min(width - 1);
            }
            _ => {}
        }
    }
    let mut ex_style = WS_EX_TOOLWINDOW;
    // Only the upper layers have a topmost counterpart. A lower one degrades to an ordinary
    // window: Win32 has no supported way to sit beneath other applications' windows.
    if matches!(layer.level, Layer::Overlay | Layer::Top) {
        ex_style |= WS_EX_TOPMOST;
    }
    if layer.keyboard == KeyboardMode::None {
        ex_style |= WS_EX_NOACTIVATE;
    }
    // A composition swap chain needs a window the shell does not draw into, or the redirection
    // surface stays black behind the frame.
    ex_style |= WS_EX_NOREDIRECTIONBITMAP;
    Ok(Opening {
        x,
        y,
        width,
        height,
        style: WS_POPUP,
        ex_style,
        dpi: dpi_x.max(96),
        keyboard: Some(layer.keyboard),
        composited: true,
    })
}

/// The monitor the shell treats as primary.
fn primary_monitor() -> HMONITOR {
    unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }
}

/// The monitor edge an auto-hidden taskbar hides against, when it hides on `monitor`.
fn auto_hide_edge(monitor: HMONITOR) -> Option<u32> {
    let mut state = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..APPBARDATA::default()
    };
    // SAFETY: both calls fill a struct this function owns, sized as the shell requires.
    if unsafe { SHAppBarMessage(ABM_GETSTATE, &mut state) } as u32 & ABS_AUTOHIDE == 0 {
        return None;
    }
    let mut position = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..APPBARDATA::default()
    };
    unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut position) };
    // Only the monitor the taskbar hides on needs an edge kept reachable.
    (unsafe { MonitorFromRect(&position.rc, MONITOR_DEFAULTTONULL) } == monitor)
        .then_some(position.uEdge)
}

/// One axis of the monitor's rectangle, with what the spec asks of it on that axis.
struct Span {
    start: i32,
    end: i32,
    near: bool,
    far: bool,
    near_margin: f32,
    far_margin: f32,
}

impl Span {
    /// The origin and extent this axis gets: the anchors decide, the margins inset, and
    /// `size` is honoured only where a single edge (or none) leaves room for it.
    fn place(self, size: Option<u32>, scale: f32) -> (i32, i32) {
        let near = self.near.then(|| (self.near_margin * scale).round() as i32);
        let far = self.far.then(|| (self.far_margin * scale).round() as i32);
        let inner_start = self.start + near.unwrap_or(0);
        let inner_end = self.end - far.unwrap_or(0);
        let span = (inner_end - inner_start).max(1);
        // Both edges pinned is what stretches the surface; a requested size has nowhere to go.
        let requested = match (near, far) {
            (Some(_), Some(_)) => None,
            _ => size.map(|size| (size as f32 * scale).round() as i32),
        };
        let extent = requested.unwrap_or(span);
        let origin = match (near, far) {
            (Some(_), None) => inner_start,
            (None, Some(_)) => inner_end - extent,
            // Unanchored on this axis the surface is centred, as it is on Wayland.
            _ => inner_start + (span - extent) / 2,
        };
        (origin, extent)
    }
}

impl Drop for Win32Window {
    fn drop(&mut self) {
        if self.alive() {
            // OLE keeps the window on its drop-target list; the registration must go
            // before the window it names does.
            crate::dnd::unregister(self.hwnd);
            self.drop_target = None;
            let _ = unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

/// The window as the engine's driver sees it; an app never derives these rules itself.
impl FrameHost for Win32Window {
    fn size(&self) -> (u32, u32) {
        self.client_size()
    }
    fn scale(&self) -> f32 {
        Win32Window::scale(self)
    }
    fn pointer(&self) -> Option<Point> {
        Win32Window::pointer(self)
    }
    fn can_draw(&self) -> bool {
        // There is no compositor window to wait for: every turn may present.
        true
    }
    fn wait_until(&mut self, timeout: Option<Duration>) -> bool {
        Win32Window::wait_until(self, timeout)
    }
    fn wait_for_frame(&mut self) {
        Win32Window::wait_for_frame(self)
    }
    fn should_close(&self) -> bool {
        Win32Window::should_close(self)
    }
    fn take_events(&mut self) -> Vec<Event> {
        Win32Window::take_events(self)
    }
    fn take_silent_change(&mut self) -> bool {
        Win32Window::take_silent_change(self)
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // `WM_NCCREATE` is the message that hands the state over; every other message reads it.
    // It therefore has to be answered before the lookup below, or the state is never stored.
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<WindowState>;
    if raw.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    // SAFETY: the pointer is this window's state, set in `WM_NCCREATE` and cleared in
    // `WM_NCDESTROY`; every message for a window arrives on the thread that created it.
    let state = unsafe { &*raw };
    match unsafe { handle(state, hwnd, message, wparam, lparam) } {
        Some(result) => result,
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Answer one message; `None` lets the default procedure decide.
unsafe fn handle(
    state: &RefCell<WindowState>,
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    match message {
        WM_NCDESTROY => {
            let raw =
                unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<WindowState>;
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            if !raw.is_null() {
                // The count `Win32Window::new` added is released with the window itself.
                drop(unsafe { Rc::from_raw(raw) });
            }
            None
        }
        WM_CLOSE => {
            state.borrow_mut().close_requested = true;
            Some(LRESULT(0))
        }
        WM_DESTROY => {
            state.borrow_mut().close_requested = true;
            None
        }
        WM_SIZE => {
            state.borrow_mut().silent_change = true;
            None
        }
        WM_DISPLAYCHANGE => {
            // Resolution, a monitor's connection or its HDR state: the swap chain's format
            // and colour space were decided from the output this window was on, so the frame
            // that follows has to decide them again.
            let mut window = state.borrow_mut();
            window.display_changed = true;
            window.silent_change = true;
            Some(LRESULT(0))
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            state.borrow_mut().silent_change = true;
            None
        }
        WM_GETMINMAXINFO => {
            let declared = state.borrow().min_size?;
            let (width, height) = tracked_minimum(hwnd, Some(declared));
            let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            info.ptMinTrackSize = POINT { x: width, y: height };
            // Answering keeps `DefWindowProcW` from writing the system minimum back over it.
            Some(LRESULT(0))
        }
        WM_DPICHANGED => {
            let mut window = state.borrow_mut();
            window.dpi = ((wparam.0 as u32) >> 16).max(96);
            window.silent_change = true;
            let layer = window.keyboard.is_some();
            let scale = window.dpi as f32 / 96.0;
            let areas = window.input.clone();
            drop(window);
            if layer {
                // The areas are logical, so a new scale is a new shape; the rectangle itself
                // comes from the spec, and the manager's suggested one would move it off.
                if let Some(areas) = areas {
                    shape(hwnd, &areas, scale);
                }
                return Some(LRESULT(0));
            }
            // The suggested rectangle keeps the client area the same logical size.
            let suggested = unsafe { &*(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            Some(LRESULT(0))
        }
        WM_NCHITTEST => {
            // A layer's window is shaped to what it consumes, so a hit test that arrives here
            // is one the surface wants; the default answer is what the shape leaves.
            state.borrow().keyboard?;
            Some(LRESULT(HTCLIENT as isize))
        }
        WM_NCLBUTTONDOWN => {
            // The sizing border is the one drag `DefWindowProcW` runs in a modal loop, which
            // blocks the app loop until the button comes up and leaves DWM to stretch the
            // last frame. This window sizes itself instead, one message at a time, so a
            // frame follows every move. A title-bar drag stays with Windows: that loop is
            // where snapping lives, and a move needs no new frames.
            let edge = (wparam.0 as u32) & 0xFFFF;
            if !sizing_edge(edge) {
                return None;
            }
            begin_sizing(state, hwnd, edge, packed_point(lparam));
            Some(LRESULT(0))
        }
        WM_MOUSEACTIVATE => {
            let command = match state.borrow().keyboard {
                // A window the manager places activates the ordinary way.
                None => return None,
                Some(KeyboardMode::None) => MA_NOACTIVATE,
                // Windows has no focus-follows-pointer contract, so `FollowPointer` is the
                // same rule as `OnDemand`: a click on a consumed area focuses the surface.
                Some(KeyboardMode::Exclusive | KeyboardMode::OnDemand | KeyboardMode::FollowPointer) => {
                    MA_ACTIVATE
                }
            };
            Some(LRESULT(command as isize))
        }
        WM_MOUSEMOVE => {
            // A sizing drag owns the pointer until the button comes up, and the client area
            // under it is not what it tracks.
            if sizing_step(state, hwnd, packed_point(lparam)) {
                return Some(LRESULT(0));
            }
            let pos = logical(state, lparam);
            let mut window = state.borrow_mut();
            window.pointer = Some(pos);
            window.events.push(Event::PointerMoved { pos });
            let arm = !window.tracking_leave;
            window.tracking_leave = true;
            let following = window.keyboard == Some(KeyboardMode::FollowPointer);
            drop(window);
            if following {
                remember_foreground(state, hwnd);
            }
            if arm {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // A refusal only means the leave event is missing; the pointer still moves.
                let _ = unsafe { TrackMouseEvent(&mut track) };
            }
            Some(LRESULT(0))
        }
        WM_MOUSELEAVE => {
            let mut window = state.borrow_mut();
            window.pointer = None;
            window.tracking_leave = false;
            window.events.push(Event::PointerLeft);
            drop(window);
            hand_back_keyboard(state, hwnd);
            Some(LRESULT(0))
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let button = button(message);
            let pos = logical(state, lparam);
            let mut window = state.borrow_mut();
            window.pointer = Some(pos);
            window.events.push(Event::PointerPressed { pos, button });
            drop(window);
            // Capture is the implicit grab a compositor gives: a drag that leaves the
            // window still belongs to it until the release.
            unsafe { SetCapture(hwnd) };
            Some(LRESULT(0))
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            if message == WM_LBUTTONUP && end_sizing(state) {
                return Some(LRESULT(0));
            }
            let button = button(message);
            let pos = logical(state, lparam);
            let mut window = state.borrow_mut();
            window.pointer = Some(pos);
            window.events.push(Event::PointerReleased { pos, button });
            drop(window);
            let _ = unsafe { ReleaseCapture() };
            Some(LRESULT(0))
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let notches = ((wparam.0 as u32) >> 16) as u16 as i16 as f32 / 120.0;
            // Unlike the button messages these name a screen position, and a window that let
            // the pointer through never saw a move to fall back on.
            let mut point = packed_point(lparam);
            let pos = if unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
                logical_point(state, point)
            } else {
                match state.borrow().pointer {
                    Some(pointer) => pointer,
                    None => return Some(LRESULT(0)),
                }
            };
            let delta = if message == WM_MOUSEHWHEEL {
                // Positive is a wheel tilted right, which reveals content to the right; the
                // engine and Wayland both call that a positive step.
                Point::new(notches * input::WHEEL_NOTCH, 0.0)
            } else {
                // Positive is a notch away from the user, which scrolls content up.
                Point::new(0.0, -notches * input::WHEEL_NOTCH)
            };
            state
                .borrow_mut()
                .events
                .push(Event::Scroll { pos, delta });
            Some(LRESULT(0))
        }
        WM_CAPTURECHANGED => {
            // Another window took the mouse: the drag ends where the edges already are.
            state.borrow_mut().sizing = None;
            None
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if (wparam.0 & 0xFFFF) as u16 == VK_ESCAPE.0 && cancel_sizing(state, hwnd) {
                return Some(LRESULT(0));
            }
            if tsf_key(state, true, wparam, lparam) {
                return Some(LRESULT(0));
            }
            key(state, (wparam.0 & 0xFFFF) as u16, true);
            Some(LRESULT(0))
        }
        WM_KEYUP | WM_SYSKEYUP => {
            if tsf_key(state, false, wparam, lparam) {
                return Some(LRESULT(0));
            }
            key(state, (wparam.0 & 0xFFFF) as u16, false);
            Some(LRESULT(0))
        }
        WM_CHAR => {
            // A key an input method took still produces a character through
            // `TranslateMessage`, and feeding it back would type the composition twice.
            if tsf_swallows_char(state) {
                return Some(LRESULT(0));
            }
            // Control characters are the command path's: `WM_KEYDOWN` already named them.
            if let Some(character) = char::from_u32((wparam.0 & 0xFFFF) as u32)
                && !character.is_control()
            {
                state
                    .borrow_mut()
                    .events
                    .push(Event::Text(character.to_string()));
            }
            Some(LRESULT(0))
        }
        WM_IME_SETCONTEXT => {
            if tsf_attached(state) {
                return Some(LRESULT(0));
            }
            let hidden = state.borrow().ime_hidden;
            let flags = crate::ime::ui_flags(lparam, hidden);
            Some(unsafe { DefWindowProcW(hwnd, message, wparam, flags) })
        }
        WM_IME_STARTCOMPOSITION => {
            // The engine draws the composition from the preedit it is given, so the IME's
            // own composition window stays hidden and this needs no default handling. The
            // place is refreshed here because the caret is the last frame's, and the window
            // may have moved since.
            if tsf_attached(state) {
                return Some(LRESULT(0));
            }
            let (caret, dpi) = {
                let window = state.borrow();
                (window.ime.as_ref().and_then(|ime| ime.cursor), window.dpi)
            };
            if let Some(caret) = caret {
                crate::ime::place(hwnd, dpi, caret);
            }
            Some(LRESULT(0))
        }
        WM_IME_COMPOSITION => {
            if tsf_attached(state) {
                return Some(LRESULT(0));
            }
            let events = crate::ime::composition(hwnd, lparam.0 as u32);
            state.borrow_mut().events.extend(events);
            Some(LRESULT(0))
        }
        WM_IME_ENDCOMPOSITION => {
            if tsf_attached(state) {
                return Some(LRESULT(0));
            }
            state.borrow_mut().events.push(Event::Preedit(None));
            Some(LRESULT(0))
        }
        WM_SETCURSOR => {
            // The pointer has left the client area for a border it is dragging, and the
            // edge's own cursor is what the drag shows until the button comes up.
            if let Some(edge) = state.borrow().sizing.map(|sizing| sizing.edge) {
                unsafe {
                    SetCursor(Some(load_cursor(sizing_cursor(edge))));
                }
                return Some(LRESULT(1));
            }
            if (lparam.0 as u32) & 0xFFFF != HTCLIENT {
                return None;
            }
            let shape = state.borrow().cursor.unwrap_or_default();
            unsafe {
                SetCursor(Some(load_cursor(shape)));
            }
            Some(LRESULT(1))
        }
        WM_PAINT => {
            // Validated here and painted by the loop's next frame: a `WM_PAINT` without
            // this would repeat until the thread starves.
            let mut paint = PAINTSTRUCT::default();
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut paint);
                let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &paint);
            }
            state.borrow_mut().silent_change = true;
            Some(LRESULT(0))
        }
        WM_ERASEBKGND => Some(LRESULT(1)),
        WM_SETTINGCHANGE => {
            // The app polls the same registry value each frame; this is what makes a frame
            // happen while the window sits idle, and what keeps the caption in step.
            theme_caption(hwnd);
            state.borrow_mut().silent_change = true;
            None
        }
        _ => None,
    }
}

/// Paint the caption in the scheme the client area uses.
///
/// DWM gives a window a light caption until it asks otherwise, so a dark client area would sit
/// under a light bar; the attribute exists from Windows 10 20H1 on.
fn theme_caption(hwnd: HWND) {
    let preference = crate::theme::Win32ThemeWatcher::new().preference();
    let dark = windows::core::BOOL::from(preference.is_dark(false));
    let size = std::mem::size_of_val(&dark) as u32;
    if let Err(error) = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark as *const windows::core::BOOL).cast(),
            size,
        )
    } {
        log::debug!("the caption keeps the system colour: {error}");
    }
}

/// The smallest window a user may drag to: the system's tracking size or the spec's floor.
///
/// The spec names a client area in logical units; a tracked size is the whole window, so the
/// frame is added back through the same adjustment the opening size used.
fn tracked_minimum(hwnd: HWND, declared: Option<(u32, u32)>) -> (i32, i32) {
    let system = unsafe { (GetSystemMetrics(SM_CXMINTRACK), GetSystemMetrics(SM_CYMINTRACK)) };
    let Some((width, height)) = declared else {
        return system;
    };
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let scale = dpi as f32 / 96.0;
    let client = (
        (width as f32 * scale).ceil().max(1.0) as i32,
        (height as f32 * scale).ceil().max(1.0) as i32,
    );
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: client.0,
        bottom: client.1,
    };
    let style = WINDOW_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32);
    let ex_style = WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32);
    if unsafe { AdjustWindowRectExForDpi(&mut frame, style, false, ex_style, dpi) }.is_err() {
        return (system.0.max(client.0), system.1.max(client.1));
    }
    (
        (frame.right - frame.left).max(system.0),
        (frame.bottom - frame.top).max(system.1),
    )
}

/// Offer a key to the attached input methods; true when one took it.
fn tsf_key(state: &RefCell<WindowState>, down: bool, wparam: WPARAM, lparam: LPARAM) -> bool {
    let session = state.borrow().tsf.clone();
    let Some(session) = session else {
        return false;
    };
    session.borrow_mut().key(down, wparam.0, lparam.0)
}

/// Whether the input methods own this window's keyboard right now.
fn tsf_attached(state: &RefCell<WindowState>) -> bool {
    let session = state.borrow().tsf.clone();
    session
        .map(|session| session.borrow().is_attached())
        .unwrap_or(false)
}

/// Whether the pending character belongs to a key an input method already handled.
fn tsf_swallows_char(state: &RefCell<WindowState>) -> bool {
    let session = state.borrow().tsf.clone();
    session
        .map(|session| session.borrow().take_swallowed_char())
        .unwrap_or(false)
}

/// Turn one key message into modifiers and, when it names one, a key event.
fn key(state: &RefCell<WindowState>, vk: u16, down: bool) {
    let held = input::modifiers();
    let mut state = state.borrow_mut();
    if held != state.modifiers {
        state.modifiers = held;
        state.events.push(Event::Modifiers(held));
    }
    if let Some(named) = input::named(vk) {
        state.events.push(if down {
            Event::KeyPressed { key: named }
        } else {
            Event::KeyReleased { key: named }
        });
    } else if down && (held.ctrl || held.alt || held.logo) {
        // The text path never carries a chord: Ctrl+A must not insert an `a`.
        if let Some(character) = input::command_char(vk) {
            state.events.push(Event::KeyPressed {
                key: ui_core::Key::Char(character),
            });
        }
    }
}

/// Whether a hit-test code names a border or a corner, which is what a sizing drag moves.
fn sizing_edge(edge: u32) -> bool {
    matches!(
        edge,
        HTLEFT
            | HTRIGHT
            | HTTOP
            | HTBOTTOM
            | HTTOPLEFT
            | HTTOPRIGHT
            | HTBOTTOMLEFT
            | HTBOTTOMRIGHT
    )
}

/// The cursor a dragged edge shows: the axis the edge moves along.
fn sizing_cursor(edge: u32) -> CursorShape {
    match edge {
        HTLEFT | HTRIGHT => CursorShape::ResizeHorizontal,
        HTTOP | HTBOTTOM => CursorShape::ResizeVertical,
        HTTOPLEFT | HTBOTTOMRIGHT => CursorShape::ResizeDiagonalDownRight,
        _ => CursorShape::ResizeDiagonalUpRight,
    }
}

/// Start a border drag at the press position, which a non-client message gives in screen pixels.
fn begin_sizing(state: &RefCell<WindowState>, hwnd: HWND, edge: u32, at: POINT) {
    let mut rect = RECT::default();
    if let Err(error) = unsafe { GetWindowRect(hwnd, &mut rect) } {
        log::warn!("win32: a sizing drag could not read the window's rectangle: {error}");
        return;
    }
    state.borrow_mut().sizing = Some(Sizing {
        edge,
        cursor: at,
        rect,
    });
    unsafe { SetCapture(hwnd) };
    // `DefWindowProc` is what raises the window for a border click, and this drag is the one
    // path that does not reach it.
    if !unsafe { SetForegroundWindow(hwnd) }.as_bool() {
        log::warn!("win32: a sizing drag could not bring its window to the foreground");
    }
}

/// Resize to follow the pointer; false when no drag is running.
fn sizing_step(state: &RefCell<WindowState>, hwnd: HWND, at: POINT) -> bool {
    let Some(sizing) = state.borrow().sizing else {
        return false;
    };
    // A mouse move carries client pixels, and the origin they are relative to is moving.
    let mut at = at;
    if !unsafe { ClientToScreen(hwnd, &mut at) }.as_bool() {
        return true;
    }
    let (dx, dy) = (at.x - sizing.cursor.x, at.y - sizing.cursor.y);
    let (mut left, mut top) = (sizing.rect.left, sizing.rect.top);
    let (mut right, mut bottom) = (sizing.rect.right, sizing.rect.bottom);
    match sizing.edge {
        HTLEFT | HTTOPLEFT | HTBOTTOMLEFT => left += dx,
        HTRIGHT | HTTOPRIGHT | HTBOTTOMRIGHT => right += dx,
        _ => {}
    }
    match sizing.edge {
        HTTOP | HTTOPLEFT | HTTOPRIGHT => top += dy,
        HTBOTTOM | HTBOTTOMLEFT | HTBOTTOMRIGHT => bottom += dy,
        _ => {}
    }
    // The modal loop is what applies a tracked minimum, so this drag applies it itself —
    // the system's, and the floor the surface spec declared.
    let minimum = tracked_minimum(hwnd, state.borrow().min_size);
    if right - left < minimum.0 {
        match sizing.edge {
            HTLEFT | HTTOPLEFT | HTBOTTOMLEFT => left = right - minimum.0,
            _ => right = left + minimum.0,
        }
    }
    if bottom - top < minimum.1 {
        match sizing.edge {
            HTTOP | HTTOPLEFT | HTTOPRIGHT => top = bottom - minimum.1,
            _ => bottom = top + minimum.1,
        }
    }
    if let Err(error) = unsafe {
        SetWindowPos(
            hwnd,
            None,
            left,
            top,
            right - left,
            bottom - top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    } {
        log::warn!("win32: a sizing drag could not resize the window: {error}");
    }
    true
}

/// End the drag where it stands; true when one was running.
fn end_sizing(state: &RefCell<WindowState>) -> bool {
    let running = state.borrow_mut().sizing.take().is_some();
    if running
        && let Err(error) = unsafe { ReleaseCapture() }
    {
        // Losing the capture to another window already ended the drag.
        log::warn!("win32: a sizing drag could not release the mouse: {error}");
    }
    running
}

/// Put the window back where the drag began; true when one was running.
fn cancel_sizing(state: &RefCell<WindowState>, hwnd: HWND) -> bool {
    let Some(sizing) = state.borrow_mut().sizing.take() else {
        return false;
    };
    if let Err(error) = unsafe { ReleaseCapture() } {
        log::warn!("win32: a cancelled drag could not release the mouse: {error}");
    }
    let (width, height) = (
        sizing.rect.right - sizing.rect.left,
        sizing.rect.bottom - sizing.rect.top,
    );
    if let Err(error) = unsafe {
        SetWindowPos(
            hwnd,
            None,
            sizing.rect.left,
            sizing.rect.top,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    } {
        log::warn!("win32: a cancelled drag could not restore the window: {error}");
    }
    true
}

/// A message's position: two signed shorts packed into one parameter, in screen or client
/// coordinates depending on the message.
fn packed_point(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam.0 as i32) as i16 as i32,
        y: ((lparam.0 as i32) >> 16) as i16 as i32,
    }
}

/// A message's client position in logical units.
fn logical(state: &RefCell<WindowState>, lparam: LPARAM) -> Point {
    logical_point(state, packed_point(lparam))
}

/// A client position in logical units.
fn logical_point(state: &RefCell<WindowState>, point: POINT) -> Point {
    let scale = state.borrow().dpi.max(96) as f32 / 96.0;
    Point::new(point.x as f32 / scale, point.y as f32 / scale)
}

/// Give the window the shape of the areas it consumes, in window coordinates.
///
/// The system owns the region once the call succeeds; a refused one is still ours to free.
fn shape(hwnd: HWND, areas: &[Rect], scale: f32) {
    let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
    if region.0.is_null() {
        return;
    }
    for area in areas {
        let from = |value: f32, round: fn(f32) -> f32| round(value * scale) as i32;
        let (left, top) = (from(area.origin.x, f32::floor), from(area.origin.y, f32::floor));
        let right = from(area.origin.x + area.size.w, f32::ceil).max(left + 1);
        let bottom = from(area.origin.y + area.size.h, f32::ceil).max(top + 1);
        let piece = unsafe { CreateRectRgn(left, top, right, bottom) };
        if piece.0.is_null() {
            continue;
        }
        unsafe {
            CombineRgn(Some(region), Some(region), Some(piece), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(piece.0));
        }
    }
    if unsafe { SetWindowRgn(hwnd, Some(region), false) } == 0 {
        let _ = unsafe { DeleteObject(HGDIOBJ(region.0)) };
    }
}

/// Give the keyboard back to whichever window had it before a click took it.
///
/// The shell only lets a process move the foreground while it holds it, which is why this
/// happens at the moment the pointer leaves and only from that state.
fn hand_back_keyboard(state: &RefCell<WindowState>, hwnd: HWND) {
    let previous = {
        let mut window = state.borrow_mut();
        if window.keyboard != Some(KeyboardMode::FollowPointer) {
            return;
        }
        window.previous.take()
    };
    let Some(previous) = previous else {
        return;
    };
    // A click elsewhere already moved the foreground; there is nothing left to hand back.
    let foreground = unsafe { GetForegroundWindow() };
    if foreground != hwnd || !unsafe { IsWindow(Some(previous)) }.as_bool() {
        return;
    }
    unsafe {
        let _ = SetForegroundWindow(previous);
    }
}

/// Note which window holds the keyboard as the pointer arrives on what this surface consumes.
///
/// It has to be taken on movement rather than on the click: by the time `WM_MOUSEACTIVATE`
/// runs, the activation has already cleared the foreground, and `GetForegroundWindow` answers
/// null — the window to hand back to would be lost.
fn remember_foreground(state: &RefCell<WindowState>, hwnd: HWND) {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_invalid() || foreground == hwnd {
        return;
    }
    state.borrow_mut().previous = Some(foreground);
}

fn button(message: u32) -> ui_core::PointerButton {
    match message {
        WM_LBUTTONDOWN | WM_LBUTTONUP => ui_core::PointerButton::Primary,
        WM_RBUTTONDOWN | WM_RBUTTONUP => ui_core::PointerButton::Secondary,
        _ => ui_core::PointerButton::Middle,
    }
}

fn load_cursor(shape: CursorShape) -> windows::Win32::UI::WindowsAndMessaging::HCURSOR {
    let name = match shape {
        CursorShape::Arrow | CursorShape::Copy | CursorShape::Alias => IDC_ARROW,
        CursorShape::Text => IDC_IBEAM,
        CursorShape::Crosshair => IDC_CROSS,
        CursorShape::PointingHand | CursorShape::Grab | CursorShape::Grabbing => IDC_HAND,
        CursorShape::Move | CursorShape::AllScroll => IDC_SIZEALL,
        CursorShape::NotAllowed | CursorShape::NoDrop => IDC_NO,
        CursorShape::ResizeHorizontal | CursorShape::ResizeColumn => IDC_SIZEWE,
        CursorShape::ResizeVertical | CursorShape::ResizeRow => IDC_SIZENS,
        CursorShape::ResizeDiagonalDownRight => IDC_SIZENWSE,
        CursorShape::ResizeDiagonalUpRight => IDC_SIZENESW,
        CursorShape::ZoomIn | CursorShape::ZoomOut => IDC_ARROW,
    };
    unsafe { LoadCursorW(None, name) }.unwrap_or_default()
}

fn register_class(class: &[u16], instance: HINSTANCE) -> Result<(), WindowError> {
    let descriptor = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        hbrBackground: HBRUSH::default(),
        lpszClassName: PCWSTR::from_raw(class.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&descriptor) } == 0 {
        // Another window of this application already registered the class; that is the
        // normal path for a second window, not a failure.
        let error = unsafe { GetLastError() };
        if error != ERROR_CLASS_ALREADY_EXISTS {
            return Err(WindowError::Win32(format!(
                "RegisterClassExW failed: {error:?}"
            )));
        }
    }
    Ok(())
}

/// Dispatch every message already on this thread's queue; true when there was one.
///
/// The process's loop calls this after its wait as well as per surface, so a message that
/// belongs to no window of this process — a hotkey press — is still delivered when no
/// surface is open.
pub(crate) fn pump_messages() -> bool {
    let mut message = MSG::default();
    let mut any = false;
    while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        any = true;
    }
    any
}

/// How long a wait may last with no deadline given.
///
/// Bounded even then: an app-owned deadline (a lifetime, a scripted step) is only noticed
/// between turns, and an unbounded wait would swallow it until unrelated input arrived.
pub(crate) fn bounded_millis(timeout: Option<Duration>) -> u32 {
    let wait = timeout
        .unwrap_or(Duration::from_millis(250))
        .min(Duration::from_secs(1));
    wait.as_millis().min(u32::MAX as u128 - 1) as u32
}

/// Wait for the desktop compositor to take the next frame.
///
/// A Win32 window has no frame callback; `DwmFlush` blocks until the composition pass that
/// will show what was presented, which is the same pacing point.
pub(crate) fn wait_for_composition() {
    if unsafe { DwmFlush() }.is_err() {
        // Composition is off, so there is no flush to wait for; a short sleep keeps the
        // loop from spinning at unbounded speed.
        std::thread::sleep(Duration::from_millis(8));
    }
}

/// Put `content` on the system clipboard with no window of its own.
///
/// A process-wide copy has no window to own the selection, and on Windows the clipboard
/// belongs to the task when opened without one.
pub fn set_system_clipboard(content: ClipboardContent) -> bool {
    crate::clipboard::write(None, &content)
}

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
