//! The render thread: the device, the chain and `Present`, beside the loop rather than on it.
//!
//! The loop builds a frame as a packet — the draw list, the damage, the canvases and the
//! atlas bytes it needs — and hands it over. Everything that talks to the GPU then happens
//! here: uploads, the scene render, the copy into the back buffer, `Present`, the device
//! rebuild after a driver update, the frame read back for a dump. The loop's own thread
//! never blocks in a driver call, so input and layout keep running while a frame is on the
//! GPU.
//!
//! Pacing is this thread's too. `Present` waits at the compositor's next blank, so the
//! loop's next frame is built while the previous one is on screen; the loop in turn waits
//! only for this thread's event, which is [FramePace](crate::frame_pace::FramePace)'s.
//!
//! One texture is not in the packet: the text pass. It is `Rc`-based and shaped per frame,
//! so the loop copies the atlas bytes it changed instead; a rebuilt device sets
//! [`wants_atlas`](RenderThread::take_atlas_request) and the next frame carries both pages
//! again.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use ui_core::Damage;
use ui_core::draw::DrawList;
use ui_platform::present::{CanvasFrame, PresentStats};
use windows::Win32::Foundation::HWND;

use crate::frame_pace::FramePace;
use crate::presenter::{AtlasUpdate, Win32Presenter};

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

/// What the render thread keeps for the loop.
struct Shared {
    /// Frames presented since the loop last read the count.
    presented: AtomicU32,
    /// What the last frame reported, for the caller's own diagnostics.
    stats: Mutex<PresentStats>,
    /// The first failure that stopped the thread; every later call reports it.
    error: Mutex<Option<String>>,
    /// The thread is gone: no later send will be taken.
    stopped: AtomicBool,
    /// The device was rebuilt, so the next frame must carry both atlases again.
    wants_atlas: AtomicBool,
}

/// The handle a session holds: commands in, state out.
pub(crate) struct RenderThread {
    commands: SyncSender<Command>,
    shared: Arc<Shared>,
    join: Option<JoinHandle<()>>,
}

enum Command {
    Frame(Box<FramePacket>),
    Supersample(u32),
    /// The output changed: decide the chain's format and colour space again.
    Rebuild(String),
    Dump(PathBuf, Sender<Result<(), String>>),
    Finish(Sender<()>),
}

impl RenderThread {
    /// Start the thread and wait until it has a device and a chain, or has failed to.
    pub(crate) fn open(
        hwnd: HWND,
        size: (u32, u32),
        composited: bool,
        uncapped: bool,
        pace: Arc<FramePace>,
    ) -> Result<Self, String> {
        let (commands, inbox) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            presented: AtomicU32::new(0),
            stats: Mutex::new(PresentStats::default()),
            error: Mutex::new(None),
            stopped: AtomicBool::new(false),
            wants_atlas: AtomicBool::new(true),
        });
        let thread_shared = Arc::clone(&shared);
        // The window handle is a process-wide index, not a pointer into thread state.
        let raw = hwnd.0 as isize;
        let join = thread::Builder::new()
            .name(String::from("crabsnap-render"))
            .spawn(move || {
                run(
                    raw,
                    size,
                    composited,
                    uncapped,
                    pace,
                    thread_shared,
                    inbox,
                    ready_tx,
                )
            })
            .map_err(|error| format!("the render thread could not start: {error}"))?;
        // A device that cannot be built is the session's to report, on its own thread.
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                shared,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(String::from(
                    "the render thread ended before it opened a device",
                ))
            }
        }
    }

    /// Hand one frame over; the frame's own gate has already closed.
    ///
    /// The channel is one slot deep and the loop only sends when the gate is open, so this
    /// waits only in the uncapped mode, where the loop deliberately draws every turn and
    /// blocking on the frame in flight is the pacing the old synchronous present had.
    pub(crate) fn send_frame(&self, packet: FramePacket) -> Result<(), String> {
        self.send_command(Command::Frame(Box::new(packet)))
    }

    /// Apply a supersampling factor; the thread applies it to a renderer it later rebuilds.
    pub(crate) fn set_supersample(&self, factor: u32) {
        let _ = self.send_command(Command::Supersample(factor));
    }

    /// Have the thread open a fresh chain, such as after the display changed.
    pub(crate) fn rebuild(&self, reason: String) {
        let _ = self.send_command(Command::Rebuild(reason));
    }

    /// Write the last presented frame as a PPM, once the thread has drained its queue.
    pub(crate) fn dump_frame_ppm(&self, path: PathBuf) -> Result<(), String> {
        let (reply, answer) = mpsc::channel();
        self.send_command(Command::Dump(path, reply))?;
        answer
            .recv()
            .map_err(|_| self.failure())?
    }

    /// Drain the GPU and stop the thread; a no-op after the first call.
    pub(crate) fn finish(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        let (reply, answer) = mpsc::channel();
        let _ = self.commands.send(Command::Finish(reply));
        let _ = answer.recv();
        let _ = join.join();
    }

    pub(crate) fn stats(&self) -> PresentStats {
        *self.shared.stats.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// Frames presented since the last call.
    pub(crate) fn take_presented(&self) -> u32 {
        self.shared.presented.swap(0, Ordering::AcqRel)
    }

    /// The failure that stopped the thread, if one did.
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

    fn send_command(&self, command: Command) -> Result<(), String> {
        self.commands.send(command).map_err(|_| self.failure())
    }

    fn failure(&self) -> String {
        self.error().unwrap_or_else(|| {
            String::from("the render thread ended and left no reason")
        })
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        // The thread holds the chain, which outlives nothing but the window: join it before
        // the session's window goes.
        self.finish();
    }
}

/// Mark the thread gone and open the gate, whether it returned or unwound.
struct StopGuard {
    shared: Arc<Shared>,
    pace: Arc<FramePace>,
}

impl Drop for StopGuard {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.pace.finished();
    }
}

fn run(
    hwnd: isize,
    size: (u32, u32),
    composited: bool,
    uncapped: bool,
    pace: Arc<FramePace>,
    shared: Arc<Shared>,
    inbox: Receiver<Command>,
    ready: Sender<Result<(), String>>,
) {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    let _guard = StopGuard {
        shared: Arc::clone(&shared),
        pace: Arc::clone(&pace),
    };
    let mut size = size;
    let mut presenter = match Win32Presenter::open_raw(hwnd, size, composited, uncapped) {
        Ok(presenter) => presenter,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    let mut supersample = 1u32;
    if ready.send(Ok(())).is_err() {
        // The session gave up waiting; there is nothing to present to.
        return;
    }
    // The first frame owes nothing to a previous present.
    pace.finished();
    while let Ok(command) = inbox.recv() {
        match command {
            Command::Frame(mut packet) => {
                size = packet.size;
                match draw(&mut presenter, &mut packet) {
                    Ok(stats) => {
                        *shared
                            .stats
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner()) = stats;
                        shared.presented.fetch_add(1, Ordering::AcqRel);
                    }
                    Err(error) => {
                        let Some(reason) = presenter.device_loss() else {
                            // A refused call, not a lost device: the loop reports it and
                            // the thread keeps its chain for the next frame.
                            store(&shared.error, error);
                            pace.finished();
                            continue;
                        };
                        // A driver update or a lost adapter: the old chain goes first,
                        // because a window holds one flip chain at a time.
                        drop(presenter);
                        match reopen(hwnd, size, composited, uncapped, supersample) {
                            Ok(fresh) => {
                                presenter = fresh;
                                shared.wants_atlas.store(true, Ordering::Release);
                                if let Err(error) = draw(&mut presenter, &mut packet) {
                                    store(
                                        &shared.error,
                                        format!(
                                            "after rebuilding the device ({reason}): {error}"
                                        ),
                                    );
                                }
                            }
                            Err(error) => {
                                store(
                                    &shared.error,
                                    format!(
                                        "the device was lost ({reason}) and could not be rebuilt: {error}"
                                    ),
                                );
                                // No presenter is left to take another frame.
                                return;
                            }
                        }
                    }
                }
                // Whether it drew or failed, the loop must not wait on a frame nobody takes.
                pace.finished();
            }
            Command::Supersample(factor) => {
                supersample = factor;
                presenter.renderer_mut().set_supersample(factor);
            }
            Command::Rebuild(reason) => {
                drop(presenter);
                match reopen(hwnd, size, composited, uncapped, supersample) {
                    Ok(fresh) => {
                        presenter = fresh;
                        shared.wants_atlas.store(true, Ordering::Release);
                    }
                    Err(error) => {
                        store(
                            &shared.error,
                            format!("the chain could not be rebuilt after {reason}: {error}"),
                        );
                        return;
                    }
                }
                // The new chain holds no frame: the compositor's window is open already.
                pace.finished();
            }
            Command::Dump(path, reply) => {
                let _ = reply.send(presenter.dump_frame_ppm(&path));
            }
            Command::Finish(reply) => {
                presenter.finish();
                let _ = reply.send(());
                pace.finished();
                return;
            }
        }
    }
}

/// A fresh device and chain at `size`, with the supersampling the app asked for.
///
/// The caller drops the previous presenter first: a window holds one flip chain at a time.
fn reopen(
    hwnd: HWND,
    size: (u32, u32),
    composited: bool,
    uncapped: bool,
    supersample: u32,
) -> Result<Win32Presenter, String> {
    let mut presenter = Win32Presenter::open_raw(hwnd, size, composited, uncapped)
        .map_err(|error| error.to_string())?;
    presenter.renderer_mut().set_supersample(supersample);
    Ok(presenter)
}

fn store(slot: &Mutex<Option<String>>, error: String) {
    let mut slot = slot.lock().unwrap_or_else(|poison| poison.into_inner());
    if slot.is_none() {
        *slot = Some(error);
    }
}

/// Resize, upload and present one packet.
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
    )
}
