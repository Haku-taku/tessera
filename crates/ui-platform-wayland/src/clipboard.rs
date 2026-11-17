//! The seat's clipboard: `wl_data_device`, and the pipes a copy or a paste runs through.
//!
//! Wayland's clipboard is pull-based: the copy side must stay alive and answer `send`, and
//! a paste side asks an offer for bytes down a pipe. Neither may block the event loop, so
//! reads and writes are non-blocking and the window's wait polls their fds.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_core::clipboard::ClipboardContent;
use ui_core::dnd::DropEffect;
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
};
use wayland_client::{Connection, Dispatch, QueueHandle};

use crate::window::WindowState;

/// The mime types we offer, most specific first; a peer picks the first it understands.
pub(crate) const MIMES: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];

/// The baseline file-list type every desktop understands.
const URI_LIST: &str = "text/uri-list";
/// GNOME's file-list type that carries the copy/cut mark in its header.
const GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";
/// KDE's cut mark: the payload is just `1`, read beside `text/uri-list`.
const KDE_CUT: &str = "application/x-kde-cutselection";
/// A raster image, offered by picture tools and some browsers.
const PNG: &str = "image/png";

/// What we are serving while our selection is on the clipboard.
#[derive(Clone, Debug)]
pub(crate) enum Contents {
    Text(String),
    /// Opaque bytes under one MIME type, such as a PNG image, shared with every reader.
    Bytes {
        mime: String,
        data: std::sync::Arc<Vec<u8>>,
    },
    /// Copied files, served as a URI list and the desktop's cut marks.
    Files { paths: Vec<PathBuf>, cut: bool },
}

impl Contents {
    /// The MIME types this payload answers. Peers pick the first they understand.
    pub(crate) fn mimes(&self) -> Vec<String> {
        match self {
            Contents::Text(_) => MIMES.iter().map(|mime| (*mime).to_owned()).collect(),
            // One MIME type, so a peer cannot ask for a text reading of an image.
            Contents::Bytes { mime, .. } => vec![mime.clone()],
            // The GNOME type comes first: it carries the cut mark in the same transfer, so
            // a reader needs no second read to know what the copy means.
            Contents::Files { cut, .. } => {
                let mut mimes = vec![GNOME_COPIED_FILES.to_owned(), URI_LIST.to_owned()];
                if *cut {
                    mimes.push(KDE_CUT.to_owned());
                }
                mimes
            }
        }
    }

    /// The bytes for one of [`mimes`](Self::mimes). Another type gets an empty payload.
    fn bytes_for(&self, mime: &str) -> Arc<Vec<u8>> {
        match self {
            Contents::Text(text) => Arc::new(text.clone().into_bytes()),
            Contents::Bytes { data, .. } => Arc::clone(data),
            Contents::Files { paths, cut } => {
                let list = ui_platform::uri_list::encode(paths);
                let bytes = if mime.eq_ignore_ascii_case(GNOME_COPIED_FILES) {
                    let head: &[u8] = if *cut { b"cut\n" } else { b"copy\n" };
                    let mut bytes = head.to_vec();
                    bytes.extend_from_slice(&list);
                    bytes
                } else if mime.eq_ignore_ascii_case(URI_LIST) {
                    list
                } else if mime.eq_ignore_ascii_case(KDE_CUT) {
                    if *cut { b"1".to_vec() } else { Vec::new() }
                } else {
                    Vec::new()
                };
                Arc::new(bytes)
            }
        }
    }
}
/// How long a paste waits for the other side before giving up.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// A pipe we are reading a peer's clipboard payload from.
struct ReadJob {
    fd: File,
    /// The type that was asked for; it decides how the bytes are read.
    mime: String,
    bytes: Vec<u8>,
    deadline: Instant,
}

/// A pipe a peer asked us to write our clipboard text into.
struct WriteJob {
    fd: File,
    bytes: std::sync::Arc<Vec<u8>>,
    at: usize,
    deadline: Instant,
}

/// Pipes a peer reads a payload from, drained without blocking the loop.
///
/// A selection and a drag source both hand peers a payload this way, and neither may stall
/// a frame: the pipes are polled by the window's wait and written a slice per turn.
#[derive(Default)]
pub(crate) struct Outbox {
    writes: Vec<WriteJob>,
}

impl Outbox {
    /// Queue `bytes` for a peer's `send` request.
    pub(crate) fn push(&mut self, fd: OwnedFd, bytes: Arc<Vec<u8>>) {
        self.writes.push(WriteJob {
            fd: File::from(fd),
            bytes,
            at: 0,
            deadline: Instant::now() + READ_TIMEOUT,
        });
    }

    /// Move pending writes along. Safe to call every frame; it never blocks.
    pub(crate) fn service(&mut self) {
        let mut i = 0;
        while i < self.writes.len() {
            let write = &mut self.writes[i];
            let mut finished = false;
            while write.at < write.bytes.len() {
                match (&write.fd).write(&write.bytes[write.at..]) {
                    Ok(0) => {
                        finished = true;
                        break;
                    }
                    Ok(n) => write.at += n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        finished = true;
                        break;
                    }
                }
            }
            if finished || write.at >= write.bytes.len() || Instant::now() > write.deadline {
                self.writes.remove(i);
            } else {
                i += 1;
            }
        }
    }

    pub(crate) fn fds(&self) -> Vec<(BorrowedFd<'_>, bool)> {
        self.writes.iter().map(|w| (w.fd.as_fd(), true)).collect()
    }

    pub(crate) fn timeout(&self) -> Option<Duration> {
        self.writes
            .iter()
            .map(|w| w.deadline)
            .min()
            .map(|at| at.saturating_duration_since(Instant::now()))
    }

    /// Whether the peer has taken everything it asked for.
    pub(crate) fn idle(&self) -> bool {
        self.writes.is_empty()
    }
}

/// The bytes behind our selection and the pipes a peer reads them through.
///
/// A `wl_data_source` and a data-control source differ only in which object the compositor
/// asks, so the payload and its drain are shared: neither may block the loop, and both die
/// with the process that owns the selection.
pub(crate) struct Payload {
    contents: Option<Contents>,
    outbox: Outbox,
}

impl Payload {
    pub(crate) fn new() -> Self {
        Self {
            contents: None,
            outbox: Outbox::default(),
        }
    }

    pub(crate) fn set(&mut self, contents: Contents) {
        self.contents = Some(contents);
    }

    pub(crate) fn clear(&mut self) {
        self.contents = None;
    }

    /// Queue our payload for a peer's `send` request.
    pub(crate) fn serve(&mut self, mime: &str, fd: OwnedFd) {
        let bytes = match self.contents.as_ref() {
            Some(contents) => contents.bytes_for(mime),
            None => Arc::new(Vec::new()),
        };
        self.outbox.push(fd, bytes);
    }

    pub(crate) fn service(&mut self) {
        self.outbox.service();
    }

    pub(crate) fn fds(&self) -> Vec<(BorrowedFd<'_>, bool)> {
        self.outbox.fds()
    }

    pub(crate) fn timeout(&self) -> Option<Duration> {
        self.outbox.timeout()
    }
}

/// The write side: our selection and the payload behind it.
///
/// It has no window in it, so it can be owned by whatever lives long enough: a window's
/// clipboard while that window exists, or a process-level host for an app whose copying
/// surface goes away. Wayland's clipboard is pull-based, so destroying the source takes the
/// selection with it.
pub(crate) struct SelectionSource {
    source: Option<wl_data_source::WlDataSource>,
    payload: Payload,
}

impl SelectionSource {
    pub(crate) fn new() -> Self {
        Self {
            source: None,
            payload: Payload::new(),
        }
    }

    /// Offer `contents` as the selection; `serial` must come from a recent input event.
    pub(crate) fn offer<State: Dispatch<wl_data_source::WlDataSource, ()> + 'static>(
        &mut self,
        contents: Contents,
        manager: &wl_data_device_manager::WlDataDeviceManager,
        device: &wl_data_device::WlDataDevice,
        serial: u32,
        qh: &QueueHandle<State>,
        conn: &Connection,
    ) -> bool {
        if let Some(old) = self.source.take() {
            old.destroy();
        }
        let source = manager.create_data_source(qh, ());
        for mime in contents.mimes() {
            source.offer(mime);
        }
        device.set_selection(Some(&source), serial);
        // The payload stays here: the peer will ask for it later, through `send`.
        self.payload.set(contents);
        self.source = Some(source);
        let _ = conn.flush();
        true
    }

    /// Queue our payload for a peer's `send` request.
    pub(crate) fn serve(&mut self, mime: &str, fd: OwnedFd) {
        self.payload.serve(mime, fd);
    }

    /// Move pending writes along. Safe to call every frame; it never blocks.
    pub(crate) fn service(&mut self) {
        self.payload.service();
    }

    pub(crate) fn fds(&self) -> Vec<(BorrowedFd<'_>, bool)> {
        self.payload.fds()
    }

    pub(crate) fn timeout(&self) -> Option<Duration> {
        self.payload.timeout()
    }

    /// Someone else took the selection: stop serving and let the source go.
    pub(crate) fn cancelled(&mut self) {
        self.payload.clear();
        if let Some(source) = self.source.take() {
            source.destroy();
        }
    }
}

impl Drop for SelectionSource {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            source.destroy();
        }
    }
}

/// An offer announced by `data_offer` and not yet claimed by a selection or a drag.
struct PendingOffer {
    offer: wl_data_offer::WlDataOffer,
    mimes: Vec<String>,
    /// The actions a drag source permits; only a drag announces them.
    actions: wl_data_device_manager::DndAction,
}

/// Everything the seat's clipboard owns.
pub(crate) struct Clipboard {
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    device: Option<wl_data_device::WlDataDevice>,
    /// What the compositor last offered us; invalid once we lose keyboard focus.
    offer: Option<wl_data_offer::WlDataOffer>,
    mimes: Vec<String>,
    /// The offer a following `selection` or `enter` names; its MIME types arrive before
    /// either, so they are held here until the event says which side wants them.
    pending: Option<PendingOffer>,
    /// Our side of the clipboard while this window is the one that copied.
    source: SelectionSource,
    /// Serial of the latest input event; `set_selection` is ignored without a fresh one.
    serial: u32,
    read: Option<ReadJob>,
    /// Paths read before the KDE cut mark that follows them.
    pending_paths: Option<Vec<PathBuf>>,
    /// The effect a marker already read states, waiting for the paths or a second marker.
    pending_effect: Option<DropEffect>,
}

impl Clipboard {
    pub(crate) fn new() -> Self {
        Self {
            manager: None,
            device: None,
            offer: None,
            mimes: Vec::new(),
            pending: None,
            source: SelectionSource::new(),
            serial: 0,
            read: None,
            pending_paths: None,
            pending_effect: None,
        }
    }

    /// Take the seat's data device, which is what every copy and paste goes through.
    pub(crate) fn attach(
        &mut self,
        manager: wl_data_device_manager::WlDataDeviceManager,
        device: wl_data_device::WlDataDevice,
    ) {
        self.manager = Some(manager);
        self.device = Some(device);
    }

    pub(crate) fn note_serial(&mut self, serial: u32) {
        self.serial = serial;
    }

    /// The fds a wait should poll, alongside the connection, and whether each wants write.
    pub(crate) fn fds(&self) -> Vec<(BorrowedFd<'_>, bool)> {
        let mut out: Vec<(BorrowedFd<'_>, bool)> = Vec::new();
        if let Some(read) = &self.read {
            out.push((read.fd.as_fd(), false));
        }
        out.extend(self.source.fds());
        out
    }

    /// How long until the nearest deadline, so a wait never sleeps past it.
    pub(crate) fn timeout(&self) -> Option<Duration> {
        let read = self
            .read
            .as_ref()
            .map(|r| r.deadline.saturating_duration_since(Instant::now()));
        match (read, self.source.timeout()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The serial a `set_selection` needs: the latest input event this window saw.
    pub(crate) fn serial(&self) -> u32 {
        self.serial
    }

    /// Offer `contents` as the selection. Returns false when the compositor gave us no device.
    pub(crate) fn copy(
        &mut self,
        contents: Contents,
        conn: &Connection,
        qh: &QueueHandle<WindowState>,
    ) -> bool {
        let (Some(device), Some(manager)) = (self.device.clone(), self.manager.clone()) else {
            return false;
        };
        self.source
            .offer(contents, &manager, &device, self.serial, qh, conn)
    }

    /// Ask the current offer for its payload; the answer arrives as `Event::Paste`.
    ///
    /// Files win over pictures, pictures over text: the same order a Windows paste takes,
    /// and a picture tool's offer usually carries all three readings.
    pub(crate) fn request_paste(&mut self, conn: &Connection) -> bool {
        self.pending_paths = None;
        self.pending_effect = None;
        let Some(mime) = paste_mime(&self.mimes) else {
            return false;
        };
        let started = self.start_read(mime);
        if started {
            let _ = conn.flush();
        }
        started
    }

    /// Open one pipe for `mime` on the current offer.
    fn start_read(&mut self, mime: String) -> bool {
        let Some(offer) = self.offer.clone() else {
            return false;
        };
        let Ok((read_fd, write_fd)) = rustix::pipe::pipe_with(
            rustix::pipe::PipeFlags::NONBLOCK | rustix::pipe::PipeFlags::CLOEXEC,
        ) else {
            return false;
        };
        offer.receive(mime.clone(), write_fd.as_fd());
        // Our end of the write side must close, or the reader never sees EOF.
        drop(write_fd);
        self.read = Some(ReadJob {
            fd: File::from(read_fd),
            mime,
            bytes: Vec::new(),
            deadline: Instant::now() + READ_TIMEOUT,
        });
        true
    }

    /// Move bytes on every pipe we hold. Returns the payload of a paste that finished.
    pub(crate) fn service(&mut self) -> Option<ClipboardContent> {
        let mut done = None;
        if let Some(read) = &mut self.read {
            let mut buf = [0u8; 8192];
            loop {
                match (&read.fd).read(&mut buf) {
                    Ok(0) => {
                        // EOF: the peer closed its side, so the payload is complete.
                        done = Some((read.mime.clone(), std::mem::take(&mut read.bytes)));
                        break;
                    }
                    Ok(n) => read.bytes.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        // A broken pipe is an empty paste, not a stuck one.
                        done = Some((read.mime.clone(), Vec::new()));
                        break;
                    }
                }
            }
            if done.is_none() && Instant::now() > read.deadline {
                // A peer that never answers must not hold the clipboard hostage.
                done = Some((read.mime.clone(), Vec::new()));
            }
        }
        let content = match done {
            Some((mime, bytes)) => {
                self.read = None;
                self.finish_read(&mime, bytes)
            }
            None => None,
        };
        // Peers waiting on our own payload, in the same pass as the paste read.
        self.source.service();
        content
    }

    /// Turn one finished transfer into the paste, or start the second transfer a KDE cut
    /// mark or a GNOME list needs.
    fn finish_read(&mut self, mime: &str, bytes: Vec<u8>) -> Option<ClipboardContent> {
        if mime.eq_ignore_ascii_case(GNOME_COPIED_FILES) {
            let (paths, effect) = decode_gnome(bytes);
            if self.offers(KDE_CUT) {
                // The two marks must agree; KDE can only veto a cut, never invent one.
                self.pending_paths = Some(paths);
                self.pending_effect = Some(effect);
                if self.start_read(KDE_CUT.to_owned()) {
                    return None;
                }
                let paths = self.pending_paths.take().unwrap_or_default();
                return Some(ClipboardContent::Files { paths, effect });
            }
            return Some(ClipboardContent::Files { paths, effect });
        }
        if mime.eq_ignore_ascii_case(KDE_CUT) {
            let cut = bytes.first() == Some(&b'1');
            if let Some(paths) = self.pending_paths.take() {
                let effect = if cut && self.pending_effect.take() == Some(DropEffect::Move) {
                    DropEffect::Move
                } else {
                    DropEffect::Copy
                };
                return Some(ClipboardContent::Files { paths, effect });
            }
            self.pending_effect = Some(if cut {
                DropEffect::Move
            } else {
                DropEffect::Copy
            });
            if self.start_read(URI_LIST.to_owned()) {
                return None;
            }
            return Some(ClipboardContent::Files {
                paths: Vec::new(),
                effect: DropEffect::Copy,
            });
        }
        if mime.eq_ignore_ascii_case(URI_LIST) {
            let paths = ui_platform::uri_list::decode(&bytes).unwrap_or_default();
            return Some(ClipboardContent::Files {
                paths,
                effect: self.pending_effect.take().unwrap_or(DropEffect::Copy),
            });
        }
        Some(paste_content(mime, bytes))
    }

    /// Whether the current offer announced `mime`.
    fn offers(&self, mime: &str) -> bool {
        self.mimes
            .iter()
            .any(|offered| offered.eq_ignore_ascii_case(mime))
    }

    /// Take the offer `data_offer` announced, so the following `selection` or `enter`
    /// claims it together with the MIME types that arrived in between.
    pub(crate) fn set_pending(&mut self, offer: wl_data_offer::WlDataOffer) {
        if self.pending.as_ref().is_some_and(|p| p.offer == offer) {
            return;
        }
        if let Some(old) = self.pending.take() {
            old.offer.destroy();
        }
        self.pending = Some(PendingOffer {
            offer,
            mimes: Vec::new(),
            actions: wl_data_device_manager::DndAction::empty(),
        });
    }

    /// Claim the pending offer `id` names, with the MIME types and actions announced for it.
    ///
    /// A drag takes it into the drag state, a selection into the paste state. The caller
    /// owns the returned object and destroys it.
    pub(crate) fn take_pending(
        &mut self,
        id: &wl_data_offer::WlDataOffer,
    ) -> (
        wl_data_offer::WlDataOffer,
        Vec<String>,
        wl_data_device_manager::DndAction,
    ) {
        match self.pending.take() {
            Some(pending) if &pending.offer == id => {
                (pending.offer, pending.mimes, pending.actions)
            }
            Some(pending) => {
                // The announcement and the claim disagree; keep the claimed object, which
                // is the one the compositor names, and let the other go.
                pending.offer.destroy();
                (
                    id.clone(),
                    Vec::new(),
                    wl_data_device_manager::DndAction::empty(),
                )
            }
            None => (
                id.clone(),
                Vec::new(),
                wl_data_device_manager::DndAction::empty(),
            ),
        }
    }

    /// Record the actions a drag source permits on the pending offer; false when the
    /// offer is not pending (the drag module keeps it once `enter` claimed it).
    pub(crate) fn note_actions(
        &mut self,
        offer: &wl_data_offer::WlDataOffer,
        actions: wl_data_device_manager::DndAction,
    ) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if &pending.offer != offer {
            return false;
        }
        pending.actions = actions;
        true
    }

    /// Record a MIME type on whichever offer it belongs to. False when it is neither the
    /// selection's nor a pending one (a drag's, which the drag module keeps).
    pub(crate) fn note_mime(&mut self, offer: &wl_data_offer::WlDataOffer, mime: String) -> bool {
        if let Some(pending) = self.pending.as_mut()
            && &pending.offer == offer
        {
            pending.mimes.push(mime);
            return true;
        }
        if self.offer.as_ref() == Some(offer) {
            self.mimes.push(mime);
            return true;
        }
        false
    }

    /// Publish the selection: the offer `selection` names, or none.
    pub(crate) fn set_selection_offer(&mut self, id: Option<wl_data_offer::WlDataOffer>) {
        // A `selection` with no offer also drops an announcement the compositor abandoned.
        let (offer, mimes) = match id {
            Some(id) => {
                let (offer, mimes, _) = self.take_pending(&id);
                (Some(offer), mimes)
            }
            None => {
                if let Some(pending) = self.pending.take() {
                    pending.offer.destroy();
                }
                (None, Vec::new())
            }
        };
        // `data_offer` and `selection` announce the same object; destroying it twice kills it.
        if let (Some(old), Some(new)) = (self.offer.as_ref(), offer.as_ref())
            && old == new
        {
            return;
        }
        if let Some(old) = self.offer.take() {
            old.destroy();
        }
        self.mimes = mimes;
        self.offer = offer;
    }

    /// The device an outgoing drag starts from, when the compositor gave the seat one.
    pub(crate) fn device(
        &self,
    ) -> Option<(
        wl_data_device_manager::WlDataDeviceManager,
        wl_data_device::WlDataDevice,
    )> {
        Some((self.manager.clone()?, self.device.clone()?))
    }

    /// Queue a peer's request for our selection's bytes.
    pub(crate) fn serve_source(&mut self, mime: &str, fd: OwnedFd) {
        self.source.serve(mime, fd);
    }

    /// The compositor took the selection away from us.
    pub(crate) fn cancel_source(&mut self) {
        self.source.cancelled();
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        if let Some(offer) = self.offer.take() {
            offer.destroy();
        }
        if let Some(pending) = self.pending.take() {
            pending.offer.destroy();
        }
    }
}

/// The type to read from an offer, best first.
///
/// The GNOME file type is read before `text/uri-list` because its header says whether the
/// copy was a cut, and both carry the same list. KDE's mark is a separate transfer, so it
/// is read first and the list follows; with neither mark the paste is a copy.
fn paste_mime(offered: &[String]) -> Option<String> {
    let has = |mime: &str| {
        offered
            .iter()
            .find(|known| known.eq_ignore_ascii_case(mime))
            .cloned()
    };
    if let Some(mime) = has(GNOME_COPIED_FILES) {
        return Some(mime);
    }
    if has(URI_LIST).is_some() {
        return has(KDE_CUT).or_else(|| has(URI_LIST));
    }
    has(PNG).or_else(|| MIMES.iter().find_map(|mime| has(mime)))
}

/// Split a GNOME file transfer into its list and the effect its header states.
fn decode_gnome(bytes: Vec<u8>) -> (Vec<PathBuf>, DropEffect) {
    let split = bytes.iter().position(|byte| *byte == b'\n');
    let (head, list) = match split {
        Some(at) => (&bytes[..at], &bytes[at + 1..]),
        None => (&[][..], &bytes[..]),
    };
    let effect = if head.eq_ignore_ascii_case(b"cut") {
        DropEffect::Move
    } else {
        DropEffect::Copy
    };
    (
        ui_platform::uri_list::decode(list).unwrap_or_default(),
        effect,
    )
}

/// Read a finished transfer that is not a file list, according to the type asked for.
fn paste_content(mime: &str, bytes: Vec<u8>) -> ClipboardContent {
    if MIMES.iter().any(|known| mime.eq_ignore_ascii_case(known)) {
        return ClipboardContent::Text(String::from_utf8_lossy(&bytes).into_owned());
    }
    ClipboardContent::Bytes {
        mime: mime.to_owned(),
        data: Arc::new(bytes),
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for WindowState {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: <wl_data_device_manager::WlDataDeviceManager as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The manager has no events; it only creates data devices.
    }
}

// The data device itself is dispatched by `crate::dnd`: one device carries both the
// selection and drag-and-drop, and only the event says which side the offer belongs to.
