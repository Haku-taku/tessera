//! The Fluent scaffold: four named regions, laid out and nothing more.

use ui_core::layout::{AxisSize, Grow};
use ui_core::{Decl, Id, NodeKind, SizeClass};
use ui_widgets::{ScaffoldSpec, Widget};

/// `NavigationView` metrics: open pane width and the `headerHeight` band.
pub const SCAFFOLD_PANE_W: f32 = 240.0;
pub const SCAFFOLD_HEADER_H: f32 = 52.0;
pub const SCAFFOLD_INLINE_H: f32 = 40.0;
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

/// Fluent page skeleton: an optional pane beside one header + content column.
pub struct FluentScaffold {
    id: Id,
    nav: Option<Decl>,
    header: Option<Decl>,
    main: Decl,
    context: Option<Decl>,
    inline: Option<Decl>,
    overlay: Option<Decl>,
}

impl FluentScaffold {
    pub fn new(spec: &ScaffoldSpec<'_>, cx: &ui_widgets::Cx<'_, '_>) -> Self {
        Self {
            id: spec.id,
            nav: spec.nav.map(|w| w.decl(cx)),
            header: spec.header.map(|w| w.decl(cx)),
            main: spec.main.decl(cx),
            context: spec.context.map(|w| w.decl(cx)),
            inline: spec.inline.map(|w| w.decl(cx)),
            overlay: spec.overlay.map(|w| w.decl(cx)),
        }
    }
}

impl Widget for FluentScaffold {
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
        // The header is inside the content column, so it starts after the pane, not above it.
        let mut area = Decl::container(id.with("content"), NodeKind::Column).size(scaffold_size(
            fill(),
            fill(),
            Grow::Yes,
            Grow::Yes,
        ));
        if let Some(header) = &self.header {
            area = area.child(scaffold_region(
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
        area = area.child(main);
        let mut root = Decl::container(id, NodeKind::Row).size(scaffold_size(
            fill(),
            fill(),
            Grow::Yes,
            Grow::Yes,
        ));
        if let Some(nav) = &self.nav {
            root = root.child(scaffold_region(
                id.with("nav"),
                nav,
                scaffold_size(AxisSize::fixed(SCAFFOLD_PANE_W), fill(), Grow::No, Grow::No),
            ));
        }
        root = root.child(area);
        if let Some(context) = &self.context {
            root = root.child(scaffold_region(
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
        root
    }
}
