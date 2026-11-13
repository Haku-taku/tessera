//! The kit: options resolved from the scope, controls built here.

use ui_style::Style;
use ui_widgets::ScaffoldSpec;
use ui_widgets::{StyleKit, Widget};

use crate::{Md3Scaffold, Md3Toggle};

/// MD3-specific knobs the semantic specs cannot express; scopes override the instance default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Md3Options {
    /// `trackOutlineWidth` while unselected; it lerps to 0 as the fill arrives.
    pub outline_width: f32,
    /// Whether the thumb carries a glyph. Decides the 24-vs-16 handle rule, so it is a
    /// build-time knob, not a per-frame state.
    pub has_icon: bool,
    /// Draw the switch in its disabled appearance. A static sample needs this without a live
    /// disabled ancestor, so it travels on the params with the icon flag.
    pub disabled: bool,
    /// The frost behind the engine's context menu, in sigma; zero paints it opaque. An app
    /// over a live picture sets this so its menu reads as glass rather than a slab.
    pub menu_blur: f32,
}

impl Default for Md3Options {
    fn default() -> Self {
        Self {
            outline_width: crate::OUTLINE_WIDTH,
            has_icon: true,
            disabled: false,
            menu_blur: 0.0,
        }
    }
}

/// MD3's kit: every control this crate draws, built from one resolved pack.
pub struct Md3Kit {
    /// This kit's own pack constructor, so a scope-driven theme switch needs no rebuild.
    resolver: fn(ui_design::Appearance) -> Style,
    /// Every control in a frame shares one resolved pack per context.
    cache: ui_style::StyleCache,
    options: Md3Options,
    /// The context used when no `StyleScope` is on the chain.
    home: ui_style::StyleScope,
}

impl Md3Kit {
    /// A kit whose home context is the standard light one.
    pub fn new() -> Self {
        Self::with_scope(ui_style::StyleScope::default(), Md3Options::default())
    }

    /// An instance whose home context and unprefixed defaults are the given ones.
    pub fn with_scope(scope: ui_style::StyleScope, options: Md3Options) -> Self {
        Self {
            resolver: crate::pack::pack_owned,
            cache: ui_style::StyleCache::new(),
            options,
            home: scope,
        }
    }

    fn options_in(&self, env: &ui_style::Env) -> Md3Options {
        env.find::<Md3Options>().copied().unwrap_or(self.options)
    }

    /// The pack for the scope in force, with any running theme transition applied.
    fn pack_in(&self, cx: &ui_widgets::Cx<'_, '_>) -> std::rc::Rc<ui_style::Style> {
        let scope = cx.env().scope().unwrap_or(self.home);
        self.cache.get(scope, self.resolver)
    }

    /// The resolved pack for `cx`, with a running theme transition applied — the same
    /// value `pack_in` feeds the kit's own controls, so a direct `new_with` caller (e.g. a
    /// gallery shell) blends through the transition instead of snapping to the target scheme.
    pub fn pack_for(&self, cx: &ui_widgets::Cx<'_, '_>) -> std::rc::Rc<ui_style::Style> {
        self.pack_in(cx)
    }

    /// A dot badge, or a pill when `label` is given.
    pub fn badge(
        &self,
        id: ui_core::Id,
        label: Option<&str>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        Box::new(match label {
            Some(text) => crate::Md3Badge::with_label(id, text, &pack),
            None => crate::Md3Badge::dot(id, &pack),
        })
    }

    /// A checkbox; `error` paints the error role, disabled comes from the state.
    pub fn checkbox(
        &self,
        id: ui_core::Id,
        checked: bool,
        error: bool,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        self.checkbox_tri(id, crate::CheckState::from_bool(checked), error, cx)
    }

    /// The three-valued checkbox: `Mixed` draws the dash, `error` paints the error role.
    pub fn checkbox_tri(
        &self,
        id: ui_core::Id,
        value: crate::CheckState,
        error: bool,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        let mut node = cx.at(id);
        Box::new(crate::Md3Checkbox::new_tri(
            p, id, value, error, &mut node, &pack,
        ))
    }

    /// A slider `width` dp wide at `size`; `value` is 0..=1.
    pub fn slider_sized(
        &self,
        id: ui_core::Id,
        value: f32,
        width: f32,
        size: crate::spec::SliderSize,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        self.slider_opts(id, value, width, size, crate::SliderOpts::default(), cx)
    }

    /// The same slider with discrete stops and/or a trailing handle.
    pub fn slider_opts(
        &self,
        id: ui_core::Id,
        value: f32,
        width: f32,
        size: crate::spec::SliderSize,
        opts: crate::SliderOpts,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        let mut node = cx.at(id);
        Box::new(crate::Md3Slider::with_opts(
            id,
            value,
            width,
            size,
            p,
            opts,
            Some(&mut node),
            &pack,
        ))
    }

    /// A radio button; the group's selected id belongs to the app, not to the control.
    pub fn radio(
        &self,
        id: ui_core::Id,
        selected: bool,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        let mut node = cx.at(id);
        Box::new(crate::Md3Radio::new_with(p, id, selected, &mut node, &pack))
    }

    /// A divider; `inset` is the MD3 inset, `Insets::ZERO` for full bleed.
    pub fn divider(
        &self,
        id: ui_core::Id,
        axis: crate::DividerAxis,
        inset: ui_core::geom::Insets,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Divider::new(id, axis, inset, &pack))
    }

    /// A secondary navigation tab bar; `selected` is the index the app considers current and
    /// `width` is the width it will be laid out at, which the underline's travel needs.
    pub fn tab_bar(
        &self,
        id: ui_core::Id,
        items: &[crate::Md3TabItem<'_>],
        selected: usize,
        width: f32,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3TabBar::new_with(
            p, id, items, selected, width, cx, &pack,
        ))
    }

    /// A button: every axis is chosen through `ButtonParams`, in the order the label reads.
    pub fn button(
        &self,
        spec: crate::ButtonParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Button::new_with(p, spec, cx, &pack))
    }

    /// A toggle button: `selected` is the state the app owns, and a press flips it.
    pub fn toggle_button(
        &self,
        spec: crate::ButtonParams,
        selected: bool,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let toggle = if selected {
            crate::ButtonToggle::Selected
        } else {
            crate::ButtonToggle::Unselected
        };
        self.button(spec.toggle(toggle), cx)
    }

    /// A FAB menu: items above the close button that replaced the FAB. `tone` and `open`
    /// belong to the app.
    pub fn fab_menu(
        &self,
        id: ui_core::Id,
        items: &[crate::Md3FabMenuItem<'_>],
        tone: crate::FabMenuColour,
        open: bool,
        close_icon: ui_widgets::IconSource,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3FabMenu::new_with(
            p, id, items, tone, open, close_icon, cx, &pack,
        ))
    }

    /// A chip: kind, variant and the two marks are all chosen through `ChipParams`.
    pub fn chip(
        &self,
        spec: crate::ChipParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Chip::new_with(p, spec, cx, &pack))
    }

    /// A card: one surface in the chosen variant, holding the app's own content.
    pub fn card(
        &self,
        id: ui_core::Id,
        variant: crate::CardVariant,
        content: Vec<ui_core::Decl>,
        clickable: bool,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Card::new_with(
            p, id, variant, content, clickable, cx, &pack,
        ))
    }

    /// A list item: one row of a list, with its content chosen when it is built.
    pub fn list_item(
        &self,
        item: crate::ListItemParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ListItem::new_with(p, item, cx, &pack))
    }

    /// A dialog: one modal surface, with everything it shows chosen when it is built.
    pub fn dialog(
        &self,
        content: crate::DialogContent,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Dialog::new_with(p, content, cx, &pack))
    }

    /// A top app bar; `scrolled` and `collapsed` are the two facts its app owns.
    pub fn app_bar(
        &self,
        content: crate::AppBarParams<'_>,
        scrolled: bool,
        collapsed: bool,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3AppBar::new_with(
            p, content, scrolled, collapsed, cx, &pack,
        ))
    }

    /// A carousel; `scroll` is the one fact its app owns.
    pub fn carousel(
        &self,
        content: crate::CarouselParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Carousel::new_with(p, content, cx, &pack))
    }

    /// A text field; the app owns the buffer the engine edits.
    pub fn text_field(
        &self,
        content: crate::TextFieldParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3TextField::new_with(p, content, cx, &pack))
    }

    /// A search bar; the app owns the query the engine edits.
    pub fn search_bar(
        &self,
        content: crate::SearchBarParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3SearchBar::new_with(p, content, cx, &pack))
    }

    /// A search view; the app owns the query, the openness and the results.
    pub fn search_view(
        &self,
        content: crate::SearchViewParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3SearchView::new_with(p, content, cx, &pack))
    }

    /// A shortcut field; the app owns the chord it reports.
    pub fn shortcut_field(
        &self,
        content: crate::ShortcutFieldParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ShortcutField::new(content, &pack))
    }

    /// How the engine should paint its context menu, in this scope's colours.
    pub fn menu_style(&self, cx: &mut ui_widgets::Cx<'_, '_>) -> ui_widgets::MenuStyle {
        let blur = self.options_in(cx.env()).menu_blur;
        let pack = self.pack_in(cx);
        crate::menu_style(&pack, blur)
    }

    /// A connected button group: one pill whose members share their edges.
    pub fn button_group(
        &self,
        content: crate::ButtonGroupParams,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ButtonGroup::new_with(p, content, cx, &pack))
    }

    /// An outlined segmented button: equal segments, one pill, selection the app owns.
    pub fn segmented_button(
        &self,
        content: crate::SegmentedParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3SegmentedButton::new_with(p, content, cx, &pack))
    }

    /// A bottom navigation bar: equal destinations, one selected by the app.
    pub fn nav_bar(
        &self,
        content: crate::NavBarParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3NavBar::new_with(p, content, cx, &pack))
    }

    /// A navigation rail: the same destinations stacked in a fixed-width column.
    pub fn nav_rail(
        &self,
        content: crate::RailParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3NavRail::new_with(p, content, cx, &pack))
    }

    /// A standard navigation drawer: headline groups over 336dp destination capsules.
    pub fn nav_drawer(
        &self,
        content: crate::DrawerParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3NavDrawer::new_with(p, content, cx, &pack))
    }

    /// A toolbar of icon-button slots, docked or floating.
    pub fn toolbar(
        &self,
        content: crate::ToolbarParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Toolbar::new_with(p, content, cx, &pack))
    }

    /// A snackbar with body text and an optional action; the app owns its lifetime.
    pub fn snackbar(
        &self,
        content: crate::SnackbarParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3Snackbar::new_with(p, content, cx, &pack))
    }

    /// A plain tooltip hung above its anchor; declare it only while it shows.
    pub fn tool_tip(
        &self,
        params: crate::ToolTipParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ToolTip::new(params, &pack))
    }

    /// A rich tooltip with a subhead, supporting text and text-button actions.
    pub fn rich_tool_tip(
        &self,
        params: crate::RichToolTipParams<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let options = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3RichToolTip::new_with(options, params, cx, &pack))
    }

    /// A determinate linear progress bar of `width`; `value` is `0.0..=1.0`.
    pub fn progress_indicator(
        &self,
        id: ui_core::Id,
        value: f32,
        width: f32,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ProgressIndicator::new_with(
            p, id, width, value, cx, &pack,
        ))
    }

    /// The same bar with no value: it sweeps on its own clock until the app has one.
    pub fn progress_indicator_indeterminate(
        &self,
        id: ui_core::Id,
        width: f32,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ProgressIndicator::indeterminate_with(
            p, id, width, cx, &pack,
        ))
    }

    /// The expressive bar: 10 dp tall, its value painted as a travelling wave.
    pub fn progress_indicator_wavy(
        &self,
        id: ui_core::Id,
        value: f32,
        width: f32,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ProgressIndicator::wavy_with(
            p, id, width, value, cx, &pack,
        ))
    }

    /// The expressive bar with no value: two waves sweep on their own clocks.
    pub fn progress_indicator_wavy_indeterminate(
        &self,
        id: ui_core::Id,
        width: f32,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ProgressIndicator::wavy_indeterminate_with(
            p, id, width, cx, &pack,
        ))
    }

    /// A `size` ring showing `value`: a track, then an arc from twelve o'clock.
    pub fn progress_ring(
        &self,
        id: ui_core::Id,
        size: f32,
        value: f32,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ProgressIndicator::ring_with(
            p, id, size, value, cx, &pack,
        ))
    }

    /// A `size` ring with no value: its arc grows, travels and shrinks on its own clock.
    pub fn progress_ring_indeterminate(
        &self,
        id: ui_core::Id,
        size: f32,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        let p = self.options_in(cx.env());
        let pack = self.pack_in(cx);
        Box::new(crate::Md3ProgressIndicator::ring_indeterminate_with(
            p, id, size, cx, &pack,
        ))
    }
}

impl Default for Md3Kit {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleKit for Md3Kit {
    fn slider(
        &self,
        spec: &ui_style::SliderSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        self.slider_sized(
            spec.id,
            spec.value,
            crate::SLIDER_WIDTH,
            crate::spec::SliderSize::XSmall,
            cx,
        )
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
        Box::new(Md3Toggle::new_with(p, spec.id, spec.on, &mut ctx, &pack))
    }

    fn scaffold(
        &self,
        spec: &ScaffoldSpec<'_>,
        cx: &mut ui_widgets::Cx<'_, '_>,
    ) -> Box<dyn Widget> {
        Box::new(cx.with_cx(|cx| Md3Scaffold::new(spec, cx)))
    }
}
