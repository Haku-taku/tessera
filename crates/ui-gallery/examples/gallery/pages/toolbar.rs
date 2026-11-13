// The toolbar page.

use super::glyphs::*;
use crate::shell::*;

/// Members per bar; which one is toggled on belongs to the app.
pub(crate) const TOOLBAR_ACTIONS: usize = 4;

pub(crate) fn toolbar_action_id(tag: &'static str, k: usize) -> Id {
    Id::of(("toolbaraction", tag, k))
}

pub(crate) fn toolbar_id(tag: &'static str) -> Id {
    Id::of(("toolbar", tag))
}

/// The trailing FAB of a floating bar; docked bars have none.
pub(crate) fn toolbar_fab_id(tag: &'static str) -> Id {
    Id::of(("toolbarfab", tag))
}

/// Whether `id` is one of the floating bars' FABs.
pub(crate) fn toolbar_fab_hit(id: Id) -> bool {
    ["floating-standard", "floating-vibrant"]
        .into_iter()
        .any(|tag| toolbar_fab_id(tag) == id)
}

/// The member index an id names, across all four bars.
pub(crate) fn toolbar_pick(id: Id) -> Option<usize> {
    [
        "docked-standard",
        "docked-vibrant",
        "floating-standard",
        "floating-vibrant",
    ]
    .into_iter()
    .find_map(|tag| (0..TOOLBAR_ACTIONS).find(|k| toolbar_action_id(tag, *k) == id))
}

/// MD3 toolbars: docked and floating, each in the standard and vibrant themes.
pub(crate) fn toolbar_page(selected: Option<usize>, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    use ui_widgets_md3::{ToolbarTheme as T, ToolbarVariant as V};
    let mut row = Container::new(Id::of(("bodyrow", Page::Toolbar.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let icons: [IconSource; TOOLBAR_ACTIONS] =
        [favorite_glyph(), share_glyph(), tune_glyph(), more_glyph()];
    let mut scoped = cx.at_scope(style_env("md3"));
    // Floating bars hug their members, so the column must not stretch them.
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column)
        .cross_align(0.0)
        .gap(16.0);
    for (tag, variant, theme, kind, tone) in [
        (
            "docked-standard",
            V::Docked,
            T::Standard,
            "docked",
            "standard",
        ),
        ("docked-vibrant", V::Docked, T::Vibrant, "docked", "vibrant"),
        (
            "floating-standard",
            V::Floating,
            T::Standard,
            "floating",
            "standard",
        ),
        (
            "floating-vibrant",
            V::Floating,
            T::Vibrant,
            "floating",
            "vibrant",
        ),
    ] {
        let actions: Vec<ui_widgets_md3::ToolbarAction> = (0..TOOLBAR_ACTIONS)
            .map(|k| ui_widgets_md3::ToolbarAction {
                id: toolbar_action_id(tag, k),
                icon: icons[k],
                selected: selected == Some(k),
                label: None,
            })
            .collect();
        // Only a floating bar draws a FAB; the docked samples carry one so the sample
        // shows the bar ignoring it rather than merely omitting it.
        let fab = Some(ui_widgets_md3::ToolbarFab::new(
            toolbar_fab_id(tag),
            add_glyph(),
        ));
        let control = style.toolbar(
            ui_widgets_md3::ToolbarParams {
                id: toolbar_id(tag),
                variant,
                theme,
                actions: &actions,
                fab,
                disabled: false,
            },
            &mut scoped,
        );
        let caption = tf(
            &scoped,
            Msg::GALLERY_TOOLBAR_CAPTION,
            &[("kind", arg(kind)), ("theme", arg(tone))],
        );
        col = col.child(Box::new(captioned_row(
            Id::of(("toolbarcap", tag)),
            scoped.element(control),
            caption,
        )));
    }
    col = col.child_decl(label(
        Id::str("toolbar/note"),
        tr(&scoped, Msg::GALLERY_TOOLBAR_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Toolbar,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_MID_H,
        &scoped,
    ));
    Box::new(add_pending_pair(row, Page::Toolbar, COL_W, COL_MID_H, cx))
}
