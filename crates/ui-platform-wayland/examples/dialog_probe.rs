//! A window and a dialog it owns, stepped by `Host` over the Wayland backend.
//!
//! The probe prints what the connection advertises and what each surface did. What only the
//! screen can show: the dialog sits above its parent, centred, and clicking the parent leaves
//! its press counter alone while the dialog is up.
//!
//! Run in the nested session: `cargo run --release -p ui-platform-wayland --example dialog_probe`
//! Knobs: `DIALOG_PROBE_SECONDS=n` (default 4), `DIALOG_PROBE_MODAL=0` for a modeless dialog,
//! `DIALOG_PROBE_CLOSE_PARENT=n` to close the parent after n seconds — the dialog must go too.

#[cfg(not(unix))]
fn main() {
    eprintln!("the Wayland backend is Unix only");
}

#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use ui_core::geom::{Color, Corners};
#[cfg(unix)]
use ui_core::{Id, NodeKind, SizeClass};
#[cfg(unix)]
use ui_platform::host::{Host, OpenOptions};
#[cfg(unix)]
use ui_platform::view::App;
#[cfg(unix)]
use ui_platform::window::SurfaceSpec;
#[cfg(unix)]
use ui_platform::{Container, Element};
#[cfg(unix)]
use ui_platform_wayland::WaylandBackend;

/// The bar moves on the wall clock, so a surface the loop stopped stepping shows up as a
/// frozen bar instead of a frame count that is only printed at exit.
#[cfg(unix)]
struct Probe {
    accent: Color,
    /// Both surfaces log the same event; the name says which one received it.
    name: &'static str,
    pressed: u32,
    /// Wall-clock time at which this surface closes itself, for the lifecycle check.
    close_at: Option<f64>,
}

#[cfg(unix)]
impl App for Probe {
    fn changed(&mut self, _on: Id, change: ui_core::Change) -> bool {
        if matches!(change, ui_core::Change::Activated) {
            self.pressed += 1;
            println!("{}: press {}", self.name, self.pressed);
            return true;
        }
        false
    }

    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        if let Some(at) = self.close_at
            && cx.now() >= at
        {
            println!("{}: closing", self.name);
            cx.close_window();
        }
        let phase = ((cx.now() * 0.8).sin() * 0.5 + 0.5) as f32;
        let (width, height) = (cx.width() as f32, cx.height() as f32);
        let travel = (width - 48.0).max(1.0);
        Box::new(
            Container::new(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(width, height))
                .sense(ui_core::Sense::CLICK)
                .background(self.accent, Corners::ZERO)
                .child(Box::new(
                    Container::new(Id::str("bar"), NodeKind::Row)
                        .size(SizeClass::fixed(24.0 + phase * travel, 24.0))
                        .background(Color::from_rgba8(20, 22, 28, 255), Corners::all(12.0)),
                )),
        )
    }
}

#[cfg(unix)]
fn main() {
    if let Err(error) = run() {
        eprintln!("dialog_probe: {error}");
        std::process::exit(1);
    }
}

#[cfg(unix)]
fn run() -> Result<(), String> {
    let seconds = std::env::var("DIALOG_PROBE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(4.0);
    let modal = std::env::var("DIALOG_PROBE_MODAL").as_deref() != Ok("0");
    let close_parent_at = std::env::var("DIALOG_PROBE_CLOSE_PARENT")
        .ok()
        .and_then(|value| value.parse::<f64>().ok());

    // `xdg_wm_dialog_v1` is the modal hint only; `set_parent` is what every compositor has,
    // so the probe reports the hint and works either way.
    match dialog_protocol() {
        Ok(true) => println!("xdg_wm_dialog_v1: advertised"),
        Ok(false) => println!("xdg_wm_dialog_v1: absent (parenting and host filtering only)"),
        Err(error) => println!("xdg_wm_dialog_v1: unknown ({error})"),
    }

    let backend = WaylandBackend::connect().map_err(|error| error.to_string())?;
    let mut host = Host::new(Box::new(backend));
    let parent = host.open(
        SurfaceSpec::window(
            "dev.crabsnap.ui.dialog-probe",
            "dialog probe   parent",
            (640, 400),
        ),
        Probe {
            accent: Color::from_rgba8(120, 190, 255, 255),
            name: "parent",
            pressed: 0,
            close_at: close_parent_at,
        },
        OpenOptions::default(),
    )?;
    // Opened after the parent is mapped: `set_parent` needs a toplevel the compositor knows.
    let dialog = host.open_dialog(
        parent,
        SurfaceSpec::dialog(
            "dev.crabsnap.ui.dialog-probe",
            "dialog probe   dialog",
            (420, 260),
        )
        .with_modal(modal),
        Probe {
            accent: Color::from_rgba8(255, 140, 120, 255),
            name: "dialog",
            pressed: 0,
            close_at: None,
        },
        OpenOptions::default(),
    )?;
    println!(
        "opened {parent:?} and {} dialog {dialog:?}; {} live",
        if modal { "modal" } else { "modeless" },
        host.surface_count()
    );

    let stats = host.run_until(Some(Instant::now() + Duration::from_secs_f32(seconds)))?;
    println!(
        "run: {} turn(s), {} frame(s) over {seconds} s; {} of 2 surface(s) open at exit",
        stats.turns,
        stats.frames,
        host.surface_count()
    );
    if stats.frames < 2 {
        return Err(format!(
            "only {} frame(s): a surface was never stepped",
            stats.frames
        ));
    }
    Ok(())
}

/// Whether the compositor advertises the modal-hint protocol.
///
/// A second connection, read and dropped before the backend's: the backend binds its own.
#[cfg(unix)]
fn dialog_protocol() -> Result<bool, String> {
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::wl_registry;
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

    struct RegistryOnly;

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for RegistryOnly {
        fn event(
            _state: &mut Self,
            _proxy: &wl_registry::WlRegistry,
            _event: <wl_registry::WlRegistry as Proxy>::Event,
            _data: &GlobalListContents,
            _conn: &Connection,
            _queue: &QueueHandle<Self>,
        ) {
        }
    }

    let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
    let (globals, _queue) =
        registry_queue_init::<RegistryOnly>(&connection).map_err(|error| error.to_string())?;
    Ok(globals
        .contents()
        .with_list(|list| list.iter().any(|global| global.interface == "xdg_wm_dialog_v1")))
}
