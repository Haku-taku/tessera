//! Hit testing: which node receives a pointer event.

use crate::geom::{Point, Rect};
use crate::id::NodeId;
use crate::tree::Tree;

/// How a node participates in hit testing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HitBehavior {
    /// Not interactive.
    #[default]
    Ignore,
    /// Receives events; consumes them.
    Opaque,
    /// Receives events but lets the search continue underneath.
    PassThrough,
}

/// One registered hit area.
#[derive(Clone, Copy, Debug)]
pub struct HitTarget {
    pub node: NodeId,
    pub bounds: Rect,
    /// Clip in force when the node was registered.
    pub mask: Rect,
    pub behavior: HitBehavior,
    /// Higher wins; `HitIndex::build` numbers targets in paint order, layers included.
    pub z: u32,
}

/// Spatial index over hit targets.
#[derive(Debug, Default)]
pub struct HitIndex {
    entries: Vec<HitTarget>,
    /// 64pt cells over a rectangle of the plane; each lists the entries touching it.
    cells: Vec<Vec<u32>>,
    /// Cell at the grid's top-left corner.
    cell_min: (i32, i32),
    /// Grid size in cells; zero until the first insert.
    cols: i32,
    rows: i32,
    /// Targets too large to bucket; every query reads them, so no cell can miss one.
    wide: Vec<u32>,
}

const CELL: f32 = 64.0;

fn cell_of(p: Point) -> (i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

impl HitIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty the index, keeping the grid and its buckets allocated for the next frame.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.wide.clear();
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn insert(&mut self, target: HitTarget) {
        let index = self.entries.len() as u32;
        self.entries.push(target);
        // Register in every bucket the bounds touch, capped so a huge node does not
        // flood the grid.
        let (c0, c1) = (
            cell_of(target.bounds.origin),
            cell_of(Point::new(target.bounds.max_x(), target.bounds.max_y())),
        );
        let cells = (c1.0 - c0.0 + 1).max(1) as i64 * (c1.1 - c0.1 + 1).max(1) as i64;
        if cells > 4096 {
            // A node this large would flood the grid; every query reads the wide list instead.
            self.wide.push(index);
            return;
        }
        self.cover(c0, c1);
        for cy in c0.1..=c1.1 {
            for cx in c0.0..=c1.0 {
                if let Some(i) = self.cell_index((cx, cy)) {
                    self.cells[i].push(index);
                }
            }
        }
    }

    /// Index of `cell` in `cells`, or `None` when the grid does not reach it.
    fn cell_index(&self, (cx, cy): (i32, i32)) -> Option<usize> {
        let (x, y) = (cx - self.cell_min.0, cy - self.cell_min.1);
        if x < 0 || y < 0 || x >= self.cols || y >= self.rows {
            return None;
        }
        Some((y * self.cols + x) as usize)
    }

    /// Grow the grid, if needed, so that it spans `c0..=c1`; existing buckets move with it.
    fn cover(&mut self, c0: (i32, i32), c1: (i32, i32)) {
        let (old_min, old_cols, old_rows) = (self.cell_min, self.cols, self.rows);
        let have = old_cols > 0;
        let min = if have {
            (old_min.0.min(c0.0), old_min.1.min(c0.1))
        } else {
            c0
        };
        let max = if have {
            (
                (old_min.0 + old_cols - 1).max(c1.0),
                (old_min.1 + old_rows - 1).max(c1.1),
            )
        } else {
            c1
        };
        let (cols, rows) = (max.0 - min.0 + 1, max.1 - min.1 + 1);
        if have && min == old_min && (cols, rows) == (old_cols, old_rows) {
            return;
        }
        let mut cells = vec![Vec::new(); (cols * rows) as usize];
        for y in 0..old_rows {
            for x in 0..old_cols {
                let from = (y * old_cols + x) as usize;
                if self.cells[from].is_empty() {
                    continue;
                }
                let to_x = old_min.0 + x - min.0;
                let to_y = old_min.1 + y - min.1;
                cells[(to_y * cols + to_x) as usize] = std::mem::take(&mut self.cells[from]);
            }
        }
        self.cells = cells;
        self.cell_min = min;
        self.cols = cols;
        self.rows = rows;
    }

    /// Every target that can cover `p`: the point's own bucket plus the wide ones.
    fn covering(&self, p: Point) -> impl Iterator<Item = &HitTarget> {
        self.cell_index(cell_of(p))
            .and_then(|i| self.cells.get(i))
            .into_iter()
            .flatten()
            .chain(self.wide.iter())
            .filter_map(|i| self.entries.get(*i as usize))
            .filter(move |t| t.bounds.contains(p) && t.mask.contains(p))
    }

    /// The highest of the covering targets for which `keep` holds; ties keep the first.
    fn highest(&self, p: Point, keep: impl Fn(&HitTarget) -> bool) -> Option<NodeId> {
        let mut best: Option<&HitTarget> = None;
        for t in self.covering(p).filter(|t| keep(t)) {
            if best.map(|b| t.z > b.z).unwrap_or(true) {
                best = Some(t);
            }
        }
        best.map(|t| t.node)
    }

    /// Topmost target at `p`, honouring mask and `PassThrough`.
    pub fn topmost(&self, p: Point) -> Option<NodeId> {
        self.highest(p, |_| true)
    }

    /// Highest target at `p` that consumes the event, so a pass-through node is skipped.
    ///
    /// `topmost` answers "what is drawn here"; this answers "what takes the input".
    pub fn topmost_consumer(&self, p: Point) -> Option<NodeId> {
        self.highest(p, |t| t.behavior == HitBehavior::Opaque)
    }

    /// All targets at `p` from top to bottom, stopping at the first opaque one.
    pub fn stack_at(&self, p: Point) -> Vec<NodeId> {
        let mut hits: Vec<&HitTarget> = self.covering(p).collect();
        hits.sort_by_key(|t| std::cmp::Reverse(t.z));
        let mut out = Vec::new();
        for t in hits {
            out.push(t.node);
            if t.behavior == HitBehavior::Opaque {
                break;
            }
        }
        out
    }

    /// The areas this surface consumes pointer input in, in surface space.
    ///
    /// The union is exactly the surface's input shape: a point lies in it if and only if
    /// [`topmost_consumer`](Self::topmost_consumer) names a node there, because a
    /// pass-through target never ends the search and an ignored one is not indexed at all.
    pub fn consumer_areas(&self) -> impl Iterator<Item = Rect> + '_ {
        self.entries
            .iter()
            .filter(|target| target.behavior == HitBehavior::Opaque)
            .filter_map(|target| target.bounds.intersection(target.mask))
            .filter(|area| !area.is_empty())
    }

    /// Build an index from the tree, using painted rects intersected with masks.
    pub fn build(tree: &Tree, behaviors: impl Fn(NodeId) -> HitBehavior) -> Self {
        let mut idx = Self::new();
        idx.rebuild(tree, behaviors);
        idx
    }

    /// Refill this index from the tree, reusing its buckets: a frame rebuilds the index
    /// with the same geometry, so only the contents need replacing.
    pub fn rebuild(&mut self, tree: &Tree, behaviors: impl Fn(NodeId) -> HitBehavior) {
        self.clear();
        // A scaled subtree paints somewhere else, so its targets must move with it.
        let scaling = tree.paints_scaled();
        for (order, nid) in tree.walk().into_iter().enumerate() {
            let Some(node) = tree.get(nid) else {
                continue;
            };
            let behavior = behaviors(nid);
            if behavior == HitBehavior::Ignore {
                continue;
            }
            // Hit testing follows the pixels: an offset or scaled node is clickable where it
            // is drawn; the mask stays the viewport it was laid out against.
            let visual = if scaling {
                tree.scaled_visual_rect(nid)
            } else {
                node.visual_rect()
            };
            let bounds = visual.intersection(node.mask).unwrap_or(Rect::ZERO);
            if bounds.is_empty() {
                continue;
            }
            self.insert(HitTarget {
                node: nid,
                bounds,
                mask: node.mask,
                behavior,
                z: order as u32,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(x: f32, y: f32, w: f32, h: f32, z: u32, behavior: HitBehavior) -> HitTarget {
        HitTarget {
            node: NodeId::from_raw(z, 0),
            bounds: Rect::new(x, y, w, h),
            mask: Rect::new(-1000.0, -1000.0, 5000.0, 5000.0),
            behavior,
            z: 1000 + z,
        }
    }

    /// A pass-through target stays in the index but never consumes the event.
    #[test]
    fn topmost_consumer_skips_a_pass_through_target() {
        let mut idx = HitIndex::new();
        idx.insert(target(0.0, 0.0, 100.0, 100.0, 0, HitBehavior::Opaque));
        idx.insert(target(0.0, 0.0, 100.0, 100.0, 5, HitBehavior::PassThrough));
        let p = Point::new(15.0, 15.0);
        assert_eq!(
            idx.topmost(p),
            Some(NodeId::from_raw(5, 0)),
            "drawing order unchanged"
        );
        assert_eq!(
            idx.topmost_consumer(p),
            Some(NodeId::from_raw(0, 0)),
            "the opaque target behind must take the input"
        );
    }

    /// A surface publishes its consumer areas as its input shape, so the two must agree at
    /// every point — including where a pass-through decoration sits in front of the target.
    #[test]
    fn consumer_areas_are_exactly_where_a_consumer_wins() {
        let mut idx = HitIndex::new();
        // A clipped consumer: its bounds reach further than its mask allows.
        let mut clipped = target(0.0, 0.0, 100.0, 100.0, 0, HitBehavior::Opaque);
        clipped.mask = Rect::new(0.0, 0.0, 60.0, 100.0);
        idx.insert(clipped);
        // A decoration in front of it that never consumes.
        idx.insert(target(20.0, 20.0, 40.0, 40.0, 1, HitBehavior::PassThrough));
        // A second consumer far away, so the union is genuinely several rects.
        idx.insert(target(200.0, 120.0, 30.0, 30.0, 2, HitBehavior::Opaque));
        let areas: Vec<Rect> = idx.consumer_areas().collect();
        for y in (-10..=170).step_by(5) {
            for x in (-10..=240).step_by(5) {
                let p = Point::new(x as f32, y as f32);
                let covered = areas.iter().any(|area| area.contains(p));
                assert_eq!(
                    covered,
                    idx.topmost_consumer(p).is_some(),
                    "input shape disagrees with the hit test at {p:?}: {areas:?}"
                );
            }
        }
        assert_eq!(
            areas,
            vec![
                Rect::new(0.0, 0.0, 60.0, 100.0),
                Rect::new(200.0, 120.0, 30.0, 30.0)
            ],
            "a clipped consumer publishes only what its mask allows"
        );
    }

    /// A target too large to bucket is still hit far from its origin.
    #[test]
    fn a_target_larger_than_the_bucket_cap_is_hit_across_its_whole_extent() {
        let mut idx = HitIndex::new();
        let mut huge = target(0.0, 0.0, 20_000.0, 20_000.0, 0, HitBehavior::Opaque);
        huge.mask = Rect::new(0.0, 0.0, 20_000.0, 20_000.0);
        idx.insert(huge);
        let far = Point::new(19_000.0, 19_000.0);
        assert_eq!(idx.topmost(far), Some(NodeId::from_raw(0, 0)));
        assert_eq!(idx.topmost_consumer(far), Some(NodeId::from_raw(0, 0)));
        assert_eq!(idx.stack_at(far), vec![NodeId::from_raw(0, 0)]);
    }

    #[test]
    fn mask_excludes_points_outside_the_clip() {
        let mut idx = HitIndex::new();
        let mut t = target(0.0, 0.0, 100.0, 100.0, 0, HitBehavior::Opaque);
        t.mask = Rect::new(0.0, 0.0, 50.0, 50.0);
        idx.insert(t);
        assert!(idx.topmost(Point::new(25.0, 25.0)).is_some());
        assert!(
            idx.topmost(Point::new(75.0, 75.0)).is_none(),
            "point inside bounds but outside mask must not hit"
        );
    }

    #[test]
    fn pass_through_lets_the_search_reach_lower_nodes() {
        let mut idx = HitIndex::new();
        idx.insert(target(0.0, 0.0, 100.0, 100.0, 0, HitBehavior::Opaque));
        idx.insert(target(0.0, 0.0, 100.0, 100.0, 1, HitBehavior::PassThrough));

        let stack = idx.stack_at(Point::new(50.0, 50.0));
        assert_eq!(stack.len(), 2, "pass-through must not stop the search");
        assert_eq!(stack[0].index, 1);

        let p = Point::new(50.0, 50.0);
        let opaque_stack = {
            let mut i = HitIndex::new();
            i.insert(target(0.0, 0.0, 100.0, 100.0, 0, HitBehavior::Opaque));
            i.insert(target(0.0, 0.0, 100.0, 100.0, 1, HitBehavior::Opaque));
            i.stack_at(p)
        };
        assert_eq!(opaque_stack.len(), 1, "opaque must stop the search");
    }
}
