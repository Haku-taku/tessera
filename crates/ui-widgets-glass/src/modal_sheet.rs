//! The glass modal sheet: a bottom sheet that rests at a half or a full detent.

use ui_core::geom::{Color, CornerShape, Corners, Insets, lerp};
use ui_core::layout::{AxisSize, Clip, Grow, Len, LenInsets, Overflow, OverlayAlign, SizeClass};
use ui_core::shortcut::{KeyChord, ShortcutId};
use ui_core::tree::ModalTarget;
use ui_core::{Change, CursorShape, Decl, Id, Key, Modifiers, NodeKind, Sense};
use ui_style::Style;
use ui_widgets::build::{DRAG_FOLLOW, IMMEDIATE};
use ui_widgets::{Cx, Widget, WidgetState};

use crate::material::{Control, glass_recipe};
use crate::shared::glass_backdrop;

/// `halfSize`: 45 % of the screen height.
pub const SHEET_HALF: f32 = 0.45;
/// `fullSize` defaults to the screen height less 90 dp (iOS page-sheet style).
pub const SHEET_FULL_TOP: f32 = 90.0;
/// `horizontalMargin` at the half detent; the full detent runs edge to edge.
pub const SHEET_MARGIN: f32 = 5.0;
/// `bottomMargin` at the half detent, shrinking to 0 as the sheet fills.
pub const SHEET_BOTTOM: f32 = 6.0;
/// `topBorderRadius` and `fullTopBorderRadius`.
pub const SHEET_TOP_RADIUS: f32 = 56.0;
pub const SHEET_TOP_RADIUS_FULL: f32 = 46.0;
/// `GlassDefaults.barrierColor` — 54 % black.
pub const SHEET_DIM: f32 = 0.54;
/// `snapThreshold`: how far into a segment a drag settles on the far detent.
pub const SHEET_SNAP: f32 = 0.4;
/// `resistance`: how much of the pull past an end detent the sheet shows.
pub const SHEET_RESISTANCE: f32 = 0.08;
/// The corners finish wrapping at 92 % of the way to full; the rest is the sink.
const WRAP: f32 = 0.92;
/// The drag handle: 36×4, 8 dp below the top edge and 8 dp above the content.
pub const SHEET_HANDLE_W: f32 = 36.0;
pub const SHEET_HANDLE_H: f32 = 4.0;
pub const SHEET_HANDLE_TOP: f32 = 8.0;
pub const SHEET_HANDLE_GAP: f32 = 8.0;
/// iOS 26's handle: white at 35 % in dark, black at 20 % in light.
pub const SHEET_HANDLE_DARK: f32 = 0.35;
pub const SHEET_HANDLE_LIGHT: f32 = 0.20;
/// `SpringDescription(mass: 1, stiffness: 220, damping: 30)`, the sheet's own snap.
const SNAP_K: f32 = 220.0;
const SNAP_C: f32 = 30.0;
/// The id Escape is bound to.
pub const SHEET_DISMISS_SHORTCUT: ShortcutId = ShortcutId("glass.modal_sheet.dismiss");

/// A rest height the sheet can occupy; the reference's `GlassSheetDetent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassSheetDetent {
    Half,
    Full,
}

/// Everything a modal sheet is built with.
pub struct GlassModalSheetParams {
    pub id: Id,
    /// The app's blocks, below the drag handle.
    pub content: Vec<Decl>,
    /// The content's own inset; the reference defaults to none.
    pub padding: Insets,
    /// Ids inside `content` whose press the sheet reports; their widgets are the app's.
    pub presses: Vec<Id>,
    /// Which rest heights this sheet offers, low to high; empty means both.
    pub detents: Vec<GlassSheetDetent>,
    /// `halfSize`: a fraction of the window height, or pixels when above 1.
    pub half: f32,
    /// `fullSize`: the same units; `None` is the window height less 90 dp.
    pub full: Option<f32>,
    /// The detent the sheet opens at.
    pub initial: GlassSheetDetent,
    /// Whether a drag below the lowest detent, a press outside or Escape closes it.
    pub dismissible: bool,
    /// Whether the sheet dims and blocks what is behind it.
    pub barrier: bool,
    pub target: ModalTarget,
}

impl GlassModalSheetParams {
    pub fn new(id: Id, content: Vec<Decl>) -> Self {
        Self {
            id,
            content,
            padding: Insets::ZERO,
            presses: Vec::new(),
            detents: vec![GlassSheetDetent::Half, GlassSheetDetent::Full],
            half: SHEET_HALF,
            full: None,
            initial: GlassSheetDetent::Half,
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

    pub fn detents(mut self, detents: Vec<GlassSheetDetent>) -> Self {
        self.detents = detents;
        self
    }

    pub fn half(mut self, half: f32) -> Self {
        self.half = half;
        self
    }

    pub fn full(mut self, full: f32) -> Self {
        self.full = Some(full);
        self
    }

    pub fn initial(mut self, initial: GlassSheetDetent) -> Self {
        self.initial = initial;
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

/// Where a released drag settles; `Hidden` is the dismissal the reference calls that state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rest {
    Hidden,
    At(usize),
}

/// A modal sheet: the whole tree is resolved once per frame, since its position is a spring.
pub struct GlassModalSheet {
    body: Decl,
    /// The content id pressed this frame, if any.
    fired: Option<Id>,
    /// The detent a drag settled on this frame, if it changed.
    settled: Option<Id>,
    dismissed: bool,
}

impl GlassModalSheet {
    pub fn new(params: GlassModalSheetParams, cx: &mut Cx<'_, '_>, pack: &Style) -> Self {
        let GlassModalSheetParams {
            id,
            content,
            padding,
            presses,
            detents,
            half,
            full,
            initial,
            dismissible,
            barrier,
            target,
        } = params;
        let sheet_id = id.with("sheet");

        // ── The window height is last frame's root rect; a drag needs a previous frame anyway,
        // ── so the one frame without it only delays the initial placement.
        let window_h = cx.rect_of(id).map_or(0.0, |r| r.size.h);
        let state = cx.state(sheet_id);
        let step = state.drag_step;
        let fired = presses.iter().copied().find(|p| cx.state(*p).clicked);

        // ── The ladder of rest positions, low to high, plus `hidden` at 0 when dismissible.
        let mut rungs: Vec<Rest> = Vec::new();
        if dismissible {
            rungs.push(Rest::Hidden);
        }
        let mut levels: Vec<GlassSheetDetent> = Vec::new();
        for d in [GlassSheetDetent::Half, GlassSheetDetent::Full] {
            if detents.is_empty() || detents.contains(&d) {
                levels.push(d);
            }
        }
        if levels.is_empty() {
            levels.push(GlassSheetDetent::Half);
        }
        for (k, _) in levels.iter().enumerate() {
            rungs.push(Rest::At(k));
        }
        let at: Vec<f32> = levels
            .iter()
            .map(|d| match d {
                GlassSheetDetent::Half => fraction(half, window_h, SHEET_HALF),
                GlassSheetDetent::Full => match full {
                    Some(v) => fraction(v, window_h, SHEET_HALF),
                    None if window_h > 0.0 => 1.0 - SHEET_FULL_TOP / window_h,
                    None => SHEET_HALF,
                },
            })
            .collect();
        let position = |r: Rest| match r {
            Rest::Hidden => 0.0,
            Rest::At(k) => at[k],
        };
        // The rubber band starts at the lowest rest state, which is `hidden` when dismissible.
        let lo = position(rungs[0]);
        let hi = position(rungs[rungs.len() - 1]);

        let initial_index = levels.iter().position(|d| *d == initial).unwrap_or(0);
        // `raw` carries the drag's unclamped travel, `snap` the position actually shown.
        let raw_id = sheet_id.with("raw");
        let snap_id = sheet_id.with("snap");
        let live_id = sheet_id.with("live");
        let detent_id = sheet_id.with("detent");
        let primed_id = sheet_id.with("primed");

        let mut node = cx.at(id);
        let primed = node.value(primed_id) > 0.5;
        let mut index = if primed {
            (node.value(detent_id) as usize).min(levels.len() - 1)
        } else {
            initial_index
        };
        let mut pos = node.spring_value(snap_id).unwrap_or(at[index]);
        // The first frame with a window height places the sheet at its initial detent, since
        // the frame before it had no height to measure the full detent against.
        if !primed && window_h > 0.0 {
            node.forget_spring(snap_id);
            pos = node.spring(snap_id, at[index], SNAP_K, SNAP_C).x;
            node.animate(detent_id, index as f32, IMMEDIATE);
            node.animate(primed_id, 1.0, IMMEDIATE);
        }

        // ── A drag moves the sheet by the pointer's own travel, resisted past the ends; a
        // ── release settles it on a detent or dismisses it.
        let mut settled = None;
        let mut dismissed = false;
        if state.dragging && window_h > 0.0 {
            if node.value(live_id) <= 0.5 {
                node.animate(live_id, 1.0, IMMEDIATE);
                // The press banks where the sheet stands, so the drag moves from there.
                node.animate(raw_id, pos, IMMEDIATE);
            }
            // While the finger owns the sheet, `raw` is where it stands; the spring is parked.
            pos = node.value(raw_id);
            if let Some(step) = step {
                let raw = pos - step.y / window_h;
                pos = node.animate(raw_id, raw, DRAG_FOLLOW);
            }
        } else if node.value(live_id) > 0.5 {
            node.animate(live_id, 0.0, IMMEDIATE);
            // Release starts where the finger left it: the spring still holds the pre-drag
            // position, so reading it here would snap the sheet back before settling.
            let released = node.value(raw_id);
            let shown = resist(released, lo, hi);
            let from = Rest::At(index);
            let rest = resolve(shown, from, &rungs, &position);
            match rest {
                Rest::Hidden => dismissed = true,
                Rest::At(k) if k != index => {
                    index = k;
                    node.animate(detent_id, index as f32, IMMEDIATE);
                    settled = Some(detent_id_of(id, levels[index]));
                }
                Rest::At(_) => {}
            }
            // Re-seed the spring where the finger left it: its stored state is pre-drag, so
            // letting it run from there would snap the sheet back before it sprang.
            node.forget_spring(snap_id);
            pos = node.spring(snap_id, shown, SNAP_K, SNAP_C).x;
        } else {
            pos = node.spring(snap_id, at[index], SNAP_K, SNAP_C).x;
        }
        let shown = resist(pos, lo, hi);

        // ── Geometry: the sheet fills `shown` of the window and its margins and corners wrap
        // ── toward the full detent; the top edge is what the insets express.
        let t = if levels.len() > 1 {
            let (p1, p2) = (at[0], at[1]);
            if p2 - p1 > 1e-4 {
                ((shown - p1) / (p2 - p1)).clamp(0.0, 1.0)
            } else {
                1.0
            }
        } else if levels[0] == GlassSheetDetent::Full {
            1.0
        } else {
            0.0
        };
        let wrap = (t / WRAP).clamp(0.0, 1.0);
        let h_pad = lerp(SHEET_MARGIN, 0.0, wrap);
        let bottom = lerp(SHEET_BOTTOM, 0.0, wrap);
        let top_radius = lerp(SHEET_TOP_RADIUS, SHEET_TOP_RADIUS_FULL, t);
        let corners = Corners {
            tl: top_radius,
            tr: top_radius,
            br: 0.0,
            bl: 0.0,
            shape: CornerShape::ROUND,
        };

        let dark = pack.appearance() == Some(ui_design::Appearance::Dark);
        let handle_ink = Color {
            a: if dark {
                SHEET_HANDLE_DARK
            } else {
                SHEET_HANDLE_LIGHT
            },
            ..if dark { Color::WHITE } else { Color::BLACK }
        };

        let mut sheet = Decl::container(sheet_id, NodeKind::Column)
            .fixed()
            .inset(LenInsets::new(
                Len::Px(h_pad),
                Len::Pct((1.0 - shown) * 100.0),
                Len::Px(h_pad),
                Len::Px(bottom),
            ))
            .padding(Insets {
                left: 0.0,
                top: SHEET_HANDLE_TOP,
                right: 0.0,
                bottom: 0.0,
            })
            .gap(SHEET_HANDLE_GAP)
            .cross_align(1.0)
            .corners(corners)
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .background(Color::TRANSPARENT, corners)
            .sense(Sense::DRAG)
            .cursor(CursorShape::Arrow);
        // The reference's sheet glass is `thickness: 10` with a barely-perceptible rim.
        let mut glass = glass_recipe(pack, Control::Container);
        glass.refractive_index = 1.15;
        if let Some(backdrop) = glass_backdrop(&glass, 1.0)
            && let Some(p) = sheet.data.params_mut()
        {
            p.backdrop_blur = Some(backdrop);
        }
        sheet = sheet.child(
            Decl::container(sheet_id.with("handle-row"), NodeKind::Stack)
                .stack_align(OverlayAlign::Center)
                .child(
                    Decl::container(sheet_id.with("handle"), NodeKind::Box)
                        .size(SizeClass::fixed(SHEET_HANDLE_W, SHEET_HANDLE_H))
                        .background(handle_ink, Corners::all(SHEET_HANDLE_H * 0.5)),
                ),
        );
        let mut inner = Decl::container(sheet_id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::Yes,
            })
            .padding(padding);
        for block in content {
            inner = inner.child(block);
        }
        sheet = sheet.child(inner);

        let body = if !barrier {
            sheet
        } else {
            // The scrim subscribes: a press outside the sheet must reach `change()`.
            let mut root = Decl::container(id, NodeKind::Box)
                .fixed()
                .inset(LenInsets::new(
                    Len::Px(0.0),
                    Len::Px(0.0),
                    Len::Px(0.0),
                    Len::Px(0.0),
                ))
                .modal(target)
                .sense(Sense::CLICK | Sense::FOCUSABLE)
                .shortcut(
                    KeyChord::new(Key::Escape, Modifiers::NONE),
                    SHEET_DISMISS_SHORTCUT,
                )
                .cursor(CursorShape::Arrow)
                .background(
                    Color {
                        a: SHEET_DIM,
                        ..Color::BLACK
                    },
                    Corners::ZERO,
                );
            root = root.child(sheet);
            root
        };
        // A press inside the sheet reaches the root's own click sense too, so only a press
        // that missed the sheet is a dismissal.
        let dismissed =
            dismissed || (dismissible && cx.state(id).clicked && !cx.state(sheet_id).clicked);
        Self {
            body,
            fired,
            settled,
            dismissed,
        }
    }
}

/// A detent's covered fraction, from the reference's fraction-or-pixels input.
fn fraction(value: f32, window_h: f32, fallback: f32) -> f32 {
    if value > 1.0 {
        if window_h > 0.0 {
            value / window_h
        } else {
            fallback
        }
    } else {
        value
    }
}

/// `applyResistance`: past an end detent the sheet shows 8 % of the pull.
fn resist(pos: f32, lo: f32, hi: f32) -> f32 {
    if pos < lo {
        lo - (lo - pos) * SHEET_RESISTANCE
    } else if pos > hi {
        hi + (pos - hi) * SHEET_RESISTANCE
    } else {
        pos
    }
}

/// The reference's `resolveTarget` without its velocity branches, which the engine cannot feed.
fn resolve(pos: f32, from: Rest, rungs: &[Rest], position: &dyn Fn(Rest) -> f32) -> Rest {
    let (first, last) = (rungs[0], rungs[rungs.len() - 1]);
    if pos <= position(first) {
        return first;
    }
    if pos >= position(last) {
        return last;
    }
    for pair in rungs.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (p1, p2) = (position(a), position(b));
        if pos < p1 || pos > p2 || p2 - p1 <= 1e-4 {
            continue;
        }
        let progress = (pos - p1) / (p2 - p1);
        // Coming from one end of the segment, the threshold decides; from neither, the
        // nearest one does.
        return if from == a {
            if progress >= SHEET_SNAP { b } else { a }
        } else if from == b {
            if 1.0 - progress >= SHEET_SNAP { a } else { b }
        } else if progress >= 0.5 {
            b
        } else {
            a
        };
    }
    from
}

/// The id a detent is reported under, relative to the id the sheet was built with.
pub fn detent_id_of(sheet: Id, detent: GlassSheetDetent) -> Id {
    match detent {
        GlassSheetDetent::Half => sheet.with("detent-half"),
        GlassSheetDetent::Full => sheet.with("detent-full"),
    }
}

impl Widget for GlassModalSheet {
    fn handles_disabled(&self) -> bool {
        // A sheet has no disabled state; the adapter must not fade it either.
        true
    }

    fn change(&self, state: &WidgetState, _bounds: ui_core::geom::Rect) -> Option<Change> {
        if let Some(id) = self.fired {
            return Some(Change::Selected(Some(id)));
        }
        // Escape is bound on the root, so it reaches here only while the sheet holds keys.
        if state.shortcut == Some(SHEET_DISMISS_SHORTCUT) || self.dismissed {
            return Some(Change::Dismissed);
        }
        self.settled.map(|id| Change::Selected(Some(id)))
    }

    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        self.body.clone()
    }
}
