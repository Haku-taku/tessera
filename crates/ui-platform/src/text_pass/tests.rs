//! Tests for this module, split from the source file; one module per contract area.

mod contracts {
    use crate::text_pass::*;
    use ui_core::NodeId;
    use ui_core::geom::{Color, Rect};

    /// The family a style gets when it names none.
    const DEFAULT_FAMILY: Family = Family::Generic(GenericFamily::SystemUi);

    /// An unchanged text primitive is reused; anything that changes it resolves again.
    ///
    /// Reuse is what makes a still frame cheap, and the exact comparison in `shape_cached` is
    /// what keeps it honest — so both halves are checked here.
    #[test]
    fn an_unchanged_text_primitive_is_reused_and_a_changed_one_is_not() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let list = text_list("Hello", 24.0);
        let first = pass.resolve(&list, 1.0);
        let resolved = pass.resolve_misses();
        assert!(resolved > 0, "the first pass has nothing to reuse");

        let again = pass.resolve(&list, 1.0);
        assert_eq!(
            pass.resolve_misses(),
            resolved,
            "an unchanged primitive must be reused"
        );
        assert_eq!(
            first.prims.glyphs, again.prims.glyphs,
            "reuse must reproduce the glyphs"
        );
        assert_eq!(first.ops, again.ops, "and the op order that places them");

        // Ink is deliberately not in the key: only the exact comparison catches it. This has
        // to come before the scale change, which rasterizes at a new size and restamps the atlas.
        let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
        let mut recoloured = text_list("Hello", 24.0);
        recoloured.prims.texts[0].ink = red;
        let painted = pass.resolve(&recoloured, 1.0);
        assert!(
            painted.prims.glyphs.iter().all(|g| g.color == red),
            "a recoloured label must not reuse the previous ink"
        );

        // The scale reaches glyph placement, so it is part of what makes a primitive its own.
        let before_scale = pass.resolve_misses();
        pass.resolve(&list, 2.0);
        assert!(
            pass.resolve_misses() > before_scale,
            "a different scale must resolve again"
        );
    }

    /// A drag reveals syntax construct by construct, and each revealed delimiter can be a
    /// glyph the atlas has never held at that phase. Rasterizing one must not reshape the
    /// texts that did not change: on a document page that is the whole frame.
    #[test]
    fn a_new_glyph_does_not_invalidate_the_shaped_text_cache() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let mut plain = text_list("Hello", 24.0);
        plain.prims.texts[0].origin.y = 0.0;
        let mut novel = text_list("Hello \u{1e9e}", 24.0);
        novel.prims.texts[0].origin.y = 40.0;

        pass.resolve(&plain, 1.0);
        pass.resolve(&novel, 1.0);
        let settled = pass.resolve_misses();
        pass.resolve(&plain, 1.0);
        assert_eq!(
            pass.resolve_misses(),
            settled,
            "a glyph rasterized elsewhere must not resolve this primitive again"
        );
    }

    /// Tracking is an advance, not a paint-only offset: it widens the measured line by one
    /// spacing per cluster — the last one included, as CSS counts it — and wraps sooner.
    #[test]
    fn letter_spacing_widens_the_measured_line() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let plain = ui_core::TextStyle::new(24.0);
        let tracking = 6.0;
        let text = "abcd ef";
        let flat = pass.layout(&ui_core::TextBlock::plain(text, plain), f32::INFINITY);
        let wide = pass.layout(
            &ui_core::TextBlock::plain(text, plain.tracking(tracking)),
            f32::INFINITY,
        );
        let clusters = text.chars().count() as f32;
        assert!(
            (wide.width - (flat.width + tracking * clusters)).abs() < 0.01,
            "every cluster takes one spacing: {} vs {}",
            wide.width,
            flat.width + tracking * clusters
        );
        let one_line = pass.layout(&ui_core::TextBlock::plain(text, plain), flat.width + 1.0);
        let wrapped = pass.layout(
            &ui_core::TextBlock::plain(text, plain.tracking(tracking)),
            flat.width + 1.0,
        );
        assert_eq!(one_line.lines.len(), 1, "without tracking the line fits");
        assert!(
            wrapped.lines.len() > 1,
            "the tracking must reach the wrap, not just the paint"
        );
    }

    /// A hidden run keeps its bytes for offsets and takes no room: the paragraph measures
    /// and wraps as if it were absent, every caret offset inside it sits where the visible
    /// text starts, and no glyph is drawn for it.
    #[test]
    fn a_hidden_run_takes_no_room_and_leaves_every_offset_where_it_was() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let style = ui_core::TextStyle::new(24.0);
        let marked = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("**", style).hidden(),
                ui_core::TextRun::new("bold", style),
                ui_core::TextRun::new("**", style).hidden(),
            ],
            ..Default::default()
        };
        let plain = ui_core::TextBlock::plain("bold", style);
        let marked_width = pass.layout(&marked, f32::INFINITY).width;
        let plain_width = pass.layout(&plain, f32::INFINITY).width;
        assert!(
            (marked_width - plain_width).abs() < 0.01,
            "hidden markers must not widen the line: {marked_width} against {plain_width}"
        );

        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(marked.clone()),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let prim = list.prims.texts[0].clone();
        let x = |pass: &mut TextPass, at: usize| {
            pass.caret_rect(&prim, at, 1.0)
                .expect("the block has a caret line")
                .origin
                .x
        };
        let inside_opening = (x(&mut pass, 0), x(&mut pass, 1), x(&mut pass, 2));
        assert_eq!(
            inside_opening.0, inside_opening.1,
            "a caret inside the opening marker sits where it starts"
        );
        assert_eq!(
            inside_opening.0, inside_opening.2,
            "and the marker's end is the text's start"
        );
        let after_text = x(&mut pass, 6);
        assert!(
            after_text > inside_opening.0,
            "the text occupies the room the markers gave up"
        );
        assert_eq!(
            after_text,
            x(&mut pass, 8),
            "a caret past the closing marker sits where the text ends"
        );

        let hidden_glyphs = pass.resolve(&list, 1.0).prims.glyphs.len();
        let plain_glyphs = pass
            .resolve(&styled_list("bold", style), 1.0)
            .prims
            .glyphs
            .len();
        assert_eq!(
            hidden_glyphs, plain_glyphs,
            "asterisks must not be shaped or drawn"
        );
    }

    /// A roomed run takes the width it declares, not its glyphs', and lays them by its
    /// alignment: a marker's bytes and the stand-in drawn over them share one column.
    #[test]
    fn a_roomed_run_keeps_its_width_and_aligns_its_glyphs() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let style = ui_core::TextStyle::new(24.0);
        let room = 60.0;
        let block = |align| ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("1. ", style).roomed(room, align),
                ui_core::TextRun::new("body", style),
            ],
            ..Default::default()
        };
        let plain = ui_core::TextBlock::plain("body", style);
        let plain_width = pass.layout(&plain, f32::INFINITY).width;
        let ended = block(ui_core::TextAlign::End);
        let width = pass.layout(&ended, f32::INFINITY).width;
        assert!(
            (width - (room + plain_width)).abs() < 0.5,
            "the line is the room plus the text beside it: {width}"
        );

        let marker_x = |pass: &mut TextPass, align| {
            let text = block(align);
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
            list.push_text(
                NodeId::from_raw(1, 0),
                TextPrim {
                    block: std::rc::Rc::new(text),
                    origin: ui_core::geom::Point::new(0.0, 0.0),
                    ink: Color::WHITE,
                    max_width: f32::INFINITY,
                    layout: None,
                    height: 0.0,
                    caret: None,
                    selection: None,
                    composing: None,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
            pass.resolve(&list, 1.0)
                .prims
                .glyphs
                .iter()
                .map(|glyph| glyph.rect.origin.x)
                .fold(f32::MAX, f32::min)
        };
        let start_x = marker_x(&mut pass, ui_core::TextAlign::Start);
        let end_x = marker_x(&mut pass, ui_core::TextAlign::End);
        assert!(
            end_x > start_x + 1.0,
            "an end-aligned room pushes the marker right: {end_x} against {start_x}"
        );
        assert!(
            end_x + room * 0.5 > room,
            "and it stays inside the room its bytes share"
        );

        // Both edges of the room are caret positions, at the room's own width.
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(2, 0),
            TextPrim {
                block: std::rc::Rc::new(ended),
                origin: ui_core::geom::Point::new(0.0, 0.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let prim = list.prims.texts[0].clone();
        let x = |pass: &mut TextPass, at: usize| {
            let rect = pass
                .caret_rect(&prim, at, 1.0)
                .expect("the block has a caret line");
            rect.origin.x + rect.size.w * 0.5
        };
        let (start, inside, stop) = (x(&mut pass, 0), x(&mut pass, 2), x(&mut pass, 3));
        assert_eq!(start, 0.0, "the room starts at the run's own origin");
        assert!(
            inside > start && inside < stop,
            "the marker's bytes keep positions inside"
        );
        assert_eq!(stop, room, "and the room ends at its declared width");
    }

    /// Revealing a hidden run takes room the measured lines did not have, so the lines of
    /// the hidden state are not the lines the shown text draws: the measured layout belongs
    /// to the runs *and* to which of them are hidden.
    #[test]
    fn a_revealed_run_wraps_again_instead_of_overflowing_the_width() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let style = ui_core::TextStyle::new(24.0);
        let body = ui_core::TextRun::new("bold text here", style);
        let width = pass
            .layout(
                &ui_core::TextBlock::plain("bold text here", style),
                f32::INFINITY,
            )
            .width
            + 2.0;
        let hidden = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("**", style).hidden(),
                body.clone(),
                ui_core::TextRun::new("**", style).hidden(),
            ],
            ..Default::default()
        };
        let shown = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("**", style),
                body,
                ui_core::TextRun::new("**", style),
            ],
            ..Default::default()
        };
        let hidden_lines = pass.layout(&hidden, width).lines.len();
        let shown_layout = pass.layout(&shown, width);
        assert_eq!(
            hidden_lines, 1,
            "the hidden markers leave the text on one line"
        );
        assert!(
            shown_layout.lines.len() > 1,
            "the shown markers must wrap, not run past the width"
        );
        assert!(
            shown_layout
                .lines
                .iter()
                .all(|line| line.width <= width + 0.5),
            "no line may draw wider than the block"
        );
    }

    /// A line a hard break leaves empty still names where it sits, so the caret can land
    /// on it; taking the line's bytes from its runs would collapse it to offset zero and
    /// put that caret on the last line.
    #[test]
    fn an_empty_line_from_a_hard_break_keeps_its_offset() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let block = ui_core::TextBlock::plain("a\n\nb", ui_core::TextStyle::new(24.0));
        let layout = pass.layout(&block, f32::INFINITY);
        let ranges: Vec<_> = layout.lines.iter().map(|line| line.range()).collect();
        assert_eq!(
            ranges,
            [0..1, 2..2, 3..4],
            "three lines, the middle one empty at its own offset"
        );
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 200.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 0.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: Some(layout),
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let prim = list.prims.texts[0].clone();
        let y = |pass: &mut TextPass, at: usize| {
            pass.caret_rect(&prim, at, 1.0)
                .expect("the block has a caret line")
                .origin
                .y
        };
        let (first, middle, last) = (y(&mut pass, 0), y(&mut pass, 2), y(&mut pass, 3));
        assert!(
            first < middle && middle < last,
            "the caret at 2 belongs to the empty middle line: {first}, {middle}, {last}"
        );
    }

    /// A cut inside a grapheme shapes like one at the cluster's edge: the mark stays with
    /// its base. Cutting there shapes the mark alone, which in an RTL run is UAX #9 L3's
    /// "marks precede their base" and drops the accent in any other direction.
    #[test]
    fn a_cut_inside_a_cluster_puts_the_mark_with_its_base() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let style = ui_core::TextStyle::new(40.0);
        let with = |first: &str, second: &str| ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new(first, style),
                ui_core::TextRun::new(second, style),
            ],
            ..Default::default()
        };
        // "éx": the acute is its own run, with the boundary inside the cluster and at its edge.
        let inside = with("e", "\u{301}x");
        let edge = with("e\u{301}", "x");
        let (inside_flat, inside_spans) = inside.flatten();
        let (edge_flat, edge_spans) = edge.flatten();
        assert_eq!(
            inside_spans
                .iter()
                .map(|s| s.range.clone())
                .collect::<Vec<_>>(),
            edge_spans
                .iter()
                .map(|s| s.range.clone())
                .collect::<Vec<_>>(),
            "an inside cut takes the cluster's style, like a cut at its edge"
        );
        let measure = |pass: &mut TextPass, text: &str, spans: &[ui_core::Span]| {
            pass.measure_styled(text, spans, 0, text.len(), false)
        };
        let inside_width = measure(&mut pass, &inside_flat, &inside_spans);
        let edge_width = measure(&mut pass, &edge_flat, &edge_spans);
        assert!(
            (inside_width - edge_width).abs() < 0.01,
            "the mark must shape with its base: {inside_width} vs {edge_width}"
        );
        let plain = ui_core::TextBlock::plain("e\u{301}x", style);
        let shaped = |pass: &mut TextPass,
                      block: &ui_core::TextBlock,
                      selected: Option<std::ops::Range<usize>>| {
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 600.0, 100.0));
            list.push_text(
                NodeId::from_raw(1, 0),
                TextPrim {
                    block: std::rc::Rc::new(block.clone()),
                    origin: ui_core::geom::Point::new(10.0, 20.0),
                    ink: Color::WHITE,
                    max_width: f32::INFINITY,
                    layout: None,
                    height: 0.0,
                    caret: None,
                    selection: selected.map(|range| ui_core::selection::SelectionPrim {
                        range,
                        fill: Color::rgba(0.0, 0.0, 1.0, 1.0),
                        painter: Some(ui_core::selection::SelectionPainterRef(&INVERTS)),
                    }),
                    composing: None,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
            pass.resolve(&list, 1.0).prims.glyphs.len()
        };
        assert_eq!(
            shaped(&mut pass, &inside, None),
            shaped(&mut pass, &edge, None),
            "the mark must not be painted as a glyph of its own"
        );
        assert_eq!(
            shaped(&mut pass, &plain, Some(0..1)),
            shaped(&mut pass, &plain, Some(0..3)),
            "a selection edge inside the cluster must take the whole cluster"
        );
    }

    /// An RTL line keeps its word order across a style change: the logical first byte
    /// sits at the right edge, whichever run it belongs to.
    #[test]
    fn an_rtl_line_keeps_its_order_across_a_style_change() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let block = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new(
                    "\u{645}\u{631}\u{62d}\u{628}\u{627}",
                    ui_core::TextStyle::new(32.0),
                ),
                ui_core::TextRun::new(
                    "\u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}",
                    ui_core::TextStyle::new(32.0),
                ),
            ],
            ..Default::default()
        };
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 600.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let prim = list.prims.texts[0].clone();
        let prep = pass.prepare(&prim);
        if prep.layout.lines.is_empty() {
            return;
        }
        let bounds = pass.boundaries(&prim, &prep, 0, 1.0);
        let first = bounds.iter().min_by_key(|b| b.byte).expect("boundaries").x;
        let last = bounds.iter().max_by_key(|b| b.byte).expect("boundaries").x;
        assert!(
            first > last,
            "RTL text runs right to left: the first byte sits at {first}, the last at {last}"
        );
    }

    /// A line with no strong character of its own takes the paragraph's direction: the same
    /// "123!" reads with the bang after the digits in a left-to-right paragraph and before
    /// them in a right-to-left one, because the neutral takes the base level.
    #[test]
    fn a_line_without_strong_characters_follows_its_paragraph() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let prim = |text: &str| {
            let block = ui_core::TextBlock {
                runs: vec![ui_core::TextRun::new(text, ui_core::TextStyle::new(24.0))],
                ..Default::default()
            };
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 600.0, 100.0));
            list.push_text(
                NodeId::from_raw(1, 0),
                TextPrim {
                    block: std::rc::Rc::new(block),
                    origin: ui_core::geom::Point::new(10.0, 20.0),
                    ink: Color::WHITE,
                    max_width: 600.0,
                    layout: None,
                    height: 0.0,
                    caret: None,
                    selection: None,
                    composing: None,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
            list.prims.texts[0].clone()
        };
        // Where the byte at `at` sits on the second line, which is the "123!" line.
        let x_at = |pass: &mut TextPass, text: &str, at: usize| {
            let prim = prim(text);
            let prep = pass.prepare(&prim);
            assert_eq!(
                prep.layout.lines.len(),
                2,
                "the block must hold a paragraph line and a digit line"
            );
            let bounds = pass.boundaries(&prim, &prep, 1, 1.0);
            bounds
                .iter()
                .find(|b| b.byte == at)
                .map(|b| b.x)
                .unwrap_or_else(|| panic!("byte {at} has no boundary in {text:?}"))
        };
        // "abc\n123!": the bang is byte 7, the 3 is byte 6.
        let (bang, three) = (
            x_at(&mut pass, "abc\n123!", 7),
            x_at(&mut pass, "abc\n123!", 6),
        );
        assert!(bang > three, "a left-to-right paragraph ends ...3!");
        // "שלום\n123!": the bang is byte 12, the 3 is byte 11.
        let (bang, three) = (
            x_at(&mut pass, "שלום\n123!", 12),
            x_at(&mut pass, "שלום\n123!", 11),
        );
        assert!(
            bang < three,
            "the paragraph's base level puts the neutral bang first, got {bang} against {three}"
        );
    }

    /// A run's own ink paints its glyphs and stops at the run boundary — the mechanism a link
    /// or an inline code span inside a paragraph is built on.
    #[test]
    fn a_runs_ink_paints_only_its_own_glyphs() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
        let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
        let block = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("ab", ui_core::TextStyle::new(24.0)).inked(red),
                ui_core::TextRun::new("cd", ui_core::TextStyle::new(24.0)).inked(blue),
            ],
            ..Default::default()
        };
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let paint = pass.resolve(&list, 1.0);
        let mut glyphs = paint.prims.glyphs.clone();
        assert_eq!(glyphs.len(), 4, "four letters, four glyphs");
        glyphs.sort_by(|a, b| a.rect.origin.x.total_cmp(&b.rect.origin.x));
        assert!(
            glyphs[..2].iter().all(|g| g.color == red),
            "the first run must keep its ink"
        );
        assert!(
            glyphs[2..].iter().all(|g| g.color == blue),
            "and the second run its own"
        );
    }

    /// A background is the run's own claim: it tints that run's glyphs and stops at the
    /// neighbour, rather than covering the paragraph the way a selection does.
    #[test]
    fn a_runs_background_covers_only_its_own_glyphs() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let tint = Color::rgba(1.0, 1.0, 0.0, 1.0);
        let block = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("ab", ui_core::TextStyle::new(24.0)),
                ui_core::TextRun::new("cd", ui_core::TextStyle::new(24.0)).on_background(tint),
            ],
            ..Default::default()
        };
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let paint = pass.resolve(&list, 1.0);
        let tinted: Vec<_> = paint
            .prims
            .rects
            .iter()
            .filter(|r| r.fill == tint)
            .collect();
        assert_eq!(tinted.len(), 1, "one run on one line is one rect");
        let rect = tinted[0].rect;
        let mut glyphs = paint.prims.glyphs.clone();
        assert_eq!(glyphs.len(), 4, "four letters, four glyphs");
        glyphs.sort_by(|a, b| a.rect.origin.x.total_cmp(&b.rect.origin.x));
        let centre = |g: &ui_core::GlyphPrim| g.rect.origin.x + g.rect.size.w * 0.5;
        assert!(
            rect.origin.x < centre(&glyphs[2]) && centre(&glyphs[3]) < rect.origin.x + rect.size.w,
            "the tint spans the run it was given"
        );
        assert!(
            centre(&glyphs[0]) < rect.origin.x && centre(&glyphs[1]) < rect.origin.x,
            "and none of the untinted run"
        );
        assert!(
            rect.origin.y <= glyphs[0].rect.origin.y
                && glyphs[0].rect.origin.y + glyphs[0].rect.size.h <= rect.origin.y + rect.size.h,
            "it is the line box, so the glyphs sit inside it"
        );
    }

    /// An underline sits under the baseline and a strikeout above it, each over its own
    /// run: a decoration placed from the line box instead would miss the face entirely.
    #[test]
    fn decorations_follow_the_face_and_stay_inside_their_run() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
        let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
        let block = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("ab", ui_core::TextStyle::new(24.0))
                    .inked(red)
                    .underlined(),
                ui_core::TextRun::new("cd", ui_core::TextStyle::new(24.0))
                    .inked(blue)
                    .struck_through(),
            ],
            ..Default::default()
        };
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let paint = pass.resolve(&list, 1.0);
        let rule = |ink: Color| {
            let hits: Vec<_> = paint
                .prims
                .rects
                .iter()
                .filter(|r| r.fill == ink && r.rect.size.h <= 4.0)
                .collect();
            assert_eq!(hits.len(), 1, "one run on one line is one line of rule");
            hits[0].rect
        };
        let underline = rule(red);
        let strikeout = rule(blue);
        let mut glyphs = paint.prims.glyphs.clone();
        assert_eq!(glyphs.len(), 4, "four letters, four glyphs");
        glyphs.sort_by(|a, b| a.rect.origin.x.total_cmp(&b.rect.origin.x));
        let centre = |g: &ui_core::GlyphPrim| g.rect.origin.x + g.rect.size.w * 0.5;
        assert!(
            underline.origin.x < centre(&glyphs[0])
                && centre(&glyphs[1]) < underline.origin.x + underline.size.w
                && centre(&glyphs[2]) > underline.origin.x + underline.size.w,
            "the underline spans the run that asked for it"
        );
        let letters_bottom = glyphs[..2]
            .iter()
            .fold(f32::MIN, |lo, g| lo.max(g.rect.origin.y + g.rect.size.h));
        assert!(
            underline.origin.y >= letters_bottom,
            "an underline starts at or below the foot of its letters"
        );
        let letters_top = glyphs[2..]
            .iter()
            .fold(f32::MAX, |hi, g| hi.min(g.rect.origin.y));
        assert!(
            strikeout.origin.y > letters_top
                && strikeout.origin.y < glyphs[3].rect.origin.y + glyphs[3].rect.size.h,
            "a strikeout crosses the letters rather than sitting under them"
        );
    }

    /// A painter that inverts the selected text, so the rule's ink can be told apart from
    /// the run's own.
    #[derive(Debug)]
    struct Inverts;

    impl ui_core::selection::SelectionPainter for Inverts {
        fn paint(
            &self,
            geometry: &ui_core::SelectionGeometry<'_>,
        ) -> ui_core::selection::SelectionPaint {
            let mut paint = ui_core::flat_selection(geometry);
            paint.ink = Some(Color::rgba(0.0, 1.0, 0.0, 1.0));
            paint
        }
    }

    static INVERTS: Inverts = Inverts;

    /// A selection cuts a link's rule where it starts: the selected half takes the
    /// selection's ink and the rest keeps the link's, instead of one rule in one colour.
    #[test]
    fn a_selection_cuts_a_decoration_at_its_edges() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let link = Color::rgba(1.0, 0.0, 0.0, 1.0);
        let inverted = Color::rgba(0.0, 1.0, 0.0, 1.0);
        let block = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("abcd", ui_core::TextStyle::new(24.0))
                    .inked(link)
                    .underlined(),
            ],
            ..Default::default()
        };
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: Some(ui_core::selection::SelectionPrim {
                    range: 2..4,
                    fill: Color::rgba(0.0, 0.0, 1.0, 1.0),
                    painter: Some(ui_core::selection::SelectionPainterRef(&INVERTS)),
                }),
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let paint = pass.resolve(&list, 1.0);
        let rule = |ink: Color| {
            let hits: Vec<_> = paint
                .prims
                .rects
                .iter()
                .filter(|r| r.fill == ink && r.rect.size.h <= 4.0)
                .collect();
            assert_eq!(hits.len(), 1, "one line, one rule in this ink");
            hits[0].rect
        };
        let kept = rule(link);
        let cut = rule(inverted);
        let boundary = kept.origin.x + kept.size.w;
        assert!(
            (boundary - cut.origin.x).abs() < 1.0,
            "the rule changes ink exactly where the selection starts"
        );
        assert!(
            boundary > kept.origin.x && cut.size.w > 0.0,
            "both halves carry a rule, none is dropped"
        );
        assert!(
            (kept.origin.y - cut.origin.y).abs() < 0.5,
            "the two halves are one rule at one height"
        );
    }

    /// A justified paragraph spreads each full line's leftover space across its break
    /// opportunities, so every line but the last ends at the block's edge; the geometry
    /// that places the glyphs is the geometry a caret reads.
    #[test]
    fn a_justified_paragraph_fills_its_width_and_leaves_the_last_line_flush() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let text = "the quick brown fox jumps over the lazy dog again and again";
        let style = ui_core::TextStyle::new(24.0);
        let width = 320.0;
        let prim = |text: &str, align| TextPrim {
            block: std::rc::Rc::new(ui_core::TextBlock {
                runs: vec![ui_core::TextRun::new(text, style)],
                align,
                ..Default::default()
            }),
            origin: ui_core::geom::Point::new(0.0, 0.0),
            ink: Color::WHITE,
            max_width: width,
            layout: None,
            height: 0.0,
            caret: None,
            selection: None,
            composing: None,
            zoom: None,
            shape_scale: 1.0,
        };
        let plain = prim(text, ui_core::TextAlign::Start);
        let stretched = prim(text, ui_core::TextAlign::Justify);
        let plain_prep = pass.prepare(&plain);
        let just_prep = pass.prepare(&stretched);
        let lines = plain_prep.layout.lines.len();
        assert!(
            lines >= 2,
            "the sample must wrap into a full line and a last one, got {lines}"
        );
        assert_eq!(
            just_prep.layout.lines.len(),
            lines,
            "alignment must not change the wrap"
        );
        let end_of = |pass: &mut TextPass, text: &TextPrim, prep: &Prepared, index: usize| {
            pass.boundaries(text, prep, index, 1.0)
                .last()
                .map_or(0.0, |b| b.x)
        };
        for index in 0..lines {
            let before = end_of(&mut pass, &plain, &plain_prep, index);
            let after = end_of(&mut pass, &stretched, &just_prep, index);
            if index + 1 < lines {
                let extra = (width - plain_prep.layout.lines[index].width).max(0.0);
                assert!(
                    (after - before - extra).abs() < 1.0,
                    "line {index} must grow by the space it had left: {} against {extra}",
                    after - before
                );
            } else {
                assert!(
                    (after - before).abs() < 0.01,
                    "the last line keeps its own width: {after} against {before}"
                );
            }
        }
        // Cutting each line at its gaps must place the same glyphs, none lost or repeated,
        // and none pushed past the block.
        let paint = |pass: &mut TextPass, text: &TextPrim| {
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 400.0));
            list.push_text(NodeId::from_raw(1, 0), text.clone());
            pass.resolve(&list, 1.0).prims.glyphs.clone()
        };
        let plain_glyphs = paint(&mut pass, &plain);
        let just_glyphs = paint(&mut pass, &stretched);
        assert_eq!(
            plain_glyphs.len(),
            just_glyphs.len(),
            "a line cut at its gaps must keep every glyph once"
        );
        assert!(
            just_glyphs
                .iter()
                .all(|g| g.rect.origin.x + g.rect.size.w <= width + 2.0),
            "a stretched line must stay inside the block it fills"
        );
        // A forced break closes a line the way the paragraph's end does: the words before a
        // newline are left flush instead of being spread across the row.
        let broken = "a line of words that ends at a forced break\nshort tail";
        let broken_plain = prim(broken, ui_core::TextAlign::Start);
        let broken_just = prim(broken, ui_core::TextAlign::Justify);
        let broken_plain_prep = pass.prepare(&broken_plain);
        let broken_just_prep = pass.prepare(&broken_just);
        let broken_lines = broken_plain_prep.layout.lines.len();
        assert!(
            broken_lines >= 3,
            "the sample needs a wrapped part and a tail after the break, got {broken_lines}"
        );
        let before_break = broken_lines - 2;
        assert!(
            (end_of(&mut pass, &broken_just, &broken_just_prep, before_break)
                - end_of(&mut pass, &broken_plain, &broken_plain_prep, before_break))
            .abs()
                < 0.01,
            "the line before a forced break keeps its own width"
        );
        assert!(
            end_of(&mut pass, &broken_just, &broken_just_prep, 0)
                > end_of(&mut pass, &broken_plain, &broken_plain_prep, 0) + 1.0,
            "the wrapped lines above it are still justified"
        );
    }

    /// A held widget paints itself through the ordinary node walker; the text pass leaves
    /// its atom blank instead of drawing the faint placeholder an unresolved embed gets.
    #[test]
    fn a_holder_atom_paints_no_placeholder() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let rects = |pass: &mut TextPass, holder: Option<ui_core::Id>| {
            let mut block = ui_core::TextBlock::plain(
                format!("a{}b", ui_core::text::OBJECT),
                ui_core::TextStyle::new(24.0),
            );
            let mut atom = ui_core::text::AtomBox::new(1, ui_core::geom::Size::new(30.0, 20.0));
            if let Some(id) = holder {
                atom = atom.holder(id);
            }
            block.atoms = vec![atom];
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
            list.push_text(
                NodeId::from_raw(1, 0),
                TextPrim {
                    block: std::rc::Rc::new(block),
                    origin: ui_core::geom::Point::new(10.0, 20.0),
                    ink: Color::WHITE,
                    max_width: f32::INFINITY,
                    layout: None,
                    height: 0.0,
                    caret: None,
                    selection: None,
                    composing: None,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
            pass.resolve(&list, 1.0).prims.rects.len()
        };
        assert_eq!(
            rects(&mut pass, Some(ui_core::Id::str("chip"))),
            0,
            "a held widget is not a hole in the text"
        );
        assert_eq!(
            rects(&mut pass, None),
            1,
            "an atom with neither object nor holder still draws its placeholder"
        );
    }

    /// A family set after the pass was built reaches layout: the host's font choice is
    /// runtime state, and every answer cached under the old stack must go with it.
    #[test]
    fn a_family_set_at_runtime_reaches_layout() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        // The two bundled faces disagree on line height (1.0 em against 1.21 em), which
        // makes the face in force readable from a measurement without needing coverage.
        let latin = ui_core::TextBlock {
            runs: vec![ui_core::TextRun::new("MMMM", ui_core::TextStyle::new(24.0))],
            ..Default::default()
        };
        let icons: std::sync::Arc<[u8]> =
            include_bytes!("../../../assets/fonts/FluentSystemIcons-Regular.ttf")
                .as_slice()
                .into();
        let inter: std::sync::Arc<[u8]> = include_bytes!("../../../assets/fonts/InterVariable.ttf")
            .as_slice()
            .into();
        assert!(
            pass.register("Runtime Icons", icons),
            "the bytes must register"
        );
        assert!(
            pass.register("Runtime Latin", inter),
            "the bytes must register"
        );
        pass.set_families(&["Runtime Icons"]);
        let under_icons = pass.measure(&latin, f32::INFINITY).h;
        pass.set_families(&["Runtime Latin"]);
        let under_latin = pass.measure(&latin, f32::INFINITY).h;
        // A measure that loads a face is cached under the version it started with, so it
        // takes a second one to leave the answer keyed to the stack now in force.
        pass.measure(&latin, f32::INFINITY);
        if under_icons == under_latin {
            eprintln!("skipped: the two faces agree on line height");
            return;
        }
        // Both files are loaded by now, so nothing but dropping the cached answer can
        // make this last switch visible.
        pass.set_families(&["Runtime Icons"]);
        assert_eq!(
            pass.measure(&latin, f32::INFINITY).h,
            under_icons,
            "a family set at runtime reaches layout instead of the cached answer"
        );
        assert_eq!(pass.families().len(), 1);
        assert_eq!(pass.families()[0], "Runtime Icons");
    }

    /// A segment the breaker hands over may cross a style change; both styles must count.
    #[test]
    fn a_segment_measured_across_a_style_change_counts_both_sizes() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let block = ui_core::TextBlock {
            runs: vec![
                ui_core::TextRun::new("aa", ui_core::TextStyle::new(16.0)),
                ui_core::TextRun::new("aa", ui_core::TextStyle::new(64.0)),
            ],
            ..Default::default()
        };
        let (flat, spans) = block.flatten();
        let small = pass.measure_range(&flat, &spans, &[], 0..2, false);
        let large = pass.measure_range(&flat, &spans, &[], 2..4, false);
        let mixed = pass.measure_range(&flat, &spans, &[], 0..4, false);
        assert!(large > small * 2.0, "the larger style must measure wider");
        assert!(
            (mixed - (small + large)).abs() < 0.01,
            "the segment is its pieces: {mixed} vs {} + {}",
            small,
            large
        );
    }

    /// A line opened by a hard break is a line box, not a 1 px sliver.
    #[test]
    fn a_line_after_a_hard_break_is_as_tall_as_the_text() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let block = ui_core::TextBlock::plain("a\n", ui_core::TextStyle::new(20.0));
        let layout = pass.layout(&block, 200.0);
        assert_eq!(
            layout.lines.len(),
            2,
            "the trailing break opens a second line"
        );
        assert_eq!(
            layout.lines[1].height, layout.lines[0].height,
            "the empty line takes the same box as the text around it"
        );
    }

    /// A block whose every run is empty still owns one line box, as tall as its text:
    /// a blank source line reserves a line, so the document's blank lines do not collapse.
    #[test]
    fn an_empty_block_is_one_line_box_of_its_own_style() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let style = ui_core::TextStyle::new(20.0);
        let blank = ui_core::TextBlock {
            runs: vec![ui_core::TextRun::new("", style)],
            ..Default::default()
        };
        let blank = pass.layout(&blank, 200.0);
        assert_eq!(
            blank.lines.len(),
            1,
            "the empty block still owns a caret line"
        );
        let text = pass.layout(&ui_core::TextBlock::plain("x", style), 200.0);
        assert_eq!(
            blank.lines[0].height, text.lines[0].height,
            "an empty line takes the box its own style asks for"
        );
    }

    /// An atom's box decides the wrap, so a changed box must not reuse the old measurement.
    #[test]
    fn a_measured_block_key_notices_its_atoms() {
        let mut small = ui_core::TextBlock::plain("abc\u{fffc}def", ui_core::TextStyle::new(20.0));
        small.atoms = vec![ui_core::AtomBox::new(
            3,
            ui_core::geom::Size::new(4.0, 20.0),
        )];
        let mut large = small.clone();
        large.atoms = vec![ui_core::AtomBox::new(
            3,
            ui_core::geom::Size::new(180.0, 20.0),
        )];
        assert_ne!(
            measure_key(&small, 200.0, 1),
            measure_key(&large, 200.0, 1),
            "the atom's box is part of what the wrap depends on"
        );
    }

    fn text_list(text: &str, size: f32) -> DrawList {
        styled_list(text, ui_core::TextStyle::new(size))
    }

    fn styled_list(text: &str, style: ui_core::TextStyle) -> DrawList {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(ui_core::TextBlock::plain(text, style)),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        list
    }

    /// A formula's glyphs rasterize into the same atlas as text, so a pass whose last
    /// primitive is a formula must still ask for the page to be uploaded. Checking only
    /// inside text resolution left a newly typed character blank until something else
    /// rasterized.
    #[test]
    fn a_formula_rasterized_after_the_last_text_uploads_the_atlas() {
        let mut pass = TextPass::new();
        let Some(family) = pass.default_math_family() else {
            eprintln!("skipped: no installed math face");
            return;
        };
        let Ok(formula) = pass.layout_math(&family, "x", 16.0, false, None) else {
            eprintln!("skipped: the math face refused the sample");
            return;
        };
        // The text settles the atlas, and its face need not cover the formula's glyph.
        let text = text_list("hello", 16.0);
        pass.resolve(&text, 1.0);
        let before = pass.version();

        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_math(
            NodeId::from_raw(2, 0),
            ui_core::draw::MathPrim {
                formula: std::rc::Rc::new(formula),
                family: family.into(),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        pass.resolve(&list, 1.0);
        assert!(
            pass.version() > before,
            "a formula's rasterized glyph must reach the renderer"
        );
    }

    #[test]
    fn a_bold_run_resolves_a_different_face_than_a_regular_one() {
        // The styled stack is what makes weight reach shaping at all; if it returns the
        // same faces, every run rasterises Regular outlines regardless of the request.
        let mut pass = TextPass::with_stack(&["DejaVu Sans"]);
        let regular = pass.stack_for(DEFAULT_FAMILY, ui_core::Weight::REGULAR, false);
        let bold = pass.stack_for(DEFAULT_FAMILY, ui_core::Weight::BOLD, false);
        if regular.is_empty() {
            eprintln!("skipped: no system fonts");
            return;
        }
        assert_ne!(
            regular, bold,
            "a bold run must not reuse the regular face stack"
        );
        // The cache must be stable: asking twice cannot drift.
        assert_eq!(
            pass.stack_for(DEFAULT_FAMILY, ui_core::Weight::BOLD, false),
            bold
        );
    }

    #[test]
    fn the_default_style_shapes_in_the_platforms_ui_face() {
        // The default style names `SystemUi`. A base stack built from the generic sans
        // instead served every app Arial on Windows while the style asked for the UI face.
        // The system provider: `new` is isolated, and an isolated pass has no stack at all.
        let mut pass = TextPass::with_stack(&[]);
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let ui = pass
            .provider
            .borrow()
            .resolve(&FontRequest::new(GenericFamily::SystemUi));
        let stack = pass.stack_for(DEFAULT_FAMILY, ui_core::Weight::REGULAR, false);
        assert_eq!(
            ui,
            stack.first().copied(),
            "the base stack must begin with the platform's UI face"
        );
    }

    #[test]
    fn an_icon_run_does_not_poison_the_latin_stack() {
        // PUA icons read as Script::LATIN, so a face cache keyed by script alone let the
        // icon face answer for later Latin text — the font's ligature component glyphs.
        let icon = || {
            styled_list(
                "\u{e88a}",
                ui_core::TextStyle::new(24.0).named("Material Symbols Outlined"),
            )
        };
        let latin = || styled_list("Hamburgefonstiv", ui_core::TextStyle::new(16.0));

        let mut shared = TextPass::new();
        if !shared.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        if shared.resolve(&icon(), 1.0).prims.glyphs.is_empty() {
            eprintln!("skipped: Material Symbols Outlined is not installed");
            return;
        }
        let pen = |list: DrawList, pass: &mut TextPass| -> Vec<f32> {
            pass.resolve(&list, 1.0)
                .prims
                .glyphs
                .iter()
                .map(|g| g.rect.origin.x)
                .collect()
        };
        let after_icon = pen(latin(), &mut shared);
        let mut fresh = TextPass::new();
        let alone = pen(latin(), &mut fresh);
        assert_eq!(
            after_icon, alone,
            "an icon run must not change how Latin is shaped"
        );
    }

    #[test]
    fn resolve_replaces_text_with_glyphs() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            return;
        }
        let out = pass.resolve(&text_list("Hi", 24.0), 1.0);
        assert!(out.prims.texts.is_empty(), "text must be consumed");
        assert_eq!(out.prims.glyphs.len(), 2, "one glyph per visible char");
        assert_eq!(pass.unresolved(), 0);
    }

    #[test]
    fn a_selection_across_a_wrap_reports_one_fragment_per_line() {
        // One rect per visual line piece: a painter must never have to re-break the text,
        // and a selection that spans a wrap must cover both lines.
        let mut pass = TextPass::with_stack(&["Tahoma"]);
        let block =
            ui_core::TextBlock::plain("alpha bravo charlie delta", ui_core::TextStyle::new(20.0));
        let natural = pass.measure(&block, f32::INFINITY).w;
        if natural <= 0.0 {
            eprintln!("skipped: Tahoma is not available");
            return;
        }
        let len = block.flatten().0.len();
        let prim = TextPrim {
            block: std::rc::Rc::new(block),
            origin: ui_core::geom::Point::new(10.0, 20.0),
            ink: Color::WHITE,
            max_width: natural * 0.5,
            layout: None,
            height: 0.0,
            caret: None,
            selection: None,
            composing: None,
            zoom: None,
            shape_scale: 1.0,
        };
        let frags = pass.selection_fragments(&prim, 0..len, 1.0);
        assert!(
            frags.len() >= 2,
            "the selection must span at least two lines, got {}",
            frags.len()
        );
        assert!(frags[0].first, "the first fragment must say so");
        assert!(!frags[0].last, "one fragment cannot be both ends");
        assert!(frags.last().is_some_and(|f| f.last));
        assert!(
            frags[1].rect.origin.y > frags[0].rect.origin.y,
            "the second fragment must sit on the next line"
        );
        for fragment in &frags {
            assert!(
                fragment.rect.size.w > 0.0,
                "a fragment with no width would paint nothing"
            );
        }
    }

    /// A line break draws no glyph, so a selection that covers one would paint nothing for
    /// it. The pass gives it a small block past the line's last cluster, and only when the
    /// break is selected whole.
    /// An empty line paints no ink and still owns a line box: a selection that takes its
    /// break draws a block on that line, as it does on any other line's break.
    #[test]
    fn an_empty_lines_selected_break_gets_a_block() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let style = ui_core::TextStyle::new(20.0);
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(ui_core::TextBlock {
                    runs: vec![ui_core::TextRun::new("", style)],
                    ..Default::default()
                }),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::BLACK,
                max_width: f32::INFINITY,
                layout: None,
                height: 0.0,
                caret: None,
                selection: Some(ui_core::SelectionPrim {
                    range: 0..1,
                    fill: Color::WHITE,
                    painter: None,
                }),
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let out = pass.resolve(&list, 1.0);
        assert!(
            out.prims
                .rects
                .iter()
                .any(|rect| rect.fill == Color::WHITE && rect.rect.size.h > 0.0),
            "the empty line's selected break draws a block: {:?}",
            out.prims.rects
        );
    }

    #[test]
    fn a_selected_line_break_gets_a_block_past_the_line() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let block = ui_core::TextBlock::plain("alpha\nbravo", ui_core::TextStyle::new(20.0));
        if pass.measure(&block, f32::INFINITY).w <= 0.0 {
            eprintln!("skipped: the system stack shapes nothing");
            return;
        }
        let len = block.flatten().0.len();
        let prim = TextPrim {
            block: std::rc::Rc::new(block),
            origin: ui_core::geom::Point::new(0.0, 0.0),
            ink: Color::WHITE,
            max_width: f32::INFINITY,
            layout: None,
            height: 0.0,
            caret: None,
            selection: None,
            composing: None,
            zoom: None,
            shape_scale: 1.0,
        };
        let blocks =
            |frags: &[ui_core::SelectionFragment]| frags.iter().filter(|f| f.line_break).count();
        let marked = pass.selection_fragments(&prim, 0..len, 1.0);
        assert_eq!(blocks(&marked), 1, "the one break inside the text");
        let mark = marked
            .iter()
            .find(|f| f.line_break)
            .expect("the break's block");
        assert_eq!(mark.line, 0, "the break belongs to the line it ends");
        let line = marked
            .iter()
            .find(|f| !f.line_break && f.line == 0)
            .expect("the first line's glyphs");
        assert!(
            (mark.rect.size.h - line.rect.size.h).abs() < 0.5
                && mark.rect.size.w > 0.0
                && mark.rect.size.w < mark.rect.size.h,
            "a narrow block as tall as its line: {:?} against {:?}",
            mark.rect,
            line.rect
        );
        let plain = pass.selection_fragments(&prim, 0..5, 1.0);
        assert_eq!(blocks(&plain), 0, "the break itself is not selected");
        let trailing = pass.selection_fragments(&prim, 0..len + 1, 1.0);
        assert_eq!(
            blocks(&trailing),
            2,
            "the break inside the text and the break the range reaches past its end"
        );
    }

    #[test]
    fn an_emoji_in_a_line_reaches_the_colour_atlas() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let list = text_list("hi\u{1F600}there", 24.0);
        let out = pass.resolve(&list, 1.0);
        let colour = out
            .prims
            .glyphs
            .iter()
            .filter(|g| g.kind == ui_core::draw::GlyphKind::Color)
            .count();
        if pass.color_atlas().is_empty() {
            eprintln!("skipped: no colour emoji face is installed");
            return;
        }
        assert!(
            colour > 0,
            "the emoji must become a colour glyph, got {} glyphs and no colour entry",
            out.prims.glyphs.len()
        );
        // The other characters must stay on the coverage path.
        assert!(
            out.prims
                .glyphs
                .iter()
                .any(|g| g.kind == ui_core::draw::GlyphKind::Coverage),
            "Latin characters must not be redirected to the colour page"
        );
    }

    /// A digit written after a keycap is not part of it: shaping the whole run on one emoji
    /// face made it resolve to an empty keycap component, which draws nothing.
    #[test]
    fn a_digit_after_a_keycap_keeps_its_own_glyph() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let list = text_list("3\u{fe0f}\u{20e3}4", 24.0);
        let out = pass.resolve(&list, 1.0);
        if pass.color_atlas().is_empty() {
            eprintln!("skipped: no colour emoji face is installed");
            return;
        }
        let digits: Vec<_> = out
            .prims
            .glyphs
            .iter()
            .filter(|g| g.kind == ui_core::draw::GlyphKind::Coverage)
            .collect();
        assert!(
            !digits.is_empty(),
            "the trailing digit must keep a coverage glyph"
        );
        assert!(
            digits.iter().any(|g| g.rect.size.w > 0.0),
            "the trailing digit must draw with a width, got {:?}",
            digits.iter().map(|g| g.rect.size.w).collect::<Vec<_>>()
        );
    }

    /// A display-list line is placed at the formula's origin and baseline, so a
    /// `\cancel` stroke lands on the body it crosses.
    #[test]
    fn a_formula_line_is_placed_on_the_baseline() {
        let origin = ui_core::geom::Point::new(10.0, 20.0);
        let line = math_line(origin, 32.0, [1.0, 2.0, 30.0, 40.0], 1.5, Color::WHITE);
        assert_eq!(line.from, ui_core::geom::Point::new(11.0, 34.0));
        assert_eq!(line.to, ui_core::geom::Point::new(40.0, 72.0));
        assert_eq!(line.thickness, 1.5);
        assert_eq!(line.colour, Color::WHITE);
    }

    /// A formula paints through the same atlas as text: its glyph ids become glyph prims on
    /// the node's baseline and its fraction bar becomes a rect inside the formula's box.
    #[test]
    fn a_formula_resolves_to_glyphs_on_its_baseline_and_a_bar() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let family = pass
            .family_names()
            .into_iter()
            .find(|name| name.to_lowercase().contains("math"))
            .expect("a system family with a MATH table; install one such as DejaVu Math TeX Gyre");
        let formula = pass
            .layout_math(&family, r"\frac{x}{y}", 16.0, false, None)
            .expect("the family lays a formula out");

        let mut list = DrawList::new(Rect::new(0.0, 0.0, 200.0, 100.0));
        list.push_math(
            NodeId::from_raw(1, 0),
            ui_core::draw::MathPrim {
                formula: std::rc::Rc::new(formula.clone()),
                family: family.clone().into(),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let out = pass.resolve(&list, 1.0);

        assert_eq!(
            out.prims.maths.len(),
            0,
            "a resolved formula leaves no math prim behind"
        );
        assert!(
            out.prims.glyphs.len() >= 2,
            "numerator and denominator become glyphs"
        );
        let bar = out
            .prims
            .rects
            .first()
            .expect("the fraction bar is a rect")
            .rect;
        assert!(
            (bar.size.w - formula.width).abs() < 0.01,
            "the bar spans the formula: {} vs {}",
            bar.size.w,
            formula.width
        );
        let baseline = 20.0 + formula.ascent;
        assert!(bar.origin.y < baseline, "the bar sits above the baseline");
        for glyph in &out.prims.glyphs {
            assert!(
                glyph.rect.origin.x >= 8.0 && glyph.rect.origin.x <= 10.0 + formula.width + 2.0,
                "a glyph stays within the formula's width: {:?}",
                glyph.rect
            );
        }

        // A subscript hangs below the baseline; the sign of the display list's y is what
        // puts it there, and a flipped one would lift it into the superscript position.
        // `n` is used because its ink sits flat on the baseline, so any ink below the
        // baseline can only be the script's.
        let sub = pass
            .layout_math(&family, "n_2", 16.0, false, None)
            .expect("a subscript lays out");
        let mut subscript = DrawList::new(Rect::new(0.0, 0.0, 200.0, 100.0));
        subscript.push_math(
            NodeId::from_raw(2, 0),
            ui_core::draw::MathPrim {
                origin: ui_core::geom::Point::new(10.0, 20.0),
                formula: std::rc::Rc::new(sub.clone()),
                family: family.into(),
                ink: Color::WHITE,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let resolved = pass.resolve(&subscript, 1.0);
        let bottom = |g: &&ui_core::draw::GlyphPrim| g.rect.origin.y + g.rect.size.h;
        let left = resolved
            .prims
            .glyphs
            .iter()
            .min_by(|a, b| a.rect.origin.x.total_cmp(&b.rect.origin.x))
            .expect("the base and its script are glyphs");
        let right = resolved
            .prims
            .glyphs
            .iter()
            .max_by(|a, b| a.rect.origin.x.total_cmp(&b.rect.origin.x))
            .expect("the base and its script are glyphs");
        assert!(
            bottom(&right) > bottom(&left) + 1.0,
            "the subscript sits lower than its base: {} vs {}",
            bottom(&right),
            bottom(&left)
        );
    }

    /// An atom filled with a formula paints the formula where the atom reserved its box, and
    /// leaves the placeholder alone: an atom with no object still paints its faint block, so
    /// the two cases must not be confused.
    #[test]
    fn an_inline_formula_paints_in_place_of_the_placeholder() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let family = pass
            .family_names()
            .into_iter()
            .find(|name| name.to_lowercase().contains("math"))
            .expect("a system family with a MATH table; install one such as DejaVu Math TeX Gyre");
        let formula = pass
            .layout_math(&family, "x^2", 14.0, false, None)
            .expect("the family lays a formula out");
        let object = ui_core::InlineObject::Math {
            formula: formula.clone(),
            family,
            ink: Color::WHITE,
        };
        let atom = ui_core::AtomBox::new(2, object.size());
        let line = |atom: ui_core::AtomBox| {
            let mut block = ui_core::TextBlock::plain(
                format!("ab{}cd", ui_core::OBJECT),
                ui_core::TextStyle::new(14.0),
            );
            block.atoms.push(atom);
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
            list.push_text(
                NodeId::from_raw(4, 0),
                TextPrim {
                    block: std::rc::Rc::new(block),
                    origin: ui_core::geom::Point::new(10.0, 20.0),
                    ink: Color::WHITE,
                    max_width: f32::INFINITY,
                    layout: None,
                    height: 0.0,
                    caret: None,
                    selection: None,
                    composing: None,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
            list
        };
        let plain = pass.resolve(&line(atom.clone()), 1.0);
        assert_eq!(
            plain.prims.rects.len(),
            1,
            "an empty atom reserves its box with a block"
        );
        let filled = pass.resolve(&line(atom.object(object)), 1.0);
        assert!(
            filled.prims.rects.is_empty(),
            "the formula draws no placeholder block: {:?}",
            filled.prims.rects
        );
        assert!(
            filled.prims.glyphs.len() > plain.prims.glyphs.len(),
            "the formula adds its own glyphs: {} vs {}",
            filled.prims.glyphs.len(),
            plain.prims.glyphs.len()
        );
    }

    /// A block with no glyphs is resolved rather than sent to the fallback, so its caret
    /// paints: an empty line, and a window of spaces the breaker trims off the line range.

    #[test]
    fn a_block_without_glyphs_still_draws_its_caret() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let block = ui_core::TextBlock::plain("", ui_core::TextStyle::new(20.0));
        let layout = pass.layout(&block, f32::INFINITY);
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(1, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width: f32::INFINITY,
                height: layout.height,
                layout: Some(layout),
                caret: Some(ui_core::draw::Caret {
                    offset: 0,
                    color: Color::WHITE,
                }),
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let out = pass.resolve(&list, 1.0);
        let rects: Vec<Rect> = out.prims.rects.iter().map(|rect| rect.rect).collect();
        assert_eq!(
            rects.len(),
            1,
            "the caret is the empty block's only ink: {rects:?}"
        );
        // The caret is centred on the insertion point, which is the line's own start.
        assert_eq!(rects[0].origin.x + rects[0].size.w * 0.5, 10.0);
        assert!(rects[0].size.h > 0.0, "and spans the blank line's box");
        assert_eq!(
            pass.unresolved(),
            0,
            "an empty block is resolved, not a missing face"
        );

        // The same gate, with a byte range instead of none: a markdown row's indent cell is
        // spaces, and the caret can stand anywhere inside them, each space owning its room.
        let blank = ui_core::TextBlock::plain("  ", ui_core::TextStyle::new(20.0));
        let blank_layout = pass.layout(&blank, f32::INFINITY);
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        for offset in 0..=2 {
            list.push_text(
                NodeId::from_raw(2 + offset as u32, 0),
                TextPrim {
                    block: std::rc::Rc::new(blank.clone()),
                    origin: ui_core::geom::Point::new(10.0, 20.0),
                    ink: Color::WHITE,
                    max_width: f32::INFINITY,
                    height: blank_layout.height,
                    layout: Some(blank_layout.clone()),
                    caret: Some(ui_core::draw::Caret {
                        offset,
                        color: Color::WHITE,
                    }),
                    selection: None,
                    composing: None,
                    zoom: None,
                    shape_scale: 1.0,
                },
            );
        }
        let out = pass.resolve(&list, 1.0);
        let mut centres: Vec<f32> = out
            .prims
            .rects
            .iter()
            .map(|rect| rect.rect.origin.x + rect.rect.size.w * 0.5)
            .collect();
        assert_eq!(centres.len(), 3, "one caret per offset: {centres:?}");
        centres.sort_by(f32::total_cmp);
        assert_eq!(centres[0], 10.0, "the first stands at the line's start");
        assert!(
            centres[0] < centres[1] && centres[1] < centres[2],
            "each space owns room: {centres:?}"
        );
        assert_eq!(
            pass.unresolved(),
            0,
            "spaces are resolved, not a missing face"
        );
    }

    /// A gutter numbers source lines, not the lines a wrap opened: one source line broken
    /// into several carries one number, and every number ends left of the code it numbers.
    #[test]
    fn a_gutter_numbers_source_lines_not_wrapped_ones() {
        let mut pass = TextPass::new();
        if !pass.has_face() {
            eprintln!("skipped: no system fonts");
            return;
        }
        let style = ui_core::TextStyle::new(16.0)
            .family(ui_core::Family::Generic(ui_core::GenericFamily::Monospace));
        let advance = pass
            .layout(&ui_core::TextBlock::plain("a", style), f32::INFINITY)
            .width;
        let gutter = advance * 3.0;
        let mut block = ui_core::TextBlock::plain("aaaa aaaa aaaa\nbb", style);
        block.indent = gutter;
        block.hanging = gutter;
        block.line_numbers = Some(ui_core::text::LineNumbers {
            start: 1,
            ink: Color::BLACK,
            size: 16.0,
            gap: advance,
        });
        let max_width = gutter + advance * 5.5;
        let layout = pass.layout(&block, max_width);
        assert!(
            layout.lines.len() >= 3,
            "the sample's first line wraps: {} lines",
            layout.lines.len()
        );
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 400.0, 100.0));
        list.push_text(
            NodeId::from_raw(9, 0),
            TextPrim {
                block: std::rc::Rc::new(block),
                origin: ui_core::geom::Point::new(10.0, 20.0),
                ink: Color::WHITE,
                max_width,
                height: layout.height,
                layout: Some(layout),
                caret: None,
                selection: None,
                composing: None,
                zoom: None,
                shape_scale: 1.0,
            },
        );
        let out = pass.resolve(&list, 1.0);
        let numbers: Vec<Rect> = out
            .prims
            .glyphs
            .iter()
            .filter(|glyph| glyph.color == Color::BLACK)
            .map(|glyph| glyph.rect)
            .collect();
        assert_eq!(
            numbers.len(),
            2,
            "two source lines take two numbers, however many lines they wrapped into: {numbers:?}"
        );
        for number in &numbers {
            assert!(
                number.origin.x >= 10.0,
                "the number stays inside the gutter: {number:?}"
            );
            assert!(
                number.origin.x + number.size.w <= 10.0 + gutter + 0.5,
                "and ends before the code it numbers: {number:?}"
            );
        }
    }
}

mod editing {
    use crate::text_pass::TextPass;
    use ui_core::{EditKey, Id};

    /// A face with a MATH table on this machine, or the reason to skip.
    fn math_family(pass: &mut TextPass) -> Option<String> {
        let family = pass.default_math_family();
        if family.is_none() {
            eprintln!("skipped: no installed math face");
        }
        family
    }

    /// A keystroke edits the model the pass paints: source, display list and caret all come
    /// from the one layout the editor holds.
    #[test]
    fn a_key_edits_the_formula_the_pass_paints() {
        let mut pass = TextPass::new();
        let Some(family) = math_family(&mut pass) else {
            return;
        };
        let id = Id::str("math-edit");
        assert!(pass.math_edit_attach(ui_core::MathAttach {
            id,
            family: &family,
            source: "x",
            size: 16.0,
            display: false,
            frame: 1,
            owner: None,
        }));
        assert!(pass.math_edit_key(id, EditKey::Character('+'), false));

        assert_eq!(pass.math_edit_source(id).as_deref(), Some("x+"));
        let formula = pass
            .math_edit_formula(id, None)
            .expect("the edited source lays out");
        assert!(formula.width > 0.0, "the edit reaches the display list");
        let caret = pass
            .math_edit_caret(id)
            .expect("the editor reports its caret");
        assert!(caret.x > 0.0, "the caret followed the edit: {caret:?}");
    }

    /// The app's copy catching up is not an external change, but a source the editor did
    /// not write is: it replaces the model and drops the history. An editor not attached
    /// for a frame is freed.
    #[test]
    fn an_external_source_replaces_the_model_and_an_unattached_editor_is_freed() {
        let mut pass = TextPass::new();
        let Some(family) = math_family(&mut pass) else {
            return;
        };
        let id = Id::str("math-edit");
        assert!(pass.math_edit_attach(ui_core::MathAttach {
            id,
            family: &family,
            source: "x",
            size: 16.0,
            display: false,
            frame: 1,
            owner: None,
        }));
        assert!(pass.math_edit_key(id, EditKey::Character('+'), false));

        // The app now hands back the source the editor itself wrote.
        assert!(pass.math_edit_attach(ui_core::MathAttach {
            id,
            family: &family,
            source: "x+",
            size: 16.0,
            display: false,
            frame: 1,
            owner: None,
        }));
        assert_eq!(pass.math_edit_source(id).as_deref(), Some("x+"));

        // A source the app chose on its own replaces the model.
        assert!(pass.math_edit_attach(ui_core::MathAttach {
            id,
            family: &family,
            source: "y",
            size: 16.0,
            display: false,
            frame: 1,
            owner: None,
        }));
        assert_eq!(pass.math_edit_source(id).as_deref(), Some("y"));
        assert!(
            !pass.math_edit_undo(id),
            "an external change drops the history"
        );

        // A later frame that does not attach the same id frees it.
        assert!(pass.math_edit_attach(ui_core::MathAttach {
            id: Id::str("other"),
            family: &family,
            source: "z",
            size: 16.0,
            display: false,
            frame: 3,
            owner: None,
        }));
        assert!(!pass.math_edit_is_editing(id), "the stale editor is freed");
    }
}
