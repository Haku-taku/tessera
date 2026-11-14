//! The seat's clipboard: `wl_data_device`, and the pipes a copy or a paste runs through.
//!
//! Wayland's clipboard is pull-based: the copy side must stay alive and answer `send`, and
//! a paste side asks an offer for bytes down a pipe. Neither may block the event loop, so
//! reads and writes are non-blocking and the window's wait polls their fds.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
};
use wayland_client::{Connection, Dispatch, QueueHandle};

use crate::window::WindowState;

/// The mime types we offer, most specific first; a peer picks the first it understands.
pub(crate) const MIMES: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];

/// What we are serving while our selection is on the clipboard.
#[derive(Clone, Debug)]
pub(crate) enum Contents {
    Text(String),
    /// Opaque bytes under one MIME type, such as a PNG image, shared with every reader.
    Bytes {
        mime: String,
        data: std::sync::Arc<Vec<u8>>,
    },
}
/// How long a paste waits for the other side before giving up.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// A pipe we are reading a peer's clipboard text from.
struct ReadJob {
    fd: File,
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
    pub(crate) fn serve(&mut self, fd: OwnedFd) {
        let bytes = match self.contents.as_ref() {
            Some(Contents::Text(text)) => Arc::new(text.clone().into_bytes()),
            Some(Contents::Bytes { data, .. }) => Arc::clone(data),
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
        match &contents {
            Contents::Text(_) => {
                for mime in MIMES {
                    source.offer(mime.to_string());
                }
            }
            // One MIME type, so a peer cannot ask for a text reading of an image.
            Contents::Bytes { mime, .. } => source.offer(mime.clone()),
        }
        device.set_selection(Some(&source), serial);
        // The payload stays here: the peer will ask for it later, through `send`.
        self.payload.set(contents);
        self.source = Some(source);
        let _ = conn.flush();
        true
    }

    /// Queue our payload for a peer's `send` request.
    pub(crate) fn serve(&mut self, fd: OwnedFd) {
        self.payload.serve(fd);
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

    /// Ask the current offer for its text; the answer arrives as `Event::Paste`.
    pub(crate) fn request_paste(&mut self, conn: &Connection) -> bool {
        let Some(offer) = self.offer.clone() else {
            return false;
        };
        let Some(mime) = self
            .mimes
            .iter()
            .find(|m| MIMES.contains(&m.as_str()))
            .cloned()
        else {
            return false;
        };
        let Ok((read_fd, write_fd)) = rustix::pipe::pipe_with(
            rustix::pipe::PipeFlags::NONBLOCK | rustix::pipe::PipeFlags::CLOEXEC,
        ) else {
            return false;
        };
        offer.receive(mime, write_fd.as_fd());
        // Our end of the write side must close, or the reader never sees EOF.
        drop(write_fd);
        self.read = Some(ReadJob {
            fd: File::from(read_fd),
            bytes: Vec::new(),
            deadline: Instant::now() + READ_TIMEOUT,
        });
        let _ = conn.flush();
        true
    }

    /// Move bytes on every pipe we hold. Returns the text of a paste that finished.
    pub(crate) fn service(&mut self) -> Option<String> {
        let mut done = None;
        if let Some(read) = &mut self.read {
            let mut buf = [0u8; 8192];
            loop {
                match (&read.fd).read(&mut buf) {
                    Ok(0) => {
                        // EOF: the peer closed its side, so the text is complete.
                        done = Some(String::from_utf8_lossy(&read.bytes).into_owned());
                        break;
                    }
                    Ok(n) => read.bytes.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        // A broken pipe is an empty paste, not a stuck one.
                        done = Some(String::new());
                        break;
                    }
                }
            }
            if done.is_none() && Instant::now() > read.deadline {
                // A peer that never answers must not hold the clipboard hostage.
                done = Some(String::new());
            }
        }
        if done.is_some() {
            self.read = None;
        }
        // Peers waiting on our own payload, in the same pass as the paste read.
        self.source.service();
        done
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
        });
    }

    /// Claim the pending offer `id` names, with the MIME types announced for it.
    ///
    /// A drag takes it into the drag state, a selection into the paste state. The caller
    /// owns the returned object and destroys it.
    pub(crate) fn take_pending(
        &mut self,
        id: &wl_data_offer::WlDataOffer,
    ) -> (wl_data_offer::WlDataOffer, Vec<String>) {
        match self.pending.take() {
            Some(pending) if &pending.offer == id => (pending.offer, pending.mimes),
            Some(pending) => {
                // The announcement and the claim disagree; keep the claimed object, which
                // is the one the compositor names, and let the other go.
                pending.offer.destroy();
                (id.clone(), Vec::new())
            }
            None => (id.clone(), Vec::new()),
        }
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
                let (offer, mimes) = self.take_pending(&id);
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
    pub(crate) fn serve_source(&mut self, fd: OwnedFd) {
        self.source.serve(fd);
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
