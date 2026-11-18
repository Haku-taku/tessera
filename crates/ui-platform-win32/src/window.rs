//! A Win32 window: class registration, creation, the message pump, input and the extent.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ui_core::clipboard::ClipboardContent;
use ui_core::geom::{Point, Rect};
use ui_core::{CursorShape, Event};
use ui_platform::ThemeWatcher as _;
use ui_platform::driver::FrameHost;
use ui_platform::present::ImeState;
use ui_platform::window::{
    AutoHide, ColourPolicy, KeyboardMode, Layer, LayerSpec, PopupSpec, SurfaceRole, SurfaceSpec,
};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT,
    WAIT_OBJECT_0, WPARAM,
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
    GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::Ime::HIMC;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT,
    TrackMouseEvent, VK_ESCAPE,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETSTATE, ABM_GETTASKBARPOS, ABS_AUTOHIDE,
    APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWL_EXSTYLE, GWL_STYLE, GWLP_USERDATA, GetClientRect, GetCursorPos, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT,
    HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, IDC_CROSS,
    IDC_HAND, IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
    IsWindow, KillTimer,
    LoadCursorW, MA_ACTIVATE, MA_NOACTIVATE, MINMAXINFO, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PM_NOREMOVE, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    RegisterClassExW, SM_CXMINTRACK, SM_CYMINTRACK, SW_SHOW, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOZORDER, SetCursor, SetCursorPos, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CAPTURECHANGED,
    WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENTERSIZEMOVE,
    WM_ERASEBKGND, WM_EXITSIZEMOVE,
    WM_GETMINMAXINFO,
    WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION,
    WM_KEYDOWN, WM_KEYUP,
    WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEACTIVATE,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST,
    WM_NCLBUTTONDOWN, WM_PAINT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE, WM_WINDOWPOSCHANGED,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_TIMER, WM_APP,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_CAPTION, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_OVERLAPPEDWINDOW, WS_POPUP,
    WS_SYSMENU,
};
use windows::core::PCWSTR;

use crate::{WindowError, input};

/// The one message this backend uses that the syscall-binding module does not declare:
/// `WM_MOUSELEAVE` lives with the common controls, whose whole API surface is not worth a
/// feature flag for one constant.
const WM_MOUSELEAVE: u32 = 0x02A3;

/// Posted by a worker that finished what the next build must see — the shell menu's rows,
/// or a command whose effect the folder's read now shows.
pub(crate) const WAKE_MESSAGE: u32 = WM_APP + 0x52;

/// The strip an auto-hidden taskbar needs left free, in physical pixels.
///
/// The shell will not reveal a hidden taskbar while a window covers the monitor edge it hides
/// against, so every toolkit that fills the screen — Chromium, Windows Terminal — keeps this
/// much of that edge for the desktop.
const AUTO_HIDE_STRIP: i32 = 2;

/// What a window of this backend is, which decides how it activates and what may shape it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Kind {
    /// A window the manager frames, decorates and activates.
    #[default]
    Window,
    /// An undecorated surface held at a level over one monitor.
    Layer,
    /// A panel's own window, owned by the surface that opened it.
    Popup,
    /// A secondary window owned by another, and modal over it when asked.
    Dialog,
}

/// What the window procedure has collected since the loop last looked.
///
/// Windows hands a window its messages on the thread that created it, so this state is
/// confined to that thread and shared with the procedure by raw pointer, not by `Arc`.
#[derive(Default)]
struct WindowState {
    kind: Kind,
    events: Vec<Event>,
    pointer: Option<Point>,
    modifiers: ui_core::Modifiers,
    close_requested: bool,
    cursor: Option<CursorShape>,
    /// Focus, extent or scale changed without an `Event`; a frame is still owed.
    silent_change: bool,
    /// The display mode or topology changed: what the output presents has to be re-read.
    display_changed: bool,
    /// The output this window is on, so a move to another one is noticed: that output's
    /// refresh rate paces the surface, and its chain carries that output's colour space.
    monitor: isize,
    /// Physical pixels per logical unit, in whole DPI.
    dpi: u32,
    /// The smallest client area the spec allows, in logical units.
    min_size: Option<(u32, u32)>,
    /// A layer's keyboard request; `None` for a window the manager places, which has no
    /// shape and no activation rule of its own.
    keyboard: Option<KeyboardMode>,
    /// The input-method state the last frame published, so one change applies once.
    ime: Option<ImeState>,
    /// The input context taken away while the window has no field; restored when one needs it.
    imm: Option<HIMC>,
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
    /// The shell's move or resize loop has the thread: it dispatches this window's
    /// messages and owns the pointer until the release.
    size_move: bool,
    /// The popups this window opened, which it carries when it moves and hands the grab to.
    popups: Rc<RefCell<Vec<OwnedPopup>>>,
    /// A popup holds the mouse for as long as it is open, which is how an outside press
    /// reaches the engine at all.
    grab: bool,
}

/// A popup window one surface opened, and the request that decides where it goes.
#[derive(Clone, Copy)]
struct OwnedPopup {
    hwnd: HWND,
    spec: PopupSpec,
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
    /// Whether the frame's own alpha reaches the desktop: a layer shows it, a decorated
    /// window is opaque and the compositor ignores it.
    alpha: crate::presenter::Alpha,
    /// What the surface's colour follows, as the surface spec asked.
    colour: ColourPolicy,
    /// The surface's frame window, set once its session exists. Absent for a window a
    /// caller drives directly, which is always drawable and paces on `DwmFlush`.
    pace: Option<std::sync::Arc<crate::frame_pace::FramePace>>,
    /// The surface this popup hangs off; `None` for a window or a layer.
    owner: Option<Owner>,
    /// The window this dialog is modal over, disabled while the dialog is up and enabled
    /// again when it goes. `None` for every other kind, and for a non-modal dialog.
    dialog_parent: Option<HWND>,
}

/// The surface a popup hangs off: the window that places and carries it.
struct Owner {
    hwnd: HWND,
    /// The list the parent carries, so the popup can move itself in it and leave on drop.
    popups: Rc<RefCell<Vec<OwnedPopup>>>,
}

impl Win32Window {
    /// Open the window `spec` asks for: a window the manager places, or a layer this backend
    /// approximates with an undecorated popup held at the level the spec names.
    pub fn new(spec: SurfaceSpec) -> Result<Self, WindowError> {
        // A manifest may already have fixed the process's awareness; a refusal means it did.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let opening = match &spec.role {
            SurfaceRole::Window { size } => opening_window(*size)?,
            SurfaceRole::Layer(layer) => opening_layer(layer)?,
            SurfaceRole::Dialog { .. } => {
                return Err(WindowError::Win32(String::from(
                    "a dialog opens through the surface it belongs to",
                )));
            }
        };
        Self::build(spec, opening, None, None, None)
    }

    /// A dialog's own window, owned by `parent` and centred on it.
    ///
    /// Ownership is what keeps it above its parent, out of the taskbar, and closed with it.
    /// A modal dialog also disables the parent; `Drop` re-enables it, so a dialog that
    /// closes itself, is closed by its parent, or fails to open leaves no disabled window
    /// behind.
    pub fn new_dialog(parent: &Win32Window, spec: SurfaceSpec) -> Result<Self, WindowError> {
        Self::open_dialog(parent, spec, WS_OVERLAPPEDWINDOW)
    }

    /// A panel's window: framed and owned like a dialog, but with no sizing border, because
    /// its size is its content's and the user must not drag it away from that.
    pub fn new_panel_dialog(
        parent: &Win32Window,
        spec: SurfaceSpec,
    ) -> Result<Self, WindowError> {
        Self::open_dialog(parent, spec, WS_CAPTION | WS_SYSMENU)
    }

    fn open_dialog(
        parent: &Win32Window,
        spec: SurfaceSpec,
        style: WINDOW_STYLE,
    ) -> Result<Self, WindowError> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let size = match &spec.role {
            SurfaceRole::Dialog { size, .. } => *size,
            _ => return Err(WindowError::Win32(String::from("not a dialog spec"))),
        };
        let modal = spec.dialog_modal().unwrap_or(false);
        let opening = opening_dialog(parent.hwnd, size, style)?;
        let window = Self::build(
            spec,
            opening,
            None,
            Some(parent.hwnd),
            modal.then_some(parent.hwnd),
        )?;
        if modal {
            // The platform half of modality; the host also filters the parent's events,
            // because a client that trusts only this would still see them on Wayland.
            let _ = unsafe { EnableWindow(parent.hwnd, false) };
        }
        Ok(window)
    }

    /// A panel's own window: undecorated, owned by `parent`, and holding the mouse.
    ///
    /// The owner is what keeps it above the window it belongs to and out of the taskbar; the
    /// grab is what lets a press outside the panel reach its presenter, which is how the
    /// engine hears about a click that dismisses it.
    pub fn new_popup(
        parent: &Win32Window,
        spec: SurfaceSpec,
        popup: PopupSpec,
    ) -> Result<Self, WindowError> {
        // A manifest may already have fixed the process's awareness; a refusal means it did.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let owner = Owner {
            hwnd: parent.hwnd,
            popups: Rc::clone(&parent.state.borrow().popups),
        };
        let rect = popup_rect(owner.hwnd, &popup)?;
        let opening = Opening {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            style: WS_POPUP,
            // Alpha is composed, so the panel's rounded corners and shadow reach the desktop.
            ex_style: WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP,
            dpi: unsafe { GetDpiForWindow(owner.hwnd) }.max(96),
            kind: Kind::Popup,
            keyboard: None,
            alpha: crate::presenter::Alpha::Premultiplied,
        };
        let window = Self::build(spec, opening, Some((owner, popup)), Some(parent.hwnd), None)?;
        // The grab outlives the press that opened the panel: it is released on close, or when
        // the panel's own child takes it.
        let _ = unsafe { SetCapture(window.hwnd) };
        window.state.borrow_mut().grab = true;
        Ok(window)
    }

    /// Where a popup of this window goes for `spec`, and the request the window now carries.
    ///
    /// A popup has no positioner of its own: it is placed again from the request, against the
    /// work area of the monitor its owner is on.
    pub fn move_popup(&mut self, spec: PopupSpec) -> Result<(), WindowError> {
        let Some(owner) = &self.owner else {
            return Err(WindowError::Win32(String::from(
                "this window is not a popup",
            )));
        };
        if let Some(entry) = owner
            .popups
            .borrow_mut()
            .iter_mut()
            .find(|owned| owned.hwnd == self.hwnd)
        {
            entry.spec = spec;
        }
        let rect = popup_rect(owner.hwnd, &spec)?;
        // The return value says whether the window moved, which a move to the same place
        // does not; nothing here has a use for it.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        Ok(())
    }

    fn build(
        surface: SurfaceSpec,
        opening: Opening,
        owner: Option<(Owner, PopupSpec)>,
        z_order_owner: Option<HWND>,
        dialog_parent: Option<HWND>,
    ) -> Result<Self, WindowError> {
        let app_id = if surface.app_id.is_empty() {
            "crabsnap"
        } else {
            surface.app_id.as_str()
        };
        let class = wide(app_id);
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|error| WindowError::Win32(format!("GetModuleHandleW: {error}")))?;
        let instance = HINSTANCE(instance.0);
        register_class(&class, instance)?;
        let title = wide(&surface.name);
        let state = Rc::new(RefCell::new(WindowState {
            kind: opening.kind,
            dpi: opening.dpi,
            keyboard: opening.keyboard,
            min_size: surface.min_size,
            // Recorded on the first `WM_WINDOWPOSCHANGED`; there is no window yet here.
            monitor: 0,
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
                z_order_owner,
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
        let owner = owner.map(|(owner, popup)| {
            owner.popups.borrow_mut().push(OwnedPopup { hwnd, spec: popup });
            owner
        });
        // A layer's pixels are a second window: owned, so it stays above and dies with this
        // one, and styled to be invisible to the hit test, which this window answers.
        let visual = if opening.kind == Kind::Layer {
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
            let command = if matches!(opening.kind, Kind::Window | Kind::Dialog) {
                SW_SHOW
            } else {
                // Showing a layer or a panel must not take the keyboard from what it covers.
                SW_SHOWNOACTIVATE
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
        if matches!(opening.kind, Kind::Window | Kind::Dialog) {
            // Only a framed window has a caption, and a dialog is framed the same way.
            theme_caption(hwnd);
        }
        // OLE hands every drag over this window to the target; its callbacks only queue
        // events, so nothing here needs the engine to exist yet. A panel takes no drops: its
        // presenter has nowhere to deliver them, and registering would swallow the drag.
        let drag = Rc::new(crate::dnd::DragQueue::new(hwnd));
        let drop_target = if opening.kind == Kind::Popup {
            None
        } else {
            crate::dnd::register(hwnd, Rc::clone(&drag))
        };
        Ok(Self {
            hwnd,
            visual,
            state,
            drag,
            drop_target,
            alpha: opening.alpha,
            colour: surface.colour,
            pace: None,
            owner,
            dialog_parent,
        })
    }

    /// The window handle a swap chain presents through.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// What OLE's drop target queues while a drag is over this window.
    pub(crate) fn drag_queue(&self) -> Rc<crate::dnd::DragQueue> {
        self.drag.clone()
    }

    /// The window the frame is composed into.
    pub fn visual_hwnd(&self) -> HWND {
        self.visual
    }

    /// Whether the frame's own alpha is composed over the desktop.
    pub fn alpha(&self) -> crate::presenter::Alpha {
        self.alpha
    }

    /// What this surface's colour follows, as its spec asked.
    pub fn colour(&self) -> ColourPolicy {
        self.colour
    }

    /// Whether the shell's move or resize loop has this window's thread and pointer.
    pub fn in_size_move(&self) -> bool {
        self.state.borrow().size_move
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
        // The frame this window really has: a dialog without a sizing border measures
        // smaller than WS_OVERLAPPEDWINDOW, and the difference shows as a blank strip.
        let style = WINDOW_STYLE(unsafe { GetWindowLongPtrW(self.hwnd, GWL_STYLE) } as u32);
        let ex_style = WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) } as u32);
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: size.0.max(1) as i32,
            bottom: size.1.max(1) as i32,
        };
        unsafe { AdjustWindowRectExForDpi(&mut frame, style, false, ex_style, dpi) }
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

    /// The monitor's work area, in this window's client coordinates and logical units.
    ///
    /// This is the output a panel must fit: the engine places platform panels against it,
    /// so the surface the host opens for one stands where the tree's geometry says it does.
    pub fn work_area(&self) -> Option<Rect> {
        let mut origin = POINT::default();
        if !unsafe { ClientToScreen(self.hwnd, &mut origin) }.as_bool() {
            return None;
        }
        let monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return None;
        }
        let scale = self.scale();
        let work = info.rcWork;
        Some(Rect::new(
            (work.left - origin.x) as f32 / scale,
            (work.top - origin.y) as f32 / scale,
            (work.right - work.left) as f32 / scale,
            (work.bottom - work.top) as f32 / scale,
        ))
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
        // Inside a modal gesture the shell's own loop dispatches this thread's messages;
        // taking them here as well would remove them from under the gesture.
        if crate::modal::inside() {
            return false;
        }
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

    /// The wheel steps queued since the last call, in arrival order.
    ///
    /// A system drag runs its own modal loop on this thread, so a wheel that arrives
    /// mid-gesture would wait for the turn after the drop; the drag's frame pump takes
    /// these and leaves the pointer's own messages for that turn.
    pub(crate) fn take_wheel_events(&mut self) -> Vec<Event> {
        let mut state = self.state.borrow_mut();
        let mut wheel = Vec::new();
        state.events.retain(|event| match event {
            Event::Scroll { .. } => {
                wheel.push(event.clone());
                false
            }
            _ => true,
        });
        wheel
    }

    /// Publish what dropping the drag under the pointer would do, for OLE's own answer.
    pub fn set_drop_effect(&self, effect: Option<ui_core::DropEffect>) {
        self.drag.set_effect(effect);
    }

    /// Start a drag this app carries, so another window can take the payload.
    ///
    /// The session's own form arms a frame pump; this one, for callers holding only a
    /// window, lets the surface hold still for the gesture.
    pub fn begin_drag(&mut self, drag: ui_platform::view::NativeDrag) -> bool {
        crate::dnd::begin(self.hwnd, drag, &self.drag, None)
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

    /// Adopt the frame window of the surface this window presents through.
    ///
    /// The session sets this once its render thread has a chain; the event belongs to the
    /// pace, so replacing it here never closes the previous one.
    pub(crate) fn set_frame_pace(&mut self, pace: std::sync::Arc<crate::frame_pace::FramePace>) {
        self.pace = Some(pace);
    }

    /// Wait for the desktop compositor to take the next frame.
    ///
    /// The signal is the render thread's, and it arrives when that thread has finished the
    /// frame it took; the wait also ends on input, so a resize or a title-bar drag is
    /// dispatched by the loop's next turn instead of waiting behind the frame in flight.
    pub fn wait_for_frame(&mut self) {
        let Some(pace) = self.pace.as_ref() else {
            wait_for_composition();
            return;
        };
        let Some(handle) = pace.handle() else {
            wait_for_composition();
            return;
        };
        let deadline = Instant::now() + Duration::from_millis(500);
        while !pace.can_draw() {
            let now = Instant::now();
            if now >= deadline {
                // A render thread that never answers must not deadlock the app; the next
                // turn either draws or reports the thread's own failure.
                return;
            }
            let millis = bounded_millis(Some(deadline - now));
            let result = unsafe {
                MsgWaitForMultipleObjectsEx(Some(&[handle]), millis, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
            };
            if result == WAIT_OBJECT_0 {
                // The wait consumed the auto-reset object: the frame is finished.
                return;
            }
            if result.0 == WAIT_OBJECT_0.0 + 1 {
                // Input arrived first. Return to the loop, which dispatches it before the
                // next frame; dispatching here would run the app inside its own pace.
                return;
            }
        }
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
    /// Only a layer has a shape to publish: its pixels are a second window, and the shape is
    /// what decides where the pointer is passed through. A popup's alpha carries its shadow,
    /// so clipping the window to the areas would clip the picture too.
    pub fn set_input_region(&mut self, areas: &[Rect]) {
        let mut state = self.state.borrow_mut();
        if state.kind != Kind::Layer || state.input.as_deref() == Some(areas) {
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
        // A panel never has the keyboard of its own, so an input context here would sit on a
        // window the input method cannot reach; the owner's context serves the whole tree.
        if self.state.borrow().kind == Kind::Popup {
            return;
        }
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
        let mut attached = false;
        if let Some(session) = self.tsf(wants_tsf) {
            attached = {
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
        }
        // IMM32 serves every state TSF does not. A window with no field keeps no context at
        // all: an associated one lets the user's own switch open a composition nothing takes.
        associate_ime(&self.state, self.hwnd, state.enabled && !attached);
        if attached {
            return;
        }
        if !state.enabled {
            return;
        }
        if crate::ime::set_open(self.hwnd, state.enabled, open) {
            self.state.borrow_mut().ime_open = state.enabled;
        }
        if let Some(caret) = state.cursor {
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

    /// Read the clipboard into a paste event; false when it holds nothing this app reads.
    pub fn request_paste(&mut self) -> bool {
        let Some(content) = crate::clipboard::read_paste(Some(self.hwnd)) else {
            return false;
        };
        self.state
            .borrow_mut()
            .events
            .push(Event::Paste(content));
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
    kind: Kind,
    /// A layer's keyboard request; `None` for a window the manager places.
    keyboard: Option<KeyboardMode>,
    /// Whether the desktop should see through the frame where it is transparent.
    alpha: crate::presenter::Alpha,
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
        // The frame arrives through a composition visual, so the shell must not keep a
        // redirection surface behind the window: it would be black, and DXGI would own it.
        ex_style: WS_EX_NOREDIRECTIONBITMAP,
        dpi,
        kind: Kind::Window,
        keyboard: None,
        alpha: crate::presenter::Alpha::Opaque,
    })
}

/// A framed window centred on `parent`, sized for `size` logical pixels.
///
/// The system frames and decorates a dialog like a window; ownership, not the style, is
/// what keeps it above the surface that opened it.
fn opening_dialog(
    parent: HWND,
    size: (u32, u32),
    style: WINDOW_STYLE,
) -> Result<Opening, WindowError> {
    let dpi = unsafe { GetDpiForWindow(parent) }.max(96);
    let scale = dpi as f32 / 96.0;
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: (size.0 as f32 * scale).round().max(1.0) as i32,
        bottom: (size.1 as f32 * scale).round().max(1.0) as i32,
    };
    unsafe {
        AdjustWindowRectExForDpi(&mut frame, style, false, WS_EX_NOREDIRECTIONBITMAP, dpi)
    }
    .map_err(|error| WindowError::Win32(format!("AdjustWindowRectExForDpi: {error}")))?;
    let width = frame.right - frame.left;
    let height = frame.bottom - frame.top;
    let mut bounds = RECT::default();
    unsafe { GetWindowRect(parent, &mut bounds) }.map_err(|error| {
        WindowError::Win32(format!("GetWindowRect the parent's rectangle: {error}"))
    })?;
    let x = bounds.left + ((bounds.right - bounds.left) - width) / 2;
    let y = bounds.top + ((bounds.bottom - bounds.top) - height) / 2;
    Ok(Opening {
        x,
        y,
        width,
        height,
        style,
        ex_style: WS_EX_NOREDIRECTIONBITMAP,
        dpi,
        kind: Kind::Dialog,
        keyboard: None,
        alpha: crate::presenter::Alpha::Opaque,
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
        kind: Kind::Layer,
        keyboard: Some(layer.keyboard),
        alpha: crate::presenter::Alpha::Premultiplied,
    })
}

/// Where a popup goes, in physical screen pixels, from the rect the tree placed it at.
///
/// The engine constrains the panel against the output before the host opens the surface,
/// so the request is final: moving it here would leave the input the grab routes back into
/// the tree pointing at geometry the user is not looking at.
fn popup_rect(owner: HWND, spec: &PopupSpec) -> Result<RECT, WindowError> {
    let mut origin = POINT::default();
    if !unsafe { ClientToScreen(owner, &mut origin) }.as_bool() {
        return Err(WindowError::Win32(String::from(
            "ClientToScreen: the owner has no client area",
        )));
    }
    let scale = unsafe { GetDpiForWindow(owner) }.max(96) as f32 / 96.0;
    let at = |value: f32, base: i32| base + (value * scale).round() as i32;
    let left = at(spec.rect.origin.x, origin.x);
    let top = at(spec.rect.origin.y, origin.y);
    Ok(RECT {
        left,
        top,
        right: left + (spec.rect.size.w * scale).round().max(1.0) as i32,
        bottom: top + (spec.rect.size.h * scale).round().max(1.0) as i32,
    })
}

/// Carry every popup this window owns along with it.
///
/// An owned window does not follow its owner: the shell only guarantees it stays above and is
/// destroyed with it, so a move or a resize is when the backend places the panels again.
fn place_popups(state: &RefCell<WindowState>, owner: HWND) {
    let popups = Rc::clone(&state.borrow().popups);
    let owned: Vec<OwnedPopup> = popups.borrow().clone();
    for popup in owned {
        let Ok(rect) = popup_rect(owner, &popup.spec) else {
            continue;
        };
        // The return value says whether the window moved; a popup already there needs nothing.
        let _ = unsafe {
            SetWindowPos(
                popup.hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    }
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
        // A modal dialog disabled its parent; the parent must not outlive it disabled,
        // whether the dialog was dismissed, closed with its parent, or failed to show.
        if let Some(parent) = self.dialog_parent {
            let _ = unsafe { EnableWindow(parent, true) };
        }
        // The owner must stop carrying a window that is about to go, or its next move would
        // place a handle the shell has already freed.
        if let Some(owner) = &self.owner {
            owner
                .popups
                .borrow_mut()
                .retain(|owned| owned.hwnd != self.hwnd);
            // Only this window's own grab: a child panel may hold it instead, and releasing
            // that one would leave the child with no capture to dismiss it.
            if self.state.borrow().grab && unsafe { GetCapture() } == self.hwnd {
                let _ = unsafe { ReleaseCapture() };
            }
        }
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
        // The compositor's frame window, when the chain has one; otherwise every turn may
        // present and `DwmFlush` does the pacing.
        self.pace.as_ref().is_none_or(|pace| pace.can_draw())
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
        WAKE_MESSAGE => {
            // A message alone only wakes the pump: it carries no event, so the driver would
            // sleep again without building the frame that reads what the worker finished.
            state.borrow_mut().silent_change = true;
            Some(LRESULT(0))
        }
        WM_DESTROY => {
            state.borrow_mut().close_requested = true;
            None
        }
        WM_SIZE => {
            state.borrow_mut().silent_change = true;
            place_popups(state, hwnd);
            None
        }
        WM_MOVE => {
            place_popups(state, hwnd);
            None
        }
        WM_WINDOWPOSCHANGED => {
            // A window moved to another output presents on that output now: its refresh
            // rate is the new pacing, and its chain was opened for the old output's colour
            // space. `None` keeps the default handling, which is what places the window.
            let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let mut window = state.borrow_mut();
            if window.monitor == 0 {
                // The first placement: nothing was read from an output yet.
                window.monitor = monitor.0 as isize;
            } else if window.monitor != monitor.0 as isize {
                window.monitor = monitor.0 as isize;
                window.display_changed = true;
                window.silent_change = true;
            }
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
            let enabled = state.borrow().ime.as_ref().is_some_and(|ime| ime.enabled);
            state.borrow_mut().silent_change = true;
            // An activated window can come back with an association the input method made
            // for it; a window with no field takes none, whatever the method did meanwhile.
            if message == WM_SETFOCUS && !enabled {
                associate_ime(state, hwnd, false);
            }
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
            // A popup is placed by its owner, against the owner's scale; the suggested
            // rectangle would move it somewhere the request never named.
            if window.kind == Kind::Popup {
                return Some(LRESULT(0));
            }
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
            // The sizing border is a drag `DefWindowProcW` would run in a modal loop; this
            // window sizes itself instead, one message at a time, so a frame follows every
            // move. Every other non-client part — the title bar most of all — keeps the
            // default handling: `DefWindowProc` activates, raises, starts the move loop
            // and owns snapping, and `WM_ENTERSIZEMOVE` arms the timer that keeps the
            // rest of the process drawing while that loop runs.
            let edge = (wparam.0 as u32) & 0xFFFF;
            if sizing_edge(edge) {
                begin_sizing(state, hwnd, edge, packed_point(lparam));
                return Some(LRESULT(0));
            }
            None
        }
        WM_ENTERSIZEMOVE => {
            state.borrow_mut().size_move = true;
            // The shell's move or resize loop owns this thread's queue until the pointer
            // comes up, and it dispatches nothing but input and `WM_TIMER`. This timer is
            // that channel: every other surface of the process draws from it while the
            // moved one keeps the frame it had.
            crate::modal::enter_shell();
            // SAFETY: the window is this thread's and the id is this module's own.
            unsafe {
                let _ = SetTimer(
                    Some(hwnd),
                    crate::modal::SHELL_TIMER_ID,
                    crate::modal::TICK_MS,
                    None,
                );
            }
            // The default procedure does nothing for this notification, and Qt skips it
            // the same way; the loop itself runs either way.
            Some(LRESULT(0))
        }
        WM_EXITSIZEMOVE => {
            state.borrow_mut().size_move = false;
            crate::modal::exit_shell();
            // SAFETY: the id is the one `SetTimer` was given, on this window.
            unsafe {
                let _ = KillTimer(Some(hwnd), crate::modal::SHELL_TIMER_ID);
            }
            // A move may have crossed a display: its rate and colour space are the new
            // output's, and `WM_WINDOWPOSCHANGED` has already asked for that re-read.
            None
        }
        WM_MOUSEACTIVATE => {
            let window = state.borrow();
            // A panel never takes the keyboard from the window it belongs to: the engine's
            // focus scope already names it, and the owner keeps receiving the keys.
            if window.kind == Kind::Popup {
                return Some(LRESULT(MA_NOACTIVATE as isize));
            }
            let command = match window.keyboard {
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
            let following = window.keyboard == Some(KeyboardMode::FollowPointer);
            drop(window);
            if following {
                remember_foreground(state, hwnd);
            }
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            // Armed on every move rather than once per entry: the call is idempotent, and
            // a tracking the system ended without a message must not leave the window
            // unable to hear the next leave. A refusal only means the leave event is
            // missing; the pointer still moves.
            let _ = unsafe { TrackMouseEvent(&mut track) };
            Some(LRESULT(0))
        }
        WM_MOUSELEAVE => {
            let mut window = state.borrow_mut();
            window.pointer = None;
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
            let grab = window.grab;
            drop(window);
            // A panel's grab outlives the press: it is what hears the click that dismisses it.
            // A window that does not hold the capture must not take it from the panel that
            // does: the panel would lose the grab it dismisses with.
            if !grab && unsafe { GetCapture() } == hwnd {
                let _ = unsafe { ReleaseCapture() };
            }
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
            let mut window = state.borrow_mut();
            // Another window took the mouse: the drag ends where the edges already are.
            window.sizing = None;
            // A panel whose grab a foreign window took cannot hear a dismissal any more, so
            // it closes now. Its own grab set again on a press reports the panel itself as
            // the gainer, and a panel of its own taking it is the nested case: neither is a
            // loss, and the topmost one keeps hearing the pointer.
            //
            // A null gainer is not a takeover: the system drops a grab it did not grant for a
            // gesture yet — a background thread's capture ends with the button that asked for
            // it — and no window owns the pointer now. Reading that as a dismissal closed
            // every menu the moment its panel opened off a click.
            if window.kind == Kind::Popup && window.grab {
                let gained = HWND(lparam.0 as *mut core::ffi::c_void);
                let kept = gained.is_invalid()
                    || gained == hwnd
                    || window.popups.borrow().iter().any(|owned| owned.hwnd == gained);
                if !kept {
                    window.close_requested = true;
                }
            }
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
        WM_TIMER => {
            // A modal gesture (OLE's drag, the system's title-bar move) owns the thread's
            // loop; its timer is the only thing that can make the engine build a frame in
            // the meantime.
            if crate::modal::pump_frame() {
                Some(LRESULT(0))
            } else {
                None
            }
        }
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
    // The tracking size is per-DPI: the system-wide figure is the 96-DPI one, and using it
    // on a scaled display lets a window shrink below what its frame can show.
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let system = unsafe {
        (
            GetSystemMetricsForDpi(SM_CXMINTRACK, dpi),
            GetSystemMetricsForDpi(SM_CYMINTRACK, dpi),
        )
    };
    let Some((width, height)) = declared else {
        return system;
    };
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

/// Give the window its input context, or take it away; the taken one is kept on the state.
fn associate_ime(state: &RefCell<WindowState>, hwnd: HWND, wanted: bool) {
    let mut saved = state.borrow_mut().imm.take();
    crate::ime::set_associated(hwnd, wanted, &mut saved);
    state.borrow_mut().imm = saved;
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
    if input::menu(vk, held.shift) {
        state.events.push(if down {
            Event::KeyPressed {
                key: ui_core::Key::Menu,
            }
        } else {
            Event::KeyReleased {
                key: ui_core::Key::Menu,
            }
        });
    } else if let Some(named) = input::named(vk) {
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
    pointer_position(point.x, point.y, scale)
}

/// Where the pointer is, given the client pixel it is over.
///
/// A Win32 client position names a pixel by its top-left corner, while the pointer sits
/// inside the pixel it is over; its centre keeps the far edge of the surface reachable.
fn pointer_position(x: i32, y: i32, scale: f32) -> Point {
    Point::new((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_over_the_last_pixel_sits_inside_it() {
        // A 2560×1440 client reports its last pixel as 2559: at the corner the pointer never
        // reaches 2560, and a crop rounded outwards loses the last row and column.
        assert_eq!(pointer_position(0, 0, 1.0), Point::new(0.5, 0.5));
        assert_eq!(
            pointer_position(2559, 1439, 1.0),
            Point::new(2559.5, 1439.5)
        );
        let scaled = pointer_position(0, 0, 1.25);
        assert!((scaled.x - 0.4).abs() < 1e-6 && (scaled.y - 0.4).abs() < 1e-6);
    }
}
