//! The classic shell context menu: build it from one folder's items, read its rows, run it.
//!
//! The shell builds the menu from `IContextMenu`, so the rows can be re-hosted in the
//! picker's own menu. A row the shell draws itself has no text to carry: it is left out and
//! counted, and the native menu the caller keeps beside the merge still shows it. Reads and
//! commands run on a worker thread: the shell calls third-party in-process code that may block.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem::{ManuallyDrop, size_of};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use ui_platform::shell_menu::{ShellMenuHost, ShellMenuRead, ShellMenuRow};

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RPC_E_CHANGED_MODE, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_ITEMMENU, CMF_NORMAL, CMF_SYNCCASCADEMENU, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    DEFCONTEXTMENU, IContextMenu, IShellFolder, SHBindToParent, SHCreateDefaultContextMenu,
    SHParseDisplayName,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetMenuItemInfoW, HBMMENU_CALLBACK, HMENU,
    IsWindow, MENUITEMINFOW, MFS_CHECKED, MFS_DISABLED, MFS_GRAYED, MFT_OWNERDRAW, MFT_SEPARATOR,
    MIIM_BITMAP, MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING, MIIM_SUBMENU, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PostMessageW, PostThreadMessageW, QS_ALLINPUT, SW_SHOWNORMAL,
    SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_APP, WM_NULL,
};
use windows::core::{PCSTR, PCWSTR, PWSTR};

/// The first command id the shell is given. Its offsets are what `invoke` takes.
const ID_FIRST: u32 = 1;
/// The shell reserves command ids in a 16-bit word; extension ids stay below this.
const ID_LAST: u32 = 0x7FFF;
/// Every shell row label fits here; a longer one is treated as unreadable.
const LABEL_UNITS: usize = 512;
/// Shell menus are shallow; a deeper cascade is treated as unreadable rather than walked.
const MAX_SUBMENU_DEPTH: u32 = 8;
/// Reading wants lazy cascades populated before the walk; hosting leaves them lazy.
const READ_FLAGS: u32 = CMF_NORMAL | CMF_ITEMMENU | CMF_SYNCCASCADEMENU;
const HOST_FLAGS: u32 = CMF_NORMAL | CMF_ITEMMENU;

/// The window a menu belongs to, shared by every surface the backend opens.
pub(crate) type MenuOwner = Arc<AtomicPtr<c_void>>;

/// Wakes the worker's wait; a thread message, because the wait also owes the apartment its
/// message pump and a condvar would block without one.
const WORK_MESSAGE: u32 = WM_APP + 0x53;

/// The classic shell context menu of one folder's selection.
#[derive(Clone, Debug)]
pub struct Win32ShellMenu {
    /// The window the client point and the popup are placed in.
    owner: MenuOwner,
}

impl Win32ShellMenu {
    /// A menu tied to one window, for a session that drives a single surface.
    pub(crate) fn for_window(window: HWND) -> Self {
        Self {
            owner: Arc::new(AtomicPtr::new(window.0)),
        }
    }

    /// A menu that follows the window the backend opened last, dialogs included.
    pub(crate) fn following(owner: MenuOwner) -> Self {
        Self { owner }
    }

    /// The current owner, when the window still exists.
    fn owner_window(&self) -> Option<HWND> {
        let raw = self.owner.load(Ordering::Relaxed);
        if raw.is_null() {
            return None;
        }
        // A closed dialog leaves its handle behind; a reused handle would place the menu in
        // a window this menu does not belong to.
        unsafe { IsWindow(Some(HWND(raw))) }
            .as_bool()
            .then(|| HWND(raw))
    }
}

/// A slot for the owner window, filled as the host opens surfaces.
pub(crate) fn empty_owner() -> MenuOwner {
    Arc::new(AtomicPtr::new(std::ptr::null_mut()))
}

pub(crate) fn set_owner(owner: &MenuOwner, window: HWND) {
    owner.store(window.0, Ordering::Relaxed);
}

/// Clear the slot only when it still names `window`, so a later surface keeps its entry.
pub(crate) fn clear_owner(owner: &MenuOwner, window: HWND) {
    let _ = owner.compare_exchange(
        window.0,
        std::ptr::null_mut(),
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
}

impl ShellMenuHost for Win32ShellMenu {
    fn available(&self, paths: &[PathBuf]) -> bool {
        folder_key(paths).is_some()
    }

    fn read(&self, paths: Vec<PathBuf>) -> u64 {
        let ticket = worker().tickets.fetch_add(1, Ordering::Relaxed) + 1;
        let owner = self.owner_window().unwrap_or_default();
        submit(Job::Read(ReadJob {
            ticket,
            owner: owner.0 as isize,
            paths,
        }));
        ticket
    }

    fn take_rows(&self, ticket: u64) -> Option<ShellMenuRead> {
        let mut answer = lock(&worker().answer);
        match answer.as_ref() {
            Some((answered, _)) if *answered == ticket => answer.take().map(|(_, rows)| rows),
            _ => None,
        }
    }

    fn invoke(&self, paths: Vec<PathBuf>, token: u32) {
        if token > ID_LAST - ID_FIRST {
            log::debug!("shell context menu: command {token} is outside the menu's range");
            return;
        }
        let Some(owner) = self.owner_window() else {
            log::debug!("shell context menu: the owner window is gone");
            return;
        };
        submit(Job::Invoke(InvokeJob {
            owner: owner.0 as isize,
            paths,
            token,
        }));
    }

    fn invocations(&self) -> u64 {
        worker().invocations.load(Ordering::Acquire)
    }

    fn show_native(&self, paths: &[PathBuf], at: (f32, f32)) -> bool {
        let Some(owner) = self.owner_window() else {
            log::debug!("shell context menu: the owner window is gone");
            return false;
        };
        let built = match build(owner, paths, HOST_FLAGS) {
            Ok(built) => built,
            Err(error) => {
                log::debug!("shell context menu: {error}");
                return false;
            }
        };
        let Some(context) = built.context.as_ref() else {
            return false;
        };
        let Some((x, y)) = screen_point(at, owner) else {
            log::debug!("shell context menu: the client point is outside the window");
            return false;
        };
        // The menu dismisses on a click elsewhere only while its owner is in front.
        let _ = unsafe { SetForegroundWindow(owner) };
        // SAFETY: the menu and its owner are live for the call, and `TrackPopupMenuEx` keeps
        // the menu alive until it returns.
        let chosen = unsafe {
            TrackPopupMenuEx(
                built.menu,
                (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
                x,
                y,
                owner,
                None,
            )
        };
        // An owner left in front until the next message would take the next click itself.
        let _ = unsafe { PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0)) };
        let Some(token) = token_from_id(chosen.0 as u32, built.first, ID_LAST) else {
            return true;
        };
        if let Err(error) = invoke_command(context, token, owner) {
            log::debug!("shell context menu: {error}");
        }
        true
    }
}

/// The menu, the object that answers for it, and the PIDLs both name.
struct BuiltMenu {
    menu: HMENU,
    /// Taken in `Drop` so the shell object goes before the items it was built from.
    context: Option<IContextMenu>,
    first: u32,
    /// Kept alive with the object, which may still name them during a command.
    _pidls: OwnedPidls,
    /// Last, so it outlives every COM object built inside it.
    ///
    /// A shell object released after `CoUninitialize` runs in a closed apartment; an
    /// extension that waits for the apartment's message loop then blocks the thread for good.
    _apartment: Apartment,
}

impl Drop for BuiltMenu {
    fn drop(&mut self) {
        drop(self.context.take());
        // SAFETY: the root owns every submenu, so one destroy frees the tree.
        let _ = unsafe { DestroyMenu(self.menu) };
    }
}

/// The absolute PIDLs the menu's child pointers point into.
///
/// `SHBindToParent` answers a child pointer inside the absolute item it was given, owned by
/// the caller's allocation rather than by a second one; freeing the child alone corrupts the
/// heap and freeing the absolute first leaves every use afterwards dangling.
struct OwnedPidls(Vec<*mut ITEMIDLIST>);

impl Drop for OwnedPidls {
    fn drop(&mut self) {
        for pidl in self.0.drain(..) {
            // SAFETY: each pointer came from `SHParseDisplayName` and is freed once.
            unsafe { CoTaskMemFree(Some(pidl.cast())) };
        }
    }
}

/// The thread shell extensions are called on, and the answers it has finished.
///
/// Building a menu runs third-party code that may block for as long as it likes, so the
/// picker's thread never waits for one. One worker keeps the apartment warm, which is what
/// keeps the extensions' handlers loaded between menus.
///
/// The thread pumps: a command that opens a window of its own — the properties sheet — is
/// an STA's UI, and it never appears until that thread dispatches the messages posting it
/// queued.
struct Worker {
    queue: Mutex<VecDeque<Job>>,
    /// The worker's thread id, for the message that wakes its wait; zero until it starts.
    thread: AtomicU32,
    /// The last finished read, with the ticket it answers.
    answer: Mutex<Option<(u64, ShellMenuRead)>>,
    /// The ticket the next read takes.
    tickets: AtomicU64,
    /// Commands that have finished.
    invocations: AtomicU64,
}

enum Job {
    Read(ReadJob),
    Invoke(InvokeJob),
}

struct ReadJob {
    ticket: u64,
    /// The raw handle, because a handle type is not `Send`; it names the same window.
    owner: isize,
    paths: Vec<PathBuf>,
}

struct InvokeJob {
    owner: isize,
    paths: Vec<PathBuf>,
    token: u32,
}

static WORKER: OnceLock<Arc<Worker>> = OnceLock::new();

fn worker() -> &'static Arc<Worker> {
    WORKER.get_or_init(|| {
        let worker = Arc::new(Worker {
            queue: Mutex::new(VecDeque::new()),
            thread: AtomicU32::new(0),
            answer: Mutex::new(None),
            tickets: AtomicU64::new(0),
            invocations: AtomicU64::new(0),
        });
        let running = Arc::clone(&worker);
        if let Err(error) = std::thread::Builder::new()
            .name("shell-menu".into())
            .spawn(move || run(running))
        {
            log::error!("shell context menu: the worker could not start: {error}");
        }
        worker
    })
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn submit(job: Job) {
    let worker = worker();
    let mut queue = lock(&worker.queue);
    // A newer read makes a queued older one pointless; dropping it keeps a burst of presses
    // from queueing a menu each.
    if matches!(job, Job::Read(_)) {
        queue.retain(|queued| !matches!(queued, Job::Read(_)));
    }
    queue.push_back(job);
    drop(queue);
    // A submit that beats the thread's start needs no wake: the loop reads the queue before
    // it waits, and a zero id is only possible until the thread has stored it.
    let thread = worker.thread.load(Ordering::Acquire);
    if thread != 0 {
        let _ = unsafe { PostThreadMessageW(thread, WORK_MESSAGE, WPARAM(0), LPARAM(0)) };
    }
}

fn run(worker: Arc<Worker>) {
    // Entered once and kept: `build` enters again for each menu and releases that count on
    // the way out, so the handlers stay loaded for as long as this thread lives.
    let _apartment = Apartment::enter();
    worker.thread.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    loop {
        // The queue comes first: that is what makes a submit before the thread's start safe.
        let job = lock(&worker.queue).pop_front();
        match job {
            Some(job) => run_job(&worker, job),
            None => {
                // An STA owes its thread a pump. The properties sheet a command opens is
                // posted here, and without this wait-with-messages it stays invisible.
                unsafe {
                    MsgWaitForMultipleObjectsEx(
                        None,
                        crate::window::bounded_millis(None),
                        QS_ALLINPUT,
                        MWMO_INPUTAVAILABLE,
                    )
                };
                crate::window::pump_messages();
            }
        }
    }
}

fn run_job(worker: &Worker, job: Job) {
    match job {
        Job::Read(job) => {
            let rows = read_menu(hwnd(job.owner), &job.paths);
            *lock(&worker.answer) = Some((job.ticket, rows));
            wake(hwnd(job.owner));
        }
        Job::Invoke(job) => {
            invoke_menu(hwnd(job.owner), &job.paths, job.token);
            worker.invocations.fetch_add(1, Ordering::Release);
            wake(hwnd(job.owner));
        }
    }
}

/// A job's raw handle back as the handle type.
fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

/// The menu's rows and how many could not be carried; empty when the menu did not build.
fn read_menu(owner: HWND, paths: &[PathBuf]) -> ShellMenuRead {
    let built = match build(owner, paths, READ_FLAGS) {
        Ok(built) => built,
        Err(error) => {
            log::debug!("shell context menu: {error}");
            return ShellMenuRead::default();
        }
    };
    let (rows, omitted) = read_rows(built.menu, built.first, 0);
    ShellMenuRead { rows, omitted }
}

fn invoke_menu(owner: HWND, paths: &[PathBuf], token: u32) {
    // The command ids must come from the same enumeration the caller read its token from.
    let built = match build(owner, paths, READ_FLAGS) {
        Ok(built) => built,
        Err(error) => {
            log::debug!("shell context menu: {error}");
            return;
        }
    };
    let Some(context) = built.context.as_ref() else {
        log::debug!("shell context menu: the menu did not build a context");
        return;
    };
    if let Err(error) = invoke_command(context, token, owner) {
        log::debug!("shell context menu: InvokeCommand({token}): {error}");
    }
}

/// Wake the surface the job was run for, so its loop builds the frame that shows the answer.
fn wake(owner: HWND) {
    if unsafe { IsWindow(Some(owner)) }.as_bool() {
        // A message that only wakes the pump carries no event, and the loop would sleep
        // again without reading what the job left; this one also owes a frame.
        let _ = unsafe { PostMessageW(Some(owner), crate::window::WAKE_MESSAGE, WPARAM(0), LPARAM(0)) };
    }
}

/// Build the shell menu for `paths`, with `flags` given to `QueryContextMenu`.
fn build(owner: HWND, paths: &[PathBuf], flags: u32) -> Result<BuiltMenu, String> {
    folder_key(paths).ok_or_else(|| String::from("the selection is empty or spans folders"))?;
    let apartment = Apartment::enter().map_err(|error| format!("CoInitializeEx: {error}"))?;
    let mut pidls = OwnedPidls(Vec::with_capacity(paths.len()));
    let mut children = Vec::with_capacity(paths.len());
    let mut folder: Option<IShellFolder> = None;
    for path in paths {
        let (parent, absolute, child) = bind_child(path)?;
        if folder.is_none() {
            folder = Some(parent);
        }
        pidls.0.push(absolute);
        children.push(child);
    }
    let folder = folder.ok_or_else(|| String::from("the selection names no folder"))?;
    let context = create_context(owner, &folder, &mut children)?;
    // SAFETY: `CreatePopupMenu` returns a menu this call owns.
    let menu = unsafe { CreatePopupMenu() }.map_err(|error| format!("CreatePopupMenu: {error}"))?;
    // SAFETY: the context menu is live and `menu` is an empty popup it can append to.
    let queried = unsafe { context.QueryContextMenu(menu, 0, ID_FIRST, ID_LAST, flags) };
    if let Err(error) = queried.ok() {
        // The menu has no readable rows; the shell refused before contributing any.
        let _ = unsafe { DestroyMenu(menu) };
        return Err(format!("QueryContextMenu: {error}"));
    }
    Ok(BuiltMenu {
        menu,
        context: Some(context),
        first: ID_FIRST,
        _pidls: pidls,
        _apartment: apartment,
    })
}

/// The parent folder and the two views of one path's item id.
///
/// The child is a pointer into `absolute`, which it does not own: the caller keeps the
/// absolute id alive for as long as the shell object may read the child.
fn bind_child(path: &Path) -> Result<(IShellFolder, *mut ITEMIDLIST, *mut ITEMIDLIST), String> {
    let name = wide(path);
    let mut absolute: *mut ITEMIDLIST = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated and the shell writes one allocated PIDL.
    unsafe {
        SHParseDisplayName(
            PCWSTR::from_raw(name.as_ptr()),
            None::<&IBindCtx>,
            &mut absolute,
            0,
            None,
        )
    }
    .map_err(|error| format!("SHParseDisplayName: {error}"))?;
    let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
    // SAFETY: `absolute` is a live PIDL; the child points into it rather than owning memory.
    let parent: IShellFolder =
        match unsafe { SHBindToParent(absolute, Some(&mut child as *mut *mut ITEMIDLIST)) } {
            Ok(parent) => parent,
            Err(error) => {
                unsafe { CoTaskMemFree(Some(absolute.cast())) };
                return Err(format!("SHBindToParent: {error}"));
            }
        };
    if child.is_null() {
        unsafe { CoTaskMemFree(Some(absolute.cast())) };
        return Err(String::from("the shell named no child item"));
    }
    Ok((parent, absolute, child))
}

/// The default context menu for one folder and its selected children.
fn create_context(
    owner: HWND,
    folder: &IShellFolder,
    pointers: &mut [*mut ITEMIDLIST],
) -> Result<IContextMenu, String> {
    let mut definition = DEFCONTEXTMENU {
        hwnd: owner,
        psf: ManuallyDrop::new(Some(folder.clone())),
        cidl: u32::try_from(pointers.len()).map_err(|_| String::from("too many items"))?,
        apidl: pointers.as_mut_ptr(),
        ..Default::default()
    };
    // SAFETY: `definition` describes the folder and the child PIDLs held by the caller, and
    // `IContextMenu` is the interface the shell's default menu implements.
    let context = unsafe { SHCreateDefaultContextMenu::<IContextMenu>(&definition) };
    // SAFETY: the call has returned; the reference this definition added is released once.
    unsafe { ManuallyDrop::drop(&mut definition.psf) };
    context.map_err(|error| format!("SHCreateDefaultContextMenu: {error}"))
}

/// Walk a menu into the portable rows, counting the rows that cannot be carried.
///
/// A row the shell draws itself has no text to draw in this menu, but the native menu beside
/// the merge still shows it, so it is left out and counted rather than refusing the walk.
fn read_rows(menu: HMENU, first: u32, depth: u32) -> (Vec<ShellMenuRow>, usize) {
    if depth > MAX_SUBMENU_DEPTH {
        return (Vec::new(), 1);
    }
    // SAFETY: `menu` is a live menu handle.
    let count = unsafe { GetMenuItemCount(Some(menu)) };
    let Ok(count) = usize::try_from(count) else {
        return (Vec::new(), 1);
    };
    let mut rows = Vec::with_capacity(count);
    let mut omitted = 0;
    for index in 0..count {
        let Ok(index) = u32::try_from(index) else {
            omitted += 1;
            continue;
        };
        match read_row(menu, index, first, depth) {
            ReadRow::Row(row, inside) => {
                rows.push(row);
                omitted += inside;
            }
            ReadRow::Omitted(hidden) => omitted += hidden,
        }
    }
    (rows, omitted)
}

/// One menu item, as far as this menu can carry it.
enum ReadRow {
    /// A row this menu can draw, and how many rows inside its cascade it could not.
    Row(ShellMenuRow, usize),
    /// The item and everything inside it that the walk could not carry.
    Omitted(usize),
}

fn read_row(menu: HMENU, index: u32, first: u32, depth: u32) -> ReadRow {
    let mut info = MENUITEMINFOW {
        cbSize: u32::try_from(size_of::<MENUITEMINFOW>()).unwrap_or(u32::MAX),
        fMask: MIIM_FTYPE | MIIM_STATE | MIIM_ID | MIIM_SUBMENU | MIIM_BITMAP,
        ..Default::default()
    };
    // SAFETY: `info` lives for the call, and `index` came from the item count.
    if unsafe { GetMenuItemInfoW(menu, index, true, &mut info) }.is_err() {
        return ReadRow::Omitted(1);
    }
    let kind = info.fType.0;
    let state = info.fState.0;
    let id = info.wID;
    let children = info.hSubMenu;
    // A handler that paints a row through `WM_DRAWITEM` has no text this menu can carry.
    if kind & MFT_OWNERDRAW.0 != 0 || info.hbmpItem.0 == HBMMENU_CALLBACK.0 {
        return ReadRow::Omitted(1);
    }
    let separator = kind & MFT_SEPARATOR.0 != 0;
    let label = if separator {
        String::new()
    } else {
        let mut text = vec![0u16; LABEL_UNITS];
        info.fMask = MIIM_STRING;
        info.dwTypeData = PWSTR(text.as_mut_ptr());
        info.cch = u32::try_from(text.len()).unwrap_or(u32::MAX);
        // SAFETY: the same live item is asked only for its text, into the buffer above.
        if unsafe { GetMenuItemInfoW(menu, index, true, &mut info) }.is_err() {
            return ReadRow::Omitted(1);
        }
        let Ok(length) = usize::try_from(info.cch) else {
            return ReadRow::Omitted(1);
        };
        display_label(&text[..length.min(text.len())])
    };
    let (submenu, inside) = if children.is_invalid() {
        (None, 0)
    } else {
        let (rows, omitted) = read_rows(children, first, depth + 1);
        // A cascade was not populated, or nothing in it could be carried: a header that
        // cannot open is not a row.
        if rows.iter().all(|row| row.separator) {
            return ReadRow::Omitted(1 + omitted);
        }
        (Some(rows), omitted)
    };
    if !separator && label.is_empty() {
        return ReadRow::Omitted(1 + inside);
    }
    let token = match separator || submenu.is_some() {
        true => 0,
        false => match token_from_id(id, first, ID_LAST) {
            Some(token) => token,
            None => return ReadRow::Omitted(1 + inside),
        },
    };
    ReadRow::Row(
        ShellMenuRow {
            label,
            token,
            enabled: state & (MFS_DISABLED.0 | MFS_GRAYED.0) == 0,
            checked: state & MFS_CHECKED.0 != 0,
            separator,
            submenu,
        },
        inside,
    )
}

/// A menu label without its mnemonic marker; `&&` is a literal ampersand.
fn display_label(units: &[u16]) -> String {
    let text = String::from_utf16_lossy(units);
    let mut label = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '&' {
            label.push(character);
        } else if characters.peek() == Some(&'&') {
            label.push('&');
            characters.next();
        }
    }
    label
}

/// The offset `invoke` takes for a command id the shell reported.
fn token_from_id(id: u32, first: u32, last: u32) -> Option<u32> {
    (first..=last).contains(&id).then(|| id - first)
}

/// Run one command on the object that built the menu.
fn invoke_command(context: &IContextMenu, token: u32, owner: HWND) -> windows::core::Result<()> {
    let info = CMINVOKECOMMANDINFOEX {
        cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        hwnd: owner,
        // A verb whose pointer value is below the 16-bit id space names a command offset.
        lpVerb: PCSTR::from_raw(token as usize as *const u8),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `CMINVOKECOMMANDINFOEX` begins with the `CMINVOKECOMMANDINFO` layout, and both
    // the object and `info` are live for the call.
    unsafe { context.InvokeCommand(&info as *const _ as *const CMINVOKECOMMANDINFO) }
}

/// `at` from the window's logical client units into physical screen pixels.
fn screen_point(at: (f32, f32), owner: HWND) -> Option<(i32, i32)> {
    let dpi = unsafe { GetDpiForWindow(owner) };
    let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
    let mut point = POINT {
        x: (at.0 * scale).round() as i32,
        y: (at.1 * scale).round() as i32,
    };
    // SAFETY: `owner` was checked to be a live window, and `point` is a local in client space.
    unsafe { ClientToScreen(owner, &mut point) }
        .as_bool()
        .then_some((point.x, point.y))
}

/// The folder every path shares, compared the way Windows names folders.
/// The folder the paths must share, or `None` when they cannot share one.
///
/// A root has no parent path to compare, and one root is still one folder — the case a
/// background press reads — so the shared-parent rule only applies where a parent exists.
fn folder_key(paths: &[PathBuf]) -> Option<String> {
    let first = paths.first()?;
    match parent_key(first) {
        Some(key) => paths
            .iter()
            .all(|path| parent_key(path).as_deref() == Some(key.as_str()))
            .then_some(key),
        None => paths.len().eq(&1).then(String::new),
    }
}

fn parent_key(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    (!parent.as_os_str().is_empty()).then(|| parent.to_string_lossy().to_lowercase())
}

/// An apartment on the calling thread, released only when this call opened it.
struct Apartment {
    uninitialize: bool,
}

impl Apartment {
    fn enter() -> windows::core::Result<Self> {
        // SAFETY: the matching release is in `drop`, which knows who opened the apartment.
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialized == RPC_E_CHANGED_MODE {
            // Another model is already on the thread; the shell may still answer, and tearing
            // that model down is not this call's to do.
            return Ok(Self {
                uninitialize: false,
            });
        }
        initialized.ok()?;
        Ok(Self { uninitialize: true })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: this call opened the apartment whose count it now balances.
            unsafe { CoUninitialize() };
        }
    }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, MENU_ITEM_FLAGS, MF_CHECKED, MF_GRAYED, MF_OWNERDRAW, MF_POPUP, MF_SEPARATOR,
        MF_STRING,
    };

    fn popup() -> HMENU {
        unsafe { CreatePopupMenu() }.expect("a popup menu")
    }

    fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) {
        let text = wide(Path::new(text));
        unsafe { AppendMenuW(menu, flags, id, PCWSTR::from_raw(text.as_ptr())) }
            .expect("a menu row");
    }

    #[test]
    fn a_menu_becomes_rows_with_offsets_and_readable_state() {
        let menu = popup();
        append(menu, MF_STRING | MF_CHECKED, ID_FIRST as usize, "&Open");
        append(
            menu,
            MF_STRING | MF_GRAYED,
            (ID_FIRST + 1) as usize,
            "Save && Exit",
        );
        append(menu, MF_SEPARATOR, 0, "");
        let nested = popup();
        append(nested, MF_STRING, (ID_FIRST + 2) as usize, "Copy");
        let parent = wide(Path::new("Send to"));
        unsafe {
            AppendMenuW(
                menu,
                MF_POPUP | MF_STRING,
                nested.0 as usize,
                PCWSTR::from_raw(parent.as_ptr()),
            )
        }
        .expect("a submenu row");

        let (rows, omitted) = read_rows(menu, ID_FIRST, 0);
        let _ = unsafe { DestroyMenu(menu) };

        assert_eq!(omitted, 0, "every row was readable");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].label, "Open", "the mnemonic marker is not drawn");
        assert_eq!(rows[0].token, 0);
        assert!(rows[0].checked);
        assert!(rows[0].enabled);
        assert_eq!(
            rows[1].label, "Save & Exit",
            "a doubled ampersand stays literal"
        );
        assert_eq!(rows[1].token, 1);
        assert!(!rows[1].enabled);
        assert!(rows[2].separator);
        assert_eq!(rows[2].token, 0);
        assert_eq!(rows[3].label, "Send to");
        assert_eq!(
            rows[3].token, 0,
            "a submenu header has no command of its own"
        );
        let nested = rows[3].submenu.as_ref().expect("a cascading menu");
        assert_eq!(nested[0].label, "Copy");
        assert_eq!(nested[0].token, 2, "offsets continue through the cascade");
    }

    #[test]
    fn an_owner_drawn_row_is_omitted_and_the_rest_still_merges() {
        let menu = popup();
        append(menu, MF_STRING, ID_FIRST as usize, "Open");
        // SAFETY: an owner-drawn row takes its item data as the third argument.
        unsafe { AppendMenuW(menu, MF_OWNERDRAW, 2, PCWSTR::null()) }.expect("a row");
        let (rows, omitted) = read_rows(menu, ID_FIRST, 0);
        let _ = unsafe { DestroyMenu(menu) };
        assert_eq!(rows.len(), 1, "the readable row is still carried");
        assert_eq!(rows[0].label, "Open");
        assert_eq!(omitted, 1, "the owner-drawn row is counted as left out");
    }

    #[test]
    fn an_empty_cascade_is_omitted() {
        let menu = popup();
        let nested = popup();
        let label = wide(Path::new("Send to"));
        unsafe {
            AppendMenuW(
                menu,
                MF_POPUP | MF_STRING,
                nested.0 as usize,
                PCWSTR::from_raw(label.as_ptr()),
            )
        }
        .expect("a submenu row");
        let (rows, omitted) = read_rows(menu, ID_FIRST, 0);
        let _ = unsafe { DestroyMenu(menu) };
        assert!(rows.is_empty(), "a header that cannot open is not a row");
        assert_eq!(omitted, 1, "the header stands for what it would have held");
    }

    #[test]
    fn a_selection_outside_one_folder_has_no_menu() {
        let menu = Win32ShellMenu::for_window(HWND::default());
        assert!(menu.available(&[
            PathBuf::from(r"C:\one\a.txt"),
            PathBuf::from(r"C:\one\b.txt"),
        ]));
        assert!(!menu.available(&[
            PathBuf::from(r"C:\one\a.txt"),
            PathBuf::from(r"C:\two\b.txt"),
        ]));
        assert!(!menu.available(&[]));
    }
}
