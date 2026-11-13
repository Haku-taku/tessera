//! The Fluent content dialog: a modal surface with a title, content and up to three actions.

use ui_core::anim::{AnimSpec, Easing};
use ui_core::geom::{Color, Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::tree::{ModalTarget, Presence};
use ui_core::{Change, Decl, Id, Key, Modifiers, NodeKind, Sense, Weight};
use ui_style::Style;
use ui_style::motion::WINUI_SPLINE;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::button::{ButtonParams, ButtonVariant, FluentButton};
use crate::paint::role_of;

/// `ContentDialogMinWidth` / `ContentDialogMaxWidth`.
pub const DIALOG_MIN_WIDTH: f32 = 320.0;
pub const DIALOG_MAX_WIDTH: f32 = 548.0;
/// `ContentDialogMinHeight` / `ContentDialogMaxHeight`.
pub const DIALOG_MIN_HEIGHT: f32 = 184.0;
pub const DIALOG_MAX_HEIGHT: f32 = 756.0;
/// `ContentDialogPadding`, `ContentDialogTitleMargin` and `ContentDialogButtonSpacing`.
pub const DIALOG_PAD: f32 = 24.0;
pub const DIALOG_TITLE_GAP: f32 = 12.0;
pub const DIALOG_BUTTON_SPACING: f32 = 8.0;
/// `ContentDialogSeparatorThickness` 0,0,0,1 and `ContentDialogBorderWidth` 1.
pub const DIALOG_SEPARATOR: f32 = 1.0;
pub const DIALOG_BORDER: f32 = 1.0;
/// `OverlayCornerRadius`.
pub const DIALOG_RADIUS: f32 = 8.0;
/// The template's `FontSize="20" FontWeight="SemiBold"`, and `ControlContentThemeFontSize`.
pub const DIALOG_TITLE_FONT: f32 = 20.0;
pub const DIALOG_CONTENT_FONT: f32 = 14.0;
/// The title's `MaxLines="2"`.
pub const DIALOG_TITLE_LINES: u32 = 2;
/// `DialogShowingStates`: opacity 0→1 on show and 1→0 on hide, at
/// `ControlFasterAnimationDuration` with a linear key frame.
pub const DIALOG_FADE: AnimSpec = AnimSpec::new(0.083, Easing::Linear);
/// `ControlNormalAnimationDuration` on `ControlFastOutSlowInKeySpline`, which is what makes
/// the dialog read as opening: the fade alone lands before the eye registers it.
pub const DIALOG_SCALE_IN: AnimSpec = AnimSpec::new(0.25, WINUI_SPLINE);
/// `ControlFastAnimationDuration`, the same curve: the scale leaves in 167 ms, behind the
/// 83 ms fade that has already made it invisible.
pub const DIALOG_SCALE_OUT: AnimSpec = AnimSpec::new(0.167, WINUI_SPLINE);
/// `ScaleTransform` at the hidden end, both directions.
pub const DIALOG_SCALE_FROM: f32 = 1.05;

/// Which action the dialog treats as its default, i.e. the accent one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentDialogButton {
    /// `ContentDialogButton.None`: no accent, which is the property's own default.
    #[default]
    None,
    Primary,
    Secondary,
    Close,
}

impl ContentDialogButton {
    fn accent(self) -> Option<ActionSlot> {
        match self {
            Self::None => None,
            Self::Primary => Some(ActionSlot::Primary),
            Self::Secondary => Some(ActionSlot::Secondary),
            Self::Close => Some(ActionSlot::Close),
        }
    }
}

/// The three columns the command area can fill, in the order the template lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionSlot {
    Primary,
    Secondary,
    Close,
}

impl ActionSlot {
    /// The slot's id suffix and the template's `x:Name` for its button.
    const fn tag(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Close => "close",
        }
    }
}

/// Everything a dialog is built with, named after the properties the reference exposes.
pub struct ContentDialogParams<'a> {
    pub id: Id,
    pub title: &'a str,
    /// `Content`: the body text; app blocks go in `content`, below it.
    pub body: &'a str,
    pub content: Vec<Decl>,
    /// `PrimaryButtonText` / `SecondaryButtonText` / `CloseButtonText`; empty hides a button.
    pub primary: &'a str,
    pub secondary: &'a str,
    pub close: &'a str,
    pub default_button: ContentDialogButton,
    /// `IsPrimaryButtonEnabled` / `IsSecondaryButtonEnabled`.
    pub primary_enabled: bool,
    pub secondary_enabled: bool,
    /// The region the smoke layer dims and blocks.
    pub target: ModalTarget,
    /// Whether the dialog dims and blocks what is behind it. Off draws the surface alone,
    /// which is what a gallery preview needs.
    pub barrier: bool,
}

impl<'a> ContentDialogParams<'a> {
    pub fn new(id: Id, title: &'a str, body: &'a str) -> Self {
        Self {
            id,
            title,
            body,
            content: Vec::new(),
            primary: "",
            secondary: "",
            close: "",
            default_button: ContentDialogButton::None,
            primary_enabled: true,
            secondary_enabled: true,
            target: ModalTarget::Global,
            barrier: true,
        }
    }

    pub fn primary(mut self, text: &'a str) -> Self {
        self.primary = text;
        self
    }

    pub fn secondary(mut self, text: &'a str) -> Self {
        self.secondary = text;
        self
    }

    pub fn close(mut self, text: &'a str) -> Self {
        self.close = text;
        self
    }

    pub fn default_button(mut self, button: ContentDialogButton) -> Self {
        self.default_button = button;
        self
    }

    pub fn primary_enabled(mut self, on: bool) -> Self {
        self.primary_enabled = on;
        self
    }

    pub fn secondary_enabled(mut self, on: bool) -> Self {
        self.secondary_enabled = on;
        self
    }

    pub fn content(mut self, blocks: Vec<Decl>) -> Self {
        self.content = blocks;
        self
    }

    pub fn target(mut self, target: ModalTarget) -> Self {
        self.target = target;
        self
    }

    pub fn barrier(mut self, barrier: bool) -> Self {
        self.barrier = barrier;
        self
    }
}

/// One resolved action: the button that draws it, and the id it reports under.
struct Action {
    /// Read only by the tests that pin which button took the accent.
    #[cfg_attr(not(test), allow(dead_code))]
    slot: ActionSlot,
    id: Id,
    button: FluentButton,
}

/// A content dialog, resolved against the pack in force where it was built.
pub struct FluentContentDialog {
    id: Id,
    title: Option<String>,
    body: Option<String>,
    content: Vec<Decl>,
    actions: Vec<Action>,
    target: ModalTarget,
    barrier: bool,
    surface: Color,
    stroke: Color,
    overlay: Color,
    separator: Color,
    smoke: Color,
    ink: Color,
    /// The action whose button took this frame's click.
    fired: Option<Id>,
    /// `DefaultButton`, which Enter presses whatever holds focus.
    default: ContentDialogButton,
}

/// Enter presses the default button; Escape presses the close button, as WinUI binds them.
pub const DIALOG_DEFAULT_SHORTCUT: ShortcutId = ShortcutId("dialog.default");
pub const DIALOG_CLOSE_SHORTCUT: ShortcutId = ShortcutId("dialog.close");

impl FluentContentDialog {
    pub fn new(params: ContentDialogParams<'_>, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let ContentDialogParams {
            id,
            title,
            body,
            content,
            primary,
            secondary,
            close,
            default_button,
            primary_enabled,
            secondary_enabled,
            target,
            barrier,
        } = params;
        let accent = default_button.accent();
        let mut actions = Vec::new();
        for (slot, text, enabled) in [
            (ActionSlot::Primary, primary, primary_enabled),
            (ActionSlot::Secondary, secondary, secondary_enabled),
            (ActionSlot::Close, close, true),
        ] {
            if text.is_empty() {
                continue;
            }
            let bid = id.with(slot.tag());
            let variant = if accent == Some(slot) {
                ButtonVariant::Accent
            } else {
                ButtonVariant::Default
            };
            // `HorizontalAlignment="Stretch"` on all three: each fills its grid column.
            let mut params = ButtonParams::new(bid, text).variant(variant).fill_width();
            if !enabled {
                params = params.disabled();
            }
            let mut node = cx.at(bid);
            actions.push(Action {
                slot,
                id: bid,
                button: FluentButton::new(params, &mut node, pack),
            });
        }
        let fired = actions
            .iter()
            .find(|a| cx.state(a.id).clicked)
            .map(|a| a.id);
        Self {
            id,
            title: (!title.is_empty()).then(|| title.to_string()),
            body: (!body.is_empty()).then(|| body.to_string()),
            content,
            actions,
            target,
            barrier,
            surface: role_of(pack, "ContentDialogBackground"),
            stroke: role_of(pack, "ContentDialogBorderBrush"),
            overlay: role_of(pack, "ContentDialogTopOverlay"),
            separator: role_of(pack, "ContentDialogSeparatorBorderBrush"),
            smoke: role_of(pack, "ContentDialogSmokeFill"),
            ink: role_of(pack, "ContentDialogForeground"),
            fired,
            default: default_button,
        }
    }

    /// Each action's fill, keyed by its slot tag, for the tests that pin the accent.
    #[cfg(test)]
    pub(crate) fn action_fills(&self) -> Vec<(&'static str, Color)> {
        self.actions
            .iter()
            .map(|a| (a.slot.tag(), a.button.fill()))
            .collect()
    }

    /// The width the content may wrap in: the surface minus its border and padding.
    fn wrap(&self) -> f32 {
        DIALOG_MAX_WIDTH - 2.0 * (DIALOG_BORDER + DIALOG_PAD)
    }

    /// One text band: the title or the body, wrapping inside the surface.
    fn text(
        &self,
        which: &'static str,
        body: &str,
        size: f32,
        weight: Weight,
        max_lines: Option<u32>,
    ) -> Decl {
        let mut text = Text::new(self.id.with(which), body.to_string(), size, self.ink)
            .weight(weight)
            .max_width(self.wrap());
        if let Some(lines) = max_lines {
            text = text.max_lines(lines);
        }
        text.resolve()
    }

    /// The `*` row: title and content on the `ContentDialogTopOverlay` tint, over a bottom
    /// separator. It grows, so the command area stays at the surface's bottom edge.
    fn content_decl(&self) -> Decl {
        // The grid is clipped by `DialogSpace`'s `CornerRadius`, so its tint takes the
        // surface's own top corners instead of squaring them off.
        let top = Corners {
            tl: DIALOG_RADIUS,
            tr: DIALOG_RADIUS,
            br: 0.0,
            bl: 0.0,
            shape: ui_core::geom::CornerShape::ROUND,
        };
        let mut area = Decl::container(self.id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            })
            .padding(Insets::all(DIALOG_PAD))
            .cross_align(1.0)
            .background(self.overlay, top)
            .relative();
        if let Some(title) = &self.title {
            area = area.child(
                self.text(
                    "title",
                    title,
                    DIALOG_TITLE_FONT,
                    Weight::SEMIBOLD,
                    Some(DIALOG_TITLE_LINES),
                )
                // `ContentDialogTitleMargin` 0,0,0,12.
                // `ContentDialogTitleMargin`: 0,0,0,12.
                .margin(Insets {
                    left: 0.0,
                    top: 0.0,
                    right: 0.0,
                    bottom: DIALOG_TITLE_GAP,
                }),
            );
        }
        if let Some(body) = &self.body {
            area = area.child(self.text("body", body, DIALOG_CONTENT_FONT, Weight::REGULAR, None));
        }
        for block in &self.content {
            area = area.child(block.clone());
        }
        // `ContentDialogSeparatorThickness` 0,0,0,1 sits on the grid's outer bottom edge.
        area = area.child(
            Decl::container(self.id.with("separator"), NodeKind::Box)
                .absolute()
                .inset(ui_core::layout::LenInsets::new(
                    ui_core::layout::Len::Px(0.0),
                    ui_core::layout::Len::Auto,
                    ui_core::layout::Len::Px(0.0),
                    ui_core::layout::Len::Px(0.0),
                ))
                .size(SizeClass {
                    width: AxisSize::hug(0.0, 0.0),
                    height: AxisSize::fixed(DIALOG_SEPARATOR),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .background(self.separator, Corners::ZERO),
        );
        area
    }

    /// `CommandSpace`: equal `*` columns with an 8 pt gap. One lone action still sits in
    /// the right half, because the empty primary column keeps its share.
    fn commands_decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let mut row = Decl::container(self.id.with("commands"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::all(DIALOG_PAD))
            .gap(DIALOG_BUTTON_SPACING)
            .cross_align(0.0)
            .cross_pos(0.5);
        if self.actions.len() == 1 {
            row = row.child(
                Decl::container(self.id.with("commands-lead"), NodeKind::Column).size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::Split(1),
                    grow_y: Grow::No,
                }),
            );
        }
        for action in &self.actions {
            row = row.child(action.button.decl(cx));
        }
        row
    }

    /// `BackgroundElement`: the surface itself, sized to its content between 320×184 and
    /// 548×756, with the border outside the content.
    fn surface_decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let corners = Corners::all(DIALOG_RADIUS);
        let mut column = Decl::container(self.id.with("surface"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(DIALOG_MIN_WIDTH, DIALOG_MAX_WIDTH),
                height: AxisSize::hug(DIALOG_MIN_HEIGHT, DIALOG_MAX_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // `BackgroundSizing="InnerBorderEdge"`: the border takes its point out of the
            // 320 pt box, which is why the 320 pt dialog's command area is 318 wide.
            .padding(Insets::all(DIALOG_BORDER))
            .cross_align(1.0)
            .corners(corners)
            .background(self.surface, corners)
            .border(self.stroke, DIALOG_BORDER)
            .child(self.content_decl());
        if !self.actions.is_empty() {
            column = column.child(self.commands_decl(cx));
        }
        column
    }
}

/// A zero-basis grower: it takes a share of the spare height and draws nothing.
fn spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

impl Widget for FluentContentDialog {
    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        // Enter presses the default button and Escape the close one, wherever focus sits.
        let slot = match state.shortcut? {
            DIALOG_CLOSE_SHORTCUT => ActionSlot::Close,
            DIALOG_DEFAULT_SHORTCUT => self.default.accent()?,
            _ => return None,
        };
        let action = self.actions.iter().find(|a| a.slot == slot)?;
        Some(Change::Selected(Some(action.id)))
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let id = self.id;
        // The app declares the dialog only while it is open; the engine holds the subtree
        // for the way out, so `DialogShowingStates` is a pair of declarations, not a state
        // the app has to keep alive across frames.
        if !self.barrier {
            return enter_and_leave(self.surface_decl(cx));
        }
        // `LayoutRoot`: the smoke layer, and the surface centred on both axes. A modal root
        // is placed on its target's rect, so the declared size never reaches the screen.
        let mut root = Decl::container(id, NodeKind::Column)
            .size(SizeClass::hug(0.0, 0.0))
            .modal(self.target)
            // The smoke layer takes the pointer so nothing behind the dialog does.
            .sense(Sense::HOVER)
            .cross_align(0.0)
            .cross_pos(0.5)
            .background(self.smoke, Corners::ZERO);
        // Only the modal owns keys: an inline sample would otherwise claim Enter whenever
        // the pointer crossed it, and Enter belongs to whoever holds the keyboard.
        if self.barrier && !self.actions.is_empty() {
            root = root
                .shortcut(
                    KeyChord::new(Key::Enter, Modifiers::NONE),
                    DIALOG_DEFAULT_SHORTCUT,
                )
                .shortcut(
                    KeyChord::new(Key::Escape, Modifiers::NONE),
                    DIALOG_CLOSE_SHORTCUT,
                );
        }
        // Two equal growers centre the surface: the engine has no main-axis position.
        root = root
            .child(spacer(id.with("pad-top")))
            // The surface carries the scale channel alone: WinUI scales `BackgroundElement`
            // while the smoke layer stays put and only fades.
            .child(
                self.surface_decl(cx)
                    .enter(Presence::fade(DIALOG_SCALE_IN, 1.0).scaled(DIALOG_SCALE_FROM)),
            )
            .child(spacer(id.with("pad-bottom")));
        // The root owns the opacity channel: the smoke comes and goes with the surface, and
        // a held subtree drives only its own root's exit.
        root.enter(Presence::fade(DIALOG_FADE, 0.0))
            .exit(Presence::fade(DIALOG_FADE, 0.0))
    }
}

/// Both channels on the one node a barrier-less dialog is.
fn enter_and_leave(surface: Decl) -> Decl {
    surface
        .enter(Presence::fade(DIALOG_FADE, 0.0).scaled_over(DIALOG_SCALE_FROM, DIALOG_SCALE_IN))
        .exit(Presence::fade(DIALOG_FADE, 0.0).scaled_over(DIALOG_SCALE_FROM, DIALOG_SCALE_OUT))
}
