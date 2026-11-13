//! MD3 modal bottom sheet: a bottom-anchored surface with a drag handle over a scrim.

use ui_core::draw::Shape;
use ui_core::geom::{Corners, Insets, Rect};
use ui_core::layout::{AxisSize, Grow, OverlayAlign, SizeClass};
use ui_core::tree::{ModalTarget, Presence, ShadowSpec};
use ui_core::{Change, CursorShape, Decl, Id, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::{DRAG_FOLLOW, IMMEDIATE};
use ui_widgets::{Cx, Widget, WidgetState};

use crate::dialog::{ENTER, LEAVE};
use crate::paint::spec_ink;
use crate::spec::{SheetForm, sheet_spec};

/// `BottomSheetDefaults.SheetMaxWidth`: a sheet stops widening here.
pub const SHEET_MAX_WIDTH: f32 = 640.0;
/// MD3's extra-large corner, on the top two corners only.
pub const SHEET_RADIUS: f32 = 28.0;
/// `DragHandleVerticalPadding`: the space above the handle.
pub const SHEET_HANDLE_TOP: f32 = 22.0;
/// The same padding below the handle, before the app's content.
pub const SHEET_HANDLE_GAP: f32 = 22.0;
/// `ScrimTokens.ContainerOpacity`, the dim the modal drawer also uses.
pub const SHEET_SCRIM: f32 = 0.32;
/// The settle spring: the drawer's pair, slow enough to read as weight.
const SNAP_K: f32 = 220.0;
const SNAP_C: f32 = 30.0;

/// Everything a sheet is built with; the surface owns none of it.
pub struct BottomSheetParams {
    pub id: Id,
    /// The app's blocks, below the drag handle.
    pub content: Vec<Decl>,
    /// The content's own inset; the sheet adds none of its own.
    pub padding: Insets,
    /// Ids inside `content` whose press the sheet reports.
    pub presses: Vec<Id>,
    /// Whether a drag past half the height, or a press outside, closes it.
    pub dismissible: bool,
    /// Whether the sheet dims and blocks what is behind it.
    pub barrier: bool,
    /// The region the scrim covers — the whole window, or the panel this sheet belongs to.
    pub target: ModalTarget,
}

impl BottomSheetParams {
    pub fn new(id: Id, content: Vec<Decl>) -> Self {
        Self {
            id,
            content,
            padding: Insets::ZERO,
            presses: Vec::new(),
            dismissible: true,
            barrier: true,
            target: ModalTarget::Global,
        }
    }

    pub fn padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    pub fn presses(mut self, presses: Vec<Id>) -> Self {
        self.presses = presses;
        self
    }

    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
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

/// A modal MD3 bottom sheet: the whole tree is rebuilt per frame, since its height is a spring.
pub struct Md3BottomSheet {
    body: Decl,
    /// The content id pressed this frame, if any.
    fired: Option<Id>,
    /// Whether the sheet asked to close this frame.
    dismissed: bool,
}

impl Md3BottomSheet {
    /// Resolve the surface, its position and what the user did to it this frame.
    pub fn new_with(params: BottomSheetParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let BottomSheetParams {
            id,
            content,
            padding,
            presses,
            dismissible,
            barrier,
            target,
        } = params;
        let surface_id = id.with("sheet");
        let spec = sheet_spec(SheetForm::Modal);
        let fill = spec_ink(pack, spec.container.color, 1.0);
        let handle_ink = spec_ink(pack, spec.drag_handle.color, 1.0);
        let corners = Corners {
            tl: SHEET_RADIUS,
            tr: SHEET_RADIUS,
            br: 0.0,
            bl: 0.0,
            ..Corners::ZERO
        };

        // The window and the sheet are both last frame's rects; the frame that opens the
        // sheet has neither, and it is drawn transparent so the entrance starts once they are
        // known.
        let window = cx.rect_of(id);
        let measured = cx.rect_of(surface_id);
        let window_w = window.map_or(0.0, |r| r.size.w);
        let height = measured.map_or(0.0, |r| r.size.h);
        let width = if window_w > 0.0 {
            window_w.min(SHEET_MAX_WIDTH)
        } else {
            SHEET_MAX_WIDTH
        };

        let state = cx.state(surface_id);
        let fired = presses.iter().copied().find(|p| cx.state(*p).clicked);
        let slide_id = surface_id.with("slide");
        let raw_id = surface_id.with("raw");
        let live_id = surface_id.with("live");
        let primed_id = surface_id.with("primed");

        let mut node = cx.at(surface_id);
        let primed = node.value(primed_id) > 0.5;
        let mut travelled = node.spring_value(slide_id).unwrap_or(0.0);
        if !primed && height > 0.0 {
            node.forget_spring(slide_id);
            travelled = node.spring(slide_id, height, SNAP_K, SNAP_C).x;
            node.animate(primed_id, 1.0, IMMEDIATE);
        }

        // A drag moves the sheet down by the pointer's own travel; a release settles it on
        // the nearer anchor, expanded at 0 and hidden at the sheet's height.
        let mut dismissed = false;
        if state.dragging && height > 0.0 {
            if node.value(live_id) <= 0.5 {
                node.animate(live_id, 1.0, IMMEDIATE);
                node.animate(raw_id, travelled, IMMEDIATE);
            }
            travelled = node.value(raw_id);
            if let Some(step) = state.drag_step {
                let raw = travelled + step.y;
                travelled = node.animate(raw_id, raw.max(0.0), DRAG_FOLLOW);
            }
        } else if node.value(live_id) > 0.5 {
            node.animate(live_id, 0.0, IMMEDIATE);
            // Release starts where the finger left it: the spring still holds the pre-drag
            // position, so reading it here would snap the sheet back before settling.
            let released = node.value(raw_id).max(0.0);
            if dismissible && released > height * 0.5 {
                dismissed = true;
            }
            node.forget_spring(slide_id);
            travelled = node.spring(slide_id, 0.0, SNAP_K, SNAP_C).x;
        } else {
            travelled = node.spring(slide_id, 0.0, SNAP_K, SNAP_C).x;
        }

        let mut body = Decl::container(surface_id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(width),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: 0.0,
                top: SHEET_HANDLE_TOP,
                right: 0.0,
                bottom: 0.0,
            })
            .gap(SHEET_HANDLE_GAP)
            .cross_align(1.0)
            .sense(Sense::DRAG)
            .cursor(CursorShape::Arrow)
            .offset(0.0, travelled)
            .opacity(if primed { 1.0 } else { 0.0 })
            .shapes(vec![Shape::Box {
                corners,
                fill,
                gradient: None,
                border: None,
                // MD3's elevation is a 1 dp ambient+caster pair; one analytic shadow stands in.
                shadow: (spec.container.elevation > 0.0)
                    .then(|| ShadowSpec::elevation(spec.container.elevation as u8)),
                image: None,
            }]);
        // The handle shares the surface's full width so it can be centred in the sheet,
        // which is wider than the 32 dp bar itself.
        body = body.child(
            Decl::container(surface_id.with("handle-row"), NodeKind::Stack)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .stack_align(OverlayAlign::Center)
                .child(
                    Decl::container(surface_id.with("handle"), NodeKind::Box)
                        .size(SizeClass::fixed(
                            spec.drag_handle.width,
                            spec.drag_handle.height,
                        ))
                        .background(handle_ink, Corners::all(spec.drag_handle.height * 0.5)),
                ),
        );
        let mut inner = Decl::container(surface_id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(padding);
        for block in content {
            inner = inner.child(block);
        }
        body = body.child(inner);

        let body = if !barrier {
            body
        } else {
            // The scrim subscribes: a press outside the surface must reach `change()`, and a
            // modal sheet blocks the page instead of letting it take the press.
            let mut root = Decl::container(id, NodeKind::Column)
                // Measured as its own content; the layout then places it on the target rect.
                .size(SizeClass::hug(0.0, 0.0))
                .modal(target)
                .cross_align(0.0)
                .cross_pos(0.5)
                .sense(Sense::CLICK)
                .cursor(CursorShape::Arrow)
                .shapes(vec![Shape::Box {
                    corners: Corners::ZERO,
                    fill: spec_ink(pack, "scrim", SHEET_SCRIM),
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                }]);
            root = root
                .child(grower(id.with("pad-top")))
                .child(body)
                .enter(Presence::fade(ENTER, 0.0))
                .exit(Presence::fade(LEAVE, 0.0));
            root
        };
        let dismissed =
            dismissed || (dismissible && cx.state(id).clicked && !cx.state(surface_id).clicked);
        Self {
            body,
            fired,
            dismissed,
        }
    }
}

/// A zero-basis grower: it takes the space above the sheet and draws nothing.
fn grower(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

impl Widget for Md3BottomSheet {
    fn handles_disabled(&self) -> bool {
        // A sheet has no disabled state; the adapter must not fade it either.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        self.dismissed.then_some(Change::Cancelled)
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.body.clone()
    }
}
