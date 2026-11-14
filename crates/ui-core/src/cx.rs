//! The engine's build context: this frame's facts, its routing, and its animation store.
//!
//! This layer knows nothing about styles or widgets, which is what keeps the dependency
//! direction (`ui-core` → `ui-platform` → `ui-widgets`) intact. Anything style-shaped is
//! added by a wrapper one layer up.

use crate::geom::Point;
use crate::id::{Id, NodeId};
use crate::memo::{CachedKey, DeclCache, Dep, Entry, Recorder};
use crate::tree::Tree;
use crate::{AnimSpec, AnimStore};

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

/// One `SCROLL_OWNED` container's geometry for the build to read, both axes: content
/// extent, viewport size, offset, and whether a wheel moved it since the previous frame.
#[derive(Clone, Debug, Copy, PartialEq)]
pub struct ScrollGeom {
    pub id: Id,
    pub extent_w: f32,
    pub extent_h: f32,
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    /// A wheel moved it since the previous frame; an auto scrollbar fades in on it.
    pub active: bool,
}

/// The host's formula layout.
///
/// A build runs before layout, so a formula control has to turn LaTeX into a display list
/// itself; the host owns the math face, and this is the only door onto it.
pub trait MathLayout {
    /// The family a formula asks for when its caller names none, or `None` when the host
    /// has no installed face with a MATH table.
    fn default_family(&mut self) -> Option<String>;

    /// Lay `source` out with the face `family` resolves to.
    ///
    /// A source with errors comes back laid out, with one [`MathErrorMark`] per refused
    /// run; the error is reserved for a missing face or a chain failure.
    ///
    /// [`MathErrorMark`]: crate::math::MathErrorMark
    fn layout(
        &mut self,
        family: &str,
        source: &str,
        size: f32,
        display: bool,
        style: Option<crate::math::MathErrorStyle>,
    ) -> Result<crate::math::Formula, crate::math::MathError>;
}

/// One engine-owned scroller's stored offset, the source of truth across frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollOffset {
    pub x: f32,
    pub y: f32,
}

/// An app's request to move an engine-owned scroller, merged after the view returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollRequest {
    pub id: Id,
    pub x: f32,
    pub y: f32,
    /// Ease to the offset instead of landing on it at once.
    pub animate: bool,
}

/// A replacement a build asks an engine-owned text input to make, merged after the view
/// returns.
///
/// The engine applies it as its own edit, so it lands in the input's history, the caret is
/// placed where the caller says, and the app still receives the new buffer as
/// `Change::Text`. A control that rewrites a buffer it does not own — a list break, an
/// indent — has no other way to keep all three.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputEdit {
    /// The text input whose buffer changes.
    pub owner: Id,
    /// The bytes replaced; empty for an insertion.
    pub range: Range<usize>,
    pub text: String,
    /// Where the caret lands in the buffer the edit produces.
    pub caret: usize,
    /// The selection the edit leaves, as `(anchor, caret)` in the buffer it produces.
    /// `None` leaves a collapsed caret at `caret`; an indent keeps the lines selected.
    pub selection: Option<(usize, usize)>,
    /// A control's edit rather than the caret's: it applies to `owner` whether or not that
    /// input has focus, and moves its caret only by what the edit did.
    pub detached: bool,
}

/// Engine facts for one build, plus the only writable door it offers.
/// Everything the engine routes this frame, as named fields: positional arguments this
/// numerous invite silent swaps, and every new fact would break every call site.
#[derive(Clone, Debug, Default)]
pub struct Routing {
    pub hovered: Option<Id>,
    pub pressed: Option<Id>,
    /// The clicked node and its ancestors, root first; empty when nothing was clicked.
    pub clicked_path: Vec<Id>,
    /// The keyboard-activated node and its ancestors, root first; empty when nothing fired.
    pub activated_path: Vec<Id>,
    /// The node a drag was released on, and its ancestors, root first.
    pub dropped_path: Vec<Id>,
    /// The node under the pointer that accepts drops, when a drag ended this frame.
    pub drop_target: Option<Id>,
    /// The node a live drag is over: it accepts what the drag offers.
    pub drag_target: Option<Id>,
    /// That node's ancestors, root first; empty when no drag is over the surface.
    pub drag_path: Vec<Id>,
    /// The MIME types the live drag offers, best first.
    pub drag_types: Option<Rc<Vec<String>>>,
    pub focused: Option<Id>,
    /// True when focus came from the keyboard, which is what a focus ring keys on.
    pub focus_visible: bool,
    /// The popup a press outside it dismissed this frame: the light-dismiss every flyout has.
    pub dismissed: Option<Id>,
    /// The chord that fired this frame and the node that declared it.
    pub shortcut: Option<(Id, crate::shortcut::ShortcutId)>,
    /// The buffer of the input holding the caret, on a frame that fired a shortcut. An
    /// edit is computed against this, because the app's parsed copy of the source is a
    /// frame behind the bytes the engine will apply it to.
    pub input: Option<(Id, String)>,
    /// The focused input's selection at the moment the chord fired. A burst can carry later
    /// keys before the frame answers, and their caret or selection is not what the chord asked
    /// about — an edit replacing the selection would otherwise delete whatever they selected.
    pub shortcut_selection: Option<(Id, crate::selection::TextSelection)>,
    /// The text input holding the caret and the caret's byte offset, when one is focused.
    pub caret: Option<(Id, usize)>,
    /// The caret's x inside the text node that draws a buffer, in that node's layout space.
    pub caret_in_text: Option<(Id, f32)>,
    /// Every text node a selection covers, each with the bytes selected in it. One entry for
    /// a selection inside a single block; several when a drag reached across blocks.
    pub selection: std::rc::Rc<[(Id, crate::selection::TextSelection)]>,
    /// The text input an input method is composing into, and the preedit string.
    pub preedit: Option<(Id, crate::ime::Preedit)>,
    pub hovered_path: Vec<Id>,
    pub pressed_path: Vec<Id>,
    pub drag_owner: Option<Id>,
    /// Whether the live press has travelled past the click threshold.
    pub drag_started: bool,
    /// Pointer movement since the previous frame; the reference accumulates it directly.
    pub drag_step: Option<Point>,
    /// Where the live press landed, in the surface's own coordinates.
    pub press_origin: Option<Point>,
    pub local: Option<Point>,
    pub disabled: Vec<Id>,
}

pub struct Cx<'a> {
    /// Frame serial; every node in this build sees the same value.
    frame: u64,
    width: u32,
    height: u32,
    /// Seconds since the engine's epoch; the engine ticks its clock once per frame.
    now: f64,
    /// Whole-frame step, for readers that want it. Animations use `now` only.
    dt: f32,
    scale: f32,
    pointer: Option<Point>,
    /// Held modifiers at build time; a shortcut editor shows them, an Alt-drag reads them.
    modifiers: crate::event::Modifiers,
    /// The clicked node's ancestor chain, root first; empty when nothing was clicked.
    clicked_path: Vec<Id>,
    /// The keyboard-activated node's ancestor chain, root first.
    activated_path: Vec<Id>,
    /// The drag-release chain, root first.
    dropped_path: Vec<Id>,
    /// The node a drag was released onto, when it accepts drops.
    drop_target: Option<Id>,
    /// The node a live drag is over, when it accepts what the drag offers.
    drag_target: Option<Id>,
    /// That node's ancestors, root first.
    drag_path: Vec<Id>,
    /// The MIME types the live drag offers.
    drag_types: Option<Rc<Vec<String>>>,
    /// Every node folded as inert this frame; normally empty.
    disabled: Vec<Id>,
    hovered: Option<Id>,
    pressed: Option<Id>,
    focused: Option<Id>,
    focus_visible: bool,
    dismissed: Option<Id>,
    shortcut: Option<(Id, crate::shortcut::ShortcutId)>,
    input: Option<(Id, String)>,
    shortcut_selection: Option<(Id, crate::selection::TextSelection)>,
    caret: Option<(Id, usize)>,
    caret_in_text: Option<(Id, f32)>,
    selection: Rc<[(Id, crate::selection::TextSelection)]>,
    /// The selection a live composition began with; reveal is cut from it, not the
    /// live one, so the bytes under the composition do not change look.
    reveal: Option<(Id, crate::selection::TextSelection)>,
    preedit: Option<(Id, crate::ime::Preedit)>,
    /// Scroll geometry per `SCROLL_OWNED` container, both axes.
    scrolls: Vec<ScrollGeom>,
    /// Offsets the app asked to write this frame, shared across reborrows.
    scroll_req: Rc<RefCell<Vec<ScrollRequest>>>,
    /// Nodes the build asked to keep visible, resolved once geometry is known.
    bring_req: Rc<RefCell<Vec<Id>>>,
    /// Edits the build asked an engine-owned input to make, shared across reborrows.
    edit_req: Rc<RefCell<Vec<InputEdit>>>,
    /// Focus the build asked for, applied once the declaration names the node.
    focus_req: Rc<RefCell<Vec<Id>>>,
    /// Payloads the build asked to put on the clipboard; the engine hands them to the
    /// platform.
    copy_req: Rc<RefCell<Vec<crate::clipboard::ClipboardContent>>>,
    /// A build asked for its surface to close; the host owns the surface, not the app.
    close_req: Rc<Cell<bool>>,
    /// Pointer steps the build asked for, in logical points; only the platform can move
    /// the real pointer, so these are requests like a copy.
    nudge_req: Rc<RefCell<Vec<(f32, f32)>>>,
    /// Pictures the build produced, keyed by their stable texture id.
    ///
    /// A content that rasterizes at the drawn size mints a new one when the pin zooms, and
    /// only a build knows the zoom; the engine drains these after the view returns.
    image_req: Rc<RefCell<Vec<crate::draw::ImageEntry>>>,
    /// Canvases the build wants painted: content at its own resolution, kept on the GPU
    /// between frames. Drained with the pictures, after the view returns.
    canvas_req: Rc<RefCell<Vec<crate::draw::Canvas>>>,
    hovered_path: Vec<Id>,
    pressed_path: Vec<Id>,
    drag_owner: Option<Id>,
    /// Whether the live press has travelled past the click threshold.
    drag_started: bool,
    /// Pointer movement since the previous event while a drag is live.
    drag_step: Option<Point>,
    /// Where the live press landed, in the surface's own coordinates: the fixed end of a
    /// drag, so a control measures `pointer - press_origin` instead of accumulating steps.
    press_origin: Option<Point>,
    /// The pointer in the pressed node's own space, when a press is live.
    ///
    /// A control cannot compute its own value from a surface point: only the node that was
    /// hit knows where the finger is inside it.
    local: Option<Point>,
    anim: &'a mut AnimStore,
    /// Dependency frames for the declaration cache; shared by every reborrow.
    rec: Rc<RefCell<Recorder>>,
    /// The cache itself, when a frame is building under one.
    cache: Option<&'a mut DeclCache>,
    /// Entries on the validation stack; a key met twice means the graph has a loop.
    visiting: crate::collections::FastSet<Id>,
    /// The engine's text measurer, when the host has one: a build may size a label
    /// before layout runs, which is how an outline cuts its notch.
    measure: Option<Rc<RefCell<dyn crate::TextMeasure>>>,
    /// The host's formula layout, when the host has a math face: a build turns LaTeX into
    /// the display list the layout then sizes.
    math: Option<Rc<RefCell<dyn crate::MathLayout>>>,
    /// The host's structural editing, over the same faces that paint the formula.
    math_edit: Option<Rc<RefCell<dyn crate::MathEdit>>>,
    /// The retained tree, for a build that needs last frame's geometry.
    tree: Option<&'a Tree>,
}

impl<'a> Cx<'a> {
    /// Assemble a context; the engine is the only intended caller.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        anim: &'a mut AnimStore,
        frame: u64,
        width: u32,
        height: u32,
        now: f64,
        dt: f32,
        scale: f32,
        pointer: Option<Point>,
    ) -> Self {
        Self {
            frame,
            width,
            height,
            now,
            dt,
            scale,
            pointer,
            modifiers: crate::event::Modifiers::default(),
            hovered: None,
            pressed: None,
            clicked_path: Vec::new(),
            activated_path: Vec::new(),
            dropped_path: Vec::new(),
            drop_target: None,
            drag_target: None,
            drag_path: Vec::new(),
            drag_types: None,
            focused: None,
            focus_visible: false,
            dismissed: None,
            shortcut: None,
            input: None,
            shortcut_selection: None,
            caret: None,
            caret_in_text: None,
            selection: Rc::from([]),
            reveal: None,
            preedit: None,
            scrolls: Vec::new(),
            scroll_req: Rc::new(RefCell::new(Vec::new())),
            bring_req: Rc::new(RefCell::new(Vec::new())),
            edit_req: Rc::new(RefCell::new(Vec::new())),
            focus_req: Rc::new(RefCell::new(Vec::new())),
            copy_req: Rc::new(RefCell::new(Vec::new())),
            close_req: Rc::new(Cell::new(false)),
            nudge_req: Rc::new(RefCell::new(Vec::new())),
            image_req: Rc::new(RefCell::new(Vec::new())),
            canvas_req: Rc::new(RefCell::new(Vec::new())),
            hovered_path: Vec::new(),
            pressed_path: Vec::new(),
            drag_owner: None,
            drag_started: false,
            drag_step: None,
            press_origin: None,
            local: None,
            disabled: Vec::new(),
            anim,
            rec: Rc::new(RefCell::new(Recorder::default())),
            cache: None,
            visiting: crate::collections::FastSet::default(),
            measure: None,
            math: None,
            math_edit: None,
            tree: None,
        }
    }

    /// Share the layout's own measurer with this build, so a widget may size text early.
    pub fn attach_measure(&mut self, measure: Rc<RefCell<dyn crate::TextMeasure>>) {
        self.measure = Some(measure);
    }

    /// The size `block` takes wrapped at `max_width`, measured the way layout will measure it.
    ///
    /// `None` when the host supplied no measurer (tests, headless layout): a caller must then
    /// fall back to something it can derive without text, not invent a width.
    pub fn text_size(
        &self,
        block: &crate::text::TextBlock,
        max_width: f32,
    ) -> Option<crate::geom::Size> {
        let measure = self.measure.as_ref()?;
        // A held borrow means paint is mid-flight; measuring is an optimisation, not a need.
        let mut measure = measure.try_borrow_mut().ok()?;
        Some(measure.measure(block, max_width))
    }

    /// The layout `block` takes wrapped at `max_width`, for a caller that needs the line
    /// baseline as well as the size — an inline object aligns on that baseline.
    pub fn text_layout(
        &self,
        block: &crate::text::TextBlock,
        max_width: f32,
    ) -> Option<std::sync::Arc<crate::text::InlineLayout>> {
        let measure = self.measure.as_ref()?;
        let mut measure = measure.try_borrow_mut().ok()?;
        Some(measure.layout(block, max_width))
    }

    /// Share the host's formula layout with this build.
    pub fn attach_math(&mut self, math: Rc<RefCell<dyn crate::MathLayout>>) {
        self.math = Some(math);
    }

    /// Share the host's structural formula editing with this build.
    pub fn attach_math_edit(&mut self, edit: Rc<RefCell<dyn crate::MathEdit>>) {
        self.math_edit = Some(edit);
    }

    /// Run `f` against the host's editing, when it has one and paint is not mid-flight.
    pub fn with_math_edit<R>(&self, f: impl FnOnce(&mut dyn crate::MathEdit) -> R) -> Option<R> {
        let edit = self.math_edit.as_ref()?;
        let mut edit = edit.try_borrow_mut().ok()?;
        Some(f(&mut *edit))
    }

    /// The family a formula uses when its caller names none.
    ///
    /// `None` when the host supplied no math layout or has no math face: a formula control
    /// then draws its source instead of an empty box.
    pub fn math_family(&self) -> Option<String> {
        let math = self.math.as_ref()?;
        // A held borrow means paint is mid-flight; the source text is the fallback.
        let mut math = math.try_borrow_mut().ok()?;
        math.default_family()
    }

    /// `source` laid out as a formula, for a node that will paint it.
    ///
    /// `style` colours the runs the parser refuses; without one they are marked in the
    /// formula's own ink.
    pub fn math_layout(
        &self,
        family: &str,
        source: &str,
        size: f32,
        display: bool,
        style: Option<crate::math::MathErrorStyle>,
    ) -> Result<crate::math::Formula, crate::math::MathError> {
        let Some(math) = self.math.as_ref() else {
            return Err(crate::math::MathError::NoFace);
        };
        // A held borrow means paint is mid-flight; the control falls back to its source text.
        let Ok(mut math) = math.try_borrow_mut() else {
            return Err(crate::math::MathError::NoFace);
        };
        math.layout(family, source, size, display, style)
    }

    /// Build under a declaration cache, so subtrees whose facts held can be reused.
    pub fn attach_cache(&mut self, cache: &'a mut DeclCache) {
        self.cache = Some(cache);
    }

    /// Throw away every cached declaration; an app that mutated state must call this.
    pub fn invalidate_decls(&mut self) {
        if let Some(cache) = self.cache.as_deref_mut() {
            cache.clear();
        }
    }

    /// Stop recording while an engine pass reads routing after the declarations are built.
    ///
    /// Those reads cannot change this frame's declarations — they are already built — so they
    /// must not be mistaken for app state that later builds depend on.
    pub fn mute(&self) -> bool {
        self.rec.borrow_mut().mute()
    }

    pub fn unmute(&self, was: bool) {
        self.rec.borrow_mut().unmute(was);
    }

    fn record(&self, dep: Dep) {
        if let Ok(mut rec) = self.rec.try_borrow_mut() {
            rec.push(dep);
        }
    }

    /// A handle on the recorder, for a narrower context that must record on its own.
    pub fn recorder_handle(&self) -> Rc<RefCell<Recorder>> {
        Rc::clone(&self.rec)
    }

    /// Whether the entries may stand, given the facts the app read outside every build.
    ///
    /// Called once per frame, before the app builds: it takes the previous frame's readings,
    /// which must cover a whole frame for the comparison to mean anything.
    pub fn settle_outside(&mut self) -> bool {
        let deps = self.rec.borrow_mut().take_unattributed();
        match self.cache.as_deref_mut() {
            Some(cache) => cache.settle_outside(&deps),
            None => true,
        }
    }

    /// Whether a build in this frame asked for the clock; resets the flag.
    pub fn take_time_driven(&mut self) -> bool {
        self.rec.borrow_mut().take_time_driven()
    }

    /// Whether this frame's builds read the raw pointer position; resets the flag.
    pub fn take_pointer_read(&mut self) -> bool {
        self.rec.borrow_mut().take_pointer_read()
    }

    /// Whether this frame's builds read the held modifiers; resets the flag.
    pub fn take_modifier_read(&mut self) -> bool {
        self.rec.borrow_mut().take_modifier_read()
    }

    /// Start recording the facts one element's build reads.
    pub fn begin_record(&mut self) {
        self.rec.borrow_mut().begin();
    }

    /// Stop recording and store the declaration under `key`, if it read any fact.
    ///
    /// A declaration that recorded nothing offers no way to prove it still holds; caching it
    /// would freeze the subtree for good, so it is kept out of the cache.
    pub fn end_record(&mut self, key: CachedKey, decl: crate::Decl) -> Rc<crate::Decl> {
        let (deps, mut whole, kids) = self.rec.borrow_mut().end_whole();
        // A declaration that declares an input's bytes is live state, not a projection of
        // its inputs: the engine edits that buffer in place, so replaying the declaration
        // would restore bytes the user already changed.
        if decl
            .data
            .params()
            .is_some_and(|params| params.text_input.is_some())
        {
            whole = false;
        }
        let decl = Rc::new(decl);
        let frame = self.frame;
        if let Some(cache) = self.cache.as_deref_mut() {
            cache.insert(
                key.id,
                Entry {
                    decl: Rc::clone(&decl),
                    deps,
                    props: key.props,
                    whole,
                    kids,
                    checked: frame,
                },
            );
        }
        decl
    }

    /// Note that the child just built was rebuilt: its parent's declaration must not be reused.
    pub fn note_rebuilt_child(&mut self) {
        self.rec.borrow_mut().note_rebuilt_child();
    }

    /// Note an element child built under the current frame, so its entry is validated first.
    pub fn note_element_child(&mut self, id: Id) {
        self.rec.borrow_mut().note_element_child(id);
    }

    /// The cached declaration for `key`, if it was built for facts that still hold.
    pub fn cached_decl(&mut self, key: CachedKey) -> Option<Rc<crate::Decl>> {
        self.cached_decl_matching(key, |_| true)
    }

    /// Same lookup, with the caller able to compare the cached declaration against its own
    /// inputs: a container's shell is app-built, so only a field-by-field match proves it.
    pub fn cached_decl_matching(
        &mut self,
        key: CachedKey,
        same: impl FnOnce(&crate::Decl) -> bool,
    ) -> Option<Rc<crate::Decl>> {
        // The element's own inputs are half of what the declaration was built from; when
        // they moved, every recorded fact may still hold and the entry is still stale.
        // `whole` is part of the proof: an entry built while a child was rebuilt embeds that
        // child's old declaration, so reusing it would freeze the change that caused it.
        let fresh = self
            .cache
            .as_ref()
            .and_then(|c| c.get(key.id))
            .is_some_and(|e| e.props == key.props && e.whole && same(&e.decl));
        if !fresh {
            if let Some(cache) = self.cache.as_deref_mut() {
                cache.note(false);
            }
            return None;
        }
        let key = key.id;
        let hit = self.validate(key);
        if let Some(cache) = self.cache.as_deref_mut() {
            cache.note(hit);
        }
        if hit {
            self.cache.as_ref()?.get(key).map(|e| Rc::clone(&e.decl))
        } else {
            None
        }
    }

    /// Whether `key` and every cached descendant still read the facts they were built from.
    fn validate(&mut self, key: Id) -> bool {
        let frame = self.frame;
        let (deps, kids) = {
            let Some(entry) = self.cache.as_ref().and_then(|c| c.get(key)) else {
                return false;
            };
            if entry.checked == frame {
                return true;
            }
            // Only the element children the build recorded can have entries. A declaration
            // child has none: its inputs belong to this entry's own fingerprint.
            let kids = entry.kids.clone();
            (entry.deps.clone(), kids)
        };
        // A stale entry can point back at an ancestor after the tree was reshaped; a loop
        // here would recurse forever, so a second visit is simply a miss.
        if !self.visiting.insert(key) {
            return false;
        }
        // Recording is muted while replaying: validation must not feed its own reads back.
        let was = self.rec.borrow_mut().mute();
        let now = self.now;
        let held = deps.iter().all(|dep| self.dep_holds(dep, now));
        self.rec.borrow_mut().unmute(was);
        if !held {
            self.visiting.remove(&key);
            return false;
        }
        if !kids.into_iter().all(|kid| self.validate(kid)) {
            self.visiting.remove(&key);
            return false;
        }
        self.visiting.remove(&key);
        if let Some(cache) = self.cache.as_deref_mut() {
            cache.mark_checked(key, frame);
        }
        true
    }

    fn dep_holds(&mut self, dep: &Dep, now: f64) -> bool {
        match dep {
            Dep::Hovered(v) => self.hovered == *v,
            Dep::Pressed(v) => self.pressed == *v,
            Dep::Focused(v) => self.focused == *v,
            Dep::FocusVisible(v) => self.focus_visible == *v,
            Dep::Dismissed(v) => self.dismissed == *v,
            Dep::Shortcut(v) => self.shortcut == *v,
            Dep::Caret(v) => self.caret == *v,
            Dep::CaretInText(v) => self.caret_in_text == *v,
            Dep::Selection(v) => self.selection == *v,
            Dep::Preedit(v) => self.preedit == *v,
            Dep::Scroll {
                id,
                extent_w,
                extent_h,
                viewport_w,
                viewport_h,
                offset_x,
                offset_y,
                active,
            } => self
                .scrolls
                .iter()
                .find(|g| g.id == *id)
                .map(|g| {
                    g.extent_w == *extent_w
                        && g.extent_h == *extent_h
                        && g.viewport_w == *viewport_w
                        && g.viewport_h == *viewport_h
                        && g.offset_x == *offset_x
                        && g.offset_y == *offset_y
                        && g.active == *active
                })
                .unwrap_or(false),
            Dep::HoveredPath(v) => self.hovered_path == *v,
            Dep::PressedPath(v) => self.pressed_path == *v,
            Dep::HoveredWithin(id, v) => self.hovered_path.contains(id) == *v,
            Dep::PressedWithin(id, v) => self.pressed_path.contains(id) == *v,
            Dep::ClickedWithin(id, v) => self.clicked_path.contains(id) == *v,
            Dep::ActivatedWithin(id, v) => self.activated_path.contains(id) == *v,
            Dep::DroppedWithin(id, v) => self.dropped_path.contains(id) == *v,
            Dep::DragOverWithin(id, v) => self.drag_path.contains(id) == *v,
            Dep::DragTypes(v) => self.drag_types == *v,
            Dep::DraggingWithin(id, v) => (self.drag_owner == Some(*id)) == *v,
            Dep::DragStarted(id, v) => self.drag_started_within_now(*id) == *v,
            Dep::Disabled(id, v) => self.disabled.contains(id) == *v,
            Dep::Local(v) => self.local == *v,
            Dep::DragStep(v) => self.drag_step == *v,
            Dep::PressOrigin(v) => self.press_origin == *v,
            Dep::Modifiers(bits) => {
                let m = self.modifiers;
                let now =
                    m.shift as u8 | (m.ctrl as u8) << 1 | (m.alt as u8) << 2 | (m.logo as u8) << 3;
                now == *bits
            }
            Dep::Anim { .. }
            | Dep::AnimColour { .. }
            | Dep::AnimStops { .. }
            | Dep::Value { .. }
            | Dep::Status { .. }
            | Dep::Age { .. } => crate::memo::anim_still_matches(self.anim, dep, now),
            Dep::Now(bits) => self.now.to_bits() == *bits,
            Dep::Size(w, h) => (self.width, self.height) == (*w, *h),
            Dep::Scale(bits) => self.scale.to_bits() == *bits,
            Dep::Dt(bits) => self.dt.to_bits() == *bits,
            Dep::Frame(f) => self.frame == *f,
            Dep::Pointer(p) => self.pointer == *p,
            Dep::Volatile => false,
        }
    }

    /// Attach this frame's routing; called once, before any node is built.
    pub fn route(&mut self, r: Routing) {
        self.hovered = r.hovered;
        self.pressed = r.pressed;
        self.clicked_path = r.clicked_path;
        self.activated_path = r.activated_path;
        self.dropped_path = r.dropped_path;
        self.drop_target = r.drop_target;
        self.drag_target = r.drag_target;
        self.drag_path = r.drag_path;
        self.drag_types = r.drag_types;
        self.focused = r.focused;
        self.focus_visible = r.focus_visible;
        self.dismissed = r.dismissed;
        self.shortcut = r.shortcut;
        self.input = r.input;
        self.shortcut_selection = r.shortcut_selection;
        self.caret = r.caret;
        self.caret_in_text = r.caret_in_text;
        self.selection = r.selection;
        self.preedit = r.preedit;
        self.hovered_path = r.hovered_path;
        self.pressed_path = r.pressed_path;
        self.drag_owner = r.drag_owner;
        self.drag_started = r.drag_started;
        self.drag_step = r.drag_step;
        self.press_origin = r.press_origin;
        self.local = r.local;
        self.disabled = r.disabled;
    }

    /// Whether a drag ended on this node or a descendant this frame.
    pub fn dropped_within(&self, id: Id) -> bool {
        let v = self.dropped_path.contains(&id);
        self.record(Dep::DroppedWithin(id, v));
        v
    }

    /// The node a drag was released onto this frame, when it accepts drops.
    ///
    /// Not recorded: it is only ever set on the frame `dropped_within` turns true, and
    /// cleared on the frame that turns it false, so that dependency already covers it.
    pub fn drop_target(&self) -> Option<Id> {
        self.drop_target
    }

    /// Whether a live drag is over this node or a descendant that accepts it.
    pub fn drag_over_within(&self, id: Id) -> bool {
        let v = self.drag_path.contains(&id);
        self.record(Dep::DragOverWithin(id, v));
        v
    }

    /// The node a live drag is over, when it accepts what the drag offers.
    ///
    /// Not recorded, for the same reason as [`Cx::drop_target`]: the ancestor path answers
    /// it, and that read is what the cache holds.
    pub fn drag_over(&self) -> Option<Id> {
        self.drag_target
    }

    /// The MIME types a live drag offers, best first; `None` when no drag is over the
    /// surface.
    pub fn drag_types(&self) -> Option<&[String]> {
        self.record(Dep::DragTypes(self.drag_types.clone()));
        self.drag_types.as_deref().map(Vec::as_slice)
    }

    /// Whether this node or a descendant was activated by the keyboard this frame; a snackbar
    /// action is a child of the bar that reports it, exactly as with `clicked_within`.
    pub fn activated_within(&self, id: Id) -> bool {
        let v = self.activated_path.contains(&id);
        self.record(Dep::ActivatedWithin(id, v));
        v
    }

    /// The popup a press outside it dismissed this frame, if any.
    pub fn dismissed(&self) -> Option<Id> {
        self.record(Dep::Dismissed(self.dismissed));
        self.dismissed
    }

    /// The chord that fired this frame, and the node that declared it.
    ///
    /// A burst may fire several; the engine answers them one per frame, in order.
    pub fn shortcut(&self) -> Option<(Id, crate::shortcut::ShortcutId)> {
        self.record(Dep::Shortcut(self.shortcut));
        self.shortcut
    }

    /// The live buffer of the text input `id`, on the frame a shortcut fired into it.
    ///
    /// An edit must be computed against the bytes it will be applied to. The app's parsed
    /// copy of the source is one frame behind the buffer, so a key that arrives in the same
    /// burst as typed text would otherwise be computed against the text before it.
    pub fn input_value(&self, id: Id) -> Option<&str> {
        self.input
            .as_ref()
            .filter(|(owner, _)| *owner == id)
            .map(|(_, value)| value.as_str())
    }

    /// The input `id` held selected when a shortcut fired into it.
    ///
    /// The frame answers a burst of keys, so the live selection may already belong to a later
    /// one: an edit computed from it would replace text the user selected after the chord.
    /// Only valid on a shortcut frame, like [`Cx::input_value`].
    pub fn shortcut_selection(&self, id: Id) -> Option<crate::selection::TextSelection> {
        self.shortcut_selection
            .filter(|(owner, _)| *owner == id)
            .map(|(_, selection)| selection)
    }

    /// Whether a click completed on this node or a descendant this frame.
    pub fn clicked_within(&self, id: Id) -> bool {
        let v = self.clicked_path.contains(&id);
        self.record(Dep::ClickedWithin(id, v));
        v
    }

    /// Whether this node is inert; a disabled ancestor makes every descendant inert.
    pub fn is_disabled(&self, id: Id) -> bool {
        let v = self.disabled.contains(&id);
        self.record(Dep::Disabled(id, v));
        v
    }

    /// The pointer in the pressed node's own space.
    /// Pointer movement since the previous frame, while a drag is live.
    pub fn drag_step(&self) -> Option<Point> {
        self.record(Dep::DragStep(self.drag_step));
        self.drag_step
    }

    pub fn local(&self) -> Option<Point> {
        self.record(Dep::Local(self.local));
        self.local
    }

    /// Where the live press landed, in surface coordinates.
    ///
    /// The dragging object moves by `pointer - press_origin`, an absolute measurement that
    /// cannot drift: unlike a sum of per-event steps, it does not accumulate rounding or a
    /// frame's worth of stale input.
    pub fn press_origin(&self) -> Option<Point> {
        self.record(Dep::PressOrigin(self.press_origin));
        self.press_origin
    }

    pub fn hovered(&self) -> Option<Id> {
        self.record(Dep::Hovered(self.hovered));
        self.hovered
    }

    pub fn pressed(&self) -> Option<Id> {
        self.record(Dep::Pressed(self.pressed));
        self.pressed
    }

    /// True when focus arrived by keyboard, so a focus ring is expected.
    pub fn focus_visible(&self) -> bool {
        self.record(Dep::FocusVisible(self.focus_visible));
        self.focus_visible
    }

    pub fn focused(&self) -> Option<Id> {
        self.record(Dep::Focused(self.focused));
        self.focused
    }

    /// The caret's byte offset inside the text input `id`, when that input has focus.
    pub fn caret_for(&self, id: Id) -> Option<usize> {
        self.record(Dep::Caret(self.caret));
        self.caret
            .filter(|(at, _)| *at == id)
            .map(|(_, offset)| offset)
    }

    /// The caret's x inside the text node `id`, in that node's own layout space.
    ///
    /// Layout space, not visual: a control that scrolls text by this value must not read the
    /// offset it itself applied, or the scroll would chase its own tail.
    pub fn caret_in_text_for(&self, id: Id) -> Option<f32> {
        self.record(Dep::CaretInText(self.caret_in_text));
        self.caret_in_text
            .filter(|(at, _)| *at == id)
            .map(|(_, x)| x)
    }

    /// The selection inside the text node `id`, when the node is one of the blocks a
    /// selection covers.
    pub fn selection_for(&self, id: Id) -> Option<crate::selection::TextSelection> {
        self.record(Dep::Selection(std::rc::Rc::clone(&self.selection)));
        self.selection
            .iter()
            .find(|(owner, _)| *owner == id)
            .map(|(_, selection)| *selection)
    }

    /// Where structure reveal is cut from: the selection a live composition started
    /// with, else the live one.
    pub fn reveal_range_for(&self, id: Id) -> Option<std::ops::Range<usize>> {
        self.record(Dep::Selection(Rc::clone(&self.selection)));
        self.reveal
            .filter(|(owner, _)| *owner == id)
            .map(|(_, selection)| selection.range())
            .or_else(|| {
                self.selection
                    .iter()
                    .find(|(owner, _)| *owner == id)
                    .map(|(_, selection)| selection.range())
            })
    }

    /// The engine hands the selection a composition began with, once a preedit lives.
    pub fn set_reveal(&mut self, reveal: Option<(Id, crate::selection::TextSelection)>) {
        self.reveal = reveal;
    }

    /// The preedit string an input method is composing into `id`, if any.
    pub fn preedit_for(&self, id: Id) -> Option<crate::ime::Preedit> {
        self.record(Dep::Preedit(self.preedit.clone()));
        self.preedit
            .as_ref()
            .filter(|(owner, _)| *owner == id)
            .map(|(_, preedit)| preedit.clone())
    }

    /// The scroll geometry of the `SCROLL_OWNED` container `id`, both axes. `None` when
    /// `id` is not such a container this frame.
    pub fn scroll_of(&self, id: Id) -> Option<ScrollGeom> {
        let got = self.scrolls.iter().find(|g| g.id == id).copied();
        self.record(Dep::Scroll {
            id,
            extent_w: got.map(|g| g.extent_w).unwrap_or(0.0),
            extent_h: got.map(|g| g.extent_h).unwrap_or(0.0),
            viewport_w: got.map(|g| g.viewport_w).unwrap_or(0.0),
            viewport_h: got.map(|g| g.viewport_h).unwrap_or(0.0),
            offset_x: got.map(|g| g.offset_x).unwrap_or(0.0),
            offset_y: got.map(|g| g.offset_y).unwrap_or(0.0),
            active: got.is_some_and(|g| g.active),
        });
        got
    }

    /// Ask an engine-owned scroller to hold this offset. The engine merges the request
    /// after the view returns; a wheel it handled itself still wins over a stale request.
    pub fn set_scroll(&self, id: Id, x: f32, y: f32) {
        self.scroll_req.borrow_mut().push(ScrollRequest {
            id,
            x,
            y,
            animate: false,
        });
    }

    /// Ask an engine-owned scroller to travel to this offset under its spring, so a
    /// programmatic move eases the way a hand would instead of jumping.
    pub fn scroll_to(&self, id: Id, x: f32, y: f32) {
        self.scroll_req.borrow_mut().push(ScrollRequest {
            id,
            x,
            y,
            animate: true,
        });
    }

    /// Take the offsets the app asked for; the engine calls this once the view returns.
    pub fn take_scroll_requests(&self) -> Vec<ScrollRequest> {
        std::mem::take(&mut self.scroll_req.borrow_mut())
    }

    /// Ask the nearest engine-owned scroller to keep `id` visible once this frame is laid
    /// out; WinUI's `BringIntoViewOnFocusChange`, stated by the control that needs it.
    pub fn bring_into_view(&self, id: Id) {
        self.bring_req.borrow_mut().push(id);
    }

    /// Take the nodes the build asked to keep visible; the engine calls this after the view.
    pub fn take_bring_requests(&self) -> Vec<Id> {
        std::mem::take(&mut self.bring_req.borrow_mut())
    }

    /// Ask an engine-owned text input to replace `range` with `text` and put the caret at
    /// `caret`. The engine merges the request after the view returns, records it in that
    /// input's history and reports the new buffer to the app, so a control that rewrites
    /// its own text stays one edit the user can undo — and the caret lands where the
    /// control says instead of where the old offset happens to point.
    pub fn edit_input(
        &self,
        owner: Id,
        range: Range<usize>,
        text: impl Into<String>,
        caret: usize,
    ) {
        self.edit_req.borrow_mut().push(InputEdit {
            owner,
            range,
            text: text.into(),
            caret,
            selection: None,
            detached: false,
        });
    }

    /// Rewrite `range` of `owner`'s buffer without taking the caret: the press of a control
    /// inside a document input flips a byte the caret does not own, and whether that input
    /// has focus is beside the point. Its own caret, if it has one, moves by the edit alone.
    pub fn edit_input_detached(&self, owner: Id, range: Range<usize>, text: impl Into<String>) {
        let caret = range.start;
        self.edit_req.borrow_mut().push(InputEdit {
            owner,
            range,
            text: text.into(),
            caret,
            selection: None,
            detached: true,
        });
    }

    /// Same as [`Cx::edit_input`], leaving `(anchor, caret)` selected. An edit that rewrites
    /// several lines — an indent over a selection — must keep them selected, or the next
    /// command would act on one line and the user would lose what they had chosen.
    pub fn edit_input_selecting(
        &self,
        owner: Id,
        range: Range<usize>,
        text: impl Into<String>,
        selection: (usize, usize),
    ) {
        self.edit_req.borrow_mut().push(InputEdit {
            owner,
            range,
            text: text.into(),
            caret: selection.1,
            selection: Some(selection),
            detached: false,
        });
    }

    /// Take the edits the build asked for; the engine calls this once the view returns.
    pub fn take_input_edits(&self) -> Vec<InputEdit> {
        std::mem::take(&mut self.edit_req.borrow_mut())
    }

    /// Ask for the keyboard focus on `id`.
    ///
    /// A request rather than a move: focus belongs to the engine, and the node it names may
    /// only exist from this very build — a canvas that opens a text field on a press has no
    /// field to press. It is the app's half of "click here and type": the engine applies it
    /// once this declaration is in the tree and laid out, which is when the node can be a
    /// focus stop at all.
    pub fn request_focus(&self, id: Id) {
        self.focus_req.borrow_mut().push(id);
    }

    /// Take the focus the build asked for; the engine calls this once the view returns.
    pub fn take_focus_requests(&self) -> Vec<Id> {
        std::mem::take(&mut self.focus_req.borrow_mut())
    }

    /// Put `text` on the clipboard, as a copy button in a control does.
    ///
    /// A request, not a write: the platform owns the clipboard, and the engine drains this
    /// after the build the same way it drains edits.
    pub fn copy_text(&self, text: impl Into<String>) {
        self.copy_req
            .borrow_mut()
            .push(crate::clipboard::ClipboardContent::Text(text.into()));
    }

    /// Put opaque bytes on the clipboard under one MIME type, such as a PNG image.
    ///
    /// Same contract as [`copy_text`](Self::copy_text): a request the engine drains after
    /// the build.
    pub fn copy_bytes(&self, mime: impl Into<String>, data: impl Into<std::sync::Arc<Vec<u8>>>) {
        self.copy_req
            .borrow_mut()
            .push(crate::clipboard::ClipboardContent::Bytes {
                mime: mime.into(),
                data: data.into(),
            });
    }

    /// Take the copies the build asked for; the engine calls this once the view returns.
    pub fn take_copies(&self) -> Vec<crate::clipboard::ClipboardContent> {
        std::mem::take(&mut self.copy_req.borrow_mut())
    }

    /// Hand the engine a picture this build produced, under its stable texture id.
    ///
    /// A request, not a registration: the engine owns the texture table and drains this
    /// after the view returns, so the frame that produced the pixels paints them.
    pub fn register_image(&self, entry: crate::draw::ImageEntry) {
        self.image_req.borrow_mut().push(entry);
    }

    /// Take the pictures the build registered; the engine calls this once the view returns.
    pub fn take_image_requests(&self) -> Vec<crate::draw::ImageEntry> {
        std::mem::take(&mut self.image_req.borrow_mut())
    }

    /// Declare a canvas this build wants painted, keyed by its stable id.
    ///
    /// The list is in the canvas's own pixels. The engine repaints the target only when
    /// `version` or `size` changes, so a build may hand the same list every frame.
    pub fn canvas(&self, canvas: crate::draw::Canvas) {
        self.canvas_req.borrow_mut().push(canvas);
    }

    /// Take the canvases the build declared; the engine calls this once the view returns.
    pub fn take_canvas_requests(&self) -> Vec<crate::draw::Canvas> {
        std::mem::take(&mut self.canvas_req.borrow_mut())
    }

    /// Ask the host to close this surface after the frame, as a document window does when
    /// its own state says it is finished. The app does not own the surface, so this is a
    /// request like a copy, not an action.
    pub fn close_window(&self) {
        self.close_req.set(true);
    }

    /// Whether the build asked for a close; the engine takes it once per frame.
    pub fn take_close_request(&self) -> bool {
        self.close_req.replace(false)
    }

    /// Move the pointer by `(dx, dy)` logical points, as a keyboard-driven cursor does.
    ///
    /// A step, not a position: the pointer may not be over this surface, and only the
    /// engine knows where it is and how far the surface reaches. The platform may refuse
    /// (no protocol for it, or no pointer focus), and then nothing moves.
    pub fn nudge_pointer(&self, dx: f32, dy: f32) {
        self.nudge_req.borrow_mut().push((dx, dy));
    }

    /// Take the steps the build asked for; the engine calls this once the view returns.
    pub fn take_pointer_nudges(&self) -> Vec<(f32, f32)> {
        std::mem::take(&mut self.nudge_req.borrow_mut())
    }

    /// Hand the build the frame's scroll geometry; the engine calls this before the view.
    pub fn set_scrolls(&mut self, scrolls: Vec<ScrollGeom>) {
        self.scrolls = scrolls;
    }

    pub fn hovered_path(&self) -> &[Id] {
        self.record(Dep::HoveredPath(self.hovered_path.clone()));
        &self.hovered_path
    }

    pub fn pressed_path(&self) -> &[Id] {
        self.record(Dep::PressedPath(self.pressed_path.clone()));
        &self.pressed_path
    }

    /// Whether the pointer is over `id` or anywhere inside it.
    ///
    /// `hovered` alone is the topmost node, so a container is never reported while the
    /// pointer is over its children; the ancestor path answers the container question.
    pub fn hovered_within(&self, id: Id) -> bool {
        let v = self.hovered_path.contains(&id);
        self.record(Dep::HoveredWithin(id, v));
        v
    }

    pub fn pressed_within(&self, id: Id) -> bool {
        let v = self.pressed_path.contains(&id);
        self.record(Dep::PressedWithin(id, v));
        v
    }

    /// Whether a drag that started inside `id` is in progress.
    pub fn dragging_within(&self, id: Id) -> bool {
        let v = self.drag_owner == Some(id);
        self.record(Dep::DraggingWithin(id, v));
        v
    }

    /// Whether the press on `id` has become a real drag, past `DRAG_THRESHOLD`.
    ///
    /// `dragging_within` is armed at the press so a control can hold its value for the whole
    /// gesture; this one starts only when the pointer actually travels, so a tap stays a tap.
    pub fn drag_started_within(&self, id: Id) -> bool {
        let v = self.drag_started_within_now(id);
        self.record(Dep::DragStarted(id, v));
        v
    }

    fn drag_started_within_now(&self, id: Id) -> bool {
        self.drag_started && self.drag_owner == Some(id)
    }

    /// Drive one animation instance toward `target` and read this frame's value.
    pub fn animate(&mut self, id: Id, target: f32, spec: AnimSpec) -> f32 {
        let v = self.anim.animate(id, target, spec, self.now);
        self.record(Dep::Anim {
            id,
            target,
            spec,
            value: v,
        });
        v
    }

    /// Drive one colour toward `target` and read this frame's value.
    ///
    /// All four channels share one clock, so a colour never desyncs from itself.
    pub fn animate_colour(
        &mut self,
        id: Id,
        target: crate::geom::Color,
        spec: AnimSpec,
    ) -> crate::geom::Color {
        let v = self.anim.animate_colour(id, target, spec, self.now);
        self.record(Dep::AnimColour {
            id,
            target,
            spec,
            value: v,
        });
        v
    }

    /// The current value without asking it to move.
    pub fn value(&self, id: Id) -> f32 {
        let v = self.anim.value(id, self.now).unwrap_or(0.0);
        self.record(Dep::Value { id, value: v });
        v
    }

    /// Seconds since this id first asked: a loop's own clock, zero on the first frame.
    ///
    /// Reading it asks for every frame; a control that stops reading stops asking.
    pub fn age(&mut self, id: Id) -> f32 {
        let age = self.anim.age(id, self.now) as f32;
        self.record(Dep::Age { id, age });
        age
    }

    /// Borrow the same facts and store again, so a wrapper can hold it for one build.
    pub fn reborrow(&mut self) -> Cx<'_> {
        Cx {
            frame: self.frame,
            width: self.width,
            height: self.height,
            now: self.now,
            dt: self.dt,
            scale: self.scale,
            pointer: self.pointer,
            modifiers: self.modifiers,
            hovered: self.hovered,
            pressed: self.pressed,
            clicked_path: self.clicked_path.clone(),
            activated_path: self.activated_path.clone(),
            dropped_path: self.dropped_path.clone(),
            drop_target: self.drop_target,
            drag_target: self.drag_target,
            drag_path: self.drag_path.clone(),
            drag_types: self.drag_types.clone(),
            focused: self.focused,
            focus_visible: self.focus_visible,
            dismissed: self.dismissed,
            shortcut: self.shortcut,
            input: self.input.clone(),
            shortcut_selection: self.shortcut_selection,
            caret: self.caret,
            caret_in_text: self.caret_in_text,
            selection: Rc::clone(&self.selection),
            reveal: self.reveal,
            preedit: self.preedit.clone(),
            scrolls: self.scrolls.clone(),
            scroll_req: Rc::clone(&self.scroll_req),
            bring_req: Rc::clone(&self.bring_req),
            edit_req: Rc::clone(&self.edit_req),
            focus_req: Rc::clone(&self.focus_req),
            copy_req: Rc::clone(&self.copy_req),
            close_req: Rc::clone(&self.close_req),
            nudge_req: Rc::clone(&self.nudge_req),
            image_req: Rc::clone(&self.image_req),
            canvas_req: Rc::clone(&self.canvas_req),
            hovered_path: self.hovered_path.clone(),
            pressed_path: self.pressed_path.clone(),
            disabled: self.disabled.clone(),
            drag_owner: self.drag_owner,
            drag_started: self.drag_started,
            drag_step: self.drag_step,
            press_origin: self.press_origin,
            local: self.local,
            anim: self.anim,
            rec: Rc::clone(&self.rec),
            tree: self.tree,
            cache: self.cache.as_deref_mut(),
            visiting: crate::collections::FastSet::default(),
            measure: self.measure.clone(),
            math: self.math.clone(),
            math_edit: self.math_edit.clone(),
        }
    }

    /// The store itself, for a wrapper that hands one node at a time to a control.
    pub fn anim_mut(&mut self) -> &mut AnimStore {
        self.anim
    }

    /// One instance's state; read it *after* retargeting so it describes this frame.
    pub fn status(&self, id: Id) -> Option<crate::anim::Status> {
        let status = self.anim.status(id, self.now);
        self.record(Dep::Status { id, status });
        status
    }

    /// The retained tree, when a caller needs geometry the declarations do not carry.
    ///
    /// Attached by the engine before the view runs; a build then reads last frame's layout,
    /// which is what a strip measuring its siblings for a drop needs.
    pub fn attach_tree(&mut self, tree: &'a Tree) {
        self.tree = Some(tree);
    }

    /// The rect `id` occupied in the last layout, if it exists and a tree is attached.
    ///
    /// Volatile: the engine cannot replay a layout, so no cached declaration survives it.
    pub fn rect_of(&self, id: Id) -> Option<crate::geom::Rect> {
        let tree = self.tree?;
        self.record(Dep::Volatile);
        let nid: NodeId = tree.find_by_id(id)?;
        tree.get(nid).map(|n| n.visual_rect())
    }

    /// The constraints layout measured this node under last frame.
    ///
    /// A control that must size itself to its parent — a text field whose wrap width is
    /// its slot, a grid that picks its column count from the space — reads them here.
    /// Build runs before layout, so these are the previous frame's: a resize reaches the
    /// control one frame later, the same lag `rect_of` has. `None` before the node has
    /// been laid out once. The constraints are the slot the parent offers, not the size
    /// the node chose inside it, so a hugged child still learns it could have been wider.
    pub fn constraints_of(&self, id: Id) -> Option<crate::layout::Constraints> {
        let tree = self.tree?;
        self.record(Dep::Volatile);
        let nid: NodeId = tree.find_by_id(id)?;
        tree.get(nid)
            .and_then(|n| n.last_measure)
            .map(|measure| measure.constraints)
    }

    /// The frame serial.
    pub fn frame(&self) -> u64 {
        self.record(Dep::Frame(self.frame));
        self.frame
    }

    /// The frame clock, recording a dependency: any reader rebuilds next frame.
    pub fn now(&self) -> f64 {
        self.record(Dep::Now(self.now.to_bits()));
        self.now
    }

    /// The clock without recording, for snapshotting a narrower context.
    pub fn clock(&self) -> f64 {
        self.now
    }

    pub fn dt(&self) -> f32 {
        self.record(Dep::Dt(self.dt.to_bits()));
        self.dt
    }

    pub fn scale(&self) -> f32 {
        self.record(Dep::Scale(self.scale.to_bits()));
        self.scale
    }

    pub fn pointer(&self) -> Option<Point> {
        self.record(Dep::Pointer(self.pointer));
        self.pointer
    }

    /// Set the held modifiers for this build; the engine does it once per frame.
    pub fn set_modifiers(&mut self, modifiers: crate::event::Modifiers) {
        self.modifiers = modifiers;
    }

    /// Held modifiers, as of the last input event.
    pub fn modifiers(&self) -> crate::event::Modifiers {
        let m = self.modifiers;
        self.record(Dep::Modifiers(
            m.shift as u8 | (m.ctrl as u8) << 1 | (m.alt as u8) << 2 | (m.logo as u8) << 3,
        ));
        m
    }

    pub fn width(&self) -> u32 {
        self.record(Dep::Size(self.width, self.height));
        self.width
    }

    pub fn height(&self) -> u32 {
        self.record(Dep::Size(self.width, self.height));
        self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Easing;

    fn cx<'a>(anim: &'a mut AnimStore) -> Cx<'a> {
        Cx::new(anim, 3, 200, 100, 5.0, 0.016, 2.0, None)
    }

    #[test]
    fn an_animation_written_here_is_readable_through_the_same_context() {
        // One store, one clock: a write and a read must agree inside the same frame.
        let mut anim = AnimStore::new();
        let mut cx = cx(&mut anim);
        let key = Id::str("k");
        cx.animate(key, 1.0, AnimSpec::new(0.1, Easing::Linear));
        assert_eq!(cx.value(key), 1.0, "first call settles at the target");
    }
}
