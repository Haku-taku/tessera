//! Wheel translation: Wayland's axis events into the pixel deltas the engine scrolls by.
//!
//! One notch arrives as up to two events in a single `wl_pointer.frame`: `axis_value120`,
//! which counts 1/120 of a detent, and an `axis` restating that motion in surface
//! coordinates. Counting both scrolls every notch twice, so a frame's events are held per
//! axis until its end, where the detent wins — in whichever order the compositor sent them.

use ui_core::geom::Point;
use wayland_client::protocol::wl_pointer::Axis;

/// Pixels one detent scrolls, Chromium's `kPixelsPerLineStep`.
const PIXELS_PER_DETENT: f32 = 40.0;

/// One frame's scroll, per axis.
#[derive(Default)]
pub(crate) struct Wheel {
    /// The frame's detent, already a length in surface coordinates.
    detent: [Option<f32>; 2],
    /// The frame's continuous step: a finger drag, or the restatement of a detent.
    axis: [Option<f32>; 2],
}

impl Wheel {
    /// A high-resolution step: `value120` is 120 per detent, and a quarter turn still moves
    /// a quarter of the distance, so a smooth wheel is not quantised to whole notches.
    pub(crate) fn value120(&mut self, axis: Axis, value120: i32) {
        // Zero is a compositor saying it has no detent for this motion, not a detent of 0.
        if let Some(index) = Self::index(axis)
            && value120 != 0
        {
            self.detent[index] = Some(value120 as f32 / 120.0 * PIXELS_PER_DETENT);
        }
    }

    /// A continuous step, already a length in surface coordinates. Held until the frame
    /// ends: the same frame's `axis_value120` for this axis is the detent it restates.
    pub(crate) fn axis(&mut self, axis: Axis, value: f32) {
        if let Some(index) = Self::index(axis) {
            self.axis[index] = Some(value);
        }
    }

    /// The frame's scroll, one delta per axis that moved: the detent where the compositor
    /// sent one, the surface length where it did not.
    pub(crate) fn frame(&mut self) -> Vec<Point> {
        let mut out = Vec::new();
        for (index, axis) in [Axis::VerticalScroll, Axis::HorizontalScroll]
            .into_iter()
            .enumerate()
        {
            let detent = self.detent[index].take();
            let length = self.axis[index].take();
            if let Some(pixels) = detent.or(length) {
                // Wayland reports positive values as scrolling down/right, and our `delta`
                // means the same, so the sign passes through unchanged.
                out.push(Self::point(axis, pixels));
            }
        }
        out
    }

    /// Forget a half-frame: a pointer that left mid-frame will never complete it.
    pub(crate) fn leave(&mut self) {
        self.detent = [None; 2];
        self.axis = [None; 2];
    }

    fn index(axis: Axis) -> Option<usize> {
        match axis {
            Axis::VerticalScroll => Some(0),
            Axis::HorizontalScroll => Some(1),
            _ => None,
        }
    }

    fn point(axis: Axis, pixels: f32) -> Point {
        match axis {
            Axis::HorizontalScroll => Point::new(pixels, 0.0),
            _ => Point::new(0.0, pixels),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One notch is one scroll whatever order its two halves arrive in: Hyprland sends the
    /// `axis` restatement first, and counting it as well made a notch 55 pixels instead of
    /// the 40 its detent states.
    #[test]
    fn one_detent_scrolls_once_in_either_order() {
        for reversed in [false, true] {
            let mut wheel = Wheel::default();
            let detent = |wheel: &mut Wheel| wheel.value120(Axis::VerticalScroll, 120);
            let restated = |wheel: &mut Wheel| wheel.axis(Axis::VerticalScroll, 15.0);
            if reversed {
                restated(&mut wheel);
                detent(&mut wheel);
            } else {
                detent(&mut wheel);
                restated(&mut wheel);
            }
            assert_eq!(
                wheel.frame(),
                vec![Point::new(0.0, 40.0)],
                "a detent is 120 and scrolls one line, restatement or not"
            );
            assert!(wheel.frame().is_empty(), "and the frame is spent");
        }
    }

    /// A finger's axis stands alone, a smooth wheel's fraction of a detent is not rounded,
    /// each axis restates on its own, and a half-frame the pointer left behind is dropped.
    #[test]
    fn a_finger_scroll_and_a_smooth_wheel_pass_through() {
        let mut wheel = Wheel::default();
        wheel.axis(Axis::VerticalScroll, 3.0);
        assert_eq!(wheel.frame(), vec![Point::new(0.0, 3.0)]);

        wheel.value120(Axis::VerticalScroll, 30);
        assert_eq!(wheel.frame(), vec![Point::new(0.0, 10.0)]);

        wheel.axis(Axis::HorizontalScroll, 5.0);
        assert_eq!(wheel.frame(), vec![Point::new(5.0, 0.0)]);

        wheel.axis(Axis::VerticalScroll, 7.0);
        wheel.leave();
        assert!(
            wheel.frame().is_empty(),
            "an abandoned frame scrolls nothing"
        );
    }
}
