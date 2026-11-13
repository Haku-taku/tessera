//! The process's local channel and global hotkeys, exercised through `Host` on Win32.
//!
//! Run: `cargo run -p ui-platform-win32 --example host_services --release`
//!
//! A client thread connects to the named pipe, sends one message and reads the answer, then
//! posts the `WM_HOTKEY` the system would post for the bound chord; both must reach the
//! handlers the probe registered before the run ends.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("host_services: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use ui_core::event::{Key, Modifiers};
    use ui_core::shortcut::{KeyChord, ShortcutId};
    use ui_platform::host::{Effect, Host};
    use ui_platform_win32::Win32Backend;

    const PIPE: &str = "crabsnap-host-services-probe";
    const HOTKEY: ShortcutId = ShortcutId("probe.hotkey");

    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut host = Host::new(Box::new(Win32Backend::new()));

    let seen = Arc::clone(&events);
    host.listen(PIPE, move |reply, bytes, _context| {
        let message = String::from_utf8_lossy(bytes).into_owned();
        seen.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(format!("message:{message}"));
        let answer = format!("pong:{message}");
        if let Err(error) = reply.send(answer.as_bytes()) {
            seen.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(format!("reply-failed:{error}"));
        }
        Effect::None
    })?;

    let chord = KeyChord::new(
        Key::Char('p'),
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::NONE
        },
    );
    let seen = Arc::clone(&events);
    host.on_hotkey(move |id, _context| {
        seen.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(format!("hotkey:{}", id.0));
        Effect::None
    });
    // Hotkeys are bound through the loop's own door, which the first turn reaches before
    // the client has posted anything. The same turn callback ends the run: this daemon owns
    // no surface, so `run_until` would return before the client has connected.
    let started = Instant::now();
    let mut bound = false;
    let seen = Arc::clone(&events);
    let watched = Arc::clone(&events);
    host.on_turn(move |context, event_loop| {
        if !bound {
            bound = true;
            let result = context.bind_hotkey(chord, HOTKEY);
            seen.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(match result {
                    Ok(()) => String::from("bound"),
                    Err(error) => format!("bind-failed:{error:?}"),
                });
        }
        let events = watched
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let answered = events.iter().any(|event| event == "reply:pong:ping");
        let pressed = events.iter().any(|event| event == "hotkey:probe.hotkey");
        if (answered && pressed) || started.elapsed() > Duration::from_secs(2) {
            event_loop.quit();
        }
    });

    let client_events = Arc::clone(&events);
    let client = std::thread::spawn(move || {
        let record = |text: String| {
            client_events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(text);
        };
        // Give the loop one turn first, so the pipe's first instance is listening.
        std::thread::sleep(Duration::from_millis(200));
        match exchange(PIPE, b"ping") {
            Ok(answer) => record(format!("reply:{answer}")),
            Err(error) => record(format!("client-failed:{error}")),
        }
        // What the system posts for the chord this probe bound: the first chord a fresh
        // registry takes is registration id 1, and the sink window is found by its class.
        match post_hotkey() {
            Ok(()) => {}
            Err(error) => record(format!("post-failed:{error}")),
        }
    });

    let stats = host.run()?;
    client.join().map_err(|_| "the client thread panicked")?;

    let events = events
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    for event in &events {
        println!("{event}");
    }
    println!("run: {} turn(s), {} frame(s)", stats.turns, stats.frames);
    let answered = events.iter().any(|event| event == "reply:pong:ping");
    let pressed = events.iter().any(|event| event == "hotkey:probe.hotkey");
    if !events.iter().any(|event| event == "bound") {
        return Err(String::from("the chord was never bound"));
    }
    if !answered {
        return Err(String::from("the channel never answered the client"));
    }
    if !pressed {
        return Err(String::from("the posted hotkey never reached the handler"));
    }
    println!("PASS: the channel answered and the hotkey reached its handler");
    Ok(())
}

/// Connect to `name`, send one message and read one answer.
#[cfg(windows)]
fn exchange(name: &str, message: &[u8]) -> Result<String, String> {
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE, OPEN_EXISTING, ReadFile, WriteFile,
    };
    use windows::Win32::System::Pipes::{PIPE_READMODE_MESSAGE, SetNamedPipeHandleState};
    use windows::core::PCWSTR;

    let path = format!(r"\\.\pipe\{name}");
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a terminated path that outlives the call.
    let handle = unsafe {
        CreateFileW(
            PCWSTR::from_raw(wide.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|error| format!("connect: {error}"))?;
    let mode = PIPE_READMODE_MESSAGE;
    // SAFETY: the handle is open and `mode` outlives the call.
    if unsafe { SetNamedPipeHandleState(handle, Some(&mode), None, None) }.is_err() {
        let _ = unsafe { CloseHandle(handle) };
        return Err(String::from("the pipe refused message mode"));
    }
    let mut written = 0u32;
    // SAFETY: the handle is open and the slices outlive each call.
    unsafe { WriteFile(handle, Some(message), Some(&mut written), None) }
        .map_err(|error| format!("write: {error}"))?;
    let mut buffer = [0u8; 256];
    let mut read = 0u32;
    let result = unsafe { ReadFile(handle, Some(&mut buffer), Some(&mut read), None) }
        .map_err(|error| format!("read: {error}"));
    let _ = unsafe { CloseHandle(handle) };
    result?;
    let length = (read as usize).min(buffer.len());
    buffer
        .get(..length)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .ok_or_else(|| String::from("read: short buffer"))
}

/// Post the message the system posts for a registered chord.
#[cfg(windows)]
fn post_hotkey() -> Result<(), String> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, HWND_MESSAGE, PostMessageW, WM_HOTKEY,
    };
    use windows::core::PCWSTR;

    let class: Vec<u16> = ui_platform_win32::HOTKEY_WINDOW_CLASS
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: the class name outlives the call; message-only windows are only found under
    // `HWND_MESSAGE`.
    let window: HWND = unsafe {
        FindWindowExW(
            Some(HWND_MESSAGE),
            None,
            PCWSTR::from_raw(class.as_ptr()),
            PCWSTR::null(),
        )
    }
    .map_err(|error| format!("find the hotkey window: {error}"))?;
    unsafe { PostMessageW(Some(window), WM_HOTKEY, WPARAM(1), LPARAM(0)) }
        .map_err(|error| format!("post WM_HOTKEY: {error}"))
}
