//! One engine-owned surface on Windows: window, render thread, driver and teardown order.

use std::sync::Arc;
use std::time::Duration;

use ui_core::Damage;
use ui_core::draw::DrawList;
use ui_core::geom::Point;
use ui_platform::driver::{Driver, FrameHost as _, Turn};
use ui_platform::present::{CanvasFrame, PresentStats, SceneTarget};
use ui_platform::session::{HostSession, SessionOptions, Tick};
use ui_platform::text_pass::TextPass;
use ui_platform::view::{App, AppLoop, SceneFrameOutcome};

use crate::frame_pace::FramePace;
use crate::presenter::AtlasUpdate;
use crate::render::{FramePacket, RenderThread};
use crate::window::Win32Window;

/// Window, render thread and driver, owned together so an app never orders them itself.
///
/// The frame's GPU work happens on [`RenderThread`]; this type is what a host holds, and
/// every call on it stays on the host's own thread.
pub struct Session<A: App> {
    /// The device, chain and presents; stopped before the window goes, in `Drop`.
    render: RenderThread,
    driver: Driver,
    lp: AppLoop<A>,
    win: Win32Window,
    /// The surface's frame event, shared with the window and the host's wait set.
    pace: Arc<FramePace>,
    /// The atlas versions the render thread has been sent, so a frame carries only what
    /// changed and a rebuilt device is given everything again.
    atlases: Atlases,
    /// Scripted pointer for the next frame; see [`set_pointer`](Self::set_pointer).
    pointer: Option<Point>,
    /// The app asked for this surface to close.
    closed: bool,
    /// Supersampling the app asked for, sent on to a renderer rebuilt later.
    supersample: u32,
}

/// The two atlas pages' versions, as last handed to the render thread.
///
/// The text starts at the presenter's own `u64::MAX`: a fresh renderer holds no glyphs, so
/// it is owed the first frame's page whatever version that frame reports.
struct Atlases {
    text: u64,
    color: u64,
}

impl Atlases {
    /// Nothing sent yet: a render thread that just opened a chain.
    fn fresh() -> Self {
        Self {
            text: u64::MAX,
            color: 0,
        }
    }

    /// Nothing holds these bytes any more, so the next frame carries both pages.
    fn forget(&mut self) {
        self.text = u64::MAX;
        self.color = u64::MAX;
    }
}

impl<A: App> Session<A> {
    /// Open the window, start the render thread and build the loop the caller owns.
    pub fn open(lp: AppLoop<A>, opts: SessionOptions) -> Result<Self, String> {
        if opts.cpu_presenter {
            return Err(String::from(
                "the Win32 backend presents through D3D11 only; it has no CPU presenter",
            ));
        }
        let mut win = Win32Window::new(opts.surface)
            .map_err(|error| format!("cannot open a Win32 window: {error}"))?;
        // This loop pumps frames inside the system's move loop, so it must start the move.
        win.defer_system_move();
        let pace = Arc::new(FramePace::new());
        let render = RenderThread::open(
            win.visual_hwnd(),
            win.client_size(),
            win.composited(),
            opts.uncapped,
            Arc::clone(&pace),
        )
        .map_err(|error| format!("the D3D11 renderer could not start: {error}"))?;
        win.set_frame_pace(Arc::clone(&pace));
        Ok(Self {
            render,
            driver: Driver::new().uncapped(opts.uncapped),
            lp,
            win,
            pace,
            atlases: Atlases::fresh(),
            pointer: None,
            closed: false,
            supersample: 1,
        })
    }

    pub fn lp(&self) -> &AppLoop<A> {
        &self.lp
    }

    pub fn lp_mut(&mut self) -> &mut AppLoop<A> {
        &mut self.lp
    }

    pub fn window(&self) -> &Win32Window {
        &self.win
    }

    pub fn window_mut(&mut self) -> &mut Win32Window {
        &mut self.win
    }

    /// This surface's frame window, for the host's process-wide wait set.
    pub(crate) fn frame_pace(&self) -> Arc<FramePace> {
        Arc::clone(&self.pace)
    }

    /// Physical pixels per logical unit.
    pub fn scale(&self) -> f32 {
        self.win.scale()
    }

    /// The renderer in use, for the app's own report.
    pub fn presenter_name(&self) -> Option<String> {
        Some(String::from("D3D11"))
    }

    /// What the pointer must be for the next frame, overriding the window's own.
    ///
    /// Scripted input needs it, or the frame snaps back to wherever the real cursor is.
    pub fn set_pointer(&mut self, pointer: Option<Point>) {
        self.pointer = pointer;
    }

    /// Whether the surface asked to close.
    pub fn finished(&self) -> bool {
        self.closed || self.win.should_close()
    }

    /// How long the last turn waited for the compositor.
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

    /// Frames presented since the last call: the rate the compositor actually took.
    pub fn take_frame_callbacks(&mut self) -> u32 {
        self.render.take_presented()
    }

    /// Write the last presented frame as a PPM.
    pub fn dump_frame_ppm(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.render.dump_frame_ppm(path.to_path_buf())
    }

    /// Drain the GPU and stop the render thread; how long this takes is the backlog that
    /// teardown would otherwise inherit. A no-op after the first call.
    pub fn finish(&mut self) {
        self.render.finish();
    }

    /// One non-blocking turn: take the thread's messages, then draw if a frame is due.
    ///
    /// The host's form. It never waits — the queue is the process's and the loop sleeps
    /// once for every surface — so it can be called for each surface of a process in turn.
    pub fn step(&mut self) -> Result<Tick, String> {
        self.win.pump();
        if self.finished() {
            return Ok(Tick::default());
        }
        self.check_render()?;
        if self.win.take_move_request() {
            self.run_native_move();
        }
        let pointer = match self.driver.decide(&mut self.lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        self.draw(pointer)
    }

    /// When the loop must wake with nothing arriving: the engine's own deadlines.
    pub fn next_deadline(&self) -> Option<std::time::Instant> {
        // A frame the render thread has not finished is not a deadline of ours: its event
        // wakes the host. Saying "now" here would spin the loop through a zero-length sleep
        // for the whole of a long frame.
        if !self.win.can_draw() {
            return self.driver.deadline();
        }
        // A turn that drew clears the driver's deadline, yet an animation in flight or a
        // build that read the clock still owes the next frame. Saying "now" is what makes
        // the host come straight back instead of sleeping until the poll timeout.
        let owed = self.lp.is_animating().then(std::time::Instant::now);
        match (owed, self.driver.deadline()) {
            (Some(now), Some(at)) => Some(now.min(at)),
            (now, at) => now.or(at),
        }
    }

    /// Intake, decide, draw and pace — everything a frame needs, waiting included.
    ///
    /// The single-surface form: scripted probes and tests drive one session themselves.
    pub fn tick(&mut self) -> Result<Tick, String> {
        if self.finished() {
            return Ok(Tick::default());
        }
        self.check_render()?;
        let turn = self.driver.step(&mut self.lp, &mut self.win);
        if self.win.take_move_request() {
            self.run_native_move();
        }
        let pointer = match turn {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        let tick = self.draw(pointer)?;
        self.driver.pace(&mut self.win);
        Ok(tick)
    }

    /// Report what the render thread refused, if anything, before a turn tries to draw.
    fn check_render(&self) -> Result<(), String> {
        if self.render.stopped() {
            return Err(self
                .render
                .error()
                .unwrap_or_else(|| String::from("the surface's render thread ended")));
        }
        match self.render.error() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn draw(&mut self, pointer: Option<Point>) -> Result<Tick, String> {
        let tick = self.draw_frame(pointer)?;
        // The render thread presents asynchronously: a refused frame is reported here, one
        // turn late, rather than closing the window from a driver error nobody can see.
        match self.render.error() {
            Some(error) => Err(error),
            None => Ok(tick),
        }
    }

    fn draw_frame(&mut self, pointer: Option<Point>) -> Result<Tick, String> {
        let tick = self.present_frame(pointer)?;
        // A drag that asked to leave the app becomes OLE's; one that highlighted a drop
        // publishes the effect the pointer should show from now on.
        if let Some(drag) = self.lp.take_native_drag() {
            self.run_native_drag(drag);
        }
        self.win.set_drop_effect(self.lp.drop_effect());
        Ok(tick)
    }

    /// Build, layout, paint and hand one frame to the render thread; no drag starts here.
    fn present_frame(&mut self, pointer: Option<Point>) -> Result<Tick, String> {
        // A scripted pointer wins over the window's, which the driver just read.
        let pointer = self.pointer.take().or(pointer);
        if self.win.client_size().0 == 0 || self.win.client_size().1 == 0 {
            // Minimized: there is no client area to draw into, and the turn ends in a wait.
            return Ok(Tick::default());
        }
        if self.win.take_display_change() {
            // Resolution, a monitor or its HDR state changed: the chain's format and colour
            // space were decided from the old output, so the render thread opens a new one.
            self.render
                .rebuild(String::from("the display changed"));
        }
        if self.render.take_atlas_request() {
            self.atlases.forget();
        }
        let Session {
            lp,
            win,
            render,
            pace,
            atlases,
            ..
        } = self;
        let framed = present_once(lp, win, render, pace, atlases, pointer)?;
        // A close the build asked for takes effect in this turn, not the next one.
        self.closed |= self.lp.take_close();
        // A build may ask to move the pointer, which only the window can do.
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

    /// Hand the gesture to OLE, and keep building frames from inside its modal loop.
    ///
    /// `DoDragDrop` does not return until the drag is over, so without this the surface
    /// would hold its last frame for the whole gesture — a drop highlight that never
    /// appears, an animation that stops. The pump OLE's timer calls is one frame of the
    /// same work a turn does, plus the drag phases the drop target queued since the last
    /// one; a still frame costs a build and presents nothing.
    fn run_native_drag(&mut self, drag: ui_platform::view::NativeDrag) {
        let hwnd = self.win.hwnd();
        let queue = self.win.drag_queue();
        let Session {
            lp,
            win,
            render,
            pace,
            atlases,
            closed,
            ..
        } = self;
        let mut pump = || {
            // Nothing to say and nothing moving: the surface already shows the truth.
            if queue.is_empty() && !lp.is_animating() {
                return;
            }
            // The drag the app is carrying may itself be over one of its targets.
            for event in queue.take() {
                lp.dispatch(event);
            }
            let size = win.client_size();
            if size.0 == 0 || size.1 == 0 {
                return;
            }
            let pointer = win.pointer();
            // A failed frame during a drag is not worth ending the surface for: the next
            // turn reports it the ordinary way.
            let _ = present_once(lp, win, render, pace, atlases, pointer);
            *closed |= lp.take_close();
            for (dx, dy) in lp.take_pointer_steps() {
                win.nudge_pointer(dx, dy);
            }
            win.set_drop_effect(lp.drop_effect());
        };
        // The drag runs to its end inside this call; the frame after this one is the first
        // that can show its outcome.
        if !crate::dnd::begin(hwnd, drag, &queue, Some(&mut pump)) {
            // The host refused, so the gesture ends without a drop.
            lp.dispatch(ui_core::Event::Dnd(ui_core::DndEvent::Finished {
                dropped: false,
            }));
            lp.request_frame();
        }
    }

    /// Start the system's title-bar move, and keep building frames inside its modal loop.
    ///
    /// `SendMessage(WM_SYSCOMMAND, SC_MOVE)` does not return until the window is dropped,
    /// exactly like `DoDragDrop`, so the same timer pump keeps an animation alive while the
    /// shell moves the window. There is no queue to serve here — the shell owns the pointer
    /// — and a still surface has nothing to rebuild, so the pump only runs while the app is
    /// animating.
    fn run_native_move(&mut self) {
        let hwnd = self.win.hwnd();
        let Session {
            lp,
            win,
            render,
            pace,
            atlases,
            closed,
            ..
        } = self;
        let mut pump = || {
            if !lp.is_animating() {
                return;
            }
            let size = win.client_size();
            if size.0 == 0 || size.1 == 0 {
                return;
            }
            let pointer = win.pointer();
            let _ = present_once(lp, win, render, pace, atlases, pointer);
            *closed |= lp.take_close();
            for (dx, dy) in lp.take_pointer_steps() {
                win.nudge_pointer(dx, dy);
            }
            win.set_drop_effect(lp.drop_effect());
        };
        let timer = crate::modal::arm(hwnd, &mut pump);
        let moved = crate::window::start_system_move(hwnd);
        crate::modal::disarm(hwnd, timer);
        if moved {
            // The window may have crossed a display, which changes its scale or extent.
            lp.request_frame();
        }
    }
}

impl<A: App> Drop for Session<A> {
    fn drop(&mut self) {
        // The chain belongs to the render thread and the window to this type: the thread
        // must let go of the chain before the window it presents through is destroyed.
        self.finish();
    }
}

/// Build one frame and hand it to the render thread.
///
/// The sink behind the target packages what the renderer needs; the text pass stays here,
/// because it belongs to the loop, and only the atlas bytes it changed cross over.
fn present_once<A: App>(
    lp: &mut AppLoop<A>,
    win: &mut Win32Window,
    render: &RenderThread,
    pace: &Arc<FramePace>,
    atlases: &mut Atlases,
    pointer: Option<Point>,
) -> Result<SceneFrameOutcome, String> {
    let size = win.client_size();
    let scale = win.scale();
    let mut target = Target {
        win,
        sink: Sink {
            render,
            pace,
            atlases,
            size,
            scale,
        },
    };
    lp.frame_scene(&mut target, pointer)
}

/// The Win32 answer to the surface an app is written against; see [`HostSession`].
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
        process_cpu_seconds()
    }

    fn logical_size(&self) -> (u32, u32) {
        self.win.logical_size()
    }

    fn scale(&self) -> f32 {
        Session::scale(self)
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

    fn begin_drag(&mut self, drag: ui_platform::view::NativeDrag) -> bool {
        self.win.begin_drag(drag)
    }

    fn gpu_present(&self) -> bool {
        // Every frame reaches the screen through D3D11; there is no CPU presenter here.
        true
    }

    fn set_supersample(&mut self, factor: u32) {
        self.supersample = factor;
        self.render.set_supersample(factor);
    }

    fn frame_callbacks(&mut self) -> u32 {
        Session::take_frame_callbacks(self)
    }

    fn refresh_hz(&self) -> Option<f32> {
        self.win.refresh_hz()
    }

    fn dmabuf_stats(&self) -> Option<(usize, u32)> {
        // A flip-model swap chain holds no dma-buf.
        None
    }

    fn dump_frame(&mut self, path: &std::path::Path) -> Result<(), String> {
        Session::dump_frame_ppm(self, path)
    }

    fn finish(&mut self) {
        Session::finish(self);
    }
}

/// This process's CPU time from the kernel's process clock, which counts both halves.
fn process_cpu_seconds() -> Option<f64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    fn ticks(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }
    let (mut created, mut exited) = (FILETIME::default(), FILETIME::default());
    let (mut kernel, mut user) = (FILETIME::default(), FILETIME::default());
    // SAFETY: the handle is this process and every out-parameter outlives the call.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .ok()?;
    }
    // A FILETIME counts 100 ns intervals.
    Some((ticks(kernel) + ticks(user)) as f64 / 1e7)
}

/// The frame work that stays on the loop's thread, as the engine's scene target.
///
/// A frame is packaged and handed over here; the window learns what is window state rather
/// than pixel state — the pointer shape, the input region, the input method.
struct Target<'a> {
    win: &'a mut Win32Window,
    sink: Sink<'a>,
}

/// What the render thread is handed: the packet's source and the version bookkeeping.
struct Sink<'a> {
    render: &'a RenderThread,
    pace: &'a Arc<FramePace>,
    atlases: &'a mut Atlases,
    size: (u32, u32),
    scale: f32,
}

impl Sink<'_> {
    fn packet(
        &mut self,
        scene: &DrawList,
        text: &TextPass,
        damage: &Damage,
        canvases: &[CanvasFrame],
    ) -> FramePacket {
        let mut packet = FramePacket {
            scene: scene.clone(),
            damage: damage.clone(),
            canvases: canvases.to_vec(),
            atlas: None,
            color_atlas: None,
            size: self.size,
            scale: self.scale,
        };
        if text.version() != self.atlases.text {
            self.atlases.text = text.version();
            let (width, height) = text.dimensions();
            packet.atlas = Some(AtlasUpdate {
                version: text.version(),
                width,
                height,
                data: text.atlas().to_vec(),
            });
        }
        // The colour page is tracked separately: a new text glyph must not re-upload it.
        if text.color_version() != self.atlases.color {
            self.atlases.color = text.color_version();
            if !text.color_atlas().is_empty() {
                let (width, height) = text.color_dimensions();
                packet.color_atlas = Some(AtlasUpdate {
                    version: text.color_version(),
                    width,
                    height,
                    data: text.color_atlas().to_vec(),
                });
            }
        }
        packet
    }
}

impl SceneTarget for Target<'_> {
    fn size(&self) -> (u32, u32) {
        self.sink.size
    }

    fn present_scene(
        &mut self,
        scene: &DrawList,
        text: &TextPass,
        damage: &Damage,
        canvases: &[CanvasFrame],
    ) -> Result<PresentStats, String> {
        let packet = self.sink.packet(scene, text, damage, canvases);
        // Close the gate before the packet can be finished with: the render thread may
        // signal the moment it has the frame, and a gate closed afterwards would never open.
        self.sink.pace.presented();
        if let Err(error) = self.sink.render.send_frame(packet) {
            self.sink.pace.release();
            return Err(error);
        }
        // The last frame's counts: presentation is the render thread's now, and these feed
        // diagnostics only.
        Ok(self.sink.render.stats())
    }

    fn set_cursor(&mut self, shape: ui_core::CursorShape) {
        self.win.set_cursor(shape);
    }

    fn set_input_region(&mut self, areas: &[ui_core::geom::Rect]) {
        self.win.set_input_region(areas);
    }

    fn set_ime(&mut self, state: ui_platform::present::ImeState) {
        self.win.set_ime(state);
    }
}
