//! A Win32 window: class registration, creation, the message pump and the client's extent.

use ui_platform::window::{SurfaceRole, SurfaceSpec};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
    GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, IDC_ARROW, IsWindow, LoadCursorW, MSG, PM_REMOVE, PeekMessageW,
    RegisterClassExW, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};
use windows::core::PCWSTR;

use crate::WindowError;

/// A window the window manager places, decorates and stacks.
///
/// Dropping it destroys the window, so a presenter that draws into it must be dropped first.
pub struct Win32Window {
    hwnd: HWND,
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
                None,
            )
        }
        .map_err(|error| WindowError::Win32(format!("CreateWindowExW: {error}")))?;
        // The return value says whether the window was previously visible, which is not
        // an outcome this backend has a use for.
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        Ok(Self { hwnd })
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

    /// Physical pixels per logical unit.
    pub fn scale(&self) -> f32 {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 {
            1.0
        } else {
            dpi as f32 / 96.0
        }
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
}

impl Drop for Win32Window {
    fn drop(&mut self) {
        if self.alive() {
            let _ = unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

/// The default procedure already handles paint, close and the resize border, so the backend
/// keeps no window state for it and every message takes its default path.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
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

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
