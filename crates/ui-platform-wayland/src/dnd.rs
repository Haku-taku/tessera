//! Drag and drop on Wayland: the seat's data device, in both directions.
//!
//! One `wl_data_device` carries the selection and drag-and-drop, and the events say which
//! side an offer belongs to: `selection` names a paste, `enter` names a drag. Both are
//! pull-based, so a drag destination reads the payload down pipes after the drop, and a
//! source answers `send` until the compositor reports the outcome.
//!
//! The engine acts on what it hears from the compositor: an incoming drag arrives as
//! [`DndEvent::Entered`](ui_core::DndEvent::Entered), `Over`, and either `Dropped` or
//! `Left`; a drag this window started ends with `Finished`.

use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_core::Event;
use ui_core::dnd::{
    DndEvent, DragData, DragItem, DropEffect, DropEffects, DropOutcome, Performer,
};
use ui_core::geom::Point;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::clipboard::Outbox;
use crate::window::WindowState;

/// How long a drop's payload may take before it is reported as far as it got.
const READ_TIMEOUT: Duration = Duration::from_secs(2);
/// The most types one drop reads, and the most one of them may carry. A drag is not a
/// trusted channel; a source that announces a hundred types gets the first few.
const MAX_TYPES: usize = 16;
const MAX_BYTES: usize = 64 << 20;

/// A pipe a drop's payload is arriving through.
struct ReadJob {
    fd: File,
    mime: String,
    bytes: Vec<u8>,
    deadline: Instant,
}

/// The payload one of our drag sources serves: each offered type and the pipes peers read it
/// through.
struct DragPayload {
    offers: Vec<(String, Arc<Vec<u8>>)>,
    outbox: Outbox,
}

impl DragPayload {
    fn serve(&mut self, mime: &str, fd: OwnedFd) {
        let bytes = self
            .offers
            .iter()
            .find(|(offered, _)| offered.eq_ignore_ascii_case(mime))
            .or_else(|| self.offers.first())
            .map(|(_, bytes)| Arc::clone(bytes))
            .unwrap_or_else(|| Arc::new(Vec::new()));
        self.outbox.push(fd, bytes);
    }
}

/// A drag this window started, while a target may still ask for its payload.
struct Outgoing {
    source: wl_data_source::WlDataSource,
    payload: DragPayload,
    version: u32,
    /// The action the destination chose, as `dnd_action` reported it.
    action: Option<DndAction>,
    /// Set once the outcome has been reported, so a late event adds nothing.
    settled: bool,
}

/// The seat's drag state: an incoming drag, and one this window started.
#[derive(Default)]
pub(crate) struct Drag {
    /// The offer an incoming drag is over this surface with.
    offer: Option<wl_data_offer::WlDataOffer>,
    mimes: Vec<String>,
    /// The actions the source permits; `None` means it announced none, which reads as
    /// copy, the one action every source honours.
    source_actions: Option<DndAction>,
    /// The effect the engine last published for the pointer.
    effect: Option<DropEffect>,
    pos: Option<Point>,
    serial: u32,
    /// Set by `drop`: the drag object is gone, but its payload is still arriving.
    dropped: bool,
    reads: Vec<ReadJob>,
    items: Vec<DragItem>,
    outgoing: Option<Outgoing>,
}

impl Drag {
    /// Whether the offer bringing MIME events is the incoming drag's.
    pub(crate) fn note_mime(&mut self, offer: &wl_data_offer::WlDataOffer, mime: &str) -> bool {
        if self.offer.as_ref() != Some(offer) {
            return false;
        }
        self.mimes.push(mime.to_owned());
        true
    }

    /// The actions the source permits, when they did not arrive before `enter`.
    pub(crate) fn note_actions(
        &mut self,
        offer: &wl_data_offer::WlDataOffer,
        actions: DndAction,
    ) -> bool {
        if self.offer.as_ref() != Some(offer) {
            return false;
        }
        self.source_actions = Some(actions);
        self.apply_actions();
        true
    }

    /// Publish the effect the engine negotiated for the pointer.
    ///
    /// Wayland has one side choose: the destination sends the preferred action to the
    /// source, and the source reports it back in `dnd_action`. The engine's frame decides
    /// from the modifiers it sees, so the backend follows `drop_effect()`.
    pub(crate) fn set_effect(&mut self, effect: Option<DropEffect>) {
        if self.effect == effect {
            return;
        }
        self.effect = effect;
        self.apply_actions();
    }

    /// Tell the source which action this destination supports and prefers.
    fn apply_actions(&mut self) {
        let Some(offer) = self.offer.as_ref() else {
            return;
        };
        if offer.version() < 3 {
            return;
        }
        let supported = supported_actions(self.source_actions.unwrap_or(DndAction::Copy));
        if supported.is_empty() {
            return;
        }
        let wanted = match self.effect {
            Some(DropEffect::Move) => DndAction::Move,
            _ => DndAction::Copy,
        };
        let preferred = if supported.contains(wanted) {
            wanted
        } else {
            first_action(supported)
        };
        offer.set_actions(supported, preferred);
    }

    /// A drag entered the surface with `offer`, the types announced for it and the actions
    /// the source permits.
    pub(crate) fn enter(
        &mut self,
        offer: wl_data_offer::WlDataOffer,
        mimes: Vec<String>,
        actions: DndAction,
        pos: Point,
        serial: u32,
    ) -> Vec<Event> {
        // A previous drag that never reported its end must not keep its object alive.
        self.abandon();
        self.offer = Some(offer);
        self.mimes = mimes;
        self.source_actions = Some(actions);
        self.effect = None;
        self.pos = Some(pos);
        self.serial = serial;
        let version = self.offer.as_ref().map_or(0, |offer| offer.version());
        if version >= 3 {
            // The mask this destination supports, and the action it prefers.
            self.apply_actions();
        }
        let Some(offer) = self.offer.as_ref() else {
            return Vec::new();
        };
        // Version 3 cancels a drag whose destination accepted no type, taking the source's
        // data with it. The engine's own verdict needs a frame this event precedes, so the
        // first offered type is the optimistic answer — the same one OLE gets on Windows.
        if let Some(mime) = self.mimes.first() {
            offer.accept(serial, Some(mime.clone()));
        }
        vec![Event::Dnd(DndEvent::Entered {
            pos,
            types: self.mimes.clone(),
            effects: effects_of(actions),
        })]
    }

    /// The drag moved within the surface.
    pub(crate) fn motion(&mut self, pos: Point) -> Option<Event> {
        // After a drop the pointer events of the same drag mean nothing; the outcome is
        // already being read.
        if self.dropped {
            return None;
        }
        self.offer.as_ref()?;
        self.pos = Some(pos);
        Some(Event::Dnd(DndEvent::Over { pos }))
    }

    /// The drag left the surface without a drop.
    pub(crate) fn leave(&mut self) -> Vec<Event> {
        if self.dropped || self.offer.is_none() {
            // `leave` also follows a drop; the payload read outlives the drag object.
            return Vec::new();
        }
        self.abandon();
        vec![Event::Dnd(DndEvent::Left)]
    }

    /// The user released the drag over this surface: ask for every announced type.
    pub(crate) fn drop(&mut self, conn: &Connection) -> Vec<Event> {
        if self.dropped {
            return Vec::new();
        }
        self.dropped = true;
        let Some(offer) = self.offer.clone() else {
            // A release with no offer ends the drag exactly as leaving does.
            return vec![Event::Dnd(DndEvent::Left)];
        };
        for mime in self.mimes.iter().take(MAX_TYPES) {
            let Ok((read_fd, write_fd)) = rustix::pipe::pipe_with(
                rustix::pipe::PipeFlags::NONBLOCK | rustix::pipe::PipeFlags::CLOEXEC,
            ) else {
                break;
            };
            offer.receive(mime.clone(), write_fd.as_fd());
            // Our end of the write side must close, or the reader never sees EOF.
            drop(write_fd);
            self.reads.push(ReadJob {
                fd: File::from(read_fd),
                mime: mime.clone(),
                bytes: Vec::new(),
                deadline: Instant::now() + READ_TIMEOUT,
            });
        }
        let _ = conn.flush();
        if self.reads.is_empty() {
            return self.finish_drop();
        }
        Vec::new()
    }

    /// Move every pipe forward; a finished drop becomes `Dropped` here.
    pub(crate) fn service(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let mut index = 0;
        while index < self.reads.len() {
            let read = &mut self.reads[index];
            let mut done = false;
            let mut oversized = false;
            let mut buf = [0u8; 16384];
            loop {
                match (&read.fd).read(&mut buf) {
                    Ok(0) => {
                        // EOF: the source closed its side, so this type is complete.
                        done = true;
                        break;
                    }
                    Ok(n) => read.bytes.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    // A broken pipe is the end of what arrived, not a stuck transfer.
                    Err(_) => {
                        done = true;
                        break;
                    }
                }
                if read.bytes.len() > MAX_BYTES {
                    oversized = true;
                    done = true;
                    break;
                }
            }
            if !done && Instant::now() > read.deadline {
                done = true;
            }
            if done {
                let read = self.reads.remove(index);
                if !oversized && !read.bytes.is_empty() {
                    self.items.push(DragItem {
                        mime: read.mime,
                        data: read.bytes,
                    });
                }
            } else {
                index += 1;
            }
        }
        if self.dropped && self.reads.is_empty() {
            events.extend(self.finish_drop());
        }
        // Our source may still be writing a payload a peer asked for.
        if let Some(outgoing) = self.outgoing.as_mut() {
            outgoing.payload.outbox.service();
        }
        events
    }

    /// Report the drop with everything read so far, and let the offer go.
    fn finish_drop(&mut self) -> Vec<Event> {
        // The destination's preferred action must be on the offer before `finish`, or the
        // source hears the action it last saw rather than the one the drop meant.
        self.apply_actions();
        let Some(offer) = self.offer.take() else {
            return Vec::new();
        };
        // Version 3: `finish` is what tells the source its data was taken, and it must come
        // after the transfer. No other request may follow it but `destroy`.
        if offer.version() >= 3 {
            offer.finish();
        }
        offer.destroy();
        let data = DragData::new(std::mem::take(&mut self.items));
        let pos = self.pos.take().unwrap_or_else(|| Point::new(0.0, 0.0));
        self.mimes.clear();
        self.source_actions = None;
        self.effect = None;
        self.dropped = false;
        vec![Event::Dnd(DndEvent::Dropped {
            pos,
            data,
            // Wayland's drag gesture is the primary button's; there is no right drag in
            // the protocol, so the target never sees a decision the button would imply.
            button: ui_core::PointerButton::Primary,
            // A move here is finished by the source after `dnd_finished`, so the
            // destination copies and leaves the originals in place.
            performer: Performer::Source,
        })]
    }

    /// Drop an incoming drag that ended without a report.
    fn abandon(&mut self) {
        if let Some(offer) = self.offer.take() {
            offer.destroy();
        }
        self.mimes.clear();
        self.source_actions = None;
        self.effect = None;
        self.pos = None;
        self.serial = 0;
        self.dropped = false;
        self.reads.clear();
        self.items.clear();
    }

    /// Whether `source` is the one this window started.
    pub(crate) fn owns_source(&self, source: &wl_data_source::WlDataSource) -> bool {
        self.outgoing
            .as_ref()
            .is_some_and(|outgoing| &outgoing.source == source)
    }

    /// A peer asked our drag source for `mime`; the older protocol versions have no final
    /// event, so a `send` is the only sign a target took the payload.
    pub(crate) fn serve(&mut self, mime: &str, fd: OwnedFd) -> Option<Event> {
        let outgoing = self.outgoing.as_mut()?;
        outgoing.payload.serve(mime, fd);
        if outgoing.version < 3 && !outgoing.settled {
            outgoing.settled = true;
            return Some(Event::Dnd(DndEvent::Finished {
                outcome: Some(DropOutcome {
                    action: DropEffect::Copy,
                    performed_by: Performer::Source,
                }),
            }));
        }
        None
    }

    /// The compositor reports the action the destination chose.
    pub(crate) fn note_action(
        &mut self,
        source: &wl_data_source::WlDataSource,
        action: DndAction,
    ) -> bool {
        let Some(outgoing) = self.outgoing.as_mut() else {
            return false;
        };
        if &outgoing.source != source {
            return false;
        }
        outgoing.action = Some(action);
        true
    }

    /// The compositor reports how our drag ended.
    pub(crate) fn settle(&mut self, dropped: bool) -> Option<Event> {
        let outgoing = self.outgoing.as_mut()?;
        if outgoing.settled {
            return None;
        }
        outgoing.settled = true;
        let outcome = dropped.then(|| DropOutcome {
            // `ask` is a request for a decision this source cannot make after the
            // compositor has already delivered the drop; copying keeps the data and
            // leaves deletion to the user, which is the safe reading.
            action: effect_of(outgoing.action.unwrap_or(DndAction::Copy)),
            // Wayland gives the move to the source: the destination copied, and the
            // originals stay here until this side removes them.
            performed_by: Performer::Source,
        });
        Some(Event::Dnd(DndEvent::Finished { outcome }))
    }

    /// Retire a settled source once the writes it was asked for have drained.
    pub(crate) fn retire(&mut self) {
        let Some(outgoing) = self.outgoing.as_ref() else {
            return;
        };
        if outgoing.settled && outgoing.payload.outbox.idle() {
            if let Some(outgoing) = self.outgoing.take() {
                outgoing.source.destroy();
            }
        }
    }

    /// Start a drag from this window, offering `data`'s types.
    pub(crate) fn begin(
        &mut self,
        data: &DragData,
        manager: &wl_data_device_manager::WlDataDeviceManager,
        device: &wl_data_device::WlDataDevice,
        surface: &wl_surface::WlSurface,
        serial: u32,
        qh: &QueueHandle<WindowState>,
        conn: &Connection,
    ) -> bool {
        let offers = offered(data);
        let actions = wayland_actions(data.effects);
        if offers.is_empty() || actions.is_empty() || serial == 0 {
            return false;
        }
        // The compositor tracks one drag at a time; a second gesture replaces the first.
        if let Some(previous) = self.outgoing.take() {
            previous.source.destroy();
        }
        let source = manager.create_data_source(qh, ());
        let version = source.version();
        // Every action the payload means. Wayland has no link action, so a link drag
        // copies where it lands; `ask` is not offered because this source has no way to
        // put a question in front of the user after the compositor has the gesture.
        if version >= 3 {
            source.set_actions(actions);
        }
        for (mime, _) in &offers {
            source.offer(mime.clone());
        }
        device.start_drag(Some(&source), surface, None, serial);
        self.outgoing = Some(Outgoing {
            source,
            payload: DragPayload {
                offers,
                outbox: Outbox::default(),
            },
            version,
            action: None,
            settled: false,
        });
        let _ = conn.flush();
        true
    }

    /// The fds a wait should poll: payloads still arriving, and writes still owed.
    pub(crate) fn fds(&self) -> Vec<(BorrowedFd<'_>, bool)> {
        let mut out: Vec<(BorrowedFd<'_>, bool)> = self
            .reads
            .iter()
            .map(|read| (read.fd.as_fd(), false))
            .collect();
        if let Some(outgoing) = &self.outgoing {
            out.extend(outgoing.payload.outbox.fds());
        }
        out
    }

    /// How long until the nearest deadline, so a wait never sleeps past it.
    pub(crate) fn timeout(&self) -> Option<Duration> {
        let read = self
            .reads
            .iter()
            .map(|read| read.deadline)
            .min()
            .map(|at| at.saturating_duration_since(Instant::now()));
        let write = self
            .outgoing
            .as_ref()
            .and_then(|outgoing| outgoing.payload.outbox.timeout());
        match (read, write) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

/// The types a payload offers, best first, with the spellings a Wayland peer may ask for.
fn offered(data: &DragData) -> Vec<(String, Arc<Vec<u8>>)> {    let mut offers: Vec<(String, Arc<Vec<u8>>)> = Vec::new();
    for item in &data.items {
        let bytes = Arc::new(item.data.clone());
        // A text payload is asked for under any of the three names; they all read the
        // same bytes, and a peer that speaks only one still takes it.
        let aliases: &[&str] = if item.mime.eq_ignore_ascii_case("text/plain") {
            &["text/plain;charset=utf-8", "UTF8_STRING"]
        } else {
            &[]
        };
        for mime in std::iter::once(item.mime.as_str()).chain(aliases.iter().copied()) {
            if offers
                .iter()
                .any(|(known, _)| known.eq_ignore_ascii_case(mime))
            {
                continue;
            }
            offers.push((mime.to_owned(), Arc::clone(&bytes)));
        }
    }
    offers
}

/// The `DndAction`s a payload's permitted effects map to.
///
/// Wayland has copy and move; a link is a copy on this protocol. An empty set refuses the
/// drag instead of starting one no destination can honour.
fn wayland_actions(effects: DropEffects) -> DndAction {
    let mut actions = DndAction::empty();
    if effects.contains(DropEffect::Copy) || effects.contains(DropEffect::Link) {
        actions |= DndAction::Copy;
    }
    if effects.contains(DropEffect::Move) {
        actions |= DndAction::Move;
    }
    actions
}

/// The actions this destination supports out of what a source permits.
fn supported_actions(source: DndAction) -> DndAction {
    if source.is_empty() {
        // A version-2 source announces nothing; copying is the action every source has.
        return DndAction::Copy;
    }
    let mut actions = DndAction::empty();
    if source.contains(DndAction::Copy) {
        actions |= DndAction::Copy;
    }
    if source.contains(DndAction::Move) {
        actions |= DndAction::Move;
    }
    actions
}

/// One action out of a set, for a preferred action a mask cannot express.
fn first_action(actions: DndAction) -> DndAction {
    if actions.contains(DndAction::Copy) {
        DndAction::Copy
    } else {
        DndAction::Move
    }
}

/// The effects a source's action mask permits.
fn effects_of(actions: DndAction) -> DropEffects {
    let mut effects = DropEffects::NONE;
    if actions.contains(DndAction::Copy) {
        effects |= DropEffects::COPY;
    }
    if actions.contains(DndAction::Move) {
        effects |= DropEffects::MOVE;
    }
    if effects.is_empty() {
        DropEffects::COPY
    } else {
        effects
    }
}

/// The effect an action means; `ask` and anything unknown read as a copy.
fn effect_of(action: DndAction) -> DropEffect {
    if action.contains(DndAction::Move) {
        DropEffect::Move
    } else {
        DropEffect::Copy
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for WindowState {
    // `data_offer` is the only event that creates an object; without this the client panics.
    wayland_client::event_created_child!(WindowState, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        conn: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut events = Vec::new();
        match event {
            wl_data_device::Event::DataOffer { id } => state.clipboard.set_pending(id),
            wl_data_device::Event::Selection { id } => state.clipboard.set_selection_offer(id),
            wl_data_device::Event::Enter {
                serial, x, y, id, ..
            } => {
                let Some(id) = id else {
                    return;
                };
                // The types arrived between `data_offer` and this event.
                let (offer, mimes, actions) = state.clipboard.take_pending(&id);
                let pos = Point::new(x as f32, y as f32);
                events.extend(state.dnd.enter(offer, mimes, actions, pos, serial));
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                let pos = Point::new(x as f32, y as f32);
                events.extend(state.dnd.motion(pos));
            }
            wl_data_device::Event::Drop => events.extend(state.dnd.drop(conn)),
            wl_data_device::Event::Leave => events.extend(state.dnd.leave()),
            _ => {}
        }
        for event in events {
            state.push_event(event);
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for WindowState {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            // The drag state sees the offer only once `enter` names it; until then the
            // clipboard holds the announcement.
            if !state.dnd.note_mime(offer, &mime_type) {
                state.clipboard.note_mime(offer, mime_type);
            }
        } else if let wl_data_offer::Event::SourceActions { source_actions } = event
            && let Ok(actions) = source_actions.into_result()
            && !state.dnd.note_actions(offer, actions)
        {
            state.clipboard.note_actions(offer, actions);
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for WindowState {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut events = Vec::new();
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                if state.dnd.owns_source(source) {
                    events.extend(state.dnd.serve(&mime_type, fd));
                } else {
                    state.clipboard.serve_source(&mime_type, fd);
                }
            }
            wl_data_source::Event::Cancelled => {
                if state.dnd.owns_source(source) {
                    events.extend(state.dnd.settle(false));
                } else {
                    state.clipboard.cancel_source();
                }
            }
            // Version 3: the action the destination chose for this session.
            wl_data_source::Event::Action { dnd_action } => {
                if state.dnd.owns_source(source)
                    && let Ok(action) = dnd_action.into_result()
                {
                    state.dnd.note_action(source, action);
                }
            }
            // Version 3: the drop destination finished reading, so the drag is over.
            wl_data_source::Event::DndFinished => {
                if state.dnd.owns_source(source) {
                    events.extend(state.dnd.settle(true));
                }
            }
            _ => {}
        }
        for event in events {
            state.push_event(event);
        }
    }
}
