//! Opaque, chainable style-parameter scopes; the semantic layer never interprets them.

use std::any::Any;
use std::sync::Arc;

/// One layer of the scope chain: its parameters, its context, and the enclosing scope.
pub struct Env {
    parent: Option<Arc<Env>>,
    /// Present only on layers that state a context; `None` inherits from the parent.
    scope: Option<StyleScope>,
    params: Arc<dyn Any + Send + Sync>,
}

impl Env {
    /// Push a layer; `None` starts a chain, which is what a top-level call passes.
    pub fn with<P: 'static + Send + Sync>(parent: Option<Arc<Env>>, params: P) -> Arc<Env> {
        Self::layered(parent, None, params)
    }

    /// The same layer, also stating the context its tokens resolve in.
    pub fn with_scope<P: 'static + Send + Sync>(
        parent: Option<Arc<Env>>,
        scope: StyleScope,
        params: P,
    ) -> Arc<Env> {
        Self::layered(parent, Some(scope), params)
    }

    fn layered<P: 'static + Send + Sync>(
        parent: Option<Arc<Env>>,
        scope: Option<StyleScope>,
        params: P,
    ) -> Arc<Env> {
        Arc::new(Env {
            parent,
            scope,
            params: Arc::new(params),
        })
    }

    /// Re-layer an existing scope's parameters onto a new parent, keeping the chain intact.
    ///
    /// A `Scoped` container needs this: the scope it opens is defined by its own value, but
    /// it must still see whatever its ancestors said.
    pub fn relayer(&self, parent: Option<Arc<Env>>) -> Arc<Env> {
        Arc::new(Env {
            parent,
            scope: self.scope,
            params: self.params.clone(),
        })
    }

    /// Read a scope back out of the opaque handle the declaration tree stores.
    ///
    /// `ui-core` only knows `Arc<dyn Any>`; this is the one place that knows better.
    pub fn from_opaque(v: &Arc<dyn Any + Send + Sync>) -> Option<&Arc<Env>> {
        v.downcast_ref::<Arc<Env>>()
    }

    /// Whether two handles are the same layer, by identity rather than by value.
    pub fn same(a: &Arc<Env>, b: &Arc<Env>) -> bool {
        Arc::ptr_eq(a, b)
    }

    /// The root a top-level call may pass; it carries no parameters at all.
    pub fn root() -> Arc<Env> {
        Env::with(None, ())
    }

    /// The nearest layer whose parameters are a `P`; a type mismatch is skipped, not an error.
    pub fn find<P: 'static>(&self) -> Option<&P> {
        let mut cur = Some(self);
        while let Some(env) = cur {
            if let Some(p) = env.params.downcast_ref::<P>() {
                return Some(p);
            }
            cur = env.parent.as_deref();
        }
        None
    }

    /// The nearest layer that states a context; layers below inherit it.
    pub fn scope(&self) -> Option<StyleScope> {
        let mut cur = Some(self);
        while let Some(env) = cur {
            if let Some(s) = env.scope {
                return Some(s);
            }
            cur = env.parent.as_deref();
        }
        None
    }

    pub fn parent(&self) -> Option<&Arc<Env>> {
        self.parent.as_ref()
    }
}

/// The context a subtree resolves its style in: which appearance, and any transition in flight.
/// `Env` carries it, so a family kit need not repeat these three fields in its own parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleScope {
    pub appearance: ui_design::Appearance,
    /// Appearance to blend from while a theme transition runs; `None` means no transition.
    pub theme_from: Option<ui_design::Appearance>,
    /// Transition progress, 0..=1; 1 means fully `appearance`.
    pub theme_t: f32,
}

impl Default for StyleScope {
    fn default() -> Self {
        Self {
            appearance: ui_design::Appearance::Light,
            theme_from: None,
            theme_t: 1.0,
        }
    }
}

impl StyleScope {
    /// A plain context for `appearance`, with no transition running.
    pub const fn new(appearance: ui_design::Appearance) -> Self {
        Self {
            appearance,
            theme_from: None,
            theme_t: 1.0,
        }
    }

    /// The nearest scope on the chain, or the standard light context.
    pub fn of(env: &Env) -> Self {
        env.scope().unwrap_or_default()
    }

    /// The same context with no transition when the transition is over or a no-op.
    pub fn settled(self) -> Self {
        let theme_t = self.theme_t.clamp(0.0, 1.0);
        let theme_from = self
            .theme_from
            .filter(|from| *from != self.appearance && theme_t < 1.0);
        Self {
            appearance: self.appearance,
            theme_from,
            theme_t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct A(u32);
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct B(u32);

    #[test]
    fn the_nearest_layer_of_the_matching_type_wins() {
        let root = Env::root();
        let outer = Env::with(Some(root.clone()), A(1));
        let middle = Env::with(Some(outer.clone()), B(2));
        let inner = Env::with(Some(middle.clone()), A(3));
        assert_eq!(inner.find::<A>(), Some(&A(3)));
        assert_eq!(middle.find::<A>(), Some(&A(1)));
        assert_eq!(middle.find::<B>(), Some(&B(2)));
        assert_eq!(root.find::<A>(), None);
        assert!(inner.parent().is_some());
    }
}
