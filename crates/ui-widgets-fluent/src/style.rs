//! The kit: options resolved from the scope, controls built here.

use ui_style::Style;
use ui_widgets::ScaffoldSpec;
use ui_widgets::{StyleKit, Widget};

use crate::slider::SLIDER_TRACK_H;
use crate::{FluentScaffold, FluentSlider, FluentToggleSwitch};

/// Fluent-specific knobs the semantic specs cannot express; scopes override the instance default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FluentOptions {
    /// Slider track thickness; WinUI exposes it as the track-height resource.
    pub slider_track_h: f32,
}

impl Default for FluentOptions {
    fn default() -> Self {
        Self {
            slider_track_h: SLIDER_TRACK_H,
        }
    }
}

/// Fluent's kit: every control this crate draws, built from one resolved pack.
pub struct FluentKit {
    /// This kit's own pack constructor, so a scope-driven theme switch needs no rebuild.
    resolver: fn(ui_design::Appearance) -> Style,
    /// Every control in a frame shares one resolved pack per context.
    cache: ui_style::StyleCache,
    options: FluentOptions,
    /// The context used when no `StyleScope` is on the chain.
    home: ui_style::StyleScope,
}

impl FluentKit {
    /// A kit whose home context is the standard light one.
    pub fn new() -> Self {
        Self::with_scope(ui_style::StyleScope::default(), FluentOptions::default())
    }

    /// An instance whose home context and unprefixed defaults are the given ones.
    pub fn with_scope(scope: ui_style::StyleScope, options: FluentOptions) -> Self {
        Self {
            resolver: crate::pack::pack_owned,
            cache: ui_style::StyleCache::new(),
            options,
            home: scope,
        }
    }

    fn options_in(&self, env: &ui_style::Env) -> FluentOptions {
        env.find::<FluentOptions>().copied().unwrap_or(self.options)
    }

    /// The pack for the scope in force, with any running theme transition applied.
    fn pack_in(&self, cx: &ui_widgets::Cx<'_, '_>) -> std::rc::Rc<Style> {
        let scope = cx.env().scope().unwrap_or(self.home);
        self.cache.get(scope, self.resolver)
    }

    /// One radio button, resolved against the pack in force here.
    pub fn radio(
        &self,
        params: crate::RadioParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let id = params.id;
        let mut node = cx.at(id);
        Box::new(crate::FluentRadio::new(params, &mut node, &pack))
    }

    /// One check box, resolved against the pack in force here.
    pub fn checkbox(
        &self,
        params: crate::CheckBoxParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let id = params.id;
        let mut node = cx.at(id);
        Box::new(crate::FluentCheckBox::new(params, &mut node, &pack))
    }

    /// One text box, resolved against the pack in force here.
    pub fn text_box(
        &self,
        params: crate::TextBoxParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let id = params.id;
        let mut node = cx.at(id);
        Box::new(crate::FluentTextBox::new(params, &mut node, &pack))
    }

    /// One info bar, resolved against the pack in force here.
    pub fn info_bar(
        &self,
        params: crate::InfoBarParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let id = params.id;
        // The close button carries its own hover and press, so its state is read apart.
        let close = cx.at(id.with("close")).state();
        let mut node = cx.at(id);
        Box::new(crate::FluentInfoBar::new(params, &mut node, close, &pack))
    }

    /// One progress bar, resolved against the pack in force here.
    pub fn progress_bar(
        &self,
        params: crate::ProgressBarParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let id = params.id;
        let mut node = cx.at(id);
        Box::new(crate::FluentProgressBar::new(params, &mut node, &pack))
    }

    /// One tab strip; a composite, so it returns an element rather than a widget.
    pub fn tab_view(
        &self,
        params: crate::TabViewParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn ui_widgets::Element> {
        let pack = self.pack_in(cx);
        crate::tab_view(params, cx, &pack)
    }

    /// One content dialog, resolved against the pack in force here.
    pub fn content_dialog(
        &self,
        params: crate::ContentDialogParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        Box::new(crate::FluentContentDialog::new(params, cx, &pack))
    }

    /// One combo box, resolved against the pack in force here.
    pub fn combo_box(
        &self,
        params: crate::ComboBoxParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        Box::new(crate::FluentComboBox::new(params, cx, &pack))
    }

    /// One navigation view, resolved against the pack in force here.
    pub fn navigation_view(
        &self,
        params: crate::NavigationViewParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn ui_platform::Element> {
        let pack = self.pack_in(cx);
        Box::new(crate::FluentNavigationView::new(params, cx, &pack))
    }

    /// One menu flyout, resolved against the pack in force here.
    pub fn menu_flyout(
        &self,
        params: crate::MenuFlyoutParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        Box::new(crate::FluentMenuFlyout::new(params, cx, &pack))
    }

    /// One tooltip panel, resolved against the pack in force here.
    pub fn tool_tip(
        &self,
        params: crate::ToolTipParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let mut node = cx.at(params.id);
        Box::new(crate::FluentToolTip::new(params, &mut node, &pack))
    }

    /// One progress ring, resolved against the pack in force here.
    pub fn progress_ring(
        &self,
        params: crate::ProgressRingParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let id = params.id;
        let mut node = cx.at(id);
        Box::new(crate::FluentProgressRing::new(params, &mut node, &pack))
    }

    /// One button, resolved against the pack in force here.
    pub fn button(
        &self,
        params: crate::ButtonParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let id = params.id;
        let mut node = cx.at(id);
        Box::new(crate::FluentButton::new(params, &mut node, &pack))
    }
}

impl Default for FluentKit {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleKit for FluentKit {
    fn slider(
        &self,
        spec: &ui_style::SliderSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        let node = cx.at(spec.id);
        let mut ctx = node;
        Box::new(FluentSlider::new_with(
            p, spec.id, spec.value, spec.label, &mut ctx, &pack,
        ))
    }

    fn toggle(
        &self,
        spec: &ui_style::ToggleSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        let node = cx.at(spec.id);
        let mut ctx = node;
        Box::new(FluentToggleSwitch::new(
            spec.id, spec.on, spec.label, &mut ctx, &pack,
        ))
    }

    fn scaffold(
        &self,
        spec: &ScaffoldSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        Box::new(cx.with_cx(|cx| FluentScaffold::new(spec, cx)))
    }
}
