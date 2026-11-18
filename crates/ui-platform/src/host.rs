//! The process loop: one wait, every surface, every platform service.
//!
//! A `Host` owns the loop and the surfaces; the platform `Backend` owns the connection and the
//! native objects behind it. Nothing here names a protocol, a file descriptor or a platform.

use std::sync::Arc;

use ui_core::collections::FastSet;
use std::time::{Duration, Instant};

use ui_core::draw::ImageEntry;
use ui_core::shortcut::{KeyChord, ShortcutId};

use crate::chooser::{
    ChooserAnswer, ChooserCapabilities, ChooserError, ChooserId, ChooserRequest,
};
use crate::clipboard::ClipboardContent;
use crate::clipboard::{ClipboardCopy, ClipboardKind};
use crate::hotkeys::{GlobalHotkeyError, GlobalHotkeys};
use crate::ipc::{Channel, Reply};
use crate::file_icons::{FileIconProvider, FileIcons};
use crate::file_operation::{FileOperations, FileOperator};
use crate::jobs::{Jobs, Waker};
use crate::listing::{DirectoryLister, Listings};
use crate::output::OutputInfo;
use crate::places::{Device, Place};
use crate::search::{DirectorySearcher, Searches};
use crate::shell_menu::ShellMenuHost;
use crate::transfer::{Selection, SelectionReader};
use crate::tray::{Icon, Tray, TrayEvent, TrayHandle, TrayId};
use crate::thumbnails::{Thumbnailer, Thumbnails};
use crate::view::{App, AppLoop};
use crate::volumes::Volumes;
use crate::watch::{DirectoryWatcher, Watches};
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

    /// Read what the platform delivered for this surface and hand it to `engine`, without
    /// drawing.
    ///
    /// This is the pass a group makes over its panels before it builds a frame: the
    /// pointer crossing from a window onto a panel leaves one surface and enters another
    /// in the same batch, and only the panel's queue holds the half that says where it
    /// went. `step` calls it too, so a surface that has already taken its input this turn
    /// finds nothing left.
    fn intake(&mut self, engine: &mut Engine) -> Result<(), String> {
        let _ = engine;
        Ok(())
    }

    /// Where the pointer stands on this surface, in the tree's coordinates, when it is on it.
    ///
    /// A surface has a pointer only while the pointer is over it; the host asks every
    /// surface of a group so the frame they share is built with the one pointer they share.
    fn pointer(&self) -> Option<ui_core::geom::Point> {
        None
    }

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

    /// Open a window of this one's own for the panel `node`.
    ///
    /// Unlike a popup it carries the platform's frame and takes the keyboard like any
    /// window, and it stays until the app stops declaring the panel. `title` is the app's
    /// localized name for it. A surface whose platform has no windows returns the refusal,
    /// and the host falls back to a popup and then to painting the panel in this surface.
    fn open_panel_window(
        &mut self,
        node: ui_core::NodeId,
        spec: PopupSpec,
        title: &str,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let _ = (node, spec, title);
        Err("this presentation has no window surfaces".into())
    }

    /// Move or resize a popup this surface opened, per `spec`.
    ///
    /// The default keeps the surface where it is: a platform that cannot reposition a popup
    /// closes and reopens it instead.
    fn reconfigure(&mut self, spec: PopupSpec) -> Result<(), String> {
        let _ = spec;
        Ok(())
    }

    /// Open a dialog window belonging to this surface, per `spec`.
    ///
    /// The dialog is a surface of its own with an engine of its own; the platform stacks it
    /// above this one, and closes it when this one goes. A modal dialog's input filtering is
    /// enforced by the host, not only asked of the platform: Wayland's `xdg-dialog` is a
    /// hint and tells the client to filter the parent's events itself.
    ///
    /// The default is a presentation with no dialog windows. The host then reports the
    /// refusal to the caller, which can present the same content in its own window.
    fn open_dialog(
        &mut self,
        spec: SurfaceSpec,
        options: PresentationOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let _ = (spec, options);
        Err("this presentation has no dialog surfaces".into())
    }

    /// What this surface's platform offers as a native file chooser, when it offers one.
    ///
    /// `None` means the platform has no chooser for this surface — the drawn picker is the
    /// only implementation there. The capabilities are read before every request, so a
    /// backend that loses its portal service simply stops answering here.
    fn chooser_capabilities(&self) -> Option<ChooserCapabilities> {
        None
    }

    /// Start a native file chooser parented to this surface.
    ///
    /// A backend that can answer at once may return the answer through
    /// [`chooser_events`](Self::chooser_events) in the same call; one that cannot (a
    /// portable portal dialog, a Windows dialog that runs its own message loop) queues it
    /// and returns. `Ok` means the chooser is running, and the host now expects exactly
    /// one event carrying `id`. An `Err` means nothing started and no event will come.
    fn choose(
        &mut self,
        id: ChooserId,
        request: ChooserRequest,
    ) -> Result<(), ChooserError> {
        let _ = (id, request);
        Err(ChooserError::Unavailable(
            "this presentation has no native file chooser".into(),
        ))
    }

    /// Answers that finished since the last turn, each exactly once.
    fn chooser_events(&mut self) -> Vec<(ChooserId, ChooserAnswer)> {
        Vec::new()
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
    /// The fallback families text resolves through, in order: the same stack an `AppLoop`
    /// takes at construction. A surface opened through the host lays its text out with these,
    /// so a build that needs a CJK face does not silently fall to the host's default.
    pub stack: Vec<String>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId(u64);

/// The surface a dialog belongs to, and whether it holds that surface's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DialogLink {
    parent: SurfaceId,
    modal: bool,
}

/// A native chooser that is running, and the one callback its answer may reach.
struct PendingChooser {
    id: ChooserId,
    /// The surface whose input the chooser holds, and whose life the chooser shares.
    parent: SurfaceId,
    done: Option<ChooserCallback>,
}

/// What a message handler may ask of the loop: the outputs it can see, and a surface.
///
/// A command that arrives over the channel has no surface of its own — a daemon holds none
/// until a pin asks for one — so answering it needs the loop's own door, not just the reply.
pub struct MessageCtx<'a> {
    outputs: &'a [OutputInfo],
    backend: &'a mut dyn Backend,
    surfaces: &'a mut Vec<LiveSurface>,
    next_surface: &'a mut u64,
    /// Native choosers this process is waiting for; the answer is delivered by the host.
    choosers: &'a mut ChooserQueue,
    /// A surface appeared or took a texture while the loop was turning. It has drawn
    /// nothing yet, so the loop must turn again before it blocks.
    pending_draw: &'a mut bool,
}

/// The callback a chooser's answer reaches, on the loop's own thread.
///
/// It gets no context on purpose: a drawn picker answers from its own `App`, where no
/// message door exists, and the two implementations must be indistinguishable to a caller.
/// Work that needs the loop — opening the next dialog — belongs to the app's next turn,
/// which sees the answer in its own state.
pub type ChooserCallback = Box<dyn FnOnce(ChooserAnswer)>;

/// Native choosers waiting for an answer, and the ids they were given.
///
/// The queue lives in the host, not in a surface: a platform may run the dialog on a thread
/// of its own, and the answer must come back through the surface that owns it, never
/// through whichever surface happened to be stepped first.
#[derive(Default)]
struct ChooserQueue {
    next: u64,
    pending: Vec<PendingChooser>,
}

impl ChooserQueue {
    fn next_id(&mut self) -> ChooserId {
        let id = ChooserId::from_u64(self.next);
        self.next += 1;
        id
    }

    fn take(&mut self, id: ChooserId) -> Option<PendingChooser> {
        let index = self.pending.iter().position(|pending| pending.id == id)?;
        Some(self.pending.remove(index))
    }
}

/// A surface, its id, and the engine it presents, with the panels it put on surfaces of
/// their own.
struct LiveSurface {
    id: SurfaceId,
    root: Box<dyn SurfaceHandle>,
    /// The surface this one is a dialog of, when it is one.
    dialog: Option<DialogLink>,
    /// A modal dialog of this surface is open: it paints, but takes no input.
    blocked: bool,
    /// A native chooser owned by this surface is running. It is not a dialog surface the
    /// host steps, so its input hold is remembered here and folded into `blocked` each turn.
    native_blocked: bool,
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
fn popup_spec(panel: &PlatformPanel, grid: f32) -> PopupSpec {
    PopupSpec {
        rect: crate::window::snap_surface_rect(panel.rect, grid),
        anchor: snap_panel(panel.anchor, grid),
        side: panel.side,
        align: panel.align,
        offset: panel.offset,
        constraint: crate::window::Constraint::default(),
    }
}

/// The anchor moved onto the whole pixels its surface is rasterized on, keeping its extent.
fn snap_panel(rect: ui_core::geom::Rect, grid: f32) -> ui_core::geom::Rect {
    let origin = crate::window::snap_surface_origin(rect.origin, grid);
    ui_core::geom::Rect::new(origin.x, origin.y, rect.size.w, rect.size.h)
}

/// The nearest panel an inner one hangs off, so a nested panel parents the outer one.
///
/// A window panel holds nothing: it is a top-level of its own, and a panel declared inside
/// one still belongs to the surface behind it.
fn panel_parent(tree: &ui_core::tree::Tree, node: ui_core::NodeId) -> Option<ui_core::NodeId> {
    let mut cursor = tree.get(node)?.parent;
    while let Some(nid) = cursor {
        if tree
            .platform_panels()
            .iter()
            .any(|panel| panel.node == nid && !panel.window)
        {
            return Some(nid);
        }
        cursor = tree.get(nid)?.parent;
    }
    None
}

/// Build the engine a surface presents for: the app plus everything the open declared
/// before its first frame.
fn engine_for(
    app: Box<dyn App>,
    options: &mut OpenOptions,
    timing: PointerTiming,
) -> Result<Engine, String> {
    let stack: Vec<&str> = options.stack.iter().map(String::as_str).collect();
    let mut engine = AppLoop::with_stack(app, &stack);
    engine.set_pointer_timing(timing);
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
    // The loop reconciles a panel binding for every frame it presents, so its windows can
    // show a panel on a surface of their own; an engine built without it keeps panels inside
    // the window, where nobody would otherwise paint them.
    engine.set_panel_surfaces(true);
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
        let timing = self.backend.pointer_timing();
        let engine = engine_for(app, &mut options, timing)?;
        let handle = self.backend.open(spec, PresentationOptions { uncapped })?;
        let id = SurfaceId(*self.next_surface);
        *self.next_surface += 1;
        // The surface has not been stepped: the turn that opened it is already past the
        // surfaces, so without this the loop would sleep on a surface nobody drew.
        *self.pending_draw = true;
        self.surfaces.push(LiveSurface {
            id,
            root: handle,
            dialog: None,
            blocked: false,
            native_blocked: false,
            panels: Vec::new(),
            dismissed: FastSet::default(),
            refused: FastSet::default(),
            engine,
        });
        Ok(id)
    }

    /// Open a dialog window belonging to `parent`; the loop ticks it from then on.
    ///
    /// The dialog's engine is its own, like every surface's. The parent stays on screen and
    /// keeps painting; while a modal dialog is open the host drops the parent's input, and
    /// the platform is asked for the same where it can express it.
    pub fn open_dialog(
        &mut self,
        parent: SurfaceId,
        spec: SurfaceSpec,
        app: Box<dyn App>,
        mut options: OpenOptions,
    ) -> Result<SurfaceId, String> {
        let modal = spec
            .dialog_modal()
            .ok_or("a dialog surface is opened with SurfaceSpec::dialog")?;
        let index = self
            .surfaces
            .iter()
            .position(|group| group.id == parent)
            .ok_or("no such surface")?;
        let uncapped = options.uncapped;
        let timing = self.backend.pointer_timing();
        let engine = engine_for(app, &mut options, timing)?;
        let handle = self.surfaces[index]
            .root
            .open_dialog(spec, PresentationOptions { uncapped })?;
        let id = SurfaceId(*self.next_surface);
        *self.next_surface += 1;
        // The dialog has not been stepped either, for the same reason a new surface has not.
        *self.pending_draw = true;
        self.surfaces.push(LiveSurface {
            id,
            root: handle,
            dialog: Some(DialogLink { parent, modal }),
            blocked: false,
            native_blocked: false,
            panels: Vec::new(),
            dismissed: FastSet::default(),
            refused: FastSet::default(),
            engine,
        });
        Ok(id)
    }

    /// What `parent`'s platform offers as a native chooser, if anything.
    pub fn chooser_capabilities(&self, parent: SurfaceId) -> Option<ChooserCapabilities> {
        self.surfaces
            .iter()
            .find(|group| group.id == parent)
            .and_then(|group| group.root.chooser_capabilities())
    }

    /// Ask the platform to run its own file chooser for `parent`.
    ///
    /// The request is checked against the provider's capabilities first: a feature the
    /// provider cannot honour is [`ChooserError::Unsupported`], so the caller can fall back
    /// to the drawn picker rather than accept a dialog missing what it asked for. On `Ok`
    /// the parent's input is held until the answer arrives, exactly like a modal dialog
    /// surface, and `done` runs once on the loop's thread — even when the parent closes
    /// first, where the answer is [`ChooserError::Cancelled`].
    pub fn choose_native(
        &mut self,
        parent: SurfaceId,
        request: ChooserRequest,
        done: impl FnOnce(ChooserAnswer) + 'static,
    ) -> Result<ChooserId, ChooserError> {
        request.validate().map_err(ChooserError::Failed)?;
        let index = self
            .surfaces
            .iter()
            .position(|group| group.id == parent)
            .ok_or_else(|| ChooserError::Failed("no such surface".to_string()))?;
        if self
            .choosers
            .pending
            .iter()
            .any(|pending| pending.parent == parent)
        {
            // One parent has one input state; a second dialog would make the first's
            // completion unblock a surface the second still holds.
            return Err(ChooserError::Failed(
                "a chooser is already open for this surface".to_string(),
            ));
        }
        let capabilities = self.surfaces[index]
            .root
            .chooser_capabilities()
            .ok_or_else(|| {
                ChooserError::Unavailable(
                    "this presentation has no native file chooser".to_string(),
                )
            })?;
        if let Some(feature) = capabilities.missing(&request) {
            return Err(ChooserError::Unsupported(feature));
        }
        let modal = request.modal;
        let id = self.choosers.next_id();
        self.choosers.pending.push(PendingChooser {
            id,
            parent,
            done: Some(Box::new(done)),
        });
        // The hold is set before the call: a Windows dialog runs its own message loop from
        // inside it, and the parent must already be untouchable if that loop turns.
        self.surfaces[index].native_blocked = modal;
        if let Err(error) = self.surfaces[index].root.choose(id, request) {
            if let Some(pending) = self.choosers.take(id) {
                let _ = pending.done;
            }
            self.surfaces[index].native_blocked = false;
            return Err(error);
        }
        // A backend that answered inside `choose` has no other chance to wake the loop: the
        // answer is queued, but nothing arrived on the platform's wait. One extra turn
        // drains it; an asynchronous provider takes a no-op turn and then waits for its
        // own wake.
        *self.pending_draw = true;
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

/// How a desktop decides that two presses are one multi-click.
///
/// Both rules must hold: a press inside the window is still a new click when it lands away
/// from the last one. The engine reports the burst length (1, 2, 3, …) with the click, and
/// every pointer button is counted on its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerTiming {
    /// The longest gap between two presses of one button that still joins them.
    pub multi_click: Duration,
    /// How far apart two presses may land, in logical points.
    pub multi_click_slop: f32,
}

impl Default for PointerTiming {
    fn default() -> Self {
        // The desktop convention: Windows' default 500 ms and a four-point rectangle.
        Self {
            multi_click: Duration::from_millis(500),
            multi_click_slop: 4.0,
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

    /// How this desktop decides that two presses are one multi-click.
    ///
    /// The default is the desktop convention; a platform with a system setting overrides it
    /// so a picker follows what the user tuned in the control panel.
    fn pointer_timing(&self) -> PointerTiming {
        PointerTiming::default()
    }

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

    /// The platform's thumbnail source, or `None` where the platform has no thumbnail
    /// system. Called once, when the host is built; the provider runs on its own worker.
    fn thumbnails(&mut self) -> Option<Box<dyn Thumbnailer>> {
        None
    }

    /// The platform's file-type icons, or `None` where it has none. Called once, when the
    /// host is built; the provider runs on its own worker.
    fn file_icons(&mut self) -> Option<Box<dyn FileIconProvider>> {
        None
    }

    /// The platform's directory lister, or `None` where it cannot list directories. Called
    /// once, when the host is built; the provider runs on its own worker.
    fn file_listing(&mut self) -> Option<Box<dyn DirectoryLister>> {
        None
    }

    /// The platform's file operations, or `None` where it cannot copy, trash or link files.
    /// Called once, when the host is built; the service runs on its own worker.
    fn file_operator(&mut self) -> Option<Box<dyn FileOperator>> {
        None
    }

    /// The platform's classic file context menu, or `None` where its desktop has none. The
    /// answer is stateless: every call builds the menu for the selection it is given.
    fn shell_menu(&mut self) -> Option<Arc<dyn ShellMenuHost>> {
        None
    }

    /// The platform's recursive search, or `None` where it cannot walk directories. Called
    /// once, when the host is built; the walk runs on its own worker.
    fn file_search(&mut self) -> Option<Box<dyn DirectorySearcher>> {
        None
    }

    /// The platform's directory watcher, or `None` where it cannot watch one. Called once,
    /// when the host is built; a watch runs on a thread of its own.
    fn file_watch(&mut self) -> Option<Box<dyn DirectoryWatcher>> {
        None
    }

    /// The platform's mounting of volumes that are not mounted, or `None` where it has no
    /// removable media to mount. Called once, when the host is built; enumeration and
    /// mounting run on the service's own worker.
    fn volume_mounter(&mut self) -> Option<Box<dyn crate::volumes::VolumeMounter>> {
        None
    }

    /// The places a file picker offers. Read once, when the host is built: the desktop's
    /// user directories are a small, synchronous table.
    fn places(&mut self) -> Vec<Place> {
        Vec::new()
    }

    /// The mounted volumes a file picker offers beside the places. Read once, when the host
    /// is built, for the same reason: a mount table is a small, synchronous read.
    fn devices(&mut self) -> Vec<Device> {
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
    mark_modality(surfaces);
    let mut copies = Vec::new();
    let mut retired = Vec::new();
    for group in surfaces {
        let id = group.id;
        if let Err(reason) = step_group(group, stats, &mut copies) {
            retired.push((id, reason));
        }
    }
    (copies, retired)
}

/// Mark every surface whose input a modal dialog holds.
///
/// The platform's modal hint is advisory — Wayland's `xdg-dialog` says the client filters
/// the parent's events itself — so the engine is told directly, and a surface blocked this
/// way still paints.
fn mark_modality(surfaces: &mut [LiveSurface]) {
    let mut blocked: FastSet<SurfaceId> = FastSet::default();
    for group in surfaces.iter() {
        if let Some(dialog) = group.dialog
            && dialog.modal
        {
            blocked.insert(dialog.parent);
        }
    }
    for group in surfaces {
        // A native chooser is not a surface the host steps, so its hold is carried on the
        // parent itself; folding it in here keeps one answer to "may this surface take
        // input", whatever holds it.
        group.blocked = blocked.contains(&group.id) || group.native_blocked;
        group.engine.set_input_blocked(group.blocked);
    }
}

/// Drop retired and closed surfaces, then every dialog whose parent has gone.
///
/// A dialog belongs to the surface that opened it. One whose parent closed has nobody to
/// answer it, so it closes too; the chain is walked until nothing more is orphaned. Returns
/// how many surfaces left.
fn retain_live(surfaces: &mut Vec<LiveSurface>, retired: &[(SurfaceId, String)]) -> usize {
    let before = surfaces.len();
    surfaces.retain(|group| {
        !retired.iter().any(|(retired, _)| retired == &group.id) && !group.root.is_closed()
    });
    loop {
        let live: FastSet<SurfaceId> = surfaces.iter().map(|group| group.id).collect();
        let count = surfaces.len();
        surfaces.retain(|group| {
            group
                .dialog
                .is_none_or(|dialog| live.contains(&dialog.parent))
        });
        if surfaces.len() == count {
            break;
        }
    }
    before - surfaces.len()
}

/// Draw every remaining surface once after another left.
///
/// What a surface was showing is gone — a dialog answered the app that opened it, a window
/// closed under the pointer — and the surfaces around the hole may depend on it. Only a
/// build reads app state, so a departure is a frame request, like a finished job.
fn wake_after_departure(surfaces: &mut [LiveSurface], pending_draw: &mut bool) {
    for group in surfaces {
        group.engine.request_frame();
        group.root.frame_requested();
    }
    *pending_draw = true;
}

/// One group's turn: every panel takes its input, the tree builds one frame with the
/// pointer resolved across all its surfaces, and each panel then shows its part of it.
fn step_group(
    group: &mut LiveSurface,
    stats: &mut RunStats,
    copies: &mut Vec<ClipboardCopy>,
) -> Result<(), String> {
    // The panels read their queues first: a pointer crossing from the window onto a panel
    // leaves one surface and enters the other in the same batch, and only the panel's
    // queue holds the half that says where it went. The frame built below is the one that
    // has to show it, and neither surface alone can say where the pointer is.
    intake_panels(group);
    // A surface a modal dialog holds neither reads the pointer nor takes events; its frame
    // still builds, so it stays painted behind the dialog.
    let pointer = if group.blocked {
        None
    } else {
        group_pointer(group)
    };
    // A surface the platform's own move loop is moving draws from that loop's tick and
    // nowhere else: a step here would pump the queue the loop is reading.
    if group.root.moving() {
        group.engine.set_pointer_hint(pointer);
        let drew = group.root.pump_moved(&mut group.engine);
        group.engine.clear_pointer_hint();
        if drew {
            stats.frames += 1;
        }
        step_panels(group, stats, copies);
        return Ok(());
    }
    group.engine.set_pointer_hint(pointer);
    let stepped = group.root.step(&mut group.engine);
    group.engine.clear_pointer_hint();
    let step = stepped?;
    if step.drew {
        stats.frames += 1;
    }
    copies.extend(group.root.take_copies(&mut group.engine));
    // The frame just built names the panels: bind what is new, move what moved, and let
    // each panel show its own part of it.
    reconcile_panels(group);
    step_panels(group, stats, copies);
    Ok(())
}

/// Let every panel take the input that arrived for it, before the group's frame is built.
fn intake_panels(group: &mut LiveSurface) {
    for binding in &mut group.panels {
        // A surface that cannot read its queue is retired by its own step below, which
        // fails on the same queue rather than losing the panel here.
        if let Err(reason) = binding.handle.intake(&mut group.engine) {
            log::warn!("ui-platform: a panel could not take its input: {reason}");
        }
    }
}

/// The one pointer a group's surfaces share, in the tree's coordinates.
///
/// Only a panel answers: its intake has just run, so its answer is this turn's. The root's
/// queue is read by its own step, which comes later, so its answer is last turn's — and a
/// hint is what the frame hit-tests with. Handing it that place kept a pin hovered after
/// the pointer had left: the leave's own frame was built where the pointer no longer was,
/// so its hover came out unchanged and nothing asked for another frame. The root's place
/// still reaches the frame as the event its step dispatches, which the tree keeps.
/// Panels are asked deepest-first: a flyout opened off another panel is declared after it,
/// and while the pointer is on the flyout the outer panel still remembers where it last
/// was, which would freeze the shared hover.
fn group_pointer(group: &LiveSurface) -> Option<ui_core::geom::Point> {
    group
        .panels
        .iter()
        .rev()
        .find_map(|binding| binding.handle.pointer())
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
        let spec = popup_spec(panel, group.engine.text_scale());
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
        match open_panel(group, panel, spec) {
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
    panel: &PlatformPanel,
    spec: PopupSpec,
) -> Result<Box<dyn SurfaceHandle>, String> {
    let node = panel.node;
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
        None if panel.window => {
            let title = panel.title.as_deref().unwrap_or_default();
            match group.root.open_panel_window(node, spec, title) {
                Ok(handle) => Ok(handle),
                Err(reason) => {
                    // A panel that only reports must not vanish because the platform has
                    // no windows: the popup form still shows it.
                    log::info!("ui-platform: a window panel fell back to a popup: {reason}");
                    group.root.open_popup(node, spec)
                }
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
        .map(|panel| {
            crate::window::snap_surface_rect(panel.rect, group.engine.text_scale()).origin
        })
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
    /// The platform's thumbnail source, when it has one; a finished request forces a frame.
    thumbnails: Option<Thumbnails>,
    /// Thumbnail requests completed as of the last turn.
    thumbnails_seen: u64,
    /// The platform's file-type icons, when it has them; a finished request forces a frame.
    file_icons: Option<FileIcons>,
    /// File-icon requests completed as of the last turn.
    file_icons_seen: u64,
    /// The platform's directory lister, when it has one; a finished read forces a frame.
    listings: Option<Listings>,
    /// Listings completed as of the last turn.
    listings_seen: u64,
    /// The platform's copy, move, delete and link service, when it has one.
    file_operations: Option<FileOperations>,
    /// Operations completed as of the last turn.
    file_operations_seen: u64,
    /// The platform's classic file context menu, when its desktop has one.
    shell_menu: Option<Arc<dyn ShellMenuHost>>,
    /// The platform's recursive search, when it has one; an answer forces a frame.
    searches: Option<Searches>,
    /// Search answers as of the last turn.
    searches_seen: u64,
    /// The platform's directory watcher, when it has one; a change forces a frame.
    watches: Option<Watches>,
    /// Watch changes as of the last turn.
    watches_seen: u64,
    /// The platform's mounting of unmounted volumes, when it has one.
    volumes: Option<Volumes>,
    /// Volume list and mount completions as of the last turn.
    volumes_seen: u64,
    /// Native choosers waiting for an answer.
    choosers: ChooserQueue,
    /// The places the platform's desktop names, read once at construction.
    places: Vec<Place>,
    /// The mounted volumes the platform offers, read once at construction.
    devices: Vec<Device>,
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
        let jobs = Jobs::new(Arc::clone(&waker));
        let thumbnails = backend
            .thumbnails()
            .map(|provider| Thumbnails::new(provider, Arc::clone(&waker)));
        let file_icons = backend
            .file_icons()
            .map(|provider| FileIcons::new(provider, Arc::clone(&waker)));
        let listings = backend
            .file_listing()
            .map(|provider| Listings::new(provider, Arc::clone(&waker)));
        let file_operations = backend
            .file_operator()
            .map(|operator| FileOperations::new(operator, Arc::clone(&waker)));
        let shell_menu = backend.shell_menu();
        let searches = backend
            .file_search()
            .map(|provider| Searches::new(provider, Arc::clone(&waker)));
        let watches = backend
            .file_watch()
            .map(|provider| Watches::new(provider, Arc::clone(&waker)));
        let volumes = backend
            .volume_mounter()
            .map(|provider| Volumes::new(provider, Arc::clone(&waker)));
        let places = backend.places();
        let devices = backend.devices();
        Self {
            jobs,
            completed_seen: 0,
            thumbnails,
            thumbnails_seen: 0,
            file_icons,
            file_icons_seen: 0,
            listings,
            listings_seen: 0,
            file_operations,
            file_operations_seen: 0,
            shell_menu,
            searches,
            searches_seen: 0,
            watches,
            watches_seen: 0,
            volumes,
            volumes_seen: 0,
            choosers: ChooserQueue::default(),
            places,
            devices,
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

    /// The platform's thumbnails, when it has a thumbnail system. A finished request wakes
    /// the loop and forces one frame, exactly as a job does, so the next `view` sees it.
    pub fn thumbnails(&self) -> Option<Thumbnails> {
        self.thumbnails.clone()
    }

    /// The platform's file-type icons, when it has them. A lookup answers from the cache or
    /// queues the provider; a finished request wakes the loop and forces one frame, exactly
    /// as a thumbnail does.
    pub fn file_icons(&self) -> Option<FileIcons> {
        self.file_icons.clone()
    }

    /// The platform's directory listings, when it can read directories. A finished read
    /// wakes the loop and forces one frame, exactly as a thumbnail does.
    pub fn listings(&self) -> Option<Listings> {
        self.listings.clone()
    }

    /// The platform's file operations, when it can move files. A finished operation forces
    /// a frame, and the exit policy waits for one that is still running.
    pub fn file_operations(&self) -> Option<FileOperations> {
        self.file_operations.clone()
    }

    /// The platform's classic file context menu, when its desktop has one. The picker may
    /// merge its readable rows or host the native menu; another desktop answers `None`.
    pub fn shell_menu(&self) -> Option<Arc<dyn ShellMenuHost>> {
        self.shell_menu.clone()
    }

    /// The platform's recursive search, when it can walk directories. Each answer wakes the
    /// loop and forces one frame, so the picker's next `view` sees the matches.
    pub fn searches(&self) -> Option<Searches> {
        self.searches.clone()
    }

    /// The platform's directory watches, when it can watch one. A change wakes the loop and
    /// forces one frame, so the picker's next `view` can read the directory again.
    pub fn watches(&self) -> Option<Watches> {
        self.watches.clone()
    }

    /// The platform's mounting of volumes that are not mounted, when it can mount one. A
    /// finished list or mount wakes the loop and forces one frame, as every service does.
    pub fn volumes(&self) -> Option<Volumes> {
        self.volumes.clone()
    }

    /// The places the desktop names, in the order the platform returned them.
    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// The mounted volumes the platform offers, in the order it returned them.
    pub fn devices(&self) -> &[Device] {
        &self.devices
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
            choosers: &mut self.choosers,
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
        let timing = self.backend.pointer_timing();
        let engine = engine_for(Box::new(app), &mut options, timing)?;
        let handle = self.backend.open(spec, PresentationOptions { uncapped })?;
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        self.surfaces.push(LiveSurface {
            id,
            root: handle,
            dialog: None,
            blocked: false,
            native_blocked: false,
            panels: Vec::new(),
            dismissed: FastSet::default(),
            refused: FastSet::default(),
            engine,
        });
        Ok(id)
    }

    /// Open a dialog window belonging to `parent`; the loop ticks it from then on.
    ///
    /// The same contract as [`MessageCtx::open_dialog`], for work between turns.
    pub fn open_dialog<A: App + 'static>(
        &mut self,
        parent: SurfaceId,
        spec: SurfaceSpec,
        app: A,
        mut options: OpenOptions,
    ) -> Result<SurfaceId, String> {
        let modal = spec
            .dialog_modal()
            .ok_or("a dialog surface is opened with SurfaceSpec::dialog")?;
        let index = self
            .surfaces
            .iter()
            .position(|group| group.id == parent)
            .ok_or("no such surface")?;
        let uncapped = options.uncapped;
        let timing = self.backend.pointer_timing();
        let engine = engine_for(Box::new(app), &mut options, timing)?;
        let handle = self.surfaces[index]
            .root
            .open_dialog(spec, PresentationOptions { uncapped })?;
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        self.surfaces.push(LiveSurface {
            id,
            root: handle,
            dialog: Some(DialogLink { parent, modal }),
            blocked: false,
            native_blocked: false,
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
                if self.jobs.outstanding() == 0
                    && self
                        .file_operations
                        .as_ref()
                        .is_none_or(|operations| operations.outstanding() == 0)
                {
                    break;
                }
                // A window that closed while a save or a transfer ran must not take it with
                // it. The worker's wake ends this wait early; the timeout covers a platform
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
        // A thumbnail arrived; turning it into a picture is the app's view, so it needs a
        // frame for the same reason a job result does.
        if let Some(thumbnails) = &self.thumbnails {
            let done = thumbnails.completed();
            if done != self.thumbnails_seen {
                self.thumbnails_seen = done;
                for group in &mut self.surfaces {
                    group.engine.request_frame();
                    group.root.frame_requested();
                }
            }
        }
        // An icon arrived; like a thumbnail, the row that shows it is the app's view.
        if let Some(file_icons) = &self.file_icons {
            let done = file_icons.completed();
            if done != self.file_icons_seen {
                self.file_icons_seen = done;
                for group in &mut self.surfaces {
                    group.engine.request_frame();
                    group.root.frame_requested();
                }
            }
        }
        // A listing arrived; the rows it names are the app's view as well.
        if let Some(listings) = &self.listings {
            let done = listings.completed();
            if done != self.listings_seen {
                self.listings_seen = done;
                for group in &mut self.surfaces {
                    group.engine.request_frame();
                    group.root.frame_requested();
                }
            }
        }
        // A transfer progressed, asked a conflict question, or finished; the footer and the
        // conflict panel are the app's view.
        if let Some(operations) = &self.file_operations {
            let revision = operations.revision();
            if revision != self.file_operations_seen {
                self.file_operations_seen = revision;
                for group in &mut self.surfaces {
                    group.engine.request_frame();
                    group.root.frame_requested();
                }
            }
        }
        // A search found more entries, or stopped; the rows are the app's view.
        if let Some(searches) = &self.searches {
            let done = searches.completed();
            if done != self.searches_seen {
                self.searches_seen = done;
                for group in &mut self.surfaces {
                    group.engine.request_frame();
                    group.root.frame_requested();
                }
            }
        }
        // Something inside a watched directory changed; its listing is stale now.
        if let Some(watches) = &self.watches {
            let done = watches.completed();
            if done != self.watches_seen {
                self.watches_seen = done;
                for group in &mut self.surfaces {
                    group.engine.request_frame();
                    group.root.frame_requested();
                }
            }
        }
        // A volume list arrived or a mount finished; the sidebar and the directory it
        // names are the app's view.
        if let Some(volumes) = &self.volumes {
            let done = volumes.completed();
            if done != self.volumes_seen {
                self.volumes_seen = done;
                for group in &mut self.surfaces {
                    group.engine.request_frame();
                    group.root.frame_requested();
                }
            }
        }
        // Before anything answers a command: a status reply or a pin placement must not use
        // yesterday's outputs.
        self.outputs = self.backend.outputs();
        // A step may start a modal gesture — a title-bar move, an OLE drag — whose message
        // loop does not return until the pointer is released. The surface is taken out of
        // the list while its step runs, so the gesture's own timer can reach the rest of
        // the process through [`modal::pump_others`](crate::modal::pump_others).
        mark_modality(&mut self.surfaces);
        let mut index = 0;
        while index < self.surfaces.len() {
            let mut group = self.surfaces.remove(index);
            let mut others = || self.pump_others();
            crate::modal::arm(&mut others);
            let stepped = step_group(&mut group, &mut self.stats, &mut copies);
            crate::modal::disarm();
            if let Err(reason) = stepped {
                retired.push((group.id, reason));
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
        if retain_live(&mut self.surfaces, &retired) > 0 {
            wake_after_departure(&mut self.surfaces, &mut self.pending_draw);
        }
        self.serve_choosers();
        self.serve_hotkeys();
        self.serve_tray();
        self.serve_channel();
        self.serve_turn();
        Ok(())
    }

    /// Hand every native chooser answer to the one callback waiting for it.
    ///
    /// Answers are drained from every surface before a callback runs: a callback may open
    /// the next dialog, and the surface list must not be borrowed while it does.
    fn serve_choosers(&mut self) {
        let mut answers = Vec::new();
        for group in &mut self.surfaces {
            answers.extend(group.root.chooser_events());
        }
        for (id, answer) in answers {
            let Some(pending) = self.choosers.take(id) else {
                log::warn!("ui-platform: a chooser answered with an unknown id {id:?}");
                continue;
            };
            self.release_parent(pending.parent);
            if let Some(done) = pending.done {
                // The callback runs after the queue entry is gone, so it may start another
                // chooser through the app's next turn without finding this one again.
                done(answer);
            }
        }
        self.abandon_dead_choosers();
    }

    /// Give a surface its input back and make it draw: the chooser covered part of it.
    fn release_parent(&mut self, parent: SurfaceId) {
        let released = match self.surfaces.iter_mut().find(|group| group.id == parent) {
            Some(group) => {
                group.native_blocked = false;
                group.engine.request_frame();
                group.root.frame_requested();
                true
            }
            None => false,
        };
        self.pending_draw |= released;
    }

    /// Answer the choosers whose parent went away while they were running.
    ///
    /// A dialog that loses its parent has nobody to answer; the portal may answer later, but
    /// its callback must still run exactly once, and the surface it would have answered for
    /// is gone. The parent's disappearance is the close the user could not give.
    fn abandon_dead_choosers(&mut self) {
        if self.choosers.pending.is_empty() {
            return;
        }
        let live: FastSet<SurfaceId> = self.surfaces.iter().map(|group| group.id).collect();
        let dead: Vec<ChooserId> = self
            .choosers
            .pending
            .iter()
            .filter(|pending| !live.contains(&pending.parent))
            .map(|pending| pending.id)
            .collect();
        if dead.is_empty() {
            return;
        }
        for id in dead {
            let Some(pending) = self.choosers.take(id) else {
                continue;
            };
            if let Some(done) = pending.done {
                done(ChooserAnswer::native(Err(ChooserError::Cancelled)));
            }
        }
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
        if retain_live(&mut self.surfaces, &retired) > 0 {
            wake_after_departure(&mut self.surfaces, &mut self.pending_draw);
        }
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
                choosers: &mut self.choosers,
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
                choosers: &mut self.choosers,
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
            choosers,
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
            choosers,
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
            choosers,
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
                choosers,
                pending_draw,
            };
            if handler(reply, bytes, &mut context) == Effect::Quit {
                quit = true;
            }
        });
        *host_quit |= quit;
    }
}
