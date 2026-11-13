//! Where a control sits inside a group its parent built.
//!
//! The parent pushes this down, the child decides what it means: the same model Flutter and
//! GPUI use, and the reason the engine needs no "sibling style" machinery.

use ui_core::geom::Corners;

/// A control's slot in a connected group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupPosition {
    pub index: usize,
    pub len: usize,
}

impl GroupPosition {
    pub fn new(index: usize, len: usize) -> Self {
        Self { index, len }
    }

    pub fn is_first(&self) -> bool {
        self.index == 0
    }

    pub fn is_last(&self) -> bool {
        self.index + 1 == self.len
    }

    /// The only member: both ends are outer.
    pub fn is_only(&self) -> bool {
        self.len <= 1
    }

    /// Corners for a pill: `outer` where the group ends, `inner` where it meets a neighbour.
    pub fn corners(&self, outer: f32, inner: f32) -> Corners {
        Corners {
            tl: if self.is_first() { outer } else { inner },
            bl: if self.is_first() { outer } else { inner },
            tr: if self.is_last() { outer } else { inner },
            br: if self.is_last() { outer } else { inner },
            ..Corners::ZERO
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_of_a_group_has_two_inner_corners_and_the_ends_two_outer_ones() {
        let middle = GroupPosition::new(1, 3).corners(20.0, 8.0);
        assert_eq!(
            (middle.tl, middle.bl, middle.tr, middle.br),
            (8.0, 8.0, 8.0, 8.0)
        );
        let first = GroupPosition::new(0, 3).corners(20.0, 8.0);
        assert_eq!(
            (first.tl, first.bl, first.tr, first.br),
            (20.0, 20.0, 8.0, 8.0)
        );
        let last = GroupPosition::new(2, 3).corners(20.0, 8.0);
        assert_eq!((last.tl, last.bl, last.tr, last.br), (8.0, 8.0, 20.0, 20.0));
        // A lone member is a plain pill, not a group with one inner side.
        let only = GroupPosition::new(0, 1).corners(20.0, 8.0);
        assert_eq!((only.tl, only.tr), (20.0, 20.0));
    }
}
