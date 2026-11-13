// The navigation rail page.

use ui_widgets::IconSource;

use super::glyphs::*;
use super::nav_bar::{NAV_ITEMS, nav_item_id};
use crate::shell::*;

pub(crate) fn rail_id(tag: &'static str) -> Id {
    Id::of(("rail", tag))
}

/// The button that flips the Expressive rail between its two widths.
pub(crate) fn rail_expand_id() -> Id {
    Id::str("rail/expand")
}

/// MD3 navigation rail: the three label types side by side, one shared selection, and the
/// Expressive pair underneath.
pub(crate) fn nav_rail_page(
    selected: usize,
    expanded: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_widgets_md3::RailLabelType as L;
    // The body hugs: this page is the one with two rows of samples, so the card has to grow
    // with them instead of taking a guessed height.
    let mut row = Container::new(Id::of(("bodyrow", Page::NavRail.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::No,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let labels = [
        tr(cx, Msg::GALLERY_NAV_HOME),
        tr(cx, Msg::GALLERY_NAV_SEARCH),
        tr(cx, Msg::GALLERY_NAV_PROFILE),
    ];
    let icons: [IconSource; NAV_ITEMS] = [home_glyph(), search_glyph(), profile_glyph()];
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(16.0);

    let mut line = Container::new(Id::str("rail/line"), NodeKind::Row)
        .cross_align(0.0)
        .gap(16.0);
    for (tag, label_type, kind) in [
        ("all", L::All, "all"),
        ("selected", L::Selected, "selected"),
        ("none", L::None, "none"),
    ] {
        let items: Vec<ui_widgets_md3::Md3NavItem> = (0..NAV_ITEMS)
            .map(|k| ui_widgets_md3::Md3NavItem {
                id: nav_item_id(tag, k),
                label: labels[k].as_str(),
                icon: icons[k],
            })
            .collect();
        let control = style.nav_rail(
            ui_widgets_md3::RailParams {
                id: rail_id(tag),
                items: &items,
                selected,
                label_type,
                variant: ui_widgets_md3::RailVariant::Classic,
                disabled: false,
            },
            &mut scoped,
        );
        let caption = tf(&scoped, Msg::GALLERY_NAV_CAPTION, &[("kind", arg(kind))]);
        let mut group = Container::new(Id::of(("railgroup", tag)), NodeKind::Column)
            .cross_align(0.0)
            .gap(4.0);
        group = group.child_decl(label(
            Id::of(("railcap", tag)),
            caption,
            11.0,
            muted(),
            100.0,
        ));
        group = group.child(scoped.element(control));
        line = line.child(Box::new(group));
    }
    col = col.child(Box::new(line));
    col = col.child_decl(label(
        Id::str("rail/note"),
        tr(&scoped, Msg::GALLERY_NAV_RAIL_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));

    // The Expressive pair: one control at 96dp or 220dp, so its width has somewhere to go.
    let items: Vec<ui_widgets_md3::Md3NavItem> = (0..NAV_ITEMS)
        .map(|k| ui_widgets_md3::Md3NavItem {
            id: nav_item_id("expressive", k),
            label: labels[k].as_str(),
            icon: icons[k],
        })
        .collect();
    let control = style.nav_rail(
        ui_widgets_md3::RailParams {
            id: rail_id("expressive"),
            items: &items,
            selected,
            label_type: L::All,
            variant: ui_widgets_md3::RailVariant::Expressive { expanded },
            disabled: false,
        },
        &mut scoped,
    );
    let caption = tf(
        &scoped,
        Msg::GALLERY_NAV_CAPTION,
        &[("kind", arg("expressive"))],
    );
    let mut group = Container::new(Id::of(("railgroup", "expressive")), NodeKind::Column)
        .cross_align(0.0)
        .gap(4.0);
    group = group.child_decl(label(
        Id::of(("railcap", "expressive")),
        caption,
        11.0,
        muted(),
        100.0,
    ));
    group = group.child(scoped.element(control));
    let button = style.button(
        ui_widgets_md3::ButtonParams::new(
            rail_expand_id(),
            tr(&scoped, Msg::GALLERY_NAV_RAIL_EXPAND),
        )
        .style(ui_widgets_md3::ButtonStyle::Tonal)
        .size(ui_widgets_md3::ButtonSize::Base),
        &mut scoped,
    );
    // Under the rail, not beside it: expanded it is 220dp wide and the card is 380.
    group = group.child(scoped.element(button));
    col = col.child(Box::new(group));
    col = col.child_decl(label(
        Id::str("rail/expnote"),
        tr(&scoped, Msg::GALLERY_NAV_RAIL_EXPRESSIVE_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column_hug(
        Page::NavRail,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_W,
        &scoped,
    ));
    Box::new(add_pending_pair(row, Page::NavRail, COL_W, COL_CTRL_H, cx))
}
