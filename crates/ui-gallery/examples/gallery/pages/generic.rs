// The generic pack-driven page.

use crate::shell::*;
use ui_core::tree::{ExitSpace, Presence};

/// The button that hides and shows the collapsing banner.
pub(crate) fn banner_toggle_id() -> Id {
    Id::str("generic/banner-toggle")
}

/// The button that adds and removes the card's line.
pub(crate) fn note_toggle_id() -> Id {
    Id::str("generic/note-toggle")
}

/// A pack button with a click to report: a bare `Decl` is inert, so the toggle is a widget.
struct PackToggle {
    id: Id,
    label: String,
}

impl Widget for PackToggle {
    fn decl(&self, cx: &Cx<'_, '_>) -> Decl {
        let pack = ui_style::legacy_pack();
        let state = cx.state(self.id);
        let hover = if state.hovered || state.pressed {
            1.0
        } else {
            0.0
        };
        ui_widgets::Button::new(self.id, self.label.clone()).resolve(state, &pack, hover)
    }

    fn change(&self, state: &ui_widgets::WidgetState, _bounds: Rect) -> Option<ui_widgets::Change> {
        state.activation().then_some(ui_widgets::Change::Activated)
    }
}

/// The pack-driven widgets: no style of their own, everything read from one pack.
pub(crate) fn generic_page(
    heights: &std::cell::RefCell<ui_widgets::RowHeights>,
    banner_open: bool,
    note_open: bool,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let pack = ui_style::legacy_pack();
    let mut row = Container::new(Id::of(("bodyrow", Page::Generic.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    // The button's fade is advanced through the node context, using the pack's own token.
    let button_id = Id::str("generic/button");
    let state = cx.state(button_id);
    let spec = pack.motion.get(ui_style::MotionName::StateChange).spec;
    let hover = {
        let mut node = cx.at(button_id);
        node.animate(
            button_id.with("hover"),
            if state.hovered { 1.0 } else { 0.0 },
            spec,
        )
    };
    let button = ui_widgets::Button::new(button_id, tr(cx, Msg::GALLERY_GENERIC_BUTTON))
        .resolve(state, &pack, hover);
    let card = ui_widgets::Card {
        id: Id::str("generic/card"),
        title: Some(tr(cx, Msg::GALLERY_GENERIC_CARD_TITLE)),
        title_ink: None,
        pack: &pack,
    }
    .resolve(vec![button]);
    row = row.child(column(
        Page::Generic,
        "card",
        Msg::GALLERY_GENERIC_COLUMN_CARD,
        from_decl(card),
        COL_CTRL_H,
        cx,
    ));

    let list = ui_widgets::List {
        id: Id::str("generic/list"),
        width: 300.0,
        height: 180.0,
        row_height: 22.0,
        // A thousand rows: the list declares only the eight or so its viewport shows, so
        // this sample is also the long-list case the scroll container is built for.
        rows: (0..1_000)
            .map(|i| tf(cx, Msg::GALLERY_GENERIC_ROW, &[("index", arg(i as i64))]))
            .collect(),
        pack: &pack,
    }
    .resolve_in(cx, &mut heights.borrow_mut());
    row = row.child(column(
        Page::Generic,
        "list",
        Msg::GALLERY_GENERIC_COLUMN_LIST,
        from_decl(list),
        COL_CTRL_H,
        cx,
    ));

    let panel = ui_widgets::Panel {
        id: Id::str("generic/panel"),
        size: (260.0, 140.0),
        offset: (0.0, 0.0),
        dragging: false,
        title: tr(cx, Msg::GALLERY_GENERIC_PANEL_TITLE).into(),
        hint: tr(cx, Msg::GALLERY_GENERIC_PANEL_HINT).into(),
        title_ink: None,
        hint_ink: None,
        pack: &pack,
    }
    .resolve();
    row = row.child(column(
        Page::Generic,
        "panel",
        Msg::GALLERY_GENERIC_COLUMN_PANEL,
        from_decl(panel),
        COL_CTRL_H,
        cx,
    ));

    // A paged strip: every page is the strip's own width, so a wheel notch or a released
    // drag turns exactly one card and the strip rests on it.
    let page_width = 360.0;
    let page_height = 120.0;
    // The same roles the list reads, so a page is a visible card on any pack.
    let role = |name: &'static str, fallback: Color| {
        pack.roles.get(ui_style::RoleName(name)).unwrap_or(fallback)
    };
    let fill = role("scrollerFill", Color::TRANSPARENT);
    let ink = role("rowText", Color::WHITE);
    let radius = pack.metric_or(ui_design::CommonMetric::Radius, pack.metrics.control.radius);
    let size = pack.type_table.get(ui_style::TextRole::Caption).size;
    let mut strip =
        ui_widgets::Scroll::horizontal(Id::str("generic/paged"), page_width, page_height)
            .snap(page_width);
    for page in 0..4 {
        let page_id = Id::of(("generic/paged/page", page));
        strip = strip.child(
            Decl::container(page_id, NodeKind::Box)
                .size(SizeClass::fixed(page_width, page_height))
                .padding(Insets::all(12.0))
                .background(fill, Corners::all(radius))
                .child(ui_widgets::leaf(
                    page_id.with("label"),
                    tf(
                        cx,
                        Msg::GALLERY_GENERIC_PAGE,
                        &[("index", arg(page as i64 + 1))],
                    )
                    .into(),
                    size,
                    ink,
                    Weight::REGULAR,
                    page_width - 24.0,
                    0.0,
                    0.0,
                )),
        );
    }
    let strip = strip.resolve_in(cx);

    // A banner whose slot collapses as it leaves, so the button under it travels up with it
    // instead of jumping once the fade lands.
    let banner_id = Id::str("generic/banner");
    let toggle = PackToggle {
        id: banner_toggle_id(),
        label: tr(
            cx,
            if banner_open {
                Msg::GALLERY_GENERIC_BANNER_HIDE
            } else {
                Msg::GALLERY_GENERIC_BANNER_SHOW
            },
        ),
    };
    let mut stack = Container::new(Id::str("generic/collapse"), NodeKind::Column).gap(8.0);
    if banner_open {
        stack = stack.child_decl(
            Decl::container(banner_id, NodeKind::Box)
                .size(SizeClass::fixed(COL_W - 24.0, 44.0))
                .padding(Insets::all(12.0))
                .background(fill, Corners::all(radius))
                .child(label(
                    banner_id.with("text"),
                    tr(cx, Msg::GALLERY_GENERIC_BANNER_TEXT),
                    size,
                    ink,
                    COL_W - 48.0,
                ))
                .exit(Presence::fade(
                    ui_core::AnimSpec::new(0.18, ui_core::Easing::Linear),
                    0.0,
                ))
                .exit_space(ExitSpace::Collapse),
        );
    }
    stack = stack.child(cx.element(Box::new(toggle)));

    // A card that hugs its content and walks the height it gains or loses: the line is
    // declared or not, and the card's own extent is a measurement either way.
    let note_toggle = PackToggle {
        id: note_toggle_id(),
        label: tr(
            cx,
            if note_open {
                Msg::GALLERY_GENERIC_NOTE_REMOVE
            } else {
                Msg::GALLERY_GENERIC_NOTE_ADD
            },
        ),
    };
    let mut note_body = Container::new(Id::str("generic/note-body"), NodeKind::Column).gap(8.0);
    if note_open {
        note_body = note_body.child_decl(label(
            Id::str("generic/note"),
            tr(cx, Msg::GALLERY_GENERIC_NOTE),
            size,
            muted(),
            COL_W - 24.0,
        ));
    }
    note_body = note_body.child(cx.element(Box::new(note_toggle)));

    let mut body =
        Container::new(Id::of(("page", Page::Generic.tag())), NodeKind::Column).gap(20.0);
    body = body
        .child(Box::new(row))
        .child(column(
            Page::Generic,
            "collapse",
            Msg::GALLERY_GENERIC_COLUMN_COLLAPSE,
            Box::new(stack),
            150.0,
            cx,
        ))
        .child(column_hug_transition(
            Page::Generic,
            "grow",
            Msg::GALLERY_GENERIC_COLUMN_GROW,
            Box::new(note_body),
            COL_W,
            ui_core::AnimSpec::new(0.2, ui_core::Easing::EaseOut),
            cx,
        ))
        .child(column(
            Page::Generic,
            "paged",
            Msg::GALLERY_GENERIC_COLUMN_PAGED,
            from_decl(strip),
            // The card carries a title above the strip, so it is taller than the pages.
            page_height + 60.0,
            cx,
        ));
    Box::new(body)
}
