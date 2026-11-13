//! Tests for this module, split from the source file; one module per contract area.

mod image_fit_tests {
    use crate::geom::Rect;
    use crate::tree::*;

    const TEX: (u32, u32) = (400, 300);

    #[test]
    fn no_fit_ever_samples_outside_the_texture() {
        // The invariant behind "never repeat": every source rect stays within the texture.
        let boxes = [
            Rect::new(0.0, 0.0, 10.0, 1000.0),
            Rect::new(0.0, 0.0, 1000.0, 10.0),
            Rect::new(-5.0, 7.0, 1.0, 1.0),
            Rect::new(0.0, 0.0, 400.0, 300.0),
        ];
        for fit in [ImageFit::Cover, ImageFit::Contain, ImageFit::Fill] {
            for b in boxes {
                let (_, src) = fit.place(b, TEX);
                assert!(
                    src.min_x() >= -1e-3
                        && src.min_y() >= -1e-3
                        && src.max_x() <= 400.0 + 1e-3
                        && src.max_y() <= 300.0 + 1e-3,
                    "{fit:?} on {b:?} gave {src:?}"
                );
            }
        }
    }
}

mod contracts {
    use crate::tree::*;

    fn box_node(id: &'static str, w: f32, h: f32) -> Decl {
        Decl {
            id: Id::str(id),
            kind: NodeKind::Box,
            shapes: Vec::new(),
            data: NodeData::Container(ContainerParams::default()),
            box_model: BoxModel::default(),
            size_class: SizeClass::fixed(w, h),
            ink_margin: Insets::ZERO,
            offset: crate::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: crate::sense::Sense::NONE,
            disabled: false,
            grid: GridItem::default(),
            placement: crate::layout::Placement::default(),
            env: None,
            children: Vec::new(),
        }
    }

    // The check itself is `#[cfg(debug_assertions)]`, so release builds have nothing to panic.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "duplicate sibling Id")]
    fn duplicate_sibling_ids_are_rejected_in_debug() {
        // Without this check the second declaration silently reuses the first node and
        // the first one's content disappears — six rows rendering as one, in practice.
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .child(box_node("row", 10.0, 10.0))
                .child(box_node("row", 10.0, 10.0)),
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "reuses its parent's Id")]
    fn a_child_sharing_its_parents_id_is_rejected_in_debug() {
        // A wrapper sized around a widget that carries the same id loses its own size:
        // the two declarations become one node, so the widget fills the grandparent.
        let mut tree = Tree::new();
        tree.apply(&Decl::container(Id::str("root"), NodeKind::Column).child(
            Decl::container(Id::str("slot"), NodeKind::Column).child(box_node("slot", 10.0, 10.0)),
        ));
    }

    #[test]
    fn same_id_across_frames_reuses_the_node() {
        let mut tree = Tree::new();
        let decl =
            Decl::container(Id::str("root"), NodeKind::Column).child(box_node("a", 10.0, 10.0));
        let first = tree.apply(&decl);
        let root_before = tree.root();
        let child_before = tree.find_by_id(Id::str("a"));

        let second = tree.apply(&decl);

        assert_eq!(first.created, 2);
        assert_eq!(second.reused, 2);
        assert_eq!(second.created, 0);
        assert_eq!(tree.root(), root_before);
        assert_eq!(tree.find_by_id(Id::str("a")), child_before);
    }
}
