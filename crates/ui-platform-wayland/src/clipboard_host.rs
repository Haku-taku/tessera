//! A clipboard owner that outlives the surface that copied.
//!
//! Wayland's clipboard is pull-based: the source stays alive and answers `send` whenever a
//! peer asks, so a source dies with the object that created it. A window that copies and then
//! closes would take the selection with it, which a capture tool does every time — its overlay
//! exists exactly long enough to finish one action. The host therefore owns its own data
//! device and queue on the shared connection and serves the payload for the whole process,
//! while paste stays with the focused window (an offer follows focus).
//!
//! The host also reads the selection through the same data-control device: that channel needs
//! no keyboard focus, so a daemon can take what the user copied without stealing the keyboard
//! from the window below. A read is a pipe from the selection owner, drained each turn.

use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, BorrowedFd};
use std::time::{Duration, Instant};

use ui_core::clipboard::ClipboardContent;
use ui_platform::transfer::{Selection, SelectionReader};
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_seat,
};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1, ext_data_control_manager_v1, ext_data_control_offer_v1,
    ext_data_control_source_v1,
};

use crate::clipboard::{Contents, Payload, SelectionSource};
use crate::link::WaylandLink;
use crate::window::WindowError;

/// How long a read waits for the selection owner before giving up.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// A pipe we are reading the selection owner's payload from.
struct ReadJob {
    fd: File,
    mime: String,
    bytes: Vec<u8>,
    deadline: Instant,
}

/// How the host publishes a selection.
///
/// A seat device only takes a selection under a fresh input serial, and a command that never
/// had a surface has none. Data-control exists for exactly that client: it is the compositor's
/// privileged clipboard channel, and a copy through it needs no input at all.
enum Writer {
    Control {
        manager: ext_data_control_manager_v1::ExtDataControlManagerV1,
        device: ext_data_control_device_v1::ExtDataControlDeviceV1,
        source: Option<ext_data_control_source_v1::ExtDataControlSourceV1>,
    },
    Seat {
        manager: wl_data_device_manager::WlDataDeviceManager,
        device: wl_data_device::WlDataDevice,
    },
}

/// State of the host's own queue. Only the write side lives here.
pub struct HostState {
    /// The ordinary seat source; unused while data-control carries the selection.
    seat: SelectionSource,
    /// The data-control payload, served the same way as the seat source's.
    control: Payload,
    /// The selection the compositor offered this device. The host never pastes, so the offer
    /// is only kept alive, replaced, and destroyed; dropping it without handling `data_offer`
    /// is a panic, because that event creates an object.
    offer: Option<wl_data_offer::WlDataOffer>,
    /// The data-control offer and the MIME types it announced; this is what a read asks for.
    control_offer: Option<ext_data_control_offer_v1::ExtDataControlOfferV1>,
    control_mimes: Vec<String>,
    /// A data-control read in flight; the payload lands in [`service`](Self::service).
    read: Option<ReadJob>,
}

impl HostState {
    /// Replace the offer, releasing the one it displaces.
    fn set_offer(&mut self, offer: Option<wl_data_offer::WlDataOffer>) {
        if self.offer.as_ref() != offer.as_ref()
            && let Some(previous) = self.offer.take()
        {
            previous.destroy();
        }
        self.offer = offer;
    }

    fn set_control_offer(
        &mut self,
        offer: Option<ext_data_control_offer_v1::ExtDataControlOfferV1>,
    ) {
        // `data_offer` and `selection` name the same object, and `selection` arrives *after*
        // the offer's MIME events: clearing on every call would wipe the list just read.
        let changed = self.control_offer.as_ref() != offer.as_ref();
        if changed && let Some(previous) = self.control_offer.take() {
            previous.destroy();
        }
        self.control_offer = offer;
        if changed {
            self.control_mimes.clear();
        }
    }

    /// Ask the current data-control offer for `mime`, into a pipe we then read. False when
    /// there is no offer to ask.
    fn read(&mut self, mime: &str, conn: &Connection) -> bool {
        let Some(offer) = self.control_offer.clone() else {
            return false;
        };
        let Ok((read_fd, write_fd)) = rustix::pipe::pipe_with(
            rustix::pipe::PipeFlags::NONBLOCK | rustix::pipe::PipeFlags::CLOEXEC,
        ) else {
            return false;
        };
        offer.receive(mime.to_string(), write_fd.as_fd());
        // Our end of the write side must close, or the reader never sees EOF.
        drop(write_fd);
        self.read = Some(ReadJob {
            fd: File::from(read_fd),
            mime: mime.to_string(),
            bytes: Vec::new(),
            deadline: Instant::now() + READ_TIMEOUT,
        });
        let _ = conn.flush();
        true
    }

    /// Move a data-control read along; the finished payload comes back here.
    fn poll_read(&mut self) -> Option<Selection> {
        let mut done = None;
        if let Some(read) = &mut self.read {
            let mut buf = [0u8; 8192];
            loop {
                match (&read.fd).read(&mut buf) {
                    Ok(0) => {
                        // EOF: the selection owner closed its side, so the payload is complete.
                        done = Some((read.mime.clone(), read.bytes.clone()));
                        break;
                    }
                    Ok(n) => read.bytes.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        // A broken pipe is an empty selection, not a stuck one.
                        done = Some((read.mime.clone(), Vec::new()));
                        break;
                    }
                }
            }
            if done.is_none() && Instant::now() > read.deadline {
                // A peer that never answers must not hold the command hostage.
                done = Some((read.mime.clone(), Vec::new()));
            }
        }
        if done.is_some() {
            self.read = None;
        }
        done.map(|(mime, data)| Selection { mime, data })
    }

    fn read_deadline(&self) -> Option<Instant> {
        self.read.as_ref().map(|read| read.deadline)
    }

    fn read_fds(&self) -> Vec<(BorrowedFd<'_>, bool)> {
        self.read
            .as_ref()
            .map(|read| vec![(read.fd.as_fd(), false)])
            .unwrap_or_default()
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for HostState {
    // `data_offer` is the only event that creates an object; without this the client panics.
    wayland_client::event_created_child!(HostState, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The host only writes; offers and pastes belong to the window that has focus.
        match event {
            wl_data_device::Event::DataOffer { id } => state.set_offer(Some(id)),
            wl_data_device::Event::Selection { id } => state.set_offer(id),
            _ => {}
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for HostState {
    fn event(
        _: &mut Self,
        _: &wl_data_offer::WlDataOffer,
        _: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Mime types and drag actions describe a paste the host will never make.
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for HostState {
    fn event(
        state: &mut Self,
        _: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { fd, .. } => state.seat.serve(fd),
            wl_data_source::Event::Cancelled => state.seat.cancelled(),
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for HostState {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for HostState {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: <wl_data_device_manager::WlDataDeviceManager as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_data_control_manager_v1::ExtDataControlManagerV1, ()> for HostState {
    fn event(
        _: &mut Self,
        _: &ext_data_control_manager_v1::ExtDataControlManagerV1,
        _: <ext_data_control_manager_v1::ExtDataControlManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The manager has no events; it only creates data devices and sources.
    }
}

impl Dispatch<ext_data_control_device_v1::ExtDataControlDeviceV1, ()> for HostState {
    wayland_client::event_created_child!(HostState, ext_data_control_device_v1::ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ext_data_control_offer_v1::ExtDataControlOfferV1, ()),
        ext_data_control_device_v1::EVT_PRIMARY_SELECTION_OPCODE => (ext_data_control_offer_v1::ExtDataControlOfferV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &ext_data_control_device_v1::ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Only the write side matters here; an offer is held so the protocol object stays
        // alive, and `finished` means the seat the device came from is gone.
        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                state.set_control_offer(Some(id));
            }
            ext_data_control_device_v1::Event::Selection { id } => state.set_control_offer(id),
            ext_data_control_device_v1::Event::PrimarySelection { id } => {
                state.set_control_offer(id);
            }
            _ => {}
        }
    }
}

impl Dispatch<ext_data_control_offer_v1::ExtDataControlOfferV1, ()> for HostState {
    fn event(
        state: &mut Self,
        _: &ext_data_control_offer_v1::ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The MIME list is what a read asks for; without it the offer cannot be used.
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.control_mimes.push(mime_type);
        }
    }
}

impl Dispatch<ext_data_control_source_v1::ExtDataControlSourceV1, ()> for HostState {
    fn event(
        state: &mut Self,
        _: &ext_data_control_source_v1::ExtDataControlSourceV1,
        event: ext_data_control_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { fd, .. } => state.control.serve(fd),
            ext_data_control_source_v1::Event::Cancelled => state.control.clear(),
            _ => {}
        }
    }
}

/// The process's clipboard writer, on the link's connection and a queue of its own.
pub struct ClipboardHost {
    conn: Connection,
    queue: EventQueue<HostState>,
    /// Kept alive: the device it created is useless once the seat goes.
    _seat: wl_seat::WlSeat,
    writer: Writer,
    state: HostState,
}

impl ClipboardHost {
    /// Take a data device for the seat. `None` when the compositor has no clipboard.
    pub fn connect(link: &WaylandLink) -> Result<Option<Self>, WindowError> {
        let conn = link.connection().clone();
        let queue = conn.new_event_queue::<HostState>();
        let qh = queue.handle();
        let globals = link.globals();
        let Ok(seat) = globals.bind::<wl_seat::WlSeat, HostState, ()>(&qh, 1..=9, ()) else {
            return Ok(None);
        };
        let writer = match globals
            .bind::<ext_data_control_manager_v1::ExtDataControlManagerV1, HostState, ()>(
                &qh,
                1..=1,
                (),
            ) {
            Ok(manager) => {
                let device = manager.get_data_device(&seat, &qh, ());
                Writer::Control {
                    manager,
                    device,
                    source: None,
                }
            }
            Err(_) => {
                let Ok(manager) = globals
                    .bind::<wl_data_device_manager::WlDataDeviceManager, HostState, ()>(
                        &qh,
                        1..=3,
                        (),
                    )
                else {
                    return Ok(None);
                };
                let device = manager.get_data_device(&seat, &qh, ());
                Writer::Seat { manager, device }
            }
        };
        Ok(Some(Self {
            conn,
            queue,
            _seat: seat,
            writer,
            state: HostState {
                seat: SelectionSource::new(),
                control: Payload::new(),
                offer: None,
                control_offer: None,
                control_mimes: Vec::new(),
                read: None,
            },
        }))
    }

    /// Offer `content` as the selection. `serial` is the caller's latest input, which only the
    /// seat path needs; data-control takes the selection without one.
    pub fn set_selection(&mut self, content: ClipboardContent, serial: u32) -> bool {
        let contents = match content {
            ClipboardContent::Text(text) => Contents::Text(text),
            ClipboardContent::Bytes { mime, data } => Contents::Bytes { mime, data },
        };
        let mimes: Vec<String> = match &contents {
            Contents::Text(_) => crate::clipboard::MIMES
                .iter()
                .map(|m| m.to_string())
                .collect(),
            Contents::Bytes { mime, .. } => vec![mime.clone()],
        };
        let qh = self.queue.handle();
        match &mut self.writer {
            Writer::Control {
                manager,
                device,
                source,
            } => {
                if let Some(old) = source.take() {
                    old.destroy();
                }
                let created = manager.create_data_source(&qh, ());
                for mime in mimes {
                    created.offer(mime);
                }
                device.set_selection(Some(&created));
                self.state.control.set(contents);
                *source = Some(created);
                let _ = self.conn.flush();
                true
            }
            Writer::Seat { manager, device } => self
                .state
                .seat
                .offer(contents, manager, device, serial, &qh, &self.conn),
        }
    }

    /// Move the host's queue and its pipes forward. Never blocks.
    pub fn service(&mut self) -> Result<(), WindowError> {
        // A daemon without a surface has no other queue reading the socket, and the
        // selection's `data_offer`/`offer` events arrive here: read what already arrived
        // before dispatching. Another queue mid-read leaves this one to the next turn.
        if let Some(guard) = self.queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(_)) => {}
                // A protocol error belongs to the object that caused it.
                Err(_) => {}
            }
        }
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|error| WindowError::Protocol(error.to_string()))?;
        self.state.seat.service();
        self.state.control.service();
        Ok(())
    }

    /// The pipes a wait could poll; the app's frame loop calls [`service`](Self::service)
    /// often enough that this is only needed by a loop that sleeps.
    pub fn fds(&self) -> Vec<(BorrowedFd<'_>, bool)> {
        let mut fds = self.state.seat.fds();
        fds.extend(self.state.control.fds());
        fds.extend(self.state.read_fds());
        fds
    }

    /// When the nearest pending read must be given up; a wait must wake for it.
    pub fn deadline(&self) -> Option<Instant> {
        self.state.read_deadline()
    }
}

impl SelectionReader for ClipboardHost {
    fn offers(&self) -> Vec<String> {
        // The data-control offer is the process's view of the selection, no focus needed.
        self.state.control_mimes.clone()
    }

    fn read(&mut self, mime: &str) -> bool {
        self.state.read(mime, &self.conn)
    }

    fn poll(&mut self) -> Option<Selection> {
        self.state.poll_read()
    }
}
