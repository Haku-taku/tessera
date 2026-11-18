//! Acceptance gallery: one window, a nav tree on the left, three styles per section.

use ui_core::timed;
use ui_platform::host::{FontFace, Host, OpenOptions};
use ui_platform::window::SurfaceSpec;

/// Heap traffic per frame: `GALLERY_ALLOC=1` for the `[hb]` counters, `GALLERY_ALLOC_SIZES=1`
/// for the size histogram, `GALLERY_ALLOC_SITES=1` for the top caller chains (1 in 64 sampled).
struct Counting;

static ALLOC_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SIZES_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ALLOCS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SIZES: [std::sync::atomic::AtomicU64; 15] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 15];

fn size_class(n: usize) -> usize {
    match n {
        0..=8 => 0,
        9..=16 => 1,
        17..=24 => 2,
        25..=32 => 3,
        33..=48 => 4,
        49..=64 => 5,
        65..=96 => 6,
        97..=128 => 7,
        129..=256 => 8,
        257..=512 => 9,
        513..=1024 => 10,
        1025..=2048 => 11,
        2049..=4096 => 12,
        4097..=16384 => 13,
        _ => 14,
    }
}

/// Sampling switch and per-site tally: the caller chain of one allocation in 64.
static SITES_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SITES: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> =
    std::sync::Mutex::new(None);
thread_local! {
    static IN_PROBE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The three innermost frames that are not the allocator plumbing itself.
#[inline(never)]
fn note_site() {
    let bt = std::backtrace::Backtrace::force_capture();
    let text = bt.to_string();
    let mut frames = Vec::with_capacity(3);
    for line in text.lines().skip(1) {
        let name = line.trim();
        if name.is_empty() || name.contains("backtrace") || name.contains("Counting") {
            continue;
        }
        frames.push(name.to_string());
        if frames.len() == 3 {
            break;
        }
    }
    let key = frames.join(" <- ");
    // Never block here: a lock held by the reporting path would wait on this very allocation.
    if let Ok(mut slot) = SITES.try_lock()
        && let Some(map) = slot.as_mut()
    {
        *map.entry(key).or_insert(0) += 1;
    }
}

/// Count one allocation; sampling a caller chain every 64th when the site probe is on.
unsafe fn count(size: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    if !ALLOC_ON.load(Relaxed) {
        return;
    }
    let n = ALLOCS.fetch_add(1, Relaxed);
    BYTES.fetch_add(size as u64, Relaxed);
    if SIZES_ON.load(Relaxed) {
        SIZES[size_class(size)].fetch_add(1, Relaxed);
    }
    if SITES_ON.load(Relaxed) && !IN_PROBE.with(|p| p.get()) && n % 64 == 0 {
        IN_PROBE.with(|p| p.set(true));
        note_site();
        IN_PROBE.with(|p| p.set(false));
    }
}

unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        unsafe { count(l.size()) };
        unsafe { std::alloc::System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, n: usize) -> *mut u8 {
        unsafe { count(n) };
        unsafe { std::alloc::System.realloc(p, l, n) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

mod app;
mod pages;
mod selftest;
mod shell;
mod wallpaper;

use crate::app::Gallery;
use crate::selftest::{dump_frame, frame, route_clipboard, selftest, step, want_dump};
use crate::shell::*;

/// Hand the loop the app's words for the engine's built-in text rows. Neither the locale
/// nor the bindings change during a run, so once is enough.
fn sync_text_menu_labels(lp: &mut AppLoop<Gallery>) {
    let env = base_env();
    let Some(catalog) = env
        .find::<std::sync::Arc<ui_i18n::Language>>()
        .map(|language| language.catalog())
    else {
        return;
    };
    let row = |msg: Msg, id: ui_core::ShortcutId| ui_widgets::MenuRowText {
        label: catalog.text(msg).to_string(),
        shortcut: lp
            .shortcut(id)
            .map(|chord| ui_widgets::shortcut_label_in(&catalog, &chord)),
    };
    lp.set_text_menu_labels(Some(ui_widgets::TextMenuLabels {
        select_all: row(
            Msg::WIDGETS_MENU_SELECT_ALL,
            ui_core::ShortcutId::SELECT_ALL,
        ),
        copy: row(Msg::WIDGETS_MENU_COPY, ui_core::ShortcutId::COPY),
        cut: row(Msg::WIDGETS_MENU_CUT, ui_core::ShortcutId::CUT),
        paste: row(Msg::WIDGETS_MENU_PASTE, ui_core::ShortcutId::PASTE),
    }));
}

fn lifetime() -> f32 {
    std::env::var("GALLERY_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300.0)
}

/// `GALLERY_CAPTURE_SCALE=v` pins every glass capture's density: 1.0 shows the backdrop
/// without downsampling, where the frost normally pays for its blur with a coarse capture.
fn capture_scale() -> Option<f32> {
    std::env::var("GALLERY_CAPTURE_SCALE")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|v: &f32| v.is_finite() && *v > 0.0)
}

/// `GALLERY_SS=n` renders the scene at n× and box-averages down on present.
fn supersample() -> u32 {
    std::env::var("GALLERY_SS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .clamp(1, 4)
}

/// Milliseconds since the epoch, for correlating two processes' logs.
fn epoch_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// `GALLERY_CLOSE_AFTER=n` closes the window itself after n seconds.
fn close_after() -> Option<f32> {
    std::env::var("GALLERY_CLOSE_AFTER")
        .ok()
        .and_then(|s| s.parse().ok())
}

/// `GALLERY_PRESS=x,y` holds one synthetic press there, so a screenshot can catch press-only
/// paint (ink, state layers) that a pointer sitting still never produces.
fn held_press() -> Option<Point> {
    let raw = std::env::var("GALLERY_PRESS").ok()?;
    let (x, y) = raw.split_once(',')?;
    Some(Point::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `GALLERY_CLICK=x,y` presses there, drags the pointer 300px away and releases.
fn clicked_once() -> Option<Point> {
    let raw = std::env::var("GALLERY_CLICK").ok()?;
    let (x, y) = raw.split_once(',')?;
    Some(Point::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `GALLERY_HOLD=x,y,dx,dy` presses at `x,y`, moves the pointer by `dx,dy` and never
/// releases: the held-ink-after-leave shape a screenshot can read.
fn held_and_left() -> Option<(Point, Point)> {
    let raw = std::env::var("GALLERY_HOLD").ok()?;
    let v: Vec<f32> = raw
        .split(',')
        .map(|s| s.trim().parse().ok())
        .collect::<Option<_>>()?;
    (v.len() == 4).then(|| (Point::new(v[0], v[1]), Point::new(v[0] + v[2], v[1] + v[3])))
}

/// `GALLERY_SWAY=x,y,dx,ms` presses there and sweeps to `x + dx` every `ms`, never releasing:
/// a wobble is a motion, so only a repeating sweep can be caught.
fn sway_once() -> Option<(Point, Point, u64)> {
    let raw = std::env::var("GALLERY_SWAY").ok()?;
    let mut parts = raw.split(',');
    let x: f32 = parts.next()?.trim().parse().ok()?;
    let y: f32 = parts.next()?.trim().parse().ok()?;
    let dx: f32 = parts.next()?.trim().parse().ok()?;
    let ms: u64 = parts.next()?.trim().parse().ok()?;
    Some((Point::new(x, y), Point::new(x + dx, y), ms.max(16)))
}

/// `GALLERY_FOCUS=x,y` clicks there without dragging: the plain-click path, for focus.
fn focus_once() -> Option<Point> {
    let raw = std::env::var("GALLERY_FOCUS").ok()?;
    let (x, y) = raw.split_once(',')?;
    Some(Point::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `GALLERY_RIGHT=x,y` opens the context menu a right click would.
fn right_once() -> Option<Point> {
    let raw = std::env::var("GALLERY_RIGHT").ok()?;
    let (x, y) = raw.split_once(',')?;
    Some(Point::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// `GALLERY_CLIPBOARD=copy:<text>` puts text on the clipboard; `paste` asks for it.
fn clipboard_once() -> Option<String> {
    std::env::var("GALLERY_CLIPBOARD").ok()
}

/// `GALLERY_KEY=ctrl+a,ctrl+c` presses chords in order; the last part of each names the key.
fn key_once() -> Option<Vec<(ui_core::Modifiers, ui_core::Key)>> {
    let raw = std::env::var("GALLERY_KEY").ok()?;
    let mut out = Vec::new();
    for chord in raw.split(',') {
        let mut mods = ui_core::Modifiers::default();
        let mut key = None;
        for part in chord.split('+').map(str::trim) {
            key = Some(match part.to_ascii_lowercase().as_str() {
                "ctrl" => {
                    mods.ctrl = true;
                    continue;
                }
                "alt" => {
                    mods.alt = true;
                    continue;
                }
                "shift" => {
                    mods.shift = true;
                    continue;
                }
                "super" | "logo" => {
                    mods.logo = true;
                    continue;
                }
                "left" => ui_core::Key::Left,
                "right" => ui_core::Key::Right,
                "up" => ui_core::Key::Up,
                "down" => ui_core::Key::Down,
                "home" => ui_core::Key::Home,
                "end" => ui_core::Key::End,
                "enter" => ui_core::Key::Enter,
                "tab" => ui_core::Key::Tab,
                "space" => ui_core::Key::Space,
                "esc" => ui_core::Key::Escape,
                "backspace" => ui_core::Key::Backspace,
                "delete" => ui_core::Key::Delete,
                other => ui_core::Key::Char(other.chars().next()?),
            });
        }
        out.push((mods, key?));
    }
    Some(out)
}

/// `GALLERY_WHEEL=x,y,delta` sends one wheel notch at that point.
fn wheel_once() -> Option<(Point, f32)> {
    let raw = std::env::var("GALLERY_WHEEL").ok()?;
    let v: Vec<f32> = raw
        .split(',')
        .map(|s| s.trim().parse().ok())
        .collect::<Option<_>>()?;
    (v.len() == 3).then(|| (Point::new(v[0], v[1]), v[2]))
}

/// `GALLERY_DRAG=x1,y1,x2,y2` presses at the first point, drags to the second, releases.
fn drag_once() -> Option<(Point, Point)> {
    let raw = std::env::var("GALLERY_DRAG").ok()?;
    let v: Vec<f32> = raw
        .split(',')
        .map(|s| s.trim().parse().ok())
        .collect::<Option<_>>()?;
    (v.len() == 4).then(|| (Point::new(v[0], v[1]), Point::new(v[2], v[3])))
}

/// `GALLERY_DRAGS=x1,y1,x2,y2;...` performs several drags in a row, a few frames apart.
///
/// One drag per process cannot reach a state that only a second gesture on a still-moving
/// control creates; that is where a stuck gesture shows itself.
fn drags() -> Vec<(Point, Point)> {
    let Ok(raw) = std::env::var("GALLERY_DRAGS") else {
        return Vec::new();
    };
    raw.split(';')
        .filter_map(|one| {
            let v: Vec<f32> = one
                .split(',')
                .map(|s| s.trim().parse().ok())
                .collect::<Option<_>>()?;
            (v.len() == 4).then(|| (Point::new(v[0], v[1]), Point::new(v[2], v[3])))
        })
        .collect()
}

/// The part of an animation report that must change while something moves.
fn anim_key(report: &str) -> String {
    report
        .split(';')
        .map(|part| match part.find(" idle=") {
            Some(at) => part[..at].to_string(),
            None => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// One tick per loop turn, so a watchdog can tell a hung process from a quiet one.
static BEAT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Set by `GALLERY_PERF` once its reports are printed: the loop then leaves through its own
/// teardown instead of `exit`, which would kill the render threads mid-call.
static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `GALLERY_STALL=secs`: if the loop stops turning for that long, capture stacks and exit.
///
/// A freeze is reported by whoever saw it, in the act: `perf` samples the hung threads while
/// they are still hung, so the report names the call that never returns.
fn watch_for_stalls() {
    let Ok(raw) = std::env::var("GALLERY_STALL") else {
        return;
    };
    let limit = raw.parse::<u64>().unwrap_or(5).max(2);
    std::thread::spawn(move || {
        let mut last = BEAT.load(std::sync::atomic::Ordering::Relaxed);
        let mut still = 0u64;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let now = BEAT.load(std::sync::atomic::Ordering::Relaxed);
            still = if now == last { still + 1 } else { 0 };
            last = now;
            if still >= limit {
                break;
            }
        }
        let pid = std::process::id();
        eprintln!("[stall] no loop turn for {limit}s in pid {pid}");
        for entry in std::fs::read_dir("/proc/self/task")
            .into_iter()
            .flatten()
            .flatten()
        {
            let comm = std::fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
            let stat = std::fs::read_to_string(entry.path().join("stat")).unwrap_or_default();
            let fields: Vec<&str> = stat.split_whitespace().collect();
            eprintln!(
                "[stall]   tid={} comm={} cpu={}",
                entry.file_name().to_string_lossy(),
                comm.trim(),
                fields
                    .get(13)
                    .zip(fields.get(14))
                    .map(|(a, b)| format!("{a}+{b}"))
                    .unwrap_or_default()
            );
        }
        let sample = std::process::Command::new("perf")
            .args([
                "record",
                "-g",
                "-o",
                "/tmp/stall.perf",
                "-p",
                &pid.to_string(),
                "--",
                "sleep",
                "2",
            ])
            .status();
        eprintln!("[stall] perf record: {sample:?}");
        if let Ok(out) = std::process::Command::new("perf")
            .args([
                "report",
                "--stdio",
                "--no-children",
                "-g",
                "none",
                "-i",
                "/tmp/stall.perf",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines().take(60) {
                eprintln!("[stall] {line}");
            }
        }
        std::process::exit(3);
    });
}

/// `GALLERY_PERF=<wait>:<sample>`: sample this process with `perf` after `wait` seconds.
///
/// Sampling runs while the app is live, so the profile covers what the user is doing then:
/// the flat list names the hot symbols, the caller tree names who called them.
fn watch_for_perf() {
    let Ok(raw) = std::env::var("GALLERY_PERF") else {
        return;
    };
    let mut parts = raw.split(':').map(|s| s.trim().parse::<u64>().unwrap_or(0));
    let wait = parts.next().unwrap_or(0).max(1);
    let sample = parts.next().unwrap_or(5).max(1);
    std::thread::spawn(move || {
        for left in (1..=wait).rev() {
            eprintln!("[perf] sampling in {left}s — interact with the window now");
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        let pid = std::process::id().to_string();
        eprintln!("[perf] sampling {sample}s of pid {pid}");
        let started = std::process::Command::new("perf")
            .args([
                "record",
                "-g",
                "-F",
                "999",
                "-o",
                "/tmp/hot.perf",
                "-p",
                &pid,
                "--",
                "sleep",
                &sample.to_string(),
            ])
            .status();
        eprintln!("[perf] record: {started:?}");
        for graph in ["none", "caller"] {
            eprintln!("[perf] ---- -g {graph} ----");
            let out = std::process::Command::new("perf")
                .args([
                    "report",
                    "--stdio",
                    "--no-children",
                    "-g",
                    graph,
                    "-i",
                    "/tmp/hot.perf",
                    "--percent-limit",
                    "0.3",
                ])
                .output();
            if let Ok(out) = out {
                for line in String::from_utf8_lossy(&out.stdout).lines().take(90) {
                    eprintln!("[perf] {line}");
                }
            }
        }
        eprintln!("[perf] done — closing the window");
        STOP.store(true, std::sync::atomic::Ordering::Relaxed);
    });
}

// Run: `WAYLAND_DISPLAY=wayland-2 cargo run -q -p ui-gallery --example gallery`
#[cfg_attr(feature = "profile", hotpath::main)]
fn main() {
    if std::env::var_os("EXITDBG").is_some() {
        ui_host::allow_debugger();
    }
    let want_sizes = std::env::var_os("GALLERY_ALLOC_SIZES").is_some();
    let want_allocs = want_sizes
        || std::env::var_os("GALLERY_ALLOC").is_some()
        || std::env::var_os("GALLERY_ALLOC_SITES").is_some();
    ALLOC_ON.store(want_allocs, std::sync::atomic::Ordering::Relaxed);
    SIZES_ON.store(want_sizes, std::sync::atomic::Ordering::Relaxed);
    let uncapped = std::env::var_os("GALLERY_UNCAPPED").is_some();
    if uncapped {
        eprintln!("[uncapped] frame pacing disabled: fps is throughput, not refresh");
    }
    // `GALLERY_HOST=1` runs the app on the process loop instead of a one-window session.
    if std::env::var_os("GALLERY_HOST").is_some() {
        run_host(uncapped);
        return;
    }
    // Layout must measure with the same faces the painter rasterises: the monospace
    // estimate calls an icon 1 em wide at 0.6 em and its box no longer holds the glyph.
    // A bare session has no backend to ask, so the environment is the only answer here; on a
    // POSIX session it is the whole answer.
    let mut lp = AppLoop::with_stack(
        Gallery::new(ui_platform::posix_system_languages()),
        TEXT_STACK,
    );
    // The engine ships no fonts: the host supplies the SF Symbols file the iOS controls use.
    lp.register_font(
        ui_widgets::SF_SYMBOLS_FAMILY,
        include_bytes!("../../../assets/fonts/SFSymbols.ttf")
            .as_slice()
            .into(),
    );
    // Latin text: Inter is the closest open face to SF Pro, so the stack does not fall to
    // whichever sans the host happens to default to.
    lp.register_font(
        TEXT_FAMILY,
        include_bytes!("../../../assets/fonts/InterVariable.ttf")
            .as_slice()
            .into(),
    );
    // WinUI draws its control glyphs from an icon face of its own; the Fluent controls
    // ask for it by the family they registered here.
    lp.register_font(
        ui_widgets_fluent::glyphs::ICON_FAMILY,
        include_bytes!("../../../assets/fonts/FluentSystemIcons-Regular.ttf")
            .as_slice()
            .into(),
    );
    let opts = SessionOptions {
        surface: SurfaceSpec::window(
            "dev.crabsnap.ui.gallery",
            "crabsnap-ui   gallery",
            (WIN_W, WIN_H),
        ),
        uncapped,
        cpu_presenter: std::env::var("GALLERY_CPU").is_ok(),
    };
    let session = match Session::open(lp, opts) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    if let Some(name) = session.presenter_name() {
        println!("gpu presenter: {name}");
    }
    run(session);
}

/// Run the gallery the way an application runs: the process loop, which is the only path that
/// reconciles platform panels, so a menu that asks for its own surface gets one.
fn run_host(uncapped: bool) {
    let backend = match ui_host::connect() {
        Ok(backend) => backend,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let mut host = Host::new(Box::new(backend));
    let face = |name: &str, data: &[u8]| FontFace {
        name: name.to_string(),
        data: data.into(),
    };
    let options = OpenOptions {
        uncapped,
        images: wallpaper::entry().into_iter().collect(),
        fonts: vec![
            face(
                ui_widgets::SF_SYMBOLS_FAMILY,
                include_bytes!("../../../assets/fonts/SFSymbols.ttf"),
            ),
            face(
                TEXT_FAMILY,
                include_bytes!("../../../assets/fonts/InterVariable.ttf"),
            ),
            face(
                ui_widgets_fluent::glyphs::ICON_FAMILY,
                include_bytes!("../../../assets/fonts/FluentSystemIcons-Regular.ttf"),
            ),
        ],
        stack: TEXT_STACK.iter().map(|family| (*family).to_string()).collect(),
        ..OpenOptions::default()
    };
    let spec = SurfaceSpec::window(
        "dev.crabsnap.ui.gallery",
        "crabsnap-ui   gallery",
        (WIN_W, WIN_H),
    );
    let system = host.languages();
    if let Err(e) = host.open(spec, Gallery::new(system), options) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    let until = Instant::now() + std::time::Duration::from_secs_f32(lifetime());
    if let Err(e) = host.run_until(Some(until)) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// Write the last presented frame into the temp directory; the path is the one printed.
fn dump_frame_now<H: HostSession<Gallery>>(session: &mut H) -> Option<std::path::PathBuf> {
    let path = std::env::temp_dir().join("gallery_frame.ppm");
    session.dump_frame(&path).ok()?;
    Some(path)
}

fn run<H: HostSession<Gallery>>(mut session: H) {
    watch_for_stalls();
    watch_for_perf();
    // `GALLERY_PRESS` taps twice a second for the whole run, so it is read before setup.
    let tap = held_press();
    // The photo the whole shell sits on; the root names it, so it must be in before frame one.
    wallpaper::install(session.lp_mut());
    // The text rows' words and chords are fixed for the run, so they go in once.
    sync_text_menu_labels(session.lp_mut());
    if std::env::var("GALLERY_SELFTEST").is_ok() {
        let (lw, lh) = session.logical_size();
        selftest(session.lp_mut(), (lw.max(WIN_W), lh.max(WIN_H)));
    }
    // One laid-out frame first: the splat starts from hit testing, and before layout a
    // press hits nothing.
    if let Some(at) = tap {
        let (lw, lh) = session.logical_size();
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: at });
        frame(session.lp_mut(), (lw.max(WIN_W), lh.max(WIN_H)), Some(at));
    }
    // Press, drag out, release: the control keeps pointer focus with the pointer elsewhere.
    // A screenshot after it settles shows whether any state layer outlived the press.
    if let Some(at) = clicked_once() {
        let (lw, lh) = session.logical_size();
        let size = (lw.max(WIN_W), lh.max(WIN_H));
        let button = ui_core::PointerButton::Primary;
        let out = Point::new(at.x + 300.0, at.y);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: at });
        step(session.lp_mut(), size, at, 1);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerPressed { pos: at, button });
        step(session.lp_mut(), size, at, 8);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: out });
        step(session.lp_mut(), size, out, 1);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerReleased { pos: out, button });
        step(session.lp_mut(), size, out, 40);
    }
    // Press, leave, keep holding: the ink is still alive while the state layer has gone,
    // which is when a clip that followed the painted box used to lose its curve.
    if let Some((at, out)) = held_and_left() {
        let (lw, lh) = session.logical_size();
        let size = (lw.max(WIN_W), lh.max(WIN_H));
        let button = ui_core::PointerButton::Primary;
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: at });
        step(session.lp_mut(), size, at, 1);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerPressed { pos: at, button });
        step(session.lp_mut(), size, at, 2);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: out });
        // Past the hover reaction, inside the ripple's own life: the pill must still be round.
        step(session.lp_mut(), size, out, 12);
    }
    // A plain click: focus and caret placement without the drag `GALLERY_CLICK` performs.
    if let Some(at) = focus_once() {
        let (lw, lh) = session.logical_size();
        let size = (lw.max(WIN_W), lh.max(WIN_H));
        let button = ui_core::PointerButton::Primary;
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: at });
        step(session.lp_mut(), size, at, 1);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerPressed { pos: at, button });
        step(session.lp_mut(), size, at, 1);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerReleased { pos: at, button });
        step(session.lp_mut(), size, at, 4);
        // `GALLERY_TYPE=text` then types it into whatever holds the caret.
        if let Ok(text) = std::env::var("GALLERY_TYPE") {
            for c in text.chars() {
                session
                    .lp_mut()
                    .dispatch(ui_core::Event::Text(c.to_string()));
            }
            step(session.lp_mut(), size, at, 4);
        }
    }
    // A scripted wheel notch: the one gesture a screenshot cannot produce by hand. It runs
    // before the drag so a drag can target content the wheel scrolled into view.
    if let Some((at, delta)) = wheel_once() {
        let (lw, lh) = session.logical_size();
        let size = (lw.max(WIN_W), lh.max(WIN_H));
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: at });
        step(session.lp_mut(), size, at, 1);
        session.lp_mut().dispatch(ui_core::Event::wheel(at, delta));
        step(session.lp_mut(), size, at, 20);
    }
    // Scripted drags: selection across lines, and many pointer gestures in a row.
    let mut sequence: Vec<(Point, Point)> = drag_once().into_iter().collect();
    sequence.extend(drags());
    for (from, to) in sequence {
        let (lw, lh) = session.logical_size();
        let size = (lw.max(WIN_W), lh.max(WIN_H));
        let button = ui_core::PointerButton::Primary;
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: from });
        step(session.lp_mut(), size, from, 1);
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerPressed { pos: from, button });
        step(session.lp_mut(), size, from, 1);
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            let p = Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
            session
                .lp_mut()
                .dispatch(ui_core::Event::PointerMoved { pos: p });
            step(session.lp_mut(), size, p, 1);
        }
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerReleased { pos: to, button });
        step(session.lp_mut(), size, to, 4);
    }
    // A scripted right click: the engine's own menu is drawn from its own state.
    if let Some(at) = right_once() {
        let (lw, lh) = session.logical_size();
        let size = (lw.max(WIN_W), lh.max(WIN_H));
        session
            .lp_mut()
            .dispatch(ui_core::Event::PointerMoved { pos: at });
        step(session.lp_mut(), size, at, 1);
        session.lp_mut().dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Secondary,
        });
        step(session.lp_mut(), size, at, 4);
    }
    // `GALLERY_ANIM=1` reports, once a second, what is still moving and where it lives.
    let watch_anim = std::env::var_os("GALLERY_ANIM").is_some();
    let mut anim_at = Instant::now();
    let mut anim_frames = 0u32;
    let mut last_anim = String::new();
    let mut anim_stuck = 0u32;
    let mut holding = false;
    let sway = sway_once();
    let mut sway_pressed = false;
    // The held press must win over the seat pointer every frame, or a drag reads the real
    // cursor and the control ends up rubber-banded away from rest.
    let hold = held_and_left().map(|(at, _)| at);
    // Injected input is this frame's platform pointer, or `build` would snap back to the seat.
    let mut synthetic: Option<Point> = None;
    // One setting with two halves: the renderer draws on the finer grid and the text pass
    // rasterizes on it, or glyphs would only be magnified from a 1× atlas.
    let ss = supersample();
    session.lp_mut().set_supersample(ss);
    session.set_supersample(ss);
    // A pinned capture density reaches every glass material through the tuning table.
    if let Some(v) = capture_scale() {
        ui_widgets_glass::tuning::set(ui_widgets_glass::tuning::Knob::CaptureScale, Some(v));
    }
    let started = Instant::now();
    let mut dumped = false;
    let mut frames = 0u32;
    // Intake, demand and pacing live in the driver; the loop only draws when it says so.
    // `GALLERY_HEARTBEAT=1` stamps the loop once a second: a stalled output stops the stamps,
    // which is how a frozen screen becomes measurable without a camera.
    let heartbeat = std::env::var_os("GALLERY_HEARTBEAT").is_some();
    // `GALLERY_FRAMELOG=1` prints each frame's CPU time and page; a switch's first frame stands out.
    let framelog = std::env::var_os("GALLERY_FRAMELOG").is_some();
    // `GALLERY_SWITCH=<frame>:<tag>[,<frame>:<tag>]` jumps pages mid-run, so one run can
    // time a cold switch and a warm one to the same page.
    let switch_at: Vec<(u32, Page)> = std::env::var("GALLERY_SWITCH")
        .map(|s| {
            s.split(',')
                .filter_map(|one| {
                    let (n, tag) = one.split_once(':')?;
                    Some((
                        n.parse::<u32>().ok()?,
                        PAGES.into_iter().find(|p| p.tag() == tag)?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut beat_at = Instant::now();
    let mut beat_cpu = session.cpu_seconds();
    // Kept for the exit line: frame rate follows the compositor, so CPU per frame is the
    // only number two builds can be compared on.
    let cpu_start = beat_cpu;
    let mut beat_frames = 0u32;
    let mut beat_turns = 0u32;
    let mut turns = 0u32;
    // Why each frame commits in the last second: the loop is only as cheap as its gate.
    let mut beat_commit = 0u32;
    let mut beat_anim = 0u32;
    let mut beat_damage = 0u32;
    let mut beat_idle = 0u32;
    let mut beat_draws = 0u32;
    // Redrawn area as a share of the surface, averaged over the second: a band that is
    // always the whole window means damage tracking is not paying for itself.
    let mut beat_band = 0f64;
    let mut beat_band_n = 0u32;
    let mut beat_allocs = ALLOCS.load(std::sync::atomic::Ordering::Relaxed);
    let mut beat_bytes = BYTES.load(std::sync::atomic::Ordering::Relaxed);
    let clipboard = clipboard_once();
    let mut clipboard_done = false;
    let key = key_once();
    let mut key_done = false;
    // The presenter never changes mid-run; the report below reads it once per turn.
    let gpu_present = session.gpu_present();

    while !session.finished()
        && !STOP.load(std::sync::atomic::Ordering::Relaxed)
        && started.elapsed().as_secs_f32() < lifetime()
    {
        let frame_at = Instant::now();
        turns += 1;
        BEAT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Reports come first: a loop turn that skips the build leaves through `continue`, and
        // a report at the end would then vanish exactly when the loop is doing the least.
        if watch_anim && anim_at.elapsed().as_millis() >= 1000 {
            anim_at = Instant::now();
            // A second of the same answer means nothing moved: that is the defect, caught live.
            // Compare the motion itself: the diagnostics beside it change every second even
            // when nothing moves, and would hide exactly the freeze they exist to expose.
            let report = session.lp().anim_report();
            let key = anim_key(&report);
            if key == last_anim && !key.starts_with("still") {
                anim_stuck += 1;
            } else {
                anim_stuck = 0;
            }
            last_anim = key;
            eprintln!(
                "[anim] builds={} {} {report}",
                frames - anim_frames,
                if anim_stuck >= 2 { "STUCK" } else { "ok" }
            );
            anim_frames = frames;
        }
        if heartbeat && beat_at.elapsed().as_millis() >= 1000 {
            // CPU share of one core over the last second: idle and busy runs differ here.
            let wall = beat_at.elapsed().as_secs_f64();
            let cpu = session.cpu_seconds();
            // A host that cannot report CPU time leaves the field out rather than printing 0.
            let share = cpu.zip(beat_cpu).map_or_else(
                || String::from("n/a"),
                |(cpu, previous)| format!("{:.0}%", (cpu - previous) / wall.max(1e-3) * 100.0),
            );
            beat_at = Instant::now();
            beat_cpu = cpu;
            // `cbs` is what the compositor offered: builds below it means the loop dropped
            // frames, builds equal to it means the compositor itself paced us there.
            let allocs = ALLOCS.load(std::sync::atomic::Ordering::Relaxed);
            let bytes = BYTES.load(std::sync::atomic::Ordering::Relaxed);
            let built = (frames - beat_frames).max(1) as u64;
            let heap = if ALLOC_ON.load(std::sync::atomic::Ordering::Relaxed) {
                format!(
                    " alloc={} {}kB",
                    (allocs - beat_allocs) / built,
                    (bytes - beat_bytes) / built / 1024
                )
            } else {
                String::new()
            };
            let cbs = session.frame_callbacks();
            let hz = session.refresh_hz();
            eprintln!(
                "[hb] t={} turns={} builds={} cbs={} cpu={share} commit={beat_commit} (dirty={beat_damage} anim={beat_anim} draws={beat_draws} band={band:.1}%{heap}) idle={beat_idle} refresh={:?}",
                epoch_ms(),
                turns - beat_turns,
                frames - beat_frames,
                cbs,
                hz,
                band = if beat_band_n == 0 {
                    0.0
                } else {
                    beat_band / beat_band_n as f64 * 100.0
                }
            );
            if std::env::var_os("GALLERY_ALLOC_SITES").is_some() {
                // Collect under the lock, print after releasing it: `eprintln` allocates,
                // and an allocation inside the lock would sample itself into a deadlock.
                let mut top: Vec<(u64, String)> = Vec::new();
                SITES_ON.store(false, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut slot) = SITES.lock() {
                    match slot.as_mut() {
                        None => {
                            *slot = Some(std::collections::HashMap::new());
                        }
                        Some(map) => {
                            top = map.iter().map(|(k, v)| (*v, k.clone())).collect();
                        }
                    }
                }
                SITES_ON.store(true, std::sync::atomic::Ordering::Relaxed);
                top.sort_unstable_by(|a, b| b.0.cmp(&a.0));
                for (n, k) in top.iter().take(18) {
                    eprintln!("[site] {n}: {k}");
                }
            }
            if std::env::var_os("GALLERY_ALLOC_SIZES").is_some() {
                let mut line = String::from("[alloc]");
                for (i, n) in SIZES.iter().enumerate() {
                    let v = n.load(std::sync::atomic::Ordering::Relaxed);
                    if v > 0 {
                        line.push_str(&format!(" b{i}={v}"));
                    }
                }
                eprintln!("{line}");
            }
            beat_allocs = allocs;
            beat_bytes = bytes;
            beat_turns = turns;
            beat_frames = frames;
            beat_commit = 0;
            beat_anim = 0;
            beat_damage = 0;
            beat_idle = 0;
            beat_draws = 0;
            beat_band = 0.0;
            beat_band_n = 0;
        }
        // Jump pages at their frames, to time a switch inside a running app.
        for (n, page) in &switch_at {
            if frames == *n {
                session.lp_mut().app_mut().page = *page;
            }
        }
        // A pressed finger sweeping back and forth: the one shape in which the thumb's
        // wobble exists from frame to frame.
        if let Some((from, to, ms)) = &sway {
            let button = ui_core::PointerButton::Primary;
            if !sway_pressed {
                // Wait for the first layout: a press before the hit index exists lands nowhere.
                if started.elapsed().as_secs_f32() > 0.3 {
                    sway_pressed = true;
                    let pos = *from;
                    synthetic = Some(pos);
                    session
                        .lp_mut()
                        .dispatch(ui_core::Event::PointerMoved { pos });
                    session
                        .lp_mut()
                        .dispatch(ui_core::Event::PointerPressed { pos, button });
                }
            } else {
                let cycles = started.elapsed().as_secs_f32() / (*ms as f32 / 1000.0);
                let s = (cycles * std::f32::consts::TAU).sin();
                let pos = Point::new(from.x + (to.x - from.x) * s, from.y);
                synthetic = Some(pos);
                session
                    .lp_mut()
                    .dispatch(ui_core::Event::PointerMoved { pos });
            }
        }
        // Tap twice a second at `GALLERY_PRESS`, so a screenshot catches the ink in flight;
        // a held press settles into a flat tint within 225ms and proves nothing.
        if let Some(at) = tap {
            let down = started.elapsed().as_millis() % 500 < 250;
            if down != holding {
                holding = down;
                let button = ui_core::PointerButton::Primary;
                let pos = at;
                synthetic = Some(pos);
                // The move lands the pointer before the press, as the sway script does; an
                // event built inside the `if` below would be dropped instead of delivered.
                if down {
                    session
                        .lp_mut()
                        .dispatch(ui_core::Event::PointerMoved { pos });
                }
                session.lp_mut().dispatch(if down {
                    ui_core::Event::PointerPressed { pos, button }
                } else {
                    ui_core::Event::PointerReleased { pos, button }
                });
            }
        }
        // A held press has to be re-asserted before the frame is judged, or it becomes the
        // one input that cannot ask for a frame.
        if let Some(at) = hold {
            synthetic = Some(at);
            session
                .lp_mut()
                .dispatch(ui_core::Event::PointerMoved { pos: at });
        }
        // `GALLERY_CLOSE_AFTER=n` closes the window itself after n seconds, so an exit test
        // needs no hand on the mouse; compare it against a real close.
        if close_after().is_some_and(|secs| started.elapsed().as_secs_f32() >= secs) {
            session.request_close();
        }
        route_clipboard(&mut session);
        if let Some(chords) = &key {
            // After the focus knobs have run, and once: a held modifier set is ambient.
            if !key_done && started.elapsed().as_secs_f32() > 0.3 {
                key_done = true;
                for (mods, k) in chords {
                    session.lp_mut().dispatch(ui_core::Event::Modifiers(*mods));
                    session
                        .lp_mut()
                        .dispatch(ui_core::Event::KeyPressed { key: *k });
                }
                session
                    .lp_mut()
                    .dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
            }
        }
        if let Some(spec) = &clipboard {
            // Retried: the seat announces its offer a few frames after the window maps.
            if !clipboard_done && started.elapsed().as_secs_f32() > 0.3 {
                let ok = if let Some(text) = spec.strip_prefix("copy:") {
                    session.write_clipboard(ui_core::clipboard::ClipboardContent::Text(
                        text.to_string(),
                    ))
                } else if spec == "paste" {
                    session.request_paste()
                } else {
                    true
                };
                if ok {
                    println!(
                        "clipboard {}: true",
                        if spec == "paste" { "paste" } else { "copy" }
                    );
                    clipboard_done = true;
                } else if started.elapsed().as_secs_f32() > 3.0 {
                    println!("clipboard {}: false", spec);
                    clipboard_done = true;
                }
            }
        }
        // Scripted input is this frame's platform pointer, or the frame snaps back to the seat.
        session.set_pointer(synthetic);
        // Intake, demand, pacing and the commit decision are the engine's; a turn either
        // draws or ends here.
        let tick = match timed!("turn", session.tick()) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };
        // A frame wait this long means no callback arrived: the output is stalled.
        if heartbeat && session.last_wait().as_millis() >= 50 {
            eprintln!(
                "[stall] t={} wait={}ms",
                epoch_ms(),
                session.last_wait().as_millis()
            );
        }
        for ev in session.events() {
            // The one event a screenshot cannot show: what the peer actually handed us.
            if let ui_core::Event::Paste(content) = ev {
                if let Some(text) = content.text() {
                    println!("clipboard pasted: {text:?}");
                }
            }
        }
        if !tick.drew {
            continue;
        }
        let Some(outcome) = tick.outcome else {
            continue;
        };
        // `GALLERY_BATCHES=1` prints one frame's batches: draw calls are priced per
        // call, so a page that needs 100 of them is paying for its paint order.
        let batches = frames == 3 && std::env::var_os("GALLERY_BATCHES").is_some();
        if batches {
            let all = session.lp().draw_list().plan_batches();
            let mut kinds = [0u32; ui_core::draw::PrimKind::COUNT];
            let mut prims = [0u32; ui_core::draw::PrimKind::COUNT];
            for b in &all {
                kinds[b.kind as usize] += 1;
                prims[b.kind as usize] += b.range.end - b.range.start;
            }
            eprintln!(
                "[batches] total={} rect={}/{} glyph={}/{} shadow={}/{} sector={}/{} image={}/{}",
                all.len(),
                kinds[0],
                prims[0],
                kinds[2],
                prims[2],
                kinds[4],
                prims[4],
                kinds[5],
                prims[5],
                kinds[3],
                prims[3],
            );
        }
        if framelog {
            let page = session.lp_mut().app_mut().page;
            let misses = session.lp().text_pass().measure_misses();
            eprintln!(
                "[frame] {frames} {:.1}ms page={page:?} measure_misses={misses}",
                frame_at.elapsed().as_secs_f64() * 1000.0,
            );
        }
        // The engine already decided whether this frame was worth a commit; this only
        // reports it. Without a GPU presenter no frame reaches a target that can dump.
        if gpu_present {
            if let Some(stats) = tick.presented {
                beat_commit += 1;
                beat_anim += outcome.animating as u32;
                beat_damage += !outcome.damage.is_none() as u32;
                if framelog {
                    // On a host that runs at refresh rate, why each frame commits is the
                    // one fact a screenshot cannot show.
                    eprintln!(
                        "[present] damage={} animating={}",
                        !outcome.damage.is_none(),
                        outcome.animating
                    );
                }
                beat_draws += stats.draw_calls;
                let (lw, lh) = session.logical_size();
                let s = session.scale().max(f32::EPSILON);
                let full = (lw as f32 * s) * (lh as f32 * s);
                let band = stats.band.map_or(full, |b| (b[2] * b[3]) as f32);
                beat_band += (band / full.max(1.0)) as f64;
                beat_band_n += 1;
                if batches {
                    eprintln!("[band] {:?} surface={}x{}", stats.band, lw, lh);
                }
                if want_dump()
                    && !dumped
                    && frames >= dump_frame()
                    && let Some(path) = dump_frame_now(&mut session)
                {
                    dumped = true;
                    println!("dumped {}", path.display());
                }
            } else {
                beat_idle += 1;
            }
        } else if want_dump() && !dumped {
            eprintln!("gallery: GALLERY_DUMP needs the GPU presenter");
            dumped = true;
        }
        frames += 1;
    }
    if framelog {
        let (partial, full) = partial_frames();
        eprintln!("redraw: partial={partial} full={full}");
        // Entries held and lookups (hits, misses): reuse means a still frame rebuilt nothing.
        let (kept, hits, misses) = session.lp().decl_cache_stats();
        eprintln!("decl cache: {kept} entries, {hits} hits, {misses} misses");
    }
    let secs = started.elapsed().as_secs_f32();
    let (waited, waited_n) = session.wait_stats();
    println!(
        "frames {frames} in {secs:.1}s ({:.0} fps), frame wait avg {:.2}ms over {waited_n}",
        frames as f32 / secs.max(1e-3),
        waited.as_secs_f64() * 1000.0 / waited_n.max(1) as f64
    );
    let (partial, full) = partial_frames();
    // Frame rate follows the compositor, so CPU per frame is the only number two builds can
    // be compared on; a host that cannot report it prints the draw counts alone.
    match (cpu_start, session.cpu_seconds()) {
        (Some(start), Some(end)) => {
            let cpu = end - start;
            println!(
                "cpu {cpu:.3}s over {frames} frames = {:.0}µs/frame (partial {partial}, full {full})",
                cpu * 1e6 / frames.max(1) as f64
            );
        }
        _ => println!("redraw: partial={partial} full={full}"),
    }
    // In-flight dma-bufs at exit: a growing count means the compositor is not releasing them.
    if let Some((inflight, released)) = session.dmabuf_stats() {
        println!("dmabuf: {inflight} inflight, {released} released");
    }
    // Teardown order is the session's: GPU objects while the surface still exists, then the
    // loop, then the window. Timed because a slow exit is a user-visible bug.
    let t0 = Instant::now();
    ui_platform::exit_probe::stage("loop-end");
    // `EXIT_FINISH=1` drains the GPU before teardown; how long that takes is the backlog
    // the compositor would otherwise inherit.
    if std::env::var_os("EXIT_FINISH").is_some() {
        ui_platform::exit_probe::stage("gpu: finish");
        session.finish();
        ui_platform::exit_probe::stage("gpu: finished");
    }
    drop(session);
    let t1 = Instant::now();
    ui_platform::exit_probe::stage("session-dropped");
    if t1.duration_since(t0).as_millis() > 50 || ui_platform::exit_probe::armed() {
        eprintln!("teardown: session {}ms", t1.duration_since(t0).as_millis());
    }
    // Everything is gone; anything slow after this is libc's atexit, not ours.
    ui_platform::exit_probe::stage("main-return");
}
