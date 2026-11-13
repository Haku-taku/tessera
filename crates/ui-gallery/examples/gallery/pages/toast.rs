// The toast page.

use ui_widgets_glass::{
    GlassButton, GlassButtonParams, GlassToast, GlassToastAction, GlassToastKind, GlassToastParams,
    GlassToastPosition, HigOptions,
};

use crate::shell::*;

/// The app's own ids: the two triggers, the toast root and its action.
pub(crate) fn toast_show_id() -> Id {
    Id::of(("toast", Page::Toast.tag(), "show"))
}

pub(crate) fn toast_error_id() -> Id {
    Id::of(("toast", Page::Toast.tag(), "error"))
}

pub(crate) fn toast_id() -> Id {
    Id::of(("toast", Page::Toast.tag()))
}

pub(crate) fn toast_action_id() -> Id {
    toast_id().with("action")
}

/// Which toast the app is showing, and therefore where it sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastShown {
    Success,
    ErrorTop,
}

/// The live toast, declared at the window root: the engine places it against the window.
pub(crate) fn toast_layer(shown: ToastShown, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let scoped = cx.at_scope(style_env("glass"));
    let (message, kind, position, action) = match shown {
        ToastShown::Success => (
            Msg::GALLERY_TOAST_SAVED,
            GlassToastKind::Success,
            GlassToastPosition::Bottom,
            Some(Msg::GALLERY_TOAST_UNDO),
        ),
        ToastShown::ErrorTop => (
            Msg::GALLERY_TOAST_FAILED,
            GlassToastKind::Error,
            GlassToastPosition::Top,
            None,
        ),
    };
    let body = tr(&scoped, message);
    let mut params = GlassToastParams::new(toast_id(), &body)
        .kind(kind)
        .position(position);
    if let Some(msg) = action {
        let label = tr(&scoped, msg);
        params = params.action(GlassToastAction::new(toast_action_id(), &label));
    }
    let toast = GlassToast::new(params, cx, &pack);
    cx.element(Box::new(toast))
}

/// The page: the two triggers and the app's record of what the toast reported.
pub(crate) fn toast_page(picked: u32, dismissed: u32, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Toast.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    // MD3 and Fluent have no toast in this repository yet.
    for tag in ["md3", "fluent"] {
        row = row.child(pending_column(Page::Toast, tag, COL_W, COL_TALL_H, cx));
    }

    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut scoped = cx.at_scope(style_env("glass"));
    let mut col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);
    for (id, label_msg) in [
        (toast_show_id(), Msg::GALLERY_TOAST_SHOW),
        (toast_error_id(), Msg::GALLERY_TOAST_SHOW_ERROR),
    ] {
        let trigger_label = tr(&scoped, label_msg);
        let params = GlassButtonParams::labelled(id, None, &trigger_label);
        let mut node = scoped.at(id);
        let button = GlassButton::new_with(HigOptions::default(), params, &mut node, &pack);
        col = col.child(Box::new(captioned_row(
            id,
            scoped.element(Box::new(button)),
            tr(&scoped, label_msg),
        )));
    }
    col = col.child_decl(label(
        Id::str("toast/readout"),
        tf(
            &scoped,
            Msg::GALLERY_TOAST_READOUT,
            &[
                ("picked", arg(picked as i64)),
                ("count", arg(dismissed as i64)),
            ],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Toast,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
