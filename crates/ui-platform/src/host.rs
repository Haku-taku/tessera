//! The process loop: one wait, every surface, every platform service.
//!
//! A `Host` owns the loop and the surfaces; the platform `Backend` owns the connection and the
//! native objects behind it. Nothing here names a protocol, a file descriptor or a platform.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_core::draw::ImageEntry;
use ui_core::shortcut::{KeyChord, ShortcutId};

use crate::clipboard::ClipboardContent;
use crate::clipboard::{ClipboardCopy, ClipboardKind};
use crate::hotkeys::{GlobalHotkeyError, GlobalHotkeys};
use crate::ipc::{Channel, Reply};
use crate::jobs::{Jobs, Waker};
use crate::output::OutputInfo;
use crate::transfer::{Selection, SelectionReader};
use crate::tray::{Icon, Tray, TrayEvent, TrayHandle, TrayId};
use crate::view::App;
use crate::window::SurfaceSpec;

/// What a run did, for the application's own diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    /// Turns the loop took.
    pub turns: u64,
    /// Frames the surfaces presented.
    pub frames: u64,
}

/// What one non-blocking surface turn did.
#[derive(Clone, Copy, Debug, Default)]
pub struct SurfaceStep {
    /// The surface built and committed a frame.
    pub drew: bool,
}

/// One surface as the loop sees it: a step that never waits, and when it must be woken.
///
/// The platform implements this for its surface type. `step` drains what has already
/// arrived, decides, and draws only if a frame is due and the surface can take one.
pub trait SurfaceHandle {
    fn step(&mut self) -> Result<SurfaceStep, String>;

    /// Hand this surface a texture it did not have when it opened.
    ///
    /// A pin's canvas is already on screen when the next pin arrives, so the texture cannot
    /// wait for the next `open`.
    fn add_image(&mut self, image: ui_core::draw::ImageEntry) -> Result<(), String> {
        let _ = image;
        Err("this platform cannot add a texture to a running surface".into())
    }

    /// When this surface must be woken with no input arriving (a caret blink, a snapping
    /// scroller). `None` means it only moves when something happens.
    fn next_deadline(&self) -> Option<Instant>;

    /// Build a frame on the next turn even though no event arrived. A finished job is
    /// the usual reason: its result lives in app state only the build reads.
    fn request_frame(&mut self) {}

    fn is_closed(&self) -> bool;

    /// Copies the last frame asked for, each with the serial it must be served under.
    ///
    /// The host owns the clipboard, so a surface hands over the request instead of serving
    /// it; paste requests are the surface's own business and never reach here.
    fn take_copies(&mut self) -> Vec<ClipboardCopy>;
}

/// One font the surface draws with: bytes under the family name that resolves to them.
///
/// The engine ships no font, and an icon face a control names by family is usually not
/// installed on the machine either, so whoever distributes the app supplies the bytes.
pub struct FontFace {
    pub name: String,
    pub data: Arc<[u8]>,
}

/// What to bring up with a surface.
#[derive(Default)]
pub struct OpenOptions {
    /// Draw on every turn instead of one frame per compositor callback.
    pub uncapped: bool,
    /// Textures the surface draws before its first frame.
    pub images: Vec<ImageEntry>,
    /// Chords this surface's app answers through `App::shortcut`, bound before its first
    /// frame: a key the surface exists for (an overlay's Escape) is not known to the app's
    /// build, which has no way to register one.
    pub shortcuts: Vec<(KeyChord, ShortcutId)>,
    /// Families registered before the first frame, so a build that draws an icon by name
    /// finds its glyph.
    pub fonts: Vec<FontFace>,
}

/// What an application message may ask of the loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
}

/// The context a turn callback gets: ask for what the loop itself must do.
pub struct LoopCtx<'a> {
    quit: &'a mut bool,
    deadline: &'a mut Option<Instant>,
    outputs: &'a [OutputInfo],
}

impl LoopCtx<'_> {
    /// Stop the loop after this turn.
    pub fn quit(&mut self) {
        *self.quit = true;
    }

    /// Wake the loop at `at` with nothing arriving; `None` drops the deadline.
    pub fn set_deadline(&mut self, at: Option<Instant>) {
        *self.deadline = at;
    }

    /// The outputs as of this turn.
    pub fn outputs(&self) -> &[OutputInfo] {
        self.outputs
    }
}

/// A surface this process opened, by the loop's own bookkeeping.
///
/// Stable for the surface's whole life: closing another surface must not make a held id
/// name a different one, which an index into the live list would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceId(u64);

/// What a message handler may ask of the loop: the outputs it can see, and a surface.
///
/// A command that arrives over the channel has no surface of its own — a daemon holds none
/// until a pin asks for one — so answering it needs the loop's own door, not just the reply.
pub struct MessageCtx<'a> {
    outputs: &'a [OutputInfo],
    backend: &'a mut dyn Backend,
    surfaces: &'a mut Vec<LiveSurface>,
    next_surface: &'a mut u64,
    /// A surface appeared or took a texture while the loop was turning. It has drawn
    /// nothing yet, so the loop must turn again before it blocks.
    pending_draw: &'a mut bool,
}

/// A surface and the id the loop gave it.
type LiveSurface = (SurfaceId, Box<dyn SurfaceHandle>);

impl MessageCtx<'_> {
    /// The outputs as of this turn.
    pub fn outputs(&self) -> &[OutputInfo] {
        self.outputs
    }

    /// Open a surface on the host's connection; the loop ticks it from then on.
    pub fn open(
        &mut self,
        spec: SurfaceSpec,
        app: Box<dyn App>,
        options: OpenOptions,
    ) -> Result<SurfaceId, String> {
        let handle = self.backend.open(spec, app, options)?;
        let id = SurfaceId(*self.next_surface);
        *self.next_surface += 1;
        // The surface has not been stepped: the turn that opened it is already past the
        // surfaces, so without this the loop would sleep on a surface nobody drew.
        *self.pending_draw = true;
        self.surfaces.push((id, handle));
        Ok(id)
    }

    /// Add a texture to a surface this handler opened.
    pub fn add_image(
        &mut self,
        id: SurfaceId,
        image: ui_core::draw::ImageEntry,
    ) -> Result<(), String> {
        let surface = self
            .surfaces
            .iter_mut()
            .find(|(sid, _)| *sid == id)
            .map(|(_, surface)| surface)
            .ok_or("no such surface")?;
        surface.add_image(image)?;
        // Registering a texture changes what a running surface shows only when it builds
        // again: the app put the node that draws it into its own state, and nothing else
        // will make the surface draw.
        surface.request_frame();
        *self.pending_draw = true;
        Ok(())
    }

    /// Have `id` build a frame on the next turn.
    ///
    /// A request that changed what a running surface draws reaches the app only through
    /// its next build; without this the old frame stays on screen until input arrives.
    pub fn request_frame(&mut self, id: SurfaceId) -> Result<(), String> {
        let surface = self
            .surfaces
            .iter_mut()
            .find(|(sid, _)| *sid == id)
            .map(|(_, surface)| surface)
            .ok_or("no such surface")?;
        surface.request_frame();
        *self.pending_draw = true;
        Ok(())
    }

    /// Whether `id` still names a surface on screen.
    ///
    /// A pressed hotkey may open a surface only when none is already there; the answer comes
    /// from the loop, which owns the lifecycle, not from a flag the app would have to clear.
    pub fn is_open(&self, id: SurfaceId) -> bool {
        self.surfaces
            .iter()
            .any(|(sid, surface)| *sid == id && !surface.is_closed())
    }

    /// Put `content` on the clipboard from a handler that owns no surface.
    ///
    /// A command's answer is sometimes a copy, and the command itself exits. The serial is
    /// the loop's own: a platform whose clipboard needs one learns it from surface input, and
    /// one that offers a privileged clipboard channel needs none. False when the platform has
    /// no clipboard at all.
    pub fn copy(&mut self, content: ClipboardContent, kind: ClipboardKind) -> bool {
        self.backend.set_clipboard_copy(ClipboardCopy {
            content,
            kind,
            serial: 0,
        })
    }

    /// The MIME types the process's selection offers, for a command that reads it.
    pub fn selection_offers(&mut self) -> Vec<String> {
        self.backend
            .selection_reader()
            .map(|reader| reader.offers())
            .unwrap_or_default()
    }

    /// Start reading one MIME of the selection; its bytes arrive through
    /// [`take_selection`](Self::take_selection) on a later turn.
    pub fn read_selection(&mut self, mime: &str) -> bool {
        self.backend
            .selection_reader()
            .is_some_and(|reader| reader.read(mime))
    }

    /// A selection read finished since the last call: the payload, once.
    ///
    /// Called from the turn callback, where a command's deferred answer also lives.
    pub fn take_selection(&mut self) -> Option<Selection> {
        self.backend
            .selection_reader()
            .and_then(|reader| reader.poll())
    }

    /// Bind a chord; a press later reaches the handler set with `Host::on_hotkey`.
    pub fn bind_hotkey(
        &mut self,
        chord: KeyChord,
        id: ShortcutId,
    ) -> Result<(), GlobalHotkeyError> {
        match self.backend.hotkeys() {
            Some(hotkeys) => hotkeys.register(chord, id),
            None => Err(GlobalHotkeyError::Unsupported),
        }
    }

    /// Give the chord back. A platform without hotkeys has nothing to release.
    pub fn unbind_hotkey(&mut self, id: ShortcutId) {
        if let Some(hotkeys) = self.backend.hotkeys() {
            hotkeys.unregister(id);
        }
    }
}

/// The platform half: open surfaces, hold the connection, wait, serve the clipboard.
///
/// `wait` is the only blocking call in the process's loop. It polls the platform's handles
/// (the compositor connection, the clipboard pipes) and returns; surfaces then dispatch what
/// arrived. A backend may add its own sources to that poll as the loop grows.
pub trait Backend {
    fn open(
        &mut self,
        spec: SurfaceSpec,
        app: Box<dyn App>,
        options: OpenOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String>;

    /// Publish a copy the loop drained from a surface. False when no clipboard exists.
    fn set_clipboard_copy(&mut self, copy: ClipboardCopy) -> bool;

    /// Answer selection requests that arrived since the last call.
    fn serve_clipboard(&mut self) -> Result<(), String>;

    /// The process's selection reader, when this platform can read one.
    fn selection_reader(&mut self) -> Option<&mut (dyn SelectionReader + 'static)> {
        None
    }

    /// When this backend must be woken with nothing arriving (a clipboard read that is
    /// about to time out). `None` means it only moves when something happens.
    fn next_deadline(&self) -> Option<Instant> {
        None
    }

    /// Block until an event, a source, or `timeout`; `None` waits for an event.
    fn wait(&mut self, timeout: Option<Duration>) -> Result<(), String>;

    /// Send whatever the turn queued.
    fn flush(&mut self);

    /// The host or the backend asked to end the loop.
    fn should_quit(&self) -> bool {
        false
    }

    /// Open the process's local channel. The backend keeps the socket in its wait set; the
    /// returned handle is what the loop polls for messages.
    fn listen(&mut self, name: &str) -> Result<Box<dyn Channel>, String> {
        let _ = name;
        Err("this platform has no local channel".into())
    }

    /// The outputs as they are now; empty when the platform reports none.
    fn outputs(&mut self) -> Vec<OutputInfo> {
        Vec::new()
    }

    /// The user's preferred UI languages, most preferred first, as BCP-47 tags.
    ///
    /// The default is the POSIX environment's answer, which is what a Linux session
    /// states; a platform whose system API reports it another way overrides this.
    fn system_languages(&self) -> Vec<String> {
        crate::locale::posix_system_languages()
    }

    /// How a worker wakes the loop when it finishes. `None` when this platform has no
    /// wait to interrupt; jobs still run, but the loop hears about them on its next turn.
    fn waker(&mut self) -> Option<Arc<dyn Waker>> {
        None
    }

    /// The system-wide hotkeys this backend can provide, opened on the first request.
    ///
    /// The platform is fixed when the backend is built; a platform with several transports
    /// picks one here, at runtime, because only the running desktop knows what it has.
    fn hotkeys(&mut self) -> Option<&mut (dyn GlobalHotkeys + 'static)> {
        None
    }

    /// Put `icon` in the system tray, reporting what the user does through
    /// [`Self::tray_events`].
    ///
    /// `id` is what every later report for this icon carries. The default is a platform with
    /// no notification area, which is what the error says.
    fn create_tray(&mut self, id: TrayId, icon: Icon) -> Result<Box<dyn TrayHandle>, String> {
        let _ = (id, icon);
        Err("this platform has no system tray".into())
    }

    /// What the user did to a tray icon since the last call, in order.
    fn tray_events(&mut self) -> Vec<(TrayId, TrayEvent)> {
        Vec::new()
    }
}

/// A platform with no wait to interrupt: a finished job waits for the next turn.
struct NoWaker;

impl Waker for NoWaker {
    fn wake(&self) {}
}

/// The loop. One per process; created by the application's platform layer.
pub struct Host {
    backend: Box<dyn Backend>,
    surfaces: Vec<LiveSurface>,
    /// The id the next surface gets; never reused, so a held id cannot name a later surface.
    next_surface: u64,
    quit: bool,
    stats: RunStats,
    /// The application's messages, and what answers them.
    channel: Option<(Box<dyn Channel>, MessageHandler)>,
    /// Outputs as of the last turn; the channel handler and the turn callback read these.
    outputs: Vec<OutputInfo>,
    /// A deadline the application set; the loop wakes for it even with nothing arriving.
    deadline: Option<Instant>,
    /// Set by anything that gave a surface work it has not drawn; see [`MessageCtx`].
    pending_draw: bool,
    /// The application's per-turn work: the loop's own context beside the same door a
    /// message handler gets, so a turn can open a surface for what a surface just asked.
    on_turn: Option<TurnHandler>,
    /// What a pressed hotkey does; it answers the same way a channel request does.
    on_hotkey: Option<HotkeyHandler>,
    /// What the user did to a tray icon; the icons themselves are the app's.
    on_tray: Option<TrayHandler>,
    /// The id the next tray icon gets; never reused, so a held id cannot name a later icon.
    next_tray: u64,
    /// Background work shared by the app; a completion forces one frame.
    jobs: Jobs,
    /// Jobs completed as of the last turn.
    completed_seen: u64,
}

type MessageHandler = Box<dyn FnMut(&mut dyn Reply, &[u8], &mut MessageCtx<'_>) -> Effect>;
type TurnHandler = Box<dyn FnMut(&mut MessageCtx<'_>, &mut LoopCtx<'_>)>;
type HotkeyHandler = Box<dyn FnMut(ShortcutId, &mut MessageCtx<'_>) -> Effect>;
type TrayHandler = Box<dyn FnMut(TrayId, TrayEvent, &mut MessageCtx<'_>) -> Effect>;

impl Host {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        let mut backend = backend;
        let waker = backend
            .waker()
            .unwrap_or_else(|| Arc::new(NoWaker) as Arc<dyn Waker>);
        Self {
            jobs: Jobs::new(waker),
            completed_seen: 0,
            backend,
            surfaces: Vec::new(),
            next_surface: 0,
            quit: false,
            stats: RunStats::default(),
            channel: None,
            outputs: Vec::new(),
            deadline: None,
            pending_draw: false,
            on_turn: None,
            on_hotkey: None,
            on_tray: None,
            next_tray: 0,
        }
    }

    /// The process's worker. A job that finishes wakes the loop and forces one frame,
    /// so an app may read the result in `view`.
    pub fn jobs(&self) -> Jobs {
        self.jobs.clone()
    }

    /// Serve the process's local channel: every message goes to `handler`, which answers
    /// through the reply path it is given.
    pub fn listen(
        &mut self,
        name: &str,
        handler: impl FnMut(&mut dyn Reply, &[u8], &mut MessageCtx<'_>) -> Effect + 'static,
    ) -> Result<(), String> {
        let channel = self.backend.listen(name)?;
        self.channel = Some((channel, Box::new(handler)));
        Ok(())
    }

    /// Wake the loop at `at` with nothing arriving; `None` drops the deadline.
    pub fn set_deadline(&mut self, at: Option<Instant>) {
        self.deadline = at;
    }

    /// The outputs as of the last turn.
    pub fn outputs(&self) -> &[OutputInfo] {
        &self.outputs
    }

    /// The user's preferred UI languages, most preferred first, as BCP-47 tags.
    ///
    /// Read from the backend rather than the environment: a platform that keeps the list in
    /// its own settings has no locale variables to read.
    pub fn languages(&self) -> Vec<String> {
        self.backend.system_languages()
    }

    /// Called once per turn: the application's own periodic work (an idle timeout, a pin a
    /// surface handed over). The context is the same one a message handler gets, because the
    /// work may open a surface.
    pub fn on_turn(
        &mut self,
        callback: impl FnMut(&mut MessageCtx<'_>, &mut LoopCtx<'_>) + 'static,
    ) {
        self.on_turn = Some(Box::new(callback));
    }

    /// A context for work between turns — startup and reload bind hotkeys through it, as a
    /// handler would.
    pub fn context(&mut self) -> MessageCtx<'_> {
        MessageCtx {
            outputs: &self.outputs,
            backend: &mut *self.backend,
            surfaces: &mut self.surfaces,
            next_surface: &mut self.next_surface,
            pending_draw: &mut self.pending_draw,
        }
    }

    /// Called for each hotkey the backend reports, with the same context a channel handler
    /// gets: a hotkey starts a capture, which opens surfaces and reads the output list.
    pub fn on_hotkey(
        &mut self,
        handler: impl FnMut(ShortcutId, &mut MessageCtx<'_>) -> Effect + 'static,
    ) {
        self.on_hotkey = Some(Box::new(handler));
    }

    /// Open a surface on the host's connection; the loop ticks it from then on.
    pub fn open<A: App + 'static>(
        &mut self,
        spec: SurfaceSpec,
        app: A,
        options: OpenOptions,
    ) -> Result<SurfaceId, String> {
        let handle = self.backend.open(spec, Box::new(app), options)?;
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        self.surfaces.push((id, handle));
        Ok(id)
    }

    /// Put an icon in the system tray, visible at once.
    ///
    /// The returned handle is the app's; dropping it takes the icon out of the tray. The
    /// platform's reason comes back when it has no tray at all or refuses the icon.
    pub fn tray(&mut self, icon: Icon) -> Result<Tray, String> {
        let id = TrayId::from_u64(self.next_tray);
        let handle = self.backend.create_tray(id, icon)?;
        self.next_tray += 1;
        Ok(Tray::new(id, handle))
    }

    /// Called for each tray event the backend reports, with the same context a hotkey
    /// handler gets: "Open" opens a surface, "Quit" stops the loop.
    ///
    /// A right-click both opens the icon's menu and reports [`TrayEvent::Click`], so a report
    /// is not news that the menu stayed shut.
    pub fn on_tray(
        &mut self,
        handler: impl FnMut(TrayId, TrayEvent, &mut MessageCtx<'_>) -> Effect + 'static,
    ) {
        self.on_tray = Some(Box::new(handler));
    }

    /// Ask the loop to stop after the current turn.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    /// Run until `quit`, or until the backend says so. Works with no surface at all, which
    /// is what a daemon does while it waits.
    pub fn run(&mut self) -> Result<RunStats, String> {
        self.pump(None, false)
    }

    /// Run until `until`, until no surface is left, or until `quit`.
    ///
    /// This is the one-shot form: a command opens what it needs, runs the loop and exits when
    /// the screen is clear again.
    pub fn run_until(&mut self, until: Option<Instant>) -> Result<RunStats, String> {
        self.pump(until, true)
    }

    /// `exit_when_empty` separates a command from a daemon: the first ends when its last
    /// surface closes, the second keeps waiting with none.
    fn pump(&mut self, until: Option<Instant>, exit_when_empty: bool) -> Result<RunStats, String> {
        loop {
            if until.is_some_and(|at| Instant::now() >= at) {
                break;
            }
            // A turn first: the first frame must not wait for an event that may never come.
            self.turn()?;
            self.backend.flush();
            if self.stopped() || (exit_when_empty && self.surfaces.is_empty()) {
                if self.jobs.outstanding() == 0 {
                    break;
                }
                // A window that closed while a save ran must not take the save with it.
                // The worker's wake ends this wait early; the timeout covers a platform
                // with no waker at all.
                self.backend.wait(Some(Duration::from_millis(50)))?;
                continue;
            }
            // A surface opened or textured during the turn has no frame yet. Waiting here
            // would hold it until unrelated input arrived, so turn once more instead.
            if std::mem::take(&mut self.pending_draw) {
                continue;
            }
            // Everything that had already arrived is done: only now is a sleep correct.
            let timeout = self.timeout(until);
            self.backend.wait(timeout)?;
        }
        Ok(self.stats)
    }

    fn stopped(&self) -> bool {
        self.quit || self.backend.should_quit()
    }

    /// The nearest deadline the loop must wake for, clamped by `until`.
    fn timeout(&self, until: Option<Instant>) -> Option<Duration> {
        let now = Instant::now();
        let mut next = until;
        if let Some(at) = self.deadline {
            next = Some(next.map_or(at, |current| current.min(at)));
        }
        if let Some(at) = self.backend.next_deadline() {
            next = Some(next.map_or(at, |current| current.min(at)));
        }
        for (_, surface) in &self.surfaces {
            if let Some(at) = surface.next_deadline() {
                next = Some(next.map_or(at, |current| current.min(at)));
            }
        }
        next.map(|at| at.saturating_duration_since(now))
    }

    /// One non-blocking pass: step every surface, hand over its copies, serve the clipboard.
    ///
    /// A surface that fails is retired and the loop carries on. One lost swap chain must not
    /// take the tray, the hotkeys and every other window with it — only [`Backend::wait`]
    /// still ends the process, because with no wait left there is nothing to run.
    fn turn(&mut self) -> Result<(), String> {
        let mut copies = Vec::new();
        let mut retired = Vec::new();
        self.stats.turns += 1;
        // A job finished between turns: its result is app state, and only a build reads it.
        let completed = self.jobs.completed();
        if completed != self.completed_seen {
            self.completed_seen = completed;
            for (_, surface) in &mut self.surfaces {
                surface.request_frame();
            }
        }
        // Before anything answers a command: a status reply or a pin placement must not use
        // yesterday's outputs.
        self.outputs = self.backend.outputs();
        for (id, surface) in &mut self.surfaces {
            match surface.step() {
                Ok(step) => {
                    if step.drew {
                        self.stats.frames += 1;
                    }
                    copies.extend(surface.take_copies());
                }
                Err(reason) => retired.push((*id, reason)),
            }
        }
        for copy in copies {
            self.backend.set_clipboard_copy(copy);
        }
        if let Err(reason) = self.backend.serve_clipboard() {
            // A clipboard the desktop refused is not the desktop going away; the next turn
            // serves it again, and `wait` reports a connection that is really gone.
            log::warn!("ui-platform: the clipboard was not served: {reason}");
        }
        for (id, reason) in &retired {
            log::warn!("ui-platform: surface {id:?} was retired: {reason}");
        }
        self.surfaces.retain(|(id, surface)| {
            !retired.iter().any(|(retired, _)| retired == id) && !surface.is_closed()
        });
        self.serve_hotkeys();
        self.serve_tray();
        self.serve_channel();
        self.serve_turn();
        Ok(())
    }

    /// Hand every hotkey the backend queued to the application's handler.
    fn serve_hotkeys(&mut self) {
        let Some(handler) = self.on_hotkey.as_mut() else {
            return;
        };
        // The ids are copied out so the backend borrow ends before the handler can open a
        // surface or read the output list through the same backend.
        let pressed = match self.backend.hotkeys() {
            Some(hotkeys) => hotkeys.poll(),
            None => return,
        };
        let mut quit = false;
        for id in pressed {
            let mut context = MessageCtx {
                outputs: &self.outputs,
                backend: &mut *self.backend,
                surfaces: &mut self.surfaces,
                next_surface: &mut self.next_surface,
                pending_draw: &mut self.pending_draw,
            };
            if handler(id, &mut context) == Effect::Quit {
                quit = true;
            }
        }
        self.quit |= quit;
    }

    /// Hand every tray event the backend queued to the application's handler.
    ///
    /// Drained even with no handler set: an app may create its icon before it decides what a
    /// click does, and a queue nobody empties would grow for the life of the process.
    fn serve_tray(&mut self) {
        let reports = self.backend.tray_events();
        let Some(handler) = self.on_tray.as_mut() else {
            return;
        };
        let mut quit = false;
        for (id, event) in reports {
            let mut context = MessageCtx {
                outputs: &self.outputs,
                backend: &mut *self.backend,
                surfaces: &mut self.surfaces,
                next_surface: &mut self.next_surface,
                pending_draw: &mut self.pending_draw,
            };
            if handler(id, event, &mut context) == Effect::Quit {
                quit = true;
            }
        }
        self.quit |= quit;
    }

    /// The application's own work for this turn, with the loop's context beside it.
    fn serve_turn(&mut self) {
        let Host {
            backend,
            surfaces,
            next_surface,
            outputs,
            quit,
            deadline,
            pending_draw,
            on_turn,
            ..
        } = self;
        let Some(callback) = on_turn.as_mut() else {
            return;
        };
        let mut context = MessageCtx {
            outputs,
            backend: &mut **backend,
            surfaces,
            next_surface,
            pending_draw,
        };
        let mut loop_context = LoopCtx {
            quit,
            deadline,
            outputs,
        };
        callback(&mut context, &mut loop_context);
    }

    /// Answer whatever arrived on the local channel.
    fn serve_channel(&mut self) {
        let Host {
            channel,
            outputs,
            backend,
            surfaces,
            next_surface,
            quit: host_quit,
            pending_draw,
            ..
        } = self;
        let Some((channel, handler)) = channel.as_mut() else {
            return;
        };
        let mut quit = false;
        channel.poll(&mut |reply, bytes| {
            let mut context = MessageCtx {
                outputs,
                backend: &mut **backend,
                surfaces,
                next_surface,
                pending_draw,
            };
            if handler(reply, bytes, &mut context) == Effect::Quit {
                quit = true;
            }
        });
        *host_quit |= quit;
    }
}
