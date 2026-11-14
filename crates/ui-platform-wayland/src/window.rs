//! The XDG toplevel window.

use std::cell::Cell;
use std::rc::Rc;
use std::collections::HashMap;
use std::os::unix::io::AsFd;
use std::sync::Arc;

/// Startup chatter about scales and colour management, only under `WAYLAND_TRACE=1`.
fn trace(args: std::fmt::Arguments<'_>) {
    if std::env::var_os("WAYLAND_TRACE").is_some() {
        eprintln!("[wayland] {args}");
    }
}

use crate::conn::shape_device;
use crate::link::WaylandLink;
use ui_core::CursorShape;
use ui_core::Damage;
use ui_core::geom::Insets;
use ui_paint::Pixels;
use ui_platform::present::{PresentTarget, SurfaceKind};
use ui_platform::window::{
    ExclusiveZone, KeyboardMode, Layer, LayerSpec, SurfaceRole, SurfaceSpec,
};
use wayland_client::globals::GlobalList;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_region, wl_seat,
    wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::{
    Event as ParamsEvent, ZwpLinuxBufferParamsV1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::{
    Event as DmaBufEvent, ZwpLinuxDmabufV1,
};
use wayland_protocols::wp::pointer_warp::v1::client::wp_pointer_warp_v1::WpPointerWarpV1;
use wayland_protocols::wp::linux_drm_syncobj::v1::client::{
    wp_linux_drm_syncobj_manager_v1::WpLinuxDrmSyncobjManagerV1,
    wp_linux_drm_syncobj_surface_v1::WpLinuxDrmSyncobjSurfaceV1,
    wp_linux_drm_syncobj_timeline_v1::WpLinuxDrmSyncobjTimelineV1,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    self, Event as TextInputEvent, ZwpTextInputV3,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::dmabuf::{self, DmaBufBuffer, DmaBufError, DmaBufFormats, DmaBufImage};
use crate::input::InputState;
use crate::shm::ShmBuffer;

/// Why window creation failed.
#[derive(Debug)]
pub enum WindowError {
    /// No Wayland session.
    Connect(String),
    /// A required global is missing.
    MissingGlobal(&'static str),
    /// Buffer allocation failed.
    Buffer,
    /// Protocol error while round-tripping.
    Protocol(String),
    /// The dma-buf path could not present this frame.
    DmaBuf(DmaBufError),
}

impl std::fmt::Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowError::Connect(e) => write!(f, "wayland connect failed: {e}"),
            WindowError::MissingGlobal(g) => write!(f, "missing global: {g}"),
            WindowError::Buffer => write!(f, "could not allocate a shm buffer"),
            WindowError::Protocol(e) => write!(f, "protocol error: {e}"),
            WindowError::DmaBuf(e) => write!(f, "dma-buf import failed: {e}"),
        }
    }
}

impl std::error::Error for WindowError {}

/// What a surface must encode to once it takes its HDR output's own description.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HdrSurface {
    /// The display's SDR white in nits: the luminance linear 1.0 is mapped to.
    pub reference_nits: u32,
    /// The display's black level in nits, converted from the protocol's 1/10000 units.
    pub min_nits: f32,
    /// The display's peak luminance in nits: what content above it must be mapped into.
    pub max_nits: f32,
    /// Buffer layout that carries this peak without banding in the darks.
    pub format: ui_core::colour::BufferFormat,
    /// sRGB to output-gamut matrix, row-major.
    pub gamut: [[f32; 3]; 3],
}

/// Device pixels for a logical size: round to nearest and never ask for zero.
pub(crate) fn device_size(logical: (u32, u32), scale: f32) -> (u32, u32) {
    let px = |v: u32| (v as f32 * scale).round().max(1.0) as u32;
    (px(logical.0), px(logical.1))
}

/// State the compositor and input drive.
pub(crate) struct WindowState {
    /// Set when the compositor rejects an image description.
    pub(crate) colour_failed: Option<u32>,
    /// The output the surface sits on, from `wl_surface.enter` and `leave`.
    pub(crate) entered_output: Option<wl_output::WlOutput>,
    /// Set when an input to the surface's colour changed; the presenter resolves it.
    pub(crate) colour_dirty: bool,
    /// (min, max, reference) luminance from the compositor's preferred description.
    pub(crate) preferred_luminances: Option<(u32, u32, u32)>,
    /// Transfer function and primaries code points from the same description.
    pub(crate) preferred_tf: Option<u32>,
    pub(crate) preferred_primaries: Option<u32>,
    /// Exact primaries from the same description, when it carries coordinates.
    pub(crate) preferred_chromaticities: Option<ui_core::colour::Chromaticities>,
    /// Only client-created descriptions accept `get_information` on this compositor.
    pub(crate) allow_info: bool,
    /// Bitmask of `wp_color_manager_v1.feature` values the compositor advertised.
    pub(crate) colour_features: u32,
    pub(crate) colour_features_known: bool,
    shm: Option<wl_shm::WlShm>,
    base: Option<xdg_wm_base::XdgWmBase>,
    configured: bool,
    closed: bool,
    /// Latest pointer position in surface-local coordinates.
    pointer: Option<ui_core::geom::Point>,
    /// Pointer enter/leave tracking.
    pointer_over: bool,
    /// Whether the compositor currently gives this surface the keyboard.
    keyboard_focus: bool,
    /// Serial from the latest pointer enter; `set_shape` is ignored without it.
    pointer_enter_serial: Option<u32>,
    /// Last shape handed to the compositor, so a still pointer sends nothing.
    cursor_shape: Option<ui_core::CursorShape>,
    /// Input events collected since the last drain; the loop dispatches them.
    events: Vec<ui_core::Event>,
    /// Turns the wheel's two events per notch into one delta.
    wheel: crate::wheel::Wheel,
    /// False while a frame callback is outstanding: drawing another frame would
    /// be discarded by the compositor.
    frame_ready: bool,
    /// Frame callbacks delivered since the app last asked: the rate the compositor offers.
    frame_callbacks: u32,
    /// Set when something changed and a new frame is wanted.
    redraw_requested: bool,
    /// A change the app cannot see as an `Event` — focus, IME focus, output scale. A loop
    /// that skips frames on unchanged input must still rebuild for these.
    silent_change: bool,
    /// The output's mode refresh rate in millihertz, as the compositor declared it.
    refresh_mhz: u32,
    /// Keyboard focus came back: the compositor reset its text-input session, so the
    /// client must re-send `enable` and the whole state even though nothing changed.
    ime_reenter: bool,
    /// The compositor's own text-input `enter` arrived: only then may `enable` be sent.
    ime_entered: bool,
    /// Logical (surface-local) size, in points. Buffers are `logical * scale`.
    width: u32,
    height: u32,
    /// Fractional scale from the compositor, stored as quarters (150 => 1.5).
    scale_quarters: u32,
    /// Keyboard and pointer state.
    input: InputState,
    /// The seat's clipboard: the data device, the offer, and the pipes in flight.
    pub(crate) clipboard: crate::clipboard::Clipboard,
    /// Format/modifier pairs the compositor advertised for dma-buf import.
    dmabuf_formats: DmaBufFormats,
    /// dma-buf buffers the compositor still holds; each is released on `wl_buffer.release`.
    dmabuf_inflight: Vec<wl_buffer::WlBuffer>,
    /// Count of `wl_buffer.release` events seen for dma-buf buffers.
    dmabuf_released: u32,
    /// Count of `zwp_linux_buffer_params_v1.failed` events.
    dmabuf_rejected: u32,
    /// Composition the input method sent since the last `done`.
    ime: ImePending,
    /// The latest input serial, as the warp request must carry one of the client's own.
    warp_serial: Rc<Cell<u32>>,
    /// Whether the pointer is over this surface: a warp is only honoured with focus there.
    warp_focus: Rc<Cell<bool>>,
    /// The seat's key-repeat rate in hertz and delay in milliseconds; zero means none.
    repeat_rate: i32,
    repeat_delay: i32,
    /// The keys held down that are repeating, and when each is next due.
    repeating: Repeats,
}

/// The compositor's warp manager, as the cursor controller sees it.
///
/// Holds the surface and pointer to move, plus the two facts only the window hears: the
/// latest input serial, and whether the pointer is over this surface at all.
struct SurfaceWarp {
    manager: WpPointerWarpV1,
    surface: wl_surface::WlSurface,
    pointer: wl_pointer::WlPointer,
    serial: Rc<Cell<u32>>,
    focused: Rc<Cell<bool>>,
}

impl ui_platform::cursor::PointerWarp for SurfaceWarp {
    fn warp(&self, at: ui_core::geom::Point) -> bool {
        let serial = self.serial.get();
        // Both are the protocol's conditions: no focus, or a serial that is not one of the
        // client's recent inputs, and the compositor ignores the request.
        if serial == 0 || !self.focused.get() {
            return false;
        }
        self.manager.warp_pointer(
            &self.surface,
            &self.pointer,
            f64::from(at.x),
            f64::from(at.y),
            serial,
        );
        true
    }
}

/// The keys held down that are repeating, each with the instant its next repeat is due.
///
/// A repeat belongs to a physical key, not to the surface's one "held" slot: pressing a
/// modifier while a letter is held changes what that letter means, it does not end it, and
/// two directions held together both keep repeating.
#[derive(Default)]
struct Repeats {
    next: std::collections::HashMap<u32, std::time::Instant>,
}

impl Repeats {
    /// Start repeating `code` unless it already is, so a press does not reset its rhythm.
    fn hold(&mut self, code: u32, now: std::time::Instant, delay: std::time::Duration) {
        self.next.entry(code).or_insert(now + delay);
    }

    fn release(&mut self, code: u32) {
        self.next.remove(&code);
    }

    fn clear(&mut self) {
        self.next.clear();
    }

    /// The keys whose period has come round, each advanced past `now`.
    ///
    /// A turn that was busy for several periods replays once rather than catching up: a
    /// burst would type the same character many times for one press of the loop.
    fn due(&mut self, now: std::time::Instant, period: std::time::Duration) -> Vec<u32> {
        let mut due = Vec::new();
        for (code, next) in &mut self.next {
            if *next > now {
                continue;
            }
            while *next <= now {
                *next += period;
            }
            due.push(*code);
        }
        due
    }

    /// When one of them is next due, so the loop can wake for it with nothing else arriving.
    fn deadline(&self) -> Option<std::time::Instant> {
        self.next.values().copied().min()
    }
}

/// One batch of input-method state, applied together on `done`.
#[derive(Default)]
struct ImePending {
    /// Preedit text with its cursor byte offset; `None` means none arrived this batch.
    preedit: Option<(String, usize)>,
    commit: Option<String>,
    delete: Option<(usize, usize)>,
}

impl WindowState {
    fn new(width: u32, height: u32) -> Self {
        Self {
            shm: None,
            base: None,
            configured: false,
            closed: false,
            pointer: None,
            events: Vec::new(),
            wheel: crate::wheel::Wheel::default(),
            pointer_over: false,
            keyboard_focus: false,
            pointer_enter_serial: None,
            cursor_shape: None,
            frame_ready: true,
            frame_callbacks: 0,
            redraw_requested: true,
            silent_change: true,
            refresh_mhz: 0,
            ime_reenter: false,
            ime_entered: false,
            width,
            height,
            scale_quarters: 100,
            input: InputState::new(),
            clipboard: crate::clipboard::Clipboard::new(),
            colour_failed: None,
            entered_output: None,
            colour_dirty: true,
            preferred_luminances: None,
            preferred_tf: None,
            preferred_primaries: None,
            preferred_chromaticities: None,
            allow_info: false,
            colour_features: 0,
            colour_features_known: false,
            dmabuf_formats: DmaBufFormats::default(),
            dmabuf_inflight: Vec::new(),
            dmabuf_released: 0,
            dmabuf_rejected: 0,
            ime: ImePending::default(),
            warp_serial: Rc::new(Cell::new(0)),
            warp_focus: Rc::new(Cell::new(false)),
            repeat_rate: 0,
            repeat_delay: 0,
            repeating: Repeats::default(),
        }
    }

    /// A serial from this client's input, kept for the requests that need one.
    fn note_serial(&mut self, serial: u32) {
        self.clipboard.note_serial(serial);
        self.warp_serial.set(serial);
    }

    /// The events a press of `evdev` produces under the modifiers held now.
    ///
    /// A repeat asks the same question again rather than replaying the first press, so a
    /// modifier taken down or let go mid-hold changes what the key means, as it would for a
    /// client that never sees the repeat.
    fn press_events(&mut self, evdev: u32) -> Vec<ui_core::Event> {
        let mut events = Vec::new();
        // A command modifier turns the character into a command, never text:
        // Ctrl+A must not insert an `a`.
        let command =
            self.input.is_held(0xffe3) || self.input.is_held(0xffe9) || self.input.is_held(0xffeb);
        // The key names itself; only the unmodified path also types text, which is what
        // composes a dead key with the letter after it.
        if let Some(character) = self.input.key(evdev, true) {
            if command {
                events.push(ui_core::Event::KeyPressed {
                    key: ui_core::Key::Char(character),
                });
            } else {
                let typed = self.input.text(evdev).unwrap_or(character);
                self.input.push_text(typed);
                events.push(ui_core::Event::Text(typed.to_string()));
            }
        }
        // Named keys travel as themselves; text arrives as `Text` above.
        if let Some(named) = named_key(evdev) {
            events.push(ui_core::Event::KeyPressed { key: named });
        }
        events
    }
}

macro_rules! noop_dispatch {
    ($iface:ty, $udata:ty) => {
        impl Dispatch<$iface, $udata> for WindowState {
            fn event(
                _: &mut Self,
                _: &$iface,
                _: <$iface as Proxy>::Event,
                _: &$udata,
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    };
}

noop_dispatch!(wl_compositor::WlCompositor, ());
noop_dispatch!(wl_region::WlRegion, ());
noop_dispatch!(wl_shm_pool::WlShmPool, ());
noop_dispatch!(wl_shm::WlShm, ());
noop_dispatch!(wl_seat::WlSeat, ());
noop_dispatch!(shape_device::WpCursorShapeDeviceV1, ());
noop_dispatch!(
    crate::conn::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
    ()
);
noop_dispatch!(
    wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    ()
);
noop_dispatch!(wp_viewporter::WpViewporter, ());
noop_dispatch!(WpPointerWarpV1, ());
noop_dispatch!(wp_viewport::WpViewport, ());
noop_dispatch!(
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    ()
);

impl Dispatch<ZwpTextInputV3, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &ZwpTextInputV3,
        event: TextInputEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Collected, never applied here: `done` is the only commit point.
        match event {
            // The session is the compositor's to grant: `enable` is only legal between these
            // two, and a focus change ends the old session without changing its enabled flag.
            TextInputEvent::Enter { .. } => {
                state.ime_entered = true;
                state.ime_reenter = true;
                state.redraw_requested = true;
                state.silent_change = true;
            }
            TextInputEvent::Leave { .. } => {
                state.ime_entered = false;
                state.redraw_requested = true;
                state.silent_change = true;
            }
            TextInputEvent::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                let text = text.unwrap_or_default();
                // The caret sits at the *end* of the highlighted range: fcitx5 sends
                // `begin=0, end=len` for every pinyin preedit, and clients draw the caret
                // after the composition, not before it.
                let cursor = if cursor_begin < 0 || cursor_end < 0 {
                    text.len()
                } else {
                    (cursor_end as usize).min(text.len())
                };
                state.ime.preedit = Some((text, cursor));
            }
            TextInputEvent::CommitString { text } => {
                state.ime.commit = text;
            }
            TextInputEvent::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                state.ime.delete = Some((before_length as usize, after_length as usize));
            }
            TextInputEvent::Done { .. } => state.apply_ime_batch(),
            _ => {}
        }
    }
}

impl WindowState {
    /// Turn one `done` batch into engine events, in the order the engine expects them.
    fn apply_ime_batch(&mut self) {
        if let Some((before, after)) = self.ime.delete.take() {
            self.events
                .push(ui_core::Event::DeleteSurrounding { before, after });
        }
        // `done` resets the pending values, so a batch without a preedit means the input
        // method has none: the old one must go, or the last composed character sticks.
        let preedit = self.ime.preedit.take().and_then(|(text, cursor)| {
            (!text.is_empty()).then(|| ui_core::Preedit::new(text, cursor))
        });
        self.events.push(ui_core::Event::Preedit(preedit));
        if let Some(text) = self.ime.commit.take()
            && !text.is_empty()
        {
            self.events.push(ui_core::Event::Text(text));
        }
        self.redraw_requested = true;
    }
}

impl Dispatch<wl_output::WlOutput, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Mode { refresh, .. } = event {
            // The protocol reports millihertz, and zero when the compositor does not know.
            state.refresh_mhz = refresh.max(0) as u32;
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for WindowState {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            // The compositor stopped sampling; dropping the buffer now is safe.
            let before = state.dmabuf_inflight.len();
            state.dmabuf_inflight.retain(|held| held != buffer);
            state.dmabuf_released += (before - state.dmabuf_inflight.len()) as u32;
        }
    }
}

impl Dispatch<WpLinuxDrmSyncobjSurfaceV1, ()> for WindowState {
    fn event(
        _: &mut Self,
        _: &WpLinuxDrmSyncobjSurfaceV1,
        _: wayland_protocols::wp::linux_drm_syncobj::v1::client::wp_linux_drm_syncobj_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The release point tells us the compositor is done reading; we never reuse a
        // buffer before its `wl_buffer.release`, so there is nothing to act on.
    }
}

impl Dispatch<WpLinuxDrmSyncobjTimelineV1, ()> for WindowState {
    fn event(
        _: &mut Self,
        _: &WpLinuxDrmSyncobjTimelineV1,
        _: wayland_protocols::wp::linux_drm_syncobj::v1::client::wp_linux_drm_syncobj_timeline_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The timeline has no events.
    }
}

impl Dispatch<WpLinuxDrmSyncobjManagerV1, ()> for WindowState {
    fn event(
        _: &mut Self,
        _: &WpLinuxDrmSyncobjManagerV1,
        _: wayland_protocols::wp::linux_drm_syncobj::v1::client::wp_linux_drm_syncobj_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The manager has no events.
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &ZwpLinuxDmabufV1,
        event: DmaBufEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            DmaBufEvent::Format { format } => state.dmabuf_formats.record_format(format),
            DmaBufEvent::Modifier {
                format,
                modifier_hi,
                modifier_lo,
            } => state.dmabuf_formats.record_modifier(
                format,
                dmabuf::modifier_from_parts(modifier_hi, modifier_lo),
            ),
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &ZwpLinuxBufferParamsV1,
        event: ParamsEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // create_immed reports a rejected import asynchronously, never as Err.
        if let ParamsEvent::Failed = event {
            state.dmabuf_rejected += 1;
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                // Only XKB_V1 text keymaps are handled; anything else is ignored.
                if matches!(
                    format,
                    wayland_client::WEnum::Value(wl_keyboard::KeymapFormat::XkbV1)
                ) {
                    // Hand the fd to xkbcommon: the string path cannot work here,
                    // because keeping the trailing NUL panics CString and dropping
                    // it makes the parser reject the text.
                    let _ = unsafe { state.input.set_keymap_fd(fd, size as usize) };
                }
                state.redraw_requested = true;
            }
            wl_keyboard::Event::Enter { serial, .. } => {
                state.keyboard_focus = true;
                // Focus returned, so the session the compositor dropped must be rebuilt; the
                // text-input `enter` says the same thing and gates when it may be sent.
                // `set_selection` is only honoured with a serial from our own input.
                state.note_serial(serial);
                state.ime_reenter = true;
                state.redraw_requested = true;
                state.silent_change = true;
            }
            wl_keyboard::Event::Leave { .. } => {
                state.keyboard_focus = false;
                // A key can no longer be heard as released, so it must not keep repeating.
                state.repeating.clear();
                state.redraw_requested = true;
                state.silent_change = true;
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                // The repeat is the client's: the seat states the rate and delay once, and
                // nothing more arrives while a key stays down.
                state.repeat_rate = rate;
                state.repeat_delay = delay;
            }
            wl_keyboard::Event::Key {
                key,
                serial,
                state: key_state,
                ..
            } => {
                state.note_serial(serial);
                let pressed = matches!(
                    key_state,
                    wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed)
                );
                if pressed {
                    let produced = state.press_events(key);
                    // A key that produced nothing never repeats: a modifier only changes what
                    // the keys held beside it mean, and a period of replaying nothing would
                    // take the repeat from a letter that is still down.
                    let repeatable = !produced.is_empty() && state.repeat_rate > 0;
                    if repeatable {
                        state.repeating.hold(
                            key,
                            std::time::Instant::now(),
                            std::time::Duration::from_millis(state.repeat_delay.max(0) as u64),
                        );
                    }
                    state.events.extend(produced);
                } else {
                    state.input.key(key, false);
                    state.repeating.release(key);
                }
                state.redraw_requested = true;
            }
            wl_keyboard::Event::Modifiers {
                serial,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => {
                state.note_serial(serial);
                state
                    .input
                    .update_modifiers(mods_depressed, mods_latched, mods_locked, group);
                // Shift_L / Control_L / Alt_L / Super_L keysyms.
                state
                    .events
                    .push(ui_core::Event::Modifiers(ui_core::Modifiers {
                        shift: state.input.is_held(0xffe1),
                        ctrl: state.input.is_held(0xffe3),
                        alt: state.input.is_held(0xffe9),
                        logo: state.input.is_held(0xffeb),
                    }));
            }
            _ => {}
        }
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            // The compositor's value is in 120ths of 1.0 by protocol, but the
            // integer encoding used here is quarters so 150 means 1.5.
            // Protocol scale is in 120ths of 1.0; `scale_quarters` is in 100ths.
            let quarters = ((scale as f64) * 100.0 / 120.0).round() as u32;
            #[cfg(debug_assertions)]
            if std::env::var_os("UI_WAYLAND_DEBUG").is_some() {
                trace(format_args!(
                    "preferred_scale {scale} -> {:.2}x",
                    quarters as f32 / 100.0
                ));
            }
            if quarters != state.scale_quarters {
                state.scale_quarters = quarters.max(100);
                // A scale change needs a full repaint and a buffer resize.
                state.redraw_requested = true;
                state.silent_change = true;
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for WindowState {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for WindowState {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 && height > 0 {
                    state.width = width as u32;
                    state.height = height as u32;
                }
                // A state change (maximised, tiled) is not an `Event` either.
                state.silent_change = true;
            }
            xdg_toplevel::Event::Close => {
                ui_platform::exit_probe::stage("toplevel-close");
                state.closed = true
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for WindowState {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // A zero axis means "no constraint"; the anchors already decided it.
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                surface.ack_configure(serial);
                if width > 0 {
                    state.width = width;
                }
                if height > 0 {
                    state.height = height;
                }
                state.configured = true;
            }
            zwlr_layer_surface_v1::Event::Closed => state.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            // The compositor is ready for another frame.
            state.frame_ready = true;
            state.frame_callbacks += 1;
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                serial,
                surface_x,
                surface_y,
                ..
            } => {
                // set_shape is ignored unless this matches the latest enter serial.
                state.pointer_enter_serial = Some(serial);
                // The compositor drops our shape when the pointer leaves, so the cache must
                // not suppress the re-assert on the way back in.
                state.cursor_shape = None;
                state.pointer_over = true;
                state.warp_focus.set(true);
                let p = ui_core::geom::Point::new(surface_x as f32, surface_y as f32);
                state.pointer = Some(p);
                state.input.set_pointer(Some(p));
                state.events.push(ui_core::Event::PointerMoved { pos: p });
                state.redraw_requested = true;
            }
            wl_pointer::Event::Leave { .. } => {
                state.pointer_over = false;
                state.warp_focus.set(false);
                state.pointer_enter_serial = None;
                state.cursor_shape = None;
                state.pointer = None;
                // The coupled `axis` of a pending notch will never arrive now.
                state.wheel.leave();
                state.input.set_pointer(None);
                state.events.push(ui_core::Event::PointerLeft);
                state.redraw_requested = true;
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                let p = ui_core::geom::Point::new(surface_x as f32, surface_y as f32);
                state.pointer = Some(p);
                state.input.set_pointer(Some(p));
                state.events.push(ui_core::Event::PointerMoved { pos: p });
                state.redraw_requested = true;
            }
            wl_pointer::Event::Axis {
                axis: wayland_client::WEnum::Value(axis),
                value,
                ..
            } => {
                // A surface-local length: a finger drag, or the restatement of a notch. Which
                // one it is depends on the rest of the frame, so it is only recorded here.
                state.wheel.axis(axis, value as f32);
            }
            wl_pointer::Event::AxisValue120 {
                axis: wayland_client::WEnum::Value(axis),
                value120,
            } => {
                // The wheel's discrete step, 120 per detent: the only place the size of a
                // notch is defined, so the engine's distance does not depend on the
                // compositor's choice of pixels per line. Held for the frame, which decides
                // between it and the `axis` that restates it.
                state.wheel.value120(axis, value120);
            }
            wl_pointer::Event::Frame => {
                // The frame is the unit the protocol groups a notch into, and the events of
                // one may arrive in either order: scroll is settled here, once per axis.
                if let Some(pos) = state.pointer {
                    for delta in state.wheel.frame() {
                        state.events.push(ui_core::Event::Scroll { pos, delta });
                        state.redraw_requested = true;
                    }
                } else {
                    state.wheel.leave();
                }
            }
            wl_pointer::Event::Button {
                button,
                state: bs,
                serial,
                ..
            } => {
                // A click is a fresh serial, and `set_selection` needs one from our input.
                state.note_serial(serial);
                // Linux input-event-codes: BTN_LEFT is 0x110; other buttons map by order.
                let which = match button {
                    0x110 => ui_core::PointerButton::Primary,
                    0x111 => ui_core::PointerButton::Secondary,
                    _ => ui_core::PointerButton::Middle,
                };
                let pos = state
                    .pointer
                    .unwrap_or_else(|| ui_core::geom::Point::new(0.0, 0.0));
                let ev = if bs == wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed) {
                    ui_core::Event::PointerPressed { pos, button: which }
                } else {
                    ui_core::Event::PointerReleased { pos, button: which }
                };
                state.events.push(ev);
                state.redraw_requested = true;
            }
            _ => {}
        }
    }
}

/// Map an evdev keycode to the keys the framework names; others stay text-only.
fn named_key(evdev: u32) -> Option<ui_core::Key> {
    // Linux input-event-codes.h, the raw codes Wayland delivers (no +8 shift).
    Some(match evdev {
        1 => ui_core::Key::Escape,
        14 => ui_core::Key::Backspace,
        15 => ui_core::Key::Tab,
        28 => ui_core::Key::Enter,
        57 => ui_core::Key::Space,
        102 => ui_core::Key::Home,
        103 => ui_core::Key::Up,
        104 => ui_core::Key::PageUp,
        105 => ui_core::Key::Left,
        106 => ui_core::Key::Right,
        107 => ui_core::Key::End,
        108 => ui_core::Key::Down,
        109 => ui_core::Key::PageDown,
        110 => ui_core::Key::Insert,
        111 => ui_core::Key::Delete,
        _ => return None,
    })
}

/// A real XDG toplevel; `wl_shm` by default, dma-buf when asked to present one.
pub struct WaylandWindow {
    /// Kept alive: dropping the connection closes the window.
    _conn: Connection,
    /// The registry this window bound from; shared with the application's [`WaylandLink`].
    globals: Arc<GlobalList>,
    queue: EventQueue<WindowState>,
    state: WindowState,
    qh: QueueHandle<WindowState>,
    surface: wl_surface::WlSurface,
    /// Kept to build the input region a layer surface derives from its hit index.
    compositor: wl_compositor::WlCompositor,
    /// The consumer areas last published as the input region, for the dedupe.
    input_areas: Vec<ui_core::geom::Rect>,
    /// The region changed since the last commit; a drawing-less frame must still publish it.
    input_pending: bool,
    /// Kept alive so the compositor keeps the toplevel mapped; `None` for a layer surface.
    _xdg: Option<xdg_surface::XdgSurface>,
    _toplevel: Option<xdg_toplevel::XdgToplevel>,
    /// The layer-shell role, when this surface is an overlay instead of a window.
    layer: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    /// The logical size last asked of the layer role, for the dedupe: the configure that
    /// carries it arrives later, so the configured size is not the answer yet.
    layer_size_request: Option<(u32, u32)>,
    /// Whether this layer surface follows the pointer with its keyboard interactivity.
    keyboard_follow: bool,
    /// The interactivity last requested, for the dedupe: motion arrives every frame and a
    /// repeated request is a needless commit.
    keyboard_sent: Option<zwlr_layer_surface_v1::KeyboardInteractivity>,
    /// dma-buf factory, when the compositor exposes one. Kept at version <= 3.
    dmabuf: Option<ZwpLinuxDmabufV1>,
    /// `wl_buffer`s reused per dma-buf identity; creating one per frame makes the compositor
    /// build and tear down an import object every frame for no reason.
    dmabuf_cache: HashMap<(u64, u64, u32, u32, u32, u32, u32, u64), DmaBufBuffer>,
    /// Explicit-synchronization objects, present only when the compositor advertises them.
    syncobj_manager: Option<WpLinuxDrmSyncobjManagerV1>,
    syncobj_surface: Option<WpLinuxDrmSyncobjSurfaceV1>,
    syncobj_timeline: Option<WpLinuxDrmSyncobjTimelineV1>,
    /// The compositor signals this timeline once it is done reading a frame.
    syncobj_release_timeline: Option<WpLinuxDrmSyncobjTimelineV1>,
    /// Point we ask the compositor to signal on the release timeline; grows every frame.
    /// Colour management, when the compositor exposes it.
    color_manager: Option<wayland_protocols::wp::color_management::v1::client::wp_color_manager_v1::WpColorManagerV1>,
    /// First output; its description carries the real luminances.
    output: Option<wayland_client::protocol::wl_output::WlOutput>,
    /// The output's own image description, kept for [`declare_output_description`].
    output_desc: Option<wayland_protocols::wp::color_management::v1::client::wp_image_description_v1::WpImageDescriptionV1>,
    /// The output object the current description came from, kept for its change event.
    colour_output: Option<wayland_protocols::wp::color_management::v1::client::wp_color_management_output_v1::WpColorManagementOutputV1>,
    /// One colour-management object per surface: a second `get_surface` is a protocol error.
    colour_surface: Option<wayland_protocols::wp::color_management::v1::client::wp_color_management_surface_v1::WpColorManagementSurfaceV1>,
    /// Colour feedback, kept alive so `preferred_changed` reaches the window.
    feedback: Option<wayland_protocols::wp::color_management::v1::client::wp_color_management_surface_feedback_v1::WpColorManagementSurfaceFeedbackV1>,
    /// Logical surface size set through the viewport, if fractional scaling is on.
    viewport: Option<wp_viewport::WpViewport>,
    /// Set when the logical size changed and must be pushed to the viewport.
    viewport_dirty: bool,
    /// Logical size last pushed to the viewport.
    viewport_size: (u32, u32),
    /// Double buffering: each slot owns its own pool, so a buffer still held by
    /// the compositor can never be overwritten.
    back: Option<ShmBuffer>,
    front: Option<(wl_buffer::WlBuffer, ShmBuffer)>,
    pixels: Pixels,
    /// Buffers handed to the compositor, so we can mark them released.
    _inflight: Vec<wl_buffer::WlBuffer>,
    /// Pointer device, kept alive so motion events keep arriving.
    _pointer: Option<wl_pointer::WlPointer>,
    /// The compositor's warp manager, when it offers one; this is how a keyboard moves
    /// the pointer, and destroying it is polite at close.
    pointer_warp: Option<WpPointerWarpV1>,
    /// The surface's pointer as keyboard steps move it.
    cursor: ui_platform::cursor::Cursor,
    /// Named-shape device for that pointer, when the compositor supports it.
    cursor_device: Option<shape_device::WpCursorShapeDeviceV1>,
    /// Keyboard device, kept alive so key events keep arriving.
    _keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Text-input object; absent when the compositor has no input-method protocol.
    text_input: Option<ZwpTextInputV3>,
    /// IME state the engine asked for on the last frame, and what was last sent.
    ime: Option<ui_platform::present::ImeState>,
    ime_sent: Option<ui_platform::present::ImeState>,
    /// Set when buffers were just reallocated and the back slot must be replaced.
    pool_fresh: bool,
}

impl Drop for WaylandWindow {
    /// Tear down in protocol order instead of leaving it to the disconnect.
    ///
    /// An input method left enabled keeps the compositor's IM session alive until it
    /// notices the disconnect; destroying the surface first would strand the buffers.
    fn drop(&mut self) {
        ui_platform::exit_probe::stage("window: ime");
        if let Some(ti) = self.text_input.as_ref() {
            ti.disable();
            ti.commit();
        }
        // Drop the surface's buffer before the proxies: a compositor that still holds a
        // dma-buf at close time may sync on it, and this is the client's only lever there.
        let keep = std::env::var_os("EXIT_KEEP_BUFFERS").is_some();
        if !std::env::var_os("EXIT_NO_DETACH").is_some() {
            self.surface.attach(None, 0, 0);
            self.surface.commit();
            let _ = self._conn.flush();
        }
        ui_platform::exit_probe::stage("window: buffers");
        if !keep {
            if let Some((buffer, _)) = self.front.take() {
                buffer.destroy();
            }
            for (_, cached) in self.dmabuf_cache.drain() {
                cached.destroy();
            }
            self.state.dmabuf_inflight.clear();
        }
        ui_platform::exit_probe::stage("window: proxies");
        if let Some(t) = self._toplevel.take() {
            t.destroy();
        }
        if let Some(x) = self._xdg.take() {
            x.destroy();
        }
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
        if let Some(warp) = self.pointer_warp.take() {
            warp.destroy();
        }
        if let Some(l) = self.layer.take() {
            l.destroy();
        }
        if let Some(ti) = self.text_input.take() {
            ti.destroy();
        }
        ui_platform::exit_probe::stage("window: surface");
        self.surface.clone().destroy();
        let _ = self._conn.flush();
        ui_platform::exit_probe::stage("window: dropped");
    }
}

impl WaylandWindow {
    /// Create and map a surface, waiting until the compositor configures it.
    pub fn new(spec: SurfaceSpec) -> Result<Self, WindowError> {
        Self::create(spec, None)
    }

    /// The connection this window runs on, for platform-bound application code.
    ///
    /// Hand it to code that speaks protocols the engine does not; that code binds them
    /// on a queue of its own ([`WaylandLink::new_queue`]) instead of opening a second
    /// connection, whose events this window's wait would never wake for.
    pub fn link(&self) -> WaylandLink {
        WaylandLink::new(self._conn.clone(), Arc::clone(&self.globals))
    }

    pub(crate) fn create(
        spec: SurfaceSpec,
        link: Option<&WaylandLink>,
    ) -> Result<Self, WindowError> {
        let layer: Option<LayerSpec> = spec.layer_spec().cloned();
        let keyboard_follow = layer
            .as_ref()
            .is_some_and(|layer| layer.keyboard == KeyboardMode::FollowPointer);
        let (conn, mut queue, caps, globals) = match link {
            None => {
                let (conn, queue, caps, globals) = crate::conn::connect()?;
                (conn, queue, caps, Arc::new(globals))
            }
            // A second surface binds its own proxies on a queue of its own; only the
            // connection and the registry are shared.
            Some(link) => {
                let conn = link.connection().clone();
                let queue = conn.new_event_queue::<WindowState>();
                let caps = crate::conn::bind(link.globals(), &queue.handle())?;
                (conn, queue, caps, link.globals_shared())
            }
        };
        let qh: QueueHandle<WindowState> = queue.handle();
        let crate::conn::Capabilities {
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
            text_input: text_input_manager,
            pointer_warp,
            output,
        } = caps;

        // A layer's configure carries the real size; the opening one is a placeholder there.
        let opening = match &spec.role {
            SurfaceRole::Window { size } => *size,
            SurfaceRole::Layer(layer) => layer.size.unwrap_or((0, 0)),
        };
        let mut state = WindowState::new(opening.0, opening.1);
        state.base = wm_base.clone();

        let surface = compositor.create_surface(&qh, ());
        // Kept for the surface's lifetime: the compositor reports a change of mind through
        // this object, and it says nothing to an object that was destroyed after one read.
        let feedback = color_manager
            .as_ref()
            .map(|manager| manager.get_surface_feedback(&surface, &qh, ()));
        // The explicit-sync surface is created only once a timeline is attached: creating it
        // first makes every dma-buf commit require an acquire point, which we would not have.
        let syncobj_surface = None;
        // Fractional scaling needs a viewport: the buffer is `logical * scale`, and
        // the viewport shrinks it back to the logical size.
        let viewport = match (&viewporter, &fractional) {
            (Some(vp), Some(fs)) => {
                let viewport = vp.get_viewport(&surface, &qh, ());
                let _scale = fs.get_fractional_scale(&surface, &qh, ());
                // Buffer scale stays 1 under fractional scaling.
                surface.set_buffer_scale(1);
                Some(viewport)
            }
            _ => {
                // Integer fallback: ask the compositor for an integer buffer scale.
                surface.set_buffer_scale(1);
                None
            }
        };
        let (xdg, toplevel, layer_surface) = match layer {
            None => {
                let wm = wm_base
                    .clone()
                    .ok_or(WindowError::MissingGlobal("xdg_wm_base"))?;
                let xdg = wm.get_xdg_surface(&surface, &qh, ());
                let toplevel = xdg.get_toplevel(&qh, ());
                toplevel.set_title(spec.name.clone());
                toplevel.set_app_id(spec.app_id.clone());
                // Logical units, which is what xdg_toplevel's minimum is in.
                if let Some((width, height)) = spec.min_size {
                    toplevel.set_min_size(width as i32, height as i32);
                }
                (Some(xdg), Some(toplevel), None)
            }
            Some(cfg) => {
                let shell = layer_shell
                    .clone()
                    .ok_or(WindowError::MissingGlobal("zwlr_layer_shell_v1"))?;
                let surface_role = shell.get_layer_surface(
                    &surface,
                    output.as_ref(),
                    match cfg.level {
                        Layer::Background => zwlr_layer_shell_v1::Layer::Background,
                        Layer::Bottom => zwlr_layer_shell_v1::Layer::Bottom,
                        Layer::Top => zwlr_layer_shell_v1::Layer::Top,
                        Layer::Overlay => zwlr_layer_shell_v1::Layer::Overlay,
                    },
                    spec.name.clone(),
                    &qh,
                    (),
                );
                // Zero means "whatever the anchors leave"; the configure carries the answer.
                let (w, h) = cfg.size.unwrap_or((0, 0));
                surface_role.set_size(w, h);
                let mut anchor = zwlr_layer_surface_v1::Anchor::empty();
                anchor.set(zwlr_layer_surface_v1::Anchor::Top, cfg.anchor.top);
                anchor.set(zwlr_layer_surface_v1::Anchor::Bottom, cfg.anchor.bottom);
                anchor.set(zwlr_layer_surface_v1::Anchor::Left, cfg.anchor.left);
                anchor.set(zwlr_layer_surface_v1::Anchor::Right, cfg.anchor.right);
                surface_role.set_anchor(anchor);
                let margin = cfg.margin;
                surface_role.set_margin(
                    margin.top.round() as i32,
                    margin.right.round() as i32,
                    margin.bottom.round() as i32,
                    margin.left.round() as i32,
                );
                surface_role.set_exclusive_zone(match cfg.exclusive {
                    ExclusiveZone::Ignore => -1,
                    ExclusiveZone::Reserve(amount) => amount,
                });
                // Following the pointer starts at `None`: a surface that covers the screen
                // must not take the keyboard while it maps, only once the pointer reaches
                // something it claims.
                surface_role.set_keyboard_interactivity(match cfg.keyboard {
                    KeyboardMode::None | KeyboardMode::FollowPointer => {
                        zwlr_layer_surface_v1::KeyboardInteractivity::None
                    }
                    KeyboardMode::OnDemand => {
                        zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand
                    }
                    KeyboardMode::Exclusive => {
                        zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive
                    }
                });
                (None, None, Some(surface_role))
            }
        };
        surface.commit();

        // Wait for the first configure so the size is known before allocating.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !state.configured {
            if std::time::Instant::now() > deadline {
                return Err(WindowError::Protocol("configure timeout".into()));
            }
            queue
                .blocking_dispatch(&mut state)
                .map_err(|e| WindowError::Protocol(e.to_string()))?;
        }

        // Grab a pointer for hover feedback.
        let mut pointer: Option<wl_pointer::WlPointer> = None;
        let mut keyboard: Option<wl_keyboard::WlKeyboard> = None;
        let mut text_input: Option<ZwpTextInputV3> = None;
        if let Some(seat) = seat {
            pointer = Some(seat.get_pointer(&qh, ()));
            keyboard = Some(seat.get_keyboard(&qh, ()));
            // Text input is per seat, like the keyboard it replaces.
            text_input = text_input_manager.map(|m| m.get_text_input(&seat, &qh, ()));
            // The clipboard is per seat too: one data device carries its selection.
            if let Some(manager) = data_device_manager {
                let device = manager.get_data_device(&seat, &qh, ());
                state.clipboard.attach(manager, device);
            }
        }
        // The shape device is bound to a specific pointer, so it follows that pointer.
        let cursor_device = match (cursor_shape, pointer.as_ref()) {
            (Some(manager), Some(p)) => Some(manager.get_pointer(p, &qh, ())),
            _ => None,
        };
        // A keyboard moves the pointer only where the compositor offers the protocol; the
        // serial and the focus it authorises with are the window's, written by its events.
        let cursor = match (pointer_warp.as_ref(), pointer.as_ref()) {
            (Some(manager), Some(pointer)) => {
                ui_platform::cursor::Cursor::new(Rc::new(SurfaceWarp {
                    manager: manager.clone(),
                    surface: surface.clone(),
                    pointer: pointer.clone(),
                    serial: Rc::clone(&state.warp_serial),
                    focused: Rc::clone(&state.warp_focus),
                }))
            }
            _ => ui_platform::cursor::Cursor::fixed(),
        };

        let (w, h) = (state.width, state.height);
        let pixels = Pixels::new(w, h);
        let back = ShmBuffer::new(&shm, &qh, w, h).ok_or(WindowError::Buffer)?;

        // A roundtrip delivers the keyboard's initial keymap before returning, so
        // callers can type on the very first frame.
        queue
            .roundtrip(&mut state)
            .map_err(|e| WindowError::Protocol(e.to_string()))?;

        let mut win = Self {
            color_manager,
            output,
            output_desc: None,
            colour_output: None,
            colour_surface: None,
            feedback,
            _conn: conn,
            globals,
            queue,
            state,
            qh,
            surface,
            compositor,
            input_areas: Vec::new(),
            input_pending: false,
            layer_size_request: None,
            keyboard_follow,
            keyboard_sent: keyboard_follow
                .then_some(zwlr_layer_surface_v1::KeyboardInteractivity::None),
            _xdg: xdg,
            _toplevel: toplevel,
            layer: layer_surface,
            dmabuf,
            dmabuf_cache: HashMap::new(),
            syncobj_manager: syncobj,
            syncobj_surface,
            syncobj_timeline: None,
            syncobj_release_timeline: None,
            viewport,
            viewport_dirty: true,
            viewport_size: (0, 0),
            back: Some(back),
            front: None,
            pixels,
            _inflight: Vec::new(),
            _pointer: pointer,
            pointer_warp,
            cursor,
            _keyboard: keyboard,
            text_input,
            ime: None,
            ime_sent: None,
            cursor_device,
            pool_fresh: false,
        };
        win.state.shm = Some(shm);
        win.resize_with_callback();
        Ok(win)
    }

    /// Drain input events collected since the last call, in arrival order.
    ///
    /// Reads the socket first: intake must not depend on the pacing wait, or a loop that
    /// never blocks (uncapped frames) sees no input at all.
    pub fn take_events(&mut self) -> Vec<ui_core::Event> {
        let _ = self.drain_events();
        self.replay_repeat();
        std::mem::take(&mut self.state.events)
    }

    /// Emit each held key's events when its period has come round.
    fn replay_repeat(&mut self) {
        let rate = self.state.repeat_rate;
        if rate <= 0 {
            return;
        }
        let period = std::time::Duration::from_secs_f64(1.0 / f64::from(rate));
        for code in self.state.repeating.due(std::time::Instant::now(), period) {
            let events = self.state.press_events(code);
            self.state.events.extend(events);
            self.state.redraw_requested = true;
        }
    }

    /// When a held key is next due, so the loop wakes for it with nothing else arriving.
    pub fn repeat_deadline(&self) -> Option<std::time::Instant> {
        self.state.repeating.deadline()
    }

    /// Move the pointer by one step from where the seat last put it.
    ///
    /// Answers where the pointer now is; `None` when nothing moved, which the caller reads
    /// as "keep the position you had" rather than as a failure.
    pub fn nudge_pointer(&mut self, dx: f32, dy: f32) -> Option<ui_core::geom::Point> {
        let bounds = (self.state.width as f32, self.state.height as f32);
        let at = self.cursor.step(self.pointer(), (dx, dy), bounds)?;
        // The compositor's motion events are the truth, but the next step in this same frame
        // must build on this one, so the position is taken now.
        self.state.pointer = Some(at);
        Some(at)
    }

    /// Pointer position, when the pointer is over the surface.
    pub fn pointer(&self) -> Option<ui_core::geom::Point> {
        // Leaving the surface must clear it, or hover sticks forever after the pointer goes.
        if self.state.pointer_over {
            self.state.pointer
        } else {
            None
        }
    }

    pub fn should_close(&self) -> bool {
        self.state.closed
    }

    /// Close the window the way the compositor's Close event does, for scripted exit tests.
    pub fn request_close(&mut self) {
        ui_platform::exit_probe::stage("requested-close");
        self.state.closed = true;
    }

    /// Typed characters since the last call.
    pub fn take_typed(&mut self) -> Vec<char> {
        self.state.input.take_typed()
    }

    /// Whether the compositor's keymap has been installed.
    pub fn has_keymap(&self) -> bool {
        self.state.input.has_keymap()
    }

    /// The output's refresh rate, when the compositor declared one.
    ///
    /// This is the pacing ground truth: measuring "commit until the callback" instead gives
    /// about half a period, because the callback lands on the next boundary, not a period later.
    pub fn refresh_hz(&self) -> Option<f32> {
        (self.state.refresh_mhz > 0).then(|| self.state.refresh_mhz as f32 / 1000.0)
    }

    /// Whether focus, IME focus, output scale or window state changed since the last call.
    ///
    /// These carry no `ui_core::Event`, so a loop that skips rebuilds on unchanged input
    /// would otherwise show a stale focus ring until unrelated input arrived.
    pub fn take_silent_change(&mut self) -> bool {
        std::mem::take(&mut self.state.silent_change)
    }

    /// Whether something asked for a repaint.
    pub fn redraw_requested(&self) -> bool {
        self.state.redraw_requested
    }

    /// Current fractional scale (for example 1.5).
    pub fn scale(&self) -> f32 {
        self.state.scale_quarters as f32 / 100.0
    }

    /// Logical surface size in points; buffers are larger by `scale()`.
    pub fn logical_size(&self) -> (u32, u32) {
        (self.state.width, self.state.height)
    }

    /// Whether another frame may be drawn right now.
    pub fn can_draw(&self) -> bool {
        self.state.frame_ready
    }

    /// Frame callbacks since the last call: the rate the compositor is actually offering.
    ///
    /// When this is below the output's refresh, the loop is not the limiter.
    pub fn take_frame_callbacks(&mut self) -> u32 {
        std::mem::take(&mut self.state.frame_callbacks)
    }

    /// Whether the compositor exposes `zwp_linux_dmabuf_v1`.
    pub fn has_dmabuf(&self) -> bool {
        self.dmabuf.is_some()
    }

    /// Bound `zwp_linux_dmabuf_v1` version, if the global exists.
    /// Declare the surface with the output's **own** description.
    ///
    /// A constructed description claims a 10000-nit peak, so the compositor tone-maps it
    /// down to the panel and darkens everything; reusing the output's description leaves
    /// it nothing to map.
    pub fn declare_output_description(&mut self) -> bool {
        let Some(desc) = self.output_desc.clone() else {
            return false;
        };
        let Some(manager) = self.color_manager.clone() else {
            return false;
        };
        let qh = self.qh.clone();
        // One object per surface: creating a second is `CM surface already exists`.
        let cm = match self.colour_surface.clone() {
            Some(cm) => cm,
            None => {
                let cm = manager.get_surface(&self.surface, &qh, ());
                self.colour_surface = Some(cm.clone());
                cm
            }
        };
        cm.set_image_description(
            &desc,
            wayland_protocols::wp::color_management::v1::client::wp_color_manager_v1::RenderIntent::Perceptual,
        );
        // A rejected description is reported on the description object, not by the call.
        self.state.colour_failed = None;
        let _ = self.queue.roundtrip(&mut self.state);
        if self.state.colour_failed.is_some() {
            return false;
        }
        true
    }

    /// Declare the surface with its output's own description when that output is HDR.
    ///
    /// A constructed description claims a 10000-nit peak and gets tone-mapped down, so the
    /// output's own is the only one that leaves the compositor nothing to map. Returns what
    /// the content must be converted to, or `None` to present SDR.
    pub fn declare_hdr_surface(&mut self) -> Option<HdrSurface> {
        use wayland_protocols::wp::color_management::v1::client::wp_color_manager_v1::TransferFunction;
        let (min_units, max_nits, reference_nits) = self.output_luminances()?;
        // A high peak alone is not enough: the renderer only encodes PQ.
        if max_nits <= 100 || self.state.preferred_tf != Some(TransferFunction::St2084Pq as u32) {
            return None;
        }
        let min_nits = min_units as f32 / 10_000.0;
        let description = ui_core::colour::OutputDescription {
            color_space: ui_core::colour::ColorSpace::Bt2020,
            transfer: ui_core::colour::TransferFunction::Pq,
            max_luminance: max_nits as f32,
            min_luminance: min_nits,
        };
        let format = ui_core::colour::BufferFormat::choose(&description);
        // Declaring HDR with a buffer the compositor cannot import would fail at commit.
        if !self.dmabuf_formats().has_format(format.drm_fourcc()) {
            trace(format_args!(
                "compositor does not advertise {format:?}; staying SDR"
            ));
            return None;
        }
        let surface = HdrSurface {
            reference_nits: if reference_nits > 0 {
                reference_nits
            } else {
                80
            },
            min_nits,
            max_nits: max_nits as f32,
            format,
            gamut: self.output_gamut_matrix(),
        };
        self.declare_output_description().then_some(surface)
    }

    /// Whether an input to the surface's colour changed since the last resolution.
    pub fn take_colour_change(&mut self) -> bool {
        let _ = self.queue.dispatch_pending(&mut self.state);
        std::mem::take(&mut self.state.colour_dirty)
    }

    /// The output the surface actually sits on: the last `enter`, else the bound one.
    fn surface_output(&self) -> Option<wl_output::WlOutput> {
        self.state
            .entered_output
            .clone()
            .or_else(|| self.output.clone())
    }

    /// Resolve the surface's colour now and declare it, reverting to sRGB when not HDR.
    ///
    /// `allow_hdr` is the caller's policy: a surface that may not be HDR is put back on the
    /// default declaration, which is what dropping the colour-management object does.
    pub fn refresh_surface_colour(&mut self, allow_hdr: bool) -> Option<HdrSurface> {
        self.state.colour_dirty = false;
        if allow_hdr && let Some(surface) = self.declare_hdr_surface() {
            return Some(surface);
        }
        // The object stays; unsetting the description is what reverts the surface to sRGB.
        if let Some(cm) = self.colour_surface.as_ref() {
            cm.unset_image_description();
            let _ = self.queue.roundtrip(&mut self.state);
        }
        None
    }

    /// Read luminances from the **output** description, the path the old project used.
    pub fn output_luminances(&mut self) -> Option<(u32, u32, u32)> {
        let manager = self.color_manager.clone()?;
        let output = self.surface_output()?;
        let qh = self.qh.clone();
        let cm_out = manager.get_output(&output, &qh, ());
        let desc = cm_out.get_image_description(&qh, ());
        self.state.preferred_luminances = None;
        self.state.preferred_tf = None;
        self.state.preferred_primaries = None;
        self.state.preferred_chromaticities = None;
        self.state.allow_info = true;
        let _ = self.queue.roundtrip(&mut self.state);
        let _ = self.queue.roundtrip(&mut self.state);
        self.state.allow_info = false;
        let out = self.state.preferred_luminances;
        trace(format_args!(
            "output prefers tf={:?} primaries={:?}",
            self.state.preferred_tf, self.state.preferred_primaries
        ));
        // Keep the description: setting it verbatim on the surface is what makes a
        // capture-paste round trip exact, since the compositor then has nothing to map.
        if let Some(old) = self.output_desc.replace(desc) {
            old.destroy();
        }
        // Keep the output object too: `image_description_changed` is how a compositor
        // reports that the panel's description moved.
        if let Some(old) = self.colour_output.replace(cm_out) {
            old.destroy();
        }
        out
    }

    /// Ask the compositor what this surface should look like, and read its luminances.
    ///
    /// The returned reference white is the anchor every description must agree on.
    pub fn preferred_luminances(&mut self) -> Option<(u32, u32, u32)> {
        let qh = self.qh.clone();
        // The stored object is the one the compositor reports changes on; a second one
        // would double every `preferred_changed`.
        let feedback = self.feedback.clone()?;
        let desc = feedback.get_preferred(&qh, ());
        trace(format_args!(
            "colour feedback={} description={}",
            wayland_client::Proxy::version(&feedback),
            wayland_client::Proxy::version(&desc)
        ));
        self.state.preferred_luminances = None;
        // First trip delivers ready; details arrive on the next trip.
        let _ = self.queue.roundtrip(&mut self.state);
        let _ = self.queue.roundtrip(&mut self.state);
        let out = self.state.preferred_luminances;
        if out.is_none() {
            trace(format_args!(
                "preferred unavailable, failed={:?}",
                self.state.colour_failed
            ));
        }
        desc.destroy();
        out
    }

    /// Matrix converting our sRGB content into the output's own gamut, row-major.
    pub fn output_gamut_matrix(&self) -> [[f32; 3]; 3] {
        let target = self
            .state
            .preferred_chromaticities
            .unwrap_or(ui_core::colour::Chromaticities::BT2020);
        // Measured: with Bradford the HDR and SDR paths match to 1/255 in one channel,
        // so this compositor does not adapt white points itself. Kept switchable as a
        // control variable for other compositors.
        let mode = if std::env::var("CRABSNAP_NO_WHITE_ADAPT").is_ok() {
            ui_core::colour::WhiteAdaptation::None
        } else {
            ui_core::colour::WhiteAdaptation::Bradford
        };
        ui_core::colour::gamut_matrix_adapted(ui_core::colour::Chromaticities::SRGB, target, mode)
    }

    /// Whether colour management was available at all.
    pub fn has_color_management(&self) -> bool {
        self.color_manager.is_some()
    }

    pub fn dmabuf_version(&self) -> Option<u32> {
        self.dmabuf.as_ref().map(|d| d.version())
    }

    /// Format/modifier pairs the compositor advertised for dma-buf import.
    pub fn dmabuf_formats(&self) -> &DmaBufFormats {
        &self.state.dmabuf_formats
    }

    /// dma-buf buffers the compositor has released so far.
    pub fn dmabuf_releases(&self) -> u32 {
        self.state.dmabuf_released
    }

    /// dma-buf imports the compositor has rejected so far.
    pub fn dmabuf_rejections(&self) -> u32 {
        self.state.dmabuf_rejected
    }

    /// dma-buf buffers still held by the compositor.
    pub fn dmabuf_inflight(&self) -> usize {
        self.state.dmabuf_inflight.len()
    }

    /// Mark that the UI changed and wants a redraw.
    pub fn request_redraw(&mut self) {
        self.state.redraw_requested = true;
    }

    /// Block until the compositor reports the frame callback done.
    pub fn wait_for_frame(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while !self.state.frame_ready {
            // A closing toplevel gets no more callbacks: waiting for one would hold the
            // close for the whole deadline, which is exactly what a slow exit looked like.
            if self.state.closed {
                return;
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                // A compositor that never answers must not deadlock the app.
                self.state.frame_ready = true;
                break;
            }
            // Poll, never `blocking_dispatch`: that one sits in `read` until *some* event
            // arrives, so an omitted frame callback would hold the app past every deadline.
            self.poll_once(deadline - now);
            // A paste read can finish while a frame is in flight.
            self.service_clipboard();
        }
    }

    /// Pump pending events without blocking.
    pub fn pump(&mut self) {
        let _ = self.drain_events();
    }

    /// Pump pending events, surfacing protocol errors instead of discarding them.
    pub fn pump_checked(&mut self) -> Result<(), WindowError> {
        self.drain_events()
    }

    /// Read what the socket already holds, then dispatch it; never block.
    fn drain_events(&mut self) -> Result<(), WindowError> {
        let protocol = |e: String| WindowError::Protocol(e);
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| protocol(e.to_string()))?;
        // `dispatch_pending` never reads the socket; only `blocking_dispatch` does.
        if let Some(guard) = self.queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(protocol(e.to_string())),
            }
        }
        self.queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| protocol(e.to_string()))?;
        self.queue.flush().map_err(|e| protocol(e.to_string()))
    }

    /// Move a layer surface: the margin places it against the edges it is anchored to.
    ///
    /// A margin is surface state, so it only takes effect on a commit; the content is left
    /// alone, which is what lets a floating image follow the hand without a new buffer.
    pub fn set_layer_margin(&mut self, margin: Insets) {
        let Some(layer) = self.layer.as_ref() else {
            return;
        };
        layer.set_margin(
            margin.top.round() as i32,
            margin.right.round() as i32,
            margin.bottom.round() as i32,
            margin.left.round() as i32,
        );
        self.surface.commit();
    }

    /// Resize a layer surface; the compositor answers with a configure carrying the new size.
    ///
    /// The configured size lags the request, so both are remembered: asking again for a size
    /// already in flight would commit the same state every frame.
    pub fn set_layer_size(&mut self, size: (u32, u32)) {
        let Some(layer) = self.layer.as_ref() else {
            return;
        };
        if (self.state.width, self.state.height) == size || self.layer_size_request == Some(size) {
            return;
        }
        layer.set_size(size.0, size.1);
        self.layer_size_request = Some(size);
        self.surface.commit();
    }

    /// Apply the shape the frame resolved. Returns whether anything was sent.
    ///
    /// Flushed immediately: a still pointer produces no further events to carry the
    /// request, so waiting for the next dispatch would leave the shape one step late.
    pub fn set_cursor(&mut self, shape: CursorShape) -> bool {
        let Some(device) = self.cursor_device.as_ref() else {
            return false;
        };
        let Some(serial) = self.state.pointer_enter_serial else {
            return false;
        };
        if self.state.cursor_shape == Some(shape) {
            return false;
        }
        device.set_shape(serial, crate::cursor_shape::protocol_shape(shape));
        self.state.cursor_shape = Some(shape);
        let _ = self._conn.flush();
        true
    }

    /// Remember the IME state the engine resolved; it is sent with the next frame's commit.
    pub fn set_ime(&mut self, state: ui_platform::present::ImeState) {
        self.ime = Some(state);
    }

    /// Whether the compositor still needs this frame's IME state committed.
    ///
    /// A focus re-entry counts even when nothing changed: the compositor forgot the session.
    ///
    /// Outside a session nothing is owed: there is no surface to send it for.
    pub fn ime_pending(&self) -> bool {
        self.state.ime_entered
            && (self.state.ime_reenter || self.ime.as_ref() != self.ime_sent.as_ref())
    }

    /// Send the IME state, if it changed, in the same batch as the surface commit.
    ///
    /// Enabling always resends surrounding, content type and cursor rectangle: `enable`
    /// resets the compositor's copy, so a diff alone would leave it with stale values.
    fn apply_ime(&mut self) {
        // Outside a session nothing may be sent: `enable` before the compositor's `enter` is
        // a protocol violation, and a stale one hands it the keyboard with no owner.
        if !self.state.ime_entered {
            return;
        }
        let Some(state) = self.ime.as_ref() else {
            return;
        };
        // A fresh focus ends the compositor's session but leaves its own "enabled" flag set,
        // and it reactivates the input method only on a real transition — so the old session
        // is closed first, or the re-enable would be a no-op and CJK would stay dead.
        if std::mem::take(&mut self.state.ime_reenter) {
            if self.ime_sent.as_ref().is_some_and(|sent| sent.enabled)
                && let Some(ti) = self.text_input.as_ref()
            {
                ti.disable();
                ti.commit();
            }
            self.ime_sent = None;
        }
        if self.ime_sent.as_ref() == Some(state) {
            return;
        }
        let Some(ti) = self.text_input.as_ref() else {
            return;
        };
        let was_enabled = self.ime_sent.as_ref().is_some_and(|s| s.enabled);
        if state.enabled {
            // Rule 1: never enable without a caret rectangle from this very frame.
            let Some(rect) = state.cursor else {
                return;
            };
            if !was_enabled {
                // `enable` starts a fresh session that resets everything, so it goes first
                // and the state below follows in a second commit.
                ti.enable();
                ti.commit();
            }
            // Surrounding text and content type only when they changed (or on enable):
            // resending the surrounding text on every preedit character makes the input
            // method re-sync its composition on each keystroke.
            let changed = self.ime_sent.as_ref().map(|p| p.surrounding.as_ref())
                != Some(state.surrounding.as_ref());
            if !was_enabled || changed {
                // A secret buffer sends none, and an empty statement clears whatever the
                // previous input left registered with the method.
                let (text, caret, anchor) = state.surrounding.clone().unwrap_or_default();
                ti.set_surrounding_text(text, caret as i32, anchor as i32);
            }
            let content_changed = self
                .ime_sent
                .as_ref()
                .is_none_or(|p| (p.hidden, p.purpose) != (state.hidden, state.purpose));
            if !was_enabled || content_changed {
                let (hint, purpose) = content_type(state.hidden, state.purpose);
                ti.set_content_type(hint, purpose);
            }
            ti.set_cursor_rectangle(
                rect.origin.x.round() as i32,
                rect.origin.y.round() as i32,
                rect.size.w.max(1.0).round() as i32,
                rect.size.h.max(1.0).round() as i32,
            );
        } else {
            ti.disable();
        }
        // A disable needs its commit too; without one the compositor keeps the old state.
        ti.commit();
        // One copy, and only on the frame that actually sends something.
        self.ime_sent = self.ime.clone();
    }

    /// Offer `content` as the seat's selection. Returns whether the compositor took it.
    pub fn write_clipboard(&mut self, content: ui_core::clipboard::ClipboardContent) -> bool {
        use crate::clipboard::Contents;
        use ui_core::clipboard::ClipboardContent;
        let contents = match content {
            ClipboardContent::Text(text) => Contents::Text(text),
            ClipboardContent::Bytes { mime, data } => Contents::Bytes { mime, data },
        };
        self.state.clipboard.copy(contents, &self._conn, &self.qh)
    }

    /// The serial a `set_selection` needs: the latest input event this window saw.
    pub fn input_serial(&self) -> u32 {
        self.state.clipboard.serial()
    }

    /// Ask the seat's selection for its text; it arrives as `Event::Paste`.
    pub fn request_paste(&mut self) -> bool {
        self.state.clipboard.request_paste(&self._conn)
    }

    /// Move a paste transfer forward without waiting.
    pub fn service_paste(&mut self) {
        self.service_clipboard();
    }

    /// When a paste transfer in flight must be serviced again.
    pub fn paste_deadline(&self) -> Option<std::time::Instant> {
        self.state
            .clipboard
            .timeout()
            .map(|delay| std::time::Instant::now() + delay)
    }

    /// Move clipboard pipes forward, queueing a finished paste as an event.
    fn service_clipboard(&mut self) {
        if let Some(text) = self.state.clipboard.service() {
            self.state.events.push(ui_core::Event::Paste(text));
            self.state.redraw_requested = true;
        }
    }

    /// Block until at least one event arrives.
    ///
    /// A pending clipboard pipe must be polled alongside the socket, or a paste would
    /// wait for unrelated input before its bytes are read.
    pub fn wait(&mut self) {
        let _ = self.wait_until(None);
    }

    /// Block until an event arrives or `timeout` passes; true when events are waiting.
    ///
    /// Bounded even with no deadline given: an app-owned deadline can only be noticed
    /// between frames, and an unbounded read would swallow it until unrelated input.
    pub fn wait_until(&mut self, timeout: Option<std::time::Duration>) -> bool {
        let cap = std::time::Duration::from_millis(250);
        let mut timeout = timeout
            .unwrap_or(cap)
            .min(std::time::Duration::from_secs(1));
        if let Some(t) = self.state.clipboard.timeout() {
            timeout = timeout.min(t);
        }
        self.poll_once(timeout);
        self.service_clipboard();
        let _ = self.drain_events();
        !self.state.events.is_empty()
    }

    /// Sleep at most `timeout`, waking for either the socket or a clipboard pipe.
    fn poll_once(&mut self, timeout: std::time::Duration) {
        let Some(guard) = self.queue.prepare_read() else {
            return;
        };
        let conn_fd = guard.connection_fd();
        let pipes = self.state.clipboard.fds();
        let mut fds: Vec<rustix::event::PollFd<'_>> = Vec::with_capacity(1 + pipes.len());
        fds.push(rustix::event::PollFd::from_borrowed_fd(
            conn_fd,
            rustix::event::PollFlags::IN,
        ));
        for (fd, write) in &pipes {
            let want = if *write {
                rustix::event::PollFlags::OUT
            } else {
                rustix::event::PollFlags::IN
            };
            fds.push(rustix::event::PollFd::from_borrowed_fd(*fd, want));
        }
        let deadline = rustix::event::Timespec {
            tv_sec: timeout.as_secs() as i64,
            tv_nsec: timeout.subsec_nanos() as i64,
        };
        let ready = rustix::event::poll(&mut fds, Some(&deadline)).unwrap_or(0);
        let conn_ready = ready > 0 && fds[0].revents() != rustix::event::PollFlags::empty();
        drop(fds);
        // Dropping the guard cancels the read; the socket keeps its bytes for `drain_events`.
        drop(guard);
        if conn_ready {
            let _ = self.drain_events();
        }
    }

    /// Allocate fresh buffers when the compositor changed the size.
    fn resize_with_callback(&mut self) {
        // Buffers are logical size * scale; the UI itself lays out in logical units.
        let (lw, lh) = (self.state.width, self.state.height);
        let scale = self.scale();
        let (w, h) = device_size((lw, lh), scale);
        if w == self.pixels.width && h == self.pixels.height {
            return;
        }
        self.pixels = Pixels::new(w, h);
        // Old buffers belong to the previous size; drop them and make two fresh ones.
        self.front = None;
        self.back = self
            .state
            .shm
            .as_ref()
            .and_then(|shm| ShmBuffer::new(shm, &self.qh, w, h));
        self.pool_fresh = true;
    }

    /// Map a scale-times-larger buffer back onto the logical surface size.
    fn push_viewport(&mut self) {
        if let Some(viewport) = &self.viewport {
            let target = (self.state.width, self.state.height);
            if self.viewport_dirty || self.viewport_size != target {
                viewport.set_destination(target.0 as i32, target.1 as i32);
                self.viewport_size = target;
                self.viewport_dirty = false;
            }
        }
    }

    /// Drop the cached buffers: a fresh dma-buf allocation can reuse a released inode.
    pub fn forget_dmabuf_cache(&mut self) {
        for (_, cached) in self.dmabuf_cache.drain() {
            cached.destroy();
        }
    }

    /// Hand over the acquire and release timeline fds; both are required by the protocol.
    pub fn attach_sync_timelines(
        &mut self,
        acquire_fd: std::os::fd::OwnedFd,
        release_fd: std::os::fd::OwnedFd,
    ) -> bool {
        let Some(mgr) = self.syncobj_manager.as_ref() else {
            return false;
        };
        let acquire = mgr.import_timeline(acquire_fd.as_fd(), &self.qh, ());
        let release = mgr.import_timeline(release_fd.as_fd(), &self.qh, ());
        let surface = mgr.get_surface(&self.surface, &self.qh, ());
        self.syncobj_timeline = Some(acquire);
        self.syncobj_release_timeline = Some(release);
        self.syncobj_surface = Some(surface);
        true
    }

    /// Present an existing dma-buf image for the next frame instead of the shm pixels.
    pub fn present_dmabuf(
        &mut self,
        image: &DmaBufImage,
        damage: &Damage,
        acquire_point: Option<u64>,
        release_point: u64,
    ) -> Result<(), WindowError> {
        let Some(dmabuf) = self.dmabuf.as_ref() else {
            return Err(WindowError::DmaBuf(DmaBufError::Unavailable));
        };
        // Reuse one `wl_buffer` per dma-buf: the compositor imports the buffer object once
        // instead of rebuilding it every frame.
        let (dev, ino) =
            dmabuf::image_identity(image).ok_or(WindowError::DmaBuf(DmaBufError::Fd))?;
        let key = (
            dev,
            ino,
            image.width,
            image.height,
            image.stride,
            image.offset,
            image.format,
            image.modifier,
        );
        let buffer = match self.dmabuf_cache.get(&key) {
            Some(cached) => cached.buffer().clone(),
            None => {
                let created = dmabuf::create_dmabuf_buffer(
                    dmabuf,
                    &self.state.dmabuf_formats,
                    image,
                    &self.qh,
                )
                .map_err(WindowError::DmaBuf)?;
                let handle = created.buffer().clone();
                self.dmabuf_cache.insert(key, created);
                handle
            }
        };
        self.push_viewport();
        // Logical rects scale into buffer pixels; a partial frame must still report some
        // damage or the compositor keeps the old pixels there.
        let s = self.scale();
        let full =
            ui_core::geom::Rect::new(0.0, 0.0, image.width as f32 / s, image.height as f32 / s);
        let mut reported = false;
        for r in damage.rects(full).iter() {
            let x = (r.origin.x * s).floor() as i32;
            let y = (r.origin.y * s).floor() as i32;
            let w = (r.size.w * s).ceil() as i32;
            let h = (r.size.h * s).ceil() as i32;
            if w <= 0 || h <= 0 {
                continue;
            }
            self.surface.damage_buffer(x, y, w, h);
            reported = true;
        }
        if !reported {
            self.surface
                .damage_buffer(0, 0, image.width as i32, image.height as i32);
        }
        self.surface.attach(Some(&buffer), 0, 0);
        // The acquire point must be in the same commit as the buffer it guards.
        if let (Some(surface), Some(acquire_tl), Some(release_tl), Some(point)) = (
            self.syncobj_surface.as_ref(),
            self.syncobj_timeline.as_ref(),
            self.syncobj_release_timeline.as_ref(),
            acquire_point,
        ) {
            surface.set_acquire_point(acquire_tl, (point >> 32) as u32, point as u32);
            let rp = release_point;
            surface.set_release_point(release_tl, (rp >> 32) as u32, rp as u32);
        }
        self.surface.frame(&self.qh, ());
        self.state.frame_ready = false;
        self.state.redraw_requested = false;
        // Same batch as the surface commit: v1 places the candidate window at the
        // rectangle in force when text-input commits (ime-text-design §6).
        self.apply_ime();
        self.surface.commit();
        self.input_pending = false;
        // Holding the buffer here is what keeps it alive until `release` arrives.
        self.state.dmabuf_inflight.push(buffer);
        let _ = self.queue.flush();
        Ok(())
    }

    /// Publish the frame's consumer areas as this surface's input region.
    ///
    /// Only a layer surface has one to publish: a toplevel's shape belongs to the window
    /// manager, and the protocol default (the whole surface) is what it already has. The
    /// region is pending surface state, so a frame that draws carries it; a frame that draws
    /// nothing reports [`input_pending`](Self::input_pending) instead and the engine
    /// presents one anyway.
    pub fn set_input_areas(&mut self, areas: &[ui_core::geom::Rect]) {
        if self.layer.is_none() || areas == self.input_areas.as_slice() {
            return;
        }
        let region = self.compositor.create_region(&self.qh, ());
        for area in areas {
            let x = area.origin.x.floor() as i32;
            let y = area.origin.y.floor() as i32;
            let width = area.size.w.ceil() as i32;
            let height = area.size.h.ceil() as i32;
            if width > 0 && height > 0 {
                region.add(x, y, width, height);
            }
        }
        self.surface.set_input_region(Some(&region));
        // Copy semantics: the compositor keeps the rectangles, so the object can go now.
        region.destroy();
        self.input_areas = areas.to_vec();
        self.input_pending = true;
    }

    /// Whether an input region was published since the surface last committed.
    pub fn input_pending(&self) -> bool {
        self.input_pending
    }

    /// Put the keyboard on `inside` for a layer surface that follows the pointer.
    ///
    /// Lowering the request to `None` is the only way a client hands the keyboard back: the
    /// compositor re-evaluates a focused layer's surface when the commit carrying `None`
    /// arrives, and not otherwise. Raising it back needs the same trick in reverse — the
    /// commit from `Exclusive` to `OnDemand` is what makes Hyprland re-run its pointer hit
    /// test (`LayerSurface::onCommit`), and a layer that was never exclusive has no such
    /// transition. Both requests are sent here, each with its own commit, because the
    /// intermediate value has to be the committed one to be seen.
    pub fn apply_pointer_inside(&mut self, inside: bool) {
        let Some(layer) = self.layer.as_ref() else {
            return;
        };
        if !self.keyboard_follow {
            return;
        }
        let want = if inside {
            zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand
        } else {
            zwlr_layer_surface_v1::KeyboardInteractivity::None
        };
        if self.keyboard_sent == Some(want) {
            return;
        }
        if inside && !self.state.keyboard_focus {
            layer.set_keyboard_interactivity(
                zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
            );
            self.surface.commit();
        }
        layer.set_keyboard_interactivity(want);
        self.surface.commit();
        self.keyboard_sent = Some(want);
        let _ = self.queue.flush();
    }

    pub fn present_now(&mut self, damage: &Damage) -> Result<(), WindowError> {
        self.resize_with_callback();
        self.push_viewport();

        let Some(back) = self.back.as_mut() else {
            return Err(WindowError::Buffer);
        };
        back.copy_from(self.pixels.as_bytes());

        // A wl_buffer is a view over its pool; each slot has its own pool so a
        // still-attached buffer is never written again.
        let shm_buffer = back.pool.create_buffer(
            0,
            back.width as i32,
            back.height as i32,
            back.stride as i32,
            ShmBuffer::format(),
            &self.qh,
            (),
        );
        back.set_busy(true);
        self.surface
            .damage_buffer(0, 0, back.width as i32, back.height as i32);
        let _ = damage;
        self.surface.attach(Some(&shm_buffer), 0, 0);
        // One callback per commit: no second frame is drawn until this fires.
        self.surface.frame(&self.qh, ());
        self.state.frame_ready = false;
        self.state.redraw_requested = false;
        // Same batch as the surface commit; see `apply_ime`.
        self.apply_ime();
        self.surface.commit();
        self.input_pending = false;

        // Rotate the slots: the released previous-front becomes the new back.
        let old_front = self.front.take();
        let old_back = self.back.take().expect("back buffer");
        self.front = Some((shm_buffer, old_back));
        if let Some((buf, old_buf)) = old_front {
            buf.destroy();
            if old_buf.width == self.pixels.width && old_buf.height == self.pixels.height {
                self.back = Some(old_buf);
            } else {
                // Wrong size after a configure; a fresh one is allocated next frame.
                self.back = None;
            }
        } else {
            self.back = None;
        }
        if self.back.is_none() {
            self.back = self.state.shm.as_ref().and_then(|shm| {
                ShmBuffer::new(shm, &self.qh, self.pixels.width, self.pixels.height)
            });
        }
        let _ = self.queue.flush();
        Ok(())
    }
}

impl PresentTarget for WaylandWindow {
    fn kind(&self) -> SurfaceKind {
        if self.layer.is_some() {
            SurfaceKind::LayerSurface
        } else {
            SurfaceKind::Window
        }
    }

    fn size(&self) -> (u32, u32) {
        (self.pixels.width, self.pixels.height)
    }

    fn pixels_mut(&mut self) -> &mut Pixels {
        &mut self.pixels
    }

    fn present(&mut self, damage: &Damage) -> Result<(), String> {
        self.present_now(damage).map_err(|e| e.to_string())
    }

    fn set_cursor(&mut self, shape: CursorShape) {
        Self::set_cursor(self, shape);
    }

    fn set_input_region(&mut self, areas: &[ui_core::geom::Rect]) {
        Self::set_input_areas(self, areas);
    }

    fn set_pointer_inside(&mut self, inside: bool) {
        self.apply_pointer_inside(inside);
    }

    fn set_ime(&mut self, state: ui_platform::present::ImeState) {
        Self::set_ime(self, state);
    }
}

// Keep rarely used imports meaningful for future input work.
#[allow(dead_code)]
fn _keep(_: &dyn AsFd, _: &mut std::fs::File) {}

/// The window as the engine's driver sees it; an app never derives these rules itself.
impl ui_platform::driver::FrameHost for WaylandWindow {
    fn size(&self) -> (u32, u32) {
        device_size(self.logical_size(), self.scale())
    }
    fn scale(&self) -> f32 {
        WaylandWindow::scale(self)
    }
    fn pointer(&self) -> Option<ui_core::geom::Point> {
        WaylandWindow::pointer(self)
    }
    fn can_draw(&self) -> bool {
        WaylandWindow::can_draw(self)
    }
    fn wait_until(&mut self, timeout: Option<std::time::Duration>) -> bool {
        WaylandWindow::wait_until(self, timeout)
    }
    fn wait_for_frame(&mut self) {
        WaylandWindow::wait_for_frame(self)
    }
    fn should_close(&self) -> bool {
        WaylandWindow::should_close(self)
    }
    fn take_events(&mut self) -> Vec<ui_core::Event> {
        WaylandWindow::take_events(self)
    }
    fn take_silent_change(&mut self) -> bool {
        WaylandWindow::take_silent_change(self)
    }
}

/// The protocol's content type for one input: a secret buffer is a password whatever
/// purpose it named, and every other purpose goes through as itself.
fn content_type(
    hidden: bool,
    purpose: ui_core::ime::ContentPurpose,
) -> (
    zwp_text_input_v3::ContentHint,
    zwp_text_input_v3::ContentPurpose,
) {
    if hidden {
        return (
            zwp_text_input_v3::ContentHint::HiddenText
                | zwp_text_input_v3::ContentHint::SensitiveData,
            zwp_text_input_v3::ContentPurpose::Password,
        );
    }
    match purpose {
        ui_core::ime::ContentPurpose::Normal => (
            zwp_text_input_v3::ContentHint::None,
            zwp_text_input_v3::ContentPurpose::Normal,
        ),
        ui_core::ime::ContentPurpose::Time => (
            zwp_text_input_v3::ContentHint::None,
            zwp_text_input_v3::ContentPurpose::Time,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::device_size;

    #[test]
    fn device_size_rounds_fractional_scale_instead_of_truncating() {
        // 1.5 x 461 is 691.5; truncating to 691 would shift every row by half a pixel.
        assert_eq!(device_size((461, 200), 1.5), (692, 300));
    }

    #[test]
    fn device_size_never_returns_a_zero_sized_target() {
        // A zero-sized target fails allocation, so an empty window still asks for one pixel.
        assert_eq!(device_size((0, 0), 1.0), (1, 1));
    }

    /// A repeat belongs to the key that is down. A second key pressed beside it starts its
    /// own and leaves the first running, and only that key's release stops it — before the
    /// per-key table any press, a bare modifier included, took the repeat from the letter
    /// still held, so the pointer froze the moment Shift was pressed.
    #[test]
    fn holding_another_key_leaves_a_repeat_running() {
        use super::Repeats;
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let delay = Duration::from_millis(200);
        let period = Duration::from_millis(40);
        let mut repeats = Repeats::default();
        repeats.hold(30, start, delay);
        repeats.hold(42, start + Duration::from_millis(50), delay);
        assert_eq!(
            repeats.deadline(),
            Some(start + delay),
            "each key keeps the due time its own press gave it"
        );
        assert_eq!(repeats.due(start + delay, period), vec![30]);
        assert_eq!(
            repeats.due(start + delay + period, period),
            vec![30],
            "the second key repeating does not end the first"
        );
        repeats.release(42);
        assert_eq!(
            repeats.due(start + Duration::from_millis(400), period),
            vec![30]
        );
        repeats.release(30);
        assert_eq!(repeats.deadline(), None, "nothing is left to repeat");

        // A turn that was busy for many periods replays once and keeps the rhythm.
        let mut late = Repeats::default();
        late.hold(30, start, delay);
        assert_eq!(late.due(start + delay + period * 10, period), vec![30]);
        assert!(
            late.deadline()
                .is_some_and(|next| next > start + delay + period * 10),
            "the next period is the one after now, not a backlog"
        );
    }

    #[test]
    fn a_time_input_tells_the_method_it_takes_a_time() {
        use super::content_type;
        use super::zwp_text_input_v3::ContentHint;
        use super::zwp_text_input_v3::ContentPurpose as ProtocolPurpose;
        use ui_core::ime::ContentPurpose;

        assert_eq!(
            content_type(false, ContentPurpose::Time),
            (ContentHint::None, ProtocolPurpose::Time)
        );
        // A secret buffer outranks whatever purpose it named.
        assert_eq!(
            content_type(true, ContentPurpose::Time),
            (
                ContentHint::HiddenText | ContentHint::SensitiveData,
                ProtocolPurpose::Password
            )
        );
        assert_eq!(
            content_type(false, ContentPurpose::Normal),
            (ContentHint::None, ProtocolPurpose::Normal)
        );
    }
}
