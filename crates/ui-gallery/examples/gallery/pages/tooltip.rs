// The tooltip page.

use ui_core::layout::Anchor;

use crate::app::Gallery;
use crate::shell::*;

/// Hover targets per column; the k-th names its button and its tooltip text below.
pub(crate) const TIPS: usize = 3;

/// The packs on this page. A hover episode is keyed by `column * TIPS + k`.
pub(crate) const TIP_COLUMNS: usize = 2;
/// Fluent is the first column and MD3 the second; their indexes key the hover episode.
pub(crate) const FLUENT_TIPS: usize = 0;
pub(crate) const MD3_TIPS: usize = 1;

pub(crate) const TIP_LABEL: [Msg; TIPS] = [
    Msg::GALLERY_TOOLTIP_BTN_SAVE,
    Msg::GALLERY_TOOLTIP_BTN_DELETE,
    Msg::GALLERY_TOOLTIP_BTN_LONG,
];

pub(crate) const TIP_TEXT: [Msg; TIPS] = [
    Msg::GALLERY_TOOLTIP_SAVE,
    Msg::GALLERY_TOOLTIP_DELETE,
    Msg::GALLERY_TOOLTIP_LONG,
];

pub(crate) fn tip_target_id(column: usize, k: usize) -> Id {
    Id::of(("tip-target", column, k))
}

pub(crate) fn tip_panel_id(column: usize, k: usize) -> Id {
    Id::of(("tip-panel", column, k))
}

/// The hover episode a column's k-th target belongs to.
pub(crate) fn tip_slot(column: usize, k: usize) -> usize {
    column * TIPS + k
}

/// The rich sample: its trigger, the panel the app opens, and the action that closes it.
pub(crate) fn rich_tip_trigger_id() -> Id {
    Id::str("tip/rich/trigger")
}

pub(crate) fn rich_tip_panel_id() -> Id {
    Id::str("tip/rich/panel")
}

pub(crate) fn rich_tip_action_id() -> Id {
    Id::str("tip/rich/action")
}

/// Fluent and MD3 tooltips on the same three buttons: each panel opens above its target
/// after the hover delay.
pub(crate) fn tooltip_page(g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::ToolTip.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    row = row.child(fluent_column(g, cx));
    row = row.child(md3_column(g, cx));
    Box::new(row)
}

/// The two captions above the buttons, which explain what the delay does.
fn captions(tag: &'static str, caption: Msg, cx: &Cx<'_, '_>) -> Container {
    let mut body = Container::new(Id::of(("body", tag)), NodeKind::Column).gap(12.0);
    body = body.child_decl(label(
        Id::of(("tips", tag, "caption")),
        tr(cx, caption),
        12.0,
        muted(),
        700.0,
    ));
    body.child_decl(label(
        Id::of(("tips", tag, "delay")),
        tr(cx, Msg::GALLERY_TOOLTIP_DELAY),
        12.0,
        muted(),
        700.0,
    ))
}

/// A coloured stage behind the buttons: acrylic over a flat card shows only its tint,
/// and the blur it adds needs something with structure underneath.
fn stage(tag: &'static str, line: Container) -> Container {
    Container::from_decl(
        Decl::container(Id::of(("tips", tag, "stage")), NodeKind::Box)
            .size(SizeClass::fixed(700.0, 120.0))
            .corners(Corners::all(12.0))
            .background(Color::from_rgba8(24, 54, 110, 255), Corners::all(12.0))
            .gradient(
                Color::from_rgba8(24, 54, 110, 255),
                Color::from_rgba8(16, 22, 46, 255),
                GradientAxis::Vertical,
            )
            .padding(Insets::all(20.0)),
    )
    .child(Box::new(line))
}

fn fluent_column(g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("fluent"));
    let mut body = captions("fluent", Msg::GALLERY_TOOLTIP_CAPTION, &scoped);
    let mut line = Container::new(Id::of(("tips", "fluent", "row")), NodeKind::Row).gap(8.0);
    for k in 0..TIPS {
        let label = tr(&scoped, TIP_LABEL[k]);
        let params = ui_widgets_fluent::ButtonParams::new(tip_target_id(FLUENT_TIPS, k), label);
        let control = g.fluent.button(params, &mut scoped);
        line = line.child(scoped.element(control));
        // The panel lives beside the buttons, not inside them: an overlay is deferred and
        // anchored, so it escapes the card instead of being clipped by it.
        if let Some(open) = g.tip_state(tip_slot(FLUENT_TIPS, k)) {
            let text = tr(&scoped, TIP_TEXT[k]);
            let params = ui_widgets_fluent::ToolTipParams::new(
                tip_panel_id(FLUENT_TIPS, k),
                &text,
                Anchor::Node(tip_target_id(FLUENT_TIPS, k)),
            )
            .open(open);
            let control = g.fluent.tool_tip(params, &mut scoped);
            body = body.child(scoped.element(control));
        }
    }
    body = body.child(Box::new(stage("fluent", line)));
    column_w(
        Page::ToolTip,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(body),
        760.0,
        COL_CTRL_H,
        cx,
    )
}

fn md3_column(g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut body = captions("md3", Msg::GALLERY_TOOLTIP_CAPTION_MD3, &scoped);
    let mut line = Container::new(Id::of(("tips", "md3", "row")), NodeKind::Row).gap(8.0);
    for k in 0..TIPS {
        let label = tr(&scoped, TIP_LABEL[k]);
        let params = ui_widgets_md3::ButtonParams::new(tip_target_id(MD3_TIPS, k), label);
        let control = g.md3.button(params, &mut scoped);
        line = line.child(scoped.element(control));
        // An MD3 panel is declared only while it shows: removal is what runs its exit fade.
        if g.tip_state(tip_slot(MD3_TIPS, k)) == Some(true) {
            // The long sample names the cap its own system wraps at.
            let text = tr(
                &scoped,
                if k == TIPS - 1 {
                    Msg::GALLERY_TOOLTIP_LONG_MD3
                } else {
                    TIP_TEXT[k]
                },
            );
            let params = ui_widgets_md3::ToolTipParams::new(
                tip_panel_id(MD3_TIPS, k),
                &text,
                Anchor::Node(tip_target_id(MD3_TIPS, k)),
            );
            let control = g.md3.tool_tip(params, &mut scoped);
            body = body.child(scoped.element(control));
        }
    }
    body = body.child(Box::new(stage("md3", line)));
    body = body.child(Box::new(rich_sample(g, &mut scoped)));
    column_w(
        Page::ToolTip,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(body),
        760.0,
        COL_CTRL_H,
        cx,
    )
}

/// The rich sample under the plain buttons: its own caption and trigger.
fn rich_sample(g: &Gallery, scoped: &mut Cx<'_, '_>) -> Container {
    let mut block = Container::new(Id::of(("tips", "md3-rich")), NodeKind::Column).gap(12.0);
    block = block.child_decl(label(
        Id::of(("tips", "md3-rich", "caption")),
        tr(scoped, Msg::GALLERY_TOOLTIP_CAPTION_RICH),
        12.0,
        muted(),
        700.0,
    ));
    block = block.child_decl(label(
        Id::of(("tips", "md3-rich", "hint")),
        tr(scoped, Msg::GALLERY_TOOLTIP_RICH_HINT),
        12.0,
        muted(),
        700.0,
    ));
    let params = ui_widgets_md3::ButtonParams::new(
        rich_tip_trigger_id(),
        tr(scoped, Msg::GALLERY_TOOLTIP_RICH_OPEN),
    );
    let mut line = Container::new(Id::of(("tips", "md3-rich", "row")), NodeKind::Row).gap(8.0);
    let control = g.md3.button(params, scoped);
    line = line.child(scoped.element(control));
    block = block.child(Box::new(line));
    // The app owns the panel: it is declared only while open, so removal runs the exit fade.
    if g.rich_tip_open {
        let subhead = tr(scoped, Msg::GALLERY_TOOLTIP_RICH_SUBHEAD);
        let text = tr(scoped, Msg::GALLERY_TOOLTIP_RICH_TEXT);
        let action_label = tr(scoped, Msg::GALLERY_TOOLTIP_RICH_ACTION);
        let actions = [ui_widgets_md3::RichToolTipAction::new(
            rich_tip_action_id(),
            &action_label,
        )];
        let params = ui_widgets_md3::RichToolTipParams::new(
            rich_tip_panel_id(),
            &text,
            Anchor::Node(rich_tip_trigger_id()),
        )
        .subhead(&subhead)
        .actions(&actions);
        let control = g.md3.rich_tool_tip(params, scoped);
        block = block.child(scoped.element(control));
    }
    block
}
