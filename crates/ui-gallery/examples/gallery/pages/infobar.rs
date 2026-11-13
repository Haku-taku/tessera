// The info bar page.

use ui_widgets_fluent::InfoBarSeverity;

use crate::app::Gallery;
use crate::shell::*;

/// The four severity samples; the last one is not closable, as the reference's error sample.
pub(crate) const INFO_BARS: usize = 4;

pub(crate) const INFO_TITLES: [Msg; INFO_BARS] = [
    Msg::GALLERY_INFOBAR_INFO_TITLE,
    Msg::GALLERY_INFOBAR_SUCCESS_TITLE,
    Msg::GALLERY_INFOBAR_WARNING_TITLE,
    Msg::GALLERY_INFOBAR_ERROR_TITLE,
];

pub(crate) const INFO_MESSAGES: [Msg; INFO_BARS] = [
    Msg::GALLERY_INFOBAR_INFO_MESSAGE,
    Msg::GALLERY_INFOBAR_SUCCESS_MESSAGE,
    Msg::GALLERY_INFOBAR_WARNING_MESSAGE,
    Msg::GALLERY_INFOBAR_ERROR_MESSAGE,
];

pub(crate) const INFO_SEVERITIES: [InfoBarSeverity; INFO_BARS] = [
    InfoBarSeverity::Informational,
    InfoBarSeverity::Success,
    InfoBarSeverity::Warning,
    InfoBarSeverity::Error,
];

pub(crate) fn info_bar_id(k: usize) -> Id {
    Id::of(("infobar", k))
}

pub(crate) fn info_bar_reset_id() -> Id {
    Id::of(("infobar", "reset"))
}

pub(crate) fn info_bar_narrow_id() -> Id {
    Id::of(("infobar", "narrow"))
}

/// A slot that pins its width and hugs its content's height.
fn width_only(width: f32) -> SizeClass {
    SizeClass {
        width: AxisSize::fixed(width),
        height: AxisSize::hug(0.0, f32::INFINITY),
        grow_x: Grow::No,
        grow_y: Grow::No,
    }
}

/// Four severities, one narrow sample that has to stack, and a button that reopens them.
pub(crate) fn info_bar_page(g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut body = Container::new(Id::of(("infobar", "body")), NodeKind::Column).gap(12.0);
    body = body.child_decl(label(
        Id::of(("infobar", "caption")),
        tr(cx, Msg::GALLERY_INFOBAR_CAPTION),
        12.0,
        muted(),
        700.0,
    ));
    let mut list = Container::new(Id::of(("infobar", "list")), NodeKind::Column)
        .gap(8.0)
        .size(width_only(700.0));
    for k in 0..INFO_BARS {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let title = tr(&scoped, INFO_TITLES[k]);
        let message = tr(&scoped, INFO_MESSAGES[k]);
        let params = ui_widgets_fluent::InfoBarParams::new(info_bar_id(k), &title, &message)
            .severity(INFO_SEVERITIES[k])
            // The error sample shows `IsClosable=false`, as the reference's does.
            .closable(k + 1 != INFO_BARS)
            .open(g.info_open[k]);
        let control = g.fluent.info_bar(params, &mut scoped);
        list = list.child(scoped.element(control));
    }
    body = body.child(Box::new(list));

    let mut scoped = cx.at_scope(style_env("fluent"));
    let reset = g.fluent.button(
        ui_widgets_fluent::ButtonParams::new(
            info_bar_reset_id(),
            tr(&scoped, Msg::GALLERY_INFOBAR_REOPEN),
        ),
        &mut scoped,
    );
    body = body.child(scoped.element(reset));

    // A 320 dp column: the message can no longer share the title's line, so the panel stacks.
    body = body.child_decl(label(
        Id::of(("infobar", "narrow-cap")),
        tr(cx, Msg::GALLERY_INFOBAR_NARROW_CAPTION),
        12.0,
        muted(),
        700.0,
    ));
    let mut scoped = cx.at_scope(style_env("fluent"));
    let title = tr(&scoped, Msg::GALLERY_INFOBAR_NARROW_TITLE);
    let message = tr(&scoped, Msg::GALLERY_INFOBAR_NARROW_MESSAGE);
    let params = ui_widgets_fluent::InfoBarParams::new(info_bar_narrow_id(), &title, &message)
        .severity(InfoBarSeverity::Success);
    let narrow = g.fluent.info_bar(params, &mut scoped);
    // The slot needs its own id: sharing the bar's would merge the two declarations.
    let mut slot = Container::new(Id::of(("infobar", "narrow-slot")), NodeKind::Column)
        .size(width_only(320.0));
    slot = slot.child(scoped.element(narrow));
    body = body.child(Box::new(slot));

    let mut row = Container::new(Id::of(("bodyrow", Page::InfoBar.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    row = row.child(column_w(
        Page::InfoBar,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(body),
        760.0,
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
