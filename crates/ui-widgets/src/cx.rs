//! The one build context: engine facts, derived routing queries, and the two writable doors.
//!
//! Everything that used to be four near-duplicate structs (`FrameInfo`, `Fx`, `BuildCtx`,
//! `Frame`) is one type here. A narrower capability is *derived* from it, never declared
//! beside it, so `now` and `scale` can only be wrong in one place.

use std::sync::Arc;

use ui_core::{AnimSpec, AnimStore, Id};
use ui_i18n::{Args, Catalog, Msg};
use ui_style::Env;

use crate::state::WidgetState;

/// Engine facts for one build, plus what may be written during it.
pub struct Cx<'a, 'b> {
    /// Everything the engine knows and the one door it opens, owned by `ui-core`.
    core: &'b mut ui_core::cx::Cx<'a>,
    env: Arc<Env>,
    /// Resolved once per element build; the Env chain is not walked per lookup.
    i18n: Option<Arc<Catalog>>,
}

impl<'a, 'b> Cx<'a, 'b> {
    /// Wrap the engine context the app was handed; the widget layer adds scope and state.
    pub fn new(core: &'b mut ui_core::cx::Cx<'a>, env: Arc<Env>) -> Cx<'a, 'b> {
        let i18n = env
            .find::<Arc<ui_i18n::Language>>()
            .map(|language| language.catalog());
        Cx { core, env, i18n }
    }

    /// A built-in message: one array index and one refcount bump.
    pub fn t(&self, msg: Msg) -> Arc<str> {
        self.catalog().text(msg)
    }

    /// A built-in message with arguments, formatted on demand.
    pub fn t_args(&self, msg: Msg, args: &Args<'_>) -> Arc<str> {
        self.catalog().text_with(msg, args)
    }

    /// A civil date in the catalogue's locale, at CLDR's medium length.
    pub fn date(&self, year: i32, month: u8, day: u8) -> Option<Arc<str>> {
        self.catalog().date(year, month, day)
    }

    /// The app's catalog, or the built-in English one for tests and bare `Env::root()`.
    pub(crate) fn catalog(&self) -> &Catalog {
        if let Some(catalog) = self.i18n.as_deref() {
            return catalog;
        }
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::warn!("ui-widgets: no i18n catalog in Env; using built-in English");
        }
        Catalog::english()
    }

    /// A throwaway context for tests: no routing, one frame, its own store.
    pub fn for_test<R>(f: impl FnOnce(&Cx<'_, '_>) -> R) -> R {
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        let cx = Cx::new(&mut core, Env::root());
        f(&cx)
    }

    /// The same as `for_test`, for callers that build controls and need `&mut`.
    pub fn for_test_mut<R>(f: impl FnOnce(&mut Cx<'_, '_>) -> R) -> R {
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
        core.route(ui_core::cx::Routing::default());
        let mut cx = Cx::new(&mut core, Env::root());
        f(&mut cx)
    }

    /// The engine facts, for code that only needs routing or the clock.
    pub fn core(&self) -> &ui_core::cx::Cx<'a> {
        self.core
    }

    /// The size `block` takes at `max_width`, from the host's own text measurer.
    ///
    /// `None` where no measurer is attached, which is every test context.
    pub fn text_size(
        &self,
        block: &ui_core::text::TextBlock,
        max_width: f32,
    ) -> Option<ui_core::geom::Size> {
        self.core.text_size(block, max_width)
    }

    /// The layout `block` takes at `max_width`, for a caller that needs its baseline.
    pub fn text_layout(
        &self,
        block: &ui_core::text::TextBlock,
        max_width: f32,
    ) -> Option<std::rc::Rc<ui_core::text::InlineLayout>> {
        self.core.text_layout(block, max_width)
    }

    /// The rect `id` took in the last layout, for a control that measures its siblings.
    ///
    /// `None` before the first layout, or when the host attached no retained tree.
    pub fn rect_of(&self, id: Id) -> Option<ui_core::geom::Rect> {
        self.core.rect_of(id)
    }

    /// The same frame under a narrower scope; store, clock and routing are shared.
    pub fn at_scope<'c>(&'c mut self, env: Arc<Env>) -> Cx<'a, 'c> {
        let i18n = env
            .find::<Arc<ui_i18n::Language>>()
            .map(|language| language.catalog());
        Cx {
            core: &mut *self.core,
            env,
            i18n,
        }
    }

    /// Turn a control into an engine element bound to the scope in force here.
    pub fn element(
        &self,
        control: Box<dyn crate::widget::Widget>,
    ) -> Box<dyn ui_platform::Element> {
        Box::new(crate::styled::Styled::new(control, self.env.clone()))
    }

    /// Run a closure over a context of the same frame, for building a subtree once.
    pub fn with_cx<R>(&mut self, f: impl FnOnce(&Cx<'_, '_>) -> R) -> R {
        let env = self.env.clone();
        let i18n = env
            .find::<Arc<ui_i18n::Language>>()
            .map(|language| language.catalog());
        let cx = Cx {
            core: &mut *self.core,
            env,
            i18n,
        };
        f(&cx)
    }

    pub fn now(&self) -> f64 {
        self.core.now()
    }

    pub fn scale(&self) -> f32 {
        self.core.scale()
    }

    /// This node's interaction state.
    pub fn state(&self, id: Id) -> WidgetState {
        WidgetState::of(self.core(), id)
    }

    /// The scope in force, for a style to resolve its parameters against.
    pub fn env(&self) -> &Env {
        &self.env
    }

    /// The scope as a handle, for code that needs to share or re-layer it.
    pub fn env_handle(&self) -> &Arc<Env> {
        &self.env
    }

    /// Scene `id` to one node: animation writes and state reads are all it exposes.
    pub fn at(&mut self, id: Id) -> NodeCx<'_> {
        let state = self.state(id);
        let scale = self.core.scale();
        // The clock is snapshotted, not recorded: the values derived from it are recorded
        // per animation below, which is what actually decides whether the decl changed.
        let now = self.core.clock();
        let rec = self.core.recorder_handle();
        // Text editing facts belong to the node being built, so they are snapshotted here;
        // a control must not have to reach past its own node view for its own caret.
        let caret = self.core.caret_for(id);
        let preedit = self.core.preedit_for(id);
        let selection = self.core.selection_for(id);
        NodeCx {
            id,
            state,
            scale,
            now,
            anim: self.core.anim_mut(),
            rec: Some(rec),
            caret,
            preedit,
            selection,
        }
    }
}

/// One node's slice of the build: its state, and the animations it owns.
pub struct NodeCx<'a> {
    id: Id,
    state: WidgetState,
    scale: f32,
    anim: &'a mut ui_core::AnimStore,
    now: f64,
    /// Where animation reads are recorded, so a cached subtree can tell it went stale.
    rec: Option<std::rc::Rc<std::cell::RefCell<ui_core::memo::Recorder>>>,
    /// This node's caret, composition and selection, as the engine holds them.
    caret: Option<usize>,
    preedit: Option<ui_core::ime::Preedit>,
    selection: Option<ui_core::TextSelection>,
}

impl<'a> NodeCx<'a> {
    /// Build a node view directly from the pieces the engine context holds.
    pub fn new(id: Id, state: WidgetState, scale: f32, now: f64, anim: &'a mut AnimStore) -> Self {
        Self {
            id,
            state,
            scale,
            anim,
            now,
            rec: None,
            caret: None,
            preedit: None,
            selection: None,
        }
    }

    /// This node's caret offset, when it holds focus.
    pub fn caret(&self) -> Option<usize> {
        self.caret
    }

    /// The input method's composition for this node, never merged into the buffer.
    pub fn preedit(&self) -> Option<&ui_core::ime::Preedit> {
        self.preedit.as_ref()
    }

    /// This node's text selection, when it has one.
    pub fn selection(&self) -> Option<ui_core::TextSelection> {
        self.selection
    }

    fn record(&self, dep: ui_core::Dep) {
        if let Some(rec) = self.rec.as_ref()
            && let Ok(mut rec) = rec.try_borrow_mut()
        {
            rec.push(dep);
        }
    }

    pub fn id(&self) -> Id {
        self.id
    }

    /// Interaction state of the node being built.
    pub fn state(&self) -> WidgetState {
        self.state
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The frame's time in seconds, for a widget that turns a delta into a rate.
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Drive one atomic animation instance and read what to draw this frame.
    pub fn animate(&mut self, id: Id, target: f32, spec: AnimSpec) -> f32 {
        let v = self.anim.animate(id, target, spec, self.now);
        self.record(ui_core::Dep::Anim {
            id,
            target,
            spec,
            value: v,
        });
        v
    }

    /// Drive one spring instance in engine seconds; its velocity is units per second.
    ///
    /// The spring keeps the frame loop awake until it rests, so it never freezes mid-ring.
    pub fn spring(
        &mut self,
        id: Id,
        target: f32,
        stiffness: f32,
        damping: f32,
    ) -> ui_core::anim::SpringState {
        let state = self.anim.spring(id, target, stiffness, damping, self.now);
        self.record(ui_core::Dep::Value { id, value: state.x });
        state
    }

    /// The spring's current value, unadvanced: for choosing which spring to hand it to.
    pub fn spring_value(&self, id: Id) -> Option<f32> {
        self.anim.spring_value(id).map(|s| s.x)
    }

    /// Drop a spring so the next drive starts at its target, not on the old trajectory.
    pub fn forget_spring(&mut self, id: Id) {
        self.anim.forget_spring(id);
    }

    /// [`spring`](Self::spring) that starts at `hint` units/s when the target changes.
    pub fn spring_hinted(
        &mut self,
        id: Id,
        target: f32,
        stiffness: f32,
        damping: f32,
        hint: f32,
    ) -> ui_core::anim::SpringState {
        let state = self
            .anim
            .spring_hinted(id, target, stiffness, damping, hint, self.now);
        self.record(ui_core::Dep::Value { id, value: state.x });
        state
    }

    /// Drive a colour; the engine carries the blend on one clock.
    pub fn animate_colour(
        &mut self,
        id: Id,
        target: ui_core::geom::Color,
        spec: AnimSpec,
    ) -> ui_core::geom::Color {
        let v = self.anim.animate_colour(id, target, spec, self.now);
        self.record(ui_core::Dep::AnimColour {
            id,
            target,
            spec,
            value: v,
        });
        v
    }

    /// Drive a parameter along keyframes; the last stop is its target.
    pub fn animate_stops(&mut self, id: Id, stops: &[ui_core::anim::Stop], spec: AnimSpec) -> f32 {
        let v = self.anim.animate_stops(id, stops, spec, self.now);
        self.record(ui_core::Dep::AnimStops {
            id,
            stops: stops.to_vec(),
            spec,
            value: v,
        });
        v
    }

    /// The current value without asking it to move.
    pub fn value(&self, id: Id) -> f32 {
        let v = self.anim.value(id, self.now).unwrap_or(0.0);
        self.record(ui_core::Dep::Value { id, value: v });
        v
    }

    /// Seconds since this id first asked: a loop's own clock, zero on the first frame.
    ///
    /// Reading it asks for every frame; a control that stops reading stops asking.
    pub fn age(&mut self, id: Id) -> f32 {
        let age = self.anim.age(id, self.now) as f32;
        self.record(ui_core::Dep::Age { id, age });
        age
    }

    /// Forget an id's age, so the next [`age`] read counts from zero: a gesture that
    /// ended must not leave its clock running into the next one.
    pub fn forget_age(&mut self, id: Id) {
        self.anim.forget_age(id);
    }

    /// One instance's state; read it *after* retargeting so it describes this frame.
    pub fn status(&self, id: Id) -> Option<ui_core::anim::Status> {
        let status = self.anim.status(id, self.now);
        self.record(ui_core::Dep::Status { id, status });
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests hold the engine context on the stack so both lives are concrete.
    fn wrap<'a>(core: &'a mut ui_core::cx::Cx<'a>, env: Arc<Env>) -> Cx<'a, 'a> {
        Cx::new(core, env)
    }

    #[test]
    fn a_catalog_on_the_env_reaches_the_widget_context() {
        // The Env layer is the only wiring point; a widget asks `cx`, never a global.
        let zh: ui_i18n::LanguageIdentifier = "zh-CN".parse().expect("zh-CN");
        let language =
            ui_i18n::Language::builtin(ui_i18n::LanguageChoice::Fixed(zh), &[]).expect("language");
        let env = Env::with(Some(Env::root()), language);
        let mut anim = AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut anim, 0, 200, 100, 0.0, 0.0, 1.0, None);
        let cx = wrap(&mut core, env);
        assert_eq!(&*cx.t(ui_i18n::Msg::MD3_DATE_PICKER_HEADER), "选择日期");
        let mut args = ui_i18n::Args::new();
        args.set("count", 3);
        assert_eq!(
            &*cx.t_args(ui_i18n::Msg::WIDGETS_LIST_SELECTED_COUNT, &args),
            "已选择 3 个项目"
        );
    }
}
