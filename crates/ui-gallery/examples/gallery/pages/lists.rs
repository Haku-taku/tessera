// The lists page.

use ui_widgets_glass::{
    GlassCard, GlassCardParams, GlassDivider, GlassListTile, GlassListTileParams,
};

use crate::pages::glyphs::sf_glyph;
use crate::shell::*;

/// The rows whose selection the app owns; each one flips on a click.
pub(crate) const LIST_SELECT: usize = 2;

pub(crate) fn list_id(tag: &'static str, k: usize) -> Id {
    Id::of(("list", Page::Lists.tag(), tag, k))
}

/// The badge and divider samples' ids; the badge carries the same tag as its row.
pub(crate) fn ornament_id(tag: &'static str) -> Id {
    Id::of(("ornament", Page::Lists.tag(), tag))
}

/// The glass rows, the group that holds them, and the standalone/still samples.
pub(crate) fn glass_tile_id(tag: &'static str) -> Id {
    Id::of(("gtile", Page::Lists.tag(), tag))
}

pub(crate) fn glass_list_group_id() -> Id {
    Id::of(("gtile-group", Page::Lists.tag()))
}

/// A stand-in for a leading icon or avatar letter: an outlined square at the slot's size.
pub(crate) fn list_mark(id: Id, ink: Color, size: f32) -> Decl {
    Decl::container(id, NodeKind::Box)
        .size(SizeClass::fixed(size, size))
        .shapes(vec![ui_core::draw::Shape::Box {
            corners: Corners::all(size * 0.25),
            fill: Color::TRANSPARENT,
            gradient: None,
            border: Some((ink, 2.0)),
            shadow: None,
            image: None,
        }])
}

/// One list item with its build-time axes.
pub(crate) fn list_row(
    params: ui_widgets_md3::ListItemParams,
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
    let control = style.list_item(params, &mut scoped);
    scoped.element(control)
}

/// The list's build-time variants: line counts, leading slots, segmented rows and disabled.
pub(crate) fn lists_page(
    sel: [bool; LIST_SELECT],
    glass_hits: u32,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_widgets_md3::{ListLeading as L, ListLineCount as N};
    let mut row = Container::new(Id::of(("bodyrow", Page::Lists.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(8.0);

    // The three line counts, each with the content that justifies its height.
    let mut lines = Container::new(list_id("lines", 0), NodeKind::Column);
    for (k, (count, label, supporting)) in [
        (N::One, Msg::GALLERY_LIST_ONE_LINE, None),
        (
            N::Two,
            Msg::GALLERY_LIST_TWO_LINES,
            Some(Msg::GALLERY_LIST_SUPPORTING),
        ),
        (
            N::Three,
            Msg::GALLERY_LIST_THREE_LINES,
            Some(Msg::GALLERY_LIST_SUPPORTING),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut item = ui_widgets_md3::ListItemParams::new(list_id("lines", k + 1), tr(cx, label))
            .lines(count);
        if let Some(text) = supporting {
            item = item.supporting(tr(cx, text));
        }
        if count == N::Three {
            item = item.overline(tr(cx, Msg::GALLERY_LIST_OVERLINE));
        }
        lines = lines.child(list_row(item, &style, false, cx));
    }
    col = col.child(Box::new(captioned_row(
        list_id("lines", 9),
        Box::new(lines),
        tr(cx, Msg::GALLERY_LISTS_CAPTION_LINES),
    )));

    // Two slots side by side: the row's own fill makes it take whatever width it is given.
    let mut leading = Container::new(list_id("leading", 0), NodeKind::Row).gap(8.0);
    for (k, (kind, label)) in [
        (L::Icon, Msg::GALLERY_LIST_ICON),
        (L::Avatar, Msg::GALLERY_LIST_AVATAR),
    ]
    .into_iter()
    .enumerate()
    {
        let item = ui_widgets_md3::ListItemParams::new(list_id("leading", k + 1), tr(cx, label))
            .leading(kind)
            .mark(list_mark);
        let half = Container::new(list_id("leading", k + 10), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::Split(1),
                grow_y: Grow::No,
            })
            .child(list_row(item, &style, false, cx));
        leading = leading.child(Box::new(half));
    }
    col = col.child(Box::new(captioned_row(
        list_id("leading", 9),
        Box::new(leading),
        tr(cx, Msg::GALLERY_LISTS_CAPTION_LEADING),
    )));

    let mut selection = Container::new(list_id("selection", 0), NodeKind::Column);
    for k in 0..LIST_SELECT {
        let item = ui_widgets_md3::ListItemParams::new(
            list_id("selection", k + 1),
            tf(cx, Msg::GALLERY_LIST_ROW, &[("index", arg(k as i64))]),
        )
        .selected(sel[k])
        .leading(L::Icon)
        .mark(list_mark);
        selection = selection.child(list_row(item, &style, false, cx));
    }
    col = col.child(Box::new(captioned_row(
        list_id("selection", 9),
        Box::new(selection),
        tr(cx, Msg::GALLERY_LISTS_CAPTION_SELECTION),
    )));

    let mut segmented = Container::new(list_id("segmented", 0), NodeKind::Column).gap(2.0);
    for k in 0..2 {
        let item = ui_widgets_md3::ListItemParams::new(
            list_id("segmented", k + 1),
            tf(cx, Msg::GALLERY_LIST_SEGMENT, &[("index", arg(k as i64))]),
        )
        .segmented(true);
        segmented = segmented.child(list_row(item, &style, false, cx));
    }
    col = col.child(Box::new(captioned_row(
        list_id("segmented", 9),
        Box::new(segmented),
        tr(cx, Msg::GALLERY_LISTS_CAPTION_SEGMENTED),
    )));

    // A badge rides the trailing slot; the divider between rows is the list's own separator.
    let pack = style.pack_for(cx);
    let mut ornaments = Container::new(ornament_id("group"), NodeKind::Column);
    for (k, (tag, label, label_text)) in [
        ("pill", Msg::GALLERY_LIST_UNREAD, Some("9+")),
        ("dot", Msg::GALLERY_LIST_DRAFTS, None),
    ]
    .into_iter()
    .enumerate()
    {
        let badge = match label_text {
            Some(text) => ui_widgets_md3::Md3Badge::with_label(ornament_id(tag), text, &pack),
            None => ui_widgets_md3::Md3Badge::dot(ornament_id(tag), &pack),
        };
        let item = ui_widgets_md3::ListItemParams::new(ornament_id(tag).with("row"), tr(cx, label))
            .slot(ui_widgets_md3::ListItemSlot::Trailing, badge.decl(cx));
        ornaments = ornaments.child(list_row(item, &style, false, cx));
        if k == 0 {
            ornaments = ornaments.child(cx.element(Box::new(ui_widgets_md3::Md3Divider::new(
                ornament_id("divider-full"),
                ui_widgets_md3::DividerAxis::Horizontal,
                Insets::ZERO,
                &pack,
            ))));
        }
    }
    // The inset variant starts where the row's text does, so it reads as part of that column.
    ornaments = ornaments.child(cx.element(Box::new(ui_widgets_md3::Md3Divider::new(
        ornament_id("divider-inset"),
        ui_widgets_md3::DividerAxis::Horizontal,
        Insets {
            left: 16.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        },
        &pack,
    ))));
    col = col.child(Box::new(captioned_row(
        ornament_id("caption"),
        Box::new(ornaments),
        tr(cx, Msg::GALLERY_LISTS_CAPTION_ORNAMENTS),
    )));

    let item = ui_widgets_md3::ListItemParams::new(
        list_id("disabled", 1),
        tr(cx, Msg::GALLERY_LIST_DISABLED_ROW),
    )
    .leading(L::Icon)
    .mark(list_mark);
    col = col.child(Box::new(captioned_row(
        list_id("disabled", 9),
        list_row(item, &style, true, cx),
        tr(cx, Msg::GALLERY_LISTS_CAPTION_DISABLED),
    )));
    row = row.child(column_hug(
        Page::Lists,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_W,
        cx,
    ));
    row = row.child(pending_column(Page::Lists, "fluent", COL_W, COL_TALL_H, cx));

    // ── The glass column: iOS grouped rows, a standalone row and a still one.
    let pack = ui_widgets_glass::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let mut glass_col = Container::new(Id::of(("body", "glass")), NodeKind::Column).gap(14.0);

    // A grouped section is a card with no inset: every row brings its own 16/12 padding.
    let face = GlassCard::new(GlassCardParams::container(glass_list_group_id()), &pack).resolve();
    let mut group = Container::from_decl(face);
    for (k, (tag, icon, title, sub)) in [
        (
            "wifi",
            "wifi",
            Msg::GALLERY_LIST_GLASS_WIFI,
            Msg::GALLERY_LIST_GLASS_WIFI_SUB,
        ),
        (
            "airplane",
            "airplane",
            Msg::GALLERY_LIST_GLASS_AIRPLANE,
            Msg::GALLERY_LIST_GLASS_AIRPLANE_SUB,
        ),
        (
            "dnd",
            "moon_fill",
            Msg::GALLERY_LIST_GLASS_DND,
            Msg::GALLERY_LIST_GLASS_DND_SUB,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if k > 0 {
            // The separator starts where the text does: 16 + a 28 dp slot + 12.
            let divider = GlassDivider::horizontal(glass_tile_id("divider").with(k), &pack).indent(
                ui_widgets_glass::LIST_PAD_X
                    + ui_widgets_glass::LIST_LEADING_MIN
                    + ui_widgets_glass::LIST_GAP,
            );
            group = group.child(cx.element(Box::new(divider)));
        }
        let title = tr(cx, title);
        let sub = tr(cx, sub);
        let params = GlassListTileParams::new(glass_tile_id(tag), &title)
            .subtitle(&sub)
            .leading(sf_glyph(icon))
            .chevron();
        let tile = GlassListTile::new(params, cx, &pack);
        group = group.child(cx.element(Box::new(tile)));
    }
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_tile_id("group-cap"),
        Box::new(group),
        tr(cx, Msg::GALLERY_LISTS_GLASS_CAPTION_GROUP),
    )));

    let title = tr(cx, Msg::GALLERY_LIST_GLASS_STANDALONE);
    let sub = tr(cx, Msg::GALLERY_LIST_GLASS_STANDALONE_SUB);
    let standalone = GlassListTile::new(
        GlassListTileParams::new(glass_tile_id("standalone"), &title)
            .subtitle(&sub)
            .chevron()
            .standalone(),
        cx,
        &pack,
    );
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_tile_id("standalone-cap"),
        cx.element(Box::new(standalone)),
        tr(cx, Msg::GALLERY_LISTS_GLASS_CAPTION_STANDALONE),
    )));

    let face = GlassCard::new(
        GlassCardParams::container(glass_tile_id("still-group")),
        &pack,
    )
    .resolve();
    let still = GlassListTile::new(
        GlassListTileParams::new(
            glass_tile_id("still"),
            &tr(cx, Msg::GALLERY_LIST_GLASS_STILL),
        )
        .subtitle(&tr(cx, Msg::GALLERY_LIST_GLASS_STILL_SUB))
        .leading(sf_glyph("gear"))
        .still(),
        cx,
        &pack,
    );
    let mut still_group = Container::from_decl(face);
    still_group = still_group.child(cx.element(Box::new(still)));
    glass_col = glass_col.child(Box::new(captioned_row(
        glass_tile_id("still-cap"),
        Box::new(still_group),
        tr(cx, Msg::GALLERY_LISTS_GLASS_CAPTION_STILL),
    )));

    glass_col = glass_col.child_decl(label(
        Id::str("lists/glass-readout"),
        tf(
            cx,
            Msg::GALLERY_LISTS_GLASS_READOUT,
            &[("count", arg(glass_hits as i64))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Lists,
        "glass",
        Msg::GALLERY_STYLE_GLASS,
        Box::new(glass_col),
        COL_TALL_H,
        cx,
    ));
    Box::new(row)
}
