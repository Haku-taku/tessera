//! Moving the real pointer from the keyboard.
//!
//! A desktop has no protocol every compositor honours for this, so the move is a request:
//! the platform says whether it could perform one, and an app never assumes it did. The
//! arithmetic lives here so it can be exercised with no display at all.

use std::rc::Rc;

use ui_core::geom::Point;

/// A platform's ability to put the pointer somewhere on a surface.
pub trait PointerWarp {
    /// Move the pointer to `at`, in surface-local logical points.
    ///
    /// `false` when the platform cannot: no protocol for it, no pointer focus, or no
    /// recent input serial to authorise the request. The caller keeps its own position.
    fn warp(&self, at: Point) -> bool;
}

/// The surface's pointer, as a keyboard command moves it.
///
/// The command is a step — "one point left" — not a position, because an app cannot see
/// where the pointer is: only the engine reads the seat's motion. So the surface passes in
/// where it last saw the pointer, and gets back where the pointer now is, or `None` when
/// nothing moved.
pub struct Cursor {
    warp: Option<Rc<dyn PointerWarp>>,
}

impl Cursor {
    /// A cursor this platform cannot move; every step answers `None`.
    pub fn fixed() -> Self {
        Self { warp: None }
    }

    pub fn new(warp: Rc<dyn PointerWarp>) -> Self {
        Self { warp: Some(warp) }
    }

    /// Whether this platform can move the pointer at all.
    pub fn can_move(&self) -> bool {
        self.warp.is_some()
    }

    /// Step the pointer by `(dx, dy)` from `from`, clamped to `bounds`.
    ///
    /// `None` when there is no pointer over the surface, when this platform cannot warp,
    /// or when the platform refused; in each case the caller keeps the position it had.
    pub fn step(
        &self,
        from: Option<Point>,
        (dx, dy): (f32, f32),
        bounds: (f32, f32),
    ) -> Option<Point> {
        let from = from?;
        let warp = self.warp.as_ref()?;
        let at = stepped(from, (dx, dy), bounds);
        warp.warp(at).then_some(at)
    }
}

/// Where a step of `(dx, dy)` from `from` lands, kept inside a `(width, height)` surface.
///
/// A compositor rejects a warp outside the surface, and the last addressable point is one
/// unit short of the edge, so the clamp stops there rather than at the extent.
pub fn stepped(from: Point, (dx, dy): (f32, f32), bounds: (f32, f32)) -> Point {
    let edge = |extent: f32| (extent - 1.0).max(0.0);
    Point::new(
        (from.x + dx).clamp(0.0, edge(bounds.0)),
        (from.y + dy).clamp(0.0, edge(bounds.1)),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Records what it was asked for and answers what the test tells it to.
    struct Recorder {
        seen: RefCell<Vec<Point>>,
        accepts: bool,
    }

    impl PointerWarp for Recorder {
        fn warp(&self, at: Point) -> bool {
            self.seen.borrow_mut().push(at);
            self.accepts
        }
    }

    /// A step is relative and clamped to the surface the compositor will accept; a platform
    /// that refuses the move leaves the caller's position alone rather than guessing.
    #[test]
    fn a_step_is_relative_clamped_and_reported() {
        let recorder = Rc::new(Recorder {
            seen: RefCell::new(Vec::new()),
            accepts: true,
        });
        let cursor = Cursor::new(recorder.clone());
        let bounds = (100.0, 50.0);

        let landed = cursor.step(Some(Point::new(10.0, 10.0)), (1.0, -3.0), bounds);
        assert_eq!(landed, Some(Point::new(11.0, 7.0)), "a step from where it was");

        let corner = cursor.step(Some(Point::new(0.0, 0.0)), (-5.0, -5.0), bounds);
        assert_eq!(corner, Some(Point::new(0.0, 0.0)), "nothing outside the surface");

        let far = cursor.step(Some(Point::new(99.0, 49.0)), (50.0, 50.0), bounds);
        assert_eq!(
            far,
            Some(Point::new(99.0, 49.0)),
            "the last addressable point is one short of the edge"
        );
        assert_eq!(
            recorder.seen.borrow().len(),
            3,
            "every accepted step reached the platform"
        );

        assert_eq!(
            cursor.step(None, (1.0, 0.0), bounds),
            None,
            "no pointer over the surface, nothing to step"
        );

        let refusing = Cursor::new(Rc::new(Recorder {
            seen: RefCell::new(Vec::new()),
            accepts: false,
        }));
        assert_eq!(
            refusing.step(Some(Point::new(5.0, 5.0)), (1.0, 0.0), bounds),
            None
        );
        assert!(!Cursor::fixed().can_move());
    }
}
