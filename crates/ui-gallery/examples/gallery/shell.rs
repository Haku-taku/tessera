// Shared gallery scaffolding: page enum, ids, style scopes and shell chrome.

pub(crate) use std::time::Instant;

pub(crate) use ui_core::draw::{Acrylic, BackdropMaterial, GradientAxis};
pub(crate) use ui_core::geom::{Color, Corners, Insets, Point, Rect};
pub(crate) use ui_core::layout::{AxisSize, Grow, SizeClass};
pub(crate) use ui_core::tree::ImageFit;
pub(crate) use ui_core::{Decl, Id, ModalTarget, NodeKind, Weight};
pub(crate) use ui_i18n::Msg;
pub(crate) use ui_platform::view::{App, AppLoop};
pub(crate) use ui_platform::{Container, Element, from_decl};
pub(crate) use ui_style::{Env, SliderSpec, ToggleSpec};
pub(crate) use ui_widgets::{Cx, IconSource, ScaffoldSpec, StyleKit, Widget};
pub(crate) use ui_widgets_fluent::FluentKit;
pub(crate) use ui_widgets_glass::HigKit;
pub(crate) use ui_widgets_md3::Md3Kit;

/// The UI text face: Apple's own pair where the host has it, otherwise Inter for Latin and
/// the system's Chinese face for CJK — the open pairing closest to SF Pro + PingFang SC.
pub(crate) const TEXT_FAMILY: &str = "Inter";

/// Faces in the order a glyph is looked up: Latin first, then the Chinese face behind it.
pub(crate) const TEXT_STACK: &[&str] = &[
    "SF Pro Text",
    "SF Pro Display",
    TEXT_FAMILY,
    "PingFang SC",
    "Noto Sans SC",
    "Noto Sans CJK SC",
    "MiSans VF",
    "HarmonyOS Sans SC",
];

pub(crate) const NAV_W: f32 = 360.0;
/// The content area's width: the window minus the nav column and the shell's two paddings.
pub(crate) const CONTENT_W: f32 = WIN_W as f32 - NAV_W - 32.0;
pub(crate) const COL_W: f32 = 380.0;
pub(crate) const COL_CTRL_H: f32 = 300.0;
pub(crate) const COL_TALL_H: f32 = 800.0;
// Enough for three stacked bars with their captions, without the tall column's empty half.
pub(crate) const COL_MID_H: f32 = 460.0;
pub(crate) const WIN_W: u32 = 1600;
// Tall enough for a `COL_TALL_H` column below the header; shorter squeezes its last rows.
// The nav scrolls, so pages may be added without raising this: rows below the window are
// reachable by wheel instead of being clipped away for good.
pub(crate) const WIN_H: u32 = 1060;

/// The four pages of the acceptance gallery.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Switch,
    Checkbox,
    Radio,
    Slider,
    Tabs,
    ToolTip,
    InfoBar,
    Buttons,
    IconButton,
    FabMenu,
    MenuFlyout,
    Combo,
    Chips,
    Cards,
    Lists,
    Icons,
    Dialogs,
    ActionSheet,
    Toast,
    ModalSheet,
    Stepper,
    DatePicker,
    TimePicker,
    AppBar,
    Carousel,
    TextField,
    Search,
    Selection,
    Generic,
    Progress,
    Segmented,
    NavBar,
    NavRail,
    NavDrawer,
    NavView,
    Toolbar,
    Snackbar,
    Layouts,
    Grid,
    Inline,
    Markdown,
    Math,
    Positioning,
    Materials,
    Md3System,
    FluentSystem,
    HigSystem,
}

pub(crate) const PAGES: [Page; 47] = [
    Page::Switch,
    Page::Checkbox,
    Page::Radio,
    Page::Slider,
    Page::Tabs,
    Page::ToolTip,
    Page::InfoBar,
    Page::Buttons,
    Page::IconButton,
    Page::FabMenu,
    Page::MenuFlyout,
    Page::Combo,
    Page::Chips,
    Page::Cards,
    Page::Lists,
    Page::Icons,
    Page::Dialogs,
    Page::ActionSheet,
    Page::Toast,
    Page::ModalSheet,
    Page::Stepper,
    Page::DatePicker,
    Page::TimePicker,
    Page::AppBar,
    Page::Carousel,
    Page::TextField,
    Page::Search,
    Page::Selection,
    Page::Generic,
    Page::Progress,
    Page::Segmented,
    Page::NavBar,
    Page::NavRail,
    Page::NavDrawer,
    Page::NavView,
    Page::Toolbar,
    Page::Snackbar,
    Page::Layouts,
    Page::Grid,
    Page::Inline,
    Page::Markdown,
    Page::Math,
    Page::Positioning,
    Page::Materials,
    Page::Md3System,
    Page::FluentSystem,
    Page::HigSystem,
];

/// The nav tree: one section per row of pages, each titled by its own key.
pub(crate) const SECTIONS: [(&str, Msg, &[Page]); 4] = [
    (
        "controls",
        Msg::GALLERY_SECTION_CONTROLS,
        &[
            Page::Switch,
            Page::Checkbox,
            Page::Radio,
            Page::Slider,
            Page::Tabs,
            Page::ToolTip,
            Page::InfoBar,
            Page::Buttons,
            Page::IconButton,
            Page::FabMenu,
            Page::MenuFlyout,
            Page::Combo,
            Page::Chips,
            Page::Cards,
            Page::Lists,
            Page::Icons,
            Page::Dialogs,
            Page::ActionSheet,
            Page::Toast,
            Page::ModalSheet,
            Page::Stepper,
            Page::DatePicker,
            Page::TimePicker,
            Page::AppBar,
            Page::Carousel,
            Page::TextField,
            Page::Search,
            Page::Selection,
            Page::Generic,
            Page::Progress,
            Page::Segmented,
            Page::NavBar,
            Page::NavRail,
            Page::NavDrawer,
            Page::NavView,
            Page::Toolbar,
            Page::Snackbar,
        ],
    ),
    (
        "layouts",
        Msg::GALLERY_SECTION_LAYOUTS,
        &[
            Page::Layouts,
            Page::Grid,
            Page::Inline,
            Page::Markdown,
            Page::Math,
            Page::Positioning,
        ],
    ),
    (
        "materials",
        Msg::GALLERY_SECTION_MATERIALS,
        &[Page::Materials],
    ),
    (
        "design-systems",
        Msg::GALLERY_SECTION_DESIGN_SYSTEMS,
        &[Page::Md3System, Page::FluentSystem, Page::HigSystem],
    ),
];

impl Page {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Page::Switch => "switch",
            Page::Checkbox => "checkbox",
            Page::Radio => "radio",
            Page::Slider => "slider",
            Page::Tabs => "tabs",
            Page::ToolTip => "tooltip",
            Page::InfoBar => "info-bar",
            Page::Buttons => "buttons",
            Page::IconButton => "icon-button",
            Page::FabMenu => "fab-menu",
            Page::MenuFlyout => "menu-flyout",
            Page::Combo => "combo",
            Page::Chips => "chips",
            Page::Cards => "cards",
            Page::Lists => "lists",
            Page::Icons => "icons",
            Page::Dialogs => "dialogs",
            Page::ActionSheet => "action-sheet",
            Page::Toast => "toast",
            Page::ModalSheet => "modal-sheet",
            Page::Stepper => "stepper",
            Page::DatePicker => "date-picker",
            Page::TimePicker => "time-picker",
            Page::AppBar => "app-bar",
            Page::Carousel => "carousel",
            Page::TextField => "text-field",
            Page::Search => "search",
            Page::Selection => "selection",
            Page::Generic => "generic",
            Page::Progress => "progress",
            Page::Segmented => "segmented",
            Page::NavBar => "nav-bar",
            Page::NavRail => "nav-rail",
            Page::NavDrawer => "nav-drawer",
            Page::NavView => "nav-view",
            Page::Toolbar => "toolbar",
            Page::Snackbar => "snackbar",
            Page::Layouts => "layouts",
            Page::Grid => "grid",
            Page::Inline => "inline",
            Page::Markdown => "markdown",
            Page::Math => "math",
            Page::Positioning => "positioning",
            Page::Materials => "materials",
            Page::Md3System => "md3-system",
            Page::FluentSystem => "fluent-system",
            Page::HigSystem => "hig-system",
        }
    }

    /// The page's nav and header title, as a catalog key.
    pub(crate) fn title(self) -> Msg {
        match self {
            Page::Switch => Msg::GALLERY_PAGE_SWITCH_TITLE,
            Page::Checkbox => Msg::GALLERY_PAGE_CHECKBOX_TITLE,
            Page::Radio => Msg::GALLERY_PAGE_RADIO_TITLE,
            Page::Slider => Msg::GALLERY_PAGE_SLIDER_TITLE,
            Page::Tabs => Msg::GALLERY_PAGE_TABS_TITLE,
            Page::ToolTip => Msg::GALLERY_PAGE_TOOLTIP_TITLE,
            Page::InfoBar => Msg::GALLERY_PAGE_INFO_BAR_TITLE,
            Page::Buttons => Msg::GALLERY_PAGE_BUTTONS_TITLE,
            Page::IconButton => Msg::GALLERY_PAGE_ICON_BUTTON_TITLE,
            Page::FabMenu => Msg::GALLERY_PAGE_FAB_MENU_TITLE,
            Page::MenuFlyout => Msg::GALLERY_PAGE_MENU_FLYOUT_TITLE,
            Page::Combo => Msg::GALLERY_PAGE_COMBO_TITLE,
            Page::Chips => Msg::GALLERY_PAGE_CHIPS_TITLE,
            Page::Cards => Msg::GALLERY_PAGE_CARDS_TITLE,
            Page::Lists => Msg::GALLERY_PAGE_LISTS_TITLE,
            Page::Icons => Msg::GALLERY_PAGE_ICONS_TITLE,
            Page::Dialogs => Msg::GALLERY_PAGE_DIALOGS_TITLE,
            Page::ActionSheet => Msg::GALLERY_PAGE_ACTION_SHEET_TITLE,
            Page::Toast => Msg::GALLERY_PAGE_TOAST_TITLE,
            Page::ModalSheet => Msg::GALLERY_PAGE_MODAL_SHEET_TITLE,
            Page::Stepper => Msg::GALLERY_PAGE_STEPPER_TITLE,
            Page::DatePicker => Msg::GALLERY_PAGE_DATE_PICKER_TITLE,
            Page::TimePicker => Msg::GALLERY_PAGE_TIME_PICKER_TITLE,
            Page::AppBar => Msg::GALLERY_PAGE_APP_BAR_TITLE,
            Page::Carousel => Msg::GALLERY_PAGE_CAROUSEL_TITLE,
            Page::TextField => Msg::GALLERY_PAGE_TEXT_FIELD_TITLE,
            Page::Search => Msg::GALLERY_PAGE_SEARCH_TITLE,
            Page::Selection => Msg::GALLERY_PAGE_SELECTION_TITLE,
            Page::Generic => Msg::GALLERY_PAGE_GENERIC_TITLE,
            Page::Progress => Msg::GALLERY_PAGE_PROGRESS_TITLE,
            Page::Segmented => Msg::GALLERY_PAGE_SEGMENTED_TITLE,
            Page::NavBar => Msg::GALLERY_PAGE_NAV_BAR_TITLE,
            Page::NavRail => Msg::GALLERY_PAGE_NAV_RAIL_TITLE,
            Page::NavDrawer => Msg::GALLERY_PAGE_NAV_DRAWER_TITLE,
            Page::NavView => Msg::GALLERY_PAGE_NAV_VIEW_TITLE,
            Page::Toolbar => Msg::GALLERY_PAGE_TOOLBAR_TITLE,
            Page::Snackbar => Msg::GALLERY_PAGE_SNACKBAR_TITLE,
            Page::Layouts => Msg::GALLERY_PAGE_LAYOUTS_TITLE,
            Page::Grid => Msg::GALLERY_PAGE_GRID_TITLE,
            Page::Inline => Msg::GALLERY_PAGE_INLINE_TITLE,
            Page::Markdown => Msg::GALLERY_PAGE_MARKDOWN_TITLE,
            Page::Math => Msg::GALLERY_PAGE_MATH_TITLE,
            Page::Positioning => Msg::GALLERY_PAGE_POSITIONING_TITLE,
            Page::Materials => Msg::GALLERY_PAGE_MATERIALS_TITLE,
            Page::Md3System => Msg::GALLERY_PAGE_MD3_SYSTEM_TITLE,
            Page::FluentSystem => Msg::GALLERY_PAGE_FLUENT_SYSTEM_TITLE,
            Page::HigSystem => Msg::GALLERY_PAGE_HIG_SYSTEM_TITLE,
        }
    }

    /// The Material Symbols glyph the nav shows beside this destination's label.
    ///
    /// A drawer row carries its own mark; a page with no obvious one borrows the glyph of
    /// the thing it demonstrates rather than repeating a generic dot down the whole list.
    pub(crate) fn icon_name(self) -> &'static str {
        match self {
            Page::Switch => "toggle_on",
            Page::Checkbox => "check_box",
            Page::Radio => "radio_button_checked",
            Page::Slider => "linear_scale",
            Page::Tabs => "tab",
            Page::ToolTip => "tooltip",
            Page::InfoBar => "info",
            Page::Buttons => "smart_button",
            Page::IconButton => "stylus_note",
            Page::FabMenu => "add",
            Page::MenuFlyout => "menu",
            Page::Combo => "arrow_drop_down",
            Page::Chips => "label",
            Page::Cards => "credit_card",
            Page::Lists => "list",
            Page::Icons => "star",
            Page::Dialogs => "dialogs",
            Page::ActionSheet => "vertical_split",
            Page::Toast => "notifications",
            Page::ModalSheet => "web_asset",
            Page::Stepper => "exposure",
            Page::DatePicker => "calendar_month",
            Page::TimePicker => "schedule",
            Page::AppBar => "menu_open",
            Page::Carousel => "view_carousel",
            Page::TextField => "text_fields",
            Page::Search => "search",
            Page::Selection => "highlight",
            Page::Generic => "widgets",
            Page::Progress => "progress_activity",
            Page::Segmented => "view_week",
            Page::NavBar => "menu",
            Page::NavRail => "view_sidebar",
            Page::NavDrawer => "dock_to_left",
            Page::NavView => "view_agenda",
            Page::Toolbar => "build",
            Page::Snackbar => "announcement",
            Page::Layouts => "dashboard",
            Page::Grid => "grid_view",
            Page::Inline => "format_align_left",
            Page::Markdown => "article",
            Page::Math => "functions",
            Page::Positioning => "open_with",
            Page::Materials => "blur_on",
            Page::Md3System => "palette",
            Page::FluentSystem => "window",
            Page::HigSystem => "phone_iphone",
        }
    }

    pub(crate) fn nav_id(self) -> Id {
        Id::of(("nav", self.tag()))
    }

    pub(crate) fn body_id(self) -> Id {
        Id::of({
            let tag = self.tag();
            format!("page/{tag}")
        })
    }
}

/// Column tags and title keys, in render order.
pub(crate) const STYLES: [(&str, Msg); 3] = [
    ("md3", Msg::GALLERY_STYLE_MD3),
    ("fluent", Msg::GALLERY_STYLE_FLUENT),
    ("glass", Msg::GALLERY_STYLE_GLASS),
];

pub(crate) fn col_id(page: Page, tag: &'static str) -> Id {
    Id::of(("col", tag, page.tag()))
}

pub(crate) fn switch_id(page: Page, tag: &'static str) -> Id {
    Id::of(("switch", tag, page.tag()))
}

/// The Material Symbols glyph `name` as a slot source, resolved by the control's ink.
pub(crate) fn ms(name: &str) -> IconSource {
    IconSource::glyph(name, ui_widgets::IconSet::Outlined)
}

pub(crate) fn label(id: Id, body: impl Into<String>, size: f32, color: Color, max_w: f32) -> Decl {
    ui_widgets::text::leaf(
        id,
        body.into(),
        size,
        color,
        Weight::REGULAR,
        max_w,
        0.0,
        0.0,
    )
}

/// A message as an owned string, for the sinks that take `impl Into<String>`.
pub(crate) fn tr(cx: &Cx<'_, '_>, msg: Msg) -> String {
    cx.t(msg).to_string()
}

pub(crate) fn tf(cx: &Cx<'_, '_>, msg: Msg, args: &[(&str, ui_i18n::Value<'static>)]) -> String {
    // `Cx` has no `tf`: build the `Args` bag the engine's `t_args` takes.
    let mut bag = ui_i18n::Args::new();
    for (name, value) in args {
        bag.set(name, value.clone());
    }
    cx.t_args(msg, &bag).to_string()
}

pub(crate) fn arg(value: impl Into<ui_i18n::Value<'static>>) -> ui_i18n::Value<'static> {
    value.into()
}

/// The page's secondary ink, from the same pack the surfaces are painted with: a grey
/// pinned by hand read on the dark surface and left the light one for the light card.
pub(crate) fn muted() -> Color {
    let appearance = ui_style::StyleScope::of(&root_env()).appearance;
    ui_widgets_md3::pack_of(appearance)
        .ink("onSurfaceVariant")
        .unwrap_or_else(|| Color::from_rgba8(150, 158, 176, 255))
}

// Each column's scope: the Fluent one overrides a real metric, the rest stay at defaults.
thread_local! {
    // Theme transition state, readable from `style_env` without threading it through pages.
    pub(crate) static THEME_FROM: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    pub(crate) static THEME_T: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
    /// The frame's catalog scope, so a page that builds an `Env` keeps the translations.
    static I18N_ENV: std::cell::RefCell<Option<std::sync::Arc<Env>>> = const { std::cell::RefCell::new(None) };
    /// The frame's root env (theme scope over the catalog), published per frame.
    static THEME_ENV: std::cell::RefCell<Option<std::sync::Arc<Env>>> = const { std::cell::RefCell::new(None) };
}

/// Publish the catalog-carrying scope; every `Env` the pages build hangs from it.
pub(crate) fn set_i18n_env(env: std::sync::Arc<Env>) {
    I18N_ENV.with(|slot| *slot.borrow_mut() = Some(env));
}

/// Build and publish the frame's language from `--locale` (default: the system's list).
///
/// The gallery is the only binary that takes a language on the command line, because it
/// is a test surface: a screenshot of another language must not need a config file.
pub(crate) fn init_locale() {
    let choice = locale_argument().unwrap_or(ui_i18n::LanguageChoice::Auto);
    let system = ui_platform::posix_system_languages();
    let language = ui_i18n::Language::builtin(choice, &system)
        .expect("the built-in English catalog always builds");
    set_i18n_env(Env::with(Some(Env::root()), language));
}

/// `--locale auto|<tag>`, read before anything else looks at the arguments.
fn locale_argument() -> Option<ui_i18n::LanguageChoice> {
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let value = match argument.strip_prefix("--locale=") {
            Some(value) => value.to_string(),
            None if argument == "--locale" => {
                let Some(value) = args.next() else {
                    eprintln!("--locale needs a value");
                    return None;
                };
                value
            }
            None => continue,
        };
        return match ui_i18n::LanguageChoice::parse(&value) {
            Ok(choice) => Some(choice),
            Err(error) => {
                eprintln!("--locale: {error}; following the system");
                None
            }
        };
    }
    None
}

/// The published scope, or the bare root before the app sets one.
pub(crate) fn i18n_env() -> std::sync::Arc<Env> {
    I18N_ENV
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(Env::root)
}

/// Build the scope one `Env` layer hangs under, carrying the catalog.
pub(crate) fn base_env() -> std::sync::Arc<Env> {
    i18n_env()
}

/// Publish the frame's theme env; the app sets it once per frame, before any page builds.
pub(crate) fn set_root_env(env: std::sync::Arc<Env>) {
    THEME_ENV.with(|slot| *slot.borrow_mut() = Some(env));
}

/// The frame's theme env, or the catalog scope before the app publishes one.
pub(crate) fn root_env() -> std::sync::Arc<Env> {
    THEME_ENV
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(base_env)
}

/// Pending column: a style's placeholder on a page that only MD3 implements yet.
pub(crate) fn pending_column(
    page: Page,
    tag: &'static str,
    width: f32,
    height: f32,
    cx: &Cx<'_, '_>,
) -> Box<dyn Element> {
    let body = Container::new(Id::of(("body", tag)), NodeKind::Column).child_decl(label(
        Id::of(("body", tag, "note")),
        tr(cx, Msg::GALLERY_NOT_IMPLEMENTED),
        12.0,
        muted(),
        width - 24.0,
    ));
    column_w(
        page,
        tag,
        style_title(tag),
        Box::new(body),
        width,
        height,
        cx,
    )
}

/// The two placeholder columns (Fluent, then Glass) every MD3-only page shows.
pub(crate) fn add_pending_pair(
    row: Container,
    page: Page,
    width: f32,
    height: f32,
    cx: &mut Cx<'_, '_>,
) -> Container {
    let mut row = row;
    for tag in ["fluent", "glass"] {
        row = row.child(pending_column(page, tag, width, height, cx));
    }
    row
}

pub(crate) fn theme_from_appearance() -> Option<ui_design::Appearance> {
    THEME_FROM.with(|c| c.get()).map(appearance_of)
}

pub(crate) fn theme_progress() -> f32 {
    THEME_T.with(|c| c.get())
}

/// The context this frame resolves tokens in; the root env carries it, columns inherit.
pub(crate) fn style_scope(dark: bool) -> ui_style::StyleScope {
    ui_style::StyleScope {
        appearance: appearance_of(dark),
        theme_from: theme_from_appearance(),
        theme_t: theme_progress(),
    }
}

/// Build this frame's theme env: the theme scope over the catalog scope.
pub(crate) fn theme_env(dark: bool) -> std::sync::Arc<Env> {
    Env::with_scope(Some(base_env()), style_scope(dark), ())
}

/// A column's env: only its kit's options; the theme scope is inherited from the root.
pub(crate) fn style_env(tag: &'static str) -> std::sync::Arc<Env> {
    match tag {
        "fluent" => Env::with(
            Some(root_env()),
            ui_widgets_fluent::FluentOptions::default(),
        ),
        "md3" => Env::with(Some(root_env()), ui_widgets_md3::Md3Options::default()),
        // Every style column sits on the same dark card, so Glass resolves its dark pack.
        _ => Env::with(Some(root_env()), ui_widgets_glass::HigOptions::default()),
    }
}

/// The title key of the style a column's tag names, for a page that has the tag but not the key.
pub(crate) fn style_title(tag: &str) -> Msg {
    match tag {
        "fluent" => Msg::GALLERY_STYLE_FLUENT,
        "glass" => Msg::GALLERY_STYLE_GLASS,
        _ => Msg::GALLERY_STYLE_MD3,
    }
}

/// A section column: style title over a body, all three at the same size.
pub(crate) fn column(
    page: Page,
    tag: &'static str,
    title: Msg,
    body: Box<dyn Element>,
    height: f32,
    cx: &Cx<'_, '_>,
) -> Box<dyn Element> {
    column_text(page, tag, tr(cx, title), body, COL_W, height)
}

/// The same, with a title the caller already formatted.
pub(crate) fn column_text(
    page: Page,
    tag: &'static str,
    title: String,
    body: Box<dyn Element>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    card(page, tag, title, body, width, AxisSize::fixed(height))
}

/// The same, hugging its content: a page whose body is already taller than any guessed
/// height gets a card that grows with it instead of letting it spill out.
pub(crate) fn column_hug(
    page: Page,
    tag: &'static str,
    title: Msg,
    body: Box<dyn Element>,
    width: f32,
    cx: &Cx<'_, '_>,
) -> Box<dyn Element> {
    card(
        page,
        tag,
        tr(cx, title),
        body,
        width,
        AxisSize::hug(0.0, 0.0),
    )
}

/// The same hug, walking any change in that height instead of snapping to it: the sample
/// for an axis whose extent is its content's measurement rather than a declared number.
pub(crate) fn column_hug_transition(
    page: Page,
    tag: &'static str,
    title: Msg,
    body: Box<dyn Element>,
    width: f32,
    spec: ui_core::AnimSpec,
    cx: &Cx<'_, '_>,
) -> Box<dyn Element> {
    card_walk(
        page,
        tag,
        tr(cx, title),
        body,
        width,
        AxisSize::hug(0.0, 0.0),
        Some(spec),
    )
}

/// How much of the card's surface colour survives the wallpaper behind it.
const CARD_ALPHA: f32 = 0.85;

fn card(
    page: Page,
    tag: &'static str,
    title: String,
    body: Box<dyn Element>,
    width: f32,
    height: AxisSize,
) -> Box<dyn Element> {
    card_walk(page, tag, title, body, width, height, None)
}

fn card_walk(
    page: Page,
    tag: &'static str,
    title: String,
    body: Box<dyn Element>,
    width: f32,
    height: AxisSize,
    spec: Option<ui_core::AnimSpec>,
) -> Box<dyn Element> {
    // The column is an MD3 filled card: `surfaceContainerHighest` at elevation 0, so it
    // follows the theme instead of pinning one dark colour.
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    // Translucent, so the wallpaper reads through the card; the alpha is what keeps the
    // samples legible over a busy photo.
    let fill = Color {
        a: CARD_ALPHA,
        ..pack.ink("surfaceContainerHighest").unwrap_or(Color::WHITE)
    };
    let title_ink = pack.ink("onSurfaceVariant").unwrap_or(Color::BLACK);
    let mut root = Container::new(col_id(page, tag), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fixed(width),
            height,
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets::all(12.0))
        .gap(12.0)
        .background(fill, Corners::all(12.0));
    if let Some(spec) = spec {
        root = root.transition_size(spec);
    }
    Box::new(
        root.child_decl(label(
            Id::of(("coltitle", tag, page.tag())),
            title,
            14.0,
            title_ink,
            width - 24.0,
        ))
        .child(body),
    )
}

/// A section column at an explicit width, for a page whose samples need more room than `COL_W`.
pub(crate) fn column_w(
    page: Page,
    tag: &'static str,
    title: Msg,
    body: Box<dyn Element>,
    width: f32,
    height: f32,
    cx: &Cx<'_, '_>,
) -> Box<dyn Element> {
    column_text(page, tag, tr(cx, title), body, width, height)
}

pub(crate) fn captioned_row(
    id: Id,
    control: Box<dyn Element>,
    caption: impl Into<String>,
) -> Container {
    Container::new(id.with("row"), NodeKind::Column)
        .gap(2.0)
        .child_decl(label(
            id.with("cap"),
            caption.into(),
            11.0,
            muted(),
            COL_W - 24.0,
        ))
        .child(control)
}

/// A scope whose `disabled` flag is on, for static disabled samples.
pub(crate) fn disabled_env() -> std::sync::Arc<Env> {
    Env::with(
        Some(root_env()),
        ui_widgets_md3::Md3Options {
            disabled: true,
            ..Default::default()
        },
    )
}

pub(crate) const NAV_SCROLL: Id = Id::str("nav/scroll");

/// The left nav tree: a standard MD3 drawer with pages as destinations.
/// The left nav: its element plus the drawer's content height, so the app can clamp the
/// scroll offset. The element borrows `cx`'s frame only; the height is a plain number.
pub(crate) fn sidebar(page: Page, _value: f32, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    use ui_widgets_md3::{DrawerEntry, DrawerItem};

    // Translated labels are owned strings; the entries borrow from them, so they must be
    // fully built before `new_with` takes the borrows.
    let mut labels: Vec<String> = Vec::new();
    for (_, section_msg, pages) in SECTIONS {
        labels.push(tr(cx, section_msg));
        for p in pages.iter() {
            labels.push(tr(cx, p.title()));
        }
    }
    let mut entries: Vec<DrawerEntry> = Vec::new();
    let mut k = 0;
    for (_, _, pages) in SECTIONS {
        entries.push(DrawerEntry::Headline(labels[k].as_str()));
        k += 1;
        for p in pages.iter() {
            entries.push(DrawerEntry::Item(DrawerItem {
                id: p.nav_id(),
                label: labels[k].as_str(),
                icon: Some(ms(p.icon_name())),
            }));
            k += 1;
        }
    }

    // Resolve through the kit so a running theme transition blends this drawer's colours
    // too; `pack_of` alone would snap to the target scheme.
    let drawer_pack = Md3Kit::new().pack_for(cx);
    let drawer = ui_widgets_md3::Md3NavDrawer::new_with(
        ui_widgets_md3::Md3Options::default(),
        ui_widgets_md3::DrawerParams {
            id: Id::str("gallery/drawer"),
            entries: &entries,
            selected: Some(page.nav_id()),
            disabled: false,
            mode: ui_widgets_md3::DrawerMode::Standard,
        },
        cx,
        &drawer_pack,
    );
    // The drawer paints its own `surface-container`, so the scroll container only clips and
    // scrolls it; any background here would double-paint over the drawer's rounded edge.
    // The engine owns the offset (`SCROLL_OWNED`); the fill height lets the shell's cross
    // stretch size the viewport, so a window resize needs no recomputed constant.
    // The drawer stays a `Styled` element: only the element adapter reports its `change`,
    // which is how a nav row tells the app which destination was picked.
    let scoped = cx.at_scope(base_env());
    Box::new(
        Container::new(NAV_SCROLL, NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(NAV_W),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .scrollable()
            .sense(ui_core::Sense::SCROLL_OWNED)
            .child(scoped.element(Box::new(drawer))),
    )
}

/// The top app bar: the page's title plus an appearance toggle in the trailing slot.
pub(crate) fn toolbar(title: &str, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let action = ui_widgets_md3::AppBarAction {
        id: Id::str("toolbar/appearance"),
        icon: ms("dark_mode"),
    };
    // Same kit resolution as the drawer: the app bar fades through a theme transition.
    let bar_pack = Md3Kit::new().pack_for(cx);
    let bar = ui_widgets_md3::Md3AppBar::new_with(
        ui_widgets_md3::Md3Options::default(),
        ui_widgets_md3::AppBarParams {
            id: Id::str("toolbar"),
            variant: ui_widgets_md3::AppBarVariant::Small,
            title,
            subtitle: None,
            leading: ui_widgets_md3::AppBarLeading::None,
            actions: std::slice::from_ref(&action),
            width: CONTENT_W,
        },
        false,
        false,
        cx,
        &bar_pack,
    );
    cx.element(Box::new(bar))
}

/// Map the gallery's `dark` flag to a design appearance.
pub(crate) fn appearance_of(dark: bool) -> ui_design::Appearance {
    if dark {
        ui_design::Appearance::Dark
    } else {
        ui_design::Appearance::Light
    }
}

// The host seam: the one place the gallery names its platform.
//
// A target's binaries can only name the backends that compile there, so the surface type is
// an alias per target while everything else the gallery asks of its host is a capability
// question answered here — a platform that cannot do something says so, and the report
// leaves the number out instead of printing a zero.

#[cfg(unix)]
pub(crate) use ui_platform_wayland::{Session, SessionOptions};
#[cfg(windows)]
pub(crate) use ui_platform_win32::{Session, SessionOptions};

pub(crate) use host::*;

#[cfg(unix)]
mod host {
    use super::Session;
    use crate::app::Gallery;

    /// XDG portal, so the desktop's colour scheme reaches the app.
    pub(crate) type ThemeWatch = ui_platform_wayland::PortalThemeWatcher;

    /// Frames drawn wholly and partly, for the exit line.
    pub(crate) fn partial_frames() -> (u64, u64) {
        ui_platform_wayland::partial_frames()
    }

    pub(crate) fn set_supersample(session: &mut Session<Gallery>, factor: u32) {
        if let Some(gpu) = session.gpu_mut() {
            gpu.renderer_mut().set_supersample(factor);
        }
    }

    pub(crate) fn gpu_present(session: &mut Session<Gallery>) -> bool {
        session.gpu_mut().is_some()
    }

    pub(crate) fn frame_callbacks(session: &mut Session<Gallery>) -> u32 {
        session.window_mut().take_frame_callbacks()
    }

    pub(crate) fn refresh_hz(session: &Session<Gallery>) -> Option<f32> {
        session.window().refresh_hz()
    }

    pub(crate) fn dmabuf_stats(session: &Session<Gallery>) -> Option<(usize, u32)> {
        Some((
            session.window().dmabuf_inflight(),
            session.window().dmabuf_releases(),
        ))
    }

    /// This process's CPU time, from `/proc`.
    pub(crate) fn cpu_seconds() -> Option<f64> {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let fields: Vec<&str> = stat.split_whitespace().collect();
        let ticks: f64 = fields.get(13).and_then(|s| s.parse().ok())?
            + fields.get(14).and_then(|s| s.parse().ok())?;
        Some(ticks / 100.0)
    }

    /// Write the renderer's frame and its top layer; the path is the one printed.
    pub(crate) fn dump_frame_now(session: &mut Session<Gallery>) -> Option<std::path::PathBuf> {
        let frame = std::path::Path::new("/tmp/gallery_frame.ppm");
        let layer = std::path::Path::new("/tmp/gallery_layer.ppm");
        let gpu = session.gpu_mut()?;
        let _ = gpu.dump_ppm(frame, layer);
        Some(frame.to_path_buf())
    }

    pub(crate) fn finish(session: &mut Session<Gallery>) {
        if let Some(gpu) = session.gpu_mut() {
            gpu.finish();
        }
    }
}

#[cfg(windows)]
mod host {
    use super::Session;
    use crate::app::Gallery;

    /// The registry value Windows keeps the app colour scheme in.
    pub(crate) type ThemeWatch = ui_platform_win32::Win32ThemeWatcher;

    pub(crate) fn partial_frames() -> (u64, u64) {
        ui_gpu_scene::partial_frames()
    }

    pub(crate) fn set_supersample(session: &mut Session<Gallery>, factor: u32) {
        session.renderer_mut().set_supersample(factor);
    }

    /// There is no CPU presenter: every frame reaches the screen through D3D11.
    pub(crate) fn gpu_present(_: &mut Session<Gallery>) -> bool {
        true
    }

    pub(crate) fn frame_callbacks(session: &mut Session<Gallery>) -> u32 {
        session.take_frame_callbacks()
    }

    pub(crate) fn refresh_hz(session: &Session<Gallery>) -> Option<f32> {
        session.window().refresh_hz()
    }

    /// A flip-model swap chain holds no dma-buf.
    pub(crate) fn dmabuf_stats(_: &Session<Gallery>) -> Option<(usize, u32)> {
        None
    }

    /// CPU time would need process counters this host does not read yet.
    pub(crate) fn cpu_seconds() -> Option<f64> {
        None
    }

    /// Write the last composited frame; the path is the one printed.
    pub(crate) fn dump_frame_now(session: &mut Session<Gallery>) -> Option<std::path::PathBuf> {
        let path = std::env::temp_dir().join("gallery_frame.ppm");
        session.dump_frame_ppm(&path).ok()?;
        Some(path)
    }

    pub(crate) fn finish(session: &mut Session<Gallery>) {
        session.finish();
    }
}
