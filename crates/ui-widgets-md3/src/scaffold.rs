//! The MD3 scaffold: six named regions, laid out and nothing more.

use ui_core::layout::{AxisSize, Grow};
use ui_core::{Decl, Id, NodeKind, SizeClass};
use ui_widgets::{ScaffoldSpec, Widget};

/// MD3 `Scaffold` slot widths and heights; the style's own breakpoint-free metrics.
pub const SCAFFOLD_NAV_W: f32 = 80.0;
pub const SCAFFOLD_HEADER_H: f32 = 64.0;
pub const SCAFFOLD_INLINE_H: f32 = 48.0;
pub const SCAFFOLD_CONTEXT_W: f32 = 96.0;

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

/// MD3 page skeleton: named slots beside and above one flexible content column.
pub struct Md3Scaffold {
    id: Id,
    nav: Option<Decl>,
    header: Option<Decl>,
    main: Decl,
    context: Option<Decl>,
    inline: Option<Decl>,
    overlay: Option<Decl>,
    bottom: Option<Decl>,
}

impl Md3Scaffold {
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

impl Widget for Md3Scaffold {
    fn decl(&self, _cx: &ui_widgets::Cx<'_, '_>) -> Decl {
        let id = self.id;
        let fill = || AxisSize::fill(0.0, 0.0);
        let mut main = Decl::container(id.with("main"), NodeKind::Column).size(scaffold_size(
            fill(),
            fill(),
            Grow::No,
            Grow::Yes,
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
        let mut content = Decl::container(id.with("content"), NodeKind::Column)
            .size(scaffold_size(fill(), fill(), Grow::Yes, Grow::No));
        if let Some(header) = &self.header {
            content = content.child(scaffold_region(
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
        content = content.child(main);
        let mut body = Decl::container(id.with("body"), NodeKind::Row).size(scaffold_size(
            fill(),
            fill(),
            Grow::No,
            Grow::Yes,
        ));
        if let Some(nav) = &self.nav {
            body = body.child(scaffold_region(
                id.with("nav"),
                nav,
                scaffold_size(AxisSize::fixed(SCAFFOLD_NAV_W), fill(), Grow::No, Grow::No),
            ));
        }
        body = body.child(content);
        if let Some(context) = &self.context {
            body = body.child(scaffold_region(
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
        let mut root = Decl::container(id, NodeKind::Column)
            .size(scaffold_size(fill(), fill(), Grow::Yes, Grow::Yes))
            .child(body);
        // MD3's bottom bar is docked, not floating: it takes a slot below the body.
        if let Some(bottom) = &self.bottom {
            root = root.child(bottom.clone());
        }
        root
    }
}
