//! One window and one loop, whichever platform provides them.
//!
//! A backend's `Session` owns the window, the presenter and the driver; this is the part an
//! app sees, so app code is written once and runs on either host. What one host cannot
//! answer is an `Option`, never a missing method: a Win32 window has no dma-buf, and says so
//! rather than failing to compile.

use std::path::Path;
use std::time::Duration;

use ui_core::Event;
use ui_core::clipboard::ClipboardContent;
use ui_core::geom::Point;

use crate::present::PresentStats;
use crate::view::{App, AppLoop, FrameOutcome};
use crate::window::SurfaceSpec;

/// What a host asks for when it hands the engine a loop.
pub struct SessionOptions {
    /// The window to open.
    pub surface: SurfaceSpec,
    /// Draw on every turn instead of one frame per composition callback (a stress knob).
    pub uncapped: bool,
    /// Present through the CPU rasterizer even when a GPU is available.
    pub cpu_presenter: bool,
}

/// One turn's result: whether it drew, and what the target did.
#[derive(Default)]
pub struct Tick {
    pub drew: bool,
    pub outcome: Option<FrameOutcome>,
    pub presented: Option<PresentStats>,
    /// The pointer the frame was built with, for the host's own diagnostics.
    pub pointer: Option<Point>,
}

/// The session an app is given, whichever backend runs it.
pub trait HostSession<A: App>: Sized {
    /// Open the window and its presenter; a refusal is a message the app can print.
    fn open(lp: AppLoop<A>, options: SessionOptions) -> Result<Self, String>;

    /// Intake, decide, draw and pace — everything a frame needs, waiting included.
    fn tick(&mut self) -> Result<Tick, String>;

    /// Whether the window asked to close; the loop is what ends.
    fn finished(&self) -> bool;

    fn lp(&self) -> &AppLoop<A>;

    fn lp_mut(&mut self) -> &mut AppLoop<A>;

    /// The pointer to build the next frame with, overriding the window's own.
    ///
    /// Scripted input needs it, or the frame snaps back to wherever the cursor is.
    fn set_pointer(&mut self, pointer: Option<Point>);

    /// Events the last turn took from the window and dispatched to the app.
    fn events(&self) -> &[Event];

    /// The presenter in use, for the app's own report.
    fn presenter_name(&self) -> Option<String>;

    /// How long the last turn waited for the compositor.
    fn last_wait(&self) -> Duration;

    /// Frame waits so far and their total, for the exit line.
    fn wait_stats(&self) -> (Duration, u32);

    /// CPU time this process has used over its life, where the host reports a process clock.
    fn cpu_seconds(&self) -> Option<f64>;

    /// The client area in logical units, which is what layout and input use.
    fn logical_size(&self) -> (u32, u32);

    /// Physical pixels per logical unit.
    fn scale(&self) -> f32;

    fn request_close(&mut self);

    /// Put text on the clipboard; false when the host cannot carry this content.
    fn write_clipboard(&mut self, content: ClipboardContent) -> bool;

    /// Start an operating-system drag carrying `data`, so another window can take it.
    ///
    /// The engine has already told the source its drag began. The host reports the outcome
    /// as [`Event::Dnd(DndEvent::Finished)`](ui_core::DndEvent::Finished); false means the
    /// host could not start one, and the engine cancels the gesture.
    fn begin_drag(&mut self, data: ui_core::DragData) -> bool;

    /// Read the clipboard as a paste event; false when it holds no text.
    fn request_paste(&mut self) -> bool;

    /// Whether frames reach the screen through the GPU, rather than a CPU presenter.
    fn gpu_present(&self) -> bool;

    /// Render at `factor` device pixels per logical pixel, where a GPU presenter exists.
    fn set_supersample(&mut self, factor: u32);

    /// Frames the compositor took since the last call.
    fn frame_callbacks(&mut self) -> u32;

    /// The output's refresh rate, where the host can report one.
    fn refresh_hz(&self) -> Option<f32>;

    /// dma-bufs in flight and released: a Wayland answer, and `None` where there are none.
    fn dmabuf_stats(&self) -> Option<(usize, u32)>;

    /// Write the last presented frame as a PPM.
    fn dump_frame(&mut self, path: &Path) -> Result<(), String>;

    /// Drain the GPU; how long this takes is the backlog teardown would otherwise inherit.
    fn finish(&mut self);
}
