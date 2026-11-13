//! The compositor connection, lent to platform-bound application code.
//!
//! An application that speaks protocols the engine does not — screen capture, layer
//! placement, input injection — binds them on this same connection, in a queue of its
//! own. One connection is one display fd: the window's single wait wakes for both, and
//! ordering against the compositor (`wl_display.sync`, serials, dma-buf transfer)
//! stays global instead of being split across sockets that know nothing of each other.
//!
//! The window keeps owning its queue, its proxies and its bindings. What crosses the
//! seam is the connection and the registry, never a queue the window dispatches.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ui_platform::OutputInfo;
use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};

use crate::window::{WaylandWindow, WindowError};

/// State for the queue the registry snapshot is taken on.
///
/// Global add/remove events are not tracked: the list this crate binds from is the
/// snapshot `registry_queue_init` returns, and a global that appears later is not bound.
struct RegistryState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for RegistryState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// A live window's connection and registry.
///
/// Cloning is cheap and shares the same socket; the window must outlive every queue
/// created from it, because the window is what reads that socket.
#[derive(Clone)]
pub struct WaylandLink {
    connection: Connection,
    globals: Arc<GlobalList>,
    outputs: Rc<RefCell<Outputs>>,
}

/// The output tracker: its own registry, so additions and removals reach one place.
///
/// `GlobalList` is a snapshot bound for one version; a monitor plugged in after the app
/// started never reaches it, and a status answer or a pin placement must see it.
struct Outputs {
    queue: EventQueue<OutputState>,
    state: OutputState,
    /// Kept alive: dropping the proxy stops the events.
    _registry: wl_registry::WlRegistry,
}

#[derive(Default)]
struct OutputState {
    outputs: Vec<OutputEntry>,
}

struct OutputEntry {
    /// The registry's name for this global, which is also the udata of its proxy.
    global: u32,
    name: String,
    description: String,
    /// Top-left in the compositor's layout.
    x: i32,
    y: i32,
    /// The current mode's physical size and refresh, in millihertz.
    mode: Option<(u32, u32, u32)>,
    scale: i32,
    /// The wire value of `wl_output.transform`: 1, 3, 5 and 7 are quarter turns.
    transform: u32,
}

impl Default for OutputEntry {
    fn default() -> Self {
        Self {
            global: 0,
            name: String::new(),
            description: String::new(),
            x: 0,
            y: 0,
            mode: None,
            scale: 1,
            transform: 0,
        }
    }
}

impl Outputs {
    /// A tracker on its own queue; the snapshot arrives with the next read.
    fn open(connection: &Connection) -> Self {
        let queue = connection.new_event_queue::<OutputState>();
        let registry = connection.display().get_registry(&queue.handle(), ());
        Self {
            queue,
            state: OutputState::default(),
            _registry: registry,
        }
    }
}

impl OutputState {
    fn list(&self) -> Vec<OutputInfo> {
        self.outputs
            .iter()
            .map(|entry| {
                let (width, height, refresh) = entry.mode.unwrap_or((0, 0, 0));
                let quarter = matches!(entry.transform, 1 | 3 | 5 | 7);
                let (width, height) = if quarter {
                    (height, width)
                } else {
                    (width, height)
                };
                let scale = entry.scale.max(1) as f32;
                OutputInfo {
                    name: entry.name.clone(),
                    description: entry.description.clone(),
                    x: entry.x,
                    y: entry.y,
                    width: (width as f32 / scale).round() as u32,
                    height: (height as f32 / scale).round() as u32,
                    scale,
                    refresh_mhz: refresh,
                    rotated: quarter,
                }
            })
            .collect()
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for OutputState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_output" => {
                state.outputs.push(OutputEntry {
                    global: name,
                    ..OutputEntry::default()
                });
                // A monitor bound after the fact still answers with its full state.
                registry.bind::<WlOutput, _, _>(name, version.min(4), queue, name);
            }
            wl_registry::Event::GlobalRemove { name } => {
                state.outputs.retain(|entry| entry.global != name);
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, u32> for OutputState {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(entry) = state
            .outputs
            .iter_mut()
            .find(|entry| entry.global == *global)
        else {
            return;
        };
        match event {
            wl_output::Event::Geometry {
                x, y, transform, ..
            } => {
                entry.x = x;
                entry.y = y;
                entry.transform = transform.into_result().map(|t| t as u32).unwrap_or(0);
            }
            // Only the mode in use describes the output's size; others are alternatives.
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => {
                let current = flags
                    .into_result()
                    .is_ok_and(|flags| flags.contains(wl_output::Mode::Current));
                if current {
                    entry.mode = Some((
                        width.max(0) as u32,
                        height.max(0) as u32,
                        refresh.max(0) as u32,
                    ));
                }
            }
            wl_output::Event::Scale { factor } => entry.scale = factor.max(1),
            wl_output::Event::Name { name } => entry.name = name,
            wl_output::Event::Description { description } => entry.description = description,
            _ => {}
        }
    }
}

impl WaylandLink {
    pub(crate) fn new(connection: Connection, globals: Arc<GlobalList>) -> Self {
        let outputs = Outputs::open(&connection);
        Self {
            connection,
            globals,
            outputs: Rc::new(RefCell::new(outputs)),
        }
    }

    /// Open a connection with no surface at all.
    ///
    /// A capture tool must see the screen before it shows anything of its own; opening a
    /// window first would put the tool inside its own screenshot.
    pub fn connect() -> Result<Self, WindowError> {
        let connection =
            Connection::connect_to_env().map_err(|e| WindowError::Connect(e.to_string()))?;
        // The queue is only needed for the initial snapshot; the registry proxy keeps the
        // snapshot's data alive, and later add/remove events are out of this contract.
        let (globals, _snapshot) = registry_queue_init::<RegistryState>(&connection)
            .map_err(|e| WindowError::Connect(e.to_string()))?;
        // A second registry, on its own queue: it receives the snapshot now and every
        // addition or removal afterwards.
        let mut outputs = Outputs::open(&connection);
        // The first roundtrip announces the globals, and the bindings it makes answer on the
        // next one: the outputs exist only once both have run.
        for _ in 0..2 {
            outputs
                .queue
                .roundtrip(&mut outputs.state)
                .map_err(|e| WindowError::Connect(e.to_string()))?;
        }
        Ok(Self {
            connection,
            globals: Arc::new(globals),
            outputs: Rc::new(RefCell::new(outputs)),
        })
    }

    /// The connection itself, for calls that take it (`wl_surface` proxies, flush).
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// The registry, for binding globals this window does not bind.
    ///
    /// Binding against a [`QueueHandle`] of the caller's own queue keeps the resulting
    /// proxies on that queue: their events never reach the window's state.
    ///
    /// [`QueueHandle`]: wayland_client::QueueHandle
    pub fn globals(&self) -> &GlobalList {
        &self.globals
    }

    pub(crate) fn globals_shared(&self) -> Arc<GlobalList> {
        Arc::clone(&self.globals)
    }

    /// Open a surface on this connection; its role decides whether it is a window or a layer.
    ///
    /// Every surface keeps its own queue and its own proxies, so each one dispatches only
    /// its own events; the connection is what they share.
    pub fn surface(
        &self,
        spec: ui_platform::window::SurfaceSpec,
    ) -> Result<crate::WaylandWindow, WindowError> {
        WaylandWindow::create(spec, Some(self))
    }

    /// The outputs as they are now, including ones that appeared after the link opened.
    pub fn outputs(&self) -> Vec<OutputInfo> {
        let mut outputs = self.outputs.borrow_mut();
        let Outputs { queue, state, .. } = &mut *outputs;
        // Read what arrived: a monitor is plugged in or unplugged without any surface.
        if let Some(guard) = queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                // Nothing to read yet, or a transient error the next call retries.
                Err(wayland_client::backend::WaylandError::Io(_)) => {}
                // A protocol error belongs to the surface that owns the object; the refresh
                // reports what it has and tries again on the next turn.
                Err(_) => {}
            }
        }
        let _ = queue.dispatch_pending(state);
        state.list()
    }

    /// A queue for the caller's own state.
    ///
    /// The caller must dispatch it after each window turn: the window reads the socket,
    /// so whatever arrived for this queue is sitting pending until it does. Dropping the
    /// queue drops its objects' events, so keep it alive as long as the proxies.
    pub fn new_queue<State: 'static>(&self) -> EventQueue<State> {
        self.connection.new_event_queue()
    }
}
