// The layouts page.

use crate::shell::*;

pub(crate) fn scaffold_id(tag: &'static str) -> Id {
    Id::of(("scaffold", tag))
}

/// A fixed square or a region-filling block; the skeletons only place it.
pub(crate) struct Block {
    id: Id,
    caption: Msg,
    fill: Color,
    card: Option<(f32, f32)>,
    offset: Option<(f32, f32)>,
}

impl Block {
    fn slot(id: Id, caption: Msg, fill: Color) -> Self {
        Self {
            id,
            caption,
            fill,
            card: None,
            offset: None,
        }
    }

    fn floating(id: Id, caption: Msg, fill: Color, w: f32, h: f32, x: f32, y: f32) -> Self {
        Self {
            id,
            caption,
            fill,
            card: Some((w, h)),
            offset: Some((x, y)),
        }
    }
}

impl Widget for Block {
    fn decl(&self, cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let size = match self.card {
            Some((w, h)) => SizeClass::fixed(w, h),
            None => SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            },
        };
        let mut decl = Decl::container(self.id, NodeKind::Column)
            .size(size)
            .padding(Insets::all(6.0))
            .background(self.fill, Corners::all(6.0))
            .border(Color::from_rgba8(255, 255, 255, 40), 1.0)
            .child(label(
                self.id.with("label"),
                tr(cx, self.caption),
                12.0,
                Color::from_rgba8(240, 244, 252, 255),
                240.0,
            ));
        if let Some((x, y)) = self.offset {
            decl = decl.offset(x, y);
        }
        decl
    }
}

pub(crate) fn layouts_page(
    styles: &[(&dyn StyleKit, &'static str, Msg); 3],
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let mut row = Container::new(Id::of(("bodyrow", Page::Layouts.tag())), NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fill(0.0, 0.0),
            grow_x: Grow::Yes,
            grow_y: Grow::Yes,
        })
        .gap(20.0);
    for (style, tag, title) in styles {
        let nav = Block::slot(
            Id::of(("lnav", *tag)),
            Msg::GALLERY_SCAFFOLD_NAV,
            Color::from_rgba8(56, 92, 175, 255),
        );
        let header = Block::slot(
            Id::of(("lheader", *tag)),
            Msg::GALLERY_SCAFFOLD_HEADER,
            Color::from_rgba8(36, 130, 120, 255),
        );
        let main = Block::slot(
            Id::of(("lmain", *tag)),
            Msg::GALLERY_SCAFFOLD_MAIN,
            Color::from_rgba8(92, 70, 150, 255),
        );
        let context = Block::slot(
            Id::of(("lcontext", *tag)),
            Msg::GALLERY_SCAFFOLD_CONTEXT,
            Color::from_rgba8(150, 96, 48, 255),
        );
        let inline = Block::slot(
            Id::of(("linline", *tag)),
            Msg::GALLERY_SCAFFOLD_INLINE,
            Color::from_rgba8(58, 130, 70, 255),
        );
        let overlay = Block::floating(
            Id::of(("loverlay", *tag)),
            Msg::GALLERY_SCAFFOLD_OVERLAY,
            Color::from_rgba8(245, 246, 252, 240),
            150.0,
            64.0,
            36.0,
            96.0,
        );
        // One scope for the whole page, carried by the declarations it builds.
        let spec = ScaffoldSpec::main_only(scaffold_id(tag), &main)
            .nav(&nav)
            .header(&header)
            .context(&context)
            .inline(&inline)
            .overlay(&overlay);
        let scaffold = style.scaffold(&spec, cx);
        row = row.child(column(
            Page::Layouts,
            tag,
            *title,
            cx.element(scaffold),
            COL_TALL_H,
            cx,
        ));
    }
    Box::new(row)
}
