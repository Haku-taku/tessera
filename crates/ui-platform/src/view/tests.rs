//! Tests for this module, split from the source file; one module per contract area.

mod contracts {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;
    use ui_core::geom::{Color, Corners};
    use ui_core::layout::{AxisSize, Grow, SizeClass};

    /// Records which node the keyboard activated.
    struct Counting {
        activated: Vec<Id>,
        scrolled: Vec<(Id, f32)>,
        expand: bool,
        /// Whether the bar owns drags, so "a drag is not a click" can be exercised.
        draggable: bool,
    }

    impl App for Counting {
        fn view(&mut self, f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Demo {
                expand: self.expand,
                draggable: self.draggable,
            }
            .view(f)
        }
        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            use ui_core::change::Change as C;
            match change {
                C::Activated => self.activated.push(id),
                C::Value(v) => self.scrolled.push((id, -v as f32)),
                _ => {}
            }
            true
        }
    }

    struct Demo {
        expand: bool,
        /// Whether the bar subscribes to drags; the click-vs-drag test flips this.
        draggable: bool,
    }

    /// The demo's bar as a control: a completed click means "activated".
    struct DemoBar {
        draggable: bool,
    }

    impl crate::Element for DemoBar {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            bar_decl(self.draggable)
        }

        fn change(
            &self,
            state: &crate::WidgetState,
            _bounds: Rect,
        ) -> Option<ui_core::change::Change> {
            (state.activation() && !state.disabled).then_some(ui_core::change::Change::Activated)
        }
    }

    fn bar_decl(draggable: bool) -> Decl {
        Decl {
            id: Id::str("bar"),
            kind: NodeKind::Box,
            shapes: vec![ui_core::draw::Shape::Box {
                corners: ui_core::geom::Corners::ZERO,
                fill: ui_core::geom::Color::WHITE,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }],
            data: NodeData::Container(ui_core::tree::ContainerParams::default()),
            size_class: SizeClass {
                width: AxisSize::fill(0.0, 100.0),
                height: AxisSize::fixed(40.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            },
            ink_margin: Insets::ZERO,
            box_model: Default::default(),
            offset: ui_core::geom::Point::new(0.0, 0.0),
            cursor: None,
            // The bar is the demo's one interactive, keyboard-reachable node.
            sense: ui_core::Sense::HOVER
                | ui_core::Sense::CLICK
                | ui_core::Sense::FOCUSABLE
                | if draggable {
                    ui_core::Sense::DRAG
                } else {
                    ui_core::Sense::NONE
                },
            disabled: false,
            grid: ui_core::GridItem::default(),
            placement: ui_core::layout::Placement::default(),
            env: None,
            drag: None,
            drag_out: false,
            drop_types: None,
            children: Vec::new(),
        }
    }

    impl App for Demo {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let mut root = crate::Container::new(Id::str("root"), NodeKind::Column)
                .padding(Insets::all(10.0))
                .gap(8.0)
                .child(Box::new(DemoBar {
                    draggable: self.draggable,
                }));
            if self.expand {
                root = root.child_decl(Decl {
                    id: Id::str("extra"),
                    kind: NodeKind::Box,
                    shapes: vec![ui_core::draw::Shape::Box {
                        corners: ui_core::geom::Corners::ZERO,
                        fill: ui_core::geom::Color::WHITE,
                        gradient: None,
                        border: None,
                        shadow: None,
                        image: None,
                    }],
                    data: NodeData::Container(ui_core::tree::ContainerParams::default()),
                    size_class: SizeClass::fixed(50.0, 30.0),
                    ink_margin: Insets::ZERO,
                    box_model: Default::default(),
                    offset: ui_core::geom::Point::new(0.0, 0.0),
                    cursor: None,
                    // The extra box is Tab-reachable, like the bar.
                    sense: ui_core::Sense::HOVER
                        | ui_core::Sense::CLICK
                        | ui_core::Sense::FOCUSABLE,
                    disabled: false,
                    grid: ui_core::GridItem::default(),
                    placement: ui_core::layout::Placement::default(),
                    env: None,
                    drag: None,
                    drag_out: false,
                    drop_types: None,
                    children: Vec::new(),
                });
            }
            Box::new(root)
        }
    }

    /// An app whose one node accepts text and keeps whatever the engine reports.
    struct Typing {
        text: String,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl App for Typing {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("field"), NodeKind::Row)
                    .size(SizeClass::fixed(100.0, 40.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone()),
            )
        }

        fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Text(next) = change {
                self.seen.borrow_mut().push(next.clone());
                self.text = next;
            }
            true
        }
    }

    /// A control sizes itself to its parent through the constraints it was measured
    /// under; build runs before layout, so a resize reaches it one frame later.
    #[test]
    fn a_control_reads_the_slot_its_parent_offers() {
        struct Sizing {
            seen: std::rc::Rc<std::cell::RefCell<Vec<Option<f32>>>>,
            width: f32,
        }
        impl App for Sizing {
            fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                let slot = cx
                    .constraints_of(Id::str("field"))
                    .map(|constraints| constraints.max.w);
                self.seen.borrow_mut().push(slot);
                crate::from_decl(
                    Decl::container(Id::str("root"), ui_core::NodeKind::Column)
                        .size(SizeClass::fixed(self.width, 60.0))
                        .child(Decl::container(Id::str("field"), ui_core::NodeKind::Box)),
                )
            }
        }
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Sizing {
            seen: seen.clone(),
            width: 200.0,
        });
        let mut target = OffscreenTarget::new(400, 200);
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        assert_eq!(
            seen.borrow().as_slice(),
            [None, Some(200.0)],
            "the first frame has no layout yet, the second learns its slot"
        );
        lp.app_mut().width = 300.0;
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        assert_eq!(
            seen.borrow()[2..],
            [Some(200.0), Some(300.0)],
            "a resize reaches the control on the frame after the one that applied it"
        );
    }

    #[test]
    fn typing_reaches_the_focused_text_input_as_the_whole_buffer() {
        // Keys only go to a focused input, and the app must receive the whole buffer.
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Typing {
            text: String::new(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        let pos = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        assert!(
            lp.dispatch(ui_core::Event::Text("hé".to_string())),
            "an edit asks for a repaint"
        );
        lp.frame(&mut target, None);
        assert_eq!(seen.borrow().as_slice(), ["hé"]);
        assert_eq!(
            lp.editing,
            Some((Id::str("field"), ui_core::TextSelection::collapsed(3))),
            "the caret follows the edit"
        );
    }

    /// A canvas that opens a field on a press has no field to press: the build asks for the
    /// keyboard itself, and typing reaches the field from the frame after the ask.
    #[test]
    fn a_build_can_ask_for_the_keyboard_focus() {
        struct Opening {
            open: bool,
            text: String,
            seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        }
        struct Canvas {
            open: bool,
            text: String,
        }
        impl crate::Element for Canvas {
            fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
                let mut root = Decl::container(Id::str("root"), NodeKind::Box)
                    .size(SizeClass::fixed(200.0, 120.0))
                    .sense(ui_core::Sense::CLICK)
                    .background(ui_core::geom::Color::WHITE, Corners::ZERO);
                if self.open {
                    root = root.child(
                        Decl::container(Id::str("field"), NodeKind::Row)
                            .size(SizeClass::fixed(100.0, 40.0))
                            .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                            .background(
                                ui_core::geom::Color::from_rgba8(20, 20, 20, 255),
                                Corners::ZERO,
                            )
                            .text_input(self.text.clone()),
                    );
                }
                root
            }

            fn change(
                &self,
                state: &crate::WidgetState,
                _bounds: Rect,
            ) -> Option<ui_core::change::Change> {
                state
                    .activation()
                    .then_some(ui_core::change::Change::Activated)
            }
        }
        impl App for Opening {
            fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                if self.open {
                    cx.request_focus(Id::str("field"));
                }
                Box::new(Canvas {
                    open: self.open,
                    text: self.text.clone(),
                })
            }

            fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
                match change {
                    ui_core::change::Change::Activated => self.open = true,
                    ui_core::change::Change::Text(next) => {
                        self.seen.borrow_mut().push(next.clone());
                        self.text = next;
                    }
                    _ => {}
                }
                true
            }
        }

        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Opening {
            open: false,
            text: String::new(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        assert_eq!(lp.focused(), None, "nothing has a field yet");

        let pos = ui_core::geom::Point::new(20.0, 20.0);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerPressed { pos, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos, button });
        // One frame applies the click, the next declares the field and asks for the keys.
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        assert_eq!(
            lp.focused(),
            Some(Id::str("field")),
            "the ask lands on the node the same build declared"
        );
        assert!(lp.dispatch(ui_core::Event::Text("hi".to_string())));
        lp.frame(&mut target, None);
        assert_eq!(seen.borrow().as_slice(), ["hi"]);
    }

    /// A `Typing` loop focused on `field` with `text` as its starting buffer.
    fn focused_typing(
        text: &str,
    ) -> (
        AppLoop<Typing>,
        std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    ) {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Typing {
            text: text.to_string(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        let pos = ui_core::geom::Point::new(20.0, 20.0);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerPressed { pos, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos, button });
        (lp, seen)
    }

    fn press_ctrl<A: App>(lp: &mut AppLoop<A>, key: char, mods: ui_core::Modifiers) -> bool {
        lp.dispatch(ui_core::Event::Modifiers(mods));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char(key),
        })
    }

    fn undo<A: App>(lp: &mut AppLoop<A>) -> bool {
        press_ctrl(
            lp,
            'z',
            ui_core::Modifiers {
                ctrl: true,
                ..ui_core::Modifiers::default()
            },
        )
    }

    fn redo<A: App>(lp: &mut AppLoop<A>) -> bool {
        press_ctrl(
            lp,
            'z',
            ui_core::Modifiers {
                ctrl: true,
                shift: true,
                ..ui_core::Modifiers::default()
            },
        )
    }

    /// The buffer the engine most recently handed the app.
    fn last_text(seen: &std::rc::Rc<std::cell::RefCell<Vec<String>>>) -> Option<String> {
        seen.borrow().last().cloned()
    }

    #[test]
    fn typing_merges_into_one_undo_group_and_redo_restores_it() {
        // A run of single-character insertions is one user action: undo must not walk back
        // one keystroke at a time.
        let (mut lp, seen) = focused_typing("");
        lp.dispatch(ui_core::Event::Text("a".to_string()));
        lp.dispatch(ui_core::Event::Text("b".to_string()));
        assert_eq!(seen.borrow().as_slice(), ["a", "ab"]);

        assert!(undo(&mut lp), "undo asks for a repaint");
        assert_eq!(
            last_text(&seen).as_deref(),
            Some(""),
            "one undo returns to the run's start"
        );
        assert_eq!(
            lp.editing,
            Some((Id::str("field"), ui_core::TextSelection::collapsed(0))),
            "and the caret returns to where the run began"
        );

        assert!(!undo(&mut lp), "an empty undo stack asks for no frame");
        assert_eq!(last_text(&seen).as_deref(), Some(""), "and changes nothing");

        assert!(redo(&mut lp), "redo asks for a repaint");
        assert_eq!(
            last_text(&seen).as_deref(),
            Some("ab"),
            "redo restores the whole merged group"
        );
        assert_eq!(
            lp.editing,
            Some((Id::str("field"), ui_core::TextSelection::collapsed(2)))
        );
    }

    #[test]
    fn a_pause_longer_than_a_second_ends_the_typing_run() {
        // Structure alone is not the rule: a typist who stops for a moment has finished a
        // thought, and undo must take back that burst rather than everything typed since the
        // field was touched.
        let (mut lp, seen) = focused_typing("");
        lp.clock = FrameClock::scripted(vec![0.0]);
        lp.dispatch(ui_core::Event::Text("a".to_string()));
        lp.dispatch(ui_core::Event::Text("b".to_string()));
        lp.clock.advance_by(1.5);
        lp.dispatch(ui_core::Event::Text("c".to_string()));
        assert_eq!(seen.borrow().as_slice(), ["a", "ab", "abc"]);
        assert!(undo(&mut lp));
        assert_eq!(
            last_text(&seen).as_deref(),
            Some("ab"),
            "one undo takes back the second burst alone"
        );
        assert!(undo(&mut lp));
        assert_eq!(
            last_text(&seen).as_deref(),
            Some(""),
            "and the burst before the pause is its own group"
        );
    }

    #[test]
    fn a_new_edit_after_undo_clears_the_redo_stack() {
        // Editing after an undo forks the history: the undone state must not be reachable
        // again, or redo would resurrect text the buffer never held.
        let (mut lp, seen) = focused_typing("");
        lp.dispatch(ui_core::Event::Text("a".to_string()));
        undo(&mut lp);
        assert_eq!(last_text(&seen).as_deref(), Some(""));

        lp.dispatch(ui_core::Event::Text("b".to_string()));
        assert_eq!(last_text(&seen).as_deref(), Some("b"));

        assert!(!redo(&mut lp), "the id still fires, but the stack is empty");
        assert_eq!(
            last_text(&seen).as_deref(),
            Some("b"),
            "the superseded undo is not redone"
        );
        undo(&mut lp);
        assert_eq!(
            last_text(&seen).as_deref(),
            Some(""),
            "and the new edit is the only group left"
        );
    }

    #[test]
    fn a_deletion_is_its_own_undo_group() {
        // Deleting ends the typing run: undoing the delete must keep the text typed before
        // it, and undoing again must take the whole run.
        let (mut lp, seen) = focused_typing("");
        lp.dispatch(ui_core::Event::Text("a".to_string()));
        lp.dispatch(ui_core::Event::Text("b".to_string()));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Backspace,
        });
        assert_eq!(last_text(&seen).as_deref(), Some("a"));

        undo(&mut lp);
        assert_eq!(
            last_text(&seen).as_deref(),
            Some("ab"),
            "the delete undoes alone, without the typing before it"
        );
        undo(&mut lp);
        assert_eq!(
            last_text(&seen).as_deref(),
            Some(""),
            "and the typing run is still one group of its own"
        );
    }

    #[test]
    fn a_caret_move_between_keystrokes_starts_a_new_undo_group() {
        // Two insertions at different carets are two actions: merging them would make one
        // undo move text the user placed deliberately.
        let (mut lp, seen) = focused_typing("");
        lp.dispatch(ui_core::Event::Text("a".to_string()));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Left,
        });
        lp.dispatch(ui_core::Event::Text("b".to_string()));
        assert_eq!(
            last_text(&seen).as_deref(),
            Some("ba"),
            "the second character lands at the moved caret"
        );

        undo(&mut lp);
        assert_eq!(
            last_text(&seen).as_deref(),
            Some("a"),
            "undo returns to the caret move, not past it"
        );
        undo(&mut lp);
        assert_eq!(last_text(&seen).as_deref(), Some(""));
    }

    /// The text's own commands stay the text's: an app binding for `Ctrl+A` must not take the
    /// press that selects the field's contents.
    #[test]
    fn a_caret_keeps_the_text_commands_from_an_app_binding() {
        let (mut lp, _seen) = focused_typing("abc");
        let app = ui_core::ShortcutId("app.select-all");
        lp.bind_shortcut(
            ui_core::KeyChord::new(
                ui_core::Key::Char('a'),
                ui_core::Modifiers {
                    ctrl: true,
                    ..ui_core::Modifiers::default()
                },
            ),
            app,
        );
        let ctrl = ui_core::Modifiers {
            ctrl: true,
            ..ui_core::Modifiers::default()
        };
        press_ctrl(&mut lp, 'a', ctrl);
        assert_eq!(
            lp.editing,
            Some((Id::str("field"), ui_core::TextSelection::new(0, 3))),
            "select-all reached the field rather than the app's binding"
        );
    }

    /// One selectable paragraph with `pad` of its own on every side.
    fn paragraph_decl(pad: f32) -> Decl {
        paragraph_named("para", pad)
    }

    /// The same paragraph under a name, for a fixture with several of them.
    fn paragraph_named(id: &'static str, pad: f32) -> Decl {
        Decl {
            id: Id::str(id),
            kind: NodeKind::Text,
            shapes: Vec::new(),
            data: NodeData::Text {
                ink: Color::BLACK,
                block: std::sync::Arc::new(ui_core::TextBlock {
                    runs: vec![ui_core::TextRun::new(
                        "drag across this paragraph to select it",
                        ui_core::TextStyle::new(14.0),
                    )],
                    align: ui_core::TextAlign::Start,
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
                width: AxisSize::hug(0.0, 600.0),
                height: AxisSize::hug(14.0, 20.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            ink_margin: Insets::ZERO,
            box_model: ui_core::layout::BoxModel {
                padding: Insets::all(pad),
                ..ui_core::layout::BoxModel::default()
            },
            offset: ui_core::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: ui_core::Sense::SELECT,
            disabled: false,
            grid: ui_core::GridItem::default(),
            placement: ui_core::layout::Placement::default(),
            env: None,
            drag: None,
            drag_out: false,
            drop_types: None,
            children: Vec::new(),
        }
    }

    /// An app whose one node is a selectable paragraph, so a drag has something to select.
    struct Selectable;

    /// Two selectable paragraphs stacked, for a drag that crosses the boundary between them.
    struct TwoParagraphs;

    impl App for TwoParagraphs {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("stack"), NodeKind::Column)
                    .size(SizeClass::fixed(600.0, 100.0))
                    .gap(8.0)
                    .child(paragraph_named("first", 0.0))
                    .child(paragraph_named("second", 0.0)),
            )
        }
    }

    #[test]
    fn a_drag_that_leaves_its_paragraph_selects_the_blocks_it_crossed() {
        // A paragraph is one text node; the next one is another. A drag from the middle of
        // the first to the start of the second must select the first's tail, and the copy
        // must carry both blocks, one line each.
        use crate::clipboard::{ClipboardCommand, ClipboardKind};
        let mut lp = AppLoop::new(TwoParagraphs);
        let mut target = OffscreenTarget::new(600, 120);
        lp.frame(&mut target, None);
        let from = ui_core::geom::Point::new(120.0, 10.0);
        let to = ui_core::geom::Point::new(120.0, 38.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        assert!(
            lp.dispatch(ui_core::Event::PointerMoved { pos: to }),
            "crossing into the next block must ask for a repaint"
        );
        let spans = &lp.selection_spans;
        assert_eq!(spans.len(), 2, "both blocks are covered: {spans:?}");
        assert_eq!(spans[0].0, Id::str("first"));
        assert_eq!(spans[1].0, Id::str("second"));
        assert!(
            spans[0].1.range().start > 0,
            "the first block keeps only the bytes after the press"
        );
        assert_eq!(
            spans[1].1.range().start,
            0,
            "the second block starts at its own beginning"
        );
        assert!(
            lp.selection()
                .is_some_and(|(id, sel)| id == Id::str("second") && !sel.is_collapsed()),
            "the caret end is in the block the drag reached"
        );

        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            ctrl: true,
            ..Default::default()
        }));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char('c'),
        });
        let copied = lp.take_clipboard_commands();
        let ClipboardCommand::Copy { content, kind } = copied.first().expect("nothing was copied")
        else {
            panic!("a copy must ask the host to copy");
        };
        assert_eq!(*kind, ClipboardKind::Selection);
        let crate::clipboard::ClipboardContent::Text(text) = content else {
            panic!("a selection copies text, not bytes");
        };
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines.len(), 2, "one line per selected block, got {text:?}");
        let body = "drag across this paragraph to select it";
        assert!(
            body.ends_with(lines[0]) && lines[0].len() < body.len(),
            "the first line is the first block's tail, got {lines:?}"
        );
        assert!(
            body.starts_with(lines[1]),
            "the second line is the second block's head, got {lines:?}"
        );
    }

    impl App for Selectable {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(paragraph_decl(0.0))
        }
    }

    /// A selectable paragraph inside a hoverable box, so the pointer can leave the text
    /// without leaving the hovered node.
    struct TextInBox;

    impl App for TextInBox {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("host"), NodeKind::Column)
                    .size(SizeClass::fixed(600.0, 100.0))
                    .sense(ui_core::Sense::HOVER)
                    .child(paragraph_named("text", 0.0)),
            )
        }
    }

    #[test]
    fn selectable_text_shows_the_text_cursor_without_the_app_naming_one() {
        // The engine gave the paragraph selectability, so it owes it the I-beam: an app that
        // also had to declare `CursorShape::Text` would duplicate a rule, and would have to
        // lie on text it never made selectable. A shape reaches the target only with a
        // frame, so a move that changes nothing but the shape still has to ask for one.
        let mut target = OffscreenTarget::new(600, 120);
        let mut lp = AppLoop::new(TextInBox);
        lp.frame(&mut target, None);

        let over_text = ui_core::geom::Point::new(30.0, 10.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: over_text });
        let out = lp.frame_with_dt(&mut target, Some(over_text), 0.0);
        assert_eq!(
            out.cursor,
            ui_core::CursorShape::Text,
            "a selectable paragraph answers with the text cursor on its own"
        );

        // Same hovered node, different shape: only the shape comparison can demand this.
        // The flag survives a frame, so what the hover move set is taken and dropped first.
        let _ = lp.take_dispatched();
        let over_box = ui_core::geom::Point::new(300.0, 80.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: over_box });
        assert!(
            lp.take_dispatched(),
            "a move that changes only the cursor shape still needs a frame"
        );
        let out = lp.frame_with_dt(&mut target, Some(over_box), 0.0);
        assert_eq!(
            out.cursor,
            ui_core::CursorShape::Arrow,
            "off the text the box's own shape applies"
        );
    }

    #[test]
    fn a_shift_press_extends_the_selection_it_finds() {
        // Shift is the selection's own modifier: a press that holds it moves the head and
        // keeps the anchor, even into the next block.
        let mut lp = AppLoop::new(TwoParagraphs);
        let mut target = OffscreenTarget::new(600, 120);
        lp.frame(&mut target, None);
        let from = ui_core::geom::Point::new(120.0, 10.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..Default::default()
        }));
        // Inside the second paragraph's middle, not its start (a range with an empty head
        // block is dropped) nor its bottom edge where the offset clamps to the end.
        let to = ui_core::geom::Point::new(560.0, 36.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        assert!(
            lp.dispatch(ui_core::Event::PointerPressed {
                pos: to,
                button: ui_core::PointerButton::Primary,
            }),
            "extending the selection must ask for a repaint"
        );
        let spans = &lp.selection_spans;
        assert_eq!(spans.len(), 2, "both blocks are covered: {spans:?}");
        assert_eq!(spans[0].0, Id::str("first"));
        assert_eq!(spans[1].0, Id::str("second"));
        assert!(
            spans[0].1.range().start > 0,
            "the anchor stays where the first press left it"
        );
        assert_eq!(
            spans[1].1.range().start,
            0,
            "the head reaches the block the shift press hit"
        );
    }

    #[test]
    fn a_press_inside_the_selection_carries_its_text_out_as_a_drag() {
        // Selecting text and then dragging it is the engine's own gesture: the payload is
        // the same text Copy would take, and the host gets an image of the selected lines
        // instead of the block around them.
        let y = 8.0;
        let button = ui_core::PointerButton::Primary;
        let mut lp = AppLoop::new(Selectable);
        let mut target = OffscreenTarget::new(600, 120);
        lp.frame(&mut target, None);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: ui_core::geom::Point::new(1.0, y),
            button,
        });
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(220.0, y),
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: ui_core::geom::Point::new(220.0, y),
            button,
        });
        let selected = lp.selected_text().expect("the drag selected text");
        let ends = lp.selection().expect("the selection survives the release");

        let inside = ui_core::geom::Point::new(60.0, y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: inside });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: inside,
            button,
        });
        assert_eq!(
            lp.selection(),
            Some(ends),
            "a press inside the selection leaves it alone"
        );
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(inside.x + 40.0, y),
        });
        let drag = lp
            .take_native_drag()
            .expect("the selection leaves as a drag");
        assert_eq!(
            drag.data.text(),
            Some(selected.as_str()),
            "the drag carries the selected text"
        );
        let image = drag
            .image
            .as_ref()
            .expect("the host gets an image of the selected lines");
        let bounds = drag.bounds.expect("the selection has bounds");
        assert!(
            (image.size.0 as f32 - bounds.size.w).abs() <= 8.0,
            "the image is cropped to the selection, not its block: {image:?} for {bounds:?}"
        );
    }

    #[test]
    fn shift_extends_a_read_only_selection_from_the_keyboard() {
        // A click in a selectable paragraph leaves a caret; Shift+arrow must extend from
        // it, as it does in an input, rather than walking focus to another widget.
        let y = 8.0;
        let button = ui_core::PointerButton::Primary;
        let mut lp = AppLoop::new(Selectable);
        let mut target = OffscreenTarget::new(600, 120);
        lp.frame(&mut target, None);
        let at = ui_core::geom::Point::new(60.0, y);
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        let start = lp.selection().expect("the click placed a caret");
        assert!(start.1.is_collapsed(), "the click leaves no range yet");

        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..ui_core::Modifiers::default()
        }));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Left,
        });
        let after = lp.selection().expect("the selection is still there");
        assert_eq!(after.0, start.0, "the head stays in the same block");
        let range = after.1.range();
        let (from, to) = (range.start, range.end);
        assert_eq!(
            to, start.1.caret,
            "the click stays at one end of the new range: {after:?}"
        );
        assert!(from < to, "the range reaches left of the click: {after:?}");
        // If the anchor had followed the head, stepping right would leave `from..to` in
        // place instead of closing the range at the click again.
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Right,
        });
        let closed = lp.selection().expect("the caret is still there");
        assert!(
            closed.1.is_collapsed(),
            "Shift+Right back to the click collapses the range: {closed:?}"
        );
        assert_eq!(closed.1.caret, start.1.caret);
    }

    #[test]
    fn ctrl_a_selects_everything_without_arming_a_pointer_drag() {
        // The chord is not a gesture: a hover afterwards must not resize what it selected.
        let mut lp = AppLoop::new(TwoParagraphs);
        let mut target = OffscreenTarget::new(600, 120);
        lp.frame(&mut target, None);
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            ctrl: true,
            ..ui_core::Modifiers::default()
        }));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char('a'),
        });
        let all = lp.selection().expect("select all leaves a range");
        assert!(!all.1.is_collapsed(), "everything is selected");
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(10.0, 60.0),
        });
        assert_eq!(
            lp.selection(),
            Some(all),
            "a hover with no button must not move the selection"
        );
    }

    #[test]
    fn a_selection_drag_asks_for_every_frame_it_traverses() {
        // The pointer reports faster than the screen, and a move that changes no hover
        // owner is normally dropped. A selection drag changes what is painted without
        // moving the hover, so dropping it would show the range only when the button
        // came up.
        let mut lp = AppLoop::new(Selectable);
        let mut target = OffscreenTarget::new(600, 120);
        lp.frame(&mut target, Some(ui_core::geom::Point::new(500.0, 100.0)));
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: ui_core::geom::Point::new(1.0, 8.0),
            button: ui_core::PointerButton::Primary,
        });
        assert!(lp.take_dispatched(), "the press itself wants a frame");
        lp.frame(&mut target, Some(ui_core::geom::Point::new(1.0, 8.0)));
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(120.0, 8.0),
        });
        assert!(
            lp.take_dispatched(),
            "the move must want the frame that draws the range it crossed"
        );
        assert!(
            lp.selection().is_some_and(|(_, s)| !s.is_collapsed()),
            "and the range must be the one the move crossed"
        );
    }

    /// The same, with a declared line mode and a log of activations, for the Enter-split tests.
    struct Editable {
        text: String,
        multiline: bool,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        activated: Vec<Id>,
    }

    impl App for Editable {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("field"), NodeKind::Row)
                    .size(SizeClass::fixed(100.0, 40.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .text_input_multiline(self.multiline),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            match change {
                ui_core::change::Change::Text(next) => {
                    self.seen.borrow_mut().push(next.clone());
                    self.text = next;
                }
                ui_core::change::Change::Activated => self.activated.push(id),
                _ => {}
            }
            true
        }
    }

    /// A field whose text is a real view, so a press maps to a byte inside the buffer.
    struct FieldWithText {
        text: String,
    }

    impl App for FieldWithText {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let mut text = paragraph_named("view", 0.0);
            if let Decl {
                data: NodeData::Text { block, .. },
                ..
            } = &mut text
            {
                let block = std::sync::Arc::make_mut(block);
                block.input_base = Some(0);
                block.runs = vec![ui_core::TextRun::new(
                    self.text.clone(),
                    ui_core::TextStyle::new(14.0),
                )];
            }
            crate::from_decl(
                Decl::container(Id::str("field"), NodeKind::Row)
                    .size(SizeClass::fixed(300.0, 40.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .child(text),
            )
        }
    }

    #[test]
    fn a_shift_press_in_an_input_extends_its_selection() {
        // The same rule inside a field: the press moves the caret and keeps the anchor,
        // so a shift press selects instead of dropping what was selected.
        let mut lp = AppLoop::new(FieldWithText {
            text: "abcdefghijklmnop".to_string(),
        });
        let mut target = OffscreenTarget::new(300, 120);
        lp.frame(&mut target, None);
        let near = ui_core::geom::Point::new(8.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: near,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: near,
            button: ui_core::PointerButton::Primary,
        });
        let (_, first) = lp.selection().expect("the press left a caret");
        assert!(
            first.caret < 8,
            "the first press sits near the start, got {first:?}"
        );
        // A shift press in the middle of the field extends from the anchor: a text view
        // gives the press a byte to land on.
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..Default::default()
        }));
        let far = ui_core::geom::Point::new(120.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: far });
        assert!(
            lp.dispatch(ui_core::Event::PointerPressed {
                pos: far,
                button: ui_core::PointerButton::Primary,
            }),
            "the extension must ask for a repaint"
        );
        let (id, selection) = lp.selection().expect("the shift press left a selection");
        assert_eq!(id, Id::str("field"));
        assert_eq!(
            selection.anchor, first.caret,
            "the anchor is where the caret was"
        );
        assert!(
            !selection.is_collapsed() && selection.caret > selection.anchor,
            "the head moves to the line end, got {selection:?}"
        );
    }

    /// What one build saw: `reveal_range_for` beside the live selection, both for `field`.
    type RevealSeen = std::rc::Rc<
        std::cell::RefCell<
            Vec<(
                Option<std::ops::Range<usize>>,
                Option<std::ops::Range<usize>>,
            )>,
        >,
    >;

    /// A field whose build reports the selection structure reveal is cut from.
    struct ComposingField {
        text: String,
        seen: RevealSeen,
    }

    impl App for ComposingField {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let id = Id::str("field");
            self.seen.borrow_mut().push((
                cx.reveal_range_for(id),
                cx.selection_for(id).map(|selection| selection.range()),
            ));
            let mut text = paragraph_named("view", 0.0);
            if let Decl {
                data: NodeData::Text { block, .. },
                ..
            } = &mut text
            {
                let block = std::sync::Arc::make_mut(block);
                block.input_base = Some(0);
                block.runs = vec![ui_core::TextRun::new(
                    self.text.clone(),
                    ui_core::TextStyle::new(14.0),
                )];
            }
            crate::from_decl(
                Decl::container(id, NodeKind::Row)
                    .size(SizeClass::fixed(300.0, 40.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .child(text),
            )
        }
    }

    /// A live composition freezes reveal where it began: a press may move the caret the
    /// control draws, but the bytes under the composition do not change shape until the
    /// preedit ends.
    #[test]
    fn a_composition_freezes_the_selection_reveal_is_cut_from() {
        let seen: RevealSeen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(ComposingField {
            text: "abcdefghijklmnop".to_string(),
            seen: std::rc::Rc::clone(&seen),
        });
        let mut target = OffscreenTarget::new(300, 120);
        lp.frame(&mut target, None);
        let near = ui_core::geom::Point::new(8.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: near,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: near,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, None);
        let (reveal, selection) = seen.borrow().last().cloned().expect("a build ran");
        assert_eq!(
            reveal, selection,
            "with no composition the live caret rules"
        );
        let start = reveal.expect("the press left a caret");

        lp.dispatch(ui_core::Event::Preedit(Some(ui_core::Preedit::new(
            "に", 3,
        ))));
        lp.frame(&mut target, None);
        assert_eq!(
            seen.borrow().last().cloned().expect("a build ran").0,
            Some(start.clone()),
            "the composition starts where the caret is"
        );

        // A press during the composition moves the caret, which is where the preedit sits.
        let far = ui_core::geom::Point::new(120.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: far });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: far,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: far,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, None);
        let (reveal, selection) = seen.borrow().last().cloned().expect("a build ran");
        let moved = selection.expect("the field still holds a caret");
        assert_ne!(moved, start, "the press moved the caret");
        assert_eq!(
            reveal,
            Some(start.clone()),
            "reveal stays where the composition found it"
        );

        lp.dispatch(ui_core::Event::Preedit(None));
        lp.frame(&mut target, None);
        let (reveal, selection) = seen.borrow().last().cloned().expect("a build ran");
        assert_eq!(selection, Some(moved));
        assert_eq!(reveal, selection, "the live caret is the source again");
    }

    #[test]
    fn shift_enter_inserts_a_newline_in_a_multiline_input() {
        // The declared multiline intent is what turns the chord into a hard break, reported
        // as one whole-buffer edit like any other insertion.
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Editable {
            text: "ab".to_string(),
            seen: seen.clone(),
            activated: Vec::new(),
            multiline: true,
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        let pos = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        // The caret sits at the end of "ab"; Shift+Enter breaks after it.
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..Default::default()
        }));
        assert!(
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Enter
            }),
            "a hard break asks for a repaint"
        );
        assert_eq!(seen.borrow().as_slice(), ["ab\n"]);
        assert_eq!(
            lp.editing,
            Some((Id::str("field"), ui_core::TextSelection::collapsed(3))),
            "the caret lands on the new line"
        );
    }

    /// An input whose owner draws `**` as a hidden run, for the atomic-motion tests.
    struct Marked {
        text: String,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl App for Marked {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let style = ui_core::TextStyle::new(20.0);
            let mut text =
                Decl::container(Id::str("field-text"), ui_core::NodeKind::Text).size(SizeClass {
                    width: AxisSize::fixed(180.0),
                    height: AxisSize::hug(0.0, 0.0),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                });
            text.data = ui_core::NodeData::Text {
                ink: Color::WHITE,
                block: std::sync::Arc::new(ui_core::TextBlock {
                    runs: vec![
                        ui_core::TextRun::new("**", style).hidden(),
                        ui_core::TextRun::new("bold", style),
                        ui_core::TextRun::new("**", style).hidden(),
                    ],
                    ..Default::default()
                }),
                caret: None,
                selection: None,
                composing: None,
                nowrap: false,
            };
            crate::from_decl(
                Decl::container(Id::str("field"), ui_core::NodeKind::Row)
                    .size(SizeClass::fixed(180.0, 40.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .child(text),
            )
        }

        fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Text(next) = change {
                self.seen.borrow_mut().push(next.clone());
                self.text = next;
            }
            true
        }
    }

    /// The caret never rests inside syntax the editor is not drawing: one press crosses a
    /// hidden run, and a deletion that would take part of one takes all of it.
    #[test]
    fn a_hidden_run_is_crossed_in_one_step_and_deleted_whole() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Marked {
            text: "**bold**".to_string(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(300, 120);
        lp.frame(&mut target, None);
        // Below the text line: the press focuses the field without placing the caret at a
        // point, so focus leaves it at the end of the buffer.
        let pos = ui_core::geom::Point::new(170.0, 36.0);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerMoved { pos });
        lp.dispatch(ui_core::Event::PointerPressed { pos, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos, button });
        lp.frame(&mut target, None);
        let caret = |lp: &AppLoop<Marked>| lp.editing.map(|(_, sel)| sel.caret);
        let left = |lp: &mut AppLoop<Marked>| {
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Left,
            });
        };
        let right = |lp: &mut AppLoop<Marked>| {
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Right,
            });
        };
        // Both runs are zero-width, so a press past the text lands on the trailing run's
        // first byte rather than after it.
        assert_eq!(caret(&lp), Some(6), "the press lands at the trailing run");
        right(&mut lp);
        assert_eq!(caret(&lp), Some(8), "one press crosses the closing run");
        left(&mut lp);
        assert_eq!(caret(&lp), Some(6), "and one crosses it back");
        for _ in 0..4 {
            left(&mut lp);
        }
        assert_eq!(
            caret(&lp),
            Some(2),
            "the four visible bytes take four presses"
        );
        left(&mut lp);
        assert_eq!(caret(&lp), Some(0), "and one more crosses the opening run");

        right(&mut lp);
        assert_eq!(caret(&lp), Some(2), "forward motion skips it the same way");
        left(&mut lp);
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Delete,
        });
        assert_eq!(
            seen.borrow().as_slice(),
            ["bold**"],
            "Delete at the run's start removes the whole run, not one star"
        );

        // The same at the end: Backspace takes the closing run whole.
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Marked {
            text: "**bold**".to_string(),
            seen: seen.clone(),
        });
        lp.frame(&mut target, None);
        lp.dispatch(ui_core::Event::PointerMoved { pos });
        lp.dispatch(ui_core::Event::PointerPressed { pos, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos, button });
        lp.frame(&mut target, None);
        right(&mut lp);
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Backspace,
        });
        assert_eq!(
            seen.borrow().as_slice(),
            ["**bold"],
            "Backspace at the run's end removes the whole run"
        );
    }

    /// Two blocks drawn inside one input, with the line break between them in no view.
    struct Pages {
        text: String,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    fn page(id: &'static str, text: &'static str, base: usize) -> Decl {
        let mut node = Decl::container(Id::str(id), ui_core::NodeKind::Text).size(SizeClass {
            width: AxisSize::fixed(180.0),
            height: AxisSize::hug(0.0, 0.0),
            grow_x: Grow::No,
            grow_y: Grow::No,
        });
        node.data = ui_core::NodeData::Text {
            ink: Color::WHITE,
            block: std::sync::Arc::new(ui_core::TextBlock {
                runs: vec![ui_core::TextRun::new(text, ui_core::TextStyle::new(20.0))],
                input_base: Some(base),
                ..Default::default()
            }),
            caret: None,
            selection: None,
            composing: None,
            nowrap: false,
        };
        node
    }

    impl App for Pages {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("doc"), ui_core::NodeKind::Column)
                    .size(SizeClass::fixed(180.0, 100.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .text_input_enter_newline()
                    .child(page("one", "ab", 0))
                    .child(page("two", "cd", 3)),
            )
        }

        fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Text(next) = change {
                self.seen.borrow_mut().push(next.clone());
                self.text = next;
            }
            true
        }
    }

    /// One input over several drawn blocks: the caret crosses the line break between them,
    /// a drag selects across both, typing replaces the whole buffer, and a plain Enter
    /// writes a break instead of activating the node.
    #[test]
    fn a_selection_spans_the_blocks_of_one_input() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Pages {
            text: "ab\ncd".to_string(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(260, 160);
        lp.frame(&mut target, None);
        let rect = |lp: &AppLoop<Pages>, id: &'static str| {
            let nid = lp
                .tree()
                .find_by_id(Id::str(id))
                .expect("the view lays out");
            lp.tree().get(nid).expect("the view exists").visual_rect()
        };
        let caret = |lp: &AppLoop<Pages>| lp.editing.map(|(_, sel)| sel.caret);
        let at = |rect: Rect, x: f32| {
            ui_core::geom::Point::new(rect.origin.x + x, rect.origin.y + rect.size.h * 0.5)
        };
        let button = ui_core::PointerButton::Primary;

        let first = rect(&lp, "one");
        let second = rect(&lp, "two");
        // A press in the second block is a buffer offset, not one local to that block.
        let press = at(second, 10.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: press });
        lp.dispatch(ui_core::Event::PointerPressed { pos: press, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos: press, button });
        assert_eq!(
            caret(&lp),
            Some(4),
            "a press in the second block is its base plus one"
        );

        let press = at(first, 10.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: press });
        lp.dispatch(ui_core::Event::PointerPressed { pos: press, button });
        assert_eq!(
            caret(&lp),
            Some(1),
            "a press in the first block lands there"
        );

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Down,
        });
        assert_eq!(
            caret(&lp),
            Some(4),
            "Down keeps the column and crosses into the second block"
        );

        // Drag back up to the middle of the first block: one selection over both.
        let to = at(first, 10.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        assert_eq!(
            caret(&lp),
            Some(1),
            "the drag's far end is in the first block"
        );
        lp.dispatch(ui_core::Event::PointerReleased { pos: to, button });

        lp.dispatch(ui_core::Event::Text("X".to_owned()));
        assert_eq!(
            seen.borrow().as_slice(),
            ["aXd"],
            "the selection takes the line break with it in one whole-buffer edit"
        );

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Enter,
        });
        assert_eq!(
            seen.borrow().last().map(String::as_str),
            Some("aX\nd"),
            "a plain Enter writes a break into a document input"
        );
    }

    /// Two views that meet at one offset: the second starts where the first ends.
    struct Halves {
        text: String,
        /// Which view the engine measured the caret in this frame, per view.
        held: std::rc::Rc<std::cell::RefCell<Vec<(Id, bool)>>>,
    }

    impl App for Halves {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let mut held = self.held.borrow_mut();
            held.clear();
            for id in ["one", "two"] {
                held.push((Id::str(id), cx.caret_in_text_for(Id::str(id)).is_some()));
            }
            crate::from_decl(
                Decl::container(Id::str("doc"), ui_core::NodeKind::Column)
                    .size(SizeClass::fixed(180.0, 100.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .child(page("one", "ab", 0))
                    .child(page("two", "cd", 2)),
            )
        }
    }

    /// A position where one view ends and the next begins is one caret, and it belongs to
    /// the view whose byte follows it: that is where typing goes.
    #[test]
    fn the_text_after_a_boundary_owns_the_caret_there() {
        let held = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Halves {
            text: "abcd".to_string(),
            held: held.clone(),
        });
        let mut target = OffscreenTarget::new(260, 160);
        lp.frame(&mut target, None);
        let nid = lp
            .tree()
            .find_by_id(Id::str("two"))
            .expect("the second view lays out");
        let rect = lp.tree().get(nid).expect("the view exists").visual_rect();
        let pos = ui_core::geom::Point::new(rect.origin.x + 1.0, rect.origin.y + rect.size.h * 0.5);
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerMoved { pos });
        lp.dispatch(ui_core::Event::PointerPressed { pos, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos, button });
        lp.frame(&mut target, None);
        assert_eq!(
            lp.editing.map(|(_, sel)| sel.caret),
            Some(2),
            "the press lands on the boundary itself"
        );
        assert_eq!(
            held.borrow().as_slice(),
            [(Id::str("one"), false), (Id::str("two"), true)],
            "the view ending at the boundary leaves the caret to the one starting there"
        );
    }

    /// A document whose last line is empty: the view drawing it has no runs at all.
    struct Ends;

    impl App for Ends {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("doc"), ui_core::NodeKind::Column)
                    .size(SizeClass::fixed(180.0, 100.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input("ab\n")
                    .text_input_enter_newline()
                    .child(page("ab", "ab", 0))
                    .child(page("tail", "", 3)),
            )
        }
    }

    /// The empty line a trailing break leaves is a caret position like any other: a press
    /// lands on it and Up returns to the line above, although the view draws no glyphs.
    #[test]
    fn the_empty_last_line_takes_the_caret() {
        let mut lp = AppLoop::new(Ends);
        let mut target = OffscreenTarget::new(260, 160);
        lp.frame(&mut target, None);
        let rect = |lp: &AppLoop<Ends>, id: &'static str| {
            let nid = lp
                .tree()
                .find_by_id(Id::str(id))
                .expect("the view lays out");
            lp.tree().get(nid).expect("the view exists").visual_rect()
        };
        let tail = rect(&lp, "tail");
        // The far right, so the column held by Up is past the end of the line above.
        let pos = ui_core::geom::Point::new(
            tail.origin.x + tail.size.w - 2.0,
            tail.origin.y + tail.size.h * 0.5,
        );
        let button = ui_core::PointerButton::Primary;
        lp.dispatch(ui_core::Event::PointerMoved { pos });
        lp.dispatch(ui_core::Event::PointerPressed { pos, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos, button });
        let caret = |lp: &AppLoop<Ends>| lp.editing.map(|(_, sel)| sel.caret);
        assert_eq!(caret(&lp), Some(3), "the empty last line is offset three");

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Up,
        });
        // An empty line has one column, its start, so Up lands at the start of the line above.
        assert_eq!(
            caret(&lp),
            Some(0),
            "Up leaves the empty line for the one above"
        );
    }

    /// A pasted line break survives only where the input declared it can hold one, and a
    /// Windows line ending becomes the buffer's own newline either way.
    #[test]
    fn a_paste_keeps_line_breaks_only_in_a_multiline_input() {
        let pasted = |multiline: bool| {
            let seen = Rc::new(RefCell::new(Vec::new()));
            let mut lp = AppLoop::new(Editable {
                text: "ab".to_string(),
                seen: seen.clone(),
                activated: Vec::new(),
                multiline,
            });
            let mut target = OffscreenTarget::new(200, 120);
            lp.frame(&mut target, None);
            let pos = ui_core::geom::Point::new(20.0, 20.0);
            lp.dispatch(ui_core::Event::PointerPressed {
                pos,
                button: ui_core::PointerButton::Primary,
            });
            lp.dispatch(ui_core::Event::PointerReleased {
                pos,
                button: ui_core::PointerButton::Primary,
            });
            lp.dispatch(ui_core::Event::Paste(
                ui_core::clipboard::ClipboardContent::Text("a\r\nb".to_owned()),
            ));
            seen.borrow().clone()
        };
        assert_eq!(
            pasted(true).as_slice(),
            ["aba\nb"],
            "a multiline input keeps the break, as LF"
        );
        assert_eq!(
            pasted(false).as_slice(),
            ["aba b"],
            "a one-line input turns it into a space"
        );
    }

    #[test]
    fn a_chord_resolves_focus_then_the_hit_chain_then_the_app() {
        // Three layers can claim one chord; the order between them is the whole contract.
        struct Layered {
            log: Rc<RefCell<Vec<(Option<Id>, &'static str)>>>,
        }

        fn ctrl(c: char) -> ui_core::KeyChord {
            ui_core::KeyChord::new(
                ui_core::Key::Char(c),
                ui_core::Modifiers {
                    ctrl: true,
                    ..ui_core::Modifiers::default()
                },
            )
        }

        impl App for Layered {
            fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                let box_at = |id: &'static str, sense: ui_core::Sense| {
                    Decl::container(Id::str(id), NodeKind::Row)
                        .size(SizeClass::fixed(80.0, 40.0))
                        .sense(sense)
                        .shortcut(
                            ctrl('k'),
                            ui_core::ShortcutId(if id == "a" { "a.k" } else { "b.k" }),
                        )
                };
                crate::from_decl(
                    Decl::container(Id::str("root"), NodeKind::Row)
                        .size(SizeClass::fixed(200.0, 40.0))
                        .gap(20.0)
                        .child(box_at(
                            "a",
                            ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE,
                        ))
                        .child(box_at("b", ui_core::Sense::CLICK)),
                )
            }

            fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
                if let ui_core::change::Change::Shortcut(shortcut) = change {
                    self.log.borrow_mut().push((Some(id), shortcut.0));
                }
                true
            }

            fn shortcut(&mut self, id: ui_core::ShortcutId, _: ui_core::Modifiers) -> bool {
                self.log.borrow_mut().push((None, id.0));
                true
            }
        }

        let log = Rc::new(RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Layered {
            log: Rc::clone(&log),
        });
        lp.bind_shortcut(ctrl('j'), ui_core::ShortcutId("app.j"));
        let mut target = OffscreenTarget::new(200, 60);
        lp.frame(&mut target, None);

        let press = |lp: &mut AppLoop<Layered>, x: f32| {
            lp.dispatch(ui_core::Event::PointerMoved {
                pos: ui_core::geom::Point::new(x, 10.0),
            });
            lp.dispatch(ui_core::Event::PointerPressed {
                pos: ui_core::geom::Point::new(x, 10.0),
                button: ui_core::PointerButton::Primary,
            });
        };
        let chord = |lp: &mut AppLoop<Layered>, c: char| {
            lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
                ctrl: true,
                ..ui_core::Modifiers::default()
            }));
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Char(c),
            });
        };

        // Focus on `a`, pointer over `b`: the focused node wins over the hit chain.
        press(&mut lp, 40.0);
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(140.0, 10.0),
        });
        chord(&mut lp, 'k');
        assert_eq!(
            log.borrow().last().copied(),
            Some((Some(Id::str("a")), "a.k"))
        );

        // `b` is not focusable, so pressing it drops focus; now the hit chain answers.
        press(&mut lp, 140.0);
        chord(&mut lp, 'k');
        assert_eq!(
            log.borrow().last().copied(),
            Some((Some(Id::str("b")), "b.k"))
        );

        // No node binds this chord, so the app-level binding is the one that fires.
        chord(&mut lp, 'j');
        assert_eq!(log.borrow().last().copied(), Some((None, "app.j")));
    }

    /// A multiline field whose Enter and Tab are control shortcuts, as a document editor has.
    struct Queued {
        text: String,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl App for Queued {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let id = Id::str("field");
            if let Some((owner, shortcut)) = cx.shortcut()
                && owner == id
            {
                let (start, end) = match cx.shortcut_selection(id) {
                    Some(selection) => (
                        selection.anchor.min(selection.caret),
                        selection.anchor.max(selection.caret),
                    ),
                    None => match cx.caret_for(id) {
                        Some(at) => (at, at),
                        None => (0, 0),
                    },
                };
                let source = cx.input_value(id).unwrap_or(&self.text).to_owned();
                let insert = if shortcut == ui_core::ShortcutId("break") {
                    "\n"
                } else {
                    "  "
                };
                let (rest, at) = ui_core::text_input::delete_range(&source, start..end);
                let (_, caret) = ui_core::text_input::insert(&rest, at, insert);
                cx.edit_input(id, start..end, insert, caret);
            }
            crate::from_decl(
                Decl::container(id, NodeKind::Row)
                    .size(SizeClass::fixed(200.0, 60.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .text_input_multiline(true)
                    .shortcut(
                        ui_core::KeyChord::new(ui_core::Key::Enter, ui_core::Modifiers::default()),
                        ui_core::ShortcutId("break"),
                    )
                    .shortcut(
                        ui_core::KeyChord::new(ui_core::Key::Tab, ui_core::Modifiers::default()),
                        ui_core::ShortcutId("indent"),
                    ),
            )
        }

        fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Text(next) = change {
                self.seen.borrow_mut().push(next.clone());
                self.text = next;
            }
            true
        }
    }

    #[test]
    fn a_burst_of_shortcuts_is_answered_one_frame_each() {
        // Two keys in one batch must not collapse into the last: the second edit has to see
        // the caret the first leaves, and the engine must ask for the frame that answers it.
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Queued {
            text: "ab".to_string(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(240, 120);
        lp.frame(&mut target, None);
        let pos = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::End,
        });
        assert_eq!(
            lp.editing,
            Some((Id::str("field"), ui_core::TextSelection::collapsed(2))),
            "the burst starts at the end of the buffer"
        );

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Enter,
        });
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        });
        assert_eq!(
            lp.shortcut_queue.len(),
            2,
            "both keys are facts, not one fact overwritten by the other"
        );

        lp.frame(&mut target, None);
        assert_eq!(
            lp.shortcut_queue.len(),
            1,
            "one frame answers one fact out of the queue"
        );
        assert!(
            lp.take_dispatched(),
            "a fact left in the queue asks for another frame"
        );

        lp.frame(&mut target, None);
        assert!(
            lp.shortcut_queue.is_empty(),
            "the second frame answers the remaining fact"
        );
        assert_eq!(
            seen.borrow().as_slice(),
            ["ab\n", "ab\n  "],
            "the indent lands after the break, at the caret the break left"
        );
    }

    /// A control can be drawn inside the input that owns the caret — a code card's copy
    /// button. Its press belongs to the button: dropping the caret into the text under it
    /// would open the block for editing the moment the button was clicked.
    struct Inset {
        text: String,
    }

    impl App for Inset {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let button = Decl::container(Id::str("copy"), ui_core::NodeKind::Row)
                .size(SizeClass::fixed(100.0, 24.0))
                .sense(ui_core::Sense::CLICK);
            crate::from_decl(
                Decl::container(Id::str("doc"), ui_core::NodeKind::Row)
                    .size(SizeClass::fixed(300.0, 60.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(self.text.clone())
                    .child(page("body", "abcdef", 0))
                    .child(button),
            )
        }
    }

    #[test]
    fn a_press_on_a_control_inside_an_input_leaves_the_caret_alone() {
        let mut lp = AppLoop::new(Inset {
            text: "abcdef".to_string(),
        });
        let mut target = OffscreenTarget::new(320, 100);
        lp.frame(&mut target, None);
        let press = |lp: &mut AppLoop<Inset>, target: &mut OffscreenTarget, x: f32| {
            let pos = ui_core::geom::Point::new(x, 12.0);
            lp.dispatch(ui_core::Event::PointerMoved { pos });
            lp.frame(target, Some(pos));
            lp.dispatch(ui_core::Event::PointerPressed {
                pos,
                button: ui_core::PointerButton::Primary,
            });
            lp.dispatch(ui_core::Event::PointerReleased {
                pos,
                button: ui_core::PointerButton::Primary,
            });
            lp.frame(target, Some(pos));
        };
        press(&mut lp, &mut target, 10.0);
        let placed = lp.editing;
        assert!(
            placed.is_some_and(|(owner, _)| owner == Id::str("doc")),
            "a press on the text places the caret"
        );
        press(&mut lp, &mut target, 200.0);
        assert_eq!(
            lp.editing, placed,
            "a press on the button inside the input must not move the caret"
        );
    }

    /// A control inside a document rewrites a byte of it without owning the caret — a task
    /// box flips its marker. The other input's buffer changes, no caret moves, and the step
    /// is that input's own undo entry.
    struct Detached {
        doc: String,
        field: String,
        pending: std::rc::Rc<std::cell::RefCell<Option<(std::ops::Range<usize>, String)>>>,
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl App for Detached {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            if let Some((range, text)) = self.pending.borrow_mut().take() {
                cx.edit_input_detached(Id::str("doc"), range, text);
            }
            let input = |id: &'static str, value: &str| {
                Decl::container(Id::str(id), ui_core::NodeKind::Row)
                    .size(SizeClass::fixed(300.0, 40.0))
                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                    .text_input(value.to_owned())
            };
            crate::from_decl(
                Decl::container(Id::str("root"), ui_core::NodeKind::Column)
                    .child(input("doc", &self.doc))
                    .child(input("field", &self.field)),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Text(next) = change {
                if id == Id::str("doc") {
                    self.seen.borrow_mut().push(next.clone());
                    self.doc = next;
                } else {
                    self.field = next;
                }
            }
            true
        }
    }

    #[test]
    fn a_control_edits_an_input_the_caret_is_not_in() {
        let pending = std::rc::Rc::new(std::cell::RefCell::new(None));
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Detached {
            doc: "0123".to_string(),
            field: "held".to_string(),
            pending: pending.clone(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(320, 120);
        lp.frame(&mut target, None);
        // The caret lives in the second input; the first one is nobody's.
        let pos = ui_core::geom::Point::new(20.0, 60.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos });
        lp.frame(&mut target, Some(pos));
        lp.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, Some(pos));
        let held = lp.editing;
        assert!(
            held.is_some_and(|(owner, _)| owner == Id::str("field")),
            "the press puts the caret in the second input"
        );

        *pending.borrow_mut() = Some((1..2, "X".to_string()));
        lp.frame(&mut target, None);
        assert_eq!(
            lp.app_mut().doc,
            "0X23",
            "the detached edit reaches an input with no caret"
        );
        assert_eq!(lp.editing, held, "the caret keeps its owner and its offset");
        assert_eq!(
            seen.borrow().as_slice(),
            ["0X23"],
            "the owning app hears the new buffer"
        );
        assert!(
            lp.take_dispatched(),
            "a frame that changed bytes paints again"
        );

        // With the caret in the edited input, the edit shifts it by what it did and no more:
        // two bytes in, one out, so everything after the byte it replaced moves one right.
        let pos = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos });
        lp.frame(&mut target, Some(pos));
        lp.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, Some(pos));
        let placed = lp
            .editing
            .expect("a press in the first input places the caret");
        assert_eq!(placed.0, Id::str("doc"));
        *pending.borrow_mut() = Some((0..1, "AB".to_string()));
        lp.frame(&mut target, None);
        assert_eq!(lp.app_mut().doc, "ABX23");
        assert_eq!(
            lp.editing,
            Some((
                placed.0,
                ui_core::TextSelection::collapsed(placed.1.caret + 1)
            )),
            "the caret follows its own bytes, not the edit"
        );

        // The steps belong to the input they rewrote: its own undo takes both back.
        undo(&mut lp);
        lp.frame(&mut target, None);
        assert_eq!(lp.app_mut().doc, "0X23", "one undo for one detached edit");
        undo(&mut lp);
        lp.frame(&mut target, None);
        assert_eq!(lp.app_mut().doc, "0123");
    }

    #[test]
    fn a_copy_cut_and_paste_round_trip_through_the_clipboard_queue() {
        // Copy, cut and paste are one path: the engine names the text, the host does the
        // round trip, and the answer comes back as `Paste`.
        use crate::clipboard::{ClipboardCommand, ClipboardKind};
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Typing {
            text: "alpha".to_string(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        let pos = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Home,
        });
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..Default::default()
        }));
        for _ in 0..2 {
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Right,
            });
        }
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            ctrl: true,
            ..Default::default()
        }));
        let copy = ClipboardCommand::Copy {
            content: crate::clipboard::ClipboardContent::Text("al".to_string()),
            kind: ClipboardKind::Selection,
        };
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char('c'),
        });
        assert_eq!(lp.take_clipboard_commands(), vec![copy.clone()]);
        assert_eq!(
            lp.editing.map(|(_, sel)| sel),
            Some(ui_core::TextSelection::new(0, 2))
        );

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char('x'),
        });
        assert_eq!(lp.take_clipboard_commands(), vec![copy]);
        assert_eq!(
            seen.borrow().last().map(String::as_str),
            Some("pha"),
            "a cut removes the selection from the buffer"
        );

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Char('v'),
        });
        assert_eq!(
            lp.take_clipboard_commands(),
            vec![ClipboardCommand::RequestPaste {
                kind: ClipboardKind::Selection
            }],
            "the engine asks for the text instead of reading it"
        );
        lp.dispatch(ui_core::Event::Paste(
            ui_core::clipboard::ClipboardContent::Text("al".to_string()),
        ));
        assert_eq!(
            seen.borrow().last().map(String::as_str),
            Some("alpha"),
            "the paste lands at the caret"
        );
        // Releasing Ctrl is its own event; without it the platform would still be holding it.
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::End,
        });
        lp.dispatch(ui_core::Event::Paste(
            ui_core::clipboard::ClipboardContent::Text("\r\nb".to_string()),
        ));
        assert_eq!(
            seen.borrow().last().map(String::as_str),
            Some("alpha b"),
            "a line break flattens to a space: this input is one line"
        );
    }

    /// The purpose the app declared reaches the input method with the caret; an input that
    /// declared none says `Normal`.
    #[test]
    fn a_time_input_tells_the_ime_what_it_is_for() {
        struct Field(ui_core::ime::ContentPurpose);
        impl App for Field {
            fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                crate::from_decl(
                    Decl::container(Id::str("field"), NodeKind::Row)
                        .size(SizeClass::fixed(100.0, 40.0))
                        .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                        .text_input("12")
                        .text_input_purpose(self.0),
                )
            }
        }
        let focused = |purpose| {
            let mut lp = AppLoop::new(Field(purpose));
            let mut target = OffscreenTarget::new(200, 120);
            lp.frame(&mut target, None);
            let pos = ui_core::geom::Point::new(20.0, 20.0);
            lp.dispatch(ui_core::Event::PointerPressed {
                pos,
                button: ui_core::PointerButton::Primary,
            });
            lp.dispatch(ui_core::Event::PointerReleased {
                pos,
                button: ui_core::PointerButton::Primary,
            });
            lp.frame(&mut target, None);
            lp.ime_state().purpose
        };
        assert_eq!(
            focused(ui_core::ime::ContentPurpose::Time),
            ui_core::ime::ContentPurpose::Time
        );
        assert_eq!(
            focused(ui_core::ime::ContentPurpose::Normal),
            ui_core::ime::ContentPurpose::Normal
        );
    }

    #[test]
    fn composing_text_does_not_reach_the_app_until_it_commits() {
        // The preedit belongs to the engine: the app sees one edit, at commit time.
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Typing {
            text: String::new(),
            seen: seen.clone(),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        let pos = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos,
            button: ui_core::PointerButton::Primary,
        });
        let preedit = ui_core::Preedit::new("にほ", 6);
        assert!(
            lp.dispatch(ui_core::Event::Preedit(Some(preedit.clone()))),
            "a composition change asks for a repaint"
        );
        assert_eq!(
            lp.preedit,
            Some((Id::str("field"), preedit)),
            "the engine holds the composition"
        );
        assert!(
            seen.borrow().is_empty(),
            "the app must not see composing text"
        );
        lp.dispatch(ui_core::Event::Text("日本".to_string()));
        assert_eq!(seen.borrow().as_slice(), ["日本"], "one edit, at commit");
        assert_eq!(lp.preedit, None, "a commit ends the composition");
        assert_eq!(
            lp.editing,
            Some((Id::str("field"), ui_core::TextSelection::collapsed(6)))
        );
    }

    #[test]
    fn first_frame_reports_full_damage() {
        let mut target = OffscreenTarget::new(200, 120);
        let outcomes = run_app(
            Demo {
                expand: false,
                draggable: true,
            },
            &mut target,
            1,
            None,
        );
        assert!(outcomes[0].damage.is_full());
        assert_eq!(outcomes[0].nodes, 2, "root + bar");
    }

    /// Counts presents, so a test can ask whether a frame handed anything over.
    struct CountingTarget {
        inner: OffscreenTarget,
        presents: u32,
    }

    impl CountingTarget {
        fn new(width: u32, height: u32) -> Self {
            Self {
                inner: OffscreenTarget::new(width, height),
                presents: 0,
            }
        }
    }

    impl PresentTarget for CountingTarget {
        fn kind(&self) -> crate::present::SurfaceKind {
            self.inner.kind()
        }
        fn size(&self) -> (u32, u32) {
            self.inner.size()
        }
        fn pixels_mut(&mut self) -> &mut ui_paint::Pixels {
            self.inner.pixels_mut()
        }
        fn present(&mut self, _damage: &Damage) -> Result<(), String> {
            self.presents += 1;
            Ok(())
        }
    }

    /// Fades a box, but only after a delay, so a frame can be mid-animation and still.
    struct Delayed {
        target: f32,
    }

    impl App for Delayed {
        fn view(&mut self, f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let spec = ui_core::AnimSpec {
                delay: 0.05,
                ..ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear)
            };
            let a = f.animate(Id::str("late"), self.target, spec);
            crate::from_decl(
                Decl::container(Id::str("late"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(50.0, 50.0))
                    .background(
                        Color::from_rgba8(200, 60, 60, (a.clamp(0.0, 1.0) * 255.0) as u8),
                        Corners::ZERO,
                    ),
            )
        }
    }

    #[test]
    fn an_animating_frame_presents_even_when_nothing_changed_yet() {
        // The loop waits on input for a frame that hands it nothing; an animation whose
        // first frame moves no pixel would then stall until unrelated input arrived.
        let mut target = CountingTarget::new(120, 120);
        let mut lp = AppLoop::new(Delayed { target: 0.0 });
        lp.frame(&mut target, None);

        lp.app_mut().target = 1.0;
        let before = target.presents;
        // Still inside the delay: the value is pinned, so the frame draws the same pixels.
        let out = lp.frame_with_dt(&mut target, None, 0.01);
        assert!(out.damage.is_none(), "nothing has moved yet");
        assert!(out.animating, "a pending delay is still motion");
        assert_eq!(
            target.presents,
            before + 1,
            "an animating frame must present so the loop gets a frame callback"
        );
    }

    /// A still frame: one opaque box, nothing declared that moves.
    struct Still;

    impl App for Still {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("still"), NodeKind::Box)
                    .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                    .background(Color::from_rgba8(10, 20, 30, 255), Corners::ZERO),
            )
        }
    }

    /// A scene target that counts what reached it and can owe a commit of its own.
    struct CommitTarget {
        size: (u32, u32),
        presents: usize,
        owes: bool,
    }

    impl crate::present::SceneTarget for CommitTarget {
        fn size(&self) -> (u32, u32) {
            self.size
        }

        fn present_scene(
            &mut self,
            _scene: &ui_core::draw::DrawList,
            _text: &crate::text_pass::TextPass,
            _damage: &ui_core::Damage,
            _canvases: &[crate::present::CanvasFrame],
        ) -> Result<crate::present::PresentStats, String> {
            self.presents += 1;
            Ok(crate::present::PresentStats {
                draw_calls: 1,
                band: None,
            })
        }

        fn commit_pending(&self) -> bool {
            self.owes
        }
    }

    #[test]
    fn a_scene_frame_presents_only_when_it_has_something_to_carry() {
        // Damage and animation are the obvious reasons to commit. A surface can also owe one
        // with nothing to show for it — input method state — and dropping that term leaves
        // a stale caret or preedit rectangle on screen.
        let mut target = CommitTarget {
            size: (120, 120),
            presents: 0,
            owes: false,
        };
        let mut lp = AppLoop::new(Still);
        let first = lp.frame_scene(&mut target, None).expect("a frame");
        assert!(
            first.presented.is_some(),
            "the first frame carries the whole surface"
        );
        let still = lp.frame_scene(&mut target, None).expect("a frame");
        assert!(
            still.presented.is_none(),
            "a still frame with nothing owed hands the target nothing"
        );

        target.owes = true;
        let owed = lp.frame_scene(&mut target, None).expect("a frame");
        assert!(
            owed.presented.is_some(),
            "a frame that owes a commit must reach the target"
        );
    }

    /// A target that records the rect primitives a list carried and who owns them.
    #[derive(Default)]
    struct RectTarget {
        rects: Vec<(ui_core::NodeId, Rect)>,
    }

    impl crate::present::SceneTarget for RectTarget {
        fn size(&self) -> (u32, u32) {
            (400, 400)
        }

        fn present_scene(
            &mut self,
            scene: &ui_core::draw::DrawList,
            _text: &crate::text_pass::TextPass,
            _damage: &ui_core::Damage,
            _canvases: &[crate::present::CanvasFrame],
        ) -> Result<crate::present::PresentStats, String> {
            self.rects = scene
                .prims
                .rects
                .iter()
                .zip(&scene.owners.rects)
                .map(|(prim, owner)| (*owner, prim.rect))
                .collect();
            Ok(crate::present::PresentStats {
                draw_calls: 1,
                band: None,
            })
        }
    }

    /// A panel declared for a surface of the platform's own paints into that surface's list,
    /// rebased on the panel's own top-left, and never into its parent's; where no surface is
    /// had — the platform refuses, or no host is there to ask — the parent paints it.
    #[test]
    fn a_platform_panel_paints_into_its_own_list_and_not_the_parent() {
        struct PanelApp;
        impl App for PanelApp {
            fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                crate::from_decl(
                    Decl::container(Id::str("page"), NodeKind::Stack)
                        .size(ui_core::layout::SizeClass::fixed(400.0, 400.0))
                        .background(Color::from_rgba8(0, 0, 255, 255), Corners::ZERO)
                        .child(
                            Decl::container(Id::str("anchor"), NodeKind::Box)
                                .size(ui_core::layout::SizeClass::fixed(40.0, 20.0))
                                .offset(150.0, 300.0)
                                .background(Color::from_rgba8(255, 0, 0, 255), Corners::ZERO),
                        )
                        .child(
                            Decl::container(Id::str("panel"), NodeKind::Box)
                                .size(ui_core::layout::SizeClass::fixed(120.0, 80.0))
                                .background(Color::from_rgba8(0, 255, 0, 255), Corners::ZERO)
                                .overlay(
                                    ui_core::layout::OverlayPlacement::below(
                                        ui_core::layout::Anchor::Node(Id::str("anchor")),
                                    )
                                    .presentation(ui_core::layout::Presentation::Platform),
                                ),
                        ),
                )
            }
        }
        let mut parent = RectTarget::default();
        let mut lp = AppLoop::new(PanelApp);
        // What a host says when it builds the loop: it reconciles a surface for every panel.
        lp.set_panel_surfaces(true);
        lp.frame_scene(&mut parent, None).expect("a frame");
        let panel = lp.tree.find_by_id(Id::str("panel")).expect("laid out");
        assert!(
            parent.rects.iter().all(|(owner, _)| *owner != panel),
            "a panel the host has not placed yet is held back, not painted in the window"
        );
        // The host's answer, which it gives right after the build that declared the panel.
        lp.set_panel_presentation(panel, true);
        lp.frame_scene(&mut parent, None).expect("a frame");
        assert!(
            parent.rects.iter().all(|(owner, _)| *owner != panel),
            "the parent's surface must not paint a panel that has one of its own"
        );

        let mut own = RectTarget::default();
        lp.present_panel(panel, &mut own).expect("the panel's list");
        let mut painted: Vec<Rect> = own
            .rects
            .iter()
            .filter(|(owner, _)| *owner == panel)
            .map(|(_, rect)| *rect)
            .collect();
        assert_eq!(painted.len(), 1, "the panel's background, once: {painted:?}");
        let background = painted.remove(0);
        assert_eq!(
            (background.origin.x, background.origin.y),
            (0.0, 0.0),
            "the panel's list starts at the surface's own top-left"
        );
        assert_eq!(
            (background.size.w, background.size.h),
            (120.0, 80.0),
            "the panel paints its laid-out size"
        );

        // A platform with no surface to open hands the panel back to its parent.
        let mut parent = RectTarget::default();
        let mut lp = AppLoop::new(PanelApp);
        // What a host says when it builds the loop: it reconciles a surface for every panel.
        lp.set_panel_surfaces(true);
        lp.frame_scene(&mut parent, None).expect("a frame");
        let panel = lp.tree.find_by_id(Id::str("panel")).expect("laid out");
        lp.set_panel_presentation(panel, false);
        lp.frame_scene(&mut parent, None).expect("a frame");
        assert!(
            parent.rects.iter().any(|(owner, _)| *owner == panel),
            "with no surface of its own, the parent paints the panel"
        );
        assert!(
            lp.present_panel(panel, &mut RectTarget::default()).is_err(),
            "a panel the parent paints has no list of its own"
        );

        // A loop with no host never presents a panel, so the window answers the declaration
        // itself: the panel is placed and painted there from the frame it appeared in.
        let mut parent = RectTarget::default();
        let mut lp = AppLoop::new(PanelApp);
        lp.frame_scene(&mut parent, None).expect("a frame");
        let panel = lp.tree.find_by_id(Id::str("panel")).expect("laid out");
        assert!(
            lp.platform_panels().is_empty(),
            "a surface is not asked for where nothing would present one"
        );
        assert!(
            parent.rects.iter().any(|(owner, _)| *owner == panel),
            "the window paints the panel without waiting for a host"
        );
        assert!(
            lp.present_panel(panel, &mut RectTarget::default()).is_err(),
            "a panel the window paints has no list of its own"
        );
    }

    /// A page with a panel that offers a context menu.
    struct ContextMenu;

    impl App for ContextMenu {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("page"), NodeKind::Stack)
                    .size(ui_core::layout::SizeClass::fixed(300.0, 300.0))
                    .background(Color::from_rgba8(0, 0, 255, 255), Corners::ZERO)
                    .child(
                        Decl::container(Id::str("panel"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                            .sense(ui_core::Sense::CLICK | ui_core::Sense::HOVER)
                            .menu(vec![ui_core::menu::MenuItem::new(Id::str("copy"), "Copy")]),
                    ),
            )
        }
    }

    #[test]
    fn a_menu_opened_after_an_idle_gap_starts_its_entrance_at_the_click() {
        // The entrance's zero used the last frame's stamp. No frame is built while the user
        // sits idle, so after a pause the 167 ms entrance was already over before the first
        // frame drawn after the click: the menu appeared fully open. The zero is the click.
        let mut target = OffscreenTarget::new(300, 300);
        let mut lp = AppLoop::new(ContextMenu);
        lp.frame(&mut target, None);
        let page = target.pixels.get(150, 150);

        // Two seconds pass with no frame built: the wall moves, `frame_now` does not.
        lp.clock.advance_by(2.0);
        let at = ui_core::geom::Point::new(100.0, 100.0);
        let button = ui_core::PointerButton::Secondary;
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        let centre = lp
            .menu
            .as_ref()
            .expect("the press opened a menu")
            .at;
        let (x, y) = ((centre.x + 10.0) as u32, (centre.y + 10.0) as u32);

        lp.frame_with_dt(&mut target, None, 0.0);
        // The panel hangs off the point the press named, so the point just inside it is
        // known without waiting for a layout to read the panel's own rect.
        let panel = lp
            .tree
            .find_by_id(crate::menu::Menu::panel_id(0))
            .and_then(|nid| lp.tree.get(nid))
            .map(|node| node.rect)
            .expect("the menu panel is laid out in the clicking frame");
        assert!(
            panel.contains(ui_core::geom::Point::new(x as f32, y as f32)),
            "the sampled point is on the menu: {panel:?}"
        );
        assert_eq!(
            target.pixels.get(x, y),
            page,
            "the first frame after the click is the entrance's start, not its end"
        );
        lp.frame_with_dt(&mut target, None, 0.05);
        assert_ne!(
            target.pixels.get(x, y),
            page,
            "half way in the surface is showing"
        );
        lp.frame_with_dt(&mut target, None, 0.2);
        assert_ne!(
            target.pixels.get(x, y),
            page,
            "and it lands on the menu surface"
        );
    }

    /// A panel that asks for the secondary button and also offers a menu.
    ///
    /// The two disagree on purpose: the button must reach the app on the press, and the
    /// menu must still be reachable by the tap that a quick right click is.
    struct SecondaryPanel {
        seen: Vec<(Id, bool)>,
    }

    impl App for SecondaryPanel {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("page"), NodeKind::Stack)
                    .size(ui_core::layout::SizeClass::fixed(300.0, 300.0))
                    .child(
                        Decl::container(Id::str("panel"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                            .sense(ui_core::Sense::SECONDARY)
                            .menu(vec![ui_core::menu::MenuItem::new(Id::str("copy"), "Copy")]),
                    ),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Secondary { down, .. } = change {
                self.seen.push((id, down));
                return true;
            }
            false
        }
    }

    fn secondary_panel() -> (AppLoop<SecondaryPanel>, OffscreenTarget) {
        let mut target = OffscreenTarget::new(300, 300);
        let mut lp = AppLoop::new(SecondaryPanel { seen: Vec::new() });
        lp.frame(&mut target, None);
        (lp, target)
    }

    #[test]
    fn a_secondary_press_reaches_the_node_that_asked_and_a_tap_still_opens_its_menu() {
        let (mut lp, mut target) = secondary_panel();
        let at = ui_core::geom::Point::new(100.0, 100.0);
        let button = ui_core::PointerButton::Secondary;
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });

        assert_eq!(
            lp.app_mut().seen,
            vec![(Id::str("panel"), true), (Id::str("panel"), false)],
            "the press and the release reach the node that asked for them"
        );
        assert!(
            lp.menu.is_some(),
            "a release inside the tap window still opens the menu"
        );
        lp.frame(&mut target, None);
    }

    /// A panel whose single row carries a chord, so a press can pick the row by name.
    struct ChordedMenu {
        picked: Vec<Id>,
    }

    impl App for ChordedMenu {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("panel"), NodeKind::Column)
                    .size(SizeClass::fixed(300.0, 300.0))
                    .sense(ui_core::Sense::CLICK)
                    .menu(vec![
                        ui_core::menu::MenuItem::new(Id::str("copy"), "Copy").with_chord(
                            ui_core::KeyChord::new(
                                ui_core::Key::Char('c'),
                                ui_core::Modifiers {
                                    ctrl: true,
                                    ..Default::default()
                                },
                            ),
                            "Ctrl+C",
                        ),
                    ]),
            )
        }

        fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::MenuItem(item) = change {
                self.picked.push(item);
            }
            true
        }
    }

    /// The chord a row prints picks that row while the menu is open — and only then: with
    /// the menu closed the same press is whatever the app bound, or nothing.
    #[test]
    fn a_rows_chord_picks_it_while_the_menu_is_open() {
        let ctrl = ui_core::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let mut target = OffscreenTarget::new(300, 300);
        let mut lp = AppLoop::new(ChordedMenu { picked: Vec::new() });
        lp.frame(&mut target, None);

        press_ctrl(&mut lp, 'c', ctrl);
        assert!(lp.app_mut().picked.is_empty(), "no menu, no row to pick");

        let at = ui_core::geom::Point::new(100.0, 100.0);
        let button = ui_core::PointerButton::Secondary;
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        assert!(lp.menu.is_some(), "the press opened the menu");

        press_ctrl(&mut lp, 'c', ctrl);
        assert_eq!(
            lp.app_mut().picked,
            [Id::str("copy")],
            "the row's own chord picked it"
        );
        assert!(lp.menu.is_none(), "and the pick closed the menu");

        press_ctrl(&mut lp, 'c', ctrl);
        assert_eq!(lp.app_mut().picked.len(), 1, "a closed menu picks nothing");
    }

    #[test]
    fn a_held_secondary_press_reports_the_button_and_withholds_the_menu() {
        let (mut lp, mut target) = secondary_panel();
        let at = ui_core::geom::Point::new(100.0, 100.0);
        let button = ui_core::PointerButton::Secondary;
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        // Longer than the tap window: this press means the tool, not the menu.
        lp.clock.advance_by(0.4);
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });

        assert_eq!(
            lp.app_mut().seen,
            vec![(Id::str("panel"), true), (Id::str("panel"), false)],
            "a held press is reported exactly like a tapped one"
        );
        assert!(
            lp.menu.is_none(),
            "a hold must not leave a menu behind on the release"
        );
        lp.frame(&mut target, None);
    }

    /// A target that keeps the input region the engine published.
    struct RegionTarget {
        inner: OffscreenTarget,
        regions: Vec<Rect>,
    }

    impl crate::present::PresentTarget for RegionTarget {
        fn kind(&self) -> crate::present::SurfaceKind {
            self.inner.kind()
        }
        fn size(&self) -> (u32, u32) {
            self.inner.size()
        }
        fn pixels_mut(&mut self) -> &mut ui_paint::Pixels {
            self.inner.pixels_mut()
        }
        fn present(&mut self, damage: &Damage) -> Result<(), String> {
            self.inner.present(damage)
        }
        fn set_input_region(&mut self, areas: &[Rect]) {
            self.regions = areas.to_vec();
        }
    }

    /// A surface whose only opaque area is one small panel: everything else lets input past,
    /// the way a pin's shadow padding does.
    struct SmallPanel {
        picked: Vec<Id>,
    }

    impl App for SmallPanel {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("page"), NodeKind::Stack)
                    .size(ui_core::layout::SizeClass::fixed(300.0, 300.0))
                    .sense(ui_core::Sense::PASS_THROUGH)
                    .child(
                        Decl::container(Id::str("panel"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                            .sense(ui_core::Sense::CLICK)
                            .menu(vec![ui_core::menu::MenuItem::new(Id::str("copy"), "Copy")]),
                    ),
            )
        }

        fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::MenuItem(item) = change {
                self.picked.push(item);
            }
            true
        }
    }

    /// A menu that reaches past the node it belongs to is still a node where it lands: with
    /// no host to present its own surface, the window paints it, so the row below the node
    /// must be in the published input region and must take the press.
    #[test]
    fn a_menu_reaching_past_its_node_still_takes_the_press_there() {
        let mut target = RegionTarget {
            inner: OffscreenTarget::new(300, 300),
            regions: Vec::new(),
        };
        let mut lp = AppLoop::new(SmallPanel { picked: Vec::new() });
        lp.frame(&mut target, None);

        // Near the panel's bottom edge, so the panel — 200 units tall — cannot hold the menu.
        let at = ui_core::geom::Point::new(50.0, 190.0);
        let button = ui_core::PointerButton::Secondary;
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        // The entrance's first frame is its start; a second one lands it where it rests.
        lp.frame_with_dt(&mut target, None, 0.2);
        lp.frame_with_dt(&mut target, None, 0.2);

        let row = lp
            .tree
            .find_by_id(crate::menu::Menu::row_id(0, 0))
            .and_then(|nid| lp.tree.get(nid))
            .map(|node| node.rect)
            .expect("the menu row is laid out");
        assert!(
            row.max_y() > 200.0,
            "the menu reaches below the node that owns it: {row:?}"
        );
        let press = row.center();
        assert!(
            target.regions.iter().any(|area| area.contains(press)),
            "the row past the node is in the input region: {:?}",
            target.regions
        );
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: press,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: press,
            button: ui_core::PointerButton::Primary,
        });
        // A click becomes a change the next frame applies, as any control's click does.
        lp.frame(&mut target, Some(press));
        assert_eq!(
            lp.app_mut().picked,
            [Id::str("copy")],
            "the row past the node took the press"
        );
    }

    /// Records the outcomes of programmatic and keyboard-driven menus.
    struct MenuSink {
        picked: Vec<Id>,
        closed: Vec<Id>,
    }

    impl App for MenuSink {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("page"), NodeKind::Stack)
                    .size(SizeClass::fixed(400.0, 300.0))
                    .background(Color::from_rgba8(0, 0, 255, 255), Corners::ZERO)
                    .child(
                        Decl::container(Id::str("row"), NodeKind::Column)
                            .size(SizeClass::fixed(200.0, 100.0))
                            .sense(ui_core::Sense::FOCUSABLE | ui_core::Sense::CLICK)
                            .background(Color::from_rgba8(0, 0, 0, 255), Corners::ZERO)
                            .menu(vec![
                                ui_core::menu::MenuItem::new(Id::str("copy"), "Copy"),
                                ui_core::menu::MenuItem::new(Id::str("paste"), "Paste"),
                            ]),
                    ),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            match change {
                ui_core::change::Change::MenuItem(item) => self.picked.push(item),
                ui_core::change::Change::MenuClosed => self.closed.push(id),
                _ => {}
            }
            true
        }
    }

    fn menu_sink() -> (AppLoop<MenuSink>, OffscreenTarget) {
        let mut target = OffscreenTarget::new(400, 300);
        let mut lp = AppLoop::new(MenuSink {
            picked: Vec::new(),
            closed: Vec::new(),
        });
        lp.frame(&mut target, None);
        (lp, target)
    }

    fn menu_key(lp: &mut AppLoop<MenuSink>, key: ui_core::Key) {
        lp.dispatch(ui_core::Event::KeyPressed { key });
    }

    /// A row's label and chord are both measured, not left at nothing.
    ///
    /// Menu text once declared no size class, and a nowrap line is capped by that hint: every
    /// text node measured zero, so the label column took the row's whole width and the chord
    /// was drawn from the trailing edge with no room of its own.
    #[test]
    fn a_menu_row_measures_its_label_and_chord() {
        let (mut lp, mut target) = menu_sink();
        let ctrl_shift = ui_core::Modifiers {
            ctrl: true,
            shift: true,
            ..ui_core::Modifiers::NONE
        };
        let chord = ui_core::KeyChord::new(ui_core::Key::Char('n'), ctrl_shift);
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("a"), "Create Hard Link")
                    .with_chord(chord, "Ctrl+Shift+N"),
                ui_core::menu::MenuItem::new(Id::str("b"), "A much longer row label here")
                    .with_chord(chord, "Ctrl+Shift+N"),
            ],
        ));
        // The entrance slides the panel; the rows rest where they stay once it lands.
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        for row in 0..2 {
            let id = crate::menu::Menu::row_id(0, row);
            let width = |part: &str| {
                lp.tree
                    .find_by_id(id.with(part))
                    .map(|nid| lp.tree.get(nid).unwrap().rect.size.w)
            };
            assert!(
                width("text").is_some_and(|w| w > 0.0),
                "row {row}: the label measured no width"
            );
            assert!(
                width("shortcut").is_some_and(|w| w > 0.0),
                "row {row}: the chord measured no width"
            );
        }
    }
    /// Quick actions share one row, each cell an equal share of the panel's width.
    ///
    /// The run grouping is the whole strip: a cell drawn as an ordinary row would stack the
    /// actions vertically and the panel would size itself to the tallest one.
    #[test]
    fn a_menu_strip_lays_its_quick_actions_side_by_side() {
        let (mut lp, mut target) = menu_sink();
        let mark = ui_core::menu::MenuIcon::new(
            ui_core::text::Family::Named("Material Symbols Outlined"),
            '\u{e14e}',
        );
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("cut"), "Cut").compact(mark),
                ui_core::menu::MenuItem::new(Id::str("copy"), "Copy").compact(mark),
                ui_core::menu::MenuItem::separator(),
                ui_core::menu::MenuItem::new(Id::str("paste"), "Paste"),
            ],
        ));
        // The entrance slides the panel; the rows rest where they stay once it lands.
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        let rect = |index: usize| {
            let id = crate::menu::Menu::row_id(0, index);
            let nid = lp.tree.find_by_id(id).expect("the row is a node");
            lp.tree.get(nid).expect("a live node").rect
        };
        let (cut, copy, paste) = (rect(0), rect(1), rect(3));
        assert_eq!(
            cut.origin.y, copy.origin.y,
            "the quick actions sit on one row"
        );
        assert!(copy.origin.x > cut.origin.x, "the cells run left to right");
        assert!(
            (cut.size.w - copy.size.w).abs() < 0.5,
            "cells share the strip's width equally"
        );
        assert!(
            paste.origin.y >= cut.max_y(),
            "a row after the strip starts below it"
        );
        let label = crate::menu::Menu::row_id(0, 0).with("label");
        let width = lp
            .tree
            .find_by_id(label)
            .and_then(|nid| lp.tree.get(nid))
            .map(|node| node.rect.size.w);
        assert!(
            width.is_some_and(|width| width > 0.0),
            "a cell's label is measured by the cell's share"
        );
    }

    /// A header's mark keeps its room: a long label ends before it, and it stays inside.
    ///
    /// Without the reservation in `level_width` a long label takes the panel's width and the
    /// mark is pushed past its edge, where it is clipped or drawn over the label.
    #[test]
    fn a_submenu_header_keeps_its_cascade_mark_inside_the_panel() {
        let (mut lp, mut target) = menu_sink();
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![ui_core::menu::MenuItem::submenu(
                Id::str("more"),
                "A label long enough to want the whole panel for itself",
                vec![ui_core::menu::MenuItem::new(Id::str("a"), "A")],
            )],
        ));
        // The entrance slides the panel; the rows rest where they stay once it lands.
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        let id = crate::menu::Menu::row_id(0, 0);
        let rect = |part: &str| {
            lp.tree
                .find_by_id(id.with(part))
                .and_then(|nid| lp.tree.get(nid))
                .map(|node| node.rect)
        };
        let label = rect("text").expect("the label is a node");
        let chevron = rect("chevron").expect("a header carries its cascade mark");
        assert!(chevron.size.w > 0.0, "the mark measured nothing");
        assert!(
            label.max_x() <= chevron.origin.x + 0.5,
            "the label ends before the mark's own room"
        );
    }

    /// A flyout hangs off the row that heads it, not off the menu's own corner.
    ///
    /// The anchor was read from the flyout's own `child_of`, which only names *its* flyout:
    /// every cascade unfolded from the root's click point and covered the level it came from.
    #[test]
    fn a_flyout_hangs_off_the_row_that_heads_it() {
        let (mut lp, mut target) = menu_sink();
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("copy"), "Copy"),
                ui_core::menu::MenuItem::submenu(
                    Id::str("more"),
                    "More",
                    vec![ui_core::menu::MenuItem::new(Id::str("nested"), "Nested")],
                ),
            ],
        ));
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Right);
        // Let the entrance settle so the rects are where they are painted.
        lp.frame_with_dt(&mut target, None, 0.2);
        let rect = |id| {
            lp.tree
                .find_by_id(id)
                .and_then(|nid| lp.tree.get(nid))
                .map(|node| node.visual_rect())
        };
        let header = rect(crate::menu::Menu::row_id(0, 1)).expect("the header is a row");
        let flyout = rect(crate::menu::Menu::panel_id(1)).expect("the flyout is a panel");
        assert!(
            flyout.min_x() >= header.max_x() - 0.5,
            "the flyout starts beside the row, not over the level"
        );
        assert!(
            (flyout.min_y() - header.min_y()).abs() < 0.5,
            "the flyout starts level with the row that heads it"
        );
    }

    /// A press outside the menu is the light dismiss every flyout has, and it is spent on
    /// that dismissal: nothing underneath hears it.
    #[test]
    fn a_press_outside_the_menu_dismisses_it_and_reaches_nothing() {
        let (mut lp, mut target) = menu_sink();
        let owner = Id::str("row");
        assert!(lp.open_menu_at(
            owner,
            ui_core::geom::Point::new(20.0, 20.0),
            vec![ui_core::menu::MenuItem::new(Id::str("copy"), "Copy")],
        ));
        lp.frame(&mut target, None);
        let outside = ui_core::geom::Point::new(350.0, 250.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: outside,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: outside,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, Some(outside));

        assert_eq!(lp.app_mut().closed, [owner], "the owner hears the dismissal");
        assert!(lp.app_mut().picked.is_empty(), "no row was picked");
        assert!(lp.menu.is_none(), "the menu is closed");
    }

    /// A divider is paint and spacing: neither the pointer nor the keyboard can choose it.
    #[test]
    fn a_separator_cannot_be_picked_by_pointer_or_keyboard() {
        let (mut lp, mut target) = menu_sink();
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("a"), "A"),
                ui_core::menu::MenuItem::separator(),
                ui_core::menu::MenuItem::new(Id::str("b"), "B"),
            ],
        ));
        // Down crosses the divider to the row after it.
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Down);
        assert_eq!(lp.menu.as_ref().and_then(|menu| menu.hovered), Some(2));

        // Let the entrance settle so the row rect is where it is painted.
        lp.frame_with_dt(&mut target, None, 0.2);
        let divider = lp
            .tree
            .find_by_id(crate::menu::Menu::row_id(0, 1))
            .and_then(|nid| lp.tree.get(nid))
            .map(|node| node.rect.center())
            .expect("the divider is a laid-out row");
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: divider,
            button: ui_core::PointerButton::Primary,
        });
        assert!(lp.app_mut().picked.is_empty(), "a divider is not a pick");
        assert!(lp.app_mut().closed.is_empty(), "a divider press is inert");
        assert!(lp.menu.is_some());

        menu_key(&mut lp, ui_core::Key::Enter);
        assert_eq!(lp.app_mut().picked, [Id::str("b")]);
    }

    /// A submenu's rows are the menu's rows: a pick reports the nested id to the owner.
    #[test]
    fn a_submenu_row_reports_its_nested_id() {
        let (mut lp, _target) = menu_sink();
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("copy"), "Copy"),
                ui_core::menu::MenuItem::submenu(
                    Id::str("more"),
                    "More",
                    vec![ui_core::menu::MenuItem::new(Id::str("nested"), "Nested")],
                ),
            ],
        ));
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Right);
        assert!(
            lp.menu.as_ref().is_some_and(|menu| menu.child.is_some()),
            "Right opens the hovered submenu"
        );
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Enter);

        assert_eq!(lp.app_mut().picked, [Id::str("nested")]);
        assert!(lp.app_mut().closed.is_empty());
        assert!(lp.menu.is_none());
    }

    /// A click on a flyout's row picks it, as a click on the menu's own row does.
    ///
    /// The flyout is a panel of its own: a press there must be judged inside it, not
    /// outside every level, or the click dismisses the whole menu instead of running the row.
    #[test]
    fn a_flyouts_row_picks_when_clicked() {
        let (mut lp, mut target) = menu_sink();
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("copy"), "Copy"),
                ui_core::menu::MenuItem::submenu(
                    Id::str("more"),
                    "More",
                    vec![ui_core::menu::MenuItem::new(Id::str("nested"), "Nested")],
                ),
            ],
        ));
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Right);
        // Let the entrances settle so the rows are where they are painted.
        lp.frame_with_dt(&mut target, None, 0.2);
        lp.frame_with_dt(&mut target, None, 0.2);
        let row = lp
            .tree
            .find_by_id(crate::menu::Menu::row_id(1, 0))
            .and_then(|nid| lp.tree.get(nid))
            .map(|node| node.visual_rect().center())
            .expect("the flyout's row is laid out");
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: row,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: row,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, Some(row));

        assert_eq!(
            lp.app_mut().picked,
            [Id::str("nested")],
            "the flyout's row runs the command it stands for"
        );
        assert!(lp.app_mut().closed.is_empty(), "the click is not a dismissal");
        assert!(lp.menu.is_none(), "the pick closes the menu");
    }

    /// The menu key opens the focused node's declared menu; Shift+F10 arrives as the same
    /// key on the platform side and follows the same path here.
    #[test]
    fn the_menu_key_and_shift_f10_open_the_focused_nodes_declared_menu() {
        let (mut lp, _target) = menu_sink();
        menu_key(&mut lp, ui_core::Key::Tab);
        assert_eq!(lp.focused(), Some(Id::str("row")));

        menu_key(&mut lp, ui_core::Key::Menu);
        assert!(lp.menu.is_some(), "the menu key opens the declared menu");
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Enter);
        assert_eq!(lp.app_mut().picked, [Id::str("copy")]);

        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..ui_core::Modifiers::default()
        }));
        menu_key(&mut lp, ui_core::Key::Menu);
        assert!(lp.menu.is_some(), "Shift+F10 opens it again");
        menu_key(&mut lp, ui_core::Key::Escape);
        assert_eq!(lp.app_mut().closed, [Id::str("row")]);
    }

    /// A disabled row cannot take the hover, so movement never stops on it.
    #[test]
    fn keyboard_movement_skips_disabled_rows() {
        let (mut lp, _target) = menu_sink();
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 20.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("a"), "A"),
                ui_core::menu::MenuItem::new(Id::str("b"), "B").disabled_because("not now"),
                ui_core::menu::MenuItem::new(Id::str("c"), "C"),
            ],
        ));
        menu_key(&mut lp, ui_core::Key::Down);
        menu_key(&mut lp, ui_core::Key::Down);
        assert_eq!(
            lp.menu.as_ref().and_then(|menu| menu.hovered),
            Some(2),
            "the disabled row is skipped"
        );
        menu_key(&mut lp, ui_core::Key::Enter);
        assert_eq!(lp.app_mut().picked, [Id::str("c")]);
    }

    /// Records clipboard payloads no editor took.
    struct PasteSink {
        pastes: Vec<(Id, ui_core::clipboard::ClipboardContent)>,
    }

    impl App for PasteSink {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("target"), NodeKind::Row)
                    .size(SizeClass::fixed(200.0, 100.0))
                    .sense(ui_core::Sense::FOCUSABLE | ui_core::Sense::CLICK)
                    .background(Color::WHITE, Corners::ZERO),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Paste(content) = change {
                self.pastes.push((id, content));
            }
            true
        }
    }

    /// Without a caret the whole payload is the app's; a text paste with one still edits.
    #[test]
    fn a_paste_without_an_editor_reaches_the_app_and_a_text_paste_still_edits() {
        let mut target = OffscreenTarget::new(300, 200);
        let mut lp = AppLoop::new(PasteSink { pastes: Vec::new() });
        lp.frame(&mut target, None);
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        });
        assert_eq!(lp.focused(), Some(Id::str("target")));

        press_ctrl(
            &mut lp,
            'v',
            ui_core::Modifiers {
                ctrl: true,
                ..ui_core::Modifiers::default()
            },
        );
        assert!(matches!(
            lp.take_clipboard_commands().last(),
            Some(crate::clipboard::ClipboardCommand::RequestPaste { .. })
        ));
        let files = ui_core::clipboard::ClipboardContent::Files {
            paths: vec![std::path::PathBuf::from("a.txt")],
            effect: ui_core::DropEffect::Copy,
        };
        assert!(lp.dispatch(ui_core::Event::Paste(files.clone())));
        assert_eq!(lp.app_mut().pastes, [(Id::str("target"), files)]);

        let (mut lp, seen) = focused_typing("");
        assert!(lp.dispatch(ui_core::Event::Paste(
            ui_core::clipboard::ClipboardContent::Text("hi".to_string())
        )));
        assert_eq!(
            seen.borrow().as_slice(),
            ["hi"],
            "a text paste still edits the focused buffer"
        );
    }

    /// An app whose node declares no menu and opens its own from the press point.
    #[derive(Default)]
    struct RequestedMenu {
        open_at: Option<ui_core::geom::Point>,
        picked: Vec<Id>,
        closed: Vec<Id>,
    }

    impl App for RequestedMenu {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            if let Some(at) = self.open_at.take() {
                cx.open_menu(
                    Id::str("row"),
                    at,
                    vec![ui_core::menu::MenuItem::new(Id::str("copy"), "Copy")],
                );
            }
            crate::from_decl(
                Decl::container(Id::str("page"), NodeKind::Stack)
                    .size(SizeClass::fixed(300.0, 300.0))
                    .background(Color::from_rgba8(0, 0, 255, 255), Corners::ZERO)
                    .child(
                        Decl::container(Id::str("row"), NodeKind::Column)
                            .size(SizeClass::fixed(200.0, 100.0))
                            .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                            .drag_data(ui_core::DragData::bytes("text/plain", b"row".to_vec()))
                            .background(Color::from_rgba8(0, 0, 0, 255), Corners::ZERO),
                    ),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            match change {
                ui_core::change::Change::MenuRequested(at) => self.open_at = Some(at),
                ui_core::change::Change::MenuItem(item) => self.picked.push(item),
                ui_core::change::Change::MenuClosed => self.closed.push(id),
                _ => {}
            }
            true
        }
    }

    /// A secondary tap on a node with no declared menu hands the point to the app, while a
    /// right drag that travels opens nothing; the menu the app builds reports its pick.
    #[test]
    fn a_secondary_press_without_a_declared_menu_asks_the_app_for_one() {
        let mut target = OffscreenTarget::new(300, 300);
        let mut lp = AppLoop::new(RequestedMenu::default());
        lp.frame(&mut target, None);
        let at = ui_core::geom::Point::new(50.0, 60.0);
        let button = ui_core::PointerButton::Secondary;

        // The row carries a payload, so a press cannot know yet whether the gesture is a
        // tap: a right drag must report its drop and open nothing.
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        assert_eq!(
            lp.app_mut().open_at,
            None,
            "the press alone must not ask for a menu a drag would leave behind"
        );
        let away = ui_core::geom::Point::new(at.x + 60.0, at.y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: away });
        lp.dispatch(ui_core::Event::PointerReleased { pos: away, button });
        assert_eq!(
            lp.app_mut().open_at,
            None,
            "a travelled right drag reports its drop, not a menu"
        );

        lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
        lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
        assert_eq!(
            lp.app_mut().open_at,
            Some(at),
            "the tap's release hands the node the press point for its rows"
        );
        lp.frame(&mut target, None);
        assert!(
            lp.menu.is_some(),
            "Cx::open_menu shows the rows the app built"
        );
        assert_eq!(lp.menu.as_ref().map(|menu| menu.owner), Some(Id::str("row")));

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Down,
        });
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Enter,
        });
        assert_eq!(lp.app_mut().picked, [Id::str("copy")]);
        assert!(lp.app_mut().closed.is_empty());
    }

    /// An app with no caret can still start the clipboard read from a build.
    struct AskingPaste {
        ask: bool,
        pastes: Vec<(Id, ui_core::clipboard::ClipboardContent)>,
    }

    impl App for AskingPaste {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            if self.ask {
                self.ask = false;
                cx.request_paste();
            }
            crate::from_decl(
                Decl::container(Id::str("target"), NodeKind::Row)
                    .size(SizeClass::fixed(200.0, 100.0))
                    .background(Color::WHITE, Corners::ZERO),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Paste(content) = change {
                self.pastes.push((id, content));
            }
            true
        }
    }

    /// The build's paste request routes its answer exactly like Ctrl+V without a caret.
    #[test]
    fn a_cx_paste_request_answers_on_the_focused_target() {
        let mut target = OffscreenTarget::new(300, 200);
        let mut lp = AppLoop::new(AskingPaste {
            ask: true,
            pastes: Vec::new(),
        });
        lp.frame(&mut target, None);
        assert!(matches!(
            lp.take_clipboard_commands().last(),
            Some(crate::clipboard::ClipboardCommand::RequestPaste { .. })
        ));

        let files = ui_core::clipboard::ClipboardContent::Files {
            paths: vec![std::path::PathBuf::from("a.txt")],
            effect: ui_core::DropEffect::Copy,
        };
        assert!(lp.dispatch(ui_core::Event::Paste(files.clone())));
        assert_eq!(lp.app_mut().pastes, [(Id::str("target"), files)]);
    }

    /// An app that asks the host for the platform's own menu from one build.
    struct AskingShellMenu {
        ask: Option<(Vec<std::path::PathBuf>, ui_core::geom::Point)>,
    }

    impl App for AskingShellMenu {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            if let Some((paths, at)) = self.ask.take() {
                cx.request_shell_menu(paths, at);
            }
            crate::from_decl(
                Decl::container(Id::str("page"), NodeKind::Stack)
                    .size(SizeClass::fixed(200.0, 200.0))
                    .background(Color::WHITE, Corners::ZERO),
            )
        }

        fn changed(&mut self, _id: Id, _change: ui_core::change::Change) -> bool {
            false
        }
    }

    /// A shell menu is handed to the host, never run inside the build that asked.
    ///
    /// The popup runs the shell's own loop until the user dismisses it; a build that opened
    /// it would leave the frame unpainted and the engine's own menu on screen meanwhile.
    #[test]
    fn a_shell_menu_request_leaves_for_the_host_after_the_frame() {
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(AskingShellMenu { ask: None });
        lp.frame(&mut target, None);
        assert!(lp.take_shell_menu().is_none(), "no build asked for a menu");

        let paths = vec![std::path::PathBuf::from("a.txt")];
        let at = ui_core::geom::Point::new(12.0, 34.0);
        lp.app_mut().ask = Some((paths.clone(), at));
        lp.frame(&mut target, None);
        let request = lp
            .take_shell_menu()
            .expect("the host runs it once this frame has presented");
        assert_eq!(request.paths, paths);
        assert_eq!(request.at, at);
        assert!(
            lp.take_shell_menu().is_none(),
            "one build's request is one popup"
        );
    }

    /// Records drag events and accepts drops on a zone that contains a child.
    struct Dragging {
        seen: Vec<(Id, ui_core::change::Change)>,
        /// Whether the declaration subscribes to drags; the only routing input.
        declare_in_tree: bool,
    }

    impl App for Dragging {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let decl = {
                let declared = self.declare_in_tree;
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .child(
                        Decl::container(Id::str("source"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(60.0, 60.0))
                            .background(Color::from_rgba8(200, 40, 40, 255), Corners::ZERO)
                            // One subscription expression: the drag bit is one of them, and a
                            // focusable node is what a press may move focus to.
                            .sense(
                                ui_core::Sense::HOVER
                                    | ui_core::Sense::FOCUSABLE
                                    | if declared {
                                        ui_core::Sense::DRAG
                                    } else {
                                        ui_core::Sense::NONE
                                    },
                            ),
                    )
                    .child(
                        Decl::container(Id::str("zone"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(120.0, 80.0))
                            .background(Color::from_rgba8(40, 200, 40, 255), Corners::ZERO)
                            // One subscription expression: a later `sense` replaces, it does not add.
                            .sense(if declared {
                                ui_core::Sense::DRAG | ui_core::Sense::DROP
                            } else {
                                ui_core::Sense::NONE
                            })
                            .child(
                                Decl::container(Id::str("inner"), NodeKind::Column)
                                    .size(ui_core::layout::SizeClass::fixed(40.0, 30.0))
                                    .background(Color::from_rgba8(40, 40, 200, 255), Corners::ZERO),
                            ),
                    )
            };
            crate::from_decl(decl)
        }
        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            // Only drag transitions count as "a drag happened"; a click is not one.
            if matches!(
                change,
                ui_core::change::Change::DragStarted
                    | ui_core::change::Change::Moved { .. }
                    | ui_core::change::Change::Dropped { .. }
                    | ui_core::change::Change::Cancelled
            ) {
                self.seen.push((id, change));
            }
            true
        }
    }

    #[test]
    fn a_node_declared_draggable_starts_a_drag_without_any_app_hook() {
        // The declaration alone must be enough: `Decl::draggable` is the single source of
        // truth, and an app that never implements `App::draggable` still gets drags.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(Dragging {
            seen: Vec::new(),
            declare_in_tree: true,
        });
        lp.frame(&mut target, None);
        let from = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: from });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(50.0, 20.0),
        });
        let started = lp.app_mut().seen.first().map(|(id, _)| *id);
        assert_eq!(
            started,
            Some(Id::str("source")),
            "the declaration alone must reach the app as a change"
        );
    }

    /// A source carrying `text/plain`, and a zone that takes what `accepts` names.
    struct Carrying {
        seen: Vec<(Id, ui_core::change::Change)>,
        accepts: &'static [&'static str],
    }

    impl App for Carrying {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let decl = Decl::container(Id::str("root"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                .child(
                    Decl::container(Id::str("source"), NodeKind::Column)
                        .size(ui_core::layout::SizeClass::fixed(60.0, 60.0))
                        .sense(ui_core::Sense::HOVER)
                        .drag_data(ui_core::DragData::bytes(
                            "text/plain",
                            b"hello".to_vec(),
                        )),
                )
                .child(
                    Decl::container(Id::str("zone"), NodeKind::Column)
                        .size(ui_core::layout::SizeClass::fixed(120.0, 80.0))
                        .accept_drop(self.accepts.iter().copied())
                        .child(
                            Decl::container(Id::str("inner"), NodeKind::Column)
                                .size(ui_core::layout::SizeClass::fixed(40.0, 30.0)),
                        ),
                );
            crate::from_decl(decl)
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            self.seen.push((id, change));
            true
        }
    }

    fn drag_across(lp: &mut AppLoop<Carrying>, to: ui_core::geom::Point) {
        let from = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: from });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
    }

    #[test]
    fn a_payload_crosses_to_the_node_that_takes_it() {
        // The whole point of the abstraction: the source never names the target, and the
        // target never names the source; the declared types decide, and the payload arrives
        // once on release.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(Carrying {
            seen: Vec::new(),
            accepts: &["text/plain"],
        });
        lp.frame(&mut target, None);
        drag_across(&mut lp, ui_core::geom::Point::new(50.0, 80.0));
        let entered = lp.app_mut().seen.iter().find_map(|(id, change)| match change {
            ui_core::change::Change::DragEntered { types } => Some((*id, types.clone())),
            _ => None,
        });
        assert_eq!(
            entered,
            Some((Id::str("zone"), vec!["text/plain".to_string()])),
            "the zone under the pointer hears what it is about to receive"
        );

        let at = ui_core::geom::Point::new(50.0, 80.0);
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        let dropped = lp.app_mut().seen.iter().find_map(|(id, change)| match change {
            ui_core::change::Change::DroppedOn(data) => Some((*id, data.clone())),
            _ => None,
        });
        assert_eq!(
            dropped.as_ref().map(|(id, _)| *id),
            Some(Id::str("zone")),
            "the drop lands on the accepting zone"
        );
        assert_eq!(
            dropped.and_then(|(_, data)| data.data.text().map(str::to_owned)),
            Some("hello".to_owned()),
            "the payload's bytes arrive intact"
        );
        assert!(
            lp.app_mut().seen.iter().any(|(id, change)| *id == Id::str("source")
                && matches!(change, ui_core::change::Change::Dropped { target: Some(zone), .. } if *zone == Id::str("zone"))),
            "the source learns where its payload went"
        );
    }

    #[test]
    fn an_in_app_drag_lights_the_zone() {
        // The highlight every app reads is `drag_over_within`, and it is only true when the
        // drag's route names the target; hearing `DragEntered` alone would leave it dark.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(Carrying {
            seen: Vec::new(),
            accepts: &["text/plain"],
        });
        lp.frame(&mut target, None);
        drag_across(&mut lp, ui_core::geom::Point::new(50.0, 80.0));
        let _ = lp.frame(&mut target, None);
        assert_eq!(
            lp.drag_path.last(),
            Some(&Id::str("zone")),
            "after a frame: {:?}",
            lp.drag_path
        );
    }

    #[test]
    fn a_target_that_takes_another_type_is_never_highlighted_or_dropped_on() {
        // The types are the contract: a zone that only takes images must not light up for
        // text, and the drop must land nowhere rather than hand it the wrong bytes.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(Carrying {
            seen: Vec::new(),
            accepts: &["image/png"],
        });
        lp.frame(&mut target, None);
        drag_across(&mut lp, ui_core::geom::Point::new(50.0, 80.0));
        let at = ui_core::geom::Point::new(50.0, 80.0);
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        assert!(
            !lp.app_mut()
                .seen
                .iter()
                .any(|(_, change)| matches!(change, ui_core::change::Change::DragEntered { .. }
                    | ui_core::change::Change::DroppedOn(_))),
            "no highlight, no payload: {:?}",
            lp.app_mut()
                .seen
                .iter()
                .map(|(id, c)| (*id, std::mem::discriminant(c)))
                .collect::<Vec<_>>()
        );
        assert!(
            lp.app_mut().seen.iter().any(|(id, change)| *id == Id::str("source")
                && matches!(change, ui_core::change::Change::Dropped { target: None, .. })),
            "the source hears that its payload landed nowhere"
        );
    }

    #[test]
    fn a_drag_that_leaves_the_target_takes_its_highlight_down() {
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(Carrying {
            seen: Vec::new(),
            accepts: &["text/plain"],
        });
        lp.frame(&mut target, None);
        drag_across(&mut lp, ui_core::geom::Point::new(50.0, 80.0));
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(400.0, 400.0),
        });
        assert!(
            lp.app_mut()
                .seen
                .iter()
                .any(|(id, change)| *id == Id::str("zone")
                    && matches!(change, ui_core::change::Change::DragExited)),
            "leaving the zone ends its hover"
        );
    }

    #[test]
    fn a_system_drag_reaches_the_target_with_its_decoded_payload() {
        // A drag from another application never passes through the press machinery: the
        // platform reports it, and the target must still see the same enter and drop.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(Carrying {
            seen: Vec::new(),
            accepts: &["text/*"],
        });
        lp.frame(&mut target, None);
        let at = ui_core::geom::Point::new(50.0, 80.0);
        lp.dispatch(ui_core::Event::Dnd(ui_core::DndEvent::Entered {
            pos: at,
            types: vec!["text/plain".to_string()],
            effects: ui_core::DropEffects::COPY,
        }));
        lp.dispatch(ui_core::Event::Dnd(ui_core::DndEvent::Dropped {
            pos: at,
            data: ui_core::DragData::bytes("text/plain", b"from elsewhere".to_vec()),
            button: ui_core::PointerButton::Primary,
            performer: ui_core::Performer::Target,
        }));
        let seen: Vec<_> = lp
            .app_mut()
            .seen
            .iter()
            .map(|(id, change)| (*id, change))
            .collect();
        assert!(
            seen.iter().any(|(id, change)| *id == Id::str("zone")
                && matches!(change, ui_core::change::Change::DragEntered { .. })),
            "the system drag highlights the zone: {seen:?}"
        );
        assert!(
            seen.iter().any(|(id, change)| *id == Id::str("zone")
                && matches!(change, ui_core::change::Change::DroppedOn(data)
                    if data.data.text() == Some("from elsewhere"))),
            "the payload decoded by the platform arrives at the drop: {seen:?}"
        );
    }

    /// A card that leaves the app, stacked on a sibling that must not travel with it.
    struct LeavingCard;

    impl App for LeavingCard {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("root"), NodeKind::Stack)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .child(
                        Decl::container(Id::str("under"), NodeKind::Box)
                            .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                            .background(
                                ui_core::geom::Color::rgb(0.0, 0.0, 1.0),
                                ui_core::geom::Corners::ZERO,
                            ),
                    )
                    .child(
                        Decl::container(Id::str("source"), NodeKind::Box)
                            .size(ui_core::layout::SizeClass::fixed(120.0, 80.0))
                            .background(
                                ui_core::geom::Color::rgb(1.0, 0.0, 0.0),
                                ui_core::geom::Corners::all(20.0),
                            )
                            .drag_data(ui_core::DragData::bytes("text/plain", b"card".to_vec()))
                            .drag_out(),
                    ),
            )
        }
    }

    #[test]
    fn a_drag_image_is_the_element_and_never_what_sat_behind_it() {
        // The host must be able to draw the element alone. A surface photograph would carry
        // the blue sibling that lies under the card's rounded corners; the rendered subtree
        // leaves them transparent instead.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(LeavingCard);
        lp.frame(&mut target, None);
        let nid = lp
            .tree()
            .find_by_id(Id::str("source"))
            .expect("the source is laid out");
        let rect = lp
            .tree()
            .get(nid)
            .expect("the source has a node")
            .visual_rect();
        let at = ui_core::geom::Point::new(rect.origin.x + 60.0, rect.origin.y + 40.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(at.x + 60.0, at.y),
        });
        let drag = lp
            .take_native_drag()
            .expect("the gesture leaves for the host");
        let image = drag.image.expect("the host gets the element's own pixels");
        assert_eq!(
            image.size,
            (120, 80),
            "the image is the source box, not the surface"
        );
        let center = ((40 * 120 + 60) * 4) as usize;
        let pixel = &image.bgra[center..center + 4];
        assert!(
            pixel[2] > 200 && pixel[0] < 60,
            "the card's own fill is in the image: {pixel:?}"
        );
        assert!(
            image
                .bgra
                .chunks_exact(4)
                .all(|px| px[0] < 60 || px[2] > 200),
            "nothing that sat behind the card may travel with it"
        );
        assert_eq!(
            image.hotspot,
            (60, 40),
            "the image hangs from the grab point"
        );
    }

    #[test]
    fn a_secondary_press_on_a_payload_starts_a_right_drag() {
        // The right button's drag belongs to the payload, not to `SECONDARY`: a node that
        // never asked to hear the press still has to leave with the pointer. The threshold
        // still separates the drag from the tap whose menu the press withheld.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(LeavingCard);
        lp.frame(&mut target, None);
        let nid = lp
            .tree()
            .find_by_id(Id::str("source"))
            .expect("the source is laid out");
        let rect = lp
            .tree()
            .get(nid)
            .expect("the source has a node")
            .visual_rect();
        let at = ui_core::geom::Point::new(rect.origin.x + 60.0, rect.origin.y + 40.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Secondary,
        });
        // A tap is not a drag: nothing leaves for the host.
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: at,
            button: ui_core::PointerButton::Secondary,
        });
        assert!(
            lp.take_native_drag().is_none(),
            "a secondary tap opens the menu instead of dragging"
        );
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Secondary,
        });
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(at.x + 60.0, at.y),
        });
        let drag = lp
            .take_native_drag()
            .expect("a right drag leaves for the host like a left one");
        assert!(
            drag.image.is_some(),
            "the host gets the element's pixels for a right drag too"
        );
    }

    /// An app that keeps no drag state of its own and answers every change with "nothing".
    struct Silent;

    impl App for Silent {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .child(
                        Decl::container(Id::str("zone"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(120.0, 80.0))
                            .accept_drop(["text/*"]),
                    ),
            )
        }

        fn changed(&mut self, _id: Id, _change: ui_core::change::Change) -> bool {
            false
        }
    }

    #[test]
    fn a_drag_entering_a_target_asks_for_a_frame_by_itself() {
        // The highlight and the effect the host publishes are both read from the next
        // frame, so a target change must request one even when the app reports nothing:
        // otherwise the pointer keeps showing the previous effect to the OS.
        let mut target = OffscreenTarget::new(220, 220);
        let mut lp = AppLoop::new(Silent);
        lp.frame(&mut target, None);
        assert!(
            lp.dispatch(ui_core::Event::Dnd(ui_core::DndEvent::Entered {
                pos: ui_core::geom::Point::new(20.0, 20.0),
                types: vec!["text/plain".to_string()],
                effects: ui_core::DropEffects::COPY,
            })),
            "entering a target is a frame, whatever the app answered"
        );
    }

    /// A red block offset by `dx`, with a green sibling that must not move.
    struct Offset {
        dx: f32,
    }

    impl App for Offset {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let decl = {
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .child(
                        Decl::container(Id::str("mover"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .sense(ui_core::Sense::HOVER)
                            .offset(self.dx, 0.0)
                            .background(Color::from_rgba8(255, 0, 0, 255), Corners::ZERO),
                    )
                    .child(
                        Decl::container(Id::str("stay"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .background(Color::from_rgba8(0, 255, 0, 255), Corners::ZERO),
                    )
            };
            crate::from_decl(decl)
        }
    }

    /// A frosted panel with no fill: the blur is its only painted output.
    struct Frost {
        dx: f32,
    }

    impl App for Frost {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let decl = {
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .child(
                        Decl::container(Id::str("frost"), NodeKind::Box)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .offset(self.dx, 0.0)
                            .backdrop_blur(4.0),
                    )
            };
            crate::from_decl(decl)
        }
    }

    #[test]
    fn a_moved_frost_dirties_where_it_was_and_where_it_is() {
        // A backdrop writes no primitive of its own, so a panel that only frosts owns no
        // fingerprint: moving it must still repaint both the blur it leaves behind and the
        // one it now draws.
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(Frost { dx: 0.0 });
        lp.frame(&mut target, None);
        lp.app_mut().dx = 80.0;
        let out = lp.frame(&mut target, None);
        let rects = out.damage.rects(Rect::new(0.0, 0.0, 200.0, 200.0));
        let covers_old = rects
            .iter()
            .any(|r| r.contains(ui_core::geom::Point::new(20.0, 20.0)));
        let covers_new = rects
            .iter()
            .any(|r| r.contains(ui_core::geom::Point::new(100.0, 20.0)));
        assert!(covers_old, "the vacated blur must be repainted: {rects:?}");
        assert!(covers_new, "the moved frost must be repainted: {rects:?}");
    }

    /// A frost beside a small block whose colour the app flips.
    struct FrostBeside {
        lit: bool,
    }

    impl App for FrostBeside {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let dot = if self.lit {
                Color::from_rgba8(255, 0, 0, 255)
            } else {
                Color::from_rgba8(0, 0, 255, 255)
            };
            let decl = {
                Decl::container(Id::str("root"), NodeKind::Row)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .child(
                        Decl::container(Id::str("frost"), NodeKind::Box)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .backdrop_blur(4.0),
                    )
                    .child(
                        Decl::container(Id::str("dot"), NodeKind::Box)
                            .size(ui_core::layout::SizeClass::fixed(10.0, 10.0))
                            .background(dot, Corners::ZERO),
                    )
            };
            crate::from_decl(decl)
        }
    }

    #[test]
    fn a_frost_repaints_when_something_changes_within_its_reach() {
        // The dot sits outside the frost and inside its kernel: its pixels change, so the
        // blur must be recomputed even though nothing inside the frost moved. A panel with
        // no primitives of its own has no fingerprint to inherit damage from unless the
        // backdrop itself is tracked.
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(FrostBeside { lit: false });
        lp.frame(&mut target, None);
        lp.app_mut().lit = true;
        let out = lp.frame(&mut target, None);
        let rects = out.damage.rects(Rect::new(0.0, 0.0, 200.0, 200.0));
        assert!(
            rects
                .iter()
                .any(|r| r.contains(ui_core::geom::Point::new(10.0, 10.0))),
            "the frost must repaint its whole area: {rects:?}"
        );
    }

    /// A box showing a registered texture.
    struct Picture;

    impl App for Picture {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let decl = {
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(100.0, 100.0))
                    .child(
                        Decl::container(Id::str("pic"), NodeKind::Box)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .background_image(3, ui_core::tree::ImageFit::Fill),
                    )
            };
            crate::from_decl(decl)
        }
    }

    #[test]
    fn new_pixels_behind_a_known_image_id_damage_the_node() {
        // Re-registering an id with a new buffer changes no primitive's geometry, so the
        // fingerprint has to name the buffer or the frame reports nothing and the old
        // picture stays on screen.
        let image = |data: Vec<u8>| ui_core::draw::ImageEntry {
            id: 3,
            width: 1,
            height: 1,
            source: ui_core::draw::ImageSource::Pixels(std::sync::Arc::new(data)),
            colour: ui_core::colour::ContentColour::SDR,
            sampling: ui_core::draw::Sampling::Smooth,
        };
        let mut target = OffscreenTarget::new(100, 100);
        let mut lp = AppLoop::new(Picture);
        lp.register_image(image(vec![0, 0, 0, 255]));
        lp.frame(&mut target, None);
        lp.register_image(image(vec![255, 0, 0, 255]));
        let out = lp.frame(&mut target, None);
        let rects = out.damage.rects(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(
            rects
                .iter()
                .any(|r| r.contains(ui_core::geom::Point::new(20.0, 20.0))),
            "a replaced texture must repaint its node, got {rects:?}"
        );
    }

    /// A picture a build hands over lives for the frame it painted; one registered on the
    /// loop lives for the run. A picker hands over a thumbnail per visible row, so a table
    /// that kept every one of them grows as long as the user browses.
    #[test]
    fn a_builds_picture_does_not_outlive_its_frame() {
        struct EachFrame(u64);

        impl App for EachFrame {
            fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                let id = self.0;
                self.0 += 1;
                cx.register_image(ui_core::draw::ImageEntry {
                    id,
                    width: 1,
                    height: 1,
                    source: ui_core::draw::ImageSource::Pixels(std::sync::Arc::new(vec![
                        0, 0, 0, 255,
                    ])),
                    colour: ui_core::colour::ContentColour::SDR,
                    sampling: ui_core::draw::Sampling::Smooth,
                });
                crate::from_decl(
                    Decl::container(Id::str("pic"), NodeKind::Box)
                        .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                        .background_image(id, ui_core::tree::ImageFit::Fill),
                )
            }
        }

        let texture = |id: u64| ui_core::draw::ImageEntry {
            id,
            width: 1,
            height: 1,
            source: ui_core::draw::ImageSource::Pixels(std::sync::Arc::new(vec![0, 0, 0, 255])),
            colour: ui_core::colour::ContentColour::SDR,
            sampling: ui_core::draw::Sampling::Smooth,
        };
        let mut target = OffscreenTarget::new(100, 100);
        let mut lp = AppLoop::new(EachFrame(10));
        lp.register_image(texture(3));
        for _ in 0..5 {
            lp.frame(&mut target, None);
        }
        assert_eq!(
            lp.images.len(),
            2,
            "the run's picture and this frame's, not five frames' worth"
        );
        assert!(lp.images.contains_key(&3), "the run's picture stays");
    }

    /// An app whose picture is a canvas: two columns, and a version the app owns.
    struct CanvasPicture {
        version: u64,
        lit: bool,
        declare: bool,
    }

    const CANVAS: ui_core::draw::CanvasId = ui_core::draw::CanvasId(9);

    impl App for CanvasPicture {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let mut list = ui_core::draw::DrawList::new(Rect::new(0.0, 0.0, 2.0, 2.0));
            let column = |x: f32, fill: Color| ui_core::draw::RectPrim {
                rect: Rect::new(x, 0.0, 1.0, 2.0),
                radius: Corners::ZERO,
                fill,
                gradient: None,
                border: None,
                border_gradient: None,
                notch: None,
            };
            let lit = if self.lit {
                Color::from_rgba8(0, 0, 255, 255)
            } else {
                Color::from_rgba8(255, 0, 0, 255)
            };
            list.push_rect(ui_core::draw::DrawList::canvas_owner(), column(0.0, lit));
            list.push_rect(
                ui_core::draw::DrawList::canvas_owner(),
                column(1.0, Color::from_rgba8(0, 255, 0, 255)),
            );
            if self.declare {
                cx.canvas(ui_core::draw::Canvas {
                    id: CANVAS,
                    size: (2, 2),
                    version: self.version,
                    list,
                });
                cx.register_image(ui_core::draw::ImageEntry {
                    id: 3,
                    width: 2,
                    height: 2,
                    source: ui_core::draw::ImageSource::Canvas(CANVAS),
                    colour: ui_core::colour::ContentColour::SDR,
                    sampling: ui_core::draw::Sampling::Pixelated,
                });
            }
            crate::from_decl(
                Decl::container(Id::str("root"), NodeKind::Box)
                    .size(ui_core::layout::SizeClass::fixed(8.0, 8.0))
                    .background_image(3, ui_core::tree::ImageFit::Fill)
                    .image_sampling(ui_core::draw::Sampling::Pixelated),
            )
        }
    }

    /// A canvas repaints when the app says its content changed, and not before: the version
    /// is the whole cache key, so a list rebuilt with new colours behind the same version
    /// must leave the target alone.
    #[test]
    fn a_canvas_repaints_only_when_its_version_changes() {
        let mut target = OffscreenTarget::new(8, 8);
        let mut lp = AppLoop::new(CanvasPicture {
            version: 1,
            lit: false,
            declare: true,
        });
        lp.frame(&mut target, None);
        assert_eq!(
            target.pixels.get(1, 1),
            [255, 0, 0, 255],
            "the first frame paints the canvas"
        );
        lp.app_mut().lit = true;
        lp.frame(&mut target, None);
        assert_eq!(
            target.pixels.get(1, 1),
            [255, 0, 0, 255],
            "an unchanged version keeps the target"
        );
        lp.app_mut().version = 2;
        lp.frame(&mut target, None);
        assert_eq!(
            target.pixels.get(1, 1),
            [0, 0, 255, 255],
            "a new version repaints it"
        );
    }

    /// A canvas is in its own pixels end to end: two 1-pixel columns drawn over an 8-pixel
    /// box with nearest sampling become two 4-pixel blocks, with no offset between them.
    #[test]
    fn canvas_pixels_reach_the_node_at_the_canvas_scale() {
        let mut target = OffscreenTarget::new(8, 8);
        let mut lp = AppLoop::new(CanvasPicture {
            version: 1,
            lit: false,
            declare: true,
        });
        lp.frame(&mut target, None);
        assert_eq!(target.pixels.get(1, 1), [255, 0, 0, 255]);
        assert_eq!(target.pixels.get(3, 1), [255, 0, 0, 255]);
        assert_eq!(
            target.pixels.get(4, 1),
            [0, 255, 0, 255],
            "the second canvas column starts at the fourth surface pixel"
        );
        assert_eq!(target.pixels.get(6, 6), [0, 255, 0, 255]);
    }

    /// A target only outlives the frame that names it, so a canvas that leaves and comes
    /// back must be painted again: its bytes are gone even though its version is not new.
    #[test]
    fn a_canvas_that_leaves_and_returns_is_repainted() {
        let mut target = OffscreenTarget::new(8, 8);
        let mut lp = AppLoop::new(CanvasPicture {
            version: 1,
            lit: false,
            declare: true,
        });
        lp.frame(&mut target, None);
        assert_eq!(target.pixels.get(1, 1), [255, 0, 0, 255]);
        lp.app_mut().declare = false;
        lp.frame(&mut target, None);
        assert_eq!(
            target.pixels.get(1, 1),
            [0, 0, 0, 0],
            "the frame without the canvas draws nothing where it was"
        );
        lp.app_mut().declare = true;
        lp.app_mut().lit = true;
        lp.frame(&mut target, None);
        assert_eq!(
            target.pixels.get(1, 1),
            [0, 0, 255, 255],
            "a returning canvas is painted from its list, not from a dropped target"
        );
    }

    /// A paragraph with one linked word, whose ink the app flips.
    struct LinkedWord {
        lit: bool,
    }

    impl App for LinkedWord {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let link = if self.lit {
                Color::from_rgba8(0, 0, 255, 255)
            } else {
                Color::from_rgba8(0, 100, 200, 255)
            };
            let mut label = Decl::container(Id::str("label"), ui_core::NodeKind::Text);
            label.data = ui_core::NodeData::Text {
                ink: Color::from_rgba8(20, 20, 20, 255),
                block: std::sync::Arc::new(ui_core::TextBlock {
                    runs: vec![
                        ui_core::TextRun::new("see ", ui_core::TextStyle::new(14.0)),
                        ui_core::TextRun::new("here", ui_core::TextStyle::new(14.0)).inked(link),
                    ],
                    ..Default::default()
                }),
                caret: None,
                selection: None,
                composing: None,
                nowrap: false,
            };
            label.size_class = SizeClass::fixed(120.0, 20.0);
            let decl = Decl::container(Id::str("root"), ui_core::NodeKind::Column).child(label);
            crate::from_decl(decl)
        }
    }

    #[test]
    fn a_runs_ink_change_repaints_its_paragraph() {
        // The paragraph's own ink never changes, and the shaped glyph boxes stay where they
        // were, so the recolour only reaches the frame if a glyph's colour is in the
        // fingerprint. It also pins that a run ink survives the pass: the glyph carries it.
        let mut target = OffscreenTarget::new(200, 100);
        let mut lp = AppLoop::new(LinkedWord { lit: false });
        lp.frame(&mut target, None);
        lp.app_mut().lit = true;
        let out = lp.frame(&mut target, None);
        let rects = out.damage.rects(Rect::new(0.0, 0.0, 200.0, 100.0));
        assert!(
            !rects.is_empty(),
            "a recoloured run must repaint its paragraph"
        );

        // The control: a third frame with nothing changed reports nothing.
        let settled = lp.frame(&mut target, None);
        assert!(
            settled.damage.is_none(),
            "an unchanged frame must stay quiet: {:?}",
            settled.damage
        );
    }

    #[test]
    fn a_moved_node_dirties_both_the_old_and_new_area() {
        // An offset stresses the damage contract: when a node moves, the pixels it
        // vacates must be cleared as well as the ones it now covers, or a ghost stays.
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(Offset { dx: 0.0 });
        lp.frame(&mut target, None);

        lp.app_mut().dx = 80.0;
        let out = lp.frame(&mut target, None);
        let rects = out.damage.rects(Rect::new(0.0, 0.0, 200.0, 200.0));
        let covers_old = rects
            .iter()
            .any(|r| r.contains(ui_core::geom::Point::new(20.0, 20.0)));
        let covers_new = rects
            .iter()
            .any(|r| r.contains(ui_core::geom::Point::new(100.0, 20.0)));
        assert!(covers_old, "the vacated area must be damaged: {rects:?}");
        assert!(
            covers_new,
            "the newly covered area must be damaged: {rects:?}"
        );
    }

    /// A box whose declared width the app toggles; the declaration never sees the tween.
    struct Resizing {
        wide: bool,
    }

    impl App for Resizing {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let width = if self.wide { 200.0 } else { 100.0 };
            Box::new(
                crate::Container::new(Id::str("resize"), NodeKind::Box)
                    .size(SizeClass {
                        width: AxisSize::fixed(width),
                        height: AxisSize::fixed(50.0),
                        grow_x: ui_core::layout::Grow::No,
                        grow_y: ui_core::layout::Grow::No,
                    })
                    .transition_size(ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear)),
            )
        }
    }

    fn resized_width(lp: &AppLoop<Resizing>) -> f32 {
        lp.tree()
            .find_by_id(Id::str("resize"))
            .and_then(|nid| lp.tree().get(nid))
            .map(|n| n.rect.size.w)
            .expect("resize box")
    }

    #[test]
    fn a_size_transition_reaches_its_target_through_the_layout() {
        // The declaration only ever names 100 or 200; the frames in between must come from
        // the engine's interpolation, and layout must see each of them.
        let mut target = OffscreenTarget::new(320, 120);
        let mut lp = AppLoop::new(Resizing { wide: false });
        lp.frame(&mut target, None);
        assert_eq!(
            resized_width(&lp),
            100.0,
            "the first frame is the target itself"
        );

        lp.app_mut().wide = true;
        // The frame that observes the change still shows the old size; the leg starts here.
        lp.frame_with_dt(&mut target, None, 0.0);
        assert_eq!(
            resized_width(&lp),
            100.0,
            "a retarget starts from where the box is"
        );
        lp.frame_with_dt(&mut target, None, 0.05);
        let middle = resized_width(&lp);
        assert!(
            (middle - 150.0).abs() < 6.0,
            "halfway through a 100ms linear leg the box is about 150 wide, got {middle}"
        );

        lp.frame_with_dt(&mut target, None, 0.06);
        assert_eq!(
            resized_width(&lp),
            200.0,
            "past the leg the box is its declared size"
        );
    }

    /// A card that hugs its content and walks its height when a second row comes and goes.
    struct Hugging {
        extra: bool,
    }

    impl App for Hugging {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let row = |id: &'static str| {
                Box::new(
                    crate::Container::new(Id::str(id), NodeKind::Box)
                        .size(SizeClass::fixed(120.0, 40.0)),
                ) as Box<dyn crate::Element>
            };
            Box::new(
                crate::Container::new(Id::str("host"), NodeKind::Stack).child(Box::new(
                    crate::Container::new(Id::str("hug"), NodeKind::Column)
                        .size(SizeClass {
                            width: AxisSize::fixed(120.0),
                            height: AxisSize::hug(0.0, 0.0),
                            grow_x: ui_core::layout::Grow::No,
                            grow_y: ui_core::layout::Grow::No,
                        })
                        .transition_size(ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear))
                        .child(row("hug/one"))
                        .child_if(self.extra, || row("hug/two")),
                )),
            )
        }
    }

    fn hug_height(lp: &AppLoop<Hugging>) -> f32 {
        lp.tree()
            .find_by_id(Id::str("hug"))
            .and_then(|nid| lp.tree().get(nid))
            .map(|n| n.rect.size.h)
            .expect("hugging card")
    }

    #[test]
    fn a_hug_axis_walks_to_the_extent_its_content_measured() {
        // A hug axis has no declared target: the extent is the measurement. The card must
        // neither jump to the new content nor snap back once the leg starts, and the same
        // leg must run backwards when the row goes.
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(Hugging { extra: false });
        lp.frame(&mut target, None);
        assert_eq!(hug_height(&lp), 40.0, "the card hugs its one row");

        lp.app_mut().extra = true;
        // The app changed state outside a control, which the declaration cache cannot see.
        lp.invalidate_decls();
        // The row's frame lays out with the old extent: the new one is a measurement.
        lp.frame_with_dt(&mut target, None, 0.0);
        assert_eq!(
            hug_height(&lp),
            40.0,
            "a grown card starts from where it was"
        );
        // The next frame reads the measurement and starts the leg from the same place.
        lp.frame_with_dt(&mut target, None, 0.0);
        assert_eq!(hug_height(&lp), 40.0, "the leg starts from the old extent");
        lp.frame_with_dt(&mut target, None, 0.05);
        let middle = hug_height(&lp);
        assert!(
            (middle - 60.0).abs() < 3.0,
            "halfway through a 100ms linear leg the card is about 60 high, got {middle}"
        );
        lp.frame_with_dt(&mut target, None, 0.06);
        assert_eq!(
            hug_height(&lp),
            80.0,
            "the leg lands on the measured extent"
        );

        lp.app_mut().extra = false;
        lp.invalidate_decls();
        lp.frame_with_dt(&mut target, None, 0.0);
        assert_eq!(hug_height(&lp), 80.0, "a shrunk card does not snap");
        lp.frame_with_dt(&mut target, None, 0.0);
        lp.frame_with_dt(&mut target, None, 0.05);
        let back = hug_height(&lp);
        assert!(
            (back - 60.0).abs() < 3.0,
            "the same leg runs down: about 60 high, got {back}"
        );
        lp.frame_with_dt(&mut target, None, 0.06);
        assert_eq!(
            hug_height(&lp),
            40.0,
            "it settles back on the one-row extent"
        );
    }

    /// A card declared only while `shown`; it states how it enters and leaves.
    struct Appearing {
        shown: bool,
        enter: Option<ui_core::tree::Presence>,
        exit: Option<ui_core::tree::Presence>,
    }

    impl App for Appearing {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let (enter, exit) = (self.enter, self.exit);
            Box::new(
                crate::Container::new(Id::str("host"), NodeKind::Stack).child_if(
                    self.shown,
                    || {
                        let mut card = crate::Container::new(Id::str("card"), NodeKind::Box)
                            .size(SizeClass::fixed(60.0, 40.0))
                            .background(Color::from_rgba8(220, 60, 60, 255), Corners::ZERO)
                            .sense(ui_core::Sense::CLICK | ui_core::Sense::HOVER);
                        if let Some(enter) = enter {
                            card = card.enter(enter);
                        }
                        if let Some(exit) = exit {
                            card = card.exit(exit);
                        }
                        Box::new(card)
                    },
                ),
            )
        }
    }

    #[test]
    fn an_entering_node_fades_in_from_its_declared_opacity() {
        // A node that did not exist last frame has no previous value to tween from: if the
        // enter starts at the declaration, the first frame is its end state and the fade is
        // never seen. The declared start is what the first frame must paint.
        let mut target = OffscreenTarget::new(120, 120);
        let mut lp = AppLoop::new(Appearing {
            shown: false,
            enter: Some(ui_core::tree::Presence::fade(
                ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear),
                0.0,
            )),
            exit: None,
        });
        lp.frame_with_dt(&mut target, None, 0.0);
        let empty = target.pixels.get(30, 20);
        lp.app_mut().shown = true;
        // A zero step: the node appears this frame at the opacity it declared.
        lp.frame_with_dt(&mut target, None, 0.0);
        assert_eq!(
            target.pixels.get(30, 20),
            empty,
            "the first frame paints the declared start, not the end state"
        );
        lp.frame_with_dt(&mut target, None, 0.05);
        let half = target.pixels.get(30, 20);
        assert_ne!(
            half, empty,
            "half way in it is already drawing, got {half:?}"
        );
        assert_ne!(
            half,
            [220, 60, 60, 255],
            "and it has not landed yet, got {half:?}"
        );
        lp.frame_with_dt(&mut target, None, 0.1);
        assert_eq!(
            target.pixels.get(30, 20),
            [220, 60, 60, 255],
            "the fade lands on the declared state"
        );
    }

    #[test]
    fn an_entering_node_is_painted_from_its_start_offset() {
        // The offset end is stated, never travelled from layout: the card paints and takes
        // hits where it is drawn, and the box it will land on stays where layout put it.
        let mut target = OffscreenTarget::new(200, 120);
        let mut lp = AppLoop::new(Appearing {
            shown: false,
            enter: Some(ui_core::tree::Presence::slide(
                ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear),
                1.0,
                ui_core::geom::Point::new(40.0, 0.0),
            )),
            exit: None,
        });
        lp.frame_with_dt(&mut target, None, 0.0);
        let empty = target.pixels.get(70, 20);

        lp.app_mut().shown = true;
        lp.frame_with_dt(&mut target, None, 0.0);
        assert_eq!(
            target.pixels.get(70, 20),
            [220, 60, 60, 255],
            "the first frame paints where the declared offset puts the card"
        );
        assert_eq!(
            target.pixels.get(30, 20),
            empty,
            "the box it will land on stays clear: the offset is paint, not geometry"
        );
        let card = lp.tree.find_by_id(Id::str("card")).expect("the card");
        assert_eq!(
            lp.tree.get(card).expect("the card").rect.origin.x,
            0.0,
            "layout ignored the offset"
        );

        let painted = ui_core::geom::Point::new(70.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: painted });
        lp.frame_with_dt(&mut target, Some(painted), 0.0);
        assert_eq!(
            lp.hovered(),
            Some(Id::str("card")),
            "the pointer resting where the card is drawn is over it"
        );

        let laid_out = ui_core::geom::Point::new(30.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: laid_out });
        lp.frame_with_dt(&mut target, Some(laid_out), 0.0);
        assert_ne!(
            lp.hovered(),
            Some(Id::str("card")),
            "the empty box it will land on is not it"
        );

        lp.frame_with_dt(&mut target, None, 0.2);
        assert_eq!(
            target.pixels.get(30, 20),
            [220, 60, 60, 255],
            "the leg ends on the laid-out box"
        );
    }

    #[test]
    fn an_entering_node_scales_about_its_centre_and_hits_where_it_is_painted() {
        // A declared scale wraps the node about its own centre, so the box it will land on
        // stays the middle of what is drawn; hits follow that smaller box, not the layout one.
        let mut target = OffscreenTarget::new(120, 120);
        let mut lp = AppLoop::new(Appearing {
            shown: false,
            enter: Some(
                ui_core::tree::Presence::fade(
                    ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear),
                    1.0,
                )
                .scaled(0.5),
            ),
            exit: None,
        });
        lp.frame_with_dt(&mut target, None, 0.0);
        let empty = target.pixels.get(5, 5);

        lp.app_mut().shown = true;
        lp.frame_with_dt(&mut target, None, 0.0);
        // Half size about (30, 20) covers 15..45 by 10..30.
        assert_eq!(
            target.pixels.get(40, 25),
            [220, 60, 60, 255],
            "the half-size card still covers the middle of its box"
        );
        assert_eq!(
            target.pixels.get(5, 5),
            empty,
            "and it no longer reaches the corner: the scale is about the centre"
        );

        let painted = ui_core::geom::Point::new(40.0, 25.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: painted });
        lp.frame_with_dt(&mut target, Some(painted), 0.0);
        assert_eq!(
            lp.hovered(),
            Some(Id::str("card")),
            "the pointer over the drawn card is over it"
        );

        let outside = ui_core::geom::Point::new(5.0, 5.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: outside });
        lp.frame_with_dt(&mut target, Some(outside), 0.0);
        assert_ne!(
            lp.hovered(),
            Some(Id::str("card")),
            "the corner it no longer covers is not it"
        );

        lp.frame_with_dt(&mut target, None, 0.1);
        assert_eq!(
            target.pixels.get(5, 5),
            [220, 60, 60, 255],
            "the leg ends on the whole laid-out box"
        );
    }

    /// A declared scale is a paint, not a size: the node keeps its laid-out box, is drawn
    /// from its own top-left at the larger size, and is hit where it is drawn.
    #[test]
    fn a_declared_scale_paints_and_hits_from_the_node_origin() {
        struct Scaled;
        impl App for Scaled {
            fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                Box::new(
                    crate::Container::new(Id::str("host"), NodeKind::Stack).child(Box::new(
                        crate::Container::new(Id::str("card"), NodeKind::Box)
                            .size(SizeClass::fixed(60.0, 40.0))
                            .offset(20.0, 20.0)
                            .scale(2.0)
                            .background(Color::from_rgba8(220, 60, 60, 255), Corners::ZERO)
                            .sense(ui_core::Sense::HOVER),
                    )),
                )
            }
        }
        let mut target = OffscreenTarget::new(180, 180);
        let mut lp = AppLoop::new(Scaled);
        lp.frame_with_dt(&mut target, None, 0.0);
        let card_colour = [220, 60, 60, 255];
        assert_eq!(
            target.pixels.get(130, 90),
            card_colour,
            "a 60x40 box at (20,20) scaled twice reaches (140,100)"
        );
        assert_ne!(
            target.pixels.get(10, 10),
            card_colour,
            "and nothing paints before its own top-left"
        );
        let card = lp
            .tree
            .get(lp.tree.find_by_id(Id::str("card")).expect("the card"))
            .expect("the card");
        assert_eq!(
            (card.rect.size.w, card.rect.size.h),
            (60.0, 40.0),
            "layout keeps the size it was given"
        );

        let over = ui_core::geom::Point::new(130.0, 90.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: over });
        lp.frame_with_dt(&mut target, Some(over), 0.0);
        assert_eq!(
            lp.hovered(),
            Some(Id::str("card")),
            "the pointer over the drawn card is over it"
        );
        let past = ui_core::geom::Point::new(150.0, 110.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: past });
        lp.frame_with_dt(&mut target, Some(past), 0.0);
        assert_ne!(
            lp.hovered(),
            Some(Id::str("card")),
            "the empty corner the scaled box no longer reaches is not it"
        );
    }

    #[test]
    fn a_scale_on_its_own_clock_outlives_the_fade_that_rides_the_presence() {
        // WinUI's dialog fades in 83 ms and scales over 250; one shared value would land
        // the scale with the fade, which is the difference between opening and appearing.
        let mut target = OffscreenTarget::new(120, 120);
        let mut lp = AppLoop::new(Appearing {
            shown: false,
            enter: Some(
                ui_core::tree::Presence::fade(
                    ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear),
                    0.0,
                )
                .scaled_over(2.0, ui_core::AnimSpec::new(0.3, ui_core::Easing::Linear)),
            ),
            exit: None,
        });
        lp.frame_with_dt(&mut target, None, 0.0);
        lp.app_mut().shown = true;
        lp.frame_with_dt(&mut target, None, 0.0);
        lp.frame_with_dt(&mut target, None, 0.15);
        let card = lp
            .tree
            .get(lp.tree.find_by_id(Id::str("card")).expect("the card"))
            .expect("the card");
        assert_eq!(
            card.presence_alpha, 1.0,
            "the 100 ms fade has landed by 150 ms"
        );
        assert!(
            card.presence_scale > 1.0 && card.presence_scale < 2.0,
            "and the 300 ms scale is still on its way, got {}",
            card.presence_scale
        );
    }

    /// The dialogs' scrim is translucent and its surface scales: MD3's exact shape.
    struct Md3Page {
        show: bool,
    }

    impl App for Md3Page {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let enter = ui_core::tree::Presence::fade(
                ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear),
                0.0,
            );
            Box::new(
                crate::Container::new(Id::str("page"), NodeKind::Stack)
                    .background(Color::from_rgba8(0, 0, 255, 255), Corners::ZERO)
                    .child_if(self.show, || {
                        Box::new(
                            crate::Container::new(Id::str("scrim"), NodeKind::Stack)
                                .map(|d| d.modal(ui_core::tree::ModalTarget::Global))
                                .background(Color::from_rgba8(0, 0, 0, 138), Corners::ZERO)
                                .enter(enter)
                                .exit(enter)
                                .child(Box::new(
                                    crate::Container::new(Id::str("surface"), NodeKind::Box)
                                        .size(SizeClass::fixed(20.0, 20.0))
                                        .background(Color::WHITE, Corners::ZERO)
                                        .enter(enter.scaled(0.8))
                                        .exit(enter),
                                )),
                        )
                    }),
            )
        }
    }

    #[test]
    fn an_exit_starts_from_the_pixels_its_subtree_rested_on() {
        // A held root left in its parent's paint list is drawn twice — once through the
        // parent, once from `held()` — and a translucent scrim doubles its own alpha.
        let mut target = OffscreenTarget::new(120, 120);
        let mut lp = AppLoop::new(Md3Page { show: false });
        lp.frame(&mut target, None);
        lp.app_mut().show = true;
        lp.frame_with_dt(&mut target, None, 0.0);
        lp.frame_with_dt(&mut target, None, 0.06);
        lp.frame_with_dt(&mut target, None, 0.06);
        let settled = target.pixels.get(60, 60);
        assert_eq!(
            settled,
            [0, 0, 117, 255],
            "black54 over the page, composited once"
        );

        lp.app_mut().show = false;
        // A zero step freezes the clock, so this frame paints whatever the exit starts from.
        assert!(
            lp.frame_with_dt(&mut target, None, 0.0).animating,
            "the exit is under way"
        );
        assert_eq!(
            target.pixels.get(60, 60),
            settled,
            "the first exit frame must not jump away from the state it rested in"
        );
    }

    #[test]
    fn a_modal_scrim_fades_instead_of_appearing_dark() {
        // The fade rides a group layer: its opacity is a layer property, not a primitive's,
        // so the frame that lands it changes nothing in the list. A layer that disappears
        // between frames has to damage its own bounds, or that frame is never committed and
        // the scrim stays at 0.97 until something unrelated repaints it.
        let mut target = OffscreenTarget::new(120, 120);
        let mut lp = AppLoop::new(Md3Page { show: false });
        lp.frame(&mut target, None);
        let page = target.pixels.get(60, 60);
        assert_eq!(page, [0, 0, 255, 255], "the page before the scrim");

        lp.app_mut().show = true;
        lp.frame_with_dt(&mut target, None, 0.0);
        assert_eq!(
            target.pixels.get(60, 60),
            page,
            "the scrim starts at the opacity it declared"
        );

        lp.frame_with_dt(&mut target, None, 0.06);
        let landing = lp.frame_with_dt(&mut target, None, 0.06);
        assert!(!landing.animating, "the 100 ms fade is over");
        // The scrim's own rect never moved and its contents never changed, so the 20 px
        // surface inside it is all the damage a per-primitive pass can find. The far corner
        // is what only the dropped layer accounts for.
        let canvas = ui_core::geom::Rect::new(0.0, 0.0, 120.0, 120.0);
        assert!(
            landing
                .damage
                .rects(canvas)
                .iter()
                .any(|r| r.contains(ui_core::geom::Point::new(100.0, 100.0))),
            "the frame that dropped the layer must damage what the layer covered"
        );
        assert_eq!(
            target.pixels.get(60, 60),
            [0, 0, 117, 255],
            "and it lands on the composited scrim, not the page"
        );
    }

    #[test]
    fn a_leaving_node_keeps_painting_until_its_exit_lands() {
        // The node is out of the declaration and out of layout on the exit's first frame;
        // only the held paint keeps it on screen. Releasing it then, or painting it at the
        // presence it left with, are the two ways this reads as a snap instead of a fade.
        let mut target = OffscreenTarget::new(120, 120);
        let mut lp = AppLoop::new(Appearing {
            shown: true,
            enter: None,
            exit: Some(ui_core::tree::Presence::fade(
                ui_core::AnimSpec::new(0.2, ui_core::Easing::Linear),
                0.0,
            )),
        });
        lp.frame(&mut target, None);
        let opaque = [220, 60, 60, 255];
        let empty = target.pixels.get(100, 100);
        assert_eq!(
            target.pixels.get(30, 20),
            opaque,
            "the card is opaque while it stays"
        );

        lp.app_mut().shown = false;
        // The exit's clock starts on the frame that first sees it, so the fade is measured
        // from the frame after: a zero step opens it without moving it.
        lp.frame_with_dt(&mut target, None, 0.0);
        lp.frame_with_dt(&mut target, None, 0.1);
        let half = target.pixels.get(30, 20);
        assert_ne!(
            half, opaque,
            "half way out the held card has faded, got {half:?}"
        );
        assert_ne!(
            half, empty,
            "and it is still painted: the empty canvas is not what shows, got {half:?}"
        );

        for _ in 0..4 {
            lp.frame_with_dt(&mut target, None, 0.05);
        }
        assert_eq!(
            target.pixels.get(30, 20),
            empty,
            "nothing of it remains once the exit lands"
        );
    }

    /// Three stacked rows whose middle one leaves; the bottom one's top edge is the probe.
    struct Leaving {
        shown: bool,
        space: ui_core::ExitSpace,
    }

    impl App for Leaving {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let row = |id: &'static str, height: f32| {
                crate::Container::new(Id::str(id), NodeKind::Box)
                    .size(SizeClass::fixed(100.0, height))
                    .background(Color::from_rgba8(200, 200, 200, 255), Corners::ZERO)
            };
            let mut host = crate::Container::new(Id::str("host"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 200.0))
                .child(Box::new(row("top", 20.0)));
            if self.shown {
                host = host.child(Box::new(
                    row("mid", 40.0)
                        .exit(ui_core::tree::Presence::fade(
                            ui_core::AnimSpec::new(0.2, ui_core::Easing::Linear),
                            0.0,
                        ))
                        .exit_space(self.space),
                ));
            }
            host = host.child(Box::new(row("bottom", 20.0)));
            Box::new(host)
        }
    }

    fn leaving(space: ui_core::ExitSpace) -> (AppLoop<Leaving>, OffscreenTarget) {
        let mut lp = AppLoop::new(Leaving { shown: true, space });
        let mut target = OffscreenTarget::new(100, 200);
        lp.frame(&mut target, None);
        (lp, target)
    }

    /// The top edge of the bottom row, which moves when the middle row's slot changes.
    fn bottom_top(lp: &AppLoop<Leaving>) -> f32 {
        lp.tree
            .find_by_id(Id::str("bottom"))
            .and_then(|nid| lp.tree.get(nid))
            .map(|node| node.visual_rect().origin.y)
            .expect("the bottom row is laid out")
    }

    #[test]
    fn a_kept_slot_holds_its_space_until_the_exit_lands() {
        let (mut lp, mut target) = leaving(ui_core::ExitSpace::KeepSpace);
        assert_eq!(bottom_top(&lp), 60.0, "the middle row holds 40");
        lp.app_mut().shown = false;
        lp.frame(&mut target, None);
        assert_eq!(
            bottom_top(&lp),
            60.0,
            "a kept slot stays while the row fades out"
        );
        for _ in 0..8 {
            lp.frame_with_dt(&mut target, None, 0.05);
        }
        assert_eq!(
            bottom_top(&lp),
            20.0,
            "and the siblings reflow once the exit lands"
        );
    }

    #[test]
    fn a_collapsing_row_takes_its_siblings_with_it() {
        let (mut lp, mut target) = leaving(ui_core::ExitSpace::Collapse);
        assert_eq!(bottom_top(&lp), 60.0);
        lp.app_mut().shown = false;
        lp.frame(&mut target, None);
        lp.frame_with_dt(&mut target, None, 0.1);
        let half = bottom_top(&lp);
        assert!(
            half > 25.0 && half < 55.0,
            "the slot shrinks with the exit instead of waiting or vanishing, got {half}"
        );
        for _ in 0..8 {
            lp.frame_with_dt(&mut target, None, 0.05);
        }
        assert_eq!(bottom_top(&lp), 20.0, "and it lands at nothing");
    }

    #[test]
    fn tab_walks_the_focus_ring_and_wraps_at_both_ends() {
        // Three painted boxes; Tab visits them in document order and then returns.
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(Demo {
            expand: true,
            draggable: true,
        });
        lp.frame(&mut target, None);
        assert_eq!(lp.focused(), None, "nothing is focused before Tab");

        let tab = || ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        };
        assert!(lp.dispatch(tab()), "first Tab must ask for a repaint");
        assert_eq!(
            lp.focused(),
            Some(Id::str("bar")),
            "starts at the first stop"
        );
        lp.dispatch(tab());
        assert_eq!(lp.focused(), Some(Id::str("extra")));
        lp.dispatch(tab());
        assert_eq!(lp.focused(), Some(Id::str("bar")), "wraps to the start");

        // Shift reverses the walk from wherever focus is.
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..Default::default()
        }));
        lp.dispatch(tab());
        assert_eq!(lp.focused(), Some(Id::str("extra")), "shift goes backwards");
    }

    #[test]
    fn enter_activates_the_focused_node_and_nothing_when_unfocused() {
        // Activation is routed by identity, so it must reach the app with the right one.
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(Counting {
            activated: Vec::new(),
            scrolled: Vec::new(),
            expand: true,
            draggable: false,
        });
        lp.frame(&mut target, None);

        // No focus yet: Enter must not invent one.
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Enter,
        });
        assert!(
            lp.app_mut().activated.is_empty(),
            "nothing focused, nothing activated"
        );

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        });
        assert!(
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Enter,
            }),
            "an accepted activation must ask for a repaint"
        );
        // The control reports it in `change()`, so exactly one delivery comes out of the frame.
        lp.frame(&mut target, None);
        assert_eq!(lp.app_mut().activated, vec![Id::str("bar")]);
        // Space is the other activation key on most toolkits.
        assert!(lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Space,
        }));
        lp.frame(&mut target, None);
        assert_eq!(lp.app_mut().activated, vec![Id::str("bar"), Id::str("bar")]);
    }

    /// A 200×200 scroller over 400 of content: 200 of travel and one optional child that
    /// claims drags itself, so the panning rule can be exercised against it.
    struct Tall {
        drag_child: bool,
    }

    impl App for Tall {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let mut list = Decl::container(Id::str("content"), NodeKind::Column)
                .size(SizeClass::fixed(200.0, 400.0));
            if self.drag_child {
                list = list.child(
                    Decl::container(Id::str("knob"), NodeKind::Box)
                        .size(SizeClass::fixed(200.0, 40.0))
                        .sense(ui_core::Sense::HOVER | ui_core::Sense::DRAG),
                );
            }
            crate::from_decl(
                Decl::container(Id::str("scroller"), NodeKind::Column)
                    .size(SizeClass::fixed(200.0, 200.0))
                    .sense(ui_core::Sense::HOVER | ui_core::Sense::SCROLL_OWNED)
                    .clip(ui_core::layout::Clip {
                        x: ui_core::layout::Overflow::Hidden,
                        y: ui_core::layout::Overflow::Scroll,
                    })
                    .child(list),
            )
        }
    }

    /// The scroller laid out, with a clock the test steps frame by frame.
    fn tall(drag_child: bool, frames: usize) -> (AppLoop<Tall>, OffscreenTarget) {
        let mut lp = AppLoop::new(Tall { drag_child });
        lp.clock = FrameClock::scripted((0..=frames).map(|i| i as f64 / 60.0).collect());
        let mut target = OffscreenTarget::new(200, 200);
        lp.frame(&mut target, None);
        (lp, target)
    }

    fn dragged(lp: &mut AppLoop<Tall>, from: f32, to: f32) {
        let at = |y: f32| ui_core::geom::Point::new(100.0, y);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: at(from),
            button: ui_core::PointerButton::Primary,
        });
        // The first move only crosses the threshold; the pan starts where it crossed.
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: at(from - 6.0),
        });
        lp.dispatch(ui_core::Event::PointerMoved { pos: at(to) });
    }

    fn offset_y(lp: &AppLoop<Tall>) -> f32 {
        lp.scroll_offset(Id::str("scroller"))
            .map(|o| o.y)
            .unwrap_or(0.0)
    }

    #[test]
    fn a_drag_past_the_end_stretches_less_than_the_pointer_travels() {
        // The rubber band: past the top edge the content keeps moving, but damped, so it
        // never runs away however far the pointer goes.
        let (mut lp, mut target) = tall(false, 4);
        // Dragging down pulls the content down past the top edge, where it has no room.
        dragged(&mut lp, 100.0, 180.0);
        lp.frame(&mut target, None);
        let y = offset_y(&lp);
        assert!(y < 0.0, "the content must follow past the edge, got {y}");
        assert!(y > -86.0, "and by less than the 86 travelled, got {y}");
    }

    #[test]
    fn releasing_a_stretched_scroller_springs_back_to_the_edge() {
        // iOS behaviour: let go mid-stretch and the content returns to the boundary.
        let (mut lp, mut target) = tall(false, 90);
        dragged(&mut lp, 100.0, 180.0);
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: ui_core::geom::Point::new(100.0, 180.0),
            button: ui_core::PointerButton::Primary,
        });
        assert!(
            !lp.scroll_motion.is_empty(),
            "a spring still pulling must keep the loop awake"
        );
        for _ in 0..80 {
            lp.frame(&mut target, None);
        }
        assert_eq!(
            offset_y(&lp),
            0.0,
            "the stretch must end exactly on the edge"
        );
        assert!(
            lp.scroll_motion.is_empty(),
            "and the scroller must come to rest"
        );
    }

    #[test]
    fn a_drag_keeps_its_owner_after_the_pointer_leaves_the_control() {
        // Resolving the owner from the pointer loses the gesture as soon as it leaves the
        // control, and every family that gates on `dragging` snaps back.
        struct Watch {
            dragging: Vec<bool>,
        }
        impl App for Watch {
            fn view(&mut self, f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                self.dragging.push(f.dragging_within(Id::str("bar")));
                Demo {
                    expand: false,
                    draggable: true,
                }
                .view(f)
            }
        }

        let mut target = OffscreenTarget::new(200, 120);
        let mut lp = AppLoop::new(Watch {
            dragging: Vec::new(),
        });
        lp.frame(&mut target, None);

        let on_bar = ui_core::geom::Point::new(50.0, 30.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: on_bar,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, Some(on_bar));
        assert!(*lp.app_mut().dragging.last().unwrap(), "pressed");

        // Far outside the bar, still holding the button: the gesture is still the bar's.
        let outside = ui_core::geom::Point::new(190.0, 110.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: outside });
        lp.frame(&mut target, Some(outside));
        assert!(
            *lp.app_mut().dragging.last().unwrap(),
            "a drag outlives the pointer leaving its control"
        );
    }

    #[test]
    fn a_press_is_not_a_drag_until_the_pointer_travels() {
        // `dragging` is armed at the press so a control holds its value; a tap must not read
        // as a drag, or clicking a glass slider sends its thumb to the cursor.
        struct Watch {
            armed: Vec<bool>,
            started: Vec<bool>,
        }
        impl App for Watch {
            fn view(&mut self, f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                self.armed.push(f.dragging_within(Id::str("bar")));
                self.started.push(f.drag_started_within(Id::str("bar")));
                Demo {
                    expand: false,
                    draggable: true,
                }
                .view(f)
            }
        }

        let mut target = OffscreenTarget::new(200, 120);
        let mut lp = AppLoop::new(Watch {
            armed: Vec::new(),
            started: Vec::new(),
        });
        lp.frame(&mut target, None);

        let on_bar = ui_core::geom::Point::new(50.0, 30.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: on_bar,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, Some(on_bar));
        assert!(
            *lp.app_mut().armed.last().unwrap(),
            "the press arms the gesture"
        );
        assert!(
            !(*lp.app_mut().started.last().unwrap()),
            "a press alone is a tap"
        );

        let moved = ui_core::geom::Point::new(on_bar.x + 20.0, on_bar.y);
        lp.dispatch(ui_core::Event::PointerMoved { pos: moved });
        lp.frame(&mut target, Some(moved));
        assert!(
            *lp.app_mut().started.last().unwrap(),
            "travelling past the threshold starts the drag"
        );
    }

    #[test]
    fn escape_cancels_the_drag_and_hands_the_gesture_back() {
        // `Cancelled` tells the app to restore its start state; a `dragging` that outlives
        // the cancel keeps every family gated on it frozen at the abandoned value.
        struct Watch {
            dragging: Vec<bool>,
        }
        impl App for Watch {
            fn view(&mut self, f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                self.dragging.push(f.dragging_within(Id::str("bar")));
                Demo {
                    expand: false,
                    draggable: true,
                }
                .view(f)
            }
        }

        let mut target = OffscreenTarget::new(200, 120);
        let mut lp = AppLoop::new(Watch {
            dragging: Vec::new(),
        });
        lp.frame(&mut target, None);

        let on_bar = ui_core::geom::Point::new(50.0, 30.0);
        let moved = ui_core::geom::Point::new(90.0, 30.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: on_bar,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved { pos: moved });
        lp.frame(&mut target, Some(moved));
        assert!(*lp.app_mut().dragging.last().unwrap(), "a live drag");

        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Escape,
        });
        lp.frame(&mut target, Some(moved));
        assert!(
            !(*lp.app_mut().dragging.last().unwrap()),
            "the cancelled gesture must not keep the control"
        );
    }

    #[test]
    fn a_frame_sees_every_drag_step_since_the_last_frame() {
        // Several motion events can land between frames; a control accumulates `drag_step`,
        // so a frame must see their sum, not only the last slice.
        struct Watch {
            steps: Vec<Option<ui_core::geom::Point>>,
        }
        impl App for Watch {
            fn view(&mut self, f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
                self.steps.push(f.drag_step());
                Demo {
                    expand: false,
                    draggable: true,
                }
                .view(f)
            }
        }

        let mut target = OffscreenTarget::new(200, 120);
        let mut lp = AppLoop::new(Watch { steps: Vec::new() });
        lp.frame(&mut target, None);

        lp.dispatch(ui_core::Event::PointerPressed {
            pos: ui_core::geom::Point::new(20.0, 30.0),
            button: ui_core::PointerButton::Primary,
        });
        // The first move past the threshold starts the drag and carries no step.
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(40.0, 30.0),
        });
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(60.0, 30.0),
        });
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(70.0, 30.0),
        });
        lp.frame(&mut target, Some(ui_core::geom::Point::new(70.0, 30.0)));
        assert_eq!(
            *lp.app_mut().steps.last().unwrap(),
            Some(ui_core::geom::Point::new(30.0, 0.0)),
            "40 -> 60 -> 70 is one frame's worth of movement"
        );
    }

    /// A caret that keeps moving is still a blinking caret: the cycle belongs to the
    /// editor, and restarting it on every offset change pins the caret lit while a key
    /// repeats.
    #[test]
    fn a_caret_walking_with_the_keys_keeps_blinking() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Typing {
            text: "abc".to_string(),
            seen,
        });
        lp.clock = FrameClock::scripted(vec![
            0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8,
        ]);
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        let at = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, None);
        let start = lp.editing.expect("the click placed a caret");
        for step in 0..7 {
            let key = if step % 2 == 0 {
                ui_core::Key::Left
            } else {
                ui_core::Key::Right
            };
            lp.dispatch(ui_core::Event::KeyPressed { key });
            lp.frame(&mut target, None);
        }
        assert!(
            lp.editing.is_some_and(|(_, selection)| selection.caret < start.1.caret),
            "the keys walked the caret, so the cycle was under a moving caret"
        );
        // Seven tenths past the first frame that held the caret: the lit half is over.
        assert!(
            !lp.caret_blink_lit(lp.frame_now),
            "a caret that kept moving must still reach its dark half"
        );
    }


    /// A platform panel is placed against the output the host names: the surface a backend
    /// opens for it stands where the tree put it, so the tree is what constrains it.
    #[test]
    fn a_platform_panel_unfolds_inside_the_output() {
        let (mut lp, mut target) = menu_sink();
        lp.set_panel_surfaces(true);
        lp.set_output_bounds(Some(ui_core::geom::Rect::new(0.0, 0.0, 400.0, 200.0)));
        lp.frame(&mut target, None);
        // The anchor sits at the output's bottom, so below it there is no room.
        assert!(lp.open_menu_at(
            Id::str("row"),
            ui_core::geom::Point::new(20.0, 180.0),
            vec![
                ui_core::menu::MenuItem::new(Id::str("copy"), "Copy"),
                ui_core::menu::MenuItem::new(Id::str("paste"), "Paste"),
            ],
        ));
        lp.frame_with_dt(&mut target, None, 0.2);
        let panel = lp
            .platform_panels()
            .first()
            .map(|panel| panel.rect)
            .expect("the menu is a platform panel");
        assert!(
            (panel.max_y() - 180.0).abs() < 0.5,
            "the panel unfolds above the anchor rather than past the output"
        );
    }
}

mod layer_emission_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;
    use ui_core::geom::{Color, Insets as InsetsT};
    use ui_core::layout::{AxisSize, Grow, SizeClass};

    fn white_box(id: &'static str) -> Decl {
        Decl {
            id: Id::str(id),
            kind: NodeKind::Box,
            shapes: vec![ui_core::draw::Shape::Box {
                corners: ui_core::geom::Corners::ZERO,
                fill: ui_core::geom::Color::WHITE,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }],
            data: NodeData::Container(ui_core::tree::ContainerParams::default()),
            size_class: SizeClass {
                width: AxisSize::fixed(30.0),
                height: AxisSize::fixed(30.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            },
            ink_margin: InsetsT::ZERO,
            box_model: Default::default(),
            offset: ui_core::geom::Point::new(0.0, 0.0),
            cursor: None,
            sense: ui_core::Sense::NONE,
            disabled: false,
            grid: ui_core::GridItem::default(),
            placement: ui_core::layout::Placement::default(),
            env: None,
            drag: None,
            drag_out: false,
            drop_types: None,
            children: Vec::new(),
        }
    }

    struct Canvas {
        frost: bool,
    }

    impl App for Canvas {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let mut canvas = Decl::container(Id::str("canvas"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(100.0, 100.0))
                .offscreen()
                .child(white_box("inside"));
            if self.frost {
                canvas = canvas.child(
                    Decl::container(Id::str("frost"), NodeKind::Box)
                        .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                        .backdrop_blur(4.0),
                );
            }
            crate::from_decl(
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .child(canvas),
            )
        }
    }

    #[test]
    fn an_offscreen_subtree_reaches_the_renderer_as_one_cached_layer() {
        // The key is what lets the target outlive the frame; a layer without one is rebuilt
        // every frame, so caching it by bounds alone would freeze on the first stale subtree.
        let mut target = OffscreenTarget::new(200, 200);
        let mut lp = AppLoop::new(Canvas { frost: false });
        lp.frame(&mut target, None);
        let layers = &lp.draw_list().layers;
        assert_eq!(layers.len(), 1, "the canvas is the only layer");
        assert_eq!(layers[0].cache_key, Some(Id::str("canvas").as_u64()));
    }

    /// A group holding a child that has not finished entering.
    struct Entering;

    impl App for Entering {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            use ui_core::tree::Presence;
            let spec = ui_core::anim::AnimSpec::new(200.0, ui_core::anim::Easing::Linear);
            let decl = {
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(60.0, 60.0))
                    .child(
                        Decl::container(Id::str("group"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .opacity(0.5)
                            .child(white_box("child").enter(Presence::fade(spec, 1.0).scaled(2.0))),
                    )
            };
            crate::from_decl(decl)
        }
    }

    #[test]
    fn a_group_layer_covers_a_scaled_child() {
        // The child starts the frame at twice its size, so it paints from -15 to +45 while
        // the group's box is 40 wide: bounds taken from the layout rects cut its ring off.
        let mut target = OffscreenTarget::new(60, 60);
        let mut lp = AppLoop::new(Entering);
        lp.frame(&mut target, None);
        let bounds = lp.draw_list().layers[0].bounds;
        assert!(
            bounds.max_x() > 40.0 && bounds.min_x() < 0.0,
            "the layer must cover the scaled child, got {bounds:?}"
        );
    }

    /// A group holding a child that blurs on its own.
    struct Blurring;

    impl App for Blurring {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let decl = {
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(60.0, 60.0))
                    .child(
                        Decl::container(Id::str("group"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .opacity(0.5)
                            .child(
                                white_box("child")
                                    .blur(6, 2.0)
                                    .size(ui_core::layout::SizeClass::fixed(40.0, 40.0)),
                            ),
                    )
            };
            crate::from_decl(decl)
        }
    }

    #[test]
    fn a_group_layer_covers_a_child_layers_reach() {
        // The child is its own layer and blurs 6 px past its box; the group's layer has to
        // hold that tail or the outer layer crops it.
        let mut target = OffscreenTarget::new(60, 60);
        let mut lp = AppLoop::new(Blurring);
        lp.frame(&mut target, None);
        let layers = &lp.draw_list().layers;
        assert_eq!(layers.len(), 2, "the group and the blurred child");
        // Layers are pushed outermost first: the group's own bounds are the ones that clip.
        let group = layers[0].bounds;
        assert!(
            group.max_x() >= 46.0,
            "the group layer must reach the child's blur, got {group:?}"
        );
    }

    #[test]
    fn a_kernel_layer_is_cached_only_when_its_area_pays_for_the_copy() {
        // The same blurred child, in a frame it nearly fills and one it barely covers: a
        // cached copy replaces a recompute only while the layer outweighs the full-frame copy.
        let mut small = OffscreenTarget::new(60, 60);
        let mut lp = AppLoop::new(Blurring);
        lp.frame(&mut small, None);
        assert!(
            lp.draw_list().layers.iter().any(|l| l.cache_key.is_some()),
            "a blurred layer covering most of the frame must be cached: {:?}",
            lp.draw_list().layers
        );
        let mut large = OffscreenTarget::new(600, 600);
        let mut lp = AppLoop::new(Blurring);
        lp.frame(&mut large, None);
        assert!(
            lp.draw_list().layers.iter().all(|l| l.cache_key.is_none()),
            "a small blurred layer is cheaper to recompute than to copy"
        );
    }

    /// A focusable child whose ring reaches 3 px outside it.
    struct Ringed;

    impl App for Ringed {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let ring = ui_core::draw::FocusVisual {
                outer: Color::WHITE,
                outer_width: 2.0,
                inner: Color::TRANSPARENT,
                inner_width: 0.0,
                margin: InsetsT::all(-3.0),
                radius: 0.0,
            };
            let decl = {
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(60.0, 60.0))
                    .child(
                        Decl::container(Id::str("group"), NodeKind::Column)
                            .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                            .opacity(0.5)
                            .child(
                                white_box("child")
                                    .size(ui_core::layout::SizeClass::fixed(40.0, 40.0))
                                    .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE)
                                    .focus_visual(Some(ring)),
                            ),
                    )
            };
            crate::from_decl(decl)
        }
    }

    #[test]
    fn a_group_layer_covers_the_focus_ring() {
        // The ring is drawn 3 px outside the control. A group that stops at its own box
        // cuts the ring on the side that touches the edge.
        let mut target = OffscreenTarget::new(60, 60);
        let mut lp = AppLoop::new(Ringed);
        lp.frame(&mut target, None);
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        });
        lp.frame(&mut target, None);
        let group = lp.draw_list().layers[0].bounds;
        assert!(
            group.min_x() <= -3.0 && group.max_x() >= 43.0,
            "the ring must sit inside the layer, got {group:?}"
        );
    }

}

mod sense_subscription_tests {
    use crate::OffscreenTarget;
    use crate::view::*;
    use ui_core::geom::Color;
    use ui_core::layout::SizeClass;
    use ui_core::{Decl, Id, NodeData, NodeKind, Sense};

    /// Records what the engine reports, so each subscription bit can be asserted on.
    #[derive(Default)]
    struct Recorder {
        activated: Vec<Id>,
        scrolled: Vec<(Id, f32)>,
    }

    struct One {
        decl: Decl,
        rec: std::rc::Rc<std::cell::RefCell<Recorder>>,
    }

    impl App for One {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(self.decl.clone())
        }
        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            let mut rec = self.rec.borrow_mut();
            match change {
                ui_core::change::Change::Activated => rec.activated.push(id),
                ui_core::change::Change::Value(v) => rec.scrolled.push((id, v as f32)),
                _ => {}
            }
            true
        }
    }

    fn node(id: Id, sense: Sense, x: f32, y: f32, w: f32, h: f32) -> Decl {
        Decl {
            id,
            kind: NodeKind::Box,
            shapes: vec![ui_core::draw::Shape::Box {
                corners: ui_core::geom::Corners::ZERO,
                fill: ui_core::geom::Color::WHITE,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }],
            data: NodeData::Container(ui_core::tree::ContainerParams::default()),
            size_class: SizeClass::fixed(w, h),
            ink_margin: Insets::ZERO,
            box_model: Default::default(),
            offset: ui_core::geom::Point::new(x, y),
            cursor: None,
            sense,
            disabled: false,
            grid: ui_core::GridItem::default(),
            placement: ui_core::layout::Placement::default(),
            env: None,
            drag: None,
            drag_out: false,
            drop_types: None,
            children: Vec::new(),
        }
    }

    fn run(decl: Decl) -> (AppLoop<One>, std::rc::Rc<std::cell::RefCell<Recorder>>) {
        let rec = std::rc::Rc::new(std::cell::RefCell::new(Recorder::default()));
        let app = One {
            decl,
            rec: rec.clone(),
        };
        (AppLoop::new(app), rec)
    }

    /// A control that reports its own click, so the engine's job stays routing only.
    struct ClickReporter {
        sense: Sense,
    }

    impl crate::Element for ClickReporter {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            node(Id::str("go"), self.sense, 0.0, 0.0, 80.0, 40.0)
        }

        fn change(
            &self,
            state: &crate::WidgetState,
            _bounds: Rect,
        ) -> Option<ui_core::change::Change> {
            (state.clicked && !state.disabled).then_some(ui_core::change::Change::Activated)
        }
    }

    struct Clicks {
        sense: Sense,
        disabled: bool,
        rec: std::rc::Rc<std::cell::RefCell<Recorder>>,
    }

    impl App for Clicks {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Box::new(
                crate::Container::new(Id::str("root"), NodeKind::Column)
                    .disabled(self.disabled)
                    .child(Box::new(ClickReporter { sense: self.sense })),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Activated = change {
                self.rec.borrow_mut().activated.push(id);
            }
            true
        }
    }

    /// A wheel belongs to whoever asked to scroll. A control that subscribed to `SCROLL`
    /// owns its offset and gets the delta; page chrome that asked for nothing is not a
    /// scroll target, and a wheel over it is not a semantic change at all.
    #[test]
    fn a_wheel_reports_a_value_only_to_the_node_that_asked_for_it() {
        let at = ui_core::geom::Point::new(100.0, 100.0);
        let wheel = || ui_core::Event::Scroll {
            pos: at,
            delta: ui_core::geom::Point::new(0.0, 30.0),
        };
        let mut target = OffscreenTarget::new(200, 200);

        let (mut lp, rec) = run(node(
            Id::str("strip"),
            Sense::HOVER | Sense::SCROLL,
            0.0,
            0.0,
            200.0,
            200.0,
        ));
        lp.frame(&mut target, None);
        assert!(lp.dispatch(wheel()), "a consumed wheel asks for a frame");
        assert_eq!(
            rec.borrow().scrolled,
            vec![(Id::str("strip"), -30.0)],
            "the app owns the offset, and the value is the change it applies to it"
        );

        let (mut lp, rec) = run(node(Id::str("page"), Sense::HOVER, 0.0, 0.0, 200.0, 200.0));
        lp.frame(&mut target, None);
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        lp.frame(&mut target, None);
        // Hover has settled and the first frames owe their own follow-ups; the wheel is
        // what is being judged.
        lp.take_dispatched();
        let repaint = lp.dispatch(wheel());
        assert!(
            rec.borrow().scrolled.is_empty(),
            "a node that asked for nothing must not be told the wheel moved"
        );
        assert!(
            !repaint,
            "and nothing consumed it, so the dispatch asks for nothing"
        );
    }

    /// A change handled during a frame lands after `view()` ran, so that frame paints the
    /// state before it. Without a follow-up frame the effect waits for an unrelated event.
    #[test]
    fn a_change_applied_mid_frame_asks_for_the_frame_that_shows_it() {
        let mut target = OffscreenTarget::new(200, 120);
        let at = ui_core::geom::Point::new(20.0, 20.0);
        let rec = std::rc::Rc::new(std::cell::RefCell::new(Recorder::default()));
        let mut lp = AppLoop::new(Clicks {
            sense: Sense::HOVER | Sense::CLICK,
            disabled: false,
            rec: rec.clone(),
        });
        lp.frame(&mut target, Some(at));
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        lp.dispatch(ui_core::Event::PointerPressed {
            button: ui_core::PointerButton::Primary,
            pos: at,
        });
        lp.dispatch(ui_core::Event::PointerReleased {
            button: ui_core::PointerButton::Primary,
            pos: at,
        });
        // The clicks' own request: the change this frame applies has to earn its own.
        assert!(lp.take_dispatched(), "a click asks for a frame");

        lp.frame(&mut target, Some(at));
        assert_eq!(
            rec.borrow().activated,
            vec![Id::str("go")],
            "the click landed"
        );
        assert!(
            lp.take_dispatched(),
            "the frame that applies the change must ask for the one that shows it"
        );

        // A frame that applies nothing asks for nothing, or the loop never goes idle.
        lp.frame(&mut target, Some(at));
        assert!(
            !lp.take_dispatched(),
            "a quiet frame must not ask for another"
        );
    }

    const INK: Id = Id::str("ink");

    /// The clip entry in force where the splat is painted, rect and curve together.
    fn splat_clip_entry(lp: &AppLoop<One>) -> Option<ui_core::draw::ClipEntry> {
        let list = &lp.resolved;
        let mut clips = Vec::new();
        for op in &list.ops {
            match op {
                ui_core::draw::Op::PushClip(i) => clips.push(*list.clips.get(*i as usize)?),
                ui_core::draw::Op::PopClip => {
                    clips.pop()?;
                }
                ui_core::draw::Op::Prim(prim) if prim.kind == PrimKind::Rect => {
                    let p = &list.prims.rects[prim.index as usize];
                    let black = p.fill.r == 0.0 && p.fill.g == 0.0 && p.fill.b == 0.0;
                    // The node's own box is white, so anything dark here is the splat.
                    if black && lp.tree.get(list.owners.rects[prim.index as usize])?.id == INK {
                        return clips.last().copied();
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// A parent node whose ink is clipped to a named indicator child: the navigation
    /// bar's destination, where the capsule behind the icon is the effective background.
    fn ink_clipped_to_indicator() -> Decl {
        let indicator = Id::str("indicator");
        let parent = Decl::container(INK, NodeKind::Column)
            .size(SizeClass::fixed(100.0, 40.0))
            .sense(Sense::CLICK)
            .press_ink(Some(ui_core::PressInk::with_target(
                Color::from_rgba8(0, 0, 0, 64),
                ui_core::InkRadius::Cover,
                ui_core::InkTarget::Descendant(indicator),
            )))
            .child(
                Decl::container(indicator, NodeKind::Box)
                    .size(SizeClass::fixed(20.0, 32.0))
                    // Left of centre: the indicator is the effective background, so the
                    // splash must settle on its centre, not the parent's.
                    .offset(30.0, 4.0),
            );
        Decl::container(Id::str("root"), NodeKind::Column)
            .size(SizeClass::fixed(200.0, 60.0))
            .child(parent)
    }

    #[test]
    fn the_ink_clips_to_its_effective_background_instead_of_the_whole_node() {
        // A navigation destination's ripple covers the indicator capsule, not the whole
        // item: the clip is the child the control named, resolved per frame.
        let (mut lp, _rec) = run(ink_clipped_to_indicator());
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        press(&mut lp, ui_core::geom::Point::new(80.0, 20.0));
        lp.build_with_dt(
            (200, 120),
            Some(ui_core::geom::Point::new(80.0, 20.0)),
            0.30,
        );
        let entry = splat_clip_entry(&lp).expect("the ink is clipped");
        assert_eq!(
            (
                entry.rect.origin.x,
                entry.rect.origin.y,
                entry.rect.size.w,
                entry.rect.size.h
            ),
            (30.0, 4.0, 20.0, 32.0),
            "the clip follows the indicator, not the 100x40 parent: {:?}",
            entry.rect
        );
    }

    #[test]
    fn a_modal_owns_the_keyboard_so_tab_cannot_walk_out_of_it() {
        // A scrim that only blocks the pointer leaves Tab and Enter reaching the controls it
        // covers, which makes the modal a picture rather than a mode.
        let root = Decl::container(Id::str("root"), NodeKind::Column)
            .size(SizeClass::fixed(200.0, 200.0))
            .child(node(
                Id::str("behind"),
                Sense::FOCUSABLE,
                0.0,
                0.0,
                40.0,
                40.0,
            ))
            .child(
                Decl::container(Id::str("modal"), NodeKind::Box)
                    .size(SizeClass::fixed(200.0, 200.0))
                    .modal(ui_core::ModalTarget::Global)
                    .child(node(
                        Id::str("inside"),
                        Sense::FOCUSABLE,
                        0.0,
                        0.0,
                        40.0,
                        40.0,
                    ))
                    .child(node(
                        Id::str("also-inside"),
                        Sense::FOCUSABLE,
                        0.0,
                        0.0,
                        40.0,
                        40.0,
                    )),
            );
        let (mut lp, _rec) = run(root);
        let mut target = OffscreenTarget::new(200, 200);
        lp.frame(&mut target, None);
        let tab = || ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        };
        // The modal takes focus as it opens, so the ring is already inside before any Tab.
        assert_eq!(
            lp.focused(),
            Some(Id::str("inside")),
            "the modal owns the keyboard"
        );
        lp.dispatch(tab());
        assert_eq!(
            lp.focused(),
            Some(Id::str("also-inside")),
            "Tab moves within it"
        );
        lp.dispatch(tab());
        assert_eq!(
            lp.focused(),
            Some(Id::str("inside")),
            "and wraps inside it, never out"
        );
        lp.dispatch(tab());
        assert_eq!(
            lp.focused(),
            Some(Id::str("also-inside")),
            "never reaching what it covers"
        );
    }

    fn press(lp: &mut AppLoop<One>, at: ui_core::geom::Point) {
        lp.dispatch(ui_core::Event::PointerPressed {
            button: ui_core::PointerButton::Primary,
            pos: at,
        });
    }
}

mod decl_cache_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;
    use ui_core::geom::Color;
    use ui_core::layout::{AxisSize, Grow, SizeClass};

    /// Counts builds and depends on its own hover and an animation it drives.
    struct Tally {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
        /// App state the engine cannot see: the fingerprint carries it into the cache key.
        tint: Color,
    }

    /// A colour's bits, the way a control fingerprints its own inputs.
    fn tint_fp(c: Color) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()].hash(&mut h);
        h.finish()
    }

    impl crate::Element for Tally {
        fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            self.builds.set(self.builds.get() + 1);
            let id = Id::str("tally");
            let hovered = cx.hovered_within(id);
            let a = cx.animate(
                Id::str("tally/hover"),
                if hovered { 1.0 } else { 0.0 },
                ui_core::AnimSpec::new(0.1, ui_core::Easing::Linear),
            );
            Decl::container(id, NodeKind::Box)
                .size(SizeClass::fixed(80.0, 40.0))
                .sense(ui_core::Sense::HOVER | ui_core::Sense::CLICK)
                .offset(a * 10.0, 0.0)
                .background(self.tint, ui_core::geom::Corners::ZERO)
        }

        fn cache_key(&self) -> Option<ui_core::memo::CachedKey> {
            Some(ui_core::memo::CachedKey::new(
                Id::str("tally"),
                tint_fp(self.tint),
            ))
        }

        fn change(
            &self,
            state: &crate::state::WidgetState,
            _bounds: Rect,
        ) -> Option<ui_core::Change> {
            state.clicked.then_some(ui_core::change::Change::Activated)
        }
    }

    struct Cached {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
        tint: Color,
    }

    /// App state with no engine fact behind it: only a reported change can invalidate it.
    struct Swatch {
        tint: Color,
    }

    impl crate::Element for Swatch {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            Decl::container(Id::str("swatch"), NodeKind::Box)
                .size(SizeClass::fixed(40.0, 40.0))
                .background(self.tint, ui_core::geom::Corners::ZERO)
        }

        fn cache_key(&self) -> Option<ui_core::memo::CachedKey> {
            Some(ui_core::memo::CachedKey::new(
                Id::str("swatch"),
                tint_fp(self.tint),
            ))
        }
    }

    /// A bare click target; the app reads nothing from it but the report.
    struct Knob;

    impl crate::Element for Knob {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            Decl::container(Id::str("knob"), NodeKind::Box)
                .size(SizeClass::fixed(40.0, 40.0))
                .sense(ui_core::Sense::CLICK)
        }

        fn cache_key(&self) -> Option<ui_core::memo::CachedKey> {
            // No inputs at all: a constant fingerprint is the whole truth.
            Some(ui_core::memo::CachedKey::new(Id::str("knob"), 0))
        }

        fn change(
            &self,
            state: &crate::state::WidgetState,
            _bounds: Rect,
        ) -> Option<ui_core::Change> {
            state.clicked.then_some(ui_core::change::Change::Activated)
        }
    }

    impl App for Cached {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Box::new(
                crate::Container::new(Id::str("root"), NodeKind::Column)
                    .child(Box::new(Tally {
                        builds: std::rc::Rc::clone(&self.builds),
                        tint: self.tint,
                    }))
                    .child(Box::new(Swatch { tint: self.tint }))
                    .child(Box::new(Knob)),
            )
        }

        fn changed(&mut self, _id: Id, _change: ui_core::change::Change) -> bool {
            self.tint = if self.tint == Color::from_rgba8(200, 60, 60, 255) {
                Color::from_rgba8(60, 60, 200, 255)
            } else {
                Color::from_rgba8(200, 60, 60, 255)
            };
            true
        }
    }

    fn cached_loop() -> (AppLoop<Cached>, std::rc::Rc<std::cell::Cell<usize>>) {
        let builds = std::rc::Rc::new(std::cell::Cell::new(0));
        (
            AppLoop::new(Cached {
                builds: std::rc::Rc::clone(&builds),
                tint: Color::from_rgba8(200, 60, 60, 255),
            }),
            builds,
        )
    }

    #[test]
    fn a_cached_element_is_not_rebuilt_while_its_facts_hold() {
        let (mut lp, builds) = cached_loop();
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        assert_eq!(builds.get(), 1, "the first frame builds it once");
        lp.frame(&mut target, None);
        assert_eq!(builds.get(), 1, "nothing the declaration read has moved");
    }

    /// Builds its whole subtree as declarations: no child is an element, so no child has an
    /// entry of its own, and the entry's own inputs are the only proof of what is inside.
    struct Stack {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl crate::Element for Stack {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            self.builds.set(self.builds.get() + 1);
            let mut decl = Decl::container(Id::str("stack"), NodeKind::Column);
            for at in 0..3 {
                decl = decl.child(
                    Decl::container(Id::of(("stack/cell", at)), NodeKind::Box)
                        .size(SizeClass::fixed(40.0, 20.0))
                        .background(Color::BLACK, ui_core::geom::Corners::ZERO),
                );
            }
            decl
        }

        fn cache_key(&self) -> Option<ui_core::memo::CachedKey> {
            Some(ui_core::memo::CachedKey::new(Id::str("stack"), 0))
        }
    }

    struct Built {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl App for Built {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            // The root is built directly, so the element under test has to be a child.
            Box::new(
                crate::Container::new(Id::str("built/root"), NodeKind::Column).child(Box::new(
                    Stack {
                        builds: std::rc::Rc::clone(&self.builds),
                    },
                )),
            )
        }
    }

    /// A cached element whose children are declarations, not elements, is still reused: the
    /// children cannot have entries, and their inputs are the entry's own fingerprint.
    #[test]
    fn an_element_that_builds_its_own_children_is_reused() {
        let builds = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut lp = AppLoop::new(Built {
            builds: std::rc::Rc::clone(&builds),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        assert_eq!(builds.get(), 1, "the first frame builds it once");
        lp.frame(&mut target, None);
        assert_eq!(builds.get(), 1, "its children are not elements to validate");
    }

    /// Declares an input's bytes, which the engine edits in place rather than rebuilding.
    struct Field {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl crate::Element for Field {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            self.builds.set(self.builds.get() + 1);
            Decl::container(Id::str("field"), NodeKind::Box)
                .size(SizeClass::fixed(80.0, 20.0))
                .sense(ui_core::Sense::FOCUSABLE)
                .text_input("seed")
        }

        fn cache_key(&self) -> Option<ui_core::memo::CachedKey> {
            Some(ui_core::memo::CachedKey::new(Id::str("field"), 0))
        }
    }

    struct Typed {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl App for Typed {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Box::new(
                crate::Container::new(Id::str("typed/root"), NodeKind::Column).child(Box::new(
                    Field {
                        builds: std::rc::Rc::clone(&self.builds),
                    },
                )),
            )
        }
    }

    /// A declaration that owns an input's bytes is live state: replaying it would restore
    /// bytes the engine edited in place, so it is rebuilt every frame.
    #[test]
    fn a_declaration_that_owns_an_input_is_never_reused() {
        let builds = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut lp = AppLoop::new(Typed {
            builds: std::rc::Rc::clone(&builds),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        assert_eq!(builds.get(), 2, "the buffer is the engine's to change");
    }

    /// Hands over a finished declaration, as `from_decl` does.
    struct Finished;

    impl crate::Element for Finished {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            Decl::container(Id::str("finished"), NodeKind::Box)
        }

        fn decl_shared(&self) -> Option<std::rc::Rc<Decl>> {
            Some(std::rc::Rc::new(
                Decl::container(Id::str("finished"), NodeKind::Box)
                    .size(SizeClass::fixed(20.0, 20.0)),
            ))
        }
    }

    struct Host {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl crate::Element for Host {
        fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            self.builds.set(self.builds.get() + 1);
            let mut decl = Decl::container(Id::str("host"), NodeKind::Column);
            decl.children = vec![crate::element::element_decl(&Finished, cx)];
            decl
        }

        fn cache_key(&self) -> Option<ui_core::memo::CachedKey> {
            Some(ui_core::memo::CachedKey::new(Id::str("host"), 0))
        }
    }

    struct Hosted {
        builds: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl App for Hosted {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Box::new(
                crate::Container::new(Id::str("hosted/root"), NodeKind::Column).child(Box::new(
                    Host {
                        builds: std::rc::Rc::clone(&self.builds),
                    },
                )),
            )
        }
    }

    /// A child that hands over a finished declaration may differ frame to frame; that is
    /// app input, so its parent cannot be reused either.
    #[test]
    fn a_parent_of_a_finished_declaration_is_rebuilt() {
        let builds = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut lp = AppLoop::new(Hosted {
            builds: std::rc::Rc::clone(&builds),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        assert_eq!(builds.get(), 2, "the child's declaration is not provable");
    }

    #[test]
    fn a_hover_change_rebuilds_the_element_that_read_it() {
        let (mut lp, builds) = cached_loop();
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(40.0, 20.0),
        });
        lp.frame(&mut target, None);
        assert_eq!(builds.get(), 2, "the hovered fact it read changed");
    }

    /// An element handing back the same declaration object every frame, which is what lets
    /// the cache skip the whole subtree.
    struct Shared(std::rc::Rc<Decl>);

    impl crate::Element for Shared {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            (*self.0).clone()
        }

        fn decl_shared(&self) -> Option<std::rc::Rc<Decl>> {
            Some(std::rc::Rc::clone(&self.0))
        }
    }

    /// A card that never changes beside a box that does, so a second layout pass runs over
    /// a subtree the cache skipped.
    struct CachedCard {
        card: std::rc::Rc<Decl>,
        step: f32,
    }

    impl App for CachedCard {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            self.step += 1.0;
            let tick = Decl::container(Id::str("tick"), NodeKind::Box)
                .size(SizeClass::fixed(10.0 + self.step, 10.0));
            Box::new(
                crate::Container::new(Id::str("board"), NodeKind::Stack)
                    .size(SizeClass {
                        width: AxisSize::fill(0.0, 0.0),
                        height: AxisSize::fill(0.0, 0.0),
                        grow_x: Grow::Yes,
                        grow_y: Grow::Yes,
                    })
                    .child(Box::new(Shared(std::rc::Rc::clone(&self.card))))
                    .child(Box::new(crate::from_decl(tick))),
            )
        }
    }

    /// The offset fold starts from the declaration's own displacement, never from the value
    /// it folded into last frame. A skipped subtree keeps last frame's folded offset, so
    /// folding from that compounds it and the card's content drifts away from the card.
    #[test]
    fn a_cached_subtree_under_an_offset_keeps_its_place_when_layout_runs_again() {
        let card = std::rc::Rc::new(
            Decl::container(Id::str("card"), NodeKind::Column)
                .size(SizeClass::fixed(60.0, 60.0))
                .offset(100.0, 50.0)
                .child(
                    Decl::container(Id::str("swatch"), NodeKind::Box)
                        .size(SizeClass::fixed(20.0, 20.0)),
                ),
        );
        let mut lp = AppLoop::new(CachedCard { card, step: 0.0 });
        let mut target = OffscreenTarget::new(200, 200);
        let origin = |lp: &AppLoop<CachedCard>| {
            let nid = lp
                .tree()
                .find_by_id(Id::str("swatch"))
                .expect("the swatch is laid out");
            lp.tree()
                .get(nid)
                .expect("the view exists")
                .visual_rect()
                .origin
        };
        lp.frame(&mut target, None);
        assert_eq!(origin(&lp), ui_core::geom::Point::new(100.0, 50.0));
        lp.frame(&mut target, None);
        assert_eq!(
            origin(&lp),
            ui_core::geom::Point::new(100.0, 50.0),
            "the cached card must not move when a sibling dirties the layout"
        );
    }
}

mod popup_scope_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;
    use ui_core::geom::{Color, Corners};
    use ui_core::layout::SizeClass;

    /// A page with one focusable box and, when open, a panel that asks for the keyboard.
    struct Flyout {
        open: bool,
        dismissed: Rc<RefCell<Vec<Id>>>,
        /// The row the panel should open onto, when it names one.
        opens_onto: Option<Id>,
        /// Whether the panel's rows sit displaced, as during its entrance.
        displaced: bool,
    }

    /// The panel: it asks for the keyboard on its rows' behalf and reports a light dismissal.
    struct FlyoutPanel {
        focus_initial: Option<Id>,
        /// Whether the rows sit displaced, as they do while the panel slides into place.
        displaced: bool,
    }

    impl crate::Element for FlyoutPanel {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            let row = |id: &'static str| {
                Decl::container(Id::str(id), NodeKind::Box)
                    .size(SizeClass::fixed(100.0, 30.0))
                    .background(Color::WHITE, Corners::ZERO)
                    .sense(ui_core::Sense::FOCUSABLE)
            };
            let mut rows = Decl::container(Id::str("rows"), NodeKind::Column)
                .child(row("a"))
                .child(row("b"));
            if self.displaced {
                rows = rows.offset(0.0, -35.0);
            }
            let mut panel = Decl::container(Id::str("panel"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 60.0))
                .background(Color::WHITE, Corners::ZERO)
                .overlay(ui_core::layout::OverlayPlacement::below(
                    ui_core::layout::Anchor::Point(ui_core::geom::Point::new(180.0, 10.0)),
                ))
                .sense(ui_core::Sense::FOCUSABLE)
                .child(rows);
            if let Some(id) = self.focus_initial {
                panel = panel.focus_initial(id);
            }
            panel
        }

        fn change(
            &self,
            state: &crate::WidgetState,
            _bounds: Rect,
        ) -> Option<ui_core::change::Change> {
            state
                .dismissed
                .then_some(ui_core::change::Change::Dismissed)
        }
    }

    impl App for Flyout {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let mut root = crate::Container::new(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(300.0, 300.0))
                .background(Color::WHITE, Corners::ZERO)
                .child_decl(
                    Decl::container(Id::str("page"), NodeKind::Box)
                        .size(SizeClass::fixed(100.0, 40.0))
                        .background(Color::WHITE, Corners::ZERO)
                        .sense(ui_core::Sense::CLICK | ui_core::Sense::FOCUSABLE),
                );
            if self.open {
                root = root.child(Box::new(FlyoutPanel {
                    focus_initial: self.opens_onto,
                    displaced: self.displaced,
                }));
            }
            Box::new(root)
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if matches!(change, ui_core::change::Change::Dismissed) {
                self.dismissed.borrow_mut().push(id);
            }
            true
        }
    }

    fn flyout(open: bool) -> (AppLoop<Flyout>, Rc<RefCell<Vec<Id>>>, OffscreenTarget) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Flyout {
            open,
            dismissed: Rc::clone(&log),
            opens_onto: None,
            displaced: false,
        });
        let mut target = OffscreenTarget::new(300, 300);
        lp.frame(&mut target, None);
        (lp, log, target)
    }

    #[test]
    fn a_press_outside_the_open_panel_asks_that_panel_to_close() {
        // The page behind is a click target too; only the panel hears "you were dismissed".
        let (mut lp, log, mut target) = flyout(true);
        let at = ui_core::geom::Point::new(20.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, None);
        assert_eq!(*log.borrow(), vec![Id::str("panel")]);
    }
}

/// A panel opened from inside another panel: the shape of a menu whose item opens a
/// submenu, or a picker opened from a field that is itself floating.
mod nested_popup_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;
    use ui_core::geom::{Color, Corners, Point};
    use ui_core::layout::{Anchor, OverlayPlacement, SizeClass};

    /// A 100×60 panel of two focusable rows, floating below its anchor.
    struct Panel {
        id: &'static str,
        rows: [&'static str; 2],
        anchor: Anchor,
    }

    fn row(id: &'static str) -> Decl {
        Decl::container(Id::str(id), NodeKind::Box)
            .size(SizeClass::fixed(100.0, 30.0))
            .background(Color::WHITE, Corners::ZERO)
            .sense(ui_core::Sense::FOCUSABLE)
    }

    impl crate::Element for Panel {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            Decl::container(Id::str(self.id), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 60.0))
                .background(Color::WHITE, Corners::ZERO)
                .sense(ui_core::Sense::FOCUSABLE)
                .overlay(OverlayPlacement::below(self.anchor))
                .child(row(self.rows[0]))
                .child(row(self.rows[1]))
        }

        fn change(
            &self,
            state: &crate::WidgetState,
            _bounds: Rect,
        ) -> Option<ui_core::change::Change> {
            state
                .dismissed
                .then_some(ui_core::change::Change::Dismissed)
        }
    }

    struct Nested {
        log: Rc<RefCell<Vec<Id>>>,
        inner_open: bool,
    }

    impl App for Nested {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            // The inner panel hangs off a row of the outer one, as a submenu does.
            let inner = Panel {
                id: "inner",
                rows: ["c", "d"],
                anchor: Anchor::Node(Id::str("b")),
            };
            let outer = Panel {
                id: "outer",
                rows: ["a", "b"],
                anchor: Anchor::Point(Point::new(150.0, 10.0)),
            };
            let mut root = crate::Container::new(Id::str("root"), NodeKind::Column)
                .size(SizeClass::fixed(300.0, 300.0))
                .background(Color::WHITE, Corners::ZERO)
                .child_decl(row("page"))
                .child(Box::new(outer));
            if self.inner_open {
                root = root.child(Box::new(inner));
            }
            Box::new(root)
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if matches!(change, ui_core::change::Change::Dismissed) {
                self.log.borrow_mut().push(id);
                if id == Id::str("inner") {
                    self.inner_open = false;
                }
            }
            true
        }
    }

    fn nested() -> (AppLoop<Nested>, Rc<RefCell<Vec<Id>>>, OffscreenTarget) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Nested {
            log: Rc::clone(&log),
            inner_open: true,
        });
        let mut target = OffscreenTarget::new(300, 300);
        lp.frame(&mut target, None);
        (lp, log, target)
    }

    /// The ids of the boxes painted, in paint order.
    fn paint_order(lp: &AppLoop<Nested>) -> Vec<Id> {
        let list = lp.draw_list();
        let mut order = Vec::new();
        for op in &list.ops {
            let ui_core::draw::Op::Prim(prim) = op else {
                continue;
            };
            if prim.kind != ui_core::draw::PrimKind::Rect {
                continue;
            }
            if let Some(nid) = list.owners.rects.get(prim.index as usize).copied()
                && let Some(node) = lp.tree.get(nid)
            {
                order.push(node.id);
            }
        }
        order
    }

    #[test]
    fn an_inner_panel_paints_above_the_panel_that_opened_it() {
        let (lp, _, _) = nested();
        let order = paint_order(&lp);
        let at = |id: &'static str| order.iter().position(|seen| *seen == Id::str(id));
        let (Some(inner), Some(outer_row)) = (at("inner"), at("b")) else {
            panic!("both panels paint: {order:?}");
        };
        assert!(
            inner > outer_row,
            "the inner panel covers the row it hangs off: {order:?}"
        );
    }

    #[test]
    fn a_press_inside_the_outer_panel_dismisses_only_the_inner_one() {
        let (mut lp, log, mut target) = nested();
        let at = Point::new(160.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: at,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, None);
        assert_eq!(
            *log.borrow(),
            vec![Id::str("inner")],
            "the topmost panel is the one a press outside it closes"
        );
    }

    /// An overlay panel with a sibling declared after it that covers its whole area.
    struct Covered;

    impl App for Covered {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            crate::from_decl(
                Decl::container(Id::str("root"), NodeKind::Stack)
                    .size(SizeClass::fixed(300.0, 300.0))
                    .background(Color::from_rgba8(0, 0, 0, 255), Corners::ZERO)
                    .child(
                        Decl::container(Id::str("panel"), NodeKind::Column)
                            .size(SizeClass::fixed(100.0, 100.0))
                            .background(Color::from_rgba8(0, 255, 0, 255), Corners::ZERO)
                            .overlay(ui_core::layout::OverlayPlacement::below(
                                ui_core::layout::Anchor::Point(Point::new(150.0, 10.0)),
                            )),
                    )
                    .child(
                        Decl::container(Id::str("later"), NodeKind::Box)
                            .size(SizeClass::fixed(300.0, 300.0))
                            .background(Color::from_rgba8(0, 0, 255, 255), Corners::ZERO),
                    ),
            )
        }
    }

    #[test]
    fn an_overlay_paints_over_content_declared_after_it() {
        // Painting follows declaration order unless a node is deferred. An overlay is
        // deferred by the walk, so paint has to apply the same predicate: otherwise a
        // sibling declared after the panel draws straight over it, even though the hit
        // index already gives the panel the pointer.
        let mut lp = AppLoop::new(Covered);
        let mut target = OffscreenTarget::new(300, 300);
        lp.frame(&mut target, None);
        let panel = lp
            .tree()
            .find_by_id(Id::str("panel"))
            .and_then(|nid| lp.tree().get(nid))
            .map(|node| node.rect)
            .expect("the panel");
        let centre = panel.center();
        assert_eq!(
            target.pixels.get(centre.x as u32, centre.y as u32),
            [0, 255, 0, 255],
            "the panel paints above the sibling declared after it"
        );
        assert_eq!(
            target.pixels.get(10, 290),
            [0, 0, 255, 255],
            "and the later sibling still paints where the panel is not"
        );
    }

    /// A panel declared inside another panel: a submenu in the menu that opened it.
    struct Deep;

    impl App for Deep {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let inner = Decl::container(Id::str("inner"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 60.0))
                .background(Color::WHITE, Corners::ZERO)
                .sense(ui_core::Sense::FOCUSABLE)
                .overlay(OverlayPlacement::below(Anchor::Node(Id::str("b"))))
                .child(row("c"))
                .child(row("d"));
            let outer = Decl::container(Id::str("outer"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 120.0))
                .background(Color::WHITE, Corners::ZERO)
                .sense(ui_core::Sense::FOCUSABLE)
                .overlay(OverlayPlacement::below(Anchor::Point(Point::new(
                    150.0, 10.0,
                ))))
                .child(row("a"))
                .child(row("b"))
                .child(inner)
                .child(row("cover"));
            crate::from_decl(
                Decl::container(Id::str("root"), NodeKind::Column)
                    .size(SizeClass::fixed(300.0, 300.0))
                    .background(Color::WHITE, Corners::ZERO)
                    .child(outer),
            )
        }
    }

    #[test]
    fn a_panel_nested_in_another_floats_last_in_the_walk() {
        let mut lp = AppLoop::new(Deep);
        let mut target = OffscreenTarget::new(300, 300);
        lp.frame(&mut target, None);
        let order: Vec<Id> = lp
            .tree()
            .walk()
            .into_iter()
            .filter_map(|nid| lp.tree().get(nid).map(|node| node.id))
            .collect();
        let at = |id: &'static str| order.iter().position(|seen| *seen == Id::str(id));
        let (Some(inner), Some(cover)) = (at("inner"), at("cover")) else {
            panic!("both are laid out: {order:?}");
        };
        assert!(
            inner > cover,
            "an overlay nested in another floats last, as it paints: {order:?}"
        );
    }

    #[test]
    fn dismissing_the_inner_panel_hands_the_keyboard_back_to_the_outer_one() {
        let (mut lp, log, mut target) = nested();
        assert_eq!(
            lp.focused(),
            Some(Id::str("c")),
            "the open inner panel owns the keyboard, not the one that opened it"
        );
        let inside_outer = Point::new(160.0, 20.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos: inside_outer });
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: inside_outer,
            button: ui_core::PointerButton::Primary,
        });
        lp.frame(&mut target, None);
        assert_eq!(
            *log.borrow(),
            vec![Id::str("inner")],
            "the inner one closes"
        );
        // The change reaches the app on the next frame, as every change does.
        lp.frame(&mut target, None);
        assert!(
            lp.tree().find_by_id(Id::str("inner")).is_none(),
            "the app obeyed the dismissal"
        );
        assert_eq!(
            lp.focused(),
            Some(Id::str("a")),
            "focus goes back inside the panel that is still open, not to the page"
        );
    }
}

/// The engine owns an automatic scrollbar's fade: the bar is declared with no state of its
/// own, so the hold and the two fade durations belong to the loop.
mod scrollbar_fade_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;

    /// A 100×100 scroller over 300 of content, with an automatic bar down its right edge.
    /// The thumb's own geometry is the widget's business; this exercises what the engine
    /// owns: whether the bar is showing, and whether a press can take it.
    struct Barred;

    impl App for Barred {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let content = Decl::container(Id::str("content"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(100.0, 300.0));
            let mut decl = Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(100.0, 100.0))
                .sense(ui_core::Sense::HOVER | ui_core::Sense::SCROLL_OWNED)
                .clip(ui_core::layout::Clip {
                    x: ui_core::layout::Overflow::Hidden,
                    y: ui_core::layout::Overflow::Scroll,
                })
                .child(content);
            if let Some(params) = decl.data.params_mut() {
                params.scrollbar = Some(ui_core::Scrollbar {
                    vertical: Some(ui_core::ScrollbarBar {
                        thumb: Rect::new(96.0, 0.0, 4.0, 33.0),
                        track: Rect::new(96.0, 0.0, 4.0, 100.0),
                        track_fill: ui_core::geom::Color::TRANSPARENT,
                        thumb_fill: ui_core::geom::Color::from_rgba8(0, 0, 0, 255),
                        track_corners: ui_core::geom::Corners::ZERO,
                        thumb_corners: ui_core::geom::Corners::ZERO,
                    }),
                    horizontal: None,
                    fade: true,
                });
            }
            crate::from_decl(decl)
        }
    }

    fn bar(lp: &AppLoop<Barred>) -> f32 {
        lp.scrollbar_opacity(Id::str("scroller")).unwrap_or(-1.0)
    }

    /// The scroller laid out, with a clock the test steps frame by frame.
    fn barred(count: usize) -> (AppLoop<Barred>, OffscreenTarget) {
        let mut lp = AppLoop::new(Barred);
        lp.clock = FrameClock::scripted((0..=count).map(|i| i as f64 / 60.0).collect());
        let mut target = OffscreenTarget::new(100, 100);
        lp.frame(&mut target, None);
        (lp, target)
    }

    /// Drive frames with the pointer held where it is, reporting whether any of them was
    /// engine-owned motion. The loop passes the live pointer every frame in production, so a
    /// test that hovered once and then drove frames without it would lose the hover.
    fn step(
        lp: &mut AppLoop<Barred>,
        target: &mut OffscreenTarget,
        count: usize,
        pointer: ui_core::geom::Point,
    ) -> bool {
        let mut animating = false;
        for _ in 0..count {
            animating |= lp.frame(target, Some(pointer)).animating;
        }
        animating
    }

    #[test]
    fn an_auto_scrollbar_holds_after_the_wheel_then_fades_out_and_lets_the_loop_idle() {
        let (mut lp, mut target) = barred(300);
        let away = ui_core::geom::Point::new(500.0, 500.0);
        // A scroller that has existed for a frame but saw nothing is not showing a bar.
        step(&mut lp, &mut target, 1, away);
        assert_eq!(bar(&lp), 0.0, "a bar nobody touched is not shown");
        let pos = ui_core::geom::Point::new(50.0, 50.0);
        lp.dispatch(ui_core::Event::PointerMoved { pos });
        lp.dispatch(ui_core::Event::wheel(pos, 30.0));
        step(&mut lp, &mut target, 12, pos);
        assert_eq!(bar(&lp), 1.0, "the wheel brings it in, at full strength");
        // This stretch is inside the 0.6 s hold with the pointer gone: the bar must not
        // start leaving yet, or a wheel flick would show a bar that is already gone. The
        // hold also has to keep the loop awake, or it would expire unseen and the bar would
        // vanish on whatever frame came next instead of fading.
        let during_hold = step(&mut lp, &mut target, 12, away);
        assert_eq!(bar(&lp), 1.0, "the hold outlasts a short pause");
        assert!(during_hold, "the hold must keep frames coming for the fade");
        // Past the hold the fade runs. Only frames where the bar is between its two ends
        // count: the hold above asks a different question, and an OR across both would pass
        // even if a fading bar stopped asking for frames.
        let mut passed_through = false;
        let mut awake_while_fading = false;
        for _ in 0..40 {
            let out = lp.frame(&mut target, Some(away));
            let value = bar(&lp);
            if value > 0.001 && value < 0.999 {
                passed_through = true;
                awake_while_fading |= out.animating;
            }
        }
        assert!(passed_through, "the bar must fade, not jump");
        assert!(awake_while_fading, "a fading bar is engine-owned motion");
        assert_eq!(bar(&lp), 0.0, "and it must land exactly at nothing");
        assert!(
            !step(&mut lp, &mut target, 1, away),
            "a landed bar must not keep the loop awake"
        );
    }

    #[test]
    fn a_faded_out_bar_does_not_take_the_press() {
        let (mut lp, mut target) = barred(300);
        step(
            &mut lp,
            &mut target,
            1,
            ui_core::geom::Point::new(500.0, 500.0),
        );
        // The scroller is at the top and the pointer is over the content: a press here is a
        // content press either way, so the scroller's offset is what tells them apart. A
        // press on the bar's track jumps the thumb; a press on the content does not.
        let on_bar = ui_core::geom::Point::new(98.0, 50.0);
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: on_bar,
            button: ui_core::PointerButton::Primary,
        });
        step(&mut lp, &mut target, 1, on_bar);
        let offset = lp
            .scroll_offset(Id::str("scroller"))
            .map(|o| o.y)
            .unwrap_or(0.0);
        assert_eq!(offset, 0.0, "an invisible bar must not answer a press");
    }
}

/// A list that declares only the rows its window covers, the way the picker does: the
/// window comes from the offset the scroller reports, and a frame that moves the offset
/// has to declare the rows it paints rather than the rows of where it stood before.
mod windowed_list_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;

    const ROW: f32 = 20.0;

    struct Listed {
        content: f32,
        /// The row the second build scrolls to before choosing its window.
        jump_row: Option<usize>,
        views: usize,
    }

    impl App for Listed {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            self.views += 1;
            let scroller = Id::str("scroller");
            // The picker's reveal: ask first, choose the window from the answer.
            let (offset, viewport) = cx
                .scroll_of(scroller)
                .map_or((0.0, 100.0), |geom| (geom.offset_y, geom.viewport_h));
            if self.views == 2
                && let Some(row) = self.jump_row
            {
                let top = row as f32 * ROW;
                if top < offset || top + ROW > offset + viewport {
                    cx.set_scroll(scroller, 0.0, top);
                }
            }
            let (offset, viewport) = cx
                .scroll_of(scroller)
                .map_or((0.0, 100.0), |geom| (geom.offset_y, geom.viewport_h));
            let count = (self.content / ROW) as usize;
            let first = ((offset / ROW).floor() as usize).min(count);
            let last = (((offset + viewport) / ROW).ceil() as usize).min(count);
            let mut column = Decl::container(Id::str("content"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(100.0, self.content));
            if first > 0 {
                column = column.child(
                    Decl::container(Id::str("above"), NodeKind::Box).size(
                        ui_core::layout::SizeClass::fixed(100.0, first as f32 * ROW),
                    ),
                );
            }
            for row in first..last {
                column = column.child(
                    Decl::container(Id::str("row").with(row), NodeKind::Box)
                        .size(ui_core::layout::SizeClass::fixed(100.0, ROW)),
                );
            }
            let scroller = Decl::container(scroller, NodeKind::Column)
                // Hug, as `Scroll::vertical` does: a short list's viewport is its own
                // height, which is what a frame that receives a longer list still reads.
                .size(ui_core::layout::SizeClass {
                    width: ui_core::layout::AxisSize::fixed(100.0),
                    height: ui_core::layout::AxisSize::hug(0.0, 100.0),
                    grow_x: ui_core::layout::Grow::No,
                    grow_y: ui_core::layout::Grow::No,
                })
                .sense(ui_core::Sense::HOVER | ui_core::Sense::SCROLL_OWNED)
                .clip(ui_core::layout::Clip {
                    x: ui_core::layout::Overflow::Hidden,
                    y: ui_core::layout::Overflow::Scroll,
                })
                .child(column);
            crate::from_decl(scroller)
        }
    }

    fn listed(jump_row: Option<usize>) -> (AppLoop<Listed>, OffscreenTarget) {
        let app = Listed {
            content: 4000.0,
            jump_row,
            views: 0,
        };
        (AppLoop::new(app), OffscreenTarget::new(100, 100))
    }

    fn listing(content: f32) -> (AppLoop<Listed>, OffscreenTarget) {
        let app = Listed {
            content,
            jump_row: None,
            views: 0,
        };
        (AppLoop::new(app), OffscreenTarget::new(100, 100))
    }

    fn offset(lp: &AppLoop<Listed>) -> f32 {
        lp.scroll_of(Id::str("scroller")).unwrap_or(0.0)
    }

    /// The id table salts with the hash of the value, and a `usize` salt is not the `i32`
    /// one: the row the app declares is `with(row)` where `row` counts rows.
    fn row(index: usize) -> Id {
        Id::str("row").with(index)
    }

    #[test]
    fn a_revealed_row_is_declared_by_the_frame_that_reveals_it() {
        let (mut lp, mut target) = listed(Some(120));
        lp.frame(&mut target, None);
        assert_eq!(offset(&lp), 0.0, "the first frame stands at the top");
        lp.frame(&mut target, None);
        assert_eq!(offset(&lp), 2400.0, "the reveal moved the scroller");
        assert!(
            lp.tree().find_by_id(row(120)).is_some(),
            "the frame must declare the rows it paints, not the rows of the old offset"
        );
    }

    #[test]
    fn a_shrunken_list_owes_the_frame_that_rewindows_it() {
        let (mut lp, mut target) = listed(Some(195));
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        assert_eq!(offset(&lp), 3900.0, "the list stands on its last page");
        // The directory behind the list shrinks under the offset it stood at.
        lp.app_mut().content = 300.0;
        lp.take_dispatched();
        lp.frame(&mut target, None);
        assert_eq!(offset(&lp), 200.0, "layout clamps the offset past the new end");
        assert!(
            lp.frame_due(),
            "the frame that shrank the box owes the window that follows it"
        );
        lp.frame(&mut target, None);
        assert!(
            lp.tree().find_by_id(row(10)).is_some(),
            "the next window lands on the rows the clamped offset shows"
        );
    }

    #[test]
    fn a_list_whose_viewport_grows_owes_the_frame_that_rewindows_it() {
        // A three-row list: the scroller hugs it, so its viewport is three rows tall.
        let (mut lp, mut target) = listing(60.0);
        lp.frame(&mut target, None);
        lp.frame(&mut target, None);
        lp.take_dispatched();
        // The directory grows to twenty rows, and the scroller to its ceiling.
        lp.app_mut().content = 400.0;
        lp.frame(&mut target, None);
        assert!(
            lp.tree().find_by_id(row(4)).is_none(),
            "the frame windowed from the short box it read before layout"
        );
        assert!(
            lp.frame_due(),
            "the frame that grew the viewport owes the window that follows it"
        );
        lp.frame(&mut target, None);
        assert!(
            lp.tree().find_by_id(row(4)).is_some(),
            "the next frame windows from the viewport it now has"
        );
    }
}

/// A drag hands its movement down a chain of scrollers: each link takes what its own range
/// allows on the axes it owns, and the rest passes to the ancestor behind it.
mod pan_chain_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;

    /// A 200×200 vertical page holding a 200×40 horizontal strip of 600-wide content, both
    /// engine-owned: the shape of a carousel inside a scrolling page.
    struct Nested;

    impl App for Nested {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let strip_content = Decl::container(Id::str("strip-content"), NodeKind::Row)
                .size(ui_core::layout::SizeClass::fixed(600.0, 40.0));
            let strip = Decl::container(Id::str("strip"), NodeKind::Row)
                .size(ui_core::layout::SizeClass::fixed(200.0, 40.0))
                .sense(ui_core::Sense::HOVER | ui_core::Sense::SCROLL_OWNED)
                .clip(ui_core::layout::Clip {
                    x: ui_core::layout::Overflow::Scroll,
                    y: ui_core::layout::Overflow::Hidden,
                })
                .child(strip_content);
            // The strip sits below a spacer, so a drag inside it can travel both ways.
            let spacer = Decl::container(Id::str("spacer"), NodeKind::Box)
                .size(ui_core::layout::SizeClass::fixed(200.0, 100.0));
            let page_content = Decl::container(Id::str("content"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(200.0, 400.0))
                .child(spacer)
                .child(strip);
            crate::from_decl(
                Decl::container(Id::str("page"), NodeKind::Column)
                    .size(ui_core::layout::SizeClass::fixed(200.0, 200.0))
                    .sense(ui_core::Sense::HOVER | ui_core::Sense::SCROLL_OWNED)
                    .clip(ui_core::layout::Clip {
                        x: ui_core::layout::Overflow::Hidden,
                        y: ui_core::layout::Overflow::Scroll,
                    })
                    .child(page_content),
            )
        }
    }

    fn nested() -> (AppLoop<Nested>, OffscreenTarget) {
        let mut lp = AppLoop::new(Nested);
        lp.clock = FrameClock::scripted((0..=60).map(|i| i as f64 / 60.0).collect());
        let mut target = OffscreenTarget::new(200, 200);
        lp.frame(&mut target, None);
        (lp, target)
    }

    /// Press inside the strip at `from` and drag to `to`, crossing the pan threshold on the
    /// way.
    fn drag(
        lp: &mut AppLoop<Nested>,
        target: &mut OffscreenTarget,
        from: ui_core::geom::Point,
        to: ui_core::geom::Point,
    ) {
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: from,
            button: ui_core::PointerButton::Primary,
        });
        // The first move only crosses the threshold, and the pan then re-resolves its
        // scroller at that position; a long first step would start it outside the strip.
        // `f32::signum` answers 1.0 for 0.0, so an axis that does not move must be left
        // alone rather than nudged by six.
        let step = |a: f32, b: f32| {
            if (b - a).abs() < f32::EPSILON {
                a
            } else {
                a + (b - a).signum() * 6.0
            }
        };
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(step(from.x, to.x), step(from.y, to.y)),
        });
        lp.dispatch(ui_core::Event::PointerMoved { pos: to });
        lp.frame(target, None);
    }

    fn offset(lp: &AppLoop<Nested>, id: Id) -> (f32, f32) {
        lp.scroll_offset(id).map(|o| (o.x, o.y)).unwrap_or_default()
    }

    #[test]
    fn a_vertical_drag_over_a_horizontal_strip_moves_the_page_behind_it() {
        // The strip takes the press but owns only x, so a vertical drag passes straight
        // through it; before the chain the gesture died there and the page never moved.
        let (mut lp, mut target) = nested();
        // Upwards: the finger moves up and the content follows, so the page scrolls down.
        drag(
            &mut lp,
            &mut target,
            ui_core::geom::Point::new(100.0, 120.0),
            ui_core::geom::Point::new(100.0, 40.0),
        );
        let (strip_x, strip_y) = offset(&lp, Id::str("strip"));
        let (page_x, page_y) = offset(&lp, Id::str("page"));
        assert_eq!(
            (strip_x, strip_y),
            (0.0, 0.0),
            "the strip has no vertical range and must not move"
        );
        assert_eq!(page_x, 0.0, "and the page owns no horizontal range");
        assert!(page_y > 0.0, "the drag must reach the page, got {page_y}");
    }

    #[test]
    fn a_horizontal_drag_stops_at_the_strip_and_leaves_the_page_still() {
        // The innermost scroller owns the axis: once it can take the movement, nothing
        // reaches the page, so a carousel does not drag its container sideways.
        let (mut lp, mut target) = nested();
        drag(
            &mut lp,
            &mut target,
            ui_core::geom::Point::new(100.0, 120.0),
            ui_core::geom::Point::new(40.0, 120.0),
        );
        let (strip_x, _) = offset(&lp, Id::str("strip"));
        let (_, page_y) = offset(&lp, Id::str("page"));
        assert!(strip_x > 0.0, "the strip must take the drag, got {strip_x}");
        assert_eq!(page_y, 0.0, "and the page must stay put");
    }
}

mod scrollbar_axis_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;

    /// 200×100 over 600×400 of content, with a bar on each axis. The horizontal bar is the
    /// only handle the sideways range has, so it must drag x and leave y alone.
    struct Both;

    impl App for Both {
        fn view(&mut self, _f: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            let content = Decl::container(Id::str("content"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(600.0, 400.0));
            let mut decl = Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(ui_core::layout::SizeClass::fixed(200.0, 100.0))
                .sense(ui_core::Sense::HOVER | ui_core::Sense::SCROLL_OWNED)
                .clip(ui_core::layout::Clip {
                    x: ui_core::layout::Overflow::Scroll,
                    y: ui_core::layout::Overflow::Scroll,
                })
                .child(content);
            let bar = |thumb: Rect, track: Rect| ui_core::ScrollbarBar {
                thumb,
                track,
                track_fill: ui_core::geom::Color::TRANSPARENT,
                thumb_fill: ui_core::geom::Color::from_rgba8(0, 0, 0, 255),
                track_corners: ui_core::geom::Corners::ZERO,
                thumb_corners: ui_core::geom::Corners::ZERO,
            };
            if let Some(params) = decl.data.params_mut() {
                params.scrollbar = Some(ui_core::Scrollbar {
                    vertical: Some(bar(
                        Rect::new(196.0, 0.0, 4.0, 25.0),
                        Rect::new(196.0, 0.0, 4.0, 100.0),
                    )),
                    horizontal: Some(bar(
                        Rect::new(0.0, 96.0, 66.0, 4.0),
                        Rect::new(0.0, 96.0, 200.0, 4.0),
                    )),
                    fade: false,
                });
            }
            crate::from_decl(decl)
        }
    }

    #[test]
    fn the_horizontal_thumb_drags_the_x_offset_only() {
        let mut lp = AppLoop::new(Both);
        lp.clock = FrameClock::scripted((0..=10).map(|i| i as f64 / 60.0).collect());
        let mut target = OffscreenTarget::new(200, 100);
        lp.frame(&mut target, None);
        // Right of the thumb: the press jumps it there, and the drag then carries it on.
        lp.dispatch(ui_core::Event::PointerPressed {
            pos: ui_core::geom::Point::new(100.0, 98.0),
            button: ui_core::PointerButton::Primary,
        });
        lp.dispatch(ui_core::Event::PointerMoved {
            pos: ui_core::geom::Point::new(120.0, 98.0),
        });
        lp.frame(&mut target, None);
        let offset = lp.scroll_offset(Id::str("scroller")).unwrap_or_default();
        assert!(
            offset.x > 0.0,
            "the bar must move the sideways offset, got {}",
            offset.x
        );
        assert_eq!(offset.y, 0.0, "and leave the vertical one alone");
    }
}

mod modifiers {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use ui_core::Decl;
    use ui_core::layout::{AxisSize, Grow, SizeClass};

    /// Reads the held modifiers only when it says so: the read is the whole contract.
    struct Watcher {
        watches: bool,
    }

    impl App for Watcher {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            if self.watches {
                let _ = cx.modifiers().alt;
            }
            Box::new(Plain)
        }
    }

    struct Plain;

    impl crate::Element for Plain {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            Decl::container(Id::str("plain"), NodeKind::Box).size(SizeClass {
                width: AxisSize::fill(0.0, 100.0),
                height: AxisSize::fixed(20.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
        }
    }

    fn held(alt: bool) -> ui_core::Event {
        ui_core::Event::Modifiers(ui_core::Modifiers {
            alt,
            ..Default::default()
        })
    }

    /// A modifier change repaints only when the last build read the modifiers: a tree that
    /// never asked for them must not pay a frame each time Alt is pressed.
    #[test]
    fn a_modifier_change_repaints_when_the_build_read_it() {
        let mut target = OffscreenTarget::new(400, 200);

        let mut blind = AppLoop::new(Watcher { watches: false });
        blind.frame(&mut target, None);
        assert!(!blind.dispatch(held(true)), "nothing read the modifiers");

        let mut watcher = AppLoop::new(Watcher { watches: true });
        watcher.frame(&mut target, None);
        assert!(watcher.dispatch(held(true)), "a reader must be repainted");
        watcher.frame(&mut target, None);
        assert!(watcher.dispatch(held(false)), "the release repaints again");
    }
}

mod cursor_command_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::view::*;
    use crate::{Container, OffscreenTarget};
    use ui_core::geom::Point;
    use ui_core::layout::SizeClass;
    use ui_core::{Event, Id, Key, KeyChord, Modifiers, NodeKind, PointerButton, Sense, ShortcutId};

    /// A `MIDDLE` root with a child that takes the primary button, recording what the
    /// middle button reports.
    struct MiddleButton {
        seen: Rc<RefCell<Vec<(Id, bool)>>>,
    }

    impl App for MiddleButton {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Box::new(
                Container::new(Id::str("root"), NodeKind::Box)
                    .size(SizeClass::fixed(200.0, 120.0))
                    .sense(Sense::MIDDLE)
                    .child(Box::new(
                        Container::new(Id::str("inside"), NodeKind::Box)
                            .size(SizeClass::fixed(100.0, 80.0))
                            .sense(Sense::DRAG),
                    )),
            )
        }

        fn changed(&mut self, id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Middle { down, .. } = change {
                self.seen.borrow_mut().push((id, down));
                return true;
            }
            false
        }
    }

    /// The middle button belongs to the nearest node that declared it — a control taking the
    /// primary button does not swallow it — and the release returns to that same node even
    /// when the pointer has left the surface area it went down in.
    #[test]
    fn a_middle_press_reaches_the_nearest_declaring_ancestor_and_returns() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(MiddleButton {
            seen: Rc::clone(&seen),
        });
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);

        let button = PointerButton::Middle;
        let over_child = Point::new(20.0, 20.0);
        assert!(lp.dispatch(Event::PointerPressed {
            pos: over_child,
            button
        }));
        assert_eq!(
            *seen.borrow(),
            [(Id::str("root"), true)],
            "the child that takes the primary button is not what the middle button reports"
        );

        lp.dispatch(Event::PointerReleased {
            pos: Point::new(190.0, 110.0),
            button,
        });
        assert_eq!(
            *seen.borrow(),
            [(Id::str("root"), true), (Id::str("root"), false)],
            "a release far from the press still ends that press"
        );
    }

    struct PlainKeys {
        seen: Rc<RefCell<Vec<ShortcutId>>>,
    }

    impl App for PlainKeys {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Box::new(
                Container::new(Id::str("root"), NodeKind::Box).size(SizeClass::fixed(200.0, 120.0)),
            )
        }

        fn shortcut(&mut self, id: ShortcutId, _: ui_core::Modifiers) -> bool {
            self.seen.borrow_mut().push(id);
            true
        }
    }

    /// A letter arrives as text, never as a key event; with nothing typing, a lone character
    /// is the key it names, which is how a plain letter can be a command. Anything longer is
    /// an insertion, not a chord.
    #[test]
    fn a_lone_letter_is_the_key_it_names_when_nothing_types() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(PlainKeys {
            seen: Rc::clone(&seen),
        });
        let plain = ShortcutId("plain.a");
        lp.bind_shortcut(KeyChord::new(Key::Char('a'), Modifiers::default()), plain);
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);

        assert!(lp.dispatch(Event::Text("A".to_string())));
        assert_eq!(
            *seen.borrow(),
            [plain],
            "a capital is the same chord as the letter it spells"
        );

        seen.borrow_mut().clear();
        lp.dispatch(Event::Text("ab".to_string()));
        assert!(
            seen.borrow().is_empty(),
            "several characters are text, not a command"
        );
    }

    struct Nudger;

    impl App for Nudger {
        fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            cx.nudge_pointer(-2.0, 1.0);
            Box::new(
                Container::new(Id::str("root"), NodeKind::Box).size(SizeClass::fixed(20.0, 20.0)),
            )
        }
    }

    /// A cursor key's step leaves the build as a request the surface drains, once: the app
    /// cannot move the pointer itself, and a step it re-declares every frame would keep
    /// moving it.
    #[test]
    fn a_pointer_request_leaves_the_build_once() {
        let mut lp = AppLoop::new(Nudger);
        let mut target = OffscreenTarget::new(200, 120);
        lp.frame(&mut target, None);
        assert_eq!(lp.take_pointer_steps(), vec![(-2.0, 1.0)]);
        assert!(lp.take_pointer_steps().is_empty(), "a request is drained once");
    }
}

mod press_burst_tests {
    use crate::present::OffscreenTarget;
    use crate::view::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_core::Decl;
    use ui_core::Sense;
    use ui_core::geom::Point;
    use ui_core::layout::SizeClass;

    /// One row's view of the burst: every completed primary click and the count it carried.
    struct Tap {
        sense: Sense,
        clicks: Rc<RefCell<Vec<u32>>>,
    }

    impl crate::Element for Tap {
        fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
            Decl::container(Id::str("tap"), NodeKind::Box)
                .size(SizeClass::fixed(80.0, 40.0))
                .sense(self.sense)
        }

        fn change(
            &self,
            state: &crate::WidgetState,
            _bounds: ui_core::geom::Rect,
        ) -> Option<ui_core::change::Change> {
            if state.clicked {
                self.clicks.borrow_mut().push(state.clicks);
            }
            None
        }
    }

    struct Taps {
        sense: Sense,
        clicks: Rc<RefCell<Vec<u32>>>,
        secondary: Rc<RefCell<Vec<u32>>>,
    }

    impl App for Taps {
        fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
            Box::new(Tap {
                sense: self.sense,
                clicks: Rc::clone(&self.clicks),
            })
        }

        fn changed(&mut self, _id: Id, change: ui_core::change::Change) -> bool {
            if let ui_core::change::Change::Secondary { down: true, clicks } = change {
                self.secondary.borrow_mut().push(clicks);
            }
            true
        }
    }

    fn press(
        lp: &mut AppLoop<Taps>,
        button: ui_core::PointerButton,
        at: Point,
    ) {
        lp.dispatch(ui_core::Event::PointerPressed { button, pos: at });
        lp.dispatch(ui_core::Event::PointerReleased { button, pos: at });
    }

    /// The desktop rule: a press inside the gap and the rectangle joins the previous one.
    /// Each button has its own burst, so a right double-click is two presses, not a mix.
    #[test]
    fn a_press_joins_its_burst_only_within_the_desktops_gap_and_rectangle() {
        let clicks = Rc::new(RefCell::new(Vec::new()));
        let secondary = Rc::new(RefCell::new(Vec::new()));
        let mut lp = AppLoop::new(Taps {
            sense: Sense::HOVER | Sense::CLICK | Sense::SECONDARY,
            clicks: Rc::clone(&clicks),
            secondary: Rc::clone(&secondary),
        });
        lp.clock = FrameClock::scripted(vec![0.0]);
        let mut target = OffscreenTarget::new(200, 200);
        lp.frame(&mut target, None);
        let at = Point::new(20.0, 20.0);
        let primary = ui_core::PointerButton::Primary;

        press(&mut lp, primary, at);
        lp.frame(&mut target, None);
        press(&mut lp, primary, at);
        lp.frame(&mut target, None);
        assert_eq!(*clicks.borrow(), [1, 2], "one double-click reads as two presses");

        // Past the desktop's half second, the next press is a new single click.
        lp.build_with_dt((200, 200), None, 0.6);
        press(&mut lp, primary, at);
        lp.frame(&mut target, None);
        assert_eq!(*clicks.borrow(), [1, 2, 1], "a slow second press starts over");

        // The rectangle is what separates a double-click from two clicks in one row.
        press(&mut lp, primary, Point::new(20.0, 32.0));
        lp.frame(&mut target, None);
        assert_eq!(*clicks.borrow(), [1, 2, 1, 1], "a moved press starts over");

        let secondary_button = ui_core::PointerButton::Secondary;
        press(&mut lp, secondary_button, at);
        lp.frame(&mut target, None);
        press(&mut lp, secondary_button, at);
        lp.frame(&mut target, None);
        assert_eq!(
            *secondary.borrow(),
            [1, 2],
            "the secondary button counts its own presses, so a right double-click reads 2"
        );
    }
}
