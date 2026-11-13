//! Exit-stage tracing: names the teardown step the process is in, and what every thread waits on.
//!
//! Armed by `EXITDBG=1`; every stage prints its elapsed time, and a watchdog repeats the
//! current stage plus each thread's kernel wait channel once a second while it lasts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The stage the process is in; a plain string so callers name their own steps.
static STAGE: Mutex<&'static str> = Mutex::new("idle");
static START: OnceLock<Instant> = OnceLock::new();
static ARMED: AtomicBool = AtomicBool::new(false);

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("EXITDBG").is_some())
}

/// Enter `name`, starting the clock and the watchdog on the first call.
pub fn stage(name: &'static str) {
    if !enabled() {
        return;
    }
    *STAGE.lock().unwrap_or_else(|e| e.into_inner()) = name;
    let start = START.get_or_init(Instant::now);
    eprintln!("[exit] t={} +{:>6.1}ms  {name}", epoch_ms(), millis(start));
    if !ARMED.swap(true, Ordering::SeqCst) {
        let _ = std::thread::Builder::new()
            .name("exit-probe".into())
            .spawn(watchdog);
    }
}

/// Whether tracing is on, for callers that build expensive detail only when it is.
pub fn armed() -> bool {
    enabled()
}

fn millis(start: &Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Wall clock in epoch milliseconds, so a line can be lined up with another log's stamps.
fn epoch_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

fn watchdog() {
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let name = *STAGE.lock().unwrap_or_else(|e| e.into_inner());
        let elapsed = START.get().map_or(0.0, millis);
        eprintln!(
            "[exit] t={} +{elapsed:>6.1}ms  still in {name};{}",
            epoch_ms(),
            thread_snapshot()
        );
    }
}

/// One `tid:comm/wchan` per thread; `wchan` is the kernel function it sleeps in.
fn thread_snapshot() -> String {
    let Ok(dir) = std::fs::read_dir("/proc/self/task") else {
        return String::new();
    };
    let mut out = String::new();
    for entry in dir.flatten() {
        let tid = entry.file_name().to_string_lossy().into_owned();
        let comm = std::fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
        let wchan = std::fs::read_to_string(entry.path().join("wchan")).unwrap_or_default();
        out.push_str(&format!(" {tid}:{}/{}", comm.trim(), wchan.trim()));
    }
    out
}
