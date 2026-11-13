//! What a host asks for when it opens a surface.
//!
//! One description covers a window the platform's window manager places and a layer the engine
//! places. A layer is `zwlr_layer_shell_v1`'s model — GTK apps reach the same protocol through
//! `gtk4-layer-shell`, whose vocabulary (`layer`, `anchor`, `margin`, `exclusive zone`,
//! `keyboard mode`, `namespace`) is the one used here.
//!
//! A platform without layer surfaces approximates the role with an undecorated window held at
//! the level the spec names: `WS_EX_TOPMOST` plus `WS_EX_TOOLWINDOW` on Windows,
//! `NSWindow.level` on macOS, and the equivalent of winit's `WindowLevel` on X11. Those
//! platforms have no compositor-side anchoring, so the backend there must turn `anchor`,
//! `margin` and `size` into a rectangle on the output itself; `exclusive` and `keyboard` have
//! no counterpart and are ignored, or approximated with "does not activate" window styles.

use ui_core::geom::Insets;

/// What a host asks for when it opens a surface.
#[derive(Clone, Debug)]
pub struct SurfaceSpec {
    /// The application, as the platform names it: `app_id` on Wayland, the window class on
    /// X11, the bundle identifier on macOS, the AppUserModelID on Windows.
    pub app_id: String,
    /// The surface's own name: a window's title, or a layer's namespace. Compositor rules
    /// match layer namespaces, so those conventionally start with the app id.
    pub name: String,
    pub role: SurfaceRole,
}

impl SurfaceSpec {
    /// A window the window manager places, decorates and stacks.
    pub fn window(app_id: impl Into<String>, title: impl Into<String>, size: (u32, u32)) -> Self {
        Self {
            app_id: app_id.into(),
            name: title.into(),
            role: SurfaceRole::Window { size },
        }
    }

    /// A layer the engine places; see [`LayerSpec`].
    pub fn layer(
        app_id: impl Into<String>,
        namespace: impl Into<String>,
        layer: LayerSpec,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            name: namespace.into(),
            role: SurfaceRole::Layer(layer),
        }
    }

    /// The layer spec, when this surface is one.
    pub fn layer_spec(&self) -> Option<&LayerSpec> {
        match &self.role {
            SurfaceRole::Layer(layer) => Some(layer),
            SurfaceRole::Window { .. } => None,
        }
    }
}

/// Who places the surface.
#[derive(Clone, Debug)]
pub enum SurfaceRole {
    /// The window manager places, decorates and stacks it; `size` is its opening size in
    /// logical pixels.
    Window { size: (u32, u32) },
    /// The engine places it: no decoration, no window-manager layout.
    Layer(LayerSpec),
}

/// Where a layer surface goes and how it takes input.
///
/// The fields a platform cannot honour degrade rather than fail: a surface still opens, at the
/// closest placement that platform has.
#[derive(Clone, Debug)]
pub struct LayerSpec {
    /// The stack it is drawn in; later layers cover earlier ones.
    pub level: Layer,
    /// The edges it is pinned to; pinned to all four it takes the whole output.
    pub anchor: Anchor,
    /// Logical size, or `None` to take whatever the anchors leave.
    pub size: Option<(u32, u32)>,
    /// Space between the surface and each edge it is anchored to, in logical pixels. With a
    /// single-edge anchor this *is* the position.
    pub margin: Insets,
    pub exclusive: ExclusiveZone,
    pub keyboard: KeyboardMode,
}

impl Default for LayerSpec {
    fn default() -> Self {
        Self {
            level: Layer::Overlay,
            anchor: Anchor::ALL,
            size: None,
            margin: Insets::ZERO,
            exclusive: ExclusiveZone::Ignore,
            keyboard: KeyboardMode::None,
        }
    }
}

/// Which stack a layer surface is drawn in; later layers cover earlier ones.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    Background,
    Bottom,
    #[default]
    Top,
    Overlay,
}

/// Which edges a layer surface is pinned to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Anchor {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Anchor {
    /// All four edges: the surface fills the output.
    pub const ALL: Self = Self {
        top: true,
        bottom: true,
        left: true,
        right: true,
    };
}

/// Whether the layer reserves room in the output's layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExclusiveZone {
    /// Not part of the layout at all: a floating image or a transient overlay must not push
    /// other windows around.
    #[default]
    Ignore,
    /// Keep other windows this many logical pixels away from the anchored edge — a panel.
    Reserve(i32),
}

/// How much of the keyboard a layer surface asks for.
///
/// Wayland calls this keyboard interactivity; no other platform has the concept, where the
/// closest equivalent is whether the window can take focus at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardMode {
    /// Never: the surface only shows.
    #[default]
    None,
    /// When the user clicks it. A floating image wants this: it may be dismissed with the
    /// keyboard, but it must not take the keyboard away from the desktop until asked.
    OnDemand,
    /// Always, while it is mapped: a capture overlay needs its Escape at once.
    Exclusive,
    /// While the pointer is over an area this surface consumes, or a gesture it owns is
    /// live; `None` otherwise, which is what hands the keyboard back to whatever is below.
    ///
    /// A surface that covers the screen but claims only patches of it needs this: keyboard
    /// focus is the compositor's, and it holds until the client asks for less. A pin canvas
    /// wants it — typing under the canvas belongs to the window under the pointer.
    FollowPointer,
}
