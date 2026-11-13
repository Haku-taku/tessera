// The menu flyout page.

use ui_core::layout::Anchor;
use ui_widgets_fluent::{MenuFlyoutEntry, MenuFlyoutItem, MenuFlyoutParams};
use ui_widgets_glass::{
    GlassButton, GlassButtonParams, GlassMenu, GlassMenuEntry, GlassMenuParams, GlassPopover,
    GlassPopoverParams, HigOptions, POPOVER_PAD, POPOVER_W,
};

use crate::app::Gallery;
use crate::pages::glyphs::sf_glyph;
use crate::shell::*;

/// The commands the sample menu lists; a divider splits the file commands from the rest.
pub(crate) const MENU_COMMANDS: usize = 5;
pub(crate) const MENU_SEPARATOR_AT: usize = 3;
/// Room the open panel unfolds into: five 32 dp rows, a divider and the presenter's padding.
pub(crate) const MENU_ROOM: f32 = 240.0;

pub(crate) const MENU_LABELS: [Msg; MENU_COMMANDS] = [
    Msg::GALLERY_MENU_NEW,
    Msg::GALLERY_MENU_OPEN,
    Msg::GALLERY_MENU_SAVE,
    Msg::GALLERY_MENU_SHARE,
    Msg::GALLERY_MENU_PRINT,
];

pub(crate) const MENU_ACCELS: [Msg; MENU_COMMANDS] = [
    Msg::GALLERY_MENU_ACCEL_NEW,
    Msg::GALLERY_MENU_ACCEL_OPEN,
    Msg::GALLERY_MENU_ACCEL_SAVE,
    Msg::GALLERY_MENU_ACCEL_SHARE,
    Msg::GALLERY_MENU_ACCEL_PRINT,
];

/// The `IconRoot` glyph each command names; the last row is the disabled sample.
const MENU_ICONS: [&str; MENU_COMMANDS] = [
    "document_add_16_regular",
    "folder_open_16_regular",
    "save_16_regular",
    "share_16_regular",
    "print_16_regular",
];

/// The SF symbol each glass row leads with, matching the Fluent column's glyphs.
const MENU_GLASS_ICONS: [&str; MENU_COMMANDS] = [
    "document",
    "folder",
    "square_and_arrow_down",
    "square_and_arrow_up",
    "printer",
];

pub(crate) fn menu_flyout_id() -> Id {
    Id::of("menu-flyout")
}

/// The MD3 menu's rows: the same five commands, with the last one disabled.
pub(crate) const MD3_MENU_ITEMS: usize = MENU_COMMANDS;

pub(crate) fn md3_menu_id() -> Id {
    Id::of("md3-menu")
}

pub(crate) fn md3_menu_item_id(j: usize) -> Id {
    md3_menu_id().with(("item", j))
}

pub(crate) fn menu_anchor_id() -> Id {
    Id::of(("menu-flyout", "anchor"))
}

pub(crate) fn menu_item_id(j: usize) -> Id {
    menu_flyout_id().with(("item", j))
}

pub(crate) fn glass_menu_id() -> Id {
    Id::of("glass-menu")
}

pub(crate) fn glass_menu_anchor_id() -> Id {
    Id::of(("glass-menu", "anchor"))
}

pub(crate) fn glass_menu_item_id(j: usize) -> Id {
    glass_menu_id().with(("item", j))
}

pub(crate) fn glass_popover_id() -> Id {
    Id::of("glass-popover")
}

pub(crate) fn glass_popover_anchor_id() -> Id {
    Id::of(("glass-popover", "anchor"))
}

pub(crate) fn glass_popover_done_id() -> Id {
    Id::of(("glass-popover", "done"))
}

/// Room the popover unfolds into: the sheet's own height plus a margin.
pub(crate) const POPOVER_ROOM: f32 = 190.0;

/// An anchor button and the menu it unfolds, plus the command the app was last told about.
pub(crate) fn menu_flyout_page(g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut body = Container::new(Id::of(("menu", "body")), NodeKind::Column).gap(12.0);
    body = body.child_decl(label(
        Id::of(("menu", "caption")),
        tr(cx, Msg::GALLERY_MENU_CAPTION),
        12.0,
        muted(),
        420.0,
    ));

    let labels: [String; MENU_COMMANDS] = std::array::from_fn(|j| tr(cx, MENU_LABELS[j]));
    let accels: [String; MENU_COMMANDS] = std::array::from_fn(|j| tr(cx, MENU_ACCELS[j]));

    let mut scoped = cx.at_scope(style_env("fluent"));
    let anchor = g.fluent.button(
        ui_widgets_fluent::ButtonParams::new(menu_anchor_id(), tr(&scoped, Msg::GALLERY_MENU_SHOW)),
        &mut scoped,
    );
    // A row keeps the button at its own width; a column would stretch it across the page.
    body = body.child(Box::new(
        Container::new(Id::of(("menu", "anchor-row")), NodeKind::Row)
            .cross_align(0.0)
            .child(scoped.element(anchor)),
    ));

    // The panel is viewport-anchored, so the page only has to leave it somewhere to land.
    body = body.child(Box::new(
        Container::new(Id::of(("menu", "room")), NodeKind::Box)
            .size(SizeClass::fixed(420.0, MENU_ROOM)),
    ));

    let mut entries: Vec<MenuFlyoutEntry> = Vec::with_capacity(MENU_COMMANDS + 1);
    for j in 0..MENU_COMMANDS {
        if j == MENU_SEPARATOR_AT {
            entries.push(MenuFlyoutEntry::Separator);
        }
        let mut item = MenuFlyoutItem::new(menu_item_id(j), labels[j].clone())
            .icon(ui_widgets_fluent::glyphs::icon(MENU_ICONS[j]).expect("the table carries it"))
            .accelerator(accels[j].clone());
        // `MenuFlyoutItem.IsEnabled=false`: the last row shows the disabled brushes.
        if j + 1 == MENU_COMMANDS {
            item = item.disabled();
        }
        entries.push(MenuFlyoutEntry::Item(item));
    }

    let mut scoped = cx.at_scope(style_env("fluent"));
    let params = MenuFlyoutParams::new(menu_flyout_id(), Anchor::Node(menu_anchor_id()), &entries)
        .open(g.menu_open);
    let control = g.fluent.menu_flyout(params, &mut scoped);
    body = body.child(scoped.element(control));

    let readout = match g.menu_picked {
        Some(j) => tf(
            cx,
            Msg::GALLERY_MENU_READOUT_PICKED,
            &[("command", arg(labels[j].clone()))],
        ),
        None => tr(cx, Msg::GALLERY_MENU_READOUT_NONE),
    };
    body = body.child_decl(label(
        Id::of(("menu", "readout")),
        readout,
        12.0,
        muted(),
        420.0,
    ));

    let mut row = Container::new(Id::of(("bodyrow", Page::MenuFlyout.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    row = row.child(column_w(
        Page::MenuFlyout,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(body),
        480.0,
        COL_TALL_H,
        cx,
    ));
    let glass_body = glass_menu_body(g, &labels, cx);
    row = row.child(column(
        Page::MenuFlyout,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        glass_body,
        COL_TALL_H,
        cx,
    ));
    // The MD3 menu sits after the two anchored panels: a column before them would push the
    // glass anchor far enough right that its panel clamps to the window edge.
    let md3_body = md3_menu_body(g, &labels, &accels, cx);
    row = row.child(column_w(
        Page::MenuFlyout,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        md3_body,
        320.0,
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}

/// The MD3 menu as a surface the page places: the same commands, one with a supporting
/// line, one selected and one disabled.
fn md3_menu_body(
    g: &Gallery,
    labels: &[String; MENU_COMMANDS],
    accels: &[String; MENU_COMMANDS],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_widgets_md3::{Md3Menu, Md3MenuEntry, Md3MenuItem, Md3MenuParams};
    let icons = ["note_add", "folder_open", "save", "share", "print"];
    let caption = tr(cx, Msg::GALLERY_MD3_MENU_CAPTION);
    let section = tr(cx, Msg::GALLERY_MD3_MENU_SECTION);
    let supporting = tr(cx, Msg::GALLERY_MD3_MENU_SUPPORTING);
    let readout = match g.md3_menu_picked {
        Some(j) => tf(
            cx,
            Msg::GALLERY_MENU_READOUT_PICKED,
            &[("command", arg(labels[j].clone()))],
        ),
        None => tr(cx, Msg::GALLERY_MENU_READOUT_NONE),
    };
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(cx.env()).appearance);
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut entries = Vec::with_capacity(MENU_COMMANDS + 2);
    entries.push(Md3MenuEntry::Section(section));
    for j in 0..MENU_COMMANDS {
        let mut item = Md3MenuItem::new(md3_menu_item_id(j), labels[j].clone())
            .leading(ui_widgets::IconSource::glyph(
                icons[j],
                ui_widgets::IconSet::Outlined,
            ))
            .shortcut(accels[j].clone())
            .selected(g.md3_menu_picked == Some(j));
        if j == 2 {
            item = item.supporting(supporting.clone());
        }
        if j + 1 == MENU_COMMANDS {
            item = item.disabled(true);
        }
        entries.push(Md3MenuEntry::Item(item));
    }
    let menu = Md3Menu::new_with(
        Md3MenuParams::new(md3_menu_id(), entries),
        &mut scoped,
        pack,
    );
    let mut body = Container::new(Id::of(("md3-menu", "body")), NodeKind::Column).gap(12.0);
    body = body.child_decl(label(
        Id::of(("md3-menu", "caption")),
        caption,
        12.0,
        muted(),
        300.0,
    ));
    body = body.child(scoped.element(Box::new(menu)));
    body = body.child_decl(label(
        Id::of(("md3-menu", "readout")),
        readout,
        12.0,
        muted(),
        300.0,
    ));
    Box::new(body)
}

/// The glass menu: a glass button for the anchor and the frosted panel it unfolds.
fn glass_menu_body(
    g: &Gallery,
    labels: &[String; MENU_COMMANDS],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("glass"));
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let anchor_label = tr(&scoped, Msg::GALLERY_MENU_SHOW);
    let anchor = {
        let params = GlassButtonParams::labelled(glass_menu_anchor_id(), None, &anchor_label);
        let mut node = scoped.at(glass_menu_anchor_id());
        GlassButton::new_with(HigOptions::default(), params, &mut node, &pack)
    };

    let mut entries: Vec<GlassMenuEntry> = Vec::with_capacity(MENU_COMMANDS + 1);
    for j in 0..MENU_COMMANDS {
        if j == MENU_SEPARATOR_AT {
            entries.push(GlassMenuEntry::divider(glass_menu_id().with("divider")));
        }
        let mut item = GlassMenuEntry::item(glass_menu_item_id(j), &labels[j])
            .icon(sf_glyph(MENU_GLASS_ICONS[j]));
        if j + 1 == MENU_COMMANDS {
            item = item.disabled();
        }
        entries.push(item);
    }
    let menu = GlassMenu::new(
        GlassMenuParams::new(
            glass_menu_id(),
            Anchor::Node(glass_menu_anchor_id()),
            &entries,
        )
        .open(g.gmenu_open),
        &mut scoped,
        &pack,
    );

    let readout = match g.gmenu_picked {
        Some(j) => tf(
            &scoped,
            Msg::GALLERY_MENU_READOUT_PICKED,
            &[("command", arg(labels[j].clone()))],
        ),
        None => tr(&scoped, Msg::GALLERY_MENU_READOUT_NONE),
    };
    // ── The popover: the same sheet, holding the app's own blocks. Its "Done" reports to
    // the app like any other control; the app is what closes the sheet.
    let pop_anchor_label = tr(&scoped, Msg::GALLERY_POPOVER_SHOW);
    let pop_anchor = {
        let params =
            GlassButtonParams::labelled(glass_popover_anchor_id(), None, &pop_anchor_label);
        let mut node = scoped.at(glass_popover_anchor_id());
        GlassButton::new_with(HigOptions::default(), params, &mut node, &pack)
    };
    let pop_ink = pack.ink("textFillColorPrimary").unwrap_or(Color::BLACK);
    let pop_text_w = POPOVER_W - POPOVER_PAD * 2.0;
    let pop_title = ui_widgets::Text::new(
        Id::of(("gpop", "title")),
        tr(&scoped, Msg::GALLERY_POPOVER_TITLE),
        16.0,
        pop_ink,
    )
    .weight(ui_core::Weight::SEMIBOLD)
    .max_width(pop_text_w)
    .resolve()
    .margin(Insets {
        bottom: 6.0,
        ..Insets::ZERO
    });
    let pop_body = ui_widgets::Text::new(
        Id::of(("gpop", "body")),
        tr(&scoped, Msg::GALLERY_POPOVER_BODY),
        13.0,
        Color { a: 0.65, ..pop_ink },
    )
    .max_width(pop_text_w)
    .resolve()
    .margin(Insets {
        bottom: 12.0,
        ..Insets::ZERO
    });
    let pop_done_label = tr(&scoped, Msg::GALLERY_POPOVER_DONE);
    let pop_done = {
        let params = GlassButtonParams::labelled(glass_popover_done_id(), None, &pop_done_label);
        let mut node = scoped.at(glass_popover_done_id());
        GlassButton::new_with(HigOptions::default(), params, &mut node, &pack)
    };
    let popover = GlassPopover::new(
        GlassPopoverParams::new(glass_popover_id(), Anchor::Node(glass_popover_anchor_id()))
            .open(g.gpop_open)
            .presses(vec![glass_popover_done_id()])
            .content(vec![pop_title, pop_body, pop_done.decl(&scoped)]),
        &mut scoped,
        &pack,
    );
    let pop_readout = tf(
        &scoped,
        Msg::GALLERY_POPOVER_READOUT,
        &[("count", arg(g.gpop_done as i64))],
    );
    Box::new(
        Container::new(Id::of(("gmenu", "body")), NodeKind::Column)
            .gap(12.0)
            .child_decl(label(
                Id::of(("gmenu", "caption")),
                tr(&scoped, Msg::GALLERY_MENU_GLASS_CAPTION),
                12.0,
                muted(),
                COL_W - 24.0,
            ))
            .child(Box::new(
                Container::new(Id::of(("gmenu", "anchor-row")), NodeKind::Row)
                    .cross_align(0.0)
                    .child(scoped.element(Box::new(anchor))),
            ))
            .child(Box::new(
                Container::new(Id::of(("gmenu", "room")), NodeKind::Box)
                    .size(SizeClass::fixed(COL_W - 40.0, MENU_ROOM)),
            ))
            .child(scoped.element(Box::new(menu)))
            .child_decl(label(
                Id::of(("gmenu", "readout")),
                readout,
                12.0,
                muted(),
                COL_W - 24.0,
            ))
            .child_decl(label(
                Id::of(("gpop", "caption")),
                tr(&scoped, Msg::GALLERY_POPOVER_CAPTION),
                12.0,
                muted(),
                COL_W - 24.0,
            ))
            .child(Box::new(
                Container::new(Id::of(("gpop", "anchor-row")), NodeKind::Row)
                    .cross_align(0.0)
                    .child(scoped.element(Box::new(pop_anchor))),
            ))
            .child(Box::new(
                Container::new(Id::of(("gpop", "room")), NodeKind::Box)
                    .size(SizeClass::fixed(COL_W - 40.0, POPOVER_ROOM)),
            ))
            .child(scoped.element(Box::new(popover)))
            .child_decl(label(
                Id::of(("gpop", "readout")),
                pop_readout,
                12.0,
                muted(),
                COL_W - 24.0,
            )),
    )
}
