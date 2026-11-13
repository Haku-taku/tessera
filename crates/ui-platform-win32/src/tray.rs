//! The notification area icon: a hidden window per icon, the shell's icon and its menu.
//!
//! The shell talks to one window per icon: `Shell_NotifyIcon` posts its callback there, and
//! the same window owns the popup menu, because Win32 menus belong to a window. The window is
//! never shown and never activated on its own; it exists to receive messages, and the shell
//! is the only sender.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ui_core::Id;
use ui_core::event::PointerButton;
use ui_core::geom::Point;
use ui_core::menu::MenuItem;
use ui_platform::tray::{Icon, Menu, MenuEntry, TrayEvent, TrayHandle, TrayId};
use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject, HGDIOBJ, MONITOR_DEFAULTTONEAREST, MonitorFromPoint,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETFOCUS,
    NIM_SETVERSION, NIN_SELECT, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW,
    DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow, GWLP_USERDATA, GetCursorPos,
    GetWindowLongPtrW, HICON, HMENU, ICONINFO, MENU_ITEM_FLAGS, MF_CHECKED, MF_GRAYED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, PostMessageW, RegisterClassExW, RegisterWindowMessageW,
    SetForegroundWindow, SetWindowLongPtrW, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU, WM_LBUTTONDBLCLK,
    WM_LBUTTONUP, WM_MBUTTONUP, WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_RBUTTONUP, WNDCLASSEXW,
    WS_POPUP,
};
use windows::core::{BOOL, PCWSTR};

use crate::window::wide;

/// The message every tray window receives from the shell.
const TRAY_MESSAGE: u32 = WM_APP + 0x51;

/// The class every tray window is created from.
const TRAY_CLASS: &str = "crabsnap-tray";

/// The identifier the shell files an icon under. Each window holds one icon, so one id
/// covers the process.
const TRAY_UID: u32 = 1;

/// The shell's message for "the icon was activated with the keyboard", which the syscall
/// bindings leave out because nothing else uses it.
const NIN_KEYSELECT: u32 = 0x0401;

/// One press can arrive as two notifications — `NIN_SELECT` beside `WM_LBUTTONUP`,
/// `WM_CONTEXTMENU` beside `WM_RBUTTONUP` — so a second word for the same press within this
/// window is the same click. It is well under the shortest double-click time Windows allows.
const REPEAT: Duration = Duration::from_millis(250);

/// What the tray icons reported, for the loop to drain each turn.
pub(crate) type TrayQueue = Rc<RefCell<Vec<(TrayId, TrayEvent)>>>;

/// An icon in the notification area.
pub(crate) struct Win32TrayIcon {
    window: TrayWindow,
}

/// Something the shell can address: a hidden window and the state behind it.
struct TrayWindow {
    hwnd: HWND,
    state: Rc<RefCell<TrayState>>,
}

/// What the window procedure and the handle both need.
struct TrayState {
    id: TrayId,
    events: TrayQueue,
    /// The menu a right-click opens, kept as the app gave it: the native menu is built per
    /// opening, which is also what makes a `set_menu` between two openings take effect.
    menu: Option<Menu>,
    /// The picture the shell draws, and what it needs again after Explorer restarts.
    icon: Option<HICON>,
    tooltip: String,
    visible: bool,
    /// The last primary and secondary notification reported, for [`REPEAT`].
    last_primary: Option<(u32, Instant)>,
    last_secondary: Option<(u32, Instant)>,
}

impl Drop for TrayState {
    fn drop(&mut self) {
        // The picture goes with the state, including when creation gave up halfway.
        if let Some(picture) = self.icon.take() {
            let _ = unsafe { DestroyIcon(picture) };
        }
    }
}

impl Win32TrayIcon {
    pub(crate) fn create(id: TrayId, icon: Icon, events: TrayQueue) -> Result<Self, String> {
        let picture = create_icon(&icon)?;
        let window = match TrayWindow::open(id, events) {
            Ok(window) => window,
            Err(error) => {
                let _ = unsafe { DestroyIcon(picture) };
                return Err(error);
            }
        };
        {
            let mut state = window.state.borrow_mut();
            state.icon = Some(picture);
            state.visible = true;
            if let Err(error) = add_icon(window.hwnd, &mut state) {
                state.visible = false;
                return Err(error);
            }
        }
        Ok(Self { window })
    }

    fn state(&self) -> &RefCell<TrayState> {
        &self.window.state
    }
}

impl TrayHandle for Win32TrayIcon {
    fn set_icon(&mut self, icon: Icon) -> Result<(), String> {
        let picture = create_icon(&icon)?;
        let mut state = self.state().borrow_mut();
        if state.visible {
            let mut data = icon_data(self.window.hwnd);
            data.uFlags = NIF_ICON;
            data.hIcon = picture;
            if !unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.as_bool() {
                let _ = unsafe { DestroyIcon(picture) };
                return Err("the notification area refused the new picture".into());
            }
        }
        if let Some(old) = state.icon.replace(picture) {
            let _ = unsafe { DestroyIcon(old) };
        }
        Ok(())
    }

    fn set_tooltip(&mut self, tooltip: &str) {
        let mut state = self.state().borrow_mut();
        state.tooltip = tooltip.to_string();
        if !state.visible {
            return;
        }
        let mut data = icon_data(self.window.hwnd);
        data.uFlags = NIF_TIP | NIF_SHOWTIP;
        fill_tip(&mut data, &state.tooltip);
        // A tooltip the shell drops leaves the old one showing; there is nothing else to do
        // from here, and saying so would change nothing the app could act on.
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }

    fn set_menu(&mut self, menu: Option<Menu>) {
        self.state().borrow_mut().menu = menu;
    }

    fn set_visible(&mut self, visible: bool) {
        let mut state = self.state().borrow_mut();
        if state.visible == visible {
            return;
        }
        if visible {
            // A refused icon stays hidden; the next `set_visible(true)` asks again.
            let added = add_icon(self.window.hwnd, &mut state).is_ok();
            state.visible = added;
        } else {
            let data = icon_data(self.window.hwnd);
            // The shell may already have forgotten the icon (an Explorer restart); its
            // refusal leaves nothing on screen either way.
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
            state.visible = false;
        }
    }
}

impl Drop for Win32TrayIcon {
    fn drop(&mut self) {
        let mut state = self.window.state.borrow_mut();
        if state.visible {
            let data = icon_data(self.window.hwnd);
            // The process is taking the icon away, and the shell refusing has the same result.
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
            state.visible = false;
        }
    }
}

impl TrayWindow {
    fn open(id: TrayId, events: TrayQueue) -> Result<Self, String> {
        // The anchor point arrives in physical pixels, and the rest of the engine works in
        // logical units. A daemon has no surface to set the process's awareness for it.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|error| format!("GetModuleHandleW: {error}"))?;
        let instance = HINSTANCE(instance.0);
        let class = wide(TRAY_CLASS);
        register_class(&class, instance)?;
        let state = Rc::new(RefCell::new(TrayState {
            id,
            events,
            menu: None,
            icon: None,
            tooltip: String::new(),
            visible: false,
            last_primary: None,
            last_secondary: None,
        }));
        // The count the window keeps is released in `WM_NCDESTROY`, which runs even when a
        // step after creation fails.
        let raw = Rc::as_ptr(&state);
        unsafe { Rc::increment_strong_count(raw) };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR::from_raw(class.as_ptr()),
                PCWSTR::from_raw(class.as_ptr()),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                Some(raw as *const c_void),
            )
        };
        let hwnd = match hwnd {
            Ok(hwnd) => hwnd,
            Err(error) => {
                // No window exists to end, so the count this function added is still owed.
                drop(unsafe { Rc::from_raw(raw) });
                return Err(format!("CreateWindowExW: {error}"));
            }
        };
        Ok(Self { hwnd, state })
    }
}

impl Drop for TrayWindow {
    fn drop(&mut self) {
        // Destroying the window releases the count `open` added, in `WM_NCDESTROY`.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

/// The icon's data as the shell reads it, with nothing flagged.
fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        ..Default::default()
    }
}

/// Add the icon to the notification area, or put it back after Explorer restarted.
fn add_icon(hwnd: HWND, state: &mut TrayState) -> Result<(), String> {
    let Some(picture) = state.icon else {
        return Err("a tray icon needs a picture".into());
    };
    let mut data = icon_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    data.uCallbackMessage = TRAY_MESSAGE;
    data.hIcon = picture;
    fill_tip(&mut data, &state.tooltip);
    if !unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
        return Err("the notification area refused the icon".into());
    }
    // Version 4 puts the anchor point in the callback's `wParam` and sends `WM_CONTEXTMENU`
    // for a menu request; the older layout is not worth a second code path nobody runs.
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    if !unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) }.as_bool() {
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        return Err(
            "the notification area does not support the icon protocol this backend speaks".into(),
        );
    }
    Ok(())
}

/// Copy as much of `tooltip` as `szTip` holds, always leaving the terminator room.
fn fill_tip(data: &mut NOTIFYICONDATAW, tooltip: &str) {
    let mut units = tooltip.encode_utf16();
    let limit = data.szTip.len() - 1;
    for slot in data.szTip.iter_mut().take(limit) {
        *slot = units.next().unwrap_or(0);
    }
}

fn register_class(class: &[u16], instance: HINSTANCE) -> Result<(), String> {
    let descriptor = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(tray_proc),
        hInstance: instance,
        lpszClassName: PCWSTR::from_raw(class.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&descriptor) } == 0 {
        // A second icon registers the class the first one registered, which is not a failure.
        let error = unsafe { GetLastError() };
        if error != ERROR_CLASS_ALREADY_EXISTS {
            return Err(format!("RegisterClassExW failed: {error:?}"));
        }
    }
    Ok(())
}

unsafe extern "system" fn tray_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // `WM_NCCREATE` hands the state over; every other message reads it.
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<TrayState>;
    if raw.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    // SAFETY: the pointer is this window's state, set in `WM_NCCREATE` and cleared in
    // `WM_NCDESTROY`; every message for a window arrives on the thread that created it.
    let state = unsafe { &*raw };
    if message == WM_NCDESTROY {
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        // The count `TrayWindow::open` added for the window is released with the window.
        drop(unsafe { Rc::from_raw(raw) });
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let taskbar = taskbar_created();
    if taskbar != 0 && message == taskbar {
        re_register(state, hwnd);
        return LRESULT(0);
    }
    if message == TRAY_MESSAGE {
        handle_notification(state, hwnd, wparam, lparam);
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// The message Explorer broadcasts when it starts: it has forgotten every icon, so every app
/// puts its own back.
fn taskbar_created() -> u32 {
    use std::sync::OnceLock;
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| {
        let name = wide("TaskbarCreated");
        unsafe { RegisterWindowMessageW(PCWSTR::from_raw(name.as_ptr())) }
    })
}

fn re_register(state: &RefCell<TrayState>, hwnd: HWND) {
    let mut state = state.borrow_mut();
    if !state.visible {
        return;
    }
    // The shell that refused the icon has just restarted; there is no second answer to give.
    let _ = add_icon(hwnd, &mut state);
}

fn handle_notification(state: &RefCell<TrayState>, hwnd: HWND, wparam: WPARAM, lparam: LPARAM) {
    // Version 4: `LOWORD(lParam)` is the notification and `wParam` holds the anchor point;
    // `HIWORD(lParam)` is the icon id, which one window per icon makes uninteresting.
    let notification = (lparam.0 as u32) & 0xFFFF;
    // The shell also reports the pointer moving over the icon, which is not an event the
    // app has a word for; the point is only worked out for the messages that are.
    if !matches!(
        notification,
        WM_LBUTTONUP
            | NIN_SELECT
            | NIN_KEYSELECT
            | WM_LBUTTONDBLCLK
            | WM_MBUTTONUP
            | WM_CONTEXTMENU
            | WM_RBUTTONUP
    ) {
        return;
    }
    let at = anchor(wparam);
    match notification {
        WM_LBUTTONUP | NIN_SELECT | NIN_KEYSELECT => {
            if is_fresh(state, notification, PointerButton::Primary) {
                report(
                    state,
                    TrayEvent::Click {
                        button: PointerButton::Primary,
                        at,
                    },
                );
            }
        }
        WM_LBUTTONDBLCLK => report(state, TrayEvent::DoubleClick { at }),
        WM_MBUTTONUP => report(
            state,
            TrayEvent::Click {
                button: PointerButton::Middle,
                at,
            },
        ),
        WM_CONTEXTMENU | WM_RBUTTONUP => {
            if is_fresh(state, notification, PointerButton::Secondary) {
                report(
                    state,
                    TrayEvent::Click {
                        button: PointerButton::Secondary,
                        at,
                    },
                );
            }
            open_menu(state, hwnd);
        }
        _ => {}
    }
}

/// Whether this notification is a click of its own rather than the shell's second word for
/// the one just reported.
fn is_fresh(state: &RefCell<TrayState>, notification: u32, button: PointerButton) -> bool {
    let now = Instant::now();
    let mut state = state.borrow_mut();
    let slot = match button {
        PointerButton::Primary => &mut state.last_primary,
        PointerButton::Secondary => &mut state.last_secondary,
        // A middle click has one spelling on every shell that sends it.
        PointerButton::Middle => return true,
    };
    let fresh = !matches!(
        *slot,
        Some((previous, at)) if previous != notification && now.duration_since(at) < REPEAT
    );
    *slot = Some((notification, now));
    fresh
}

fn report(state: &RefCell<TrayState>, event: TrayEvent) {
    let state = state.borrow();
    state.events.borrow_mut().push((state.id, event));
}

/// Show the icon's menu at the pointer, and report the row chosen.
fn open_menu(state: &RefCell<TrayState>, hwnd: HWND) {
    let menu = state.borrow().menu.clone();
    let Some(menu) = menu else {
        return;
    };
    // A menu the shell cannot build cannot be shown; the click that asked for it has already
    // been reported, and there is no other door to the app from inside a window procedure.
    let Ok(native) = NativeMenu::build(&menu) else {
        return;
    };
    // The menu belongs to the window that shows it, and it is dismissed when another window
    // takes the foreground: the caller's own window has to be in front for that to happen.
    // The shell may refuse a hidden window the foreground, and the menu still opens.
    let _ = unsafe { SetForegroundWindow(hwnd) };
    let at = cursor();
    let chosen = unsafe {
        TrackPopupMenu(
            native.handle,
            TPM_RETURNCMD | TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        )
    };
    // Without a message to answer, the shell keeps this window in front until the next one,
    // and the next click elsewhere would go to the menu instead of dismissing it.
    let _ = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };
    match native.chosen(chosen.0 as u16) {
        Some(row) => {
            let state = state.borrow();
            state
                .events
                .borrow_mut()
                .push((state.id, TrayEvent::Menu { id: row }));
        }
        None => {
            // Dismissed rather than chosen: the keyboard focus belongs back in the area the
            // menu took it from.
            let data = icon_data(hwnd);
            let _ = unsafe { Shell_NotifyIconW(NIM_SETFOCUS, &data) };
        }
    }
}

/// The pointer in physical pixels, which is where a tray menu opens.
fn cursor() -> POINT {
    let mut at = POINT::default();
    // A pointer that cannot be read opens the menu at the corner, which is still dismissable.
    let _ = unsafe { GetCursorPos(&mut at) };
    at
}

/// The anchor the shell sent, in the output coordinates the rest of the engine uses.
fn anchor(wparam: WPARAM) -> Point {
    // Both halves are signed: a monitor left of or above the first one is at a negative
    // coordinate.
    let packed = wparam.0 as u32;
    let x = (packed & 0xFFFF) as u16 as i16 as i32;
    let y = ((packed >> 16) & 0xFFFF) as u16 as i16 as i32;
    let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    let scale =
        match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) } {
            Ok(()) if dpi_x > 0 => dpi_x as f32 / 96.0,
            // A monitor that reports no scale is at the system's own.
            _ => 1.0,
        };
    Point::new(x as f32 / scale, y as f32 / scale)
}

/// The picture the shell draws, from the app's straight RGBA pixels.
fn create_icon(icon: &Icon) -> Result<HICON, String> {
    let width = i32::try_from(icon.width).map_err(|_| "a tray icon is wider than Win32 allows")?;
    let height =
        i32::try_from(icon.height).map_err(|_| "a tray icon is taller than Win32 allows")?;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // A negative height is top-down, which is the order the app's rows are in.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let colour = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
        .map_err(|error| format!("CreateDIBSection: {error}"))?;
    if bits.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ(colour.0)) };
        return Err("the icon's pixels could not be allocated".into());
    }
    // Windows reads an icon's colour bitmap as BGRA. The alpha stays as the app wrote it:
    // the destination is blended with it, not with an already-multiplied colour.
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, icon.rgba.len()) };
    let sources = icon.rgba.as_chunks::<4>().0;
    for (source, target) in sources.iter().zip(pixels.as_chunks_mut::<4>().0) {
        target[0] = source[2];
        target[1] = source[1];
        target[2] = source[0];
        target[3] = source[3];
    }
    // A one-bit mask per pixel row, padded to the 16-bit boundary Win32 wants. Pixels the app
    // made transparent are the ones the mask hides, for paths that read the mask and not the
    // alpha channel.
    let stride = (icon.width as usize).div_ceil(16) * 2;
    let mut mask = vec![0u8; stride * icon.height as usize];
    for row in 0..icon.height as usize {
        for column in 0..icon.width as usize {
            let alpha = icon.rgba[(row * icon.width as usize + column) * 4 + 3];
            if alpha == 0 {
                mask[row * stride + column / 8] |= 0x80 >> (column % 8);
            }
        }
    }
    let mask_bitmap =
        unsafe { CreateBitmap(width, height, 1, 1, Some(mask.as_ptr() as *const c_void)) };
    if mask_bitmap.is_invalid() {
        let _ = unsafe { DeleteObject(HGDIOBJ(colour.0)) };
        return Err("the icon's mask could not be created".into());
    }
    let icon_info = ICONINFO {
        fIcon: BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bitmap,
        hbmColor: colour,
    };
    let picture = unsafe { CreateIconIndirect(&icon_info) };
    let _ = unsafe { DeleteObject(HGDIOBJ(mask_bitmap.0)) };
    let _ = unsafe { DeleteObject(HGDIOBJ(colour.0)) };
    picture.map_err(|error| format!("CreateIconIndirect: {error}"))
}

/// A native menu built from the app's model, and the row each command id came from.
///
/// Win32 names menu commands with 16 bits, and an [`Id`] is a hash: the ids the shell
/// reports are this build's own numbering, mapped back to the app's here.
struct NativeMenu {
    handle: HMENU,
    rows: Vec<(u16, Id)>,
}

impl NativeMenu {
    fn build(menu: &Menu) -> Result<Self, String> {
        let handle =
            unsafe { CreatePopupMenu() }.map_err(|error| format!("CreatePopupMenu: {error}"))?;
        let mut rows = Vec::new();
        if let Err(error) = fill(handle, &menu.entries, &mut rows) {
            let _ = unsafe { DestroyMenu(handle) };
            return Err(error);
        }
        Ok(Self { handle, rows })
    }

    /// The row the shell reports as chosen; `None` when the menu was dismissed.
    fn chosen(&self, command: u16) -> Option<Id> {
        self.rows
            .iter()
            .find(|(id, _)| *id == command)
            .map(|(_, row)| *row)
    }
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        // A menu owns its submenus; destroying the root frees the whole tree.
        let _ = unsafe { DestroyMenu(self.handle) };
    }
}

fn fill(parent: HMENU, entries: &[MenuEntry], rows: &mut Vec<(u16, Id)>) -> Result<(), String> {
    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                unsafe { AppendMenuW(parent, MF_SEPARATOR, 0, PCWSTR::null()) }
                    .map_err(|error| format!("AppendMenuW: {error}"))?;
            }
            MenuEntry::Item(item) => {
                let text = wide(&row_text(item));
                unsafe {
                    AppendMenuW(
                        parent,
                        row_flags(item),
                        command(rows, item.id)? as usize,
                        PCWSTR::from_raw(text.as_ptr()),
                    )
                }
                .map_err(|error| format!("AppendMenuW: {error}"))?;
            }
            MenuEntry::Check { item, checked } => {
                let flags = if *checked {
                    row_flags(item) | MF_CHECKED
                } else {
                    row_flags(item)
                };
                let text = wide(&row_text(item));
                unsafe {
                    AppendMenuW(
                        parent,
                        flags,
                        command(rows, item.id)? as usize,
                        PCWSTR::from_raw(text.as_ptr()),
                    )
                }
                .map_err(|error| format!("AppendMenuW: {error}"))?;
            }
            MenuEntry::Submenu {
                label,
                enabled,
                entries,
            } => {
                // A submenu row reports nothing itself: the rows inside it carry the ids.
                let submenu = unsafe { CreatePopupMenu() }
                    .map_err(|error| format!("CreatePopupMenu: {error}"))?;
                if let Err(error) = fill(submenu, entries, rows) {
                    let _ = unsafe { DestroyMenu(submenu) };
                    return Err(error);
                }
                let mut flags = MF_POPUP | MF_STRING;
                if !enabled {
                    flags |= MF_GRAYED;
                }
                let text = wide(label);
                if let Err(error) = unsafe {
                    AppendMenuW(
                        parent,
                        flags,
                        submenu.0 as usize,
                        PCWSTR::from_raw(text.as_ptr()),
                    )
                } {
                    let _ = unsafe { DestroyMenu(submenu) };
                    return Err(format!("AppendMenuW: {error}"));
                }
            }
        }
    }
    Ok(())
}

/// The command id the shell reports for the next row: a count from one, which is what marks a
/// command as this menu's rather than a system one.
fn command(rows: &mut Vec<(u16, Id)>, id: Id) -> Result<u16, String> {
    let next = rows
        .len()
        .checked_add(1)
        .and_then(|next| u16::try_from(next).ok())
        .ok_or("a tray menu cannot hold more rows than Win32 has command ids")?;
    rows.push((next, id));
    Ok(next)
}

fn row_flags(item: &MenuItem) -> MENU_ITEM_FLAGS {
    if item.enabled {
        MF_STRING
    } else {
        MF_STRING | MF_GRAYED
    }
}

/// What the row says: its label, and the chord the desktop right-aligns beside it.
fn row_text(item: &MenuItem) -> String {
    match &item.shortcut {
        Some(shortcut) => format!("{}\t{shortcut}", item.label),
        None => item.label.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_platform::tray::Icon;

    fn icon(rgba: Vec<u8>, width: u32, height: u32) -> Icon {
        Icon::from_rgba(rgba, width, height).expect("a valid icon")
    }

    /// The pixels the shell draws for `picture`, over an opaque background.
    fn draw(picture: HICON, width: u32, height: u32, background: [u8; 4]) -> Vec<[u8; 4]> {
        use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, GdiFlush, SelectObject};
        use windows::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx};
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let target = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
            .expect("a destination bitmap");
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize)
        };
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[0] = background[2];
            pixel[1] = background[1];
            pixel[2] = background[0];
            pixel[3] = background[3];
        }
        let dc = unsafe { CreateCompatibleDC(None) };
        let previous = unsafe { SelectObject(dc, HGDIOBJ(target.0)) };
        unsafe {
            DrawIconEx(
                dc,
                0,
                0,
                picture,
                width as i32,
                height as i32,
                0,
                None,
                DI_NORMAL,
            )
        }
        .expect("the icon draws");
        let _ = unsafe { GdiFlush() };
        let drawn = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
            .collect();
        unsafe {
            SelectObject(dc, previous);
            let _ = DeleteDC(dc);
            let _ = DeleteObject(HGDIOBJ(target.0));
        }
        drawn
    }

    #[test]
    fn a_semi_transparent_icon_blends_the_way_the_app_drew_it() {
        // Half-alpha red over an opaque blue background: the drawn pixel is the app's own
        // source-over composite, which only holds if the icon's colour is not premultiplied.
        let picture = icon(vec![255, 0, 0, 128], 1, 1);
        let handle = create_icon(&picture).expect("an icon");
        let drawn = draw(handle, 1, 1, [0, 0, 255, 255]);
        let _ = unsafe { DestroyIcon(handle) };
        let [red, green, blue, alpha] = drawn[0];
        assert_eq!(alpha, 255, "the background is opaque");
        assert!(
            (127..=129).contains(&red),
            "half of the red survives: {drawn:?}"
        );
        assert_eq!(green, 0);
        assert!(
            (126..=128).contains(&blue),
            "the other half is the background: {drawn:?}"
        );
    }

    #[test]
    fn a_menu_reports_the_row_that_was_chosen() {
        let open = Id::str("open");
        let save = Id::str("save-as");
        let grid = Id::str("show-grid");
        let menu = Menu::new(vec![
            MenuEntry::item(open, "Open"),
            MenuEntry::Separator,
            MenuEntry::submenu(
                "Export",
                vec![MenuEntry::item(save, "Save as…"), MenuEntry::separator()],
            ),
            MenuEntry::check(grid, "Show grid", true),
        ]);
        let native = NativeMenu::build(&menu).expect("a native menu");
        assert_eq!(native.chosen(1), Some(open), "the first row is command one");
        assert_eq!(native.chosen(2), Some(save), "the submenu's row is next");
        assert_eq!(native.chosen(3), Some(grid));
        assert_eq!(
            native.chosen(4),
            None,
            "a command no row was given names nothing"
        );
        assert_eq!(native.chosen(0), None, "a dismissed menu chooses nothing");
    }

    #[test]
    fn one_press_is_one_click_however_the_shell_spells_it() {
        let queue: TrayQueue = Rc::new(RefCell::new(Vec::new()));
        let window =
            TrayWindow::open(TrayId::from_u64(3), Rc::clone(&queue)).expect("a hidden window");
        let post = |notification: u32, at: (i32, i32)| {
            let packed = ((at.0 as u32 & 0xFFFF) | ((at.1 as u32 & 0xFFFF) << 16)) as usize;
            unsafe {
                PostMessageW(
                    Some(window.hwnd),
                    TRAY_MESSAGE,
                    WPARAM(packed),
                    LPARAM(notification as isize),
                )
            }
            .expect("a posted message");
        };
        post(WM_LBUTTONUP, (400, 300));
        post(NIN_SELECT, (400, 300));
        post(WM_LBUTTONDBLCLK, (400, 300));
        post(WM_MBUTTONUP, (400, 300));
        crate::window::pump_messages();
        let events = queue.borrow().clone();
        assert_eq!(
            events.len(),
            3,
            "the second spelling of the left press is the same press: {events:?}"
        );
        assert_eq!(events[0].0, TrayId::from_u64(3));
        match events[0].1 {
            TrayEvent::Click {
                button: PointerButton::Primary,
                at,
            } => assert!(
                at.x > 0.0 && at.y > 0.0,
                "the anchor comes from the message: {at:?}"
            ),
            other => panic!("the first event is the left press: {other:?}"),
        }
        assert!(matches!(events[1].1, TrayEvent::DoubleClick { .. }));
        assert!(matches!(
            events[2].1,
            TrayEvent::Click {
                button: PointerButton::Middle,
                ..
            }
        ));
        drop(window);
    }
}
