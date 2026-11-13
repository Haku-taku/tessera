//! MD3 dialog: a modal surface — icon, title, body, and the actions the app owns.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::{ModalTarget, Presence, ShadowSpec};
use ui_core::{AnimSpec, Change, CursorShape, Decl, Id, NodeKind, Sense, TextAlign};
use ui_style::Style;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource, Widget, WidgetState};

use crate::paint::spec_ink;
use crate::spec::{DialogFlags, DialogIconSlot, dialog_spec};
use crate::typescale::{self, TypeStyle};
use crate::{ButtonParams, ButtonSize, ButtonStyle, Md3Button, Md3Options};

/// `_DialogDefaultsM3.shape`: one 28dp corner on every side.
pub const DIALOG_RADIUS: f32 = 28.0;
/// `_DialogDefaultsM3.elevation`, which here is only the shadow.
pub const DIALOG_ELEVATION: f32 = 6.0;
/// `_alertDialogPadding`: the side inset every band shares.
pub const DIALOG_SIDE_PAD: f32 = 24.0;
/// AlertDialog's `constraints`: a dialog never reads narrower than this.
pub const DIALOG_MIN_WIDTH: f32 = 280.0;
/// `showDialog`'s dim behind the surface: `Colors.black54`.
pub const SCRIM_OPACITY: f32 = 0.54;
/// The gap between two actions: `buttonPadding.horizontal / 2`.
pub const ACTION_GAP: f32 = 8.0;
/// M3's suggested pair for entering is emphasized decelerate at 400 ms, not the 200 ms
/// its accelerate sibling uses; at 200 ms three quarters of the fade lands in 50 ms.
pub(crate) const ENTER: AnimSpec = AnimSpec::new(0.4, ui_style::motion::EMPHASIZED_DECELERATE);
/// The dialog's exit: emphasized accelerate at 200 ms, the spec's pair for leaving.
pub(crate) const LEAVE: AnimSpec = AnimSpec::new(0.2, crate::motion::EMPHASIZED_ACCELERATE);

/// What sits above the title; the export gives it a colour only for the with-icon case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogIcon {
    None,
    /// A mark the app builds at the size and ink the dialog hands it.
    Slot,
}

/// One action of the dialog, named by the id the app gave it.
pub struct DialogAction {
    pub id: Id,
    pub label: String,
}

/// Everything a dialog is built with. Icon, actions and width are fixed at build time —
/// a running dialog never gains one — so they are parameters, not state.
pub struct DialogContent {
    pub id: Id,
    pub title: Option<String>,
    pub body: Option<String>,
    /// The app's own blocks, between the body and the actions.
    pub content: Vec<Decl>,
    pub actions: Vec<DialogAction>,
    pub icon: DialogIcon,
    /// Resolves the icon when `icon` is `Slot`; the dialog hands it the ink it resolved.
    pub build_icon: Option<IconSource>,
    /// The surface's own width; the app knows what its content needs.
    pub width: f32,
    /// The region the scrim covers — the whole window, or the panel this dialog belongs to.
    ///
    /// The engine resolves it from the tree after layout, so the app names the choice
    /// instead of measuring anything.
    pub target: ModalTarget,
    /// Whether the dialog dims and blocks what is behind it. Off draws the surface alone.
    pub barrier: bool,
}

/// One resolved action: the button that draws it. Its id rides on the button.
struct Action {
    button: Md3Button,
}

/// A dialog: a surface that holds the app's own content between its bands.
pub struct Md3Dialog {
    id: Id,
    surface_id: Id,
    barrier: bool,
    target: ModalTarget,
    width: f32,
    title: Option<String>,
    body: Option<String>,
    content: Vec<Decl>,
    actions: Vec<Action>,
    icon: Option<(IconSource, Color)>,
    surface: Color,
    shadow: Option<ShadowSpec>,
    title_style: TypeStyle,
    title_ink: Color,
    body_style: TypeStyle,
    body_ink: Color,
    scrim: Color,
    /// With an icon every line centres, without one every line starts at the inset.
    centred: bool,
    disabled: bool,
    /// The action whose button took this frame's click.
    fired: Option<Id>,
    /// Whether this frame's click landed on the scrim rather than inside the surface.
    dismiss: bool,
}

impl Md3Dialog {
    /// Build one dialog.
    pub fn new_with(
        params: Md3Options,
        content: DialogContent,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let DialogContent {
            id,
            title,
            body,
            content: extra,
            actions,
            icon,
            build_icon,
            width,
            target,
            barrier,
        } = content;
        let surface_id = id.with("surface");
        let node = cx.at(id);
        let state = node.state();
        let disabled = params.disabled || state.disabled;
        let slot = match icon {
            DialogIcon::None => DialogIconSlot::Base,
            DialogIcon::Slot => DialogIconSlot::WithIcon,
        };
        // The surface itself has no state layer — the export's `state-layer` belongs to the
        // action labels, which the buttons resolve from their own tokens.
        let spec = dialog_spec(DialogFlags::NONE, slot);
        let fade = if disabled {
            crate::DISABLED_CONTENT_OPACITY
        } else {
            1.0
        };
        // A click is a one-frame fact: read it here, since `change()` sees no context.
        let fired = actions
            .iter()
            .find(|a| cx.state(a.id).activation())
            .map(|a| a.id);
        let dismiss = barrier && cx.state(id).clicked && !cx.state(surface_id).clicked;
        let resolved: Vec<Action> = actions
            .iter()
            .map(|action| Action {
                button: Md3Button::new_with(
                    Md3Options { disabled, ..params },
                    ButtonParams::new(action.id, action.label.clone())
                        .style(ButtonStyle::Text)
                        .size(ButtonSize::Base),
                    cx,
                    pack,
                ),
            })
            .collect();
        let icon = match (icon, build_icon) {
            (DialogIcon::Slot, Some(build)) => Some((build, spec_ink(pack, spec.icon.color, fade))),
            _ => None,
        };
        Self {
            id,
            surface_id,
            barrier,
            target,
            width: width.max(DIALOG_MIN_WIDTH),
            title,
            body,
            content: extra,
            actions: resolved,
            centred: icon.is_some(),
            icon,
            surface: spec_ink(pack, spec.container.color, 1.0),
            // MD3's elevation is a 1dp ambient+caster pair; one analytic shadow stands in.
            shadow: (spec.container.elevation > 0.0).then(|| {
                ShadowSpec::black(
                    spec.container.elevation * 2.0,
                    0.18,
                    spec.container.elevation * 0.5,
                )
            }),
            title_style: typescale::named(spec.headline.type_),
            title_ink: spec_ink(pack, spec.headline.color, fade),
            body_style: typescale::named(spec.supporting_text.type_),
            body_ink: spec_ink(pack, spec.supporting_text.color, fade),
            // `showDialog`'s barrier is `Colors.black54`: opaque would hide the very content
            // the dialog is asking about.
            scrim: spec_ink(pack, "scrim", SCRIM_OPACITY),
            disabled,
            fired,
            dismiss,
        }
    }

    /// The width the text may wrap in: the surface minus its side insets.
    fn wrap(&self) -> f32 {
        (self.width - 2.0 * DIALOG_SIDE_PAD).max(0.0)
    }

    /// One text band: the headline and the body wrap inside the surface, not the window.
    fn text(&self, which: &str, body: &str, style: TypeStyle, ink: Color) -> Decl {
        Text::new(self.id.with(which), body.to_string(), style.size, ink)
            .weight(style.weight)
            .max_width(self.wrap())
            .align(if self.centred {
                TextAlign::Center
            } else {
                TextAlign::Start
            })
            .resolve()
    }

    /// One band: its own insets, and how its children sit across the surface. Text bands
    /// stretch so every line wraps at the same width; a mark or a row hugs and is placed.
    fn band(&self, which: &str, pad: Insets, cross_align: f32, cross_pos: f32) -> Decl {
        Decl::container(self.id.with(which), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(pad)
            .cross_align(cross_align)
            .cross_pos(cross_pos)
    }

    /// The surface: bands at their own insets, the whole thing rounded as one shape.
    fn surface_decl(&self, cx: &Cx<'_, '_>) -> Decl {
        // Flutter's AlertDialog insets, per band: an icon keeps 16dp below itself only when a
        // title follows, the title keeps 20dp below itself only when nothing follows.
        let mut bands: Vec<Decl> = Vec::new();
        if let Some((source, ink)) = self.icon {
            let bottom = if self.title.is_some() {
                16.0
            } else if self.body.is_some() || !self.content.is_empty() {
                0.0
            } else {
                24.0
            };
            let pad = Insets {
                left: DIALOG_SIDE_PAD,
                top: 24.0,
                right: DIALOG_SIDE_PAD,
                bottom,
            };
            bands.push(
                self.band("icon-box", pad, 0.0, 0.5)
                    .child(source.resolve(self.id.with("icon"), ink)),
            );
        }
        if let Some(title) = &self.title {
            let top = if self.icon.is_some() { 0.0 } else { 24.0 };
            let bottom = if self.body.is_some() || !self.content.is_empty() {
                0.0
            } else {
                20.0
            };
            let pad = Insets {
                left: DIALOG_SIDE_PAD,
                top,
                right: DIALOG_SIDE_PAD,
                bottom,
            };
            bands.push(self.band("headline-box", pad, 1.0, 0.0).child(self.text(
                "headline",
                title,
                self.title_style,
                self.title_ink,
            )));
        }
        if let Some(body) = &self.body {
            let pad = Insets {
                left: DIALOG_SIDE_PAD,
                top: 16.0,
                right: DIALOG_SIDE_PAD,
                bottom: 24.0,
            };
            bands.push(self.band("body-box", pad, 1.0, 0.0).child(self.text(
                "body",
                body,
                self.body_style,
                self.body_ink,
            )));
        }
        if !self.content.is_empty() {
            let pad = Insets {
                left: DIALOG_SIDE_PAD,
                top: 16.0,
                right: DIALOG_SIDE_PAD,
                bottom: 24.0,
            };
            let mut band = self.band("content-box", pad, 1.0, 0.0).gap(12.0);
            for block in &self.content {
                band = band.child(block.clone());
            }
            bands.push(band);
        }
        if !self.actions.is_empty() {
            let pad = Insets {
                left: DIALOG_SIDE_PAD,
                top: 0.0,
                right: DIALOG_SIDE_PAD,
                bottom: 24.0,
            };
            // OverflowBar: when the labels cannot share one line, stack them at the far end
            // instead of letting them crush. No measurer means no answer, so keep the row.
            let avail = self.width - DIALOG_SIDE_PAD * 2.0;
            let mut needed = ACTION_GAP * (self.actions.len() as f32 - 1.0);
            let mut measured = true;
            for action in &self.actions {
                match action.button.width_in(cx) {
                    Some(w) => needed += w,
                    None => {
                        measured = false;
                        break;
                    }
                }
            }
            let stacked = measured && needed > avail;
            let mut row = if stacked {
                Decl::container(self.id.with("actions"), NodeKind::Column)
                    .gap(ACTION_GAP)
                    .cross_align(0.0)
                    .cross_pos(1.0)
            } else {
                Decl::container(self.id.with("actions"), NodeKind::Row)
                    .gap(ACTION_GAP)
                    .cross_align(0.0)
                    .cross_pos(0.5)
            };
            for action in &self.actions {
                row = row.child(action.button.decl(cx));
            }
            // OverflowBar alignment: the actions sit at the far end of their band.
            bands.push(self.band("actions-box", pad, 0.0, 1.0).child(row));
        }
        // Without a barrier the surface *is* the widget, so it carries the widget's own id;
        // with one the widget is the scrim and the surface is a child of it.
        let root_id = if self.barrier {
            self.surface_id
        } else {
            self.id
        };
        let mut column = Decl::container(root_id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(1.0)
            .disabled(self.disabled)
            .shapes(vec![Shape::Box {
                corners: Corners::all(DIALOG_RADIUS),
                fill: self.surface,
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }]);
        for band in bands {
            column = column.child(band);
        }
        column
    }
}

/// A zero-basis grower: it takes a share of the spare space and draws nothing.
fn spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

impl Widget for Md3Dialog {
    fn handles_disabled(&self) -> bool {
        // The 38% fade is resolved per band, so the adapter must not fade the subtree again.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if self.disabled {
            return None;
        }
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        self.dismiss.then_some(Change::Cancelled)
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let surface = enter_and_leave(self.surface_decl(cx));
        if !self.barrier {
            return surface;
        }
        // The scrim subscribes: a press outside the surface must reach `change()`, and MD3
        // blocks the content behind a dialog rather than letting it take the press.
        let mut root = Decl::container(self.id, NodeKind::Column)
            // Measured as its own content; the layout then places it on the target rect.
            .size(SizeClass::hug(0.0, 0.0))
            .modal(self.target)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::CLICK)
            .cursor(CursorShape::Arrow)
            .disabled(self.disabled)
            .shapes(vec![Shape::Box {
                corners: Corners::ZERO,
                fill: self.scrim,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }]);
        // Two equal growers centre the surface: the engine has no main-axis position.
        root = root
            .child(spacer(self.id.with("pad-top")))
            .child(surface)
            .child(spacer(self.id.with("pad-bottom")));
        // Only the scrim fades on its own: the surface it contains already scales.
        root.enter(Presence::fade(ENTER, 0.0))
            .exit(Presence::fade(LEAVE, 0.0))
    }
}

/// MD3's enter/exit pair for a surface that interrupts: it grows and fades as one unit.
///
/// Emphasized decelerate in, emphasized accelerate out — the spec's pairing for a small
/// surface, at the short end because a dialog is small and exits are quicker than entries.
fn enter_and_leave(decl: Decl) -> Decl {
    decl.enter(Presence::fade(ENTER, 0.0).scaled(0.8))
        .exit(Presence::fade(LEAVE, 0.0))
}
