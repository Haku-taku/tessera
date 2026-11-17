//! The desktop's own file chooser, over `org.freedesktop.portal.FileChooser`.
//!
//! The portal is the only portable way to show the dialog the desktop is configured with,
//! and the only one a sandboxed process may use. The call and its `Response` live on a
//! thread: a portal answer arrives as a D-Bus signal, which the compositor connection knows
//! nothing about, so the thread queues the answer and bumps the backend's eventfd — the
//! same wake the hotkey reader and the job worker use.
//!
//! The portal cannot pick several folders, force hidden files, or append a default
//! extension; [`ChooserCapabilities`] says so, and the caller's fallback policy decides what
//! happens instead of the dialog silently doing less than it was asked.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use percent_encoding::percent_decode_str;
use ui_platform::chooser::{
    ChooserAnswer, ChooserCapabilities, ChooserError, ChooserId, ChooserMode, ChooserRequest,
    ChooserSelection, FileTypeFilter,
};
use zbus::zvariant::{OwnedValue, Value};

/// Bus name owning every portal object.
const DESKTOP_DEST: &str = "org.freedesktop.portal.Desktop";
/// Object path of the portal's desktop root.
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
/// The file chooser interface.
const FILE_CHOOSER_IFACE: &str = "org.freedesktop.portal.FileChooser";
/// The interface on a portal request object.
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
/// The version that first defines the options this module uses.
const MIN_VERSION: u32 = 1;

/// What the portal offers. SaveFiles is defined by the interface but left out of the drawn
/// UI's scope, so it is not claimed; several folders have no portal spelling at all.
pub(crate) fn capabilities() -> ChooserCapabilities {
    let mut capabilities = ChooserCapabilities::none()
        .mode(ChooserMode::OpenFile, true)
        .mode(ChooserMode::OpenFiles, true)
        .mode(ChooserMode::SelectFolder, true)
        .mode(ChooserMode::SaveFile, true);
    capabilities.filters = true;
    capabilities.mime_filters = true;
    capabilities.filter_advice_only = true;
    capabilities.custom_places = false;
    capabilities.choices = true;
    capabilities.modeless = true;
    capabilities.show_hidden = false;
    capabilities.start_at_file = true;
    capabilities.default_extension = false;
    capabilities.overwrite_prompt = true;
    capabilities.state_persistence = false;
    capabilities
}

/// One surface's connection to the portal, opened on first use.
#[derive(Default)]
pub(crate) struct PortalChooser {
    connection: Option<zbus::blocking::Connection>,
    /// The reason the portal was not opened; kept so a caller asking per request does not
    /// pay for a bus round trip each time.
    unavailable: Option<String>,
    /// Distinct per request: the token names the request's object path.
    token: u32,
    /// Answers queued by the worker threads, drained by `chooser_events`.
    events: Arc<Mutex<Vec<(ChooserId, ChooserAnswer)>>>,
}

impl PortalChooser {
    /// The portal's capabilities, or `None` when no portal answers on this bus.
    pub(crate) fn capabilities(&mut self) -> Option<ChooserCapabilities> {
        self.ensure_connection().ok()?;
        Some(capabilities())
    }

    /// Start one dialog and return at once; the answer arrives through `take_events`.
    pub(crate) fn choose(
        &mut self,
        id: ChooserId,
        request: ChooserRequest,
        wake: Option<Arc<OwnedFd>>,
    ) -> Result<(), ChooserError> {
        if let Err(reason) = request.validate() {
            return Err(ChooserError::Failed(reason));
        }
        let connection = self
            .ensure_connection()
            .map_err(ChooserError::Unavailable)?
            .clone();
        let token = format!("ui_chooser_{}", self.token);
        self.token = self.token.wrapping_add(1);
        let events = Arc::clone(&self.events);
        thread::Builder::new()
            .name("portal-chooser".into())
            .spawn(move || {
                let result = run(&connection, &request, &token);
                if let Ok(mut queue) = events.lock() {
                    queue.push((id, ChooserAnswer::native(result)));
                }
                if let Some(wake) = &wake {
                    // A counter already above zero is still a wake; a failed write has
                    // nothing left to fix.
                    let _ = rustix::io::write(&**wake, &1u64.to_ne_bytes());
                }
            })
            .map_err(|error| {
                ChooserError::Failed(format!("cannot start the portal chooser thread: {error}"))
            })?;
        Ok(())
    }

    /// Answers finished since the last turn.
    pub(crate) fn take_events(&mut self) -> Vec<(ChooserId, ChooserAnswer)> {
        match self.events.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }

    fn ensure_connection(&mut self) -> Result<&zbus::blocking::Connection, String> {
        if let Some(reason) = &self.unavailable {
            return Err(reason.clone());
        }
        if self.connection.is_none() {
            let connection = zbus::blocking::Connection::session()
                .map_err(|error| format!("no session bus: {error}"))?;
            // A bus without the portal, or with a version too old for the options used
            // here, is a machine with no native chooser — not a failure to retry.
            let version = zbus::blocking::Proxy::new(
                &connection,
                DESKTOP_DEST,
                DESKTOP_PATH,
                FILE_CHOOSER_IFACE,
            )
            .and_then(|proxy| proxy.get_property::<u32>("version"))
            .map_err(|error| format!("the FileChooser portal is unavailable: {error}"))?;
            if version < MIN_VERSION {
                let reason = format!("the FileChooser portal is version {version}, need {MIN_VERSION}");
                self.unavailable = Some(reason.clone());
                return Err(reason);
            }
            self.connection = Some(connection);
        }
        self.connection
            .as_ref()
            .ok_or_else(|| "the portal connection disappeared".to_string())
    }
}

/// Run one request on the worker thread and wait for its `Response`.
fn run(
    connection: &zbus::blocking::Connection,
    request: &ChooserRequest,
    token: &str,
) -> Result<ChooserSelection, ChooserError> {
    let options = options_for(request, token)?;
    let method = if request.mode.is_save() {
        "SaveFile"
    } else {
        "OpenFile"
    };
    // The empty parent window is what the portal documents for a caller with no exported
    // surface; the desktop still shows the dialog, just without a transient parent.
    let title = request.title.clone().unwrap_or_default();
    let body = ("", title.as_str(), &options);

    let request_proxy = request_proxy(connection, request_path(connection, token))?;
    let mut response = request_proxy
        .receive_signal("Response")
        .map_err(|error| ChooserError::Failed(format!("cannot watch the portal request: {error}")))?;
    portal_proxy(connection)?
        .call_noreply(method, &body)
        .map_err(start_error)?;

    let message = response.next().ok_or_else(|| {
        ChooserError::Failed("the portal request stream closed".to_string())
    })?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = message
        .body()
        .deserialize()
        .map_err(|_| ChooserError::Failed("malformed portal response".to_string()))?;
    match code {
        0 => selection_from(results, request),
        1 => Err(ChooserError::Cancelled),
        // 2 is "other error" in the portal's parlance; a newer code is one this client
        // does not know but still is not a selection.
        code => Err(ChooserError::Failed(format!(
            "the portal chooser answered code {code}"
        ))),
    }
}

fn selection_from(
    results: HashMap<String, OwnedValue>,
    request: &ChooserRequest,
) -> Result<ChooserSelection, ChooserError> {
    let uris = results.get("uris").map(string_list).unwrap_or_default();
    if uris.is_empty() {
        return Err(ChooserError::Failed(
            "the portal chooser answered without a path".to_string(),
        ));
    }
    let mut paths = Vec::with_capacity(uris.len());
    for uri in uris {
        let path = file_uri_to_path(&uri).ok_or_else(|| {
            ChooserError::Failed(format!("the portal answered a URI this chooser cannot use: {uri}"))
        })?;
        paths.push(path);
    }
    let choices = results.get("choices").map(choice_list).unwrap_or_default();
    // The answer names the filter by label; the contract counts them by position. A filter
    // the portal applied unconditionally matches no request entry and stays unnamed.
    let filter = results
        .get("current_filter")
        .and_then(structure_name)
        .and_then(|label| request.filters.iter().position(|filter| filter.label == label));
    Ok(ChooserSelection {
        paths,
        filter,
        choices,
    })
}

/// A filter as the interface serializes it: an entry's number is 0 for a glob and 1 for a
/// MIME type.
///
/// The portal refuses a filter with no entries, so a filter that names neither asks for
/// every name instead of being dropped.
fn filter_entries(filter: &FileTypeFilter) -> Vec<(u32, String)> {
    let mut entries: Vec<(u32, String)> = filter
        .patterns
        .iter()
        .map(|pattern| (0, pattern.clone()))
        .chain(filter.mime_types.iter().map(|mime| (1, mime.clone())))
        .collect();
    if entries.is_empty() {
        entries.push((0, "*".to_string()));
    }
    entries
}

/// The option dictionary one request turns into.
///
/// Every key here is optional in the interface; a desktop that does not know one ignores
/// it, which is why the capabilities call the filters advice rather than enforcement.
fn options_for(
    request: &ChooserRequest,
    token: &str,
) -> Result<HashMap<String, OwnedValue>, ChooserError> {
    let mut options: HashMap<String, OwnedValue> = HashMap::new();
    insert(&mut options, "handle_token", string(token)?);
    insert(&mut options, "modal", boolean(request.modal)?);
    if let Some(title) = &request.accept_label {
        insert(&mut options, "accept_label", string(title)?);
    }
    match request.mode {
        ChooserMode::OpenFiles => insert(&mut options, "multiple", boolean(true)?),
        ChooserMode::SelectFolder => insert(&mut options, "directory", boolean(true)?),
        _ => {}
    }
    if !request.filters.is_empty() {
        let filters: Vec<(String, Vec<(u32, String)>)> = request
            .filters
            .iter()
            .map(|filter| (filter.label.clone(), filter_entries(filter)))
            .collect();
        insert(&mut options, "filters", owned(Value::from(filters))?);
        if let Some(active) = request.active_filter() {
            let active = (active.label.clone(), filter_entries(active));
            insert(&mut options, "current_filter", owned(Value::from(active))?);
        }
    }
    if !request.choices.is_empty() {
        let choices: Vec<(String, String, Vec<(String, String)>, String)> = request
            .choices
            .iter()
            .map(|choice| {
                let options = match &choice.kind {
                    ui_platform::chooser::ChoiceKind::Select(options) => options
                        .iter()
                        .map(|option| (option.value.clone(), option.label.clone()))
                        .collect(),
                    // A boolean portal choice is a two-entry list; the labels are the
                    // desktop's business and this client gives it none.
                    ui_platform::chooser::ChoiceKind::Boolean => vec![
                        ("true".to_string(), String::new()),
                        ("false".to_string(), String::new()),
                    ],
                };
                (
                    choice.id.clone(),
                    choice.label.clone(),
                    options,
                    choice.initial.clone(),
                )
            })
            .collect();
        insert(&mut options, "choices", owned(Value::from(choices))?);
    }

    // A file start sets both the folder and, for save, the name; a folder start sets only
    // the folder. Both are byte arrays terminated by NUL, not strings.
    let (folder, file) = match &request.start {
        ui_platform::chooser::StartAt::Remembered(path)
        | ui_platform::chooser::StartAt::Folder(path) => (Some(path.as_path()), None),
        ui_platform::chooser::StartAt::File(path) => (path.parent(), Some(path.as_path())),
    };
    if let Some(folder) = folder {
        insert(&mut options, "current_folder", path_bytes(folder)?);
    }
    if let Some(file) = file {
        if request.mode.is_save() {
            if let Some(name) = file.file_name().and_then(|name| name.to_str()) {
                insert(&mut options, "current_name", string(name)?);
            }
        } else {
            insert(&mut options, "current_file", path_bytes(file)?);
        }
    }
    if let Some(name) = request
        .save
        .as_ref()
        .and_then(|save| save.suggested_name.as_ref())
        && !options.contains_key("current_name")
    {
        insert(&mut options, "current_name", string(name)?);
    }
    Ok(options)
}

/// A portal object path for this request, derived from the bus unique name and the token.
fn request_path(connection: &zbus::blocking::Connection, token: &str) -> String {
    let unique = connection
        .unique_name()
        .map(|name| name.as_str().to_string())
        .unwrap_or_else(|| "unknown".into());
    format!(
        "{DESKTOP_PATH}/request/{}/{}",
        unique.trim_start_matches(':').replace('.', "_"),
        token
    )
}

fn portal_proxy(
    connection: &zbus::blocking::Connection,
) -> Result<zbus::blocking::Proxy<'static>, ChooserError> {
    zbus::blocking::Proxy::new_owned(
        connection.clone(),
        DESKTOP_DEST,
        DESKTOP_PATH,
        FILE_CHOOSER_IFACE,
    )
    .map_err(start_error)
}

fn request_proxy(
    connection: &zbus::blocking::Connection,
    path: String,
) -> Result<zbus::blocking::Proxy<'static>, ChooserError> {
    zbus::blocking::Proxy::new_owned(
        connection.clone(),
        DESKTOP_DEST,
        path,
        REQUEST_IFACE,
    )
    .map_err(start_error)
}

fn start_error(error: zbus::Error) -> ChooserError {
    let text = error.to_string();
    if text.contains("org.freedesktop.DBus.Error.ServiceUnknown")
        || text.contains("org.freedesktop.DBus.Error.NameHasNoOwner")
        || text.contains("org.freedesktop.DBus.Error.UnknownObject")
        || text.contains("org.freedesktop.DBus.Error.UnknownInterface")
    {
        ChooserError::Unavailable(text)
    } else {
        ChooserError::Failed(text)
    }
}

/// `file:///home/me/a.png` as a path; anything with a remote authority is refused, because
/// this contract returns filesystem paths only.
fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = match rest.strip_prefix("localhost") {
        Some(path) => path,
        None => rest,
    };
    if !path.starts_with('/') {
        return None;
    }
    let decoded = percent_decode_str(path).decode_utf8().ok()?;
    Some(PathBuf::from(decoded.into_owned()))
}

fn string(text: &str) -> Result<OwnedValue, ChooserError> {
    // Owned, not borrowed: the value outlives this call in the option dictionary.
    owned(Value::from(text.to_string()))
}

/// The strings of an `as` value; a value of another shape contributes none.
fn string_list(value: &OwnedValue) -> Vec<String> {
    let Value::Array(array) = &**value else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|element| <&str>::try_from(element).ok().map(str::to_string))
        .collect()
}

/// The `(id, value)` pairs of an `a(ss)` value, the portal's `choices` answer.
fn choice_list(value: &OwnedValue) -> Vec<(String, String)> {
    let Value::Array(array) = &**value else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|element| {
            let Value::Structure(choice) = element else {
                return None;
            };
            let fields = choice.fields();
            let id = <&str>::try_from(fields.first()?).ok()?;
            let value = <&str>::try_from(fields.get(1)?).ok()?;
            Some((id.to_string(), value.to_string()))
        })
        .collect()
}

/// The first string of a `(s…)` structure — a filter's label in the `current_filter` result.
fn structure_name(value: &OwnedValue) -> Option<String> {
    let Value::Structure(structure) = &**value else {
        return None;
    };
    <&str>::try_from(structure.fields().first()?)
        .ok()
        .map(str::to_string)
}

fn boolean(value: bool) -> Result<OwnedValue, ChooserError> {
    owned(Value::from(value))
}

/// A NUL-terminated path, the portal's `ay` convention for a filename.
fn path_bytes(path: &std::path::Path) -> Result<OwnedValue, ChooserError> {
    use std::os::unix::ffi::OsStrExt;
    let mut bytes = path.as_os_str().as_bytes().to_vec();
    bytes.push(0);
    owned(Value::from(bytes))
}

fn owned(value: Value<'static>) -> Result<OwnedValue, ChooserError> {
    value
        .try_to_owned()
        .map_err(|error| ChooserError::Failed(format!("cannot encode a portal option: {error}")))
}

fn insert(options: &mut HashMap<String, OwnedValue>, key: &str, value: OwnedValue) {
    options.insert(key.to_string(), value);
}
