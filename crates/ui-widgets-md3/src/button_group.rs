//! MD3 connected button group: a row of buttons sharing one pill and each other's edges.
//!
//! The group is the parent that knows, and it tells: it computes each member's
//! [`GroupPosition`] and hands the resolved corners down to the button it builds. A button
//! standing alone never sees a position, so neither the engine nor a plain button changes.

use ui_core::geom::{Color, Corners};
use ui_core::layout::SizeClass;
use ui_core::{Decl, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::{Cx, GroupPosition, Widget};

use crate::Md3Options;
use crate::button::{ButtonParams, Md3Button};
use crate::paint::role;
use crate::spec::{
    ButtonGroupConnectedFlags, ButtonGroupConnectedSize, ButtonSize, button_group_connected_spec,
};

/// One connected group: its members, in order, left to right (or top to bottom).
pub struct ButtonGroupParams {
    pub id: Id,
    /// The members, described but not built: the group builds them so the sizes match.
    pub members: Vec<ButtonParams>,
    pub size: ButtonGroupConnectedSize,
    /// Stack the members instead of lining them up.
    pub vertical: bool,
}

impl ButtonGroupParams {
    pub fn new(id: Id, members: Vec<ButtonParams>) -> Self {
        Self {
            id,
            members,
            size: ButtonGroupConnectedSize::Medium,
            vertical: false,
        }
    }

    pub fn size(mut self, size: ButtonGroupConnectedSize) -> Self {
        self.size = size;
        self
    }

    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }
}

/// The group's own surface, which shows through the gap between members.
fn group_surface(pack: &Style) -> Color {
    role(pack, "surfaceContainerLow")
}

/// The button size the group's height corresponds to: one scale for both.
fn button_size(size: ButtonGroupConnectedSize) -> ButtonSize {
    match size {
        ButtonGroupConnectedSize::XSmall => ButtonSize::XSmall,
        ButtonGroupConnectedSize::Small => ButtonSize::Small,
        ButtonGroupConnectedSize::Large => ButtonSize::Large,
        ButtonGroupConnectedSize::XLarge => ButtonSize::XLarge,
        _ => ButtonSize::Medium,
    }
}

/// One MD3 connected button group.
pub struct Md3ButtonGroup {
    id: Id,
    members: Vec<Md3Button>,
    height: f32,
    gap: f32,
    surface: Color,
    vertical: bool,
    disabled: bool,
}

impl Md3ButtonGroup {
    pub fn new_with(
        options: Md3Options,
        params: ButtonGroupParams,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let ButtonGroupParams {
            id,
            members,
            size,
            vertical,
        } = params;
        let state = cx.state(id);
        let len = members.len();
        // The export's `selected` flag means the group is showing a choice: that is a
        // member's own toggle, which the app has already stated on its parameters.
        let selected = members
            .iter()
            .any(|m| m.toggle == crate::ButtonToggle::Selected);
        // A press anywhere in the group is a press on the group: the shared edges round over.
        let pressed = state.pressed;
        let spec =
            button_group_connected_spec(ButtonGroupConnectedFlags { selected, pressed }, size);
        let corners_of = |i: usize| -> Corners {
            GroupPosition::new(i, len).corners(
                spec.container.height * 0.5,
                spec.root.corner_size.min(spec.container.height * 0.5),
            )
        };
        let resolved: Vec<Md3Button> = members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                Md3Button::new_with(
                    options,
                    ButtonParams {
                        size: button_size(size),
                        corners: Some(corners_of(i)),
                        ..m.clone()
                    },
                    cx,
                    pack,
                )
            })
            .collect();
        Self {
            id,
            members: resolved,
            height: spec.container.height,
            gap: spec.root.between_space,
            surface: group_surface(pack),
            vertical,
            disabled: options.disabled || state.disabled,
        }
    }

    /// The group's own height, so a caller can align what sits beside it.
    pub fn height(&self) -> f32 {
        self.height
    }
}

impl Widget for Md3ButtonGroup {
    fn handles_disabled(&self) -> bool {
        // The members fade themselves, exactly as a standalone disabled button does.
        true
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let kind = if self.vertical {
            NodeKind::Column
        } else {
            NodeKind::Row
        };
        // Both axes hug: the members carry their own sizes, and the group is exactly their sum.
        let mut row = Decl::container(self.id, kind)
            .size(SizeClass::hug(0.0, 0.0))
            .gap(self.gap)
            // The gap reads as the group's own surface: one pill divided, not pills apart.
            .background(self.surface, ui_core::geom::Corners::all(self.height * 0.5))
            // A group is a surface, never a target: every press belongs to a member.
            .sense(Sense::PASS_THROUGH)
            .disabled(self.disabled);
        for member in &self.members {
            row = row.child(member.decl(cx));
        }
        row
    }
}
