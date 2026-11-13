// The chips page.

use ui_widgets_glass::{GlassChip, GlassChipParams, HigOptions};

use crate::pages::glyphs::sf_glyph;
use crate::shell::*;

/// How many filter chips the Chips page shows; each one's selection belongs to the app.
pub(crate) const CHIP_FILTERS: usize = 2;
/// The glass column's toggling chips, and the one that can be deleted.
pub(crate) const CHIP_GLASS_FILTERS: usize = 2;

pub(crate) fn chip_filter_id(k: usize) -> Id {
    Id::of(("chipfilter", k))
}

pub(crate) fn chip_glass_id(k: usize) -> Id {
    Id::of(("chipglass", k))
}

/// The chip whose delete glyph removes it; the glyph itself reports on `.with("delete")`.
pub(crate) fn chip_glass_removable_id() -> Id {
    Id::of("chipglass-delete")
}

/// One chip with its build-time axes.
pub(crate) fn chip(
    id: Id,
    label: &str,
    kind: ui_widgets_md3::ChipKind,
    variant: ui_widgets_md3::ChipVariant,
    selected: bool,
    icons: bool,
    style: &Md3Kit,
    disabled: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let env = if disabled {
        disabled_env()
    } else {
        style_env("md3")
    };
    let mut scoped = cx.at_scope(env);
    let mark = icons.then_some(ms("check"));
    let spec = ui_widgets_md3::ChipParams::new(id, label, kind)
        .variant(variant)
        .selected(selected);
    let spec = match mark {
        Some(m) => spec.lead(m).trail(m),
        None => spec,
    };
    let control = style.chip(spec, &mut scoped);
    scoped.element(control)
}

/// One glass chip: a capsule of icon, label and optional delete.
fn glass_chip(
    id: Id,
    label: &str,
    icon: Option<&str>,
    selected: bool,
    deletable: bool,
    disabled: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut scoped = cx.at_scope(if disabled {
        disabled_env()
    } else {
        style_env("glass")
    });
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut params = GlassChipParams::new(id, label).selected(selected);
    if let Some(name) = icon {
        params = params.icon(sf_glyph(name));
    }
    if deletable {
        params = params.deletable();
    }
    if disabled {
        params = params.disabled();
    }
    let control = GlassChip::new_with(HigOptions::default(), params, &mut scoped, &pack);
    scoped.element(Box::new(control))
}

/// The chip's build-time variants: kind, flat/elevated, icons and disabled.
pub(crate) fn chips_page(
    sel: [bool; CHIP_FILTERS],
    glass: [bool; CHIP_GLASS_FILTERS],
    gone: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_widgets_md3::{ChipKind as K, ChipVariant as V};
    let mut row = Container::new(Id::of(("bodyrow", Page::Chips.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    // One id per chip: duplicate siblings would silently merge into one node.
    let cid =
        |tag: &'static str, label: &'static str| Id::of(("chip", Page::Chips.tag(), tag, label));
    // Kind labels double as the chip's text and part of its id, so they must stay `&'static str`.
    let kinds = [
        (K::Assist, "Assist", Msg::GALLERY_CHIP_ASSIST),
        (K::Suggestion, "Suggestion", Msg::GALLERY_CHIP_SUGGESTION),
        (K::Filter, "Filter", Msg::GALLERY_CHIP_FILTER),
        (K::Input, "Input", Msg::GALLERY_CHIP_INPUT),
    ];
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(10.0);
    // Two per row: four chips with these labels overrun the column.
    let mut flat = Container::new(cid("flat", "col"), NodeKind::Column).gap(8.0);
    for pair in [0, 2] {
        let mut line = Container::new(
            cid("flat", if pair == 0 { "row0" } else { "row1" }),
            NodeKind::Row,
        )
        .gap(8.0);
        for (kind, tag, msg) in [kinds[pair], kinds[pair + 1]] {
            line = line.child(chip(
                cid("flat", tag),
                &tr(cx, msg),
                kind,
                V::Flat,
                false,
                false,
                &style,
                false,
                cx,
            ));
        }
        flat = flat.child(Box::new(line));
    }
    col = col.child(Box::new(captioned_row(
        cid("flat", "cap"),
        Box::new(flat),
        tr(cx, Msg::GALLERY_CHIPS_CAPTION_KINDS),
    )));
    let mut icons = Container::new(cid("icons", "col"), NodeKind::Column).gap(8.0);
    for pair in [0, 2] {
        let mut line = Container::new(
            cid("icons", if pair == 0 { "row0" } else { "row1" }),
            NodeKind::Row,
        )
        .gap(8.0);
        for (kind, tag, msg) in [kinds[pair], kinds[pair + 1]] {
            line = line.child(chip(
                cid("icons", tag),
                &tr(cx, msg),
                kind,
                V::Flat,
                false,
                true,
                &style,
                false,
                cx,
            ));
        }
        icons = icons.child(Box::new(line));
    }
    col = col.child(Box::new(captioned_row(
        cid("icons", "cap"),
        Box::new(icons),
        tr(cx, Msg::GALLERY_CHIPS_CAPTION_ICONS),
    )));
    let mut elevated = Container::new(cid("elevated", "row"), NodeKind::Row).gap(8.0);
    for (kind, tag, msg) in [kinds[0], kinds[1], kinds[2]] {
        elevated = elevated.child(chip(
            cid("elevated", tag),
            &tr(cx, msg),
            kind,
            V::Elevated,
            false,
            false,
            &style,
            false,
            cx,
        ));
    }
    col = col.child(Box::new(captioned_row(
        cid("elevated", "cap"),
        Box::new(elevated),
        tr(cx, Msg::GALLERY_CHIPS_CAPTION_ELEVATED),
    )));
    let mut selected = Container::new(cid("selected", "row"), NodeKind::Row).gap(8.0);
    for k in 0..CHIP_FILTERS {
        let msg = if sel[k] {
            Msg::GALLERY_CHIP_SELECTED
        } else {
            Msg::GALLERY_CHIP_UNSELECTED
        };
        selected = selected.child(chip(
            chip_filter_id(k),
            &tr(cx, msg),
            K::Filter,
            V::Flat,
            sel[k],
            false,
            &style,
            false,
            cx,
        ));
    }
    col = col.child(Box::new(captioned_row(
        cid("selected", "cap"),
        Box::new(selected),
        tr(cx, Msg::GALLERY_CHIPS_CAPTION_SELECTED),
    )));
    let mut disabled = Container::new(cid("disabled", "col"), NodeKind::Column).gap(8.0);
    for pair in [0, 2] {
        let mut line = Container::new(
            cid("disabled", if pair == 0 { "row0" } else { "row1" }),
            NodeKind::Row,
        )
        .gap(8.0);
        for (kind, tag, msg) in [kinds[pair], kinds[pair + 1]] {
            line = line.child(chip(
                cid("disabled", tag),
                &tr(cx, msg),
                kind,
                V::Flat,
                false,
                false,
                &style,
                true,
                cx,
            ));
        }
        disabled = disabled.child(Box::new(line));
    }
    col = col.child(Box::new(captioned_row(
        cid("disabled", "cap"),
        Box::new(disabled),
        tr(cx, Msg::GALLERY_CHIPS_CAPTION_DISABLED),
    )));
    col = col.child_decl(label(
        Id::str("chips/readout"),
        tf(
            cx,
            Msg::GALLERY_CHIPS_READOUT,
            &[("sel", arg(format!("{sel:?}")))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Chips,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    row = row.child(pending_column(Page::Chips, "fluent", COL_W, COL_TALL_H, cx));

    // ── The glass column: the reference's capsule chips, with icons, selection and delete.
    let gid = |tag: &'static str| Id::of(("chipglass", Page::Chips.tag(), tag));
    let mut glass_col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(10.0);
    let mut marks = Container::new(gid("marks"), NodeKind::Row).gap(8.0);
    for (k, name) in ["star", "tag", "bell"].into_iter().enumerate() {
        marks = marks.child(glass_chip(
            gid(match k {
                0 => "star",
                1 => "tag",
                _ => "bell",
            }),
            &tr(cx, Msg::GALLERY_CHIP_FILTER),
            Some(name),
            false,
            false,
            false,
            cx,
        ));
    }
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("marks-cap"),
        Box::new(marks),
        tr(cx, Msg::GALLERY_CHIPS_GLASS_CAPTION_MARKS),
    )));
    let mut picked = Container::new(gid("picked"), NodeKind::Row).gap(8.0);
    for k in 0..CHIP_GLASS_FILTERS {
        let msg = if glass[k] {
            Msg::GALLERY_CHIP_SELECTED
        } else {
            Msg::GALLERY_CHIP_UNSELECTED
        };
        picked = picked.child(glass_chip(
            chip_glass_id(k),
            &tr(cx, msg),
            None,
            glass[k],
            false,
            false,
            cx,
        ));
    }
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("picked-cap"),
        Box::new(picked),
        tr(cx, Msg::GALLERY_CHIPS_GLASS_CAPTION_SELECTED),
    )));
    let mut deletable = Container::new(gid("deletable"), NodeKind::Row).gap(8.0);
    if !gone {
        deletable = deletable.child(glass_chip(
            chip_glass_removable_id(),
            &tr(cx, Msg::GALLERY_CHIPS_GLASS_DELETABLE),
            Some("leaf"),
            false,
            true,
            false,
            cx,
        ));
    } else {
        deletable = deletable.child(Box::new(
            Container::new(
                Id::of(("chipglass", Page::Chips.tag(), "gone")),
                NodeKind::Box,
            )
            .size(SizeClass::fixed(1.0, 1.0)),
        ));
    }
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("deletable-cap"),
        Box::new(deletable),
        tr(cx, Msg::GALLERY_CHIPS_GLASS_CAPTION_DELETABLE),
    )));
    let mut off = Container::new(gid("off"), NodeKind::Row).gap(8.0);
    off = off.child(glass_chip(
        gid("off-chip"),
        &tr(cx, Msg::GALLERY_CHIP_FILTER),
        Some("star"),
        false,
        false,
        true,
        cx,
    ));
    glass_col = glass_col.child(Box::new(captioned_row(
        gid("off-cap"),
        Box::new(off),
        tr(cx, Msg::GALLERY_CHIPS_GLASS_CAPTION_DISABLED),
    )));
    glass_col = glass_col.child_decl(label(
        Id::str("chips/glass-readout"),
        tf(
            cx,
            Msg::GALLERY_CHIPS_GLASS_READOUT,
            &[
                ("sel", arg(format!("{glass:?}"))),
                ("gone", arg(i64::from(gone))),
            ],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Chips,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(glass_col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
