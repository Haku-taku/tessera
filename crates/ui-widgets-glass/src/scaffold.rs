//! The glass scaffold: four named regions, laid out and nothing more.

use ui_core::layout::{AxisSize, Grow};
use ui_core::{Decl, Id, NodeKind, SizeClass};
use ui_widgets::{ScaffoldSpec, Widget};

/// Composed-layout metrics: a side rail, a band and an inline strip, all optional.
pub const SCAFFOLD_RAIL_W: f32 = 88.0;
pub const SCAFFOLD_HEADER_H: f32 = 56.0;
pub const SCAFFOLD_INLINE_H: f32 = 44.0;
pub const SCAFFOLD_CONTEXT_W: f32 = 104.0;

fn scaffold_size(width: AxisSize, height: AxisSize, grow_x: Grow, grow_y: Grow) -> SizeClass {
    SizeClass {
        width,
        height,
        grow_x,
        grow_y,
    }
}

fn scaffold_region(id: Id, child: &Decl, size: SizeClass) -> Decl {
    Decl::container(id, NodeKind::Column)
        .size(size)
        .child(child.clone())
}

/// Glass page skeleton: a plain VStack plus an optional side rail and float.
pub struct GlassScaffold {
    id: Id,
    nav: Option<Decl>,
    header: Option<Decl>,
    main: Decl,
    context: Option<Decl>,
    inline: Option<Decl>,
    overlay: Option<Decl>,
    bottom: Option<Decl>,
}

impl GlassScaffold {
    pub fn new(spec: &ScaffoldSpec<'_>, cx: &ui_widgets::Cx<'_, '_>) -> Self {
        Self {
            id: spec.id,
            nav: spec.nav.map(|w| w.decl(cx)),
            header: spec.header.map(|w| w.decl(cx)),
            main: spec.main.decl(cx),
            context: spec.context.map(|w| w.decl(cx)),
            inline: spec.inline.map(|w| w.decl(cx)),
            overlay: spec.overlay.map(|w| w.decl(cx)),
            bottom: spec.bottom.map(|w| w.decl(cx)),
        }
    }
}

impl Widget for GlassScaffold {
    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let fill = || AxisSize::fill(0.0, 0.0);
        let mut main = Decl::container(id.with("main"), NodeKind::Column).size(scaffold_size(
            fill(),
            fill(),
            Grow::Yes,
            Grow::No,
        ));
        if let Some(inline) = &self.inline {
            main = main.child(scaffold_region(
                id.with("inline"),
                inline,
                scaffold_size(
                    fill(),
                    AxisSize::fixed(SCAFFOLD_INLINE_H),
                    Grow::No,
                    Grow::No,
                ),
            ));
        }
        main = main.child(self.main.clone());
        if let Some(overlay) = &self.overlay {
            main = main.child(
                Decl::container(id.with("overlay"), NodeKind::Overlay).child(overlay.clone()),
            );
        }
        // The band spans the full width above the rail, so the rail only divides the row.
        let mut row = Decl::container(id.with("row"), NodeKind::Row).size(scaffold_size(
            fill(),
            fill(),
            Grow::No,
            Grow::Yes,
        ));
        if let Some(nav) = &self.nav {
            row = row.child(scaffold_region(
                id.with("nav"),
                nav,
                scaffold_size(AxisSize::fixed(SCAFFOLD_RAIL_W), fill(), Grow::No, Grow::No),
            ));
        }
        row = row.child(main);
        if let Some(context) = &self.context {
            row = row.child(scaffold_region(
                id.with("context"),
                context,
                scaffold_size(
                    AxisSize::fixed(SCAFFOLD_CONTEXT_W),
                    fill(),
                    Grow::No,
                    Grow::No,
                ),
            ));
        }
        // The bottom bar floats over the content at the bottom centre, as the reference's
        // `Positioned(bottom: 0)` does, so the page scrolls behind it.
        let mut body = row;
        if let Some(bottom) = &self.bottom {
            body = Decl::container(id.with("body"), NodeKind::Stack)
                .size(scaffold_size(fill(), fill(), Grow::No, Grow::Yes))
                .stack_align_axes(
                    ui_core::layout::OverlayAlign::Center,
                    ui_core::layout::OverlayAlign::End,
                )
                .child(body)
                .child(bottom.clone());
        }
        let mut root = Decl::container(id, NodeKind::Column).size(scaffold_size(
            fill(),
            fill(),
            Grow::Yes,
            Grow::Yes,
        ));
        if let Some(header) = &self.header {
            root = root.child(scaffold_region(
                id.with("header"),
                header,
                scaffold_size(
                    fill(),
                    AxisSize::fixed(SCAFFOLD_HEADER_H),
                    Grow::No,
                    Grow::No,
                ),
            ));
        }
        root.child(body)
    }
}
