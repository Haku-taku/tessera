// The navigation drawer page.

use super::glyphs::*;
use crate::shell::*;

/// Destinations in the drawer; which one is selected belongs to the app.
pub(crate) const DRAWER_ITEMS: usize = 3;

pub(crate) fn drawer_item_id(k: usize) -> Id {
    Id::of(("draweritem", k))
}

pub(crate) fn drawer_id() -> Id {
    Id::str("drawer")
}

/// The destination index an id names.
pub(crate) fn drawer_pick(id: Id) -> Option<usize> {
    (0..DRAWER_ITEMS).find(|k| drawer_item_id(*k) == id)
}

/// The modal sample: its own ids, so both drawers can be on the page at once.
pub(crate) fn modal_drawer_id() -> Id {
    Id::str("drawer/modal")
}

pub(crate) fn modal_drawer_item_id(k: usize) -> Id {
    Id::of(("drawer/modal/item", k))
}

pub(crate) fn modal_drawer_pick(id: Id) -> Option<usize> {
    (0..DRAWER_ITEMS).find(|k| modal_drawer_item_id(*k) == id)
}

/// The button that opens the modal sample; the app owns whether it is open.
pub(crate) fn drawer_open_id() -> Id {
    Id::str("drawer/open")
}

/// MD3 navigation drawer: a headline group over three destinations.
pub(crate) fn nav_drawer_page(
    selected: usize,
    open: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::NavDrawer.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let headline = tr(cx, Msg::GALLERY_DRAWER_HEADLINE);
    let labels = [
        tr(cx, Msg::GALLERY_DRAWER_INBOX),
        tr(cx, Msg::GALLERY_DRAWER_SENT),
        tr(cx, Msg::GALLERY_DRAWER_DRAFTS),
    ];
    let icons: [IconSource; DRAWER_ITEMS] = [inbox_glyph(), sent_glyph(), drafts_glyph()];
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column)
        .cross_align(0.0)
        .gap(16.0);

    let mut entries = vec![ui_widgets_md3::DrawerEntry::Headline(headline.as_str())];
    for k in 0..DRAWER_ITEMS {
        entries.push(ui_widgets_md3::DrawerEntry::Item(
            ui_widgets_md3::DrawerItem {
                id: drawer_item_id(k),
                label: labels[k].as_str(),
                icon: Some(icons[k]),
            },
        ));
    }
    let control = style.nav_drawer(
        ui_widgets_md3::DrawerParams {
            id: drawer_id(),
            entries: &entries,
            selected: Some(drawer_item_id(selected)),
            disabled: false,
            mode: ui_widgets_md3::DrawerMode::Standard,
        },
        &mut scoped,
    );
    // The drawer fills its parent's height, so the sample gives it one.
    let mut frame = Container::new(Id::str("drawer/frame"), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::hug(0.0, 0.0),
            height: AxisSize::fixed(280.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .cross_align(1.0);
    frame = frame.child(scoped.element(control));
    col = col.child(Box::new(captioned_row(
        Id::str("drawer/sample"),
        Box::new(frame),
        tr(&scoped, Msg::GALLERY_DRAWER_CAPTION),
    )));
    col = col.child_decl(label(
        Id::str("drawer/note"),
        tr(&scoped, Msg::GALLERY_DRAWER_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    // The modal sample: a trigger, and the drawer itself only while the app has it open.
    col = col.child_decl(label(
        Id::str("drawer/modal-caption"),
        tr(&scoped, Msg::GALLERY_DRAWER_MODAL_CAPTION),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    let open_button = style.button(
        ui_widgets_md3::ButtonParams::new(drawer_open_id(), tr(&scoped, Msg::GALLERY_DRAWER_OPEN))
            .style(ui_widgets_md3::ButtonStyle::Filled),
        &mut scoped,
    );
    col = col.child(scoped.element(open_button));
    if open {
        let mut modal_entries = vec![ui_widgets_md3::DrawerEntry::Headline(headline.as_str())];
        for k in 0..DRAWER_ITEMS {
            modal_entries.push(ui_widgets_md3::DrawerEntry::Item(
                ui_widgets_md3::DrawerItem {
                    id: modal_drawer_item_id(k),
                    label: labels[k].as_str(),
                    icon: Some(icons[k]),
                },
            ));
        }
        let control = style.nav_drawer(
            ui_widgets_md3::DrawerParams {
                id: modal_drawer_id(),
                entries: &modal_entries,
                selected: Some(modal_drawer_item_id(selected)),
                disabled: false,
                mode: ui_widgets_md3::DrawerMode::Modal,
            },
            &mut scoped,
        );
        col = col.child(scoped.element(control));
    }
    row = row.child(column(
        Page::NavDrawer,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_CTRL_H,
        &scoped,
    ));
    Box::new(add_pending_pair(
        row,
        Page::NavDrawer,
        COL_W,
        COL_CTRL_H,
        cx,
    ))
}
