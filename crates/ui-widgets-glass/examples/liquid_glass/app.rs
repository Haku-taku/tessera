// The showcase's app state, its four tabs and its `App` implementation.

use ui_core::change::Change;
use ui_core::geom::Color;
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::tree::ImageFit;
use ui_core::{Id, NodeKind, Sense};
use ui_platform::view::App;
use ui_platform::{Container, Element};
use ui_style::{Env, Style, StyleScope};
use ui_widgets::Cx;
use ui_widgets_glass::{BAR_H, GlassTab, GlassTabBar, pack_of};

use crate::pages;

/// The four tabs the reference's bottom bar carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tab {
    Explore,
    Widgets,
    Demos,
    Examples,
}

impl Tab {
    pub(crate) fn all() -> [Tab; 4] {
        [Tab::Explore, Tab::Widgets, Tab::Demos, Tab::Examples]
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Explore => "Explore",
            Tab::Widgets => "Widgets",
            Tab::Demos => "Demos",
            Tab::Examples => "Examples",
        }
    }

    /// The SF Symbol the bar draws, and the filled variant for the selected tab.
    pub(crate) fn icon(self) -> (&'static str, &'static str) {
        match self {
            Tab::Explore => ("safari", "safari.fill"),
            Tab::Widgets => ("square.grid.2x2", "square.grid.2x2.fill"),
            Tab::Demos => ("sparkles", "sparkles"),
            Tab::Examples => ("cube", "cube.fill"),
        }
    }

    pub(crate) fn id(self) -> Id {
        let k = Tab::all().iter().position(|t| *t == self).unwrap_or(0);
        Id::of(("tab", k))
    }
}

/// The widgets tab's six categories, in the reference's order.
pub(crate) const CATEGORIES: [&str; 6] = [
    "Containers",
    "Feedback",
    "Input",
    "Interactive",
    "Overlays",
    "Surfaces",
];

pub(crate) struct LiquidGlassApp {
    pub(crate) tab: Tab,
    /// The widgets tab's own selection.
    pub(crate) category: usize,
    pub(crate) toggle_on: bool,
    pub(crate) slider: f32,
    pub(crate) text: String,
    pub(crate) chip_on: bool,
    pub(crate) progress: f32,
    pub(crate) playing: bool,
    pub(crate) track: usize,
    pub(crate) menu_open: bool,
    pub(crate) dialog_open: bool,
    pub(crate) sheet_open: bool,
}

impl LiquidGlassApp {
    pub fn new() -> Self {
        Self {
            tab: Tab::Explore,
            category: 0,
            toggle_on: true,
            slider: 0.35,
            text: String::new(),
            chip_on: true,
            progress: 0.4,
            playing: false,
            track: 0,
            menu_open: false,
            dialog_open: false,
            sheet_open: false,
        }
    }
}

/// The pack every control on this screen resolves against.
pub(crate) fn pack(cx: &Cx<'_, '_>) -> &'static Style {
    pack_of(StyleScope::of(cx.env()).appearance)
}

/// The pack's primary label ink.
pub(crate) fn ink() -> Color {
    pack_of(ui_design::Appearance::Dark)
        .ink("textFillColorPrimary")
        .unwrap_or(Color::WHITE)
}

/// The pack's secondary label ink.
pub(crate) fn ink_muted() -> Color {
    pack_of(ui_design::Appearance::Dark)
        .ink("textFillColorSecondary")
        .unwrap_or(Color::from_rgba8(180, 180, 190, 255))
}

impl App for LiquidGlassApp {
    fn view(&mut self, info: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        let env = Env::with_scope(
            None,
            StyleScope {
                appearance: ui_design::Appearance::Dark,
                theme_from: None,
                theme_t: 0.0,
            },
            (),
        );
        let mut cx = Cx::new(info, env);
        let body = pages::page(self, &mut cx);
        let bar = self.tab_bar(&mut cx);
        // The photo is the surface every pane refracts: the reference's scaffold paints
        // one behind the whole app.
        Box::new(
            Container::new(Id::str("root"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fill(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::Yes,
                })
                .background_image(ui_demo::WALLPAPER, ImageFit::Cover)
                .child(Box::new(
                    Container::new(Id::str("scroll"), NodeKind::Column)
                        .size(SizeClass {
                            width: AxisSize::fill(0.0, 0.0),
                            height: AxisSize::fill(0.0, 0.0),
                            grow_x: Grow::Yes,
                            grow_y: Grow::Yes,
                        })
                        .scrollable()
                        .sense(Sense::SCROLL_OWNED)
                        // White glass labels need a floor under them: the photo is dimmed
                        // where the page draws, as the reference's backdrop is.
                        .background(Color::from_rgba8(0, 0, 0, 96), ui_core::geom::Corners::ZERO)
                        .child(body),
                ))
                .child(bar),
        )
    }

    fn changed(&mut self, id: Id, change: Change) -> bool {
        match change {
            Change::Selected(Some(target)) => {
                if let Some(tab) = Tab::all().into_iter().find(|t| t.id() == target) {
                    self.tab = tab;
                    return true;
                }
                if let Some(k) = index_of(&target, "cat", CATEGORIES.len()) {
                    self.category = k;
                    return true;
                }
                if target == Id::str("chip").with("delete") {
                    self.chip_on = false;
                    return true;
                }
                // A dialog answers with one of its own buttons; both close it here.
                if target == Id::str("dialog/ok") || target == Id::str("dialog/cancel") {
                    self.dialog_open = false;
                    return true;
                }
                if id == Id::str("menu") {
                    self.menu_open = false;
                    return true;
                }
                if id == Id::str("dialog") || id == Id::str("sheet") {
                    self.dialog_open = false;
                    self.sheet_open = false;
                    return true;
                }
            }
            Change::Toggled { on: Some(on) } => {
                if id == Id::str("toggle") {
                    self.toggle_on = on;
                    return true;
                }
                if id == Id::str("dialog") || id == Id::str("sheet") {
                    self.dialog_open = on;
                    self.sheet_open = on;
                    return true;
                }
            }
            Change::Value(v) => {
                if id == Id::str("slider") {
                    self.slider = v as f32;
                    return true;
                }
                if id == Id::str("music/progress") {
                    self.progress = v as f32;
                    return true;
                }
            }
            Change::Cancelled => {
                if id == Id::str("dialog") {
                    self.dialog_open = false;
                    return true;
                }
            }
            Change::Text(text) => {
                if id == Id::str("field") {
                    self.text = text;
                    return true;
                }
            }
            Change::Dismissed => {
                if id == Id::str("menu") {
                    self.menu_open = false;
                    return true;
                }
                if id == Id::str("dialog") || id == Id::str("sheet") {
                    self.dialog_open = false;
                    self.sheet_open = false;
                    return true;
                }
            }
            Change::Activated => {
                // A chip has no two-state wash of its own: the app owns whether it is picked.
                if id == Id::str("chip") {
                    self.chip_on = !self.chip_on;
                    return true;
                }
                if let Some(k) = index_of(&id, "track", 3) {
                    self.track = k;
                    return true;
                }
                if id == Id::str("menu/anchor") {
                    self.menu_open = !self.menu_open;
                    return true;
                }
                if id == Id::str("dialog/show") {
                    self.dialog_open = true;
                    return true;
                }
                if id == Id::str("sheet/show") {
                    self.sheet_open = true;
                    return true;
                }
                if id == Id::str("music/play") {
                    self.playing = !self.playing;
                    return true;
                }
                if id == Id::str("explore/music") {
                    self.tab = Tab::Examples;
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}

impl LiquidGlassApp {
    /// The bottom bar: four tabs in one glass capsule, centred over the photo.
    fn tab_bar(&self, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
        let style = pack(cx);
        let width = 4.0 * 92.0 + 24.0;
        let tabs: Vec<GlassTab> = Tab::all()
            .into_iter()
            .map(|tab| {
                let (icon, _) = tab.icon();
                GlassTab {
                    id: tab.id(),
                    label: tab.label().to_string(),
                    icon: ui_widgets::IconSource::sf(icon),
                }
            })
            .collect();
        let selected = Tab::all().iter().position(|t| *t == self.tab).unwrap_or(0);
        let id = Id::str("tabbar");
        let bar = GlassTabBar::new(id, tabs, selected, width, &mut cx.at(id), &style);
        let bar = cx.element(Box::new(bar));
        Box::new(
            Container::new(Id::str("bar-slot"), NodeKind::Stack)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::hug(0.0, BAR_H + 28.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .map(|d| {
                    d.padding(ui_core::geom::Insets::new(0.0, 0.0, 0.0, 16.0))
                        .background(Color::from_rgba8(0, 0, 0, 96), ui_core::geom::Corners::ZERO)
                        .stack_align(ui_core::layout::OverlayAlign::Center)
                })
                .child(bar),
        )
    }
}

/// The index a tuple id like `("tab", 2)` carries, out of `n` candidates.
fn index_of(id: &Id, tag: &'static str, n: usize) -> Option<usize> {
    (0..n).find(|k| Id::of((tag, *k)) == *id)
}
