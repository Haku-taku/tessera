// The Fluent navigation view page.

use ui_widgets_fluent::{
    ButtonParams, ButtonVariant, NavigationItem, NavigationViewParams, PaneDisplayMode,
};

use crate::app::Gallery;
use crate::shell::*;

pub(crate) const NAV_VIEW_ITEMS: usize = 4;

pub(crate) const NAV_VIEW_LABELS: [Msg; NAV_VIEW_ITEMS] = [
    Msg::GALLERY_NAV_VIEW_DOCUMENTS,
    Msg::GALLERY_NAV_VIEW_SEARCH,
    Msg::GALLERY_NAV_VIEW_SAVED,
    Msg::GALLERY_NAV_VIEW_PRINT,
];

/// Names the vendored icon face carries; the reference draws its own glyphs from here.
const NAV_VIEW_ICONS: [&str; NAV_VIEW_ITEMS] = [
    "folder_open_16_regular",
    "search_16_regular",
    "save_16_regular",
    "print_16_regular",
];

/// The view's height inside the demo column; its content needs a bounded box.
const NAV_H: f32 = 420.0;

/// The three `PaneDisplayMode` samples, in the order the reference lists them.
pub(crate) const NAV_MODES: [PaneDisplayMode; 3] = [
    PaneDisplayMode::Expanded,
    PaneDisplayMode::Compact,
    PaneDisplayMode::Minimal,
];

const NAV_MODE_LABELS: [Msg; 3] = [
    Msg::GALLERY_NAV_VIEW_MODE_EXPANDED,
    Msg::GALLERY_NAV_VIEW_MODE_COMPACT,
    Msg::GALLERY_NAV_VIEW_MODE_MINIMAL,
];

pub(crate) fn nav_view_id() -> Id {
    Id::of("nav-view")
}

pub(crate) fn nav_view_item_id(k: usize) -> Id {
    nav_view_id().with(("item", k))
}

pub(crate) fn nav_mode_id(k: usize) -> Id {
    nav_view_id().with(("mode", k))
}

pub(crate) fn nav_view_page(g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let labels: [String; NAV_VIEW_ITEMS] = std::array::from_fn(|k| tr(cx, NAV_VIEW_LABELS[k]));
    let items: Vec<NavigationItem> = (0..NAV_VIEW_ITEMS)
        .map(|k| NavigationItem::new(nav_view_item_id(k), labels[k].clone(), NAV_VIEW_ICONS[k]))
        .collect();

    // The page the pane navigated to: the destination's own name over a line of body text.
    // Its inks come from the Fluent pack, which follows the appearance the page is drawn in.
    let ink = ui_widgets_fluent::pack_of(ui_style::StyleScope::of(cx.env()).appearance);
    let head = label(
        Id::of(("nav-view", "heading")),
        labels[g.nav_view_sel].clone(),
        20.0,
        ink.ink("TextFillColorPrimaryBrush").unwrap_or(Color::BLACK),
        640.0,
    );
    let para = label(
        Id::of(("nav-view", "body")),
        tr(cx, Msg::GALLERY_NAV_VIEW_BODY),
        13.0,
        ink.ink("TextFillColorSecondaryBrush").unwrap_or_else(muted),
        640.0,
    );
    let content = vec![ui_platform::from_decl(
        Decl::container(Id::of(("nav-view", "page")), NodeKind::Column)
            .padding(Insets::all(24.0))
            .gap(6.0)
            .child(head)
            .child(para),
    )];

    let mut scoped = cx.at_scope(style_env("fluent"));
    // The accent marks the mode on screen; the sample's own state is the app's.
    let mut modes = Container::new(Id::of(("nav-view", "modes")), NodeKind::Row).gap(8.0);
    for (k, mode) in NAV_MODES.iter().enumerate() {
        let variant = if *mode == g.nav_mode {
            ButtonVariant::Accent
        } else {
            ButtonVariant::Default
        };
        let params =
            ButtonParams::new(nav_mode_id(k), tr(&scoped, NAV_MODE_LABELS[k])).variant(variant);
        let button = g.fluent.button(params, &mut scoped);
        modes = modes.child(scoped.element(button));
    }
    let params = NavigationViewParams::new(nav_view_id(), &items)
        .selected(g.nav_view_sel)
        .open(g.nav_open)
        .display_mode(g.nav_mode)
        .content(content);
    let control = g.fluent.navigation_view(params, &mut scoped);
    let view = Container::new(Id::of(("nav-view", "box")), NodeKind::Box).size(SizeClass {
        width: AxisSize::fill(0.0, 0.0),
        height: AxisSize::fixed(NAV_H),
        grow_x: Grow::No,
        grow_y: Grow::No,
    });
    let view = Container::new(Id::of(("nav-view", "demo")), NodeKind::Column)
        .gap(12.0)
        .child(Box::new(modes))
        .child(Box::new(view.child(control)));

    let mut row = Container::new(Id::of(("bodyrow", Page::NavView.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    row = row.child(column_w(
        Page::NavView,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(view),
        760.0,
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
