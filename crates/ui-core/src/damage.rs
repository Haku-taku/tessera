//! Damage: which regions changed this frame.

use crate::geom::Rect;

/// Most rects one frame may carry before they collapse into a single bounding rect.
///
/// Exact merging is quadratic, and a frame that scrolls a long document changes every row
/// it holds: thousands of rects arrive at once, most of them outside the viewport.
const DAMAGE_RECTS_MAX: usize = 64;

/// Regions that must be redrawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Damage {
    /// Nothing changed; the frame can be skipped entirely.
    #[default]
    None,
    /// Specific rects.
    Rects(Vec<Rect>),
    /// Everything.
    Full,
}

impl Damage {
    pub fn is_none(&self) -> bool {
        matches!(self, Damage::None)
    }

    pub fn is_full(&self) -> bool {
        matches!(self, Damage::Full)
    }

    /// Rects as a slice, flattening `Full` to the given bounds.
    ///
    /// Borrowed when the damage is already rects: the GPU band and the compositor's damage
    /// both read it every frame, and a copy of it allocates for nothing.
    pub fn rects(&self, full: Rect) -> std::borrow::Cow<'_, [Rect]> {
        match self {
            Damage::None => std::borrow::Cow::Borrowed(&[]),
            Damage::Rects(r) => std::borrow::Cow::Borrowed(r),
            Damage::Full => std::borrow::Cow::Owned(vec![full]),
        }
    }
}

/// Collects damage rects across a frame, clipping to the frame and merging at the end.
#[derive(Default, Debug)]
pub struct DamageAccum {
    rects: Vec<Rect>,
    full: bool,
    /// The area the frame can paint; `None` until the frame states its size.
    bounds: Option<Rect>,
}

impl DamageAccum {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clip damage to what the frame can show.
    ///
    /// A row scrolled past the viewport is not painted, so its damage is not either: the
    /// presenter would otherwise be handed thousands of rects that reach no pixel.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = Some(bounds);
    }

    pub fn add(&mut self, r: Rect) {
        if r.is_empty() || self.full {
            return;
        }
        let r = match self.bounds {
            Some(bounds) => match bounds.intersection(r) {
                Some(clipped) => clipped,
                None => return,
            },
            None => r,
        };
        self.rects.push(r);
        if self.rects.len() > DAMAGE_RECTS_MAX {
            self.collapse();
        }
    }

    /// Snapshot of what has been collected so far, for rules that depend on it.
    pub fn snapshot(&self) -> Damage {
        if self.full {
            Damage::Full
        } else if self.rects.is_empty() {
            Damage::None
        } else {
            Damage::Rects(Self::merged(&self.rects))
        }
    }

    pub fn add_full(&mut self) {
        self.full = true;
        self.rects.clear();
    }

    /// Replace a scattered set with the box around it.
    ///
    /// Merging is deferred to the end of the frame, so this is what keeps the list bounded
    /// while a scroll damages a row per item; the box costs pixels, not time.
    fn collapse(&mut self) {
        let Some(first) = self.rects.first().copied() else {
            return;
        };
        let union = self.rects.iter().fold(first, |acc, rect| acc.union(*rect));
        self.rects.clear();
        self.rects.push(union);
    }

    /// Overlapping rects merged transitively, so the presenter paints each pixel once.
    fn merged(rects: &[Rect]) -> Vec<Rect> {
        let mut merged = rects.to_vec();
        let mut joined = true;
        while joined {
            joined = false;
            'outer: for i in 0..merged.len() {
                for j in (i + 1)..merged.len() {
                    if merged[i].intersects(merged[j]) {
                        merged[i] = merged[i].union(merged[j]);
                        merged.remove(j);
                        joined = true;
                        break 'outer;
                    }
                }
            }
        }
        merged
    }

    pub fn finish(self) -> Damage {
        if self.full {
            Damage::Full
        } else if self.rects.is_empty() {
            Damage::None
        } else {
            Damage::Rects(Self::merged(&self.rects))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn overlapping_rects_merge_into_one() {
        let mut acc = DamageAccum::new();
        acc.add(r(0.0, 0.0, 10.0, 10.0));
        acc.add(r(5.0, 5.0, 10.0, 10.0));
        match acc.finish() {
            Damage::Rects(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0], r(0.0, 0.0, 15.0, 15.0));
            }
            other => panic!("expected rects, got {other:?}"),
        }
    }

    #[test]
    fn full_swallows_everything() {
        let mut acc = DamageAccum::new();
        acc.add(r(0.0, 0.0, 10.0, 10.0));
        acc.add_full();
        acc.add(r(50.0, 50.0, 10.0, 10.0));
        assert_eq!(acc.finish(), Damage::Full);
    }

    #[test]
    fn damage_stays_inside_the_frame_and_covers_every_change() {
        // Scrolling a long list changes every row at once, and only the rows the viewport
        // shows can be painted: the rest must not reach the presenter.
        let frame = r(0.0, 0.0, 200.0, 100.0);
        let mut acc = DamageAccum::new();
        acc.set_bounds(frame);
        let mut changed = Vec::new();
        for row in 0..2_000 {
            let rect = r(-10.0, row as f32 * 20.0 - 40.0, 300.0, 18.0);
            acc.add(rect);
            if let Some(clipped) = frame.intersection(rect) {
                changed.push(clipped);
            }
        }
        let Damage::Rects(rects) = acc.finish() else {
            panic!("a bounded set stays a set of rects");
        };
        assert!(
            rects.len() <= DAMAGE_RECTS_MAX,
            "the region list is bounded, got {}",
            rects.len()
        );
        for rect in &rects {
            assert_eq!(
                frame.intersection(*rect),
                Some(*rect),
                "damage left the frame: {rect:?}"
            );
        }
        for clipped in changed {
            assert!(
                rects
                    .iter()
                    .any(|rect| rect.intersection(clipped) == Some(clipped)),
                "no region covers the change at {clipped:?}"
            );
        }
    }
}
