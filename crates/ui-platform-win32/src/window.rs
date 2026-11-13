//! A Win32 window: class registration, creation, the message pump, input and the extent.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use ui_core::clipboard::ClipboardContent;
use ui_core::geom::{Point, Rect};
use ui_core::{CursorShape, Event};
use ui_platform::driver::FrameHost;
use ui_platform::present::ImeState;
use ui_platform::window::{KeyboardMode, Layer, LayerSpec, SurfaceRole, SurfaceSpec};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW, HBRUSH,
    MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW,
    MonitorFromPoint, MonitorFromWindow, PAINTSTRUCT, ScreenToClient,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
    GetDpiForSystem, GetDpiForWindow, MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetCursorPos, GetWindowLongPtrW, HTCLIENT, HTTRANSPARENT,
    IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS,
    IDC_SIZENWSE, IDC_SIZEWE, IsWindow, LoadCursorW, MA_ACTIVATE, MA_NOACTIVATE, MSG,
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_NOREMOVE, PM_REMOVE, PeekMessageW,
    QS_ALLINPUT, RegisterClassExW, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOZORDER, SetCursor, SetCursorPos, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_CLOSE, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS,
    WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSEXW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP,
};
use windows::core::PCWSTR;

use crate::{WindowError, input};

/// The one message this backend uses that the syscall-binding module does not declare:
/// `WM_MOUSELEAVE` lives with the common controls, whose whole API surface is not worth a
/// feature flag for one constant.
const WM_MOUSELEAVE: u32 = 0x02A3;

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
    /// Physical pixels per logical unit, in whole DPI.
    dpi: u32,
    /// The system light/dark preference changed since the last poll.
    theme_changed: bool,
    /// A layer's keyboard request; `None` for a window the manager places, which has no
    /// shape and no activation rule of its own.
    keyboard: Option<KeyboardMode>,
    /// The input-method state the last frame published, so one change applies once.
    ime: Option<ImeState>,
    /// Whether the input context is open, which is the last state that reached it.
    ime_open: bool,
    /// The buffer is secret: the input method's own candidate list is hidden too.
    ime_hidden: bool,
    /// The areas the last frame's tree consumes, surface-local logical units; `None` until a
    /// frame publishes one, which leaves the whole surface consuming input.
    input: Option<Vec<Rect>>,
}

/// A window the window manager places, decorates and stacks.
///
/// Dropping it destroys the window, so a presenter that draws into it must be dropped first.
pub struct Win32Window {
    hwnd: HWND,
    state: Rc<RefCell<WindowState>>,
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
            if opening.keyboard == Some(KeyboardMode::Exclusive) {
                // A hotkey press is what usually makes this succeed; the shell may refuse,
                // and the click still activates the surface through `WM_MOUSEACTIVATE`.
                let _ = SetForegroundWindow(hwnd);
            }
        }
        Ok(Self { hwnd, state })
    }

    /// The window handle a swap chain presents through.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
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
        std::mem::take(&mut self.state.borrow_mut().events)
    }

    /// Whether focus, extent or scale changed without an `Event`.
    pub fn take_silent_change(&mut self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().silent_change)
    }

    /// Whether the system light/dark preference changed since the last call.
    pub fn take_theme_change(&mut self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().theme_changed)
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
    }

    /// Apply this frame's input-method state: open or close the context, and keep the IME's
    /// composition and candidate windows at the caret.
    ///
    /// A context change sends messages back to this thread's window procedure, so nothing is
    /// borrowed while the IME calls run.
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
        if crate::ime::set_open(self.hwnd, state.enabled, open) {
            self.state.borrow_mut().ime_open = state.enabled;
        }
        if state.enabled
            && let Some(caret) = state.cursor
        {
            crate::ime::place(self.hwnd, dpi, caret);
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
    })
}

/// A layer as the closest thing Win32 has: a borderless popup on the primary monitor.
///
/// The portable spec names no output, so the primary monitor is where a layer lands, as the
/// first bound output is on Wayland. The popup is placed by arithmetic on the monitor's
/// rectangle; nothing in the shell anchors it afterwards.
fn opening_layer(layer: &LayerSpec) -> Result<Opening, WindowError> {
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(WindowError::Win32(String::from(
            "GetMonitorInfoW: no primary monitor",
        )));
    }
    let (mut dpi_x, mut dpi_y) = (0, 0);
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
        .map_err(|error| WindowError::Win32(format!("GetDpiForMonitor: {error}")))?;
    let scale = dpi_x.max(96) as f32 / 96.0;
    let rect = info.rcMonitor;
    let (x, width) = Span {
        start: rect.left,
        end: rect.right,
        near: layer.anchor.left,
        far: layer.anchor.right,
        near_margin: layer.margin.left,
        far_margin: layer.margin.right,
    }
    .place(layer.size.map(|size| size.0), scale);
    let (y, height) = Span {
        start: rect.top,
        end: rect.bottom,
        near: layer.anchor.top,
        far: layer.anchor.bottom,
        near_margin: layer.margin.top,
        far_margin: layer.margin.bottom,
    }
    .place(layer.size.map(|size| size.1), scale);
    let mut ex_style = WS_EX_TOOLWINDOW;
    // Only the upper layers have a topmost counterpart. A lower one degrades to an ordinary
    // window: Win32 has no supported way to sit beneath other applications' windows.
    if matches!(layer.level, Layer::Overlay | Layer::Top) {
        ex_style |= WS_EX_TOPMOST;
    }
    if layer.keyboard == KeyboardMode::None {
        ex_style |= WS_EX_NOACTIVATE;
    }
    Ok(Opening {
        x,
        y,
        width,
        height,
        style: WS_POPUP,
        ex_style,
        dpi: dpi_x.max(96),
        keyboard: Some(layer.keyboard),
    })
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
        WM_SETFOCUS | WM_KILLFOCUS => {
            state.borrow_mut().silent_change = true;
            None
        }
        WM_DPICHANGED => {
            let mut window = state.borrow_mut();
            window.dpi = ((wparam.0 as u32) >> 16).max(96);
            window.silent_change = true;
            let layer = window.keyboard.is_some();
            drop(window);
            if layer {
                // A layer's rectangle comes from its spec, and the manager's suggested one
                // would move it off that placement.
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
            // Only a layer answers; a window the manager placed keeps the default hit test.
            state.borrow().keyboard?;
            let consumed = consumes_at(state, hwnd, packed_point(lparam));
            Some(LRESULT(if consumed {
                HTCLIENT as isize
            } else {
                HTTRANSPARENT as isize
            }))
        }
        WM_MOUSEACTIVATE => {
            let keyboard = state.borrow().keyboard;
            let command = match keyboard {
                // A window the manager places activates the ordinary way.
                None => return None,
                Some(KeyboardMode::None) => MA_NOACTIVATE,
                Some(KeyboardMode::Exclusive) => MA_ACTIVATE,
                // Windows has no focus-follows-pointer contract, so `FollowPointer` is the
                // same rule as `OnDemand`: a click on a consumed area focuses the surface.
                Some(KeyboardMode::OnDemand | KeyboardMode::FollowPointer) => {
                    let mut point = POINT::default();
                    // The cursor is where the click landed; the message carries no position.
                    let inside = unsafe { GetCursorPos(&mut point) }.is_ok()
                        && consumes_at(state, hwnd, point);
                    if inside { MA_ACTIVATE } else { MA_NOACTIVATE }
                }
            };
            Some(LRESULT(command as isize))
        }
        WM_MOUSEMOVE => {
            let pos = logical(state, lparam);
            let mut window = state.borrow_mut();
            window.pointer = Some(pos);
            window.events.push(Event::PointerMoved { pos });
            let arm = !window.tracking_leave;
            window.tracking_leave = true;
            drop(window);
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
            let button = button(message);
            let pos = logical(state, lparam);
            let mut window = state.borrow_mut();
            window.pointer = Some(pos);
            window.events.push(Event::PointerReleased { pos, button });
            drop(window);
            let _ = unsafe { ReleaseCapture() };
            Some(LRESULT(0))
        }
        WM_MOUSEWHEEL => {
            let notches = ((wparam.0 as u32) >> 16) as u16 as i16 as f32 / 120.0;
            let pointer = state.borrow().pointer;
            let pos = pointer.unwrap_or_else(|| logical(state, lparam));
            // A positive delta is a notch away from the user, which scrolls content up.
            state
                .borrow_mut()
                .events
                .push(Event::wheel(pos, -notches * input::WHEEL_NOTCH));
            Some(LRESULT(0))
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            key(state, (wparam.0 & 0xFFFF) as u16, true);
            Some(LRESULT(0))
        }
        WM_KEYUP | WM_SYSKEYUP => {
            key(state, (wparam.0 & 0xFFFF) as u16, false);
            Some(LRESULT(0))
        }
        WM_CHAR => {
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
            let hidden = state.borrow().ime_hidden;
            let flags = crate::ime::ui_flags(lparam, hidden);
            Some(unsafe { DefWindowProcW(hwnd, message, wparam, flags) })
        }
        WM_IME_STARTCOMPOSITION => {
            // The engine draws the composition from the preedit it is given, so the IME's
            // own composition window stays hidden and this needs no default handling. The
            // place is refreshed here because the caret is the last frame's, and the window
            // may have moved since.
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
            let events = crate::ime::composition(hwnd, lparam.0 as u32);
            state.borrow_mut().events.extend(events);
            Some(LRESULT(0))
        }
        WM_IME_ENDCOMPOSITION => {
            state.borrow_mut().events.push(Event::Preedit(None));
            Some(LRESULT(0))
        }
        WM_SETCURSOR => {
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
            state.borrow_mut().theme_changed = true;
            None
        }
        _ => None,
    }
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
    let point = packed_point(lparam);
    let scale = state.borrow().dpi.max(96) as f32 / 96.0;
    Point::new(point.x as f32 / scale, point.y as f32 / scale)
}

/// Whether a screen position lands on an area the last frame's tree consumes.
fn consumes_at(state: &RefCell<WindowState>, hwnd: HWND, mut point: POINT) -> bool {
    let window = state.borrow();
    let Some(areas) = window.input.as_ref() else {
        // Nothing published yet: the whole surface is the engine's, which is the default.
        return true;
    };
    // A refused conversion leaves the point in screen coordinates; the whole surface is the
    // better answer for a window that no longer maps where the message says.
    if !unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        return true;
    }
    let scale = window.dpi.max(96) as f32 / 96.0;
    let pos = Point::new(point.x as f32 / scale, point.y as f32 / scale);
    areas.iter().any(|area| area.contains(pos))
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

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
