//! System light/dark preference on Wayland, read from the XDG desktop portal.
//!
//! `org.freedesktop.appearance color-scheme` is the cross-DE standard (0 no preference,
//! 1 prefer dark, 2 prefer light). A background thread polls it over D-Bus: a session bus
//! connection, not a command, so the answer costs no process and no text to parse.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ui_platform::theme::{ThemePreference, ThemeWatcher};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

/// How often the background thread re-reads the portal.
const POLL: Duration = Duration::from_millis(1000);

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS_IFACE: &str = "org.freedesktop.portal.Settings";

/// The setting this watcher follows, as the portal namespaces it.
const APPEARANCE: (&str, &str) = ("org.freedesktop.appearance", "color-scheme");

/// A watcher backed by the XDG portal's `color-scheme`.
pub struct PortalThemeWatcher {
    shared: Arc<Mutex<ThemePreference>>,
    last: ThemePreference,
}

impl PortalThemeWatcher {
    /// Read the current value and start a background thread that follows changes.
    pub fn new() -> Self {
        let proxy = open_settings();
        let initial = proxy
            .as_ref()
            .and_then(read_portal)
            .unwrap_or(ThemePreference::NoPreference);
        let shared = Arc::new(Mutex::new(initial));
        let sink = Arc::clone(&shared);
        let _ = thread::Builder::new()
            .name("xdg-appearance".into())
            .spawn(move || watch_loop(proxy, sink));
        Self {
            shared,
            last: initial,
        }
    }
}

impl Default for PortalThemeWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeWatcher for PortalThemeWatcher {
    fn preference(&self) -> ThemePreference {
        *self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn poll(&mut self) -> Option<ThemePreference> {
        let now = self.preference();
        if now != self.last {
            self.last = now;
            Some(now)
        } else {
            None
        }
    }
}

/// The portal's settings object, or `None` on a machine with no session bus.
fn open_settings() -> Option<Proxy<'static>> {
    let connection = Connection::session().ok()?;
    Proxy::new(&connection, PORTAL_DEST, PORTAL_PATH, SETTINGS_IFACE).ok()
}

fn read_portal(proxy: &Proxy<'static>) -> Option<ThemePreference> {
    let value: OwnedValue = proxy.call("ReadOne", &APPEARANCE).ok()?;
    Some(preference(&value))
}

/// The portal answers with a variant, and a backend may wrap it once more; the first `u32`
/// inside is the scheme.
fn preference(value: &Value<'_>) -> ThemePreference {
    match value {
        Value::U32(1) => ThemePreference::PreferDark,
        Value::U32(2) => ThemePreference::PreferLight,
        Value::Value(inner) => preference(inner),
        _ => ThemePreference::NoPreference,
    }
}

fn watch_loop(proxy: Option<Proxy<'static>>, shared: Arc<Mutex<ThemePreference>>) {
    // No portal means nothing to follow; a preference that appears later needs a restart.
    let Some(proxy) = proxy else {
        return;
    };
    loop {
        thread::sleep(POLL);
        let Some(pref) = read_portal(&proxy) else {
            continue;
        };
        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        if *guard != pref {
            *guard = pref;
        }
    }
}
