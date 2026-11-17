//! The app loop: build -> layout -> paint -> present.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::text_pass::TextPass;
use ui_core::collections::{FastHasher, FastMap, FastSet};
use ui_core::draw::{DrawList, PrimKind, RectPrim, TextPrim};
use ui_core::geom::{Color, Insets, Rect, Size};
use ui_core::hit::{HitBehavior, HitIndex};
use ui_core::layout::{
    Anchor, AxisSize, Grow, Len, OverlayAlign, OverlayPlacement, Presentation, Side, SizeClass,
    Sizing,
};
use ui_core::timed;
use ui_core::{
    AnimSpec, BuiltinLayout, Constraints, Damage, DamageAccum, Decl, Easing, Id, LayoutEngine,
    MonospaceMeasure, NodeData, NodeKind, Tree,
};
use ui_paint::CpuRenderer;

use crate::present::PresentTarget;

/// Per-frame information handed to user code.
#[derive(Clone, Debug, Default)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
    /// Pointer position, if the pointer is over the surface.
    pub pointer: Option<ui_core::geom::Point>,
    /// Index of this frame, starting at 0.
    pub frame: u64,
    /// What the last frame reported as hovered, pressed and focused.
    pub hovered: Option<ui_core::Id>,
    pub pressed: Option<ui_core::Id>,
    /// The clicked node and its ancestors, root first; empty when nothing was clicked.
    clicked_path: Vec<Id>,
    /// How many presses the click was part of: one for a single, two for a double.
    pub clicks: u32,
    /// The drag-release chain, root first.
    dropped_path: Vec<Id>,
    /// The node that release landed on, when it accepts drops.
    drop_target: Option<Id>,
    /// The node a live drag is over, when it accepts what the drag offers, and its chain.
    drag_path: Vec<Id>,
    /// The MIME types the live drag offers, best first.
    drag_types: Option<Rc<Vec<String>>>,
    /// Whether the live press has travelled past the click threshold.
    pub drag_started: bool,
    /// Pointer movement since the previous event while a drag is live.
    pub drag_step: Option<ui_core::geom::Point>,
    /// Where the live press landed, in surface coordinates.
    pub press_origin: Option<ui_core::geom::Point>,
    pub focused: Option<ui_core::Id>,
    /// Focus arrived by keyboard, so a focus ring may be drawn.
    pub focus_visible: bool,
    /// The node Enter or Space reached since the last frame, and the chord that fired.
    activated: Option<Id>,
    /// That node's ancestor chain, root first; empty when nothing was activated.
    activated_path: Vec<Id>,
    /// The popup a press outside it asked to close since the last frame.
    pub dismissed: Option<Id>,
    shortcut: Option<(Id, ui_core::ShortcutId)>,
    /// The focused input, its selection and its buffer at the moment a chord fired. The edit
    /// is computed a frame later, and a later key in the same burst may have moved the caret
    /// or selected the very text the edit would then replace.
    shortcut_focus: Option<(Id, ui_core::TextSelection, String)>,
    /// The focused text input and the caret's byte offset inside it.
    pub caret: Option<(ui_core::Id, usize)>,
    /// The caret's x inside the text node drawing the buffer, in that node's layout space.
    pub caret_in_text: Option<(ui_core::Id, f32)>,
    /// Every text node a selection covers, each with the bytes selected in it.
    pub selection: Rc<[(ui_core::Id, ui_core::TextSelection)]>,
    /// The input method's composition, when one is live.
    pub preedit: Option<(ui_core::Id, ui_core::Preedit)>,
    /// Seconds since the previous frame, fed by the platform; 0 for a static frame.
    pub dt: f32,
    /// This frame's timestamp from the engine clock. Apps pass it to `AnimStore` and
    /// never read a clock themselves.
    pub now: f64,
    /// The hovered node and its ancestors, root first; empty when nothing is hovered.
    hovered_path: Vec<Id>,
    /// The pressed node and its ancestors, root first; empty when nothing is pressed.
    pressed_path: Vec<Id>,
    /// The node owning the current drag, resolved by the engine from the pressed node.
    drag_owner: Option<Id>,
}

impl FrameInfo {
    /// A frame with no derived state, for callers that drive `view` by hand.
    pub fn new(width: u32, height: u32, pointer: Option<ui_core::geom::Point>, frame: u64) -> Self {
        Self {
            width,
            height,
            pointer,
            frame,
            ..Self::default()
        }
    }

    /// Whether the pointer is over `id` or anywhere inside it.
    ///
    /// `hovered` alone is the *topmost* node, so a container is never reported while the
    /// pointer is over its children. Hit testing has to stay topmost (that is what a
    /// click means), so the container question is answered by the ancestor path.
    pub fn hovered_within(&self, id: Id) -> bool {
        self.hovered_path.contains(&id)
    }

    /// The hovered node and its ancestors, root first.
    pub fn hovered_path(&self) -> &[Id] {
        &self.hovered_path
    }

    /// The pressed path, for the engine to fold into the build context.
    fn pressed_path_field(&self) -> &[Id] {
        &self.pressed_path
    }

    /// Whether `id` is the pressed node or contains it. A control's thumb is hit first, so
    /// an exact match would hide the press from the control that owns it.
    pub fn pressed_within(&self, id: Id) -> bool {
        self.pressed_path.contains(&id)
    }

    /// Whether a drag that started inside `id` is in progress.
    pub fn dragging_within(&self, id: Id) -> bool {
        match self.drag_owner {
            Some(owner) => owner == id || self.pressed_path.contains(&id),
            None => false,
        }
    }
}

/// User code: describe the UI and react to derived state.
pub trait App {
    /// Produce this frame's live control tree; the engine turns it into declarations.
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::element::Element>;

    /// A control reported a semantic change. The engine already resolved which node and
    /// what the new value is, so the app never parses a gesture or hit-tests.
    fn changed(&mut self, _id: Id, _change: ui_core::change::Change) -> bool {
        false
    }

    /// An app-level shortcut fired: one bound to the loop, not to a control.
    ///
    /// `held` is the modifier set the press resolved with. A binding that leaves a modifier
    /// free reads it here, so the effect is decided by the press rather than by whatever is
    /// held at the next build.
    fn shortcut(&mut self, _id: ui_core::ShortcutId, _held: ui_core::Modifiers) -> bool {
        false
    }

    /// The look of the engine's own menu, resolved by the app during `view`. The engine
    /// asks after every `view`, so chrome and content always share one transition.
    fn menu_style(&self) -> Option<crate::menu::MenuStyle> {
        None
    }
}

/// A boxed app is an app: the loop stores one without knowing which surface type it drives.
impl App for Box<dyn App> {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::element::Element> {
        (**self).view(cx)
    }

    fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
        (**self).changed(id, change)
    }

    fn shortcut(&mut self, id: ui_core::ShortcutId, held: ui_core::Modifiers) -> bool {
        (**self).shortcut(id, held)
    }

    fn menu_style(&self) -> Option<crate::menu::MenuStyle> {
        (**self).menu_style()
    }
}

/// Result of one frame, useful for tests and diagnostics.
#[derive(Debug, Default)]
pub struct FrameOutcome {
    pub damage: Damage,
    pub nodes: usize,
    pub hit: Option<Id>,
    /// Something is still moving: present even without damage so the commit's callback
    /// drives the next frame. Blocking on input here stalls the animation.
    pub animating: bool,
    /// Resolved pointer shape for this frame; definite, so a cleared shape cannot stick.
    pub cursor: ui_core::CursorShape,
}

/// What a scene frame produced: the frame, and what the target did with it.
pub struct SceneFrameOutcome {
    pub outcome: FrameOutcome,
    /// `None` when the frame had nothing to show.
    pub presented: Option<crate::present::PresentStats>,
}

/// Layout measurement sharing the painter's pass, so both see identical state.
struct PassMeasure(Rc<RefCell<TextPass>>);

/// The build's measurer over an existing pass; never a second pass.
fn shared_measure(text: &Rc<RefCell<TextPass>>) -> Rc<RefCell<dyn ui_core::TextMeasure>> {
    Rc::new(RefCell::new(PassMeasure(Rc::clone(text))))
}

/// The pass's formula layout, behind the engine's trait.
fn shared_math(text: &Rc<RefCell<TextPass>>) -> Rc<RefCell<dyn ui_core::MathLayout>> {
    Rc::new(RefCell::new(PassMath(Rc::clone(text))))
}

fn shared_edit(text: &Rc<RefCell<TextPass>>) -> Rc<RefCell<dyn ui_core::MathEdit>> {
    Rc::new(RefCell::new(PassEdit(Rc::clone(text))))
}

struct PassMath(Rc<RefCell<TextPass>>);

impl ui_core::MathLayout for PassMath {
    fn default_family(&mut self) -> Option<String> {
        self.0.try_borrow_mut().ok()?.default_math_family()
    }

    fn layout(
        &mut self,
        family: &str,
        source: &str,
        size: f32,
        display: bool,
        style: Option<ui_core::math::MathErrorStyle>,
    ) -> Result<ui_core::math::Formula, ui_core::math::MathError> {
        let Ok(mut text) = self.0.try_borrow_mut() else {
            return Err(ui_core::math::MathError::NoFace);
        };
        text.layout_math(family, source, size, display, style)
    }
}

struct PassEdit(Rc<RefCell<TextPass>>);

impl PassEdit {
    fn with<R>(&self, default: R, f: impl FnOnce(&mut TextPass) -> R) -> R {
        match self.0.try_borrow_mut() {
            Ok(mut text) => f(&mut text),
            Err(_) => default,
        }
    }
}

impl ui_core::MathEdit for PassEdit {
    fn attach(&mut self, request: ui_core::MathAttach<'_>) -> bool {
        self.with(false, |text| text.math_edit_attach(request))
    }

    fn is_editing(&mut self, id: Id) -> bool {
        self.with(false, |text| text.math_edit_is_editing(id))
    }

    fn owner(&mut self, id: Id) -> Option<(Id, usize, usize)> {
        self.with(None, |text| text.math_edit_owner(id))
    }

    fn formula_at(&mut self, owner: Id, offset: usize) -> Option<Id> {
        self.with(None, |text| text.math_edit_at(owner, offset))
    }

    fn key(&mut self, id: Id, key: ui_core::EditKey, shift: bool) -> bool {
        self.with(false, |text| text.math_edit_key(id, key, shift))
    }

    fn insert(&mut self, id: Id, latex: &str) -> bool {
        self.with(false, |text| text.math_edit_insert(id, latex))
    }

    fn click(&mut self, id: Id, x: f32, y: f32, extend: bool) -> bool {
        self.with(false, |text| text.math_edit_click(id, x, y, extend))
    }

    fn formula(
        &mut self,
        id: Id,
        style: Option<ui_core::math::MathErrorStyle>,
    ) -> Option<ui_core::Formula> {
        self.with(None, |text| text.math_edit_formula(id, style))
    }

    fn source(&mut self, id: Id) -> Option<String> {
        self.with(None, |text| text.math_edit_source(id))
    }

    fn caret(&mut self, id: Id) -> Option<ui_core::MathCaret> {
        self.with(None, |text| text.math_edit_caret(id))
    }

    fn selection(&mut self, id: Id) -> Vec<ui_core::MathEditRect> {
        self.with(Vec::new(), |text| text.math_edit_selection(id))
    }

    fn placeholders(&mut self, id: Id) -> Vec<ui_core::MathEditRect> {
        self.with(Vec::new(), |text| text.math_edit_placeholders(id))
    }

    fn selection_offsets(&mut self, id: Id) -> Option<(usize, usize)> {
        self.with(None, |text| text.math_edit_selection_offsets(id))
    }

    fn select_offsets(&mut self, id: Id, anchor: usize, caret: usize) -> bool {
        self.with(false, |text| {
            text.math_edit_select_offsets(id, anchor, caret)
        })
    }

    fn offset_at(&mut self, id: Id, x: f32, y: f32) -> Option<usize> {
        self.with(None, |text| text.math_edit_offset_at(id, x, y))
    }

    fn set_preedit(&mut self, id: Id, text: &str) -> bool {
        self.with(false, |pass| pass.math_edit_set_preedit(id, text))
    }

    fn commit_preedit(&mut self, id: Id, text: &str) -> bool {
        self.with(false, |pass| pass.math_edit_commit_preedit(id, text))
    }

    fn cancel_preedit(&mut self, id: Id) -> bool {
        self.with(false, |pass| pass.math_edit_cancel_preedit(id))
    }

    fn undo(&mut self, id: Id) -> bool {
        self.with(false, |text| text.math_edit_undo(id))
    }

    fn redo(&mut self, id: Id) -> bool {
        self.with(false, |text| text.math_edit_redo(id))
    }

    fn select_all(&mut self, id: Id) -> bool {
        self.with(false, |text| text.math_edit_select_all(id))
    }

    fn selection_latex(&mut self, id: Id) -> Option<String> {
        self.with(None, |text| text.math_edit_selection_latex(id))
    }

    fn cut(&mut self, id: Id) -> Option<String> {
        self.with(None, |text| text.math_edit_cut(id))
    }
}

impl ui_core::TextMeasure for PassMeasure {
    fn layout(
        &mut self,
        block: &ui_core::TextBlock,
        max_width: f32,
    ) -> std::sync::Arc<ui_core::InlineLayout> {
        self.0.borrow_mut().layout(block, max_width)
    }

    fn intrinsic(&mut self, block: &ui_core::TextBlock) -> (ui_core::Size, ui_core::Size) {
        self.0.borrow_mut().intrinsic(block)
    }
}

/// The row ids the engine supplies for text editing; an app may reuse them in its own menu.
pub const MENU_SELECT_ALL: &str = "menu/select-all";
pub const MENU_COPY: &str = "menu/copy";
pub const MENU_CUT: &str = "menu/cut";
pub const MENU_PASTE: &str = "menu/paste";

/// The engine key a math editor understands, or `None` when the key belongs to the
/// engine (focus traversal, activation).
fn math_edit_key(key: ui_core::Key) -> Option<ui_core::EditKey> {
    Some(match key {
        ui_core::Key::Backspace => ui_core::EditKey::Backspace,
        ui_core::Key::Delete => ui_core::EditKey::Delete,
        ui_core::Key::Left => ui_core::EditKey::Left,
        ui_core::Key::Right => ui_core::EditKey::Right,
        ui_core::Key::Up => ui_core::EditKey::Up,
        ui_core::Key::Down => ui_core::EditKey::Down,
        ui_core::Key::Home => ui_core::EditKey::Home,
        ui_core::Key::End => ui_core::EditKey::End,
        ui_core::Key::Escape => ui_core::EditKey::Escape,
        ui_core::Key::Space => ui_core::EditKey::Character(' '),
        _ => return None,
    })
}

/// The shortcut this node binds for a chord, if any.
fn bound_on(
    node: &ui_core::tree::Node,
    key: ui_core::Key,
    mods: ui_core::Modifiers,
) -> Option<ui_core::ShortcutId> {
    ui_core::shortcut::best_match(&node.data.params()?.shortcuts, key, mods)
}

/// Where a byte offset lands after `start..end` was replaced by `inserted` bytes.
fn rebase_offset(at: usize, start: usize, end: usize, inserted: usize) -> usize {
    if at < start {
        at
    } else if at >= end {
        at - (end - start) + inserted
    } else {
        start
    }
}

/// The start and end byte offsets of the line that holds `at`.
fn line_bounds(value: &str, at: usize) -> (usize, usize) {
    let at = ui_core::text_input::clamp(value, at);
    let start = value[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = value[at..].find('\n').map_or(value.len(), |i| at + i);
    (start, end)
}

/// Where a text node's lines start: its box origin moved in by its own padding.
///
/// Hit testing, painting and the caret all read this one answer, or a padded label would
/// wrap at one width and be clicked at another.
/// The height a text block's own lines are laid out in: the content box, since
/// [`text_origin`] already stands at the padding's inner edge. Centring against the border
/// box instead would add the padding a second time and push the last line out of the node.
fn text_height(node: &ui_core::Node) -> f32 {
    let rect = node.visual_rect();
    let pad = node.box_model.padding;
    (rect.size.h - pad.top - pad.bottom).max(0.0)
}

fn text_origin(node: &ui_core::Node) -> ui_core::geom::Point {
    let rect = node.visual_rect();
    ui_core::geom::Point::new(
        rect.origin.x + node.box_model.padding.left,
        rect.origin.y + node.box_model.padding.top,
    )
}

/// Walk the live element tree beside the declaration tree, asking each node for a change.
fn collect_changes(
    element: &dyn crate::Element,
    decl: &ui_core::Decl,
    tree: &ui_core::Tree,
    cx: &ui_core::cx::Cx<'_>,
    out: &mut Vec<(Id, ui_core::change::Change)>,
) {
    let rect = tree
        .find_by_id(decl.id)
        .and_then(|nid| tree.get(nid))
        .map(|n| n.visual_rect())
        .unwrap_or(Rect::ZERO);
    // `state.local` is the press point in *layout* space, so a control inside a scroller
    // sees content coordinates (the scroller's offset is visual, not layout). Using the
    // visual rect would fold the offset back in and shift every hit inside a scrolled
    // control; hit testing already follows the visual rect, so the press still lands on
    // the right node — this is only about the coordinates handed to `change`.
    let mut state = crate::state::WidgetState::of(cx, decl.id);
    let layout_rect = tree
        .find_by_id(decl.id)
        .and_then(|nid| tree.get(nid))
        .map(|n| n.rect)
        .unwrap_or(Rect::ZERO);
    // The press point must be in *this* node's space, not the deepest hit node's.
    state.local = cx
        .pointer()
        .map(|p| ui_core::geom::Point::new(p.x - layout_rect.origin.x, p.y - layout_rect.origin.y));
    if let Some(change) = element.change(&state, rect) {
        out.push((decl.id, change));
    }
    let kids = element.children();
    if kids.is_empty() {
        return;
    }
    for (child_element, child_decl) in kids.iter().zip(decl.children.iter()) {
        collect_changes(child_element.as_ref(), child_decl, tree, cx, out);
    }
}

/// Owns the engine state and drives frames.
/// The engine's single time source. `now` is monotonic seconds from the loop's epoch.
pub struct FrameClock {
    epoch: std::time::Instant,
    last: std::time::Instant,
    now: f64,
    /// Injectable for tests: returns the next `now` instead of a wall clock.
    script: Option<Vec<f64>>,
    cursor: usize,
}

impl Default for FrameClock {
    fn default() -> Self {
        let t = std::time::Instant::now();
        Self {
            epoch: t,
            last: t,
            now: 0.0,
            script: None,
            cursor: 0,
        }
    }
}

impl FrameClock {
    /// A clock that hands out `times` in order, so a test controls every frame.
    pub fn scripted(times: Vec<f64>) -> Self {
        let t = std::time::Instant::now();
        Self {
            epoch: t,
            last: t,
            now: *times.first().unwrap_or(&0.0),
            script: Some(times),
            cursor: 0,
        }
    }

    /// Advance to this frame's timestamp. Called exactly once per frame.
    pub fn tick(&mut self) -> f64 {
        if let Some(times) = &self.script {
            let t = times.get(self.cursor).copied().unwrap_or(self.now);
            self.cursor += 1;
            self.now = t;
            return t;
        }
        let t = std::time::Instant::now();
        self.now = (t - self.epoch).as_secs_f64();
        self.last = t;
        self.now
    }

    /// This frame's timestamp, without advancing.
    pub fn now(&self) -> f64 {
        self.now
    }

    /// The wall clock right now, without advancing the frame's stamp.
    ///
    /// Input arrives between frames, and those gaps are as long as the user was idle. A
    /// transition started in one must start now, not at the last frame: after a pause of a
    /// second, stamping the old frame would land the whole entrance before it is drawn.
    pub fn live(&self) -> f64 {
        if self.script.is_some() {
            // A scripted clock only moves when a frame is built; there is no wall to read.
            return self.now;
        }
        (std::time::Instant::now() - self.epoch).as_secs_f64()
    }

    /// Push the timestamp forward by `dt` seconds, for scripted test frames.
    pub fn advance_by(&mut self, dt: f64) {
        self.now += dt;
        let wall = std::time::Instant::now();
        // `tick` derives `now` from `Instant::now() - epoch`, so the epoch has to move with a
        // scripted jump; otherwise the next real frame rewinds time and every running
        // animation freezes until the wall clock catches up.
        self.epoch = wall
            .checked_sub(std::time::Duration::from_secs_f64(self.now.max(0.0)))
            .unwrap_or(wall);
        self.last = wall;
    }
}

/// One press splat: where it was splatted, how big it may grow, and its life so far.
struct Splat {
    /// The node that asked for the ink; the splat paints on that node only.
    node: Id,
    /// The press point in surface space; the node may move or scroll under it.
    origin: ui_core::geom::Point,
    ink: ui_core::draw::PressInk,
    started: f64,
    /// When the press ended; the fade starts here and the origin stops mattering.
    released: Option<f64>,
}

impl Splat {
    /// Whether the fade after release has finished; such a splat can be dropped.
    fn finished(&self, now: f64) -> bool {
        self.released.is_some_and(|at| {
            now >= at + self.ink.fade.delay.max(0.0) as f64 + self.ink.fade.duration.max(0.0) as f64
        })
    }

    /// This frame's circle in surface space, or `None` when it would be invisible.
    ///
    /// `clip` is the effective-background rect in node space, resolved this frame from the
    /// descendant the control named; the ripple sizes to it and is clipped to it, as
    /// Flutter's `_IndicatorInkWell`. `None` uses the node's own box.
    fn prim(
        &self,
        now: f64,
        node: Rect,
        clip: Option<Rect>,
    ) -> Option<(Rect, ui_core::geom::Color)> {
        let grow = self.ink.grow.eased_at(self.started, now);
        // The fade-in keeps running after release; freezing it there would freeze the
        // opacity a fast click had reached — which is zero, so the ink would never show.
        let alpha = self.ink.fade_in.eased_at(self.started, now)
            * match self.released {
                None => 1.0,
                Some(at) => 1.0 - self.ink.fade.eased_at(at, now),
            };
        if alpha <= 0.0 {
            return None;
        }
        // Measured in node space, so the splat travels with the node when it moves.
        let x = self.origin.x - node.origin.x;
        let y = self.origin.y - node.origin.y;
        let (x, y, target) = match self.ink.radius {
            ui_core::InkRadius::Fixed(r) => (x, y, r),
            // Flutter's `InkRipple`: the splash drifts to the reference box centre and ends
            // at half the diagonal, which covers that box from its middle.
            ui_core::InkRadius::Cover => {
                // `clip` is in node space; the node's own box must be too, or the drift
                // target is off by the node's surface origin.
                let box_rect = clip.unwrap_or_else(|| {
                    Rect::from_origin_size(ui_core::geom::Point::ZERO, node.size)
                });
                let cx = box_rect.origin.x + box_rect.size.w * 0.5;
                let cy = box_rect.origin.y + box_rect.size.h * 0.5;
                let half = (box_rect.size.w * box_rect.size.w + box_rect.size.h * box_rect.size.h)
                    .sqrt()
                    * 0.5;
                (x + (cx - x) * grow, y + (cy - y) * grow, half)
            }
        };
        // Flutter starts the radius at 30% of its target, so a tap shows a dot at once.
        let radius = target * (0.3 + 0.7 * grow);
        (radius > 0.0).then(|| {
            let colour = ui_core::geom::Color {
                a: self.ink.colour.a * alpha,
                ..self.ink.colour
            };
            // The circle stays whole: the effective-background clip below is a rounded clip,
            // so cutting the rect here would square off the splash inside the pill.
            (
                Rect::new(
                    node.origin.x + x - radius,
                    node.origin.y + y - radius,
                    radius * 2.0,
                    radius * 2.0,
                ),
                colour,
            )
        })
    }
}

/// How long an automatic scrollbar stays at full strength after the last activity, and the
/// two fade durations. Long enough to read the bar, short enough that it leaves before it
/// becomes clutter.
const BAR_HOLD: f64 = 0.6;
const BAR_FADE_IN: AnimSpec = AnimSpec::new(0.12, Easing::EaseOut);
const BAR_FADE_OUT: AnimSpec = AnimSpec::new(0.3, Easing::EaseOut);

/// How long a secondary press may last and still count as a tap.
///
/// A tap opens the menu the node would have opened on the press; a longer hold suppresses
/// it, which is what lets one button aim a tool and offer a menu. The threshold is the
/// legacy overlay's `RIGHT_TAP_MS`.
/// The key a one-character text event names, for a surface with nothing to type into.
///
/// A plain letter reaches the surface as text, never as a key event, so a binding of `A`
/// only ever sees it here. A capital is stored unshifted, as every chord is.
fn typed_key(text: &str) -> Option<ui_core::Key> {
    let mut chars = text.chars();
    let character = chars.next()?;
    chars
        .next()
        .is_none()
        .then(|| ui_core::Key::Char(character.to_ascii_lowercase()))
}

const SECONDARY_TAP: f64 = 0.3;

/// One automatic scrollbar's fade state: the engine owns it because the bar is declared with
/// no state of its own and a widget cannot tween what it stops declaring.
struct BarFade {
    /// The wheel's activity keeps the bar up until this stamp.
    hold_until: f64,
    /// The pointer was over the scroller on the frame `advance` last ran.
    hovered: bool,
    /// The fade in progress: the value it left, where it goes, and when it started.
    from: f32,
    target: f32,
    since: f64,
    opacity: f32,
}

impl BarFade {
    fn new() -> Self {
        Self {
            hold_until: f64::NEG_INFINITY,
            hovered: false,
            from: 0.0,
            target: 0.0,
            since: 0.0,
            opacity: 0.0,
        }
    }

    /// Fold this frame's activity in and move the fade one step.
    fn advance(&mut self, now: f64, hovered: bool, wheel: bool) {
        if wheel {
            self.hold_until = now + BAR_HOLD;
        }
        self.hovered = hovered;
        let target = if hovered || now < self.hold_until {
            1.0
        } else {
            0.0
        };
        if target != self.target {
            self.from = self.opacity;
            self.target = target;
            self.since = now;
        }
        let spec = if self.target > 0.5 {
            BAR_FADE_IN
        } else {
            BAR_FADE_OUT
        };
        let eased = spec.eased_at(self.since, now);
        self.opacity = ui_core::geom::lerp(self.from, self.target, eased);
    }

    /// Whether the fade still needs frames: a fade in flight, or a hold that will expire.
    ///
    /// A bar resting under the pointer is not awake — nothing would change, and an idle
    /// window must stay idle.
    fn awake(&self, now: f64) -> bool {
        if self.opacity != self.target {
            return true;
        }
        !self.hovered && self.opacity > 0.0 && now < self.hold_until
    }
}

/// A secondary press that belongs to a node rather than to the menu.
///
/// It is kept until the release, which is where the tap/hold split is decided: the press
/// point is what a tap's menu opens at, and the time is what makes it a tap.
struct SecondaryPress {
    /// The node that asked to hear the button; `None` when only a drag could start, so the
    /// press is recorded for the tap rule alone and nobody is told about it.
    node: Option<Id>,
    at: ui_core::geom::Point,
    since: f64,
    /// The burst length of the press: two is a double-click on the secondary button.
    clicks: u32,
    /// Where the app hears the tap when no node declared a menu: the node it landed on.
    /// Held back until the release proves the button did not travel into a right drag.
    request: Option<Id>,
}

/// One button's last press, kept only as long as the burst rule can join the next one.
#[derive(Clone, Copy)]
struct MultiPress {
    at: f64,
    pos: ui_core::geom::Point,
    clicks: u32,
}

/// One live scrollbar drag: the scroller, the press position, and the offset it began at.
struct ScrollbarDrag {
    id: Id,
    axis: ui_core::ScrollAxis,
    press: f32,
    start_offset: f32,
    /// Pixels of track the thumb may travel; the offset's range maps onto it.
    travel: f32,
    /// The offset's range, `extent - viewport`.
    overflow: f32,
}

/// One scroller in a pan's chain. The innermost is the one under the press; the rest are
/// its ancestors.
#[derive(Clone, Copy)]
struct PanLink {
    id: Id,
    /// The axes this scroller owns; a drag on any other axis passes through it.
    axes: (bool, bool),
    /// `extent - viewport` per axis.
    overflow: ui_core::geom::Point,
    viewport: ui_core::geom::Size,
    /// The offset the finger asks for, before any bound. Clamping this rather than the
    /// visible offset keeps a stretch from accumulating error.
    raw: ui_core::cx::ScrollOffset,
    /// Offset at the previous frame, for measuring the velocity a fling starts with.
    prev: ui_core::cx::ScrollOffset,
    /// Velocity in logical px/s, opposite the pointer.
    v: ui_core::geom::Point,
}

/// The scrollers a pointer holds: content follows the finger, and each link takes what it
/// can on its own axes and passes the rest up. Past an end the link that owns the axis
/// stretches by a rubber band instead of stopping dead.
struct ScrollPan {
    links: Vec<PanLink>,
    /// The link that may stretch on each axis: the outermost one that scrolls it.
    elastic: (Option<usize>, Option<usize>),
    /// Where the pointer was at the previous move.
    last: ui_core::geom::Point,
}

/// A scroller moving after the pointer let go: momentum, then the spring that pulls an
/// overscroll back to the edge.
#[derive(Clone, Copy)]
struct ScrollMotion {
    /// The hand's speed at release, in logical px/s opposite the pointer.
    v0: ui_core::geom::Point,
    /// Where the coast began; the closed form is evaluated from here, not from the last
    /// frame, so a long frame lands where the same instant would have put it.
    origin: ui_core::cx::ScrollOffset,
    /// When the coast began.
    started_at: f64,
    /// The speed this instant, for the crossing test and the hand-over to the spring.
    v: ui_core::geom::Point,
    /// The spring owns the offset from here on.
    spring: bool,
    /// The travel range, captured when the motion began so a relayout cannot retarget it.
    overflow: ui_core::geom::Point,
    /// The viewport the overscroll is damped against, captured with the range.
    viewport: ui_core::geom::Size,
    /// The edge the spring settles on, fixed when the bounce starts.
    target: ui_core::cx::ScrollOffset,
}

/// A wheel burst on a snapping scroller, waiting for the burst to go quiet.
struct SnapBurst {
    /// The offset the burst began at, per axis: which page it lands on depends on the way
    /// the burst travelled, not only on where it stopped.
    origin: ui_core::cx::ScrollOffset,
    /// When the last wheel of the burst arrived, on the frame clock.
    at: f64,
}

/// How long a wheel burst must be quiet before a snapping scroller rests on its page.
const SNAP_IDLE: f64 = 0.12;

/// Put a scroll offset on the grid text is rasterized on, in logical units.
///
/// The stored offset stays exact, so a slow trackpad gesture still accumulates instead of
/// rounding itself away; what is painted lands on whole raster pixels. Text outside a scroll
/// keeps its subpixel phase, but scrolling content must not: a glyph that changes vertical
/// phase on every step of a glide reads as the text jittering against rows that move
/// smoothly. Every desktop scrolls by whole device pixels for this reason.
fn on_text_grid(offset: f32, grid: f32) -> f32 {
    if grid.is_finite() && grid > 0.0 {
        (offset * grid).round() / grid
    } else {
        offset
    }
}

/// The page a gesture from `origin` to `offset` rests on, along one axis.
///
/// `turn` is the wheel's rule: a burst is a request to change pages, so any travel turns at
/// least one, and a notch shorter than half a page still moves the strip. Without it the
/// nearest page wins, which is what a released finger and a fling's landing want.
fn snapped_axis(offset: f32, origin: f32, step: f32, overflow: f32, turn: bool) -> f32 {
    if step <= 0.0 {
        return offset;
    }
    let from = (origin / step).floor();
    let here = (offset / step).floor();
    let page = if !turn {
        (offset / step).round()
    } else if offset > origin {
        here.max(from + 1.0)
    } else if offset < origin {
        here.min(from - 1.0)
    } else {
        (offset / step).round()
    };
    (page * step).clamp(0.0, overflow.max(0.0))
}

/// Velocity decay per second: GPUI's `0.998` per millisecond.
const SCROLL_FRICTION: f32 = 2.0;
/// Below this a fling is over, in logical px/s.
const SCROLL_STOP: f32 = 30.0;
/// How far a stretch travels per unit of drag at the end of the viewport.
const RUBBER: f32 = 0.55;
/// Spring returning an overscroll: under-damped enough to feel like a bounce.
const BOUNCE_STIFFNESS: f32 = 280.0;
const BOUNCE_DAMPING: f32 = 30.0;
/// A flick reaches the edge faster than the band can absorb; the entry speed is scaled so the
/// overshoot is a nudge, not a jump.
const BOUNCE_ENTRY: f32 = 0.35;

/// The share of the frame a kernel layer must cover before its output is cached across
/// frames; below it the copy costs more than the blur it saves.
const EFFECT_CACHE_MIN_SHARE: f32 = 0.25;

/// The insertion caret rests visible this long, then hidden for as long again.
const CARET_BLINK: f64 = 0.5;

/// Lines of context kept above and below the caret's line when a step reveals it. One line
/// keeps the caret off the clip edge without spending much of a small viewport on margin.
const CARET_REVEAL_MARGIN_LINES: f32 = 1.0;

/// The caret and selection a frame holds: a change in either restarts the blink.
#[derive(Clone, Copy, PartialEq)]
struct CaretFact {
    /// The editing input and its caret's byte offset.
    editing: Option<(Id, usize)>,
    /// The read-only selection: the block the caret sits in, the block it started in, and
    /// the two byte offsets. A cross-block drag changes the block, not just the byte.
    selection: Option<(Id, Id, usize, usize)>,
}

/// The two bytes a read-only selection runs between, each with the block it sits in.
///
/// A drag crosses block boundaries in both directions, so an end keeps its own block and the
/// covered blocks are derived from the pair rather than from one node's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectionEnds {
    anchor: (Id, usize),
    head: (Id, usize),
}

/// `offset` with its overshoot past either end damped, as iOS damps a stretched scroll view.
fn stretch(offset: f32, overflow: f32, viewport: f32) -> f32 {
    if offset < 0.0 {
        -rubber(-offset, viewport)
    } else if offset > overflow {
        overflow + rubber(offset - overflow, viewport)
    } else {
        offset
    }
}

/// How far a stretch of `x` actually travels in a viewport of `dim`; the first pixels are
/// nearly 1:1 and the travel asymptotes, so the content never runs away.
fn rubber(x: f32, dim: f32) -> f32 {
    let dim = dim.max(1.0);
    (1.0 - 1.0 / (x * RUBBER / dim + 1.0)) * dim
}

/// One buffer state an undo or redo step returns to.
#[derive(Clone, Debug)]
struct InputSnapshot {
    value: String,
    selection: ui_core::TextSelection,
}

/// The undo and redo stacks of one input owner.
///
/// Whole buffers rather than inverse edits: a merged typing run has no single inverse, and
/// an inverse would still need this log to describe it.
#[derive(Default)]
struct InputHistory {
    undo: Vec<InputSnapshot>,
    redo: Vec<InputSnapshot>,
    /// Whether the newest undo entry is a typing run further keystrokes extend.
    typing: bool,
    /// When the run last took a keystroke; a pause longer than `TYPING_PAUSE` closes it.
    typed_at: f64,
}

/// How long a typing run may pause before the next keystroke starts a new undo group.
const TYPING_PAUSE: f64 = 1.0;

/// Groups kept per input owner; the oldest is dropped past this.
const HISTORY_LIMIT: usize = 200;

/// One control shortcut that fired, plus the input state it was pressed against.
///
/// A burst can fire several before a frame answers, and each later edit must see the
/// buffer and caret the earlier ones leave behind.
struct ShortcutFact {
    owner: Id,
    shortcut: ui_core::ShortcutId,
    /// The focused input, its selection and its buffer when the chord fired; absent
    /// when no input held focus.
    focus: Option<(Id, ui_core::TextSelection, String)>,
}

/// A drag crossing one surface.
///
/// One shape for both directions: a drag this app started (its `data` is known from the
/// press) and one the system is carrying, whose payload it reveals on release.
struct DragSession {
    /// The node the press started on, when this app's own gesture began the drag.
    source: Option<Id>,
    /// What the drag carries, when this app knows it before the release.
    data: Option<Rc<ui_core::DragData>>,
    /// What it offers, best first; empty for a gesture that carries nothing.
    types: Rc<Vec<String>>,
    /// The effects the payload permits or the source announced.
    effects: ui_core::DropEffects,
    /// What the payload prefers when no modifier is held.
    preferred: ui_core::DropEffect,
    /// The button that started it: `Secondary` means a right drag, which asks.
    button: ui_core::PointerButton,
    /// The node under the pointer that takes it.
    target: Option<Id>,
}

impl DragSession {
    /// Whether the offered types decide what takes the drag.
    ///
    /// A gesture-only drag started here has nothing to match, so any `DROP` node takes it,
    /// exactly as it did before payloads existed.
    fn types_decide(&self) -> bool {
        self.data.is_some() || self.source.is_none()
    }

    /// The effect a release with `modifiers` held means, or `None` when nothing is allowed.
    fn negotiated(&self, modifiers: ui_core::Modifiers) -> Option<ui_core::DropEffect> {
        ui_core::negotiate_effect(self.effects, self.preferred, modifiers)
    }
}

/// A drag this app owns, ready for the host to run as an operating-system drag.
#[derive(Clone, Debug)]
pub struct NativeDrag {
    /// What the drop carries.
    pub data: ui_core::DragData,
    /// The dragged node's box when the gesture started, in surface units.
    pub bounds: Option<ui_core::geom::Rect>,
    /// The press point, in surface units: the image hangs from here under the pointer.
    pub origin: ui_core::geom::Point,
    /// The element's own pixels, rendered from its subtree alone.
    ///
    /// A host paints this under the pointer instead of photographing the surface, so the
    /// image holds the dragged element and nothing that happened to be around it.
    pub image: Option<DragImage>,
}

/// One element's painting, ready to be shown under the pointer.
#[derive(Clone)]
pub struct DragImage {
    /// Image size in physical pixels.
    pub size: (u32, u32),
    /// Straight (non-premultiplied) sRGB in BGRA byte order, top-down.
    pub bgra: Vec<u8>,
    /// The grab point inside the image, in image pixels.
    pub hotspot: (i32, i32),
}

impl std::fmt::Debug for DragImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The pixels are megabytes; a report only ever wants the shape.
        f.debug_struct("DragImage")
            .field("size", &self.size)
            .field("hotspot", &self.hotspot)
            .field("bytes", &self.bgra.len())
            .finish()
    }
}

/// A picture the app handed the engine, and the build that last asked for it.
struct RegisteredImage {
    entry: ui_core::draw::ImageEntry,
    /// The build that registered it, or `None` for a picture [`AppLoop::register_image`]
    /// keeps for the run.
    seen: Option<u64>,
}

pub struct AppLoop<A: App> {
    app: A,
    tree: Tree,
    engine: BuiltinLayout<Box<dyn ui_core::TextMeasure>>,
    renderer: CpuRenderer,
    text: Rc<RefCell<TextPass>>,
    /// The same pass, shaped for the build: a widget measuring a label must see the faces
    /// paint will use, so it shares `text` rather than opening a second pass.
    measure: Rc<RefCell<dyn ui_core::TextMeasure>>,
    /// The host's formula layout, over the same pass that paints the formula.
    math: Rc<RefCell<dyn ui_core::MathLayout>>,
    /// The host's structural formula editing, over the same pass and faces.
    math_edit: Rc<RefCell<dyn ui_core::MathEdit>>,
    /// Painted list before text resolution; kept for inspection.
    list: DrawList,
    /// Backend-neutral list: text already turned into positioned glyphs.
    resolved: DrawList,
    scale: f32,
    /// Renderer supersample factor; text must be rasterized on the same finer grid.
    supersample: u32,
    /// Atlas generation already copied into the renderer.
    atlas_version: u64,
    /// Colour-page generation already copied into the renderer.
    color_atlas_version: u64,
    /// Target buffer size of the last presented frame.
    last_size: (u32, u32),
    /// The only clock in the engine. Ticked once per frame; everything else reads `now`.
    clock: FrameClock,
    /// This frame's timestamp, ticked once at the top of the frame.
    frame_now: f64,
    /// The step the clock took into this frame, in seconds; scroll physics integrates on it.
    frame_step: f32,
    /// Whether the last build read the raw pointer position, anywhere in the tree.
    pointer_read: bool,
    /// Whether the last build read the held modifiers, so a change of them repaints.
    modifier_read: bool,
    /// Whether the last build read the clock, so the loop must keep it alive.
    time_driven: bool,
    /// An event arrived that the tree has to be rebuilt for; the loop takes it each build.
    dispatched: bool,
    /// A modal dialog of this surface is open: it paints, but takes no input.
    ///
    /// The host sets this from the surfaces it owns; the platform's modal hint is only a
    /// hint, so this is what actually holds the parent still.
    input_blocked: bool,
    /// Pointer steps the last build asked for, relative and in logical points.
    pointer_steps: Vec<(f32, f32)>,
    /// The engine owns the animation store: it advances it, damages it, and hands it to
    /// the build. An app that held it itself would need a second, conflicting borrow.
    anim_store: ui_core::AnimStore,
    damage: DamageAccum,
    frame: u64,
    /// Engine-owned scroll offsets per `SCROLL_OWNED` container; re-applied after every
    /// apply so the app's declaration never resets the wheel position.
    scroll_offsets: FastMap<Id, ui_core::cx::ScrollOffset>,
    /// Containers a wheel moved since the previous frame; the build reads it once to drive
    /// the auto scrollbar, then it is cleared.
    scrolled_since: FastSet<Id>,
    /// Fade state per automatic scrollbar, keyed by its scroller.
    scrollbar_fade: FastMap<Id, BarFade>,
    /// A bar was mid-fade or mid-hold when the build ran; the loop keeps presenting until
    /// the fade lands, the way it does for ink and springs.
    scrollbar_awake: bool,
    /// Pictures the app registered, keyed by stable id. A build's own pictures live for that
    /// frame; [`AppLoop::register_image`] keeps one for the run. Without the split, a picker
    /// that shows a new thumbnail every row grows this table until the app exits.
    images: FastMap<u64, RegisteredImage>,
    /// Canvases this frame declared, in declaration order.
    canvases: Vec<ui_core::draw::Canvas>,
    /// Canvas versions already painted by the target, keyed by id: `(version, size)`.
    canvas_state: FastMap<u64, (u64, (u32, u32))>,
    /// A canvas's pixels on the CPU path, keyed by id; the target owns them on the GPU.
    canvas_pixels: FastMap<u64, std::sync::Arc<Vec<u8>>>,
    /// This frame's canvas versions, for the image fingerprint that decides damage.
    canvas_versions: FastMap<u64, u64>,
    /// Press splats the engine owns; they outlive the press that made them.
    splats: Vec<Splat>,
    /// Per-frame texture index, so one texture is uploaded once per frame. Keyed by the
    /// picture and its sampling: the same file drawn smooth and pixelated is two entries.
    image_slots: FastMap<(u64, ui_core::draw::Sampling), u32>,
    /// Last frame's ink rect and content hash per node id, for damage diffing.
    node_prints: FastMap<Id, (Rect, u64)>,
    /// The content box and viewport each scroller had when a build last read them; a list
    /// windows its rows from them, so a frame that resizes one owes the next.
    scroller_boxes: FastMap<Id, (Size, Size)>,
    /// Panels the last build painted for a surface of the platform's own: each subtree's
    /// list, rebased on the panel's own top-left, ready for that surface to take.
    panel_views: Vec<(ui_core::NodeId, ui_core::draw::DrawList)>,
    /// Which presentation the host gave each platform panel: `true` for a surface of its
    /// own, `false` for the parent's. Absent means the host has not decided yet, and the
    /// panel is held back rather than painted somewhere it would have to move from.
    panel_presentation: FastMap<ui_core::NodeId, bool>,
    /// The backdrop each node painted last frame. A frost writes no primitive of its own,
    /// so it is absent from `node_prints` and must be diffed on its own.
    backdrops: FastMap<Id, ui_core::draw::BackdropEntry>,
    /// This frame's backdrops, collected while painting.
    backdrops_now: FastMap<Id, ui_core::draw::BackdropEntry>,
    /// Set while `paint_panels` paints a panel's list: that list is shown on a surface of its
    /// own, so nothing of the window is behind it for a backdrop to read.
    painting_panel: bool,
    /// Last frame's offscreen layers. Their opacity and effects live outside every primitive,
    /// so a change to them alone would otherwise produce no damage at all.
    layer_prints: Vec<(Rect, Vec<ui_core::Effect>)>,
    /// Node under the pointer last frame.
    hovered: Option<Id>,
    /// A click completed on this node since the last frame; consumed by the next frame.
    clicked: Option<Id>,
    /// That node's ancestor chain, root first; rebuilt with the frame.
    clicked_path: Vec<Id>,
    /// Enter or Space reached this node while it held focus; one frame only.
    activated: Option<Id>,
    /// That node's ancestor chain, root first; empty when nothing was activated.
    activated_path: Vec<Id>,
    /// The popup a press landed outside of; one frame only.
    dismissed: Option<Id>,
    /// Chords that fired since the last frame, oldest first. One frame answers the front
    /// and asks for another when facts remain, so a burst is not collapsed to its last key.
    shortcut_queue: VecDeque<ShortcutFact>,
    /// The node a drag was released on since the last frame; one frame only.
    dropped: Option<Id>,
    /// What that release landed on, when the node there accepts drops; one frame only.
    drop_target: Option<Id>,
    /// Pointer movement since the previous event while a drag is live.
    drag_step: Option<ui_core::geom::Point>,
    /// That node's ancestor chain, root first; rebuilt with the frame.
    dropped_path: Vec<Id>,
    /// The live drag's target and its ancestors, root first; rebuilt with the frame.
    drag_path: Vec<Id>,
    /// A drag crossing this surface, whether this app started it or the system did.
    drag_session: Option<DragSession>,
    /// The node that started the native drag the host is running, until it finishes.
    native_source: Option<Id>,
    /// A native drag the host has not started yet.
    native_request: Option<NativeDrag>,
    /// The platform menu a build asked the host to run once this frame presented.
    shell_menu_request: Option<ui_core::cx::ShellMenuRequest>,
    /// The part of the surface the pending drag's image shows, when it is not the source's
    /// whole ink: a text drag carries the selected lines, not the block holding them.
    native_crop: Option<ui_core::geom::Rect>,
    /// Hovered node and its ancestors, root first; only hover subscribers appear here.
    hovered_path: Vec<Id>,
    /// Every node under the pointer, subscribers or not; cursor inheritance walks this.
    pointer_path: Vec<Id>,
    /// The drag gesture in progress, if any.
    drag: ui_core::DragTracker,
    /// Node the primary button went down on, until it is released.
    pressed: Option<Id>,
    /// Where the primary button went down: the threshold is measured from here.
    press_at: Option<ui_core::geom::Point>,
    /// The secondary press a node asked for, until it is released.
    secondary: Option<SecondaryPress>,
    /// The node a middle press was reported to, and its burst length, until it is released.
    middle: Option<(Id, u32)>,
    /// The pressed node and its ancestors, so a control sees press through its children.
    pressed_path: Vec<Id>,
    /// Click target; `Escape` or a click elsewhere releases it, pointer leave does not.
    focused: Option<Id>,
    /// True when the last focus move came from the keyboard, not a pointer press.
    focus_visible: bool,
    /// The popup that last took the keyboard, so its arrival and departure are noticed once.
    scope_root: Option<ui_core::NodeId>,
    /// What held focus before that popup opened; closing it hands focus back.
    focus_before_scope: Option<Id>,
    /// The focus the previous frame ended with, so a move is noticed once.
    focus_shown: Option<Id>,
    /// The focused text input and its selection; a collapsed selection is a plain caret.
    editing: Option<(Id, ui_core::TextSelection)>,
    /// The column an Up/Down run is holding, kept so crossing a short line does not pull
    /// later steps to that line's end.
    goal_x: Option<f32>,
    /// The text node that drew an input's buffer last frame, by the input's own id.
    input_text_cache: Option<(Id, Id)>,
    /// Undo and redo snapshots per input owner; focus changes do not drop them.
    histories: FastMap<Id, InputHistory>,
    /// Where a read-only selection starts and where it reaches, each a block and a byte.
    selection: Option<SelectionEnds>,
    /// The blocks that selection covers, rebuilt whenever either end moves. Shared with the
    /// build's fact so every block in between paints its own bytes.
    selection_spans: Rc<[(Id, ui_core::TextSelection)]>,
    /// The selectable text node a pointer drag is extending.
    select_drag: Option<Id>,
    /// A press that landed inside the selection: where it landed and the byte under it.
    ///
    /// Travel past the threshold carries the selected text out as a drag; a release
    /// without travel collapses the selection there, as an editor's click does.
    selection_drag: Option<(Id, ui_core::geom::Point, usize)>,
    /// The text input a pointer drag is selecting inside.
    editing_drag: Option<Id>,
    /// A live scrollbar drag: which scroller, where the press landed, and the offset it
    /// began at. The bar owns the pointer for the duration, like a selection drag.
    scrollbar_drag: Option<ScrollbarDrag>,
    /// A scroller the pointer is currently dragging.
    pan: Option<ScrollPan>,
    /// The scroller a live press would pan once it travels past the threshold.
    pan_candidate: Option<Id>,
    /// Scrollers still coasting or springing back after the pointer let go.
    scroll_motion: FastMap<Id, ScrollMotion>,
    /// The scroller a running wheel burst is locked to, and where the pointer was when it
    /// started. A wheel carries no phases, so the pointer staying put stands in for one.
    wheel_lock: Option<(Id, ui_core::geom::Point)>,
    /// Snapping scrollers a wheel burst has moved and not yet settled: the burst has no end
    /// event either, so going quiet is what ends it.
    snap_pending: FastMap<Id, SnapBurst>,
    /// The input method's composition, owned by the engine and never merged early.
    preedit: Option<(Id, ui_core::Preedit)>,
    /// The selection a live composition started with: reveal is cut from it while the
    /// composition lives, so no byte under it is swapped between rendered and source.
    reveal: Option<(Id, ui_core::TextSelection)>,
    /// What was last handed to the target, so a change with no damage still presents.
    last_ime: crate::present::ImeState,
    /// Last press of each pointer button, for the multi-click rule.
    last_clicks: [Option<MultiPress>; 3],
    /// How this desktop joins presses into one multi-click.
    pointer_timing: crate::host::PointerTiming,
    /// The burst length of the primary press a pending click completes.
    pressed_clicks: u32,
    /// This frame's click burst length, reported with `clicked`.
    clicks: u32,
    /// Held modifiers, reported separately from key events.
    modifiers: ui_core::Modifiers,
    /// Hit index of the last laid-out frame; input arrives between frames.
    hit: HitIndex,
    /// This frame's consumer areas, reused: the input shape a layer surface publishes.
    input_areas: Vec<ui_core::geom::Rect>,
    /// Latest pointer position, kept so `dispatch` can re-run the hit test.
    pointer: Option<ui_core::geom::Point>,
    /// Where the host resolved the pointer across the surfaces of this tree, for one build:
    /// a window and the popups it hangs are one tree, and only one of them has the pointer.
    pointer_hint: Option<ui_core::geom::Point>,
    /// The shape the target was last told; a move that only changes it still needs a frame.
    cursor_sent: ui_core::CursorShape,
    /// When the caret's blink cycle last restarted: another editor took it, or the buffer
    /// changed. Moving the caret within one editor does not restart it.
    caret_blink_at: f64,
    /// The caret fact of the previous frame, for noticing a new editor.
    caret_last: CaretFact,
    /// The focused input's buffer digest when the cycle last restarted, so an edit restarts
    /// it and a caret move does not.
    caret_digest: Option<(Id, u64)>,
    /// Whether this frame drew a caret: the blink's own reason to want the next one.
    caret_painted: bool,
    /// The caret last revealed, with a digest of its input's buffer. A caret that did not
    /// move and an unchanged buffer must not drag a hand-scrolled view back.
    caret_revealed: Option<(Id, usize, u64)>,
    /// Declarations kept from the previous frame, reused while the facts they read hold.
    decl_cache: ui_core::DeclCache,
    /// Set when something the cache cannot see changed; the next build drops every entry.
    decls_stale: bool,
    /// Clipboard work the engine asked for; the host drains it and hands it to the platform.
    clipboard: Vec<crate::clipboard::ClipboardCommand>,
    /// The build asked for its surface to close; the host takes it once.
    close: bool,
    /// App-level bindings and the built-in text-editing table.
    shortcuts: crate::shortcut::Bindings,
    /// The open context menu, if any; it is declared as a panel of the platform's own.
    menu: Option<crate::menu::Menu>,
    /// How that menu looks; a design system replaces it through `set_menu_style`.
    menu_style: crate::menu::MenuStyle,
    /// The app's words for the engine's built-in text rows; none until it supplies them.
    text_menu_labels: Option<crate::menu::TextMenuLabels>,
    /// The surface wrapper's own child that the app's tree hangs from.
    ///
    /// Every surface is declared as `wrapper -> app root`, so the tree the app owns never
    /// changes parent: a menu panel appearing and leaving is the only structural change.
    app_root: Option<Id>,
    /// Where a clipboard answer goes when no editor owns the caret.
    paste_target: Option<Id>,
}

impl<A: App> AppLoop<A> {
    /// How a node kind participates in hit testing. An engine rule, not an app one:
    /// text paints but must not swallow the pointer, or a label steals its container's
    /// hover and every click lands on the wrong node.
    /// Text paints but must not swallow the pointer, or a label steals its control's hover.
    fn hit_behavior_of(kind: NodeKind, sense: ui_core::Sense, disabled: bool) -> HitBehavior {
        // An inert node takes nothing: no press, no hover, no drop.
        if disabled {
            return HitBehavior::Ignore;
        }
        // A node that lets hover through stays in the index but never ends the search.
        if sense.contains(ui_core::Sense::PASS_THROUGH) {
            return HitBehavior::PassThrough;
        }
        match kind {
            // Only text that asked to be selectable takes the pointer; every other label
            // stays inert so it cannot steal the click of the control it sits in.
            NodeKind::Text if sense.contains(ui_core::Sense::SELECT) => HitBehavior::Opaque,
            NodeKind::Text => HitBehavior::Ignore,
            _ => HitBehavior::Opaque,
        }
    }

    /// Loop whose text uses an explicit font stack.
    pub fn with_stack(app: A, families: &[&str]) -> Self {
        let mut this = Self::new(app);
        // One pass for both layout and paint: two instances diverge because their face
        // caches end up in different states, and then the line breaks differ.
        this.text = Rc::new(RefCell::new(TextPass::with_stack(families)));
        this.engine = BuiltinLayout::new(Box::new(PassMeasure(Rc::clone(&this.text))));
        this.measure = shared_measure(&this.text);
        this
    }

    pub fn new(app: A) -> Self {
        let text = Rc::new(RefCell::new(TextPass::new()));
        Self {
            app,
            tree: Tree::new(),
            engine: BuiltinLayout::new(Box::new(MonospaceMeasure::default())),
            renderer: CpuRenderer::new(),
            measure: shared_measure(&text),
            math: shared_math(&text),
            math_edit: shared_edit(&text),
            text,
            list: DrawList::default(),
            resolved: DrawList::default(),
            scroll_offsets: FastMap::default(),
            scrolled_since: FastSet::default(),
            scrollbar_fade: FastMap::default(),
            scrollbar_awake: false,
            images: FastMap::default(),
            canvases: Vec::new(),
            canvas_state: FastMap::default(),
            canvas_pixels: FastMap::default(),
            canvas_versions: FastMap::default(),
            splats: Vec::new(),
            image_slots: FastMap::default(),
            scale: 1.0,
            supersample: 1,
            atlas_version: u64::MAX,
            color_atlas_version: 0,
            last_size: (0, 0),
            clock: FrameClock::default(),
            frame_now: 0.0,
            frame_step: 0.0,
            pointer_read: false,
            modifier_read: false,
            time_driven: false,
            dispatched: false,
            input_blocked: false,
            pointer_steps: Vec::new(),
            anim_store: ui_core::AnimStore::new(),
            damage: DamageAccum::new(),
            frame: 0,
            node_prints: FastMap::default(),
            scroller_boxes: FastMap::default(),
            panel_views: Vec::new(),
            panel_presentation: FastMap::default(),
            backdrops: FastMap::default(),
            backdrops_now: FastMap::default(),
            painting_panel: false,
            layer_prints: Vec::new(),
            hovered: None,
            pressed: None,
            secondary: None,
            middle: None,
            focused: None,
            focus_visible: false,
            scope_root: None,
            focus_before_scope: None,
            focus_shown: None,
            editing: None,
            goal_x: None,
            input_text_cache: None,
            histories: FastMap::default(),
            selection: None,
            selection_spans: Rc::from(Vec::new()),
            select_drag: None,
            selection_drag: None,
            editing_drag: None,
            scrollbar_drag: None,
            pan: None,
            pan_candidate: None,
            scroll_motion: FastMap::default(),
            wheel_lock: None,
            snap_pending: FastMap::default(),
            preedit: None,
            reveal: None,
            last_ime: crate::present::ImeState::default(),
            last_clicks: [None; 3],
            pointer_timing: crate::host::PointerTiming::default(),
            pressed_clicks: 0,
            clicks: 0,
            modifiers: ui_core::Modifiers::default(),
            hit: HitIndex::new(),
            input_areas: Vec::new(),
            pointer: None,
            pointer_hint: None,
            cursor_sent: ui_core::CursorShape::Arrow,
            caret_blink_at: 0.0,
            caret_last: CaretFact {
                editing: None,
                selection: None,
            },
            caret_digest: None,
            caret_painted: false,
            caret_revealed: None,
            clicked: None,
            clicked_path: Vec::new(),
            activated: None,
            activated_path: Vec::new(),
            dismissed: None,
            shortcut_queue: VecDeque::new(),
            dropped: None,
            drop_target: None,
            dropped_path: Vec::new(),
            drag_path: Vec::new(),
            drag_session: None,
            native_source: None,
            native_request: None,
            shell_menu_request: None,
            native_crop: None,
            drag_step: None,
            hovered_path: Vec::new(),
            pointer_path: Vec::new(),
            pressed_path: Vec::new(),
            drag: ui_core::DragTracker::new(),
            press_at: None,
            decl_cache: ui_core::DeclCache::default(),
            decls_stale: true,
            clipboard: Vec::new(),
            close: false,
            shortcuts: crate::shortcut::Bindings::with_builtins(),
            menu: None,
            menu_style: crate::menu::MenuStyle::default(),
            text_menu_labels: None,
            app_root: None,
            paste_target: None,
        }
    }

    /// Replace the fallback menu's look with the design system's own.
    pub fn set_menu_style(&mut self, style: crate::menu::MenuStyle) {
        self.menu_style = style;
    }

    /// Supply the words the built-in text rows are shown with; `None` withdraws them.
    pub fn set_text_menu_labels(&mut self, labels: Option<crate::menu::TextMenuLabels>) {
        self.text_menu_labels = labels;
    }

    /// The chord `id` answers right now, so an app can print it beside a menu row.
    pub fn shortcut(&self, id: ui_core::ShortcutId) -> Option<ui_core::KeyChord> {
        self.shortcuts.chord_of(id)
    }

    /// Bind a chord for the whole app: it fires only when no control claimed it.
    pub fn bind_shortcut(&mut self, chord: ui_core::KeyChord, id: ui_core::ShortcutId) {
        self.shortcuts.bind(chord, id);
    }

    /// Point an id at another chord, built-in or app-level.
    pub fn rebind_shortcut(&mut self, id: ui_core::ShortcutId, chord: ui_core::KeyChord) {
        self.shortcuts.rebind(id, chord);
    }

    /// Drop every binding of `id`; the chords reach nothing afterwards.
    pub fn unbind_shortcut(&mut self, id: ui_core::ShortcutId) {
        self.shortcuts.unbind(id);
    }

    /// Clipboard work this frame's events asked for; the host hands it to the platform.
    pub fn take_clipboard_commands(&mut self) -> Vec<crate::clipboard::ClipboardCommand> {
        std::mem::take(&mut self.clipboard)
    }

    /// Whether the app asked for its surface to close; takes the request.
    pub fn take_close(&mut self) -> bool {
        std::mem::take(&mut self.close)
    }

    /// Pointer steps the last build asked for, oldest first; takes them.
    pub fn take_pointer_steps(&mut self) -> Vec<(f32, f32)> {
        std::mem::take(&mut self.pointer_steps)
    }

    /// The platform menu a build asked the host to run, waiting for the frame to present.
    ///
    /// The host runs it from its own turn: the platform's popup blocks on a message loop of
    /// its own, and a build that opened it would stay unpainted until the user dismissed it.
    pub fn take_shell_menu(&mut self) -> Option<ui_core::cx::ShellMenuRequest> {
        self.shell_menu_request.take()
    }

    /// The drag that asked to leave the app, waiting for the host to start it.
    ///
    /// The source has already heard `DragStarted`; the host reports the outcome later as
    /// `Event::Dnd(DndEvent::Finished)`.
    pub fn take_native_drag(&mut self) -> Option<NativeDrag> {
        let mut drag = self.native_request.take()?;
        let crop = self.native_crop.take();
        // The image is cut from the frame that just painted the source, not the one that
        // asked for the drag: an element that changed on the drag's first frame has to look
        // the way it does now, not the way it did before the press.
        if drag.image.is_none()
            && let Some(source) = self.native_source
            && let Some(nid) = self.tree.find_by_id(source)
        {
            drag.image = self.paint_drag_image(nid, drag.origin, crop);
        }
        Some(drag)
    }

    /// Paint `source`'s own subtree into a buffer, with no surface around it.
    ///
    /// The edge-to-edge frame is the space the primitives already live in, so the subtree is
    /// rasterized into a buffer of that size and the painted box is cropped out of it: no
    /// primitive is rewritten, and clips, layers and effects keep the meaning they had. The
    /// buffer is one frame of window size, taken once per gesture. `crop` narrows what the
    /// host shows — a text drag takes the selected lines rather than the block around them.
    fn paint_drag_image(
        &mut self,
        source: ui_core::id::NodeId,
        origin: ui_core::geom::Point,
        crop: Option<ui_core::geom::Rect>,
    ) -> Option<DragImage> {
        let (list, painted) = self.resolved.subtree(&self.tree, source);
        let painted = match crop {
            Some(crop) => painted.intersection(crop)?,
            None => painted,
        };
        if painted.size.w <= 0.0 || painted.size.h <= 0.0 {
            return None;
        }
        let scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        };
        let canvas = self.resolved.canvas;
        let width = (canvas.size.w * scale).ceil().max(1.0) as u32;
        let height = (canvas.size.h * scale).ceil().max(1.0) as u32;
        let mut pixels = ui_paint::Pixels::new(width, height);
        self.prepare_cpu_renderer(scale);
        self.renderer
            .render(&list, &mut pixels, &ui_core::Damage::Full);
        // The crop is the painted box, clamped to the frame it was painted into.
        let x0 = (painted.origin.x * scale).floor().max(0.0) as u32;
        let y0 = (painted.origin.y * scale).floor().max(0.0) as u32;
        let x1 = ((painted.origin.x + painted.size.w) * scale).ceil() as i64;
        let y1 = ((painted.origin.y + painted.size.h) * scale).ceil() as i64;
        let x1 = x1.clamp(0, width as i64) as u32;
        let y1 = y1.clamp(0, height as i64) as u32;
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if w == 0 || h == 0 {
            return None;
        }
        let mut bgra = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for row in y0..y0 + h {
            let start = ((row as usize) * (width as usize) + x0 as usize) * 4;
            bgra.extend_from_slice(&pixels.data[start..start + (w as usize) * 4]);
        }
        let hotspot = (
            ((origin.x * scale).round() as i32) - x0 as i32,
            ((origin.y * scale).round() as i32) - y0 as i32,
        );
        Some(DragImage {
            size: (w, h),
            bgra,
            hotspot,
        })
    }

    /// What dropping the live drag here would do, or `None` when nothing takes it.
    ///
    /// The host publishes this to the operating system so the pointer shows the effect the
    /// target negotiated instead of a generic drag cursor.
    pub fn drop_effect(&self) -> Option<ui_core::DropEffect> {
        let session = self.drag_session.as_ref()?;
        session.target?;
        session.negotiated(self.modifiers)
    }

    /// The MIME types the drag over this surface offers, when one is live.
    pub fn drag_types(&self) -> Option<&[String]> {
        self.drag_session
            .as_ref()
            .filter(|session| session.types_decide())
            .map(|session| session.types.as_slice())
    }

    /// Drop every cached declaration; the app calls this when it changed state silently.
    pub fn invalidate_decls(&mut self) {
        self.decls_stale = true;
    }

    /// How many declarations the cache is holding, for diagnostics.
    pub fn cached_decls(&self) -> usize {
        self.decl_cache.len()
    }

    /// Cache entries held, plus the running hit and miss totals.
    pub fn decl_cache_stats(&self) -> (usize, u64, u64) {
        self.decl_cache.stats()
    }

    /// What still keeps the loop awake, with the node each animation belongs to.
    /// Whether this pointer position can change what the last frame drew.
    ///
    /// A loop uses it to absorb pointer samples above the refresh rate: moving inside one
    /// hover owner, with nothing reading the position, cannot change a pixel.
    pub fn pointer_affects_frame(&self, p: ui_core::geom::Point) -> bool {
        // A gesture owns the position; every sample must reach the tree. A text selection
        // is one of them: it takes the press without keeping `pressed`, and it repaints
        // while the hover stays exactly where it was.
        if self.pressed.is_some()
            || self.pan.is_some()
            || self.select_drag.is_some()
            || self.middle.is_some()
            || self.drag.is_dragging()
        {
            return true;
        }
        // Something asked for the raw position, so it can change anywhere inside itself.
        if self.pointer_read {
            return true;
        }
        let hovered = self
            .hit
            .topmost_consumer(p)
            .and_then(|nid| self.tree.get(nid).map(|n| n.id))
            .and_then(|id| self.hover_owner(id));
        // A shape reaches the target only with a frame, so a move that changes just the
        // shape is still work: hovering a paragraph changes no pixel of its own.
        hovered != self.hovered || self.cursor_shape_at(p) != self.cursor_sent
    }

    /// Whether an event since the last call requires a build; takes the flag.
    /// Whether something asked for a frame that no build has drawn yet.
    ///
    /// A loop that is about to block must wake at once while this holds: the demand came
    /// from input or a dismissal that arrived after the last build, and nothing else will
    /// arrive to carry it.
    pub fn frame_due(&self) -> bool {
        self.dispatched
    }

    pub fn take_dispatched(&mut self) -> bool {
        std::mem::take(&mut self.dispatched)
    }

    /// Ask for a rebuild with no event behind it: an app that changed its own state says so.
    pub fn request_frame(&mut self) {
        self.dispatched = true;
    }

    /// Whether anything in the engine still wants the next frame.
    pub fn is_animating(&self) -> bool {
        self.anim_store.is_animating(self.clock.now())
            || !self.splats.is_empty()
            || !self.scroll_motion.is_empty()
            // A control that read the clock must be rebuilt to see the new time.
            || self.time_driven
    }

    /// The caret and selection this frame holds, in the shape a blink reset compares.
    fn caret_fact(&self) -> CaretFact {
        CaretFact {
            editing: self.editing.map(|(id, s)| (id, s.caret)),
            selection: self.selection.map(|ends| {
                let span = self
                    .span_of(ends.head.0)
                    .unwrap_or(ui_core::TextSelection::collapsed(ends.head.1));
                (ends.head.0, ends.anchor.0, span.anchor, span.caret)
            }),
        }
    }

    /// Whether the caret is in its lit half of the cycle at `now`.
    fn caret_blink_lit(&self, now: f64) -> bool {
        let elapsed = (now - self.caret_blink_at).max(0.0);
        ((elapsed / CARET_BLINK) as u64).is_multiple_of(2)
    }

    /// How long the engine itself wants to be woken, with no input: the caret's next toggle.
    pub fn next_wake(&self) -> Option<std::time::Duration> {
        let caret = if !self.caret_painted || self.editing.is_none() {
            None
        } else {
            let elapsed = (self.frame_now - self.caret_blink_at).max(0.0);
            Some(std::time::Duration::from_secs_f64(
                CARET_BLINK - elapsed % CARET_BLINK,
            ))
        };
        // A snapping scroller owes the frame that settles it once its burst has gone quiet;
        // nothing else would wake the loop for it.
        let snap = self
            .snap_pending
            .values()
            .map(|burst| (burst.at + SNAP_IDLE - self.frame_now).max(0.0))
            .min_by(f64::total_cmp)
            .map(std::time::Duration::from_secs_f64);
        match (caret, snap) {
            (Some(caret), Some(snap)) => Some(caret.min(snap)),
            (caret, snap) => caret.or(snap),
        }
    }

    pub fn anim_report(&self) -> String {
        let moving = self.anim_store.moving(self.clock.now());
        let (springs, live) = self.anim_store.spring_counts();
        // A frozen spring is the whole question here, so the counts come first.
        let head = format!(
            "springs={live}/{springs} scroll={} tween={}",
            self.scroll_motion.len(),
            moving.len().saturating_sub(live)
        );
        if moving.is_empty() {
            return format!("still ({head})");
        }
        let body = moving
            .iter()
            .map(|m| {
                let place = self
                    .tree
                    .find_by_id(m.id())
                    .and_then(|n| self.tree.get(n))
                    .map(|n| format!("{:?}@{:?}", n.kind, n.rect.origin))
                    .unwrap_or_else(|| "no node (derived id)".to_string());
                format!("{m} -> {place}")
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("{head} | {body}")
    }

    /// Device pixels per logical unit. Text is rasterized and placed at this scale.
    pub fn set_scale(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            if scale != self.scale {
                self.decls_stale = true;
            }
            self.scale = scale;
        }
    }

    /// Adopt the desktop's multi-click rule: the gap and the rectangle that join two presses.
    pub fn set_pointer_timing(&mut self, timing: crate::host::PointerTiming) {
        self.pointer_timing = timing;
    }

    /// The grid text is rasterized on: the device scale times the renderer's supersample.
    pub fn set_supersample(&mut self, ss: u32) {
        self.supersample = ss.clamp(1, 4);
    }

    fn text_scale(&self) -> f32 {
        self.scale * self.supersample as f32
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The backend-neutral draw list: every `Text` primitive already resolved to glyphs.
    pub fn draw_list(&self) -> &DrawList {
        &self.resolved
    }

    pub fn text_pass(&self) -> std::cell::Ref<'_, TextPass> {
        self.text.borrow()
    }

    /// Mutable text pass, for measuring and diagnostics.
    pub fn text_pass_mut(&mut self) -> std::cell::RefMut<'_, TextPass> {
        self.text.borrow_mut()
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The engine's stored offset for a `SCROLL_OWNED` container, or `None` when the id
    /// never received a wheel. Tests and apps that follow the scroll read this instead of
    /// keeping their own copy.
    pub fn scroll_of(&self, id: Id) -> Option<f32> {
        self.scroll_offsets.get(&id).map(|o| o.y)
    }

    /// Both stored offsets of a `SCROLL_OWNED` container; `None` when it never scrolled.
    pub fn scroll_offset(&self, id: Id) -> Option<ui_core::cx::ScrollOffset> {
        self.scroll_offsets.get(&id).copied()
    }

    /// The opacity an automatic scrollbar had this frame, or `None` for a bar that does not
    /// fade or a scroller that is not in the tree. Public so a caller can tell whether the
    /// bar it draws is still fading, which is the engine's state and not the widget's.
    pub fn scrollbar_opacity(&self, id: Id) -> Option<f32> {
        self.scrollbar_fade.get(&id).map(|f| f.opacity)
    }

    /// Register a texture for the run; the app refers to it by `ImageSpec::id`.
    ///
    /// A picture a build produced goes through [`crate::cx::Cx::register_image`] instead: the
    /// engine keeps it for that frame only, so a list of many pictures does not accumulate.
    pub fn register_image(&mut self, entry: ui_core::draw::ImageEntry) {
        self.images.insert(
            entry.id,
            RegisteredImage {
                entry,
                seen: None,
            },
        );
    }

    /// Register font bytes under `name`; runs naming that family resolve to them.
    ///
    /// The bytes stay the host's business: the engine ships no fonts, and a proprietary one
    /// must be supplied by whoever is allowed to distribute it.
    pub fn register_font(&mut self, name: &str, data: std::sync::Arc<[u8]>) -> bool {
        let ok = self.text.borrow_mut().register(name, data);
        if ok {
            // A new family moves metrics, so a node laid out before it must be laid out
            // again — the same reason a stack change drops every declaration.
            self.decls_stale = true;
        }
        ok
    }

    /// The preferred font families, in order; empty leaves the platform's own choice.
    pub fn font_stack(&self) -> Vec<String> {
        self.text.borrow().families().to_vec()
    }

    /// Follow the host's or the user's font choice, at any time.
    ///
    /// The name must name an installed or registered family; `font_families` lists them.
    /// Every cached declaration is dropped with the change: a new family moves every
    /// metric, and a node laid out under the old one would keep its old size.
    pub fn set_font_stack(&mut self, families: &[&str]) {
        self.text.borrow_mut().set_families(families);
        self.decls_stale = true;
    }

    /// Installed and registered family names, for a font picker.
    pub fn font_families(&self) -> Vec<String> {
        self.text.borrow().family_names()
    }

    pub fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }

    /// Write `id`'s ancestor chain into `out`, root first; empty when there is no such node.
    fn fill_ancestors(tree: &ui_core::Tree, out: &mut Vec<Id>, id: Option<Id>) {
        out.clear();
        let mut cur = id.and_then(|id| tree.find_by_id(id));
        while let Some(node) = cur.and_then(|nid| tree.get(nid)) {
            out.push(node.id);
            cur = node.parent;
        }
        out.reverse();
    }

    /// Run one frame against `target`, advancing animations by the wall clock.
    pub fn frame(
        &mut self,
        target: &mut dyn PresentTarget,
        pointer: Option<ui_core::geom::Point>,
    ) -> FrameOutcome {
        self.present(target, pointer, None)
    }

    /// Present one frame after advancing animations by exactly `dt` seconds; `0.0` freezes them.
    /// Test-only: production uses `frame`. Scripted time never reads the wall clock.
    pub fn frame_with_dt(
        &mut self,
        target: &mut dyn PresentTarget,
        pointer: Option<ui_core::geom::Point>,
        dt: f32,
    ) -> FrameOutcome {
        self.present(target, pointer, Some(dt.max(0.0)))
    }

    /// Build one frame and hand its scene to `target`; the engine owns the commit decision.
    pub fn frame_scene(
        &mut self,
        target: &mut dyn crate::present::SceneTarget,
        pointer: Option<ui_core::geom::Point>,
    ) -> Result<SceneFrameOutcome, String> {
        let (outcome, damage, ime, ime_changed) =
            timed!("build", self.begin_frame(target.size(), pointer, None));
        // Before the early return: a pointer crossing between a claimed area and bare
        // canvas changes no pixel by itself, and the keyboard must follow it anyway.
        target.set_pointer_inside(self.pointer_inside(pointer));
        self.collect_input_areas();
        target.set_input_region(&self.input_areas);
        // Before any early return: the shape can change on a frame that paints nothing, and
        // a still pointer produces no further event to carry it later.
        target.set_cursor(outcome.cursor);
        target.set_ime(ime);
        // Still frames hand nothing over; an animating one must commit even with no damage,
        // because that commit is what requests the callback the loop paces on.
        if damage.is_none() && !outcome.animating && !ime_changed && !target.commit_pending() {
            return Ok(SceneFrameOutcome {
                outcome,
                presented: None,
            });
        }
        let canvases = self.resolve_canvases();
        let text = self.text.borrow();
        let presented = timed!(
            "present",
            target.present_scene(&self.resolved, &text, &damage, &canvases)
        )?;
        drop(text);
        for canvas in &canvases {
            if canvas.changed {
                self.canvas_state
                    .insert(canvas.id.0, (canvas.version, canvas.size));
            }
        }
        Ok(SceneFrameOutcome {
            outcome,
            presented: Some(presented),
        })
    }

    /// The panels the last layout put on a surface of the platform's own.
    pub fn platform_panels(&self) -> &[ui_core::layout::PlatformPanel] {
        self.tree.platform_panels()
    }

    /// Which build the panel lists come from.
    ///
    /// A panel surface shows the frame it was handed, not the tree as it is now: comparing
    /// this against the number it presented last is what tells it there is nothing new.
    pub fn frame_number(&self) -> u64 {
        self.frame
    }

    /// Say whether this window's host opens a surface for the panels that ask for one.
    ///
    /// The host is the only party that knows, and it knows before the first frame: it sets
    /// this when it builds the loop. A loop left without it keeps every panel in the window,
    /// laid out and painted with the parent, because an unanswered request must not make a
    /// panel invisible — a bare loop has nobody to paint a surface it cannot open.
    pub fn set_panel_surfaces(&mut self, presents: bool) {
        self.tree.set_panel_surfaces(presents);
    }

    /// Name the output the platform panels are placed against, in this surface's own
    /// coordinates; `None` leaves them at their preferred spot.
    ///
    /// The host owns this: only the backend knows the monitor's work area, and a panel's
    /// surface stands where the tree placed it, so the tree is the one that constrains it.
    pub fn set_output_bounds(&mut self, bounds: Option<ui_core::geom::Rect>) {
        self.tree.set_output_bounds(bounds);
    }

    /// Say where a panel's declaration is presented, once the host has decided.
    ///
    /// `true` when the platform opened a surface of its own for the panel, `false` when no
    /// surface could be had and the panel is painted by its parent after all.
    pub fn set_panel_presentation(&mut self, node: ui_core::NodeId, platform: bool) {
        self.panel_presentation.insert(node, platform);
    }

    /// The platform took a panel's surface back — the grab ended, the window server closed
    /// it. Reported to the app the way a press outside is, so it closes what it declared.
    ///
    /// Returns false when the node is no longer in the tree, where there is nothing to
    /// dismiss and nobody to tell.
    pub fn dismiss_panel(&mut self, node: ui_core::NodeId) -> bool {
        let Some(id) = self.tree.get(node).map(|decl| decl.id) else {
            return false;
        };
        self.dismissed = Some(id);
        // The menu's panels are the engine's own: no app declared them, so there is no
        // decision to hand over, and the menu closes here. An app-declared panel keeps the
        // fact for its own build to obey.
        self.menu_dismissed(id);
        // The app reads the dismissal in a build; without the demand the next build would
        // only happen when unrelated input arrived, which is too late to matter.
        self.dispatched = true;
        true
    }

    /// Say where the pointer stands in this tree, for the build this turn will make.
    ///
    /// A window and the popups it hangs share one tree, and each surface knows the pointer
    /// only while it is on that surface. The host steps all of them, so it is the one place
    /// that can name the pointer wherever it is; a build with a pointer of its own still
    /// wins over this answer.
    pub fn set_pointer_hint(&mut self, pointer: Option<ui_core::geom::Point>) {
        self.pointer_hint = pointer;
    }

    /// Drop a hint no build took, so it cannot outlive the turn that resolved it.
    pub fn clear_pointer_hint(&mut self) {
        self.pointer_hint = None;
    }

    /// Whether the last build painted a list for this panel.
    ///
    /// A panel the host bound after that build has none yet: its surface waits for the
    /// frame the binding itself asked for.
    pub fn panel_painted(&self, node: ui_core::NodeId) -> bool {
        self.panel_views.iter().any(|(id, _)| *id == node)
    }

    /// Hand `node`'s panel, as the last frame painted it, to the surface the platform
    /// opened for it.
    ///
    /// The list is rebased on the panel's own top-left and carries the whole panel every
    /// time: that surface shows nothing else, so it has nothing to diff against.
    pub fn present_panel(
        &self,
        node: ui_core::NodeId,
        target: &mut dyn crate::present::SceneTarget,
    ) -> Result<crate::present::PresentStats, String> {
        let Some((_, list)) = self.panel_views.iter().find(|(id, _)| *id == node) else {
            return Err("the panel is not in this frame".into());
        };
        let text = self.text.borrow();
        target.present_scene(list, &text, &ui_core::Damage::Full, &[])
    }

    /// Drive each transitioned node's axes toward the size they declared, writing the
    /// interpolated values back before layout; a settled node matches its declaration.
    ///
    /// A `Fit` (hug) axis has no declared pixel extent, so its target is the extent its
    /// content last measured — the same clamp hug itself applies — and the axis is pinned
    /// there for the leg instead of being left to re-measure.
    fn resolve_size_transitions(&mut self) {
        if self.tree.animated().is_empty() {
            return;
        }
        // The list lives on the tree; take a copy so nodes can be written while walking it.
        let ids = self.tree.animated().to_vec();
        let now = self.frame_now;
        for nid in ids {
            let Some((id, spec, target)) = self.tree.get(nid).and_then(|n| {
                let spec = n.data.params()?.size_transition?;
                Some((n.id, spec, n.declared_size))
            }) else {
                continue;
            };
            let mut size = target;
            for (axis, key, vertical) in [
                (&mut size.width, "size.w", false),
                (&mut size.height, "size.y", true),
            ] {
                let (target, hug) = match *axis {
                    AxisSize {
                        sizing: Sizing::Definite(Len::Px(asked)),
                        ..
                    } => (asked, false),
                    AxisSize {
                        sizing: Sizing::Fit,
                        ..
                    } => match self
                        .tree
                        .get(nid)
                        .and_then(|n| ui_core::hug_extent(n, vertical))
                    {
                        Some(extent) => (extent, true),
                        None => continue,
                    },
                    _ => continue,
                };
                let value = self.anim_store.animate(id.with(key), target, spec, now);
                *axis = if hug {
                    AxisSize::hug_pinned(value)
                } else {
                    AxisSize::fixed(value)
                };
            }
            // The measure cache answers by constraints alone, so the write itself must say
            // the node changed — the frame a leg lands on a new value is exactly that case.
            let changed = matches!(self.tree.get(nid), Some(n) if n.size_class != size);
            if let Some(n) = self.tree.get_mut(nid) {
                n.size_class = size;
            }
            if changed {
                self.tree.mark_dirty_up(nid);
            }
        }
    }

    /// Advance each entering node toward full presence, and each held one toward zero.
    fn resolve_presence(&mut self) {
        // A node id outlives the node, so a dialog shown again must start over.
        for nid in self.tree.appeared().to_vec() {
            if let Some(id) = self.tree.get(nid).map(|n| n.id) {
                self.anim_store.forget(id.with("presence"));
            }
        }
        let now = self.frame_now;
        for nid in self.tree.animated().to_vec() {
            let Some((id, enter, presence)) = self
                .tree
                .get(nid)
                .and_then(|n| Some((n.id, n.data.params()?.enter?, n.presence)))
            else {
                continue;
            };
            let value = self
                .anim_store
                .travel(id.with("presence"), presence, 1.0, enter.spec, now);
            // A scale on its own clock: WinUI fades its dialog in 83 ms and scales it over
            // 250, and one shared value would make one of the two wrong.
            let scale_value = match enter.scale_spec {
                Some(spec) => {
                    self.anim_store
                        .travel(id.with("presence-scale"), presence, 1.0, spec, now)
                }
                None => value,
            };
            self.paint_presence(nid, enter, value, scale_value);
        }

        // Exit: the node is already out of the declaration and out of layout; it only has
        // to fade out where it stood, and then nothing of it remains.
        for nid in self.tree.held().to_vec() {
            let Some((id, exit, presence, space)) = self.tree.get(nid).and_then(|n| {
                let params = n.data.params()?;
                Some((n.id, params.exit?, n.presence, params.exit_space))
            }) else {
                self.tree.release(nid);
                continue;
            };
            // A node is held once in its life, so the handle names this exit uniquely: it
            // cannot inherit the landed 0 a previous appearance left behind.
            let key = id.with(("exit", nid.index(), nid.generation()));
            let value = self.anim_store.travel(key, presence, 0.0, exit.spec, now);
            if value <= 0.0 {
                self.anim_store.forget(key);
                self.anim_store.forget(id.with("presence-scale"));
                self.tree.release(nid);
            } else {
                if space == ui_core::ExitSpace::Collapse {
                    self.collapse_held(nid, value);
                }
                let scale_value = match exit.scale_spec {
                    Some(spec) => {
                        self.anim_store
                            .travel(id.with("presence-scale"), presence, 0.0, spec, now)
                    }
                    None => value,
                };
                self.paint_presence(nid, exit, value, scale_value);
            }
        }
    }

    /// Shrink a collapsing node's slot with its exit, so its siblings travel with it.
    ///
    /// Only a definite axis shrinks: a `fill` axis takes its parent's size anyway, and a
    /// `hug` one is the content's, which the exit does not change.
    fn collapse_held(&mut self, nid: ui_core::NodeId, value: f32) {
        let Some(node) = self.tree.get(nid) else {
            return;
        };
        let base = node.exit_size;
        let mut size = node.size_class;
        for (axis, extent) in [(&mut size.width, base.w), (&mut size.height, base.h)] {
            if matches!(axis.sizing, Sizing::Definite(Len::Px(_))) {
                *axis = AxisSize::fixed(extent * value.max(0.0));
            }
        }
        if let Some(node) = self.tree.get_mut(nid) {
            node.size_class = size;
        }
        self.tree.mark_dirty_up(nid);
    }

    /// Write what paint reads for a node `value` of the way from `far` to its declaration.
    fn paint_presence(
        &mut self,
        nid: ui_core::NodeId,
        far: ui_core::tree::Presence,
        value: f32,
        scale_value: f32,
    ) {
        if let Some(n) = self.tree.get_mut(nid) {
            n.presence = value;
            let paint = far.values_at(value, scale_value);
            n.presence_alpha = paint.alpha;
            n.presence_shift = paint.shift;
            n.presence_scale = paint.scale;
        }
    }

    /// Whether the pointer is on what this surface consumes, or a gesture holds it.
    ///
    /// The same set the input region claims, plus what the region cannot express: a gesture
    /// keeps the keyboard past the node's edge.
    fn pointer_inside(&self, pointer: Option<ui_core::geom::Point>) -> bool {
        self.pressed.is_some()
            || self.select_drag.is_some()
            || self.pan.is_some()
            || pointer.is_some_and(|at| self.hit.topmost_consumer(at).is_some())
    }

    /// Fill the scratch with this frame's consumer areas: the surface's input shape.
    ///
    /// Taken from the hit index, not the paint list: a decoration paints without taking
    /// input, and the areas a tree would consume are exactly what a layer surface may claim.
    fn collect_input_areas(&mut self) {
        let mut areas = std::mem::take(&mut self.input_areas);
        areas.clear();
        areas.extend(self.hit.consumer_areas());
        self.input_areas = areas;
    }

    /// Everything a frame needs before it can be drawn: size, build, damage, IME state.
    fn begin_frame(
        &mut self,
        (dw, dh): (u32, u32),
        pointer: Option<ui_core::geom::Point>,
        step: Option<f32>,
    ) -> (
        FrameOutcome,
        ui_core::Damage,
        crate::present::ImeState,
        bool,
    ) {
        // Layout runs in logical units; the buffer is larger by the scale factor.
        let scale = self.scale;
        let logical = (
            (dw as f32 / scale).round() as u32,
            (dh as f32 / scale).round() as u32,
        );
        // A resize repaints everything: the damage diff was computed against a buffer of a
        // different size, so its rects mean nothing in the new one.
        if self.last_size != (dw, dh) {
            self.last_size = (dw, dh);
            self.damage.add_full();
            self.decls_stale = true;
        }
        let outcome = self.build_step(logical, pointer, step);
        let damage = outcome.damage.clone();
        // The IME state travels with the same frame's layout: its caret rect is what the
        // compositor places the candidate window against.
        let ime = self.ime_state();
        let ime_changed = ime != self.last_ime;
        // The stored copy only changes when the state does; an unchanged frame would
        // otherwise copy the surrounding-text window (up to 4 KB) for nothing.
        if ime_changed {
            self.last_ime = ime.clone();
        }
        (outcome, damage, ime, ime_changed)
    }

    /// Give the CPU rasterizer this frame's scale and glyph atlases.
    ///
    /// The CPU path is not only a presenter: a drag image is cut with the same rasterizer,
    /// so both ask for the same textures before they draw.
    fn prepare_cpu_renderer(&mut self, scale: f32) {
        self.renderer.set_scale(scale);
        // Hand the atlas to the renderer only when it actually grew: copying 1 MiB
        // every frame would dominate the frame cost.
        let pass = self.text.borrow();
        if pass.version() != self.atlas_version {
            self.atlas_version = pass.version();
            let (w, h) = pass.dimensions();
            self.renderer.set_atlas(ui_paint::CoverageAtlas {
                width: w,
                height: h,
                data: pass.atlas().to_vec(),
            });
        }
        // The colour page changes only when an emoji is first rasterized; re-uploading
        // its megabytes alongside every text glyph would be waste.
        if pass.color_version() != self.color_atlas_version {
            self.color_atlas_version = pass.color_version();
            if !pass.color_atlas().is_empty() {
                let (w, h) = pass.color_dimensions();
                self.renderer.set_color_atlas(ui_paint::ColorAtlas {
                    width: w,
                    height: h,
                    data: pass.color_atlas().to_vec(),
                });
            }
        }
    }

    fn present(
        &mut self,
        target: &mut dyn PresentTarget,
        pointer: Option<ui_core::geom::Point>,
        step: Option<f32>,
    ) -> FrameOutcome {
        let scale = self.scale;
        let (outcome, damage, ime, ime_changed) = self.begin_frame(target.size(), pointer, step);
        target.set_pointer_inside(self.pointer_inside(pointer));
        self.collect_input_areas();
        target.set_input_region(&self.input_areas);
        target.set_cursor(outcome.cursor);
        target.set_ime(ime);
        self.prepare_cpu_renderer(scale);

        // Still frames hand nothing over; an animating one must present even with no
        // damage, because the commit is what requests the callback the loop paces on.
        if damage.is_none() && !outcome.animating && !ime_changed {
            return outcome;
        }

        // Canvases before the list: it samples their pixels this frame. The CPU path keeps
        // them in memory; the GPU path hands the same frames to the target.
        let canvases = self.resolve_canvases();
        self.paint_canvases(&canvases);

        let target_pixels = target.pixels_mut();
        self.renderer.render(&self.resolved, target_pixels, &damage);
        // The renderer repaints the whole surface, so the compositor must be told the whole
        // surface changed. Reporting only the diff would leave it showing its cached copy
        // wherever we repainted but did not report — the same class of stale-pixel bug as
        // the caching this replaced.
        let _ = target.present(&ui_core::damage::Damage::Full);
        outcome
    }

    /// Resolve this frame's canvas lists at 1:1, and say which targets have to be painted.
    ///
    /// A canvas is in its content's pixels, so text is shaped at scale 1: the pin's zoom
    /// only ever scales the finished texture. An unchanged canvas hands over no primitives,
    /// which is what keeps a still frame free of canvas work.
    fn resolve_canvases(&mut self) -> Vec<crate::present::CanvasFrame> {
        let canvases = std::mem::take(&mut self.canvases);
        // A target only survives while the frame names it, so the recorded state goes with
        // it: a canvas that comes back is repainted rather than assumed intact.
        self.canvas_state
            .retain(|id, _| canvases.iter().any(|canvas| canvas.id.0 == *id));
        let mut frames = Vec::with_capacity(canvases.len());
        for canvas in canvases {
            let size = canvas.size;
            let changed =
                self.canvas_state.get(&canvas.id.0) != Some(&(canvas.version, canvas.size));
            let mut list = ui_core::draw::DrawList::new(ui_core::geom::Rect::new(
                0.0,
                0.0,
                size.0 as f32,
                size.1 as f32,
            ));
            if changed {
                self.text
                    .borrow_mut()
                    .resolve_into(&canvas.list, 1.0, &mut list);
            }
            frames.push(crate::present::CanvasFrame {
                id: canvas.id,
                size,
                version: canvas.version,
                changed,
                list,
            });
        }
        frames
    }

    /// Paint changed canvases into CPU buffers, then point the list's canvas entries at
    /// them.
    ///
    /// The CPU presenter has no texture to keep, so the target's pixels live here. The
    /// buffers are `Arc`s: sampling one copies nothing, and a still canvas keeps its bytes
    /// across frames.
    fn paint_canvases(&mut self, frames: &[crate::present::CanvasFrame]) {
        let scale = self.scale;
        for frame in frames {
            if !frame.changed {
                continue;
            }
            let mut pixels = ui_paint::Pixels::new(frame.size.0.max(1), frame.size.1.max(1));
            self.renderer.set_scale(1.0);
            self.renderer
                .render(&frame.list, &mut pixels, &ui_core::damage::Damage::Full);
            let bytes = ui_paint::encode_rgba_straight(&pixels);
            self.canvas_pixels
                .insert(frame.id.0, std::sync::Arc::new(bytes));
            self.canvas_state
                .insert(frame.id.0, (frame.version, frame.size));
        }
        self.renderer.set_scale(scale);
        let live: FastSet<u64> = frames.iter().map(|frame| frame.id.0).collect();
        self.canvas_pixels.retain(|id, _| live.contains(id));
        for entry in &mut self.resolved.images {
            if let ui_core::draw::ImageSource::Canvas(id) = entry.source
                && let Some(bytes) = self.canvas_pixels.get(&id.0)
            {
                entry.source = ui_core::draw::ImageSource::Pixels(std::sync::Arc::clone(bytes));
                // The CPU canvas holds sRGB8 whatever the picture it sampled said.
                entry.colour = ui_core::colour::ContentColour::SDR;
            }
        }
    }

    /// Build one frame without touching a backend, advancing animations by the wall clock.
    pub fn build(
        &mut self,
        size: (u32, u32),
        pointer: Option<ui_core::geom::Point>,
    ) -> FrameOutcome {
        self.build_step(size, pointer, None)
    }

    /// Build with an explicit time step, so animations advance by exactly that amount.
    pub fn build_with_dt(
        &mut self,
        size: (u32, u32),
        pointer: Option<ui_core::geom::Point>,
        dt: f32,
    ) -> FrameOutcome {
        self.build_step(size, pointer, Some(dt.max(0.0)))
    }

    fn build_step(
        &mut self,
        (w, h): (u32, u32),
        pointer: Option<ui_core::geom::Point>,
        step: Option<f32>,
    ) -> FrameOutcome {
        // The sample is the surface's own; the tree's own pointer came in as events and may
        // be standing on a panel of this tree, which is a different surface. A surface has a
        // pointer only while the pointer is on it, so its sample wins when it has one, then
        // the hint the host resolved across the group, and the tree's answer last — one
        // pointer, several surfaces, one hover.
        let pointer = match pointer {
            Some(p) => Some(p),
            None => self.pointer_hint.take().or(self.pointer),
        };
        // Damage is only meaningful where the frame can paint; the scroll pass below adds
        // one rect per row that moved, including the rows far past the viewport.
        self.damage
            .set_bounds(Rect::new(0.0, 0.0, w as f32, h as f32));
        // The surface size, kept for anything placed outside the tree (the context menu).
        self.last_size = (w, h);
        // Scripted time moves by exactly the step; with none, the clock reads the wall.
        match step {
            Some(dt) => self.clock.advance_by(dt as f64),
            None => {
                self.clock.tick();
            }
        }
        // Adopt the timestamp before anything reads it: `FrameInfo` and the animation
        // store must see this frame's time, not the previous frame's.
        let previous_now = self.frame_now;
        self.frame_now = self.clock.now();
        // The step the clock actually took, capped so a stalled app cannot fling the content.
        self.frame_step = (self.frame_now - previous_now).clamp(0.0, 0.05) as f32;
        // What the caller asked for when scripting time, else what the wall clock took.
        let dt = step.unwrap_or(self.frame_step);
        // A splat that finished fading is gone before anything paints it again.
        self.splats.retain(|splat| !splat.finished(self.frame_now));
        // The click's chain is derived here, so the context can answer `clicked_within`
        // for a control whose root is an ancestor of the node that was actually hit.
        Self::fill_ancestors(&self.tree, &mut self.clicked_path, self.clicked);
        Self::fill_ancestors(&self.tree, &mut self.activated_path, self.activated);
        Self::fill_ancestors(&self.tree, &mut self.dropped_path, self.dropped);
        // The drag's own route is derived from its live target, not from a one-frame fact:
        // a drag stays over a node for many frames and every one of them paints the hover.
        Self::fill_ancestors(
            &self.tree,
            &mut self.drag_path,
            self.drag_session.as_ref().and_then(|s| s.target),
        );
        // The cycle belongs to the editor, not to the offset: a caret walking with the
        // arrow keys must keep blinking. Taking the caret and changing the buffer restart
        // it, so a new editor opens lit and typing shows the caret solid as it does anywhere.
        let fact = self.caret_fact();
        let owner = fact
            .editing
            .map(|(id, _)| id)
            .or_else(|| fact.selection.map(|(id, _, _, _)| id));
        let digest = fact
            .editing
            .and_then(|(id, _)| self.input_digest(id).map(|digest| (id, digest)));
        if owner
            != self
                .caret_last
                .editing
                .map(|(id, _)| id)
                .or_else(|| self.caret_last.selection.map(|(id, _, _, _)| id))
            || digest != self.caret_digest
        {
            self.caret_blink_at = self.frame_now;
            self.caret_digest = digest;
        }
        self.caret_last = fact;
        self.caret_painted = false;
        let caret_in_text = self.caret_in_text_fact();
        let mut info = FrameInfo {
            width: w,
            height: h,
            pointer,
            frame: self.frame,
            // The previous frame's answer: input arrives between frames, not during one.
            hovered: self.hovered,
            pressed: self.pressed,
            clicked_path: self.clicked_path.clone(),
            dropped_path: self.dropped_path.clone(),
            drop_target: self.drop_target,
            drag_path: self.drag_path.clone(),
            drag_types: self
                .drag_session
                .as_ref()
                .filter(|s| s.types_decide())
                .map(|s| Rc::clone(&s.types)),
            drag_step: self.drag_step,
            press_origin: self.press_at,
            focused: self.focused,
            focus_visible: self.focus_visible,
            activated: self.activated,
            activated_path: self.activated_path.clone(),
            clicks: self.clicks,
            dismissed: self.dismissed,
            // Only the front fact is answered this frame; the rest wait for later frames.
            shortcut: self
                .shortcut_queue
                .front()
                .map(|fact| (fact.owner, fact.shortcut)),
            // Copied only for the frame that reads it: the app's parsed source lags the
            // buffer, so an edit is computed against the bytes it lands in, as the key saw them.
            shortcut_focus: self
                .shortcut_queue
                .front()
                .and_then(|fact| fact.focus.clone()),
            // An input's selection and a read-only one are never live at the same time.
            caret: self.editing.map(|(id, sel)| (id, sel.caret)),
            caret_in_text,
            selection: match (self.selection, self.editing) {
                (Some(_), _) => Rc::clone(&self.selection_spans),
                (None, Some((id, span))) => Rc::from([(id, span)]),
                (None, None) => Rc::from(Vec::new()),
            },
            preedit: self.preedit.clone(),
            dt,
            now: self.frame_now,
            hovered_path: self.hovered_path.clone(),
            pressed_path: self.pressed_path.clone(),
            drag_owner: if self.pressed.is_some() {
                self.drag_owner()
            } else {
                None
            },
            drag_started: self.drag.is_dragging(),
        };
        // One-frame facts: the frame that reads them also consumes them, so nothing else
        // may clear them — events arrive in bursts and would erase a click no frame has seen.
        self.clicked = None;
        self.clicks = 0;
        self.activated = None;
        self.dismissed = None;
        self.dropped = None;
        self.drop_target = None;
        self.drag_step = None;
        self.pointer = pointer;

        // 0. Advance animations before the view runs, so this frame reads settled values.
        self.advance_animations();

        // 1. Declaration, built under the one context that owns the engine's store.
        let (
            root_element,
            app_decl,
            decl,
            scroll_requests,
            bring_requests,
            input_edits,
            copies,
            images,
            canvases,
            close,
            pointer_steps,
            focus_requests,
            menu_requests,
            menu_update,
            shell_menu_request,
            paste_request,
            time_driven,
            pointer_read,
            modifier_read,
        ) = {
            // Collect everything the context needs before it borrows the store, or the
            // two borrows of `self` would overlap.
            let drag_owner = if info.pressed.is_some() {
                self.drag_owner()
            } else {
                None
            };
            // A middle button down is a press on the node that hears it, so that node sees
            // the pointer in its own space for as long as the button is held.
            let local = self.pointer_local(
                info.pressed.or(self.middle.map(|(node, _)| node)),
                info.pointer,
            );
            let inert = self.inert_ids();
            // Anything the cache cannot observe dropped its entries; start the frame clean.
            if self.decls_stale {
                self.decl_cache.clear();
                self.decls_stale = false;
            }
            let mut cx = ui_core::cx::Cx::new(
                &mut self.anim_store,
                info.frame,
                info.width,
                info.height,
                info.now,
                info.dt,
                self.scale,
                info.pointer,
            );
            cx.attach_measure(Rc::clone(&self.measure));
            cx.attach_math(Rc::clone(&self.math));
            cx.attach_math_edit(Rc::clone(&self.math_edit));
            cx.attach_cache(&mut self.decl_cache);
            // Last frame's layout, for a build that measures other nodes: the retained tree
            // is only read here, and the apply that writes it happens after this block.
            cx.attach_tree(&self.tree);
            cx.set_modifiers(self.modifiers);
            cx.set_reveal(self.reveal);
            // Scroll geometry for the view: content extent and offset per engine-owned
            // scroller, from the previous frame's layout and this frame's stored offsets.
            // A scroller that never saw a wheel still reports itself, offset 0.
            {
                let mut scrolls: Vec<ui_core::cx::ScrollGeom> = Vec::new();
                // Every scroller still in the tree, so the fade table forgets the ones that
                // left, and this frame's activity for the bars that fade.
                let mut live: FastSet<Id> = FastSet::default();
                let mut fading: Vec<(Id, bool, bool)> = Vec::new();
                for nid in self.tree.walk() {
                    if let Some(node) = self.tree.get(nid)
                        && node.sense.contains(ui_core::Sense::SCROLL_OWNED)
                    {
                        let offset = self
                            .scroll_offsets
                            .get(&node.id)
                            .copied()
                            .unwrap_or_default();
                        let extent = node.scroll_extent.unwrap_or(Size::ZERO);
                        let pad = node.box_model.padding;
                        let wheel = self.scrolled_since.contains(&node.id);
                        scrolls.push(ui_core::cx::ScrollGeom {
                            id: node.id,
                            extent_w: extent.w,
                            extent_h: extent.h,
                            viewport_w: (node.rect.size.w - pad.left - pad.right).max(0.0),
                            viewport_h: (node.rect.size.h - pad.top - pad.bottom).max(0.0),
                            offset_x: offset.x,
                            offset_y: offset.y,
                            active: wheel,
                        });
                        live.insert(node.id);
                        if node
                            .data
                            .params()
                            .and_then(|p| p.scrollbar)
                            .is_some_and(|bar| bar.fade)
                        {
                            fading.push((node.id, info.hovered_within(node.id), wheel));
                        }
                    }
                }
                // The fade is declared here rather than in the widget: the bar's declaration
                // carries no state, and the activity to read is the engine's.
                let now = self.clock.live();
                let mut awake = false;
                for (id, hovered, wheel) in fading {
                    let fade = self.scrollbar_fade.entry(id).or_insert_with(BarFade::new);
                    fade.advance(now, hovered, wheel);
                    awake |= fade.awake(now);
                }
                self.scrollbar_fade.retain(|id, _| live.contains(id));
                self.scrollbar_awake = awake;
                self.scrolled_since.clear();
                cx.set_scrolls(scrolls);
            }
            // Read the frame's own routing; a context has no routing until this call.
            cx.route(ui_core::cx::Routing {
                hovered: info.hovered,
                pressed: info.pressed,
                clicked_path: info.clicked_path.clone(),
                clicks: info.clicks,
                dropped_path: info.dropped_path.clone(),
                drop_target: info.drop_target,
                drag_target: info.drag_path.last().copied(),
                drag_path: info.drag_path.clone(),
                drag_types: info.drag_types.clone(),
                focused: info.focused,
                focus_visible: info.focus_visible,
                activated_path: info.activated_path.clone(),
                dismissed: info.dismissed,
                shortcut: info.shortcut,
                input: info
                    .shortcut_focus
                    .as_ref()
                    .map(|(id, _, value)| (*id, value.clone())),
                shortcut_selection: info
                    .shortcut_focus
                    .as_ref()
                    .map(|(id, selection, _)| (*id, *selection)),
                caret: info.caret,
                caret_in_text: info.caret_in_text,
                selection: Rc::clone(&info.selection),
                preedit: info.preedit.clone(),
                hovered_path: info.hovered_path().to_vec(),
                pressed_path: info.pressed_path_field().to_vec(),
                drag_owner,
                drag_started: info.drag_started,
                drag_step: info.drag_step,
                press_origin: info.press_origin,
                local,
                disabled: inert.clone(),
            });
            // Facts the app reads outside every build gate the cache as a whole: they may
            // change state that later builds read without recording it.
            if !cx.settle_outside() {
                cx.invalidate_decls();
            }
            let (root, app_decl, decl, time_driven, pointer_read, modifier_read) = timed!("decl", {
                // Two different costs: what the app builds, and what the engine makes of it.
                let root = timed!("view", self.app.view(&mut cx));
                // Asked here, before paint: the menu and the content share this frame's look.
                if let Some(style) = self.app.menu_style() {
                    self.menu_style = style;
                }
                let app_decl = timed!("root_decl", root.decl(&mut cx));
                self.app_root = Some(app_decl.id);
                let menu = menu_panel_decl(&mut cx, &mut self.menu, &self.menu_style, &self.text);
                let decl = timed!("surface", with_menu(app_decl.clone(), menu));
                (
                    root,
                    app_decl,
                    decl,
                    cx.take_time_driven(),
                    cx.take_pointer_read(),
                    cx.take_modifier_read(),
                )
            });
            (
                root,
                app_decl,
                decl,
                cx.take_scroll_requests(),
                cx.take_bring_requests(),
                cx.take_input_edits(),
                cx.take_copies(),
                cx.take_image_requests(),
                cx.take_canvas_requests(),
                cx.take_close_request(),
                cx.take_pointer_nudges(),
                cx.take_focus_requests(),
                cx.take_menu_requests(),
                cx.take_menu_update(),
                cx.take_shell_menu_request(),
                cx.take_paste_request(),
                time_driven,
                pointer_read,
                modifier_read,
            )
        };
        // Pictures the build minted, before paint: the shapes that refer to them resolve a
        // texture slot from this table, and a pin that zoomed this frame draws its new raster
        // now rather than a frame late. They name this build, so the next one drops them.
        for entry in images {
            self.images.insert(
                entry.id,
                RegisteredImage {
                    entry,
                    seen: Some(self.frame),
                },
            );
        }
        // Canvases are known before paint as well: the image fingerprint hashes their
        // version, which is what makes a canvas change damage the node that draws it.
        self.canvases = canvases;
        self.canvas_versions.clear();
        for canvas in &self.canvases {
            self.canvas_versions.insert(canvas.id.0, canvas.version);
        }
        // A control's copy button asks through the same door a shortcut uses: the platform
        // owns the clipboard, and the host drains these after the frame.
        for content in copies {
            self.clipboard
                .push(crate::clipboard::ClipboardCommand::Copy {
                    content,
                    kind: crate::clipboard::ClipboardKind::Selection,
                });
        }
        self.close |= close;
        self.pointer_steps = pointer_steps;
        // A control that read the raw pointer can change with any move of it: the loop must
        // not absorb those moves without building.
        self.pointer_read = pointer_read;
        self.modifier_read = modifier_read;
        self.time_driven = time_driven;
        // The app may move a scroller itself (a drag, a page button); its request becomes
        // the stored offset before the declaration is applied.
        for req in scroll_requests {
            // An app that moves a scroller takes it back from a coast, exactly as a wheel does.
            self.end_motion(req.id);
            self.snap_pending.remove(&req.id);
            // Clamp to what the container can reach, as `scroll_by` does: a `scroll_to` past
            // the end used to slide past it and then be yanked back by the next layout.
            let (x, y) = match self.scroll_range(req.id) {
                Some((clip, extent, viewport)) => (
                    Self::in_range(req.x, clip.x, extent.w, viewport.w),
                    Self::in_range(req.y, clip.y, extent.h, viewport.h),
                ),
                // Not a placed scroller yet: there is no range to clamp against, and the
                // first layout's reconciliation will do it.
                None => (req.x, req.y),
            };
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            if req.animate {
                self.start_settle(req.id, x, y);
            } else {
                self.scroll_offsets
                    .insert(req.id, ui_core::cx::ScrollOffset { x, y });
            }
        }

        // 2. Apply: diff against the retained tree.
        let _stats = timed!("apply", self.tree.apply(&decl));

        // The edits a control asked for during its build. A declaration names the
        // inputs, so these land after it; the frame that shows them is the next one.
        let input_edited = self.apply_input_edits(input_edits);
        // The frame answered the front fact; leftover facts each need a frame of their own.
        self.shortcut_queue.pop_front();
        if !self.shortcut_queue.is_empty() {
            self.dispatched = true;
        }

        // 2b. Re-apply engine-owned scroll offsets: the app's declaration may not reset the
        // wheel position of a `SCROLL_OWNED` container.
        timed!("scroll_pass", {
            if !self.scroll_offsets.is_empty() {
                // Only a scroller the engine is actively stretching may show an offset past its
                // end; every other one is clamped by layout, as an app-declared offset is.
                let elastic = self.elastic_scrollers();
                let grid = self.text_scale();
                for (id, offset) in self.scroll_offsets.iter() {
                    if let Some(nid) = self.tree.find_by_id(*id)
                        && let Some(params) =
                            self.tree.get_mut(nid).and_then(|n| n.data.params_mut())
                    {
                        params.scroll_x = on_text_grid(offset.x, grid);
                        params.scroll_y = on_text_grid(offset.y, grid);
                        params.scroll_elastic = elastic.contains(id);
                    }
                }
            }
        });
        // No global fallback: `compute_damage` already damages new and retired nodes by
        // their own ink. A full-screen safety net would hide exactly the bugs it covers.

        timed!("prepare", {
            // 2c. Size transitions: resolve declared fixed sizes into this frame's animated
            // values before layout, so siblings see the travelling size and reflow with it.
            self.resolve_size_transitions();

            // 2d. Presence: an entering node is painted from its declared start values, which
            // layout never sees; presence reaches 1 and the node matches its declaration again.
            self.resolve_presence();
        });

        // 3. Layout.
        let root = self.tree.root().expect("apply always yields a root");
        let constraints = Constraints::loose(Size::new(w as f32, h as f32));
        timed!(
            "layout",
            self.engine.solve(&mut self.tree, root, constraints)
        );
        // The layout just said how much content every scroller has: an offset the content no
        // longer reaches is out of range now, and nothing else brings it back.
        self.reconcile_scroll_offsets();
        if self.scroller_boxes_moved() {
            self.dispatched = true;
        }
        // Panels the host has not placed yet are held back for one frame, and this is the
        // build that asks for it: the host decides right after this step, and the next
        // frame paints each panel where it will stay.
        if self.tree.platform_panels().is_empty() {
            self.panel_presentation.clear();
        } else {
            let live: FastSet<ui_core::NodeId> = self
                .tree
                .platform_panels()
                .iter()
                .map(|panel| panel.node)
                .collect();
            self.panel_presentation.retain(|node, _| live.contains(node));
            if self
                .tree
                .platform_panels()
                .iter()
                .any(|panel| !self.panel_presentation.contains_key(&panel.node))
            {
                self.dispatched = true;
            }
        }
        // A control that opens an editor asks for the keyboard in the same build: the node
        // exists from that declaration, and only layout gives it the rect a stop is.
        for id in focus_requests {
            if self.focus_node(id) {
                // A text field that just took the keys needs its caret, as a press gives it.
                self.sync_caret();
                self.decls_stale = true;
                self.damage.add(Rect::new(0.0, 0.0, w as f32, h as f32));
                self.dispatched = true;
            }
        }
        // A build can ask for a menu without holding the loop; the last request wins. An
        // identical menu already open is left alone so a per-frame request cannot churn it.
        if let Some(request) = menu_requests.into_iter().last() {
            let same = self.menu.as_ref().is_some_and(|menu| {
                menu.owner == request.owner && menu.at == request.at && menu.items == request.items
            });
            if !same {
                self.open_menu_at(request.owner, request.at, request.items);
            }
        }
        // Rows that answered after the menu was up replace its rows where it stands, so a
        // slow platform read joins the menu the user is looking at instead of a later one.
        if let Some(update) = menu_update {
            self.replace_menu_items(update.owner, update.items);
        }
        // The platform's own menu blocks on its popup's loop, so the host runs it from its
        // turn, after this frame has presented with the engine's menu closed.
        if let Some(request) = shell_menu_request {
            self.shell_menu_request = Some(request);
        }
        // A control with no chord to press asks for the clipboard; the answer follows the
        // same route a Ctrl+V request takes.
        if paste_request {
            // The same fallback the keyboard route uses: with no caret, the app root takes it.
            self.paste_target = self.focused.or(self.app_root);
            self.clipboard
                .push(crate::clipboard::ClipboardCommand::RequestPaste {
                    kind: crate::clipboard::ClipboardKind::Selection,
                });
        }
        // A modal owns the keyboard, so focus must be inside it once it is laid out.
        let settled = self.settle_focus();
        if settled {
            info.focused = self.focused;
            info.focus_visible = self.focus_visible;
        }
        // 3a. Keep the nodes the build asked for visible. The offset is stored now and
        // reaches the tree on the next frame's scroll pass, so the frame repaints.
        let mut brought = false;
        for id in bring_requests {
            brought |= self.bring_into_view(id);
        }
        // Focus moving to a node keeps it visible, as every WinUI scroller does by default.
        if self.focused != self.focus_shown {
            self.focus_shown = self.focused;
            if let Some(id) = self.focused {
                // A text input's node is its whole buffer, so bringing it into view drags
                // an edge of the document to the viewport even when the line just pressed
                // is on screen. The caret's own line is that node's reveal, and it has to
                // run from the offset the user left, not from the one this would set.
                if self.is_text_input(id) {
                    self.caret_revealed = None;
                } else {
                    brought |= self.bring_into_view(id);
                }
            }
        }
        // The caret's own line, after focus's whole-node move: a block's rect is the whole
        // fragment, so revealing it would show an edge rather than where the caret is.
        // An edit applied this frame still draws the previous buffer, so its line has to
        // wait for the next frame's text.
        if !input_edited {
            brought |= self.reveal_caret();
        }
        if brought {
            self.decls_stale = true;
            self.damage.add(Rect::new(0.0, 0.0, w as f32, h as f32));
        }

        // 3b. Semantic changes from live controls, now that geometry is settled.
        let mut changes: Vec<(Id, ui_core::change::Change)> = Vec::new();
        let inert = self.inert_ids();
        {
            let drag_owner = if info.pressed.is_some() {
                self.drag_owner()
            } else {
                None
            };
            // A middle button down is a press on the node that hears it, so that node sees
            // the pointer in its own space for as long as the button is held.
            let local = self.pointer_local(
                info.pressed.or(self.middle.map(|(node, _)| node)),
                info.pointer,
            );
            let mut cx = ui_core::cx::Cx::new(
                &mut self.anim_store,
                info.frame,
                info.width,
                info.height,
                info.now,
                info.dt,
                self.scale,
                info.pointer,
            );
            cx.attach_measure(Rc::clone(&self.measure));
            cx.attach_math(Rc::clone(&self.math));
            cx.attach_math_edit(Rc::clone(&self.math_edit));
            cx.route(ui_core::cx::Routing {
                hovered: info.hovered,
                pressed: info.pressed,
                clicked_path: info.clicked_path.clone(),
                clicks: info.clicks,
                dropped_path: info.dropped_path.clone(),
                drop_target: info.drop_target,
                drag_target: info.drag_path.last().copied(),
                drag_path: info.drag_path.clone(),
                drag_types: info.drag_types.clone(),
                focused: info.focused,
                focus_visible: info.focus_visible,
                activated_path: info.activated_path.clone(),
                dismissed: info.dismissed,
                shortcut: info.shortcut,
                input: info
                    .shortcut_focus
                    .as_ref()
                    .map(|(id, _, value)| (*id, value.clone())),
                shortcut_selection: info
                    .shortcut_focus
                    .as_ref()
                    .map(|(id, selection, _)| (*id, *selection)),
                caret: info.caret,
                caret_in_text: info.caret_in_text,
                selection: Rc::clone(&info.selection),
                preedit: info.preedit.clone(),
                hovered_path: info.hovered_path().to_vec(),
                pressed_path: info.pressed_path_field().to_vec(),
                drag_owner,
                drag_started: info.drag_started,
                drag_step: info.drag_step,
                press_origin: info.press_origin,
                local,
                disabled: inert.clone(),
            });
            cx.set_modifiers(self.modifiers);
            let was_muted = cx.mute();
            timed!(
                "changes",
                collect_changes(root_element.as_ref(), &app_decl, &self.tree, &cx, &mut changes,)
            );
            cx.unmute(was_muted);
            // A control reports its own activation; a bare node has nobody but the engine.
            if let Some(id) = info.activated {
                let spoke = changes
                    .iter()
                    .any(|(cid, _)| info.activated_path.contains(cid));
                if !spoke {
                    changes.push((id, ui_core::change::Change::Activated));
                }
            }
            // The menu's rows are the engine's own nodes, so the app's tree cannot speak for
            // a click on one; the same pick path that answers Enter takes it here. The click
            // lands on whatever the row draws (a label's text), so its chain names the row.
            if let Some(id) = self.menu.as_ref().and_then(|menu| {
                info.clicked_path
                    .iter()
                    .rev()
                    .find(|id| menu.locate(**id).is_some())
                    .copied()
            }) {
                changes.push((id, ui_core::change::Change::Activated));
            }
            // A menu panel's surface was taken back by the platform or by a press outside
            // it; the panel is an engine node, so the change is the engine's to make too.
            if let Some(id) = info.dismissed
                && self.menu.as_ref().is_some_and(|menu| {
                    (0..=menu.depth()).any(|depth| crate::menu::Menu::panel_id(depth) == id)
                })
            {
                changes.push((id, ui_core::change::Change::Dismissed));
            }
        }
        let mut changed_any = false;
        for (id, change) in changes {
            // A scroll request names no scroller: the nearest one that owns its axis takes it.
            if let ui_core::change::Change::ScrollBy { dx, dy, owner } = change {
                let moved = match owner {
                    Some(owner) => self.scroll_owner(owner, dx as f32, dy as f32),
                    None => self.scroll_owner_by(id, dx as f32, dy as f32),
                };
                // The requesting control's own state (a chevron's enablement) follows the
                // new offset, so its declaration must be rebuilt.
                if moved {
                    self.decls_stale = true;
                }
                changed_any |= moved;
                continue;
            }
            // A focus request names its target, and only a node the frame can focus may take
            // the keyboard; the app is never told.
            if let ui_core::change::Change::Focus(target) = change {
                changed_any |= self.focus_node(target);
                continue;
            }
            // The menu's rows and panels are the engine's own nodes: a click on a row is a
            // pick, and a popup the platform took back is a dismissal, so neither reaches
            // the app under the menu's owner id.
            let menu_handled = match &change {
                ui_core::change::Change::Activated => self.menu_activated(id),
                ui_core::change::Change::Dismissed => self.menu_dismissed(id),
                _ => false,
            };
            if menu_handled {
                changed_any = true;
                continue;
            }
            changed_any |= self.app_changed(id, change);
        }
        if changed_any {
            self.damage.add(Rect::new(0.0, 0.0, w as f32, h as f32));
        }
        // These changes land after `view()` already ran, so this frame still paints the
        // state before them: a click that closes something would stay on screen until an
        // unrelated event. Ask for the frame that makes them visible.
        if changed_any {
            self.dispatched = true;
        }

        // 4. Paint into the draw list.
        self.list.clear();
        self.image_slots.clear();
        self.backdrops_now.clear();
        self.list.canvas = Rect::new(0.0, 0.0, w as f32, h as f32);
        timed!("paint", self.paint_tree(root));

        // 5. Hit test with this frame's rects.
        timed!(
            "hit",
            self.hit.rebuild(&self.tree, |nid| {
                self.tree
                    .get(nid)
                    .map(|n| Self::hit_behavior_of(n.kind, n.sense, n.disabled))
                    .unwrap_or(HitBehavior::Ignore)
            })
        );
        let hit_id = pointer
            .and_then(|p| self.hit.topmost_consumer(p))
            .and_then(|nid| self.tree.get(nid).map(|n| n.id));

        // Any change of the hovered node repaints both old and new nodes. Compare like with
        // like: `hovered` holds the owner, so comparing it against the raw hit would repaint
        // every frame whenever the pointer rests on a node that declares no hover.
        let hovered = hit_id.and_then(|nid| self.hover_owner(nid));
        if self.hovered != hovered {
            self.damage.add(Rect::new(0.0, 0.0, w as f32, h as f32));
            // Scrolling moved a different node under a resting pointer: this frame painted
            // the old hover owner, so the new one has to be built to be seen at all.
            self.dispatched = true;
        }
        self.hovered = hovered;
        self.refresh_hovered_path();
        let cursor = self.resolve_cursor();
        // The caller hands this same value to the target; remembering it is what tells a
        // later move whether its shape is news.
        self.cursor_sent = cursor;

        // 6. Resolve text once, before damage: a changed label must dirty its node even
        // when the node's geometry did not move.
        let text_scale = self.text_scale();
        timed!("text", {
            let mut text = self.text.borrow_mut();
            text.resolve_into(&self.list, text_scale, &mut self.resolved);
        });

        // 7. Damage: fingerprint the resolved primitives per owning node.
        timed!("damage", self.compute_damage());

        let damage = std::mem::take(&mut self.damage).finish();

        // Panels with a surface of their own, after the root's damage is diffed: they are
        // not in that surface, so their ink must neither claim nor lose damage there.
        timed!("panels", self.paint_panels());
        // A build's picture is answered now, so it goes with the frame: the pixels are the
        // app's to hand over again, and a list that scrolls past them owes nothing. The
        // surface's texture is evicted by the next present that does not name it.
        let frame = self.frame;
        self.images
            .retain(|_, image| image.seen.is_none_or(|seen| seen == frame));

        // Springs are integrated, so the scheduler steps them: one left to a widget that
        // stopped building it would never land, and would ask for frames for ever.
        self.anim_store.advance(self.frame_now);
        self.frame += 1;
        FrameOutcome {
            damage,
            nodes: self.tree.len(),
            hit: hit_id,
            // A live splat is an animation the app cannot see in its own store: without
            // this, a held press would stop presenting frames and the ink would freeze.
            // A coasting or bouncing scroller is the same kind of engine-owned motion.
            animating: self.anim_store.is_animating(self.clock.now())
                || !self.splats.is_empty()
                || !self.scroll_motion.is_empty()
                // An automatic scrollbar still holding or fading is engine-owned motion too.
                || self.scrollbar_awake
                // A control reading the clock asked for every frame; it never settles.
                || time_driven,
            cursor,
        }
    }

    /// Deliver one drag transition to the app as a semantic change.
    ///
    /// Gesture mechanics stay in the engine; the app only learns "how far, and onto what".
    /// The pointer translated into the pressed node's own space.
    ///
    /// Zero when no press is live: a control decides for itself whether that matters.
    fn pointer_local(
        &self,
        pressed: Option<Id>,
        pointer: Option<ui_core::geom::Point>,
    ) -> Option<ui_core::geom::Point> {
        // The gesture belongs to the node that declares the drag, not to whatever is on top:
        // the topmost node is usually the thumb, which moves with the finger it measures.
        let id = self.drag_owner().or(pressed)?;
        let p = pointer?;
        let nid = self.tree.find_by_id(id)?;
        let rect = self.tree.get(nid)?.visual_rect();
        Some(ui_core::geom::Point::new(
            p.x - rect.origin.x,
            p.y - rect.origin.y,
        ))
    }

    /// Splat ink for a press that just landed on `at`.
    ///
    /// The nearest ancestor-or-self that declared ink owns the press: the hit node is often
    /// a label, while the control that draws is its container.
    fn begin_splat(&mut self, at: ui_core::geom::Point) {
        let Some(hit) = self.pressed else { return };
        let mut cur = self.tree.find_by_id(hit);
        while let Some(nid) = cur {
            let Some(node) = self.tree.get(nid) else {
                return;
            };
            if let Some(ink) = node.data.params().and_then(|p| p.press_ink) {
                let splat = Splat {
                    node: node.id,
                    origin: at,
                    ink,
                    started: self.clock.live(),
                    released: None,
                };
                self.splats.push(splat);
                return;
            }
            cur = node.parent;
        }
    }

    /// End the live splats; they keep fading, but only the fade's clock matters now.
    fn end_splat(&mut self) {
        let now = self.clock.now();
        for splat in &mut self.splats {
            splat.released.get_or_insert(now);
        }
    }

    fn report_drag(&mut self, event: ui_core::DragEvent) -> bool {
        use ui_core::DragEvent;
        use ui_core::change::Change;
        let mut requested = false;
        let (id, change) = match event {
            // A drag start carries no value yet: the first `Moved` says where the pointer
            // is inside the control, and that is what a control maps to a value.
            DragEvent::Started {
                on, from, button, ..
            } => {
                // A new gesture starts with no movement banked: the previous drag's slice
                // must not be added to this one if both fall inside a single frame.
                self.drag_step = None;
                let node = self
                    .tree
                    .find_by_id(on)
                    .and_then(|nid| self.tree.get(nid));
                let (data, native, bounds) = match node {
                    Some(node) => (
                        node.drag.clone(),
                        node.drag_out,
                        Some(node.visual_rect()),
                    ),
                    None => (None, false, None),
                };
                match data {
                    // A native drag hands the gesture to the host: this loop only hears
                    // how it ended.
                    Some(data) if native => {
                        self.native_request = Some(NativeDrag {
                            data: (*data).clone(),
                            bounds,
                            origin: from,
                            image: None,
                        });
                        self.native_crop = None;
                        self.native_source = Some(on);
                        self.drag.reset();
                    }
                    data => {
                        let types = data
                            .as_ref()
                            .map(|data| data.types().map(str::to_owned).collect())
                            .unwrap_or_default();
                        // A gesture that carries nothing is not a data transfer; it keeps
                        // the copy-only default every such drag had.
                        let (effects, preferred) = match &data {
                            Some(data) => (data.effects, data.preferred),
                            None => (ui_core::DropEffects::COPY, ui_core::DropEffect::Copy),
                        };
                        self.drag_session = Some(DragSession {
                            source: Some(on),
                            data,
                            types: Rc::new(types),
                            effects,
                            preferred,
                            button,
                            target: None,
                        });
                        // The target hears before the source does: a highlight that reads
                        // the source's state sees it already settled.
                        requested |= self.refresh_drag_target();
                    }
                }
                (on, Change::DragStarted)
            }
            DragEvent::Moved {
                on, delta, step, ..
            } => {
                // Several moves can land between frames, and a control adds `drag_step` to
                // what it has; the frame must see the whole slice, not the last event's.
                self.drag_step = Some(match self.drag_step {
                    Some(prev) => ui_core::geom::Point::new(prev.x + step.x, prev.y + step.y),
                    None => step,
                });
                requested |= self.refresh_drag_target();
                (
                    on,
                    Change::Moved {
                        dx: delta.x as f64,
                        dy: delta.y as f64,
                    },
                )
            }
            DragEvent::Dropped {
                on,
                target,
                at,
                button,
            } => {
                self.dropped = Some(on);
                self.drop_target = target;
                let mut outcome = None;
                if let Some(session) = self.drag_session.take() {
                    let effect = session
                        .negotiated(self.modifiers)
                        .unwrap_or(session.preferred);
                    match (session.data, target) {
                        // The target takes the payload; no exit is sent, because the drop
                        // itself ends the hover.
                        (Some(data), Some(target)) => {
                            requested |= self.app_changed(
                                target,
                                Change::DroppedOn(ui_core::DropReport {
                                    data: (*data).clone(),
                                    effect,
                                    modifiers: self.modifiers,
                                    button,
                                    performer: ui_core::Performer::Target,
                                    foreign: false,
                                    pos: at,
                                }),
                            );
                            outcome = Some(ui_core::DropOutcome {
                                action: effect,
                                performed_by: ui_core::Performer::Target,
                            });
                        }
                        _ => {
                            // Released over nothing (or over another node): whatever was
                            // highlighted comes down.
                            if let Some(stale) = session.target.filter(|t| Some(*t) != target) {
                                requested |= self.app_changed(stale, Change::DragExited);
                            }
                        }
                    }
                }
                (on, Change::Dropped { target, outcome })
            }
            DragEvent::Cancelled { on } => {
                if let Some(session) = self.drag_session.take()
                    && let Some(target) = session.target
                {
                    requested |= self.app_changed(target, Change::DragExited);
                }
                (on, Change::Cancelled)
            }
        };
        requested | self.app_changed(id, change)
    }

    /// Recompute what the live drag is over and announce a change of target.
    fn refresh_drag_target(&mut self) -> bool {
        let Some(session) = self.drag_session.as_ref() else {
            return false;
        };
        let decide = session.types_decide();
        let types = Rc::clone(&session.types);
        let next = self.pointer.and_then(|pos| {
            self.drag_target_at(pos, if decide { Some(types.as_slice()) } else { None })
        });
        self.set_drag_target(next)
    }

    /// Move the live drag's target, reporting the exit and the entry it causes.
    fn set_drag_target(&mut self, next: Option<Id>) -> bool {
        let Some(session) = self.drag_session.as_mut() else {
            return false;
        };
        if session.target == next {
            return false;
        }
        let previous = std::mem::replace(&mut session.target, next);
        let types = session.types.to_vec();
        // A target change is a frame the engine needs by itself: the host publishes
        // `drop_effect()` from the next frame, and the app's highlight is drawn there.
        // The app's own answer is about the data it keeps, not about this.
        let mut requested = true;
        if let Some(previous) = previous {
            requested |= self.app_changed(previous, ui_core::change::Change::DragExited);
        }
        if let Some(next) = next {
            requested |= self.app_changed(next, ui_core::change::Change::DragEntered { types });
        }
        requested
    }

    /// Fold one phase of an operating-system drag into the loop.
    fn dnd_event(&mut self, event: ui_core::DndEvent) -> bool {
        use ui_core::DndEvent;
        use ui_core::change::Change;
        match event {
            DndEvent::Entered { pos, types, effects } => {
                self.pointer = Some(pos);
                self.drag_session = Some(DragSession {
                    source: None,
                    data: None,
                    types: Rc::new(types),
                    effects,
                    // Another application's payload does not state a preference through
                    // the protocol; copying is the safe default, and a modifier overrides.
                    preferred: ui_core::DropEffect::Copy,
                    button: ui_core::PointerButton::Primary,
                    target: None,
                });
                self.refresh_drag_target()
            }
            DndEvent::Over { pos } => {
                self.pointer = Some(pos);
                self.refresh_drag_target()
            }
            DndEvent::Left => {
                self.pointer = None;
                let target = self.drag_session.take().and_then(|s| s.target);
                target
                    .map(|target| self.app_changed(target, Change::DragExited))
                    .unwrap_or(false)
            }
            DndEvent::Dropped {
                pos,
                data,
                button,
                performer,
            } => {
                self.pointer = Some(pos);
                let types: Vec<String> = data.types().map(str::to_owned).collect();
                let effects = data.effects;
                let preferred = data.preferred;
                // Some platforms report the release without an enter; the drop still has
                // to find its target against the payload it carries. An `Entered` already
                // stated the permitted set, so only the preference — which OLE's callbacks
                // never carry — is taken from the payload then.
                match self.drag_session.as_mut() {
                    Some(session) => {
                        if session.source.is_none() {
                            session.preferred = preferred;
                            session.button = button;
                        }
                    }
                    None => {
                        self.drag_session = Some(DragSession {
                            source: None,
                            data: None,
                            types: Rc::new(types.clone()),
                            effects,
                            preferred,
                            button,
                            target: None,
                        });
                    }
                }
                let target = self.drag_target_at(pos, Some(&types));
                self.drop_target = target;
                // The session knows what the source permits; the payload's own fields only
                // describe the data. A target's own rule (same volume moves, otherwise
                // copies) reads the permitted set, and a foreign drag is the only one whose
                // preference is unknown.
                let (permitted, preferred) = self
                    .drag_session
                    .as_ref()
                    .map(|session| (session.effects, session.preferred))
                    .unwrap_or((effects, preferred));
                // A native drag of this app's own payload comes back with no session
                // source, so the source node is the other half of the test.
                let foreign = self.native_source.is_none()
                    && self
                        .drag_session
                        .as_ref()
                        .is_some_and(|session| session.source.is_none());
                let mut data = data;
                data.effects = permitted;
                let mut requested = false;
                if let Some(target) = target {
                    let effect = ui_core::negotiate_effect(permitted, preferred, self.modifiers)
                        .unwrap_or(preferred);
                    requested |= self.app_changed(
                        target,
                        Change::DroppedOn(ui_core::DropReport {
                            data,
                            effect,
                            modifiers: self.modifiers,
                            button,
                            performer,
                            foreign,
                            pos,
                        }),
                    );
                }
                if let Some(session) = self.drag_session.take()
                    && let Some(stale) = session.target.filter(|t| Some(*t) != target)
                {
                    requested |= self.app_changed(stale, Change::DragExited);
                }
                requested
            }
            DndEvent::Finished { outcome } => {
                // The operating system owned the pointer for the whole gesture, so the
                // release this loop would have seen never arrived. Settle the press here,
                // or the source stays pressed (and inked) after the drag has ended.
                self.pressed = None;
                self.press_at = None;
                self.pressed_path.clear();
                self.pan_candidate = None;
                self.end_splat();
                let Some(source) = self.native_source.take() else {
                    return false;
                };
                // A drop on one of this app's own targets is the one landing this loop did
                // see; anything else is another application's, and the source learns only
                // that the gesture was taken by something it cannot name.
                let change = match outcome {
                    None => Change::Cancelled,
                    Some(outcome) => Change::Dropped {
                        target: self.drop_target,
                        outcome: Some(outcome),
                    },
                };
                self.app_changed(source, change)
            }
        }
    }

    /// Forward a change to the app; any app state may have moved, so the cache is dropped.
    /// Run one call against the focused formula's editor, using `fallback` when the editor
    /// is borrowed elsewhere in this turn.
    fn math_edit_call<T>(
        &mut self,
        id: Id,
        call: impl FnOnce(&mut dyn ui_core::MathEdit, Id) -> T,
        fallback: T,
    ) -> T {
        match self.math_edit.try_borrow_mut() {
            Ok(mut edit) => call(&mut *edit, id),
            Err(_) => fallback,
        }
    }

    /// The math editor that holds the keyboard, when the focused node is one.
    fn math_edit_id(&self) -> Option<Id> {
        let id = self.focused?;
        let mut edit = self.math_edit.try_borrow_mut().ok()?;
        edit.is_editing(id).then_some(id)
    }

    /// Tell the app the editor's new source, and demand a frame for the changed display
    /// list. The engine owns no copy of a formula's bytes: the editor is the buffer.
    fn math_edit_reported(&mut self, id: Id) -> bool {
        let (source, owner) = match self.math_edit.try_borrow_mut() {
            Ok(mut edit) => (edit.source(id), edit.owner(id)),
            Err(_) => return false,
        };
        let Some(source) = source else {
            return false;
        };
        // A formula inside a document rewrites its own bytes in that document's buffer, so
        // the app keeps one string instead of learning every block's range. A range that no
        // longer fits the buffer means the document moved under the editor; the app's text
        // wins and the next attach resets the model from it.
        if let Some((input, start, end)) = owner {
            let Some(text) = self.text_input_value(input) else {
                return false;
            };
            let mut text = text.to_string();
            if start > end
                || !text.is_char_boundary(start)
                || !text.is_char_boundary(end)
                || end > text.len()
            {
                return false;
            }
            text.replace_range(start..end, &source);
            // The input's caret follows the formula's new end, so the reading position and
            // the structural caret name the same place.
            if self.editing.is_some_and(|(owner, _)| owner == input) {
                let caret = ui_core::text_input::clamp(&text, start + source.len());
                self.editing = Some((input, ui_core::TextSelection::collapsed(caret)));
            }
            self.app_changed(input, ui_core::change::Change::Text(text));
            return true;
        }
        self.app_changed(id, ui_core::change::Change::Text(source));
        true
    }

    /// Route a key, committed text or composition to the focused math editor. `None` means
    /// the event is not the editor's and the rest of the dispatch should see it.
    fn math_edit_event(&mut self, event: &ui_core::Event) -> Option<bool> {
        let id = self.math_edit_id()?;
        match event {
            ui_core::Event::KeyPressed { key } => {
                if matches!(key, ui_core::Key::Tab) {
                    return None;
                }
                let edit_key = math_edit_key(*key)?;
                let changed = match self.math_edit.try_borrow_mut() {
                    Ok(mut edit) => edit.key(id, edit_key, self.modifiers.shift),
                    Err(_) => return None,
                };
                if !changed {
                    // Escape has nothing to cancel; focus traversal and dismissal still
                    // belong to the engine.
                    return (!matches!(edit_key, ui_core::EditKey::Escape)).then_some(false);
                }
                Some(self.math_edit_reported(id))
            }
            ui_core::Event::Text(text) => {
                let mut changed = false;
                for character in text.chars() {
                    let step = match self.math_edit.try_borrow_mut() {
                        Ok(mut edit) => edit.key(
                            id,
                            ui_core::EditKey::Character(character),
                            self.modifiers.shift,
                        ),
                        Err(_) => return None,
                    };
                    changed |= step;
                }
                Some(if changed {
                    self.math_edit_reported(id)
                } else {
                    true
                })
            }
            ui_core::Event::Preedit(preedit) => {
                let changed = match self.math_edit.try_borrow_mut() {
                    Ok(mut edit) => match preedit {
                        Some(preedit) if !preedit.is_empty() => edit.set_preedit(id, &preedit.text),
                        _ => edit.cancel_preedit(id),
                    },
                    Err(_) => return None,
                };
                Some(if changed {
                    self.math_edit_reported(id)
                } else {
                    true
                })
            }
            _ => None,
        }
    }

    /// Whether `id` draws a formula rather than text.
    fn is_math_node(&self, id: Id) -> bool {
        self.tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))
            .is_some_and(|node| matches!(node.data, ui_core::NodeData::Math { .. }))
    }

    /// The surface point in `id`'s formula space: `x` from the box's left edge, `y` from
    /// the baseline, which sits one ascent below that edge.
    fn math_space_at(&self, id: Id, p: ui_core::geom::Point) -> Option<(f32, f32)> {
        let nid = self.tree.find_by_id(id)?;
        let node = self.tree.get(nid)?;
        let ui_core::NodeData::Math { formula, .. } = &node.data else {
            return None;
        };
        let rect = node.visual_rect();
        Some((p.x - rect.origin.x, p.y - rect.origin.y - formula.ascent))
    }

    /// The formula's own byte offset nearest a point in its space.
    fn math_offset_at(&self, id: Id, x: f32, y: f32) -> Option<usize> {
        let mut edit = self.math_edit.try_borrow_mut().ok()?;
        edit.offset_at(id, x, y)
    }

    /// The structural caret of a formula as a surface rectangle, for the input method,
    /// which places its composition window at the caret of whatever holds the keyboard.
    fn math_caret_rect(&self, id: Id) -> Option<ui_core::geom::Rect> {
        let caret = {
            let mut edit = self.math_edit.try_borrow_mut().ok()?;
            edit.caret(id)?
        };
        let nid = self.tree.find_by_id(id)?;
        let node = self.tree.get(nid)?;
        let ui_core::NodeData::Math { formula, .. } = &node.data else {
            return None;
        };
        let rect = node.visual_rect();
        Some(ui_core::geom::Rect::new(
            rect.origin.x + caret.x,
            rect.origin.y + formula.ascent + caret.top,
            0.0,
            (caret.bottom - caret.top).max(1.0),
        ))
    }

    /// A press over an edited formula places the caret at the pressed point, in the
    /// formula's own coordinates.
    fn math_edit_place_caret(&mut self, id: Id, pos: ui_core::geom::Point) -> bool {
        let Some((x, y)) = self.math_space_at(id, pos) else {
            return false;
        };
        let extend = self.modifiers.shift;
        match self.math_edit.try_borrow_mut() {
            Ok(mut edit) => edit.click(id, x, y, extend),
            Err(_) => false,
        }
    }

    /// The formula the document's selection sits inside, with its byte range there.
    ///
    /// Only a selection wholly inside the formula is the formula's: the bytes outside it
    /// have no structural meaning, so a selection reaching past them belongs to the
    /// document, which copies and deletes them as the source text they are.
    fn formula_selection(&self) -> Option<(Id, Id, usize, usize)> {
        let (owner, selection) = self.editing?;
        let range = selection.range();
        let mut edit = self.math_edit.try_borrow_mut().ok()?;
        let id = edit.formula_at(owner, range.end)?;
        let (input, start, end) = edit.owner(id)?;
        (input == owner && start <= range.start && range.end <= end)
            .then_some((owner, id, start, end))
    }

    /// Write a structural edit back into the document that embeds the formula.
    ///
    /// The document owns the bytes and the history: the editor exports its source, the
    /// engine splices that source over the formula's range as one text edit, and the caret
    /// becomes the editor's own selection. An edit that changes no bytes — an arrow, or a
    /// composition the export leaves out — only moves the document's caret.
    fn math_commit_structure(
        &mut self,
        owner: Id,
        id: Id,
        start: usize,
        end: usize,
        typing: bool,
    ) -> bool {
        let (source, offsets) = match self.math_edit.try_borrow_mut() {
            Ok(mut edit) => (edit.source(id), edit.selection_offsets(id)),
            Err(_) => return false,
        };
        let Some(source) = source else {
            return false;
        };
        let Some(text) = self.text_input_value(owner) else {
            return false;
        };
        let mut text = text.to_string();
        if start > end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return false;
        }
        let (anchor, caret) = offsets.unwrap_or((source.len(), source.len()));
        let select = |text: &str| {
            ui_core::TextSelection::new(
                ui_core::text_input::clamp(text, start + anchor),
                ui_core::text_input::clamp(text, start + caret),
            )
        };
        if text[start..end] != source {
            text.replace_range(start..end, &source);
            let selection = select(&text);
            return self.apply_edit(owner, text, selection, typing);
        }
        let selection = select(&text);
        let moved = self.editing.is_none_or(|(_, held)| held != selection);
        if moved {
            self.end_typing_run(owner);
            self.editing = Some((owner, selection));
        }
        moved
    }

    /// Route a key, text, paste or composition to the formula whose bytes hold the caret.
    ///
    /// The document's selection stays the single caret and its history the single history,
    /// so a formula never keeps a second buffer the app cannot see. `None` when the event
    /// is not the formula's and the rest of the dispatch should have it.
    fn math_structure_event(&mut self, event: &ui_core::Event) -> Option<bool> {
        if !matches!(
            event,
            ui_core::Event::KeyPressed { .. }
                | ui_core::Event::Text(_)
                | ui_core::Event::Paste(_)
                | ui_core::Event::Preedit(_)
        ) {
            return None;
        }
        let (owner, id, start, end) = self.formula_selection()?;
        let shift = self.modifiers.shift;
        match event {
            ui_core::Event::KeyPressed { key } => {
                if matches!(key, ui_core::Key::Tab) {
                    return None;
                }
                let edit_key = math_edit_key(*key)?;
                let changed =
                    self.math_edit_call(id, |edit, id| edit.key(id, edit_key, shift), false);
                if !changed {
                    // Nothing in the structure answers: the key belongs to the document.
                    // At a formula's first or last stop a Backspace or Delete removes the
                    // byte beside its range, and an arrow moves the caret out of it, as
                    // for any other view of the buffer.
                    return None;
                }
                Some(self.math_commit_structure(owner, id, start, end, false))
            }
            ui_core::Event::Text(text) => {
                let mut changed = false;
                for character in text.chars() {
                    changed |= self.math_edit_call(
                        id,
                        |edit, id| edit.key(id, ui_core::EditKey::Character(character), shift),
                        false,
                    );
                }
                if !changed {
                    return Some(true);
                }
                // Only a lone cluster typed at a caret continues a run.
                let typing = ui_core::grapheme::next(text, 0) == text.len();
                Some(self.math_commit_structure(owner, id, start, end, typing))
            }
            ui_core::Event::Paste(content) => {
                let Some(text) = content.text() else {
                    // A file list or a picture is not an edit to the formula.
                    return Some(true);
                };
                let changed = self.math_edit_call(id, |edit, id| edit.insert(id, text), false);
                if !changed {
                    return Some(true);
                }
                Some(self.math_commit_structure(owner, id, start, end, false))
            }
            ui_core::Event::Preedit(preedit) => {
                let changed = self.math_edit_call(
                    id,
                    |edit, id| match preedit {
                        Some(preedit) if !preedit.is_empty() => edit.set_preedit(id, &preedit.text),
                        _ => edit.cancel_preedit(id),
                    },
                    false,
                );
                if !changed {
                    return Some(true);
                }
                // A composition lives in the editor's model, not the document: its export
                // is unchanged until the commit, so this only repaints.
                self.math_commit_structure(owner, id, start, end, false);
                Some(true)
            }
            _ => None,
        }
    }

    fn app_changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
        self.decls_stale = true;
        self.app.changed(id, change)
    }

    /// Nearest declared shape along the hovered path wins, so a child's `Arrow` stops a
    /// parent's `Grab`. Definite: no pointer, or nothing declared, means `Arrow`.
    fn resolve_cursor(&self) -> ui_core::CursorShape {
        self.pointer
            .map_or(ui_core::CursorShape::Arrow, |p| self.cursor_shape_at(p))
    }

    /// The shape the pointer shows at `p`, resolved against the last laid-out frame.
    ///
    /// Takes the point rather than reading the live one: a move has to know what shape it
    /// is moving to before it is stored, and the answer is a pixel-independent effect.
    pub fn cursor_shape_at(&self, p: ui_core::geom::Point) -> ui_core::CursorShape {
        let mut cur = self.hit.topmost_consumer(p);
        while let Some(nid) = cur {
            let Some(node) = self.tree.get(nid) else {
                break;
            };
            if let Some(shape) = node.cursor {
                return shape;
            }
            // Text the engine made selectable shows the I-beam without the app declaring it.
            if Self::is_selectable_text(node) {
                return ui_core::CursorShape::Text;
            }
            cur = node.parent;
        }
        ui_core::CursorShape::Arrow
    }

    /// No damage special case: changed values change primitives, which are fingerprinted.
    /// Adopt this frame's timestamp. The clock was already advanced by the frame entry
    /// point; reading it here keeps the whole frame on one value.
    fn advance_animations(&mut self) {
        self.frame_now = self.clock.now();
        // Motion is tallied per frame, so the report can tell "moving" from "abandoned".
        self.anim_store.begin_frame();
        // Scroll physics runs before the view, so this frame reads the moved offset.
        let (now, dt) = (self.frame_now, self.frame_step);
        self.sample_pan_velocity(dt);
        self.snap_idle();
        timed!("motion", self.advance_scroll_motion(now, dt));
    }

    /// The nearest node that accepts text, walking up from `id`.
    fn text_input_owner(&self, id: Id) -> Option<Id> {
        let mut cur = self.tree.find_by_id(id);
        while let Some(nid) = cur {
            let node = self.tree.get(nid)?;
            if node.data.params().is_some_and(|p| p.text_input.is_some()) {
                return Some(node.id);
            }
            cur = node.parent;
        }
        None
    }

    /// The buffer a text input currently holds, as last declared by the app.
    fn text_input_value(&self, id: Id) -> Option<std::sync::Arc<str>> {
        let nid = self.tree.find_by_id(id)?;
        self.tree.get(nid)?.data.params()?.text_input.clone()
    }

    /// What the input declared it is for; a secret buffer overrides it at the protocol edge.
    fn input_purpose(&self, id: Id) -> ui_core::ime::ContentPurpose {
        let Some(nid) = self.tree.find_by_id(id) else {
            return ui_core::ime::ContentPurpose::Normal;
        };
        self.tree
            .get(nid)
            .and_then(|node| node.data.params())
            .map_or(ui_core::ime::ContentPurpose::Normal, |params| {
                params.text_input_purpose
            })
    }

    /// Whether the input declared its buffer secret, so its bytes leave by no engine path.
    fn hidden_input(&self, id: Id) -> bool {
        let Some(nid) = self.tree.find_by_id(id) else {
            return false;
        };
        self.tree
            .get(nid)
            .and_then(|node| node.data.params())
            .is_some_and(|params| params.text_input_hidden)
    }

    /// Whether the live press is the input's own, rather than a control drawn inside it.
    ///
    /// A card's copy button is a descendant of the input that owns the caret; its click must
    /// not also drop the caret into the code it sits over.
    fn press_belongs_to_input(&self, owner: Id) -> bool {
        let mut current = self.pressed.and_then(|id| self.tree.find_by_id(id));
        while let Some(nid) = current {
            let Some(node) = self.tree.get(nid) else {
                return false;
            };
            if node.id == owner {
                return true;
            }
            if node.sense.contains(ui_core::Sense::CLICK) {
                return false;
            }
            current = node.parent;
        }
        false
    }

    /// The focused input's selection and buffer as they stand now, for a chord that asks for
    /// an edit: the frame that answers it must see the state the key was pressed in.
    fn focus_snapshot(&self) -> Option<(Id, ui_core::TextSelection, String)> {
        let (owner, selection) = self.editing?;
        let value = self.text_input_value(owner)?;
        Some((owner, selection, value.to_string()))
    }

    /// Append one fired chord, keeping the order the keys arrived in.
    fn queue_shortcut(&mut self, owner: Id, shortcut: ui_core::ShortcutId) {
        self.shortcut_queue.push_back(ShortcutFact {
            owner,
            shortcut,
            focus: self.focus_snapshot(),
        });
    }

    /// Whether a plain Enter writes a line break into `id`'s buffer.
    fn input_enter_inserts_break(&self, id: Id) -> bool {
        self.tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))
            .and_then(|n| n.data.params())
            .is_some_and(|p| p.text_input.is_some() && p.text_input_enter_newline)
    }

    /// Whether `id`'s input declared itself multiline; an absent input is not.
    fn input_is_multiline(&self, id: Id) -> bool {
        self.tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))
            .and_then(|n| n.data.params())
            .is_some_and(|p| p.text_input.is_some() && p.text_input_multiline)
    }

    /// Write an edit back into the tree so the next event in the same batch sees it.
    ///
    /// The app is still the owner: its next declaration overwrites this optimistic value.
    fn set_input_value(&mut self, owner: Id, value: &str) {
        if let Some(nid) = self.tree.find_by_id(owner)
            && let Some(params) = self.tree.get_mut(nid).and_then(|n| n.data.params_mut())
        {
            params.text_input = Some(std::sync::Arc::from(value));
        }
    }

    /// Replace `range` of the editing input, keeping every block value whole.
    ///
    /// Every member of a group is ordinary text, so nothing is refused or moved: the edit
    /// lands at the caret. When it changes the member a block's source spells out, the
    /// group's other members are rewritten to the same value inside this one edit, so it
    /// reads alike on every line and the change is one undo step.
    fn edit_structural(
        &mut self,
        owner: Id,
        value: &str,
        range: std::ops::Range<usize>,
        text: &str,
        typing: bool,
    ) -> Option<bool> {
        let structures = if self.preedit.is_some() {
            Vec::new()
        } else {
            self.input_structures(owner)
        };
        let (next, caret) =
            ui_core::text_input::edit_structural(value, range.clone(), text, &structures)
                .unwrap_or_else(|| {
                    let (cut, start) = ui_core::text_input::delete_range(value, range);
                    ui_core::text_input::insert(&cut, start, text)
                });
        Some(self.apply_edit(
            owner,
            next,
            ui_core::TextSelection::collapsed(caret),
            typing,
        ))
    }

    /// Write an engine edit into the tree, record its history, and report the new buffer.
    ///
    /// Every path that changes a buffer goes through here, so no edit escapes the undo
    /// log. `typing` merges this insertion into an open run instead of opening a group.
    fn apply_edit(
        &mut self,
        owner: Id,
        value: String,
        selection: ui_core::TextSelection,
        typing: bool,
    ) -> bool {
        let Some(before) = self.text_input_value(owner) else {
            return false;
        };
        let before_selection = self
            .editing
            .filter(|(id, _)| *id == owner)
            .map_or(selection, |(_, sel)| sel);
        // An edit that leaves the bytes alone is not a group; an empty paste or a
        // Backspace at the buffer's start must not leave an undo step that does nothing.
        if before.as_ref() != value {
            self.record_edit(owner, &before, before_selection, typing);
        }
        self.editing = Some((owner, selection));
        // An edit rewrites the bytes a composition's offsets name.
        self.preedit = None;
        self.reveal = None;
        self.set_input_value(owner, &value);
        self.app_changed(owner, ui_core::change::Change::Text(value))
    }

    /// Apply the edits a build asked for to the inputs it declared.
    ///
    /// Only the focused input takes one: a request names an owner, and a control that lost
    /// focus between the request and this frame must not take the keyboard back. Each edit
    /// goes through `apply_edit`, so it joins the history and reaches the app exactly as a
    /// keystroke would.
    ///
    /// Returns whether an edit landed: the tree still draws the buffer from before it, so
    /// the caret cannot be measured against the text the next frame will show.
    fn apply_input_edits(&mut self, edits: Vec<ui_core::cx::InputEdit>) -> bool {
        let mut applied = false;
        for edit in edits {
            let active = self.focused.and_then(|id| self.text_input_owner(id));
            if !edit.detached && active != Some(edit.owner) {
                continue;
            }
            let Some(value) = self.text_input_value(edit.owner) else {
                continue;
            };
            let value = &*value;
            let start = ui_core::text_input::clamp(value, edit.range.start.min(value.len()));
            let end = ui_core::text_input::clamp(value, edit.range.end.min(value.len()));
            let (start, end) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let next = format!("{}{}{}", &value[..start], edit.text, &value[end..]);
            let live = self.editing.filter(|(id, _)| *id == edit.owner);
            let selection = if edit.detached {
                // Nobody asked for this caret to move: it follows the bytes it was on.
                match live {
                    Some((_, held)) => ui_core::TextSelection::new(
                        rebase_offset(held.anchor, start, end, edit.text.len()),
                        rebase_offset(held.caret, start, end, edit.text.len()),
                    ),
                    None => ui_core::TextSelection::collapsed(start),
                }
            } else {
                match edit.selection {
                    Some((anchor, caret)) => ui_core::TextSelection::new(
                        ui_core::text_input::clamp(&next, anchor.min(next.len())),
                        ui_core::text_input::clamp(&next, caret.min(next.len())),
                    ),
                    None => ui_core::TextSelection::collapsed(ui_core::text_input::clamp(
                        &next, edit.caret,
                    )),
                }
            };
            let changed = next != value;
            let spoke = if edit.detached && live.is_none() {
                // An input no caret is in: the bytes and one undo step, nothing else.
                if changed {
                    self.record_edit(edit.owner, &value, selection, false);
                    self.set_input_value(edit.owner, &next);
                }
                self.app_changed(edit.owner, ui_core::change::Change::Text(next))
            } else {
                self.apply_edit(edit.owner, next, selection, false)
            };
            // A later fact was snapshotted before this edit landed; replay it into that
            // snapshot so its frame answers as a second key pressed right after the first.
            self.rebase_shortcut_facts(edit.owner, start, end, &edit.text);
            // The build that asked has already painted the text before this edit.
            if spoke || changed {
                self.dispatched = true;
                applied = true;
            }
        }
        applied
    }

    /// Replay one applied edit into every queued snapshot of the same input.
    fn rebase_shortcut_facts(&mut self, owner: Id, start: usize, end: usize, text: &str) {
        for fact in &mut self.shortcut_queue {
            let Some((focus_owner, selection, buffer)) = fact.focus.as_mut() else {
                continue;
            };
            if *focus_owner != owner {
                continue;
            }
            let start = ui_core::text_input::clamp(buffer, start.min(buffer.len()));
            let end = ui_core::text_input::clamp(buffer, end.min(buffer.len()));
            let (start, end) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            buffer.replace_range(start..end, text);
            selection.anchor = rebase_offset(selection.anchor, start, end, text.len());
            selection.caret = rebase_offset(selection.caret, start, end, text.len());
        }
    }

    /// Push the state a new group returns to, and close a run this edit interrupts.
    fn record_edit(
        &mut self,
        owner: Id,
        before: &str,
        selection: ui_core::TextSelection,
        typing: bool,
    ) {
        // A burst is one group, but a typist who stops for a moment has finished a thought:
        // undo must take back that burst, not everything typed since the field was touched.
        let now = self.clock.live();
        let history = self.histories.entry(owner).or_default();
        history.redo.clear();
        let same_run = typing && history.typing && now - history.typed_at <= TYPING_PAUSE;
        history.typed_at = now;
        if same_run {
            return;
        }
        history.undo.push(InputSnapshot {
            value: before.to_string(),
            selection,
        });
        if history.undo.len() > HISTORY_LIMIT {
            history.undo.remove(0);
        }
        history.typing = typing;
    }

    /// End the open typing run, so the next keystroke opens a new group.
    ///
    /// Any caret or selection motion ends it, including a step that lands where it started.
    fn end_typing_run(&mut self, owner: Id) {
        if let Some(history) = self.histories.get_mut(&owner) {
            history.typing = false;
        }
    }

    /// Undo or redo one group of `owner`'s history.
    ///
    /// `None` when `owner` holds no buffer, so the chord falls through to the app; an empty
    /// stack answers `false`, because the key did apply to this input and changed nothing.
    fn step_history(&mut self, owner: Id, redo: bool) -> Option<(bool, bool)> {
        let value = self.text_input_value(owner)?;
        let selection = self.editing.filter(|(id, _)| *id == owner).map_or_else(
            || ui_core::TextSelection::collapsed(value.len()),
            |(_, sel)| sel,
        );
        let Some(history) = self.histories.get_mut(&owner) else {
            return Some((false, false));
        };
        let (from, to) = if redo {
            (&mut history.redo, &mut history.undo)
        } else {
            (&mut history.undo, &mut history.redo)
        };
        let Some(snapshot) = from.pop() else {
            return Some((false, false));
        };
        to.push(InputSnapshot {
            value: value.to_string(),
            selection,
        });
        if to.len() > HISTORY_LIMIT {
            to.remove(0);
        }
        history.typing = false;
        let restored = snapshot.value;
        let restored_selection = ui_core::TextSelection::new(
            ui_core::text_input::clamp(&restored, snapshot.selection.anchor),
            ui_core::text_input::clamp(&restored, snapshot.selection.caret),
        );
        self.editing = Some((owner, restored_selection));
        // The restored bytes are not the ones a composition's offsets named.
        self.preedit = None;
        self.reveal = None;
        self.set_input_value(owner, &restored);
        Some((
            self.app_changed(owner, ui_core::change::Change::Text(restored)),
            true,
        ))
    }

    /// The selectable text node `id` is, if it is one.
    fn selectable_text(&self, id: Id) -> Option<Id> {
        let node = self
            .tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))?;
        Self::is_selectable_text(node).then_some(node.id)
    }

    /// Selectable text is the engine's own affordance, so the pointer shape follows from it.
    fn is_selectable_text(node: &ui_core::tree::Node) -> bool {
        matches!(node.data, ui_core::NodeData::Text { .. })
            && node.sense.contains(ui_core::Sense::SELECT)
    }

    /// The text primitive a node paints, sized exactly as the paint path sizes it.
    fn text_prim_of(&self, id: Id) -> Option<ui_core::draw::TextPrim> {
        let node = self
            .tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))?;
        let ui_core::NodeData::Text { block, ink, .. } = &node.data else {
            return None;
        };
        Some(ui_core::draw::TextPrim {
            block: block.clone(),
            origin: text_origin(node),
            max_width: node.wrap_width,
            layout: node.inline.clone(),
            height: text_height(node),
            ink: *ink,
            caret: None,
            selection: None,
            composing: None,
            zoom: None,
            shape_scale: 1.0,
        })
    }

    /// The text node that draws an input's buffer: the one whose flattened text is the
    /// buffer, with the preedit spliced in at the caret when a composition is live.
    fn input_text_node(
        &self,
        owner: Id,
        value: &str,
        preedit: Option<&ui_core::Preedit>,
        caret: usize,
    ) -> Option<Id> {
        let composed = composed_input(value, preedit, caret);
        let mut stack = vec![self.tree.find_by_id(owner)?];
        // Document order: the buffer is declared before the clear glyph, and an empty
        // buffer matches both, so a reversed walk would answer with the glyph.
        while let Some(nid) = stack.pop() {
            let Some(node) = self.tree.get(nid) else {
                continue;
            };
            if let ui_core::NodeData::Text { block, .. } = &node.data {
                let (flat, _) = block.flatten();
                if flat == value || flat == composed {
                    return Some(node.id);
                }
            }
            let kids = self.tree.children_of(Some(nid));
            stack.extend(kids.iter().rev());
        }
        None
    }

    /// The text input a formula's source occupies, with its byte range there.
    fn formula_range(&self, id: Id) -> Option<(Id, usize, usize)> {
        let mut edit = self.math_edit.try_borrow_mut().ok()?;
        edit.owner(id)
    }

    /// The blocks a multi-block input draws, in buffer order.
    ///
    /// `None` for an input that is one text node: a text field declares no views, and
    /// every path below keeps its old answer for it. A view names its offset in
    /// `TextBlock::input_base`; the walk is the same document order the widget used.
    fn input_views(&self, owner: Id) -> Option<Vec<InputView>> {
        let mut stack = vec![self.tree.find_by_id(owner)?];
        let mut views = Vec::new();
        // Document order: the buffer is declared before any later sibling, and resolving
        // ties by source keeps the caret's view the one the user sees there.
        while let Some(nid) = stack.pop() {
            let Some(node) = self.tree.get(nid) else {
                continue;
            };
            if let ui_core::NodeData::Text { block, .. } = &node.data
                && let Some(base) = block.input_base
            {
                views.push(InputView {
                    node: node.id,
                    base,
                    len: block.flatten().0.len(),
                });
            }
            // A formula draws a run of the buffer too, so the caret enters it like text and
            // the bytes it covers are never a gap between views.
            if let ui_core::NodeData::Math { .. } = &node.data
                && let Some((input, start, end)) = self.formula_range(node.id)
                && input == owner
                && start <= end
            {
                views.push(InputView {
                    node: node.id,
                    base: start,
                    len: end - start,
                });
            }
            let kids = self.tree.children_of(Some(nid));
            stack.extend(kids.iter().rev());
        }
        if views.is_empty() {
            return None;
        }
        views.sort_by_key(|view| view.base);
        Some(views)
    }

    /// The view a point falls on: the one covering its row, else the nearest one.
    ///
    /// A point in the gap between blocks has no width of its own, so vertical distance
    /// decides first and horizontal distance only breaks a tie.
    fn view_at_point(&self, views: &[InputView], p: ui_core::geom::Point) -> Option<InputView> {
        let rect_of = |view: &InputView| {
            self.tree
                .find_by_id(view.node)
                .and_then(|nid| self.tree.get(nid))
                .map(|node| node.visual_rect())
        };
        let mut best: Option<(f32, InputView)> = None;
        for view in views {
            let Some(rect) = rect_of(view) else {
                continue;
            };
            let dy = if p.y < rect.origin.y {
                rect.origin.y - p.y
            } else if p.y > rect.origin.y + rect.size.h {
                p.y - (rect.origin.y + rect.size.h)
            } else {
                0.0
            };
            let dx = if p.x < rect.origin.x {
                rect.origin.x - p.x
            } else if p.x > rect.origin.x + rect.size.w {
                p.x - (rect.origin.x + rect.size.w)
            } else {
                0.0
            };
            let distance = dy * 1_000.0 + dx;
            // A tie goes to the later view, as it does for a caret standing on a boundary:
            // a byte-less view placed at the end of the one before it — a fence the block
            // does not draw — is then reachable from the gap it sits in.
            if best.is_none_or(|(nearest, _)| distance <= nearest) {
                best = Some((distance, *view));
            }
        }
        best.map(|(_, view)| view)
    }

    /// What the compositor needs to place the candidate window, from this frame's layout.
    pub fn ime_state(&mut self) -> crate::present::ImeState {
        let Some((owner, sel)) = self.editing else {
            return crate::present::ImeState::default();
        };
        let Some(value) = self.text_input_value(owner) else {
            return crate::present::ImeState::default();
        };
        let preedit = self
            .preedit
            .clone()
            .filter(|(id, _)| *id == owner)
            .map(|(_, p)| p);
        let at = sel.caret + preedit.as_ref().map_or(0, |p| p.cursor);
        let cursor = match self.input_views(owner) {
            Some(views) => view_of(&views, sel.caret).and_then(|view| {
                // A formula's caret is its structure's, so the composition sits there.
                if self.is_math_node(view.node) {
                    return self.math_caret_rect(view.node);
                }
                let prim = self.text_prim_of(view.node)?;
                self.text
                    .borrow_mut()
                    .caret_rect(&prim, at - view.base, self.scale)
            }),
            None => self
                .input_text_node(owner, &value, preedit.as_ref(), sel.caret)
                .and_then(|text| self.text_prim_of(text))
                .and_then(|prim| self.text.borrow_mut().caret_rect(&prim, at, self.scale)),
        };
        let hidden = self.hidden_input(owner);
        crate::present::ImeState {
            // No rect from this frame means no enable: a stale rectangle is the bug §6 names.
            enabled: cursor.is_some(),
            cursor,
            hidden,
            purpose: self.input_purpose(owner),
            // A window, never the file: one request carries the surrounding text, and a
            // document past the protocol's 4000 bytes aborts the Wayland connection. A
            // secret buffer is not sent at all: an input method may keep what it is given.
            surrounding: (!hidden)
                .then(|| crate::present::surrounding_window(&value, sel.caret, sel.anchor)),
        }
    }

    /// The caret's x inside the text node drawing the buffer, in that node's layout space.
    ///
    /// Measured from a zero origin on purpose: a node that scrolls by this value must not have
    /// its own offset folded in, or each frame would outbid the last.
    fn caret_in_text_fact(&mut self) -> Option<(Id, f32)> {
        let (owner, sel) = self.editing?;
        let value = self.text_input_value(owner)?;
        let preedit = self
            .preedit
            .clone()
            .filter(|(id, _)| *id == owner)
            .map(|(_, p)| p);
        let at = sel.caret + preedit.as_ref().map_or(0, |p| p.cursor);
        let mut prim;
        if let Some(views) = self.input_views(owner) {
            // The caret scrolls its own block into view; another block's x means nothing here.
            let view = view_of(&views, sel.caret)?;
            let mut prim = self.text_prim_of(view.node)?;
            prim.origin = ui_core::geom::Point::new(0.0, 0.0);
            let caret = self
                .text
                .borrow_mut()
                .caret_rect(&prim, at - view.base, self.scale)?;
            return Some((view.node, caret.origin.x));
        }
        let text = match self.input_text_node(owner, &value, preedit.as_ref(), sel.caret) {
            Some(text) => {
                self.input_text_cache = Some((owner, text));
                prim = self.text_prim_of(text)?;
                text
            }
            None => {
                // A keystroke has changed the buffer while the tree still holds the old text:
                // measure the new buffer in the style its node used last frame.
                let (_, text) = *self
                    .input_text_cache
                    .as_ref()
                    .filter(|(o, _)| *o == owner)?;
                prim = self.text_prim_of(text)?;
                let style = prim.block.runs.first().map(|r| r.style).unwrap_or_default();
                let composed = composed_input(&value, preedit.as_ref(), sel.caret);
                let block = std::sync::Arc::make_mut(&mut prim.block);
                block.runs = vec![ui_core::text::TextRun::new(composed, style)];
                block.atoms.clear();
                prim.layout = None;
                text
            }
        };
        prim.origin = ui_core::geom::Point::new(0.0, 0.0);
        let caret = self.text.borrow_mut().caret_rect(&prim, at, self.scale)?;
        Some((text, caret.origin.x))
    }

    /// The byte offset in `id`'s text nearest to a surface point.
    fn text_index_at(&mut self, id: Id, p: ui_core::geom::Point) -> Option<usize> {
        let prim = self.text_prim_of(id)?;
        Some(self.text.borrow_mut().index_at(&prim, p, self.scale))
    }

    /// Whether a text node draws no glyph at all: every run unpainted, or no run at all.
    fn text_draws_nothing(&self, id: Id) -> bool {
        let Some(node) = self.tree.find_by_id(id).and_then(|nid| self.tree.get(nid)) else {
            return false;
        };
        let ui_core::NodeData::Text { block, .. } = &node.data else {
            return false;
        };
        block.runs.iter().all(|run| !run.visibility.painted())
    }

    /// Flattened text of a node, for word selection.
    fn text_value(&self, id: Id) -> Option<String> {
        let node = self
            .tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))?;
        let ui_core::NodeData::Text { block, .. } = &node.data else {
            return None;
        };
        Some(block.flatten().0)
    }

    /// The text a node holds, whether it is an editable input or a read-only label.
    fn text_of(&self, id: Id) -> Option<String> {
        self.text_input_value(id)
            .map(|value| value.to_string())
            .or_else(|| self.text_value(id))
    }

    /// The bytes a selection covers in `id`, when `id` is one of its blocks.
    fn span_of(&self, id: Id) -> Option<ui_core::TextSelection> {
        self.selection_spans
            .iter()
            .find(|(owner, _)| *owner == id)
            .map(|(_, span)| *span)
    }

    /// Every selectable read-only block and its byte length, in the walk's reading order.
    fn selectable_blocks(&self) -> Vec<(Id, usize)> {
        self.tree
            .walk()
            .into_iter()
            .filter_map(|nid| {
                let node = self.tree.get(nid)?;
                if !Self::is_selectable_text(node) {
                    return None;
                }
                let ui_core::NodeData::Text { block, .. } = &node.data else {
                    return None;
                };
                let len = block.runs.iter().map(|run| run.text.len()).sum();
                Some((node.id, len))
            })
            .collect()
    }

    /// Rebuild the covered blocks from the two ends, in the walk's reading order.
    ///
    /// The walk is the order hit testing and painting already agree on, so a drag covers
    /// exactly the blocks it travelled over. Both ends outside the walk means the blocks
    /// themselves went away, and the selection with them.
    fn refresh_selection(&mut self) {
        let Some(ends) = self.selection else {
            self.selection_spans = Rc::from(Vec::new());
            return;
        };
        let blocks = self.selectable_blocks();
        let rank = |id: Id| blocks.iter().position(|(block, _)| *block == id);
        let (Some(anchor), Some(head)) = (rank(ends.anchor.0), rank(ends.head.0)) else {
            self.selection = None;
            self.selection_spans = Rc::from(Vec::new());
            return;
        };
        let (first, last) = (anchor.min(head), anchor.max(head));
        // Ranks alone cannot order two ends inside one block: there the offsets decide,
        // and a drag that ran right to left has its head before its anchor.
        let forward = anchor < head || (anchor == head && ends.anchor.1 <= ends.head.1);
        let (start, end) = match forward {
            true => (ends.anchor.1, ends.head.1),
            false => (ends.head.1, ends.anchor.1),
        };
        let mut spans = Vec::with_capacity(last - first + 1);
        for index in first..=last {
            let (id, len) = blocks[index];
            let from = if index == first { start.min(len) } else { 0 };
            let to = if index == last { end.min(len) } else { len };
            if to > from {
                spans.push((id, ui_core::TextSelection::new(from, to)));
            }
        }
        self.selection_spans = Rc::from(spans);
    }

    /// The selectable text block whose rectangle contains `p`, deepest first. A press on a
    /// block wrapped in a row answers with the block even though the row owns the point.
    fn selectable_block_at(&self, p: ui_core::geom::Point) -> Option<Id> {
        let mut found: Option<Id> = None;
        for nid in self.tree.walk() {
            let Some(node) = self.tree.get(nid) else {
                continue;
            };
            if Self::is_selectable_text(node)
                && self.text_input_owner(node.id).is_none()
                && node.visual_rect().contains(p)
            {
                // Later in the walk is deeper, so a nested block wins over its container.
                found = Some(node.id);
            }
        }
        found
    }

    /// The selectable text block under `p`, when the topmost thing there is one.
    fn selectable_text_at(&self, p: ui_core::geom::Point) -> Option<Id> {
        self.hit
            .topmost_consumer(p)
            .and_then(|nid| self.tree.get(nid).map(|n| n.id))
            .and_then(|id| self.selectable_text(id))
    }

    /// Count a press of `button` towards its burst, and report which press this is.
    ///
    /// Two presses join when the desktop's gap and rectangle both hold; a press outside
    /// either starts a new burst, as every desktop does. The count keeps climbing, so a
    /// third press in the same place reads 3 rather than starting over.
    fn click_burst(&mut self, button: ui_core::PointerButton, at: ui_core::geom::Point) -> u32 {
        let index = match button {
            ui_core::PointerButton::Primary => 0,
            ui_core::PointerButton::Secondary => 1,
            ui_core::PointerButton::Middle => 2,
        };
        let now = self.clock.live();
        let slop = self.pointer_timing.multi_click_slop;
        let clicks = self.last_clicks[index]
            .filter(|last| {
                now - last.at <= self.pointer_timing.multi_click.as_secs_f64()
                    && (last.pos.x - at.x).abs() <= slop
                    && (last.pos.y - at.y).abs() <= slop
            })
            .map_or(1, |last| last.clicks.saturating_add(1));
        self.last_clicks[index] = Some(MultiPress {
            at: now,
            pos: at,
            clicks,
        });
        clicks
    }

    /// Start (or extend into) a selection on a read-only text node.
    ///
    /// Returns true when the selection changed, which is what asks for a repaint.
    fn begin_selection(
        &mut self,
        node: Id,
        at: ui_core::geom::Point,
        extend: bool,
        clicks: u32,
    ) -> bool {
        let Some(index) = self.text_index_at(node, at) else {
            return false;
        };
        // Shift is the selection's own modifier: it moves the head and keeps the anchor,
        // so a press in another block extends the range instead of replacing it. The
        // second press of a burst selects the word under it, as an editor does.
        let double = !extend && clicks == 2;
        let ends = if double {
            let value = self.text_value(node).unwrap_or_default();
            let word = ui_core::text_input::word_at(&value, index);
            SelectionEnds {
                anchor: (node, word.start),
                head: (node, word.end),
            }
        } else if extend {
            match self.selection {
                Some(ends) => SelectionEnds {
                    anchor: ends.anchor,
                    head: (node, index),
                },
                None => SelectionEnds {
                    anchor: (node, index),
                    head: (node, index),
                },
            }
        } else {
            SelectionEnds {
                anchor: (node, index),
                head: (node, index),
            }
        };
        let changed = self.selection != Some(ends);
        self.selection = Some(ends);
        // A drag that follows a shift press extends from the anchor, which may live in
        // another block than the one the press landed in.
        self.select_drag = Some(ends.anchor.0);
        self.refresh_selection();
        changed
    }

    /// Extend the live selection to the pointer, if a selection drag is running.
    ///
    /// The head follows the selectable block under the pointer, so a drag that leaves its
    /// paragraph keeps selecting into the next one. Over anything else it stays where it
    /// was: the gap between two blocks keeps the last block's bytes rather than emptying
    /// the selection.
    fn extend_selection(&mut self, p: ui_core::geom::Point) -> bool {
        let Some(anchor) = self.select_drag else {
            return false;
        };
        let Some(ends) = self.selection else {
            return false;
        };
        let target = self.selectable_text_at(p).unwrap_or(ends.head.0);
        let Some(index) = self.text_index_at(target, p) else {
            return false;
        };
        if ends.head == (target, index) {
            return false;
        }
        self.selection = Some(SelectionEnds {
            anchor: (anchor, ends.anchor.1),
            head: (target, index),
        });
        self.refresh_selection();
        true
    }

    /// Hand the selected text to the host as a drag the operating system can carry out.
    ///
    /// The payload is the same text Copy would place on the clipboard, so the two can
    /// never disagree about what "the selection" is. The image is cut from the selection's
    /// own fragments — the selected lines, not the block around them.
    fn begin_text_drag(&mut self, node: Id, at: ui_core::geom::Point) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        let scale = self.text_scale();
        let mut bounds: Option<Rect> = None;
        for (id, span) in self.selection_spans.iter() {
            let Some(prim) = self.text_prim_of(*id) else {
                continue;
            };
            let fragments = self
                .text
                .borrow_mut()
                .selection_fragments(&prim, span.range(), scale);
            for fragment in fragments {
                let rect = fragment.rect;
                bounds = Some(bounds.map_or(rect, |union: Rect| union.union(rect)));
            }
        }
        let bounds = bounds.or_else(|| {
            self.tree
                .find_by_id(node)
                .and_then(|nid| self.tree.get(nid))
                .map(|n| n.visual_rect())
        });
        self.native_request = Some(NativeDrag {
            data: ui_core::DragData::bytes("text/plain", text.into_bytes()),
            bounds,
            origin: at,
            image: None,
        });
        // The block is the drag's source, but the lines the user selected are what leaves.
        self.native_crop = bounds;
        self.native_source = Some(node);
        self.drag.reset();
        true
    }

    /// Extend a read-only selection from the keyboard, the way an input extends its own.
    ///
    /// Only the extend chords: a read-only block has no editable caret, so a bare arrow
    /// still walks focus. The anchor stays wherever the last press or Ctrl+A put it, and
    /// the head crosses into the neighbouring block when it reaches that block's edge.
    fn extend_readonly_selection(
        &mut self,
        id: ui_core::ShortcutId,
    ) -> Option<(bool, bool)> {
        use ui_core::ShortcutId as S;
        if self.editing.is_some() {
            return None;
        }
        let ends = self.selection?;
        let blocks = self.selectable_blocks();
        let rank = blocks
            .iter()
            .position(|(block, _)| *block == ends.head.0)?;
        let block = blocks[rank].0;
        let value = self.text_value(block)?;
        let head = ui_core::text_input::clamp(&value, ends.head.1);
        let (index, block) = match id {
            S::EXTEND_LEFT => {
                let prev = ui_core::text_input::moved(&value, head, -1);
                if prev == head && head == 0 && rank > 0 {
                    let (previous, previous_len) = blocks[rank - 1];
                    (previous_len, previous)
                } else {
                    (prev, block)
                }
            }
            S::EXTEND_RIGHT => {
                let next = ui_core::text_input::moved(&value, head, 1);
                if next == head && head == value.len() && rank + 1 < blocks.len() {
                    (0, blocks[rank + 1].0)
                } else {
                    (next, block)
                }
            }
            S::EXTEND_WORD_LEFT => {
                let prev = ui_core::text_input::step_word(&value, head, -1);
                if prev == head && head == 0 && rank > 0 {
                    let (previous, previous_len) = blocks[rank - 1];
                    (previous_len, previous)
                } else {
                    (prev, block)
                }
            }
            S::EXTEND_WORD_RIGHT => {
                let next = ui_core::text_input::step_word(&value, head, 1);
                if next == head && head == value.len() && rank + 1 < blocks.len() {
                    (0, blocks[rank + 1].0)
                } else {
                    (next, block)
                }
            }
            S::EXTEND_HOME => (line_bounds(&value, head).0, block),
            S::EXTEND_END => (line_bounds(&value, head).1, block),
            S::EXTEND_DOCUMENT_HOME => (0, blocks.first()?.0),
            S::EXTEND_DOCUMENT_END => {
                let (last, last_len) = *blocks.last()?;
                (last_len, last)
            }
            _ => return None,
        };
        let ends = SelectionEnds {
            anchor: ends.anchor,
            head: (block, index),
        };
        if self.selection == Some(ends) {
            return Some((false, false));
        }
        self.selection = Some(ends);
        self.refresh_selection();
        Some((true, false))
    }

    /// The text node drawing `owner`'s buffer, the one a click must be measured against.
    ///
    /// Never the header, placeholder or clear glyph: answering from those puts a click
    /// beside the text at their own start.
    fn input_text_at(&self, owner: Id) -> Option<Id> {
        let value = self.text_input_value(owner)?;
        let (caret, preedit) = match self.editing.filter(|(o, _)| *o == owner) {
            Some((_, sel)) => (
                sel.caret,
                self.preedit
                    .as_ref()
                    .filter(|(id, _)| *id == owner)
                    .map(|(_, p)| p),
            ),
            None => (0, None),
        };
        self.input_text_node(owner, &value, preedit, caret)
    }

    /// The byte offset in an input's buffer nearest to a surface point.
    ///
    /// A multi-block input answers in buffer space: the point names a view, the view
    /// names a byte in its own text, and the offset is that byte plus the view's base.
    fn input_index_at(&mut self, owner: Id, p: ui_core::geom::Point) -> Option<usize> {
        if let Some(views) = self.input_views(owner) {
            let view = self.view_at_point(&views, p)?;
            // A view that draws nothing — a list marker while the caret is away from its
            // item — has no byte a press can point at, so the press belongs after it.
            if self.text_draws_nothing(view.node) {
                return Some(view.end());
            }
            // A formula answers with its own structural offset, so a press between two
            // glyphs of a fraction names the buffer byte that boundary exports as.
            if self.is_math_node(view.node) {
                let local = self
                    .math_space_at(view.node, p)
                    .and_then(|(x, y)| self.math_offset_at(view.node, x, y))
                    .unwrap_or(view.len);
                return Some(view.base + local.min(view.len));
            }
            let local = self.text_index_at(view.node, p)?;
            return Some(view.base + local.min(view.len));
        }
        let value = self.text_input_value(owner)?;
        match self.input_text_at(owner) {
            Some(text) => self.text_index_at(text, p),
            // An empty buffer draws no text node; its only caret sits at the origin.
            None if value.is_empty() => Some(0),
            None => None,
        }
    }

    /// Extend the input's selection to the pointer, if an editing drag is running.
    fn extend_editing(&mut self, p: ui_core::geom::Point) -> bool {
        let Some(owner) = self.editing_drag else {
            return false;
        };
        self.end_typing_run(owner);
        let Some(index) = self.input_index_at(owner, p) else {
            return false;
        };
        let Some((_, selection)) = self.editing.as_mut() else {
            return false;
        };
        if selection.caret == index {
            return false;
        }
        selection.caret = index;
        true
    }

    /// One arrow-key step in the focused input: Shift extends, a plain arrow collapses.
    ///
    /// With Ctrl the step is a word, the unit every mature editor uses for Ctrl+arrow.
    fn step_editing(&mut self, delta: isize) -> bool {
        // Ctrl+Shift+arrow is a table shortcut; an unbound one must not fall back to a word step.
        if self.modifiers.ctrl && self.modifiers.shift {
            return false;
        }
        let Some((owner, sel)) = self.editing else {
            return false;
        };
        self.end_typing_run(owner);
        self.goal_x = None;
        let value = self.text_input_value(owner).unwrap_or_default();
        let hidden = self.input_hidden_ranges(owner, &value, sel.caret);
        let views = self.input_views(owner);
        let step = |at: usize| {
            let at = if self.modifiers.shift {
                ui_core::text_input::step_word(&value, at, delta)
            } else {
                ui_core::text_input::moved(&value, at, delta)
            };
            let at = past_hidden(&hidden, at, delta);
            match views.as_deref() {
                Some(views) => snap_to_drawn(views, at, delta),
                None => at,
            }
        };
        let next = if self.modifiers.shift {
            ui_core::TextSelection::new(sel.anchor, step(sel.caret))
        } else if !sel.is_collapsed() {
            let range = sel.range();
            ui_core::TextSelection::collapsed(if delta < 0 { range.start } else { range.end })
        } else {
            ui_core::TextSelection::collapsed(step(sel.caret))
        };
        self.editing = Some((owner, next));
        next != sel
    }

    /// Extend the selection one character from its caret, keeping the anchor.
    fn extend_char_editing(&mut self, delta: isize) -> bool {
        self.extend_editing_by(delta, |value, caret| {
            ui_core::text_input::moved(value, caret, delta)
        })
    }

    /// Extend the selection one word from its caret, keeping the anchor.
    fn extend_word_editing(&mut self, delta: isize) -> bool {
        self.extend_editing_by(delta, |value, caret| {
            ui_core::text_input::step_word(value, caret, delta)
        })
    }

    /// Move the caret to `step`'s answer while the anchor stays put.
    fn extend_editing_by(&mut self, delta: isize, step: impl Fn(&str, usize) -> usize) -> bool {
        let Some((owner, sel)) = self.editing else {
            return false;
        };
        self.end_typing_run(owner);
        self.goal_x = None;
        let value = self.text_input_value(owner).unwrap_or_default();
        let hidden = self.input_hidden_ranges(owner, &value, sel.caret);
        let at = past_hidden(&hidden, step(&value, sel.caret), delta);
        let at = match self.input_views(owner).as_deref() {
            Some(views) => snap_to_drawn(views, at, delta),
            None => at,
        };
        let next = ui_core::TextSelection::new(sel.anchor, at);
        self.editing = Some((owner, next));
        next != sel
    }

    /// Every member of a block value the input's owner draws, in buffer offsets.
    ///
    /// One member is the one its block's source spells out; an edit there rewrites the rest
    /// in the same edit, so the value reads alike on every line.
    fn input_structures(&self, owner: Id) -> Vec<ui_core::text::Structure> {
        let Some(views) = self.input_views(owner) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for view in views {
            let Some(node) = self
                .tree
                .find_by_id(view.node)
                .and_then(|nid| self.tree.get(nid))
            else {
                continue;
            };
            if let ui_core::NodeData::Text { block, .. } = &node.data {
                out.extend(
                    block
                        .structures
                        .iter()
                        .map(|member| ui_core::text::Structure {
                            group: member.group,
                            range: view.base + member.range.start..view.base + member.range.end,
                            shown: member.shown,
                        }),
                );
            }
        }
        out
    }

    /// The byte ranges of the editing input's text that its owner is not drawing.
    ///
    /// A composition rewrites the shown text, so its offsets are not the buffer's; the
    /// ranges are empty while one is live, and the input method owns the caret anyway.
    fn input_hidden_ranges(
        &self,
        owner: Id,
        value: &str,
        caret: usize,
    ) -> Vec<std::ops::Range<usize>> {
        if self.preedit.is_some() {
            return Vec::new();
        }
        if let Some(views) = self.input_views(owner) {
            let mut out = Vec::new();
            for view in views {
                let Some(node) = self
                    .tree
                    .find_by_id(view.node)
                    .and_then(|nid| self.tree.get(nid))
                else {
                    continue;
                };
                if let ui_core::NodeData::Text { block, .. } = &node.data {
                    out.extend(
                        block
                            .hidden_ranges()
                            .into_iter()
                            .map(|range| view.base + range.start..view.base + range.end),
                    );
                }
            }
            return out;
        }
        let Some(text) = self.input_text_node(owner, value, None, caret) else {
            return Vec::new();
        };
        let Some(node) = self
            .tree
            .find_by_id(text)
            .and_then(|nid| self.tree.get(nid))
        else {
            return Vec::new();
        };
        match &node.data {
            ui_core::NodeData::Text { block, .. } => block.hidden_ranges(),
            _ => Vec::new(),
        }
    }

    /// Up/Down inside a text input: the offset nearest to the caret one line away.
    ///
    /// The column is held for as long as the run lasts, so stepping through a short line
    /// does not pull later steps to that line's end. A multi-block input continues into
    /// the neighbouring block when no line lies in that direction; `None` only when the
    /// input's own text is exhausted, so focus navigation takes over there.
    fn step_line_editing(&mut self, delta: isize) -> Option<bool> {
        let (owner, sel) = self.editing?;
        self.end_typing_run(owner);
        let value = self.text_input_value(owner)?;
        let views = self.input_views(owner);
        let (text, base, len) = match views.as_deref() {
            Some(views) => {
                let view =
                    view_of(views, sel.caret).or_else(|| nearest_view(views, sel.caret, delta))?;
                (view.node, view.base, view.len)
            }
            None => {
                let text = self.input_text_node(owner, &value, None, sel.caret)?;
                (text, 0, value.len())
            }
        };
        let prim = self.text_prim_of(text);
        // A caret can stand on a byte no view draws — the break between two blocks, the
        // line a quote's `> ` opens — so it is measured at the nearest drawn offset of the
        // view it belongs to rather than by subtracting a base it may sit before.
        let caret_local = sel.caret.saturating_sub(base).min(len);
        let caret = match prim {
            Some(ref prim) => self
                .text
                .borrow_mut()
                .caret_rect(prim, caret_local, self.scale)?,
            // A formula draws no line of its own: the structure moves the caret inside it,
            // and a step that reaches here is one it declined, so the caret leaves the block.
            None => self.math_caret_rect(text)?,
        };
        // The column is where the insertion point stands, not where the caret's box starts:
        // the box is centred on the point, so its left edge lies half a caret inside the
        // marker cell a step should land past.
        let goal = self.goal_x.unwrap_or(caret.origin.x + caret.size.w * 0.5);
        let mut at = sel.caret;
        // A step past the view's own last line ran out of lines in it: continue on the
        // nearest line past it in the direction of travel. A line, not the next view in
        // source order: a row's marker cells share its line, and the step belongs to the
        // view holding the column there.
        let stayed = match prim {
            Some(ref prim) => {
                match self
                    .text
                    .borrow_mut()
                    .line_step(prim, caret_local, delta, goal, self.scale)
                {
                    Some(local) => {
                        at = base + local;
                        false
                    }
                    None => true,
                }
            }
            None => true,
        };
        if stayed {
            let views = views.as_deref()?;
            let line = caret.origin.y..caret.origin.y + caret.size.h.max(0.0);
            let from = views.iter().position(|view| view.node == text)?;
            let to = self.crossing(views, from, delta, line, goal)?;
            let view = views[to];
            // Two views on one line are the layout's columns: a marker cell and the text
            // beside it. A step between two columns keeps the caret's place in the marker
            // instead of its x: a marker is written back to front from its separator, so
            // its digits grow leftward and no x sits after both `9.` and `10.` dots.
            let local = match self.text_prim_of(view.node) {
                Some(_) if self.is_column(views, from) && self.is_column(views, to) => {
                    column_offset(caret_local, len, view.len)
                }
                Some(prim) => {
                    let y = if delta < 0 {
                        prim.origin.y + prim.height - 1.0
                    } else {
                        prim.origin.y + 1.0
                    };
                    self.text.borrow_mut().index_at(
                        &prim,
                        ui_core::geom::Point::new(goal, y),
                        self.scale,
                    )
                }
                // A formula takes the step the way it takes a press: the stop nearest the
                // column, read from the edge the step came through.
                None => {
                    let node = self
                        .tree
                        .find_by_id(view.node)
                        .and_then(|nid| self.tree.get(nid))?;
                    let rect = node.visual_rect();
                    let y = if delta < 0 {
                        rect.origin.y + rect.size.h - 1.0
                    } else {
                        rect.origin.y + 1.0
                    };
                    self.math_space_at(view.node, ui_core::geom::Point::new(goal, y))
                        .and_then(|(x, y)| self.math_offset_at(view.node, x, y))
                        .unwrap_or(view.len)
                }
            };
            at = view.base + local.min(view.len);
        }
        if at == sel.caret {
            return None;
        }
        self.goal_x = Some(goal);
        let next = if self.modifiers.shift {
            ui_core::TextSelection::new(sel.anchor, at)
        } else {
            ui_core::TextSelection::collapsed(at)
        };
        self.editing = Some((owner, next));
        Some(next != sel)
    }

    /// The view a vertical step crosses into: on the nearest line in the direction of
    /// travel, the one drawing the goal column.
    ///
    /// A line can be several views — a marker cell and the text beside it share one — and a
    /// step must land in the view holding the column, not in the first view of the line.
    fn crossing(
        &self,
        views: &[InputView],
        from: usize,
        delta: isize,
        line: std::ops::Range<f32>,
        goal: f32,
    ) -> Option<usize> {
        let step: isize = if delta < 0 { -1 } else { 1 };
        let mut index = from as isize + step;
        while index >= 0 && (index as usize) < views.len() {
            let view = views[index as usize];
            if let Some((top, bottom)) = self.view_line(view.node)
                && !(top < line.end && line.start < bottom)
            {
                return Some(self.column_view(views, index as usize, goal));
            }
            index += step;
        }
        None
    }

    /// The vertical extent of the line a view draws: a text block's line, or a formula's box.
    fn view_line(&self, id: Id) -> Option<(f32, f32)> {
        if let Some(prim) = self.text_prim_of(id) {
            return Some((prim.origin.y, prim.origin.y + prim.height));
        }
        let node = self
            .tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))?;
        if !matches!(node.data, ui_core::NodeData::Math { .. }) {
            return None;
        }
        let rect = node.visual_rect();
        Some((rect.origin.y, rect.origin.y + rect.size.h))
    }

    /// The view of the line `index` is on that the column falls into.
    ///
    /// A row's views stand left to right in buffer order, so the view holding a column is
    /// the last one that starts at or before it; a column past the line's end stays with
    /// that view, where the offset clamps to its last byte.
    fn column_view(&self, views: &[InputView], index: usize, column: f32) -> usize {
        let mut chosen = self.line_run(views, index).start;
        for at in self.line_run(views, index) {
            let left = self
                .tree
                .find_by_id(views[at].node)
                .and_then(|nid| self.tree.get(nid))
                .map(|node| node.visual_rect().origin.x);
            if left.is_some_and(|left| left <= column) {
                chosen = at;
            }
        }
        chosen
    }

    /// The views sharing `index`'s line, as one contiguous run of view indices.
    ///
    /// A row is several views laid side by side, and lines stack: views are in buffer order,
    /// so one line's views follow one another and the next line starts a new run.
    fn line_run(&self, views: &[InputView], index: usize) -> std::ops::Range<usize> {
        let Some(prim) = self.text_prim_of(views[index].node) else {
            return index..index + 1;
        };
        let (top, bottom) = (prim.origin.y, prim.origin.y + prim.height);
        let on_line = |at: usize| {
            self.text_prim_of(views[at].node)
                .is_some_and(|p| p.origin.y < bottom && top < p.origin.y + p.height)
        };
        let mut first = index;
        while first > 0 && on_line(first - 1) {
            first -= 1;
        }
        let mut last = index + 1;
        while last < views.len() && on_line(last) {
            last += 1;
        }
        first..last
    }

    /// Whether `index`'s view is a column: another view draws to its right on its line.
    fn is_column(&self, views: &[InputView], index: usize) -> bool {
        self.line_run(views, index).end > index + 1
    }

    /// Home/End in the focused input; Shift keeps the anchor where it was.
    fn jump_editing(&mut self, at: usize) -> bool {
        let Some((owner, sel)) = self.editing else {
            return false;
        };
        self.end_typing_run(owner);
        self.goal_x = None;
        let next = if self.modifiers.shift {
            ui_core::TextSelection::new(sel.anchor, at)
        } else {
            ui_core::TextSelection::collapsed(at)
        };
        self.editing = Some((owner, next));
        next != sel
    }

    /// Resolve a key press through every layer, most specific first.
    ///
    /// `None` means nothing claimed the key, so the plain key arms still see it.
    fn fire_shortcut(&mut self, key: ui_core::Key) -> Option<(bool, bool)> {
        let mods = self.modifiers;
        // A control that owns focus outranks the hit chain, which outranks the app.
        if let Some((id, shortcut)) = self.shortcut_in_focus(key, mods) {
            self.queue_shortcut(id, shortcut);
            let changed = self.app_changed(id, ui_core::change::Change::Shortcut(shortcut));
            return Some((changed, false));
        }
        if let Some((id, shortcut)) = self.shortcut_in_hit_chain(key, mods) {
            self.queue_shortcut(id, shortcut);
            let changed = self.app_changed(id, ui_core::change::Change::Shortcut(shortcut));
            return Some((changed, false));
        }
        // A caret owns the keys that type into it and move it: an app binding for a plain
        // arrow or letter must not take the press the text needs. Chords with a command
        // modifier still resolve, and the built-in table still answers the caret's own.
        if self.caret_owns(key, mods) {
            return self.run_builtin(self.shortcuts.builtin_id(key, mods)?);
        }
        // The text's own commands stay the text's while a caret is open: an app binding for
        // Ctrl+C or Ctrl+Z must not act on the picture behind the field being typed into.
        if self.editing.is_some()
            && let Some(id) = self.shortcuts.builtin_id(key, mods)
        {
            return self.run_builtin(id);
        }
        if let Some(shortcut) = self.shortcuts.app_id(key, mods) {
            self.decls_stale = true;
            return Some((self.app.shortcut(shortcut, mods), false));
        }
        self.run_builtin(self.shortcuts.builtin_id(key, mods)?)
    }

    /// Whether a caret owns this press before any app binding.
    ///
    /// The keys that type into it and move it belong to the text; a command modifier turns
    /// them back into commands.
    fn caret_owns(&self, key: ui_core::Key, mods: ui_core::Modifiers) -> bool {
        self.editing.is_some()
            && !mods.ctrl
            && !mods.alt
            && !mods.logo
            && matches!(
                key,
                ui_core::Key::Char(_)
                    | ui_core::Key::Left
                    | ui_core::Key::Right
                    | ui_core::Key::Up
                    | ui_core::Key::Down
                    | ui_core::Key::Home
                    | ui_core::Key::End
                    | ui_core::Key::Backspace
                    | ui_core::Key::Delete
                    | ui_core::Key::Space
                    | ui_core::Key::Enter
            )
    }

    /// The menu a secondary press at `p` opens: the nearest declaring node, plus text rows.
    fn menu_for(&mut self, p: ui_core::geom::Point) -> Option<(Id, Vec<ui_core::MenuItem>)> {
        let hit = self
            .hit
            .topmost_consumer(p)
            .and_then(|nid| self.tree.get(nid).map(|n| n.id));
        let mut owner = None;
        let mut items = Vec::new();
        let mut cur = hit.and_then(|id| self.tree.find_by_id(id));
        while let Some(nid) = cur {
            let node = self.tree.get(nid)?;
            if let Some(menu) = node.data.params().map(|p| &p.menu)
                && !menu.is_empty()
            {
                owner = Some(node.id);
                items = menu.clone();
                break;
            }
            cur = node.parent;
        }
        // Text always offers its own four rows, appended below whatever the app declared.
        if let Some((text_owner, text_items)) = self.text_menu() {
            owner.get_or_insert(text_owner);
            items.extend(text_items);
        }
        owner
            .map(|owner| (owner, items))
            .filter(|(_, i)| !i.is_empty())
    }

    /// Select-all / copy / cut / paste, with the enabled state the engine can see and the
    /// words the app supplied.
    fn text_menu(&self) -> Option<(Id, Vec<ui_core::MenuItem>)> {
        use ui_core::MenuItem;
        let labels = self.text_menu_labels.as_ref()?;
        // The engine owns these ids, so it owns their chords too: the row runs the very key
        // the app printed beside it, even when the app rebound the built-in.
        let row = |id: &'static str,
                   text: &crate::menu::MenuRowText,
                   builtin: ui_core::ShortcutId,
                   on: bool| {
            let mut item = MenuItem::new(Id::str(id), text.label.clone());
            if let Some(shortcut) = text.shortcut.clone() {
                item = item.with_shortcut(shortcut);
            }
            if let Some(chord) = self.shortcuts.chord_of(builtin) {
                item = item.with_accelerator(chord);
            }
            if on { item } else { item.disabled() }
        };
        if let Some((owner, sel)) = self.editing {
            let value = self.text_input_value(owner).unwrap_or_default();
            let has = !sel.is_collapsed() && !self.hidden_input(owner);
            return Some((
                owner,
                vec![
                    row(
                        MENU_SELECT_ALL,
                        &labels.select_all,
                        ui_core::ShortcutId::SELECT_ALL,
                        !value.is_empty(),
                    ),
                    row(MENU_COPY, &labels.copy, ui_core::ShortcutId::COPY, has),
                    row(MENU_CUT, &labels.cut, ui_core::ShortcutId::CUT, has),
                    row(MENU_PASTE, &labels.paste, ui_core::ShortcutId::PASTE, true),
                ],
            ));
        }
        // A read-only selection can be copied, never cut or pasted into.
        let ends = self.selection?;
        let has = !self.selection_spans.is_empty();
        Some((
            ends.head.0,
            vec![
                row(
                    MENU_SELECT_ALL,
                    &labels.select_all,
                    ui_core::ShortcutId::SELECT_ALL,
                    true,
                ),
                row(MENU_COPY, &labels.copy, ui_core::ShortcutId::COPY, has),
            ],
        ))
    }

    /// Open the declared menu at `p`; a stale menu closes first.
    fn open_menu(&mut self, p: ui_core::geom::Point) -> bool {
        self.close_menu();
        let Some((owner, items)) = self.menu_for(p) else {
            return false;
        };
        self.show_menu(owner, p, items)
    }

    /// Open an app-built menu at `at` for `owner`.
    ///
    /// The programmatic form of a declared menu: a pick reports `MenuItem` to `owner`, a
    /// dismissal reports `MenuClosed`, and an empty run of dividers opens nothing.
    pub fn open_menu_at(
        &mut self,
        owner: Id,
        at: ui_core::geom::Point,
        items: Vec<ui_core::menu::MenuItem>,
    ) -> bool {
        if !menu_has_rows(&items) {
            return false;
        }
        self.close_menu();
        self.show_menu(owner, at, items)
    }

    /// Replace the open menu's rows when it still belongs to `owner`.
    ///
    /// The panel keeps its point and does not run its entrance again; a menu that belongs to
    /// someone else, or to nobody, is left as it is.
    pub fn replace_menu_items(
        &mut self,
        owner: Id,
        items: Vec<ui_core::menu::MenuItem>,
    ) -> bool {
        let Some(menu) = self.menu.as_mut().filter(|menu| menu.owner == owner) else {
            return false;
        };
        if !menu.replace_items(items) {
            return false;
        }
        self.decls_stale = true;
        self.dispatched = true;
        true
    }

    /// The shared opener; the declared and programmatic paths differ only in owner and point.
    fn show_menu(
        &mut self,
        owner: Id,
        at: ui_core::geom::Point,
        items: Vec<ui_core::menu::MenuItem>,
    ) -> bool {
        if !menu_has_rows(&items) {
            return false;
        }
        let menu = crate::menu::Menu::open(owner, items, at);
        self.menu = Some(menu);
        self.decls_stale = true;
        self.dispatched = true;
        true
    }

    /// Close the menu, telling its owner it was dismissed.
    fn close_menu(&mut self) -> bool {
        let Some(menu) = self.menu.take() else {
            return false;
        };
        self.decls_stale = true;
        self.dispatched = true;
        self.app_changed(menu.owner, ui_core::change::Change::MenuClosed);
        true
    }

    /// Report one menu command, then close the menu without a dismissal report.
    fn pick_menu_item(&mut self, item: Id) -> bool {
        let Some(menu) = self.menu.take() else {
            return false;
        };
        self.decls_stale = true;
        self.dispatched = true;
        let builtin = if item == Id::str(MENU_SELECT_ALL) {
            Some(ui_core::ShortcutId::SELECT_ALL)
        } else if item == Id::str(MENU_COPY) {
            Some(ui_core::ShortcutId::COPY)
        } else if item == Id::str(MENU_CUT) {
            Some(ui_core::ShortcutId::CUT)
        } else if item == Id::str(MENU_PASTE) {
            Some(ui_core::ShortcutId::PASTE)
        } else {
            None
        };
        match builtin {
            Some(id) => {
                self.run_builtin(id);
                true
            }
            None => self.app_changed(menu.owner, ui_core::change::Change::MenuItem(item)),
        }
    }

    /// Handle one event while a menu is open. `None` means it falls through.
    ///
    /// The rows are ordinary nodes on a panel of their own: presses and moves are the
    /// tree's, and only the keys stay here, where the menu's own order lives.
    fn menu_event(&mut self, event: &ui_core::Event) -> Option<bool> {
        match event {
            // A second right-click closes this menu; the press then opens a fresh one.
            ui_core::Event::PointerPressed {
                button: ui_core::PointerButton::Secondary,
                ..
            } => {
                self.close_menu();
                None
            }
            // A row's own chord picks it, as a shortcut anywhere else would; a caret still
            // keeps the keys that type into it, so Enter commits the text and not the row.
            ui_core::Event::KeyPressed { key } => {
                let mods = self.modifiers;
                if !self.caret_owns(*key, mods)
                    && let Some(id) = self
                        .menu
                        .as_ref()
                        .and_then(|menu| menu.accelerator_item(*key, mods))
                {
                    return Some(self.pick_menu_item(id));
                }
                match key {
                    ui_core::Key::Escape => Some(self.close_menu()),
                    // The key that opens a menu closes it, as Escape does.
                    ui_core::Key::Menu => Some(self.close_menu()),
                    ui_core::Key::Up => Some(self.menu_move(false)),
                    ui_core::Key::Down => Some(self.menu_move(true)),
                    ui_core::Key::Home => Some(self.menu_hover_edge(false)),
                    ui_core::Key::End => Some(self.menu_hover_edge(true)),
                    ui_core::Key::Right => Some(self.menu_open_submenu()),
                    // Left backs out one flyout, then closes the menu when none is open.
                    ui_core::Key::Left => match self
                        .menu
                        .as_mut()
                        .is_some_and(|menu| menu.close_deepest_submenu())
                    {
                        true => {
                            self.decls_stale = true;
                            self.dispatched = true;
                            Some(true)
                        }
                        false => Some(self.close_menu()),
                    },
                    ui_core::Key::Enter => Some(self.menu_enter()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Move the hover in the deepest open level.
    fn menu_move(&mut self, forward: bool) -> bool {
        let moved = self
            .menu
            .as_mut()
            .is_some_and(|menu| menu.move_hover(forward));
        if moved {
            self.decls_stale = true;
            self.dispatched = true;
        }
        moved
    }

    fn menu_hover_edge(&mut self, last: bool) -> bool {
        let moved = self
            .menu
            .as_mut()
            .is_some_and(|menu| menu.move_hover_edge(last));
        if moved {
            self.decls_stale = true;
            self.dispatched = true;
        }
        moved
    }

    fn menu_open_submenu(&mut self) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        let opened = menu.open_hovered_submenu();
        if opened {
            self.decls_stale = true;
            self.dispatched = true;
        }
        opened
    }

    fn menu_enter(&mut self) -> bool {
        let outcome = match self.menu.as_mut() {
            Some(menu) => menu.enter(),
            None => return false,
        };
        match outcome {
            crate::menu::MenuEnter::Opened => {
                self.decls_stale = true;
                self.dispatched = true;
                true
            }
            crate::menu::MenuEnter::Pick(id) => self.pick_menu_item(id),
            crate::menu::MenuEnter::Nothing => true,
        }
    }

    /// A row of an open menu was activated: pick it, or open the flyout it heads.
    fn menu_activated(&mut self, id: Id) -> bool {
        let located = self.menu.as_ref().and_then(|menu| menu.locate(id));
        let Some((depth, index)) = located else {
            return false;
        };
        let Some((submenu, pick)) = self.menu.as_mut().and_then(|menu| {
            let level = menu.level_mut(depth)?;
            let item = level.items.get(index)?;
            Some((
                matches!(item.kind, ui_core::menu::MenuRow::Submenu(_)),
                item.id,
            ))
        }) else {
            return false;
        };
        if !submenu {
            return self.pick_menu_item(pick);
        }
        let opened = self
            .menu
            .as_mut()
            .and_then(|menu| menu.level_mut(depth))
            .is_some_and(|level| level.open_row(index));
        if opened {
            self.decls_stale = true;
            self.dispatched = true;
        }
        true
    }

    /// A menu panel's surface was taken back: close that flyout, or the whole menu.
    fn menu_dismissed(&mut self, id: Id) -> bool {
        let Some(depth) = self
            .menu
            .as_ref()
            .and_then(|menu| (0..=menu.depth()).find(|depth| crate::menu::Menu::panel_id(*depth) == id))
        else {
            return false;
        };
        if depth == 0 {
            return self.close_menu();
        }
        if let Some(level) = self
            .menu
            .as_mut()
            .and_then(|menu| menu.level_mut(depth - 1))
            && level.child.take().is_some()
        {
            self.decls_stale = true;
            self.dispatched = true;
        }
        true
    }

    /// The level a press asks to close: the popup stack is one menu, so only a press
    /// outside every open panel closes it all.
    fn menu_dismissal(&self, at: ui_core::geom::Point, depth: usize) -> Option<Id> {
        let contains = |level: usize| {
            self.tree
                .find_by_id(crate::menu::Menu::panel_id(level))
                .and_then(|nid| self.tree.get(nid))
                .is_some_and(|node| node.visual_rect().contains(at))
        };
        match (0..=depth).rev().find(|level| contains(*level)) {
            // The press is on the deepest flyout: it is for that level's rows.
            Some(level) if level == depth => None,
            // A flyout hangs over its parent: pressing the parent closes only the flyout.
            Some(level) => Some(crate::menu::Menu::panel_id(level + 1)),
            None => Some(crate::menu::Menu::panel_id(0)),
        }
    }

    /// Open the focused node's declared menu, or the hovered node's when nothing is
    /// focused, anchored at that node's bottom-left.
    fn open_menu_for_key(&mut self) -> bool {
        let Some(start) = self.focused.or(self.hovered) else {
            return false;
        };
        let mut current = self.tree.find_by_id(start);
        while let Some(nid) = current {
            let Some(node) = self.tree.get(nid) else {
                break;
            };
            if let Some(items) = node
                .data
                .params()
                .map(|params| &params.menu)
                .filter(|menu| !menu.is_empty())
            {
                let owner = node.id;
                let at = ui_core::geom::Point::new(node.rect.origin.x, node.rect.max_y());
                let items = items.clone();
                return self.show_menu(owner, at, items);
            }
            current = node.parent;
        }
        false
    }
}

/// The room one line of text needs in its box, as the type ramp draws it.
const LINE_HINT: f32 = 1.4;

/// One menu level: its rows, and the flyout they opened.
fn menu_level_decl(
    cx: &mut ui_core::cx::Cx<'_>,
    style: &crate::menu::MenuStyle,
    level: &mut crate::menu::Menu,
    depth: usize,
    width_of: &mut impl FnMut(&ui_core::text::TextBlock) -> f32,
) -> Decl {
    // The rows are nodes now, so the pointer's verdict is the build's to read; the model
    // treats it as a move only when it differs from the last read.
    let hovered_row = (0..level.items.len()).find(|index| {
        level.hoverable(*index)
            && crate::WidgetState::of(cx, crate::menu::Menu::row_id(depth, *index)).hovered
    });
    level.pointer_over(hovered_row);
    let items = level.items.clone();
    // The panel is as wide as its widest row, measured this frame: a menu that outlives a
    // font load must not keep the metrics it was opened with.
    let width = crate::menu::level_width(style, &items, width_of);
    let hovered = level.hovered;
    let check = level.has_check().then(|| crate::menu::check_column(style));
    let mut rows = Vec::with_capacity(items.len());
    let mut index = 0;
    while index < items.len() {
        let end = crate::menu::strip_end(&items, index);
        if end > index {
            rows.push(menu_strip(
                depth,
                &items[index..end],
                index,
                hovered,
                style,
                width,
            ));
            index = end;
            continue;
        }
        let item = &items[index];
        let id = crate::menu::Menu::row_id(depth, index);
        let hovered_item = hovered == Some(index);
        index += 1;
        rows.push(match item.kind {
            ui_core::menu::MenuRow::Separator => menu_separator(id, style),
            _ => {
                // What the row leaves for its label: the panel can be capped by the style, and
                // a label that took more would push the chord past the panel's edge.
                let label_width = (width
                    - 2.0 * style.row_inset
                    - 2.0 * style.pad_x
                    - check.unwrap_or(0.0)
                    - crate::menu::trailing_width(style, item, width_of))
                    .max(0.0);
                menu_row(id, item, hovered_item, check, style, label_width)
            }
        });
    }
    let mut panel = Decl::container(crate::menu::Menu::panel_id(depth), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fixed(width),
            height: AxisSize::hug(0.0, f32::INFINITY),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets::new(0.0, style.pad_y, 0.0, style.pad_y))
        .background(style.surface, ui_core::geom::Corners::all(style.radius))
        // The style names the falloff; the lift follows it as MD3's own elevation does.
        .shadow(ui_core::tree::ShadowSpec::black(
            style.shadow,
            0.18,
            style.shadow * 0.25,
        ))
        .children(rows);
    if let Some((colour, width)) = style.border {
        panel = panel.border(colour, width);
    }
    if let Some(frost) = style.frost {
        panel = panel.backdrop_blur(frost.sigma);
    }
    // The root hangs off the point the press named; a flyout off the row that heads it,
    // and the platform is the one that decides whether either fits on the output.
    let anchor = match (depth, level.header) {
        (0, _) => Anchor::Point(level.at),
        (_, Some(index)) => Anchor::Node(crate::menu::Menu::row_id(depth - 1, index)),
        (_, None) => Anchor::Point(level.at),
    };
    let side = match depth {
        0 => Side::Below,
        _ => Side::Right,
    };
    panel = panel.overlay(OverlayPlacement {
        anchor,
        side,
        align: OverlayAlign::Start,
        offset: 0.0,
        flip: true,
        match_anchor_width: false,
        match_anchor_height: false,
        min_width: 0.0,
        presentation: Presentation::Platform,
    });
    if depth == 0 {
        // The panel travels half its height up from below the pointer and fades in. Its rows
        // are known before layout, so the height the entrance starts from is too.
        let height = crate::menu::level_height(style, &items) + 2.0 * style.pad_y;
        // The panel takes no sense: the menu owns its own keys and its own light dismiss, and
        // a focusable overlay would move the page's focus into a panel with no stops.
        panel = panel.enter(ui_core::tree::Presence::slide(
            crate::menu::ENTRANCE,
            0.0,
            ui_core::geom::Point::new(0.0, -height * 0.5),
        ));
    }
    if let Some(child) = level.child.as_mut() {
        panel = panel.child(menu_level_decl(cx, style, child, depth + 1, width_of));
    }
    panel
}

/// One command row: its mark column, the label (and a disabled reason), and its chord.
fn menu_row(
    id: Id,
    item: &ui_core::menu::MenuItem,
    hovered: bool,
    check: Option<f32>,
    style: &crate::menu::MenuStyle,
    label_width: f32,
) -> Decl {
    let ink = match item.enabled {
        true => style.ink,
        false => style.muted,
    };
    let mut row = Decl::container(id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, f32::INFINITY),
            height: AxisSize::fixed(crate::menu::row_height(style, item)),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .margin(Insets::new(style.row_inset, 0.0, style.row_inset, 0.0))
        .padding(Insets::new(style.pad_x, 0.0, style.pad_x, 0.0))
        .cross_align(0.0)
        .cross_pos(0.5);
    if item.enabled {
        row = row
            .sense(ui_core::Sense::HOVER | ui_core::Sense::CLICK)
            .cursor(ui_core::CursorShape::PointingHand);
    }
    if hovered && item.enabled {
        row = row.background(style.hover, ui_core::geom::Corners::all(style.radius));
    }
    if let Some(width) = check {
        row = row.child(menu_check(id.with("check"), item.checked, width, ink, style));
    }
    let mut label = Decl::container(id.with("label"), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, f32::INFINITY),
            height: AxisSize::hug(0.0, f32::INFINITY),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .cross_align(0.0)
        .gap(crate::menu::REASON_GAP)
        .child(menu_text(
            id.with("text"),
            style.label_block(&item.label, item.is_default),
            ink,
            label_width,
        ));
    if let Some(reason) = crate::menu::reason_of(item) {
        label = label.child(menu_text(
            id.with("reason"),
            style.reason_block(reason),
            style.muted,
            label_width,
        ));
    }
    row = row.child(label);
    if let Some(shortcut) = item.shortcut.as_deref() {
        let chord_ink = match item.enabled {
            true => style.shortcut_ink,
            false => style.muted,
        };
        row = row.child(
            menu_text(
                id.with("shortcut"),
                style.shortcut_block(shortcut),
                chord_ink,
                f32::INFINITY,
            )
            .margin(Insets::new(style.shortcut_gap, 0.0, 0.0, 0.0)),
        );
    }
    if matches!(item.kind, ui_core::menu::MenuRow::Submenu(_)) {
        // The mark is the only sign the row heads a cascade; without it a header reads as
        // a command that does nothing when pressed.
        let ink = match item.enabled {
            true => style.shortcut_ink,
            false => style.muted,
        };
        row = row.child(
            menu_text(
                id.with("chevron"),
                style.chevron_block(),
                ink,
                f32::INFINITY,
            )
            .margin(Insets::new(style.shortcut_gap, 0.0, 0.0, 0.0)),
        );
    }
    row
}

/// The strip of quick actions: equal cells of an icon over its label, side by side.
fn menu_strip(
    depth: usize,
    items: &[ui_core::menu::MenuItem],
    start: usize,
    hovered: Option<usize>,
    style: &crate::menu::MenuStyle,
    width: f32,
) -> Decl {
    // Cells share the panel's width, so a label's room is its share minus its own padding.
    let cell = ((width - 2.0 * style.row_inset) / items.len() as f32).max(0.0);
    let label_width = (cell - 2.0 * style.compact_pad_x).max(0.0);
    let cells: Vec<Decl> = items
        .iter()
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
            menu_strip_cell(
                crate::menu::Menu::row_id(depth, index),
                item,
                hovered == Some(index),
                style,
                label_width,
            )
        })
        .collect();
    Decl::container(
        crate::menu::Menu::panel_id(depth).with("strip"),
        NodeKind::Row,
    )
    .size(SizeClass {
        width: AxisSize::fill(0.0, f32::INFINITY),
        height: AxisSize::fixed(style.compact_height),
        grow_x: Grow::Yes,
        grow_y: Grow::No,
    })
    .margin(Insets::new(style.row_inset, 0.0, style.row_inset, 0.0))
    .children(cells)
}

/// One strip cell: an icon over its label, centred in the share of the strip it owns.
fn menu_strip_cell(
    id: Id,
    item: &ui_core::menu::MenuItem,
    hovered: bool,
    style: &crate::menu::MenuStyle,
    label_width: f32,
) -> Decl {
    let ink = match item.enabled {
        true => style.ink,
        false => style.muted,
    };
    let mut cell = Decl::container(id, NodeKind::Stack)
        .size(SizeClass {
            width: AxisSize::fill(0.0, f32::INFINITY),
            height: AxisSize::fixed(style.compact_height),
            // A zero basis is what makes the cells equal; a content basis would start each
            // at its own label's width and grow by the same slack from there.
            grow_x: Grow::Split(1),
            grow_y: Grow::No,
        })
        .stack_align(OverlayAlign::Center);
    if item.enabled {
        cell = cell
            .sense(ui_core::Sense::HOVER | ui_core::Sense::CLICK)
            .cursor(ui_core::CursorShape::PointingHand);
    }
    if hovered && item.enabled {
        cell = cell.background(style.hover, ui_core::geom::Corners::all(style.radius));
    }
    let mut body = Decl::container(id.with("body"), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::hug(0.0, label_width.max(style.compact_icon_size)),
            height: AxisSize::hug(0.0, f32::INFINITY),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .cross_align(0.0)
        .cross_pos(0.5)
        .gap(style.compact_gap);
    if let Some(icon) = item.icon {
        body = body.child(menu_text(
            id.with("icon"),
            style.icon_block(icon),
            ink,
            style.compact_icon_size,
        ));
    }
    body = body.child(menu_text(
        id.with("label"),
        style.compact_label_block(&item.label, item.is_default),
        ink,
        label_width,
    ));
    cell.child(body)
}

/// The leading column a row's mark sits in, reserved even when that row carries none.
fn menu_check(
    id: Id,
    checked: bool,
    width: f32,
    ink: Color,
    style: &crate::menu::MenuStyle,
) -> Decl {
    let mut slot = Decl::container(id, NodeKind::Row)
        .size(SizeClass::fixed(width, style.row_height))
        .cross_align(0.0)
        .cross_pos(0.5);
    if checked {
        slot = slot.child(menu_text(
            id.with("mark"),
            style.check_block(),
            ink,
            width,
        ));
    }
    slot
}

/// A divider is a hairline inset from both edges, not a row the pointer can aim at.
fn menu_separator(id: Id, style: &crate::menu::MenuStyle) -> Decl {
    let inset = (crate::menu::SEPARATOR_HEIGHT - 1.0) * 0.5;
    let line = Decl::container(id.with("line"), NodeKind::Box)
        .size(SizeClass {
            width: AxisSize::fill(0.0, f32::INFINITY),
            height: AxisSize::fixed(1.0),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .background(style.muted, ui_core::geom::Corners::ZERO);
    Decl::container(id, NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, f32::INFINITY),
            height: AxisSize::fixed(crate::menu::SEPARATOR_HEIGHT),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .padding(Insets::new(style.pad_x, inset, style.pad_x, inset))
        .cross_align(1.0)
        .child(line)
}

/// One shaped line of menu text.
///
/// The size class is the text's own: a plain container hugs at zero, and a nowrap line is
/// capped by that hint, so the node would measure and place at no width at all. `max_width`
/// is the room the line owns; a line past it ends with an ellipsis.
fn menu_text(
    id: Id,
    mut block: ui_core::text::TextBlock,
    ink: Color,
    max_width: f32,
) -> Decl {
    block.max_lines = Some(1);
    let size = block.runs.first().map_or(0.0, |run| run.style.size);
    let mut decl = Decl::container(id, NodeKind::Text).size(SizeClass {
        width: AxisSize::hug(0.0, max_width),
        height: AxisSize::hug(size, size * LINE_HINT),
        grow_x: Grow::No,
        grow_y: Grow::No,
    });
    decl.data = NodeData::Text {
        block: std::sync::Arc::new(block),
        ink,
        caret: None,
        selection: None,
        composing: None,
        nowrap: true,
    };
    decl
}

/// Put the open menu on the app's own root, the one node the window always has.
///
/// A wrapper of the engine's own would read as an ordinary parent and drop the root's
/// fill-the-window sizing, so the panel joins the root's children and leaves the flow at
/// once through its overlay placement.
fn with_menu(app: Decl, menu: Option<Decl>) -> Decl {
    match menu {
        Some(panel) => app.child(panel),
        None => app,
    }
}

/// Declare the open menu, leaving the pointer's verdict recorded on it as the rows build.
fn menu_panel_decl(
    cx: &mut ui_core::cx::Cx<'_>,
    menu: &mut Option<crate::menu::Menu>,
    style: &crate::menu::MenuStyle,
    text: &Rc<RefCell<TextPass>>,
) -> Option<Decl> {
    let mut open = menu.take()?;
    let mut width_of =
        |block: &ui_core::text::TextBlock| text.borrow_mut().measure(block, f32::INFINITY).w;
    let decl = menu_level_decl(cx, style, &mut open, 0, &mut width_of);
    *menu = Some(open);
    Some(decl)
}

impl<A: App> AppLoop<A> {
    /// The nearest focused node that wants raw chords, if any.
    fn key_capture_owner(&self) -> Option<Id> {
        let mut cur = self.focused.and_then(|id| self.tree.find_by_id(id));
        while let Some(nid) = cur {
            let node = self.tree.get(nid)?;
            if node.sense.contains(ui_core::Sense::CAPTURE_KEYS) {
                return Some(node.id);
            }
            cur = node.parent;
        }
        None
    }

    /// The nearest node on the focus chain that binds this chord.
    fn shortcut_in_focus(
        &self,
        key: ui_core::Key,
        mods: ui_core::Modifiers,
    ) -> Option<(Id, ui_core::ShortcutId)> {
        let mut cur = self.focused.and_then(|id| self.tree.find_by_id(id));
        while let Some(nid) = cur {
            let node = self.tree.get(nid)?;
            if let Some(found) = bound_on(node, key, mods) {
                return Some((node.id, found));
            }
            cur = node.parent;
        }
        None
    }

    /// The nearest node on the hit chain that binds this chord: hover alone can claim a key.
    fn shortcut_in_hit_chain(
        &self,
        key: ui_core::Key,
        mods: ui_core::Modifiers,
    ) -> Option<(Id, ui_core::ShortcutId)> {
        for id in self.pointer_path.iter().rev() {
            let Some(node) = self.tree.find_by_id(*id).and_then(|n| self.tree.get(n)) else {
                continue;
            };
            if let Some(found) = bound_on(node, key, mods) {
                return Some((node.id, found));
            }
        }
        None
    }

    /// Run the engine's action for a built-in id; `None` when it does not apply right now.
    fn run_builtin(&mut self, id: ui_core::ShortcutId) -> Option<(bool, bool)> {
        use ui_core::ShortcutId as S;
        // A focused formula owns the keyboard, so it also owns the chords that read or
        // rewrite the caret's history and text: the document behind it must not answer.
        if let Some(formula) = self.math_edit_id() {
            match id {
                S::UNDO => {
                    let changed = self.math_edit_call(formula, |edit, id| edit.undo(id), false);
                    if changed {
                        self.math_edit_reported(formula);
                    }
                    return Some((changed, false));
                }
                S::REDO => {
                    let changed = self.math_edit_call(formula, |edit, id| edit.redo(id), false);
                    if changed {
                        self.math_edit_reported(formula);
                    }
                    return Some((changed, false));
                }
                S::SELECT_ALL => {
                    let changed =
                        self.math_edit_call(formula, |edit, id| edit.select_all(id), false);
                    return Some((changed, false));
                }
                S::COPY => return Some((self.copy_formula(formula, false), false)),
                S::CUT => return Some((self.copy_formula(formula, true), false)),
                _ => {}
            }
        }
        // Copying also serves a read-only selection, which no input owns.
        if matches!(id, S::COPY | S::CUT) {
            return Some((self.copy_selection(id == S::CUT), false));
        }
        // Selecting everything needs no owner to write into, so read-only text serves too:
        // one selection over every selectable block, as a page's Ctrl+A does.
        if id == S::SELECT_ALL {
            if let Some((owner, _)) = self.editing {
                let len = self.text_of(owner).map_or(0, |v| v.len());
                self.end_typing_run(owner);
                self.editing = Some((owner, ui_core::TextSelection::new(0, len)));
                return Some((true, false));
            }
            let blocks = self.selectable_blocks();
            let (Some(first), Some(last)) = (blocks.first(), blocks.last()) else {
                return None;
            };
            self.selection = Some(SelectionEnds {
                anchor: (first.0, 0),
                head: (last.0, last.1),
            });
            // No pointer drag is live: arming one made the next hover extend the selection.
            self.select_drag = None;
            self.refresh_selection();
            return Some((true, false));
        }
        // A read-only selection takes the same extension chords an input does: the anchor
        // stays and the head moves, crossing into the neighbouring block at an edge.
        if let Some(result) = self.extend_readonly_selection(id) {
            return Some(result);
        }
        // A paste with no caret belongs to whoever the app focused: the payload comes back
        // as `Change::Paste`. Latest request wins, so a read that never answers cannot
        // misroute a later paste.
        if id == S::PASTE {
            self.paste_target = match self.editing {
                Some(_) => None,
                // The app's own root, not the surface wrapper the menu hangs beside.
                None => self.focused.or(self.app_root),
            };
            self.clipboard
                .push(crate::clipboard::ClipboardCommand::RequestPaste {
                    kind: crate::clipboard::ClipboardKind::Selection,
                });
            return Some((self.editing.is_some(), false));
        }
        let (owner, sel) = self.editing?;
        match id {
            S::UNDO => self.step_history(owner, false),
            S::REDO => self.step_history(owner, true),
            S::DELETE_FORWARD => {
                let value = self.text_input_value(owner)?;
                let range = if sel.is_collapsed() {
                    let next = ui_core::text_input::moved(&value, sel.caret, 1);
                    if next == sel.caret {
                        return Some((false, false));
                    }
                    sel.caret..next
                } else {
                    sel.range()
                };
                let changed = self
                    .edit_structural(owner, &value, range, "", false)
                    .unwrap_or(false);
                Some((changed, true))
            }
            S::HOME | S::EXTEND_HOME => {
                let value = self.text_input_value(owner).unwrap_or_default();
                let start = line_bounds(&value, sel.caret).0;
                // The first stop is where the line's text begins: leading indentation is
                // bytes the line spells, not something the caret edits by default. Pressing
                // Home again reaches the line's first byte, where those bytes are edited.
                let first = first_non_space(&value, start);
                let line = if sel.caret == first { start } else { first };
                Some((self.jump_editing(line), false))
            }
            S::END | S::EXTEND_END => {
                let value = self.text_input_value(owner).unwrap_or_default();
                Some((self.jump_editing(line_bounds(&value, sel.caret).1), false))
            }
            S::DOCUMENT_HOME | S::EXTEND_DOCUMENT_HOME => Some((self.jump_editing(0), false)),
            S::DOCUMENT_END | S::EXTEND_DOCUMENT_END => {
                let len = self.text_input_value(owner).map_or(0, |v| v.len());
                Some((self.jump_editing(len), false))
            }
            S::EXTEND_LEFT => Some((self.extend_char_editing(-1), false)),
            S::EXTEND_RIGHT => Some((self.extend_char_editing(1), false)),
            S::EXTEND_WORD_LEFT => Some((self.extend_word_editing(-1), false)),
            S::EXTEND_WORD_RIGHT => Some((self.extend_word_editing(1), false)),
            S::EXTEND_UP => self.step_line_editing(-1).map(|m| (m, false)),
            S::EXTEND_DOWN => self.step_line_editing(1).map(|m| (m, false)),
            _ => None,
        }
    }

    /// Copy the formula's selected runs as LaTeX; `cut` also removes them from its model.
    fn copy_formula(&mut self, id: Id, cut: bool) -> bool {
        let text = match self.math_edit.try_borrow_mut() {
            Ok(mut edit) => {
                if cut {
                    edit.cut(id)
                } else {
                    edit.selection_latex(id)
                }
            }
            Err(_) => None,
        };
        let Some(text) = text else {
            return false;
        };
        self.clipboard
            .push(crate::clipboard::ClipboardCommand::Copy {
                content: ui_core::clipboard::ClipboardContent::Text(text),
                kind: crate::clipboard::ClipboardKind::Selection,
            });
        if cut {
            self.math_edit_reported(id);
        }
        true
    }

    /// What the live read-only selection holds, one paragraph per line.
    ///
    /// `None` when nothing is selected or the selection is collapsed. Copy and a selection
    /// drag both read this, so what leaves the app as text and what leaves as a drag can
    /// never disagree.
    pub fn selected_text(&self) -> Option<String> {
        let mut parts = Vec::new();
        for (node, span) in self.selection_spans.iter() {
            let Some(value) = self.text_value(*node) else {
                continue;
            };
            let range = span.range();
            let (a, b) = (
                ui_core::text_input::clamp(&value, range.start),
                ui_core::text_input::clamp(&value, range.end),
            );
            if a < b {
                parts.push(value[a..b].to_string());
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }

    /// Copy the live selection to the clipboard; `cut` also removes it from the input.
    ///
    /// Returns true when the frame changed, which for a cut means a new buffer.
    fn copy_selection(&mut self, cut: bool) -> bool {
        let kind = crate::clipboard::ClipboardKind::Selection;
        if let Some((owner, sel)) = self.editing {
            // A secret buffer leaves by no engine path: the app still holds the value and
            // can copy it deliberately, but the engine never hands it to the clipboard.
            if self.hidden_input(owner) {
                return false;
            }
            let Some(value) = self.text_input_value(owner) else {
                return false;
            };
            let range = sel.range();
            let (a, b) = (
                ui_core::text_input::clamp(&value, range.start),
                ui_core::text_input::clamp(&value, range.end),
            );
            if a == b {
                return false;
            }
            self.clipboard
                .push(crate::clipboard::ClipboardCommand::Copy {
                    content: ui_core::clipboard::ClipboardContent::Text(value[a..b].to_string()),
                    kind,
                });
            if !cut {
                return true;
            }
            let (next, caret) = ui_core::text_input::delete_range(&value, a..b);
            return self.apply_edit(owner, next, ui_core::TextSelection::collapsed(caret), false);
        }
        // A read-only selection can be copied, never cut: nothing owns its text.
        if cut || self.selection.is_none() {
            return false;
        }
        // One block contributes its bytes alone; several are the selected paragraphs, one
        // line each, in the order the reader walked them.
        let Some(text) = self.selected_text() else {
            return false;
        };
        self.clipboard
            .push(crate::clipboard::ClipboardCommand::Copy {
                content: ui_core::clipboard::ClipboardContent::Text(text),
                kind,
            });
        true
    }

    /// Point the caret at the focused text input: the end of its buffer, or the previous
    /// offset when the same input keeps focus.
    fn sync_caret(&mut self) {
        let owner = self.focused.and_then(|id| self.text_input_owner(id));
        // A typing run belongs to one editing session; focus leaving it closes the group.
        if let Some((previous, _)) = self.editing
            && Some(previous) != owner
        {
            self.end_typing_run(previous);
        }
        // A composition belongs to one input; moving focus ends it.
        if self
            .preedit
            .as_ref()
            .is_some_and(|(id, _)| Some(*id) != owner)
        {
            self.preedit = None;
            self.reveal = None;
        }
        // A document input starts at its last drawn byte, not after the trailing line break.
        let fresh = owner.and_then(|owner| {
            let value = self.text_input_value(owner)?;
            Some(match self.input_views(owner) {
                Some(views) => views.last().map_or(value.len(), |view| view.end()),
                None => value.len(),
            })
        });
        self.editing = owner.map(|owner| {
            let value = self.text_input_value(owner).unwrap_or_default();
            let sel = match self.editing {
                // The same input keeps its selection; a newly focused one starts at the end.
                Some((prev, sel)) if prev == owner => sel,
                _ => ui_core::TextSelection::collapsed(fresh.unwrap_or(value.len())),
            };
            (
                owner,
                ui_core::TextSelection::new(
                    ui_core::text_input::clamp(&value, sel.anchor),
                    ui_core::text_input::clamp(&value, sel.caret),
                ),
            )
        });
    }

    /// Feed one input event; returns true when a repaint is needed.
    ///
    /// Hit testing uses the last laid-out frame, because input arrives between frames.
    /// Hold or release this surface's input, as a modal dialog of it does.
    ///
    /// A blocked surface still builds frames: only the events are dropped, so it stays
    /// painted behind the dialog instead of freezing on its last frame.
    pub fn set_input_blocked(&mut self, blocked: bool) {
        self.input_blocked = blocked;
    }

    pub fn dispatch(&mut self, event: ui_core::Event) -> bool {
        if self.input_blocked {
            return false;
        }
        let before = self.editing;
        let repaint = self.dispatch_event(event);
        // Events arrive in bursts between frames, so a caret this one moved into a formula
        // must reach the formula's editor before the next event, not at the next build.
        if self.editing != before {
            self.sync_formula_caret();
        }
        // A loop reads `take_dispatched`, so the flag carries every answer; the return is
        // for callers that dispatch one event and do not own the frame decision.
        self.dispatched |= repaint;
        repaint
    }

    /// Hand the input's selection to the formula the caret sits in.
    ///
    /// The document owns the caret; the structural editor is told where it stands before
    /// an event acts on it. The build repeats this for every formula it draws, because a
    /// selection can cross several formulas at once.
    fn sync_formula_caret(&mut self) {
        let Some((owner, selection)) = self.editing else {
            return;
        };
        let range = selection.range();
        let Ok(mut edit) = self.math_edit.try_borrow_mut() else {
            return;
        };
        let Some(id) = edit.formula_at(owner, range.end) else {
            return;
        };
        let Some((input, start, end)) = edit.owner(id) else {
            return;
        };
        if input != owner {
            return;
        }
        let local = |at: usize| at.clamp(start, end) - start;
        edit.select_offsets(id, local(selection.anchor), local(selection.caret));
    }

    /// The dispatch itself; `dispatch` is what records its answer as frame demand.
    fn dispatch_event(&mut self, event: ui_core::Event) -> bool {
        // A move that cannot change a pixel is delivered but does not ask for a build: the
        // pointer reports several times per refresh, and building is the expensive part.
        let absorbed =
            event.is_continuous() && event.pos().is_some_and(|p| !self.pointer_affects_frame(p));
        self.dispatched |= !absorbed;
        let before = (self.hovered, self.pressed, self.focused);
        // One-frame facts are cleared when a frame consumes them, not here: events arrive in
        // bursts, and a motion after a release must not erase a click no frame has read yet.
        // The app's answer to a callback is a repaint request, exactly like `event`'s.
        let mut requested = false;
        // A text edit already names the new buffer, so the caret must not be re-clamped
        // against the old one at the end of this dispatch.
        let mut edited = false;
        if let Some(p) = event.pos() {
            self.pointer = Some(p);
        }
        if matches!(event, ui_core::Event::PointerLeft) {
            self.pointer = None;
            // A drag keeps its capture across leave: the button is still down, and
            // dropping the gesture here would strand the app mid-move.
        }
        // An operating-system drag is its own input: it never moves the ordinary hover,
        // and it owns the pointer until it drops or leaves.
        if let ui_core::Event::Dnd(dnd) = &event {
            let requested = self.dnd_event(dnd.clone());
            return requested || before != (self.hovered, self.pressed, self.focused);
        }
        // A native drag owns the pointer from the moment the host takes it until `Finished`:
        // the press and release this window still sees belong to the OS gesture, and letting
        // them through would click the source the drag was started from.
        if self.native_source.is_some()
            && matches!(
                event,
                ui_core::Event::PointerPressed { .. }
                    | ui_core::Event::PointerMoved { .. }
                    | ui_core::Event::PointerReleased { .. }
            )
        {
            return false;
        }
        // An open menu keeps its keys, and its rows are ordinary nodes: a press or a move
        // reaches the tree, which is what routes it back as an activation of that row.
        if self.menu.is_some()
            && let Some(handled) = self.menu_event(&event)
        {
            return handled || before != (self.hovered, self.pressed, self.focused);
        }
        // The menu key opens the focused (or hovered) node's declared menu. With none
        // declared the key falls through to the app's bindings, the same way a secondary
        // tap without a declared menu is the app's to answer.
        if self.menu.is_none()
            && matches!(
                &event,
                ui_core::Event::KeyPressed {
                    key: ui_core::Key::Menu
                }
            )
            && self.open_menu_for_key()
        {
            return true;
        }
        // A node that captures keys turns them into chords instead of commands: a shortcut
        // editor must see the very chord it is about to bind.
        if let ui_core::Event::KeyPressed { key } = &event
            && let Some(id) = self.key_capture_owner()
        {
            // Tab still walks the focus ring; a capture that ate it would trap focus.
            if !matches!(key, ui_core::Key::Tab) {
                let chord = ui_core::KeyChord::new(*key, self.modifiers);
                let changed = self.app_changed(id, ui_core::change::Change::KeyChord(chord));
                return changed || before != (self.hovered, self.pressed, self.focused);
            }
        }
        // Shortcuts resolve before the key arms: a binding may claim a chord the plain
        // arms would read as navigation.
        if let ui_core::Event::KeyPressed { key } = &event
            && let Some((repaint, was_edit)) = self.fire_shortcut(*key)
        {
            let requested = repaint || was_edit;
            return requested || before != (self.hovered, self.pressed, self.focused);
        }
        // A focused formula editor owns the keyboard while it is open.
        if let Some(handled) = self.math_edit_event(&event) {
            return handled || before != (self.hovered, self.pressed, self.focused);
        }
        // A caret inside a document's formula is the formula's: its structure takes the
        // keys, and the document takes the resulting bytes.
        if let Some(handled) = self.math_structure_event(&event) {
            return handled || before != (self.hovered, self.pressed, self.focused);
        }
        if event.moves_pointer() {
            if let ui_core::Event::PointerMoved { pos } = &event {
                if self.scrollbar_drag.is_some() {
                    // A scrollbar drag owns the pointer while it lasts.
                    requested |= self.scrollbar_move(*pos);
                } else if self.pan.is_some() {
                    requested |= self.pan_move(*pos);
                } else if let Some((node, at, _)) = self.selection_drag {
                    // The press landed on the selection: once it travels, the selection is
                    // what is being dragged rather than reshaped.
                    if ui_core::past_threshold(at, *pos) {
                        self.selection_drag = None;
                        requested |= self.begin_text_drag(node, at);
                    }
                } else if self.select_drag.is_some() {
                    // A selection drag owns the pointer: it is not a control drag.
                    requested |= self.extend_selection(*pos);
                } else if self.editing_drag.is_some() {
                    // The same inside a text input: the drag selects, it does not scroll.
                    requested |= self.extend_editing(*pos);
                } else if let Some(ev) = self.drag.moved(*pos) {
                    requested |= self.report_drag(self.in_node_space(ev));
                } else if self.pan_candidate.is_some()
                    && self
                        .press_at
                        .is_some_and(|at| ui_core::past_threshold(at, *pos))
                {
                    // The press became a pan: from here the content follows the pointer.
                    requested |= self.begin_pan(*pos);
                } else if self
                    .press_at
                    .is_some_and(|at| ui_core::past_threshold(at, *pos))
                {
                    // A press with no drag owner still dies once it travels past the
                    // threshold: it is neither a click nor a drag, so releasing later must
                    // not activate the node the pointer has left behind.
                    self.pressed = None;
                    self.press_at = None;
                    self.pressed_path.clear();
                    self.end_splat();
                }
            }
            self.hovered = self.hit_at_pointer().and_then(|nid| self.hover_owner(nid));
            self.refresh_hovered_path();
        }
        match &event {
            ui_core::Event::Modifiers(m) => {
                self.modifiers = *m;
                // A shortcut editor previews the held modifiers, so it needs the repaint; so
                // does any tree that read them while building.
                if self.key_capture_owner().is_some() || self.modifier_read {
                    requested = true;
                }
            }
            ui_core::Event::PointerPressed { button, pos } => {
                // A new press cancels any candidate the previous one left behind, and ends
                // the wheel burst: the next wheel picks its scroller afresh.
                self.pan_candidate = None;
                self.wheel_lock = None;
                if *button == ui_core::PointerButton::Primary {
                    // Where this press stands in its burst, decided before anything it means.
                    self.pressed_clicks =
                        self.click_burst(ui_core::PointerButton::Primary, *pos);
                    // A press outside an open popup asks it to close: the light dismiss every
                    // flyout has, judged here because only the engine knows both rectangles.
                    self.dismissed = match self.menu.as_ref() {
                        Some(menu) => self.menu_dismissal(*pos, menu.depth()),
                        None => self.tree.popup_root().and_then(|nid| {
                            self.tree
                                .get(nid)
                                .filter(|n| !n.visual_rect().contains(*pos))
                                .map(|n| n.id)
                        }),
                    };
                    if self.dismissed.is_some() {
                        // The tap that dismisses belongs to the dismissal, as a light-dismiss
                        // layer takes it: it reaches nothing underneath and leaves no click.
                        self.pressed = None;
                        self.press_at = None;
                        self.pressed_path.clear();
                        requested = true;
                    } else if self.scrollbar_press(*pos) {
                        // The bar took the press: no click, no ink, no focus change.
                        requested = true;
                    } else {
                        let hit = self.hit_at_pointer();
                        if let Some(node) = hit
                            .and_then(|id| self.selectable_text(id))
                            .filter(|id| self.text_input_owner(*id).is_none())
                            .or_else(|| {
                                // The topmost thing under the pointer may be a container that
                                // wraps a selectable block; find it geometrically so a press on
                                // the block still starts or extends the selection.
                                self.selectable_block_at(*pos)
                                    .filter(|id| self.text_input_owner(*id).is_none())
                            })
                        {
                            // A selectable label takes the press for its selection only: it is
                            // not a control press, so it never splats ink or reports a click.
                            // A text view inside an input is not one: its press places the caret.
                            let inside = !self.modifiers.shift
                                && self
                                    .text_index_at(node, *pos)
                                    .is_some_and(|index| {
                                        self.span_of(node)
                                            .is_some_and(|span| span.covers(index))
                                    });
                            if inside {
                                // A press inside the selection is how the text leaves as a
                                // drag; a click that never travels collapses it on release.
                                let index = self
                                    .text_index_at(node, *pos)
                                    .expect("the press was just tested for membership");
                                self.selection_drag = Some((node, *pos, index));
                                self.select_drag = None;
                            } else {
                                self.selection_drag = None;
                                requested |=
                                    self.begin_selection(
                                        node,
                                        *pos,
                                        self.modifiers.shift,
                                        self.pressed_clicks,
                                    );
                            }
                            self.pressed = None;
                            self.press_at = None;
                            self.pressed_path.clear();
                            self.focus_visible = false;
                            self.editing = None;
                            self.editing_drag = None;
                            self.preedit = None;
                            self.reveal = None;
                            self.focused = hit.and_then(|id| {
                                self.tree.find_by_id(id).and_then(|nid| {
                                    self.ancestor_with_sense(nid, ui_core::Sense::FOCUSABLE)
                                })
                            });
                        } else {
                            // Pressing anywhere else drops a read-only selection.
                            if self.selection.take().is_some() {
                                self.selection_spans = Rc::from(Vec::new());
                                requested = true;
                            }
                            self.select_drag = None;
                            self.selection_drag = None;
                            self.pressed = hit;
                            self.press_at = Some(*pos);
                            // Pressing moves focus to the nearest node that subscribes to it; a
                            // press on inert ground clears focus instead of parking it on a box.
                            self.focus_visible = false;
                            self.focused = self.pressed.and_then(|id| {
                                self.tree.find_by_id(id).and_then(|nid| {
                                    self.ancestor_with_sense(nid, ui_core::Sense::FOCUSABLE)
                                })
                            });
                            self.refresh_pressed_path();
                            // A press inside an edited formula places its caret the same way.
                            if let Some(id) = self.focused
                                && self.math_edit_place_caret(id, *pos)
                            {
                                self.goal_x = None;
                                requested = true;
                            }
                            // A press inside a text input places the caret and opens a drag;
                            // the field itself still gets the click.
                            if let Some(owner) = hit.and_then(|id| self.text_input_owner(id))
                                && self.press_belongs_to_input(owner)
                                && let Some(index) = self.input_index_at(owner, *pos)
                            {
                                self.selection = None;
                                self.selection_spans = Rc::from(Vec::new());
                                self.goal_x = None;
                                self.end_typing_run(owner);
                                // A shift press keeps the anchor the caret came from, so it
                                // extends the field's selection the way it extends a block's.
                                let selection = match self.editing {
                                    Some((current, sel))
                                        if self.modifiers.shift && current == owner =>
                                    {
                                        ui_core::TextSelection {
                                            anchor: sel.anchor,
                                            caret: index,
                                        }
                                    }
                                    _ => ui_core::TextSelection::collapsed(index),
                                };
                                self.editing = Some((owner, selection));
                                self.editing_drag = Some(owner);
                                requested = true;
                            }
                            // Ink starts where the finger landed, not at the control's centre: the
                            // declaration says how much, the engine knows where.
                            self.begin_splat(*pos);
                            // A press inside a scroller may become a pan; only the distance
                            // decides, so a click on a row still works.
                            self.pan_candidate =
                                self.hit_at_pointer().and_then(|id| self.pan_target(id));
                            // Touch-down stops inertia: the content freezes under the finger,
                            // or the coast and the drag would both move it. That holds even when
                            // a slider or a selectable label owns the gesture and cannot pan.
                            let stopped = self
                                .pan_candidate
                                .or_else(|| hit.and_then(|h| self.enclosing_scroller(h)));
                            if let Some(id) = stopped {
                                self.end_motion(id);
                            }
                        }
                        if let Some(on) = self.drag_owner() {
                            self.drag.press(on, *pos, ui_core::PointerButton::Primary);
                        }
                    }
                } else if *button == ui_core::PointerButton::Secondary {
                    // A right click focuses what it lands on first, so a text field can offer
                    // its own rows; nothing else hears the press.
                    let hit = self
                        .hit
                        .topmost_consumer(*pos)
                        .and_then(|nid| self.tree.get(nid).map(|n| n.id));
                    self.focused = hit.and_then(|id| {
                        self.tree.find_by_id(id).and_then(|nid| {
                            self.ancestor_with_sense(nid, ui_core::Sense::FOCUSABLE)
                        })
                    });
                    self.focus_visible = false;
                    self.sync_caret();
                    // Selectable text takes a caret where it was hit, so the rows it offers
                    // have something to act on. No drag: a menu, not a selection, follows.
                    // A hit inside the existing selection keeps it — Copy is what the menu
                    // is for.
                    if let Some(node) = hit.and_then(|id| self.selectable_text(id))
                        && let Some(index) = self.text_index_at(node, *pos)
                    {
                        let kept = self.span_of(node).is_some_and(|span| span.covers(index));
                        let ends = SelectionEnds {
                            anchor: (node, index),
                            head: (node, index),
                        };
                        if !kept && self.selection != Some(ends) {
                            self.selection = Some(ends);
                            self.refresh_selection();
                            requested = true;
                        }
                    }
                    // A node that asked for the button reports it and waits: the menu it
                    // would have opened opens on a tap's release instead.
                    let owner = hit.and_then(|id| {
                        self.tree.find_by_id(id).and_then(|nid| {
                            self.ancestor_with_sense(nid, ui_core::Sense::SECONDARY)
                        })
                    });
                    // The secondary button starts a drag on a node that declared a payload
                    // even when the node never asked to hear the button: a file manager's
                    // right drag is the same gesture as its left drag, with the menu the
                    // tap would open held back until the release says it was not a drag.
                    // A gesture-only `DRAG` node is the pointer's on the left button;
                    // right-dragging a slider must not change its value.
                    let drag = hit.and_then(|id| self.drag_owner_from(id));
                    let transfer = drag.filter(|on| self.drag_has_payload(*on));
                    // With no declared menu the app owns the rows, so it hears where the
                    // tap landed; a node that declared one keeps the engine's menu. Either
                    // way the answer waits for the release: a right drag must open nothing.
                    let request = self.menu_for(*pos).is_none().then_some(hit).flatten();
                    match (owner, transfer) {
                        (Some(node), _) => {
                            let clicks =
                                self.click_burst(ui_core::PointerButton::Secondary, *pos);
                            self.secondary = Some(SecondaryPress {
                                node: Some(node),
                                at: *pos,
                                since: self.clock.live(),
                                clicks,
                                request,
                            });
                            if let Some(on) = drag {
                                self.drag.press(
                                    on,
                                    *pos,
                                    ui_core::PointerButton::Secondary,
                                );
                            }
                            requested |= self.app_changed(
                                node,
                                ui_core::change::Change::Secondary {
                                    down: true,
                                    clicks,
                                },
                            );
                        }
                        (None, Some(on)) => {
                            self.secondary = Some(SecondaryPress {
                                node: None,
                                at: *pos,
                                since: self.clock.live(),
                                clicks: 0,
                                request,
                            });
                            self.drag.press(on, *pos, ui_core::PointerButton::Secondary);
                        }
                        (None, None) => {
                            // Nothing can travel and nothing belongs to the button, so the
                            // press has no tap to wait for: the app or the menu answers now.
                            requested |= match request {
                                Some(node) => self.app_changed(
                                    node,
                                    ui_core::change::Change::MenuRequested(*pos),
                                ),
                                None => self.open_menu(*pos),
                            };
                        }
                    }
                } else if *button == ui_core::PointerButton::Middle {
                    // The engine has no gesture of its own on this button: a node that
                    // declared `MIDDLE` hears the press, and nothing else does.
                    let owner = self.hit.topmost_consumer(*pos).and_then(|nid| {
                        self.tree.get(nid).map(|n| n.id).and_then(|id| {
                            self.tree
                                .find_by_id(id)
                                .and_then(|nid| self.ancestor_with_sense(nid, ui_core::Sense::MIDDLE))
                        })
                    });
                    if let Some(node) = owner {
                        let clicks = self.click_burst(ui_core::PointerButton::Middle, *pos);
                        self.middle = Some((node, clicks));
                        requested |= self.app_changed(
                            node,
                            ui_core::change::Change::Middle {
                                down: true,
                                clicks,
                            },
                        );
                    }
                }
            }
            ui_core::Event::PointerReleased { button, pos } => {
                if *button == ui_core::PointerButton::Primary {
                    self.scrollbar_drag = None;
                    self.pan_candidate = None;
                    // The pan ends first: its speed becomes the fling's, and the press it
                    // came from is already gone.
                    let now = self.clock.live();
                    requested |= self.end_pan(now);
                    if let Some((node, _, index)) = self.selection_drag.take() {
                        // A click inside the selection with no travel collapses it where it
                        // landed; it was never a drag.
                        self.selection = Some(SelectionEnds {
                            anchor: (node, index),
                            head: (node, index),
                        });
                        self.refresh_selection();
                        requested = true;
                    }
                    if self.select_drag.take().is_some() {
                        // A selection drag ends here; it is neither a click nor a drag.
                        self.pressed = None;
                        self.press_at = None;
                        self.pressed_path.clear();
                    }
                    self.editing_drag = None;
                    let clicked = self.pressed.take();
                    let press_at = self.press_at.take();
                    self.pressed_path.clear();
                    // The ink outlives the press, but its growth stops here and the fade
                    // begins; waiting for the frame would delay it by one frame.
                    self.end_splat();
                    // The release is the last point of the pointer's path: a flick that
                    // produced no motion event still crosses the threshold here.
                    if !self.drag.is_dragging()
                        && let Some(ev) = self.drag.moved(*pos)
                    {
                        requested |= self.report_drag(self.in_node_space(ev));
                    }
                    // The drop target is what is under the pointer *now*, so it must be
                    // hit tested before the press state is cleared.
                    let target = self.drop_target();
                    match self.drag.release(target, *pos) {
                        Some(ev) => requested |= self.report_drag(self.in_node_space(ev)),
                        // No drag was live, so this was a click: report its identity.
                        None => {
                            // Released within the threshold of the press: a click.
                            // The engine only records the *fact*; what a click means is
                            // the control's to decide, so nothing is announced from here.
                            // Only a subscriber is told: the engine routes, it does not
                            // decide what a click means.
                            let stayed =
                                press_at.is_some_and(|at| !ui_core::past_threshold(at, *pos));
                            if let Some(id) = clicked.filter(|_| stayed) {
                                let subscribes = self
                                    .tree
                                    .find_by_id(id)
                                    .and_then(|nid| {
                                        self.ancestor_with_sense(nid, ui_core::Sense::CLICK)
                                    })
                                    .is_some();
                                if subscribes {
                                    self.clicked = Some(id);
                                    self.clicks = self.pressed_clicks;
                                }
                            }
                        }
                    }
                } else if *button == ui_core::PointerButton::Secondary {
                    // A drag that travelled is a drop, not a tap: it must land on the
                    // target under the pointer and must not also open the menu the tap
                    // would have.
                    let dragging = self.drag.is_dragging();
                    let target = if dragging { self.drop_target() } else { None };
                    if let Some(ev) = self.drag.release(target, *pos) {
                        requested |= self.report_drag(self.in_node_space(ev));
                    }
                    // The release ends a reported press and decides what the button meant:
                    // a tap keeps the menu the press withheld, a hold keeps only the report.
                    if let Some(press) = self.secondary.take() {
                        if let Some(node) = press.node {
                            requested |= self.app_changed(
                                node,
                                ui_core::change::Change::Secondary {
                                    down: false,
                                    clicks: press.clicks,
                                },
                            );
                        }
                        if !dragging && self.clock.live() - press.since < SECONDARY_TAP {
                            requested |= match press.request {
                                Some(node) => self.app_changed(
                                    node,
                                    ui_core::change::Change::MenuRequested(press.at),
                                ),
                                None => self.open_menu(press.at),
                            };
                        }
                    }
                } else if *button == ui_core::PointerButton::Middle {
                    // The node hears the release wherever the pointer has gone, as the
                    // secondary button does: the gesture is the node's, not the point's.
                    if let Some((node, clicks)) = self.middle.take() {
                        requested |= self.app_changed(
                            node,
                            ui_core::change::Change::Middle {
                                down: false,
                                clicks,
                            },
                        );
                    }
                }
            }
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Escape,
            } => {
                // A drag in flight owns Escape: cancelling it must not also drop focus.
                match self.drag.cancel() {
                    Some(ev) => {
                        // The app restores its start state on `Cancelled`, so the engine
                        // must hand the gesture back now: a `dragging` that outlives the
                        // cancel freezes every family gated on it.
                        requested |= self.report_drag(self.in_node_space(ev));
                        self.pressed = None;
                        self.press_at = None;
                        self.pressed_path.clear();
                        // A cancelled drag is not a tap: the menu its press withheld stays
                        // closed, even though the button is still down.
                        self.secondary = None;
                        self.end_splat();
                    }
                    None => self.focused = None,
                }
            }
            // Tab walks the focus ring; Shift reverses it.
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Tab,
            } => {
                // Keyboard focus may show a ring; pointer focus may not.
                self.focus_visible = true;
                self.focus_step(!self.modifiers.shift);
            }
            ui_core::Event::Scroll { pos, delta } => {
                // A scroll container the engine owns accumulates the wheel itself, clamped
                // to its content; any other scrollable reports the delta up as a semantic
                // value change, and the app keeps its own offset.
                if self.wheel_chain(*pos, *delta) {
                    requested = true;
                } else if let Some(id) = self.app_scroll_target(*pos)
                    && self.tree.find_by_id(id).is_some()
                {
                    requested |=
                        self.app_changed(id, ui_core::change::Change::Value(-delta.y as f64));
                }
            }
            // Text goes to the caret, and the app gets the whole new buffer, never a diff.
            ui_core::Event::Text(text) => {
                self.goal_x = None;
                match self.editing {
                    Some((owner, sel)) => {
                        if let Some(value) = self.text_input_value(owner) {
                            // Only a lone cluster typed at a caret continues a run; anything
                            // larger, or a replacement of a selection, is its own user action.
                            let typing = sel.is_collapsed()
                                && ui_core::grapheme::next(text, 0) == text.len();
                            if let Some(applied) =
                                self.edit_structural(owner, &value, sel.range(), text, typing)
                            {
                                edited = true;
                                requested |= applied;
                            }
                        }
                    }
                    // Nothing is typing, so a lone letter may be a command: a surface with
                    // no text context binds plain keys for what the mouse would do.
                    None => {
                        if let Some(key) = typed_key(text)
                            && let Some((changed, _)) = self.fire_shortcut(key)
                        {
                            requested |= changed;
                        }
                    }
                }
            }
            // A paste is one edit over the selection, exactly like typing one character.
            ui_core::Event::Paste(content) => {
                match content.text().filter(|_| self.editing.is_some()) {
                    Some(text) => {
                        self.goal_x = None;
                        if let Some((owner, sel)) = self.editing
                            && let Some(value) = self.text_input_value(owner)
                        {
                            // A one-line input has nowhere to put a break; a multiline one
                            // keeps it, normalised so the buffer holds only LF.
                            let text = if self.input_is_multiline(owner) {
                                text.replace("\r\n", "\n").replace('\r', "\n")
                            } else {
                                text.replace("\r\n", " ").replace(['\r', '\n'], " ")
                            };
                            if let Some(applied) =
                                self.edit_structural(owner, &value, sel.range(), &text, false)
                            {
                                edited = true;
                                requested |= applied;
                            }
                        }
                    }
                    // No caret took it, so the whole payload belongs to the app: a file
                    // list has no text for an editor to insert.
                    None => match self.paste_target.take() {
                        Some(target) => {
                            requested |= self.app_changed(
                                target,
                                ui_core::change::Change::Paste(content.clone()),
                            );
                        }
                        None => return false,
                    },
                }
            }
            // The composition lives in the engine: the app's buffer stays committed-only.
            ui_core::Event::Preedit(preedit) => {
                if let Some((owner, sel)) = self.editing {
                    let next = preedit
                        .as_ref()
                        .filter(|p| !p.is_empty())
                        .map(|p| (owner, p.clone()));
                    if self.preedit != next {
                        // Reveal freezes where the composition found it: an input method
                        // cancels when the bytes it replaces change shape.
                        self.reveal = match (&next, &self.preedit) {
                            (Some(_), None) => Some((owner, sel)),
                            (None, _) => None,
                            (Some(_), Some(_)) => self.reveal,
                        };
                        self.preedit = next;
                        requested = true;
                    }
                }
            }
            ui_core::Event::DeleteSurrounding { before, after } => {
                if let Some((owner, sel)) = self.editing
                    && let Some(value) = self.text_input_value(owner)
                {
                    // The protocol counts bytes; widen to whole clusters so a split
                    // surrogate-free sequence is never left half deleted.
                    let start = ui_core::grapheme::snap(&value, sel.caret.saturating_sub(*before));
                    let end = ui_core::grapheme::ceil(&value, sel.caret.saturating_add(*after));
                    let (next, caret) = ui_core::text_input::delete_range(&value, start..end);
                    edited = true;
                    requested |= self.apply_edit(
                        owner,
                        next,
                        ui_core::TextSelection::collapsed(caret),
                        false,
                    );
                }
            }
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Backspace,
            } => {
                self.goal_x = None;
                if let Some((owner, sel)) = self.editing
                    && let Some(value) = self.text_input_value(owner)
                {
                    let range = if sel.is_collapsed() {
                        let hidden = self.input_hidden_ranges(owner, &value, sel.caret);
                        if self.modifiers.shift {
                            // Ctrl+Backspace removes the whole word behind the caret.
                            let from = ui_core::text_input::step_word(&value, sel.caret, -1);
                            let from = over_hidden(&hidden, from..sel.caret).start;
                            from..sel.caret
                        } else {
                            let from = ui_core::grapheme::prev(&value, sel.caret);
                            let from = over_hidden(&hidden, from..sel.caret).start;
                            from..sel.caret
                        }
                    } else {
                        sel.range()
                    };
                    if let Some(applied) = self.edit_structural(owner, &value, range, "", false) {
                        edited = true;
                        requested |= applied;
                    }
                }
            }
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Delete,
            } => {
                self.goal_x = None;
                if let Some((owner, sel)) = self.editing
                    && let Some(value) = self.text_input_value(owner)
                {
                    let range = if sel.is_collapsed() {
                        let hidden = self.input_hidden_ranges(owner, &value, sel.caret);
                        if self.modifiers.shift {
                            // Ctrl+Delete removes the whole word ahead of the caret.
                            let to = ui_core::text_input::step_word(&value, sel.caret, 1);
                            let to = over_hidden(&hidden, sel.caret..to).end;
                            sel.caret..to
                        } else {
                            let to = ui_core::grapheme::next(&value, sel.caret);
                            let to = over_hidden(&hidden, sel.caret..to).end;
                            sel.caret..to
                        }
                    } else {
                        sel.range()
                    };
                    if let Some(applied) = self.edit_structural(owner, &value, range, "", false) {
                        edited = true;
                        requested |= applied;
                    }
                }
            }
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Left,
            } => {
                if self.editing.is_some() {
                    requested |= self.step_editing(-1);
                } else {
                    self.focus_spatial(Direction::Left);
                }
            }
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Right,
            } => {
                if self.editing.is_some() {
                    requested |= self.step_editing(1);
                } else {
                    self.focus_spatial(Direction::Right);
                }
            }
            // Home, End and the Ctrl/Cmd commands live in the shortcut table, so an app can
            // rebind them by id; only plain navigation stays here.
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Up,
            } => match self.step_line_editing(-1) {
                Some(moved) => requested |= moved,
                None => {
                    self.focus_spatial(Direction::Up);
                }
            },
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Down,
            } => match self.step_line_editing(1) {
                Some(moved) => requested |= moved,
                None => {
                    self.focus_spatial(Direction::Down);
                }
            },
            // Home and End jump to the ends of an open popup's list; an input keeps them for
            // the shortcut table, which is where its line editing lives.
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Home,
            } => {
                if self.editing.is_none() {
                    requested |= self.focus_popup_edge(false);
                }
            }
            ui_core::Event::KeyPressed {
                key: ui_core::Key::End,
            } => {
                if self.editing.is_none() {
                    requested |= self.focus_popup_edge(true);
                }
            }
            ui_core::Event::KeyPressed {
                key: ui_core::Key::Enter,
            }
            | ui_core::Event::KeyPressed {
                key: ui_core::Key::Space,
            } => {
                // A space inside a text input is text, already delivered as `Text`.
                if matches!(
                    event,
                    ui_core::Event::KeyPressed {
                        key: ui_core::Key::Space
                    }
                ) && self.editing.is_some()
                {
                    return requested;
                }
                // Shift+Enter in a declared multiline input is a hard break, one whole-buffer
                // edit like typing; a document input takes a plain Enter the same way.
                // Single-line fields never reach it and fall through.
                if matches!(
                    event,
                    ui_core::Event::KeyPressed {
                        key: ui_core::Key::Enter
                    }
                ) && self.editing.is_some_and(|(owner, _)| {
                    self.input_is_multiline(owner)
                        && (self.modifiers.shift || self.input_enter_inserts_break(owner))
                }) {
                    let (owner, sel) = self.editing.expect("checked above");
                    if let Some(value) = self.text_input_value(owner) {
                        let (value, at) = ui_core::text_input::delete_range(&value, sel.range());
                        let (next, caret) = ui_core::text_input::insert(&value, at, "\n");
                        requested |= self.apply_edit(
                            owner,
                            next,
                            ui_core::TextSelection::collapsed(caret),
                            false,
                        );
                        // Return instead of falling through: `Key::Enter` would otherwise
                        // activate the node, and this edit already owns the caret.
                        return requested || before != (self.hovered, self.pressed, self.focused);
                    }
                }
                // The engine owns keyboard translation: the app never sees a key code.
                // Keyboard activation is a focus event: only a focused, live node hears it,
                // and only inside the modal that owns the keyboard.
                if let Some(id) = self.focused.filter(|id| self.in_key_scope(*id)) {
                    let live = self
                        .tree
                        .find_by_id(id)
                        .and_then(|nid| self.tree.get(nid))
                        .map(|n| !n.disabled)
                        .unwrap_or(false);
                    if live {
                        // The control's `change()` turns this into its own semantic change, as
                        // it does for a click; reporting it here too would deliver it twice.
                        self.activated = Some(id);
                        // The fact lives for exactly one frame, so that frame must run.
                        requested = true;
                    }
                }
            }
            _ => {}
        }
        if !edited {
            self.sync_caret();
        }
        // Handled when anything asked for a repaint or the routing changed; there is no
        // raw-event hook for the app to answer.
        requested || before != (self.hovered, self.pressed, self.focused)
    }

    /// Recompute the pressed node's ancestor chain from the last laid-out frame.
    fn refresh_pressed_path(&mut self) {
        self.pressed_path.clear();
        let Some(id) = self.pressed else { return };
        let Some(nid) = self.tree.find_by_id(id) else {
            return;
        };
        let mut cur = Some(nid);
        while let Some(node) = cur {
            let Some(n) = self.tree.get(node) else { break };
            self.pressed_path.push(n.id);
            cur = n.parent;
        }
        self.pressed_path.reverse();
    }

    /// Recompute the hovered node's ancestor chain from the last laid-out frame.
    fn refresh_hovered_path(&mut self) {
        self.hovered_path.clear();
        self.pointer_path.clear();
        let Some(nid) = self.pointer.and_then(|p| self.hit.topmost_consumer(p)) else {
            return;
        };
        let mut chain = Vec::new();
        let mut cur = Some(nid);
        while let Some(node) = cur {
            let Some(n) = self.tree.get(node) else {
                break;
            };
            // Only subscribers are on the path, so `hovered_within` answers the same
            // question the declaration did.
            self.pointer_path.push(n.id);
            if n.sense.contains(ui_core::Sense::HOVER) {
                chain.push(n.id);
            }
            cur = n.parent;
        }
        chain.reverse();
        self.pointer_path.reverse();
        self.hovered_path = chain;
    }

    /// Where a wheel goes when the engine owns no scroller on the way up: the nearest
    /// ancestor that asked the app to hold its offset.
    ///
    /// The wheel lands on whatever is on top — a row, a label — never on the viewport that
    /// would actually move, so the raw hit target is not an answer. A node that asked for
    /// nothing gets nothing: reporting a `Value` to page chrome made every wheel step over
    /// empty space a semantic change and threw away the declaration cache with it.
    fn app_scroll_target(&self, pos: ui_core::geom::Point) -> Option<Id> {
        let mut current = self.hit.topmost_consumer(pos);
        while let Some(nid) = current {
            let Some(node) = self.tree.get(nid) else {
                break;
            };
            // The engine took this one's wheel already; its offset is not the app's to set.
            if node.sense.contains(ui_core::Sense::SCROLL_OWNED) {
                return None;
            }
            // Two ways to be a scroll container: the layout clip (overflow scrollable) or
            // the subscription bit. A control declares the bit; a container may set either.
            let scrolls_by_layout = node
                .data
                .params()
                .map(|p| p.clip.scrolls())
                .unwrap_or(false);
            if scrolls_by_layout || node.sense.contains(ui_core::Sense::SCROLL) {
                return Some(node.id);
            }
            current = node.parent;
        }
        None
    }

    /// Accumulate a wheel step into a scroll container the engine owns, on whichever axes
    /// its clip scrolls.
    ///
    /// Returns `None` when `id` is not an engine-owned scroller; otherwise the part of the
    /// step the container could not take, so the caller can hand it to the next ancestor.
    fn scroll_owned_delta(
        &mut self,
        id: Id,
        delta: ui_core::geom::Point,
    ) -> Option<ui_core::geom::Point> {
        let Some(nid) = self.tree.find_by_id(id) else {
            return None;
        };
        let Some(node) = self.tree.get(nid) else {
            return None;
        };
        if !node.sense.contains(ui_core::Sense::SCROLL_OWNED) {
            return None;
        }
        let snap = node.data.params().map(|p| p.snap).unwrap_or([0.0; 2]);
        let Some((clip, extent, viewport)) = self.scroll_range(id) else {
            return None;
        };
        // The stored offset is the source of truth across frames; the tree's own fields are
        // re-applied from it each frame and never read back.
        let current = self.scroll_offsets.get(&id).copied().unwrap_or_default();
        let mut next = current;
        // What is left of the step after this container took its share; the parent gets it.
        let mut rest = delta;
        let scrolls_x = clip.x == ui_core::layout::Overflow::Scroll;
        let scrolls_y = clip.y == ui_core::layout::Overflow::Scroll;
        let overflow_x = (extent.w - viewport.w).max(0.0);
        let overflow_y = (extent.h - viewport.h).max(0.0);
        if self.modifiers.shift {
            // Shift turns the wheel sideways, the convention a mouse expects. A step spent
            // here is not passed on, or the page behind would move at the same time.
            let step = if delta.x != 0.0 { delta.x } else { delta.y };
            if scrolls_x {
                next.x = (current.x + step).clamp(0.0, overflow_x);
                rest = ui_core::geom::Point::ZERO;
            } else {
                // Nothing sideways here: hand the horizontal step to the ancestor that has it.
                rest = ui_core::geom::Point::new(step, 0.0);
            }
        } else if scrolls_x && !scrolls_y {
            // A strip that scrolls only sideways takes the upright wheel: a mouse has one
            // wheel, and making the user hold a modifier to reach it is the wart this
            // removes. A device that does report x keeps its diagonal split, so a trackpad
            // gesture can still move the strip and the page at once.
            if delta.x != 0.0 {
                next.x = (current.x + delta.x).clamp(0.0, overflow_x);
                rest.x -= next.x - current.x;
            } else if delta.y != 0.0 {
                next.x = (current.x + delta.y).clamp(0.0, overflow_x);
                rest = ui_core::geom::Point::ZERO;
            }
        } else {
            // A trackpad's own sideways gesture needs no modifier.
            if scrolls_x && delta.x != 0.0 {
                next.x = (current.x + delta.x).clamp(0.0, overflow_x);
                rest.x -= next.x - current.x;
            }
            if scrolls_y {
                next.y = (current.y + delta.y).clamp(0.0, overflow_y);
                rest.y -= next.y - current.y;
            }
        }
        self.scroll_offsets.insert(id, next);
        // A snapping axis rests when the burst goes quiet, not per event: a trackpad sends
        // dozens of small steps and a mouse one big one, and both are one gesture.
        if next != current && snap.iter().any(|page| *page > 0.0) {
            let now = self.frame_now;
            let origin = match self.snap_pending.get(&id) {
                Some(burst) if now - burst.at <= SNAP_IDLE => burst.origin,
                _ => current,
            };
            self.snap_pending.insert(id, SnapBurst { origin, at: now });
        }
        // A wheel takes the scroller back: a coasting or bouncing offset must not fight it.
        self.end_motion(id);
        // Even a no-op wheel counts as activity: the auto scrollbar should flash on.
        self.scrolled_since.insert(id);
        Some(rest)
    }

    /// Send a wheel step to the scrollers under `pos`, innermost first, passing on what each
    /// one could not take. Returns whether any of them moved.
    ///
    /// A wheel over an inner list that has reached its end must still move the page behind
    /// it: a hit test alone stops at the inner container and the gesture dies there.
    fn wheel_chain(&mut self, pos: ui_core::geom::Point, delta: ui_core::geom::Point) -> bool {
        // A wheel has no phases, so the lock is the pointer staying put: the content moves
        // under a resting pointer, the node under it changes, and the gesture still belongs
        // to the scroller it started on.
        let start = match self.wheel_lock {
            Some((id, at)) if at == pos && self.tree.find_by_id(id).is_some() => Some(id),
            _ => self.wheel_start(pos),
        };
        self.wheel_lock = start.map(|id| (id, pos));
        let Some(mut id) = start else {
            return false;
        };
        let mut rest = delta;
        let mut moved = false;
        loop {
            let Some(nid) = self.tree.find_by_id(id) else {
                break;
            };
            let Some(node) = self.tree.get(nid) else {
                break;
            };
            // An app-owned `SCROLL` container is a boundary: it takes the `Value` itself,
            // so the engine must not also move what is behind it.
            if node.sense.contains(ui_core::Sense::SCROLL)
                && !node.sense.contains(ui_core::Sense::SCROLL_OWNED)
            {
                break;
            }
            let parent = node.parent;
            if let Some(left) = self.scroll_owned_delta(id, rest) {
                moved |= left != rest;
                rest = left;
                // Nothing left for an ancestor to take in either axis.
                if rest.x.abs() < 0.01 && rest.y.abs() < 0.01 {
                    break;
                }
            }
            let Some(parent) = parent.and_then(|p| self.tree.get(p)) else {
                break;
            };
            id = parent.id;
        }
        moved
    }

    /// The engine-owned scroller a wheel at `pos` starts from, or `None` when an app-owned
    /// `SCROLL` container sits between the hit and every engine scroller.
    fn wheel_start(&self, pos: ui_core::geom::Point) -> Option<Id> {
        let mut current = self.hit.topmost_consumer(pos);
        while let Some(nid) = current {
            let Some(node) = self.tree.get(nid) else {
                break;
            };
            if node.sense.contains(ui_core::Sense::SCROLL_OWNED) {
                return Some(node.id);
            }
            if node.sense.contains(ui_core::Sense::SCROLL) {
                return None;
            }
            current = node.parent;
        }
        None
    }

    /// Move every engine-owned scroller above `id`, innermost first, the least amount that
    /// shows the node whole on the axes they scroll.
    fn bring_into_view(&mut self, id: Id) -> bool {
        let Some(nid) = self.tree.find_by_id(id) else {
            return false;
        };
        let Some(rect) = self.tree.get(nid).map(|n| n.rect) else {
            return false;
        };
        self.bring_rect_into_view(id, rect)
    }

    /// The same walk from an arbitrary rect in `from`'s layout space, so a caller can name
    /// a line inside a text node instead of the node's whole box.
    ///
    /// One scroller is not enough: a row inside a panel that is itself scrolled out of the
    /// window is not visible because the panel's offset says so, not the panel's list. Each
    /// move shifts the node inside the next scroller, so the walk carries that displacement
    /// with it — the rects in the tree are still those of the layout that just ran.
    fn bring_rect_into_view(&mut self, from: Id, rect: Rect) -> bool {
        let Some(nid) = self.tree.find_by_id(from) else {
            return false;
        };
        let mut cur = self.tree.get(nid).and_then(|n| n.parent);
        // How far the node has moved, in paint space, from the scrollers already adjusted.
        let mut moved = ui_core::geom::Point::ZERO;
        let mut changed = false;
        while let Some(p) = cur {
            let Some(parent) = self.tree.get(p) else {
                break;
            };
            cur = parent.parent;
            if !parent.sense.contains(ui_core::Sense::SCROLL_OWNED) {
                continue;
            }
            let Some((clip, extent, viewport)) = self.scroll_range(parent.id) else {
                continue;
            };
            let scroller = parent.id;
            let pad = parent.box_model.padding;
            let box_rect = parent.rect;
            // What this layout actually applied, which is not what the stored table says
            // when an earlier request in this same frame already moved it: the rects still
            // sit at the applied offset, so the content coordinate must be built from it.
            let applied = parent
                .data
                .params()
                .map(|p| ui_core::cx::ScrollOffset {
                    x: p.scroll_x,
                    y: p.scroll_y,
                })
                .unwrap_or_default();
            // The offset the viewport will show after every request so far; a scroller the
            // engine has never moved keeps whatever the app declared.
            let intended = self
                .scroll_offsets
                .get(&scroller)
                .copied()
                .unwrap_or(applied);
            // Content space measured from the unshifted origin: the laid-out rect already
            // carries the applied offset, so the scroller's own is added back.
            let left = rect.min_x() + moved.x - (box_rect.min_x() + pad.left) + applied.x;
            let top = rect.min_y() + moved.y - (box_rect.min_y() + pad.top) + applied.y;
            let mut next = intended;
            if clip.x == ui_core::layout::Overflow::Scroll {
                next.x = Self::reveal(left, left + rect.size.w, intended.x, viewport.w, extent.w);
            }
            if clip.y == ui_core::layout::Overflow::Scroll {
                next.y = Self::reveal(top, top + rect.size.h, intended.y, viewport.h, extent.h);
            }
            if next == intended {
                continue;
            }
            self.scroll_offsets.insert(scroller, next);
            self.scrolled_since.insert(scroller);
            // Moving this scroller by `d` moves the content, and so the node, by `-d`.
            moved.x -= next.x - intended.x;
            moved.y -= next.y - intended.y;
            changed = true;
        }
        changed
    }

    /// Keep the focused input's caret line inside every engine-owned scroller above it.
    ///
    /// A text leaf draws a whole block, or a whole document, so its rect names an edge of
    /// the fragment rather than the line the caret is on. The line box comes from the same
    /// text pass that paints the caret, and the scroll walk then runs unchanged from it.
    ///
    /// Only a caret that moved, or a buffer that changed, asks for this: a view the user
    /// scrolled by hand must stay where the hand left it while the caret is still.
    fn reveal_caret(&mut self) -> bool {
        let Some((owner, selection)) = self.editing else {
            self.caret_revealed = None;
            return false;
        };
        let Some(digest) = self.input_digest(owner) else {
            return false;
        };
        let caret = selection.caret;
        if self.caret_revealed == Some((owner, caret, digest)) {
            return false;
        }
        let Some((view, rect)) = self.caret_line_box(owner, caret) else {
            // Not measurable this frame (first focus, or a composition rewriting the
            // shown text); the next frame retries because nothing was recorded.
            return false;
        };
        self.caret_revealed = Some((owner, caret, digest));
        // One line of context above and below, so the caret stops against the clip edge
        // only when the document itself ends there. A press landed where the user pointed,
        // so it takes the plain minimum instead of shifting the content under the click.
        let rect = if self.pressed.is_some() {
            rect
        } else {
            let margin = rect.size.h * CARET_REVEAL_MARGIN_LINES;
            Rect::new(
                rect.origin.x,
                rect.origin.y - margin,
                rect.size.w,
                rect.size.h + margin * 2.0,
            )
        };
        self.bring_rect_into_view(view, rect)
    }

    /// The caret's line box in `owner`'s layout space, and the view that draws it.
    fn caret_line_box(&mut self, owner: Id, caret: usize) -> Option<(Id, Rect)> {
        let (view, local) = match self.input_views(owner) {
            Some(views) => {
                let view = view_of(&views, caret).or_else(|| nearest_view(&views, caret, 1))?;
                (view.node, caret.saturating_sub(view.base).min(view.len))
            }
            None => {
                let value = self.text_input_value(owner)?;
                (self.input_text_node(owner, &value, None, caret)?, caret)
            }
        };
        let prim = self.text_prim_of(view)?;
        let rect = self
            .text
            .borrow_mut()
            .caret_rect(&prim, local, self.scale)?;
        let node = self.tree.get(self.tree.find_by_id(view)?)?;
        // `text_prim_of` measures from the painted rect; the scroll walk uses layout rects,
        // which are the ones that already carry this frame's offsets.
        Some((
            view,
            rect.translate(node.rect.origin - node.visual_rect().origin),
        ))
    }

    /// Whether `id` names a text input, whose node spans its whole buffer.
    fn is_text_input(&self, id: Id) -> bool {
        self.tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))
            .and_then(|node| node.data.params())
            .is_some_and(|params| params.text_input.is_some())
    }

    /// A digest of an input's buffer, so an edit that leaves the caret offset alone still
    /// re-measures the line the bytes under it moved to.
    fn input_digest(&self, owner: Id) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let nid = self.tree.find_by_id(owner)?;
        let value = self.tree.get(nid)?.data.params()?.text_input.as_deref()?;
        let mut hasher = ui_core::collections::FastHasher::default();
        value.hash(&mut hasher);
        Some(hasher.finish())
    }

    /// The offset that shows `[start, end]` inside a viewport `view` long, moving the current
    /// offset as little as possible and never past the content's end.
    fn reveal(start: f32, end: f32, current: f32, view: f32, extent: f32) -> f32 {
        let mut offset = current;
        if start < offset {
            offset = start;
        } else if end > offset + view {
            offset = end - view;
        }
        offset.clamp(0.0, (extent - view).max(0.0))
    }

    fn scroll_owner_by(&mut self, from: Id, dx: f32, dy: f32) -> bool {
        let mut cur = self.tree.find_by_id(from);
        while let Some(nid) = cur {
            let Some(node) = self.tree.get(nid) else {
                return false;
            };
            if node.sense.contains(ui_core::Sense::SCROLL_OWNED) && Self::scrolls(node) {
                let id = node.id;
                return self.scroll_owner(id, dx, dy);
            }
            cur = node.parent;
        }
        false
    }

    /// Move the scroller `id` by this much; a request that names a node which is not an
    /// engine-owned scroller moves nothing, rather than guessing at one.
    fn scroll_owner(&mut self, id: Id, dx: f32, dy: f32) -> bool {
        let Some(nid) = self.tree.find_by_id(id) else {
            return false;
        };
        let Some(node) = self.tree.get(nid) else {
            return false;
        };
        if !node.sense.contains(ui_core::Sense::SCROLL_OWNED) || !Self::scrolls(node) {
            return false;
        }
        let Some((clip, extent, viewport)) = self.scroll_range(id) else {
            return false;
        };
        let current = self.scroll_offsets.get(&id).copied().unwrap_or_default();
        let mut next = current;
        if clip.x == ui_core::layout::Overflow::Scroll {
            next.x = (current.x + dx).clamp(0.0, (extent.w - viewport.w).max(0.0));
        }
        if clip.y == ui_core::layout::Overflow::Scroll {
            next.y = (current.y + dy).clamp(0.0, (extent.h - viewport.h).max(0.0));
        }
        if next == current {
            return false;
        }
        self.start_settle(id, next.x, next.y);
        self.scrolled_since.insert(id);
        true
    }

    /// Whether this node clips and scrolls at least one axis.
    fn scrolls(node: &ui_core::tree::Node) -> bool {
        let clip = node.data.params().map(|p| p.clip).unwrap_or_default();
        clip.x == ui_core::layout::Overflow::Scroll || clip.y == ui_core::layout::Overflow::Scroll
    }

    /// What a stored offset may be for `id`: the axes it scrolls, its content extent, and
    /// its viewport.
    ///
    /// The viewport is the content box, not the border box: padding is not part of what the
    /// content travels through, and five call sites used to read the two differently.
    fn scroll_range(&self, id: Id) -> Option<(ui_core::layout::Clip, Size, Size)> {
        let node = self.tree.get(self.tree.find_by_id(id)?)?;
        let params = node.data.params()?;
        let pad = node.box_model.padding;
        let viewport = Size::new(
            (node.rect.size.w - pad.left - pad.right).max(0.0),
            (node.rect.size.h - pad.top - pad.bottom).max(0.0),
        );
        Some((
            params.clip,
            node.scroll_extent.unwrap_or(Size::ZERO),
            viewport,
        ))
    }

    /// `value` pulled inside `[0, extent - viewport]` on an axis that scrolls; an axis that
    /// does not scroll keeps whatever was asked for.
    fn in_range(
        value: f32,
        overflow: ui_core::layout::Overflow,
        extent: f32,
        viewport: f32,
    ) -> f32 {
        if overflow == ui_core::layout::Overflow::Scroll {
            value.clamp(0.0, (extent - viewport).max(0.0))
        } else {
            value
        }
    }

    /// The scrollers whose offset may leave their range: a pan's rubber band, or the spring
    /// returning it. Clamping those would make the two invisible.
    fn elastic_scrollers(&self) -> FastSet<Id> {
        self.pan
            .iter()
            .flat_map(|p| p.links.iter().map(|link| link.id))
            .chain(self.scroll_motion.keys().copied())
            .collect()
    }

    /// Bring the stored offsets back inside the range this frame's layout gives them, and
    /// forget the ones whose scroller is gone.
    ///
    /// The offset map is the source of truth for the view, but layout is what knows how much
    /// content there is. Shortening a list left the offset past its new end: the content was
    /// drawn clamped while the map, the scrollbar and `scroll_of` still reported the old
    /// position, and the next wheel step moved nothing.
    fn reconcile_scroll_offsets(&mut self) {
        let elastic = self.elastic_scrollers();
        let ids: Vec<Id> = self.scroll_offsets.keys().copied().collect();
        for id in ids {
            if self.tree.find_by_id(id).is_none() {
                self.scroll_offsets.remove(&id);
                continue;
            }
            if elastic.contains(&id) {
                continue;
            }
            let Some((clip, extent, viewport)) = self.scroll_range(id) else {
                continue;
            };
            let Some(offset) = self.scroll_offsets.get_mut(&id) else {
                continue;
            };
            if clip.x == ui_core::layout::Overflow::Scroll {
                offset.x = offset.x.clamp(0.0, (extent.w - viewport.w).max(0.0));
            }
            if clip.y == ui_core::layout::Overflow::Scroll {
                offset.y = offset.y.clamp(0.0, (extent.h - viewport.h).max(0.0));
            }
        }
    }

    /// Whether layout left any scroller with a different content box or viewport than the
    /// one the build read.
    ///
    /// A windowed list declares the rows its scroller's viewport reaches, and it reads that
    /// viewport before layout runs. A frame that grows or shrinks one — a short directory
    /// replaced by a long one behind a scroller that hugs its content, say — has declared
    /// the rows of the box it used to be, so it owes the frame that windows again.
    fn scroller_boxes_moved(&mut self) -> bool {
        let mut now: FastMap<Id, (Size, Size)> = FastMap::default();
        let mut moved = false;
        for nid in self.tree.walk() {
            let Some(node) = self.tree.get(nid) else {
                continue;
            };
            if !node.sense.contains(ui_core::Sense::SCROLL_OWNED) {
                continue;
            }
            let pad = node.box_model.padding;
            let box_now = (
                node.scroll_extent.unwrap_or(Size::ZERO),
                Size::new(
                    (node.rect.size.w - pad.left - pad.right).max(0.0),
                    (node.rect.size.h - pad.top - pad.bottom).max(0.0),
                ),
            );
            moved |= self.scroller_boxes.get(&node.id) != Some(&box_now);
            now.insert(node.id, box_now);
        }
        self.scroller_boxes = now;
        moved
    }

    /// The scrollbar under `pos`: its scroller, the axis it drags, and that scroller's
    /// visual rect.
    fn scrollbar_at(
        &self,
        pos: ui_core::geom::Point,
    ) -> Option<(
        Id,
        ui_core::ScrollAxis,
        ui_core::ScrollbarBar,
        ui_core::geom::Rect,
    )> {
        let mut current = self.hit.topmost_consumer(pos);
        while let Some(nid) = current {
            let node = self.tree.get(nid)?;
            if let Some(bar) = node.data.params().and_then(|p| p.scrollbar) {
                // A bar that has faded out is not there to grab: its pixels and its press
                // target leave together.
                if self.bar_opacity(node.id, &bar) <= 0.002 {
                    current = node.parent;
                    continue;
                }
                let rect = node.visual_rect();
                let local = ui_core::geom::Point::new(pos.x - rect.origin.x, pos.y - rect.origin.y);
                for (axis, piece) in bar.bars() {
                    let inside = local.x >= piece.track.min_x()
                        && local.x <= piece.track.max_x()
                        && local.y >= piece.track.min_y()
                        && local.y <= piece.track.max_y();
                    if inside {
                        return Some((node.id, axis, piece, rect));
                    }
                }
            }
            current = node.parent;
        }
        None
    }

    /// Take a primary press on a scrollbar as a thumb drag; a press outside the thumb
    /// jumps it there first. Returns whether the press belonged to a scrollbar.
    fn scrollbar_press(&mut self, pos: ui_core::geom::Point) -> bool {
        let Some((id, axis, bar, rect)) = self.scrollbar_at(pos) else {
            return false;
        };
        let Some((_, extent, viewport)) = self.scroll_range(id) else {
            return false;
        };
        let horizontal = axis == ui_core::ScrollAxis::Horizontal;
        let (extent, viewport) = if horizontal {
            (extent.w, viewport.w)
        } else {
            (extent.h, viewport.h)
        };
        let overflow = (extent - viewport).max(0.0);
        let (track, thumb) = if horizontal {
            (bar.track.size.w, bar.thumb.size.w)
        } else {
            (bar.track.size.h, bar.thumb.size.h)
        };
        let travel = (track - thumb).max(0.0);
        if overflow <= 0.0 || travel <= 0.0 {
            return false;
        }
        // The thumb is now the pointer's: a coast still running would overwrite every
        // position this drag computes, and the offset would jump back each frame.
        self.end_motion(id);
        let local = if horizontal {
            pos.x - rect.origin.x
        } else {
            pos.y - rect.origin.y
        };
        let (thumb_start, thumb_end) = if horizontal {
            (bar.thumb.min_x(), bar.thumb.max_x())
        } else {
            (bar.thumb.min_y(), bar.thumb.max_y())
        };
        let mut offset = self.scroll_offsets.get(&id).copied().unwrap_or_default();
        if local < thumb_start || local > thumb_end {
            let next = ((local - thumb * 0.5) / travel * overflow).clamp(0.0, overflow);
            if horizontal {
                offset.x = next;
            } else {
                offset.y = next;
            }
            self.scroll_offsets.insert(id, offset);
        }
        let start_offset = if horizontal { offset.x } else { offset.y };
        self.scrollbar_drag = Some(ScrollbarDrag {
            id,
            axis,
            press: local,
            start_offset,
            travel,
            overflow,
        });
        self.pressed = None;
        self.press_at = None;
        self.pressed_path.clear();
        true
    }

    /// Move the dragged thumb with the pointer; returns whether a drag was live.
    fn scrollbar_move(&mut self, pos: ui_core::geom::Point) -> bool {
        let Some(drag) = self.scrollbar_drag.as_ref() else {
            return false;
        };
        let id = drag.id;
        let horizontal = drag.axis == ui_core::ScrollAxis::Horizontal;
        // The press was recorded in the scroller's own space, so the move has to be too.
        let Some(origin) = self
            .tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))
            .map(|node| node.visual_rect().origin)
        else {
            return false;
        };
        let local = if horizontal {
            pos.x - origin.x
        } else {
            pos.y - origin.y
        };
        let next = (drag.start_offset + (local - drag.press) / drag.travel * drag.overflow)
            .clamp(0.0, drag.overflow);
        let mut offset = self.scroll_offsets.get(&id).copied().unwrap_or_default();
        if horizontal {
            offset.x = next;
        } else {
            offset.y = next;
        }
        self.scroll_offsets.insert(id, offset);
        self.scrolled_since.insert(id);
        true
    }

    /// The scroller a pointer drag from `hit` would move, if any.
    ///
    /// Anything that claims dragging or selection between the press and the scroller owns
    /// the gesture: a slider or a selectable label must not pan the page. A plain click
    /// does not block, because a drag cancels a click anyway.
    fn pan_target(&self, hit: Id) -> Option<Id> {
        let mut current = self.tree.find_by_id(hit);
        while let Some(nid) = current {
            let node = self.tree.get(nid)?;
            if node
                .sense
                .any(ui_core::Sense::DRAG | ui_core::Sense::SELECT)
            {
                return None;
            }
            if node.sense.contains(ui_core::Sense::SCROLL_OWNED) {
                return Some(node.id);
            }
            current = node.parent;
        }
        None
    }

    /// The nearest engine-owned scroller above `hit`, whoever owns the gesture.
    fn enclosing_scroller(&self, hit: Id) -> Option<Id> {
        let mut current = self.tree.find_by_id(hit);
        while let Some(nid) = current {
            let node = self.tree.get(nid)?;
            if node.sense.contains(ui_core::Sense::SCROLL_OWNED) {
                return Some(node.id);
            }
            current = node.parent;
        }
        None
    }

    /// The scrollers a drag from `hit` moves, innermost first.
    ///
    /// A drag on an axis the innermost scroller does not own passes through it to the
    /// ancestor that does: without the chain, dragging vertically over a horizontal strip
    /// moves nothing at all, because the strip takes the gesture and has no vertical range.
    /// A node that owns gestures stops the walk, as it does for `pan_target`, and so does an
    /// app-owned scroller: it takes the value itself, for the wheel's reason.
    fn pan_chain(&self, hit: Id) -> Vec<PanLink> {
        let mut links = Vec::new();
        let mut current = self.tree.find_by_id(hit);
        while let Some(nid) = current {
            let Some(node) = self.tree.get(nid) else {
                break;
            };
            if node
                .sense
                .any(ui_core::Sense::DRAG | ui_core::Sense::SELECT)
            {
                break;
            }
            if node.sense.contains(ui_core::Sense::SCROLL)
                && !node.sense.contains(ui_core::Sense::SCROLL_OWNED)
            {
                break;
            }
            if node.sense.contains(ui_core::Sense::SCROLL_OWNED) {
                if let Some((clip, extent, viewport)) = self.scroll_range(node.id) {
                    let axes = (
                        clip.x == ui_core::layout::Overflow::Scroll,
                        clip.y == ui_core::layout::Overflow::Scroll,
                    );
                    if axes.0 || axes.1 {
                        let raw = self
                            .scroll_offsets
                            .get(&node.id)
                            .copied()
                            .unwrap_or_default();
                        links.push(PanLink {
                            id: node.id,
                            axes,
                            overflow: ui_core::geom::Point::new(
                                (extent.w - viewport.w).max(0.0),
                                (extent.h - viewport.h).max(0.0),
                            ),
                            viewport,
                            raw,
                            prev: raw,
                            v: ui_core::geom::Point::ZERO,
                        });
                    }
                }
            }
            current = node.parent;
        }
        links
    }

    /// Take the pointer over as a pan once the press has travelled far enough.
    fn begin_pan(&mut self, pos: ui_core::geom::Point) -> bool {
        self.pan_candidate = None;
        let Some(hit) = self.hit_at_pointer() else {
            return false;
        };
        let links = self.pan_chain(hit);
        if links.is_empty() {
            return false;
        }
        // Each axis stretches on the outermost link that scrolls it, so a vertical page
        // behind a horizontal strip bounces on y while the strip bounces on x.
        let elastic = (
            links.iter().rposition(|l| l.axes.0),
            links.iter().rposition(|l| l.axes.1),
        );
        // The pan owns the scrollers from here; a coast still running would fight it.
        for link in &links {
            self.end_motion(link.id);
        }
        self.pan = Some(ScrollPan {
            links,
            elastic,
            // The threshold's own travel is dropped: the content starts moving from
            // where the finger crossed it, as a touch scroller does.
            last: pos,
        });
        // A pan is neither a click nor a control drag: the press it grew out of is over.
        self.pressed = None;
        self.press_at = None;
        self.pressed_path.clear();
        self.end_splat();
        true
    }

    /// Move the panned content with the pointer; returns whether a pan was live.
    fn pan_move(&mut self, pos: ui_core::geom::Point) -> bool {
        let Some(pan) = self.pan.as_mut() else {
            return false;
        };
        let (dx, dy) = (pos.x - pan.last.x, pos.y - pan.last.y);
        pan.last = pos;
        // Content follows the finger, so the offset moves opposite to it. Each link takes
        // what its own range allows and passes the remainder up the chain.
        let mut rest = ui_core::geom::Point::new(-dx, -dy);
        let (elastic_x, elastic_y) = pan.elastic;
        for (index, link) in pan.links.iter_mut().enumerate() {
            let stretch_x = elastic_x == Some(index);
            let stretch_y = elastic_y == Some(index);
            if link.axes.0 {
                let take = if stretch_x {
                    rest.x
                } else {
                    rest.x.clamp(-link.raw.x, link.overflow.x - link.raw.x)
                };
                link.raw.x += take;
                rest.x -= take;
            }
            if link.axes.1 {
                let take = if stretch_y {
                    rest.y
                } else {
                    rest.y.clamp(-link.raw.y, link.overflow.y - link.raw.y)
                };
                link.raw.y += take;
                rest.y -= take;
            }
            let next = ui_core::cx::ScrollOffset {
                x: if stretch_x {
                    stretch(link.raw.x, link.overflow.x, link.viewport.w)
                } else {
                    link.raw.x.clamp(0.0, link.overflow.x)
                },
                y: if stretch_y {
                    stretch(link.raw.y, link.overflow.y, link.viewport.h)
                } else {
                    link.raw.y.clamp(0.0, link.overflow.y)
                },
            };
            self.scroll_offsets.insert(link.id, next);
            self.scrolled_since.insert(link.id);
        }
        true
    }

    /// Release the pan: hand each link's speed to a fling, or to the spring if it is
    /// stretched. A link that moved on neither axis is left alone, so a vertical drag over a
    /// horizontal strip does not wake the strip's coast.
    fn end_pan(&mut self, now: f64) -> bool {
        let Some(pan) = self.pan.take() else {
            return false;
        };
        let (elastic_x, elastic_y) = pan.elastic;
        for (index, link) in pan.links.iter().enumerate() {
            let offset = self
                .scroll_offsets
                .get(&link.id)
                .copied()
                .unwrap_or_default();
            let outside = (elastic_x == Some(index)
                && (offset.x < 0.0 || offset.x > link.overflow.x))
                || (elastic_y == Some(index) && (offset.y < 0.0 || offset.y > link.overflow.y));
            let moving = link.v.x.abs() > SCROLL_STOP || link.v.y.abs() > SCROLL_STOP;
            if !outside {
                // A snapping strip rests on a page: an exponential fling travels `v / k`
                // more, so the page nearest that landing is where the hand was heading.
                let snap = self.snap_of(link.id);
                if snap.iter().any(|page| *page > 0.0) {
                    let landing = ui_core::cx::ScrollOffset {
                        x: offset.x + link.v.x / SCROLL_FRICTION,
                        y: offset.y + link.v.y / SCROLL_FRICTION,
                    };
                    let target = ui_core::cx::ScrollOffset {
                        x: snapped_axis(
                            landing.x,
                            landing.x,
                            if link.axes.0 { snap[0] } else { 0.0 },
                            link.overflow.x,
                            false,
                        ),
                        y: snapped_axis(
                            landing.y,
                            landing.y,
                            if link.axes.1 { snap[1] } else { 0.0 },
                            link.overflow.y,
                            false,
                        ),
                    };
                    if target != offset {
                        self.start_settle(link.id, target.x, target.y);
                    }
                    continue;
                }
                // A link that moved on neither axis is left alone, so a vertical drag over
                // a horizontal strip does not wake the strip's coast.
                if !moving {
                    continue;
                }
            }
            let target = if outside {
                self.start_bounce(link.id, now, link.v, link.overflow)
            } else {
                ui_core::cx::ScrollOffset { x: 0.0, y: 0.0 }
            };
            self.scroll_motion.insert(
                link.id,
                ScrollMotion {
                    v0: link.v,
                    origin: offset,
                    started_at: now,
                    v: link.v,
                    spring: outside,
                    overflow: link.overflow,
                    viewport: link.viewport,
                    target,
                },
            );
        }
        true
    }

    /// Seed a scroller's springs at the offset they start from, then retarget them to the
    /// edge with `v` as the launch speed: without the seed the first frame jumps.
    ///
    /// Returns the edge both axes settle on, which the caller keeps as the target: recomputing
    /// it from the moving offset would let the spring freeze wherever it overshoots.
    fn start_bounce(
        &mut self,
        id: Id,
        now: f64,
        v: ui_core::geom::Point,
        overflow: ui_core::geom::Point,
    ) -> ui_core::cx::ScrollOffset {
        let offset = self.scroll_offsets.get(&id).copied().unwrap_or_default();
        let target = ui_core::cx::ScrollOffset {
            x: offset.x.clamp(0.0, overflow.x),
            y: offset.y.clamp(0.0, overflow.y),
        };
        for (key, from, to, speed) in [
            ("scroll-x", offset.x, target.x, v.x),
            ("scroll-y", offset.y, target.y, v.y),
        ] {
            let key = id.with(key);
            // A spring left over from the previous bounce would ignore `from` and start
            // where it stopped, which is exactly the jump this seeds away.
            self.anim_store.forget_spring(key);
            self.anim_store
                .spring_hinted(key, from, BOUNCE_STIFFNESS, BOUNCE_DAMPING, 0.0, now);
            self.anim_store.spring_hinted(
                key,
                to,
                BOUNCE_STIFFNESS,
                BOUNCE_DAMPING,
                speed * BOUNCE_ENTRY,
                now,
            );
        }
        target
    }

    /// Send a scroller to an offset under the same spring a bounce settles with: a move the
    /// app asks for eases instead of landing at once.
    fn start_settle(&mut self, id: Id, x: f32, y: f32) {
        let from = self.scroll_offsets.get(&id).copied().unwrap_or_default();
        let target = ui_core::cx::ScrollOffset { x, y };
        for (key, from, to) in [("scroll-x", from.x, x), ("scroll-y", from.y, y)] {
            let key = id.with(key);
            self.anim_store.forget_spring(key);
            self.anim_store.spring_hinted(
                key,
                from,
                BOUNCE_STIFFNESS,
                BOUNCE_DAMPING,
                0.0,
                self.frame_now,
            );
            self.anim_store.spring_hinted(
                key,
                to,
                BOUNCE_STIFFNESS,
                BOUNCE_DAMPING,
                0.0,
                self.frame_now,
            );
        }
        self.scroll_motion.insert(
            id,
            ScrollMotion {
                v0: ui_core::geom::Point::new(0.0, 0.0),
                origin: from,
                started_at: self.frame_now,
                v: ui_core::geom::Point::new(0.0, 0.0),
                spring: true,
                overflow: ui_core::geom::Point::new(0.0, 0.0),
                viewport: ui_core::geom::Size::new(0.0, 0.0),
                target,
            },
        );
    }

    /// Measure the pan's speed from what the frame actually moved, so the fling starts
    /// with the hand's speed and not with an event's.
    fn sample_pan_velocity(&mut self, dt: f32) {
        let Some(pan) = self.pan.as_mut() else {
            return;
        };
        for link in pan.links.iter_mut() {
            let offset = self
                .scroll_offsets
                .get(&link.id)
                .copied()
                .unwrap_or_default();
            if dt > 0.0 {
                let instant = ui_core::geom::Point::new(
                    (offset.x - link.prev.x) / dt,
                    (offset.y - link.prev.y) / dt,
                );
                // Smoothed: one frame's jitter must not decide a fling's direction.
                link.v = ui_core::geom::Point::new(
                    link.v.x + (instant.x - link.v.x) * 0.6,
                    link.v.y + (instant.y - link.v.y) * 0.6,
                );
            }
            link.prev = offset;
        }
    }

    /// Settle every snapping scroller whose wheel burst has gone quiet.
    fn snap_idle(&mut self) {
        if self.snap_pending.is_empty() {
            return;
        }
        let now = self.frame_now;
        let elastic = self.elastic_scrollers();
        let ready: Vec<Id> = self
            .snap_pending
            .iter()
            .filter(|(id, burst)| now - burst.at >= SNAP_IDLE && !elastic.contains(id))
            .map(|(id, _)| *id)
            .collect();
        for id in ready {
            let Some(burst) = self.snap_pending.remove(&id) else {
                continue;
            };
            // A scroller that left the tree, or one the app is stretching, has no page to
            // land on; the entry goes with it.
            let Some((clip, extent, viewport)) = self.scroll_range(id) else {
                continue;
            };
            let snap = self.snap_of(id);
            let offset = self.scroll_offsets.get(&id).copied().unwrap_or_default();
            let overflow = |scrolls: bool, extent: f32, viewport: f32| {
                if scrolls {
                    (extent - viewport).max(0.0)
                } else {
                    0.0
                }
            };
            let target = ui_core::cx::ScrollOffset {
                x: snapped_axis(
                    offset.x,
                    burst.origin.x,
                    snap[0],
                    overflow(
                        clip.x == ui_core::layout::Overflow::Scroll,
                        extent.w,
                        viewport.w,
                    ),
                    true,
                ),
                y: snapped_axis(
                    offset.y,
                    burst.origin.y,
                    snap[1],
                    overflow(
                        clip.y == ui_core::layout::Overflow::Scroll,
                        extent.h,
                        viewport.h,
                    ),
                    true,
                ),
            };
            if target != offset {
                self.start_settle(id, target.x, target.y);
            }
        }
    }

    /// The page sizes a scroller rests on, per axis; zero for an axis that does not snap.
    fn snap_of(&self, id: Id) -> [f32; 2] {
        self.tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))
            .and_then(|node| node.data.params())
            .map(|params| params.snap)
            .unwrap_or([0.0; 2])
    }

    /// Drop a scroller's motion together with the springs it seeded.
    ///
    /// Only this motion advances those springs, so one left behind would report itself as
    /// still moving for ever and keep the frame loop awake with nothing to draw.
    fn end_motion(&mut self, id: Id) {
        self.scroll_motion.remove(&id);
        self.anim_store.forget_spring(id.with("scroll-x"));
        self.anim_store.forget_spring(id.with("scroll-y"));
    }

    /// Advance every released scroller: momentum decays exponentially, and past an end
    /// the spring takes over, carrying whatever speed the fling arrived with.
    fn advance_scroll_motion(&mut self, now: f64, dt: f32) {
        if dt <= 0.0 || self.scroll_motion.is_empty() {
            return;
        }
        let ids: Vec<Id> = self.scroll_motion.keys().copied().collect();
        for id in ids {
            let Some(mut motion) = self.scroll_motion.get(&id).copied() else {
                continue;
            };
            let next = if motion.spring {
                let x = self.anim_store.spring_hinted(
                    id.with("scroll-x"),
                    motion.target.x,
                    BOUNCE_STIFFNESS,
                    BOUNCE_DAMPING,
                    0.0,
                    now,
                );
                let y = self.anim_store.spring_hinted(
                    id.with("scroll-y"),
                    motion.target.y,
                    BOUNCE_STIFFNESS,
                    BOUNCE_DAMPING,
                    0.0,
                    now,
                );
                // The spring decides when it has landed, not a threshold of our own: a
                // looser one would end the motion while the spring still reports movement.
                let settled = !self.anim_store.spring_live(id.with("scroll-x"))
                    && !self.anim_store.spring_live(id.with("scroll-y"));
                if settled {
                    self.end_motion(id);
                    Some(motion.target)
                } else {
                    Some(ui_core::cx::ScrollOffset { x: x.x, y: y.x })
                }
            } else {
                // Closed form: the same distance the decay integral gives, so a dropped frame
                // does not turn into one large jump.
                let t = (now - motion.started_at) as f32;
                let decay = (-SCROLL_FRICTION * t).exp();
                motion.v = ui_core::geom::Point::new(motion.v0.x * decay, motion.v0.y * decay);
                let travel = (1.0 - decay) / SCROLL_FRICTION;
                let (raw_x, raw_y) = (
                    motion.origin.x + motion.v0.x * travel,
                    motion.origin.y + motion.v0.y * travel,
                );
                let crossed = raw_x < 0.0
                    || raw_x > motion.overflow.x
                    || raw_y < 0.0
                    || raw_y > motion.overflow.y;
                // Past an end the same rubber band damps the coast, so a long frame cannot
                // fling the content far out and then snap it back.
                let moved = ui_core::cx::ScrollOffset {
                    x: stretch(raw_x, motion.overflow.x, motion.viewport.w),
                    y: stretch(raw_y, motion.overflow.y, motion.viewport.h),
                };
                if crossed {
                    // The momentum hands its speed to the spring at the edge.
                    self.scroll_offsets.insert(id, moved);
                    motion.spring = true;
                    motion.target = self.start_bounce(id, now, motion.v, motion.overflow);
                    self.scroll_motion.insert(id, motion);
                    Some(moved)
                } else if motion.v.x.abs() <= SCROLL_STOP && motion.v.y.abs() <= SCROLL_STOP {
                    self.end_motion(id);
                    Some(moved)
                } else {
                    self.scroll_motion.insert(id, motion);
                    Some(moved)
                }
            };
            if let Some(next) = next {
                self.scroll_offsets.insert(id, next);
                self.scrolled_since.insert(id);
            }
        }
    }

    /// The subtree that owns the keyboard: the topmost modal, else the topmost popup.
    fn focus_scope(&self) -> Option<ui_core::NodeId> {
        self.tree.modal_root().or_else(|| self.tree.popup_root())
    }

    /// Keep focus on a live node inside the topmost modal; report whether it moved.
    ///
    /// A dialog that opens takes focus from whatever was behind it, as WinUI's
    /// `ContentDialog` does, or Enter and Escape would reach nothing at all.
    fn settle_focus(&mut self) -> bool {
        let before = (self.focused, self.focus_visible);
        if self
            .focused
            .is_some_and(|id| self.tree.find_by_id(id).is_none())
        {
            self.focused = None;
        }
        if let Some(modal) = self.tree.modal_root() {
            let inside = self
                .focused
                .and_then(|id| self.tree.find_by_id(id))
                .is_some_and(|nid| self.tree.within(nid, modal));
            if !inside {
                self.focused = self.focus_stops(false).into_iter().next();
                // Programmatic: the ring waits until the user reaches the dialog by key.
                self.focus_visible = false;
            }
            self.scope_root = None;
        } else if let Some(popup) = self.tree.popup_root() {
            // A popup takes the keyboard once, when it appears: WinUI's flyouts do the same,
            // or the arrows would keep walking the page behind an open menu.
            if self.scope_root != Some(popup) {
                let inside = self
                    .focused
                    .and_then(|id| self.tree.find_by_id(id))
                    .is_some_and(|nid| self.tree.within(nid, popup));
                if !inside {
                    self.focus_before_scope = self.focused;
                    // A combo box opens onto its selected row; a menu onto its first.
                    let preferred = self
                        .tree
                        .get(popup)
                        .and_then(|n| n.data.params())
                        .and_then(|p| p.focus_initial);
                    let stops = self.focus_stops(false);
                    self.focused = preferred
                        .filter(|id| stops.contains(id))
                        .or_else(|| stops.first().copied());
                    self.focus_visible = false;
                }
                self.scope_root = Some(popup);
            }
        } else if self.scope_root.take().is_some() {
            // The popup went away. Focus inside it died with it, so it goes back to where it
            // was; a press outside already moved focus somewhere live, and keeps it.
            let alive = self
                .focused
                .is_some_and(|id| self.tree.find_by_id(id).is_some());
            if alive {
                self.focus_before_scope = None;
            } else {
                self.focused = self
                    .focus_before_scope
                    .take()
                    .filter(|id| self.tree.find_by_id(*id).is_some());
            }
        }
        if before != (self.focused, self.focus_visible) {
            self.damage.add(Rect::new(
                0.0,
                0.0,
                self.last_size.0 as f32,
                self.last_size.1 as f32,
            ));
            return true;
        }
        false
    }

    /// Nodes Tab can reach, in document order: they receive input and paint something.
    ///
    /// A bare layout container is skipped: it is a hit target but has nothing to focus.
    fn focusables(&self) -> Vec<Id> {
        self.focus_stops(true)
    }

    /// Focus stops inside the keyboard scope, in document order.
    ///
    /// `clipped` keeps only what the mask lets through, which is what Tab must walk; the
    /// initial take passes `false`, since an entrance animation clips its own first rows.
    fn focus_stops(&self, clipped: bool) -> Vec<Id> {
        let scope = self.focus_scope();
        self.tree
            .walk()
            .into_iter()
            // The scope root asks for the keyboard on behalf of its contents; it is not a stop
            // of its own, or a menu's first Tab would land on a panel that draws no ring.
            .filter(|nid| scope.is_none_or(|m| *nid != m && self.tree.within(*nid, m)))
            .filter_map(|nid| self.tree.get(nid))
            .filter(|n| Self::hit_behavior_of(n.kind, n.sense, n.disabled) != HitBehavior::Ignore)
            .filter(|n| n.sense.contains(ui_core::Sense::FOCUSABLE))
            .filter(|n| focus_stop(n))
            .filter(|n| {
                // The mask is in visual space, so the rect compared against it must be too.
                !clipped
                    || !n
                        .visual_rect()
                        .intersection(n.focus_mask)
                        .unwrap_or(ui_core::geom::Rect::ZERO)
                        .is_empty()
            })
            .map(|n| n.id)
            .collect()
    }

    /// Whether `id` may hear the keyboard: it sits inside the modal that owns it, if any.
    fn in_key_scope(&self, id: Id) -> bool {
        match self.tree.modal_root() {
            None => true,
            Some(modal) => self
                .tree
                .find_by_id(id)
                .is_some_and(|nid| self.tree.within(nid, modal)),
        }
    }

    fn focusables_with_rects(&self) -> Vec<(Id, Rect)> {
        // A modal owns the keyboard: Tab and the arrows may not walk out of the surface it
        // covers, or the scrim would only be a picture of a modal.
        let scope = self.focus_scope();
        self.tree
            .walk()
            .into_iter()
            .filter(|nid| scope.is_none_or(|m| *nid != m && self.tree.within(*nid, m)))
            .filter_map(|nid| self.tree.get(nid))
            // An inert node is not a Tab stop either, so the same gate applies here.
            .filter(|n| Self::hit_behavior_of(n.kind, n.sense, n.disabled) != HitBehavior::Ignore)
            .filter(|n| n.sense.contains(ui_core::Sense::FOCUSABLE))
            .filter(|n| focus_stop(n))
            .filter_map(|n| {
                let r = n
                    .visual_rect()
                    .intersection(n.focus_mask)
                    .unwrap_or(Rect::ZERO);
                (!r.is_empty()).then_some((n.id, r))
            })
            .collect()
    }

    /// Move focus to the nearest node in `dir`, judged from layout rects.
    ///
    /// Geometry, not tree order: a node can be visually anywhere regardless of where it
    /// sits in the tree, so structural order answers a different question (that is Tab).
    fn focus_spatial(&mut self, dir: Direction) -> bool {
        // Arrow keys are keyboard focus like Tab, so they may show a ring after a click.
        self.focus_visible = true;
        let items = self.focusables_with_rects();
        if items.is_empty() {
            return false;
        }
        let Some((current, from)) = self
            .focused
            .and_then(|f| items.iter().find(|(id, _)| *id == f).copied())
        else {
            // Nothing focused: arrows start the ring, like Tab does.
            let first = items[0].0;
            if self.focused == Some(first) {
                return false;
            }
            self.focused = Some(first);
            return true;
        };
        let best = items
            .iter()
            .filter(|(id, _)| *id != current)
            .filter_map(|(id, r)| spatial_score(from, *r, dir).map(|score| (score, *id)))
            .min_by(|a, b| {
                // Lexicographic: band miss, then gap, then misalignment.
                a.0.0
                    .total_cmp(&b.0.0)
                    .then(a.0.1.total_cmp(&b.0.1))
                    .then(a.0.2.total_cmp(&b.0.2))
            });
        match best {
            // No wrap: an arrow that finds nothing in its direction must not jump across
            // the screen, which would be indistinguishable from a wrong answer.
            Some((_, id)) => {
                self.focused = Some(id);
                true
            }
            // A menu's ends meet: Down on the last item wraps to the first, as its own list
            // does. Only a popup wraps — on a page an arrow that finds nothing must not jump.
            None => self.wrap_in_popup(dir, &items, current),
        }
    }

    /// Move focus to the first or last item of an open popup, in document order.
    fn focus_popup_edge(&mut self, last: bool) -> bool {
        if !self.popup_owns_keys() {
            return false;
        }
        let items = self.focusables();
        let next = if last { items.last() } else { items.first() }.copied();
        match next {
            Some(id) if self.focused != Some(id) => {
                self.focused = Some(id);
                self.focus_visible = true;
                true
            }
            _ => false,
        }
    }

    /// Whether the keyboard belongs to an open popup rather than to the page behind it.
    fn popup_owns_keys(&self) -> bool {
        self.tree.modal_root().is_none() && self.tree.popup_root().is_some()
    }

    /// Wrap an arrow that found nothing to the far end of an open popup's axis.
    fn wrap_in_popup(&mut self, dir: Direction, items: &[(Id, Rect)], current: Id) -> bool {
        if !self.popup_owns_keys() {
            return false;
        }
        let along_y = matches!(dir, Direction::Up | Direction::Down);
        let key = |r: Rect| if along_y { r.origin.y } else { r.origin.x };
        let forward = matches!(dir, Direction::Down | Direction::Right);
        let far = items
            .iter()
            .filter(|(id, _)| *id != current)
            .min_by(|a, b| {
                let (a, b) = (key(a.1), key(b.1));
                if forward {
                    a.total_cmp(&b)
                } else {
                    b.total_cmp(&a)
                }
            })
            .map(|(id, _)| *id);
        match far {
            Some(id) => {
                self.focused = Some(id);
                true
            }
            None => false,
        }
    }

    /// Move the keyboard focus to one named node, if the frame can focus it.
    fn focus_node(&mut self, id: Id) -> bool {
        if self.focused == Some(id) || !self.focusables().contains(&id) {
            return false;
        }
        self.focused = Some(id);
        // A control that steers focus by itself is doing it for the keyboard.
        self.focus_visible = true;
        true
    }

    /// Move focus one step around the ring, wrapping at both ends.
    fn focus_step(&mut self, forward: bool) -> bool {
        let items = self.focusables();
        if items.is_empty() {
            return false;
        }
        let next = match self
            .focused
            .and_then(|f| items.iter().position(|id| *id == f))
        {
            Some(i) if forward => items[(i + 1) % items.len()],
            Some(i) => items[(i + items.len() - 1) % items.len()],
            None if forward => items[0],
            None => items[items.len() - 1],
        };
        if self.focused == Some(next) {
            return false;
        }
        self.focused = Some(next);
        true
    }

    /// The nearest ancestor-or-self of the *pressed* node that can be dragged.
    ///
    /// Anchored on the press, not on the pointer: a drag belongs to what was pressed, and
    /// resolving it from the pointer loses the gesture the moment it leaves the control.
    /// Rewrite a drag event's `local` point into the dragged node's own space.
    ///
    /// The tracker is a pure state machine with no tree, so it reports surface points; the
    /// only place that knows the node's rect is here.
    fn in_node_space(&self, ev: ui_core::DragEvent) -> ui_core::DragEvent {
        let origin = |id: Id| -> Option<ui_core::geom::Point> {
            let nid = self.tree.find_by_id(id)?;
            Some(self.tree.get(nid)?.visual_rect().origin)
        };
        match ev {
            ui_core::DragEvent::Started {
                on,
                from,
                local,
                button,
            } => {
                let o = origin(on).unwrap_or(ui_core::geom::Point::new(0.0, 0.0));
                ui_core::DragEvent::Started {
                    on,
                    from,
                    local: ui_core::geom::Point::new(local.x - o.x, local.y - o.y),
                    button,
                }
            }
            ui_core::DragEvent::Moved {
                on,
                delta,
                step,
                local,
            } => {
                let o = origin(on).unwrap_or(ui_core::geom::Point::new(0.0, 0.0));
                ui_core::DragEvent::Moved {
                    on,
                    delta,
                    step,
                    local: ui_core::geom::Point::new(local.x - o.x, local.y - o.y),
                }
            }
            other => other,
        }
    }

    fn drag_owner(&self) -> Option<Id> {
        self.drag_owner_from(self.pressed?)
    }

    /// The nearest ancestor-or-self of `start` that declared [`Sense::DRAG`].
    fn drag_owner_from(&self, start: Id) -> Option<Id> {
        let nid = self.tree.find_by_id(start)?;
        // The declaration is the only source: no hook can widen or veto it.
        self.ancestor_matching(nid, |id| {
            self.tree
                .find_by_id(id)
                .and_then(|n| self.tree.get(n))
                .map(|n| n.sense.contains(ui_core::Sense::DRAG))
                .unwrap_or(false)
        })
    }

    /// Whether the drag owner `on` carries data, as opposed to a gesture-only drag.
    fn drag_has_payload(&self, on: Id) -> bool {
        self.tree
            .find_by_id(on)
            .and_then(|nid| self.tree.get(nid))
            .is_some_and(|n| n.drag.is_some())
    }

    /// Walk from `start` up to the root, returning the first id the predicate accepts.
    /// Every node folded as inert this frame, for the context to answer `is_disabled`.
    fn inert_ids(&self) -> Vec<Id> {
        self.tree
            .walk()
            .into_iter()
            .filter_map(|nid| self.tree.get(nid))
            .filter(|n| n.disabled)
            .map(|n| n.id)
            .collect()
    }

    /// The nearest ancestor-or-self that subscribes to hover; `None` when none does.
    ///
    /// Hover is a subscription, not a side effect of being drawn: a plain box that never
    /// asked for it must not report itself hovered.
    fn hover_owner(&self, start: Id) -> Option<Id> {
        self.tree
            .find_by_id(start)
            .and_then(|nid| self.ancestor_with_sense(nid, ui_core::Sense::HOVER))
    }

    /// The nearest ancestor-or-self whose node declares `bit`.
    fn ancestor_with_sense(&self, start: ui_core::NodeId, bit: ui_core::Sense) -> Option<Id> {
        let mut cur = Some(start);
        while let Some(node) = cur {
            let n = self.tree.get(node)?;
            if n.sense.contains(bit) {
                return Some(n.id);
            }
            cur = n.parent;
        }
        None
    }

    fn ancestor_matching(&self, start: ui_core::NodeId, want: impl Fn(Id) -> bool) -> Option<Id> {
        let mut cur = Some(start);
        while let Some(node) = cur {
            let n = self.tree.get(node)?;
            if want(n.id) {
                return Some(n.id);
            }
            cur = n.parent;
        }
        None
    }

    /// The nearest ancestor-or-self of the pointer that accepts the live drag.
    fn drop_target(&self) -> Option<Id> {
        let pos = self.pointer?;
        let types = self
            .drag_session
            .as_ref()
            .map(|session| (session.types_decide(), Rc::clone(&session.types)));
        let types = types.filter(|(decide, _)| *decide).map(|(_, types)| types);
        self.drag_target_at(pos, types.as_deref().map(Vec::as_slice))
    }

    /// The nearest ancestor-or-self of `pos` that takes a drag offering `types`.
    ///
    /// `None` for the types means a drag that carries nothing: any `DROP` node takes it,
    /// which is the behaviour a plain gesture had before payloads existed.
    fn drag_target_at(&self, pos: ui_core::geom::Point, types: Option<&[String]>) -> Option<Id> {
        let nid = self.hit.topmost_consumer(pos)?;
        // Declared once on the node; the ancestor walk covers containers full of children.
        self.ancestor_matching(nid, |id| {
            self.tree
                .find_by_id(id)
                .and_then(|n| self.tree.get(n))
                .is_some_and(|n| match types {
                    Some(types) => n.accepts_drop(types.iter().map(String::as_str)),
                    None => n.sense.contains(ui_core::Sense::DROP),
                })
        })
    }

    fn hit_at_pointer(&self) -> Option<Id> {
        self.pointer
            .and_then(|p| self.hit.topmost_consumer(p))
            .and_then(|nid| self.tree.get(nid).map(|n| n.id))
    }

    /// Node currently under the pointer.
    pub fn hovered(&self) -> Option<Id> {
        self.hovered
    }

    /// Node the primary button is held down on.
    pub fn pressed(&self) -> Option<Id> {
        self.pressed
    }

    /// Node holding keyboard focus.
    pub fn focused(&self) -> Option<Id> {
        self.focused
    }

    /// The focused text input and the caret's byte offset inside it.
    pub fn caret(&self) -> Option<(Id, usize)> {
        self.editing.map(|(id, sel)| (id, sel.caret))
    }

    /// The live selection: an input's own, or a read-only text node's.
    /// The live selection: the block its caret reached and the bytes there, or the input's
    /// own. A cross-block selection reports the block the drag last touched.
    pub fn selection(&self) -> Option<(Id, ui_core::TextSelection)> {
        if let Some(ends) = self.selection {
            let span = self
                .span_of(ends.head.0)
                .unwrap_or(ui_core::TextSelection::collapsed(ends.head.1));
            return Some((ends.head.0, span));
        }
        self.editing
    }

    /// Per-node fingerprint of the painted primitives from last frame.
    ///
    /// Geometry alone is not enough: changing a label, a colour or a glyph position
    /// leaves the rect identical while the pixels differ.
    /// Backdrop read distance for `id`, or `None` when it does not filter its backdrop.
    fn backdrop_pad(&self, id: Id) -> Option<f32> {
        self.tree
            .find_by_id(id)
            .and_then(|nid| self.tree.get(nid))
            .and_then(|n| n.data.params())
            .and_then(|p| p.backdrop_blur)
            .map(|b| b.radius as f32)
    }

    fn compute_damage(&mut self) {
        let current = self.prim_fingerprints();
        // Backdrop nodes are resolved after the ordinary ones: their pixels depend on what
        // changed, not on their own primitives, so they inherit damage rather than claim it.
        let mut frosts: Vec<(Rect, f32)> = Vec::new();
        for (id, (ink_now, fp_now)) in &current {
            match self.node_prints.get(id) {
                // An identical fingerprint means this node contributes identical pixels.
                Some((_, fp_before)) if fp_before == fp_now => {
                    if let Some(pad) = self.backdrop_pad(*id) {
                        frosts.push((*ink_now, pad));
                    }
                }
                Some((ink_before, _)) => {
                    // old ∪ new: the vacated area must be cleared too.
                    self.damage.add(*ink_before);
                    self.damage.add(*ink_now);
                }
                None => self.damage.add(*ink_now),
            }
        }
        for (id, (ink_before, _)) in &self.node_prints {
            if !current.contains_key(id) {
                self.damage.add(*ink_before);
            }
        }
        self.node_prints = current;
        // A layer's opacity and effects are not primitives: a change to them alone leaves
        // every fingerprint intact, so the layer's own area has to be claimed here.
        let layers: Vec<(Rect, Vec<ui_core::Effect>)> = self
            .resolved
            .layers
            .iter()
            .map(|l| (l.bounds, l.effects.clone()))
            .collect();
        for (bounds, effects) in &layers {
            let same = self
                .layer_prints
                .iter()
                .any(|(b, e)| b == bounds && e == effects);
            if !same {
                self.damage.add(*bounds);
            }
        }
        // A layer that disappears still changes the pixels under it, so its old bounds
        // need damage; a settled fade drops its layer on exactly the last frame shown.
        for (bounds, _) in &self.layer_prints {
            if !layers.iter().any(|(b, _)| b == bounds) {
                self.damage.add(*bounds);
            }
        }
        self.layer_prints = layers;
        // A backdrop writes no primitive of its own, so a node that only frosts never enters
        // `node_prints`: moving it, retuning it or removing it would leave its blur behind.
        // The whole entry is compared — rect, clip, corners, radius, sigma and material — so
        // any change to what the frost paints counts as one.
        let now = std::mem::take(&mut self.backdrops_now);
        for (id, entry) in &now {
            match self.backdrops.get(id) {
                Some(before) if before == entry => {}
                Some(before) => {
                    self.damage.add(before.rect);
                    self.damage.add(entry.rect);
                }
                None => self.damage.add(entry.rect),
            }
        }
        for (id, before) in &self.backdrops {
            if !now.contains_key(id) {
                self.damage.add(before.rect);
            }
        }
        // A settled frost with no primitives of its own still has to inherit damage from
        // what it blurs; the loop above only covers nodes that appear in `node_prints`.
        for (id, entry) in &now {
            if !self.node_prints.contains_key(id) && self.backdrops.get(id) == Some(entry) {
                frosts.push((entry.rect, entry.radius as f32));
            }
        }
        self.backdrops = now;
        // A frost only needs repainting where something under it actually changed.
        // Take the snapshot once: a frost must react to what changed under it, not to the
        // damage other frosts have already inherited.
        let inherited = self.damage.snapshot();
        let margin = |pad: f32| ui_core::geom::Insets::all(pad);
        for (ink, pad) in frosts {
            // The widened rect is a *probe* only: it decides whether a change can reach the
            // frost. What gets painted is always the change clipped back to the frost.
            let probe = margin(pad).expand(ink);
            let reached = inherited
                .rects(ink)
                .iter()
                .any(|r| r.intersection(probe).is_some());
            // Any change within the kernel's reach alters every output pixel, because the
            // convolution re-reads a neighbourhood around each one. The whole frost is the
            // damaged region, not just the overlap.
            if reached {
                self.damage.add(ink);
            }
        }
    }

    /// Union ink rect and content hash per owning node, taken from the resolved list.
    fn prim_fingerprints(&self) -> FastMap<Id, (Rect, u64)> {
        use std::hash::{Hash, Hasher};

        let list = &self.resolved;
        let mut acc: FastMap<Id, (Rect, FastHasher)> = FastMap::default();
        let mut note = |nid: ui_core::NodeId, ink: Rect, fill: &mut dyn FnMut(&mut FastHasher)| {
            let Some(node) = self.tree.get(nid) else {
                return;
            };
            let entry = acc.entry(node.id).or_insert_with(|| {
                let mut hasher = FastHasher::default();
                // The node id seeds the hash so two nodes never collide by content alone.
                node.id.hash(&mut hasher);
                (Rect::ZERO, hasher)
            });
            entry.0 = union_rect(entry.0, ink);
            fill(&mut entry.1);
        };

        for op in &list.ops {
            let ui_core::draw::Op::Prim(prim) = op else {
                continue;
            };
            match prim.kind {
                PrimKind::Shadow => {
                    let i = prim.index as usize;
                    let p = &list.prims.shadows[i];
                    // The shader pads the quad past `rect`; damage has to claim all of it.
                    note(list.owners.shadows[i], p.painted(), &mut |h| {
                        hash_rect(&p.rect, h);
                        hash_color(&p.colour, h);
                        p.blur.to_bits().hash(h);
                        p.dy.to_bits().hash(h);
                    });
                }
                PrimKind::Image => {
                    let i = prim.index as usize;
                    let p = &list.prims.images[i];
                    // The texture slot is minted afresh every frame; the picture behind it
                    // is identified by its buffer, which an app replaces without moving the
                    // node — or, for a canvas, by the version that painted it.
                    let (picture, canvas) =
                        list.images.get(p.texture as usize).map_or((0, 0), |entry| {
                            match &entry.source {
                                ui_core::draw::ImageSource::Pixels(_) => (entry.picture_id(), 0),
                                ui_core::draw::ImageSource::Canvas(id) => (
                                    id.0 as usize,
                                    self.canvas_versions.get(&id.0).copied().unwrap_or(0),
                                ),
                            }
                        });
                    note(list.owners.images[i], p.rect, &mut |h| {
                        hash_rect(&p.rect, h);
                        h.write(&p.texture.to_le_bytes());
                        hash_rect(&p.src, h);
                        picture.hash(h);
                        canvas.hash(h);
                    });
                }
                PrimKind::Rect => {
                    let i = prim.index as usize;
                    let p = &list.prims.rects[i];
                    let owner = list.owners.rects[i];
                    note(owner, p.rect, &mut |h| {
                        hash_rect(&p.rect, h);
                        for c in [p.radius.tl, p.radius.tr, p.radius.br, p.radius.bl] {
                            c.to_bits().hash(h);
                        }
                        hash_color(&p.fill, h);
                        match &p.border {
                            Some((c, w)) => {
                                true.hash(h);
                                hash_color(c, h);
                                w.to_bits().hash(h);
                            }
                            None => false.hash(h),
                        }
                    });
                }
                PrimKind::Sector => {
                    let i = prim.index as usize;
                    let p = &list.prims.sectors[i];
                    note(list.owners.sectors[i], p.bounds, &mut |h| {
                        hash_rect(&p.bounds, h);
                        hash_color(&p.colour, h);
                        let s = &p.sector;
                        for v in [
                            s.centre.x,
                            s.centre.y,
                            s.radius,
                            s.thickness,
                            s.trim.0,
                            s.trim.1,
                        ] {
                            v.to_bits().hash(h);
                        }
                        s.round_caps.hash(h);
                    });
                }
                PrimKind::Ellipse => {
                    let i = prim.index as usize;
                    let p = &list.prims.ellipses[i];
                    note(list.owners.ellipses[i], p.bounds, &mut |h| {
                        hash_rect(&p.bounds, h);
                        hash_color(&p.fill, h);
                        hash_color(&p.colour, h);
                        for v in [p.centre.x, p.centre.y, p.radii.0, p.radii.1, p.thickness] {
                            v.to_bits().hash(h);
                        }
                    });
                }
                PrimKind::Line => {
                    let i = prim.index as usize;
                    let p = &list.prims.lines[i];
                    note(list.owners.lines[i], p.bounds(), &mut |h| {
                        for v in [p.from.x, p.from.y, p.to.x, p.to.y, p.thickness] {
                            v.to_bits().hash(h);
                        }
                        hash_color(&p.colour, h);
                    });
                }
                PrimKind::Glyph => {
                    let i = prim.index as usize;
                    let p = &list.prims.glyphs[i];
                    let owner = list.owners.glyphs[i];
                    note(owner, p.rect, &mut |h| {
                        hash_rect(&p.rect, h);
                        hash_rect(&p.atlas, h);
                        p.alpha.to_bits().hash(h);
                        hash_color(&p.color, h);
                    });
                }
                PrimKind::Text => {
                    let i = prim.index as usize;
                    let p = &list.prims.texts[i];
                    let owner = list.owners.texts[i];
                    // The width is only an estimate, and the digest only has to be stable:
                    // flattening the block for either would build its span table, and this
                    // runs for every text on the page every frame.
                    let chars: usize = p.block.runs.iter().map(|r| r.text.chars().count()).sum();
                    let style = p
                        .block
                        .runs
                        .first()
                        .map_or_else(Default::default, |r| r.style);
                    let ink = Rect::new(
                        p.origin.x,
                        p.origin.y,
                        style.size * chars as f32 * 0.6,
                        style.size * 1.3,
                    );
                    note(owner, ink, &mut |h| {
                        for run in &p.block.runs {
                            run.text.hash(h);
                        }
                        style.size.to_bits().hash(h);
                        hash_color(&p.ink, h);
                    });
                }
                PrimKind::Math => {
                    let i = prim.index as usize;
                    let p = &list.prims.maths[i];
                    let owner = list.owners.maths[i];
                    let ink =
                        Rect::new(p.origin.x, p.origin.y, p.formula.width, p.formula.height());
                    note(owner, ink, &mut |h| {
                        // The display list is what pixels depend on; the family picks glyphs.
                        format!("{:?}", p.formula).hash(h);
                        p.family.hash(h);
                        hash_color(&p.ink, h);
                    });
                }
            }
        }

        acc.into_iter()
            .map(|(id, (ink, hasher))| (id, (ink, hasher.finish())))
            .collect()
    }

    /// Paint the subtree, wrapping group-opacity nodes in an offscreen layer.
    ///
    /// Group opacity is not per-primitive alpha: two overlapping opaque children inside a
    /// 50% layer must stay uniformly 50%, not darken twice where they overlap.
    fn paint_tree(&mut self, root: ui_core::NodeId) {
        // The same order the hit index uses: modal and overlay subtrees last, so the topmost
        // in paint is the topmost in hits. One found inside a deferred one queues behind it.
        let mut deferred = Vec::new();
        self.paint_node(root, true, &mut deferred);
        // A held subtree is out of the live tree but still on screen while it leaves.
        deferred.extend_from_slice(self.tree.held());
        let mut next = 0usize;
        while let Some(nid) = deferred.get(next).copied() {
            next += 1;
            // A panel with a surface of its own is painted there, not here.
            if self.is_platform_panel(nid) {
                continue;
            }
            self.paint_node(nid, false, &mut deferred);
        }
    }

    /// Whether this node is a panel the layout handed to a surface of the platform's own.
    ///
    /// A panel the host has not placed yet counts as one: it is held back for the frame
    /// the host decided in, rather than painted in the window and moved out of it.
    fn is_platform_panel(&self, nid: ui_core::NodeId) -> bool {
        self.tree.platform_panels().iter().any(|panel| panel.node == nid)
            && self.panel_presentation.get(&nid) != Some(&false)
    }

    /// Paint every platform panel into its own list, rebased on the panel's top-left.
    ///
    /// The parent's list left these subtrees out, so nothing paints twice. The lists are
    /// resolved here too: a presenter has no text pass of its own.
    fn paint_panels(&mut self) {
        self.panel_views.clear();
        let panels: Vec<_> = self
            .tree
            .platform_panels()
            .iter()
            .filter(|panel| self.panel_presentation.get(&panel.node) == Some(&true))
            .cloned()
            .collect();
        if panels.is_empty() {
            return;
        }
        let scale = self.text_scale();
        for panel in panels {
            let rect = panel.rect;
            let canvas = Rect::new(0.0, 0.0, rect.size.w, rect.size.h);
            let mut list = ui_core::draw::DrawList::new(canvas);
            list.transform = Some(ui_core::draw::Transform {
                scale: 1.0,
                offset: ui_core::geom::Point::new(-rect.origin.x, -rect.origin.y),
            });
            std::mem::swap(&mut list, &mut self.list);
            let mut deferred = Vec::new();
            self.painting_panel = true;
            self.paint_node(panel.node, true, &mut deferred);
            let mut next = 0usize;
            while let Some(nid) = deferred.get(next).copied() {
                next += 1;
                if self.is_platform_panel(nid) {
                    continue;
                }
                self.paint_node(nid, false, &mut deferred);
            }
            self.painting_panel = false;
            std::mem::swap(&mut list, &mut self.list);
            let mut resolved = ui_core::draw::DrawList::new(canvas);
            self.text
                .borrow_mut()
                .resolve_into(&list, scale, &mut resolved);
            self.panel_views.push((panel.node, resolved));
        }
        // The root's damage has already been diffed; a panel's frosts belong to the surface
        // that shows the panel, and would otherwise be diffed into the root next frame.
        self.backdrops_now.clear();
    }

    /// Whether a kernel layer is large enough that reusing its output beats recomputing it.
    ///
    /// A cached layer costs a full-frame clear and copy on every frame it changes, while a
    /// kernel pass costs about its own area times the taps. A press glare is a few thousand
    /// pixels and loses that trade; a frosted panel covering a share of the frame wins.
    fn worth_caching(&self, bounds: ui_core::geom::Rect) -> bool {
        let (width, height) = self.last_size;
        let scale = self.scale.max(f32::EPSILON);
        let frame = (width as f32 / scale) * (height as f32 / scale);
        frame > 0.0 && bounds.size.w * bounds.size.h >= frame * EFFECT_CACHE_MIN_SHARE
    }

    fn paint_node(
        &mut self,
        nid: ui_core::NodeId,
        is_root: bool,
        deferred: &mut Vec<ui_core::NodeId>,
    ) {
        let Some(node) = self.tree.get(nid) else {
            return;
        };
        if node.mask.is_empty() && !is_root {
            return;
        }
        // A presence scale wraps the whole subtree: shapes, clips, layers and glyphs alike.
        let outer = self.list.transform;
        let outer_shape = self.list.shape_scale;
        if (node.presence_scale - 1.0).abs() > 1e-6 {
            let own =
                ui_core::draw::Transform::about(node.visual_rect().center(), node.presence_scale);
            self.list.transform = Some(match outer {
                Some(outer) => outer.then(own),
                None => own,
            });
        }
        // A declared scale sits inside the presence one and turns about the node's own
        // top-left, so the box keeps growing down and right from where layout put it.
        if let Some(params) = node.data.params()
            && (params.scale - 1.0).abs() > 1e-6
        {
            let own = ui_core::draw::Transform::about(node.visual_rect().origin, params.scale);
            self.list.transform = Some(match self.list.transform {
                Some(outer) => outer.then(own),
                None => own,
            });
            self.list.shape_scale *= params.scale;
        }

        // Layout intersected every declared clip down the tree; paint has to honour it.
        let mask = node.mask;
        let visual = node.visual_rect();
        let corners = node_corners(node);
        // A container's clip cuts its subtree and nothing else. CSS's overflow clips content,
        // never the element's own box-shadow or background, so this clip is pushed after
        // `paint_self` rather than before it. Every ancestor's clip is already on the stack,
        // and the mask only ever narrows where a node asked for it, so nothing else is missing.
        // The root clips too: a scroller returned as the whole app is still a scroller, and
        // layout already cut its children's masks by this same rect.
        let declares = node.data.params().map(|p| p.clip.clips()).unwrap_or(false)
            && !self.tree.children_of(Some(nid)).is_empty();

        let backdrops_before = self.list.backdrops.len();
        // Presence folds into group opacity, so an entering subtree fades as one unit.
        let (opacity, effects) = match &node.data {
            NodeData::Container(p) => (p.opacity * node.presence_alpha, p.effects.clone()),
            _ => (1.0, Vec::new()),
        };
        let offscreen = matches!(&node.data, NodeData::Container(p) if p.offscreen);
        // A kernel re-reads its neighbours, which is the work worth keeping; an opacity-only
        // chain is one in-place pass, so it stays uncached, as the CPU renderer's does.
        let kernel = effects.iter().any(|e| !e.kernel.is_identity());
        let layered = opacity < 1.0 || !effects.is_empty() || offscreen;
        let mut layer_index = None;
        if layered {
            // Bounds must cover everything the subtree paints, not just this node's rect.
            let bounds = self.subtree_ink(nid);
            let mut chain = effects;
            if opacity < 1.0 {
                chain.push(ui_core::Effect::opacity(opacity));
            }
            // Kernels are authored in logical units, so their reach needs no conversion.
            let reach = ui_core::EffectReach::of(&chain).actual as f32;
            let bounds = bounds.inflate(reach);
            layer_index = Some(if offscreen || (kernel && self.worth_caching(bounds)) {
                self.list.push_cached_layer(
                    node.id.as_u64(),
                    bounds,
                    chain,
                    ui_core::geom::Corners::ZERO,
                )
            } else {
                self.list
                    .push_layer(bounds, chain, ui_core::geom::Corners::ZERO)
            });
        }

        self.paint_self(nid);
        if declares {
            // The clip's corners belong to the node's uncut box: an ancestor may have
            // narrowed the mask, and that cut edge stays square.
            self.list.push_clip_shaped(mask, corners, visual);
        }
        // Same order the hit index uses: `z` first, declaration order on a tie.
        // Indexed rather than iterated: painting mutates the tree it is reading.
        let child_count = self.tree.children_in_paint_order(nid).len();
        for i in 0..child_count {
            let Some(child) = self.tree.paint_child_at(nid, i) else {
                continue;
            };
            // An overlay floats above everything it covers, so paint order cannot follow
            // declaration order. `walk()` defers these kinds; paint must match it.
            if self.tree.is_modal(child)
                || self.tree.is_overlay(child)
                || self.tree.is_lifted(child)
            {
                deferred.push(child);
                continue;
            }
            self.paint_node(child, false, deferred);
        }

        // Press ink draws over the node's own shapes and over its children: Material's
        // ripple tints whatever it covers, so an opaque child cannot hide it.
        let ink_args = self
            .tree
            .get(nid)
            .map(|n| (n.id, n.visual_rect(), node_corners(n)));
        if let Some((id, visual, outline)) = ink_args {
            self.paint_press_ink(nid, id, visual, outline);
        }
        // Above the ink as well: the bar is an affordance for the whole viewport, and its
        // rects are a live press target, so it may never be covered by what it scrolls.
        self.paint_scrollbar(nid);

        // The adorner is drawn outside the box and above the subtree, so children can never
        // hide it; it is the last thing this node contributes to the frame.
        self.paint_focus_visual(nid);

        if let Some(index) = layer_index {
            // A frost inside reads its parent, which a skipped frame would not supply; such a
            // subtree has to be rebuilt every frame, so it keeps no cache.
            if self.list.backdrops.len() > backdrops_before {
                self.list.layers[index as usize].cache_key = None;
            }
            self.list.pop_layer();
        }
        if declares {
            self.list.pop_clip();
        }
        self.list.transform = outer;
        self.list.shape_scale = outer_shape;
    }

    fn paint_self(&mut self, nid: ui_core::NodeId) {
        let Some(node) = self.tree.get(nid) else {
            return;
        };
        // Paint where the node *appears*, which differs from where layout put it when a
        // drag is offsetting it.
        let visual = node.visual_rect();
        // Emitted before the fill: the blur must read the backdrop, not this node's own box.
        if let Some(blur) = node.data.params().and_then(|p| p.backdrop_blur) {
            // A panel is shown on a surface of its own: the window is not behind it, so a
            // material that samples a backdrop paints the stand-in it names instead of
            // shading nothing and leaving the panel see-through.
            match (self.painting_panel, blur.material.fallback()) {
                (true, Some(fill)) => {
                    self.list.push_rect(
                        nid,
                        RectPrim {
                            rect: visual,
                            radius: node_corners(node),
                            fill,
                            gradient: None,
                            border: None,
                            border_gradient: None,
                            notch: None,
                        },
                    );
                }
                _ => {
                    let entry = ui_core::draw::BackdropEntry {
                        rect: visual,
                        // The frost is cut by the same clip as the box it shades: a panel
                        // sliding inside its window must not blur the page it hangs over.
                        clip: node.mask,
                        corners: node_corners(node),
                        radius: blur.radius,
                        sigma: blur.sigma,
                        material: blur.material,
                    };
                    self.list.push_backdrop(entry);
                    self.backdrops_now.insert(node.id, entry);
                }
            }
        }
        let corners = node_corners(node);
        // Register the textures first: the shape pass may only read the slot table.
        for spec in node.shapes.iter().filter_map(|s| match s {
            ui_core::draw::Shape::Box {
                image: Some(spec), ..
            } => Some(spec),
            _ => None,
        }) {
            let key = (spec.id, spec.sampling);
            if !self.image_slots.contains_key(&key)
                && let Some(image) = self.images.get(&spec.id)
            {
                let slot = self.list.push_image(ui_core::draw::ImageEntry {
                    sampling: spec.sampling,
                    ..image.entry.clone()
                });
                self.image_slots.insert(key, slot);
            }
        }
        let images = &self.images;
        let slots = &self.image_slots;
        ui_core::draw::push_shapes(&mut self.list, nid, node, visual, |spec, dest| {
            let entry = &images.get(&spec.id)?.entry;
            let (dest, src) = spec.fit.place(dest, (entry.width, entry.height));
            Some(ui_core::draw::ImagePrim {
                rect: dest,
                corners,
                texture: *slots.get(&(spec.id, spec.sampling))?,
                src,
            })
        });
        if let ui_core::NodeData::Text {
            block,
            ink,
            caret,
            selection,
            composing,
            ..
        } = &node.data
        {
            // A caret blinks while an input is being edited; the engine owns the clock, so
            // every field on every design system blinks the same way without a timer each.
            // Recorded before the gate: the dark half still owes the engine a wake, or the
            // caret would go out and never come back.
            self.caret_painted |= caret.is_some() && self.editing.is_some();
            let caret = if self.editing.is_some() && !self.caret_blink_lit(self.frame_now) {
                None
            } else {
                *caret
            };
            self.list.push_text(
                nid,
                TextPrim {
                    block: block.clone(),
                    origin: text_origin(node),
                    max_width: node.wrap_width,
                    layout: node.inline.clone(),
                    height: text_height(node),
                    ink: *ink,
                    caret,
                    selection: selection.clone(),
                    composing: composing.clone(),
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
        }
        if let ui_core::NodeData::Math {
            formula,
            family,
            ink,
        } = &node.data
        {
            self.list.push_math(
                nid,
                ui_core::draw::MathPrim {
                    formula: formula.clone(),
                    family: family.clone(),
                    origin: text_origin(node),
                    ink: *ink,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
        }
    }

    /// A scrollbar's opacity this frame: the fade's value for a bar the engine fades, full
    /// strength for one it does not. A fading bar with no state yet has not appeared.
    fn bar_opacity(&self, id: Id, bar: &ui_core::Scrollbar) -> f32 {
        if !bar.fade {
            return 1.0;
        }
        self.scrollbar_fade.get(&id).map_or(0.0, |f| f.opacity)
    }

    /// Draw the node's scrollbar, if it declared one, in the node's own space.
    ///
    /// The container draws it, not the widget: only the paint pass knows that the content
    /// has already been emitted and the bar must come after it.
    fn paint_scrollbar(&mut self, nid: ui_core::NodeId) {
        let Some(node) = self.tree.get(nid) else {
            return;
        };
        let Some(bar) = node.data.params().and_then(|p| p.scrollbar) else {
            return;
        };
        let opacity = self.bar_opacity(node.id, &bar);
        if opacity <= 0.002 {
            return;
        }
        let origin = node.visual_rect().origin;
        for (_, piece) in bar.bars() {
            if piece.track_fill.a != 0.0 {
                self.list.push_rect(
                    nid,
                    ui_core::draw::RectPrim {
                        rect: piece.track.translate(origin),
                        radius: piece.track_corners,
                        fill: piece.track_fill.faded(opacity),
                        gradient: None,
                        border: None,
                        border_gradient: None,
                        notch: None,
                    },
                );
            }
            self.list.push_rect(
                nid,
                ui_core::draw::RectPrim {
                    rect: piece.thumb.translate(origin),
                    radius: piece.thumb_corners,
                    fill: piece.thumb_fill.faded(opacity),
                    gradient: None,
                    border: None,
                    border_gradient: None,
                    notch: None,
                },
            );
        }
    }

    /// Draw the node's focus adorner, if it holds visible focus and asked for one.
    fn paint_focus_visual(&mut self, nid: ui_core::NodeId) {
        if !self.focus_visible {
            return;
        }
        let Some(node) = self.tree.get(nid) else {
            return;
        };
        if self.focused != Some(node.id) {
            return;
        }
        let Some(visual) = node.data.params().and_then(|p| p.focus_visual) else {
            return;
        };
        let rect = node.visual_rect();
        // XAML writes a negative margin when the adorner reaches outside the control.
        let outer_rect = Rect::new(
            rect.origin.x + visual.margin.left,
            rect.origin.y + visual.margin.top,
            rect.size.w - visual.margin.left - visual.margin.right,
            rect.size.h - visual.margin.top - visual.margin.bottom,
        );
        let base = node_corners(node);
        let ring = |grow: f32| ui_core::geom::Corners {
            tl: base.tl + grow,
            tr: base.tr + grow,
            br: base.br + grow,
            bl: base.bl + grow,
            shape: base.shape,
        };
        if visual.outer_width > 0.0 {
            self.list.push_rect(
                nid,
                ui_core::draw::RectPrim {
                    rect: outer_rect,
                    radius: ring(visual.radius),
                    fill: ui_core::geom::Color::TRANSPARENT,
                    gradient: None,
                    border: Some((visual.outer, visual.outer_width)),
                    border_gradient: None,
                    notch: None,
                },
            );
        }
        if visual.inner_width > 0.0 {
            let w = visual.outer_width;
            let inner_rect = Rect::new(
                outer_rect.origin.x + w,
                outer_rect.origin.y + w,
                (outer_rect.size.w - 2.0 * w).max(0.0),
                (outer_rect.size.h - 2.0 * w).max(0.0),
            );
            self.list.push_rect(
                nid,
                ui_core::draw::RectPrim {
                    rect: inner_rect,
                    radius: ring((visual.radius - w).max(0.0)),
                    fill: ui_core::geom::Color::TRANSPARENT,
                    gradient: None,
                    border: Some((visual.inner, visual.inner_width)),
                    border_gradient: None,
                    notch: None,
                },
            );
        }
    }

    /// The live splats belonging to one node, oldest first.
    fn paint_press_ink(
        &mut self,
        nid: ui_core::NodeId,
        id: Id,
        visual: Rect,
        corners: ui_core::geom::Corners,
    ) {
        if self.splats.is_empty() {
            return;
        }
        let now = self.frame_now;
        // The target is resolved per frame: a shape's rect or a descendant's box may be
        // mid-animation, so the reference must follow, as Flutter's `getRectCallback` does.
        let target = self
            .splats
            .iter()
            .find(|s| s.node == id)
            .map(|s| s.ink.target);
        let clip_geom: Option<(Rect, ui_core::geom::Corners)> = match target {
            Some(ui_core::InkTarget::Rect { rect, corners }) => Some((rect, corners)),
            Some(ui_core::InkTarget::Shape(index)) => self
                .tree
                .get(nid)
                .and_then(|node| node_shape_rect(node, visual, index)),
            Some(ui_core::InkTarget::Descendant(clip_id)) => self
                .tree
                .find_by_id(clip_id)
                .and_then(|cnid| self.tree.get(cnid))
                .map(|cnode| {
                    (
                        cnode.visual_rect().translate(ui_core::geom::Point::new(
                            -visual.origin.x,
                            -visual.origin.y,
                        )),
                        node_corners(cnode),
                    )
                }),
            Some(ui_core::InkTarget::Node) | None => None,
        };
        let clip = clip_geom.map(|(r, _)| r);
        let inks: Vec<(Rect, ui_core::geom::Color)> = self
            .splats
            .iter()
            .filter(|splat| splat.node == id)
            .filter_map(|splat| splat.prim(now, visual, clip))
            .collect();
        if inks.is_empty() {
            return;
        }
        // The effective background owns the curve too: an indicator is a pill, and the
        // splat cut to it must inherit that curve rather than the node's.
        let (clip_rect, clip_corners) = match clip_geom {
            Some((bg, curve)) => (bg.translate(visual.origin), curve),
            None => (visual, corners),
        };
        self.list.push_clip(clip_rect, clip_corners);
        for (rect, fill) in inks {
            self.list.push_rect(
                nid,
                ui_core::draw::RectPrim {
                    rect,
                    radius: ui_core::geom::Corners::all(rect.size.w * 0.5),
                    fill,
                    gradient: None,
                    border: None,
                    border_gradient: None,
                    notch: None,
                },
            );
        }
        self.list.pop_clip();
    }

    fn subtree_ink(&self, nid: ui_core::NodeId) -> Rect {
        let mut acc: Option<Rect> = None;
        let mut stack = vec![nid];
        while let Some(cur) = stack.pop() {
            if let Some(node) = self.tree.get(cur) {
                // A presence scale wraps the node and its subtree, so the ink has to come
                // from the same composed transform paint uses.
                let mut ink = self.tree.scaled_ink_rect(cur);
                if let Some(params) = node.data.params() {
                    // A node that is its own layer reads and writes past its own box.
                    ink = ink.inflate(ui_core::EffectReach::of(&params.effects).actual as f32);
                }
                // A focus ring reaches outside its control; a group at the edge cuts it.
                if self.focus_visible
                    && self.focused == Some(node.id)
                    && let Some(ring) = self.tree.scaled_adorner_rect(cur)
                {
                    ink = ink.union(ring);
                }
                acc = Some(match acc {
                    Some(a) => union_rect(a, ink),
                    None => ink,
                });
            }
            stack.extend(self.tree.children_of(Some(cur)));
        }
        acc.unwrap_or(Rect::ZERO)
    }
}

/// Convenience: build a loop and run `frames` frames.
pub fn run_app<A: App>(
    app: A,
    target: &mut dyn PresentTarget,
    frames: usize,
    pointer: Option<ui_core::geom::Point>,
) -> Vec<FrameOutcome> {
    let mut lp = AppLoop::new(app);
    (0..frames).map(|_| lp.frame(target, pointer)).collect()
}

#[cfg(test)]
mod tests;

/// Direction an arrow key asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// How well `to` sits in `dir` from `from`, as (band miss, gap, misalignment).
///
/// `None` means "not in that direction at all". Ordering is lexicographic, which encodes
/// the rule GTK and Flutter both use: stay in the current band if anything is there, then
/// take the smallest gap, then the best alignment.
fn spatial_score(from: Rect, to: Rect, dir: Direction) -> Option<(f32, f32, f32)> {
    let (a, b) = (from.center(), to.center());
    let (gap, misalign, band) = match dir {
        Direction::Right => (
            to.min_x() - from.max_x(),
            (b.y - a.y).abs(),
            to.min_y() < from.max_y() && to.max_y() > from.min_y(),
        ),
        Direction::Left => (
            from.min_x() - to.max_x(),
            (b.y - a.y).abs(),
            to.min_y() < from.max_y() && to.max_y() > from.min_y(),
        ),
        Direction::Down => (
            to.min_y() - from.max_y(),
            (b.x - a.x).abs(),
            to.min_x() < from.max_x() && to.max_x() > from.min_x(),
        ),
        Direction::Up => (
            from.min_y() - to.max_y(),
            (b.x - a.x).abs(),
            to.min_x() < from.max_x() && to.max_x() > from.min_x(),
        ),
    };
    // Overlapping rects have a negative gap: they are not "beyond" the current one, so
    // fall back to comparing centres. Without this a stacked layout would move nowhere.
    let beyond = match dir {
        Direction::Right => b.x > a.x,
        Direction::Left => b.x < a.x,
        Direction::Down => b.y > a.y,
        Direction::Up => b.y < a.y,
    };
    if gap < 0.0 && !beyond {
        return None;
    }
    Some((if band { 0.0 } else { 1.0 }, gap.max(0.0), misalign))
}

/// True when the node paints something a focus ring could attach to.
fn node_paints(node: &ui_core::Node) -> bool {
    node.shapes.iter().any(|s| match s {
        ui_core::draw::Shape::Box {
            fill,
            gradient,
            border,
            image,
            ..
        } => fill.a > 0.0 || gradient.is_some() || border.is_some() || image.is_some(),
        _ => true,
    })
}

/// A Tab stop: it paints, or it declares the ring it paints once focused.
///
/// A menu row rests on a transparent fill, so "paints nothing" would drop every command out
/// of the keyboard's reach; the declared ring is proof enough that it is a stop.
/// Whether a menu has any row other than a divider.
fn menu_has_rows(items: &[ui_core::menu::MenuItem]) -> bool {
    items
        .iter()
        .any(|item| !matches!(item.kind, ui_core::menu::MenuRow::Separator))
}

fn focus_stop(node: &ui_core::Node) -> bool {
    node.data.params().is_some_and(|p| p.focus_stop)
        || node_paints(node)
        // Text and maths draw from their node data, never from a shape, so neither has a
        // paint to ask; a formula field would otherwise be dropped from every focus request.
        || matches!(node.kind, NodeKind::Text | NodeKind::Math)
        || node.data.params().is_some_and(|p| p.focus_visual.is_some())
}

/// One block a multi-block input draws: a window onto its owner's buffer.
///
/// The blocks a document editor shows are separate text nodes inside one input, because
/// each one lays out at its own width with its own style. Offsets in the input's buffer
/// stay global; a view translates between them and its own drawn text.
#[derive(Clone, Copy)]
struct InputView {
    node: Id,
    /// The buffer offset of this view's first drawn byte.
    base: usize,
    /// How many bytes the view draws. A frame's trailing line break is not drawn, so the
    /// next view starts after a gap.
    len: usize,
}

impl InputView {
    fn end(&self) -> usize {
        self.base + self.len
    }

    /// Whether `at` is a caret position on this view's text. The end counts: the caret
    /// there is the last one on the view's final line.
    fn covers(&self, at: usize) -> bool {
        self.base <= at && at <= self.end()
    }
}

/// The view drawing a caret position, if one does. A position in the bytes between two
/// views (a line break no view draws) belongs to none.
///
/// At a boundary two views contain the position: the later one owns it, because the caret
/// there stands before its first byte. A marker cell therefore hands the caret after its
/// last byte to the text beside it, which is where typing goes. Only past the last drawn
/// byte, where no view starts, does the view ending there keep its own end.
fn view_of(views: &[InputView], at: usize) -> Option<InputView> {
    views
        .iter()
        .find(|view| view.base <= at && at < view.end())
        .or_else(|| views.iter().rev().find(|view| view.covers(at)))
        .copied()
}

/// The offset in a marker column that a vertical step from another column lands on.
///
/// A marker is written back to front from its separator: every item ends with the same
/// space and dot, and the digits grow to their left. Counting back from the end therefore
/// keeps a caret on the dot of `9. ` and of `10. `, which no column of x can, because a
/// digit is not a fixed advance. The cell's first offset is the line's start and stays so.
fn column_offset(from: usize, from_len: usize, to_len: usize) -> usize {
    if from == 0 {
        return 0;
    }
    to_len.saturating_sub(from_len.saturating_sub(from))
}

/// The caret position a step ends on, moved to the nearest drawn byte when the step
/// landed on a line break no view draws.
fn snap_to_drawn(views: &[InputView], at: usize, delta: isize) -> usize {
    if view_of(views, at).is_some() {
        return at;
    }
    if delta < 0 {
        views
            .iter()
            .rev()
            .find(|view| view.end() < at)
            .map(InputView::end)
            .or_else(|| views.first().map(|view| view.base))
            .unwrap_or(at)
    } else {
        views
            .iter()
            .find(|view| at < view.base)
            .map(|view| view.base)
            .or_else(|| views.last().map(InputView::end))
            .unwrap_or(at)
    }
}

/// The view a caret position belongs to, or the nearest one in the direction of travel.
///
/// A position between two views is drawn at the end of the one before it going down and at
/// the start of the one after it going up: taking the far side in either case would make
/// the first step skip the line it was standing on.
fn nearest_view(views: &[InputView], at: usize, delta: isize) -> Option<InputView> {
    if delta < 0 {
        views
            .iter()
            .find(|view| view.base >= at)
            .copied()
            .or_else(|| views.last().copied())
    } else {
        views
            .iter()
            .rev()
            .find(|view| view.end() <= at)
            .copied()
            .or_else(|| views.first().copied())
    }
}

/// The offset a step ends on, moved out of a hidden run in the direction of travel.
///
/// A hidden run is syntax the editor chose not to draw; letting the caret rest inside one
/// would put it where nothing visible is, and a second press would be needed to leave.
fn past_hidden(ranges: &[std::ops::Range<usize>], at: usize, delta: isize) -> usize {
    for range in ranges {
        if range.start < at && at < range.end {
            return if delta < 0 { range.start } else { range.end };
        }
    }
    at
}

/// The first byte of the line beginning at `start` that is not indentation.
fn first_non_space(value: &str, start: usize) -> usize {
    let bytes = value.as_bytes();
    let mut at = start;
    while at < bytes.len() && matches!(bytes[at], b' ' | b'\t') {
        at += 1;
    }
    at
}

/// A deletion that would take part of a hidden run takes all of it.
fn over_hidden(
    ranges: &[std::ops::Range<usize>],
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    for hidden in ranges {
        if hidden.start <= range.start && range.end <= hidden.end {
            return hidden.clone();
        }
    }
    range
}

/// The buffer as drawn: a live preedit is spliced in at the caret.
fn composed_input(value: &str, preedit: Option<&ui_core::Preedit>, caret: usize) -> String {
    let Some(p) = preedit else {
        return value.to_owned();
    };
    ui_core::text_input::composed(value, &p.text, caret).0
}

/// Corner shape this node clips and paints with; `Corners::ZERO` when it has none.
fn node_corners(node: &ui_core::Node) -> ui_core::Corners {
    // A declared curve wins: the shapes are what this frame happens to paint, and a hover
    // layer or a ripple outliving its state layer would otherwise move the curve.
    if let Some(stated) = node.data.params().and_then(|p| p.corners) {
        return stated;
    }
    // The clip curve follows the node's own box shape; sub-shapes never clip children.
    node.shapes
        .iter()
        .find_map(|s| match s {
            ui_core::draw::Shape::Box { corners, .. } => Some(*corners),
            _ => None,
        })
        .unwrap_or(ui_core::Corners::ZERO)
}

/// The rect and curve of one painted shape, in the node's own space; a shadow has neither.
fn node_shape_rect(
    node: &ui_core::Node,
    visual: Rect,
    index: usize,
) -> Option<(Rect, ui_core::geom::Corners)> {
    match node.shapes.get(index)? {
        ui_core::draw::Shape::Box { corners, .. } => Some((
            Rect::from_origin_size(ui_core::geom::Point::ZERO, visual.size),
            *corners,
        )),
        ui_core::draw::Shape::Rect { rect, corners, .. } => Some((*rect, *corners)),
        ui_core::draw::Shape::Outline { corners, .. } => Some((
            Rect::from_origin_size(ui_core::geom::Point::ZERO, visual.size),
            *corners,
        )),
        // A shadow paints no box, and an arc or an ellipse is no mask for a ripple's ink.
        ui_core::draw::Shape::Shadow { .. }
        | ui_core::draw::Shape::Sector { .. }
        | ui_core::draw::Shape::Ellipse { .. }
        | ui_core::draw::Shape::Line { .. } => None,
    }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    let min_x = a.min_x().min(b.min_x());
    let min_y = a.min_y().min(b.min_y());
    let max_x = a.max_x().max(b.max_x());
    let max_y = a.max_y().max(b.max_y());
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

fn hash_rect(r: &Rect, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    r.origin.x.to_bits().hash(h);
    r.origin.y.to_bits().hash(h);
    r.size.w.to_bits().hash(h);
    r.size.h.to_bits().hash(h);
}

fn hash_color(c: &ui_core::geom::Color, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    for v in [c.r, c.g, c.b, c.a] {
        v.to_bits().hash(h);
    }
}
