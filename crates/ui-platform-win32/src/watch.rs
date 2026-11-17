//! A directory watch on Windows: one thread per directory over `ReadDirectoryChangesW`.
//!
//! A burst — copying a folder is hundreds of notifications — is allowed to go quiet before
//! the picker is told, so one copy is one listing read instead of one per file.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use ui_platform::watch::{DirectoryWatcher, WatchGuard};
use windows::Win32::Foundation::{CloseHandle, ERROR_IO_PENDING, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    ReadDirectoryChangesW,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{
    CreateEventW, INFINITE, ResetEvent, SetEvent, WaitForMultipleObjects,
};
use windows::core::{Error, PCWSTR};

/// How long the notifications must be quiet before the change is reported.
const QUIET_MS: u32 = 250;

/// The Windows watcher: every `watch` starts a thread of its own.
pub struct Win32Watcher;

impl DirectoryWatcher for Win32Watcher {
    fn watch(
        &self,
        path: &Path,
        changed: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Box<dyn WatchGuard>, String> {
        let target = path.to_path_buf();
        // SAFETY: the handle is this watch's own stop event, created and closed here.
        let stop = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
            .map_err(|error| format!("watch event: {error}"))?;
        let stop = SharedHandle(stop);
        let signal = SharedHandle(stop.0);
        let worker = thread::Builder::new()
            .name("directory-watch".into())
            .spawn(move || run(&target, signal, changed));
        match worker {
            Ok(worker) => Ok(Box::new(Win32Watch {
                stop,
                worker: Some(worker),
            })),
            Err(error) => {
                // SAFETY: no thread holds the event; this watch is not running.
                let _ = unsafe { CloseHandle(stop.0) };
                Err(format!("watch thread: {error}"))
            }
        }
    }
}

struct Win32Watch {
    stop: SharedHandle,
    worker: Option<JoinHandle<()>>,
}

impl WatchGuard for Win32Watch {}

impl Drop for Win32Watch {
    fn drop(&mut self) {
        // SAFETY: the stop event belongs to this watch; the thread is waiting on it.
        let _ = unsafe { SetEvent(self.stop.0) };
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // SAFETY: the thread has returned, so nothing waits on the event any more.
        let _ = unsafe { CloseHandle(self.stop.0) };
    }
}

/// A handle is usable from any thread; only the owner closes it.
struct SharedHandle(HANDLE);

// SAFETY: Win32 handles are process-wide values, and every use here is a wait or a set.
unsafe impl Send for SharedHandle {}
unsafe impl Sync for SharedHandle {}

fn run(path: &Path, stop: SharedHandle, changed: Arc<dyn Fn() + Send + Sync>) {
    let mut wide: Vec<u16> = {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().collect()
    };
    wide.push(0);
    // SAFETY: `wide` is a terminated path that outlives the call.
    let directory = match unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            None,
        )
    } {
        Ok(handle) => handle,
        Err(error) => {
            log::warn!("ui-platform-win32: cannot watch {}: {error}", path.display());
            return;
        }
    };
    // SAFETY: the event is created for this thread's own overlapped reads.
    let io = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
        Ok(event) => event,
        Err(error) => {
            log::warn!("ui-platform-win32: no watch event: {error}");
            // SAFETY: the directory handle is this thread's own.
            let _ = unsafe { CloseHandle(directory) };
            return;
        }
    };
    let mut buffer = vec![0u8; 4096];
    loop {
        let mut overlapped = OVERLAPPED {
            hEvent: io,
            ..Default::default()
        };
        // SAFETY: the event belongs to this thread; the read re-arms it.
        let _ = unsafe { ResetEvent(io) };
        if let Err(error) = start_read(directory, &mut buffer, &mut overlapped) {
            log::warn!("ui-platform-win32: watch read failed: {error}");
            break;
        }
        // SAFETY: both handles are live for the whole wait.
        let ready = unsafe { WaitForMultipleObjects(&[io, stop.0], false, INFINITE) };
        if ready != WAIT_OBJECT_0 {
            // The stop event, or a failed wait: either way this watch is over.
            cancel(directory, &overlapped);
            break;
        }
        let mut transferred = 0u32;
        // SAFETY: the read completed on this overlapped structure.
        if unsafe { GetOverlappedResult(directory, &overlapped, &mut transferred, false) }.is_err()
        {
            // The watched directory was deleted or renamed; the next listing will say so.
            changed();
            break;
        }
        if !drain(directory, io, stop.0, &mut buffer) {
            break;
        }
        changed();
    }
    // SAFETY: no read is pending and this thread owns both handles.
    unsafe {
        let _ = CloseHandle(io);
        let _ = CloseHandle(directory);
    }
}

fn start_read(
    directory: HANDLE,
    buffer: &mut [u8],
    overlapped: &mut OVERLAPPED,
) -> Result<(), Error> {
    // SAFETY: the buffer and the overlapped structure outlive the operation, which the wait
    // below completes.
    let result = unsafe {
        ReadDirectoryChangesW(
            directory,
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len() as u32,
            false,
            FILE_NOTIFY_CHANGE_FILE_NAME
                | FILE_NOTIFY_CHANGE_DIR_NAME
                | FILE_NOTIFY_CHANGE_LAST_WRITE
                | FILE_NOTIFY_CHANGE_SIZE,
            None,
            Some(overlapped),
            None,
        )
    };
    match result {
        Ok(()) => Ok(()),
        // The normal answer to an overlapped read: it is pending until the event fires.
        Err(error) if error.code().0 as u32 & 0xFFFF == ERROR_IO_PENDING.0 => Ok(()),
        Err(error) => Err(error),
    }
}

/// Keep re-arming reads until the directory has been quiet for the window; `false` when the
/// stop event arrived while the burst was still going.
fn drain(directory: HANDLE, io: HANDLE, stop: HANDLE, buffer: &mut Vec<u8>) -> bool {
    loop {
        let mut overlapped = OVERLAPPED {
            hEvent: io,
            ..Default::default()
        };
        // SAFETY: the event is this thread's own.
        let _ = unsafe { ResetEvent(io) };
        if start_read(directory, buffer, &mut overlapped).is_err() {
            return true;
        }
        // SAFETY: both handles stay live for the wait.
        let ready = unsafe { WaitForMultipleObjects(&[io, stop], false, QUIET_MS) };
        if ready == WAIT_TIMEOUT {
            cancel(directory, &overlapped);
            return true;
        }
        if ready != WAIT_OBJECT_0 {
            cancel(directory, &overlapped);
            return false;
        }
        let mut transferred = 0u32;
        // SAFETY: the read completed on this overlapped structure.
        if unsafe { GetOverlappedResult(directory, &overlapped, &mut transferred, false) }.is_err()
        {
            return true;
        }
    }
}

fn cancel(directory: HANDLE, overlapped: &OVERLAPPED) {
    // SAFETY: cancelling this thread's own pending read. The wait below keeps the overlapped
    // structure alive until the kernel is done with it; a read that already completed answers
    // at once.
    let _ = unsafe { CancelIoEx(directory, Some(overlapped)) };
    let mut transferred = 0u32;
    let _ = unsafe { GetOverlappedResult(directory, overlapped, &mut transferred, true) };
}
