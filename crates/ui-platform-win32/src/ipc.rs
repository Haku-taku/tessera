//! The local channel on Windows: a message-mode named pipe, one reader thread per client.
//!
//! Message mode carries the boundaries the portable contract promises, so a read is one
//! whole message; `poll` only takes what a reader thread has already pushed. Every handle is
//! overlapped: a synchronous handle allows one operation at a time, and a reader parked in
//! `ReadFile` would block the reply the loop wants to write on the same pipe.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use ui_platform::ipc::{Channel, Reply};
use ui_platform::jobs::Waker;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, ERROR_MORE_DATA, ERROR_PIPE_CONNECTED, HANDLE,
};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_OVERLAPPED, FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent};
use windows::core::{Error, PCWSTR};

/// Largest message accepted, the same bound the Unix channel carries.
const MAX_MESSAGE: usize = 64 * 1024;

/// The loop's handle on the daemon's channel.
pub(crate) struct Win32Channel {
    incoming: Arc<Mutex<VecDeque<Incoming>>>,
}

impl Channel for Win32Channel {
    fn poll(&mut self, on_message: &mut dyn FnMut(&mut dyn Reply, &[u8])) {
        loop {
            let Some(incoming) = lock(&self.incoming).pop_front() else {
                break;
            };
            let mut reply = PipeReply {
                peer: &incoming.peer,
            };
            on_message(&mut reply, &incoming.bytes);
        }
    }
}

/// Serve the channel under `name`, which may be a bare pipe name or a full `\\.\pipe\` path.
pub(crate) fn open(name: &str, waker: Option<Arc<dyn Waker>>) -> Result<Win32Channel, String> {
    let path = if name.starts_with(r"\\") {
        name.to_string()
    } else {
        format!(r"\\.\pipe\{name}")
    };
    // The first instance opens here so a name another process owns is reported to the caller,
    // not to a thread that has nobody to tell.
    let first = Instance::create(&path)?;
    let incoming = Arc::new(Mutex::new(VecDeque::new()));
    let queue = Arc::clone(&incoming);
    std::thread::spawn(move || accept(path, first, queue, waker));
    Ok(Win32Channel { incoming })
}

/// One client's message, waiting for the loop.
struct Incoming {
    peer: Arc<Peer>,
    bytes: Vec<u8>,
}

/// A connected client, reachable from the loop and from its reader thread.
struct Peer {
    handle: Handle,
    /// A message write is one operation; the lock keeps a deferred reply from interleaving
    /// with the handler's own answer, and guards the event the write completes on.
    writes: Mutex<()>,
    /// Signalled when a write completes.
    write_done: Handle,
}

// A pipe handle is a process-wide index, not a pointer into thread state.
unsafe impl Send for Peer {}
unsafe impl Sync for Peer {}

impl Peer {
    fn send(&self, bytes: &[u8]) -> Result<(), String> {
        let _write = lock(&self.writes);
        let handle = self.handle.0;
        complete(handle, self.write_done.0, |overlapped| {
            // SAFETY: the handle is open for the life of the `Arc`, `bytes` outlives the call,
            // and the overlapped structure outlives the operation.
            unsafe { WriteFile(handle, Some(bytes), None, Some(overlapped)) }
        })
        .map(|_| ())
        .map_err(|code| format!("pipe write failed: {}", code.0))
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        // A disconnected client is already gone; a failure here has nothing left to repair.
        let _ = unsafe { DisconnectNamedPipe(self.handle.0) };
    }
}

/// A kernel handle owned by one struct, closed on drop.
struct Handle(HANDLE);

// Every handle here is a process-wide index moved to the thread that serves it.
unsafe impl Send for Handle {}

impl Handle {
    /// A manual-reset event, so a waiter can read the result after the signal.
    fn event() -> Result<Self, String> {
        unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
            .map(Self)
            .map_err(|error| format!("create event: {error}"))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // An instance that gave its pipe away keeps a null handle, which closes to nothing.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// A pipe instance the process has created but not yet handed to a connection.
struct Instance(Handle);

impl Instance {
    fn create(path: &str) -> Result<Self, String> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is a terminated string that outlives the call, and the security
        // attributes are omitted, which takes the process token's default descriptor: only
        // the current user and the system can connect. Remote clients are refused outright.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR::from_raw(wide.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                MAX_MESSAGE as u32,
                MAX_MESSAGE as u32,
                0,
                None,
            )
        };
        if handle.is_invalid() {
            let error = unsafe { windows::Win32::Foundation::GetLastError() };
            return Err(format!("create pipe {path}: {}", error.0));
        }
        Ok(Self(Handle(handle)))
    }

    /// Give the pipe to the peer that connected to it.
    fn take(&mut self) -> Handle {
        std::mem::replace(&mut self.0, Handle(HANDLE::default()))
    }
}

/// Accept each client as it connects, serving it on a thread of its own.
///
/// The next instance is created before the connected one is served, so the name never
/// disappears while a command is in flight: the next client connects instead of finding the
/// pipe busy.
fn accept(
    path: String,
    mut listening: Instance,
    incoming: Arc<Mutex<VecDeque<Incoming>>>,
    waker: Option<Arc<dyn Waker>>,
) {
    loop {
        let Ok(ready) = Handle::event() else {
            return;
        };
        let pipe = listening.0.0;
        let connected = complete(pipe, ready.0, |overlapped| {
            // SAFETY: the instance is a live pipe handle and the structure outlives it.
            unsafe { ConnectNamedPipe(pipe, Some(overlapped)) }
        });
        // A client that connected between creation and this call is already here, which the
        // call reports as an error; anything else means the instance could not be used.
        if connected.is_err_and(|code| code.0 != ERROR_PIPE_CONNECTED.0) {
            match Instance::create(&path) {
                Ok(instance) => {
                    listening = instance;
                    continue;
                }
                // Out of handles, or the name was taken over. `listen` has already returned,
                // so nothing is left to report to; the channel stays closed until the backend
                // is built again.
                Err(_) => return,
            }
        }
        let write_done = match Handle::event() {
            Ok(event) => event,
            Err(_) => return,
        };
        let read_done = match Handle::event() {
            Ok(event) => event,
            Err(_) => return,
        };
        let peer = Arc::new(Peer {
            handle: listening.take(),
            writes: Mutex::new(()),
            write_done,
        });
        // The pipe stays a name in the directory only while an instance is listening on it.
        let next = Instance::create(&path);
        let queue = Arc::clone(&incoming);
        let wake = waker.clone();
        std::thread::spawn(move || serve(peer, queue, wake, read_done));
        match next {
            Ok(instance) => listening = instance,
            Err(_) => return,
        }
    }
}

/// Read one client's messages until it disconnects, handing each to the loop.
fn serve(
    peer: Arc<Peer>,
    incoming: Arc<Mutex<VecDeque<Incoming>>>,
    waker: Option<Arc<dyn Waker>>,
    read_done: Handle,
) {
    let mut buffer = vec![0u8; MAX_MESSAGE];
    loop {
        let handle = peer.handle.0;
        let read = complete(handle, read_done.0, |overlapped| {
            // SAFETY: the handle is open while `peer` is alive and the buffer outlives the
            // operation.
            unsafe { ReadFile(handle, Some(buffer.as_mut_slice()), None, Some(overlapped)) }
        });
        let count = match read {
            Ok(count) => count,
            // A message past the portable maximum cannot be a message this channel carries;
            // its tail is drained so the next read starts at the next message.
            Err(code) if code.0 == ERROR_MORE_DATA.0 => {
                drain(&peer, &read_done, &mut buffer);
                continue;
            }
            // The client closed or the pipe broke; both end this connection.
            Err(_) => break,
        };
        let length = (count as usize).min(buffer.len());
        let bytes = buffer.get(..length).map(<[u8]>::to_vec).unwrap_or_default();
        lock(&incoming).push_back(Incoming {
            peer: Arc::clone(&peer),
            bytes,
        });
        if let Some(wake) = &waker {
            wake.wake();
        }
    }
    // Let the client read its last reply before the handle goes away.
    let _ = unsafe { FlushFileBuffers(peer.handle.0) };
}

/// Discard the rest of an oversized message.
fn drain(peer: &Peer, read_done: &Handle, buffer: &mut [u8]) {
    loop {
        let handle = peer.handle.0;
        let result = complete(handle, read_done.0, |overlapped| {
            // SAFETY: as in `serve`.
            unsafe { ReadFile(handle, Some(buffer), None, Some(overlapped)) }
        });
        match result {
            Err(code) if code.0 == ERROR_MORE_DATA.0 => continue,
            _ => return,
        }
    }
}

/// Start one overlapped operation and wait for it, returning the bytes it transferred.
///
/// The failure carries the Win32 code, so a caller can tell the errors that are outcomes —
/// an early connect, an oversized message — from the ones that end a connection.
fn complete(
    handle: HANDLE,
    done: HANDLE,
    start: impl FnOnce(&mut OVERLAPPED) -> Result<(), Error>,
) -> Result<u32, Code> {
    let mut overlapped = OVERLAPPED {
        hEvent: done,
        ..Default::default()
    };
    // SAFETY: the event belongs to the caller; an earlier wait consumed its signal.
    let _ = unsafe { ResetEvent(done) };
    if let Err(error) = start(&mut overlapped)
        && win32_code(&error) != ERROR_IO_PENDING.0
    {
        return Err(Code(win32_code(&error)));
    }
    let mut transferred = 0u32;
    // SAFETY: `overlapped` is the structure the operation was started with, and the handle is
    // the one it was started on.
    unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, true) }
        .map_err(|error| Code(win32_code(&error)))?;
    Ok(transferred)
}

/// A Win32 error code, kept apart from the text so a caller can compare it with a constant.
#[derive(Clone, Copy)]
struct Code(u32);

/// The Win32 code inside a failure from a `Result`-returning call.
fn win32_code(error: &Error) -> u32 {
    error.code().0 as u32 & 0xFFFF
}

/// The reply path of the message being handled.
struct PipeReply<'a> {
    peer: &'a Arc<Peer>,
}

impl Reply for PipeReply<'_> {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.peer.send(bytes)
    }

    fn deferred(&mut self) -> Option<Box<dyn Reply + Send>> {
        // The handle stays open while a reader exists, so a worker may answer after the
        // handler returned.
        Some(Box::new(OwnedReply {
            peer: Arc::clone(self.peer),
        }))
    }
}

/// A reply path for an answer the loop will send later.
struct OwnedReply {
    peer: Arc<Peer>,
}

impl Reply for OwnedReply {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.peer.send(bytes)
    }
}

/// Take the queue, recovering from a panic in a message handler rather than ending the
/// channel: the queue itself is consistent, only the handler was not.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
