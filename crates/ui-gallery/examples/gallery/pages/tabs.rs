// The tabs page.

use crate::shell::*;

/// The Tabs page's four bars: text-only, with icons, five tabs, and a disabled sample.
pub(crate) const TAB_BARS: usize = 4;
pub(crate) const TAB_COUNT: [usize; TAB_BARS] = [3, 3, 5, 3];
pub(crate) const TAB_TEXT: [Msg; 3] = [
    Msg::GALLERY_TAB_OVERVIEW,
    Msg::GALLERY_TAB_SPECS,
    Msg::GALLERY_TAB_REVIEWS,
];
pub(crate) const TAB_ICON: [Msg; 3] = [
    Msg::GALLERY_TAB_HOME,
    Msg::GALLERY_TAB_SEARCH,
    Msg::GALLERY_TAB_LIBRARY,
];
pub(crate) const TAB_MANY: [Msg; 5] = [
    Msg::GALLERY_TAB_ONE,
    Msg::GALLERY_TAB_TWO,
    Msg::GALLERY_TAB_THREE,
    Msg::GALLERY_TAB_FOUR,
    Msg::GALLERY_TAB_FIVE,
];

pub(crate) fn tab_bar_id(bar: usize) -> Id {
    Id::of(("tabbar", bar))
}

pub(crate) fn tab_item_id(bar: usize, k: usize) -> Id {
    Id::of(("tabitem", bar, k))
}

/// The Fluent strip's chrome and its tabs; a slot names one of `TAB_MANY`.
pub(crate) fn fluent_tabs_id() -> Id {
    Id::str("ftabs")
}

pub(crate) fn fluent_tab_id(slot: usize) -> Id {
    Id::of(("ftab", slot))
}

pub(crate) fn fluent_tab_add_id() -> Id {
    fluent_tabs_id().with("add")
}

/// The tab bar's construction-time variants: text-only, with icons, five tabs, and a disabled
/// sample. Which tab is selected is live, and each bar owns its own choice.
pub(crate) fn tabs_page(
    sel: [usize; TAB_BARS],
    fluent_slots: &[usize],
    fluent_sel: usize,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Tabs.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    // A static disabled sample needs the flag on the scope; there is no disabled ancestor.
    let disabled = Env::with(
        Some(root_env()),
        ui_widgets_md3::Md3Options {
            disabled: true,
            ..Default::default()
        },
    );
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(10.0);
    for bar in 0..TAB_BARS {
        let id = tab_bar_id(bar);
        let keys: &[Msg] = match bar {
            1 => &TAB_ICON[..],
            2 => &TAB_MANY[..],
            _ => &TAB_TEXT[..],
        };
        let labels: Vec<String> = keys.iter().map(|msg| tr(cx, *msg)).collect();
        let items: Vec<ui_widgets_md3::Md3TabItem<'_>> = labels
            .iter()
            .enumerate()
            .map(|(k, text)| {
                let id = tab_item_id(bar, k);
                if bar == 1 {
                    ui_widgets_md3::Md3TabItem::with_icon(id, text, ms("home"))
                } else {
                    ui_widgets_md3::Md3TabItem::text(id, text)
                }
            })
            .collect();
        let mut scoped = cx.at_scope(if bar == 3 {
            disabled.clone()
        } else {
            style_env("md3")
        });
        let control = style.tab_bar(id, &items, sel[bar], COL_W - 24.0, &mut scoped);
        let caption = match bar {
            0 => Msg::GALLERY_TABS_CAPTION_TEXT,
            1 => Msg::GALLERY_TABS_CAPTION_ICONS,
            2 => Msg::GALLERY_TABS_CAPTION_MANY,
            _ => Msg::GALLERY_TABS_CAPTION_DISABLED,
        };
        let sample = captioned_row(id, scoped.element(control), tr(&scoped, caption));
        col = col.child(Box::new(sample));
    }
    col = col.child_decl(label(
        Id::str("tabs/readout"),
        tf(
            cx,
            Msg::GALLERY_TABS_READOUT,
            &[("sel", arg(format!("{sel:?}")))],
        ),
        12.0,
        muted(),
        COL_W - 24.0,
    ));
    row = row.child(column(
        Page::Tabs,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_TALL_H,
        cx,
    ));
    // WinUI's strip: headers that close and an add button, the selected one joined to a
    // content slab by covering the hairline.
    let kit = ui_widgets_fluent::FluentKit::new();
    let mut fcol = Container::new(Id::of(("body", "fluent")), NodeKind::Column).gap(12.0);
    {
        let mut scoped = cx.at_scope(style_env("fluent"));
        let labels: Vec<String> = fluent_slots
            .iter()
            .map(|slot| tr(&scoped, TAB_MANY[*slot]))
            .collect();
        let items: Vec<ui_widgets_fluent::TabViewItemParams<'_>> = fluent_slots
            .iter()
            .zip(labels.iter())
            .map(|(slot, text)| {
                ui_widgets_fluent::TabViewItemParams::new(fluent_tab_id(*slot), text)
                    .closable(*slot != 0)
            })
            .collect();
        let picked = fluent_slots
            .iter()
            .position(|slot| *slot == fluent_sel)
            .unwrap_or(0);
        let strip = kit.tab_view(
            ui_widgets_fluent::TabViewParams::new(fluent_tabs_id(), items)
                .selected(picked)
                .add_button(true),
            &mut scoped,
        );
        // The template's `TabContentPresenter` paints no background, so the strip sits
        // straight on the page; an empty slab below marks where content would go.
        let caption = tr(&scoped, Msg::GALLERY_TABS_FLUENT);
        let block = Container::new(Id::str("ftabs/block"), NodeKind::Column)
            .gap(6.0)
            .child(strip)
            .child_decl(label(
                Id::str("ftabs/cap"),
                caption,
                11.0,
                muted(),
                COL_W - 24.0,
            ));
        fcol = fcol.child(Box::new(block));
    }
    row = row.child(column(
        Page::Tabs,
        "fluent",
        Msg::GALLERY_STYLE_FLUENT,
        Box::new(fcol),
        COL_TALL_H,
        cx,
    ));
    row = row.child(pending_column(Page::Tabs, "glass", COL_W, COL_TALL_H, cx));
    Box::new(row)
}
