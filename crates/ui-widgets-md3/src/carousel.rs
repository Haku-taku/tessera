//! MD3 carousel: a strip of items that change size as they pass the leading edge.

use ui_core::draw::Shape;
use ui_core::geom::{Color, Corners, Insets};
use ui_core::layout::{AUTO, AxisSize, Clip, Grow, Overflow, SizeClass};
use ui_core::tree::ShadowSpec;
use ui_core::{Change, CursorShape, Decl, Id, InkRadius, NodeKind, PressInk, Sense};
use ui_style::Style;
use ui_widgets::build::param;
use ui_widgets::{Cx, Widget, WidgetState};

use crate::Md3Options;
use crate::layer::press_ink_shape;
use crate::paint::{lerp, role, spec_ink};
use crate::spec::{CarouselItemFlags, carousel_item_spec};

/// MD3's gutter: 16dp at the container's edges, 8dp above and below the strip.
pub const CAROUSEL_PAD: f32 = 16.0;
/// Top/bottom gutter of a horizontal carousel.
pub const CAROUSEL_VPAD: f32 = 8.0;
/// Space between two items in every layout but full-screen.
pub const CAROUSEL_GAP: f32 = 8.0;
/// Full-screen carousels drop the gutter and separate items by 16dp.
pub const CAROUSEL_FULL_GAP: f32 = 16.0;
/// `rounded_corners` in the export, 28dp on the MD3 shape scale.
pub const CAROUSEL_RADIUS: f32 = 28.0;
/// MD3's small-item width range, in dp.
pub const CAROUSEL_SMALL: (f32, f32) = (40.0, 56.0);

use crate::motion::REACTION;

/// The five layouts MD3 names; each fixes its strip geometry at build time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarouselKind {
    /// A large, a medium and a small item, the leading one largest.
    MultiBrowse,
    /// One large item with a small one peeking in from the trailing edge.
    Hero,
    /// A large item centred between two small ones.
    HeroCentered,
    /// Items keep one size and run off the edges.
    Uncontained,
    /// One edge-to-edge item at a time, scrolling vertically.
    FullScreen,
}

impl CarouselKind {
    /// The relative widths that fill one page, from the leading edge backwards.
    ///
    /// These come from Flutter's weighted carousel; the export sizes nothing but the gaps
    /// and the small range, so the ratios are what turn a width into item extents.
    pub fn weights(self) -> &'static [u32] {
        match self {
            Self::MultiBrowse => &[3, 2, 1],
            Self::Hero => &[7, 1],
            Self::HeroCentered => &[1, 5, 1],
            Self::Uncontained | Self::FullScreen => &[1],
        }
    }

    /// Whether items resize as the strip scrolls.
    pub fn shrinking(self) -> bool {
        matches!(self, Self::MultiBrowse | Self::Hero | Self::HeroCentered)
    }

    /// Whether the strip runs vertically.
    pub fn vertical(self) -> bool {
        matches!(self, Self::FullScreen)
    }

    fn gap(self) -> f32 {
        if self.vertical() {
            CAROUSEL_FULL_GAP
        } else {
            CAROUSEL_GAP
        }
    }

    /// The gutter the container insets its strip by; uncontained items bleed over it.
    fn gutter(self) -> (f32, f32) {
        if self.shrinking() {
            (CAROUSEL_PAD, CAROUSEL_VPAD)
        } else {
            (0.0, 0.0)
        }
    }
}

/// One slot of the strip this frame: which item, where it starts, and how far it runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarouselSlot {
    pub index: usize,
    /// Distance from the strip's leading edge to this item's, in dp.
    pub offset: f32,
    pub extent: f32,
}

/// The id of the strip a carousel scrolls, for an app that reads or sets its offset.
pub fn carousel_strip_id(id: Id) -> Id {
    id.with("strip")
}

/// The weights in play: a strip with fewer items than weights uses the first ones.
fn active_weights(kind: CarouselKind, count: usize) -> &'static [u32] {
    let all = kind.weights();
    &all[..all.len().min(count.max(1))]
}

/// What one unit of weight covers, with the gaps between a page's items taken out first.
fn extent_unit(kind: CarouselKind, usable: f32, count: usize) -> f32 {
    let w = active_weights(kind, count);
    if !kind.shrinking() || usable <= 0.0 {
        return 0.0;
    }
    let sum: u32 = w.iter().sum();
    let gaps = kind.gap() * w.len().saturating_sub(1) as f32;
    ((usable - gaps) / sum as f32).max(0.0)
}

/// The width one page of a weighted layout gives its leading slot: one unit of its weight.
fn leading_extent(kind: CarouselKind, usable: f32, count: usize) -> f32 {
    let w = active_weights(kind, count);
    w.first().copied().unwrap_or(0) as f32 * extent_unit(kind, usable, count)
}

/// How far one page moves the strip: one widest item, not the leading slot — a centred hero
/// leads with a small one, and that slot as a page would rush the whole strip past.
pub fn carousel_page(kind: CarouselKind, usable: f32, count: usize) -> f32 {
    let w = active_weights(kind, count);
    let widest = w.iter().copied().max().unwrap_or(0);
    widest as f32 * extent_unit(kind, usable, count)
}

/// How many leading weights sit before the first maximum one.
///
/// Such a strip expands its first item to the maximum weight, leaving that much space
/// before it — which is what centres a hero item.
fn leading_smaller(kind: CarouselKind, count: usize) -> usize {
    let w = active_weights(kind, count);
    let max = w.iter().copied().max().unwrap_or(0);
    w.iter().take_while(|weight| **weight != max).count()
}

/// The last page that still lays its items out from the leading edge.
fn last_full_page(kind: CarouselKind, count: usize) -> usize {
    let k = active_weights(kind, count).len();
    (count + leading_smaller(kind, count)).saturating_sub(k)
}

/// How many pages a strip spends walking its widest weight to the last slot.
///
/// Each step hands the focal role to the next item, so every item takes its turn as the
/// widest one before the strip ends; a pattern that already ends wide needs no step.
fn turning_pages(kind: CarouselKind, count: usize) -> usize {
    if !kind.shrinking() {
        return 0;
    }
    let w = active_weights(kind, count);
    w.len() - 1 - leading_smaller(kind, count)
}

/// The weights after `step` turns: the trailing weights move in front of the widest one.
fn turned_weights(w: &[u32], step: usize) -> Vec<u32> {
    let max = w.iter().copied().max().unwrap_or(0);
    let at = w.iter().position(|weight| *weight == max).unwrap_or(0);
    let tail = &w[at + 1..];
    let step = step.min(tail.len());
    let mut out = Vec::with_capacity(w.len());
    out.extend_from_slice(&w[..at]);
    out.extend(tail.iter().rev().take(step));
    out.push(max);
    out.extend_from_slice(&tail[step..]);
    out
}

/// The width a fixed-size layout gives each item: the large width, or the viewport when it
/// runs vertically.
fn fixed_extent(kind: CarouselKind, usable: f32, large: f32) -> f32 {
    if kind.vertical() {
        usable
    } else {
        large.max(1.0)
    }
}

/// The gutter a horizontal fixed-size strip keeps in front of its first item.
fn fixed_lead(kind: CarouselKind) -> f32 {
    if kind.vertical() { 0.0 } else { CAROUSEL_PAD }
}

/// The strip's full content length: what the scroller clamps against, minus the viewport.
///
/// It ends where the last item's trailing edge does, so the clamp lands flush at the end.
pub fn carousel_content(kind: CarouselKind, usable: f32, count: usize, large: f32) -> f32 {
    if count == 0 || usable <= 0.0 {
        return 0.0;
    }
    if kind.shrinking() {
        return usable + carousel_max_scroll(kind, usable, count, large);
    }
    let gaps = kind.gap() * count.saturating_sub(1) as f32;
    fixed_lead(kind) + count as f32 * fixed_extent(kind, usable, large) + gaps
}

/// The furthest the strip may scroll: where its last item ends on the trailing edge.
pub fn carousel_max_scroll(kind: CarouselKind, usable: f32, count: usize, large: f32) -> f32 {
    if count == 0 || usable <= 0.0 {
        return 0.0;
    }
    if !kind.shrinking() {
        return (carousel_content(kind, usable, count, large) - usable).max(0.0);
    }
    // Every item takes a turn at the leading edge, so the strip ends one page short of
    // one page per item: the last page is where the widest weight reaches the last slot.
    (count - 1) as f32 * carousel_page(kind, usable, count)
}

/// The scroll at which `index` is the strip's widest item.
///
/// The widest slot is the leading one, and one page hands it to the next item, so item `i`
/// holds it on page `i`; the turn walks it to the last item on the page that same item leads.
pub fn carousel_offset_of(
    kind: CarouselKind,
    usable: f32,
    count: usize,
    index: usize,
    large: f32,
) -> f32 {
    if !kind.shrinking() {
        let lead =
            fixed_lead(kind) + index as f32 * (fixed_extent(kind, usable, large) + kind.gap());
        return lead.min(carousel_max_scroll(kind, usable, count, large));
    }
    let page = carousel_page(kind, usable, count);
    (index as f32 * page).min(carousel_max_scroll(kind, usable, count, large))
}

/// How far one page press moves the strip: a page for a weighted layout, an item for a fixed
/// one.
pub fn carousel_step(kind: CarouselKind, usable: f32, count: usize, large: f32) -> f32 {
    if kind.shrinking() {
        carousel_page(kind, usable, count)
    } else {
        fixed_extent(kind, usable, large) + kind.gap()
    }
}

/// How far one full pass of the items covers: the period a looped strip repeats on.
pub fn carousel_cycle(kind: CarouselKind, usable: f32, count: usize, large: f32) -> f32 {
    if count == 0 || usable <= 0.0 {
        return 0.0;
    }
    count as f32 * carousel_step(kind, usable, count, large)
}

/// Passes a looped strip lays out: an end that is never reached on a canvas that needs one,
/// and few enough that item positions stay inside f32's exact range.
const LOOP_PASSES: usize = 101;

/// The canvas a looped strip is laid out on, and the furthest it may scroll.
pub fn carousel_looped_content(kind: CarouselKind, usable: f32, count: usize, large: f32) -> f32 {
    let cycle = carousel_cycle(kind, usable, count, large);
    (cycle * LOOP_PASSES as f32).max(cycle + usable)
}

pub fn carousel_looped_max(kind: CarouselKind, usable: f32, count: usize, large: f32) -> f32 {
    (carousel_looped_content(kind, usable, count, large) - usable).max(0.0)
}

/// Where a looped strip starts: the middle pass, so it can travel both ways from the first
/// frame.
pub fn carousel_looped_start(kind: CarouselKind, usable: f32, count: usize, large: f32) -> f32 {
    carousel_cycle(kind, usable, count, large) * (LOOP_PASSES / 2) as f32
}

/// The scroll that puts `index` in the middle of the viewport, or as near as the strip gets,
/// chosen among the copies of it nearest `near`.
pub fn carousel_centre_of(
    kind: CarouselKind,
    usable: f32,
    count: usize,
    index: usize,
    near: f32,
    large: f32,
    looped: bool,
) -> f32 {
    if count == 0 || usable <= 0.0 || index >= count {
        return 0.0;
    }
    let step = carousel_step(kind, usable, count, large);
    let max = if looped {
        carousel_looped_max(kind, usable, count, large)
    } else {
        carousel_max_scroll(kind, usable, count, large)
    };
    if step <= 0.0 {
        return 0.0;
    }
    // The item reaches the middle during the page before it leads, so that page is the whole
    // search range; a looped strip takes the copy one pass along so the range stays inside.
    let pass = if looped {
        carousel_cycle(kind, usable, count, large)
    } else {
        0.0
    };
    let lead = (index + leading_smaller(kind, count)) as f32 * step + pass;
    let lo = (lead - step).max(0.0);
    let hi = lead.min(max).max(lo);
    let half = usable / 2.0;
    let centre = |at: f32| -> f32 {
        slots_at(kind, usable, count, at, large, looped)
            .iter()
            .find(|slot| slot.index == index)
            .map_or(
                if at < lead {
                    f32::INFINITY
                } else {
                    f32::NEG_INFINITY
                },
                |slot| slot.offset + slot.extent / 2.0,
            )
    };
    // The item reaches the middle somewhere in that page; when it cannot (a strip end holds
    // it short), the nearest end of the range is the answer.
    let found = if centre(lo) <= half {
        lo
    } else if centre(hi) >= half {
        hi
    } else {
        let (mut a, mut b) = (lo, hi);
        for _ in 0..24 {
            let mid = (a + b) / 2.0;
            if centre(mid) > half {
                a = mid;
            } else {
                b = mid;
            }
        }
        (a + b) / 2.0
    };
    if looped {
        // The layout repeats every pass, so any copy of the answer is the same view.
        let cycle = carousel_cycle(kind, usable, count, large);
        found + ((near - found) / cycle).round() * cycle
    } else {
        found
    }
}

/// The slots that reach the viewport at `scroll`, in paint order.
///
/// A weighted strip shrinks its leading item away; its last page turns the pattern around,
/// so it can end on its widest item. A fixed one keeps every item and slides the strip.
pub fn carousel_slots(
    kind: CarouselKind,
    usable: f32,
    count: usize,
    scroll: f32,
    large: f32,
) -> Vec<CarouselSlot> {
    slots_at(kind, usable, count, scroll, large, false)
}

/// The slots of a strip that repeats its items for ever, so it has no first or last one.
pub fn carousel_slots_looped(
    kind: CarouselKind,
    usable: f32,
    count: usize,
    scroll: f32,
    large: f32,
) -> Vec<CarouselSlot> {
    slots_at(kind, usable, count, scroll, large, true)
}

fn slots_at(
    kind: CarouselKind,
    usable: f32,
    count: usize,
    scroll: f32,
    large: f32,
    looped: bool,
) -> Vec<CarouselSlot> {
    if count == 0 || usable <= 0.0 {
        return Vec::new();
    }
    let max = if looped {
        carousel_looped_max(kind, usable, count, large)
    } else {
        carousel_max_scroll(kind, usable, count, large)
    };
    let scroll = scroll.clamp(0.0, max);
    if !kind.shrinking() {
        let extent = fixed_extent(kind, usable, large);
        let lead = fixed_lead(kind);
        let step = extent + kind.gap();
        // Every item keeps its place on the canvas, so the whole strip slides by `scroll`.
        if !looped {
            return (0..count)
                .map(|index| CarouselSlot {
                    index,
                    offset: lead + index as f32 * step - scroll,
                    extent,
                })
                .collect();
        }
        // A looped strip only needs the items the viewport can reach; the rest repeat.
        let mut slots = Vec::new();
        let mut i = ((scroll - lead) / step).floor() as i64 - 1;
        loop {
            let offset = lead + i as f32 * step - scroll;
            if offset >= usable {
                break;
            }
            if offset + extent > 0.0 {
                slots.push(CarouselSlot {
                    index: i.rem_euclid(count as i64) as usize,
                    offset,
                    extent,
                });
            }
            i += 1;
        }
        return slots;
    }
    let w = active_weights(kind, count);
    let k = w.len();
    let unit = extent_unit(kind, usable, count);
    let first = leading_extent(kind, usable, count);
    let pitch = carousel_page(kind, usable, count);
    if unit <= 0.0 || first <= 0.0 || pitch <= 0.0 {
        return Vec::new();
    }
    if !looped {
        let steps = turning_pages(kind, count);
        let start = last_full_page(kind, count) as f32 * pitch;
        if steps > 0 && scroll >= start - 0.01 {
            // The turn is one page per step, so a strip whose widest weight is already last
            // has none, and a leading-anchored one takes as many as it has weights behind it.
            let walked = ((scroll - start) / pitch).clamp(0.0, steps as f32);
            let step = (walked.floor() as usize).min(steps - 1);
            return turned_slots(kind, usable, count, step, walked - step as f32);
        }
    }
    let max_weight = w.iter().copied().max().unwrap_or(1) as f32;
    let max_extent = max_weight * unit;
    let page = (scroll / pitch).floor();
    let progress = (scroll - page * pitch) / pitch;
    // The leading slot may be a phantom before item 0: a centred hero expands its first
    // item into the maximum weight and leaves that much space in front of it.
    let head = page as i64 - leading_smaller(kind, count) as i64;
    let mut slots = Vec::with_capacity(k + 1);
    let mut cursor = 0.0_f32;
    for j in 0.. {
        if cursor >= usable - 0.01 {
            break;
        }
        // A slot owns the gap behind it; the card is what the slot has left. The leading
        // slot shrinks whole, so its pad stays a pad until the card is gone.
        let (extent, slot) = if j == 0 {
            let slot = (first + kind.gap()) * (1.0 - progress);
            ((slot - kind.gap()).max(0.0), slot)
        } else if j < k {
            let weight = w[j] as f32;
            // The item behind the leading one grows toward the weight in front of it, so
            // the two meet when the page turns.
            let extent =
                weight * unit + (w[j - 1] as f32 - weight) / max_weight * progress * max_extent;
            (extent, extent + kind.gap())
        } else {
            // Past the weights, an item only fills what the viewport has left.
            let extent = (usable - cursor).max(0.0);
            (extent, extent)
        };
        let index = head + j as i64;
        let within = index >= 0 && (index as usize) < count;
        if extent > 0.0 && (looped || within) {
            let index = if looped {
                index.rem_euclid(count as i64)
            } else {
                index
            };
            slots.push(CarouselSlot {
                index: index as usize,
                offset: cursor,
                extent,
            });
        }
        cursor += slot;
    }
    slots
}

/// One step of the turn: the sizes trade places as the widest weight walks to the last slot,
/// keeping the page exactly full, so the strip ends wide and flush.
fn turned_slots(
    kind: CarouselKind,
    usable: f32,
    count: usize,
    step: usize,
    t: f32,
) -> Vec<CarouselSlot> {
    let w = active_weights(kind, count);
    let from_w = turned_weights(w, step);
    let to_w = turned_weights(w, step + 1);
    let k = w.len();
    let unit = extent_unit(kind, usable, count);
    let mut slots = Vec::with_capacity(k);
    let mut cursor = 0.0_f32;
    for j in 0..k {
        let from = from_w[j] as f32 * unit;
        let to = to_w[j] as f32 * unit;
        let extent = lerp(from, to, t);
        if extent > 0.0 {
            slots.push(CarouselSlot {
                index: count - k + j,
                offset: cursor,
                extent,
            });
        }
        cursor += extent + kind.gap();
    }
    slots
}

/// One item: the id the app knows it by, and what it holds.
#[derive(Clone)]
pub struct CarouselItem {
    pub id: Id,
    pub content: Vec<Decl>,
    /// The export's `with-outline` state: a 1dp border around the item.
    pub outlined: bool,
}

impl CarouselItem {
    pub fn new(id: Id, content: Vec<Decl>) -> Self {
        Self {
            id,
            content,
            outlined: false,
        }
    }

    pub fn outlined(mut self) -> Self {
        self.outlined = true;
        self
    }
}

/// Everything a carousel is built with. Kind and size are fixed at build time.
pub struct CarouselParams<'a> {
    pub id: Id,
    pub kind: CarouselKind,
    pub items: &'a [CarouselItem],
    pub width: f32,
    pub height: f32,
    /// The width a large item asks for in the fixed-size layouts.
    pub large: f32,
    /// Repeat the items for ever instead of ending on the last one.
    pub infinite: bool,
}

/// The resolved surface of one item for this frame.
struct Face {
    fill: Color,
    shadow: Option<ShadowSpec>,
    layer: Option<Color>,
    press: Option<PressInk>,
    outline: Option<(Color, f32)>,
    ring: Option<(Color, f32)>,
}

/// A carousel. Its offset lives in the engine, keyed by the strip's id, so each one scrolls
/// on its own and is clamped against its own content.
pub struct Md3Carousel {
    id: Id,
    kind: CarouselKind,
    width: f32,
    height: f32,
    large: f32,
    infinite: bool,
    items: Vec<CarouselItem>,
    faces: Vec<Face>,
    disabled: bool,
    /// The item whose press landed this frame.
    fired: Option<Id>,
}

impl Md3Carousel {
    pub fn new_with(
        params: Md3Options,
        content: CarouselParams<'_>,
        cx: &mut Cx<'_, '_>,
        pack: &Style,
    ) -> Self {
        let CarouselParams {
            id,
            kind,
            items,
            width,
            height,
            large,
            infinite,
        } = content;
        let faces = items
            .iter()
            .map(|item| face(item, params.disabled, cx, pack))
            .collect::<Vec<_>>();
        let disabled = params.disabled || items.iter().any(|item| cx.state(item.id).disabled);
        let fired = items
            .iter()
            .map(|item| item.id)
            .find(|item| cx.state(*item).activation());
        Self {
            id,
            kind,
            width,
            height,
            large,
            infinite,
            items: items.to_vec(),
            faces,
            disabled,
            fired,
        }
    }

    /// The layout this carousel was built as.
    pub fn kind(&self) -> CarouselKind {
        self.kind
    }

    /// The width one page of scrolling covers, for an app that snaps.
    pub fn page(&self) -> f32 {
        carousel_page(self.kind, self.usable(), self.items.len())
    }

    /// The furthest this carousel can scroll.
    pub fn max_scroll(&self) -> f32 {
        let (usable, count) = (self.usable(), self.items.len());
        if self.infinite {
            carousel_looped_max(self.kind, usable, count, self.large)
        } else {
            carousel_max_scroll(self.kind, usable, count, self.large)
        }
    }

    /// The scroll that brings item `index` into the middle of the viewport, from `near`.
    pub fn centre_scroll(&self, index: usize, near: f32) -> f32 {
        carousel_centre_of(
            self.kind,
            self.usable(),
            self.items.len(),
            index,
            near,
            self.large,
            self.infinite,
        )
    }

    fn usable(&self) -> f32 {
        if self.kind.vertical() {
            self.height
        } else if self.kind.shrinking() {
            (self.width - 2.0 * CAROUSEL_PAD).max(0.0)
        } else {
            self.width
        }
    }

    /// Move the strip at once, along its own axis.
    fn jump(&self, cx: &Cx<'_, '_>, strip: Id, at: f32) {
        if self.kind.vertical() {
            cx.core().set_scroll(strip, 0.0, at);
        } else {
            cx.core().set_scroll(strip, at, 0.0);
        }
    }

    /// Move the strip under the engine's spring, along its own axis.
    fn settle(&self, cx: &Cx<'_, '_>, strip: Id, at: f32) {
        if self.kind.vertical() {
            cx.core().scroll_to(strip, 0.0, at);
        } else {
            cx.core().scroll_to(strip, at, 0.0);
        }
    }

    fn item_decl(&self, slot: &CarouselSlot, inner: (f32, f32), scroll: f32) -> Decl {
        let face = &self.faces[slot.index];
        let item = &self.items[slot.index];
        let (w, h) = if self.kind.vertical() {
            (inner.0, slot.extent)
        } else {
            (slot.extent, inner.1)
        };
        // Slot offsets are measured from the viewport's leading edge; the canvas itself is
        // scrolled back by `scroll`, so the two add up to the item's place on the canvas.
        let at = slot.offset + scroll;
        let placement = if self.kind.vertical() {
            Insets {
                left: 0.0,
                top: at,
                right: AUTO,
                bottom: AUTO,
            }
        } else {
            Insets {
                left: at,
                top: 0.0,
                right: AUTO,
                bottom: AUTO,
            }
        };
        let mut shapes = Vec::with_capacity(3);
        shapes.push(Shape::Box {
            corners: Corners::all(CAROUSEL_RADIUS),
            fill: face.fill,
            gradient: None,
            border: face.outline,
            shadow: face.shadow,
            image: None,
        });
        if let Some(layer) = face.layer {
            shapes.push(Shape::Box {
                corners: Corners::all(CAROUSEL_RADIUS),
                fill: layer,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            });
        }
        // The export puts the ring `focus.indicator.outline.offset` outside the item; it is
        // drawn on the item's edge instead, since the decl cannot know the final rect.
        if let Some((ink, width)) = face.ring {
            shapes.push(Shape::Box {
                corners: Corners::all(CAROUSEL_RADIUS),
                fill: Color::TRANSPARENT,
                gradient: None,
                border: Some((ink, width)),
                shadow: None,
                image: None,
            });
        }
        Decl::container(item.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(w),
                height: AxisSize::fixed(h),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .absolute()
            .inset(placement)
            .shapes(shapes)
            // The item's own content must be cut to its rounded box, or a square child
            // fill paints over the 28dp corners the item declares.
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .sense(Sense::HOVER | Sense::CLICK | Sense::FOCUSABLE)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::PointingHand
            })
            .disabled(self.disabled)
            .press_ink(face.press)
            .children(item.content.iter().cloned())
    }
}

/// Resolve one item's surface for this frame.
fn face(item: &CarouselItem, disabled: bool, cx: &mut Cx<'_, '_>, pack: &Style) -> Face {
    let mut node = cx.at(item.id);
    let state = node.state();
    let disabled = disabled || state.disabled;
    let rest = carousel_item_spec(CarouselItemFlags {
        with_outline: item.outlined,
        hovered: state.hovered,
        focused: state.focus_visible,
        pressed: state.pressed,
        disabled,
    });
    let hover = carousel_item_spec(CarouselItemFlags {
        with_outline: item.outlined,
        hovered: true,
        disabled,
        ..CarouselItemFlags::NONE
    });
    let focus = carousel_item_spec(CarouselItemFlags {
        with_outline: item.outlined,
        focused: true,
        disabled,
        ..CarouselItemFlags::NONE
    });
    let press = carousel_item_spec(CarouselItemFlags {
        with_outline: item.outlined,
        pressed: true,
        disabled,
        ..CarouselItemFlags::NONE
    });
    let hover_t = node.animate(
        item.id.with(param::HOVER),
        f32::from(state.hovered),
        REACTION,
    );
    let focus_t = node.animate(
        item.id.with(param::FOCUS),
        f32::from(state.focus_visible),
        REACTION,
    );
    // The base state names no state layer; the ink lives on the state that has one.
    let layer_ink = role(pack, hover.state_layer.color);
    let strength = (hover_t * hover.state_layer.opacity).max(focus_t * focus.state_layer.opacity);
    let layer = (strength > 0.0 && !disabled).then_some(Color {
        a: layer_ink.a * strength,
        ..layer_ink
    });
    // The background box is always the first shape the item paints, so the ripple binds
    // to that shape rather than to whatever the node happens to draw this frame.
    let ink = (!disabled).then(|| {
        press_ink_shape(
            Color {
                a: layer_ink.a * press.state_layer.opacity,
                ..layer_ink
            },
            InkRadius::Cover,
            0,
        )
    });
    let ring_t = node.animate(
        item.id.with(param::THICKNESS),
        f32::from(state.focus_visible),
        REACTION,
    );
    let ring = (ring_t > 0.0 && !disabled).then(|| {
        (
            spec_ink(pack, rest.focus_indicator.color, ring_t),
            rest.focus_indicator.thickness,
        )
    });
    Face {
        fill: spec_ink(pack, rest.container.color, rest.container.opacity),
        shadow: (rest.container.elevation > 0.0).then(|| {
            ShadowSpec::black(
                rest.container.elevation * 2.0,
                0.18,
                rest.container.elevation * 0.5,
            )
        }),
        layer,
        press: ink,
        outline: (rest.outline.width > 0.0 && !rest.outline.color.is_empty()).then(|| {
            (
                spec_ink(pack, rest.outline.color, rest.outline.opacity),
                rest.outline.width,
            )
        }),
        ring,
    }
}

impl Widget for Md3Carousel {
    fn handles_disabled(&self) -> bool {
        // The export carries the disabled container itself, so the adapter must not fade again.
        true
    }

    fn change(&self, _state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        // The items are decls, so their own `change()` never runs; the carousel names the
        // one that was pressed or the app cannot tell one item from another.
        self.fired.map(|id| Change::Selected(Some(id)))
    }

    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let (gx, gy) = self.kind.gutter();
        let inner = (
            (self.width - 2.0 * gx).max(0.0),
            (self.height - 2.0 * gy).max(0.0),
        );
        let usable = self.usable();
        let strip_id = carousel_strip_id(self.id);
        // The engine holds the offset and clamps it against the canvas's extent, so the
        // widget reads it rather than owning it.
        let known = cx.core().scroll_of(strip_id);
        let mut offset = known
            .map(|g| {
                if self.kind.vertical() {
                    g.offset_y
                } else {
                    g.offset_x
                }
            })
            .unwrap_or(0.0);
        let count = self.items.len();
        // A looped strip starts on the middle pass so it can travel both ways; the first
        // frame still lays out at 0, where the engine is, not where it is about to be.
        if self.infinite {
            let cycle = carousel_cycle(self.kind, usable, count, self.large);
            let start = carousel_looped_start(self.kind, usable, count, self.large);
            if cycle > 0.0 {
                if known.is_none() {
                    self.jump(cx, strip_id, start);
                } else if offset < cycle {
                    offset += start;
                    self.jump(cx, strip_id, offset);
                }
            }
        }
        // The strip lays itself out at the offset it is allowed to reach; the engine adds its
        // own overscroll on top, and that difference is the rubber band.
        let max = if self.infinite {
            carousel_looped_max(self.kind, usable, count, self.large)
        } else {
            carousel_max_scroll(self.kind, usable, count, self.large)
        };
        let laid_out = offset.clamp(0.0, max);
        let (slots, content) = if self.infinite {
            let slots = carousel_slots_looped(self.kind, usable, count, laid_out, self.large);
            (
                slots,
                carousel_looped_content(self.kind, usable, count, self.large),
            )
        } else {
            let slots = carousel_slots(self.kind, usable, count, laid_out, self.large);
            (
                slots,
                carousel_content(self.kind, usable, count, self.large),
            )
        };
        // A press brings its item into the middle: the strip answers where the item sits,
        // not where the press landed.
        if let Some(index) = self
            .fired
            .and_then(|id| self.items.iter().position(|it| it.id == id))
        {
            let target = self.centre_scroll(index, offset);
            if (target - offset).abs() > 0.5 {
                self.settle(cx, strip_id, target);
            }
        }
        let strip_kind = if self.kind.vertical() {
            NodeKind::Column
        } else {
            NodeKind::Row
        };
        // Only the uncontained strip keeps a vertical gutter; the others already have one
        // from the container's padding, and full-screen has none at all.
        let (inner, canvas_pad) = if self.kind.shrinking() || self.kind.vertical() {
            (inner, Insets::ZERO)
        } else {
            let pad = Insets {
                left: 0.0,
                top: CAROUSEL_VPAD,
                right: 0.0,
                bottom: CAROUSEL_VPAD,
            };
            ((inner.0, (inner.1 - 2.0 * CAROUSEL_VPAD).max(0.0)), pad)
        };
        // The canvas is the one in-flow child, sized to the whole strip, and the items hang
        // off it out of flow: the scroller measures the canvas, so its clamp is our max.
        let canvas_size = if self.kind.vertical() {
            SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(content),
                grow_x: Grow::No,
                grow_y: Grow::No,
            }
        } else {
            SizeClass {
                width: AxisSize::fixed(content),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            }
        };
        // Positioned, so the items hang off the canvas and travel with the strip: a static
        // parent would leave their containing block at the root.
        let mut canvas = Decl::container(self.id.with("canvas"), NodeKind::Box)
            .size(canvas_size)
            .padding(canvas_pad)
            .relative();
        for slot in &slots {
            canvas = canvas.child(self.item_decl(slot, inner, laid_out));
        }
        let strip = Decl::container(strip_id, strip_kind)
            .size(SizeClass {
                width: AxisSize::fixed(inner.0),
                height: AxisSize::fixed(inner.1),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .scrollable()
            .sense(Sense::HOVER | Sense::SCROLL_OWNED)
            .cursor(if self.disabled {
                CursorShape::Arrow
            } else {
                CursorShape::Grab
            })
            .disabled(self.disabled)
            .child(canvas);
        Decl::container(self.id, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(self.width),
                height: AxisSize::fixed(self.height),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets {
                left: gx,
                top: gy,
                right: gx,
                bottom: gy,
            })
            .disabled(self.disabled)
            .child(strip)
    }
}
