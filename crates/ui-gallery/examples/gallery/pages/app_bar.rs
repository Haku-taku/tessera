// The app bar page.

use crate::shell::*;

/// The app bar column's width; the bars themselves fill it.
pub(crate) const APP_BAR_COL: f32 = 780.0;
/// What the bars are told they will be laid out at, so their titles wrap correctly.
pub(crate) const APP_BAR_INNER: f32 = (APP_BAR_COL - 24.0 - 12.0) * 0.5;

pub(crate) fn app_bar_bar_id(k: usize) -> Id {
    Id::of(("appbar", k))
}

pub(crate) fn app_bar_lead_id(k: usize) -> Id {
    Id::of(("appbar/lead", k))
}

pub(crate) fn app_bar_action_id(k: usize, j: usize) -> Id {
    Id::of(("appbar/action", k, j))
}

pub(crate) fn app_bar_scroll_id() -> Id {
    Id::str("appbar/scrolled")
}

pub(crate) fn app_bar_collapse_id() -> Id {
    Id::str("appbar/collapsed")
}

/// The four marks as const sources, so the const `APP_BAR_SAMPLES` table can hold them.
/// The codepoints are the Material Symbols cmap values the generated table carries.
const MENU_SRC: IconSource = IconSource::Glyph {
    family: ui_core::Family::Named("Material Symbols Outlined"),
    glyph: '\u{e5d2}',
    fill: 0.0,
    weight: ui_core::Weight::REGULAR,
    grade: 0.0,
};
const SEARCH_SRC: IconSource = IconSource::Glyph {
    family: ui_core::Family::Named("Material Symbols Outlined"),
    glyph: '\u{e8b6}',
    fill: 0.0,
    weight: ui_core::Weight::REGULAR,
    grade: 0.0,
};
const MORE_SRC: IconSource = IconSource::Glyph {
    family: ui_core::Family::Named("Material Symbols Outlined"),
    glyph: '\u{e5d4}',
    fill: 0.0,
    weight: ui_core::Weight::REGULAR,
    grade: 0.0,
};
const FAVORITE_SRC: IconSource = IconSource::Glyph {
    family: ui_core::Family::Named("Material Symbols Outlined"),
    glyph: '\u{e87d}',
    fill: 0.0,
    weight: ui_core::Weight::REGULAR,
    grade: 0.0,
};

/// One sample: a variant plus the slots that variant needs. Everything here is fixed when
/// the bar is built; what the app owns is `scrolled` and `collapsed`.
pub(crate) struct AppBarSample {
    pub(crate) variant: ui_widgets_md3::AppBarVariant,
    pub(crate) title: Msg,
    pub(crate) subtitle: Option<Msg>,
    /// A 32dp avatar instead of a menu icon in the leading slot.
    pub(crate) avatar: bool,
    /// Slot names a bar reports: diagnostic ids, not UI copy, so they stay `&'static str`.
    pub(crate) actions: &'static [(&'static str, IconSource)],
    pub(crate) caption: Msg,
}

pub(crate) const APP_BAR_SAMPLES: [AppBarSample; 6] = [
    AppBarSample {
        variant: ui_widgets_md3::AppBarVariant::Small,
        title: Msg::GALLERY_APPBAR_TITLE_INBOX,
        subtitle: None,
        avatar: false,
        actions: &[("favorite", FAVORITE_SRC), ("more", MORE_SRC)],
        caption: Msg::GALLERY_APPBAR_CAPTION_SMALL,
    },
    AppBarSample {
        variant: ui_widgets_md3::AppBarVariant::Medium,
        title: Msg::GALLERY_APPBAR_TITLE_MEDIUM,
        subtitle: None,
        avatar: true,
        actions: &[("search", SEARCH_SRC), ("more", MORE_SRC)],
        caption: Msg::GALLERY_APPBAR_CAPTION_MEDIUM,
    },
    AppBarSample {
        variant: ui_widgets_md3::AppBarVariant::Large,
        title: Msg::GALLERY_APPBAR_TITLE_LARGE,
        subtitle: None,
        avatar: false,
        actions: &[("favorite", FAVORITE_SRC), ("more", MORE_SRC)],
        caption: Msg::GALLERY_APPBAR_CAPTION_LARGE,
    },
    AppBarSample {
        variant: ui_widgets_md3::AppBarVariant::MediumFlexible,
        title: Msg::GALLERY_APPBAR_TITLE_FLEXIBLE,
        subtitle: Some(Msg::GALLERY_APPBAR_SUB_MEDIUM),
        avatar: false,
        actions: &[("more", MORE_SRC)],
        caption: Msg::GALLERY_APPBAR_CAPTION_MEDIUM_FLEX,
    },
    AppBarSample {
        variant: ui_widgets_md3::AppBarVariant::LargeFlexible,
        title: Msg::GALLERY_APPBAR_TITLE_FLEXIBLE,
        subtitle: Some(Msg::GALLERY_APPBAR_SUB_LARGE),
        avatar: false,
        actions: &[("search", SEARCH_SRC), ("more", MORE_SRC)],
        caption: Msg::GALLERY_APPBAR_CAPTION_LARGE_FLEX,
    },
    AppBarSample {
        variant: ui_widgets_md3::AppBarVariant::Search,
        title: Msg::GALLERY_APPBAR_TITLE_SEARCH,
        subtitle: None,
        avatar: false,
        actions: &[("more", MORE_SRC)],
        caption: Msg::GALLERY_APPBAR_CAPTION_SEARCH,
    },
];

/// Whether a change came from one of this page's bars.
pub(crate) fn app_bar_is_ours(id: Id) -> bool {
    (0..APP_BAR_SAMPLES.len()).any(|k| app_bar_bar_id(k) == id)
}

/// The slot a bar reported, by the name the sample gave it.
pub(crate) fn app_bar_slot_name(id: Id) -> Option<&'static str> {
    for (k, sample) in APP_BAR_SAMPLES.iter().enumerate() {
        if app_bar_lead_id(k) == id {
            return Some(if sample.avatar {
                "avatar"
            } else {
                "leading icon"
            });
        }
        if let Some(j) = (0..sample.actions.len()).find(|j| app_bar_action_id(k, *j) == id) {
            return Some(sample.actions[j].0);
        }
    }
    None
}

/// MD3 top app bars: all six variants, one shared scroll-under flag and one collapse flag.
pub(crate) fn app_bar_page(
    scrolled: bool,
    collapsed: bool,
    fired: Option<&'static str>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::AppBar.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(8.0);

    let mut toggles = Container::new(Id::str("appbar/toggles"), NodeKind::Row)
        .cross_align(0.0)
        .gap(8.0);
    for (id, msg) in [
        (app_bar_scroll_id(), Msg::GALLERY_APPBAR_TOGGLE_SCROLLED),
        (app_bar_collapse_id(), Msg::GALLERY_APPBAR_TOGGLE_COLLAPSED),
    ] {
        let button = style.button(
            ui_widgets_md3::ButtonParams::new(id, tr(&scoped, msg))
                .style(ui_widgets_md3::ButtonStyle::Tonal)
                .size(ui_widgets_md3::ButtonSize::Base),
            &mut scoped,
        );
        toggles = toggles.child(scoped.element(button));
    }
    col = col.child(Box::new(toggles));

    // Two sub-columns: six bars at full panel width would not fit the page's height.
    let mut halves: [Vec<Box<dyn Element>>; 2] = [Vec::new(), Vec::new()];
    for (k, sample) in APP_BAR_SAMPLES.iter().enumerate() {
        let actions: Vec<ui_widgets_md3::AppBarAction> = sample
            .actions
            .iter()
            .enumerate()
            .map(|(j, (_, build))| ui_widgets_md3::AppBarAction {
                id: app_bar_action_id(k, j),
                icon: *build,
            })
            .collect();
        let leading = if sample.avatar {
            ui_widgets_md3::AppBarLeading::Avatar(
                Decl::container(app_bar_lead_id(k).with("avatar"), NodeKind::Column)
                    .size(SizeClass::fixed(32.0, 32.0))
                    .background(Color::from_rgba8(198, 166, 118, 255), Corners::all(16.0)),
            )
        } else {
            ui_widgets_md3::AppBarLeading::Icon(MENU_SRC)
        };
        let title = tr(&scoped, sample.title);
        let subtitle = sample.subtitle.map(|msg| tr(&scoped, msg));
        let bar = style.app_bar(
            ui_widgets_md3::AppBarParams {
                id: app_bar_bar_id(k),
                variant: sample.variant,
                title: &title,
                subtitle: subtitle.as_deref(),
                leading,
                actions: &actions,
                width: APP_BAR_INNER,
            },
            scrolled,
            collapsed,
            &mut scoped,
        );
        let mut block = Container::new(Id::of(("appbar/block", k)), NodeKind::Column).gap(4.0);
        block = block.child_decl(label(
            Id::of(("appbar/cap", k)),
            tr(&scoped, sample.caption),
            11.0,
            muted(),
            APP_BAR_INNER,
        ));
        block = block.child(scoped.element(bar));
        halves[k % 2].push(Box::new(block));
    }
    let mut grid = Container::new(Id::str("appbar/grid"), NodeKind::Row)
        .cross_align(0.0)
        .gap(12.0);
    for (i, blocks) in halves.into_iter().enumerate() {
        let mut half = Container::new(Id::of(("appbar/half", i)), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(APP_BAR_INNER),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .gap(6.0);
        for block in blocks {
            half = half.child(block);
        }
        grid = grid.child(Box::new(half));
    }
    col = col.child(Box::new(grid));

    col = col.child_decl(label(
        Id::str("appbar/readout"),
        tf(
            &scoped,
            Msg::GALLERY_APPBAR_READOUT,
            &[
                ("slot", arg(fired.unwrap_or("—"))),
                ("scrolled", arg(if scrolled { "true" } else { "false" })),
                ("collapsed", arg(if collapsed { "true" } else { "false" })),
            ],
        ),
        12.0,
        muted(),
        APP_BAR_COL - 24.0,
    ));
    row = row.child(column_w(
        Page::AppBar,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        APP_BAR_COL,
        560.0,
        &scoped,
    ));
    for tag in ["fluent", "glass"] {
        row = row.child(pending_column(
            Page::AppBar,
            tag,
            190.0,
            COL_CTRL_H,
            &scoped,
        ));
    }
    Box::new(row)
}
