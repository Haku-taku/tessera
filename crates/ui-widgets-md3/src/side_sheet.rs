//! MD3 side sheet: a panel docked to one window edge, in the page or over a scrim.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets, Point, Rect};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::{ModalTarget, Presence, ShadowSpec};
use ui_core::{Change, CursorShape, Decl, Id, NodeKind, Sense, TextAlign};
use ui_style::Style;
use ui_widgets::text::Text;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::bottom_sheet::SHEET_SCRIM;
use crate::dialog::{DialogAction, ENTER, LEAVE};
use crate::paint::spec_ink;
use crate::spec::{SideSheetFlags, SideSheetForm, side_sheet_spec};
use crate::typescale::{self, TypeStyle};
use crate::{ButtonParams, ButtonSize, ButtonStyle, Md3Button, Md3Options};

/// `corner.large`: the modal form's inner corners. The M3 side sheet page states 16 dp.
pub const SIDE_SHEET_RADIUS: f32 = 16.0;
/// The export's stretch of surface either side of the content.
const SIDE_SHEET_PAD: f32 = 24.0;
/// The space between the headline and the content the app supplies.
const HEADER_GAP: f32 = 12.0;
/// The action row's insets: 16 dp above the buttons, 24 dp below them.
const ACTION_TOP: f32 = 16.0;
const ACTION_BOTTOM: f32 = 24.0;
const ACTION_GAP: f32 = 8.0;

/// Which window edge the sheet is docked to; the inner edge is the opposite one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetEdge {
    Start,
    End,
}

/// Everything a side sheet is built with. The form fixes the colour and the corner.
pub struct SideSheetParams {
    pub id: Id,
    pub form: SideSheetForm,
    pub edge: SheetEdge,
    pub headline: Option<String>,
    /// The app's own blocks, below the headline.
    pub content: Vec<Decl>,
    /// Text buttons at the bottom of the surface, left-aligned.
    pub actions: Vec<DialogAction>,
    /// Whether the sheet dims and blocks what is behind it.
    pub barrier: bool,
    /// The region the scrim covers — the whole window, or the panel this sheet belongs to.
    pub target: ModalTarget,
}

impl SideSheetParams {
    /// A modal sheet on the trailing edge, with neither headline nor actions.
    pub fn new(id: Id, edge: SheetEdge) -> Self {
        Self {
            id,
            form: SideSheetForm::Modal,
            edge,
            headline: None,
            content: Vec::new(),
            actions: Vec::new(),
            barrier: true,
            target: ModalTarget::Global,
        }
    }

    pub fn form(mut self, form: SideSheetForm) -> Self {
        self.form = form;
        self
    }

    pub fn headline(mut self, headline: impl Into<String>) -> Self {
        self.headline = Some(headline.into());
        self
    }

    pub fn content(mut self, content: Vec<Decl>) -> Self {
        self.content = content;
        self
    }

    pub fn actions(mut self, actions: Vec<DialogAction>) -> Self {
        self.actions = actions;
        self
    }

    pub fn barrier(mut self, barrier: bool) -> Self {
        self.barrier = barrier;
        self
    }

    pub fn target(mut self, target: ModalTarget) -> Self {
        self.target = target;
        self
    }
}

/// A side sheet: the surface is fixed, so only the actions and the barrier are live.
pub struct Md3SideSheet {
    id: Id,
    surface_id: Id,
    edge: SheetEdge,
    modal: bool,
    barrier: bool,
    target: ModalTarget,
    headline: Option<String>,
    content: Vec<Decl>,
    actions: Vec<Md3Button>,
    width: f32,
    surface: Color,
    shadow: Option<ShadowSpec>,
    title_style: TypeStyle,
    title_ink: Color,
    divider_ink: Color,
    scrim: Color,
    /// The action whose button took this frame's click.
    fired: Option<Id>,
    /// Whether this frame's click landed on the scrim rather than inside the surface.
    dismiss: bool,
}

impl Md3SideSheet {
    /// Resolve the surface and what the user did to it this frame.
    pub fn new_with(params: SideSheetParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let SideSheetParams {
            id,
            form,
            edge,
            headline,
            content,
            actions,
            barrier,
            target,
        } = params;
        let spec = side_sheet_spec(SideSheetFlags::NONE, form);
        let surface_id = id.with("surface");
        let fired = actions
            .iter()
            .find(|a| cx.state(a.id).activation())
            .map(|a| a.id);
        let dismiss = barrier && cx.state(id).clicked && !cx.state(surface_id).clicked;
        let resolved = actions
            .into_iter()
            .map(|action| {
                Md3Button::new_with(
                    Md3Options::default(),
                    ButtonParams::new(action.id, action.label)
                        .style(ButtonStyle::Text)
                        .size(ButtonSize::Base),
                    cx,
                    pack,
                )
            })
            .collect();
        Self {
            id,
            surface_id,
            edge,
            modal: form == SideSheetForm::Modal,
            barrier,
            target,
            headline,
            content,
            actions: resolved,
            width: spec.container.width,
            surface: spec_ink(pack, spec.container.color, 1.0),
            shadow: (spec.container.elevation > 0.0)
                .then(|| ShadowSpec::elevation(spec.container.elevation as u8)),
            title_style: typescale::named(spec.headline.type_),
            title_ink: spec_ink(pack, spec.headline.color, 1.0),
            divider_ink: spec_ink(pack, spec.divider.color, 1.0),
            scrim: spec_ink(pack, "scrim", SHEET_SCRIM),
            fired,
            dismiss,
        }
    }

    /// The body: headline, the app's blocks, then the actions pushed to the bottom.
    fn body_decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let bottom = if self.actions.is_empty() {
            SIDE_SHEET_PAD
        } else {
            0.0
        };
        let mut column = Decl::container(self.surface_id.with("body"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: SIDE_SHEET_PAD,
                top: SIDE_SHEET_PAD,
                right: SIDE_SHEET_PAD,
                bottom,
            })
            .cross_align(0.0);
        if let Some(headline) = &self.headline {
            column = column.child(
                Text::new(
                    self.surface_id.with("headline"),
                    headline.clone(),
                    self.title_style.size,
                    self.title_ink,
                )
                .weight(self.title_style.weight)
                .max_width((self.width - 2.0 * SIDE_SHEET_PAD).max(0.0))
                .align(TextAlign::Start)
                .resolve(),
            );
            if !self.content.is_empty() {
                column = column.child(spacer(self.surface_id.with("header-gap")));
            }
        }
        for block in &self.content {
            column = column.child(block.clone());
        }
        column = column.child(grower(self.surface_id.with("push")));
        if !self.actions.is_empty() {
            let mut row = Decl::container(self.surface_id.with("actions"), NodeKind::Row)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .padding(Insets {
                    left: 0.0,
                    top: ACTION_TOP,
                    right: 0.0,
                    bottom: ACTION_BOTTOM,
                })
                .gap(ACTION_GAP)
                .cross_align(0.0)
                .cross_pos(0.0);
            for action in &self.actions {
                row = row.child(action.decl(cx));
            }
            column = column.child(row);
        }
        column
    }

    /// The surface: the body and the line on its inner edge, rounded on that edge only.
    fn surface_decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let corners = if self.modal {
            match self.edge {
                SheetEdge::Start => Corners {
                    tl: 0.0,
                    tr: SIDE_SHEET_RADIUS,
                    br: SIDE_SHEET_RADIUS,
                    bl: 0.0,
                    ..Corners::ZERO
                },
                SheetEdge::End => Corners {
                    tl: SIDE_SHEET_RADIUS,
                    tr: 0.0,
                    br: 0.0,
                    bl: SIDE_SHEET_RADIUS,
                    ..Corners::ZERO
                },
            }
        } else {
            Corners::ZERO
        };
        let divider = Decl::container(self.surface_id.with("divider"), NodeKind::Box)
            .size(SizeClass {
                width: AxisSize::fixed(1.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background(self.divider_ink, Corners::ZERO);
        let body = self.body_decl(cx);
        let mut surface = Decl::container(self.surface_id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            })
            .cross_align(1.0)
            .shapes(vec![Shape::Box {
                corners,
                fill: self.surface,
                gradient: None,
                border: None,
                shadow: self.shadow,
                image: None,
            }]);
        surface = match self.edge {
            // The inner edge is the one facing the page: the right edge on the start side.
            SheetEdge::Start => surface.child(body).child(divider),
            SheetEdge::End => surface.child(divider).child(body),
        };
        surface
    }
}

/// A zero-basis grower: it takes the space left over and draws nothing.
fn grower(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

/// A fixed gap between two blocks of the header.
fn spacer(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass::fixed(0.0, HEADER_GAP))
}

impl Widget for Md3SideSheet {
    fn handles_disabled(&self) -> bool {
        // A sheet has no disabled state; the adapter must not fade it either.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        self.dismiss.then_some(Change::Cancelled)
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let surface = self.surface_decl(cx);
        if !self.barrier {
            return surface;
        }
        // The scrim subscribes: a press outside the surface must reach `change()`, and a
        // modal sheet blocks the page instead of letting it take the press.
        let mut root = Decl::container(self.id, NodeKind::Column)
            // Measured as its own content; the layout then places it on the target rect.
            .size(SizeClass::hug(0.0, 0.0))
            .modal(self.target)
            .cross_align(0.0)
            .cross_pos(match self.edge {
                SheetEdge::Start => 0.0,
                SheetEdge::End => 1.0,
            })
            .sense(Sense::CLICK)
            .cursor(CursorShape::Arrow)
            .shapes(vec![Shape::Box {
                corners: Corners::ZERO,
                fill: self.scrim,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }]);
        let travel = match self.edge {
            SheetEdge::Start => -self.width,
            SheetEdge::End => self.width,
        };
        let surface = surface
            .enter(Presence::slide(ENTER, 0.0, Point::new(travel, 0.0)))
            .exit(Presence::slide(LEAVE, 0.0, Point::new(travel, 0.0)));
        root = root
            .child(surface)
            .enter(Presence::fade(ENTER, 0.0))
            .exit(Presence::fade(LEAVE, 0.0));
        root
    }
}
