// The Fluent column on the dialogs page: three previews and the live content dialog.

use ui_core::tree::ModalTarget;

use crate::shell::*;

/// One preview's build-time facts; `None` means that button is not in the template.
pub(crate) struct FluentPreview {
    pub caption: Msg,
    /// `None` means the template's title row is absent.
    pub title: Option<Msg>,
    pub body: Msg,
    pub primary: Option<Msg>,
    pub secondary: Option<Msg>,
    pub close: Option<Msg>,
    pub default: ui_widgets_fluent::ContentDialogButton,
}

/// The three `ButtonsVisibilityStates` the page shows, one per caption.
pub(crate) const FLUENT_PREVIEWS: [FluentPreview; 3] = [
    FluentPreview {
        caption: Msg::GALLERY_DIALOG_FLUENT_CAPTION_DEFAULT,
        title: Some(Msg::GALLERY_DIALOG_TITLE_DISCARD),
        body: Msg::GALLERY_DIALOG_BODY_DISCARD,
        primary: Some(Msg::GALLERY_DIALOG_ACTION_DELETE),
        secondary: None,
        close: Some(Msg::GALLERY_DIALOG_ACTION_CANCEL),
        default: ui_widgets_fluent::ContentDialogButton::Primary,
    },
    FluentPreview {
        caption: Msg::GALLERY_DIALOG_FLUENT_CAPTION_NONE,
        title: Some(Msg::GALLERY_DIALOG_TITLE_RESET),
        body: Msg::GALLERY_DIALOG_BODY_RESET,
        primary: Some(Msg::GALLERY_DIALOG_ACTION_RESET),
        secondary: Some(Msg::GALLERY_DIALOG_ACTION_LATER),
        close: Some(Msg::GALLERY_DIALOG_ACTION_CANCEL),
        default: ui_widgets_fluent::ContentDialogButton::None,
    },
    FluentPreview {
        caption: Msg::GALLERY_DIALOG_FLUENT_CAPTION_CLOSE,
        title: None,
        body: Msg::GALLERY_DIALOG_BODY_UNDONE,
        primary: None,
        secondary: None,
        close: Some(Msg::GALLERY_DIALOG_ACTION_OK),
        default: ui_widgets_fluent::ContentDialogButton::None,
    },
];

pub(crate) fn fluent_preview_id(k: usize) -> Id {
    Id::of(("fluent-dialog-preview", k))
}

pub(crate) fn fluent_dialog_id() -> Id {
    Id::str("fluent-dialog")
}

pub(crate) fn fluent_dialog_open_id() -> Id {
    Id::str("fluent-dialog-open")
}

/// Whether `id` names one of this column's dialogs; an action arrives as the payload instead.
pub(crate) fn fluent_dialog_is_ours(id: Id) -> bool {
    id == fluent_dialog_id() || (0..FLUENT_PREVIEWS.len()).any(|k| fluent_preview_id(k) == id)
}

/// The label behind an action id, preview or modal; `None` when the id is neither.
pub(crate) fn fluent_dialog_label_of(id: Id) -> Option<Msg> {
    let modal = fluent_dialog_id();
    if id == modal.with("primary") {
        return Some(Msg::GALLERY_DIALOG_ACTION_PUBLISH);
    }
    if id == modal.with("close") {
        return Some(Msg::GALLERY_DIALOG_ACTION_CANCEL);
    }
    FLUENT_PREVIEWS.iter().enumerate().find_map(|(k, preview)| {
        let base = fluent_preview_id(k);
        if id == base.with("primary") {
            preview.primary
        } else if id == base.with("secondary") {
            preview.secondary
        } else if id == base.with("close") {
            preview.close
        } else {
            None
        }
    })
}

/// One dialog: a preview draws the surface alone, the modal adds the smoke layer.
fn dialog(
    id: Id,
    title: &str,
    body: &str,
    primary: &str,
    secondary: &str,
    close: &str,
    default: ui_widgets_fluent::ContentDialogButton,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let params = ui_widgets_fluent::ContentDialogParams::new(id, title, body)
        .primary(primary)
        .secondary(secondary)
        .close(close)
        .default_button(default)
        .target(ModalTarget::Global)
        .barrier(id == fluent_dialog_id());
    let mut scoped = cx.at_scope(style_env("fluent"));
    let control = ui_widgets_fluent::FluentKit::new().content_dialog(params, &mut scoped);
    scoped.element(control)
}

/// The live modal, declared at the window root so the engine can resolve its target.
pub(crate) fn fluent_dialog_modal(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let title = tr(cx, Msg::GALLERY_DIALOG_TITLE_PUBLISH);
    let body = tr(cx, Msg::GALLERY_DIALOG_BODY_PUBLISH);
    let primary = tr(cx, Msg::GALLERY_DIALOG_ACTION_PUBLISH);
    let close = tr(cx, Msg::GALLERY_DIALOG_ACTION_CANCEL);
    dialog(
        fluent_dialog_id(),
        &title,
        &body,
        &primary,
        "",
        &close,
        ui_widgets_fluent::ContentDialogButton::Primary,
        cx,
    )
}

/// Three previews and the opener for the live modal.
pub(crate) fn fluent_dialog_column(cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut col = Container::new(Id::of(("body", "fluent-dialogs")), NodeKind::Column).gap(10.0);
    for (k, preview) in FLUENT_PREVIEWS.iter().enumerate() {
        let id = fluent_preview_id(k);
        let title = preview.title.map(|m| tr(cx, m)).unwrap_or_default();
        let body = tr(cx, preview.body);
        let primary = preview.primary.map(|m| tr(cx, m)).unwrap_or_default();
        let secondary = preview.secondary.map(|m| tr(cx, m)).unwrap_or_default();
        let close = preview.close.map(|m| tr(cx, m)).unwrap_or_default();
        let surface = dialog(
            id,
            &title,
            &body,
            &primary,
            &secondary,
            &close,
            preview.default,
            cx,
        );
        // A row's main axis sizes a child by its content, so the surface hugs at 320 instead
        // of taking the column's cross-axis stretch the way an inline block would.
        let placed = Container::new(id.with("place"), NodeKind::Row)
            .cross_align(0.0)
            .child(surface);
        col = col.child(Box::new(captioned_row(
            id,
            Box::new(placed),
            tr(cx, preview.caption),
        )));
    }
    let opener = {
        let label = tr(cx, Msg::GALLERY_DIALOG_FLUENT_SHOW);
        let mut scoped = cx.at_scope(style_env("fluent"));
        let control = ui_widgets_fluent::FluentKit::new().button(
            ui_widgets_fluent::ButtonParams::new(fluent_dialog_open_id(), label),
            &mut scoped,
        );
        scoped.element(control)
    };
    col = col.child(Box::new(
        Container::new(Id::str("fluent-dialog-open-row"), NodeKind::Row)
            .cross_align(0.0)
            .child(opener),
    ));
    Box::new(col)
}
