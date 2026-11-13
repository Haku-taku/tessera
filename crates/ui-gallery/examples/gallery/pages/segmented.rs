// The segmented buttons page.

use ui_widgets_glass::{GlassSegment, GlassSegmentedControl, HigOptions};

use crate::pages::glyphs::sf_glyph;
use crate::shell::*;

/// Segments per group; the selection set belongs to the app.
pub(crate) const SEGMENTS: usize = 3;

pub(crate) fn seg_single_id(k: usize) -> Id {
    Id::of(("segsingle", k))
}

pub(crate) fn seg_multi_id(k: usize) -> Id {
    Id::of(("segmulti", k))
}

pub(crate) fn seg_disabled_id(k: usize) -> Id {
    Id::of(("segdisabled", k))
}

pub(crate) fn seg_glass_id(k: usize) -> Id {
    Id::of(("segglass", k))
}

pub(crate) fn seg_glass_icon_id(k: usize) -> Id {
    Id::of(("segglassicon", k))
}

fn group_id(tag: &'static str) -> Id {
    Id::of(("seg", Page::Segmented.tag(), tag))
}

/// The glass control's root id, so a self-test can reach its lens.
pub(crate) fn glass_group_id() -> Id {
    group_id("glass")
}

/// MD3 outlined segmented buttons: single-select, multi-select and disabled.
pub(crate) fn segmented_page(
    single: usize,
    multi: [bool; SEGMENTS],
    glass: usize,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Segmented.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    // Labels outlive the item lists, which borrow them.
    let labels = [
        tr(cx, Msg::GALLERY_SEGMENTED_DAY),
        tr(cx, Msg::GALLERY_SEGMENTED_WEEK),
        tr(cx, Msg::GALLERY_SEGMENTED_MONTH),
    ];
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(16.0);

    let single_items: Vec<ui_widgets_md3::Md3SegmentItem> = (0..SEGMENTS)
        .map(|k| ui_widgets_md3::Md3SegmentItem::text(seg_single_id(k), &labels[k]))
        .collect();
    let control = style.segmented_button(
        ui_widgets_md3::SegmentedParams {
            id: group_id("single"),
            items: single_items,
            selected: vec![seg_single_id(single)],
            disabled: false,
        },
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        Id::str("seg/single"),
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_SEGMENTED_SINGLE),
    )));

    let multi_items: Vec<ui_widgets_md3::Md3SegmentItem> = (0..SEGMENTS)
        .map(|k| ui_widgets_md3::Md3SegmentItem::text(seg_multi_id(k), &labels[k]))
        .collect();
    let selected: Vec<Id> = (0..SEGMENTS)
        .filter(|k| multi[*k])
        .map(seg_multi_id)
        .collect();
    let control = style.segmented_button(
        ui_widgets_md3::SegmentedParams {
            id: group_id("multi"),
            items: multi_items,
            selected,
            disabled: false,
        },
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        Id::str("seg/multi"),
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_SEGMENTED_MULTI),
    )));

    let disabled_items: Vec<ui_widgets_md3::Md3SegmentItem> = (0..SEGMENTS)
        .map(|k| ui_widgets_md3::Md3SegmentItem::text(seg_disabled_id(k), &labels[k]))
        .collect();
    let control = style.segmented_button(
        ui_widgets_md3::SegmentedParams {
            id: group_id("disabled"),
            items: disabled_items,
            selected: vec![seg_disabled_id(0)],
            disabled: true,
        },
        &mut scoped,
    );
    col = col.child(Box::new(captioned_row(
        Id::str("seg/disabled"),
        scoped.element(control),
        tr(&scoped, Msg::GALLERY_SEGMENTED_DISABLED),
    )));

    col = col.child_decl(label(
        Id::str("seg/readout"),
        tf(
            &scoped,
            Msg::GALLERY_SEGMENTED_READOUT,
            &[
                ("single", arg(format!("{single}"))),
                ("multi", arg(format!("{multi:?}"))),
            ],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    col = col.child_decl(label(
        Id::str("seg/note"),
        tr(&scoped, Msg::GALLERY_SEGMENTED_NOTE),
        11.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Segmented,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_CTRL_H,
        &scoped,
    ));
    let glass_col = glass_segmented_column(&labels, glass, cx);
    row = row.child(glass_col);
    // WinUI has no segmented control of its own; its counterpart is SelectorBar.
    Box::new(row.child(pending_column(
        Page::Segmented,
        "fluent",
        COL_W,
        COL_CTRL_H,
        cx,
    )))
}

/// The glass segmented control: a translucent track under a draggable glass lens.
fn glass_segmented_column(
    labels: &[String; SEGMENTS],
    selected: usize,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let width = COL_W - 40.0;
    let mut scoped = cx.at_scope(style_env("glass"));
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let id = group_id("glass");
    let control = {
        let segments: Vec<GlassSegment> = (0..SEGMENTS)
            .map(|k| GlassSegment::label(seg_glass_id(k), labels[k].clone()))
            .collect();
        let mut node = scoped.at(id);
        GlassSegmentedControl::new_with(
            HigOptions::default(),
            id,
            segments,
            selected,
            width,
            &mut node,
            &pack,
        )
    };
    // The same state drives an icon-only strip, so both surfaces move together.
    let icons_id = group_id("glass-icons");
    let icons = {
        let glyphs = ["list.bullet", "square.grid.2x2", "calendar"];
        let segments: Vec<GlassSegment> = (0..SEGMENTS)
            .map(|k| GlassSegment::icon(seg_glass_icon_id(k), sf_glyph(glyphs[k])))
            .collect();
        let mut node = scoped.at(icons_id);
        GlassSegmentedControl::new_with(
            HigOptions::default(),
            icons_id,
            segments,
            selected,
            width,
            &mut node,
            &pack,
        )
    };
    let readout = tf(
        &scoped,
        Msg::GALLERY_SEGMENTED_GLASS_READOUT,
        &[("index", arg(format!("{selected}")))],
    );
    let body = Box::new(
        Container::new(Id::of(("body", "glass")), NodeKind::Column)
            .gap(12.0)
            .child_decl(label(
                Id::str("seg/glass/labels"),
                tr(&scoped, Msg::GALLERY_SEGMENTED_GLASS),
                11.0,
                muted(),
                width,
            ))
            .child(scoped.element(Box::new(control)))
            .child_decl(label(
                Id::str("seg/glass/icons"),
                tr(&scoped, Msg::GALLERY_SEGMENTED_GLASS_ICONS),
                11.0,
                muted(),
                width,
            ))
            .child(scoped.element(Box::new(icons)))
            .child_decl(label(
                Id::str("seg/glass/readout"),
                readout,
                12.0,
                muted(),
                width,
            )),
    );
    column(
        Page::Segmented,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        body,
        COL_CTRL_H,
        &scoped,
    )
}
