//! What the engine needs from a live control: a declaration, children, and a change.

use ui_core::change::Change;
use ui_core::geom::Rect;
use ui_core::memo::CachedKey;
use ui_core::{Decl, Id, NodeKind};

use crate::state::WidgetState;

/// One node of the live control tree, alive for the whole frame.
///
/// Style-free: styles are already resolved into the declaration, so a container and a
/// themed slider are the same kind of thing here.
pub trait Element {
    /// This frame's declaration, built from the engine context.
    fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl;

    /// Child elements in paint order; empty for a leaf.
    fn children(&self) -> &[Box<dyn Element>] {
        &[]
    }

    /// This frame's semantic change, or `None`. Called after layout, so `bounds` is final.
    fn change(&self, _state: &WidgetState, _bounds: Rect) -> Option<Change> {
        None
    }

    /// Identity and a fingerprint of this element's own inputs, when both are known
    /// without building. Supplying one promises that everything `decl` reads beyond the
    /// context is inside the fingerprint.
    fn cache_key(&self) -> Option<CachedKey> {
        None
    }

    /// Whether the cached declaration still says what this element says now. A container
    /// compares the shell it built against the cached one, field by field.
    fn cache_matches(&self, _cached: &Decl) -> bool {
        true
    }

    /// This frame's declaration, already shared, for an element that holds one finished.
    /// Handing out the `Rc` itself skips a deep clone of text blocks and shape lists.
    fn decl_shared(&self) -> Option<std::rc::Rc<Decl>> {
        None
    }
}

/// Build one child's declaration, reusing the cached subtree when its facts still hold.
///
/// The cache is only as sound as the app's invalidation: state the engine cannot see must
/// be announced with `changed` or `Cx::invalidate_decls`.
pub fn element_decl(el: &dyn Element, cx: &mut ui_core::cx::Cx<'_>) -> std::rc::Rc<Decl> {
    if let Some(shared) = el.decl_shared() {
        // A finished declaration is app input: this frame's build may differ from the one
        // this element handed over last frame, and no recorded fact can prove otherwise.
        cx.note_rebuilt_child();
        return shared;
    }
    // Probe: `CRABSNAP_NO_DECL_CACHE=1` rebuilds every declaration, so an A/B can price reuse.
    let key = el.cache_key().filter(|_| decl_cache_enabled());
    let Some(key) = key else {
        // No key: this child is rebuilt, so the parent's declaration is no longer provable.
        cx.note_rebuilt_child();
        return std::rc::Rc::new(el.decl(cx));
    };
    // The parent's entry proves this child only through the child's own entry.
    cx.note_element_child(key.id);
    if let Some(hit) = cx.cached_decl_matching(key, |cached| el.cache_matches(cached)) {
        return hit;
    }
    // A miss rebuilt this child, so whatever encloses it cannot be reused this frame.
    cx.note_rebuilt_child();
    cx.begin_record();
    let decl = el.decl(cx);
    // A key that disagrees with the declaration would poison every later lookup.
    debug_assert_eq!(
        decl.id, key.id,
        "a cache key must be the declaration's root id"
    );
    cx.end_record(key, decl)
}

/// Read once: the answer cannot change within a run, and this is asked once per element.
fn decl_cache_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("CRABSNAP_NO_DECL_CACHE").is_none())
}

/// Forwarding so `Box<dyn Element>` composes without ceremony.
impl Element for Box<dyn Element> {
    fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        (**self).decl(cx)
    }

    fn children(&self) -> &[Box<dyn Element>] {
        (**self).children()
    }

    fn change(&self, state: &WidgetState, bounds: Rect) -> Option<Change> {
        (**self).change(state, bounds)
    }

    fn cache_key(&self) -> Option<CachedKey> {
        (**self).cache_key()
    }

    fn cache_matches(&self, cached: &Decl) -> bool {
        (**self).cache_matches(cached)
    }

    fn decl_shared(&self) -> Option<std::rc::Rc<Decl>> {
        (**self).decl_shared()
    }
}

/// A node with no children and no interaction: text, a painted box.
pub struct Leaf {
    decl: std::rc::Rc<Decl>,
}

impl Leaf {
    pub fn new(decl: Decl) -> Self {
        Self {
            decl: std::rc::Rc::new(decl),
        }
    }

    /// A leaf over a declaration that is already shared: adopting a subtree must not copy it.
    fn shared(decl: std::rc::Rc<Decl>) -> Self {
        Self { decl }
    }
}

/// Wrap a finished declaration as an element with no children and no change.
pub fn from_decl(decl: Decl) -> Box<dyn Element> {
    Box::new(Leaf::new(decl))
}

impl Element for Leaf {
    fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        (*self.decl).clone()
    }

    fn decl_shared(&self) -> Option<std::rc::Rc<Decl>> {
        Some(std::rc::Rc::clone(&self.decl))
    }
}

/// Arranges children; reports no change. Its own fields are a childless `Decl`, so the
/// node's semantics live in exactly one place.
pub struct Container {
    own: Decl,
    children: Vec<Box<dyn Element>>,
}

impl Container {
    /// A flow container: `Row`, `Column`, `Stack` or `Overlay`.
    pub fn new(id: Id, kind: NodeKind) -> Self {
        Self {
            own: Decl::container(id, kind),
            children: Vec::new(),
        }
    }

    /// Wrap an already-built declaration, e.g. one carrying effects. Any subtree it carries
    /// becomes leaf children: `decl` rebuilds the children slot from the element tree, so a
    /// declaration's own children are not where this container looks for them.
    pub fn from_decl(mut own: Decl) -> Self {
        let children = std::mem::take(&mut own.children)
            .into_iter()
            .map(|child| Box::new(Leaf::shared(child)) as Box<dyn Element>)
            .collect();
        Self { own, children }
    }

    pub fn id(&self) -> Id {
        self.own.id
    }

    /// Edit the node's own declaration; covers every `Decl` field without mirroring it.
    pub fn map(mut self, f: impl FnOnce(Decl) -> Decl) -> Self {
        self.own = f(self.own);
        self
    }

    pub fn child(mut self, child: Box<dyn Element>) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Box<dyn Element>>) -> Self {
        self.children.extend(children);
        self
    }

    /// Add a child only when the condition holds, keeping call sites flat.
    pub fn child_if(mut self, yes: bool, child: impl FnOnce() -> Box<dyn Element>) -> Self {
        if yes {
            self.children.push(child());
        }
        self
    }

    /// Add a finished declaration as a static child with no liveness.
    pub fn child_decl(mut self, decl: Decl) -> Self {
        self.children.push(Box::new(Leaf::new(decl)));
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.own = self.own.gap(gap);
        self
    }

    pub fn padding(mut self, p: ui_core::Insets) -> Self {
        self.own = self.own.padding(p);
        self
    }

    pub fn size(mut self, size: ui_core::SizeClass) -> Self {
        self.own = self.own.size(size);
        self
    }

    pub fn background(
        mut self,
        fill: ui_core::geom::Color,
        radius: ui_core::geom::Corners,
    ) -> Self {
        self.own = self.own.background(fill, radius);
        self
    }

    /// Draw a registered texture under this container's children, over its fill.
    pub fn background_image(mut self, id: u64, fit: ui_core::tree::ImageFit) -> Self {
        self.own = self.own.background_image(id, fit);
        self
    }

    /// Read the background texture as texels rather than blending between them.
    pub fn image_sampling(mut self, sampling: ui_core::draw::Sampling) -> Self {
        self.own = self.own.image_sampling(sampling);
        self
    }

    pub fn border(mut self, color: ui_core::geom::Color, width: f32) -> Self {
        self.own = self.own.border(color, width);
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.own = self.own.offset(x, y);
        self
    }

    /// Cast the surface up; see [`Decl::shadow`].
    pub fn shadow(mut self, spec: ui_core::tree::ShadowSpec) -> Self {
        self.own = self.own.shadow(spec);
        self
    }

    /// Anchor this container as an overlay: the engine places it from `placement`, paints it
    /// above the tree and lets it escape every clipping ancestor.
    pub fn overlay(mut self, placement: ui_core::layout::OverlayPlacement) -> Self {
        self.own = self.own.overlay(placement);
        self
    }

    /// Rows this node offers on a secondary click; the engine shows them and reports the pick.
    pub fn menu(mut self, items: Vec<ui_core::menu::MenuItem>) -> Self {
        self.own = self.own.menu(items);
        self
    }

    /// Make this container and its subtree inert, without the child knowing.
    pub fn disabled(mut self, yes: bool) -> Self {
        self.own = self.own.disabled(yes);
        self
    }

    pub fn sense(mut self, sense: ui_core::Sense) -> Self {
        self.own = self.own.sense(sense);
        self
    }

    pub fn cursor(mut self, shape: ui_core::CursorShape) -> Self {
        self.own = self.own.cursor(shape);
        self
    }

    pub fn scrollable(mut self) -> Self {
        self.own = self.own.scrollable();
        self
    }

    pub fn scroll_y(mut self, y: f32) -> Self {
        self.own = self.own.scroll_y(y);
        self
    }

    pub fn opacity(mut self, o: f32) -> Self {
        self.own = self.own.opacity(o);
        self
    }

    /// Paint this container and its subtree `scale` times larger, about its own top-left.
    pub fn scale(mut self, scale: f32) -> Self {
        self.own = self.own.scale(scale);
        self
    }

    /// Travel to a new fixed size instead of jumping, when the declared size changes.
    pub fn transition_size(mut self, spec: ui_core::AnimSpec) -> Self {
        self.own = self.own.transition_size(spec);
        self
    }

    /// Appear from `enter`'s start values instead of at full strength.
    pub fn enter(mut self, enter: ui_core::tree::Presence) -> Self {
        self.own = self.own.enter(enter);
        self
    }

    /// Fade out through `exit`'s values after the app stops declaring this node.
    pub fn exit(mut self, exit: ui_core::tree::Presence) -> Self {
        self.own = self.own.exit(exit);
        self
    }

    /// Whether the slot this node held stays, collapses, or goes at once while it leaves.
    pub fn exit_space(mut self, space: ui_core::ExitSpace) -> Self {
        self.own = self.own.exit_space(space);
        self
    }

    pub fn cross_align(mut self, a: f32) -> Self {
        self.own = self.own.cross_align(a);
        self
    }

    /// Place children in the cross-axis space they left over; 0.5 centres them.
    pub fn cross_pos(mut self, a: f32) -> Self {
        self.own = self.own.cross_pos(a);
        self
    }
}

impl Element for Container {
    fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        let mut decl = self.own.clone();
        decl.children = self
            .children
            .iter()
            .map(|c| element_decl(c.as_ref(), cx))
            .collect();
        decl
    }

    fn children(&self) -> &[Box<dyn Element>] {
        &self.children
    }

    fn cache_key(&self) -> Option<CachedKey> {
        // The shell is app input, so it is proved by comparison rather than by recording;
        // `cache_matches` is where that proof lives, and it runs on every lookup.
        Some(CachedKey::new(self.own.id, 0))
    }

    fn cache_matches(&self, cached: &Decl) -> bool {
        self.own.own_eq(cached)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::{Decl, Id, NodeKind};

    /// A container element rebuilds its declaration's children from its own children, so a
    /// finished subtree handed to `from_decl` has to survive as leaf children or its nodes
    /// sit in the tree unpainted.
    #[test]
    fn from_decl_adopts_the_children_of_the_declaration_it_wraps() {
        let row = Decl::container(Id::str("row"), NodeKind::Row)
            .child(Decl::container(Id::str("cell"), NodeKind::Box));
        let container = Container::from_decl(row);
        let mut anim = ui_core::AnimStore::new();
        let mut cx = ui_core::cx::Cx::new(&mut anim, 0, 200, 100, 0.0, 0.016, 1.0, None);
        let rebuilt = container.decl(&mut cx);
        assert_eq!(rebuilt.children.len(), 1);
        assert_eq!(rebuilt.children[0].id, Id::str("cell"));
        assert!(
            container.own.children.is_empty(),
            "the children live in the element tree, not in the slot rebuilt from it"
        );
    }
}
