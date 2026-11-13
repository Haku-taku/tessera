//! Names every style shares for its atomic animation instances, plus the drag follow spec.

use crate::NodeCx;
use ui_core::{AnimSpec, Easing, Id};

pub mod param {
    /// 0..=1 travel of the semantic value.
    pub const POSITION: &str = "position";
    /// 0..=1 raw progress of the semantic value, linear in time.
    pub const PROGRESS: &str = "progress";
    /// 0..=1 palette blend; styles that fade colours separately use this.
    pub const COLOUR: &str = "colour";
    /// 0..=1 press intensity.
    pub const PRESS: &str = "press";
    /// Diameter the thumb has at the off end, latched there by styles that need it.
    pub const SIZE_OFF: &str = "sizeOff";
    /// Diameter the thumb has at the on end.
    pub const SIZE_ON: &str = "sizeOn";
    /// Thumb width in px, animated on its own so a squash needs no derived sequence.
    pub const WIDTH: &str = "width";
    /// Thumb height in px.
    pub const HEIGHT: &str = "height";
    /// Track colour, animated as one colour instance.
    pub const TRACK: &str = "track";
    /// Outline colour.
    pub const OUTLINE: &str = "outline";
    /// 0..=1 hover overlay fade.
    pub const HOVER: &str = "hover";
    /// 0..=1 focus overlay fade.
    pub const FOCUS: &str = "focus";
    /// 0..=1 glass bloom thickness.
    pub const THICKNESS: &str = "thickness";
}

/// The finger is the clock: a zero-duration write, so a held gesture is not smoothed.
pub const IMMEDIATE: AnimSpec = AnimSpec::new(0.0, Easing::Linear);

/// The shortest follow that still smooths hand jitter: a drag sets a *target*, never a value.
///
/// A zero-duration write looks identical on a still frame and jumps when the target moves.
pub const DRAG_FOLLOW: AnimSpec = AnimSpec::new(0.024, Easing::Linear);

/// Where a dragged element sits: a press never moves it, the first drag frame glides from
/// `value` to the finger and later frames track it; a tap glides to its value over `settle`.
pub fn drag_follow(
    ctx: &mut NodeCx<'_>,
    id: Id,
    value: f32,
    finger: f32,
    dragging: bool,
    glide: AnimSpec,
    settle: AnimSpec,
) -> f32 {
    let live = id.with("drag-live");
    let offset = id.with("drag-offset");
    let progress = id.with("drag-progress");
    let rest = id.with("drag-rest");
    if dragging {
        if ctx.value(live) <= 0.5 {
            // First drag frame: the element stays where it is, the gap to the finger is banked,
            // and the glide's clock starts here rather than a frame later.
            ctx.animate(live, 1.0, IMMEDIATE);
            ctx.animate(offset, value - finger, IMMEDIATE);
            ctx.animate(progress, 0.0, IMMEDIATE);
            ctx.animate(progress, 1.0, glide);
            ctx.animate(rest, value, IMMEDIATE);
            return value;
        }
        let p = ctx.animate(progress, 1.0, glide);
        let pos = finger + ctx.value(offset) * (1.0 - p);
        // The settled value is wherever the finger has the element, so letting go changes nothing.
        ctx.animate(rest, pos, IMMEDIATE);
        return pos;
    }
    if ctx.value(live) > 0.5 {
        let p = ctx.animate(progress, 1.0, glide);
        // The release frame reports where the finger left the element, never `value`: the app
        // has not applied the change yet, so a control that commits by position must see the
        // finger's, not the stale semantic value. A release before the glide is spent keeps
        // the branch armed until the element has caught up, then the idle path takes over.
        let pos = finger + ctx.value(offset) * (1.0 - p);
        ctx.animate(rest, pos, IMMEDIATE);
        if p > 0.999 {
            ctx.animate(live, 0.0, IMMEDIATE);
        }
        return pos;
    }
    // Idle: a tap or a value set from outside glides to it, it does not snap.
    ctx.animate(rest, value, settle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::AnimStore;

    fn at(anim: &mut AnimStore, now: f64, value: f32, finger: f32, dragging: bool) -> f32 {
        let id = Id::str("thumb");
        let mut node = crate::cx::NodeCx::new(id, Default::default(), 1.0, now, anim);
        drag_follow(
            &mut node,
            id,
            value,
            finger,
            dragging,
            DRAG_FOLLOW_GLIDE,
            DRAG_FOLLOW_GLIDE,
        )
    }

    const DRAG_FOLLOW_GLIDE: AnimSpec = AnimSpec::new(0.150, Easing::Linear);

    #[test]
    fn a_release_before_the_glide_is_spent_reports_the_element_not_the_old_value() {
        // The app has not applied the drag yet on the release frame, so reporting `value`
        // there rewinds the element to where it started — a quick flick snapped back and
        // committed nothing. The frame must report the element's own travel so far.
        let mut anim = AnimStore::new();
        assert_eq!(at(&mut anim, 0.0, 0.0, 0.0, false), 0.0, "closed at rest");
        assert_eq!(
            at(&mut anim, 0.0, 0.0, 0.9, true),
            0.0,
            "the drag starts where it was"
        );
        let released = at(&mut anim, 0.017, 0.0, 0.9, false);
        assert!(
            released > 0.0,
            "the release keeps the travel, got {released}"
        );
        assert!(
            released < 0.9,
            "and the glide is still under way, got {released}"
        );
    }
}
