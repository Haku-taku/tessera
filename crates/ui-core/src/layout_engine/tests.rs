//! Tests for this module, split from the source file; one module per contract area.

mod layout {
    use crate::layout::LayoutEngine;
    use crate::layout_engine::*;

    /// A stack overlays its children: same origin, parent sized by the largest layer.
    #[test]
    fn a_stack_overlays_its_children_instead_of_stacking_them_down_the_box() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(
                    Decl::container(Id::str("stack"), NodeKind::Stack)
                        .child(
                            Decl::container(Id::str("wide"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(120.0, 20.0)),
                        )
                        .child(
                            Decl::container(Id::str("tall"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(40.0, 50.0)),
                        ),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));

        let wide = tree.get(tree.find_by_id(Id::str("wide")).unwrap()).unwrap();
        let tall = tree.get(tree.find_by_id(Id::str("tall")).unwrap()).unwrap();
        assert_eq!(
            (wide.rect.origin.x, wide.rect.origin.y),
            (tall.rect.origin.x, tall.rect.origin.y),
            "both layers sit at the content origin"
        );
        let stack = tree
            .get(tree.find_by_id(Id::str("stack")).unwrap())
            .unwrap();
        assert_eq!(
            (stack.rect.size.w, stack.rect.size.h),
            (120.0, 50.0),
            "the stack demands the widest and tallest layer, not their sum"
        );
    }

    /// An item's own alignment overrides the line's: CSS `align-self` is the cross axis,
    /// so it centres a Column's child across the column and a Row's child down the row.
    #[test]
    fn a_flow_item_aligns_itself_across_the_line() {
        use crate::layout::Align;
        let item = |id: &'static str, align: Option<Align>| {
            let decl = Decl::container(Id::str(id), NodeKind::Box)
                .size(crate::layout::SizeClass::fixed(20.0, 10.0));
            match align {
                Some(align) => decl.align_self(align),
                None => decl,
            }
        };
        let solve = |decl: Decl| {
            let mut tree = Tree::new();
            tree.apply(&decl);
            let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
            let root = tree.root().unwrap();
            engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));
            let rect = |id: &'static str| {
                tree.get(tree.find_by_id(Id::str(id)).unwrap())
                    .unwrap()
                    .rect
            };
            (rect("centre"), rect("end"), rect("plain"))
        };
        let (centre, end, plain) = solve(
            Decl::container(Id::str("column"), NodeKind::Column)
                .size(crate::layout::SizeClass::fixed(100.0, 40.0))
                .child(item("centre", Some(Align::Center)))
                .child(item("end", Some(Align::End)))
                .child(Decl::container(Id::str("plain"), NodeKind::Box)),
        );
        assert_eq!((centre.origin.x, centre.size.w), (40.0, 20.0));
        assert_eq!((end.origin.x, end.size.w), (80.0, 20.0));
        assert_eq!(plain.size.w, 100.0, "no alignment still stretches");

        let (centre, end, plain) = solve(
            Decl::container(Id::str("row"), NodeKind::Row)
                .size(crate::layout::SizeClass::fixed(100.0, 40.0))
                .child(item("centre", Some(Align::Center)))
                .child(item("end", Some(Align::End)))
                .child(Decl::container(Id::str("plain"), NodeKind::Box)),
        );
        assert_eq!((centre.origin.y, centre.size.h), (15.0, 10.0));
        assert_eq!((end.origin.y, end.size.h), (30.0, 10.0));
        assert_eq!(plain.size.h, 40.0, "no alignment still stretches");
    }

    /// A fitting item's hint caps it but never widens it: content decides the basis.

    /// An in-flow margin offsets the child and takes its share of both axes.

    /// A margin belongs to the declaration, not to a container body: a text leaf takes one.

    /// A switchable row stacks once its children no longer share a line, and then reads
    /// the alternate margin each child declared for that arrangement.
    #[test]
    fn a_switchable_row_stacks_when_its_children_no_longer_fit() {
        use crate::geom::Insets;
        use crate::layout::AxisSwitch;
        let solve = |width: f32| {
            let mut tree = Tree::new();
            let inline = |id: &'static str, top: f32, alt_top: f32| {
                Decl::container(Id::str(id), NodeKind::Box)
                    .size(crate::layout::SizeClass::fixed(80.0, 20.0))
                    .margin(Insets {
                        left: 12.0,
                        top,
                        ..Insets::ZERO
                    })
                    .margin_alt(Insets {
                        top: alt_top,
                        ..Insets::ZERO
                    })
            };
            tree.apply(
                &Decl::container(Id::str("root"), NodeKind::Column)
                    .size(crate::layout::SizeClass::fixed(width, 200.0))
                    .child(
                        Decl::container(Id::str("bar"), NodeKind::Row)
                            .switch_axis(AxisSwitch { max_cross: None })
                            .child(inline("a", 14.0, 4.0))
                            .child(inline("b", 8.0, 12.0)),
                    ),
            );
            let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
            let root = tree.root().unwrap();
            engine.solve(&mut tree, root, Constraints::loose(Size::new(width, 200.0)));
            let rect = |id: &'static str| {
                tree.get(tree.find_by_id(Id::str(id)).unwrap())
                    .unwrap()
                    .rect
            };
            (rect("a"), rect("b"), rect("bar"))
        };
        // 12 + 80 + 12 + 80 = 184 fits a 400-wide bar.
        let (a, b, bar) = solve(400.0);
        assert_eq!((a.origin.x, a.origin.y), (12.0, 14.0), "a leads the line");
        assert_eq!((b.origin.x, b.origin.y), (104.0, 8.0), "b follows it");
        assert_eq!(
            bar.size.h, 34.0,
            "the bar is one line tall: 14 above a 20px child"
        );

        let (a, b, bar) = solve(150.0);
        assert_eq!((a.origin.x, a.origin.y), (0.0, 4.0), "a starts the stack");
        assert_eq!((b.origin.x, b.origin.y), (0.0, 36.0), "b sits under it");
        assert_eq!(bar.size.h, 56.0, "4 + 20 + 12 + 20");
    }

    /// A row whose child has already wrapped past `max_cross` stacks too.
    /// A stretching child stays inside the fixed width of the column that holds it.
    #[test]
    fn a_stretching_child_stays_inside_its_fixed_width_column() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(crate::layout::SizeClass::fixed(320.0, 200.0))
                .child(
                    Decl::container(Id::str("bar"), NodeKind::Box)
                        .size(crate::layout::SizeClass {
                            width: crate::layout::AxisSize::fill(0.0, 0.0),
                            height: crate::layout::AxisSize::hug(0.0, 0.0),
                            grow_x: crate::layout::Grow::No,
                            grow_y: crate::layout::Grow::No,
                        })
                        .child(
                            Decl::container(Id::str("row"), NodeKind::Row).child(paragraph(
                                "long",
                                "word ".repeat(40),
                                10.0,
                                f32::INFINITY,
                            )),
                        ),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(320.0, 200.0)));
        let rect = |id: &'static str| {
            tree.get(tree.find_by_id(Id::str(id)).unwrap())
                .unwrap()
                .rect
        };
        assert_eq!(
            rect("bar").size.w,
            320.0,
            "the bar fills the column, not its content"
        );
    }

    /// A hugging container includes its children's margins in its own extent.

    /// A tooltip declared beside a bar still hangs off the button deep inside it: the
    /// anchor is resolved after the flow, whichever container declared the panel.
    #[test]
    fn an_overlay_declared_beside_the_bar_anchors_to_a_button_inside_it() {
        use crate::layout::{Anchor, OverlayPlacement};
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Stack)
                .size(crate::layout::SizeClass::fixed(400.0, 300.0))
                .child(
                    Decl::container(Id::str("shell"), NodeKind::Stack)
                        .size(crate::layout::SizeClass::fixed(200.0, 48.0))
                        .offset(100.0, 100.0)
                        .child(
                            Decl::container(Id::str("slot"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(40.0, 40.0))
                                .offset(30.0, 4.0)
                                .child(
                                    Decl::container(Id::str("button"), NodeKind::Box)
                                        .size(crate::layout::SizeClass::fixed(40.0, 40.0)),
                                ),
                        ),
                )
                .child(
                    Decl::container(Id::str("panel"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(60.0, 20.0))
                        .overlay(OverlayPlacement::below(Anchor::Node(Id::str("button")))),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(400.0, 300.0)));

        let at = |id: Id| {
            let node = tree.get(tree.find_by_id(id).unwrap()).unwrap();
            (node.visual_rect().origin.x, node.visual_rect().origin.y)
        };
        let button = at(Id::str("button"));
        let panel = at(Id::str("panel"));
        assert_eq!(button, (130.0, 104.0), "the button sits in the bar");
        assert_eq!(
            (panel.0, panel.1),
            (button.0, button.1 + 40.0),
            "the panel unfolds under the button, not at the root's edge"
        );
    }

    /// An overlay hangs off its anchor and is not cut by the clip of where it is declared.
    #[test]
    fn an_overlay_escapes_the_clip_of_its_declaring_ancestor() {
        use crate::layout::{Anchor, Clip, Overflow, OverlayPlacement};
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(crate::layout::SizeClass::fixed(200.0, 200.0))
                .child(
                    Decl::container(Id::str("clip"), NodeKind::Column)
                        .size(crate::layout::SizeClass::fixed(100.0, 40.0))
                        .clip(Clip {
                            x: Overflow::Hidden,
                            y: Overflow::Hidden,
                        })
                        .child(
                            Decl::container(Id::str("anchor"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(40.0, 20.0)),
                        )
                        .child(
                            Decl::container(Id::str("panel"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(60.0, 80.0))
                                .overlay(OverlayPlacement::below(Anchor::Node(Id::str("anchor")))),
                        ),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));

        let panel = tree
            .get(tree.find_by_id(Id::str("panel")).unwrap())
            .unwrap();
        assert_eq!(
            (panel.rect.origin.x, panel.rect.origin.y),
            (0.0, 20.0),
            "the panel unfolds right under its anchor"
        );
        assert!(
            panel.mask.max_y() >= panel.rect.max_y() - 0.01,
            "the declaring ancestor's clip must not cut the overlay: mask {:?}",
            panel.mask
        );
    }

    /// A lifted node keeps its slot but paints above the clip it was declared inside.
    #[test]
    fn a_lifted_node_keeps_its_slot_and_escapes_the_clip_around_it() {
        use crate::layout::{Clip, Overflow};
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(crate::layout::SizeClass::fixed(200.0, 200.0))
                .child(
                    Decl::container(Id::str("clip"), NodeKind::Column)
                        .size(crate::layout::SizeClass::fixed(100.0, 40.0))
                        .clip(Clip {
                            x: Overflow::Hidden,
                            y: Overflow::Hidden,
                        })
                        .child(
                            Decl::container(Id::str("slot"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(100.0, 20.0)),
                        )
                        .child(
                            Decl::container(Id::str("lifted"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(50.0, 20.0))
                                .offset(0.0, 60.0)
                                .lifted(),
                        )
                        .child(
                            Decl::container(Id::str("tail"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(100.0, 20.0)),
                        ),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));

        let lifted = tree
            .get(tree.find_by_id(Id::str("lifted")).unwrap())
            .unwrap();
        assert_eq!(lifted.rect.origin.y, 20.0, "the slot stays in the flow");
        assert_eq!(
            lifted.visual_rect().origin.y,
            80.0,
            "the offset only moves the pixels"
        );
        assert_eq!(
            lifted.mask.max_y(),
            200.0,
            "the clip around it must not cut the lifted node: mask {:?}",
            lifted.mask
        );
        let last = *tree.walk().last().unwrap();
        assert_eq!(
            tree.get(last).map(|n| n.id),
            Some(Id::str("lifted")),
            "painted last, so it also hits first"
        );
    }

    /// A panel matching its anchor keeps the minimum a short trigger cannot provide.

    /// A panel told to span its anchor is measured inside the anchor's box, so a fill child
    /// reaches the anchor's height rather than the viewport's.

    /// A panel that would leave the viewport flips to the other side of its anchor.
    #[test]
    fn an_overlay_flips_above_when_it_would_overflow_the_viewport() {
        use crate::layout::{Anchor, OverlayPlacement};
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(crate::layout::SizeClass::fixed(200.0, 200.0))
                .child(
                    Decl::container(Id::str("spacer"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(200.0, 150.0)),
                )
                .child(
                    Decl::container(Id::str("anchor"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(40.0, 20.0)),
                )
                .child(
                    Decl::container(Id::str("panel"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(60.0, 60.0))
                        .overlay(OverlayPlacement::below(Anchor::Node(Id::str("anchor")))),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));

        let panel = tree
            .get(tree.find_by_id(Id::str("panel")).unwrap())
            .unwrap();
        assert_eq!(
            (panel.rect.origin.y, panel.rect.max_y()),
            (90.0, 150.0),
            "below would reach 230 in a 200-tall viewport, so the panel sits above the anchor"
        );
    }

    /// A panel for a surface of the platform's own keeps its preferred position, even a
    /// position the viewport it was declared in does not contain, and is recorded for the
    /// platform to present.
    #[test]
    fn a_platform_panel_keeps_its_preferred_position_and_is_recorded() {
        use crate::layout::{Anchor, OverlayPlacement, Presentation};
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Stack)
                .size(crate::layout::SizeClass::fixed(200.0, 200.0))
                .child(
                    Decl::container(Id::str("anchor"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(40.0, 20.0))
                        .offset(150.0, 150.0),
                )
                .child(
                    Decl::container(Id::str("panel"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(400.0, 300.0))
                        .overlay(
                            OverlayPlacement::below(Anchor::Node(Id::str("anchor")))
                                .presentation(Presentation::Platform),
                        ),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));

        let panel = tree
            .get(tree.find_by_id(Id::str("panel")).unwrap())
            .unwrap();
        assert_eq!(
            (panel.visual_rect().origin.x, panel.visual_rect().origin.y),
            (150.0, 170.0),
            "the panel hangs under its anchor, outside a 200-square viewport"
        );
        let recorded = tree.platform_panels();
        assert_eq!(recorded.len(), 1, "one panel asks for a surface of its own");
        assert_eq!(recorded[0].node, tree.find_by_id(Id::str("panel")).unwrap());
        assert_eq!(
            (recorded[0].anchor.origin.x, recorded[0].anchor.origin.y),
            (150.0, 150.0),
            "the anchor travels with the request, in the parent's coordinates"
        );
        assert_eq!(
            (recorded[0].rect.origin.x, recorded[0].rect.origin.y),
            (150.0, 170.0),
            "the platform gets the preferred position, not a clamped one"
        );
    }

    /// A row squeezes a scrollable child to its own width instead of letting it overflow.
    #[test]
    fn a_row_squeezes_a_scrollable_child_to_the_line_before_it_overflows() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Row)
                .size(crate::layout::SizeClass::fixed(100.0, 40.0))
                .child(
                    Decl::container(Id::str("list"), NodeKind::Row)
                        .clip(crate::layout::Clip {
                            x: crate::layout::Overflow::Scroll,
                            y: crate::layout::Overflow::Hidden,
                        })
                        .child(
                            Decl::container(Id::str("wide"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(150.0, 20.0)),
                        ),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(100.0, 40.0)));

        let list = tree.get(tree.find_by_id(Id::str("list")).unwrap()).unwrap();
        assert_eq!(
            list.rect.size.w, 100.0,
            "the list takes the line, not its content"
        );
        assert_eq!(
            list.scroll_extent.map(|e| e.w),
            Some(150.0),
            "the content still overflows inside it"
        );
    }

    /// A stack has no line to measure, so its content extent is the union of the layers it
    /// lays down. Leaving it unset made a scrolling stack a dead control: no travel, no bar.
    #[test]
    fn a_scrolling_stack_reports_the_union_of_its_layers_as_its_extent() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("stack"), NodeKind::Stack)
                .size(crate::layout::SizeClass::fixed(100.0, 100.0))
                .clip(crate::layout::Clip {
                    x: crate::layout::Overflow::Hidden,
                    y: crate::layout::Overflow::Scroll,
                })
                .child(
                    Decl::container(Id::str("short"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(80.0, 40.0)),
                )
                .child(
                    Decl::container(Id::str("tall"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(60.0, 300.0)),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(100.0, 100.0)));

        let stack = tree
            .get(tree.find_by_id(Id::str("stack")).unwrap())
            .unwrap();
        assert_eq!(
            stack.scroll_extent,
            Some(Size::new(80.0, 300.0)),
            "the extent covers the widest and the tallest layer"
        );
    }

    /// An overlay takes no slot in the flow and lands at its parent's content origin.
    #[test]
    fn an_overlay_child_takes_no_space_in_the_flow() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .child(
                    Decl::container(Id::str("flow"), NodeKind::Column)
                        .size(crate::layout::SizeClass::fixed(80.0, 40.0)),
                )
                .child(
                    Decl::container(Id::str("float"), NodeKind::Overlay)
                        .size(crate::layout::SizeClass::fixed(60.0, 30.0)),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));

        let flow = tree.get(tree.find_by_id(Id::str("flow")).unwrap()).unwrap();
        let float = tree
            .get(tree.find_by_id(Id::str("float")).unwrap())
            .unwrap();
        assert_eq!(
            float.rect.size.h, 30.0,
            "the overlay keeps its own size instead of being squeezed by the flow"
        );
        assert_eq!(
            float.rect.origin.y, flow.rect.origin.y,
            "the overlay sits at the content origin, not after its siblings"
        );
    }

    /// Adding an overlay must not grow its parent: it takes no slot and no gap.

    /// A modal covers its target, not the slot the tree gave it: the nearest declared host
    /// for `Proximate`, the window for `Global`.
    #[test]
    fn a_modal_covers_its_target_instead_of_the_slot_the_tree_gave_it() {
        let solve = |target: crate::tree::ModalTarget| {
            let panel = Decl::container(Id::str("panel"), NodeKind::Box)
                .size(crate::layout::SizeClass::fixed(200.0, 100.0))
                .modal_host()
                .child(
                    Decl::container(Id::str("modal"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(40.0, 20.0))
                        .modal(target),
                );
            let root = Decl::container(Id::str("root"), NodeKind::Column)
                .padding(crate::geom::Insets::all(30.0))
                .child(panel);
            let mut tree = Tree::new();
            tree.apply(&root);
            let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
            let root = tree.root().unwrap();
            engine.solve(&mut tree, root, Constraints::loose(Size::new(400.0, 300.0)));
            let rect = |id: &'static str| {
                tree.get(tree.find_by_id(Id::str(id)).unwrap())
                    .unwrap()
                    .rect
            };
            (rect("panel"), rect("modal"))
        };
        let (panel, proximate) = solve(crate::tree::ModalTarget::Proximate);
        assert_eq!(
            panel.origin,
            crate::geom::Point::new(30.0, 30.0),
            "the panel sits in the padding"
        );
        assert_eq!(proximate, panel, "the nearest declared host is the target");
        let (_, global) = solve(crate::tree::ModalTarget::Global);
        assert_eq!(
            global,
            Rect::from_origin_size(crate::geom::Point::ZERO, Size::new(400.0, 300.0)),
            "the window is the target, padding and all"
        );
    }

    /// A modal follows its host through a resize: it is placed on the host's rect in the
    /// same pass, so there is no second source of truth to go stale.

    /// A fixed-size child in a Column keeps its width and sits where `cross_pos` says.
    #[test]
    fn a_cross_positioned_child_is_centred_in_the_space_it_left_over() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(crate::layout::SizeClass::fixed(100.0, 40.0))
                .cross_align(0.0)
                .cross_pos(0.5)
                .child(
                    Decl::container(Id::str("icon"), NodeKind::Box)
                        .size(crate::layout::SizeClass::fixed(24.0, 24.0)),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(100.0, 40.0)));
        let icon = tree.get(tree.find_by_id(Id::str("icon")).unwrap()).unwrap();
        assert_eq!(icon.rect.size.w, 24.0, "the child keeps its own width");
        assert_eq!(icon.rect.origin.x, 38.0, "centred: (100 - 24) / 2");
    }

    /// A baseline row shares one baseline and is as tall as both sides of it; the same row
    /// without the flag hangs every child from its top, so the baselines disagree.
    #[test]
    fn a_baseline_row_shares_one_line_and_grows_to_both_sides_of_it() {
        let row = |baseline: bool| {
            let mut decl = Decl::container(Id::str("row"), NodeKind::Row)
                .size(crate::layout::SizeClass::hug(0.0, 0.0))
                .gap(6.0)
                .cross_align(0.0);
            if baseline {
                decl = decl.cross_baseline();
            }
            decl.child(text_node_at("small", "Aa", 10.0, crate::layout::Grow::No))
                .child(text_node_at("large", "Bb", 20.0, crate::layout::Grow::No))
                .child(box_node("mark", 24.0, 24.0))
        };
        let solve = |baseline: bool| {
            let mut tree = Tree::new();
            tree.apply(
                &Decl::container(Id::str("root"), NodeKind::Column)
                    .cross_align(0.0)
                    .child(row(baseline)),
            );
            let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
            let root = tree.root().unwrap();
            engine.solve(&mut tree, root, Constraints::loose(Size::new(400.0, 400.0)));
            let read = |id: &'static str| {
                let node = tree.get(tree.find_by_id(Id::str(id)).unwrap()).unwrap();
                (node.rect.origin.y, node.rect.size.h, node.baseline)
            };
            let row_h = tree
                .get(tree.find_by_id(Id::str("row")).unwrap())
                .unwrap()
                .rect
                .size
                .h;
            (read("small"), read("large"), read("mark"), row_h)
        };
        let line = |(y, h, b): (f32, f32, f32)| y + if b > 0.0 { b } else { h };
        let away = solve(false);
        let on = solve(true);
        assert_eq!(
            (line(away.0), line(away.1), line(away.2)),
            (10.0, 20.0, 24.0),
            "without the flag every child sits at the row's top"
        );
        assert_eq!(away.3, 26.0, "the row is as tall as its tallest child");
        assert_eq!(
            (line(on.0), line(on.1), line(on.2)),
            (24.0, 24.0, 24.0),
            "the box's synthesised baseline is its bottom edge, and the texts meet it"
        );
        assert_eq!(
            (on.0.0, on.1.0, on.2.0),
            (14.0, 4.0, 0.0),
            "each child moves up by the distance from its own baseline to the line's"
        );
        assert_eq!(
            on.3, 30.0,
            "24 above the shared baseline and 6 below it, not the tallest child's 26"
        );
    }

    /// A stretched child whose content is taller than the line shrinks to the line, not
    /// past it: a scroll viewport must not be grown by the content it is meant to scroll.
    #[test]
    fn a_stretched_scroller_shrinks_to_the_line_instead_of_growing_past_it() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Row)
                .size(crate::layout::SizeClass::fixed(100.0, 40.0))
                .child(
                    Decl::container(Id::str("nav"), NodeKind::Column)
                        .size(crate::layout::SizeClass {
                            width: crate::layout::AxisSize::fixed(50.0),
                            height: crate::layout::AxisSize::fill(0.0, 0.0),
                            grow_x: crate::layout::Grow::No,
                            grow_y: crate::layout::Grow::No,
                        })
                        .scrollable()
                        .child(
                            Decl::container(Id::str("tall"), NodeKind::Box)
                                .size(crate::layout::SizeClass::fixed(50.0, 200.0)),
                        ),
                ),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(100.0, 40.0)));
        let nav = tree.get(tree.find_by_id(Id::str("nav")).unwrap()).unwrap();
        assert_eq!(
            nav.rect.size.h, 40.0,
            "the scroller takes the line's height"
        );
    }

    /// A `nowrap` line never breaks, but its alignment still stands in the box it took:
    /// an end-aligned label ends at the box's far edge, which is what lines up a `9.` and
    /// a `10.` on their dots.
    #[test]
    fn a_nowrap_line_aligns_inside_its_box() {
        let mut decl = text_decl(
            "label",
            crate::text::TextBlock::plain("ab".to_owned(), crate::text::TextStyle::new(10.0)),
            100.0,
            14.0,
        );
        decl.size_class.width = crate::layout::AxisSize::fixed(100.0);
        if let crate::NodeData::Text { block, nowrap, .. } = &mut decl.data {
            *nowrap = true;
            std::sync::Arc::make_mut(block).align = crate::TextAlign::End;
        }
        let mut tree = Tree::new();
        tree.apply(&Decl::container(Id::str("root"), NodeKind::Column).child(decl));
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 100.0)));

        let node = tree
            .get(tree.find_by_id(Id::str("label")).unwrap())
            .unwrap();
        let layout = node.inline.as_ref().unwrap();
        let line = &layout.lines[0];
        let origin = layout.line_origin(
            0,
            node.rect.origin,
            node.rect.size.h,
            node.wrap_width,
            crate::LineEdges::default(),
            crate::TextAlign::End,
        );
        assert_eq!(
            node.wrap_width, 100.0,
            "the unbreakable line is measured in the box it occupies"
        );
        assert_eq!(
            origin.x + line.width,
            node.rect.origin.x + 100.0,
            "an end-aligned line ends at the box's right edge"
        );
    }

    /// A text leaf that may wrap anywhere has an infinite hint; inside a Row it must
    /// not turn the sibling widths into NaN (they then never paint).

    /// A padded box hugging its text takes the text plus its padding; otherwise the box is
    /// narrower than the line inside it and the painter clips that line.

    /// A `fixed(200)` column must wrap its text at 200, not at the window width: the measure
    /// pass otherwise hands children the whole constraint and the text overflows the column.

    /// A `nowrap` text is laid out on one line inside a box too narrow for it: the line is
    /// never broken, while the box it occupies stays the one it declared.

    /// A text leaf declared the way `ui_widgets::Text` declares one: the width hugs a
    /// `max_width`, the height hugs a 1.4× line box.
    fn text_decl(
        id: &'static str,
        block: crate::text::TextBlock,
        max_width: f32,
        line_hint: f32,
    ) -> Decl {
        Decl {
            id: Id::str(id),
            kind: NodeKind::Text,
            shapes: Vec::new(),
            data: crate::NodeData::Text {
                ink: crate::geom::Color::WHITE,
                block: std::sync::Arc::new(block),
                caret: None,
                selection: None,
                composing: None,
                nowrap: false,
            },
            size_class: crate::layout::SizeClass {
                width: crate::layout::AxisSize::hug(0.0, max_width),
                height: crate::layout::AxisSize::hug(line_hint / 1.4, line_hint),
                ..crate::layout::SizeClass::fixed(0.0, 0.0)
            },
            ..Decl::container(Id::str(id), NodeKind::Text)
        }
    }

    fn paragraph(id: &'static str, body: String, size: f32, max_width: f32) -> Decl {
        text_decl(
            id,
            crate::text::TextBlock::plain(body, crate::text::TextStyle::new(size)),
            max_width,
            size * 1.4,
        )
    }

    /// Gaps sit between lines only: n lines add n-1 of them, so the block does not grow
    /// a trailing one.

    /// An atom is measured at its own size, never squeezed into the room left on a line.

    /// A top-aligned atom hangs from the line's top and grows the box downwards.

    /// The node reports its first line's baseline, not zero and not the tallest run's.

    /// Two identical paragraphs in a column stack by the height they wrapped into, so the
    /// second never starts above the first's bottom and their painted lines cannot overlap.

    /// An overlay must not be charged to a growing sibling: it takes no slot, so a
    /// fixed-size overlay above a filling child must leave that child its full height.

    /// The flow's own size must ignore overlays, or a floating panel would push it open.

    /// A stack has no line to grow in, so `fill` there means the stack's content box —
    /// the answer CSS gives `position: absolute; inset: 0`.
    #[test]
    fn a_stack_stretches_a_fill_child_to_its_content_box() {
        let mut layer = Decl::container(Id::str("layer"), NodeKind::Box);
        layer.size_class = SizeClass {
            width: crate::layout::AxisSize::fill(0.0, 0.0),
            height: crate::layout::AxisSize::fill(0.0, 0.0),
            grow_x: crate::layout::Grow::No,
            grow_y: crate::layout::Grow::No,
        };
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Stack)
                .padding(Insets::all(10.0))
                .child(layer),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::tight(Size::new(200.0, 100.0)));
        let layer = tree
            .get(tree.find_by_id(Id::str("layer")).unwrap())
            .unwrap();
        assert_eq!(
            layer.rect,
            Rect::new(10.0, 10.0, 180.0, 80.0),
            "a filling stack child must take the padded content box, got {:?}",
            layer.rect
        );
    }

    /// Column children stretch to the content width by default.
    #[test]
    fn column_children_stretch_across_the_cross_axis() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .padding(Insets::all(10.0))
                .child(Decl::container(Id::str("a"), NodeKind::Row)),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 100.0)));
        let a = tree.get(tree.find_by_id(Id::str("a")).unwrap()).unwrap();
        assert_eq!(
            a.rect.size.w, 180.0,
            "child must fill the padded content width, got {}",
            a.rect.size.w
        );
    }

    use crate::id::Id;
    use crate::tree::Decl;

    fn box_node(id: &'static str, w: f32, h: f32) -> Decl {
        Decl {
            id: Id::str(id),
            kind: NodeKind::Box,
            shapes: Vec::new(),
            data: NodeData::Container(crate::tree::ContainerParams::default()),
            size_class: SizeClass::fixed(w, h),
            ink_margin: Insets::ZERO,
            box_model: Default::default(),
            offset: crate::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: crate::Sense::NONE,
            disabled: false,
            grid: crate::tree::GridItem::default(),
            placement: crate::layout::Placement::default(),
            env: None,
            drag: None,
            drag_out: false,
            drop_types: None,
            children: Vec::new(),
        }
    }

    fn text_node(id: &'static str, body: &str, grow_x: crate::layout::Grow) -> Decl {
        text_node_at(id, body, 10.0, grow_x)
    }

    fn text_node_at(id: &'static str, body: &str, size: f32, grow_x: crate::layout::Grow) -> Decl {
        Decl {
            id: Id::str(id),
            kind: NodeKind::Text,
            shapes: Vec::new(),
            data: NodeData::Text {
                ink: crate::geom::Color::WHITE,
                block: std::sync::Arc::new(crate::text::TextBlock {
                    runs: vec![crate::text::TextRun::new(
                        body,
                        crate::text::TextStyle::new(size),
                    )],
                    align: crate::TextAlign::Start,
                    max_lines: None,
                    line_gap: 0.0,
                    indent: 0.0,
                    hanging: 0.0,
                    atoms: Vec::new(),
                    input_base: None,
                    line_numbers: None,
                    structures: Vec::new(),
                }),
                caret: None,
                selection: None,
                composing: None,
                nowrap: false,
            },
            size_class: SizeClass {
                width: crate::layout::AxisSize::hug(0.0, 0.0),
                height: crate::layout::AxisSize::hug(0.0, 0.0),
                grow_x,
                grow_y: crate::layout::Grow::No,
            },
            ink_margin: Insets::ZERO,
            box_model: Default::default(),
            offset: crate::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: crate::Sense::NONE,
            disabled: false,
            grid: crate::tree::GridItem::default(),
            placement: crate::layout::Placement::default(),
            env: None,
            drag: None,
            drag_out: false,
            drop_types: None,
            children: Vec::new(),
        }
    }

    /// One grid cell owner with no definite size, so the default stretch applies.
    fn cell_item(id: &'static str, row: u32, col: u32, row_span: u32, col_span: u32) -> Decl {
        Decl::container(Id::str(id), NodeKind::Box)
            .size(SizeClass::hug(0.0, 0.0))
            .grid_area(row, col, row_span, col_span)
    }

    fn grid_decl(
        id: &'static str,
        cols: Vec<crate::layout::Track>,
        rows: Vec<crate::layout::Track>,
    ) -> Decl {
        Decl::container(Id::str(id), NodeKind::Grid).tracks(crate::layout::TrackList {
            cols,
            rows,
            auto_flow: crate::layout::AutoFlow::Row,
        })
    }

    #[test]
    fn grid_tracks_sum_to_the_content_box() {
        // A definite grid fills its box exactly: fixed tracks keep their size, the fr
        // tracks split what is left, and the gaps count inside the same box.
        use crate::layout::Track;
        let grid = grid_decl(
            "grid",
            vec![Track::Fixed(100.0), Track::Fr(1.0), Track::Fr(1.0)],
            vec![Track::Fixed(100.0)],
        )
        .size(SizeClass::fixed(400.0, 100.0))
        .gap(10.0)
        // A `hug` item has no definite size, so the default stretch fills its cell.
        .child(cell_item("a", 0, 0, 1, 1))
        .child(cell_item("b", 0, 1, 1, 1))
        .child(cell_item("c", 0, 2, 1, 1));
        let mut tree = Tree::new();
        tree.apply(&grid);
        solve(&mut tree);
        let rect = |id: &'static str| {
            tree.get(tree.find_by_id(Id::str(id)).unwrap())
                .unwrap()
                .rect
        };
        let (a, b, c) = (rect("a"), rect("b"), rect("c"));
        assert!(
            (a.size.w - 100.0).abs() < 0.01 && (b.size.w - 140.0).abs() < 0.01,
            "fixed keeps 100, each fr takes 140: {a:?} {b:?}"
        );
        assert!(
            (b.origin.x - 110.0).abs() < 0.01,
            "the second track starts after the first plus one gap, got {}",
            b.origin.x
        );
        assert!(
            ((c.origin.x + c.size.w) - 400.0).abs() < 0.01,
            "the tracks and gaps must sum to the content box, got {}",
            c.origin.x + c.size.w
        );
    }

    #[test]
    fn a_grid_item_wraps_at_the_width_of_its_cell() {
        // A fixed 100 column inside a 400-wide grid: the item is measured against the grid's
        // own room first, so its text lays out one long line and overflows the column. It has
        // to reflow at the width it will occupy, and the auto row has to grow to hold it.
        use crate::layout::Track;
        let mut tree = Tree::new();
        tree.apply(
            &grid_decl("grid", vec![Track::Fixed(100.0)], vec![Track::Auto])
                .size(SizeClass::fixed(400.0, 200.0))
                .child(paragraph(
                    "body",
                    "aaaa bbbb cccc dddd eeee".to_string(),
                    10.0,
                    400.0,
                )),
        );
        solve(&mut tree);
        let body = rect_of(&tree, "body");
        assert!(
            body.size.w <= 100.01,
            "the item must fit its 100-wide column, got {:?}",
            body.size
        );
        assert!(
            (body.size.h - 26.0).abs() < 0.01,
            "two 13 px lines make the auto row 26 tall, got {:?}",
            body.size
        );
    }

    #[test]
    fn a_scrolling_row_keeps_a_zero_basis_child_at_its_content_width() {
        // A scrolling row is sized by its content, so its line has no deficit to shrink:
        // the sum of the items' bases must include what a zero-basis grower wants.
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Row)
                .size(SizeClass::fixed(400.0, 40.0))
                .scrollable()
                .child(text_node(
                    "label",
                    "alpha beta",
                    crate::layout::Grow::Split(1),
                )),
        );
        solve(&mut tree);
        let label = rect_of(&tree, "label");
        assert!(
            (label.size.w - 60.0).abs() < 0.01,
            "the label keeps its max-content width, got {:?}",
            label.size
        );
    }

    #[test]
    fn a_column_flow_span_taller_than_the_template_creates_implicit_rows() {
        use crate::layout::{AutoFlow, Track, TrackList};
        // CSS creates implicit rows for a span the template does not hold; probing the
        // occupancy grid past its last row would index out of bounds instead.
        let grid = Decl::container(Id::str("grid"), NodeKind::Grid)
            .tracks(TrackList {
                cols: vec![Track::Fixed(60.0); 4],
                rows: vec![Track::Fixed(20.0)],
                auto_flow: AutoFlow::Column,
            })
            .size(SizeClass::fixed(240.0, 100.0))
            .child(auto_item("tall").grid_span(2, 1))
            .child(auto_item("next"));
        let mut tree = Tree::new();
        tree.apply(&grid);
        solve(&mut tree);
        let tall = rect_of(&tree, "tall");
        assert!(
            tall.size.h.is_finite() && tall.size.h > 0.0,
            "the spanning item lands in implicit rows, got {:?}",
            tall.size
        );
    }

    #[test]
    fn a_relative_percentage_resolves_against_its_parent() {
        use crate::layout::{Len, LenInsets};
        // A relative node keeps its flow slot: `left: 50%` is half of the parent's content
        // box, not half of the nearest positioned ancestor.
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("panel"), NodeKind::Column)
                .size(SizeClass::fixed(1000.0, 200.0))
                .relative()
                .child(
                    Decl::container(Id::str("inner"), NodeKind::Column)
                        .size(SizeClass::fixed(200.0, 100.0))
                        .child(
                            Decl::container(Id::str("label"), NodeKind::Box)
                                .size(SizeClass::fixed(40.0, 20.0))
                                .relative()
                                .inset(LenInsets::new(
                                    Len::Pct(50.0),
                                    Len::Auto,
                                    Len::Auto,
                                    Len::Auto,
                                )),
                        ),
                ),
        );
        solve(&mut tree);
        let label = tree
            .get(tree.find_by_id(Id::str("label")).unwrap())
            .unwrap();
        assert!(
            (label.offset.x - 100.0).abs() < 0.01,
            "half of the 200-wide parent, got {}",
            label.offset.x
        );
    }

    #[test]
    fn a_stretched_grid_item_stops_at_its_own_maximum() {
        use crate::layout::Track;
        // Stretch fills the cell, then the item's own max clamps it: a grid child must
        // obey `max-width` exactly as it does inside a row.
        let grid = grid_decl("grid", vec![Track::Fr(1.0)], vec![Track::Fr(1.0)])
            .size(SizeClass::fixed(300.0, 100.0))
            .child(auto_item("item").size(SizeClass {
                width: crate::layout::AxisSize::hug(0.0, 100.0),
                height: crate::layout::AxisSize::hug(0.0, 0.0),
                grow_x: crate::layout::Grow::No,
                grow_y: crate::layout::Grow::No,
            }));
        let mut tree = Tree::new();
        tree.apply(&grid);
        solve(&mut tree);
        let item = rect_of(&tree, "item");
        assert!(
            (item.size.w - 100.0).abs() < 0.01,
            "the declared maximum caps the stretched width, got {:?}",
            item.size
        );
    }

    #[test]
    fn a_definite_item_sizes_its_auto_track_instead_of_its_content() {
        use crate::layout::Track;
        // CSS: an item with a definite preferred size contributes exactly that size, so an
        // `auto` track must not widen to the text inside a fixed 20-wide item.
        let grid = grid_decl("grid", vec![Track::Auto, Track::Auto], vec![Track::Auto])
            .size(SizeClass::hug(0.0, 0.0))
            .child(
                text_node(
                    "label",
                    "a line of text that measures far wider than twenty points",
                    crate::layout::Grow::No,
                )
                .size(SizeClass::fixed(20.0, 10.0)),
            )
            .child(text_node("next", "b", crate::layout::Grow::No));
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(SizeClass::hug(0.0, 0.0))
                .child(grid),
        );
        solve(&mut tree);
        let next = rect_of(&tree, "next");
        assert!(
            (next.origin.x - 20.0).abs() < 0.01,
            "the first track is the declared 20 wide, so the second starts there, got {}",
            next.origin.x
        );
    }

    #[test]
    fn a_flexible_track_inside_an_unbounded_scroller_settles_to_its_content() {
        use crate::layout::Track;
        // A scrolling column offers no height bound, so a `Fr` row has no space to divide.
        // It reads as indefinite and sizes to content, never to infinity.
        let grid = grid_decl("grid", vec![Track::Fr(1.0)], vec![Track::Fr(1.0)])
            .size(SizeClass {
                width: crate::layout::AxisSize::fill(0.0, 0.0),
                height: crate::layout::AxisSize::fill(0.0, 0.0),
                grow_x: crate::layout::Grow::No,
                grow_y: crate::layout::Grow::No,
            })
            .child(box_node("cell", 50.0, 20.0));
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(300.0, 200.0))
                .scrollable()
                .child(grid),
        );
        solve(&mut tree);
        let g = rect_of(&tree, "grid");
        assert!(
            g.size.h.is_finite() && (g.size.h - 20.0).abs() < 0.01,
            "the row takes the item's height, got {:?}",
            g.size
        );
    }

    /// A bare grid item: no definite size, so the default stretch fills its cell.
    fn auto_item(id: &'static str) -> Decl {
        Decl::container(Id::str(id), NodeKind::Box)
    }

    fn rect_of(tree: &Tree, id: &'static str) -> Rect {
        tree.get(tree.find_by_id(Id::str(id)).unwrap())
            .unwrap()
            .rect
    }

    #[test]
    fn repeat_auto_fill_fits_as_many_tracks_as_the_width_allows() {
        // 340 wide with 80-wide tracks and 10 gaps fits three (3*80 + 2*10 = 260); a fourth
        // would need 350, so the fourth item wraps instead of starting a fourth track.
        use crate::layout::{AutoFlow, Len, Repeat, Track, TrackBound, TrackList};
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("grid"), NodeKind::Grid)
                .tracks(TrackList {
                    cols: vec![Track::Repeat(
                        Repeat::AutoFill,
                        Box::new(Track::MinMax(
                            TrackBound::Len(Len::Px(80.0)),
                            TrackBound::Len(Len::Px(80.0)),
                        )),
                    )],
                    // Explicit row heights: an empty implicit row has no intrinsic height.
                    rows: vec![Track::Fixed(20.0), Track::Fixed(20.0)],
                    auto_flow: AutoFlow::Row,
                })
                .size(SizeClass::fixed(340.0, 200.0))
                .gap(10.0)
                .child(box_node("a", 10.0, 10.0))
                .child(box_node("b", 10.0, 10.0))
                .child(box_node("c", 10.0, 10.0))
                .child(box_node("d", 10.0, 10.0)),
        );
        solve(&mut tree);
        let (a, b, c, d) = (
            rect_of(&tree, "a"),
            rect_of(&tree, "b"),
            rect_of(&tree, "c"),
            rect_of(&tree, "d"),
        );
        assert!(
            (a.origin.x - 0.0).abs() < 0.01
                && (b.origin.x - 90.0).abs() < 0.01
                && (c.origin.x - 180.0).abs() < 0.01,
            "three tracks should start at 0, 90 and 180, got {} {} {}",
            a.origin.x,
            b.origin.x,
            c.origin.x
        );
        assert!(
            (d.origin.x - 0.0).abs() < 0.01 && (d.origin.y - 30.0).abs() < 0.01,
            "the fourth item must wrap to the second row, got {:?}",
            d.origin
        );
    }

    #[test]
    fn auto_fit_collapses_the_tracks_no_item_lands_in() {
        // Three tracks fit the width; the pinned item sits in the last one and the auto
        // item in the first, so the middle track is empty. `auto-fill` keeps it and the
        // pinned item stays at 180; `auto-fit` collapses it and the item moves to 90.
        use crate::layout::{AutoFlow, Len, Repeat, Track, TrackBound, TrackList};
        let run = |mode: Repeat| {
            let mut tree = Tree::new();
            tree.apply(
                &Decl::container(Id::str("grid"), NodeKind::Grid)
                    .tracks(TrackList {
                        cols: vec![Track::Repeat(
                            mode,
                            Box::new(Track::MinMax(
                                TrackBound::Len(Len::Px(80.0)),
                                TrackBound::Len(Len::Px(80.0)),
                            )),
                        )],
                        rows: vec![Track::Fixed(20.0)],
                        auto_flow: AutoFlow::Row,
                    })
                    .size(SizeClass::fixed(340.0, 60.0))
                    .gap(10.0)
                    .child(box_node("pinned", 10.0, 10.0).grid_area(0, 2, 1, 1))
                    .child(box_node("auto", 10.0, 10.0)),
            );
            solve(&mut tree);
            rect_of(&tree, "pinned").origin.x
        };
        let filled = run(Repeat::AutoFill);
        let fitted = run(Repeat::AutoFit);
        assert!(
            (filled - 180.0).abs() < 0.01,
            "auto-fill keeps the empty middle track, got {filled}"
        );
        assert!(
            (fitted - 90.0).abs() < 0.01,
            "auto-fit must collapse the empty track, got {fitted}"
        );
    }

    /// A box whose id is computed, so a repeated item can be declared without a literal.
    fn sized_box(id: Id, w: f32, h: f32) -> Decl {
        Decl {
            size_class: SizeClass::fixed(w, h),
            ..Decl::container(id, NodeKind::Box)
        }
    }

    /// The knobs one edit may turn; each step changes exactly one of them.
    #[derive(Clone, Debug)]
    struct Params {
        root_w: f32,
        root_gap: f32,
        items: usize,
        item_side: [f32; 3],
        item_abs: [bool; 3],
        body_len: usize,
        cell_w: f32,
        cell_h: f32,
        nested_gap: f32,
    }

    impl Default for Params {
        fn default() -> Self {
            Self {
                root_w: 340.0,
                root_gap: 4.0,
                items: 3,
                item_side: [20.0, 30.0, 40.0],
                item_abs: [false; 3],
                body_len: 6,
                cell_w: 24.0,
                cell_h: 18.0,
                nested_gap: 0.0,
            }
        }
    }

    fn scene(p: &Params) -> Decl {
        let mut root = Decl::container(Id::str("root"), NodeKind::Column)
            .size(SizeClass::fixed(p.root_w, 500.0))
            .gap(p.root_gap);
        for i in 0..p.items {
            let side = p.item_side[i];
            let mut item = sized_box(Id::str("item").with(i), side, side);
            if p.item_abs[i] {
                item = item.absolute().inset(Insets::new(
                    side,
                    0.0,
                    crate::layout::AUTO,
                    crate::layout::AUTO,
                ));
            }
            root = root.child(item);
        }
        root = root.child(paragraph("body", "x".repeat(p.body_len), 10.0, p.root_w));
        let mut nested = Decl::container(Id::str("nested"), NodeKind::Row).gap(p.nested_gap);
        for i in 0..2 {
            nested = nested.child(sized_box(Id::str("cell").with(i), p.cell_w, p.cell_h));
        }
        root.child(nested)
    }

    /// Turn one knob per step, cycling so every kind of edit shows up more than once.
    fn edit(p: &mut Params, step: usize) {
        match step % 7 {
            0 => p.root_w = 300.0 + (step % 5) as f32 * 40.0,
            1 => p.item_side[0] = 20.0 + (step % 4) as f32 * 10.0,
            2 => p.item_abs[1] = !p.item_abs[1],
            3 => p.body_len = 4 + step % 17,
            4 => p.cell_h = 18.0 + (step % 3) as f32 * 6.0,
            5 => p.items = 2 + step % 2,
            _ => p.root_gap = (step % 3) as f32 * 4.0,
        }
    }

    /// Assert every node of `decl` sits where a from-scratch solve of the same decl put it.
    fn assert_same_layout(incremental: &Tree, full: &Tree, decl: &Decl) {
        let rect = |tree: &Tree| {
            tree.find_by_id(decl.id)
                .and_then(|n| tree.get(n))
                .map(|n| (n.rect, n.visual_rect(), n.mask))
        };
        assert_eq!(
            rect(incremental),
            rect(full),
            "node {:?} differs between the incremental and the full solve",
            decl.id
        );
        for child in &decl.children {
            assert_same_layout(incremental, full, child);
        }
    }

    #[test]
    fn incremental_layout_agrees_with_a_full_one_after_every_edit() {
        // Forty-two edits of one scene, each solved twice: once on the reused tree and once
        // on a fresh one. A stale cache shows up as a differing rect.
        let mut tree = Tree::new();
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let constraints = Constraints::loose(Size::new(800.0, 600.0));
        let mut params = Params::default();
        for step in 0..42 {
            edit(&mut params, step);
            let decl = scene(&params);
            tree.apply(&decl);
            let root = tree.root().expect("root");
            engine.solve(&mut tree, root, constraints);

            let mut fresh = Tree::new();
            fresh.apply(&decl);
            let fresh_root = fresh.root().expect("root");
            BuiltinLayout::new(MonospaceMeasure::default()).solve(
                &mut fresh,
                fresh_root,
                constraints,
            );

            assert_same_layout(&tree, &fresh, &decl);
        }
    }

    #[test]
    fn editing_a_leaf_remeasures_only_its_ancestors() {
        // root → column → [a, nested → leaf]: editing the leaf invalidates the leaf, the
        // nested row, the column and the root, and nothing else.
        let build = |side: f32| {
            Decl::container(Id::str("root"), NodeKind::Column).child(
                Decl::container(Id::str("column"), NodeKind::Column)
                    .child(sized_box(Id::str("a"), 40.0, 10.0))
                    .child(
                        Decl::container(Id::str("nested"), NodeKind::Row).child(sized_box(
                            Id::str("leaf"),
                            side,
                            10.0,
                        )),
                    ),
            )
        };
        let mut tree = Tree::new();
        tree.apply(&build(20.0));
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().expect("root");
        let constraints = Constraints::loose(Size::new(800.0, 600.0));
        let first = engine.solve(&mut tree, root, constraints);
        tree.apply(&build(30.0));
        let second = engine.solve(&mut tree, root, constraints);
        assert!(
            first.remeasured >= 5,
            "the first pass measures the whole tree"
        );
        assert_eq!(
            second.remeasured, 4,
            "only the leaf and its three ancestors may be measured again"
        );
    }

    #[test]
    fn dense_flow_backfills_an_earlier_hole() {
        // Four 60-wide tracks: an item pinned to column 1 pushes the two-column item to
        // columns 2–3, leaving (0,0) free. Sparse flow moves on to row 1; dense returns.
        use crate::layout::{AutoFlow, Track, TrackList};
        let run = |flow| {
            let mut tree = Tree::new();
            tree.apply(
                &Decl::container(Id::str("grid"), NodeKind::Grid)
                    .tracks(TrackList {
                        cols: vec![Track::Fixed(60.0); 4],
                        rows: vec![Track::Fixed(20.0), Track::Fixed(20.0)],
                        auto_flow: flow,
                    })
                    .size(SizeClass::fixed(240.0, 200.0))
                    .child(cell_item("pinned", 0, 1, 1, 1).size(SizeClass::fixed(10.0, 10.0)))
                    .child(
                        auto_item("wide")
                            .size(SizeClass::fixed(10.0, 10.0))
                            .grid_span(1, 2),
                    )
                    .child(auto_item("last").size(SizeClass::fixed(10.0, 10.0))),
            );
            solve(&mut tree);
            (rect_of(&tree, "wide").origin, rect_of(&tree, "last").origin)
        };
        let (wide_sparse, last_sparse) = run(AutoFlow::Row);
        let (wide_dense, last_dense) = run(AutoFlow::RowDense);
        assert!(
            (wide_sparse.x - 120.0).abs() < 0.01 && (wide_dense.x - 120.0).abs() < 0.01,
            "the two-column item skips the pinned cell in both flows: {wide_sparse:?} {wide_dense:?}"
        );
        assert!(
            (last_sparse.x - 0.0).abs() < 0.01 && (last_sparse.y - 20.0).abs() < 0.01,
            "sparse flow must move on to the second row, got {last_sparse:?}"
        );
        assert!(
            (last_dense.x - 0.0).abs() < 0.01 && (last_dense.y - 0.0).abs() < 0.01,
            "dense flow must backfill the hole at (0,0), got {last_dense:?}"
        );
    }

    /// A deficit squeezes a shrinkable child only down to its min_content, never below it.
    #[test]
    fn a_child_never_shrinks_below_its_min_content() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Row)
                .size(SizeClass::fixed(50.0, 30.0))
                .child(text_node(
                    "label",
                    "alpha beta",
                    crate::layout::Grow::Flex(1),
                ))
                .child(box_node("fixed", 90.0, 30.0)),
        );
        solve(&mut tree);
        let label = tree
            .get(tree.find_by_id(Id::str("label")).unwrap())
            .unwrap();
        // "alpha" and "beta" are five characters at six units each.
        assert!(
            label.rect.size.w >= 30.0 - 0.01,
            "squeezed below the longest word: {}",
            label.rect.size.w
        );
        assert!(
            label.rect.size.w < 60.0,
            "the deficit did nothing: {}",
            label.rect.size.w
        );
    }

    /// Spare space goes to a grower, but never past the max it declares.

    fn solve(tree: &mut Tree) -> BuiltinLayout<MonospaceMeasure> {
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().expect("root");
        engine.solve(tree, root, Constraints::loose(Size::new(800.0, 600.0)));
        engine
    }

    #[test]
    fn scrolling_shifts_children_and_clips_them_without_extra_work() {
        // A 40-tall scroller holding two 30-tall children, scrolled by 20: the first
        // moves above the box, the second follows and is cut at the bottom edge.
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(40.0, 40.0))
                .scrollable()
                .scroll_y(20.0)
                .child(box_node("a", 40.0, 30.0))
                .child(box_node("b", 40.0, 30.0)),
        );
        solve(&mut tree);
        let a = tree.get(tree.find_by_id(Id::str("a")).unwrap()).unwrap();
        let b = tree.get(tree.find_by_id(Id::str("b")).unwrap()).unwrap();
        assert!(
            (a.rect.origin.y - -20.0).abs() < 0.01,
            "first child moved up by the offset, got {}",
            a.rect.origin.y
        );
        assert!(
            (b.rect.origin.y - 10.0).abs() < 0.01,
            "second child followed, got {}",
            b.rect.origin.y
        );
        assert!(
            (b.rect.size.h - 30.0).abs() < 0.01,
            "children keep their measured height instead of being squeezed, got {}",
            b.rect.size.h
        );
        // Clipping is the mask's job: it is the container box, not the child rect.
        assert!(
            (b.mask.max_y() - 40.0).abs() < 0.01,
            "second child is clipped at the container edge, got {}",
            b.mask.max_y()
        );
    }

    /// A declared size is a border box: padding sits inside it instead of growing it.
    #[test]
    fn a_declared_size_is_a_border_box() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(300.0, 300.0))
                .child(
                    Decl::container(Id::str("padded"), NodeKind::Column)
                        .size(SizeClass::fixed(100.0, 40.0))
                        .padding(crate::geom::Insets::all(10.0))
                        .child(box_node("kid", 20.0, 20.0)),
                ),
        );
        solve(&mut tree);
        let p = rect_of(&tree, "padded");
        let k = rect_of(&tree, "kid");
        assert_eq!(
            (p.size.w, p.size.h),
            (100.0, 40.0),
            "padding must not grow it"
        );
        assert_eq!(
            (k.origin.x, k.origin.y),
            (10.0, 10.0),
            "content starts past the padding"
        );
    }

    fn node_of(tree: &Tree, id: &'static str) -> Rect {
        let nid = tree.find_by_id(Id::str(id)).expect("node id");
        tree.get(nid).expect("live node").rect
    }

    #[test]
    fn a_percent_inset_resolves_against_the_containing_block() {
        // A percent side is the only way to place a bar by fraction of its parent, which an
        // indeterminate progress indicator needs; it must follow CSS, not the root.
        use crate::layout::{Len, LenInsets};
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(400.0, 300.0))
                .child(
                    Decl::container(Id::str("card"), NodeKind::Column)
                        .size(SizeClass::fixed(200.0, 100.0))
                        .relative()
                        .child(box_node("bar", 20.0, 10.0).absolute().inset(LenInsets::new(
                            Len::Pct(25.0),
                            Len::Pct(50.0),
                            Len::Auto,
                            Len::Auto,
                        ))),
                ),
        );
        solve(&mut tree);
        let card = node_of(&tree, "card");
        let bar = node_of(&tree, "bar");
        assert!(
            (bar.origin.x - (card.origin.x + 50.0)).abs() < 0.01,
            "left 25% of 200 is 50, got {}",
            bar.origin.x - card.origin.x
        );
        assert!(
            (bar.origin.y - (card.origin.y + 50.0)).abs() < 0.01,
            "top 50% of 100 is 50, got {}",
            bar.origin.y - card.origin.y
        );
    }

    #[test]
    fn a_relative_offset_moves_the_hit_test_with_the_paint() {
        // A clip inside a displaced ancestor: the mask has to travel with the pixels, or a
        // click inside the painted box hits nothing and one past it hits anyway.
        use crate::hit::{HitBehavior, HitIndex};
        use crate::layout::Overflow;
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(400.0, 300.0))
                .child(
                    Decl::container(Id::str("mover"), NodeKind::Column)
                        .size(SizeClass::fixed(200.0, 200.0))
                        .relative()
                        .inset(Insets::new(
                            30.0,
                            40.0,
                            crate::layout::AUTO,
                            crate::layout::AUTO,
                        ))
                        .child(
                            Decl::container(Id::str("clip"), NodeKind::Column)
                                .size(SizeClass::fixed(200.0, 200.0))
                                .clip(crate::layout::Clip {
                                    x: Overflow::Hidden,
                                    y: Overflow::Hidden,
                                })
                                .child(box_node("wide", 300.0, 100.0)),
                        ),
                ),
        );
        solve(&mut tree);
        let wide_id = tree.find_by_id(Id::str("wide")).unwrap();
        let clip = tree.get(tree.find_by_id(Id::str("clip")).unwrap()).unwrap();
        let wide = tree.get(wide_id).unwrap();
        assert!(
            (wide.mask.max_x() - clip.visual_rect().max_x()).abs() < 0.01,
            "mask ends at {}, the clipped pixels at {}",
            wide.mask.max_x(),
            clip.visual_rect().max_x()
        );
        let idx = HitIndex::build(&tree, |nid| {
            if tree.get(nid).map(|n| n.id) == Some(Id::str("root")) {
                HitBehavior::Ignore
            } else {
                HitBehavior::Opaque
            }
        });
        assert_eq!(
            idx.topmost(crate::geom::Point::new(210.0, 50.0)),
            Some(wide_id),
            "a click on painted pixels inside the clip found no target"
        );
        assert_eq!(
            idx.topmost(crate::geom::Point::new(250.0, 50.0)),
            None,
            "a click past the clip's edge still hit something"
        );
    }

    #[test]
    fn a_visible_container_does_not_clip_its_child() {
        // Clipping is opt-in: a card that declares nothing must let a wide child paint,
        // or text whose ink exceeds its own rect gets cut for no reason.
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column).child(
                Decl::container(Id::str("card"), NodeKind::Column)
                    .size(SizeClass::fixed(100.0, 100.0))
                    .child(box_node("wide", 160.0, 20.0)),
            ),
        );
        solve(&mut tree);
        let wide = tree.get(tree.find_by_id(Id::str("wide")).unwrap()).unwrap();
        assert!(
            wide.mask.size.w + 0.01 >= wide.rect.size.w,
            "a visible parent cut its child's mask: mask {:?}, rect {:?}",
            wide.mask,
            wide.rect
        );
    }

    #[test]
    fn scroll_offset_moves_content_not_the_clip_rect() {
        // Scrolling moves the content inside a fixed viewport; the viewport itself must
        // not travel with it, or the clip edge follows the text and stays invisible.
        let run = |scroll: f32| {
            let mut tree = Tree::new();
            tree.apply(
                &Decl::container(Id::str("root"), NodeKind::Column)
                    .size(SizeClass::fixed(40.0, 40.0))
                    .scrollable()
                    .scroll_y(scroll)
                    .child(box_node("a", 40.0, 30.0))
                    .child(box_node("b", 40.0, 30.0)),
            );
            solve(&mut tree);
            let b = tree.get(tree.find_by_id(Id::str("b")).unwrap()).unwrap();
            (b.mask, b.rect.origin.y)
        };
        let (mask_rest, y_rest) = run(0.0);
        let (mask_scrolled, y_scrolled) = run(20.0);
        assert_eq!(
            mask_rest, mask_scrolled,
            "the viewport moved with the content: {mask_rest:?} vs {mask_scrolled:?}"
        );
        assert!(
            (y_rest - y_scrolled - 20.0).abs() < 0.01,
            "content must move by the offset: {y_rest} -> {y_scrolled}"
        );
    }

    #[test]
    fn a_clipping_container_intersects_its_child_mask() {
        // The opt-in half of the rule: once a container declares `Hidden`, its box is the
        // clip again, so the wide child's mask stops at the card's edge.
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column).child(
                Decl::container(Id::str("card"), NodeKind::Column)
                    .size(SizeClass::fixed(100.0, 100.0))
                    .clip(crate::layout::Clip {
                        x: crate::layout::Overflow::Hidden,
                        y: crate::layout::Overflow::Hidden,
                    })
                    .child(box_node("wide", 160.0, 20.0)),
            ),
        );
        solve(&mut tree);
        let wide = tree.get(tree.find_by_id(Id::str("wide")).unwrap()).unwrap();
        assert_eq!(
            (wide.mask.size.w, wide.mask.size.h),
            (100.0, 100.0),
            "a declared clip must cut the child's mask to the card box, got {:?}",
            wide.mask
        );
    }

    /// A formula cannot reflow — its box is the display list's extents — and it reports the
    /// formula's ascent as its baseline, which is how a row aligns text to a formula.
    #[test]
    fn a_formula_box_is_its_display_list_and_its_baseline_is_its_ascent() {
        let mut tree = Tree::new();
        let mut math = Decl::container(Id::str("math"), NodeKind::Math);
        math.data = crate::NodeData::Math {
            formula: std::sync::Arc::new(crate::math::Formula {
                width: 80.0,
                ascent: 12.0,
                descent: 4.0,
                items: Vec::new(),
                errors: Vec::new(),
            }),
            family: "Test Math".into(),
            ink: crate::geom::Color::WHITE,
        };
        math.size_class = crate::layout::SizeClass {
            width: crate::layout::AxisSize::hug(0.0, 400.0),
            height: crate::layout::AxisSize::hug(0.0, 0.0),
            ..crate::layout::SizeClass::fixed(0.0, 0.0)
        };
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(math),
        );
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(&mut tree, root, Constraints::loose(Size::new(200.0, 200.0)));

        let node = tree.get(tree.find_by_id(Id::str("math")).unwrap()).unwrap();
        assert_eq!(
            (node.rect.size.w, node.rect.size.h),
            (80.0, 16.0),
            "the box is the formula's width and its ascent plus descent"
        );
        assert_eq!(node.baseline, 12.0, "the baseline is the formula's ascent");
    }

    /// A paragraph of `before` + one held atom + `after`. The holder is a child of the text
    /// node, named by the atom.
    fn holder_paragraph(
        id: &'static str,
        before: &str,
        after: &str,
        wrap: f32,
        align: crate::TextAlign,
        holder: Decl,
        declared: Size,
    ) -> Decl {
        let body = format!("{before}{}{after}", crate::text::OBJECT);
        let mut block = crate::text::TextBlock::plain(body, crate::text::TextStyle::new(10.0));
        block.align = align;
        let atom = crate::text::AtomBox::new(before.len(), declared).holder(holder.id);
        block.atoms = vec![atom];
        let mut decl = text_decl(id, block, wrap, 14.0);
        decl.children.push(std::rc::Rc::new(holder));
        decl
    }

    fn solve_holder(tree: &mut Tree, width: f32, height: f32) {
        let mut engine = BuiltinLayout::new(MonospaceMeasure::default());
        let root = tree.root().unwrap();
        engine.solve(tree, root, Constraints::loose(Size::new(width, height)));
    }

    /// A holder's measured size is the box its atom reserves: the line grows around the
    /// widget and the text after it starts at the widget's right edge.
    #[test]
    fn an_inline_holder_gives_its_atom_the_measured_size() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    f32::INFINITY,
                    crate::TextAlign::Start,
                    box_node("chip", 30.0, 20.0),
                    Size::ZERO,
                )),
        );
        solve_holder(&mut tree, 200.0, 200.0);

        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let layout = node
            .inline
            .as_ref()
            .expect("a measured text leaf has lines");
        assert_eq!(layout.lines.len(), 1, "the box fits beside the text");
        let atom = &layout.lines[0].atoms[0];
        assert_eq!(
            (atom.size.w, atom.size.h),
            (30.0, 20.0),
            "the atom takes the holder's size, not the builder's"
        );
        assert_eq!(
            atom.y, 0.0,
            "a box with no baseline sits by its bottom edge"
        );
        assert_eq!(
            layout.width, 42.0,
            "6 + 30 + 6: the box takes its place in the line"
        );
    }

    /// The held node is placed where its atom landed, which is not the text node's origin.
    #[test]
    fn an_inline_holder_is_placed_where_its_atom_lands() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    f32::INFINITY,
                    crate::TextAlign::Start,
                    box_node("chip", 30.0, 20.0),
                    Size::ZERO,
                )),
        );
        solve_holder(&mut tree, 200.0, 200.0);

        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let layout = node.inline.clone().unwrap();
        let atom = layout.lines[0].atoms[0];
        let line = layout.line_origin(
            0,
            node.rect.origin,
            node.rect.size.h,
            node.wrap_width,
            crate::LineEdges::default(),
            crate::TextAlign::Start,
        );
        let expected = atom.rect(line);
        let chip = tree.get(tree.find_by_id(Id::str("chip")).unwrap()).unwrap();
        assert_eq!(chip.rect, expected, "the node sits exactly on its atom");
        assert_eq!(
            chip.rect.origin.x,
            node.rect.origin.x + 6.0,
            "the first letter comes before it"
        );
        assert_eq!(
            (chip.rect.size.w, chip.rect.size.h),
            (30.0, 20.0),
            "the node keeps the size its atom reserved"
        );
    }

    /// A holder that reports a baseline is aligned by it, not by its box bottom: a 20px text
    /// child puts the line's baseline 20 down, whatever its own height is.
    #[test]
    fn a_holder_with_a_baseline_sits_on_it() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    f32::INFINITY,
                    crate::TextAlign::Start,
                    text_node_at("chip", "xx", 20.0, crate::layout::Grow::No),
                    Size::ZERO,
                )),
        );
        solve_holder(&mut tree, 200.0, 200.0);

        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let layout = node.inline.as_ref().unwrap();
        let atom = &layout.lines[0].atoms[0];
        assert_eq!(
            layout.lines[0].height, 26.0,
            "20 above the baseline and 6 below it, the child's own descent"
        );
        assert_eq!(atom.y, 0.0, "the shared baseline is the child's");
    }

    /// A container derives no baseline. CSS Inline 3 §A.3 synthesises one from the bottom
    /// margin edge, so the box's bottom sits on the baseline — aligning its top there would
    /// read as the widget floating half a line above its text.
    #[test]
    fn a_holder_without_a_baseline_sits_on_its_bottom_edge() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    f32::INFINITY,
                    crate::TextAlign::Start,
                    box_node("chip", 30.0, 20.0),
                    Size::ZERO,
                )),
        );
        solve_holder(&mut tree, 200.0, 200.0);

        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let layout = node.inline.as_ref().unwrap();
        let line = &layout.lines[0];
        let atom = &line.atoms[0];
        assert_eq!(
            line.baseline, 20.0,
            "a baseline-less box stands its own height above the line's baseline"
        );
        assert_eq!(
            atom.y + atom.size.h,
            line.baseline,
            "so its bottom edge sits on the baseline"
        );
    }

    /// A holder wider than the block takes a line of its own; it is never split.
    #[test]
    fn a_holder_wider_than_the_line_takes_a_line_of_its_own() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    40.0,
                    crate::TextAlign::Start,
                    box_node("chip", 60.0, 20.0),
                    Size::ZERO,
                )),
        );
        solve_holder(&mut tree, 200.0, 200.0);

        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let layout = node.inline.as_ref().unwrap();
        assert_eq!(
            layout.lines.len(),
            3,
            "one line each for the text and the box"
        );
        assert_eq!(
            layout.lines[1].atoms.len(),
            1,
            "the oversized box stands alone"
        );
        assert_eq!(layout.lines[1].width, 60.0);
    }

    /// A centred line moves its holder with it: placement reads the same origin the painter
    /// does, so the node lands where the line is drawn.
    #[test]
    fn a_centred_line_puts_its_holder_at_the_aligned_origin() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    200.0,
                    crate::TextAlign::Center,
                    box_node("chip", 30.0, 20.0),
                    Size::ZERO,
                )),
        );
        solve_holder(&mut tree, 200.0, 200.0);

        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let chip = tree.get(tree.find_by_id(Id::str("chip")).unwrap()).unwrap();
        assert_eq!(
            chip.rect.origin.x - node.rect.origin.x,
            79.0 + 6.0,
            "(200 - 42) / 2 leaves the line at 79, and the box follows one letter"
        );
    }

    /// A parent too narrow for the widget cannot narrow the paragraph below it: the atom is
    /// an unbreakable box, so it floors the width the hug axis may settle on.
    #[test]
    fn a_narrow_parent_never_squeezes_a_holder_below_its_own_width() {
        let mut tree = Tree::new();
        tree.apply(
            &Decl::container(Id::str("root"), NodeKind::Row)
                .size(SizeClass::fixed(60.0, 200.0))
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    f32::INFINITY,
                    crate::TextAlign::Start,
                    box_node("chip", 120.0, 20.0),
                    Size::ZERO,
                )),
        );
        solve_holder(&mut tree, 400.0, 400.0);

        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let chip = tree.get(tree.find_by_id(Id::str("chip")).unwrap()).unwrap();
        assert_eq!(
            node.rect.size.w, 120.0,
            "the box decides the width, not the words around it"
        );
        assert_eq!(
            (chip.rect.size.w, chip.rect.size.h),
            (120.0, 20.0),
            "the widget keeps its size inside a narrower row"
        );
    }

    /// A holder that grows re-wraps the paragraph it sits in on the next solve: the size is
    /// stamped by the text node's measure, so a change below it must reach that measure.
    #[test]
    fn a_holder_that_grows_rewraps_its_paragraph() {
        let root = |chip_width: f32| {
            Decl::container(Id::str("root"), NodeKind::Column)
                .cross_align(0.0)
                .child(holder_paragraph(
                    "para",
                    "a",
                    "b",
                    40.0,
                    crate::TextAlign::Start,
                    box_node("chip", chip_width, 20.0),
                    Size::ZERO,
                ))
        };
        let mut tree = Tree::new();
        tree.apply(&root(20.0));
        solve_holder(&mut tree, 200.0, 200.0);
        let first = tree
            .get(tree.find_by_id(Id::str("para")).unwrap())
            .unwrap()
            .inline
            .clone()
            .unwrap();
        assert_eq!(
            first.lines.len(),
            1,
            "a 20-wide box keeps the line together"
        );

        tree.apply(&root(60.0));
        solve_holder(&mut tree, 200.0, 200.0);
        let node = tree.get(tree.find_by_id(Id::str("para")).unwrap()).unwrap();
        let second = node.inline.as_ref().unwrap();
        assert_eq!(
            second.lines.len(),
            3,
            "the grown box no longer fits on the words' line"
        );
        assert_eq!(second.lines[1].width, 60.0, "the line takes the new width");
    }
}
