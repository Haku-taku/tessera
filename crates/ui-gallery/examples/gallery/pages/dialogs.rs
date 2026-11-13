// The dialogs page and its modal renderer.

use ui_widgets_glass::{GlassActionKind, GlassDialog, GlassDialogAction, GlassDialogParams};

use crate::shell::*;

/// The dialogs page's modal host: a panel a proximate dialog dims on its own.
pub(crate) const PANEL_W: f32 = 340.0;
pub(crate) const PANEL_H: f32 = 240.0;
/// The dialog page's build-time variants: caption, title, body, action keys, icon or not.
pub(crate) const DIALOG_PREVIEWS: [(Msg, Option<Msg>, Option<Msg>, &[Msg], bool); 3] = [
    (
        Msg::GALLERY_DIALOG_CAPTION_NO_ICON,
        Some(Msg::GALLERY_DIALOG_TITLE_DISCARD),
        Some(Msg::GALLERY_DIALOG_BODY_DISCARD),
        &[
            Msg::GALLERY_DIALOG_ACTION_CANCEL,
            Msg::GALLERY_DIALOG_ACTION_DISCARD,
        ],
        false,
    ),
    (
        Msg::GALLERY_DIALOG_CAPTION_WITH_ICON,
        Some(Msg::GALLERY_DIALOG_TITLE_RESET),
        Some(Msg::GALLERY_DIALOG_BODY_RESET),
        &[
            Msg::GALLERY_DIALOG_ACTION_LATER,
            Msg::GALLERY_DIALOG_ACTION_SAVE,
            Msg::GALLERY_DIALOG_ACTION_RESET,
        ],
        true,
    ),
    (
        Msg::GALLERY_DIALOG_CAPTION_NO_TITLE,
        None,
        Some(Msg::GALLERY_DIALOG_BODY_UNDONE),
        &[Msg::GALLERY_DIALOG_ACTION_OK],
        false,
    ),
];

/// The glass column's three alerts: caption, title, message, and each action's slot.
pub(crate) const GLASS_DIALOG_PREVIEWS: [(Msg, Option<Msg>, Msg, &[(Msg, GlassActionKind)]); 3] = [
    (
        Msg::GALLERY_DIALOG_GLASS_CAPTION_TWO,
        Some(Msg::GALLERY_DIALOG_TITLE_DISCARD),
        Msg::GALLERY_DIALOG_BODY_DISCARD,
        &[
            (Msg::GALLERY_DIALOG_ACTION_CANCEL, GlassActionKind::Default),
            (
                Msg::GALLERY_DIALOG_ACTION_DISCARD,
                GlassActionKind::Destructive,
            ),
        ],
    ),
    (
        Msg::GALLERY_DIALOG_GLASS_CAPTION_THREE,
        Some(Msg::GALLERY_DIALOG_TITLE_RESET),
        Msg::GALLERY_DIALOG_BODY_RESET,
        &[
            (Msg::GALLERY_DIALOG_ACTION_LATER, GlassActionKind::Default),
            (Msg::GALLERY_DIALOG_ACTION_SAVE, GlassActionKind::Primary),
            (Msg::GALLERY_DIALOG_ACTION_RESET, GlassActionKind::Default),
        ],
    ),
    (
        Msg::GALLERY_DIALOG_GLASS_CAPTION_ONE,
        None,
        Msg::GALLERY_DIALOG_BODY_UNDONE,
        &[(Msg::GALLERY_DIALOG_ACTION_OK, GlassActionKind::Primary)],
    ),
];

pub(crate) fn glass_dialog_preview_id(k: usize) -> Id {
    Id::of(("glass-dialog-preview", k))
}

pub(crate) fn glass_dialog_action_id(k: usize, i: usize) -> Id {
    Id::of(("glass-dialog-action", k, i))
}

/// Whether `id` names one of the glass column's alerts.
pub(crate) fn glass_dialog_is_ours(id: Id) -> bool {
    (0..GLASS_DIALOG_PREVIEWS.len()).any(|k| glass_dialog_preview_id(k) == id)
}

/// The label behind a glass action id; `None` when the id is not one of them.
pub(crate) fn glass_dialog_label_of(id: Id) -> Option<Msg> {
    for (k, preview) in GLASS_DIALOG_PREVIEWS.iter().enumerate() {
        for (i, action) in preview.3.iter().enumerate() {
            if glass_dialog_action_id(k, i) == id {
                return Some(action.0);
            }
        }
    }
    None
}

/// The modal's own actions: the same two a confirmation dialog usually carries.
pub(crate) const DIALOG_MODAL_ACTIONS: [Msg; 2] = [
    Msg::GALLERY_DIALOG_ACTION_CANCEL,
    Msg::GALLERY_DIALOG_ACTION_DISCARD,
];

pub(crate) fn dialog_preview_id(k: usize) -> Id {
    Id::of(("dialog-preview", k))
}

/// An action id is unique per dialog: two dialogs sharing one would both report the click.
pub(crate) fn dialog_action_id(preview: usize, k: usize) -> Id {
    Id::of(("dialog-action", preview, k))
}

pub(crate) fn dialog_modal_id() -> Id {
    Id::str("dialog-modal")
}

pub(crate) fn dialog_modal_action_id(k: usize) -> Id {
    Id::of(("dialog-modal-action", k))
}

pub(crate) fn dialog_open_id() -> Id {
    Id::str("dialog-open")
}

/// Opens the same modal, but resolved to the panel instead of the window.
pub(crate) fn dialog_open_panel_id() -> Id {
    Id::str("dialog-open-panel")
}

/// The label of the action behind an id; `None` when the id is not a dialog action.
pub(crate) fn dialog_label_of(id: Id) -> Option<Msg> {
    for (preview, row) in DIALOG_PREVIEWS.iter().enumerate() {
        for (k, label) in row.3.iter().enumerate() {
            if dialog_action_id(preview, k) == id {
                return Some(*label);
            }
        }
    }
    (0..DIALOG_MODAL_ACTIONS.len())
        .find(|k| dialog_modal_action_id(*k) == id)
        .map(|k| DIALOG_MODAL_ACTIONS[k])
}

/// Whether `id` names one of this page's dialogs, preview or modal.
pub(crate) fn dialog_is_ours(id: Id) -> bool {
    id == dialog_modal_id() || (0..DIALOG_PREVIEWS.len()).any(|k| dialog_preview_id(k) == id)
}

/// The dialog's build-time variants plus one live modal. Every preview is a real surface —
/// its actions report — and only the modal dims and blocks what is behind it.
pub(crate) fn dialogs_page(
    picked: Option<Msg>,
    dismissed: u32,
    open: bool,
    target: ModalTarget,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Dialogs.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(10.0);
    for (k, spec) in DIALOG_PREVIEWS.iter().enumerate() {
        let actions: Vec<ui_widgets_md3::DialogAction> = spec
            .3
            .iter()
            .enumerate()
            .map(|(i, label)| ui_widgets_md3::DialogAction {
                id: dialog_action_id(k, i),
                label: tr(cx, *label),
            })
            .collect();
        let content = ui_widgets_md3::DialogContent {
            id: dialog_preview_id(k),
            title: spec.1.map(|msg| tr(cx, msg)),
            body: spec.2.map(|msg| tr(cx, msg)),
            content: Vec::new(),
            actions,
            icon: if spec.4 {
                ui_widgets_md3::DialogIcon::Slot
            } else {
                ui_widgets_md3::DialogIcon::None
            },
            build_icon: spec.4.then_some(ms("info")),
            width: 320.0,
            target: ModalTarget::Global,
            barrier: false,
        };
        let mut scoped = cx.at_scope(style_env("md3"));
        let control = style.dialog(content, &mut scoped);
        let preview = scoped.element(control);
        col = col.child(Box::new(captioned_row(
            dialog_preview_id(k),
            preview,
            tr(cx, spec.0),
        )));
    }
    let opener = {
        let label = tr(cx, Msg::GALLERY_DIALOG_SHOW_MODAL);
        let mut scoped = cx.at_scope(style_env("md3"));
        let control = style.button(
            ui_widgets_md3::ButtonParams::new(dialog_open_id(), label)
                .style(ui_widgets_md3::ButtonStyle::Filled)
                .size(ui_widgets_md3::ButtonSize::Base),
            &mut scoped,
        );
        scoped.element(control)
    };
    col = col.child(Box::new(captioned_row(
        Id::str("dialog-open-row"),
        Box::new(
            Container::new(Id::str("dialog-open-hug"), NodeKind::Row)
                .cross_align(0.0)
                .child(opener),
        ),
        tr(cx, Msg::GALLERY_DIALOG_CAPTION_MODAL),
    )));
    // A panel that declares itself a modal host: a proximate dialog covers this panel and
    // nothing else, and follows it through every resize.
    let panel_button = {
        let label = tr(cx, Msg::GALLERY_DIALOG_SHOW_IN_PANEL);
        let mut scoped = cx.at_scope(style_env("md3"));
        let control = style.button(
            ui_widgets_md3::ButtonParams::new(dialog_open_panel_id(), label)
                .style(ui_widgets_md3::ButtonStyle::Tonal)
                .size(ui_widgets_md3::ButtonSize::Base),
            &mut scoped,
        );
        scoped.element(control)
    };
    let mut panel = Container::new(Id::str("dialog-panel"), NodeKind::Column)
        .size(SizeClass::fixed(PANEL_W, PANEL_H))
        .padding(Insets::all(16.0))
        .gap(8.0)
        .cross_align(0.0)
        .background(Color::from_rgba8(38, 42, 54, 255), Corners::all(20.0))
        .map(|d| d.modal_host())
        .child_decl(label(
            Id::str("dialog-panel-note"),
            tr(cx, Msg::GALLERY_DIALOG_PANEL_NOTE),
            12.0,
            muted(),
            PANEL_W - 32.0,
        ))
        .child(panel_button);
    if open && target == ModalTarget::Proximate {
        panel = panel.child(dialogs_modal(ModalTarget::Proximate, cx));
    }
    // The host panel's own column: the md3 column is fixed-height and its previews plus
    // openers already fill it, so a taller neighbour keeps every sample at its real size.
    let host_body = Container::new(Id::str("dialogs/host-body"), NodeKind::Column)
        .gap(10.0)
        .child_decl(label(
            Id::str("dialogs/host-note"),
            tr(cx, Msg::GALLERY_DIALOG_HOST_NOTE),
            12.0,
            muted(),
            COL_W - 24.0,
        ))
        .child(Box::new(panel));
    let action = picked
        .map(|msg| tr(cx, msg))
        .unwrap_or_else(|| "—".to_string());
    col = col.child_decl(label(
        Id::str("dialogs/readout"),
        tf(
            cx,
            Msg::GALLERY_DIALOGS_READOUT,
            &[("action", arg(action)), ("count", arg(dismissed as i64))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Dialogs,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    row = row.child(column(
        Page::Dialogs,
        "host",
        Msg::GALLERY_DIALOG_COLUMN_HOST,
        Box::new(host_body),
        COL_TALL_H,
        cx,
    ));
    // The host panel takes the third slot: a content dialog is 320 wide at its narrowest,
    // so the Fluent column cannot be the thin one every other page uses.
    row = row.child(column(
        Page::Dialogs,
        "fluent",
        Msg::GALLERY_DIALOG_COLUMN_FLUENT,
        super::dialogs_fluent::fluent_dialog_column(cx),
        COL_TALL_H,
        cx,
    ));
    row = row.child(column(
        Page::Dialogs,
        "glass",
        Msg::GALLERY_DIALOG_COLUMN_GLASS,
        Box::new(glass_dialog_column(cx)),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}

/// The glass column: three alerts, each a real surface whose actions report.
fn glass_dialog_column(cx: &mut Cx<'_, '_>) -> Container {
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    for (k, (caption, title, message, actions)) in GLASS_DIALOG_PREVIEWS.iter().enumerate() {
        let actions: Vec<GlassDialogAction> = actions
            .iter()
            .enumerate()
            .map(|(i, (label, kind))| GlassDialogAction {
                kind: *kind,
                ..GlassDialogAction::new(glass_dialog_action_id(k, i), tr(cx, *label))
            })
            .collect();
        let title = title.map(|msg| tr(cx, msg)).unwrap_or_default();
        let dialog = GlassDialog::new(
            GlassDialogParams::new(
                glass_dialog_preview_id(k),
                &title,
                &tr(cx, *message),
                actions,
            )
            .barrier(false),
            cx,
            &pack,
        );
        col = col.child(Box::new(captioned_row(
            glass_dialog_preview_id(k),
            cx.element(Box::new(dialog)),
            tr(cx, *caption),
        )));
    }
    col
}

/// The live modal, declared at the window root: the engine resolves its target, so the app
/// names the region instead of measuring it.
pub(crate) fn dialogs_modal(target: ModalTarget, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let style = Md3Kit::new();
    let content = ui_widgets_md3::DialogContent {
        id: dialog_modal_id(),
        title: Some(tr(cx, Msg::GALLERY_DIALOG_TITLE_PUBLISH)),
        body: Some(tr(cx, Msg::GALLERY_DIALOG_BODY_PUBLISH)),
        content: Vec::new(),
        actions: DIALOG_MODAL_ACTIONS
            .iter()
            .enumerate()
            .map(|(k, label)| ui_widgets_md3::DialogAction {
                id: dialog_modal_action_id(k),
                label: tr(cx, *label),
            })
            .collect(),
        icon: ui_widgets_md3::DialogIcon::Slot,
        build_icon: Some(ms("info")),
        width: 300.0,
        target,
        barrier: true,
    };
    let mut scoped = cx.at_scope(style_env("md3"));
    let control = style.dialog(content, &mut scoped);
    scoped.element(control)
}
