// The text selection page.

use crate::shell::*;

/// Three systems' selections side by side: the engine owns the geometry, each pack the paint.
pub(crate) fn selection_page(
    styles: &[(&dyn StyleKit, &'static str, Msg); 3],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Selection.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let appearance = ui_style::StyleScope::of(cx.env()).appearance;
    let packs = [
        ui_widgets_md3::pack_of(appearance),
        ui_widgets_fluent::pack_of(appearance),
        ui_widgets_glass::pack_of(appearance),
    ];
    let ink = Color::from_rgba8(228, 232, 240, 255);
    let prose = tr(cx, Msg::GALLERY_SELECTION_PROSE);
    for ((_, tag, title), pack) in styles.iter().zip(packs.iter()) {
        let mut body =
            Decl::container(Id::of(("body", *tag, "selection")), NodeKind::Column).gap(14.0);
        for k in 0..3 {
            let id = Id::of(("selection-text", *tag, k));
            body = body.child(
                ui_widgets::Text::new(id, prose.as_str(), 14.0, ink)
                    .max_width(COL_W - 40.0)
                    .line_gap(3.0)
                    .selectable_in(pack)
                    .resolve_in(cx),
            );
        }
        row = row.child(column(
            Page::Selection,
            tag,
            *title,
            from_decl(body),
            COL_TALL_H,
            cx,
        ));
    }
    Box::new(row)
}
