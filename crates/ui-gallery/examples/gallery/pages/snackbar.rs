// The snackbar page.

use crate::shell::*;

/// Snackbar samples; the app owns when each shows and what its action did.
pub(crate) const SNACKBARS: usize = 4;

pub(crate) fn snack_id(k: usize) -> Id {
    Id::of(("snackbar", k))
}

/// The button that toggles the floating overlay snackbar sample.
pub(crate) fn snack_overlay_show_id() -> Id {
    Id::str("snackbar/overlay-show")
}

/// The sample index an id names.
pub(crate) fn snack_pick(id: Id) -> Option<usize> {
    (0..SNACKBARS).find(|k| snack_id(*k) == id)
}

/// MD3 snackbars: one-line, two-line, floating, the hidden state that paints nothing, and a
/// floating overlay sample pinned to the page bottom.
pub(crate) fn snackbar_page(
    hits: u32,
    overlay_visible: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Snackbar.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let message = tr(cx, Msg::GALLERY_SNACKBAR_MESSAGE);
    let two_lines = tr(cx, Msg::GALLERY_SNACKBAR_TWO_LINES);
    let action = tr(cx, Msg::GALLERY_SNACKBAR_ACTION);
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column)
        .cross_align(0.0)
        .gap(16.0);
    let samples: [(&'static str, Msg, &str, bool, bool, bool); SNACKBARS] = [
        (
            "one-line",
            Msg::GALLERY_SNACKBAR_ONE_LINE,
            message.as_str(),
            false,
            false,
            true,
        ),
        (
            "two-lines",
            Msg::GALLERY_SNACKBAR_TWO_LINE_CAPTION,
            two_lines.as_str(),
            true,
            false,
            true,
        ),
        (
            "floating",
            Msg::GALLERY_SNACKBAR_FLOATING,
            message.as_str(),
            false,
            true,
            true,
        ),
        (
            "hidden",
            Msg::GALLERY_SNACKBAR_HIDDEN,
            message.as_str(),
            false,
            false,
            false,
        ),
    ];
    for (k, (tag, caption, text, lines, floating, visible)) in samples.into_iter().enumerate() {
        let control = style.snackbar(
            ui_widgets_md3::SnackbarParams {
                id: snack_id(k),
                message: text,
                action: Some(action.as_str()),
                two_lines: lines,
                floating,
                width: COL_W - 24.0,
                visible,
                overlay: None,
                reveal: 1.0,
            },
            &mut scoped,
        );
        col = col.child(Box::new(captioned_row(
            Id::of(("snackcap", tag)),
            scoped.element(control),
            tr(&scoped, caption),
        )));
    }
    col = col.child_decl(label(
        Id::str("snackbar/hits"),
        tf(
            &scoped,
            Msg::GALLERY_SNACKBAR_HITS,
            &[("count", arg(format!("{hits}")))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    col = col.child_decl(label(
        Id::str("snackbar/note"),
        tr(&scoped, Msg::GALLERY_SNACKBAR_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    // A button toggles the floating overlay sample: a snackbar pinned to the window
    // bottom, out of the flow, over whatever the page shows.
    let mut controls = Container::new(Id::str("snackbar/overlay-controls"), NodeKind::Row)
        .cross_align(0.0)
        .cross_pos(0.5)
        .gap(12.0);
    let show = style.button(
        ui_widgets_md3::ButtonParams::new(
            snack_overlay_show_id(),
            tr(&scoped, Msg::GALLERY_SNACKBAR_OVERLAY_SHOW),
        )
        .style(ui_widgets_md3::ButtonStyle::Tonal)
        .size(ui_widgets_md3::ButtonSize::Base),
        &mut scoped,
    );
    controls = controls.child(scoped.element(show));
    col = col.child(Box::new(controls));
    col = col.child_decl(label(
        Id::str("snackbar/overlay-caption"),
        tr(&scoped, Msg::GALLERY_SNACKBAR_OVERLAY_CAPTION),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Snackbar,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        &scoped,
    ));
    // The floating bar is the page's last child, out of the flow, over everything below.
    let overlay = style.snackbar(
        ui_widgets_md3::SnackbarParams {
            id: Id::str("snackbar/overlay"),
            message: &message,
            action: Some(action.as_str()),
            two_lines: false,
            floating: true,
            width: COL_W - 24.0,
            visible: overlay_visible,
            overlay: Some(24.0),
            reveal: 1.0,
        },
        &mut scoped,
    );
    let overlay = scoped.element(overlay);
    let mut page = add_pending_pair(row, Page::Snackbar, COL_W, COL_TALL_H, cx);
    page = page.child(overlay);
    Box::new(page)
}
