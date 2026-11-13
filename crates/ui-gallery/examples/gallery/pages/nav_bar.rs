// The navigation bar page.

use ui_widgets::IconSource;
use ui_widgets_glass::{GlassMorph, GlassTab, GlassTabBar, HigOptions};

use super::glyphs::*;
use crate::shell::*;

/// Destinations per bar; which one is selected belongs to the app.
pub(crate) const NAV_ITEMS: usize = 3;

pub(crate) fn nav_item_id(tag: &'static str, k: usize) -> Id {
    Id::of(("navitem", tag, k))
}

pub(crate) fn nav_bar_id(tag: &'static str) -> Id {
    Id::of(("navbar", tag))
}

/// The destination index an id names, across the bar and rail samples.
pub(crate) fn nav_pick(id: Id) -> Option<usize> {
    ["always", "selected", "never", "all", "none", "glass"]
        .into_iter()
        .find_map(|tag| (0..NAV_ITEMS).find(|k| nav_item_id(tag, *k) == id))
}

/// MD3 navigation bar: the three label behaviours, one shared selection.
pub(crate) fn nav_bar_page(selected: usize, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    use ui_widgets_md3::NavLabelBehavior as B;
    let row = Container::new(Id::of(("bodyrow", Page::NavBar.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let labels = [
        tr(cx, Msg::GALLERY_NAV_HOME),
        tr(cx, Msg::GALLERY_NAV_SEARCH),
        tr(cx, Msg::GALLERY_NAV_PROFILE),
    ];
    let icons: [IconSource; NAV_ITEMS] = [home_glyph(), search_glyph(), profile_glyph()];
    // iOS draws its own symbols here, so the glass bar gets the SF counterparts.
    let glass_icons: [IconSource; NAV_ITEMS] = [
        sf_glyph("house"),
        sf_glyph("magnifyingglass"),
        sf_glyph("person"),
    ];
    // Built before the MD3 scope opens: it needs `cx` mutably on its own.
    let glass_col = glass_tab_bar_column(&labels, &glass_icons, selected, cx);
    let md3_col = {
        let mut scoped = cx.at_scope(style_env("md3"));
        let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(16.0);
        for (tag, behavior, kind) in [
            ("always", B::AlwaysShow, "all"),
            ("selected", B::OnlyShowSelected, "selected"),
            ("never", B::AlwaysHide, "none"),
        ] {
            let items: Vec<ui_widgets_md3::Md3NavItem> = (0..NAV_ITEMS)
                .map(|k| ui_widgets_md3::Md3NavItem {
                    id: nav_item_id(tag, k),
                    label: labels[k].as_str(),
                    icon: icons[k],
                })
                .collect();
            let control = style.nav_bar(
                ui_widgets_md3::NavBarParams {
                    id: nav_bar_id(tag),
                    items: &items,
                    selected,
                    label_behavior: behavior,
                    disabled: false,
                },
                &mut scoped,
            );
            let caption = tf(&scoped, Msg::GALLERY_NAV_CAPTION, &[("kind", arg(kind))]);
            col = col.child(Box::new(captioned_row(
                Id::of(("navcap", tag)),
                scoped.element(control),
                caption,
            )));
        }
        col = col.child_decl(label(
            Id::str("navbar/note"),
            tr(&scoped, Msg::GALLERY_NAV_BAR_NOTE),
            11.0,
            muted(),
            COL_W - 24.0,
        ));
        // The glass bar leads: the window fits two columns, and this page's MD3 card is the taller one.
        column(
            Page::NavBar,
            "md3",
            Msg::GALLERY_STYLE_MD3,
            Box::new(col),
            COL_MID_H,
            &scoped,
        )
    };
    let mut row = row;
    row = row.child(glass_col);
    row = row.child(md3_col);
    Box::new(row.child(pending_column(Page::NavBar, "fluent", COL_W, COL_MID_H, cx)))
}

/// The glass tab bar: the iOS 26 capsule with its draggable lens.
fn glass_tab_bar_column(
    labels: &[String],
    icons: &[IconSource; NAV_ITEMS],
    selected: usize,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let width = COL_W - 40.0;
    let mut scoped = cx.at_scope(style_env("glass"));
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let tabs: Vec<GlassTab> = (0..NAV_ITEMS)
        .map(|k| GlassTab {
            id: nav_item_id("glass", k),
            label: labels[k].clone(),
            icon: icons[k],
        })
        .collect();
    let id = nav_bar_id("glass");
    let bar = {
        let mut node = scoped.at(id);
        GlassTabBar::new_with(
            HigOptions::default(),
            id,
            tabs,
            selected,
            width,
            &mut node,
            &pack,
        )
    };
    let caption = tf(&scoped, Msg::GALLERY_NAV_CAPTION, &[("kind", arg("glass"))]);
    // Below the bar, the same engine primitive does a liquid morph: press the blob and the
    // droplet tears off along the reference's back-out curve, joined by a metaball neck.
    let morph_id = Id::of(("morph", "glass"));
    let morph = {
        let mut node = scoped.at(morph_id);
        GlassMorph::new_with(
            HigOptions::default(),
            morph_id,
            (width, 96.0),
            Rect::new(12.0, 24.0, 56.0, 56.0),
            Rect::new(width - 168.0, 28.0, 156.0, 44.0),
            &mut node,
            &pack,
        )
    };
    let body = Box::new(
        Container::new(Id::of(("glassbody", "glass")), NodeKind::Column)
            .gap(12.0)
            .child(Box::new(captioned_row(
                Id::of(("navcap", "glass")),
                scoped.element(Box::new(bar)),
                caption,
            )))
            .child(scoped.element(Box::new(morph))),
    );
    column(
        Page::NavBar,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        body,
        COL_MID_H,
        &scoped,
    )
}
