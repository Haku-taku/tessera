//! The process loop: one wait, every surface, every platform service.
//!
//! A `Host` owns the loop and the surfaces; the platform `Backend` owns the connection and the
//! native objects behind it. Nothing here names a protocol, a file descriptor or a platform.

use std::sync::Arc;

use ui_core::collections::FastSet;
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
use crate::view::{App, AppLoop};
use crate::window::{PopupSpec, SurfaceSpec};
use ui_core::layout::PlatformPanel;

/// The engine behind a group: the app, its declaration tree, its clock and its focus.
///
/// Boxed so a surface the host owns never names the application's type. A group may present
/// through more than one `SurfaceHandle`; the engine is what they share.
pub type Engine = AppLoop<Box<dyn App>>;

/// What a backend needs to open a presentation.
///
/// The app-side half of `OpenOptions` — textures, fonts, shortcuts — belongs to the engine
/// and is applied before the surface exists.
#[derive(Clone, Copy, Debug, Default)]
pub struct PresentationOptions {
    pub uncapped: bool,
}

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

/// One presentation as the loop sees it: a step that never waits, and when it must be woken.
///
/// The platform implements this for its surface type. `step` drains what has already
/// arrived from the engine it is given, decides, and draws only if a frame is due and the
/// surface can take one. The engine lives outside: the same tree may present through several
/// surfaces, and a presenter only knows the one it owns.
pub trait SurfaceHandle {
    /// Take what arrived and draw a frame for `engine` if one is due.
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String>;

    /// Whether a move or resize loop the platform started owns this surface right now.
    ///
    /// That loop dispatches this thread's messages until the pointer comes up, so a step
    /// would find nothing to do — and must not pump the queue, which the loop is reading.
    fn moving(&self) -> bool {
        false
    }

    /// Build a frame for a surface a move loop is moving; true when one was handed over.
    ///
    /// The loop dispatches only input and the timer it armed, so this tick is the surface's
    /// only chance to draw while it follows the pointer.
    fn pump_moved(&mut self, engine: &mut Engine) -> bool {
        let _ = engine;
        false
    }

    /// A texture was added to this surface's engine.
    ///
    /// Registration is the engine's, so the default is to do nothing; a presenter that
    /// caches textures may use this to know one is coming.
    fn add_image(&mut self, image: &ui_core::draw::ImageEntry) -> Result<(), String> {
        let _ = image;
        Ok(())
    }

    /// When this surface must be woken with no input arriving (a caret blink, a snapping
    /// scroller). `None` means it only moves when something happens.
    fn next_deadline(&self, engine: &Engine) -> Option<Instant>;

    /// The host asked the engine for a frame; a presenter that paces its own chain may act.
    fn frame_requested(&mut self) {}

    fn is_closed(&self) -> bool;

    /// Copies the last frame asked for, each with the serial it must be served under.
    ///
    /// The host owns the clipboard, so a surface hands over the request instead of serving
    /// it; paste requests are the surface's own business and never reach here.
    fn take_copies(&mut self, engine: &mut Engine) -> Vec<ClipboardCopy>;

    /// Open a surface of this one's own for the panel `node`, hung off it per `spec`.
    ///
    /// The panel's subtree is painted by the engine, and `node` is what the new surface
    /// hands back to [`AppLoop::present_panel`] each frame. The default is a presentation
    /// with no popups, and the host then paints the panel in this surface: a panel must
    /// never disappear because a surface could not be opened.
    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        spec: PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let _ = (node, spec);
        Err("this presentation has no popup surfaces".into())
    }

    /// Move or resize a popup this surface opened, per `spec`.
    ///
    /// The default keeps the surface where it is: a platform that cannot reposition a popup
    /// closes and reopens it instead.
    fn reconfigure(&mut self, spec: PopupSpec) -> Result<(), String> {
        let _ = spec;
        Ok(())
    }
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

/// A surface, its id, and the engine it presents, with the panels it put on surfaces of
/// their own.
struct LiveSurface {
    id: SurfaceId,
    root: Box<dyn SurfaceHandle>,
    panels: Vec<PanelBinding>,
    /// Panels whose surface the platform took back — a click outside, Escape. The panel
    /// still being declared does not reopen it: the app is about to undeclare it.
    dismissed: FastSet<ui_core::NodeId>,
    /// Panels the platform refused a surface for. They are painted by their parent, and
    /// asking again every frame would only fail every frame.
    refused: FastSet<ui_core::NodeId>,
    engine: Engine,
}

/// One panel of a group that the platform shows in a surface of its own.
struct PanelBinding {
    node: ui_core::NodeId,
    /// What the platform was last told: a different request means the panel moved or
    /// resized, and the surface must follow.
    spec: PopupSpec,
    handle: Box<dyn SurfaceHandle>,
}

/// What the platform is asked to show for a laid-out panel.
fn popup_spec(panel: &PlatformPanel) -> PopupSpec {
    PopupSpec {
        rect: panel.rect,
        anchor: panel.anchor,
        side: panel.side,
        align: panel.align,
        offset: panel.offset,
        constraint: crate::window::Constraint::default(),
    }
}

/// The nearest panel an inner one hangs off, so a nested panel parents the outer one.
fn panel_parent(tree: &ui_core::tree::Tree, node: ui_core::NodeId) -> Option<ui_core::NodeId> {
    let mut cursor = tree.get(node)?.parent;
    while let Some(nid) = cursor {
        if tree.platform_panels().iter().any(|panel| panel.node == nid) {
            return Some(nid);
        }
        cursor = tree.get(nid)?.parent;
    }
    None
}

/// Build the engine a surface presents for: the app plus everything the open declared
/// before its first frame.
fn engine_for(app: Box<dyn App>, options: &mut OpenOptions) -> Result<Engine, String> {
    let mut engine = AppLoop::with_stack(app, &[]);
    for image in options.images.drain(..) {
        engine.register_image(image);
    }
    for face in options.fonts.drain(..) {
        if !engine.register_font(&face.name, face.data) {
            return Err(format!("the font {} could not be registered", face.name));
        }
    }
    for (chord, id) in options.shortcuts.drain(..) {
        engine.bind_shortcut(chord, id);
    }
    Ok(engine)
}

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
        mut options: OpenOptions,
    ) -> Result<SurfaceId, String> {
        let uncapped = options.uncapped;
        let engine = engine_for(app, &mut options)?;
        let handle = self.backend.open(spec, PresentationOptions { uncapped })?;
        let id = SurfaceId(*self.next_surface);
        *self.next_surface += 1;
        // The surface has not been stepped: the turn that opened it is already past the
        // surfaces, so without this the loop would sleep on a surface nobody drew.
        *self.pending_draw = true;
        self.surfaces.push(LiveSurface {
            id,
            root: handle,
            panels: Vec::new(),
            dismissed: FastSet::default(),
            refused: FastSet::default(),
            engine,
        });
        Ok(id)
    }

    /// Add a texture to a surface this handler opened.
    pub fn add_image(
        &mut self,
        id: SurfaceId,
        image: ui_core::draw::ImageEntry,
    ) -> Result<(), String> {
        let group = self
            .surfaces
            .iter_mut()
            .find(|group| group.id == id)
            .ok_or("no such surface")?;
        group.engine.register_image(image.clone());
        group.root.add_image(&image)?;
        // Registering a texture changes what a running surface shows only when it builds
        // again: the app put the node that draws it into its own state, and nothing else
        // will make the surface draw.
        group.engine.request_frame();
        group.root.frame_requested();
        *self.pending_draw = true;
        Ok(())
    }

    /// Have `id` build a frame on the next turn.
    ///
    /// A request that changed what a running surface draws reaches the app only through
    /// its next build; without this the old frame stays on screen until input arrives.
    pub fn request_frame(&mut self, id: SurfaceId) -> Result<(), String> {
        let group = self
            .surfaces
            .iter_mut()
            .find(|group| group.id == id)
            .ok_or("no such surface")?;
        group.engine.request_frame();
        group.root.frame_requested();
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
            .any(|group| group.id == id && !group.root.is_closed())
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
    /// Open one presentation. The engine is the host's; a backend never holds an app.
    fn open(
        &mut self,
        spec: SurfaceSpec,
        options: PresentationOptions,
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

/// Step each group once: a turn's work for it, without touching the backend.
///
/// Returns the copies each group finished and the ones that failed, so a caller that
/// cannot hold the backend — one inside a wait, stepped from a modal loop — can hand them
/// over afterwards through [`Host::absorb`].
fn step_surfaces(
    surfaces: &mut [LiveSurface],
    stats: &mut RunStats,
) -> (Vec<ClipboardCopy>, Vec<(SurfaceId, String)>) {
    let mut copies = Vec::new();
    let mut retired = Vec::new();
    for group in surfaces {
        let id = group.id;
        // A surface the platform's own move loop is moving draws from that loop's tick and
        // nowhere else: a step here would pump the queue the loop is reading.
        if group.root.moving() {
            if group.root.pump_moved(&mut group.engine) {
                stats.frames += 1;
            }
            step_panels(group, stats, &mut copies);
            continue;
        }
        match group.root.step(&mut group.engine) {
            Ok(step) => {
                if step.drew {
                    stats.frames += 1;
                }
                copies.extend(group.root.take_copies(&mut group.engine));
                reconcile_panels(group);
                step_panels(group, stats, &mut copies);
            }
            Err(reason) => retired.push((id, reason)),
        }
    }
    (copies, retired)
}

/// Give every panel the last frame laid out a surface, and let go of the ones that left.
///
/// The frame's panel list is the truth: a binding survives only while its panel does. A
/// platform that refuses a surface does not lose the panel — the engine is told to paint
/// it in its parent instead.
fn reconcile_panels(group: &mut LiveSurface) {
    let declared: Vec<PlatformPanel> = group.engine.platform_panels().to_vec();
    group
        .panels
        .retain(|binding| declared.iter().any(|panel| panel.node == binding.node));
    // A panel that left the tree is not dismissed any more: declaring it again opens it.
    group
        .dismissed
        .retain(|node| declared.iter().any(|panel| panel.node == *node));
    group
        .refused
        .retain(|node| declared.iter().any(|panel| panel.node == *node));
    for panel in &declared {
        if group.dismissed.contains(&panel.node) || group.refused.contains(&panel.node) {
            continue;
        }
        let spec = popup_spec(panel);
        if let Some(index) = group
            .panels
            .iter()
            .position(|binding| binding.node == panel.node)
        {
            if group.panels[index].spec == spec {
                continue;
            }
            // A nested panel's surface is placed against the outer popup, not the window.
            let presented = match panel_frame(group, panel.node) {
                Some(origin) => rebase(spec, origin),
                None => spec,
            };
            match group.panels[index].handle.reconfigure(presented) {
                Ok(()) => {
                    group.panels[index].spec = spec;
                    continue;
                }
                // A positioner fixes a popup's size when it maps: the surface has to be
                // made again at the new request, not moved to it.
                Err(reason) => {
                    log::warn!("ui-platform: a panel could not be moved: {reason}");
                    group.panels.remove(index);
                }
            }
        }
        match open_panel(group, panel.node, spec) {
            Ok(handle) => {
                group.engine.set_panel_presentation(panel.node, true);
                group.panels.push(PanelBinding {
                    node: panel.node,
                    spec,
                    handle,
                });
            }
            Err(reason) => {
                log::warn!("ui-platform: a panel has no surface: {reason}");
                group.engine.set_panel_presentation(panel.node, false);
                group.refused.insert(panel.node);
            }
        }
    }
}

/// Ask the surface that holds `node` for one of the panel's own.
fn open_panel(
    group: &mut LiveSurface,
    node: ui_core::NodeId,
    spec: PopupSpec,
) -> Result<Box<dyn SurfaceHandle>, String> {
    match panel_parent(group.engine.tree(), node) {
        // A panel inside a panel hangs off the outer one, so the topmost holds the grab.
        Some(outer) => {
            let Some(origin) = panel_frame(group, node) else {
                return Err("the panel that holds it has no rect".into());
            };
            match group
                .panels
                .iter_mut()
                .find(|binding| binding.node == outer)
            {
                Some(binding) => binding.handle.open_popup(node, rebase(spec, origin)),
                None => Err("the panel that holds it has no surface".into()),
            }
        }
        None => group.root.open_popup(node, spec),
    }
}

/// Where a panel's request is anchored: a nested panel hangs off the outer panel's surface,
/// which is placed away from the window, so its coordinates start there.
fn panel_frame(group: &LiveSurface, node: ui_core::NodeId) -> Option<ui_core::geom::Point> {
    let outer = panel_parent(group.engine.tree(), node)?;
    group
        .engine
        .platform_panels()
        .iter()
        .find(|panel| panel.node == outer)
        .map(|panel| panel.rect.origin)
}

/// The same request, with both rectangles moved into a parent panel's coordinates.
fn rebase(spec: PopupSpec, origin: ui_core::geom::Point) -> PopupSpec {
    let moved = |rect: ui_core::geom::Rect| {
        ui_core::geom::Rect::new(
            rect.origin.x - origin.x,
            rect.origin.y - origin.y,
            rect.size.w,
            rect.size.h,
        )
    };
    PopupSpec {
        rect: moved(spec.rect),
        anchor: moved(spec.anchor),
        ..spec
    }
}

/// Let each panel show the part of the frame the engine painted for it.
fn step_panels(group: &mut LiveSurface, stats: &mut RunStats, copies: &mut Vec<ClipboardCopy>) {
    let mut failed = Vec::new();
    let mut closed = Vec::new();
    for binding in &mut group.panels {
        // A panel bound after this turn's build has nothing to show yet; the build that
        // bound it also asked for the frame that will have it.
        if !group.engine.panel_painted(binding.node) {
            continue;
        }
        let step = binding.handle.step(&mut group.engine);
        if binding.handle.is_closed() {
            // The platform ended the panel — a click outside, Escape. Painting it in the
            // window now would flash it where it never was, so it stays bound-but-gone,
            // and the app is told so it can close what it declared.
            group.dismissed.insert(binding.node);
            group.engine.dismiss_panel(binding.node);
            closed.push(binding.node);
            continue;
        }
        match step {
            Ok(step) => {
                if step.drew {
                    stats.frames += 1;
                }
                copies.extend(binding.handle.take_copies(&mut group.engine));
            }
            Err(reason) => {
                log::warn!("ui-platform: a panel surface failed: {reason}");
                failed.push(binding.node);
            }
        }
    }
    for node in &failed {
        // The panel is not lost: the parent paints it from the next frame on.
        group.engine.set_panel_presentation(*node, false);
    }
    group.panels.retain(|binding| {
        !failed.contains(&binding.node) && !closed.contains(&binding.node)
    });
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
        mut options: OpenOptions,
    ) -> Result<SurfaceId, String> {
        let uncapped = options.uncapped;
        let engine = engine_for(Box::new(app), &mut options)?;
        let handle = self.backend.open(spec, PresentationOptions { uncapped })?;
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        self.surfaces.push(LiveSurface {
            id,
            root: handle,
            panels: Vec::new(),
            dismissed: FastSet::default(),
            refused: FastSet::default(),
            engine,
        });
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
            // A modal gesture can begin in the wait itself — a title-bar press arrives as
            // a message, not as a turn — so the rest of the loop is armed here too. The
            // shell's move loop dispatches nothing but input and `WM_TIMER`, and that
            // timer is what reaches this hook.
            // What the modal loop's steps produce is collected here and handed over once the
            // wait is over: the backend it would be handed to is the one being waited on.
            let mut copies = Vec::new();
            let mut retired = Vec::new();
            let waited = {
                let surfaces = &mut self.surfaces;
                let stats = &mut self.stats;
                let mut others = || {
                    let (stepped_copies, stepped_retired) = step_surfaces(surfaces, stats);
                    copies.extend(stepped_copies);
                    retired.extend(stepped_retired);
                };
                crate::modal::arm(&mut others);
                let waited = self.backend.wait(timeout);
                crate::modal::disarm();
                waited
            };
            self.absorb(copies, retired);
            waited?;
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
        for group in &self.surfaces {
            // A frame the engine owes but no surface has built yet: turning now is what
            // shows it, and nothing else is coming to wake the wait.
            if group.engine.frame_due() {
                next = Some(now);
            }
            if let Some(at) = group.root.next_deadline(&group.engine) {
                next = Some(next.map_or(at, |current| current.min(at)));
            }
            for binding in &group.panels {
                if let Some(at) = binding.handle.next_deadline(&group.engine) {
                    next = Some(next.map_or(at, |current| current.min(at)));
                }
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
            for group in &mut self.surfaces {
                group.engine.request_frame();
                group.root.frame_requested();
            }
        }
        // Before anything answers a command: a status reply or a pin placement must not use
        // yesterday's outputs.
        self.outputs = self.backend.outputs();
        // A step may start a modal gesture — a title-bar move, an OLE drag — whose message
        // loop does not return until the pointer is released. The surface is taken out of
        // the list while its step runs, so the gesture's own timer can reach the rest of
        // the process through [`modal::pump_others`](crate::modal::pump_others).
        let mut index = 0;
        while index < self.surfaces.len() {
            let mut group = self.surfaces.remove(index);
            let mut others = || self.pump_others();
            crate::modal::arm(&mut others);
            let stepped = group.root.step(&mut group.engine);
            crate::modal::disarm();
            match stepped {
                Ok(step) => {
                    if step.drew {
                        self.stats.frames += 1;
                    }
                    copies.extend(group.root.take_copies(&mut group.engine));
                    // The frame just built names the panels: bind what is new, move what
                    // moved, and let each panel show its own part of it.
                    reconcile_panels(&mut group);
                    step_panels(&mut group, &mut self.stats, &mut copies);
                }
                Err(reason) => retired.push((group.id, reason)),
            }
            self.surfaces.insert(index, group);
            index += 1;
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
        self.surfaces.retain(|group| {
            !retired.iter().any(|(retired, _)| retired == &group.id) && !group.root.is_closed()
        });
        self.serve_hotkeys();
        self.serve_tray();
        self.serve_channel();
        self.serve_turn();
        Ok(())
    }

    /// Step every surface but the one a modal gesture is running for.
    ///
    /// The gesture's own message loop dispatches this thread's messages, so the only thing
    /// missing is the engine's turn: this is that turn without waiting and without the
    /// gesture's surface, which the platform draws from its own timer. Copies still reach
    /// the clipboard; a failing surface is retired here exactly as a turn would retire it.
    fn pump_others(&mut self) {
        let (copies, retired) = step_surfaces(&mut self.surfaces, &mut self.stats);
        self.absorb(copies, retired);
    }

    /// Hand over what a step of every surface produced, exactly as a turn would.
    ///
    /// Kept apart from the step so a caller holding the backend — one inside a wait — can
    /// run the step from a modal loop and still deliver what it collected.
    fn absorb(&mut self, copies: Vec<ClipboardCopy>, retired: Vec<(SurfaceId, String)>) {
        for copy in copies {
            self.backend.set_clipboard_copy(copy);
        }
        for (id, reason) in &retired {
            log::warn!("ui-platform: surface {id:?} was retired: {reason}");
        }
        self.surfaces.retain(|group| {
            !retired.iter().any(|(retired, _)| retired == &group.id) && !group.root.is_closed()
        });
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
