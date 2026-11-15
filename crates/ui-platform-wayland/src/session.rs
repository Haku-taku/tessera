//! One engine-owned session: window, presenter, driver and teardown order.

use std::time::{Duration, Instant};

use ui_core::geom::Point;
#[cfg(feature = "gpu")]
use ui_core::timed;
use ui_platform::clipboard::{ClipboardCommand, ClipboardCopy, ClipboardKind};
use ui_platform::driver::{Driver, Turn};
use ui_platform::host::{Engine, SurfaceHandle, SurfaceStep};
#[cfg(feature = "gpu")]
use ui_platform::present::{PresentStats, SceneTarget};
use ui_platform::session::{HostSession, SessionOptions, Tick};
#[cfg(feature = "gpu")]
use ui_platform::text_pass::TextPass;
use ui_platform::view::{App, AppLoop};

#[cfg(feature = "gpu")]
use crate::gpu_window::{GpuWindow, GpuWindowConfig};
use crate::window::WaylandWindow;
#[cfg(feature = "gpu")]
use ui_platform::window::{PopupSpec, SurfaceRole, SurfaceSpec};

/// The GPU presenter as the engine's scene target.
#[cfg(feature = "gpu")]
struct GpuTarget<'a> {
    win: &'a mut WaylandWindow,
    gpu: &'a mut GpuWindow,
}

#[cfg(feature = "gpu")]
impl SceneTarget for GpuTarget<'_> {
    fn size(&self) -> (u32, u32) {
        let (w, h) = self.win.logical_size();
        let s = self.win.scale().max(f32::EPSILON);
        ((w as f32 * s).round() as u32, (h as f32 * s).round() as u32)
    }

    fn present_scene(
        &mut self,
        scene: &ui_core::draw::DrawList,
        text: &TextPass,
        damage: &ui_core::Damage,
        canvases: &[ui_platform::present::CanvasFrame],
    ) -> Result<PresentStats, String> {
        timed!(
            "atlas",
            self.gpu
                .sync_atlas(text)
                .map_err(|e| format!("atlas upload failed: {e}"))
        )?;
        let (w, h) = self.win.logical_size();
        // A changed canvas carries its list; an unchanged one only names the target the
        // renderer already holds.
        let uploads: Vec<ui_paint_gpu::CanvasUpload<'_>> = canvases
            .iter()
            .map(|canvas| ui_paint_gpu::CanvasUpload {
                id: canvas.id,
                size: canvas.size,
                list: canvas.changed.then_some(&canvas.list),
            })
            .collect();
        let out = self
            .gpu
            .present(self.win, (w, h), scene, damage, &uploads)
            .map_err(|e| format!("present failed: {e}"))?;
        Ok(PresentStats {
            draw_calls: out.draw_calls,
            band: out.band,
        })
    }

    fn set_cursor(&mut self, shape: ui_core::CursorShape) {
        self.win.set_cursor(shape);
    }

    fn set_input_region(&mut self, areas: &[ui_core::geom::Rect]) {
        self.win.set_input_areas(areas);
    }

    fn set_pointer_inside(&mut self, inside: bool) {
        self.win.apply_pointer_inside(inside);
    }

    fn set_ime(&mut self, state: ui_platform::present::ImeState) {
        self.win.set_ime(state);
    }

    fn commit_pending(&self) -> bool {
        self.win.ime_pending() || self.win.input_pending()
    }
}

/// One Wayland surface as a presenter: window, renderer, driver and its own input.
///
/// Dropped in declaration order, which is the order teardown needs: GPU objects while the
/// surface still exists, then the driver, then the window.
pub struct Presenter {
    #[cfg(feature = "gpu")]
    gpu: Option<GpuWindow>,
    driver: Driver,
    /// What this surface was opened as, so a panel it opens is named the same way.
    #[cfg(feature = "gpu")]
    spec: SurfaceSpec,
    win: WaylandWindow,
    /// Scripted pointer for the next frame; see `set_pointer`.
    pointer: Option<Point>,
    /// The app asked for this surface to close; the host decides what that means.
    closed: bool,
}

/// An app and the one surface it drives, for code that runs a single window itself.
///
/// The loop is dropped before the presenter, so the engine lets go of the window it built
/// frames for rather than the other way round.
pub struct Session<A: App> {
    lp: AppLoop<A>,
    presenter: Presenter,
}

impl Presenter {
    /// Open the surface and pick a presenter.
    pub fn open(opts: SessionOptions) -> Result<Self, String> {
        Self::open_linked(opts, None)
    }

    /// Open on an existing connection, so this surface is one of several.
    pub fn open_on(link: &crate::link::WaylandLink, opts: SessionOptions) -> Result<Self, String> {
        Self::open_linked(opts, Some(link))
    }

    fn open_linked(
        opts: SessionOptions,
        link: Option<&crate::link::WaylandLink>,
    ) -> Result<Self, String> {
        #[cfg(feature = "gpu")]
        let spec = opts.surface.clone();
        let win = match link {
            Some(link) => link.surface(opts.surface),
            None => WaylandWindow::new(opts.surface),
        }
        .map_err(|e| format!("cannot open a Wayland surface: {e}"))?;
        #[cfg(feature = "gpu")]
        let (win, gpu) = {
            let mut win = win;
            let gpu = if opts.cpu_presenter {
                None
            } else {
                match GpuWindow::open(&mut win, GpuWindowConfig::default()) {
                    Ok(gpu) => Some(gpu),
                    Err(e) => {
                        return Err(format!(
                            "GPU presenter unavailable: {e}\nset the CPU presenter to run without it"
                        ));
                    }
                }
            };
            (win, gpu)
        };
        Ok(Self {
            #[cfg(feature = "gpu")]
            gpu,
            driver: Driver::new().uncapped(opts.uncapped),
            #[cfg(feature = "gpu")]
            spec,
            win,
            pointer: None,
            closed: false,
        })
    }

    /// The pointer to build the next frame with, overriding the seat's — scripted input
    /// needs it, or the frame snaps back to wherever the real cursor is.
    pub fn set_pointer(&mut self, pointer: Option<Point>) {
        self.pointer = pointer;
    }

    /// The GPU renderer's name and node, when one is in use.
    pub fn presenter_name(&self) -> Option<String> {
        #[cfg(feature = "gpu")]
        {
            self.gpu
                .as_ref()
                .map(|g| format!("{} ({})", g.renderer_name(), g.node()))
        }
        #[cfg(not(feature = "gpu"))]
        {
            None
        }
    }

    pub fn window(&self) -> &WaylandWindow {
        &self.win
    }

    pub fn window_mut(&mut self) -> &mut WaylandWindow {
        &mut self.win
    }

    /// The connection this surface runs on; see [`WaylandLink`](crate::link::WaylandLink).
    pub fn link(&self) -> crate::link::WaylandLink {
        self.win.link()
    }

    #[cfg(feature = "gpu")]
    pub fn gpu_mut(&mut self) -> Option<&mut GpuWindow> {
        self.gpu.as_mut()
    }

    pub fn finished(&self) -> bool {
        self.win.should_close()
    }

    /// How long the last turn waited for a frame callback; a long one means a stalled output.
    pub fn last_wait(&self) -> Duration {
        self.driver.last_wait()
    }

    /// Frame waits so far and their total, for the host's exit line.
    pub fn wait_stats(&self) -> (Duration, u32) {
        self.driver.wait_stats()
    }

    /// Events the last turn took from the window and dispatched to the app.
    pub fn events(&self) -> &[ui_core::Event] {
        self.driver.events()
    }

    /// One non-blocking turn: dispatch what arrived and draw if a frame is due.
    ///
    /// The loop's form. It never waits — polling the connection is the loop's job — so it
    /// can be called for every surface of a process in turn.
    pub fn step_with<A: App>(&mut self, lp: &mut AppLoop<A>) -> Result<Tick, String> {
        self.win
            .pump_checked()
            .map_err(|error| format!("wayland: {error}"))?;
        self.win.service_paste();
        if self.take_close(lp) {
            return Ok(Tick::default());
        }
        let pointer = match self.driver.decide(lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        self.draw(lp, pointer)
    }

    /// Intake, decide, draw and pace — everything a frame needs, waiting included.
    ///
    /// The single-surface form: scripted probes and tests drive one session themselves.
    pub fn tick_with<A: App>(&mut self, lp: &mut AppLoop<A>) -> Result<Tick, String> {
        if self.take_close(lp) {
            return Ok(Tick::default());
        }
        let pointer = match self.driver.step(lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        let tick = self.draw(lp, pointer)?;
        self.driver.pace(&mut self.win);
        Ok(tick)
    }

    /// A close the app asked for, recorded once so a later turn stays closed.
    fn take_close<A: App>(&mut self, lp: &mut AppLoop<A>) -> bool {
        self.closed |= lp.take_close();
        self.closed
    }

    fn draw<A: App>(&mut self, lp: &mut AppLoop<A>, pointer: Option<Point>) -> Result<Tick, String> {
        // A scripted pointer wins over the seat's, which the driver just read.
        let pointer = self.pointer.take().or(pointer);
        #[cfg(feature = "gpu")]
        let framed = match self.gpu.as_mut() {
            Some(gpu) => {
                let mut target = GpuTarget {
                    win: &mut self.win,
                    gpu,
                };
                lp.frame_scene(&mut target, pointer)
            }
            None => {
                let outcome = lp.frame(&mut self.win, pointer);
                Ok(ui_platform::view::SceneFrameOutcome {
                    outcome,
                    presented: None,
                })
            }
        }?;
        #[cfg(not(feature = "gpu"))]
        let framed = {
            let outcome = lp.frame(&mut self.win, pointer);
            Ok::<_, String>(ui_platform::view::SceneFrameOutcome {
                outcome,
                presented: None,
            })
        }?;
        // A close the build asked for takes effect in this turn, not the next one.
        self.closed |= lp.take_close();
        // A build may ask to move the pointer, which only this surface's window can do. A
        // refused step moves nothing; the app keeps the position it had, as documented.
        let steps = lp.take_pointer_steps();
        for (dx, dy) in &steps {
            self.win.nudge_pointer(*dx, *dy);
        }
        // The frame just built read the pointer's old place, so one more shows the new one.
        if !steps.is_empty() {
            lp.request_frame();
        }
        // A drag that asked to leave the app becomes the compositor's; the outcome comes
        // back as an event once the target is done with it.
        if let Some(drag) = lp.take_native_drag()
            && !self.win.begin_drag(drag.data)
        {
            // The host refused, so the gesture ends without a drop.
            lp.dispatch(ui_core::Event::Dnd(ui_core::DndEvent::Finished {
                dropped: false,
            }));
            lp.request_frame();
        }
        Ok(Tick {
            drew: true,
            outcome: Some(framed.outcome),
            presented: framed.presented,
            pointer,
        })
    }
}

/// A panel's own surface: the engine paints it, this presenter shows it and routes its input.
///
/// The panel's subtree lives in the parent's tree, so this is not a second engine: what
/// arrives here is shifted back into that tree and dispatched to the same app.
#[cfg(feature = "gpu")]
pub struct PopupPresenter {
    gpu: GpuWindow,
    win: WaylandWindow,
    /// What the popup was opened as, so a panel inside it is named the same way.
    spec: SurfaceSpec,
    node: ui_core::NodeId,
    /// The tree origin of the surface this popup hangs from.
    parent_origin: Point,
    /// Where the panel's subtree stands in the parent tree's coordinates.
    origin: Point,
    /// The build whose panel list this surface last showed.
    presented: Option<u64>,
    /// The compositor took the popup back; the host dismisses the panel.
    closed: bool,
}

/// The surface spec a panel's window is opened with.
#[cfg(feature = "gpu")]
fn popup_surface_spec(parent: &SurfaceSpec, spec: PopupSpec) -> SurfaceSpec {
    SurfaceSpec {
        app_id: parent.app_id.clone(),
        name: parent.name.clone(),
        role: SurfaceRole::Window {
            size: (
                spec.rect.size.w.round().max(1.0) as u32,
                spec.rect.size.h.round().max(1.0) as u32,
            ),
        },
        min_size: None,
    }
}

/// Open a popup hung off `parent_window` that shows the panel `node`.
///
/// `spec` is in the parent presentation's coordinates, so the panel's place in the tree is
/// that offset from `parent_origin`.
#[cfg(feature = "gpu")]
fn open_panel(
    parent_window: &WaylandWindow,
    parent_spec: &SurfaceSpec,
    parent_origin: Point,
    node: ui_core::NodeId,
    spec: PopupSpec,
    grab_serial: u32,
) -> Result<PopupPresenter, String> {
    let surface = popup_surface_spec(parent_spec, spec);
    let mut win = WaylandWindow::create_popup(parent_window, surface.clone(), spec, grab_serial)
        .map_err(|e| format!("cannot open a popup surface: {e}"))?;
    let gpu = GpuWindow::open(&mut win, GpuWindowConfig::default())
        .map_err(|e| format!("GPU presenter unavailable: {e}"))?;
    Ok(PopupPresenter {
        gpu,
        win,
        spec: surface,
        node,
        parent_origin,
        origin: parent_origin + spec.rect.origin,
        presented: None,
        closed: false,
    })
}

/// Move an event's position from a panel's own space into the parent tree's.
#[cfg(feature = "gpu")]
fn shift(event: ui_core::Event, by: Point) -> ui_core::Event {
    use ui_core::Event;
    match event {
        Event::PointerMoved { pos } => Event::PointerMoved { pos: pos + by },
        Event::PointerPressed { pos, button } => Event::PointerPressed { pos: pos + by, button },
        Event::PointerReleased { pos, button } => Event::PointerReleased { pos: pos + by, button },
        Event::Scroll { pos, delta } => Event::Scroll { pos: pos + by, delta },
        other => other,
    }
}

#[cfg(feature = "gpu")]
impl PopupPresenter {
    /// Feed what arrived on this surface into the tree the panel belongs to.
    ///
    /// A dispatch that can change the frame leaves the engine owing a build, which is what
    /// wakes the loop for it; a move over inert space asks for nothing.
    fn route_input(&mut self, engine: &mut Engine) {
        let origin = self.origin;
        for event in self.win.take_events() {
            engine.dispatch(shift(event, origin));
        }
    }

    /// Show the shape the tree gives the pointer's place on the panel.
    fn follow_cursor(&mut self, engine: &Engine) {
        if let Some(at) = self.win.pointer() {
            self.win.set_cursor(engine.cursor_shape_at(at + self.origin));
        }
    }

    fn present(&mut self, engine: &Engine, frame: u64) -> Result<(), String> {
        let mut target = GpuTarget {
            win: &mut self.win,
            gpu: &mut self.gpu,
        };
        engine
            .present_panel(self.node, &mut target)
            .map_err(|e| format!("present failed: {e}"))?;
        self.presented = Some(frame);
        Ok(())
    }
}

#[cfg(feature = "gpu")]
impl SurfaceHandle for PopupPresenter {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        self.win
            .pump_checked()
            .map_err(|error| format!("wayland: {error}"))?;
        if self.win.should_close() {
            self.closed = true;
            return Ok(SurfaceStep::default());
        }
        self.route_input(engine);
        self.follow_cursor(engine);
        let frame = engine.frame_number();
        // Nothing new since the last show, nothing painted for it, or no frame window yet.
        if self.presented == Some(frame)
            || !self.win.can_draw()
            || !engine.panel_painted(self.node)
        {
            return Ok(SurfaceStep::default());
        }
        self.present(engine, frame)?;
        Ok(SurfaceStep { drew: true })
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        self.win.repeat_deadline()
    }

    fn is_closed(&self) -> bool {
        self.closed || self.win.should_close()
    }

    /// Empty: the app is one, and its copy commands are drained by the surface that steps
    /// first — the parent's, which is the one that can serve them this same turn.
    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    /// A panel inside this one hangs off this surface, so the grab stays with the topmost.
    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        spec: PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let grab = self.win.input_serial();
        let presenter = open_panel(&self.win, &self.spec, self.origin, node, spec, grab)?;
        Ok(Box::new(presenter))
    }

    fn reconfigure(&mut self, spec: PopupSpec) -> Result<(), String> {
        self.win.reposition_popup(spec)?;
        self.origin = self.parent_origin + spec.rect.origin;
        Ok(())
    }
}

impl<A: App> Session<A> {
    /// Open the surface and pick a presenter; the loop is the caller's, the lifecycle is not.
    pub fn open(lp: AppLoop<A>, opts: SessionOptions) -> Result<Self, String> {
        Ok(Self {
            lp,
            presenter: Presenter::open(opts)?,
        })
    }

    /// Open on an existing connection, so this session's surface is one of several.
    pub fn open_on(
        link: &crate::link::WaylandLink,
        lp: AppLoop<A>,
        opts: SessionOptions,
    ) -> Result<Self, String> {
        Ok(Self {
            lp,
            presenter: Presenter::open_on(link, opts)?,
        })
    }

    /// The pointer to build the next frame with, overriding the seat's — scripted input
    /// needs it, or the frame snaps back to wherever the real cursor is.
    pub fn set_pointer(&mut self, pointer: Option<Point>) {
        self.presenter.set_pointer(pointer);
    }

    pub fn presenter_name(&self) -> Option<String> {
        self.presenter.presenter_name()
    }

    pub fn lp(&self) -> &AppLoop<A> {
        &self.lp
    }

    pub fn lp_mut(&mut self) -> &mut AppLoop<A> {
        &mut self.lp
    }

    pub fn window(&self) -> &WaylandWindow {
        self.presenter.window()
    }

    pub fn window_mut(&mut self) -> &mut WaylandWindow {
        self.presenter.window_mut()
    }

    /// The connection the session runs on; see [`WaylandLink`](crate::link::WaylandLink).
    pub fn link(&self) -> crate::link::WaylandLink {
        self.presenter.link()
    }

    #[cfg(feature = "gpu")]
    pub fn gpu_mut(&mut self) -> Option<&mut GpuWindow> {
        self.presenter.gpu_mut()
    }

    /// All three at once, for host code that needs the engine and the window together
    /// (clipboard round-trips, synthetic input). Finish with them before calling `tick`.
    #[cfg(feature = "gpu")]
    pub fn split(&mut self) -> (&mut AppLoop<A>, &mut WaylandWindow, Option<&mut GpuWindow>) {
        let presenter = &mut self.presenter;
        (&mut self.lp, &mut presenter.win, presenter.gpu.as_mut())
    }

    pub fn finished(&self) -> bool {
        self.presenter.finished()
    }

    /// How long the last turn waited for a frame callback; a long one means a stalled output.
    pub fn last_wait(&self) -> Duration {
        self.presenter.last_wait()
    }

    /// Frame waits so far and their total, for the host's exit line.
    pub fn wait_stats(&self) -> (Duration, u32) {
        self.presenter.wait_stats()
    }

    /// Events the last turn took from the window and dispatched to the app.
    pub fn events(&self) -> &[ui_core::Event] {
        self.presenter.events()
    }

    /// One non-blocking turn: dispatch what arrived and draw if a frame is due.
    ///
    /// The loop's form. It never waits — polling the connection is the loop's job — so it
    /// can be called for every surface of a process in turn.
    pub fn step(&mut self) -> Result<Tick, String> {
        self.presenter.step_with(&mut self.lp)
    }

    /// Intake, decide, draw and pace — everything a frame needs, waiting included.
    ///
    /// The single-surface form: scripted probes and tests drive one session themselves.
    pub fn tick(&mut self) -> Result<Tick, String> {
        self.presenter.tick_with(&mut self.lp)
    }
}

/// The Wayland answer to the surface an app is written against; see [`HostSession`].
impl<A: App> HostSession<A> for Session<A> {
    fn open(lp: AppLoop<A>, options: SessionOptions) -> Result<Self, String> {
        Session::open(lp, options)
    }

    fn tick(&mut self) -> Result<Tick, String> {
        Session::tick(self)
    }

    fn finished(&self) -> bool {
        Session::finished(self)
    }

    fn lp(&self) -> &AppLoop<A> {
        &self.lp
    }

    fn lp_mut(&mut self) -> &mut AppLoop<A> {
        &mut self.lp
    }

    fn set_pointer(&mut self, pointer: Option<Point>) {
        Session::set_pointer(self, pointer);
    }

    fn events(&self) -> &[ui_core::Event] {
        Session::events(self)
    }

    fn presenter_name(&self) -> Option<String> {
        Session::presenter_name(self)
    }

    fn last_wait(&self) -> Duration {
        Session::last_wait(self)
    }

    fn wait_stats(&self) -> (Duration, u32) {
        Session::wait_stats(self)
    }

    fn cpu_seconds(&self) -> Option<f64> {
        // Both halves of the process's tick counters; `/proc/self/stat` is the only clock
        // every Unix kernel offers without a syscall wrapper.
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let fields: Vec<&str> = stat.split_whitespace().collect();
        let ticks: f64 = fields.get(13).and_then(|s| s.parse::<f64>().ok())?
            + fields.get(14).and_then(|s| s.parse::<f64>().ok())?;
        Some(ticks / 100.0)
    }

    fn logical_size(&self) -> (u32, u32) {
        self.presenter.win.logical_size()
    }

    fn scale(&self) -> f32 {
        self.presenter.win.scale()
    }

    fn request_close(&mut self) {
        self.presenter.win.request_close();
    }

    fn write_clipboard(&mut self, content: ui_core::clipboard::ClipboardContent) -> bool {
        self.presenter.win.write_clipboard(content)
    }

    fn request_paste(&mut self) -> bool {
        self.presenter.win.request_paste()
    }

    fn begin_drag(&mut self, drag: ui_platform::view::NativeDrag) -> bool {
        // A compositor drag has no image of its own: the surface behind it keeps painting,
        // so the element under the pointer is what shows.
        self.presenter.win.begin_drag(drag.data)
    }

    fn gpu_present(&self) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.presenter.gpu.is_some()
        }
        #[cfg(not(feature = "gpu"))]
        {
            false
        }
    }

    fn set_supersample(&mut self, factor: u32) {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = self.presenter.gpu.as_mut() {
            gpu.renderer_mut().set_supersample(factor);
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = factor;
        }
    }

    fn frame_callbacks(&mut self) -> u32 {
        self.presenter.win.take_frame_callbacks()
    }

    fn refresh_hz(&self) -> Option<f32> {
        self.presenter.win.refresh_hz()
    }

    fn dmabuf_stats(&self) -> Option<(usize, u32)> {
        Some((
            self.presenter.win.dmabuf_inflight(),
            self.presenter.win.dmabuf_releases(),
        ))
    }

    fn dump_frame(&mut self, path: &std::path::Path) -> Result<(), String> {
        #[cfg(feature = "gpu")]
        {
            fn stem(path: &std::path::Path) -> String {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("frame")
                    .to_string()
            }
            // The layer dump goes beside the frame: one path names both pictures.
            let layer = path.with_file_name(format!("{}_layer.ppm", stem(path)));
            let Some(gpu) = self.presenter.gpu.as_mut() else {
                return Err(String::from("this session presents through the CPU rasterizer"));
            };
            gpu.dump_ppm(path, &layer)
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = path;
            Err(String::from("this build has no GPU presenter"))
        }
    }

    fn finish(&mut self) {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = self.presenter.gpu.as_mut() {
            gpu.finish();
        }
    }
}

impl SurfaceHandle for Presenter {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        self.step_with(engine)
            .map(|tick| SurfaceStep { drew: tick.drew })
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<Instant> {
        [
            self.driver.deadline(),
            self.win.paste_deadline(),
            self.win.repeat_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn is_closed(&self) -> bool {
        self.closed || self.win.should_close()
    }

    /// A panel hung off this surface gets one of its own, with its own renderer.
    #[cfg(feature = "gpu")]
    fn open_popup(
        &mut self,
        node: ui_core::NodeId,
        spec: PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        // Without a GPU there is no scene target to hand a panel's list to.
        if self.gpu.is_none() {
            return Err("the CPU presenter cannot show a panel in a surface of its own".into());
        }
        // The press that declared the panel, which is what a grab may carry.
        let grab = self.win.input_serial();
        let root = Point::new(0.0, 0.0);
        let presenter = open_panel(&self.win, &self.spec, root, node, spec, grab)?;
        Ok(Box::new(presenter))
    }

    /// Copies go to the process's clipboard host, not to this window's own source: a window
    /// that copied and closed would take the selection with it.
    fn take_copies(&mut self, engine: &mut Engine) -> Vec<ClipboardCopy> {
        let serial = self.win.input_serial();
        let mut copies = Vec::new();
        for command in engine.take_clipboard_commands() {
            match command {
                ClipboardCommand::Copy { content, kind } => copies.push(ClipboardCopy {
                    content,
                    kind,
                    serial,
                }),
                // A paste is this window's own round trip: the answer comes back as an event.
                ClipboardCommand::RequestPaste { kind } => {
                    if kind == ClipboardKind::Selection {
                        self.win.request_paste();
                    }
                }
            }
        }
        copies
    }
}
