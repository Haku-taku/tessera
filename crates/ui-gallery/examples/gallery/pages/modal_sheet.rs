// The modal sheet page.

use ui_widgets_glass::{
    GlassButton, GlassButtonParams, GlassModalSheet, GlassModalSheetParams, GlassSheetDetent,
    HigOptions,
};

use crate::shell::*;

/// The app's own ids: the trigger, the sheet root and the button inside it.
pub(crate) fn modal_sheet_open_id() -> Id {
    Id::of(("modal-sheet", Page::ModalSheet.tag(), "open"))
}

pub(crate) fn modal_sheet_id() -> Id {
    Id::of(("modal-sheet", Page::ModalSheet.tag()))
}

pub(crate) fn modal_sheet_done_id() -> Id {
    modal_sheet_id().with("done")
}

/// The surface itself, below the modal root: what a drag moves.
pub(crate) fn modal_sheet_body_id() -> Id {
    modal_sheet_id().with("sheet")
}

/// The MD3 sample's own ids: its trigger, sheet root and the button inside it.
pub(crate) fn md3_sheet_id() -> Id {
    Id::str("bottom-sheet/md3")
}

pub(crate) fn md3_sheet_open_id() -> Id {
    md3_sheet_id().with("open")
}

pub(crate) fn md3_sheet_done_id() -> Id {
    md3_sheet_id().with("done")
}

/// The MD3 side sheet's own ids: its trigger, panel root and the action inside it.
pub(crate) fn md3_side_sheet_id() -> Id {
    Id::str("side-sheet/md3")
}

pub(crate) fn md3_side_sheet_open_id() -> Id {
    md3_side_sheet_id().with("open")
}

pub(crate) fn md3_side_sheet_save_id() -> Id {
    md3_side_sheet_id().with("save")
}

/// The live side sheet, declared at the window root like the bottom sheet.
pub(crate) fn md3_side_sheet_modal(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("md3"));
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(scoped.env()).appearance);
    let body = label(
        md3_side_sheet_id().with("body"),
        tr(&scoped, Msg::GALLERY_SIDE_SHEET_BODY),
        14.0,
        muted(),
        208.0,
    );
    let save = ui_widgets_md3::DialogAction {
        id: md3_side_sheet_save_id(),
        label: tr(&scoped, Msg::GALLERY_SIDE_SHEET_SAVE),
    };
    let params =
        ui_widgets_md3::SideSheetParams::new(md3_side_sheet_id(), ui_widgets_md3::SheetEdge::End)
            .headline(tr(&scoped, Msg::GALLERY_SIDE_SHEET_TITLE))
            .content(vec![body])
            .actions(vec![save]);
    let sheet = ui_widgets_md3::Md3SideSheet::new_with(params, &mut scoped, pack);
    scoped.element(Box::new(sheet))
}

/// The live MD3 sheet, declared at the window root like the glass one.
pub(crate) fn md3_sheet_modal(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("md3"));
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(scoped.env()).appearance);
    let ink = pack.ink("onSurface").unwrap_or(Color::BLACK);
    let title = label(
        md3_sheet_id().with("title"),
        tr(&scoped, Msg::GALLERY_BOTTOM_SHEET_TITLE),
        22.0,
        ink,
        COL_W - 48.0,
    );
    let body = label(
        md3_sheet_id().with("body"),
        tr(&scoped, Msg::GALLERY_BOTTOM_SHEET_BODY),
        14.0,
        muted(),
        COL_W - 48.0,
    );
    let done_label = tr(&scoped, Msg::GALLERY_BOTTOM_SHEET_DONE);
    let done = Md3Kit::new().button(
        ui_widgets_md3::ButtonParams::new(md3_sheet_done_id(), done_label)
            .style(ui_widgets_md3::ButtonStyle::Text),
        &mut scoped,
    );
    let content = vec![title, body, done.decl(&scoped)];
    let params = ui_widgets_md3::BottomSheetParams::new(md3_sheet_id(), content)
        .padding(Insets {
            left: 24.0,
            right: 24.0,
            top: 0.0,
            bottom: 24.0,
        })
        .presses(vec![md3_sheet_done_id()]);
    let sheet = ui_widgets_md3::Md3BottomSheet::new_with(params, &mut scoped, pack);
    scoped.element(Box::new(sheet))
}

/// The live sheet, declared at the window root: the engine resolves the modal region.
pub(crate) fn modal_sheet_modal(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut scoped = cx.at_scope(style_env("glass"));
    let ink = pack
        .ink("textFillColorPrimary")
        .unwrap_or(ui_core::geom::Color::BLACK);
    let title = label(
        modal_sheet_id().with("title"),
        tr(&scoped, Msg::GALLERY_MODAL_SHEET_TITLE),
        17.0,
        ink,
        COL_W - 48.0,
    );
    let body = label(
        modal_sheet_id().with("body"),
        tr(&scoped, Msg::GALLERY_MODAL_SHEET_BODY),
        13.0,
        muted(),
        COL_W - 48.0,
    );
    let done_label = tr(&scoped, Msg::GALLERY_MODAL_SHEET_DONE);
    let done = {
        let params = GlassButtonParams::labelled(modal_sheet_done_id(), None, &done_label);
        let mut node = scoped.at(modal_sheet_done_id());
        GlassButton::new_with(HigOptions::default(), params, &mut node, pack)
    };
    let params =
        GlassModalSheetParams::new(modal_sheet_id(), vec![title, body, done.decl(&scoped)])
            .padding(Insets {
                left: 24.0,
                right: 24.0,
                top: 8.0,
                bottom: 24.0,
            })
            .initial(GlassSheetDetent::Half)
            .presses(vec![modal_sheet_done_id()]);
    let sheet = GlassModalSheet::new(params, &mut scoped, pack);
    scoped.element(Box::new(sheet))
}

/// The page: a trigger that opens the sheet, and the app's record of what it reported.
pub(crate) fn modal_sheet_page(
    detent: Option<Msg>,
    dismissed: u32,
    side_dismissed: u32,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::ModalSheet.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    // Fluent has no modal sheet in this repository yet.
    row = row.child(pending_column(
        Page::ModalSheet,
        "fluent",
        COL_W,
        COL_TALL_H,
        cx,
    ));

    let mut md3 = cx.at_scope(style_env("md3"));
    let open_label = tr(&md3, Msg::GALLERY_BOTTOM_SHEET_OPEN);
    let open = Md3Kit::new().button(
        ui_widgets_md3::ButtonParams::new(md3_sheet_open_id(), open_label)
            .style(ui_widgets_md3::ButtonStyle::Filled),
        &mut md3,
    );
    let mut col3 = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(14.0);
    col3 = col3.child(Box::new(captioned_row(
        md3_sheet_open_id(),
        md3.element(open),
        tr(&md3, Msg::GALLERY_BOTTOM_SHEET_CAPTION),
    )));
    col3 = col3.child_decl(label(
        Id::str("bottom-sheet/readout"),
        tf(
            &md3,
            Msg::GALLERY_BOTTOM_SHEET_READOUT,
            &[("count", arg(dismissed as i64))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    let side_label = tr(&md3, Msg::GALLERY_SIDE_SHEET_OPEN);
    let side_open = Md3Kit::new().button(
        ui_widgets_md3::ButtonParams::new(md3_side_sheet_open_id(), side_label)
            .style(ui_widgets_md3::ButtonStyle::Tonal),
        &mut md3,
    );
    col3 = col3.child(Box::new(captioned_row(
        md3_side_sheet_open_id(),
        md3.element(side_open),
        tr(&md3, Msg::GALLERY_SIDE_SHEET_CAPTION),
    )));
    col3 = col3.child_decl(label(
        Id::str("side-sheet/readout"),
        tf(
            &md3,
            Msg::GALLERY_SIDE_SHEET_READOUT,
            &[("count", arg(side_dismissed as i64))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::ModalSheet,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col3),
        COL_TALL_H,
        &md3,
    ));

    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut scoped = cx.at_scope(style_env("glass"));
    let trigger_label = tr(&scoped, Msg::GALLERY_MODAL_SHEET_SHOW);
    let trigger = {
        let params = GlassButtonParams::labelled(modal_sheet_open_id(), None, &trigger_label);
        let mut node = scoped.at(modal_sheet_open_id());
        GlassButton::new_with(HigOptions::default(), params, &mut node, pack)
    };
    let mut col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    col = col.child(Box::new(captioned_row(
        modal_sheet_open_id(),
        scoped.element(Box::new(trigger)),
        tr(&scoped, Msg::GALLERY_MODAL_SHEET_CAPTION),
    )));
    let detent = detent
        .map(|msg| tr(&scoped, msg))
        .unwrap_or_else(|| tr(&scoped, Msg::GALLERY_MODAL_SHEET_NONE));
    col = col.child_decl(label(
        Id::str("modal-sheet/readout"),
        tf(
            &scoped,
            Msg::GALLERY_MODAL_SHEET_READOUT,
            &[("detent", arg(detent)), ("count", arg(dismissed as i64))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::ModalSheet,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
