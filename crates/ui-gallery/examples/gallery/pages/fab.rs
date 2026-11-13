// The FAB menu page.

use crate::shell::*;

/// How many FAB menus the page shows; each one's `open` belongs to the app.
/// Three tone samples plus the closed and the disabled one.
pub(crate) const FAB_MENUS: usize = 5;
pub(crate) const FAB_ITEMS: usize = 3;
pub(crate) const FAB_LABELS: [Msg; FAB_ITEMS] = [
    Msg::GALLERY_FAB_CREATE,
    Msg::GALLERY_FAB_SHARE,
    Msg::GALLERY_FAB_EDIT,
];

pub(crate) fn fab_menu_id(k: usize) -> Id {
    Id::of(("fab", k))
}

/// The fixed-size screen each menu floats over. Tall enough for the open menu: three 56dp
/// items, their two 4dp gaps, the 8dp gap and the 56dp anchor, plus the 16dp margins and a
/// band for the screen's own content above them.
pub(crate) const FAB_SCREEN_W: f32 = 220.0;
pub(crate) const FAB_SCREEN_H: f32 = 240.0 + 32.0 + 48.0;
/// The MD3 column of this page is wider than `COL_W` so three screens fit on a line.
pub(crate) const COL_FAB_W: f32 = 700.0;
pub(crate) const COL_FAB_SIDE_W: f32 = 232.0;
/// Tall enough for the sample row above the menu screens.
pub(crate) const COL_FAB_H: f32 = 880.0;

/// The FAB samples' ids and their order: four sizes, two more colours, one lowered and one
/// disabled, all of them the same control.
pub(crate) const FAB_SAMPLES: usize = 8;

pub(crate) fn fab_sample_id(k: usize) -> Id {
    Id::of(("fab", "sample", k))
}

/// What each screen's own content says about the variant it holds.
pub(crate) const FAB_SCREEN_NOTES: [Msg; FAB_MENUS] = [
    Msg::GALLERY_FAB_NOTE_PRIMARY,
    Msg::GALLERY_FAB_NOTE_SECONDARY,
    Msg::GALLERY_FAB_NOTE_TERTIARY,
    Msg::GALLERY_FAB_NOTE_CLOSED,
    Msg::GALLERY_FAB_NOTE_DISABLED,
];

pub(crate) fn fab_screen_id(k: usize) -> Id {
    fab_menu_id(k).with("screen")
}

/// One entry's id: derived from its menu, the way the control derives it.
pub(crate) fn fab_item_id(k: usize, j: usize) -> Id {
    fab_menu_id(k).with(("item", j))
}

/// One FAB menu with its three entries.
pub(crate) fn fab_menu(
    id: Id,
    tone: ui_widgets_md3::FabMenuColour,
    open: bool,
    style: &Md3Kit,
    disabled: bool,
    labels: &[String; FAB_ITEMS],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let env = if disabled {
        disabled_env()
    } else {
        style_env("md3")
    };
    let mut scoped = cx.at_scope(env);
    let items: Vec<ui_widgets_md3::Md3FabMenuItem<'_>> = (0..FAB_ITEMS)
        .map(|j| ui_widgets_md3::Md3FabMenuItem {
            id: id.with(("item", j)),
            label: &labels[j],
            icon: ms("add"),
        })
        .collect();
    let control = style.fab_menu(id, &items, tone, open, ms("close"), &mut scoped);
    scoped.element(control)
}

/// The FAB menu's build-time variants: tone, open state and disabled state.
pub(crate) fn fab_menu_page(
    open: [bool; FAB_MENUS],
    picked: Option<(usize, usize)>,
    presses: u32,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    use ui_widgets_md3::FabMenuColour as T;
    let mut row = Container::new(Id::of(("bodyrow", Page::FabMenu.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    let style = Md3Kit::new();
    let labels: [String; FAB_ITEMS] = std::array::from_fn(|j| tr(cx, FAB_LABELS[j]));
    let notes: [String; FAB_MENUS] = std::array::from_fn(|k| tr(cx, FAB_SCREEN_NOTES[k]));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(10.0);
    col = col.child_decl(label(
        Id::str("fab/samples-caption"),
        tr(cx, Msg::GALLERY_FAB_SAMPLES_CAPTION),
        12.0,
        muted(),
        COL_FAB_W - 24.0,
    ));
    col = col.child(Box::new(fab_samples(cx)));
    let mut grid = Container::new(Id::of(("fab", "grid")), NodeKind::Column).gap(10.0);
    let mut line = Container::new(Id::of(("fab", "line0")), NodeKind::Row).gap(8.0);
    for k in 0..FAB_MENUS {
        let tone = match k {
            0 | 3 => T::Primary,
            1 | 4 => T::Secondary,
            _ => T::Tertiary,
        };
        let menu = fab_menu(fab_menu_id(k), tone, open[k], &style, k == 4, &labels, cx);
        line = line.child(fab_screen(k, &notes[k], menu, cx));
        // Three per line: the screens are wide enough for a pill, and two lines hold five.
        if k == 2 {
            grid = grid.child(Box::new(line));
            line = Container::new(Id::of(("fab", "line1")), NodeKind::Row).gap(8.0);
        }
    }
    grid = grid.child(Box::new(line));
    col = col.child(Box::new(grid));
    let readout = match picked {
        // Menus 3 and 4 repeat tones 0 and 1; index the tone by menu.
        Some((k, j)) => {
            let tone = match k {
                0 | 3 => Msg::GALLERY_FAB_TONE_PRIMARY,
                1 | 4 => Msg::GALLERY_FAB_TONE_SECONDARY,
                _ => Msg::GALLERY_FAB_TONE_TERTIARY,
            };
            tf(
                cx,
                Msg::GALLERY_FAB_READOUT_PICKED,
                &[
                    ("tone", arg(tr(cx, tone))),
                    ("item", arg(tr(cx, FAB_LABELS[j]))),
                ],
            )
        }
        None => tr(cx, Msg::GALLERY_FAB_READOUT_NONE),
    };
    col = col.child_decl(label(
        Id::str("fab/readout"),
        readout,
        12.0,
        muted(),
        COL_FAB_W - 24.0,
    ));
    col = col.child_decl(label(
        Id::str("fab/samples-readout"),
        tf(
            cx,
            Msg::GALLERY_FAB_READOUT,
            &[("count", arg(presses as i64))],
        ),
        12.0,
        muted(),
        COL_FAB_W - 24.0,
    ));
    row = row.child(column_w(
        Page::FabMenu,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        COL_FAB_W,
        COL_FAB_H,
        cx,
    ));
    for tag in ["fluent", "glass"] {
        row = row.child(pending_column(
            Page::FabMenu,
            tag,
            COL_FAB_SIDE_W,
            COL_FAB_H,
            cx,
        ));
    }
    Box::new(row)
}

/// The sample row: one FAB per size, then the other colour styles.
pub(crate) fn fab_samples(cx: &mut Cx<'_, '_>) -> Container {
    use ui_widgets_md3::{FabColor, FabParams, FabSize};
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(cx.env()).appearance);
    let mut scoped = cx.at_scope(style_env("md3"));
    let samples: [(FabSize, FabColor, bool, bool); FAB_SAMPLES] = [
        (FabSize::Small, FabColor::PrimaryContainer, false, false),
        (FabSize::Base, FabColor::PrimaryContainer, false, false),
        (FabSize::Medium, FabColor::PrimaryContainer, false, false),
        (FabSize::Large, FabColor::PrimaryContainer, false, false),
        (FabSize::Base, FabColor::Primary, false, false),
        (FabSize::Base, FabColor::Tertiary, false, false),
        (FabSize::Base, FabColor::Surface, true, false),
        (FabSize::Base, FabColor::PrimaryContainer, false, true),
    ];
    let mut row = Container::new(Id::of(("fab", "samples")), NodeKind::Row)
        .gap(16.0)
        .cross_align(0.0)
        .cross_pos(1.0);
    for (k, (size, color, lowered, disabled)) in samples.into_iter().enumerate() {
        let params = FabParams::new(fab_sample_id(k), ms("add"))
            .size(size)
            .color(color)
            .lowered(lowered)
            .disabled(disabled);
        let fab = ui_widgets_md3::Md3Fab::new_with(params, &mut scoped, pack);
        row = row.child(scoped.element(Box::new(fab)));
    }
    row
}

/// A fixed-size stand-in for the screen a FAB menu floats over: the app's own content sits at
/// the top, the anchor at the bottom-right, which is where MD3 places it.
pub(crate) fn fab_screen(
    k: usize,
    note: &str,
    menu: Box<dyn Element>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    // `surface` rather than a container tier: it is the app's own background, and it reads as a
    // separate surface against the column behind it in both appearances.
    let appearance = ui_style::StyleScope::of(cx.env()).appearance;
    let surface = ui_widgets_md3::pack_of(appearance)
        .ink("surface")
        .unwrap_or(Color::from_rgba8(30, 32, 40, 255));
    let body = Container::new(fab_screen_id(k).with("body"), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .child_decl(label(
            fab_screen_id(k).with("note"),
            note.to_string(),
            11.0,
            muted(),
            FAB_SCREEN_W - 32.0,
        ));
    let grower = Decl::container(fab_screen_id(k).with("fill"), NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    });
    Box::new(
        Container::new(fab_screen_id(k), NodeKind::Column)
            .size(SizeClass::fixed(FAB_SCREEN_W, FAB_SCREEN_H))
            .padding(Insets::all(16.0))
            .background(surface, Corners::all(12.0))
            // Children hug, so the cross-axis position is what pins the anchor to the right.
            .cross_align(0.0)
            .cross_pos(1.0)
            .gap(0.0)
            .child(Box::new(body))
            .child_decl(grower)
            .child(menu),
    )
}
