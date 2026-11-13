//! The kit: options resolved from the scope, controls built here.

use ui_style::Style;
use ui_widgets::ScaffoldSpec;
use ui_widgets::{StyleKit, Widget};

use crate::{GlassScaffold, GlassSlider, GlassSwitch};

/// Glass-specific knobs the semantic specs cannot express; scopes override the instance default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HigOptions {
    /// Rim stroke width in dp; the reference rim is a hairline specular edge.
    pub rim_width: f32,
}

impl Default for HigOptions {
    fn default() -> Self {
        Self { rim_width: 1.0 }
    }
}

/// HIG's kit; its controls are composed from primitives, not drawn by a framework.
pub struct HigKit {
    /// This kit's own pack constructor, so a scope-driven theme switch needs no rebuild.
    resolver: fn(ui_design::Appearance) -> Style,
    /// Every control in a frame shares one resolved pack per context.
    cache: ui_style::StyleCache,
    options: HigOptions,
    /// The context used when no `StyleScope` is on the chain.
    home: ui_style::StyleScope,
}

impl HigKit {
    /// A kit whose home context is the standard light one.
    pub fn new() -> Self {
        Self::with_scope(ui_style::StyleScope::default(), HigOptions::default())
    }

    /// An instance whose home context and unprefixed defaults are the given ones.
    pub fn with_scope(scope: ui_style::StyleScope, options: HigOptions) -> Self {
        Self {
            resolver: crate::pack::pack_owned,
            cache: ui_style::StyleCache::new(),
            options,
            home: scope,
        }
    }

    fn options_in(&self, env: &ui_style::Env) -> HigOptions {
        env.find::<HigOptions>().copied().unwrap_or(self.options)
    }

    /// The pack for the scope in force, with any running theme transition applied.
    fn pack_in(&self, cx: &ui_widgets::Cx<'_, '_>) -> std::rc::Rc<Style> {
        let scope = cx.env().scope().unwrap_or(self.home);
        self.cache.get(scope, self.resolver)
    }
}

impl Default for HigKit {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleKit for HigKit {
    fn slider(
        &self,
        spec: &ui_style::SliderSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        let node = cx.at(spec.id);
        let mut ctx = node;
        Box::new(GlassSlider::new_with(
            p, spec.id, spec.value, spec.label, &mut ctx, &pack,
        ))
    }

    fn toggle(
        &self,
        spec: &ui_style::ToggleSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        let node = cx.at(spec.id);
        let mut ctx = node;
        Box::new(GlassSwitch::new_with(
            p, spec.id, spec.on, spec.label, &mut ctx, &pack,
        ))
    }

    fn scaffold(
        &self,
        spec: &ScaffoldSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        Box::new(cx.with_cx(|cx| GlassScaffold::new(spec, cx)))
    }
}
