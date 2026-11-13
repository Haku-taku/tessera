//! The WinUI popup entrance: the surface fades while travelling toward its trigger, clipped
//! to its own footprint, so it unfolds from that edge.

use ui_core::anim::AnimSpec;
use ui_core::layout::{Anchor, Len, LenInsets, Side};
use ui_core::{Decl, Id, NodeKind};
use ui_style::motion::WINUI_SPLINE;
use ui_widgets::Cx;
use ui_widgets::build::IMMEDIATE;

/// WinUI's `ControlFastAnimationDuration` to open, `ControlFasterAnimationDuration` to close.
pub const POPUP_OPEN: AnimSpec = AnimSpec::new(0.167, WINUI_SPLINE);
pub const POPUP_CLOSE: AnimSpec = AnimSpec::new(0.083, WINUI_SPLINE);
/// The surface starts half its own height toward the trigger, then travels back to rest.
pub const POPUP_TRAVEL: f32 = 50.0;

/// This frame's entrance: how present the popup is, and whether it is still on stage.
#[derive(Clone, Copy, Debug)]
pub struct PopupMotion {
    /// Group opacity, `0..=1`.
    pub presence: f32,
    /// Whether the node must be built: opening, or still fading out.
    pub alive: bool,
    /// Signed travel in percent of the panel's own height, toward the trigger.
    pub travel: f32,
}

impl PopupMotion {
    /// Put the surface inside its window: shift it toward the trigger and fade the group.
    ///
    /// The travel is a percentage of the containing block, which is the window the panel
    /// hugs, so the height it needs is the one layout is about to give it.
    pub fn apply(&self, panel: Decl) -> Decl {
        panel
            .relative()
            .inset(LenInsets::new(
                Len::Auto,
                Len::Pct(self.travel),
                Len::Auto,
                Len::Auto,
            ))
            .opacity(self.presence)
    }
}

/// The window an animated surface sits in: an overlay is masked to its own box, so this
/// final rect is what clips — and reveals — the panel travelling inside it.
pub fn popup_window(id: Id) -> Decl {
    Decl::container(id, NodeKind::Box)
}

/// Drive the entrance of the panel sitting in `window`; ask once per frame, before building it.
pub fn popup_motion(
    cx: &mut Cx<'_, '_>,
    window: Id,
    anchor: Anchor,
    side: Side,
    open: bool,
) -> PopupMotion {
    let key = window.with("presence");
    // A value with no history lands on its target, so a first opening seeds zero first.
    if cx.at(key).status(key).is_none() {
        cx.at(key).animate(key, 0.0, IMMEDIATE);
    }
    let spec = match open {
        true => POPUP_OPEN,
        false => POPUP_CLOSE,
    };
    let presence = cx.at(key).animate(key, f32::from(open), spec);
    // The engine flips a panel that would leave the viewport, so the drawn side wins over
    // the declared one; the window's rect is the one that stays put while the panel moves.
    let drawn = cx.rect_of(window).filter(|r| r.size.h > 0.0);
    let upward = match (drawn, anchor) {
        (Some(window), Anchor::Node(node)) => cx
            .rect_of(node)
            .is_some_and(|anchor| window.center().y < anchor.center().y),
        (Some(window), Anchor::Point(point)) => window.center().y < point.y,
        _ => side == Side::Above,
    };
    let travel = POPUP_TRAVEL * (1.0 - presence) * if upward { 1.0 } else { -1.0 };
    PopupMotion {
        presence,
        alive: open || presence > 0.0,
        travel,
    }
}
