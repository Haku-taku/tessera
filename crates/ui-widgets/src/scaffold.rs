//! Semantic page skeleton; every style exposes regions as `id.with("nav")`…`id.with("overlay")`.

use std::sync::Arc;

use ui_core::Id;
use ui_style::Env;

use crate::widget::Widget;

/// Six named regions; `main` is required, `None` means the caller has nothing there.
pub struct ScaffoldSpec<'a> {
    /// The skeleton's own node; content ids stay independent of it.
    pub id: Id,
    /// Leading navigation rail or pane.
    pub nav: Option<&'a dyn Widget>,
    /// Top bar above the content area.
    pub header: Option<&'a dyn Widget>,
    /// The one required region.
    pub main: &'a dyn Widget,
    /// Trailing context panel.
    pub context: Option<&'a dyn Widget>,
    /// A strip that sits at the top of `main`.
    pub inline: Option<&'a dyn Widget>,
    /// A layer floating above `main` without taking a slot.
    pub overlay: Option<&'a dyn Widget>,
    /// The bottom bar; each style places it the way its own system does.
    pub bottom: Option<&'a dyn Widget>,
    /// The parameter scope this page's subtree was built under.
    pub env: Arc<Env>,
}

impl<'a> ScaffoldSpec<'a> {
    /// Content only: the shape most callers need.
    pub fn main_only(id: Id, main: &'a dyn Widget) -> Self {
        Self {
            id,
            nav: None,
            header: None,
            main,
            context: None,
            inline: None,
            overlay: None,
            bottom: None,
            env: Env::root(),
        }
    }

    pub fn nav(mut self, nav: &'a dyn Widget) -> Self {
        self.nav = Some(nav);
        self
    }

    pub fn header(mut self, header: &'a dyn Widget) -> Self {
        self.header = Some(header);
        self
    }

    pub fn context(mut self, context: &'a dyn Widget) -> Self {
        self.context = Some(context);
        self
    }

    pub fn inline(mut self, inline: &'a dyn Widget) -> Self {
        self.inline = Some(inline);
        self
    }

    pub fn overlay(mut self, overlay: &'a dyn Widget) -> Self {
        self.overlay = Some(overlay);
        self
    }

    pub fn bottom(mut self, bottom: &'a dyn Widget) -> Self {
        self.bottom = Some(bottom);
        self
    }
    /// Push a parameter scope for this page's subtree. The scope rides on the declarations,
    /// so every control built under it inherits it without being told.
    pub fn scoped<P: 'static + Send + Sync>(mut self, params: P) -> Self {
        self.env = Env::with(Some(self.env.clone()), params);
        self
    }

    /// The scope `StyleKit::scaffold` and the page's controls should be built under.
    pub fn env(&self) -> &Env {
        &self.env
    }
}
