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

use ui_core::geom::{Insets, Rect};
use ui_core::layout::{OverlayAlign, Side};

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
    /// The smallest client area a user may size a window to, in logical units.
    ///
    /// A layer is placed rather than sized, so it ignores this. The platform enforces it in
    /// its own resize path; a programmatic size still wins, as it must for a surface the app
    /// itself shrinks.
    pub min_size: Option<(u32, u32)>,
}

impl SurfaceSpec {
    /// A window the window manager places, decorates and stacks.
    pub fn window(app_id: impl Into<String>, title: impl Into<String>, size: (u32, u32)) -> Self {
        Self {
            app_id: app_id.into(),
            name: title.into(),
            role: SurfaceRole::Window { size },
            min_size: None,
        }
    }

    /// Declare the smallest client area a user may size this window to.
    pub fn with_min_size(mut self, size: (u32, u32)) -> Self {
        self.min_size = Some(size);
        self
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
            min_size: None,
        }
    }

    /// A secondary window belonging to another surface: an alert, a settings panel, a picker.
    ///
    /// It is opened through [`SurfaceHandle::open_dialog`](crate::host::SurfaceHandle::open_dialog)
    /// on the surface it belongs to, never as a top-level surface of its own. The platform
    /// stacks it above that surface and keeps it there; `modal` asks the platform to hold
    /// the parent's input until the dialog is answered.
    pub fn dialog(app_id: impl Into<String>, title: impl Into<String>, size: (u32, u32)) -> Self {
        Self {
            app_id: app_id.into(),
            name: title.into(),
            role: SurfaceRole::Dialog { size, modal: true },
            min_size: None,
        }
    }

    /// Whether this dialog blocks its parent's input. Meaningful only for a dialog.
    pub fn with_modal(mut self, modal: bool) -> Self {
        if let SurfaceRole::Dialog { modal: current, .. } = &mut self.role {
            *current = modal;
        }
        self
    }

    /// The dialog's modality, when this surface is a dialog.
    pub fn dialog_modal(&self) -> Option<bool> {
        match &self.role {
            SurfaceRole::Dialog { modal, .. } => Some(*modal),
            _ => None,
        }
    }

    /// The layer spec, when this surface is one.
    pub fn layer_spec(&self) -> Option<&LayerSpec> {
        match &self.role {
            SurfaceRole::Layer(layer) => Some(layer),
            _ => None,
        }
    }
}

/// Who places the surface.
#[derive(Clone, Debug)]
pub enum SurfaceRole {
    /// The window manager places, decorates and stacks it; `size` is its opening size in
    /// logical pixels.
    Window { size: (u32, u32) },
    /// A secondary window of another surface, opened through the surface it belongs to.
    ///
    /// The platform stacks it above that surface and closes it with it; `modal` asks for
    /// the parent's input to be held until the dialog is answered. A platform that cannot
    /// express the relation is what [`SurfaceHandle::open_dialog`] refuses.
    ///
    /// [`SurfaceHandle::open_dialog`]: crate::host::SurfaceHandle::open_dialog
    Dialog { size: (u32, u32), modal: bool },
    /// The engine places it: no decoration, no window-manager layout.
    Layer(LayerSpec),
}

/// Where a layer surface goes and how it takes input.
///
/// The fields a platform cannot honour degrade rather than fail: a surface still opens, at the
/// closest placement that platform has.
#[derive(Clone, Debug)]
pub struct LayerSpec {
    /// Which output it is placed on, by the name [`Backend::outputs`](crate::host::Backend::outputs)
    /// reports; `None` lets the platform choose.
    ///
    /// A per-output surface — an overlay over this monitor, a canvas for its pins — must name
    /// its output, or it lands wherever the platform puts a new surface. A backend that does not
    /// act on the name ignores it, and the surface opens where it otherwise would.
    pub output: Option<String>,
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
    pub auto_hide: AutoHide,
}

/// What a layer surface does about the strip an auto-hidden shell bar keeps for itself.
///
/// A bar that hides when the pointer leaves is revealed by the pointer reaching the screen
/// edge, and the shell stops revealing it while a window covers that edge. A platform may
/// therefore stop a layer surface short of the strip. A surface whose picture is the screen
/// cannot give those rows up without drawing everything above them a little off where the
/// user points, so it covers the edge instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoHide {
    /// Stop short of the edge: the strip stays the shell's, and its bar stays reachable.
    #[default]
    Yield,
    /// Cover the edge: the surface is exactly the output its anchors name.
    Cover,
}

impl Default for LayerSpec {
    fn default() -> Self {
        Self {
            output: None,
            level: Layer::Overlay,
            anchor: Anchor::ALL,
            size: None,
            margin: Insets::ZERO,
            exclusive: ExclusiveZone::Ignore,
            keyboard: KeyboardMode::None,
            auto_hide: AutoHide::Yield,
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

/// What a platform popup asks for: a rectangle, an anchor, and what to do at an edge.
///
/// Every coordinate is in the parent presentation's space. The engine lays the panel out in
/// the parent tree before the surface exists, so `rect` is that subtree's rect plus whatever
/// paints outside it, and no second layout follows. Its size is never changed by a constraint
/// adjustment, and its origin is where the panel's own coordinates start: a platform that
/// routes the popup's input back into the tree shifts each position by it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupSpec {
    /// The laid-out panel: the surface's size, and the tree position it stands for.
    pub rect: Rect,
    /// The rectangle the panel hangs from, in the parent presentation's coordinates.
    pub anchor: Rect,
    /// The preferred unfolding side.
    pub side: Side,
    /// How the panel lines up with the anchor along the other axis.
    pub align: OverlayAlign,
    /// Gap between the anchor and the panel.
    pub offset: f32,
    pub constraint: Constraint,
}

/// What a platform may do when a popup's preferred position does not fit.
///
/// The set is `xdg_positioner`'s, so a Wayland backend forwards it instead of interpreting
/// it. The tree has already applied [`place_popup`]'s rule against the output it was given,
/// so a backend that replays it lands on the rect the panel was laid out at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Constraint {
    /// Mirror to the anchor's other side.
    pub flip_x: bool,
    pub flip_y: bool,
    /// Move along the axis until the popup fits, covering part of the anchor.
    pub slide_x: bool,
    pub slide_y: bool,
}

impl Constraint {
    /// Leave the request alone; the output edge cuts the panel.
    pub const NONE: Self = Self {
        flip_x: false,
        flip_y: false,
        slide_x: false,
        slide_y: false,
    };

    /// Flip first, then slide. `resize` is absent on purpose: the size is the engine's
    /// layout result, and a platform that shrank it would cut content.
    pub const FLIP_AND_SLIDE: Self = Self {
        flip_x: true,
        flip_y: true,
        slide_x: true,
        slide_y: true,
    };
}

impl Default for Constraint {
    fn default() -> Self {
        Self::FLIP_AND_SLIDE
    }
}

/// Where a popup lands: the preferred placement, adjusted against `bounds`.
///
/// `bounds` is the space the popup must stay inside, in the same coordinates as
/// [`PopupSpec::anchor`]: a backend converts its monitor's work area into the parent's client
/// space before calling. The popup's size never changes. The rule itself is the layout's, so
/// a backend that replays it lands where the tree placed the panel.
pub fn place_popup(spec: &PopupSpec, bounds: Rect) -> Rect {
    let vertical = matches!(spec.side, Side::Below | Side::Above);
    let (flip_along, flip_across) = if vertical {
        (spec.constraint.flip_y, spec.constraint.flip_x)
    } else {
        (spec.constraint.flip_x, spec.constraint.flip_y)
    };
    ui_core::layout::place_anchored(
        spec.anchor,
        spec.rect.size,
        spec.side,
        spec.align,
        spec.offset,
        ui_core::layout::Fit {
            flip_along,
            flip_across,
            slide_x: spec.constraint.slide_x,
            slide_y: spec.constraint.slide_y,
        },
        bounds,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::geom::{Point, Size};

    fn spec(anchor: Rect, size: (u32, u32), constraint: Constraint) -> PopupSpec {
        PopupSpec {
            rect: Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32),
            anchor,
            side: Side::Below,
            align: OverlayAlign::Start,
            offset: 0.0,
            constraint,
        }
    }

    /// The case the design was written around: an anchor 40 px above the output's bottom
    /// edge, whose panel cannot fit below. Flipping keeps the anchor visible; sliding would
    /// cover it. The gap is preserved on the side it flips to.
    #[test]
    fn a_panel_at_the_bottom_flips_before_it_slides() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let mut request = spec(Rect::new(100.0, 720.0, 200.0, 40.0), (300, 300), Constraint::default());
        assert_eq!(
            place_popup(&request, bounds),
            Rect::new(100.0, 420.0, 300.0, 300.0)
        );

        request.offset = 8.0;
        assert_eq!(
            place_popup(&request, bounds),
            Rect::new(100.0, 412.0, 300.0, 300.0),
            "the gap belongs on whichever side the panel unfolds"
        );
    }

    /// With flipping refused, the panel slides up and covers the anchor instead of leaving
    /// the output. This is the branch a platform without flip support takes.
    #[test]
    fn a_panel_without_flip_slides_to_the_edge() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let request = spec(
            Rect::new(100.0, 720.0, 200.0, 40.0),
            (300, 300),
            Constraint {
                flip_x: false,
                flip_y: false,
                slide_x: true,
                slide_y: true,
            },
        );
        assert_eq!(
            place_popup(&request, bounds),
            Rect::new(100.0, 500.0, 300.0, 300.0)
        );
    }

    /// A panel that cannot fit at all is pinned to the output's top-left rather than
    /// centered on a negative offset or left with an arbitrary overflow.
    #[test]
    fn a_panel_larger_than_the_bounds_pins_its_origin() {
        let bounds = Rect::new(50.0, 60.0, 200.0, 200.0);
        let request = spec(Rect::new(60.0, 70.0, 10.0, 10.0), (300, 300), Constraint::default());
        let placed = place_popup(&request, bounds);
        assert_eq!(placed.origin, Point::new(50.0, 60.0));
        assert_eq!(placed.size, Size::new(300.0, 300.0), "the size never changes");
    }

    /// `NONE` is the platform saying it has no positioner and no space to adjust: the request
    /// stands, even though the panel is cut by the output edge.
    #[test]
    fn no_constraint_leaves_the_request_where_it_was() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let request = spec(Rect::new(100.0, 720.0, 200.0, 40.0), (300, 300), Constraint::NONE);
        assert_eq!(
            place_popup(&request, bounds),
            Rect::new(100.0, 760.0, 300.0, 300.0)
        );
    }

    /// At the right edge the panel has no room to the right, and there is no other side of
    /// the anchor along x: flipping the alignment moves its far edge onto the anchor's.
    #[test]
    fn an_aligned_panel_flips_its_alignment_at_the_right_edge() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let request = spec(Rect::new(800.0, 100.0, 100.0, 40.0), (300, 100), Constraint::default());
        assert_eq!(
            place_popup(&request, bounds),
            Rect::new(600.0, 140.0, 300.0, 100.0)
        );
    }

    /// A centered panel has no other alignment to flip to, so the cross-axis flip is skipped
    /// and the slide does the work.
    #[test]
    fn a_centered_panel_slides_because_it_has_no_second_alignment() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 800.0);
        let mut request = spec(Rect::new(900.0, 100.0, 100.0, 40.0), (300, 100), Constraint::default());
        request.align = OverlayAlign::Center;
        assert_eq!(
            place_popup(&request, bounds),
            Rect::new(700.0, 140.0, 300.0, 100.0)
        );
    }
}
