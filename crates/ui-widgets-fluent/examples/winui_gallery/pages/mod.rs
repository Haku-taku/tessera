//! The gallery's pages: one function per control, each a heading, its description and a few
//! samples with captions.

use ui_core::change::Change;
use ui_core::layout::{Anchor, AxisSize, Grow};
use ui_core::{Decl, Id, NodeKind, SizeClass};
use ui_platform::{Container, Element, from_decl};
use ui_style::StyleScope;
use ui_widgets::Cx;
use ui_widgets_fluent::{
    ButtonParams, ButtonVariant, CheckBoxParams, CheckState, ComboBoxItem, ComboBoxParams,
    ContentDialogButton, ContentDialogParams, FluentSlider, FluentToggleSwitch, InfoBarParams,
    InfoBarSeverity, MenuFlyoutEntry, MenuFlyoutItem, MenuFlyoutParams, NavigationItem,
    NavigationViewParams, PaneDisplayMode, ProgressBarParams, ProgressRingParams, ProgressStatus,
    RadioParams, TabViewChange, TabViewItemParams, TabViewParams, TextBoxParams, ToolTipParams,
};

use crate::app::{Page, WinUiGallery, ink, ink_of, label};

/// A page's whole body: its heading, its description and its sections.
pub(crate) fn page_body(app: &WinUiGallery, page: Page, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut col = Container::new(Id::str("page-body"), NodeKind::Column)
        .padding(ui_core::geom::Insets::all(32.0))
        .gap(18.0)
        .child_decl(label(
            Id::str("page-title"),
            page.title(),
            28.0,
            ink(true),
            860.0,
        ))
        .child_decl(label(
            Id::str("page-desc"),
            page.description(),
            14.0,
            ink(false),
            860.0,
        ));
    for block in sections(app, page, cx) {
        col = col.child(block);
    }
    Box::new(col)
}

fn sections(app: &WinUiGallery, page: Page, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    match page {
        Page::Home => home(app, cx),
        Page::Button => button_page(app, cx),
        Page::CheckBox => checkbox_page(app, cx),
        Page::RadioButton => radio_page(app, cx),
        Page::ComboBox => combo_page(app, cx),
        Page::Slider => slider_page(app, cx),
        Page::ToggleSwitch => toggle_page(app, cx),
        Page::TextBox => textbox_page(app, cx),
        Page::InfoBar => infobar_page(app, cx),
        Page::ProgressBar => progress_bar_page(app, cx),
        Page::ProgressRing => progress_ring_page(app, cx),
        Page::ToolTip => tooltip_page(app, cx),
        Page::ContentDialog => dialog_page(app, cx),
        Page::MenuFlyout => menu_page(app, cx),
        Page::NavigationView => navigation_page(app, cx),
        Page::TabView => tab_page(app, cx),
    }
}

/// A section: its title, then the samples under it.
fn section(id: Id, title: &str, children: Vec<Box<dyn Element>>) -> Box<dyn Element> {
    let mut block = Container::new(id.with("section"), NodeKind::Column)
        .gap(10.0)
        .child_decl(label(id.with("title"), title, 18.0, ink(true), 860.0));
    for child in children {
        block = block.child(child);
    }
    Box::new(block)
}

/// A sample card: the sample on its own surface, the caption under it.
fn sample(id: Id, caption: &str, body: Box<dyn Element>) -> Box<dyn Element> {
    Box::new(
        Container::new(id.with("card"), NodeKind::Column)
            .padding(ui_core::geom::Insets::all(16.0))
            .gap(10.0)
            .cross_align(0.0)
            .background(
                ink_of("CardBackgroundFillColorDefaultBrush")
                    .unwrap_or(ui_core::geom::Color::TRANSPARENT),
                ui_core::geom::Corners::all(8.0),
            )
            .border(
                ink_of("CardStrokeColorDefaultBrush").unwrap_or(ui_core::geom::Color::TRANSPARENT),
                1.0,
            )
            .child(body)
            .child_decl(label(id.with("caption"), caption, 12.0, ink(false), 760.0)),
    )
}

fn row(id: &'static str, gap: f32) -> Container {
    Container::new(Id::str(id), NodeKind::Row).gap(gap)
}

fn column(id: &'static str, gap: f32) -> Container {
    // Fluent samples align to the start; a stretched column would widen every control to
    // the card, which is not what the reference gallery shows.
    Container::new(Id::str(id), NodeKind::Column)
        .gap(gap)
        .cross_align(0.0)
}

/// A column that holds its child at `w`, for the fields WinUI's gallery sizes explicitly.
fn field(id: Id, w: f32, body: Box<dyn Element>) -> Box<dyn Element> {
    Box::new(
        Container::new(id, NodeKind::Column)
            .size(SizeClass {
                width: ui_core::layout::AxisSize::fixed(w),
                height: ui_core::layout::AxisSize::hug(0.0, 240.0),
                grow_x: ui_core::layout::Grow::No,
                grow_y: ui_core::layout::Grow::No,
            })
            .child(body),
    )
}

fn pack(cx: &Cx<'_, '_>) -> &'static ui_style::Style {
    ui_widgets_fluent::pack_of(StyleScope::of(cx.env()).appearance)
}

fn home(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let mut links = row("home/links", 10.0);
    for (k, page) in [Page::Button, Page::NavigationView, Page::ContentDialog]
        .into_iter()
        .enumerate()
    {
        let params = ButtonParams::new(Id::of(("home-link", k)), page.title())
            .variant(ButtonVariant::Accent);
        let button = app.kit().button(params, cx);
        links = links.child(cx.element(button));
    }
    vec![section(
        Id::str("home/start"),
        "Start here",
        vec![sample(
            Id::str("home/sample"),
            "Every page follows Microsoft's gallery: the same categories and control names, \
             with a live sample of each control this engine draws.",
            Box::new(links),
        )],
    )]
}

fn button_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let mut variants = row("btn/variants", 8.0);
    for (k, (text, variant)) in [
        ("Default", ButtonVariant::Default),
        ("Accent", ButtonVariant::Accent),
        ("Subtle", ButtonVariant::Subtle),
        ("Text link", ButtonVariant::TextLink),
    ]
    .into_iter()
    .enumerate()
    {
        let params = ButtonParams::new(Id::of(("btn", k)), text).variant(variant);
        let button = app.kit().button(params, cx);
        variants = variants.child(cx.element(button));
    }
    let mut states = row("btn/states", 8.0);
    for (k, (text, variant)) in [
        ("Disabled", ButtonVariant::Default),
        ("Accent, disabled", ButtonVariant::Accent),
    ]
    .into_iter()
    .enumerate()
    {
        let params = ButtonParams::new(Id::of(("btn-disabled", k)), text)
            .variant(variant)
            .disabled();
        let button = app.kit().button(params, cx);
        states = states.child(cx.element(button));
    }
    vec![
        section(
            Id::str("btn/styles"),
            "A button with text",
            vec![sample(
                Id::str("btn/styles-sample"),
                "The four styles are the ones the WinUI templates define.",
                Box::new(variants),
            )],
        ),
        section(
            Id::str("btn/states"),
            "States",
            vec![sample(
                Id::str("btn/states-sample"),
                "A disabled button keeps its layout and loses its states.",
                Box::new(states),
            )],
        ),
    ]
}

fn checkbox_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let mut boxes = column("check/boxes", 8.0);
    for (k, text) in ["Checked", "Unchecked", "Indeterminate"]
        .into_iter()
        .enumerate()
    {
        let params = CheckBoxParams::new(Id::of(("check", k)), text, app.check[k]);
        let control = app.kit().checkbox(params, cx);
        boxes = boxes.child(cx.element(control));
    }
    let params = CheckBoxParams::new(
        Id::of(("check", "disabled")),
        "Disabled, checked",
        CheckState::Checked,
    )
    .disabled();
    let control = app.kit().checkbox(params, cx);
    let mut states = column("check/states", 8.0).child(cx.element(control));
    states = states.gap(8.0);
    vec![
        section(
            Id::str("check/three"),
            "Three states",
            vec![sample(
                Id::str("check/three-sample"),
                "A click settles a box to checked or unchecked; the indeterminate value is \
                 reached with IsThreeState.",
                Box::new(boxes),
            )],
        ),
        section(
            Id::str("check/states"),
            "States",
            vec![sample(
                Id::str("check/states-sample"),
                "A disabled box keeps its value and stops answering.",
                Box::new(states),
            )],
        ),
    ]
}

fn radio_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let mut group = column("radio/group", 8.0);
    for (k, text) in ["Option one", "Option two", "Option three"]
        .into_iter()
        .enumerate()
    {
        let params = RadioParams::new(Id::of(("radio", k)), text, k == app.radio);
        let control = app.kit().radio(params, cx);
        group = group.child(cx.element(control));
    }
    vec![section(
        Id::str("radio/sample"),
        "A group of radio buttons",
        vec![sample(
            Id::str("radio/sample-card"),
            "Exactly one option in a group is selected; the app owns which.",
            Box::new(group),
        )],
    )]
}

fn combo_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let items: Vec<ComboBoxItem> = ["Blue", "Green", "Red", "Yellow"]
        .into_iter()
        .enumerate()
        .map(|(k, text)| ComboBoxItem::new(Id::of(("combo", k)), text))
        .collect();
    let params = ComboBoxParams::new(Id::str("combo"), &items)
        .selected(app.combo)
        .open(app.combo_open)
        .placeholder("Pick a colour")
        .header("Colour");
    let control = app.kit().combo_box(params, cx);
    let control = field(Id::str("combo/box"), 320.0, cx.element(control));
    vec![section(
        Id::str("combo/sample"),
        "A drop-down list",
        vec![sample(
            Id::str("combo/sample-card"),
            "The field shows the selected item, or its placeholder while nothing is picked.",
            control,
        )],
    )]
}

fn slider_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let style = pack(cx);
    let id = Id::str("slider");
    let slider = FluentSlider::new(id, app.slider, "Volume", &mut cx.at(id), &style);
    // The slider must stay a widget: a declaration alone carries no value back.
    let control = cx.element(Box::new(slider));
    let caption = format!(
        "The app owns the value; this one reads {:.0}%.",
        app.slider * 100.0
    );
    vec![section(
        Id::str("slider/sample"),
        "A range of values",
        vec![sample(Id::str("slider/sample-card"), &caption, control)],
    )]
}

fn toggle_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let style = pack(cx);
    let mut col = column("toggle/group", 12.0);
    for (k, text) in ["Wi-Fi", "Bluetooth"].into_iter().enumerate() {
        let id = Id::of(("toggle", k));
        let control = FluentToggleSwitch::new(id, app.toggles[k], text, &mut cx.at(id), &style);
        col = col.child(cx.element(Box::new(control)));
    }
    vec![section(
        Id::str("toggle/sample"),
        "On and off",
        vec![sample(
            Id::str("toggle/sample-card"),
            "A click flips the switch; the app decides whether that sticks.",
            Box::new(col),
        )],
    )]
}

fn textbox_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let mut col = column("text/group", 12.0);
    let single = TextBoxParams::new(Id::str("text/single"), app.text.as_str())
        .placeholder("Enter your name")
        .header("Name");
    let control = app.kit().text_box(single, cx);
    col = col.child(field(
        Id::str("text/single-box"),
        320.0,
        cx.element(control),
    ));
    let empty = TextBoxParams::new(Id::str("text/empty"), app.notes.as_str()).placeholder("Notes");
    let control = app.kit().text_box(empty, cx);
    col = col.child(field(Id::str("text/empty-box"), 320.0, cx.element(control)));
    // The echo is the proof the buffer reached the app, not just the field's own copy.
    col = col.child_decl(label(
        Id::str("text/echo"),
        format!("App state: name={:?}, notes={:?}", app.text, app.notes),
        12.0,
        ink(false),
        560.0,
    ));
    vec![section(
        Id::str("text/sample"),
        "Text input",
        vec![sample(
            Id::str("text/sample-card"),
            "Each field reports the whole buffer after every edit; the app owns both buffers.",
            Box::new(col),
        )],
    )]
}

fn infobar_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    // Same reason as the progress bars: a fill-width bar needs a width to fill.
    let mut col = column("info/group", 10.0).cross_align(1.0).size(SizeClass {
        width: AxisSize::fixed(600.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    });
    let severities = [
        (
            InfoBarSeverity::Informational,
            "Informational",
            "This is an informational message.",
        ),
        (
            InfoBarSeverity::Success,
            "Success",
            "The operation completed.",
        ),
        (
            InfoBarSeverity::Warning,
            "Warning",
            "Something may need attention.",
        ),
        (InfoBarSeverity::Error, "Error", "The operation failed."),
    ];
    for (k, (severity, title, message)) in severities.into_iter().enumerate() {
        let params = InfoBarParams::new(Id::of(("info", k)), title, message)
            .severity(severity)
            .open(app.info_open[k]);
        let bar = app.kit().info_bar(params, cx);
        col = col.child(cx.element(bar));
    }
    // Closing is the app's decision, so reopening is too; the second button raises a bar
    // from the shell, above every page.
    let mut actions = row("info/actions", 8.0);
    let reset = app
        .kit()
        .button(ButtonParams::new(Id::str("info/reset"), "Reset"), cx);
    actions = actions.child(cx.element(reset));
    let show = app.kit().button(
        ButtonParams::new(Id::str("app-info/show"), "Show app-level InfoBar")
            .variant(ButtonVariant::Accent),
        cx,
    );
    actions = actions.child(cx.element(show));
    col = col.child(Box::new(actions));
    vec![section(
        Id::str("info/sample"),
        "Four severities",
        vec![sample(
            Id::str("info/sample-card"),
            "Each bar carries a severity, a message and a close button. Reset brings the \
             closed ones back; the accent button raises one from the app shell.",
            Box::new(col),
        )],
    )]
}

fn progress_bar_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    // A fill-width bar inside a hugging column would measure zero, so the sample pins the
    // width it is drawn against and stretches its children into it.
    let mut col = column("bar/group", 14.0).cross_align(1.0).size(SizeClass {
        width: AxisSize::fixed(600.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::No,
    });
    for params in [
        ProgressBarParams::new(Id::of(("bar", 0)), 0.6),
        ProgressBarParams::new(Id::of(("bar", 1)), 0.0).indeterminate(true),
        ProgressBarParams::new(Id::of(("bar", 2)), 0.4).status(ProgressStatus::Paused),
        ProgressBarParams::new(Id::of(("bar", 3)), 0.4).status(ProgressStatus::Error),
    ] {
        let bar = app.kit().progress_bar(params, cx);
        col = col.child(cx.element(bar));
    }
    vec![section(
        Id::str("bar/sample"),
        "Determinate, indeterminate and the two statuses",
        vec![sample(
            Id::str("bar/sample-card"),
            "A determinate bar fills to its value; the statuses recolour it and stop it.",
            Box::new(col),
        )],
    )]
}

fn progress_ring_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let mut r = row("ring/group", 24.0);
    for params in [
        ProgressRingParams::new(Id::of(("ring", 0)), 0.65),
        ProgressRingParams::new(Id::of(("ring", 1)), 0.0).indeterminate(true),
    ] {
        let ring = app.kit().progress_ring(params, cx);
        r = r.child(cx.element(ring));
    }
    vec![section(
        Id::str("ring/sample"),
        "Determinate and indeterminate",
        vec![sample(
            Id::str("ring/sample-card"),
            "The ring draws its progress on a circle; indeterminate sweeps on its own clock.",
            Box::new(r),
        )],
    )]
}

fn tooltip_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let params = ButtonParams::new(Id::str("tip/anchor"), "Hover me");
    let anchor = app.kit().button(params, cx);
    let anchor = cx.element(anchor);
    // A tooltip shows while its target is hovered, as it does in WinUI.
    let hovered = cx.state(Id::str("tip/anchor")).hovered;
    let tip = ToolTipParams::new(
        Id::str("tip"),
        "This is a tooltip.",
        Anchor::Node(Id::str("tip/anchor")),
    )
    .open(hovered);
    let tip = app.kit().tool_tip(tip, cx);
    let tip = cx.element(tip);
    let col = column("tip/group", 28.0).child(anchor).child(tip);
    vec![section(
        Id::str("tip/sample"),
        "A tooltip above its target",
        vec![sample(
            Id::str("tip/sample-card"),
            "The panel is anchored to the control and flips when it would not fit.",
            Box::new(col),
        )],
    )]
}

fn dialog_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let params =
        ButtonParams::new(Id::str("dialog/show"), "Show dialog").variant(ButtonVariant::Accent);
    let button = app.kit().button(params, cx);
    let col = column("dialog/group", 12.0).child(cx.element(button));
    vec![section(
        Id::str("dialog/sample"),
        "A content dialog",
        vec![sample(
            Id::str("dialog/sample-card"),
            "The dialog dims and blocks what is behind it; the app owns IsOpen and answers \
             with one of its buttons.",
            Box::new(col),
        )],
    )]
}

/// The content dialog, which floats over the window rather than inside the page.
pub(crate) fn dialog(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let params = ContentDialogParams::new(
        Id::str("dialog"),
        "Delete this file?",
        "The file will be removed from this device.",
    )
    .primary("Delete")
    .close("Cancel")
    .default_button(ContentDialogButton::Close);
    let control = app.kit().content_dialog(params, cx);
    cx.element(control)
}

fn menu_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let params = ButtonParams::new(Id::str("menu/anchor"), "Open menu");
    let anchor = app.kit().button(params, cx);
    let anchor = cx.element(anchor);
    let items = [
        MenuFlyoutEntry::Item(MenuFlyoutItem::new(Id::str("menu/copy"), "Copy")),
        MenuFlyoutEntry::Item(MenuFlyoutItem::new(Id::str("menu/paste"), "Paste")),
        MenuFlyoutEntry::Separator,
        MenuFlyoutEntry::Item(MenuFlyoutItem::new(Id::str("menu/delete"), "Delete")),
    ];
    let flyout = MenuFlyoutParams::new(
        Id::str("menu"),
        Anchor::Node(Id::str("menu/anchor")),
        &items,
    )
    .open(app.menu_open);
    let flyout = app.kit().menu_flyout(flyout, cx);
    let flyout = cx.element(flyout);
    let col = column("menu/group", 12.0).child(anchor).child(flyout);
    vec![section(
        Id::str("menu/sample"),
        "A menu flyout",
        vec![sample(
            Id::str("menu/sample-card"),
            "A press on the button opens the menu under it; a press outside closes it.",
            Box::new(col),
        )],
    )]
}

fn navigation_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let labels = ["Documents", "Search", "Saved"];
    let items: Vec<NavigationItem> = labels
        .iter()
        .enumerate()
        .map(|(k, text)| NavigationItem::new(Id::of(("demo-nav", k)), *text, "folder_16_regular"))
        .collect();
    let content = vec![from_decl(
        Decl::container(Id::str("demo-nav/page"), NodeKind::Column)
            .padding(ui_core::geom::Insets::all(16.0))
            .child(label(
                Id::str("demo-nav/heading"),
                labels[app.demo_nav.min(labels.len() - 1)],
                18.0,
                ink(true),
                420.0,
            )),
    )];
    let modes = [
        PaneDisplayMode::Expanded,
        PaneDisplayMode::Compact,
        PaneDisplayMode::Minimal,
    ];
    let params = NavigationViewParams::new(Id::str("demo-nav"), &items)
        .selected(app.demo_nav)
        .open(app.demo_nav_open)
        .display_mode(modes[app.demo_mode.min(modes.len() - 1)])
        .content(content);
    let view = app.kit().navigation_view(params, cx);
    let view = Container::new(Id::str("demo-nav/box"), NodeKind::Column)
        .size(SizeClass::fixed(680.0, 380.0))
        .child(view);
    let mut mode_row = row("demo-nav/modes", 8.0);
    for (k, text) in ["Expanded", "Compact", "Minimal"].into_iter().enumerate() {
        let variant = if k == app.demo_mode {
            ButtonVariant::Accent
        } else {
            ButtonVariant::Default
        };
        let params = ButtonParams::new(Id::of(("demo-mode", k)), text).variant(variant);
        let button = app.kit().button(params, cx);
        mode_row = mode_row.child(cx.element(button));
    }
    vec![
        section(
            Id::str("demo-nav/sample"),
            "A navigation view",
            vec![sample(
                Id::str("demo-nav/sample-card"),
                "The pane holds destinations; the content frame shows the one selected.",
                Box::new(view),
            )],
        ),
        section(
            Id::str("demo-nav/modes-section"),
            "Pane display modes",
            vec![sample(
                Id::str("demo-nav/modes-card"),
                "PaneDisplayMode decides how much of the pane stays on screen when it is closed.",
                Box::new(mode_row),
            )],
        ),
    ]
}

fn tab_page(app: &WinUiGallery, cx: &mut Cx<'_, '_>) -> Vec<Box<dyn Element>> {
    let tabs: Vec<TabViewItemParams> = app
        .tabs
        .iter()
        .map(|entry| TabViewItemParams::new(entry.id, entry.label.as_str()).closable(true))
        .collect();
    let selected = app.tab.min(app.tabs.len().saturating_sub(1));
    let params = TabViewParams::new(Id::str("tabview"), tabs)
        .selected(selected)
        .add_button(true);
    let strip = app.kit().tab_view(params, cx);
    // The strip is the control; the page under it belongs to the app, as in WinUI.
    let heading = app
        .tabs
        .get(selected)
        .map_or("No tabs", |e| e.label.as_str());
    let body = Decl::container(Id::str("tabview/body"), NodeKind::Column)
        .padding(ui_core::geom::Insets::all(16.0))
        .child(label(
            Id::str("tabview/heading"),
            heading,
            18.0,
            ink(true),
            420.0,
        ));
    let view = Container::new(Id::str("tabview/box"), NodeKind::Column)
        .size(SizeClass::fixed(680.0, 260.0))
        .gap(12.0)
        .child(strip)
        .child_decl(body);
    vec![section(
        Id::str("tabview/sample"),
        "A set of tabs",
        vec![sample(
            Id::str("tabview/sample-card"),
            "Add, close and drag the headers: the app owns the list, the order and which one \
             is selected.",
            Box::new(view),
        )],
    )]
}

/// Apply a control's change to the app. `false` means the change was not ours.
pub(crate) fn changed(app: &mut WinUiGallery, id: Id, change: Change) -> bool {
    match change {
        Change::Toggled { on } => {
            if let Some(k) = index_of(&id, "check", app.check.len()) {
                app.check[k] = match on {
                    Some(true) => CheckState::Checked,
                    Some(false) => CheckState::Unchecked,
                    None => CheckState::Indeterminate,
                };
                return true;
            }
            if let Some(k) = index_of(&id, "toggle", app.toggles.len()) {
                app.toggles[k] = on.unwrap_or(false);
                return true;
            }
            if id == Id::str("demo-nav") {
                app.demo_nav_open = on.unwrap_or(true);
                return true;
            }
            if id == Id::str("dialog") {
                app.dialog_open = on.unwrap_or(false);
                return true;
            }
        }
        Change::Selected(Some(target)) => {
            if let Some(k) = app.tabs.iter().position(|entry| entry.id == target) {
                app.tab = k;
                return true;
            }
            if let Some(k) = index_of(&target, "combo", 4) {
                app.combo = Some(k);
                app.combo_open = false;
                return true;
            }
            if let Some(k) = index_of(&target, "demo-nav", 3) {
                app.demo_nav = k;
                return true;
            }
            if id == Id::str("dialog") {
                app.dialog_open = false;
                return true;
            }
            if id == Id::str("menu") {
                app.menu_open = false;
                return true;
            }
        }
        Change::Dismissed => {
            if id == Id::str("combo") {
                app.combo_open = false;
                return true;
            }
            if id == Id::str("menu") {
                app.menu_open = false;
                return true;
            }
        }
        Change::Value(v) => {
            if id == Id::str("slider") {
                app.slider = v as f32;
                return true;
            }
        }
        Change::Text(text) => {
            if id == Id::str("text/single") {
                app.text = text;
                return true;
            }
            if id == Id::str("text/empty") {
                app.notes = text;
                return true;
            }
        }
        Change::Activated => {
            if let Some(k) = index_of(&id, "radio", 3) {
                app.radio = k;
                return true;
            }
            if id == Id::str("combo") {
                app.combo_open = true;
                return true;
            }
            if let Some(k) = index_of(&id, "info", 4) {
                app.info_open[k] = false;
                return true;
            }
            if id == Id::str("info/reset") {
                app.info_open = [true; 4];
                return true;
            }
            if id == Id::str("app-info/show") {
                app.app_info_open = true;
                return true;
            }
            if id == Id::str("app-info") {
                app.app_info_open = false;
                return true;
            }
            if id == Id::str("tabview").with("add") {
                let serial = app.next_tab;
                app.next_tab += 1;
                app.tabs.push(crate::app::TabEntry {
                    id: Id::of(("tabitem", serial)),
                    label: format!("New tab {}", serial - 2),
                });
                app.tab = app.tabs.len() - 1;
                return true;
            }
            if let Some(k) = index_of(&id, "home-link", 3) {
                app.page = [Page::Button, Page::NavigationView, Page::ContentDialog][k];
                return true;
            }
            if let Some(k) = index_of(&id, "demo-mode", 3) {
                app.demo_mode = k;
                return true;
            }
            if id == Id::str("dialog/show") {
                app.dialog_open = true;
                return true;
            }
            if id == Id::str("menu/anchor") {
                app.menu_open = !app.menu_open;
                return true;
            }
        }
        Change::Data(payload) => {
            let Some(action) = payload.downcast_ref::<TabViewChange>() else {
                return false;
            };
            match *action {
                // A dragged header takes the slot it was released into; the app owns order.
                // The slot may name the gap after the last header, so `to == count` is legal.
                TabViewChange::Dropped { from, to } => {
                    let count = app.tabs.len();
                    if from >= count || to > count {
                        return false;
                    }
                    let selected = app.tabs.get(app.tab).map(|entry| entry.id);
                    let moved = app.tabs.remove(from);
                    let at = to.min(app.tabs.len());
                    app.tabs.insert(at, moved);
                    if let Some(sel) = selected
                        && let Some(k) = app.tabs.iter().position(|entry| entry.id == sel)
                    {
                        app.tab = k;
                    }
                    return true;
                }
                TabViewChange::Closed => {
                    let Some(k) = app.tabs.iter().position(|entry| entry.id == id) else {
                        return false;
                    };
                    app.tabs.remove(k);
                    if app.tabs.is_empty() {
                        app.tab = 0;
                    } else if k < app.tab {
                        app.tab -= 1;
                    } else if app.tab >= app.tabs.len() {
                        app.tab = app.tabs.len() - 1;
                    }
                    return true;
                }
            }
        }
        _ => {}
    }
    false
}

/// The index a tuple id like `("toggle", 2)` carries, out of `n` candidates.
fn index_of(id: &Id, tag: &'static str, n: usize) -> Option<usize> {
    (0..n).find(|k| Id::of((tag, *k)) == *id)
}
