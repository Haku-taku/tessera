//! Global hotkeys over the XDG desktop portal's `GlobalShortcuts`.
//!
//! Transport only; the `KeyChord` <-> `preferred_trigger` encoding lives in [`codec`].

mod codec;

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_platform::hotkeys::{GlobalHotkeyError, GlobalHotkeys};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub use codec::{chord_to_trigger, trigger_to_chord};

/// Bus name owning every portal object.
const DESKTOP_DEST: &str = "org.freedesktop.portal.Desktop";
/// Object path of the portal's desktop root.
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
/// Interface for global shortcut binding.
const GS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
/// Interface for a portal request's completion.
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
/// Interface for a portal session, used to close it.
const SESSION_IFACE: &str = "org.freedesktop.portal.Session";
/// Portal version that first defines the methods we use.
const MIN_VERSION: u32 = 1;

/// The shared mailbox between the D-Bus reader thread and the frame's `poll`.
#[derive(Default)]
struct Shared {
    /// Bound shortcut id strings mapped back to the ids the app registered with.
    lookup: HashMap<String, ShortcutId>,
    /// Session paths we own; an `Activated` naming another is not ours.
    sessions: Vec<String>,
    /// Activations queued by the reader thread, drained by `poll`.
    activated: Vec<ShortcutId>,
}

/// Global hotkeys backed by `org.freedesktop.portal.GlobalShortcuts`.
///
/// A background thread owns the D-Bus `Activated` signal stream, matching the polling thread in
/// `theme.rs`; `poll` reads what that thread queued and never blocks.
pub struct PortalHotkeys {
    /// Blocking D-Bus connection, cloned into the reader thread.
    conn: zbus::blocking::Connection,
    /// Mailbox shared with the reader thread; see [`Shared`].
    shared: Arc<Mutex<Shared>>,
    /// Counter making each request's handle path unique.
    token: AtomicU32,
}

impl PortalHotkeys {
    /// Read the portal version and start the `Activated` reader thread; sessions come later,
    /// one per bind. `Unsupported` if the portal is absent or too old, `Failed` on D-Bus error.
    pub fn new() -> Result<Self, GlobalHotkeyError> {
        let conn = zbus::blocking::Connection::session().map_err(bus_error)?;
        Self::open(conn, None)
    }

    /// The same portal path, with a counter bumped whenever an activation is queued.
    ///
    /// The desktop's signal arrives on this reader thread, not on the compositor connection,
    /// so the loop has nothing to wake it for a hotkey except this descriptor.
    pub fn with_wake(wake: Arc<OwnedFd>) -> Result<Self, GlobalHotkeyError> {
        let conn = zbus::blocking::Connection::session().map_err(bus_error)?;
        Self::open(conn, Some(wake))
    }

    /// Open against an explicit bus address, for a non-default bus or a headless test of the
    /// no-portal path; an unreachable address yields `Failed` or `Unsupported` without blocking.
    pub fn with_address(address: &str) -> Result<Self, GlobalHotkeyError> {
        let conn = zbus::blocking::connection::Builder::address(address)
            .map_err(bus_error)?
            .build()
            .map_err(bus_error)?;
        Self::open(conn, None)
    }

    fn open(
        conn: zbus::blocking::Connection,
        wake: Option<Arc<OwnedFd>>,
    ) -> Result<Self, GlobalHotkeyError> {
        let version: u32 = portal_proxy(&conn)?
            .get_property("version")
            .map_err(bus_error)?;
        if version < MIN_VERSION {
            return Err(GlobalHotkeyError::Unsupported);
        }
        let shared = Arc::new(Mutex::new(Shared::default()));
        spawn_reader(&conn, Arc::clone(&shared), wake);
        Ok(Self {
            conn,
            shared,
            token: AtomicU32::new(0),
        })
    }

    /// A path-safe token unique to this process, appended to the request handle path.
    ///
    /// The portal turns the token into an object-path element, so it must match
    /// `[A-Za-z0-9_]+`: a hyphen makes the portal answer "Invalid object path" and no
    /// session is created at all.
    fn next_token(&self) -> String {
        let n = self.token.fetch_add(1, Ordering::Relaxed);
        format!("ui_{n}")
    }

    /// Close every session we opened, releasing the bindings they hold. Best-effort: an error is
    /// swallowed because there is nothing the caller can still do at teardown.
    pub fn close(&self) {
        let sessions = self
            .shared
            .lock()
            .map(|g| g.sessions.clone())
            .unwrap_or_default();
        for session in sessions {
            if let Ok(proxy) = zbus::blocking::Proxy::new(
                &self.conn,
                DESKTOP_DEST,
                session.as_str(),
                SESSION_IFACE,
            ) {
                let _ = proxy.call_noreply("Close", &());
            }
        }
    }
}

impl Drop for PortalHotkeys {
    fn drop(&mut self) {
        // Closing the session is the only way to give a binding back: the portal has no
        // unbind, and a binding left behind outlives this process.
        self.close();
    }
}

impl GlobalHotkeys for PortalHotkeys {
    /// Bind `chord` to `id`. The portal shows its confirmation dialog and this call waits on it:
    /// a dismissed dialog is `Refused`; anything the bus rejects is `Failed`/`Unsupported`.
    ///
    /// `BindShortcuts` is accepted once per session, so each call opens its own.
    fn register(&mut self, chord: KeyChord, id: ShortcutId) -> Result<(), GlobalHotkeyError> {
        let trigger = chord_to_trigger(chord)
            .ok_or_else(|| GlobalHotkeyError::Failed(format!("no portal name for {id:?}")))?;
        // Register the lookup before binding so an `Activated` racing the reply is not lost.
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .lookup
            .insert(id.0.to_string(), id);

        let session = match self.spawn_session() {
            Ok(session) => session,
            Err(e) => {
                self.forget(id);
                return Err(e);
            }
        };
        let token = self.next_token();
        match bind_shortcuts(&self.conn, &session, &token, id, &trigger) {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.forget(id);
                Err(GlobalHotkeyError::Refused)
            }
            Err(e) => {
                self.forget(id);
                Err(e)
            }
        }
    }

    /// Drop the local id: the portal offers no unbind, so this only stops `poll` from reporting
    /// it; the real key stays grabbed until the session closes or the app exits.
    fn unregister(&mut self, id: ShortcutId) {
        self.forget(id);
    }

    /// Drain activations the reader thread queued since the last call; never blocks.
    fn poll(&mut self) -> Vec<ShortcutId> {
        let mut guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut guard.activated)
    }
}

impl PortalHotkeys {
    /// Open a fresh session for one bind and record it, so both `Activated` filtering and
    /// [`Self::close`] know about it.
    fn spawn_session(&self) -> Result<OwnedObjectPath, GlobalHotkeyError> {
        // Each portal request gets its own token: the token names the request's object path,
        // so reusing one would put two requests at the same path.
        let token = self.next_token();
        let session = create_session(&self.conn, &token)?;
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .push(session.as_str().to_string());
        Ok(session)
    }

    /// Forget a bound id from the shared lookup, used by register's error paths and unregister.
    fn forget(&mut self, id: ShortcutId) {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .lookup
            .remove(id.0);
    }
}

/// A proxy on the desktop root for the `GlobalShortcuts` interface.
fn portal_proxy(
    conn: &zbus::blocking::Connection,
) -> Result<zbus::blocking::Proxy<'static>, GlobalHotkeyError> {
    zbus::blocking::Proxy::new(conn, DESKTOP_DEST, DESKTOP_PATH, GS_IFACE).map_err(bus_error)
}

/// `CreateSession` and follow its `Response` to the returned `session_handle`. `token` must
/// match the one used to build the request path.
fn create_session(
    conn: &zbus::blocking::Connection,
    token: &str,
) -> Result<OwnedObjectPath, GlobalHotkeyError> {
    let mut options: HashMap<String, OwnedValue> = HashMap::new();
    options.insert("handle_token".into(), owned_str(token)?);
    options.insert("session_handle_token".into(), owned_str(token)?);

    // Subscribe to this request's response before the call, so a fast reply is never missed.
    let request_proxy = request_proxy(conn, request_path(conn, token))?;
    let mut response = response_stream(&request_proxy)?;
    portal_proxy(conn)?
        .call_noreply("CreateSession", &options)
        .map_err(bus_error)?;
    let reply = read_response(&mut response)?;
    if reply.code != 0 {
        return Err(GlobalHotkeyError::Failed(format!(
            "CreateSession code {}",
            reply.code
        )));
    }
    // `session_handle` is typed `o` in the XML but backends emit it as `s`.
    let handle = reply
        .results
        .get("session_handle")
        .and_then(|v| <&str>::try_from(v).ok())
        .ok_or_else(|| GlobalHotkeyError::Failed("session_handle missing".into()))?;
    OwnedObjectPath::try_from(handle).map_err(dbus_error)
}

/// `BindShortcuts` for one shortcut and await the confirmation dialog's response. `Ok(true)` if
/// the desktop accepted, `Ok(false)` if the user dismissed the dialog.
fn bind_shortcuts(
    conn: &zbus::blocking::Connection,
    session: &OwnedObjectPath,
    token: &str,
    id: ShortcutId,
    trigger: &str,
) -> Result<bool, GlobalHotkeyError> {
    let mut info: HashMap<String, OwnedValue> = HashMap::new();
    info.insert("description".into(), owned_str(id.0)?);
    info.insert("preferred_trigger".into(), owned_str(trigger)?);
    let shortcuts: Vec<(String, HashMap<String, OwnedValue>)> = vec![(id.0.to_string(), info)];
    let mut options: HashMap<String, OwnedValue> = HashMap::new();
    options.insert("handle_token".into(), owned_str(token)?);

    // `BindShortcuts(o session, a(sa{sv}) shortcuts, s parent_window, a{sv} options)`.
    let body = (session, &shortcuts, "", &options);
    let request_proxy = request_proxy(conn, request_path(conn, token))?;
    let mut response = response_stream(&request_proxy)?;
    portal_proxy(conn)?
        .call_noreply("BindShortcuts", &body)
        .map_err(bus_error)?;
    let reply = read_response(&mut response)?;
    // Code 1 is the documented "user cancelled"; other codes are refusals we cannot retry.
    Ok(reply.code == 0)
}

/// A proxy on the portal `Request` object at `path`, to read its `Response`.
fn request_proxy(
    conn: &zbus::blocking::Connection,
    path: String,
) -> Result<zbus::blocking::Proxy<'static>, GlobalHotkeyError> {
    // `new_owned` keeps the arguments `'static`, so the path (built per call) does not borrow.
    zbus::blocking::Proxy::new_owned(conn.clone(), DESKTOP_DEST, path, REQUEST_IFACE)
        .map_err(bus_error)
}

/// Install the `Response` signal stream on `request_proxy`.
fn response_stream(
    request_proxy: &zbus::blocking::Proxy<'static>,
) -> Result<zbus::blocking::proxy::SignalIterator<'static>, GlobalHotkeyError> {
    request_proxy
        .receive_signal("Response")
        .map_err(|_| GlobalHotkeyError::Failed("cannot watch the portal request".into()))
}

/// A decoded portal `Request::Response` (`(u code, a{sv} results)`).
struct PortalResponse {
    /// The numeric response code.
    code: u32,
    /// The results vardict.
    results: HashMap<String, OwnedValue>,
}

/// Read one `Response` signal from a stream the caller installed before the request.
fn read_response(
    stream: &mut zbus::blocking::proxy::SignalIterator<'_>,
) -> Result<PortalResponse, GlobalHotkeyError> {
    let message = stream
        .next()
        .ok_or_else(|| GlobalHotkeyError::Failed("portal request stream closed".into()))?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = message
        .body()
        .deserialize()
        .map_err(|_| GlobalHotkeyError::Failed("malformed portal response".into()))?;
    Ok(PortalResponse { code, results })
}

/// Start a detached thread that turns portal `Activated` signals into queued `ShortcutId`s.
fn spawn_reader(
    conn: &zbus::blocking::Connection,
    shared: Arc<Mutex<Shared>>,
    wake: Option<Arc<OwnedFd>>,
) {
    let conn = conn.clone();
    let _ = thread::Builder::new()
        .name("portal-hotkeys".into())
        .spawn(move || reader_loop(&conn, shared, wake));
}

/// Tell the loop a hotkey is waiting. The mailbox is the truth, so a full counter — a wake
/// already pending — is not a problem worth reporting.
fn signal(wake: &Arc<OwnedFd>) {
    let _ = rustix::io::write(&**wake, &1u64.to_ne_bytes());
}

/// Block on `Activated`; each activation naming a session we opened is looked up by id and queued.
fn reader_loop(
    conn: &zbus::blocking::Connection,
    shared: Arc<Mutex<Shared>>,
    wake: Option<Arc<OwnedFd>>,
) {
    let Ok(proxy) = portal_proxy(conn) else {
        return;
    };
    let Ok(mut signals) = proxy.receive_signal("Activated") else {
        return;
    };
    while let Some(message) = signals.next() {
        // `Activated(o session_handle, s shortcut_id, t timestamp, a{sv} options)`.
        let Ok(body) = message
            .body()
            .deserialize::<(OwnedObjectPath, String, u64)>()
        else {
            continue;
        };
        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        if !guard.sessions.iter().any(|s| *s == body.0.as_str()) {
            continue;
        }
        if let Some(id) = guard.lookup.get(&body.1).copied() {
            guard.activated.push(id);
            drop(guard);
            if let Some(wake) = &wake {
                signal(wake);
            }
        }
    }
}

/// The object path a portal request handle will live at for `token`.
fn request_path(conn: &zbus::blocking::Connection, token: &str) -> String {
    // Request handles are `/…/request/<unique-id-with-dashes>/<token>`, keyed off our own bus
    // unique name; mirror how the portal derives the path from it.
    let unique = conn
        .unique_name()
        .map(|n| n.as_str().to_string())
        .unwrap_or_else(|| "unknown".into());
    format!(
        "{DESKTOP_PATH}/request/{}/{}",
        unique.trim_start_matches(':').replace('.', "_"),
        token
    )
}

/// Build an `OwnedValue` holding a string, for the vardicts the portal methods take.
fn owned_str(s: &str) -> Result<OwnedValue, GlobalHotkeyError> {
    Value::from(s).try_to_owned().map_err(dbus_error)
}

/// Map a "name/object/interface not found" bus error to `Unsupported`, everything else to
/// `Failed`: the distinction the trait asks for on a machine without the portal.
fn bus_error(error: zbus::Error) -> GlobalHotkeyError {
    let text = error.to_string();
    if text.contains("org.freedesktop.DBus.Error.ServiceUnknown")
        || text.contains("org.freedesktop.DBus.Error.NameHasNoOwner")
        || text.contains("org.freedesktop.DBus.Error.UnknownObject")
        || text.contains("org.freedesktop.DBus.Error.UnknownInterface")
        || text.contains("No such")
        || text.contains("does not exist")
    {
        GlobalHotkeyError::Unsupported
    } else {
        GlobalHotkeyError::Failed(text)
    }
}

/// Map a zvariant conversion error to `Failed` with the message it gave.
fn dbus_error(error: zbus::zvariant::Error) -> GlobalHotkeyError {
    GlobalHotkeyError::Failed(error.to_string())
}
