//! The bootstrap every showcase app shares: a window from `ui-host`, the fonts the app
//! names, and a frame loop the host paces.
//!
//! Knobs: `DEMO_CPU=1`, `DEMO_SECONDS=n`, `DEMO_CLOSE_AFTER=n`, `DEMO_WALLPAPER=path`,
//! `DEMO_CLICK=x,y;x,y`, `DEMO_DRAG=x1,y1,x2,y2`, `DEMO_TYPE=text`, `DEMO_DUMP=1`,
//! `DEMO_HEARTBEAT=1`, `EXITDBG=1`.

use std::time::Instant;

use ui_core::draw::ImageEntry;
use ui_host::{HostSession, Session, SessionOptions};
use ui_platform::view::{App, AppLoop};
use ui_platform::window::SurfaceSpec;

/// The texture id a root refers to for the wallpaper, so the renderer caches one upload.
pub const WALLPAPER: u64 = 0x7761_6c6c_7061_7065;

/// The faces every showcase app draws with; the family names stay with the crates that
/// own them, so this crate needs neither widget crate to ship the bytes.
pub const SF_SYMBOLS: &[u8] = include_bytes!("../../assets/fonts/SFSymbols.ttf");
pub const INTER: &[u8] = include_bytes!("../../assets/fonts/InterVariable.ttf");
pub const FLUENT_ICONS: &[u8] = include_bytes!("../../assets/fonts/FluentSystemIcons-Regular.ttf");

/// One showcase app: the window it opens, the faces it draws with, and the photo behind it.
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
            ui_host::allow_debugger();
        }
        let mut lp = AppLoop::with_stack(app, self.stack);
        for (family, bytes) in &self.fonts {
            lp.register_font(family, (*bytes).into());
        }
        register_wallpaper(&mut lp, self.wallpaper.as_deref());
        let opts = SessionOptions {
            surface: SurfaceSpec::window(self.app_id, self.title, (self.width, self.height)),
            uncapped: false,
            cpu_presenter: std::env::var_os("DEMO_CPU").is_some(),
        };
        let session = match Session::open(lp, opts) {
            Ok(session) => session,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        };
        if let Some(name) = session.presenter_name() {
            println!("gpu presenter: {name}");
        }
        loop_forever(session);
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

fn loop_forever<A: App, H: HostSession<A>>(mut session: H) -> ! {
    let heartbeat = std::env::var_os("DEMO_HEARTBEAT").is_some();
    let mut dumped = std::env::var_os("DEMO_DUMP").is_none();
    let started = Instant::now();
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
    let mut beat_at = Instant::now();
    // While a script runs, the scripted pointer *is* the pointer the frame sees: injected
    // events and `WidgetState::local` must agree, or a control reading both sees two.
    let mut scripted_pointer: Option<ui_core::geom::Point> = None;
    while !session.finished() && started.elapsed().as_secs_f32() < lifetime() {
        if frames >= 1 {
            match scripted.pop_front() {
                Some(ev) => {
                    if let Some(pos) = ev.pos() {
                        scripted_pointer = Some(pos);
                    }
                    session.lp_mut().dispatch(ev);
                }
                // The script is over; the real pointer takes over again.
                None => scripted_pointer = None,
            }
        }
        if close_after().is_some_and(|s| started.elapsed().as_secs_f32() >= s) {
            break;
        }
        session.set_pointer(scripted_pointer);
        // A turn that does not draw ended in the host's own wait; nothing to count.
        let tick = match session.tick() {
            Ok(tick) => tick,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        };
        if !tick.drew {
            continue;
        }
        if !dumped {
            dumped = true;
            dump(session.lp());
        }
        frames += 1;
        if heartbeat && beat_at.elapsed().as_millis() >= 1000 {
            beat_at = Instant::now();
            eprintln!("[hb] frames={frames}");
        }
    }
    let secs = started.elapsed().as_secs_f32();
    let (waited, waited_n) = session.wait_stats();
    println!(
        "frames {frames} in {secs:.1}s ({:.0} fps), frame wait avg {:.2}ms over {waited_n}",
        frames as f32 / secs.max(1e-3),
        waited.as_secs_f64() * 1000.0 / waited_n.max(1) as f64
    );
    // The session drops its own parts in teardown order; the process ends past them.
    drop(session);
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
