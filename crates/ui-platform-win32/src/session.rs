//! One engine-owned surface on Windows: window, render thread, driver and teardown order.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Weak};
use std::time::Duration;

use ui_core::Damage;
use ui_core::NodeId;
use ui_core::draw::DrawList;
use ui_core::geom::Point;
use ui_platform::chooser::{
    ChooserAnswer, ChooserCapabilities, ChooserError, ChooserId, ChooserRequest,
};
use ui_platform::clipboard::{ClipboardCommand, ClipboardCopy};
use ui_platform::driver::{Driver, FrameHost as _, Turn};
use ui_platform::host::{Engine, PresentationOptions, SurfaceHandle, SurfaceStep};
use ui_platform::present::{CanvasFrame, PresentStats, SceneTarget};
use ui_platform::session::{HostSession, SessionOptions, Tick};
use ui_platform::text_pass::TextPass;
use ui_platform::view::{App, AppLoop, SceneFrameOutcome};
use ui_platform::window::{PopupSpec, SurfaceRole, SurfaceSpec};

use crate::frame_pace::FramePace;
use crate::presenter::AtlasUpdate;
use crate::render::{FramePacket, RenderThread};
use crate::window::Win32Window;

/// One Win32 surface as a presenter: window, render thread, driver and its own input.
///
/// The frame's GPU work happens on [`RenderThread`]; this type is what a host holds, and
/// every call on it stays on the host's own thread.
pub struct Presenter {
    /// The device, chain and presents; stopped before the window goes, in `Drop`.
    render: RenderThread,
    driver: Driver,
    win: Win32Window,
    /// The surface's frame event, shared with the window and the host's wait set.
    pace: Arc<FramePace>,
    /// Scripted pointer for the next frame; see [`set_pointer`](Self::set_pointer).
    pointer: Option<Point>,
    /// The app asked for this surface to close.
    closed: bool,
    /// Supersampling the app asked for, sent on to a renderer rebuilt later.
    supersample: u32,
    /// What the window was opened as, so a panel inside it is named the same way.
    spec: SurfaceSpec,
    /// Whether frames are presented without the display's pacing, for the panels too.
    uncapped: bool,
    /// Every surface's frame event, which the host's one sleep waits on. A panel opened
    /// later adds its own, or its next frame would wait for an event nothing is waiting for.
    paces: Rc<RefCell<Vec<Weak<FramePace>>>>,
    /// The window a shell context menu belongs to, shared with the backend when there is one.
    menu_owner: crate::context_menu::MenuOwner,
    /// Answers the native dialogs produced, drained by the host on its next turn.
    chooser_answers: Vec<(ChooserId, ChooserAnswer)>,
    /// A platform menu a build asked for, held until the next turn.
    ///
    /// The host reconciles the engine's panels after a step, and the menu the user picked
    /// from must leave the screen before the platform's popup takes the input.
    pending_shell_menu: Option<ui_core::cx::ShellMenuRequest>,
}

/// An app and the one surface it drives, for code that runs a single window itself.
pub struct Session<A: App> {
    lp: AppLoop<A>,
    presenter: Presenter,
}

impl Presenter {
    /// Open the window and start the render thread.
    pub fn open(
        opts: SessionOptions,
        paces: Rc<RefCell<Vec<Weak<FramePace>>>>,
        menu_owner: crate::context_menu::MenuOwner,
    ) -> Result<Self, String> {
        if opts.cpu_presenter {
            return Err(String::from(
                "the Win32 backend presents through D3D11 only; it has no CPU presenter",
            ));
        }
        log::debug!("ui-platform-win32: creating the window for a surface");
        let win = Win32Window::new(opts.surface.clone())
            .map_err(|error| format!("cannot open a Win32 window: {error}"))?;
        log::debug!("ui-platform-win32: the window is up; asking for a chain");
        Self::with_window(win, opts.surface, opts.uncapped, paces, menu_owner)
    }

    /// Wrap an already created window in a presenter and start its render thread.
    fn with_window(
        mut win: Win32Window,
        spec: SurfaceSpec,
        uncapped: bool,
        paces: Rc<RefCell<Vec<Weak<FramePace>>>>,
        menu_owner: crate::context_menu::MenuOwner,
    ) -> Result<Self, String> {
        let pace = Arc::new(FramePace::new());
        let render = RenderThread::open(
            win.visual_hwnd(),
            win.client_size(),
            win.alpha(),
            uncapped,
            win.refresh_hz(),
            Arc::clone(&pace),
        )
        .map_err(|error| format!("the D3D11 renderer could not start: {error}"))?;
        win.set_frame_pace(Arc::clone(&pace));
        // Only a window that will exist hands menus its handle; a failed renderer above
        // leaves the previous owner in the slot.
        crate::context_menu::set_owner(&menu_owner, win.hwnd());
        Ok(Self {
            render,
            driver: Driver::new().uncapped(uncapped),
            win,
            pace,
            pointer: None,
            closed: false,
            supersample: 1,
            spec,
            uncapped,
            paces,
            menu_owner,
            chooser_answers: Vec::new(),
            pending_shell_menu: None,
        })
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

    /// Render at `factor` device pixels per logical pixel.
    pub fn set_supersample(&mut self, factor: u32) {
        self.supersample = factor;
        self.render.set_supersample(factor);
    }

    /// One non-blocking turn: take the thread's messages, then draw if a frame is due.
    ///
    /// The host's form. It never waits — the queue is the process's and the loop sleeps
    /// once for every surface — so it can be called for each surface of a process in turn.
    pub fn step_with<A: App>(&mut self, lp: &mut AppLoop<A>) -> Result<Tick, String> {
        // The shell's move or resize loop owns this window until the pointer comes up. Its
        // messages are dispatched by that loop, so pumping here would take messages out
        // from under it; the surface keeps drawing from the loop's timer instead.
        if self.win.in_size_move() {
            return Ok(Tick::default());
        }
        self.under_move_pump(lp, Presenter::step_body)
    }

    /// Run `step` with this window's own frame pump answering the shell's move-loop timer.
    ///
    /// The host answers that tick for a surface still in its list — it asks
    /// [`moving`](Self::moving) and calls [`pump_moved_with`](Self::pump_moved_with). This
    /// covers the other half: a title-bar press that arrives inside the step itself, when
    /// the surface has been taken out of that list so its own step could run. A step
    /// dispatches from more than one place — the window's pump and the driver's intake — so
    /// the pump has to be armed before the step starts, not around the call that happens to
    /// see the press.
    ///
    /// The pump reads the presenter and the engine through raw pointers because the step
    /// holds both mutably across the same call stack. That is the one alias this module
    /// allows: the closure is cleared before the step returns, and while the move loop is on
    /// the stack the step's own borrows are not used — they are blocked inside the message
    /// dispatch that started it.
    fn under_move_pump<A: App>(
        &mut self,
        lp: &mut AppLoop<A>,
        body: fn(&mut Self, &mut AppLoop<A>) -> Result<Tick, String>,
    ) -> Result<Tick, String> {
        let presenter: *mut Self = self;
        let engine: *mut AppLoop<A> = lp;
        // SAFETY: see above — both pointers are only called while this step is on the stack,
        // and only from the shell's loop, which is the step's own dispatch.
        let mut pump = || {
            let _ = unsafe { (*presenter).pump_moved_with(&mut *engine) };
        };
        let held = crate::modal::attach(&mut pump);
        let stepped = body(self, lp);
        crate::modal::detach(held);
        stepped
    }

    fn step_body<A: App>(&mut self, lp: &mut AppLoop<A>) -> Result<Tick, String> {
        self.win.pump();
        if self.finished() {
            return Ok(Tick::default());
        }
        self.check_render()?;
        let pointer = match self.driver.decide(lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        self.draw(lp, pointer)
    }

    /// Whether the shell's move or resize loop owns this surface right now.
    ///
    /// The host reads it: such a surface draws from the loop's timer and cannot take a step
    /// of its own, because the queue that step would read is the loop's.
    pub fn moving(&self) -> bool {
        self.win.in_size_move()
    }

    /// When the loop must wake with nothing arriving: the engine's own deadlines.
    pub fn next_deadline_with<A: App>(&self, lp: &AppLoop<A>) -> Option<std::time::Instant> {
        // A move loop the shell is running owes nothing of its own: its timer is what
        // steps the rest of the process, and a deadline here would only spin the host.
        if self.win.in_size_move() {
            return None;
        }
        // A frame the render thread has not finished is not a deadline of ours: its event
        // wakes the host. Saying "now" here would spin the loop through a zero-length sleep
        // for the whole of a long frame.
        if !self.win.can_draw() {
            return self.driver.deadline();
        }
        // A turn that drew clears the driver's deadline, yet an animation in flight or a
        // build that read the clock still owes the next frame. Saying "now" is what makes
        // the host come straight back instead of sleeping until the poll timeout.
        let owed = lp.is_animating().then(std::time::Instant::now);
        match (owed, self.driver.deadline()) {
            (Some(now), Some(at)) => Some(now.min(at)),
            (now, at) => now.or(at),
        }
    }

    /// Intake, decide, draw and pace — everything a frame needs, waiting included.
    ///
    /// The single-surface form: scripted probes and tests drive one session themselves.
    pub fn tick_with<A: App>(&mut self, lp: &mut AppLoop<A>) -> Result<Tick, String> {
        if self.finished() {
            return Ok(Tick::default());
        }
        // A move or resize loop the shell is running has this thread; a turn here would
        // wait for a message the loop is dispatching.
        if self.win.in_size_move() {
            return Ok(Tick::default());
        }
        self.under_move_pump(lp, Presenter::tick_body)
    }

    fn tick_body<A: App>(&mut self, lp: &mut AppLoop<A>) -> Result<Tick, String> {
        self.check_render()?;
        let turn = self.driver.step(lp, &mut self.win);
        let pointer = match turn {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return Ok(Tick::default()),
        };
        let tick = self.draw(lp, pointer)?;
        self.driver.pace(&mut self.win);
        Ok(tick)
    }

    /// Draw this surface from the shell's move-loop timer.
    ///
    /// `DefWindowProc` does not return until the pointer comes up, so this tick is the only
    /// chance the engine gets to build a frame for the window it is moving. Without it the
    /// moved surface holds its last frame for the whole drag while every other surface of
    /// the process draws from the same tick — the window would keep following the pointer
    /// with yesterday's contents in it, which is what a user sees as the window freezing.
    ///
    /// Nothing may block here: this thread is the one that moves the window, so a wait in
    /// this tick stops the window under the pointer until the wait ends. A frame the chain
    /// refuses is simply retried on the next tick, the timer being periodic.
    pub fn pump_moved_with<A: App>(&mut self, lp: &mut AppLoop<A>) -> bool {
        // The same question the loop asks: something the app or the clock reached owes a
        // frame, and a still surface owes none.
        let pointer = match self.driver.decide(lp, &mut self.win) {
            Turn::Draw { pointer } => pointer,
            Turn::Idle | Turn::Waiting => return false,
        };
        let (width, height) = self.win.client_size();
        if width == 0 || height == 0 || !self.win.can_draw() {
            return false;
        }
        lp.set_output_bounds(self.win.work_area());
        let drew = present_once(lp, &mut self.win, &self.render, &self.pace, pointer).is_ok();
        self.closed |= lp.take_close();
        for (dx, dy) in lp.take_pointer_steps() {
            self.win.nudge_pointer(dx, dy);
        }
        self.win.set_drop_effect(lp.drop_effect());
        drew
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

    fn draw<A: App>(
        &mut self,
        lp: &mut AppLoop<A>,
        pointer: Option<Point>,
    ) -> Result<Tick, String> {
        let tick = self.draw_frame(lp, pointer)?;
        // The render thread presents asynchronously: a refused frame is reported here, one
        // turn late, rather than closing the window from a driver error nobody can see.
        match self.render.error() {
            Some(error) => Err(error),
            None => Ok(tick),
        }
    }

    fn draw_frame<A: App>(
        &mut self,
        lp: &mut AppLoop<A>,
        pointer: Option<Point>,
    ) -> Result<Tick, String> {
        // The tree constrains platform panels against the monitor, and the surfaces this
        // window owns stand where the tree placed them; a moved window changes the bound.
        lp.set_output_bounds(self.win.work_area());
        // A menu the previous build asked for runs before this turn's frame: the host has
        // reconciled the panels since then, so the engine's own menu is already gone.
        if let Some(menu) = self.pending_shell_menu.take() {
            self.run_shell_menu(lp, menu);
        }
        let tick = self.present_frame(lp, pointer)?;
        // A drag that asked to leave the app becomes OLE's; one that highlighted a drop
        // publishes the effect the pointer should show from now on.
        if let Some(drag) = lp.take_native_drag() {
            self.run_native_drag(lp, drag);
        }
        // The platform's popup owns the user's input until it closes, so it runs on the turn
        // after the build that asked: by then the host has reconciled the engine's panels,
        // and the menu the user picked from is off screen under the popup.
        if let Some(menu) = lp.take_shell_menu() {
            self.pending_shell_menu = Some(menu);
            lp.request_frame();
        }
        self.win.set_drop_effect(lp.drop_effect());
        Ok(tick)
    }

    /// Run the platform menu a build asked for, at the start of a turn.
    ///
    /// The popup's loop is the shell's own and does not return until the user dismisses it;
    /// a build that opened it would stay unpainted and the host would never reconcile. This
    /// window holds the frame it has for the whole popup, and the rest of the process keeps
    /// drawing from the loop's timer.
    fn run_shell_menu<A: App>(&mut self, lp: &mut AppLoop<A>, menu: ui_core::cx::ShellMenuRequest) {
        use ui_platform::shell_menu::ShellMenuHost as _;
        let host = crate::context_menu::Win32ShellMenu::for_window(self.win.hwnd());
        let mut pump = || {};
        let timer = crate::modal::arm(self.win.hwnd(), &mut pump);
        let shown = host.show_native(&menu.paths, (menu.at.x, menu.at.y));
        crate::modal::disarm(self.win.hwnd(), timer);
        if shown {
            // A shell verb may have changed the folder; the next build reads it.
            lp.request_frame();
        }
    }

    /// Build, layout, paint and hand one frame to the render thread; no drag starts here.
    fn present_frame<A: App>(
        &mut self,
        lp: &mut AppLoop<A>,
        pointer: Option<Point>,
    ) -> Result<Tick, String> {
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
                .rebuild(String::from("the display changed"), self.win.refresh_hz());
        }
        let framed = {
            let Presenter {
                win, render, pace, ..
            } = self;
            present_once(lp, win, render, pace, pointer)?
        };
        // A close the build asked for takes effect in this turn, not the next one.
        self.closed |= lp.take_close();
        // A build may ask to move the pointer, which only the window can do.
        let steps = lp.take_pointer_steps();
        for (dx, dy) in &steps {
            self.win.nudge_pointer(*dx, *dy);
        }
        // The frame just built read the pointer's old place, so one more shows the new one.
        if !steps.is_empty() {
            lp.request_frame();
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
    fn run_native_drag<A: App>(
        &mut self,
        lp: &mut AppLoop<A>,
        drag: ui_platform::view::NativeDrag,
    ) {
        let hwnd = self.win.hwnd();
        let queue = self.win.drag_queue();
        let Presenter {
            win,
            render,
            pace,
            closed,
            ..
        } = self;
        let mut pump = || {
            // The wheel still has to reach the scroller under the pointer: OLE's loop
            // dispatches it to this window, but no turn drains the queue while it runs.
            let mut wheeled = false;
            for event in win.take_wheel_events() {
                lp.dispatch(event);
                wheeled = true;
            }
            if wheeled {
                lp.request_frame();
            }
            // Nothing to say and nothing moving: the surface already shows the truth.
            if queue.is_empty() && !lp.is_animating() && !wheeled {
                return;
            }
            // A frame still in flight means the gate is shut, and the hand-over would block
            // until the render thread takes it. This pump runs inside `DoDragDrop`, so a
            // block here stops the gesture itself — the pointer stops moving the drag. The
            // queue waits for the next tick: a phase consumed now would be drawn by no
            // frame, and OLE reports a still pointer only once.
            if !win.can_draw() {
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
            let _ = present_once(lp, win, render, pace, pointer);
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
                outcome: None,
            }));
            lp.request_frame();
        }
    }
}

impl<A: App> Session<A> {
    /// Open the window, start the render thread and build the loop the caller owns.
    pub fn open(lp: AppLoop<A>, opts: SessionOptions) -> Result<Self, String> {
        Ok(Self {
            lp,
            // A single surface drives itself: nothing else would step a panel it opened.
            presenter: Presenter::open(
                opts,
                Rc::new(RefCell::new(Vec::new())),
                crate::context_menu::empty_owner(),
            )?,
        })
    }

    pub fn lp(&self) -> &AppLoop<A> {
        &self.lp
    }

    pub fn lp_mut(&mut self) -> &mut AppLoop<A> {
        &mut self.lp
    }

    pub fn window(&self) -> &Win32Window {
        self.presenter.window()
    }

    pub fn window_mut(&mut self) -> &mut Win32Window {
        self.presenter.window_mut()
    }

    /// Physical pixels per logical unit.
    pub fn scale(&self) -> f32 {
        self.presenter.scale()
    }

    /// The renderer in use, for the app's own report.
    pub fn presenter_name(&self) -> Option<String> {
        self.presenter.presenter_name()
    }

    /// What the pointer must be for the next frame, overriding the window's own.
    ///
    /// Scripted input needs it, or the frame snaps back to wherever the real cursor is.
    pub fn set_pointer(&mut self, pointer: Option<Point>) {
        self.presenter.set_pointer(pointer);
    }

    /// Whether the surface asked to close.
    pub fn finished(&self) -> bool {
        self.presenter.finished()
    }

    /// How long the last turn waited for the compositor.
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

    /// Frames presented since the last call: the rate the compositor actually took.
    pub fn take_frame_callbacks(&mut self) -> u32 {
        self.presenter.take_frame_callbacks()
    }

    /// Write the last presented frame as a PPM.
    pub fn dump_frame_ppm(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.presenter.dump_frame_ppm(path)
    }

    /// Drain the GPU and stop the render thread; how long this takes is the backlog that
    /// teardown would otherwise inherit. A no-op after the first call.
    pub fn finish(&mut self) {
        self.presenter.finish();
    }

    /// One non-blocking turn: take the thread's messages, then draw if a frame is due.
    pub fn step(&mut self) -> Result<Tick, String> {
        self.presenter.step_with(&mut self.lp)
    }

    /// Whether the shell's move or resize loop owns this surface right now.
    pub fn moving(&self) -> bool {
        self.presenter.moving()
    }

    /// When the loop must wake with nothing arriving: the engine's own deadlines.
    pub fn next_deadline(&self) -> Option<std::time::Instant> {
        self.presenter.next_deadline_with(&self.lp)
    }

    /// Intake, decide, draw and pace — everything a frame needs, waiting included.
    ///
    /// The single-surface form: scripted probes and tests drive one session themselves.
    pub fn tick(&mut self) -> Result<Tick, String> {
        self.presenter.tick_with(&mut self.lp)
    }

    /// Draw this surface from the shell's move-loop timer; see [`Presenter::pump_moved_with`].
    pub fn pump_moved(&mut self) -> bool {
        self.presenter.pump_moved_with(&mut self.lp)
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        // A menu must not keep a handle to a window that is about to go.
        crate::context_menu::clear_owner(&self.menu_owner, self.win.hwnd());
        // The chain belongs to the render thread and the window to this type: the thread
        // must let go of the chain before the window it presents through is destroyed.
        self.finish();
    }
}

impl SurfaceHandle for Presenter {
    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        let tick = self.step_with(engine)?;
        Ok(SurfaceStep { drew: tick.drew })
    }

    fn pointer(&self) -> Option<Point> {
        self.win.pointer()
    }

    fn moving(&self) -> bool {
        Presenter::moving(self)
    }

    fn pump_moved(&mut self, engine: &mut Engine) -> bool {
        self.pump_moved_with(engine)
    }

    fn next_deadline(&self, engine: &Engine) -> Option<std::time::Instant> {
        self.next_deadline_with(engine)
    }

    fn is_closed(&self) -> bool {
        self.finished()
    }

    fn take_copies(&mut self, engine: &mut Engine) -> Vec<ClipboardCopy> {
        let mut copies = Vec::new();
        for command in engine.take_clipboard_commands() {
            match command {
                ClipboardCommand::Copy { content, kind } => copies.push(ClipboardCopy {
                    content,
                    kind,
                    // A copy here is the clipboard itself; there is no serial to serve under.
                    serial: 0,
                }),
                // Windows has no primary selection, so a paste is always the clipboard's.
                ClipboardCommand::RequestPaste { .. } => {
                    let _ = self.win.request_paste();
                }
            }
        }
        copies
    }

    /// A panel hung off this surface gets a window and a renderer of its own.
    fn open_popup(
        &mut self,
        node: NodeId,
        spec: PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let root = Point::new(0.0, 0.0);
        let presenter = open_panel(
            &self.win,
            &self.spec,
            root,
            node,
            spec,
            self.uncapped,
            &self.paces,
        )?;
        Ok(Box::new(presenter))
    }

    /// A panel presented as a window: a decorated, non-modal child of this surface.
    ///
    /// The panel still belongs to this surface's tree, so the window is a view of that
    /// subtree rather than an app of its own.
    fn open_panel_window(
        &mut self,
        node: NodeId,
        spec: PopupSpec,
        title: &str,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let root = Point::new(0.0, 0.0);
        let presenter = open_panel_window(
            &self.win,
            &self.spec,
            root,
            node,
            spec,
            title,
            self.uncapped,
            &self.paces,
        )?;
        Ok(Box::new(presenter))
    }

    /// A dialog belonging to this surface: an owned window, modal over it when asked.
    fn open_dialog(
        &mut self,
        spec: SurfaceSpec,
        options: PresentationOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let win = Win32Window::new_dialog(&self.win, spec.clone())
            .map_err(|error| format!("cannot open a Win32 dialog: {error}"))?;
        let presenter = Presenter::with_window(
            win,
            spec,
            options.uncapped,
            Rc::clone(&self.paces),
            Arc::clone(&self.menu_owner),
        )?;
        // The host's wait must end when the dialog's render thread finishes a frame.
        self.paces
            .borrow_mut()
            .push(Arc::downgrade(&presenter.frame_pace()));
        Ok(Box::new(presenter))
    }

    fn chooser_capabilities(&self) -> Option<ChooserCapabilities> {
        Some(crate::chooser::capabilities())
    }

    /// The dialog runs its own message loop until the user answers; the host has already
    /// held the parent's input, so blocking this turn is the intended shape.
    fn choose(&mut self, id: ChooserId, request: ChooserRequest) -> Result<(), ChooserError> {
        let answer = crate::chooser::choose(self.win.hwnd(), &request);
        self.chooser_answers
            .push((id, ChooserAnswer::native(answer)));
        Ok(())
    }

    fn chooser_events(&mut self) -> Vec<(ChooserId, ChooserAnswer)> {
        std::mem::take(&mut self.chooser_answers)
    }
}

/// A panel's own window: the engine paints it, this presenter shows it and routes its input.
///
/// The panel's subtree lives in the parent's tree, so this is not a second engine: what
/// arrives here is shifted back into that tree and dispatched to the same app.
struct PopupPresenter {
    render: RenderThread,
    win: Win32Window,
    pace: Arc<FramePace>,
    /// What the popup was opened as, so a panel inside it is named the same way.
    spec: SurfaceSpec,
    node: NodeId,
    /// Whether frames are presented without the display's pacing.
    uncapped: bool,
    /// Every surface's frame event, which a nested panel adds its own to.
    paces: Rc<RefCell<Vec<Weak<FramePace>>>>,
    /// The tree origin of the surface this popup hangs from.
    parent_origin: Point,
    /// Where the panel's subtree stands in the parent tree's coordinates.
    origin: Point,
    /// The build whose panel list this window last showed.
    presented: Option<u64>,
    /// The panel went away or the shell took it back; the host dismisses it.
    closed: bool,
    /// Whether this surface is a window rather than a popup: a window keeps its own place
    /// and only takes the size a changed panel asks for.
    window: bool,
}

/// The surface spec a panel's window is opened with.
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

/// Open a popup owned by `parent` that shows the panel `node`.
///
/// `spec` is in the parent presentation's coordinates, so the panel's place in the tree is
/// that offset from `parent_origin`.
fn open_panel(
    parent: &Win32Window,
    parent_spec: &SurfaceSpec,
    parent_origin: Point,
    node: NodeId,
    spec: PopupSpec,
    uncapped: bool,
    paces: &Rc<RefCell<Vec<Weak<FramePace>>>>,
) -> Result<PopupPresenter, String> {
    let surface = popup_surface_spec(parent_spec, spec);
    let mut win = Win32Window::new_popup(parent, surface.clone(), spec)
        .map_err(|error| format!("cannot open a panel's window: {error}"))?;
    let pace = Arc::new(FramePace::new());
    let render = RenderThread::open(
        win.visual_hwnd(),
        win.client_size(),
        win.alpha(),
        uncapped,
        win.refresh_hz(),
        Arc::clone(&pace),
    )
    .map_err(|error| format!("the panel's renderer could not start: {error}"))?;
    win.set_frame_pace(Arc::clone(&pace));
    paces.borrow_mut().push(Arc::downgrade(&pace));
    Ok(PopupPresenter {
        render,
        win,
        pace,
        spec: surface,
        node,
        uncapped,
        paces: Rc::clone(paces),
        parent_origin,
        origin: parent_origin + spec.rect.origin,
        presented: None,
        closed: false,
        window: false,
    })
}

/// Open a non-modal window owned by `parent` that shows the panel `node`.
///
/// The platform decides where a modeless child stands, so the panel's rect supplies the
/// client size and its own place is not imposed on the window.
fn open_panel_window(
    parent: &Win32Window,
    parent_spec: &SurfaceSpec,
    parent_origin: Point,
    node: NodeId,
    spec: PopupSpec,
    title: &str,
    uncapped: bool,
    paces: &Rc<RefCell<Vec<Weak<FramePace>>>>,
) -> Result<PopupPresenter, String> {
    let size = (
        spec.rect.size.w.round().max(1.0) as u32,
        spec.rect.size.h.round().max(1.0) as u32,
    );
    let surface = SurfaceSpec::dialog(parent_spec.app_id.clone(), title, size).with_modal(false);
    let mut win = Win32Window::new_panel_dialog(parent, surface.clone())
        .map_err(|error| format!("cannot open the panel's window: {error}"))?;
    let pace = Arc::new(FramePace::new());
    let render = RenderThread::open(
        win.visual_hwnd(),
        win.client_size(),
        win.alpha(),
        uncapped,
        win.refresh_hz(),
        Arc::clone(&pace),
    )
    .map_err(|error| format!("the panel's renderer could not start: {error}"))?;
    win.set_frame_pace(Arc::clone(&pace));
    paces.borrow_mut().push(Arc::downgrade(&pace));
    Ok(PopupPresenter {
        render,
        win,
        pace,
        spec: surface,
        node,
        uncapped,
        paces: Rc::clone(paces),
        parent_origin,
        origin: parent_origin + spec.rect.origin,
        presented: None,
        closed: false,
        window: true,
    })
}

/// Move an event's position from a panel's own space into the parent tree's.
fn shift(event: ui_core::Event, by: Point) -> ui_core::Event {
    use ui_core::Event;
    match event {
        Event::PointerMoved { pos } => Event::PointerMoved { pos: pos + by },
        Event::PointerPressed { pos, button } => Event::PointerPressed {
            pos: pos + by,
            button,
        },
        Event::PointerReleased { pos, button } => Event::PointerReleased {
            pos: pos + by,
            button,
        },
        Event::Scroll { pos, delta } => Event::Scroll {
            pos: pos + by,
            delta,
        },
        other => other,
    }
}

impl PopupPresenter {
    fn present(&mut self, engine: &Engine, frame: u64) -> Result<(), String> {
        let size = self.win.client_size();
        if size.0 == 0 || size.1 == 0 {
            return Ok(());
        }
        let scale = self.win.scale();
        let node = self.node;
        let PopupPresenter {
            win, render, pace, ..
        } = self;
        let mut target = Target {
            win,
            sink: Sink {
                render,
                pace,
                size,
                scale,
            },
        };
        engine
            .present_panel(node, &mut target)
            .map_err(|error| format!("present failed: {error}"))?;
        self.presented = Some(frame);
        Ok(())
    }
}

impl SurfaceHandle for PopupPresenter {
    fn intake(&mut self, engine: &mut Engine) -> Result<(), String> {
        // The queue is the thread's, so a pump here takes whatever arrived for this window
        // as well as for the root's; each window keeps only what its own procedure stored.
        self.win.pump();
        if self.win.should_close() {
            self.closed = true;
            return Ok(());
        }
        // A frame the render thread refused is reported here, one turn late, rather than
        // leaving the panel showing its last one forever.
        if let Some(error) = self.render.error() {
            return Err(error);
        }
        let origin = self.origin;
        for event in self.win.take_events() {
            engine.dispatch(shift(event, origin));
        }
        // The shape the tree gives the pointer's place on the panel; the pointer is the
        // panel's own, so it is lifted back into the tree's coordinates first.
        if let Some(at) = self.win.pointer() {
            self.win.set_cursor(engine.cursor_shape_at(at + origin));
        }
        Ok(())
    }

    fn step(&mut self, engine: &mut Engine) -> Result<SurfaceStep, String> {
        self.intake(engine)?;
        if self.closed {
            return Ok(SurfaceStep::default());
        }
        let frame = engine.frame_number();
        // Nothing new since the last show, nothing painted for it, or no frame window yet.
        if self.presented == Some(frame) || !self.win.can_draw() || !engine.panel_painted(self.node)
        {
            return Ok(SurfaceStep::default());
        }
        self.present(engine, frame)?;
        Ok(SurfaceStep { drew: true })
    }

    fn pointer(&self) -> Option<Point> {
        // The panel's own pointer is window-local: the tree places the panel at `origin`.
        self.win.pointer().map(|local| local + self.origin)
    }

    fn next_deadline(&self, _engine: &Engine) -> Option<std::time::Instant> {
        None
    }

    fn is_closed(&self) -> bool {
        self.closed || self.win.should_close()
    }

    /// Empty: the app is one, and its copy commands are drained by the surface that steps
    /// first — the parent's, which is the one that can serve them this same turn.
    fn take_copies(&mut self, _engine: &mut Engine) -> Vec<ClipboardCopy> {
        Vec::new()
    }

    /// A panel inside this one is owned by this window, so the grab stays with the topmost.
    fn open_popup(
        &mut self,
        node: NodeId,
        spec: PopupSpec,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let presenter = open_panel(
            &self.win,
            &self.spec,
            self.origin,
            node,
            spec,
            self.uncapped,
            &self.paces,
        )?;
        Ok(Box::new(presenter))
    }

    fn reconfigure(&mut self, spec: PopupSpec) -> Result<(), String> {
        if self.window {
            // Where a modeless child stands is the platform's; its size is the content's.
            let size = (
                spec.rect.size.w.round().max(1.0) as u32,
                spec.rect.size.h.round().max(1.0) as u32,
            );
            self.win
                .set_client_size(size)
                .map_err(|error| error.to_string())?;
            self.spec.role = SurfaceRole::Dialog {
                size,
                modal: false,
            };
            return Ok(());
        }
        self.win
            .move_popup(spec)
            .map_err(|error| error.to_string())?;
        self.origin = self.parent_origin + spec.rect.origin;
        Ok(())
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
    pointer: Option<Point>,
) -> Result<SceneFrameOutcome, String> {
    let size = win.client_size();
    let scale = win.scale();
    let mut target = Target {
        win,
        sink: Sink {
            render,
            pace,
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
        self.presenter.window().logical_size()
    }

    fn scale(&self) -> f32 {
        Session::scale(self)
    }

    fn request_close(&mut self) {
        self.presenter.window_mut().request_close();
    }

    fn shell_menu(&self) -> Option<Arc<dyn ui_platform::shell_menu::ShellMenuHost>> {
        Some(Arc::new(crate::context_menu::Win32ShellMenu::for_window(
            self.window().hwnd(),
        )))
    }

    fn write_clipboard(&mut self, content: ui_core::clipboard::ClipboardContent) -> bool {
        self.presenter.window_mut().write_clipboard(content)
    }

    fn request_paste(&mut self) -> bool {
        self.presenter.window_mut().request_paste()
    }

    fn begin_drag(&mut self, drag: ui_platform::view::NativeDrag) -> bool {
        self.presenter.window_mut().begin_drag(drag)
    }

    fn gpu_present(&self) -> bool {
        // Every frame reaches the screen through D3D11; there is no CPU presenter here.
        true
    }

    fn set_supersample(&mut self, factor: u32) {
        self.presenter.set_supersample(factor);
    }

    fn frame_callbacks(&mut self) -> u32 {
        Session::take_frame_callbacks(self)
    }

    fn refresh_hz(&self) -> Option<f32> {
        self.presenter.window().refresh_hz()
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

/// What the render thread is handed: the packet's source and the size it draws at.
struct Sink<'a> {
    render: &'a RenderThread,
    pace: &'a Arc<FramePace>,
    size: (u32, u32),
    scale: f32,
}

impl Sink<'_> {
    fn packet(
        &self,
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
        // What the renderer holds decides what a frame carries. A frame is only a proposal:
        // one the render thread never drew acknowledges nothing, so its page stays owed and
        // the next frame carries it. A page the renderer lacks is a glyph with no ink but
        // its advance.
        let (text_page, color_page) = self
            .render
            .held_atlases()
            .owes(text.version(), text.color_version());
        if text_page {
            let (width, height) = text.dimensions();
            packet.atlas = Some(AtlasUpdate {
                version: text.version(),
                width,
                height,
                data: text.atlas().to_vec(),
            });
        }
        // The colour page is tracked separately: a new text glyph must not re-upload it.
        if color_page {
            let (width, height) = text.color_dimensions();
            packet.color_atlas = Some(AtlasUpdate {
                version: text.color_version(),
                width,
                height,
                data: text.color_atlas().to_vec(),
            });
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
