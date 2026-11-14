//! The GPU thread: one device stack, every surface's chain, beside the loops rather than on them.
//!
//! The loops build frames as packets — the draw list, the damage, the canvases and the atlas
//! bytes they need — and hand them over. Everything that talks to the GPU then happens here:
//! uploads, the scene render, the copy into the back buffer, `Present`, the device rebuild
//! after a driver update, the frame read back for a dump. A loop's own thread never blocks in
//! a driver call, so input and layout keep running while a frame is on the GPU.
//!
//! There is one of these threads, not one per surface. DXGI, D3D11 and DirectComposition each
//! keep process-wide state behind their objects, and two thread stacks presenting two chains
//! at once corrupt it; one thread owning every chain is also how a compositor is built. That
//! thread must not be one window's to hold: each chain carries a frame-latency event, and the
//! thread waits on all of them at once and presents each chain the moment it says it can take
//! a frame. No window's blank is ever paid by another window.
//!
//! Pacing is still the loop's. A loop waits for its own [FramePace](crate::frame_pace::FramePace)
//! event, which this thread opens after the frame it handed over is on the chain; the loop then
//! builds the next one.
//!
//! One texture is not in the packet: the text pass. It is `Rc`-based and shaped per frame, so
//! the loop copies the atlas bytes it changed instead; a rebuilt device sets
//! [`wants_atlas`](RenderThread::take_atlas_request) and the next frame carries both pages
//! again.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use ui_core::Damage;
use ui_core::draw::DrawList;
use ui_platform::present::{CanvasFrame, PresentStats};
use windows::Win32::Foundation::{HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};

use crate::frame_pace::FramePace;
use crate::presenter::{Alpha, AtlasUpdate, Win32Presenter};

/// A chain the thread waits on when no window's frame is due.
const IDLE: u32 = u32::MAX;
/// A chain that has no latency event is presented on this tick instead of a blank.
const RETRY_MS: u32 = 4;
/// `WaitForMultipleObjects` takes 64 handles; the wake event is one of them.
const MAX_EVENTS: usize = 60;

/// One frame as the loop hands it over: everything the GPU needs, owned by the packet.
pub(crate) struct FramePacket {
    pub scene: DrawList,
    pub damage: Damage,
    pub canvases: Vec<CanvasFrame>,
    pub atlas: Option<AtlasUpdate>,
    pub color_atlas: Option<AtlasUpdate>,
    pub size: (u32, u32),
    pub scale: f32,
}

/// What the GPU thread keeps for one loop.
struct Shared {
    /// Frames presented since the loop last read the count.
    presented: AtomicU32,
    /// What the last frame reported, for the caller's own diagnostics.
    stats: Mutex<PresentStats>,
    /// The first failure that stopped this surface; every later call reports it.
    error: Mutex<Option<String>>,
    /// The surface is gone: no later send will be taken.
    stopped: AtomicBool,
    /// The device was rebuilt, so the next frame must carry both atlases again.
    wants_atlas: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            presented: AtomicU32::new(0),
            stats: Mutex::new(PresentStats::default()),
            error: Mutex::new(None),
            stopped: AtomicBool::new(false),
            wants_atlas: AtomicBool::new(true),
        }
    }

    fn store(&self, error: String) {
        let mut slot = self.error.lock().unwrap_or_else(|poison| poison.into_inner());
        if slot.is_none() {
            *slot = Some(error);
        }
    }
}

/// One loop's handle on the GPU thread: frames in, state out.
pub(crate) struct RenderThread {
    index: usize,
    shared: Arc<Shared>,
    finished: bool,
}

enum Command {
    Open {
        hwnd: isize,
        size: (u32, u32),
        alpha: Alpha,
        uncapped: bool,
        pace: Arc<FramePace>,
        shared: Arc<Shared>,
        reply: Sender<Result<usize, String>>,
    },
    Frame {
        index: usize,
        packet: Box<FramePacket>,
    },
    Supersample {
        index: usize,
        factor: u32,
    },
    /// The output changed: decide the chain's format and colour space again.
    Rebuild {
        index: usize,
        reason: String,
    },
    Dump {
        index: usize,
        path: PathBuf,
        reply: Sender<Result<(), String>>,
    },
    Close {
        index: usize,
        reply: Sender<()>,
    },
}

/// The one GPU thread's inbox: commands from every loop, and the event that wakes it.
struct Service {
    queue: Mutex<VecDeque<Command>>,
    wake: HANDLE,
}

// SAFETY: the handle is owned by the service for the life of the process and only ever
// passed to the wait and signal calls, which accept it from any thread.
unsafe impl Send for Service {}
unsafe impl Sync for Service {}

impl Service {
    fn send(&self, command: Command) {
        self.queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push_back(command);
        // SAFETY: the handle is this service's own and every caller owns a reference to it.
        unsafe {
            let _ = SetEvent(self.wake);
        }
    }

    fn drain(&self) -> Vec<Command> {
        let mut queue = self.queue.lock().unwrap_or_else(|poison| poison.into_inner());
        queue.drain(..).collect()
    }
}

/// The process's GPU thread, started by the first surface that needs one.
fn service() -> Arc<Service> {
    static SERVICE: OnceLock<Arc<Service>> = OnceLock::new();
    Arc::clone(SERVICE.get_or_init(|| {
        // SAFETY: no attributes, an auto-reset event, unsignalled, unnamed.
        let wake = unsafe { CreateEventW(None, false, false, None) }
            .expect("the GPU thread needs a wake event");
        let service = Arc::new(Service {
            queue: Mutex::new(VecDeque::new()),
            wake,
        });
        let thread_service = Arc::clone(&service);
        thread::Builder::new()
            .name(String::from("crabsnap-gpu"))
            .spawn(move || run(thread_service))
            .expect("the GPU thread could not start");
        service
    }))
}

impl RenderThread {
    /// Hand a surface to the GPU thread and wait until it has a chain, or has failed to.
    pub(crate) fn open(
        hwnd: HWND,
        size: (u32, u32),
        alpha: Alpha,
        uncapped: bool,
        pace: Arc<FramePace>,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared::new());
        let (reply, answer) = mpsc::channel();
        service().send(Command::Open {
            // The window handle is a process-wide index, not a pointer into thread state.
            hwnd: hwnd.0 as isize,
            size,
            alpha,
            uncapped,
            pace,
            shared: Arc::clone(&shared),
            reply,
        });
        match answer.recv() {
            Ok(Ok(index)) => Ok(Self {
                index,
                shared,
                finished: false,
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(String::from(
                "the GPU thread ended before it opened a chain",
            )),
        }
    }

    /// Hand one frame over; the frame's own gate has already closed.
    ///
    /// The queue is the GPU thread's, and the gate the loop closed before calling this is
    /// what keeps it one frame deep: nothing here waits for the GPU.
    pub(crate) fn send_frame(&self, packet: FramePacket) -> Result<(), String> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(self.failure());
        }
        service().send(Command::Frame {
            index: self.index,
            packet: Box::new(packet),
        });
        Ok(())
    }

    /// Apply a supersampling factor; the thread applies it to a renderer it later rebuilds.
    pub(crate) fn set_supersample(&self, factor: u32) {
        service().send(Command::Supersample {
            index: self.index,
            factor,
        });
    }

    /// Have the thread open a fresh chain, such as after the display changed.
    pub(crate) fn rebuild(&self, reason: String) {
        service().send(Command::Rebuild {
            index: self.index,
            reason,
        });
    }

    /// Write the last presented frame as a PPM, once the thread has drawn it.
    pub(crate) fn dump_frame_ppm(&self, path: PathBuf) -> Result<(), String> {
        let (reply, answer) = mpsc::channel();
        service().send(Command::Dump {
            index: self.index,
            path,
            reply,
        });
        answer.recv().map_err(|_| self.failure())?
    }

    /// Give the chain back and stop hearing from the GPU thread about this surface.
    ///
    /// A no-op after the first call. The chain is released before this returns, so the
    /// window it presents through may be destroyed next.
    pub(crate) fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let (reply, answer) = mpsc::channel();
        service().send(Command::Close {
            index: self.index,
            reply,
        });
        let _ = answer.recv();
    }

    pub(crate) fn stats(&self) -> PresentStats {
        *self.shared.stats.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// Frames presented since the last call.
    pub(crate) fn take_presented(&self) -> u32 {
        self.shared.presented.swap(0, Ordering::AcqRel)
    }

    /// The failure that stopped this surface, if one did.
    pub(crate) fn error(&self) -> Option<String> {
        self.shared
            .error
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Whether a rebuilt device has to be given both atlases again.
    pub(crate) fn take_atlas_request(&self) -> bool {
        self.shared.wants_atlas.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::Acquire)
    }

    fn failure(&self) -> String {
        self.error().unwrap_or_else(|| {
            String::from("the GPU thread ended and left no reason")
        })
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        // The thread holds the chain, which outlives nothing but the window: give it back
        // before the session's window goes.
        self.finish();
    }
}

/// One surface as the GPU thread holds it.
struct Entry {
    /// Absent only while the chain is being replaced.
    presenter: Option<Win32Presenter>,
    shared: Arc<Shared>,
    pace: Arc<FramePace>,
    /// The frame the loop handed over and the chain has not taken yet.
    pending: Option<Box<FramePacket>>,
    hwnd: HWND,
    size: (u32, u32),
    alpha: Alpha,
    uncapped: bool,
    supersample: u32,
}

fn run(service: Arc<Service>) {
    let mut entries: Vec<Option<Entry>> = Vec::new();
    loop {
        for command in service.drain() {
            apply(command, &mut entries);
        }

        // Take every frame a chain is ready for, without waiting on any of them.
        let mut due: Vec<usize> = Vec::new();
        let mut events: Vec<HANDLE> = Vec::new();
        let mut waiting: Vec<usize> = Vec::new();
        let mut untimed = false;
        for (index, entry) in entries.iter().enumerate() {
            let Some(entry) = entry else {
                continue;
            };
            if entry.pending.is_none() || entry.presenter.is_none() {
                continue;
            }
            if entry.uncapped {
                due.push(index);
                continue;
            }
            match entry.presenter.as_ref().and_then(Win32Presenter::latency_event) {
                Some(event) if events.len() < MAX_EVENTS => {
                    events.push(event);
                    waiting.push(index);
                }
                // A chain the driver gave no event, or more chains than the wait takes: it
                // is presented on the retry tick instead of at its blank.
                _ => untimed = true,
            }
        }
        for index in due {
            submit(index, &mut entries);
        }

        if events.is_empty() && !untimed {
            // No frame anywhere: sleep until a loop hands one over.
            // SAFETY: the handle is the service's own wake event.
            unsafe {
                WaitForSingleObject(service.wake, IDLE);
            }
            continue;
        }
        let mut handles: Vec<HANDLE> = Vec::with_capacity(events.len() + 1);
        handles.push(service.wake);
        handles.extend_from_slice(&events);
        let timeout = if untimed { RETRY_MS } else { IDLE };
        // SAFETY: every handle is a live event owned by the service or by a chain.
        let ready = unsafe { WaitForMultipleObjects(&handles, false, timeout) };
        if ready == WAIT_TIMEOUT {
            if untimed {
                for index in 0..entries.len() {
                    let Some(entry) = &entries[index] else {
                        continue;
                    };
                    let Some(presenter) = &entry.presenter else {
                        continue;
                    };
                    if entry.pending.is_some() && presenter.latency_event().is_none() {
                        submit(index, &mut entries);
                    }
                }
            }
            continue;
        }
        if ready == WAIT_OBJECT_0 {
            // A command arrived; the next pass serves it.
            continue;
        }
        let slot = ready.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
        if let Some(&index) = waiting.get(slot.wrapping_sub(1)) {
            submit(index, &mut entries);
        }
    }
}

fn apply(command: Command, entries: &mut Vec<Option<Entry>>) {
    match command {
        Command::Open {
            hwnd,
            size,
            alpha,
            uncapped,
            pace,
            shared,
            reply,
        } => {
            let hwnd = HWND(hwnd as *mut core::ffi::c_void);
            match Win32Presenter::open_raw(hwnd, size, alpha, uncapped) {
                Ok(presenter) => {
                    let entry = Entry {
                        presenter: Some(presenter),
                        shared,
                        pace,
                        pending: None,
                        hwnd,
                        size,
                        alpha,
                        uncapped,
                        supersample: 1,
                    };
                    let index = match entries.iter().position(Option::is_none) {
                        Some(index) => {
                            entries[index] = Some(entry);
                            index
                        }
                        None => {
                            entries.push(Some(entry));
                            entries.len() - 1
                        }
                    };
                    let _ = reply.send(Ok(index));
                }
                Err(error) => {
                    let _ = reply.send(Err(error.to_string()));
                }
            }
        }
        Command::Frame { index, packet } => {
            let Some(Some(entry)) = entries.get_mut(index) else {
                return;
            };
            // The gate kept the loop to one frame in flight, so a second one means the
            // first was never taken; the one it replaces is the frame nobody shows.
            if entry.pending.replace(packet).is_some() {
                entry.pace.release();
            }
        }
        Command::Supersample { index, factor } => {
            if let Some(Some(entry)) = entries.get_mut(index) {
                entry.supersample = factor;
                if let Some(presenter) = entry.presenter.as_mut() {
                    presenter.renderer_mut().set_supersample(factor);
                }
            }
        }
        Command::Rebuild { index, reason } => {
            if let Some(Some(entry)) = entries.get_mut(index) {
                if let Err(error) = rebuild(entry, &reason) {
                    entry.shared.store(error);
                    entry.shared.stopped.store(true, Ordering::Release);
                    entry.pace.finished();
                }
            }
        }
        Command::Dump { index, path, reply } => {
            let result = match entries.get_mut(index) {
                Some(Some(entry)) => match entry.presenter.as_mut() {
                    Some(presenter) => presenter.dump_frame_ppm(&path),
                    None => Err(String::from("the surface has no chain to read")),
                },
                _ => Err(String::from("the surface is gone")),
            };
            let _ = reply.send(result);
        }
        Command::Close { index, reply } => {
            if let Some(slot) = entries.get_mut(index) {
                if let Some(entry) = slot.take() {
                    entry.shared.stopped.store(true, Ordering::Release);
                    entry.pace.finished();
                }
            }
            let _ = reply.send(());
        }
    }
}

/// Draw the frame a chain is ready for and present it.
fn submit(index: usize, entries: &mut [Option<Entry>]) {
    let Some(entry) = entries[index].as_mut() else {
        return;
    };
    let Some(packet) = entry.pending.take() else {
        return;
    };
    let Some(mut presenter) = entry.presenter.take() else {
        entry.pace.finished();
        return;
    };
    let mut packet = *packet;
    match draw(&mut presenter, &mut packet) {
        Ok(stats) => {
            *entry
                .shared
                .stats
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) = stats;
            entry.shared.presented.fetch_add(1, Ordering::AcqRel);
            entry.presenter = Some(presenter);
        }
        Err(error) => match presenter.device_loss() {
            None => {
                // A refused call, not a lost device: the loop reports it and the chain
                // stays for the next frame.
                entry.shared.store(error);
                entry.presenter = Some(presenter);
            }
            Some(reason) => {
                // A driver update or a lost adapter: the old chain goes first, because a
                // window holds one flip chain at a time.
                drop(presenter);
                match reopen(entry.hwnd, entry.size, entry.alpha, entry.supersample) {
                    Ok(fresh) => {
                        entry.presenter = Some(fresh);
                        entry.shared.wants_atlas.store(true, Ordering::Release);
                        if let Some(presenter) = entry.presenter.as_mut() {
                            if let Err(error) = draw(presenter, &mut packet) {
                                entry.shared.store(format!(
                                    "after rebuilding the device ({reason}): {error}"
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        entry.shared.store(format!(
                            "the device was lost ({reason}) and could not be rebuilt: {error}"
                        ));
                        entry.shared.stopped.store(true, Ordering::Release);
                    }
                }
            }
        },
    }
    // Whether it drew or failed, the loop must not wait on a frame nobody takes.
    entry.pace.finished();
}

/// Replace a surface's chain, and owe the next frame both atlases.
fn rebuild(entry: &mut Entry, reason: &str) -> Result<(), String> {
    // A window holds one flip chain at a time: the old one goes before the new one opens.
    entry.presenter = None;
    let fresh = reopen(entry.hwnd, entry.size, entry.alpha, entry.supersample)
        .map_err(|error| format!("the chain could not be rebuilt after {reason}: {error}"))?;
    entry.presenter = Some(fresh);
    entry.shared.wants_atlas.store(true, Ordering::Release);
    // The new chain holds no frame: the compositor's window is open already.
    entry.pace.finished();
    Ok(())
}

/// A fresh device and chain at `size`, with the supersampling the app asked for.
///
/// The caller drops the previous presenter first: a window holds one flip chain at a time.
fn reopen(
    hwnd: HWND,
    size: (u32, u32),
    alpha: Alpha,
    supersample: u32,
) -> Result<Win32Presenter, String> {
    let mut presenter = Win32Presenter::open_raw(hwnd, size, alpha, false)
        .map_err(|error| error.to_string())?;
    presenter.renderer_mut().set_supersample(supersample);
    Ok(presenter)
}

/// Resize, upload and present one packet, with no interval: the caller waited for the chain.
fn draw(presenter: &mut Win32Presenter, packet: &mut FramePacket) -> Result<PresentStats, String> {
    presenter
        .resize(packet.size, packet.scale)
        .map_err(|error| error.to_string())?;
    presenter.present_uploads(
        &packet.scene,
        &packet.damage,
        &packet.canvases,
        packet.atlas.take(),
        packet.color_atlas.take(),
        0,
    )
}
