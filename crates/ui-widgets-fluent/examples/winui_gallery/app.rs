// The gallery's app state, its navigation table and its `App` implementation.

use ui_core::change::Change;
use ui_core::geom::{Color, Insets};
use ui_core::layout::{AxisSize, Grow, SizeClass};
use ui_core::{Decl, Id, NodeKind, Sense};
use ui_platform::view::App;
use ui_platform::{Container, Element};
use ui_style::{Env, StyleScope};
use ui_widgets::Cx;
use ui_widgets_fluent::{
    CheckState, FluentKit, InfoBarParams, InfoBarSeverity, NavigationItem, NavigationViewParams,
    PaneDisplayMode,
};

use crate::pages;

/// Every page the gallery can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Home,
    Button,
    CheckBox,
    RadioButton,
    ComboBox,
    Slider,
    ToggleSwitch,
    TextBox,
    InfoBar,
    ProgressBar,
    ProgressRing,
    ToolTip,
    ContentDialog,
    MenuFlyout,
    NavigationView,
    TabView,
}

impl Page {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Page::Home => "home",
            Page::Button => "button",
            Page::CheckBox => "checkbox",
            Page::RadioButton => "radio",
            Page::ComboBox => "combo",
            Page::Slider => "slider",
            Page::ToggleSwitch => "toggle",
            Page::TextBox => "textbox",
            Page::InfoBar => "infobar",
            Page::ProgressBar => "progressbar",
            Page::ProgressRing => "progressring",
            Page::ToolTip => "tooltip",
            Page::ContentDialog => "dialog",
            Page::MenuFlyout => "menuflyout",
            Page::NavigationView => "navigationview",
            Page::TabView => "tabview",
        }
    }

    /// The page's name as Microsoft's gallery titles it.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Button => "Button",
            Page::CheckBox => "CheckBox",
            Page::RadioButton => "RadioButton",
            Page::ComboBox => "ComboBox",
            Page::Slider => "Slider",
            Page::ToggleSwitch => "ToggleSwitch",
            Page::TextBox => "TextBox",
            Page::InfoBar => "InfoBar",
            Page::ProgressBar => "ProgressBar",
            Page::ProgressRing => "ProgressRing",
            Page::ToolTip => "ToolTip",
            Page::ContentDialog => "ContentDialog",
            Page::MenuFlyout => "MenuFlyout",
            Page::NavigationView => "NavigationView",
            Page::TabView => "TabView",
        }
    }

    /// The line under the title, taken from WinUI's own control metadata.
    pub(crate) fn description(self) -> &'static str {
        match self {
            Page::Home => "Controls, styles and guidance for building Windows apps.",
            Page::Button => "A control that responds to user input and raises a Click event.",
            Page::CheckBox => {
                "A control that a user can select or clear, with an optional indeterminate state."
            }
            Page::RadioButton => {
                "A control that allows a user to select a single option from a group of options."
            }
            Page::ComboBox => "A drop-down list of items a user can select from.",
            Page::Slider => {
                "A control that lets the user select from a range of values by moving a thumb."
            }
            Page::ToggleSwitch => "A switch that can be toggled between two states: on and off.",
            Page::TextBox => "A single-line or multi-line text input field.",
            Page::InfoBar => "An inline notification with a severity and an optional close button.",
            Page::ProgressBar => "Shows the progress of an operation in a horizontal bar.",
            Page::ProgressRing => "Shows the progress of an operation in a circular ring.",
            Page::ToolTip => "A pop-up that shows information when the user hovers over a control.",
            Page::ContentDialog => "A modal dialog that the app shows over its own content.",
            Page::MenuFlyout => "A flyout that shows a list of commands.",
            Page::NavigationView => {
                "The navigation container for an app: a pane of destinations beside a content frame."
            }
            Page::TabView => "A control that shows a set of tabs and their content.",
        }
    }

    pub(crate) fn nav_id(self) -> Id {
        Id::of(("nav", self.tag()))
    }
}

/// One row of the pane: a category title, or a destination.
pub(crate) enum NavRow {
    Header(&'static str),
    Page(Page),
}

/// The pane, in Microsoft's own category order, cut down to the controls this crate draws.
pub(crate) const NAV: &[NavRow] = &[
    NavRow::Page(Page::Home),
    NavRow::Header("Basic input"),
    NavRow::Page(Page::Button),
    NavRow::Page(Page::CheckBox),
    NavRow::Page(Page::RadioButton),
    NavRow::Page(Page::ComboBox),
    NavRow::Page(Page::Slider),
    NavRow::Page(Page::ToggleSwitch),
    NavRow::Page(Page::TextBox),
    NavRow::Header("Status & info"),
    NavRow::Page(Page::InfoBar),
    NavRow::Page(Page::ProgressBar),
    NavRow::Page(Page::ProgressRing),
    NavRow::Page(Page::ToolTip),
    NavRow::Header("Dialogs & flyouts"),
    NavRow::Page(Page::ContentDialog),
    NavRow::Page(Page::MenuFlyout),
    NavRow::Header("Navigation"),
    NavRow::Page(Page::NavigationView),
    NavRow::Page(Page::TabView),
];

/// The icon face's names for the destinations that carry one.
fn nav_icon(page: Page) -> &'static str {
    match page {
        Page::Home => "home_16_regular",
        Page::Button => "button_16_regular",
        Page::CheckBox => "checkbox_checked_16_regular",
        Page::RadioButton => "radio_button_16_regular",
        Page::ComboBox => "chevron_down_16_regular",
        Page::Slider => "options_16_regular",
        Page::ToggleSwitch => "toggle_left_16_regular",
        Page::TextBox => "text_field_16_regular",
        Page::InfoBar => "info_16_regular",
        Page::ProgressBar => "arrow_sync_16_regular",
        Page::ProgressRing => "arrow_clockwise_16_regular",
        Page::ToolTip => "comment_16_regular",
        Page::ContentDialog => "window_16_regular",
        Page::MenuFlyout => "list_16_regular",
        Page::NavigationView => "navigation_16_regular",
        Page::TabView => "tabs_16_regular",
    }
}

/// The pane index a page sits at, headers included.
pub(crate) fn nav_index(page: Page) -> usize {
    NAV.iter()
        .position(|row| matches!(row, NavRow::Page(p) if *p == page))
        .unwrap_or(0)
}

/// One tab the TabView page owns: a stable id and the label it shows.
pub(crate) struct TabEntry {
    pub(crate) id: Id,
    pub(crate) label: String,
}

pub(crate) struct WinUiGallery {
    pub(crate) page: Page,
    pub(crate) nav_open: bool,
    /// The checkbox page's three boxes, in the order Microsoft's sample lists them.
    pub(crate) check: [CheckState; 3],
    pub(crate) radio: usize,
    pub(crate) combo: Option<usize>,
    pub(crate) combo_open: bool,
    pub(crate) slider: f32,
    pub(crate) toggles: [bool; 2],
    pub(crate) text: String,
    /// The second field's buffer; the page has two text boxes, so both need an owner.
    pub(crate) notes: String,
    /// `IsOpen` per severity bar, in the order they are shown.
    pub(crate) info_open: [bool; 4],
    /// The InfoBar the app itself raises above every page.
    pub(crate) app_info_open: bool,
    pub(crate) dialog_open: bool,
    pub(crate) menu_open: bool,
    /// The TabView page's tabs and its selected index; both belong to the app.
    pub(crate) tabs: Vec<TabEntry>,
    pub(crate) tab: usize,
    /// The serial the next added tab's id carries.
    pub(crate) next_tab: usize,
    /// The embedded NavigationView sample's own selection, pane mode and pane state.
    pub(crate) demo_nav: usize,
    pub(crate) demo_mode: usize,
    pub(crate) demo_nav_open: bool,
    kit: FluentKit,
}

/// The page `DEMO_PAGE=<tag>` opens on, for a screenshot of one page without a click.
fn opening_page() -> Page {
    let Ok(tag) = std::env::var("DEMO_PAGE") else {
        return Page::Home;
    };
    NAV.iter()
        .find_map(|row| match row {
            NavRow::Page(page) if page.tag() == tag => Some(*page),
            _ => None,
        })
        .unwrap_or(Page::Home)
}

impl WinUiGallery {
    pub fn new() -> Self {
        let tabs: Vec<TabEntry> = ["Home", "Documents", "Settings"]
            .into_iter()
            .enumerate()
            .map(|(k, label)| TabEntry {
                id: Id::of(("tabitem", k)),
                label: label.to_string(),
            })
            .collect();
        Self {
            page: opening_page(),
            nav_open: true,
            check: [
                CheckState::Checked,
                CheckState::Unchecked,
                CheckState::Indeterminate,
            ],
            radio: 0,
            combo: Some(0),
            combo_open: false,
            slider: 0.4,
            toggles: [true, false],
            text: String::new(),
            notes: String::new(),
            info_open: [true; 4],
            app_info_open: false,
            dialog_open: false,
            menu_open: false,
            tabs,
            tab: 0,
            next_tab: 3,
            demo_nav: 0,
            demo_mode: 0,
            demo_nav_open: true,
            kit: FluentKit::new(),
        }
    }

    pub(crate) fn kit(&self) -> &FluentKit {
        &self.kit
    }
}

impl App for WinUiGallery {
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
        // The page is built as an element tree first: it needs the widget context, while the
        // NavigationView's content is a declaration and needs the core one.
        let body = {
            let mut cx = Cx::new(info, env.clone());
            pages::page_body(self, self.page, &mut cx)
        };
        // The page stays an element: a control inside it has to report what the user did.
        let content: Vec<Box<dyn Element>> = vec![Box::new(
            Container::new(Id::str("page"), NodeKind::Column)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fill(0.0, 0.0),
                    grow_x: Grow::Yes,
                    grow_y: Grow::Yes,
                })
                .scrollable()
                .sense(Sense::SCROLL_OWNED)
                .child(body),
        )];
        let mut cx = Cx::new(info, env);
        let items: Vec<NavigationItem> = NAV
            .iter()
            .map(|row| match row {
                NavRow::Header(label) => {
                    NavigationItem::header(Id::of(("navheader", *label)), *label)
                }
                NavRow::Page(page) => {
                    NavigationItem::new(page.nav_id(), page.title(), nav_icon(*page))
                }
            })
            .collect();
        let params = NavigationViewParams::new(Id::str("nav"), &items)
            .selected(nav_index(self.page))
            .open(self.nav_open)
            .display_mode(PaneDisplayMode::Expanded)
            .content(content);
        let view = self.kit.navigation_view(params, &mut cx);
        // WinUI paints the page itself: the NavigationView is translucent over the window,
        // and a Wayland window has no backdrop of its own to show through.
        // The dialog is a modal, so it is placed on the window's rect wherever it sits.
        let mut root = Container::new(Id::str("root"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .background(page_background(), ui_core::geom::Corners::ZERO);
        root = root.child(view);
        // The app-level bar floats over the shell: it takes no layout space, so no page
        // shrinks under it, and it is declared after the view so it paints above it.
        if self.app_info_open {
            let params = InfoBarParams::new(
                Id::str("app-info"),
                "App-level notification",
                "This InfoBar is raised by the app above every page, not by the page itself.",
            )
            .severity(InfoBarSeverity::Informational);
            let bar = self.kit.info_bar(params, &mut cx);
            let slot = Container::new(Id::str("app-info-slot"), NodeKind::Box)
                .padding(Insets::all(8.0))
                .map(|slot| {
                    slot.fixed().inset(Insets {
                        left: 0.0,
                        top: 0.0,
                        right: 0.0,
                        ..Insets::AUTO
                    })
                })
                .child(cx.element(bar));
            root = root.child(Box::new(slot));
        }
        if self.dialog_open {
            root = root.child(pages::dialog(self, &mut cx));
        }
        Box::new(root)
    }

    fn changed(&mut self, id: Id, change: Change) -> bool {
        if let Change::Selected(Some(target)) = change
            && let Some(row) = NAV.iter().find(|row| match row {
                NavRow::Page(page) => page.nav_id() == target,
                NavRow::Header(_) => false,
            })
            && let NavRow::Page(page) = row
        {
            self.page = *page;
            return true;
        }
        if let Change::Toggled { on: Some(on) } = change
            && id == Id::str("nav")
        {
            self.nav_open = on;
            return true;
        }
        pages::changed(self, id, change)
    }
}

/// The window's own surface: the brush a WinUI page root paints.
fn page_background() -> Color {
    ink_of("ApplicationPageBackgroundThemeBrush")
        .unwrap_or(Color::from_rgba8(0x20, 0x20, 0x20, 255))
}

pub(crate) fn ink_of(name: &'static str) -> Option<Color> {
    ui_widgets_fluent::pack_of(ui_design::Appearance::Dark).ink(name)
}

/// The page's ink, from the pack the gallery is drawn in.
pub(crate) fn ink(primary: bool) -> Color {
    let pack = ui_widgets_fluent::pack_of(ui_design::Appearance::Dark);
    let name = if primary {
        "TextFillColorPrimaryBrush"
    } else {
        "TextFillColorSecondaryBrush"
    };
    pack.ink(name).unwrap_or(if primary {
        Color::WHITE
    } else {
        Color::from_rgba8(160, 160, 160, 255)
    })
}

/// A line of text as a declaration.
pub(crate) fn label(id: Id, body: impl Into<String>, size: f32, colour: Color, max_w: f32) -> Decl {
    use ui_widgets::text::Text;
    Text::new(id, body, size, colour).max_width(max_w).resolve()
}
