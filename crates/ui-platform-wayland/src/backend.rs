//! The platform half of the loop: one connection, its surfaces, and the clipboard process-wide.
//!
//! `Host` decides when to wake and what to step; this backend owns the socket, polls it, and
//! turns a portable `ClipboardCopy` into the seat's selection.

use std::cell::RefCell;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ui_platform::clipboard::{ClipboardCopy, ClipboardKind};
use ui_platform::host::{Backend, OpenOptions, SurfaceHandle};
use ui_platform::hotkeys::{GlobalHotkeys, NoGlobalHotkeys};
use ui_platform::ipc::Channel;
use ui_platform::output::OutputInfo;
use ui_platform::session::SessionOptions;
use ui_platform::transfer::SelectionReader;
use ui_platform::view::{App, AppLoop};
use ui_platform::window::SurfaceSpec;

use crate::clipboard_host::ClipboardHost;
use crate::ipc::{ServerState, UnixChannel};
use crate::link::WaylandLink;
use crate::session::Session;
use crate::window::WindowError;

/// Surfaces and services on one compositor connection.
pub struct WaylandBackend {
    link: WaylandLink,
    /// Absent when the compositor has no data-device manager.
    clipboard: Option<ClipboardHost>,
    /// The daemon's local channel, when one was opened.
    channel: Option<Rc<RefCell<ServerState>>>,
    cpu_presenter: bool,
    /// Opened on the first request: a command that never binds a chord should not open a bus
    /// session or start a reader thread.
    hotkeys: Option<Box<dyn GlobalHotkeys>>,
    /// Whether the one attempt to open them has happened.
    hotkeys_opened: bool,
    /// The counter any worker bumps to interrupt the wait: the portal reader for a
    /// hotkey, the job worker for a result. One descriptor serves both.
    wake: Option<Arc<OwnedFd>>,
}

/// Tells the loop that work finished elsewhere.
struct EventfdWaker(Arc<OwnedFd>);

impl ui_platform::jobs::Waker for EventfdWaker {
    fn wake(&self) {
        // A counter already above zero is still a wake; a failed write has nothing to fix.
        let _ = rustix::io::write(&*self.0, &1u64.to_ne_bytes());
    }
}

impl WaylandBackend {
    /// Connect to the compositor, taking the clipboard for the process.
    pub fn connect() -> Result<Self, WindowError> {
        let link = WaylandLink::connect()?;
        let clipboard = ClipboardHost::connect(&link)?;
        // Without it jobs still run; the loop hears about them on its next turn, and
        // hotkeys fall back to NoGlobalHotkeys.
        let wake = rustix::event::eventfd(
            0,
            rustix::event::EventfdFlags::NONBLOCK | rustix::event::EventfdFlags::CLOEXEC,
        )
        .ok()
        .map(Arc::new);
        Ok(Self {
            link,
            clipboard,
            channel: None,
            cpu_presenter: false,
            hotkeys: None,
            hotkeys_opened: false,
            wake,
        })
    }

    /// Present every surface through the CPU rasterizer (no GPU needed).
    pub fn cpu_presenter(mut self, on: bool) -> Self {
        self.cpu_presenter = on;
        self
    }

    /// The connection, for platform-bound application code that binds its own protocols.
    pub fn link(&self) -> &WaylandLink {
        &self.link
    }

    pub fn has_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }

    /// Open this platform's hotkey transport. The platform was fixed when this crate was
    /// built; which transport it has — the portal today, a compositor's own interface later —
    /// is decided here, once, because only the running desktop knows.
    fn open_hotkeys(&mut self) {
        self.hotkeys_opened = true;
        #[cfg(feature = "global-hotkeys")]
        {
            let Some(wake) = self.wake.clone() else {
                self.hotkeys = Some(Box::new(NoGlobalHotkeys));
                return;
            };
            self.hotkeys = Some(match crate::PortalHotkeys::with_wake(wake) {
                Ok(hotkeys) => Box::new(hotkeys),
                // No portal, or a desktop that refuses it: the app gets `Unsupported` from
                // every registration instead of a silently missing feature.
                Err(_) => Box::new(NoGlobalHotkeys),
            });
        }
        #[cfg(not(feature = "global-hotkeys"))]
        {
            self.hotkeys = Some(Box::new(NoGlobalHotkeys));
        }
    }
}

impl Backend for WaylandBackend {
    fn open(
        &mut self,
        spec: SurfaceSpec,
        app: Box<dyn App>,
        options: OpenOptions,
    ) -> Result<Box<dyn SurfaceHandle>, String> {
        let mut lp = AppLoop::with_stack(app, &[]);
        for image in options.images {
            lp.register_image(image);
        }
        for (chord, id) in options.shortcuts {
            lp.bind_shortcut(chord, id);
        }
        let session = Session::open_on(
            &self.link,
            lp,
            SessionOptions {
                surface: spec,
                uncapped: options.uncapped,
                cpu_presenter: self.cpu_presenter,
            },
        )?;
        Ok(Box::new(session))
    }

    fn set_clipboard_copy(&mut self, copy: ClipboardCopy) -> bool {
        // There is no primary-selection protocol here; the engine's copies are the clipboard.
        if copy.kind != ClipboardKind::Selection {
            return false;
        }
        let Some(host) = self.clipboard.as_mut() else {
            return false;
        };
        host.set_selection(copy.content, copy.serial)
    }

    fn serve_clipboard(&mut self) -> Result<(), String> {
        let Some(host) = self.clipboard.as_mut() else {
            return Ok(());
        };
        host.service()
            .map_err(|error| format!("clipboard: {error}"))
    }

    fn selection_reader(&mut self) -> Option<&mut (dyn SelectionReader + 'static)> {
        self.clipboard.as_mut().map(|host| host as _)
    }

    fn next_deadline(&self) -> Option<std::time::Instant> {
        self.clipboard.as_ref().and_then(|host| host.deadline())
    }

    fn outputs(&mut self) -> Vec<OutputInfo> {
        self.link.outputs()
    }

    fn waker(&mut self) -> Option<Arc<dyn ui_platform::jobs::Waker>> {
        self.wake
            .as_ref()
            .map(|fd| Arc::new(EventfdWaker(Arc::clone(fd))) as Arc<dyn ui_platform::jobs::Waker>)
    }

    fn listen(&mut self, address: &str) -> Result<Box<dyn Channel>, String> {
        let state = crate::ipc::open(address)?;
        self.channel = Some(Rc::clone(&state));
        Ok(Box::new(UnixChannel::new(state)))
    }

    fn hotkeys(&mut self) -> Option<&mut (dyn GlobalHotkeys + 'static)> {
        if !self.hotkeys_opened {
            self.open_hotkeys();
        }
        self.hotkeys.as_deref_mut()
    }

    fn wait(&mut self, timeout: Option<Duration>) -> Result<(), String> {
        let conn = self.link.connection();
        // Only reads wake the connection: a socket that becomes writable with nothing queued
        // would wake the loop for no event.
        let mut fds = vec![rustix::event::PollFd::from_borrowed_fd(
            conn.as_fd(),
            rustix::event::PollFlags::IN,
        )];
        if let Some(host) = &self.clipboard {
            for (fd, write) in host.fds() {
                let want = if write {
                    rustix::event::PollFlags::OUT
                } else {
                    rustix::event::PollFlags::IN
                };
                fds.push(rustix::event::PollFd::from_borrowed_fd(fd, want));
            }
        }
        // The daemon's command channel joins the same wait: a client's request must wake the
        // loop exactly as input does.
        let channel = self.channel.as_ref().map(|state| state.borrow());
        let mut channel_fds: Vec<BorrowedFd<'_>> = Vec::new();
        if let Some(state) = &channel {
            state.fds(&mut channel_fds);
        }
        for fd in &channel_fds {
            fds.push(rustix::event::PollFd::from_borrowed_fd(
                *fd,
                rustix::event::PollFlags::IN,
            ));
        }
        // A hotkey or a finished job arrives on another thread, not this connection: its
        // counter is the only thing that can wake this wait for one.
        if let Some(wake) = &self.wake {
            fds.push(rustix::event::PollFd::from_borrowed_fd(
                wake.as_fd(),
                rustix::event::PollFlags::IN,
            ));
        }
        let deadline = timeout.map(|t| rustix::event::Timespec {
            tv_sec: t.as_secs() as i64,
            tv_nsec: t.subsec_nanos() as i64,
        });
        match rustix::event::poll(&mut fds, deadline.as_ref()) {
            Ok(_) => {}
            // A signal is a wake, not a failure: the loop simply takes its turn.
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(format!("poll: {error}")),
        }
        // Clearing the counter is the whole job: the activations themselves are in the
        // portal's mailbox and the results in the job slots, both drained elsewhere.
        if let Some(wake) = &self.wake {
            let mut count = [0u8; 8];
            let _ = rustix::io::read(&**wake, &mut count);
        }
        // The bytes stay in the socket: each surface's step reads and dispatches its own
        // queue, so one wait serves every surface without a second reader.
        Ok(())
    }

    fn flush(&mut self) {
        let _ = self.link.connection().flush();
    }
}
