// The action sheet page.

use ui_widgets_glass::{
    GlassActionSheet, GlassActionSheetParams, GlassButton, GlassButtonParams, GlassSheetAction,
    HigOptions,
};

use crate::pages::glyphs::sf_glyph;
use crate::shell::*;

/// The app's own ids: the trigger, the sheet root, each action and the cancel card.
pub(crate) fn sheet_open_id() -> Id {
    Id::of(("action-sheet", Page::ActionSheet.tag(), "open"))
}

pub(crate) fn sheet_id() -> Id {
    Id::of(("action-sheet", Page::ActionSheet.tag()))
}

fn sheet_action_id(k: usize) -> Id {
    sheet_id().with(("action", k))
}

fn sheet_cancel_id() -> Id {
    sheet_id().with("cancel")
}

/// The three actions, in order; the last one is destructive.
pub(crate) const SHEET_ACTIONS: [Msg; 3] = [
    Msg::GALLERY_ACTION_SHEET_OPEN,
    Msg::GALLERY_ACTION_SHEET_SHARE,
    Msg::GALLERY_ACTION_SHEET_DELETE,
];

/// Which action a reported id names, for the readout.
pub(crate) fn sheet_label_of(id: Id) -> Option<Msg> {
    SHEET_ACTIONS
        .iter()
        .enumerate()
        .find(|(k, _)| sheet_action_id(*k) == id)
        .map(|(_, msg)| *msg)
        .or_else(|| (sheet_cancel_id() == id).then_some(Msg::GALLERY_ACTION_SHEET_CANCEL))
}

/// The live sheet, declared at the window root: the engine resolves the modal region.
pub(crate) fn sheet_modal(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let actions: Vec<GlassSheetAction> = SHEET_ACTIONS
        .iter()
        .enumerate()
        .map(|(k, msg)| {
            let mut action = GlassSheetAction::new(sheet_action_id(k), tr(cx, *msg));
            match k {
                1 => action = action.icon(sf_glyph("square_and_arrow_up")),
                2 => action = action.destructive().icon(sf_glyph("trash")),
                _ => {}
            }
            action
        })
        .collect();
    let cancel = GlassSheetAction::new(sheet_cancel_id(), tr(cx, Msg::GALLERY_ACTION_SHEET_CANCEL));
    let title = tr(cx, Msg::GALLERY_ACTION_SHEET_TITLE);
    let message = tr(cx, Msg::GALLERY_ACTION_SHEET_MESSAGE);
    let params = GlassActionSheetParams::new(sheet_id(), actions)
        .title(&title)
        .message(&message)
        .cancel(cancel);
    let sheet = GlassActionSheet::new(params, cx, &pack);
    cx.element(Box::new(sheet))
}

/// The page: a trigger that opens the sheet, and the app's record of what it reported.
pub(crate) fn action_sheet_page(
    picked: Option<Msg>,
    dismissed: u32,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::ActionSheet.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    // MD3 and Fluent have no action sheet in this repository yet.
    for tag in ["md3", "fluent"] {
        row = row.child(pending_column(
            Page::ActionSheet,
            tag,
            COL_W,
            COL_TALL_H,
            cx,
        ));
    }

    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut scoped = cx.at_scope(style_env("glass"));
    let trigger_label = tr(&scoped, Msg::GALLERY_ACTION_SHEET_SHOW);
    let trigger = {
        let params = GlassButtonParams::labelled(sheet_open_id(), None, &trigger_label);
        let mut node = scoped.at(sheet_open_id());
        GlassButton::new_with(HigOptions::default(), params, &mut node, &pack)
    };
    let mut col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    col = col.child(Box::new(captioned_row(
        sheet_open_id(),
        scoped.element(Box::new(trigger)),
        tr(&scoped, Msg::GALLERY_ACTION_SHEET_CAPTION),
    )));
    let action = picked
        .map(|msg| tr(&scoped, msg))
        .unwrap_or_else(|| "—".to_string());
    col = col.child_decl(label(
        Id::str("action-sheet/readout"),
        tf(
            &scoped,
            Msg::GALLERY_ACTION_SHEET_READOUT,
            &[("action", arg(action)), ("count", arg(dismissed as i64))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::ActionSheet,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
