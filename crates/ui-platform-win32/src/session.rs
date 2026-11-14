//! One engine-owned surface on Windows: window, presenter, driver and teardown order.

use std::time::Duration;

use ui_core::Damage;
use ui_core::draw::DrawList;
use ui_core::geom::Point;
use ui_platform::driver::{Driver, Turn};
use ui_platform::present::{CanvasFrame, PresentStats, SceneTarget};
use ui_platform::session::{HostSession, SessionOptions, Tick};
use ui_platform::text_pass::TextPass;
use ui_platform::view::{App, AppLoop};

use crate::presenter::Win32Presenter;
use crate::window::Win32Window;

/// Window, presenter and driver, owned together so an app never orders them itself.
///
/// Dropped in declaration order, which is the order teardown needs: the presenter's GPU
/// objects while the window still exists, then the loop, then the window.
pub struct Session<A: App> {
    /// Absent only while a lost device is being replaced; no returned call observes it.
    presenter: Option<Win32Presenter>,
    driver: Driver,
    lp: AppLoop<A>,
    win: Win32Window,
    /// Scripted pointer for the next frame; see [`set_pointer`](Self::set_pointer).
    pointer: Option<Point>,
    /// The app asked for this surface to close.
    closed: bool,
    /// Supersampling the app asked for, which a rebuilt renderer starts with again.
    supersample: u32,
}

impl<A: App> Session<A> {
    /// Open the window, build the D3D11 device and start the loop the caller owns.
    pub fn open(lp: AppLoop<A>, opts: SessionOptions) -> Result<Self, String> {
        if opts.cpu_presenter {
            return Err(String::from(
                "the Win32 backend presents through D3D11 only; it has no CPU presenter",
            ));
        }
        let win = Win32Window::new(opts.surface)
            .map_err(|error| format!("cannot open a Win32 window: {error}"))?;
        let presenter = Win32Presenter::new(&win).map_err(|error| error.to_string())?;
        Ok(Self {
            presenter: Some(presenter),
            driver: Driver::new().uncapped(opts.uncapped),
            lp,
            win,
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

    /// Physical pixels per logical unit.
    pub fn scale(&self) -> f32 {
        self.win.scale()
    }

    /// The scene renderer behind the swap chain, for knobs such as supersampling.
    ///
    /// `None` only while a lost device is being replaced, which no call in between can see.
    pub fn renderer_mut(&mut self) -> Option<&mut ui_gpu_scene::GpuRenderer> {
        self.presenter
            .as_mut()
            .map(|presenter| presenter.renderer_mut())
    }

    /// All three at once, for host code that needs the engine and the window together.
    pub fn split(
        &mut self,
    ) -> (
        &mut AppLoop<A>,
        &mut Win32Window,
        Option<&mut Win32Presenter>,
    ) {
        (&mut self.lp, &mut self.win, self.presenter.as_mut())
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
        self.presenter
            .as_mut()
            .map_or(0, Win32Presenter::take_presented)
    }

    /// Write the last composited frame as a PPM.
    pub fn dump_frame_ppm(&mut self, path: &std::path::Path) -> Result<(), String> {
        let presenter = self
            .presenter
            .as_mut()
            .ok_or("the surface has no presenter to read back from")?;
        presenter.dump_frame_ppm(path)
    }

    /// Drain the GPU; how long this takes is the backlog teardown would otherwise inherit.
    pub fn finish(&mut self) {
        if let Some(presenter) = &mut self.presenter {
            presenter.finish();
        }
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
        let pointer = match self.driver.decide(&mut self.lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        self.draw(pointer)
    }

    /// When the loop must wake with nothing arriving: the engine's own deadlines.
    pub fn next_deadline(&self) -> Option<std::time::Instant> {
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
        let pointer = match self.driver.step(&mut self.lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        let tick = self.draw(pointer)?;
        self.driver.pace(&mut self.win);
        Ok(tick)
    }

    fn draw(&mut self, pointer: Option<Point>) -> Result<Tick, String> {
        let failed = match self.draw_frame(pointer) {
            Ok(tick) => return Ok(tick),
            Err(failed) => failed,
        };
        // A driver update or a lost adapter takes the device and chain with it; the surface
        // rebuilds them and draws again rather than closing the window.
        let Some(reason) = self
            .presenter
            .as_ref()
            .and_then(Win32Presenter::device_loss)
        else {
            return Err(failed);
        };
        self.rebuild_presenter(&reason)?;
        self.draw_frame(pointer)
            .map_err(|error| format!("{failed}; rebuilt the device ({reason}): {error}"))
    }

    /// Replace the device and swap chain with a fresh stack.
    ///
    /// Two things ask for it: a device DXGI no longer answers for, and a display change that
    /// can leave the chain in a colour space its new output does not present.
    fn rebuild_presenter(&mut self, reason: &str) -> Result<(), String> {
        // A window holds one flip chain at a time, so the old stack goes before the new one
        // is built; this is the only moment the surface has no presenter.
        self.presenter = None;
        let mut presenter = Win32Presenter::new(&self.win).map_err(|error| {
            format!("the presenter could not be rebuilt after {reason}: {error}")
        })?;
        presenter.renderer_mut().set_supersample(self.supersample);
        self.presenter = Some(presenter);
        Ok(())
    }

    fn draw_frame(&mut self, pointer: Option<Point>) -> Result<Tick, String> {
        // A scripted pointer wins over the window's, which the driver just read.
        let pointer = self.pointer.take().or(pointer);
        let size = self.win.client_size();
        if size.0 == 0 || size.1 == 0 {
            // Minimized: there is no client area to draw into, and the turn ends in a wait.
            return Ok(Tick::default());
        }
        if self.win.take_display_change() {
            self.rebuild_presenter("the display changed")?;
        }
        let presenter = self
            .presenter
            .as_mut()
            .ok_or("the surface has no presenter")?;
        presenter
            .resize(size, self.win.scale())
            .map_err(|error| error.to_string())?;
        let framed = {
            let mut target = Target {
                win: &mut self.win,
                presenter,
            };
            self.lp.frame_scene(&mut target, pointer)?
        };
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
        // A drag that asked to leave the app becomes OLE's; one that highlighted a drop
        // publishes the effect the pointer should show from now on.
        if let Some(data) = self.lp.take_native_drag()
            && !self.win.begin_drag(data)
        {
            // The host refused, so the gesture ends without a drop.
            self.lp
                .dispatch(ui_core::Event::Dnd(ui_core::DndEvent::Finished {
                    dropped: false,
                }));
            self.lp.request_frame();
        }
        self.win.set_drop_effect(self.lp.drop_effect());
        Ok(Tick {
            drew: true,
            outcome: Some(framed.outcome),
            presented: framed.presented,
            pointer,
        })
    }
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

    fn begin_drag(&mut self, data: ui_core::DragData) -> bool {
        self.win.begin_drag(data)
    }

    fn gpu_present(&self) -> bool {
        // Every frame reaches the screen through D3D11; there is no CPU presenter here.
        true
    }

    fn set_supersample(&mut self, factor: u32) {
        self.supersample = factor;
        if let Some(presenter) = &mut self.presenter {
            presenter.renderer_mut().set_supersample(factor);
        }
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

/// The presenter and the window as one scene target: the frame goes to the swap chain, and
/// what is window state rather than pixel state — the pointer shape — reaches the window.
struct Target<'a> {
    win: &'a mut Win32Window,
    presenter: &'a mut Win32Presenter,
}

impl SceneTarget for Target<'_> {
    fn size(&self) -> (u32, u32) {
        self.presenter.size()
    }

    fn present_scene(
        &mut self,
        scene: &DrawList,
        text: &TextPass,
        damage: &Damage,
        canvases: &[CanvasFrame],
    ) -> Result<PresentStats, String> {
        self.presenter.present_scene(scene, text, damage, canvases)
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
