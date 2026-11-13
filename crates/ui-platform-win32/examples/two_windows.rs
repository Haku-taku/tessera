//! Two engine windows in one process, stepped by `Host` over the Win32 backend.
//!
//! Run: `cargo run -p ui-platform-win32 --example two_windows --release`
//! Knobs: `TWO_WINDOWS_SECONDS=n` ends the run after n seconds (default 5).

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("two_windows: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::time::{Duration, Instant};

    use ui_core::geom::{Color, Corners};
    use ui_core::{Id, NodeKind, SizeClass};
    use ui_platform::host::{Host, OpenOptions};
    use ui_platform::view::App;
    use ui_platform::window::SurfaceSpec;
    use ui_platform::{Container, Element};
    use ui_platform_win32::Win32Backend;

    /// A window's scene: one colour of its own and a bar that never stops moving, so a
    /// surface stepped only on the turn that opened it fails this probe.
    struct Pulse {
        accent: Color,
    }

    impl App for Pulse {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
            // Reading the clock keeps frames due: a build that asked for the time must see
            // the next one, which is what makes both surfaces animate in the same loop.
            let phase = ((cx.now() * 1.2).sin() * 0.5 + 0.5) as f32;
            let (width, height) = (cx.width() as f32, cx.height() as f32);
            Box::new(
                Container::new(Id::str("root"), NodeKind::Column)
                    .size(SizeClass::fixed(width, height))
                    .background(self.accent, Corners::ZERO)
                    .child(Box::new(
                        Container::new(Id::str("bar"), NodeKind::Row)
                            .size(SizeClass::fixed(24.0 + phase * (width - 48.0), 24.0))
                            .background(Color::from_rgba8(20, 22, 28, 255), Corners::all(12.0)),
                    )),
            )
        }
    }

    let seconds = std::env::var("TWO_WINDOWS_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(5.0);

    let mut host = Host::new(Box::new(Win32Backend::new()));
    let first = host.open(
        SurfaceSpec::window(
            "crabsnap-two-windows",
            "crabsnap — first window",
            (640, 400),
        ),
        Pulse {
            accent: Color::from_rgba8(120, 190, 255, 255),
        },
        OpenOptions::default(),
    )?;
    let second = host.open(
        SurfaceSpec::window(
            "crabsnap-two-windows",
            "crabsnap — second window",
            (420, 320),
        ),
        Pulse {
            accent: Color::from_rgba8(255, 140, 120, 255),
        },
        OpenOptions::default(),
    )?;
    println!(
        "opened surfaces {first:?} and {second:?}; {} live",
        host.surface_count()
    );

    let deadline = Instant::now() + Duration::from_secs_f32(seconds);
    let stats = host.run_until(Some(deadline))?;
    println!(
        "run: {} turn(s), {} frame(s) over {seconds} s",
        stats.turns, stats.frames
    );
    for output in host.outputs() {
        println!(
            "output {}: {}x{} at ({}, {}) scale {} refresh {} mHz rotated {}",
            output.name,
            output.width,
            output.height,
            output.x,
            output.y,
            output.scale,
            output.refresh_mhz,
            output.rotated
        );
    }
    // Both windows stay open until the process ends, so both are live at exit.
    println!("{} surface(s) still open at exit", host.surface_count());
    if stats.frames < 2 {
        return Err(format!(
            "only {} frame(s): a surface was never stepped",
            stats.frames
        ));
    }
    Ok(())
}
