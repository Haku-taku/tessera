//! The Wayland connection: registry, and what this compositor offers.
//!
//! Detection lives here only, so "is this protocol available" has one answer.

use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_compositor, wl_data_device_manager, wl_output, wl_registry, wl_seat, wl_shm,
};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::wp::color_management::v1::client::wp_color_manager_v1::WpColorManagerV1;
pub(crate) use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1 as shape_device;
pub(crate) use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::wp::linux_drm_syncobj::v1::client::wp_linux_drm_syncobj_manager_v1::WpLinuxDrmSyncobjManagerV1;
use wayland_protocols::wp::pointer_warp::v1::client::wp_pointer_warp_v1::WpPointerWarpV1;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::viewporter::client::wp_viewporter;
use wayland_protocols::xdg::shell::client::xdg_wm_base;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;

use crate::window::{WindowError, WindowState};

/// The globals this client binds, with the versions it asked for.
pub struct Capabilities {
    pub compositor: wl_compositor::WlCompositor,
    pub shm: wl_shm::WlShm,
    /// Optional: a layer-only surface needs no window manager, but a normal window does.
    pub wm_base: Option<xdg_wm_base::XdgWmBase>,
    /// Optional: overlay surfaces placed by the compositor (wlroots protocol).
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub seat: Option<wl_seat::WlSeat>,
    /// Optional: seat clipboard and drag-and-drop; absent means neither exists.
    pub data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    pub cursor_shape: Option<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1>,
    pub viewporter: Option<wp_viewporter::WpViewporter>,
    pub fractional: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    pub dmabuf: Option<ZwpLinuxDmabufV1>,
    /// Optional: explicit synchronization; absent means the compositor only does implicit sync.
    pub syncobj: Option<WpLinuxDrmSyncobjManagerV1>,
    pub color_manager: Option<WpColorManagerV1>,
    /// Optional: text input; absent means the app only sees raw key events.
    pub text_input: Option<ZwpTextInputManagerV3>,
    /// Optional: moving the pointer itself, which is how a keyboard moves a cursor.
    pub pointer_warp: Option<WpPointerWarpV1>,
    /// First output; colour information is per-output.
    pub output: Option<wl_output::WlOutput>,
}

/// Open a connection: enumerate globals, bind what we need.
///
/// The caller drives the queue, so it is returned rather than owned; the registry comes
/// back too, because application code binds its own globals from the same list.
pub(crate) fn connect() -> Result<
    (
        Connection,
        EventQueue<WindowState>,
        Capabilities,
        GlobalList,
    ),
    WindowError,
> {
    let conn = Connection::connect_to_env().map_err(|e| WindowError::Connect(e.to_string()))?;
    let (globals, queue) = registry_queue_init::<WindowState>(&conn)
        .map_err(|e| WindowError::Connect(e.to_string()))?;
    let caps = bind(&globals, &queue.handle())?;
    Ok((conn, queue, caps, globals))
}

/// Bind this client's globals into a queue on an existing registry.
///
/// A second window binds its own proxies: a `wl_surface` and its input objects belong to
/// one queue, and sharing them would make two windows dispatch each other's events.
pub(crate) fn bind(
    globals: &GlobalList,
    qh: &QueueHandle<WindowState>,
) -> Result<Capabilities, WindowError> {
    // Hard requirements: without these there is no window at all.
    let compositor = globals
        .bind(&qh, 4..=6, ())
        .map_err(|_| WindowError::MissingGlobal("wl_compositor"))?;
    let shm = globals
        .bind(&qh, 1..=1, ())
        .map_err(|_| WindowError::MissingGlobal("wl_shm"))?;

    // Optional: the window degrades instead of failing.
    let wm_base: Option<xdg_wm_base::XdgWmBase> = globals.bind(qh, 1..=3, ()).ok();
    let layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1> =
        globals.bind(qh, 1..=4, ()).ok();
    let seat: Option<wl_seat::WlSeat> = globals.bind(qh, 1..=9, ()).ok();
    let data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager> =
        globals.bind(qh, 1..=3, ()).ok();
    // Named shapes only; absent means the pointer keeps its default appearance.
    let cursor_shape: Option<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1> =
        globals.bind(qh, 1..=2, ()).ok();
    let viewporter: Option<wp_viewporter::WpViewporter> = globals.bind(qh, 1..=1, ()).ok();
    let fractional: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1> =
        globals.bind(qh, 1..=1, ()).ok();
    // v2 adds create_immed, v3 adds the modifier events; v4+ needs feedback objects.
    let dmabuf: Option<ZwpLinuxDmabufV1> = globals.bind(qh, 2..=3, ()).ok();
    let syncobj: Option<WpLinuxDrmSyncobjManagerV1> = globals.bind(qh, 1..=1, ()).ok();
    let color_manager: Option<WpColorManagerV1> = globals.bind(qh, 1..=3, ()).ok();
    let text_input: Option<ZwpTextInputManagerV3> = globals.bind(qh, 1..=2, ()).ok();
    let pointer_warp: Option<WpPointerWarpV1> = globals.bind(qh, 1..=1, ()).ok();
    let output: Option<wl_output::WlOutput> = globals.bind(qh, 1..=4, ()).ok();

    Ok(Capabilities {
        compositor,
        shm,
        wm_base,
        layer_shell,
        seat,
        data_device_manager,
        cursor_shape,
        viewporter,
        fractional,
        dmabuf,
        syncobj,
        color_manager,
        text_input,
        pointer_warp,
        output,
    })
}

/// The registry only reports additions and removals; nothing is cached.
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WindowState {
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
