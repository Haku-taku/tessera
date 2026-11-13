// The icons page.

use crate::shell::*;

/// Names shown in every icon row: outlines, filled shapes and an arrow.
/// These are lookup keys into the icon font, not UI copy, so they are not translated.
pub(crate) const ICON_NAMES: [&str; 8] = [
    "home",
    "search",
    "favorite",
    "star",
    "settings",
    "delete",
    "check_circle",
    "chevron_right",
];

pub(crate) fn icon_id(tag: &'static str) -> Id {
    Id::of(("icon", Page::Icons.tag(), tag))
}

pub(crate) fn icon_toggle_id() -> Id {
    Id::str("icons/toggle")
}

/// The heart sample: a click reports "activated"; the app owns the `FILL` target.
///
/// A bare `Decl` child is inert — only an `Element` can report a change — so the tap area
/// is a widget, not the icon's text node (text never takes input either).
pub(crate) struct TapIcon {
    id: Id,
    glyph: Id,
    fill: f32,
    ink: Color,
}

impl Widget for TapIcon {
    fn decl(&self, _cx: &Cx<'_, '_>) -> Decl {
        let heart = ui_widgets::Icon::new(self.glyph, "favorite", 48.0, self.ink)
            .fill(self.fill)
            .resolve();
        Decl::container(self.id, NodeKind::Column)
            .sense(ui_core::Sense::CLICK)
            .cursor(ui_core::CursorShape::PointingHand)
            .child(heart)
    }

    fn change(&self, state: &ui_widgets::WidgetState, _bounds: Rect) -> Option<ui_widgets::Change> {
        (state.clicked && !state.disabled).then_some(ui_widgets::Change::Activated)
    }
}

/// Material Symbols: every set, three sizes, both `FILL` ends, plus a live `FILL` animation.
pub(crate) fn icons_page(filled: bool, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let ink = match ui_style::StyleScope::of(cx.env()).appearance {
        ui_design::Appearance::Dark => Color::from_rgba8(226, 232, 244, 255),
        ui_design::Appearance::Light => Color::from_rgba8(32, 36, 44, 255),
    };
    let mut row = Container::new(Id::of(("bodyrow", Page::Icons.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);

    // `set` is the id component and `title` the column's text: the two are the same word here,
    // so one table keeps the id stable while the title goes through the catalog.
    let sets = [
        (ui_widgets::IconSet::Outlined, "outlined", "Outlined"),
        (ui_widgets::IconSet::Rounded, "rounded", "Rounded"),
        (ui_widgets::IconSet::Sharp, "sharp", "Sharp"),
    ];
    for (set, tag, title) in sets {
        let mut col = Container::new(icon_id(tag), NodeKind::Column).gap(10.0);
        for (size, fill) in [
            (20.0, 0.0),
            (24.0, 0.0),
            (24.0, 1.0),
            (40.0, 0.0),
            (40.0, 1.0),
        ] {
            let mut line = Container::new(
                Id::of(("icons/row", tag, size as u32, fill as u32)),
                NodeKind::Row,
            )
            .gap(8.0);
            for name in ICON_NAMES {
                let id = Id::of(("icons/glyph", tag, name, size as u32, fill as u32));
                line = line.child_decl(
                    ui_widgets::Icon::new(id, name, size, ink)
                        .set(set)
                        .fill(fill)
                        .resolve(),
                );
            }
            let block = Container::new(
                Id::of(("icons/block", tag, size as u32, fill as u32)),
                NodeKind::Column,
            )
            .gap(2.0)
            .child_decl(label(
                Id::of(("icons/cap", tag, size as u32, fill as u32)),
                format!("{size:.0}px · FILL {fill:.0}"),
                11.0,
                muted(),
                COL_W - 24.0,
            ))
            .child(Box::new(line));
            col = col.child(Box::new(block));
        }
        // One live sample: a click retargets the `FILL` axis and the atlas follows it.
        if tag == "outlined" {
            let id = icon_toggle_id();
            let fill = {
                let mut node = cx.at(id);
                node.animate(
                    id.with("fill"),
                    if filled { 1.0 } else { 0.0 },
                    ui_core::AnimSpec::new(0.25, ui_core::Easing::EaseOut),
                )
            };
            let toggle_caption = tr(cx, Msg::GALLERY_ICONS_CAPTION_TOGGLE);
            let tap = cx.element(Box::new(TapIcon {
                id,
                glyph: id.with("glyph"),
                fill,
                ink,
            }));
            col = col.child(Box::new(captioned_row(
                icon_id("toggle-cap"),
                tap,
                toggle_caption,
            )));
        }
        let column_title = tf(cx, Msg::GALLERY_ICONS_COLUMN, &[("set", arg(title))]);
        row = row.child(column_text(
            Page::Icons,
            tag,
            column_title,
            Box::new(col),
            COL_W,
            COL_TALL_H,
        ));
    }
    Box::new(row)
}
