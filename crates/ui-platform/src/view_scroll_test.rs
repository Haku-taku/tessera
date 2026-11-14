//! Engine tests for the engine-owned scroll container: wheel accumulation and clamping.
//!
//! These tests live in ui-platform because the wheel handler (`scroll_owned_delta`) is part
//! of `AppLoop`. They use a bare `Decl` with `Sense::SCROLL_OWNED`, never ui-widgets, so the
//! layering boundary (platform must not depend on widgets) stays intact.

use ui_core::{Decl, Id, NodeKind, Sense, SizeClass, geom, layout};

use crate::{App, AppLoop, OffscreenTarget};

/// An app that reads its scroller's geometry from the context, as a scrollbar or a
/// collapsing app bar would: the read must see the engine's stored offset, not the
/// declaration's default.
struct ReadingApp {
    seen: std::rc::Rc<std::cell::RefCell<Vec<(f32, f32, f32, f32, bool)>>>,
}

impl App for ReadingApp {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let content =
            Decl::container(Id::str("content"), NodeKind::Box).size(SizeClass::fixed(100.0, 300.0));
        let read = cx.scroll_of(Id::str("scroller"));
        self.seen.borrow_mut().push(
            read.map(|g| (g.extent_h, g.viewport_w, g.viewport_h, g.offset_y, g.active))
                .unwrap_or((-1.0, -1.0, -1.0, -1.0, false)),
        );
        crate::from_decl(
            Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(content),
        )
    }
}

/// A fixed 100-tall scroll container holding a 300-tall child: 200 overflow on y.
struct ScrollApp;

impl App for ScrollApp {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        // A filled box: an unpainted container emits no primitive, and damage has nothing
        // to compare.
        let content = Decl {
            shapes: vec![ui_core::draw::Shape::Box {
                corners: geom::Corners::ZERO,
                fill: geom::Color::WHITE,
                gradient: None,
                border: None,
                shadow: None,
                image: None,
            }],
            ..Decl::container(Id::str("content"), NodeKind::Box)
                .size(SizeClass::fixed(100.0, 300.0))
        };
        crate::from_decl(
            Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(content),
        )
    }
}

/// The scroller's `scroll_y` after a frame.
fn scroll_y_of(lp: &AppLoop<ScrollApp>) -> f32 {
    let nid = lp.tree().find_by_id(Id::str("scroller")).unwrap();
    lp.tree()
        .get(nid)
        .and_then(|n| n.data.params())
        .map(|p| p.scroll_y)
        .unwrap()
}

#[test]
fn an_engine_owned_scroller_accumulates_the_wheel_and_clamps_to_its_content() {
    // A `SCROLL_OWNED` container takes the wheel itself: the engine sums the delta into
    // `scroll_y` and clamps at the content overflow, instead of reporting `Value` up.
    let mut lp = AppLoop::new(ScrollApp);
    let mut target = OffscreenTarget::new(100, 100);
    lp.frame(&mut target, None);
    let pos = geom::Point::new(50.0, 50.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.dispatch(ui_core::Event::wheel(pos, 30.0));
    lp.frame(&mut target, None);
    assert!(
        (scroll_y_of(&lp) - 30.0).abs() < 1e-3,
        "the engine must accumulate the wheel into the scroller, got {}",
        scroll_y_of(&lp)
    );
    // The content is 300 tall in a 100-tall viewport: 200 is the clamp ceiling.
    lp.dispatch(ui_core::Event::wheel(pos, 300.0));
    lp.frame(&mut target, None);
    assert!(
        (scroll_y_of(&lp) - 200.0).abs() < 1e-3,
        "the wheel must clamp at the content overflow, got {}",
        scroll_y_of(&lp)
    );
    // A negative delta scrolls back toward the top, but never below zero.
    lp.dispatch(ui_core::Event::wheel(pos, -500.0));
    lp.frame(&mut target, None);
    assert!(
        (scroll_y_of(&lp) - 0.0).abs() < 1e-3,
        "the wheel must not scroll below the top, got {}",
        scroll_y_of(&lp)
    );
}

#[test]
fn a_build_reads_the_engine_owned_offset_and_content_extent_from_the_context() {
    // A control that draws a scrollbar or follows the scroll must read the engine's stored
    // offset through `Cx::scroll_of`, one frame behind the wheel but never the declaration's
    // default. The content extent is the scroller's 300-tall child.
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut lp = AppLoop::new(ReadingApp { seen: seen.clone() });
    let mut target = OffscreenTarget::new(100, 100);
    // First frame establishes the tree; the context has nothing to report yet.
    lp.frame(&mut target, None);
    assert_eq!(*seen.borrow(), vec![(-1.0, -1.0, -1.0, -1.0, false)]);

    let pos = geom::Point::new(50.0, 50.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.dispatch(ui_core::Event::wheel(pos, 40.0));
    lp.frame(&mut target, None);
    // Second frame: the previous frame's layout and the stored offset show up; the viewport
    // size (a 100x100 scroller) lets a scrollbar size and anchor its thumb. The wheel marks
    // the scroller active, which the auto scrollbar keys on.
    assert_eq!(
        *seen.borrow(),
        vec![
            (-1.0, -1.0, -1.0, -1.0, false),
            (300.0, 100.0, 100.0, 40.0, true)
        ]
    );
    // Third frame, after another wheel: the offset follows, the extents unchanged.
    lp.dispatch(ui_core::Event::wheel(pos, 20.0));
    lp.frame(&mut target, None);
    assert_eq!(
        *seen.borrow(),
        vec![
            (-1.0, -1.0, -1.0, -1.0, false),
            (300.0, 100.0, 100.0, 40.0, true),
            (300.0, 100.0, 100.0, 60.0, true)
        ]
    );
}

/// A scroller whose build keeps the row 200 units down visible.
struct BringApp;

impl App for BringApp {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        cx.bring_into_view(Id::str("target"));
        crate::from_decl(
            Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(
                    Decl::container(Id::str("content"), NodeKind::Column)
                        .size(SizeClass::fixed(100.0, 300.0))
                        .child(
                            Decl::container(Id::str("spacer"), NodeKind::Box)
                                .size(SizeClass::fixed(100.0, 200.0)),
                        )
                        .child(
                            Decl::container(Id::str("target"), NodeKind::Box)
                                .size(SizeClass::fixed(100.0, 30.0)),
                        ),
                ),
        )
    }
}

/// A small scroller inside a taller one: 90 of content in a 60-tall inner viewport, itself
/// sitting below 40 of spacer inside a 60-tall outer viewport (40 of travel each).
struct NestedApp;

impl App for NestedApp {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        crate::from_decl(
            Decl::container(Id::str("outer"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 60.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(
                    Decl::container(Id::str("spacer"), NodeKind::Box)
                        .size(SizeClass::fixed(100.0, 40.0)),
                )
                .child(
                    Decl::container(Id::str("inner"), NodeKind::Column)
                        .size(SizeClass::fixed(100.0, 60.0))
                        .sense(Sense::SCROLL_OWNED)
                        .clip(layout::Clip {
                            x: layout::Overflow::Hidden,
                            y: layout::Overflow::Scroll,
                        })
                        .child(
                            Decl::container(Id::str("inner-content"), NodeKind::Box)
                                .size(SizeClass::fixed(100.0, 90.0)),
                        ),
                ),
        )
    }
}

#[test]
fn a_wheel_at_the_end_of_an_inner_scroller_moves_the_one_behind_it() {
    // The wheel lands on the inner list, which has 30 of travel of its own; the rest of the
    // step belongs to the page behind it. Routing by the hit alone stopped at the inner
    // container and swallowed the gesture, so an inner list at its end froze the page.
    let mut lp = AppLoop::new(NestedApp);
    let mut target = OffscreenTarget::new(100, 100);
    lp.frame(&mut target, None);
    let pos = geom::Point::new(50.0, 50.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.dispatch(ui_core::Event::wheel(pos, 50.0));
    lp.frame(&mut target, None);
    assert_eq!(
        lp.scroll_offset(Id::str("inner")).map(|o| o.y),
        Some(30.0),
        "the inner list takes all the travel it has"
    );
    assert_eq!(
        lp.scroll_offset(Id::str("outer")).map(|o| o.y),
        Some(20.0),
        "the rest of the step moves the page behind it"
    );

    // The inner list is at its end now: the whole step goes to the outer one.
    lp.dispatch(ui_core::Event::wheel(pos, 20.0));
    lp.frame(&mut target, None);
    assert_eq!(
        lp.scroll_offset(Id::str("inner")).map(|o| o.y),
        Some(30.0),
        "an exhausted inner list stays put"
    );
    assert_eq!(
        lp.scroll_offset(Id::str("outer")).map(|o| o.y),
        Some(40.0),
        "the page takes the whole step"
    );
}

/// A sideways strip inside a vertical page. A strip that scrolls one direction takes the
/// upright wheel itself, so reaching it needs no modifier; a device that reports x keeps its
/// own sideways step and the vertical part still reaches the page.
#[test]
fn a_plain_wheel_over_a_sideways_strip_scrolls_it_and_not_the_page() {
    let mut lp = AppLoop::new(SidewaysApp);
    let mut target = OffscreenTarget::new(100, 100);
    lp.frame(&mut target, None);
    let pos = geom::Point::new(50.0, 25.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });

    lp.dispatch(ui_core::Event::wheel(pos, 40.0));
    lp.frame(&mut target, None);
    assert_eq!(
        lp.scroll_offset(Id::str("strip")).map(|o| o.x),
        Some(40.0),
        "the upright wheel is spent on the only axis the strip has"
    );
    assert_eq!(
        lp.scroll_offset(Id::str("page"))
            .map(|o| o.y)
            .unwrap_or(0.0),
        0.0,
        "and is not passed on to the page behind it"
    );
}

/// A page whose content overflows on both axes.
struct BothApp;

impl App for BothApp {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let content =
            Decl::container(Id::str("content"), NodeKind::Box).size(SizeClass::fixed(300.0, 300.0));
        crate::from_decl(
            Decl::container(Id::str("page"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Scroll,
                    y: layout::Overflow::Scroll,
                })
                .child(content),
        )
    }
}

#[test]
fn a_shift_wheel_turns_the_wheel_sideways() {
    // Shift is the convention a mouse expects for the horizontal axis. Here the container
    // scrolls both ways, so the plain wheel keeps its own axis and only shift crosses over.
    let mut lp = AppLoop::new(BothApp);
    let mut target = OffscreenTarget::new(100, 100);
    lp.frame(&mut target, None);
    let pos = geom::Point::new(50.0, 50.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });

    lp.dispatch(ui_core::Event::wheel(pos, 40.0));
    lp.frame(&mut target, None);
    let plain = lp.scroll_offset(Id::str("page")).unwrap_or_default();
    assert_eq!((plain.x, plain.y), (0.0, 40.0), "a plain wheel is vertical");

    lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
        shift: true,
        ..ui_core::Modifiers::NONE
    }));
    lp.dispatch(ui_core::Event::wheel(pos, 40.0));
    lp.frame(&mut target, None);
    let shifted = lp.scroll_offset(Id::str("page")).unwrap_or_default();
    assert_eq!(
        (shifted.x, shifted.y),
        (40.0, 40.0),
        "shift spends the same wheel on x and leaves y where it was"
    );
}

struct SidewaysApp;

impl App for SidewaysApp {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let wide =
            Decl::container(Id::str("wide"), NodeKind::Box).size(SizeClass::fixed(600.0, 50.0));
        let strip = Decl::container(Id::str("strip"), NodeKind::Row)
            .size(SizeClass::fixed(100.0, 50.0))
            .sense(Sense::SCROLL_OWNED)
            .clip(layout::Clip {
                x: layout::Overflow::Scroll,
                y: layout::Overflow::Hidden,
            })
            .child(wide);
        let below =
            Decl::container(Id::str("below"), NodeKind::Box).size(SizeClass::fixed(100.0, 300.0));
        crate::from_decl(
            Decl::container(Id::str("page"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(
                    Decl::container(Id::str("content"), NodeKind::Column)
                        .size(SizeClass {
                            width: ui_core::AxisSize::hug(0.0, 0.0),
                            height: ui_core::AxisSize::hug(0.0, 0.0),
                            grow_x: ui_core::Grow::No,
                            grow_y: ui_core::Grow::No,
                        })
                        .child(strip)
                        .child(below),
                ),
        )
    }
}

/// A scroller whose content shrinks between frames, as a filtered list does.
struct ShrinkingApp {
    tall: std::rc::Rc<std::cell::Cell<bool>>,
}

impl App for ShrinkingApp {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let height = if self.tall.get() { 300.0 } else { 50.0 };
        crate::from_decl(
            Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(
                    Decl::container(Id::str("content"), NodeKind::Box)
                        .size(SizeClass::fixed(100.0, height)),
                ),
        )
    }
}

#[test]
fn a_shorter_content_pulls_the_stored_offset_back_into_range() {
    // The offset map is the source of truth, but layout is what knows how much content there
    // is. A list filtered from 300 to 50 while scrolled to its old end used to keep the
    // offset: the bar was then drawn outside its track, `scroll_of` reported a position the
    // content no longer had, and the first wheel step moved nothing.
    let tall = std::rc::Rc::new(std::cell::Cell::new(true));
    let mut lp = AppLoop::new(ShrinkingApp { tall: tall.clone() });
    let mut target = OffscreenTarget::new(100, 100);
    lp.frame(&mut target, None);
    let pos = geom::Point::new(50.0, 50.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.dispatch(ui_core::Event::wheel(pos, 300.0));
    lp.frame(&mut target, None);
    assert_eq!(
        lp.scroll_offset(Id::str("scroller")).map(|o| o.y),
        Some(200.0),
        "300 of content in a 100 viewport"
    );

    tall.set(false);
    lp.frame(&mut target, None);
    assert_eq!(
        lp.scroll_offset(Id::str("scroller")).map(|o| o.y),
        Some(0.0),
        "50 of content leaves nothing to scroll"
    );
    // The shrink is painted correctly in the frame it happens: the content was placed at
    // the old offset before the new extent was known, and the placement puts it back rather
    // than showing one empty frame.
    let scroller = lp.tree().find_by_id(Id::str("scroller")).unwrap();
    let top = lp.tree().get(scroller).unwrap().rect.origin.y;
    let content = lp.tree().find_by_id(Id::str("content")).unwrap();
    assert_eq!(
        lp.tree().get(content).unwrap().rect.origin.y,
        top,
        "the content is not drawn above the viewport at an offset that no longer exists"
    );
    lp.frame(&mut target, None);
    assert_eq!(
        scroll_y_of_shrinking(&lp),
        0.0,
        "the next frame draws from the top"
    );
}

fn scroll_y_of_shrinking(lp: &AppLoop<ShrinkingApp>) -> f32 {
    let nid = lp.tree().find_by_id(Id::str("scroller")).unwrap();
    lp.tree()
        .get(nid)
        .and_then(|n| n.data.params())
        .map(|p| p.scroll_y)
        .unwrap()
}

/// A row inside an inner list that already shows it, inside a page scrolled elsewhere.
struct NestedBringApp;

impl App for NestedBringApp {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        cx.bring_into_view(Id::str("target"));
        crate::from_decl(
            Decl::container(Id::str("outer"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(
                    Decl::container(Id::str("outer-content"), NodeKind::Column)
                        .size(SizeClass::fixed(100.0, 300.0))
                        .child(
                            Decl::container(Id::str("spacer"), NodeKind::Box)
                                .size(SizeClass::fixed(100.0, 200.0)),
                        )
                        .child(
                            Decl::container(Id::str("inner"), NodeKind::Column)
                                .size(SizeClass::fixed(100.0, 60.0))
                                .sense(Sense::SCROLL_OWNED)
                                .clip(layout::Clip {
                                    x: layout::Overflow::Hidden,
                                    y: layout::Overflow::Scroll,
                                })
                                .child(
                                    Decl::container(Id::str("inner-content"), NodeKind::Column)
                                        .size(SizeClass::fixed(100.0, 60.0))
                                        .child(
                                            Decl::container(Id::str("lead"), NodeKind::Box)
                                                .size(SizeClass::fixed(100.0, 20.0)),
                                        )
                                        .child(
                                            Decl::container(Id::str("target"), NodeKind::Box)
                                                .size(SizeClass::fixed(100.0, 20.0)),
                                        ),
                                ),
                        ),
                ),
        )
    }
}

#[test]
fn keeping_a_row_visible_moves_every_scroller_above_it() {
    // The inner list already shows the row, so it does not move — but the list itself sits
    // 200 units down a 100-tall page. Moving only the nearest scroller left the row on a
    // page that was still scrolled away, and focus landed somewhere invisible.
    let mut lp = AppLoop::new(NestedBringApp);
    let mut target = OffscreenTarget::new(200, 200);
    lp.frame(&mut target, None);
    lp.frame(&mut target, None);
    assert_eq!(
        lp.scroll_offset(Id::str("outer")).map(|o| o.y),
        Some(140.0),
        "the row's 240 end meets the page's 100 edge"
    );
    assert!(
        lp.scroll_offset(Id::str("inner"))
            .is_none_or(|o| o.y == 0.0),
        "the inner list needed no movement"
    );
}

#[test]
fn a_build_can_keep_a_node_visible_in_its_scroller() {
    // WinUI's `BringIntoViewOnFocusChange`: the control names the row that must stay seen.
    let mut lp = AppLoop::new(BringApp);
    let mut target = OffscreenTarget::new(200, 200);
    lp.frame(&mut target, None);
    lp.frame(&mut target, None);
    let offset = lp
        .scroll_offset(Id::str("scroller"))
        .expect("the request stored an offset");
    assert_eq!(
        offset.y, 130.0,
        "the target's 230 end meets the viewport's 100 edge"
    );
}

/// One tall text leaf inside an engine scroller, with the whole leaf as the input's one
/// view: the caret's line, never the node's box, is what has to stay visible.
struct ScrolledText {
    text: String,
    nowrap: bool,
}

impl App for ScrolledText {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let mut view = Decl::container(Id::str("view"), NodeKind::Text).size(SizeClass {
            // An unwrapped line is as wide as its text, so the leaf itself overflows the
            // viewport; wrapped text stays inside it.
            width: ui_core::AxisSize::fixed(if self.nowrap { 800.0 } else { 180.0 }),
            height: ui_core::AxisSize::hug(0.0, 0.0),
            grow_x: ui_core::Grow::No,
            grow_y: ui_core::Grow::No,
        });
        view.data = ui_core::NodeData::Text {
            ink: geom::Color::WHITE,
            block: std::sync::Arc::new(ui_core::TextBlock {
                runs: vec![ui_core::TextRun::new(
                    self.text.as_str(),
                    ui_core::TextStyle::new(18.0),
                )],
                input_base: Some(0),
                line_numbers: None,
                ..Default::default()
            }),
            caret: None,
            selection: None,
            composing: None,
            nowrap: self.nowrap,
        };
        crate::from_decl(
            Decl::container(Id::str("scroller"), NodeKind::Column)
                .size(SizeClass::fixed(180.0, 120.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Scroll,
                    y: layout::Overflow::Scroll,
                })
                .child(
                    Decl::container(Id::str("doc"), NodeKind::Column)
                        .size(SizeClass {
                            width: ui_core::AxisSize::hug(0.0, 0.0),
                            height: ui_core::AxisSize::hug(0.0, 0.0),
                            grow_x: ui_core::Grow::No,
                            grow_y: ui_core::Grow::No,
                        })
                        .sense(Sense::CLICK | Sense::FOCUSABLE)
                        .text_input(self.text.clone())
                        .text_input_enter_newline()
                        .child(view),
                ),
        )
    }
}

fn numbered_lines(count: usize) -> ScrolledText {
    let mut text = String::new();
    for line in 0..count {
        text.push_str(&format!("line {line}\n"));
    }
    ScrolledText {
        text,
        nowrap: false,
    }
}

fn press<A: App>(lp: &mut AppLoop<A>, target: &mut OffscreenTarget, pos: geom::Point) {
    let button = ui_core::PointerButton::Primary;
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    lp.dispatch(ui_core::Event::PointerPressed { pos, button });
    lp.frame(target, None);
    lp.dispatch(ui_core::Event::PointerReleased { pos, button });
    lp.frame(target, None);
}

fn down<A: App>(lp: &mut AppLoop<A>, target: &mut OffscreenTarget) {
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Down,
    });
    lp.frame(target, None);
}

/// The scroller's content height and viewport height, and the travel they leave.
fn travel(lp: &AppLoop<ScrolledText>) -> f32 {
    let nid = lp.tree().find_by_id(Id::str("scroller")).unwrap();
    let node = lp.tree().get(nid).unwrap();
    let extent = node.scroll_extent.unwrap().h;
    (extent - node.rect.size.h).max(0.0)
}

#[test]
fn a_caret_step_keeps_its_line_visible_instead_of_the_blocks_edge() {
    // Forty lines in a 120-tall viewport: the whole leaf's rect starts at the top, so
    // revealing the node would show the block's first line however far the caret went.
    let mut lp = AppLoop::new(numbered_lines(40));
    let mut target = OffscreenTarget::new(180, 120);
    lp.frame(&mut target, None);
    let top = lp.tree().find_by_id(Id::str("view")).unwrap();
    let origin = lp.tree().get(top).unwrap().visual_rect().origin;
    press(
        &mut lp,
        &mut target,
        geom::Point::new(origin.x + 4.0, origin.y + 4.0),
    );

    // The first lines are already on screen; the margin does not pull them.
    for _ in 0..3 {
        down(&mut lp, &mut target);
    }
    assert!(
        lp.scroll_offset(Id::str("scroller"))
            .is_none_or(|o| o.y == 0.0),
        "a caret still inside the viewport must not move the scroller"
    );

    let travel = travel(&lp);
    assert!(
        travel > 0.0,
        "the test needs content taller than the viewport"
    );
    // An intermediate line moves the viewport by an intermediate amount. Without that
    // check the final assertion could be satisfied by the focus move's whole-node reveal,
    // which rolls a tall block straight to its bottom edge.
    let mut middle = None;
    for step in 1..=12 {
        down(&mut lp, &mut target);
        let offset = lp.scroll_offset(Id::str("scroller")).unwrap_or_default().y;
        if offset > 0.0 {
            middle = Some((step, offset));
            break;
        }
    }
    let (step, offset) = middle.expect("a caret below the viewport must scroll the view");
    assert!(
        step < 8 && offset < travel,
        "a middle line must scroll part of the way, got step {step} offset {offset} of {travel}"
    );

    for _ in 0..80 {
        down(&mut lp, &mut target);
    }
    let offset = lp.scroll_offset(Id::str("scroller")).unwrap_or_default().y;
    assert!(
        (offset - travel).abs() < 0.5,
        "the caret's last line must sit at the viewport's bottom, got {offset} of {travel}"
    );
    assert!(
        lp.caret().is_some_and(|(_, at)| at > 200),
        "the caret must have walked to the end of the document"
    );
}

/// A press that only focuses the document must not move the viewport. The input's node is
/// the whole document, so revealing it drags an edge of the content into view although the
/// line the user pressed is already on screen; the caret's own line is the reveal.
#[test]
fn a_press_that_focuses_the_document_keeps_a_visible_line_still() {
    let mut lp = AppLoop::new(numbered_lines(40));
    let mut target = OffscreenTarget::new(180, 120);
    lp.frame(&mut target, None);
    // Scroll by hand first: no caret exists yet, so the wheel is the only mover.
    let over = geom::Point::new(90.0, 60.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos: over });
    lp.dispatch(ui_core::Event::wheel(over, 200.0));
    lp.frame(&mut target, None);
    let scrolled = lp.scroll_offset(Id::str("scroller")).unwrap_or_default().y;
    assert!(scrolled > 0.0, "the wheel must move the viewport");

    // A line inside the viewport now: the press lands where the user pointed.
    press(&mut lp, &mut target, geom::Point::new(40.0, 60.0));
    let after = lp.scroll_offset(Id::str("scroller")).unwrap_or_default().y;
    assert!(
        (after - scrolled).abs() < 0.5,
        "a press on a visible line must not scroll, got {after} of {scrolled}"
    );
}

#[test]
fn a_hand_scrolled_view_is_not_pulled_back_to_a_still_caret() {
    // The caret reveals once. Afterwards a wheel is the user's, and a frame with no caret
    // or buffer change must leave the offset exactly where the wheel put it.
    let mut lp = AppLoop::new(numbered_lines(40));
    let mut target = OffscreenTarget::new(180, 120);
    lp.frame(&mut target, None);
    let top = lp.tree().find_by_id(Id::str("view")).unwrap();
    let origin = lp.tree().get(top).unwrap().visual_rect().origin;
    press(
        &mut lp,
        &mut target,
        geom::Point::new(origin.x + 4.0, origin.y + 4.0),
    );
    for _ in 0..80 {
        down(&mut lp, &mut target);
    }
    let bottom = lp.scroll_offset(Id::str("scroller")).unwrap().y;

    let over = geom::Point::new(90.0, 60.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos: over });
    lp.dispatch(ui_core::Event::wheel(over, -40.0));
    lp.frame(&mut target, None);
    let wheeled = bottom - 40.0;
    assert!((lp.scroll_offset(Id::str("scroller")).unwrap().y - wheeled).abs() < 0.5);
    lp.frame(&mut target, None);
    assert!(
        (lp.scroll_offset(Id::str("scroller")).unwrap().y - wheeled).abs() < 0.5,
        "a still caret must not undo the wheel"
    );
}

#[test]
fn an_unwrapped_line_scrolls_sideways_for_its_caret() {
    let mut lp = AppLoop::new(ScrolledText {
        text: "x".repeat(400),
        nowrap: true,
    });
    let mut target = OffscreenTarget::new(180, 120);
    lp.frame(&mut target, None);
    let top = lp.tree().find_by_id(Id::str("view")).unwrap();
    let origin = lp.tree().get(top).unwrap().visual_rect().origin;
    press(
        &mut lp,
        &mut target,
        geom::Point::new(origin.x + 4.0, origin.y + 4.0),
    );
    assert!(
        lp.scroll_offset(Id::str("scroller"))
            .is_none_or(|o| o.x == 0.0),
        "the start of the line is already visible"
    );
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::End,
    });
    lp.frame(&mut target, None);
    lp.frame(&mut target, None);
    let nid = lp.tree().find_by_id(Id::str("scroller")).unwrap();
    let node = lp.tree().get(nid).unwrap();
    let travel = (node.scroll_extent.unwrap().w - node.rect.size.w).max(0.0);
    let x = lp.scroll_offset(Id::str("scroller")).unwrap_or_default().x;
    assert!(
        travel > 0.0,
        "the test needs a line wider than its viewport"
    );
    assert!(
        (x - travel).abs() < 0.5,
        "the caret at the line's end meets the viewport's right edge, got {x} of {travel}"
    );
}

#[test]
fn a_scroll_reports_damage_within_the_frame() {
    // A scroller's content is as tall as it is; only the part the viewport shows can be
    // repainted, and the rest must not reach the presenter as damage.
    let mut lp = AppLoop::new(ScrollApp);
    let mut target = OffscreenTarget::new(100, 100);
    lp.frame(&mut target, None);
    let pos = geom::Point::new(50.0, 50.0);
    lp.dispatch(ui_core::Event::wheel(pos, 30.0));
    let out = lp.frame(&mut target, None);
    let frame = geom::Rect::new(0.0, 0.0, 100.0, 100.0);
    let ui_core::Damage::Rects(rects) = out.damage else {
        panic!("a scroll damages the content, got {:?}", out.damage);
    };
    assert!(!rects.is_empty(), "the scroll moved the content");
    for rect in rects {
        assert_eq!(
            frame.intersection(rect),
            Some(rect),
            "damage reaches pixels the frame cannot paint: {rect:?}"
        );
    }
}

/// A column of text nodes far taller than its viewport: every row but the first few is out
/// of sight, and none of them needs glyphs.
struct TallText(usize);

fn tall_paragraph(index: usize) -> Decl {
    use ui_core::{NodeData, NodeKind, TextAlign, TextBlock, TextRun, TextStyle};
    Decl {
        id: Id::of(("tall-para", index)),
        kind: NodeKind::Text,
        shapes: Vec::new(),
        data: NodeData::Text {
            ink: geom::Color::BLACK,
            block: std::sync::Arc::new(TextBlock {
                runs: vec![TextRun::new(
                    format!("row {index} of a column taller than its viewport"),
                    TextStyle::new(14.0),
                )],
                align: TextAlign::Start,
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
            width: ui_core::layout::AxisSize::hug(0.0, 180.0),
            height: ui_core::layout::AxisSize::hug(14.0, 20.0),
            grow_x: ui_core::layout::Grow::No,
            grow_y: ui_core::layout::Grow::No,
        },
        ink_margin: geom::Insets::ZERO,
        box_model: ui_core::layout::BoxModel::default(),
        offset: geom::Point::new(0.0, 0.0),
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

impl App for TallText {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let mut content = Decl::container(Id::str("tall-content"), NodeKind::Column)
            .size(SizeClass::hug(0.0, 0.0));
        for index in 0..self.0 {
            content = content.child(tall_paragraph(index));
        }
        // The scroller is not the root: the root's own clip is not pushed, and the clip is
        // what this test is about.
        crate::from_decl(
            Decl::container(Id::str("tall-root"), NodeKind::Column)
                .size(SizeClass::hug(0.0, 0.0))
                .child(
                    Decl::container(Id::str("tall-scroller"), NodeKind::Column)
                        .size(SizeClass::fixed(200.0, 100.0))
                        .sense(Sense::SCROLL_OWNED)
                        .clip(ui_core::layout::Clip {
                            x: ui_core::layout::Overflow::Hidden,
                            y: ui_core::layout::Overflow::Scroll,
                        })
                        .child(content),
                ),
        )
    }
}

#[test]
fn text_past_the_viewport_is_not_resolved() {
    // Shaping and placing every row of a long document costs a glyph per character even when
    // the row is far below the clip; only the rows that can reach the frame need glyphs.
    let mut lp = AppLoop::new(TallText(200));
    let mut target = OffscreenTarget::new(200, 100);
    lp.frame(&mut target, None);
    let glyphs = lp.draw_list().prims.glyphs.len();
    assert!(glyphs > 0, "the visible rows must resolve");
    // Four rows of ~44 glyphs fit the viewport; the other 196 must not be placed at all.
    assert!(
        glyphs < 400,
        "only the rows the clip admits are shaped, got {glyphs} glyphs"
    );
}

/// A 100×100 scroller over 1,000 of content, resting on 100-unit pages.
struct SnapApp;

impl App for SnapApp {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let content = Decl::container(Id::str("snap-content"), NodeKind::Box)
            .size(SizeClass::fixed(100.0, 1_000.0));
        crate::from_decl(
            Decl::container(Id::str("snap-scroller"), NodeKind::Column)
                .size(SizeClass::fixed(100.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .snap([0.0, 100.0])
                .child(content),
        )
    }
}

fn snap_y(lp: &AppLoop<SnapApp>) -> f32 {
    let nid = lp
        .tree()
        .find_by_id(Id::str("snap-scroller"))
        .expect("the scroller is laid out");
    lp.tree()
        .get(nid)
        .and_then(|n| n.data.params())
        .map(|p| p.scroll_y)
        .expect("the scroller is a container")
}

/// Let a wheel burst go quiet, then run the settle to its end.
fn settle(lp: &mut AppLoop<SnapApp>) {
    for _ in 0..12 {
        lp.build_with_dt((100, 100), None, 1.0 / 60.0);
    }
    for _ in 0..600 {
        if !lp.is_animating() {
            return;
        }
        lp.build_with_dt((100, 100), None, 1.0 / 60.0);
    }
    panic!("the strip never came to rest");
}

#[test]
fn a_wheel_burst_turns_one_page_and_rests_on_it() {
    let mut lp = AppLoop::new(SnapApp);
    lp.build_with_dt((100, 100), None, 1.0 / 60.0);
    let pos = geom::Point::new(50.0, 50.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos });
    // The burst scrolls freely while it lasts: a wheel has no end event, so the strip
    // cannot know this is the whole gesture until the wheel goes quiet.
    lp.dispatch(ui_core::Event::wheel(pos, 30.0));
    lp.build_with_dt((100, 100), None, 1.0 / 60.0);
    assert!(
        (snap_y(&lp) - 30.0).abs() < 1e-3,
        "the burst moves the content first, got {}",
        snap_y(&lp)
    );
    settle(&mut lp);
    assert!(
        (snap_y(&lp) - 100.0).abs() < 0.5,
        "a notch shorter than half a page still turns one, got {}",
        snap_y(&lp)
    );
    // A second burst the other way turns the previous page, from the page it rests on.
    lp.dispatch(ui_core::Event::wheel(pos, -20.0));
    settle(&mut lp);
    assert!(
        snap_y(&lp).abs() < 0.5,
        "a wheel back turns the page behind, got {}",
        snap_y(&lp)
    );
}

#[test]
fn a_released_drag_rests_on_the_nearest_page() {
    let mut lp = AppLoop::new(SnapApp);
    lp.build_with_dt((100, 100), None, 1.0 / 60.0);
    let button = ui_core::PointerButton::Primary;
    let from = geom::Point::new(50.0, 80.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos: from });
    lp.dispatch(ui_core::Event::PointerPressed { pos: from, button });
    lp.build_with_dt((100, 100), None, 1.0 / 60.0);
    // The threshold's own travel is dropped, so the second move is what moves the content
    // to 130; idle frames then let the measured speed decay to nothing, so the release has
    // no fling to project and the nearest page to it is 100.
    lp.dispatch(ui_core::Event::PointerMoved {
        pos: geom::Point::new(50.0, 60.0),
    });
    lp.build_with_dt((100, 100), None, 0.1);
    let end = geom::Point::new(50.0, -70.0);
    lp.dispatch(ui_core::Event::PointerMoved { pos: end });
    lp.build_with_dt((100, 100), None, 0.1);
    for _ in 0..12 {
        lp.build_with_dt((100, 100), None, 0.1);
    }
    assert!(
        (snap_y(&lp) - 130.0).abs() < 0.5,
        "the drag moves the content between pages, got {}",
        snap_y(&lp)
    );
    lp.dispatch(ui_core::Event::PointerReleased { pos: end, button });
    assert!(
        lp.is_animating(),
        "the release eases to the page instead of jumping to it"
    );
    settle(&mut lp);
    assert!(
        (snap_y(&lp) - 100.0).abs() < 0.5,
        "the release rests on the page nearest the finger, got {}",
        snap_y(&lp)
    );
}

/// A tall column of plain boxes inside a fixed scroller, none of them windowed.
struct PaintedRows(usize);

impl App for PaintedRows {
    fn view(&mut self, _cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn crate::Element> {
        let mut column = Decl::container(Id::str("painted-rows"), NodeKind::Column)
            .size(SizeClass::hug(0.0, 0.0));
        for row in 0..self.0 {
            let id = Id::of(("painted-row", row));
            column = column.child(Decl {
                shapes: vec![ui_core::draw::Shape::Box {
                    corners: geom::Corners::ZERO,
                    fill: geom::Color::WHITE,
                    gradient: None,
                    border: None,
                    shadow: None,
                    image: None,
                }],
                ..Decl::container(id, NodeKind::Box).size(SizeClass::fixed(200.0, 20.0))
            });
        }
        crate::from_decl(
            Decl::container(Id::str("painted-scroller"), NodeKind::Column)
                .size(SizeClass::fixed(200.0, 100.0))
                .sense(Sense::SCROLL_OWNED)
                .clip(layout::Clip {
                    x: layout::Overflow::Hidden,
                    y: layout::Overflow::Scroll,
                })
                .child(column),
        )
    }
}

#[test]
fn a_row_below_the_viewport_paints_no_primitive() {
    let mut lp = AppLoop::new(PaintedRows(300));
    lp.build_with_dt((200, 100), None, 1.0 / 60.0);
    let painted = lp.draw_list().prims.rects.len();
    assert!(
        painted < 20,
        "a 300-row column paints only what its 100 dp viewport shows, painted {painted}"
    );
}
