//! One engine-owned session: window, presenter, driver and teardown order.

use std::time::{Duration, Instant};

use ui_core::geom::Point;
#[cfg(feature = "gpu")]
use ui_core::timed;
use ui_platform::clipboard::{ClipboardCommand, ClipboardCopy, ClipboardKind};
use ui_platform::driver::{Driver, Turn};
use ui_platform::host::{SurfaceHandle, SurfaceStep};
#[cfg(feature = "gpu")]
use ui_platform::present::{PresentStats, SceneTarget};
use ui_platform::session::{HostSession, SessionOptions, Tick};
#[cfg(feature = "gpu")]
use ui_platform::text_pass::TextPass;
use ui_platform::view::{App, AppLoop};

#[cfg(feature = "gpu")]
use crate::gpu_window::{GpuWindow, GpuWindowConfig};
use crate::window::WaylandWindow;

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

/// Window, presenter and driver, owned together so an app never orders them itself.
///
/// Dropped in declaration order, which is the order teardown needs: GPU objects while the
/// surface still exists, then the loop, then the window.
pub struct Session<A: App> {
    #[cfg(feature = "gpu")]
    gpu: Option<GpuWindow>,
    driver: Driver,
    lp: AppLoop<A>,
    win: WaylandWindow,
    /// Scripted pointer for the next frame; see `set_pointer`.
    pointer: Option<Point>,
    /// The app asked for this surface to close; the host decides what that means.
    closed: bool,
}

impl<A: App> Session<A> {
    /// Open the surface and pick a presenter; the loop is the caller's, the lifecycle is not.
    pub fn open(lp: AppLoop<A>, opts: SessionOptions) -> Result<Self, String> {
        Self::open_linked(lp, opts, None)
    }

    /// Open on an existing connection, so this session's surface is one of several.
    pub fn open_on(
        link: &crate::link::WaylandLink,
        lp: AppLoop<A>,
        opts: SessionOptions,
    ) -> Result<Self, String> {
        Self::open_linked(lp, opts, Some(link))
    }

    fn open_linked(
        lp: AppLoop<A>,
        opts: SessionOptions,
        link: Option<&crate::link::WaylandLink>,
    ) -> Result<Self, String> {
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
            lp,
            win,
            pointer: None,
            closed: false,
        })
    }

    /// The GPU renderer's name and node, when one is in use.
    /// The pointer to build the next frame with, overriding the seat's — scripted input
    /// needs it, or the frame snaps back to wherever the real cursor is.
    pub fn set_pointer(&mut self, pointer: Option<Point>) {
        self.pointer = pointer;
    }

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

    pub fn lp(&self) -> &AppLoop<A> {
        &self.lp
    }

    pub fn lp_mut(&mut self) -> &mut AppLoop<A> {
        &mut self.lp
    }

    pub fn window(&self) -> &WaylandWindow {
        &self.win
    }

    pub fn window_mut(&mut self) -> &mut WaylandWindow {
        &mut self.win
    }

    /// The connection the session runs on; see [`WaylandLink`](crate::link::WaylandLink).
    pub fn link(&self) -> crate::link::WaylandLink {
        self.win.link()
    }

    #[cfg(feature = "gpu")]
    pub fn gpu_mut(&mut self) -> Option<&mut GpuWindow> {
        self.gpu.as_mut()
    }

    /// All three at once, for host code that needs the engine and the window together
    /// (clipboard round-trips, synthetic input). Finish with them before calling `tick`.
    #[cfg(feature = "gpu")]
    pub fn split(&mut self) -> (&mut AppLoop<A>, &mut WaylandWindow, Option<&mut GpuWindow>) {
        (&mut self.lp, &mut self.win, self.gpu.as_mut())
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
    pub fn step(&mut self) -> Result<Tick, String> {
        self.win
            .pump_checked()
            .map_err(|error| format!("wayland: {error}"))?;
        self.win.service_paste();
        if self.take_close() {
            return Ok(Tick::default());
        }
        let pointer = match self.driver.decide(&mut self.lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        self.draw(pointer)
    }

    /// Intake, decide, draw and pace — everything a frame needs, waiting included.
    ///
    /// The single-surface form: scripted probes and tests drive one session themselves.
    pub fn tick(&mut self) -> Result<Tick, String> {
        if self.take_close() {
            return Ok(Tick::default());
        }
        let pointer = match self.driver.step(&mut self.lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        let tick = self.draw(pointer)?;
        self.driver.pace(&mut self.win);
        Ok(tick)
    }

    /// A close the app asked for, recorded once so a later turn stays closed.
    fn take_close(&mut self) -> bool {
        self.closed |= self.lp.take_close();
        self.closed
    }

    fn draw(&mut self, pointer: Option<Point>) -> Result<Tick, String> {
        // A scripted pointer wins over the seat's, which the driver just read.
        let pointer = self.pointer.take().or(pointer);
        #[cfg(feature = "gpu")]
        let framed = match self.gpu.as_mut() {
            Some(gpu) => {
                let mut target = GpuTarget {
                    win: &mut self.win,
                    gpu,
                };
                self.lp.frame_scene(&mut target, pointer)
            }
            None => {
                let outcome = self.lp.frame(&mut self.win, pointer);
                Ok(ui_platform::view::SceneFrameOutcome {
                    outcome,
                    presented: None,
                })
            }
        }?;
        #[cfg(not(feature = "gpu"))]
        let framed = {
            let outcome = self.lp.frame(&mut self.win, pointer);
            Ok::<_, String>(ui_platform::view::SceneFrameOutcome {
                outcome,
                presented: None,
            })
        }?;
        // A close the build asked for takes effect in this turn, not the next one.
        self.closed |= self.lp.take_close();
        // A build may ask to move the pointer, which only this surface's window can do. A
        // refused step moves nothing; the app keeps the position it had, as documented.
        let steps = self.lp.take_pointer_steps();
        for (dx, dy) in &steps {
            self.win.nudge_pointer(*dx, *dy);
        }
        // The frame just built read the pointer's old place, so one more shows the new one.
        if !steps.is_empty() {
            self.lp.request_frame();
        }
        Ok(Tick {
            drew: true,
            outcome: Some(framed.outcome),
            presented: framed.presented,
            pointer,
        })
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
        let ticks: f64 = fields.get(13).and_then(|s| s.parse().ok())?
            + fields.get(14).and_then(|s| s.parse().ok())?;
        Some(ticks / 100.0)
    }

    fn logical_size(&self) -> (u32, u32) {
        self.win.logical_size()
    }

    fn scale(&self) -> f32 {
        self.win.scale()
    }

    fn request_close(&mut self) {
        self.win.request_close();
    }

    fn write_clipboard(&mut self, content: ui_core::clipboard::ClipboardContent) -> bool {
        self.win.write_clipboard(content)
    }

    fn request_paste(&mut self) -> bool {
        self.win.request_paste()
    }

    fn gpu_present(&self) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu.is_some()
        }
        #[cfg(not(feature = "gpu"))]
        {
            false
        }
    }

    fn set_supersample(&mut self, factor: u32) {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.renderer_mut().set_supersample(factor);
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = factor;
        }
    }

    fn frame_callbacks(&mut self) -> u32 {
        self.win.take_frame_callbacks()
    }

    fn refresh_hz(&self) -> Option<f32> {
        self.win.refresh_hz()
    }

    fn dmabuf_stats(&self) -> Option<(usize, u32)> {
        Some((self.win.dmabuf_inflight(), self.win.dmabuf_releases()))
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
            let Some(gpu) = self.gpu.as_mut() else {
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
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.finish();
        }
    }
}

impl<A: App + 'static> SurfaceHandle for Session<A> {
    fn add_image(&mut self, image: ui_core::draw::ImageEntry) -> Result<(), String> {
        self.lp.register_image(image);
        Ok(())
    }

    fn step(&mut self) -> Result<SurfaceStep, String> {
        Session::step(self).map(|tick| SurfaceStep { drew: tick.drew })
    }

    fn next_deadline(&self) -> Option<Instant> {
        [
            self.driver.deadline(),
            self.win.paste_deadline(),
            self.win.repeat_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn request_frame(&mut self) {
        self.lp.request_frame();
    }

    fn is_closed(&self) -> bool {
        self.closed || self.win.should_close()
    }

    /// Copies go to the process's clipboard host, not to this window's own source: a window
    /// that copied and closed would take the selection with it.
    fn take_copies(&mut self) -> Vec<ClipboardCopy> {
        let serial = self.win.input_serial();
        let mut copies = Vec::new();
        for command in self.lp.take_clipboard_commands() {
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
