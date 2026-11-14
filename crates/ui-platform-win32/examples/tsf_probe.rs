//! What a focused field hands the input method on Win32: a TSF document that carries the
//! surrounding text around the caret and takes the composition as store edits.
//!
//! Run: `cargo run -p ui-platform-win32 --example tsf_probe`
//!
//! The checks are deterministic: the document's focus, the text TSF reads out of it, and the
//! character path beside it. Which kind of input method a machine ships decides whether a
//! window may use TSF at all — an out-of-process one is driven through IMM32 — so on such a
//! machine the document checks are reported as skipped rather than failed. Whether the
//! service this machine ships composes is likewise reported, not judged: which text service
//! a machine has, and whether that service starts, is not this backend's.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("tsf_probe: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
use windows::core::{Interface, implement};

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use ui_core::Event;
    use ui_core::geom::Rect;
    use ui_platform::present::ImeState;
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::Win32Window;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        ActivateKeyboardLayout, GetKeyboardLayout, LoadKeyboardLayoutW, VK_A, VK_H, VK_I, VK_N,
        VK_O, VK_SPACE,
    };
    use windows::Win32::UI::TextServices::{CLSID_TF_ThreadMgr, ITfThreadMgr};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CHAR, WM_KEYDOWN};

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:58} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;

    let mut window = Win32Window::new(SurfaceSpec::window(
        "crabsnap-win32-tsf",
        "crabsnap — tsf probe",
        (480, 320),
    ))
    .map_err(|error| error.to_string())?;
    let hwnd = window.hwnd();
    let _foreground = take_foreground(hwnd);

    // TSF is one manager per thread; activating it here and again in the window's session
    // yields the same client, which is what an edit session below is requested under.
    let manager: ITfThreadMgr =
        unsafe { CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| format!("CoCreateInstance: {error}"))?;
    let client = unsafe { manager.Activate() }.map_err(|error| format!("Activate: {error}"))?;
    let previous = unsafe { GetKeyboardLayout(0) };

    // The platform decides whether a window may use TSF at all: an out-of-process input
    // method is driven through the window's IMM32 emulation, which a TSF document would
    // switch off. The probe reports which path this machine takes.
    let served = ui_platform_win32::serves_input();
    println!(
        "{:58} {}",
        "the input method can be served through TSF",
        if served {
            "yes"
        } else {
            "no (out of process, so the window keeps IMM32)"
        }
    );

    // A field with a caret five characters into "hello world", as the engine publishes it.
    let field = ImeState {
        enabled: true,
        cursor: Some(Rect::new(20.0, 40.0, 2.0, 18.0)),
        surrounding: Some(("hello world".to_owned(), 5, 5)),
        ..ImeState::default()
    };
    window.set_ime(field.clone());
    if served {
        ok &= store_checks(&manager, client)?;
    } else {
        println!("the document checks are skipped: this input method stays on IMM32");
    }

    // The character path has to keep working: TSF owns the keyboard, not the text.
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CHAR, WPARAM('a' as usize), LPARAM(1));
    }
    let events = drain(&mut window, 300);
    ok &= expect(
        "a typed character still reaches the engine as text",
        events
            .iter()
            .any(|event| matches!(event, Event::Text(text) if text == "a")),
    );

    // No field: the document must stop taking the keyboard, and a key must reach the
    // engine again. While a document is focused an input method may legitimately take the
    // key, so the key check belongs on this side of the detach.
    window.set_ime(ImeState::default());
    println!(
        "{:58} {}",
        "TSF still holds a document after the field",
        if unsafe { manager.GetFocus() }.ok().is_none() {
            "no"
        } else {
            "yes (informational)"
        }
    );
    unsafe {
        let _ = PostMessageW(
            Some(hwnd),
            WM_KEYDOWN,
            WPARAM(VK_SPACE.0 as usize),
            LPARAM(0),
        );
    }
    let events = drain(&mut window, 300);
    ok &= expect(
        "a key reaches the engine once the document is gone",
        events
            .iter()
            .any(|event| matches!(event, Event::KeyPressed { .. })),
    );

    // The input method this machine ships, in its own layout. The keys are real keystrokes
    // rather than posted messages, because a text service decides on the keyboard's state;
    // the probe's window is the foreground window, so they cannot land in another editor.
    window.set_ime(field);
    let chinese = unsafe {
        LoadKeyboardLayoutW(
            &windows::core::HSTRING::from("00000804"),
            Default::default(),
        )
    };
    if let Ok(chinese) = chinese {
        let _ = unsafe { ActivateKeyboardLayout(chinese, Default::default()) };
    }
    let mut events = drain(&mut window, 300);
    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() } == hwnd {
        for key in [VK_N, VK_I, VK_H, VK_A, VK_O, VK_SPACE] {
            send_key(key);
            // An input method answers a key it takes within the millisecond after it.
            events.extend(drain(&mut window, 120));
        }
    } else {
        println!("the probe's window is not the foreground window; keys not sent");
    }
    events.extend(drain(&mut window, 1_500));
    let preedits = events
        .iter()
        .filter(|event| matches!(event, Event::Preedit(Some(_))))
        .count();
    let committed: String = events
        .iter()
        .filter_map(|event| match event {
            Event::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    // An input method in its English mode types the letters through, so a machine where the
    // keys arrive as text is the input method's state, not a fault of either path.
    println!(
        "{:58} preedits {preedits}, committed {committed:?}",
        "what a few typed keys produced"
    );

    window.set_ime(ImeState::default());
    unsafe {
        let _ = ActivateKeyboardLayout(previous, Default::default());
    }

    println!();
    println!("{}", if ok { "PASS" } else { "FAIL" });
    if ok {
        Ok(())
    } else {
        Err("some checks failed".to_owned())
    }
}

/// What an input method reads out of the document a focused field publishes.
#[cfg(windows)]
fn store_checks(
    manager: &windows::Win32::UI::TextServices::ITfThreadMgr,
    client: u32,
) -> Result<bool, String> {
    use std::cell::RefCell;
    use std::rc::Rc;

    use windows::Win32::UI::TextServices::{ITfEditSession, TF_ES_READ, TF_ES_SYNC};

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:58} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;

    let focused = unsafe { manager.GetFocus() }.ok();
    ok &= expect(
        "an enabled field gives TSF a focused document",
        focused.is_some(),
    );
    let context = focused
        .as_ref()
        .and_then(|document| unsafe { document.GetTop() }.ok());
    ok &= expect(
        "the focused document carries the field's context",
        context.is_some(),
    );

    // Reading the context back is what an input method does before it composes: the text
    // comes out of the engine through the store, not out of the probe.
    if let Some(context) = context {
        let seen = Rc::new(RefCell::new(None));
        let session: ITfEditSession = ReadStore {
            context: context.clone(),
            seen: seen.clone(),
        }
        .into();
        let requested =
            unsafe { context.RequestEditSession(client, &session, TF_ES_SYNC | TF_ES_READ) }
                .map_err(|error| format!("RequestEditSession: {error}"))?;
        if !requested.is_ok() {
            println!("the read session did not run synchronously: {requested:?}");
        }
        let seen = seen.borrow().clone();
        ok &= expect(
            "the document shows the field's surrounding text",
            seen.as_ref().is_some_and(|(text, start, end)| {
                text == "hello world" && *start == 5 && *end == 5
            }),
        );
        println!("TSF read: {seen:?}");
    }
    Ok(ok)
}

/// A read of the focused document, run under the store's lock as an input method would.
#[cfg(windows)]
#[implement(windows::Win32::UI::TextServices::ITfEditSession)]
struct ReadStore {
    context: windows::Win32::UI::TextServices::ITfContext,
    seen: std::rc::Rc<std::cell::RefCell<Option<(String, i32, i32)>>>,
}

#[cfg(windows)]
impl windows::Win32::UI::TextServices::ITfEditSession_Impl for ReadStore_Impl {
    fn DoEditSession(&self, cookie: u32) -> windows::core::Result<()> {
        use windows::Win32::UI::TextServices::{ITfRangeACP, TF_SELECTION};
        let mut selection = [TF_SELECTION::default()];
        let mut fetched = 0u32;
        unsafe {
            self.context
                .GetSelection(cookie, 0, &mut selection, &mut fetched)
        }?;
        let (mut start_acp, mut caret) = (0i32, 0i32);
        if let Some(range) = &*selection[0].range {
            let acp: ITfRangeACP = range.cast()?;
            unsafe { acp.GetExtent(&mut start_acp, &mut caret) }?;
        }
        // The document's range starts out empty at offset zero; widening it to the end is what
        // an input method does before it reads the text around the caret.
        let whole = unsafe { self.context.GetStart(cookie) }?;
        let end = unsafe { self.context.GetEnd(cookie) }?.cast::<ITfRangeACP>()?;
        let mut end_acp = 0i32;
        let mut length = 0i32;
        unsafe { end.GetExtent(&mut end_acp, &mut length) }?;
        let range = whole.cast::<ITfRangeACP>()?;
        unsafe { range.SetExtent(0, end_acp) }?;
        let mut buffer = [0u16; 64];
        let mut read = 0u32;
        unsafe { whole.GetText(cookie, 0, &mut buffer, &mut read) }?;
        *self.seen.borrow_mut() = Some((
            String::from_utf16_lossy(&buffer[..read as usize]),
            start_acp,
            start_acp + caret,
        ));
        Ok(())
    }
}

/// Take the foreground for the probe's window, so TSF and synthetic keys see it as focused.
///
/// A process started in the background may not set the foreground directly; attaching to the
/// current foreground thread's input queue is the documented way around that, and the
/// attachment is released again before anything else runs.
#[cfg(windows)]
fn take_foreground(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    if unsafe { GetForegroundWindow() } == hwnd {
        return true;
    }
    let foreground = unsafe { GetForegroundWindow() };
    let other = unsafe { GetWindowThreadProcessId(foreground, None) };
    let ours = unsafe { GetCurrentThreadId() };
    let attached =
        other != 0 && other != ours && unsafe { AttachThreadInput(ours, other, true) }.as_bool();
    let taken = unsafe { SetForegroundWindow(hwnd) }.as_bool();
    if taken {
        let _ = unsafe { SetFocus(Some(hwnd)) };
    }
    if attached {
        let _ = unsafe { AttachThreadInput(ours, other, false) };
    }
    taken
}

/// Send one key as a hardware keystroke, the way an input method expects to see it.
#[cfg(windows)]
fn send_key(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
        MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
    };
    let scan = unsafe { MapVirtualKeyW(key.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    let stroke = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        SendInput(
            &[
                stroke(KEYBD_EVENT_FLAGS::default()),
                stroke(KEYEVENTF_KEYUP),
            ],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Pump the window until `millis` passes, collecting what the engine was told.
#[cfg(windows)]
fn drain(window: &mut ui_platform_win32::Win32Window, millis: u64) -> Vec<ui_core::Event> {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(millis);
    let mut events = Vec::new();
    while std::time::Instant::now() < until {
        window.wait_until(Some(std::time::Duration::from_millis(10)));
        events.extend(window.take_events());
    }
    events
}
