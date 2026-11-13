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
    presenter: Win32Presenter,
    driver: Driver,
    lp: AppLoop<A>,
    win: Win32Window,
    /// Scripted pointer for the next frame; see [`set_pointer`](Self::set_pointer).
    pointer: Option<Point>,
    /// The app asked for this surface to close.
    closed: bool,
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
            presenter,
            driver: Driver::new().uncapped(opts.uncapped),
            lp,
            win,
            pointer: None,
            closed: false,
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
    pub fn renderer_mut(&mut self) -> &mut ui_gpu_scene::GpuRenderer {
        self.presenter.renderer_mut()
    }

    /// All three at once, for host code that needs the engine and the window together.
    pub fn split(&mut self) -> (&mut AppLoop<A>, &mut Win32Window, Option<&mut Win32Presenter>) {
        (&mut self.lp, &mut self.win, Some(&mut self.presenter))
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
        self.presenter.take_presented()
    }

    /// Write the last composited frame as a PPM.
    pub fn dump_frame_ppm(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.presenter.dump_frame_ppm(path)
    }

    /// Drain the GPU; how long this takes is the backlog teardown would otherwise inherit.
    pub fn finish(&mut self) {
        self.presenter.finish();
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
        // A scripted pointer wins over the window's, which the driver just read.
        let pointer = self.pointer.take().or(pointer);
        let size = self.win.client_size();
        if size.0 == 0 || size.1 == 0 {
            // Minimized: there is no client area to draw into, and the turn ends in a wait.
            return Ok(Tick::default());
        }
        self.presenter
            .resize(size, self.win.scale())
            .map_err(|error| error.to_string())?;
        let framed = {
            let mut target = Target {
                win: &mut self.win,
                presenter: &mut self.presenter,
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

    fn gpu_present(&self) -> bool {
        // Every frame reaches the screen through D3D11; there is no CPU presenter here.
        true
    }

    fn set_supersample(&mut self, factor: u32) {
        self.presenter.renderer_mut().set_supersample(factor);
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
}
