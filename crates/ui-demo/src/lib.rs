//! The bootstrap every showcase app shares: a Wayland window, a presenter, the fonts the app
//! names, and a frame loop the compositor paces.
//!
//! Knobs: `DEMO_CPU=1`, `DEMO_SECONDS=n`, `DEMO_CLOSE_AFTER=n`, `DEMO_WALLPAPER=path`,
//! `DEMO_HEARTBEAT=1`.
//! `DEMO_HEARTBEAT=1`.

use std::time::Instant;

use ui_core::draw::ImageEntry;
use ui_platform::view::{App, AppLoop};
use ui_platform::window::SurfaceSpec;
use ui_platform_wayland::{GpuWindow, GpuWindowConfig, WaylandWindow};

/// The texture id a root refers to for the wallpaper, so the renderer caches one upload.
pub const WALLPAPER: u64 = 0x7761_6c6c_7061_7065;

/// The faces every showcase app draws with; the family names stay with the crates that
/// own them, so this crate needs neither widget crate to ship the bytes.
pub const SF_SYMBOLS: &[u8] = include_bytes!("../../assets/fonts/SFSymbols.ttf");
pub const INTER: &[u8] = include_bytes!("../../assets/fonts/InterVariable.ttf");
pub const FLUENT_ICONS: &[u8] = include_bytes!("../../assets/fonts/FluentSystemIcons-Regular.ttf");

enum Presenter {
    Cpu,
    Gpu(GpuWindow),
}

/// One showcase app: its window, the faces it draws with, and the photo behind it.
pub struct Demo {
    title: &'static str,
    app_id: &'static str,
    width: u32,
    height: u32,
    stack: &'static [&'static str],
    fonts: Vec<(&'static str, &'static [u8])>,
    wallpaper: Option<String>,
}

impl Demo {
    pub fn new(title: &'static str, app_id: &'static str, width: u32, height: u32) -> Self {
        Self {
            title,
            app_id,
            width,
            height,
            stack: &[],
            fonts: Vec::new(),
            wallpaper: None,
        }
    }

    /// The family stack every run resolves through, in fallback order.
    pub fn stack(mut self, families: &'static [&'static str]) -> Self {
        self.stack = families;
        self
    }

    /// A face the app carries itself; the engine ships none.
    pub fn font(mut self, family: &'static str, bytes: &'static [u8]) -> Self {
        self.fonts.push((family, bytes));
        self
    }

    /// The photo behind the shell; `DEMO_WALLPAPER` overrides it.
    pub fn wallpaper(mut self, path: &str) -> Self {
        self.wallpaper = Some(path.to_string());
        self
    }

    /// Open the window and run until it closes. Never returns: the process ends here.
    pub fn run<A: App + 'static>(self, app: A) -> ! {
        if std::env::var_os("EXITDBG").is_some() {
            allow_ptrace();
        }
        let spec = SurfaceSpec::window(self.app_id, self.title, (self.width, self.height));
        let mut win = match WaylandWindow::new(spec) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("cannot open a Wayland window: {e}");
                std::process::exit(1);
            }
        };
        let presenter = if std::env::var_os("DEMO_CPU").is_some() {
            Presenter::Cpu
        } else {
            match GpuWindow::open(&mut win, GpuWindowConfig::default()) {
                Ok(gpu) => {
                    println!("gpu presenter: {} ({})", gpu.renderer_name(), gpu.node());
                    Presenter::Gpu(gpu)
                }
                Err(e) => {
                    eprintln!("GPU presenter unavailable: {e}");
                    eprintln!("set DEMO_CPU=1 to present through the CPU rasterizer");
                    std::process::exit(1);
                }
            }
        };
        let mut lp = AppLoop::with_stack(app, self.stack);
        for (family, bytes) in &self.fonts {
            lp.register_font(family, (*bytes).into());
        }
        register_wallpaper(&mut lp, self.wallpaper.as_deref());
        loop_forever(win, presenter, lp);
    }
}

/// Register the photo, if one is named and readable. A missing file leaves the flat surface,
/// so an app still runs on a machine that does not have this image.
pub fn register_wallpaper<A: App>(lp: &mut AppLoop<A>, configured: Option<&str>) {
    let Some(path) = std::env::var("DEMO_WALLPAPER")
        .ok()
        .or_else(|| configured.map(str::to_string))
    else {
        return;
    };
    match decode(&path) {
        Some(entry) => lp.register_image(entry),
        None => eprintln!("wallpaper: {path} could not be read; the window keeps its flat surface"),
    }
}

/// Straight-alpha RGBA8, which is what `ImageEntry` carries and what the shader samples.
fn decode(path: &str) -> Option<ImageEntry> {
    let image = image::open(path).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(ImageEntry {
        id: WALLPAPER,
        width,
        height,
        source: ui_core::draw::ImageSource::Pixels(std::sync::Arc::new(image.into_raw())),
        colour: ui_core::colour::ContentColour::SDR,
        sampling: ui_core::draw::Sampling::Smooth,
    })
}

fn lifetime() -> f32 {
    std::env::var("DEMO_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600.0)
}

fn close_after() -> Option<f32> {
    std::env::var("DEMO_CLOSE_AFTER")
        .ok()
        .and_then(|s| s.parse().ok())
}

/// `DEMO_CLICK=x,y;x,y` presses and releases at each point, in order, after the first frame.
fn clicks() -> Vec<ui_core::geom::Point> {
    let Ok(raw) = std::env::var("DEMO_CLICK") else {
        return Vec::new();
    };
    raw.split(';')
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some(ui_core::geom::Point::new(
                x.trim().parse().ok()?,
                y.trim().parse().ok()?,
            ))
        })
        .collect()
}

/// `DEMO_DRAG=x1,y1,x2,y2` presses at the first point, travels to the second and releases —
/// the gesture a click cannot express. The release point still needs the real pointer over it.
fn drag() -> Option<(ui_core::geom::Point, ui_core::geom::Point)> {
    let raw = std::env::var("DEMO_DRAG").ok()?;
    let v: Vec<f32> = raw
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    (v.len() == 4).then(|| {
        (
            ui_core::geom::Point::new(v[0], v[1]),
            ui_core::geom::Point::new(v[2], v[3]),
        )
    })
}

/// `DEMO_TYPE=text` types into the input the preceding `DEMO_CLICK` focused.
fn typed() -> Option<String> {
    std::env::var("DEMO_TYPE").ok().filter(|s| !s.is_empty())
}

fn loop_forever(mut win: WaylandWindow, mut presenter: Presenter, mut lp: AppLoop<impl App>) -> ! {
    let heartbeat = std::env::var_os("DEMO_HEARTBEAT").is_some();
    let mut dumped = std::env::var_os("DEMO_DUMP").is_none();
    let started = Instant::now();
    let mut presented: Option<(u32, u32)> = None;
    // One event per frame, so every press lands on a laid-out tree.
    let mut scripted: std::collections::VecDeque<ui_core::Event> = clicks()
        .into_iter()
        .flat_map(|at| {
            let button = ui_core::PointerButton::Primary;
            [
                ui_core::Event::PointerMoved { pos: at },
                ui_core::Event::PointerPressed { pos: at, button },
                ui_core::Event::PointerReleased { pos: at, button },
            ]
        })
        .collect();
    if let Some((from, to)) = drag() {
        let button = ui_core::PointerButton::Primary;
        scripted.push_back(ui_core::Event::PointerMoved { pos: from });
        scripted.push_back(ui_core::Event::PointerPressed { pos: from, button });
        // Several steps rather than one jump: the header only lifts after the threshold,
        // and the strip reflows from the moves it is given.
        const STEPS: usize = 8;
        for k in 1..=STEPS {
            let t = k as f32 / STEPS as f32;
            let pos = ui_core::geom::Point::new(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            );
            scripted.push_back(ui_core::Event::PointerMoved { pos });
        }
        scripted.push_back(ui_core::Event::PointerReleased { pos: to, button });
    }
    if let Some(text) = typed() {
        scripted.push_back(ui_core::Event::Text(text));
    }
    let mut frames = 0u32;
    let mut waited = std::time::Duration::ZERO;
    let mut waited_n = 0u32;
    let mut beat_at = Instant::now();
    // While a script runs, the scripted pointer *is* the pointer the frame sees: injected
    // events and `WidgetState::local` must agree, or a control reading both sees two.
    let mut scripted_pointer: Option<ui_core::geom::Point> = None;
    while !win.should_close() && started.elapsed().as_secs_f32() < lifetime() {
        for ev in win.take_events() {
            lp.dispatch(ev);
        }
        if frames >= 1 {
            match scripted.pop_front() {
                Some(ev) => {
                    if let Some(pos) = ev.pos() {
                        scripted_pointer = Some(pos);
                    }
                    lp.dispatch(ev);
                }
                // The script is over; the real pointer takes over again.
                None => scripted_pointer = None,
            }
        }
        if close_after().is_some_and(|s| started.elapsed().as_secs_f32() >= s) {
            win.request_close();
        }
        let (lw, lh) = win.logical_size();
        let resized = presented != Some((lw, lh));
        lp.set_scale(win.scale());
        let pointer = scripted_pointer.or_else(|| win.pointer());
        match &mut presenter {
            Presenter::Cpu => {
                let outcome = lp.frame(&mut win, pointer);
                if outcome.damage.is_none() && !resized && !outcome.animating {
                    win.wait();
                } else {
                    win.wait_for_frame();
                }
            }
            Presenter::Gpu(gpu) => {
                let outcome = lp.build((lw, lh), pointer);
                win.set_cursor(outcome.cursor);
                win.set_ime(lp.ime_state());
                // An animating frame presents even with no damage: its commit is what paces
                // the loop, else a press would block in `wait()` until unrelated input.
                if !outcome.damage.is_none() || resized || outcome.animating || win.ime_pending() {
                    if let Err(e) = gpu.sync_atlas(&lp.text_pass()) {
                        eprintln!("atlas upload failed: {e}");
                        std::process::exit(1);
                    }
                    if let Err(e) =
                        gpu.present(&mut win, (lw, lh), lp.draw_list(), &outcome.damage, &[])
                    {
                        eprintln!("present failed: {e}");
                        std::process::exit(1);
                    }
                    let t = Instant::now();
                    win.wait_for_frame();
                    waited += t.elapsed();
                    waited_n += 1;
                    if heartbeat && t.elapsed().as_millis() >= 50 {
                        eprintln!("[stall] wait={}ms", t.elapsed().as_millis());
                    }
                } else {
                    win.wait();
                }
            }
        }
        if !dumped {
            dumped = true;
            dump(&lp);
        }
        presented = Some((lw, lh));
        frames += 1;
        if heartbeat && beat_at.elapsed().as_millis() >= 1000 {
            beat_at = Instant::now();
            eprintln!("[hb] frames={frames}");
        }
    }
    let secs = started.elapsed().as_secs_f32();
    println!(
        "frames {frames} in {secs:.1}s ({:.0} fps), frame wait avg {:.2}ms over {waited_n}",
        frames as f32 / secs.max(1e-3),
        waited.as_secs_f64() * 1000.0 / waited_n.max(1) as f64
    );
    // Tear down in order: GPU objects while the surface still exists, then the window.
    drop(presenter);
    drop(lp);
    drop(win);
    std::process::exit(0);
}

/// Print the first frame's primitives: what was drawn, where, and in what ink.
fn dump(lp: &AppLoop<impl App>) {
    let list = lp.draw_list();
    eprintln!(
        "[dump] canvas {:.0}x{:.0} rects {} texts {} glyphs {} images {} clips {}",
        list.canvas.size.w,
        list.canvas.size.h,
        list.prims.rects.len(),
        list.prims.texts.len(),
        list.prims.glyphs.len(),
        list.prims.images.len(),
        list.clips.len()
    );
    for t in list.prims.texts.iter().take(60) {
        let body: String = t
            .block
            .runs
            .first()
            .map(|r| r.text.chars().take(28).collect())
            .unwrap_or_default();
        eprintln!(
            "[dump] text {body:?} at ({:.0},{:.0}) w {:.0} ink {:?} lines {}",
            t.origin.x,
            t.origin.y,
            t.max_width,
            t.ink,
            t.layout.as_ref().map_or(0, |l| l.lines.len())
        );
    }
}

/// Let an outside debugger attach; `yama.ptrace_scope=1` otherwise refuses.
fn allow_ptrace() {
    unsafe extern "C" {
        fn prctl(option: i32, ...) -> i32;
    }
    const PR_SET_PTRACER: i32 = 0x5961_6d61;
    const PR_SET_PTRACER_ANY: usize = usize::MAX;
    unsafe {
        prctl(PR_SET_PTRACER, PR_SET_PTRACER_ANY);
    }
}
