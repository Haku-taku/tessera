// The carousel page.

use crate::shell::*;

/// The carousel samples are drawn at a compact width, where MD3's small items stay in range.
pub(crate) const CAROUSEL_W: f32 = 360.0;
/// One sample's height; five of them plus captions must fit the page.
pub(crate) const CAROUSEL_H: f32 = 120.0;
pub(crate) fn carousel_id(k: usize) -> Id {
    Id::of(("carousel", k))
}

pub(crate) fn carousel_item_id(k: usize, j: usize) -> Id {
    Id::of(("carousel/item", k, j))
}

pub(crate) fn carousel_prev_id(k: usize) -> Id {
    Id::of(("carousel/prev", k))
}

pub(crate) fn carousel_next_id(k: usize) -> Id {
    Id::of(("carousel/next", k))
}

pub(crate) fn carousel_offset_id(k: usize) -> Id {
    Id::of(("carousel/offset", k))
}

/// The width one page of sample `k` gives its leading item.
pub(crate) fn carousel_usable(k: usize) -> f32 {
    let sample = &CAROUSEL_SAMPLES[k];
    if sample.kind.vertical() {
        CAROUSEL_H
    } else if sample.kind.shrinking() {
        CAROUSEL_W - 2.0 * ui_widgets_md3::CAROUSEL_PAD
    } else {
        CAROUSEL_W
    }
}

/// The furthest sample `k` can scroll: the strip's own content decides it, not a constant.
pub(crate) fn carousel_end(k: usize) -> f32 {
    let sample = &CAROUSEL_SAMPLES[k];
    ui_widgets_md3::carousel_max_scroll(sample.kind, carousel_usable(k), sample.items, sample.large)
}

/// What the offset readout counts against: a looping strip's pass, not its whole canvas.
pub(crate) fn carousel_span(k: usize) -> f32 {
    let sample = &CAROUSEL_SAMPLES[k];
    if sample.infinite {
        ui_widgets_md3::carousel_cycle(sample.kind, carousel_usable(k), sample.items, sample.large)
    } else {
        carousel_end(k)
    }
}

/// Where sample `k` sits within that span.
pub(crate) fn carousel_phase(k: usize, offset: f32) -> f32 {
    let span = carousel_span(k);
    if CAROUSEL_SAMPLES[k].infinite && span > 0.0 {
        offset.rem_euclid(span)
    } else {
        offset
    }
}

/// Where sample `k` is scrolled to right now: the engine holds it, keyed by the strip's id.
pub(crate) fn carousel_offset(cx: &Cx<'_, '_>, k: usize) -> f32 {
    let geom = cx
        .core()
        .scroll_of(ui_widgets_md3::carousel_strip_id(carousel_id(k)));
    match geom {
        Some(g) if CAROUSEL_SAMPLES[k].kind.vertical() => g.offset_y,
        Some(g) => g.offset_x,
        None => 0.0,
    }
}

/// One sample: the layout, how many items it holds, and the width a large item asks for.
pub(crate) struct CarouselSample {
    pub(crate) kind: ui_widgets_md3::CarouselKind,
    pub(crate) items: usize,
    pub(crate) large: f32,
    pub(crate) infinite: bool,
    pub(crate) caption: Msg,
}

pub(crate) const CAROUSEL_SAMPLES: [CarouselSample; 6] = [
    CarouselSample {
        kind: ui_widgets_md3::CarouselKind::MultiBrowse,
        items: 6,
        large: 280.0,
        infinite: false,
        caption: Msg::GALLERY_CAROUSEL_CAPTION_MULTI,
    },
    CarouselSample {
        kind: ui_widgets_md3::CarouselKind::Hero,
        items: 4,
        large: 280.0,
        infinite: false,
        caption: Msg::GALLERY_CAROUSEL_CAPTION_HERO,
    },
    CarouselSample {
        kind: ui_widgets_md3::CarouselKind::HeroCentered,
        items: 4,
        large: 280.0,
        infinite: false,
        caption: Msg::GALLERY_CAROUSEL_CAPTION_HERO_CENTERED,
    },
    CarouselSample {
        kind: ui_widgets_md3::CarouselKind::Uncontained,
        items: 5,
        large: 280.0,
        infinite: false,
        caption: Msg::GALLERY_CAROUSEL_CAPTION_UNCONTAINED,
    },
    CarouselSample {
        kind: ui_widgets_md3::CarouselKind::FullScreen,
        items: 4,
        large: 280.0,
        infinite: false,
        caption: Msg::GALLERY_CAROUSEL_CAPTION_FULL,
    },
    CarouselSample {
        kind: ui_widgets_md3::CarouselKind::MultiBrowse,
        items: 8,
        large: 280.0,
        infinite: true,
        caption: Msg::GALLERY_CAROUSEL_CAPTION_LOOP,
    },
];

/// What one item holds: a numbered tinted panel standing in for artwork.
pub(crate) fn carousel_slide(k: usize, j: usize) -> Vec<Decl> {
    const TINTS: [(u8, u8, u8); 5] = [
        (92, 118, 168),
        (168, 122, 92),
        (94, 148, 122),
        (140, 110, 160),
        (168, 150, 92),
    ];
    let (r, g, b) = TINTS[j % TINTS.len()];
    vec![
        Decl::container(Id::of(("carousel/art", k, j)), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .padding(Insets::all(10.0))
            .background(Color::from_rgba8(r, g, b, 255), Corners::ZERO)
            .child(label(
                Id::of(("carousel/num", k, j)),
                format!("{}", j + 1),
                15.0,
                Color::from_rgba8(255, 255, 255, 235),
                CAROUSEL_W,
            )),
    ]
}

/// Which sample and item a change came from.
pub(crate) fn carousel_item_of(id: Id) -> Option<(usize, usize)> {
    CAROUSEL_SAMPLES.iter().enumerate().find_map(|(k, sample)| {
        (0..sample.items).find_map(|j| (carousel_item_id(k, j) == id).then_some((k, j)))
    })
}

/// MD3 carousels: the five layouts, each scrolled by its own strip, and the item pressed.
pub(crate) fn carousel_page(
    fired: Option<(usize, usize)>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    // Cross-aligned to the start, or the row would stretch the hugging card to its own height.
    let mut row = Container::new(Id::of(("bodyrow", Page::Carousel.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .cross_align(0.0)
        .gap(20.0);
    let style = Md3Kit::new();
    let mut scoped = cx.at_scope(style_env("md3"));
    let mut col = Container::new(Id::of(("body", "md3")), NodeKind::Column).gap(8.0);

    let last = fired
        .map(|(k, j)| {
            tf(
                &scoped,
                Msg::GALLERY_CAROUSEL_ITEM,
                &[("sample", arg(k as i64)), ("index", arg(j as i64 + 1))],
            )
        })
        .unwrap_or_else(|| "—".to_string());
    col = col.child_decl(label(
        Id::str("carousel/readout"),
        tf(
            &scoped,
            Msg::GALLERY_CAROUSEL_READOUT,
            &[("item", arg(last))],
        ),
        12.0,
        muted(),
        460.0,
    ));

    let mut halves: [Vec<Box<dyn Element>>; 2] = [Vec::new(), Vec::new()];
    for (k, sample) in CAROUSEL_SAMPLES.iter().enumerate() {
        let items: Vec<ui_widgets_md3::CarouselItem> = (0..sample.items)
            .map(|j| {
                ui_widgets_md3::CarouselItem::new(carousel_item_id(k, j), carousel_slide(k, j))
            })
            .collect();
        let usable = carousel_usable(k);
        let strip = ui_widgets_md3::carousel_strip_id(carousel_id(k));
        // A looping strip has no end to stop at: its readout counts one pass, and its
        // buttons move within the canvas the widget itself laid out.
        let raw = carousel_offset(&scoped, k);
        let offset = carousel_phase(k, raw);
        let max = carousel_span(k);
        // The buttons page this strip only: the target comes from the control's own
        // geometry, and the engine eases the strip there rather than teleporting it.
        let step = ui_widgets_md3::carousel_step(sample.kind, usable, items.len(), sample.large);
        let dir = if scoped.state(carousel_prev_id(k)).clicked {
            -1.0
        } else if scoped.state(carousel_next_id(k)).clicked {
            1.0
        } else {
            0.0
        };
        if dir != 0.0 {
            let target = if sample.infinite {
                (raw + dir * step).max(0.0)
            } else {
                (raw + dir * step).clamp(0.0, max)
            };
            if sample.kind.vertical() {
                scoped.core().scroll_to(strip, 0.0, target);
            } else {
                scoped.core().scroll_to(strip, target, 0.0);
            }
        }
        let carousel = style.carousel(
            ui_widgets_md3::CarouselParams {
                id: carousel_id(k),
                kind: sample.kind,
                items: &items,
                width: CAROUSEL_W,
                height: CAROUSEL_H,
                large: sample.large,
                infinite: sample.infinite,
            },
            &mut scoped,
        );
        let mut block = Container::new(Id::of(("carousel/block", k)), NodeKind::Column).gap(4.0);
        block = block.child_decl(label(
            Id::of(("carousel/cap", k)),
            tr(&scoped, sample.caption),
            11.0,
            muted(),
            CAROUSEL_W,
        ));
        block = block.child(scoped.element(carousel));
        let mut controls = Container::new(Id::of(("carousel/ctl", k)), NodeKind::Row)
            .cross_align(0.0)
            .cross_pos(0.5)
            .gap(8.0);
        for (id, msg) in [
            (carousel_prev_id(k), Msg::GALLERY_CAROUSEL_PREV),
            (carousel_next_id(k), Msg::GALLERY_CAROUSEL_NEXT),
        ] {
            let button = style.button(
                ui_widgets_md3::ButtonParams::new(id, tr(&scoped, msg))
                    .style(ui_widgets_md3::ButtonStyle::Tonal)
                    .size(ui_widgets_md3::ButtonSize::Small),
                &mut scoped,
            );
            controls = controls.child(scoped.element(button));
        }
        controls = controls.child_decl(label(
            carousel_offset_id(k),
            tf(
                &scoped,
                Msg::GALLERY_CAROUSEL_OFFSET,
                &[
                    ("offset", arg(format!("{offset:.0}"))),
                    ("max", arg(format!("{max:.0}"))),
                ],
            ),
            11.0,
            muted(),
            200.0,
        ));
        block = block.child(Box::new(controls));
        halves[k % 2].push(Box::new(block));
    }
    let mut grid = Container::new(Id::str("carousel/grid"), NodeKind::Row)
        .cross_align(0.0)
        .gap(12.0);
    for (i, blocks) in halves.into_iter().enumerate() {
        let mut half = Container::new(Id::of(("carousel/half", i)), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(CAROUSEL_W),
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

    row = row.child(column_hug(
        Page::Carousel,
        "md3",
        Msg::GALLERY_STYLE_MD3,
        Box::new(col),
        780.0,
        cx,
    ));
    for tag in ["fluent", "glass"] {
        row = row.child(pending_column(Page::Carousel, tag, 190.0, COL_CTRL_H, cx));
    }
    Box::new(row)
}
