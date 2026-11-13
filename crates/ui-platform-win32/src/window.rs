//! A Win32 window: class registration, creation, the message pump, input and the extent.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use ui_core::clipboard::ClipboardContent;
use ui_core::geom::Point;
use ui_core::{CursorShape, Event};
use ui_platform::driver::FrameHost;
use ui_platform::window::{SurfaceRole, SurfaceSpec};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, GlobalFree, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
    RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW, HBRUSH,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow, PAINTSTRUCT,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
    GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetCursorPos, GetWindowLongPtrW, HTCLIENT, IDC_ARROW, IDC_CROSS,
    IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
    IsWindow, LoadCursorW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_NOREMOVE,
    PM_REMOVE, PeekMessageW, QS_ALLINPUT, RegisterClassExW, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOZORDER, SetCursor, SetCursorPos, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, WINDOW_EX_STYLE, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSEXW,
    WS_OVERLAPPEDWINDOW,
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
}

/// A window the window manager places, decorates and stacks.
///
/// Dropping it destroys the window, so a presenter that draws into it must be dropped first.
pub struct Win32Window {
    hwnd: HWND,
    state: Rc<RefCell<WindowState>>,
}

impl Win32Window {
    /// Open the window `spec` asks for. Only the window role exists on this backend so far.
    pub fn new(spec: SurfaceSpec) -> Result<Self, WindowError> {
        let size = match spec.role {
            SurfaceRole::Window { size } => size,
            SurfaceRole::Layer(_) => return Err(WindowError::UnsupportedRole),
        };
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
        // The spec's size is logical; the window is created at the system's DPI.
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
        let title = wide(&spec.name);
        let state = Rc::new(RefCell::new(WindowState {
            dpi,
            ..WindowState::default()
        }));
        // The procedure needs the state from `WM_NCCREATE` on, before `CreateWindowExW`
        // returns; the extra count is released in `WM_NCDESTROY`, which runs even on failure
        // of a later step in creation.
        let raw = Rc::as_ptr(&state);
        unsafe { Rc::increment_strong_count(raw) };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR::from_raw(class.as_ptr()),
                PCWSTR::from_raw(title.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                frame.right - frame.left,
                frame.bottom - frame.top,
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
            let _ = ShowWindow(hwnd, SW_SHOW);
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

    /// Put text on the clipboard; false when the content is not text this backend carries.
    pub fn write_clipboard(&mut self, content: ClipboardContent) -> bool {
        write_clipboard_text(Some(self.hwnd), content)
    }

    /// Read the clipboard into a paste event; false when it holds no text.
    pub fn request_paste(&mut self) -> bool {
        let Some(text) = (unsafe {
            if OpenClipboard(Some(self.hwnd)).is_err() {
                return false;
            }
            let text = read_clipboard_text();
            let _ = CloseClipboard();
            text
        }) else {
            return false;
        };
        self.state.borrow_mut().events.push(Event::Paste(text));
        true
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
            drop(window);
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

/// A message's client position in logical units.
fn logical(state: &RefCell<WindowState>, lparam: LPARAM) -> Point {
    let x = (lparam.0 as i32) as i16 as i32;
    let y = ((lparam.0 as i32) >> 16) as i16 as i32;
    let scale = state.borrow().dpi.max(96) as f32 / 96.0;
    Point::new(x as f32 / scale, y as f32 / scale)
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

/// Replace the clipboard contents with `wide`, a NUL-terminated UTF-16 string.
unsafe fn store_clipboard_text(wide: &[u16]) -> bool {
    unsafe {
        if EmptyClipboard().is_err() {
            return false;
        }
        let bytes = std::mem::size_of_val(wide);
        let Ok(handle) = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
            return false;
        };
        let target = GlobalLock(handle);
        if target.is_null() {
            let _ = GlobalFree(Some(handle));
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), target.cast::<u8>(), bytes);
        let _ = GlobalUnlock(handle);
        match SetClipboardData(
            CF_UNICODETEXT.0 as u32,
            Some(windows::Win32::Foundation::HANDLE(handle.0)),
        ) {
            // On success the clipboard owns the block and frees it itself.
            Ok(_) => true,
            Err(_) => {
                let _ = GlobalFree(Some(handle));
                false
            }
        }
    }
}

/// The clipboard's text, when it holds any.
unsafe fn read_clipboard_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
        let pointer = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0));
        if pointer.is_null() {
            return None;
        }
        let text = pointer.cast::<u16>();
        // Bounded by the block the clipboard handed over: nothing guarantees a terminator
        // inside it, and scanning past the end is the bug this bound exists to prevent.
        let capacity = GlobalSize(windows::Win32::Foundation::HGLOBAL(handle.0)) / 2;
        let mut length = 0usize;
        while length < capacity && *text.add(length) != 0 {
            length += 1;
        }
        let slice = std::slice::from_raw_parts(text, length);
        let text = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
        Some(text)
    }
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

/// Put text on the system clipboard with no window of its own.
///
/// A process-wide copy has no window to own the selection, and on Windows the clipboard
/// belongs to the task when opened without one.
pub fn set_system_clipboard(content: ClipboardContent) -> bool {
    write_clipboard_text(None, content)
}

/// Open the clipboard, store `content`, and close it again.
fn write_clipboard_text(owner: Option<HWND>, content: ClipboardContent) -> bool {
    let ClipboardContent::Text(text) = content else {
        return false;
    };
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(owner).is_err() {
            return false;
        }
        let stored = store_clipboard_text(&wide);
        // Closing is the copy's real commit; a failure leaves the old clipboard intact.
        let _ = CloseClipboard();
        stored
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
