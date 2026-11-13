// Self-test checks for the in-place editor: the form each construct takes while the caret
// stands where it does, the geometry the two forms share, and edits that reach the app's own
// copy of the document.

use crate::selftest::*;

pub(crate) fn markdown_edit(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    goto(lp, size, Page::Markdown);
    let root = markdown_edit_root();
    let text = root.with(0usize).with("text");

    // The editor is one input over the whole source; a block draws a window onto it.
    let value = input_value(lp, root);
    let whole = value.as_deref() == Some(lp.app_mut().markdown_src.as_str());
    // Unfocused, the heading's `# ` is a hidden run: still in the buffer, drawn nowhere.
    let hidden = runs(lp, text).is_some_and(|runs| {
        runs.iter().any(|(text, hidden)| *hidden && text == "# ")
            && visible(&runs) == "Markdown, placed"
    });

    let Some(rect) = rect_of(lp, text) else {
        println!("selftest: markdown heading block missing");
        return false;
    };
    // Away from the caret a fenced block is a card: a head, the code without fences, a foot.
    // Head and foot take the taller of their two states, so entering the block moves nothing.
    let document = ui_richtext::parse(&lp.app_mut().markdown_src);
    let code_index = document
        .blocks
        .iter()
        .position(|block| matches!(block.kind, ui_richtext::BlockKind::CodeBlock { .. }));
    let card = code_index.is_some_and(|index| {
        let block_id = root.with(index);
        let code = block_id.with("text");
        let header = block_id.with("header");
        let fence_text = header.with("fence");
        let (Some(code_rect), Some(block_rect)) = (rect_of(lp, code), rect_of(lp, block_id)) else {
            return false;
        };
        // Away from the caret the head shows the language and copy; the fence text keeps its
        // room there so the head cannot change height, but draws nothing.
        let away_header = runs(lp, header.with("language"))
            .is_some_and(|runs| visible(&runs) == "Rust")
            && opaque(lp, header.with("chrome"))
            && runs(lp, fence_text).is_some_and(|runs| visible(&runs).is_empty());
        // The foot names the code's line count, from the catalog rather than a literal, and
        // the note takes room: a `nowrap` leaf measured against a closed hint is zero wide
        // and its label spills out of the card. The foot's band is the chip's height, so the
        // chip lands inside the card rather than hanging off its bottom edge.
        let count = block_id.with("footer").with("count");
        let chip = block_id.with("footer").with("count-box");
        let bottom = block_rect.origin.y + block_rect.size.h;
        let foot = runs(lp, count).is_some_and(|runs| visible(&runs).contains('1'))
            && rect_of(lp, count).is_some_and(|rect| rect.size.w > 0.0)
            && rect_of(lp, chip).is_some_and(|rect| {
                rect.origin.y >= block_rect.origin.y && rect.origin.y + rect.size.h <= bottom + 0.5
            });
        // The card's side inset, which the editing view must keep too.
        let inset = (code_rect.origin.x - block_rect.origin.x - 12.0).abs() < 0.5;
        let away_height = block_rect.size.h;
        let away_x = code_rect.origin.x;
        click(
            lp,
            size,
            Point::new(
                code_rect.origin.x + 4.0,
                code_rect.origin.y + code_rect.size.h * 0.5,
            ),
        );
        let entered = runs(lp, fence_text)
            .is_some_and(|runs| visible(&runs).starts_with("```rust"))
            && !opaque(lp, header.with("chrome"));
        let steady = rect_of(lp, block_id).is_some_and(|now| {
            (now.size.h - away_height).abs() < 0.5
                && rect_of(lp, code).is_some_and(|code| (code.origin.x - away_x).abs() < 0.5)
        });
        away_header && foot && inset && entered && steady
    });

    // Copy hands over the code itself: the fence bytes stay out, and the card returns once
    // the caret leaves the block for the paragraph above it.
    let copied = code_index.is_some_and(|index| {
        let Some(paragraph) = rect_of(lp, root.with(1usize).with("text")) else {
            return false;
        };
        click(
            lp,
            size,
            Point::new(paragraph.origin.x + 8.0, paragraph.origin.y + 4.0),
        );
        let copy = root.with(index).with("header").with("copy");
        let Some(copy_rect) = rect_of(lp, copy) else {
            return false;
        };
        click(
            lp,
            size,
            Point::new(
                copy_rect.origin.x + copy_rect.size.w * 0.5,
                copy_rect.origin.y + copy_rect.size.h * 0.5,
            ),
        );
        let expected = document
            .source
            .get(document.blocks[index].span.clone())
            .unwrap_or("");
        lp.take_clipboard_commands().iter().any(|command| {
            matches!(
                command,
                ui_platform::clipboard::ClipboardCommand::Copy { content, .. }
                    if matches!(
                        content,
                        ui_core::clipboard::ClipboardContent::Text(text) if text == expected
                    )
            )
        })
    });

    // The reading card is the same control with `editable` off: no buffer, no caret, the
    // same blocks in the same places, so a document reads as it will read while edited.
    let read = markdown_read_root();
    let read_heading = read.with(0usize).with("text");
    let read_buffer = input_value(lp, read);
    // The reading card never reveals anything: the heading's `# ` stays in the buffer as a
    // hidden run, and only the text it marks is drawn.
    let read_marker = runs(lp, read_heading).is_some_and(|runs| {
        runs.iter().any(|(text, hidden)| *hidden && text == "# ")
            && visible(&runs) == "Markdown, placed"
    });
    let same_heading = rect_of(lp, read_heading).is_some_and(|read| {
        (read.size.h - rect.size.h).abs() < 0.5 && (read.origin.x - rect.origin.x).abs() < 0.5
    });
    let same_paragraph = match (
        rect_of(lp, read.with(1usize).with("text")),
        rect_of(lp, root.with(1usize).with("text")),
    ) {
        (Some(read), Some(editing)) => {
            (read.size.h - editing.size.h).abs() < 0.5
                && (read.origin.x - editing.origin.x).abs() < 0.5
                && (read.size.w - editing.size.w).abs() < 0.5
        }
        _ => false,
    };
    let read_still = match rect_of(lp, read_heading) {
        Some(rect) => {
            let before = lp.app_mut().markdown_src.clone();
            click(
                lp,
                size,
                Point::new(rect.origin.x + 8.0, rect.origin.y + rect.size.h * 0.5),
            );
            type_text(lp, size, "N");
            lp.app_mut().markdown_src == before
        }
        None => false,
    };

    // A task item's marker is a box, not text: away from the caret the item draws it where
    // the marker's ink ended, checked or not, and its text keeps the list's one column.
    let task_facts = lp
        .app_mut()
        .markdown_sample
        .as_ref()
        .and_then(|(_, document)| {
            let index = document.blocks.iter().position(|block| {
                block.children().iter().any(|item| {
                    matches!(
                        item.kind,
                        ui_richtext::BlockKind::ListItem { task: Some(_) }
                    )
                })
            })?;
            // A box names the flag byte it flips, so its id is that offset in the source.
            let flags: Vec<usize> = document.blocks[index]
                .children()
                .iter()
                .map(|item| {
                    ui_richtext::marker(document, item)
                        .and_then(|marker| {
                            document.source[marker.clone()]
                                .find('[')
                                .map(|at| marker.start + at + 1)
                        })
                        .unwrap_or(usize::MAX)
                })
                .collect();
            Some((index, flags))
        });
    let tasks = task_facts
        .map(|(index, flags)| {
            let block = read.with(index);
            let box_of = |item: usize| read.with(("task", flags[item])).with("box");
            let text_of = |item: usize| block.with(item).with(0usize).with("text");
            let room = |rect: Option<Rect>| {
                rect.is_some_and(|r| (r.size.w - 16.0).abs() < 0.5 && (r.size.h - 16.0).abs() < 0.5)
            };
            let check = |item: usize| read.with(("task", flags[item])).with("check");
            let checked = room(rect_of(lp, box_of(0))) && rect_of(lp, check(0)).is_some();
            let open = room(rect_of(lp, box_of(1))) && rect_of(lp, check(1)).is_none();
            // The bytes `[x]` are drawn only while the caret touches them: away from the
            // item its row hides them behind the box, and its own text holds neither.
            let unlabelled = runs(lp, text_of(0)).is_some_and(|runs| {
                !visible(&runs).contains('[')
                    && runs
                        .iter()
                        .any(|(text, hidden)| *hidden && text.trim_end() == "[x]")
            });
            // The box stands over the marker's room and the item's text starts past it, so
            // the two never share a column.
            let placed = (0..2).all(|item| {
                match (
                    rect_of(lp, box_of(item)),
                    rect_of(lp, text_of(item)),
                    first_glyph_left(lp, text_of(item)),
                ) {
                    (Some(boxed), Some(text), Some(first)) => {
                        first >= boxed.origin.x + boxed.size.w - 0.5
                            && boxed.origin.y >= text.origin.y - 1.0
                            && boxed.origin.y + boxed.size.h <= text.origin.y + text.size.h + 1.0
                    }
                    _ => false,
                }
            });
            checked && open && unlabelled && placed
        })
        .unwrap_or(false);

    // The editing card's box is a control: a press flips the byte in the source and leaves
    // the caret where it was. One byte changes, so the source keeps its length.
    let toggled = {
        let source = ui_richtext::parse(&lp.app_mut().markdown_src);
        let index = source.blocks.iter().position(|block| {
            block.children().iter().any(|item| {
                matches!(
                    item.kind,
                    ui_richtext::BlockKind::ListItem { task: Some(true) }
                )
            })
        });
        match index {
            Some(index) => {
                let root = markdown_edit_root();
                let flag = ui_richtext::marker(&source, &source.blocks[index].children()[0])
                    .and_then(|marker| {
                        source.source[marker.clone()]
                            .find('[')
                            .map(|at| marker.start + at + 1)
                    })
                    .unwrap_or(usize::MAX);
                let task = root.with(("task", flag));
                // The task list sits below the window's bottom edge: expose it and let the
                // scroll settle before reading geometry, or the press lands past the box.
                if let Some(boxed) = rect_of(lp, task.with("box")) {
                    reveal_page_point(lp, size, center(boxed));
                    for _ in 0..6 {
                        frame(lp, size, None);
                    }
                }
                match (
                    rect_of(lp, root.with(1usize).with("text")),
                    rect_of(lp, task.with("box")),
                ) {
                    (Some(parked), Some(boxed)) => {
                        click(
                            lp,
                            size,
                            Point::new(
                                parked.origin.x + 2.0,
                                parked.origin.y + parked.size.h * 0.5,
                            ),
                        );
                        let caret = lp.caret();
                        let before = lp.app_mut().markdown_src.clone();
                        // Parking the caret can reveal a mark above and move this row, so
                        // the box is read again, now that the page stands still.
                        let boxed = rect_of(lp, task.with("box")).unwrap_or(boxed);
                        click(lp, size, center(boxed));
                        let after = lp.app_mut().markdown_src.clone();
                        let flipped = after.len() == before.len()
                            && after
                                .bytes()
                                .zip(before.bytes())
                                .filter(|(a, b)| a != b)
                                .count()
                                == 1;
                        let unchecked = rect_of(lp, task.with("check")).is_none();
                        let held = lp.caret() == caret;
                        let restored = rect_of(lp, task.with("box")).is_some_and(|boxed| {
                            click(lp, size, center(boxed));
                            lp.app_mut().markdown_src == before
                                && rect_of(lp, task.with("check")).is_some()
                        });
                        println!(
                            "selftest: markdown task toggle flipped {flipped} unchecked {unchecked} caret held {held} restored {restored}"
                        );
                        flipped && unchecked && held && restored
                    }
                    _ => {
                        println!(
                            "selftest: markdown task box or the paragraph above missing box {} flag {} parked {}",
                            rect_of(lp, task.with("box")).is_some(),
                            flag,
                            rect_of(lp, root.with(1usize).with("text")).is_some(),
                        );
                        false
                    }
                }
            }
            None => {
                println!("selftest: markdown has no checked task item to flip");
                false
            }
        }
    };

    // Down from an item crosses to the block below it — the list nested under this one —
    // instead of leaving the control: a sibling view wins over focus navigation, which
    // knows nothing about lines.
    let source = lp.app_mut().markdown_src.clone();
    let document = ui_richtext::parse(&source);
    let list_span = document.blocks.get(2usize).map(|block| block.full.clone());
    let crossed = match (
        rect_of(lp, root.with(2usize).with(1usize).with(0usize).with("text")),
        list_span,
    ) {
        (Some(item), Some(span)) => {
            click(
                lp,
                size,
                Point::new(item.origin.x + 2.0, item.origin.y + item.size.h * 0.5),
            );
            let before = lp.caret().map(|(_, at)| at);
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Down,
            });
            frame(lp, size, None);
            match (lp.caret(), before) {
                (Some((owner, at)), Some(before)) => {
                    owner == root && at > before && at >= span.start && at <= span.end
                }
                _ => false,
            }
        }
        _ => false,
    };

    // A marker is shown only while the caret touches its own bytes: the end of the title
    // leaves the `# ` hidden, and a caret on the marker itself draws it. The rect is read
    // here: the card checks above scroll the page, which moves everything above.
    let heading = rect_of(lp, text).unwrap_or(rect);
    let heading_mid = heading.origin.y + heading.size.h * 0.5;
    click(
        lp,
        size,
        Point::new(heading.origin.x + heading.size.w - 2.0, heading_mid),
    );
    let title_hides = runs(lp, text)
        .is_some_and(|runs| !runs.iter().any(|(text, hidden)| !*hidden && text == "# "));
    click(lp, size, Point::new(heading.origin.x + 1.0, heading_mid));
    let touching = runs(lp, text)
        .is_some_and(|runs| runs.iter().any(|(text, hidden)| !*hidden && text == "# "));
    let revealed = title_hides && touching;

    // Typing lands at the end of the heading, where the edit belongs.
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::End,
    });
    frame(lp, size, None);
    let before = lp.app_mut().markdown_src.clone();
    type_text(lp, size, " 2");
    let after = lp.app_mut().markdown_src.clone();
    let edited = after.starts_with("# Markdown, placed 2\n") && after.len() == before.len() + 2;

    // An inline formula draws as its box while the caret is away and as its source once the
    // caret enters it. The bytes stay either way: the atom spans them, so no offset moves.
    let document = ui_richtext::parse(&lp.app_mut().markdown_src);
    let formula_mark = document
        .marks
        .iter()
        .find(|mark| mark.kind == ui_richtext::MarkKind::InlineMath)
        .map(|mark| mark.full.clone());
    let formula_index = document.blocks.iter().position(|block| {
        document
            .source
            .get(block.span.clone())
            .is_some_and(|text| text.contains("is a mark"))
    });
    let mut formula_box = false;
    let mut formula_source = false;
    if let (Some(id), Some(mark)) = (
        formula_index.map(|index| root.with(index).with("text")),
        formula_mark,
    ) {
        formula_box = atoms_of(lp, id).is_some_and(|lens| lens == vec!["a^2 + b^2 = c^2".len()]);
        if let Some(rect) = rect_of(lp, id) {
            click(
                lp,
                size,
                Point::new(rect.origin.x + 2.0, rect.origin.y + rect.size.h * 0.5),
            );
            for _ in 0..40 {
                lp.dispatch(ui_core::Event::KeyPressed {
                    key: ui_core::Key::Right,
                });
                frame(lp, size, None);
                let inside = lp
                    .caret()
                    .is_some_and(|(_, at)| at >= mark.start && at <= mark.end);
                if inside {
                    break;
                }
            }
            formula_source = atoms_of(lp, id).is_some_and(|lens| lens.is_empty());
        }
    }

    // Undo is the engine's, over the same one buffer: one step takes the typed run back to
    // the source before it, and redo puts it back, both reaching the app as a new buffer.
    press_ctrl(lp, size, 'z');
    let undone = lp.app_mut().markdown_src == before;
    press_ctrl(lp, size, 'y');
    let redone = lp.app_mut().markdown_src == after;
    lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
    let history = undone && redone;

    // A block that does not start at offset zero: the press and the typed byte must use
    // the document's offsets, not this block's own.
    let paragraph = match rect_of(lp, root.with(1usize).with("text")) {
        Some(rect) => rect,
        None => {
            println!("selftest: markdown paragraph view missing");
            return false;
        }
    };
    click(
        lp,
        size,
        Point::new(
            paragraph.origin.x + paragraph.size.w - 2.0,
            paragraph.origin.y + paragraph.size.h - 2.0,
        ),
    );
    type_text(lp, size, "!");
    let placed = lp
        .app_mut()
        .markdown_src
        .contains("its words.!\n\n- A bullet");

    // A quote writes `> ` on each of its lines: the bytes stay in the row's own runs, the
    // bar stands for them while the caret is away, they stand when the caret touches them,
    // and the row keeps their room either way, so the quoted text never moves.
    let source = lp.app_mut().markdown_src.clone();
    let quote_text = root.with(3usize).with(0usize).with("text");
    let bar = quote_bar_id(root, &source);
    let quote_x = text_ink_bounds(lp, quote_text).map(|(left, ..)| left);
    let prefix_away = runs(lp, quote_text)
        .is_some_and(|runs| runs.iter().any(|(text, hidden)| *hidden && text == "> "));
    let bar_away = shape_fill(lp, bar, 0).is_some_and(|fill| fill.a > 0.5);

    // Nested structures are views of the same buffer too: a press in a list item, in the
    // ordered list inside one, in a quote, on the quote's bar or on the bullet reaches it.
    let mut nested = Vec::new();
    let mut reached = true;
    let mut prefix_in_text = false;
    let mut bar_in_text = false;
    for name in ["item", "nested", "quote", "bar"] {
        let id = match name {
            "item" => root.with(2usize).with(0usize).with(0usize).with("text"),
            "nested" => root
                .with(2usize)
                .with(1usize)
                .with(1usize)
                .with(0usize)
                .with(0usize)
                .with("text"),
            // The quote's rows shift with every edit above them, so the bar's id is read
            // from the source as it stands, not from the first frame.
            "bar" => quote_bar_id(root, lp.app_mut().markdown_src.as_str()),
            _ => root.with(3usize).with(0usize).with("text"),
        };
        // A press past the marker's label reaches the text; the bar takes a press at its
        // own left edge, which is the nearest caret position inside the quote.
        let dx = if name == "bar" { 1.0 } else { 24.0 };
        let before = lp.app_mut().markdown_src.clone();
        if let Some(rect) = rect_of(lp, id) {
            click(
                lp,
                size,
                Point::new(rect.origin.x + dx, rect.origin.y + rect.size.h * 0.5),
            );
            // The bar carries no bytes of its own: reaching it means the caret landed in
            // the quote's row, which is what a press there must do.
            if name != "bar" {
                type_text(lp, size, "Z");
            }
        }
        let edited = if name == "bar" {
            lp.caret().is_some_and(|(owner, _)| owner == root)
        } else {
            lp.app_mut().markdown_src != before
        };
        reached &= edited;
        nested.push(format!("{name} {edited}"));
        // The caret stands in the quoted text: not one byte of the line's marker is touched,
        // so it keeps its rendered form however close the press was.
        if name == "quote" {
            prefix_in_text = runs(lp, quote_text)
                .is_some_and(|runs| runs.iter().any(|(text, hidden)| *hidden && text == "> "));
            let bar = quote_bar_id(root, lp.app_mut().markdown_src.as_str());
            bar_in_text = shape_fill(lp, bar, 0).is_some_and(|fill| fill.a > 0.5);
        }
    }
    // The bar press lands on the marker's first byte: it is the one press that draws the
    // prefix without moving into the text.
    let prefix_shown = runs(lp, quote_text)
        .is_some_and(|runs| runs.iter().any(|(text, hidden)| !*hidden && text == "> "));
    // The bar is the quote's rendered form, so it stands down while the bytes it stands
    // for are drawn — one form at a time, and the cell keeps its room either way.
    let bar_now = quote_bar_id(root, lp.app_mut().markdown_src.as_str());
    let bar_gone = !shape_fill(lp, bar_now, 0).is_some_and(|fill| fill.a > 0.5);
    // The bytes and the bar take the same room, so the quoted text stands at one column in
    // both forms: revealing a marker never moves the line it marks.
    let quote_steady = match (quote_x, text_ink_bounds(lp, quote_text)) {
        (Some(before), Some((after, ..))) => (before - after).abs() < 0.5,
        _ => false,
    };

    // A fenced block marks its own edges: a caret anywhere in the block shows both fences
    // in the card's head and foot, where they are edited in place.
    let fence_shown = match code_index.map(|index| root.with(index)) {
        Some(block) => match rect_of(lp, block.with("text")) {
            Some(rect) => {
                click(
                    lp,
                    size,
                    Point::new(rect.origin.x + 8.0, rect.origin.y + rect.size.h * 0.5),
                );
                runs(lp, block.with("header").with("fence"))
                    .is_some_and(|runs| visible(&runs).starts_with("```rust"))
                    && runs(lp, block.with("footer").with("fence"))
                        .is_some_and(|runs| visible(&runs) == "```")
            }
            None => false,
        },
        None => false,
    };

    // A list marker lives in the row's own runs. Away from the item it hides behind the
    // bullet the list draws as an atom; a press on it draws it as written, and Home then
    // Delete takes the "-" off like any other byte.
    let item_text = root.with(2usize).with(1usize).with(0usize).with("text");
    let label_away = atom_texts(lp, item_text)
        .is_some_and(|atoms| atoms.iter().any(|text| text.starts_with('•')));
    let away_height = rect_of(lp, item_text).map(|rect| rect.size.h);
    let item_x = text_ink_bounds(lp, item_text).map(|(left, ..)| left);
    // One list shares its marker column, and a nested list adds the room of its own indent
    // and marker, so its content stands one cell right of the text the nested list sits in.
    let first_text = root.with(2usize).with(0usize).with(0usize).with("text");
    let ordered_a = root
        .with(2usize)
        .with(1usize)
        .with(1usize)
        .with(0usize)
        .with(0usize)
        .with("text");
    let ordered_b = root
        .with(2usize)
        .with(1usize)
        .with(1usize)
        .with(1usize)
        .with(0usize)
        .with("text");
    let aligned = match (
        marker_room(lp, first_text),
        marker_room(lp, item_text),
        marker_room(lp, ordered_a),
        marker_room(lp, ordered_b),
    ) {
        (Some(first), Some(second), Some(above), Some(below)) => {
            (first - second).abs() < 0.5 && (above - below).abs() < 0.5 && above > second + 4.0
        }
        _ => false,
    };
    let marker_drawn = match rect_of(lp, item_text) {
        Some(rect) => {
            click(
                lp,
                size,
                Point::new(rect.origin.x + 1.0, rect.origin.y + rect.size.h * 0.5),
            );
            runs(lp, item_text).is_some_and(|runs| visible(&runs).starts_with("- "))
        }
        None => false,
    };
    // Entering the item swaps the bullet for the bytes it stands for, and the cell keeps
    // its room: the row keeps its height and the item's text keeps its column.
    let steady = match (away_height, rect_of(lp, item_text), item_x) {
        (Some(away), Some(after), Some(before)) => {
            (away - after.size.h).abs() < 0.5
                && text_ink_bounds(lp, item_text)
                    .is_some_and(|(left, ..)| (left - before).abs() < 0.5)
        }
        _ => false,
    };
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Home,
    });
    frame(lp, size, None);
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Delete,
    });
    frame(lp, size, None);
    let marker_deleted = !lp.app_mut().markdown_src.contains("- Nesting works:");

    // Down from the first item reaches the second, and Enter at the item's end splits it.
    let mut keyed = Vec::new();
    let mut pressed = true;
    for key in [ui_core::Key::Down, ui_core::Key::Enter] {
        let before = lp.app_mut().markdown_src.clone();
        if let Some(rect) = rect_of(lp, root.with(2usize).with(0usize).with(0usize).with("text")) {
            click(
                lp,
                size,
                Point::new(rect.origin.x + 24.0, rect.origin.y + rect.size.h * 0.5),
            );
            lp.dispatch(ui_core::Event::KeyPressed { key });
            frame(lp, size, None);
            type_text(lp, size, "Q");
        }
        let edited = lp.app_mut().markdown_src != before;
        pressed &= edited;
        keyed.push(format!("{key:?} {edited}"));
    }

    // A line holding only a list marker: the empty item still draws a line, and a press on
    // it puts the caret where its text would go.
    // A source of its own: the sample grows, and a block past the viewport cannot be pressed.
    lp.app_mut().markdown_src = "1. ".to_owned();
    frame(lp, size, None);
    let document = ui_richtext::parse(&lp.app_mut().markdown_src);
    let index = document.blocks.len().saturating_sub(1);
    let empty = root.with(index).with(0usize).with(0usize).with("text");
    let item_edited = match rect_of(lp, empty) {
        Some(rect) => {
            click(
                lp,
                size,
                Point::new(
                    rect.origin.x + rect.size.w - 4.0,
                    rect.origin.y + rect.size.h * 0.5,
                ),
            );
            // The marker's trailing space is hung by layout, so a press past the end lands
            // before it; End states the position this check is about.
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::End,
            });
            frame(lp, size, None);
            type_text(lp, size, "x");
            lp.app_mut().markdown_src.ends_with("1. x")
        }
        None => false,
    };

    // A quote nests as a child block, so the inner quote's text is a view like any other.
    lp.app_mut().markdown_src = "> outer\n> > inner\n".to_owned();
    frame(lp, size, None);
    let document = ui_richtext::parse(&lp.app_mut().markdown_src);
    let index = document.blocks.len().saturating_sub(1);
    let nested_quote = document
        .blocks
        .get(index)
        .and_then(|block| block.children().get(1))
        .is_some_and(|block| block.kind == ui_richtext::BlockKind::Quote);
    let inner = root.with(index).with(1usize).with(0usize).with("text");
    let nested_edited = match rect_of(lp, inner) {
        Some(rect) => {
            let before = lp.app_mut().markdown_src.clone();
            click(
                lp,
                size,
                Point::new(rect.origin.x + 24.0, rect.origin.y + rect.size.h * 0.5),
            );
            type_text(lp, size, "Z");
            let after = lp.app_mut().markdown_src.clone();
            after != before && after.contains("> > ") && after.ends_with('\n')
        }
        None => false,
    };

    // The control answers its own keys: Enter splits the item and writes its marker, and Tab
    // nests the new item under the one above while Shift+Tab takes it back. Each is one
    // engine edit, so the source, the caret and the history stay one record.
    lp.app_mut().markdown_src = "- alpha\n".to_owned();
    frame(lp, size, None);
    let document = ui_richtext::parse(&lp.app_mut().markdown_src);
    let index = document.blocks.len().saturating_sub(1);
    let alpha = root.with(index).with(0usize).with(0usize).with("text");
    let mut listing = false;
    let mut indented = false;
    let mut outdented = false;
    if let Some(rect) = rect_of(lp, alpha) {
        click(
            lp,
            size,
            Point::new(
                rect.origin.x + rect.size.w - 2.0,
                rect.origin.y + rect.size.h * 0.5,
            ),
        );
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Enter,
        });
        frame(lp, size, None);
        listing = lp.app_mut().markdown_src.contains("- alpha\n- ");
        if listing {
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Tab,
            });
            frame(lp, size, None);
            indented = lp.app_mut().markdown_src.contains("- alpha\n  - ");
            lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
                shift: true,
                ..ui_core::Modifiers::default()
            }));
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Tab,
            });
            frame(lp, size, None);
            lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
            outdented = lp.app_mut().markdown_src.contains("- alpha\n- ");
        }
    }
    // A selection over three lines, a blank one between, indents every line — the blank
    // one too. The selection survives the edit, so Shift+Tab takes the same lines back,
    // and one undo restores the whole source: the command is one engine edit, not one per
    // line. The check runs inside the single-line flags above: it is the same command.
    lp.app_mut().markdown_src = "one\n\ntwo\n".to_owned();
    frame(lp, size, None);
    let multiline = if rect_of(lp, root.with(0usize).with("text")).is_some() {
        click_id(lp, size, root.with(0usize).with("text"), 1.0);
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..ui_core::Modifiers::default()
        }));
        // Two steps: the blank line is a line of its own, so one step lands on it.
        for _ in 0..2 {
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Down,
            });
            frame(lp, size, None);
        }
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::End,
        });
        frame(lp, size, None);
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
        let selected = lp.selection().is_some_and(|(owner, selection)| {
            let range = selection.range();
            owner == root && range.start <= 3 && range.end == 8
        });
        let before = lp.app_mut().markdown_src.clone();
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        });
        frame(lp, size, None);
        let stepped = lp.app_mut().markdown_src == "  one\n  \n  two\n";
        press_ctrl(lp, size, 'z');
        let undone = lp.app_mut().markdown_src == before;
        press_ctrl(lp, size, 'y');
        let redone = lp.app_mut().markdown_src == "  one\n  \n  two\n";
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..ui_core::Modifiers::default()
        }));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Tab,
        });
        frame(lp, size, None);
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
        let back = lp.app_mut().markdown_src == before;
        selected && stepped && undone && redone && back
    } else {
        false
    };
    indented &= multiline;
    let commands = listing && indented && outdented;

    // One ordered list shares one cell, so a `9.` and a `10.` end at the same dot and both
    // items' text starts at one column: the digit the marker gained widens the cell, not
    // the step between the markers and the text.
    lp.app_mut().markdown_src = "9. nine\n10. ten\n".to_owned();
    frame(lp, size, None);
    // Park the caret in the first item's text, so both markers stand as their labels.
    if let Some(rect) = rect_of(lp, root.with(0usize).with(0usize).with(0usize).with("text")) {
        click(
            lp,
            size,
            Point::new(
                rect.origin.x + rect.size.w - 1.0,
                rect.origin.y + rect.size.h * 0.5,
            ),
        );
    }
    // The dot is the last glyph the label draws, and its rect is what the paint pass placed:
    // recovering the alignment from the node's box would repeat the layout's claim instead
    // of checking the drawing. A number's label is painted after its line's own text, and
    // both numbers come from one shared cell, so both dots end at one x.
    let dots: Vec<Option<f32>> = (0..2usize)
        .map(|item| last_glyph_right(lp, root.with(0usize).with(item).with(0usize).with("text")))
        .collect();
    let columns: Vec<Option<f32>> = (0..2usize)
        .map(|item| marker_room(lp, root.with(0usize).with(item).with(0usize).with("text")))
        .collect();
    let stepped = match (&dots[..], &columns[..]) {
        ([Some(above), Some(below)], [Some(first), Some(second)]) => {
            (above - below).abs() < 0.5 && (first - second).abs() < 0.5
        }
        _ => false,
    };
    // A row yields its end to the row that starts there: one input draws one caret, and the
    // second item is the one that would double it — a row that kept the caret past its own
    // bytes drew it on its own line and on the next one at once.
    let first_row = root.with(0usize).with(0usize).with(0usize).with("text");
    let second_row = root.with(0usize).with(1usize).with(0usize).with("text");
    let mut carets = 0usize;
    let one_caret = match rect_of(lp, first_row) {
        Some(rect) => {
            click(
                lp,
                size,
                Point::new(
                    rect.origin.x + rect.size.w - 1.0,
                    rect.origin.y + rect.size.h * 0.5,
                ),
            );
            carets = carets_in(lp, root);
            // "9. nine" is eight bytes: the row ends after its seventh, the break.
            caret_in(lp, first_row) == Some(7) && caret_in(lp, second_row).is_none() && carets == 1
        }
        None => false,
    };

    // Up and Down keep the column across sibling items: a step from the text start of one
    // lands at the text start of the next, even though the marker cell beside it starts
    // left of that column and belongs to the same line.
    lp.app_mut().markdown_src = "- a\n- b\n- c\n".to_owned();
    frame(lp, size, None);
    let item_text = |item: usize| root.with(0usize).with(item).with(0usize).with("text");
    let vertical = match rect_of(lp, item_text(0)) {
        Some(rect) => {
            click(
                lp,
                size,
                Point::new(rect.origin.x + 1.0, rect.origin.y + rect.size.h * 0.5),
            );
            // `- ` is two bytes, so Home and two steps right stand at the item's text: the
            // column every item's text shares.
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Home,
            });
            for _ in 0..2 {
                lp.dispatch(ui_core::Event::KeyPressed {
                    key: ui_core::Key::Right,
                });
            }
            frame(lp, size, None);
            let first = lp.caret().map(|(_, at)| at);
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Down,
            });
            frame(lp, size, None);
            let second = lp.caret().map(|(_, at)| at);
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Down,
            });
            frame(lp, size, None);
            let third = lp.caret().map(|(_, at)| at);
            lp.dispatch(ui_core::Event::KeyPressed {
                key: ui_core::Key::Up,
            });
            frame(lp, size, None);
            let back = lp.caret().map(|(_, at)| at);
            first == Some(2) && second == Some(6) && third == Some(10) && back == Some(6)
        }
        None => false,
    };

    // Inside a marker, Down lands on the same place in the marker below: the end of `1. `
    // goes to the end of `2. `, and across a longer number, the end of `9. ` goes to the end
    // of `10. `. Digits differ in advance, so only the shared cell and its end alignment
    // put both carets on one column.
    let marker_step = |lp: &mut AppLoop<Gallery>, source: &str, below_end: usize| -> Option<bool> {
        lp.app_mut().markdown_src = source.to_owned();
        frame(lp, size, None);
        let rect = rect_of(lp, item_text(0))?;
        let room = marker_room(lp, item_text(0))?;
        // The right half of the marker's room is the caret after its bytes.
        click(
            lp,
            size,
            Point::new(
                rect.origin.x + room - 1.0,
                rect.origin.y + rect.size.h * 0.5,
            ),
        );
        frame(lp, size, None);
        let inside = lp.caret().map(|(_, at)| at);
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Down,
        });
        frame(lp, size, None);
        let below = lp.caret().map(|(_, at)| at);
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Up,
        });
        frame(lp, size, None);
        let back = lp.caret().map(|(_, at)| at);
        Some(inside == Some(3) && below == Some(below_end) && back == Some(3))
    };
    let marker_step = marker_step(lp, "1. a\n2. b\n3. c\n", 8).unwrap_or(false)
        && marker_step(lp, "9. a\n10. b\n", 9).unwrap_or(false);

    // One frame answers a burst of keys, so a chord's edit must be computed against the
    // state it was pressed in: Enter followed by Shift+Down in the same burst used to
    // replace the line the arrows had just selected.
    lp.app_mut().markdown_src = "- burst tail\n- after\n".to_owned();
    frame(lp, size, None);
    let tail = root.with(0usize).with(0usize).with(0usize).with("text");
    let mut burst = false;
    if let Some(rect) = rect_of(lp, tail) {
        click(
            lp,
            size,
            Point::new(
                rect.origin.x + rect.size.w - 2.0,
                rect.origin.y + rect.size.h * 0.5,
            ),
        );
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Enter,
        });
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
            shift: true,
            ..ui_core::Modifiers::default()
        }));
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Down,
        });
        frame(lp, size, None);
        lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
        let source = lp.app_mut().markdown_src.clone();
        burst = source == "- burst tail\n- \n- after\n";
    }

    // Three blank lines between two paragraphs draw three line boxes: every line break is
    // one line, and the run's own bytes still answer how many lines it holds.
    lp.app_mut().markdown_src = "a\n\n\n\nb\n".to_owned();
    frame(lp, size, None);
    let blank = {
        let first = root.with(("blank", 2usize)).with("text");
        let second = root.with(("blank", 3usize)).with("text");
        let third = root.with(("blank", 4usize)).with("text");
        let rows = rect_of(lp, first).is_some()
            && rect_of(lp, second).is_some()
            && rect_of(lp, third).is_some();
        let held = if rect_of(lp, second).is_some() {
            click_id(lp, size, second, 4.0);
            lp.caret().is_some_and(|(_, at)| at == 3)
        } else {
            false
        };
        // The caret standing on a blank line is drawn by that line's own view.
        let drawn = caret_in(lp, second);
        let counted = ui_richtext::blank::blank_run_at(&lp.app_mut().markdown_src, 3)
            .is_some_and(|(run, index)| run.lines.len() == 3 && index == 1);
        rows && held && drawn.is_some() && counted
    };

    // A quote's blank lines are `>`-only lines the parser drops as paragraph breaks; each
    // still needs a row, or its marker shows nowhere and the caret cannot reach the bytes.
    lp.app_mut().markdown_src = "> a\n>\n>\n> b\n".to_owned();
    frame(lp, size, None);
    let quoted = {
        // A `>`-only line is a row no block owns, so its id is the line's own offset; its
        // marker is the row's own bytes, the only thing that line holds.
        let first = root.with(("blank", 4usize)).with("text");
        let second = root.with(("blank", 6usize)).with("text");
        let heights = rect_of(lp, first).is_some_and(|rect| rect.size.h > 0.0)
            && rect_of(lp, second).is_some_and(|rect| rect.size.h > 0.0);
        let hidden = runs(lp, first).is_some_and(|runs| !visible(&runs).contains('>'));
        let pressed = match (rect_of(lp, second), marker_room(lp, second)) {
            (Some(rect), Some(room)) => {
                // The right half of the marker's room is the caret after its bytes.
                click(
                    lp,
                    size,
                    Point::new(
                        rect.origin.x + room - 1.0,
                        rect.origin.y + rect.size.h * 0.5,
                    ),
                );
                lp.caret().map(|(_, at)| at) == Some(7)
            }
            _ => false,
        };
        let shown = runs(lp, second).is_some_and(|runs| visible(&runs).contains('>'));
        heights && hidden && pressed && shown
    };
    // A thematic break is one line's marker: the divider stands while the caret is away,
    // and the bytes replace it, not join it, once the range touches the line. A break has
    // no caret stop of its own — the line is reached by selecting across it.
    lp.app_mut().markdown_src = "before\n\n---\n\nafter\n".to_owned();
    frame(lp, size, None);
    let rule = {
        let block = root.with(1usize);
        let text = block.with("text");
        let away =
            rect_of(lp, block).is_some_and(|rect| rect.size.h < 2.0) && runs(lp, text).is_none();
        let entered = match rect_of(lp, root.with(("blank", 7usize)).with("text")) {
            Some(blank) => {
                click(
                    lp,
                    size,
                    Point::new(blank.origin.x + 2.0, blank.origin.y + blank.size.h * 0.5),
                );
                lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
                    shift: true,
                    ..ui_core::Modifiers::default()
                }));
                lp.dispatch(ui_core::Event::KeyPressed {
                    key: ui_core::Key::Down,
                });
                frame(lp, size, None);
                lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers::default()));
                let drawn = runs(lp, text).is_some_and(|runs| visible(&runs) == "---");
                let grown = rect_of(lp, text).is_some_and(|rect| rect.size.h > 8.0);
                drawn && grown
            }
            None => false,
        };
        println!("selftest: markdown rule divider {away} source {entered}");
        away && entered
    };

    // Structural (the default) keeps the rendered formula: a press on its left half puts
    // the caret at the formula's opening byte and reveals the source. Source mode builds
    // the same control with a floating preview while the caret touches the formula; the
    // preview is absent when the caret is away and in Structural mode.
    lp.app_mut().markdown_src = "Inline $x^2$ mark\n".to_owned();
    frame(lp, size, None);
    let math_mode = {
        let document = ui_richtext::parse(&lp.app_mut().markdown_src);
        let mark = document
            .marks
            .iter()
            .find(|mark| mark.kind == ui_richtext::MarkKind::InlineMath)
            .cloned();
        let text = root.with(0usize).with("text");
        // Park the caret past the line first: a caret an earlier check left inside the
        // formula would have it revealed, and a revealed formula has no atom to press.
        if let Some(rect) = rect_of(lp, text) {
            click(
                lp,
                size,
                Point::new(
                    rect.origin.x + rect.size.w - 2.0,
                    rect.origin.y + rect.size.h * 0.5,
                ),
            );
        }
        let placed = mark.as_ref().and_then(|mark| {
            let atom = atom_rect(lp, text, mark.range.len())?;
            click(
                lp,
                size,
                Point::new(
                    atom.origin.x + atom.size.w * 0.25,
                    atom.origin.y + atom.size.h * 0.5,
                ),
            );
            let caret = lp.caret().map(|(_, at)| at);
            let left = caret.is_some_and(|at| at == mark.full.start || at == mark.range.start);
            let revealed = atoms_of(lp, text).is_some_and(|lens| lens.is_empty())
                && runs(lp, text).is_some_and(|runs| visible(&runs).contains("$x^2$"));
            Some(left && revealed)
        });
        let structural = source_preview(true, ui_widgets::MathEditing::Structural);
        let source = source_preview(true, ui_widgets::MathEditing::Source);
        let away = source_preview(false, ui_widgets::MathEditing::Source);
        placed.unwrap_or(false)
            && !structural.0
            && source.0
            && source.1.as_deref().is_some_and(|text| text.contains("x^2"))
            && !away.0
    };

    // An entity stands for a character its bytes do not spell: away from the caret one atom
    // draws it, touching it gives the source back, and one Right crosses the hidden bytes.
    lp.app_mut().markdown_src = "AT&amp;T &#65; end\n\n```\n&amp; &#65;\n```\n".to_owned();
    frame(lp, size, None);
    let entity = {
        let document = ui_richtext::parse(&lp.app_mut().markdown_src);
        let mark = document
            .marks
            .iter()
            .find(|mark| mark.kind == ui_richtext::MarkKind::Entity)
            .cloned();
        let code_index = document
            .blocks
            .iter()
            .position(|block| matches!(block.kind, ui_richtext::BlockKind::CodeBlock { .. }));
        let text = root.with(0usize).with("text");
        // Park the caret past the line, so both entities are away from it and drawn.
        let parked = rect_of(lp, text).map(|rect| {
            click(
                lp,
                size,
                Point::new(
                    rect.origin.x + rect.size.w - 2.0,
                    rect.origin.y + rect.size.h * 0.5,
                ),
            );
        });
        let drawn = parked.is_some()
            && atoms_of(lp, text).is_some_and(|lens| lens == vec![5, 5])
            && atom_texts(lp, text).is_some_and(|texts| texts == ["&".to_owned(), "A".to_owned()]);
        let source_hidden = runs(lp, text).is_some_and(|runs| {
            runs.iter().any(|(text, hidden)| *hidden && text == "&amp;")
                && runs.iter().any(|(text, hidden)| *hidden && text == "&#65;")
        });
        let entered = mark.as_ref().and_then(|mark| {
            let atom = atom_rect(lp, text, mark.range.len())?;
            let at = Point::new(
                atom.origin.x + atom.size.w * 0.25,
                atom.origin.y + atom.size.h * 0.5,
            );
            click(lp, size, at);
            let caret = lp.caret().map(|(_, at)| at);
            let revealed = atoms_of(lp, text).is_some_and(|lens| lens == vec![5])
                && runs(lp, text).is_some_and(|runs| visible(&runs).contains("&amp;"));
            Some(caret == Some(mark.full.start) && revealed)
        });
        // Park away, then press into the atom and press Right in one burst: both answer
        // against the tree that still holds the hidden run, so one step reaches its far edge.
        if let Some(rect) = rect_of(lp, text) {
            click(
                lp,
                size,
                Point::new(
                    rect.origin.x + rect.size.w - 2.0,
                    rect.origin.y + rect.size.h * 0.5,
                ),
            );
        }
        let crossed = match (&mark, rect_of(lp, text), atom_rect(lp, text, 5)) {
            (Some(mark), Some(_), Some(atom)) => {
                let at = Point::new(
                    atom.origin.x + atom.size.w * 0.25,
                    atom.origin.y + atom.size.h * 0.5,
                );
                let button = ui_core::PointerButton::Primary;
                lp.dispatch(ui_core::Event::PointerMoved { pos: at });
                lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
                lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
                lp.dispatch(ui_core::Event::KeyPressed {
                    key: ui_core::Key::Right,
                });
                frame(lp, size, None);
                lp.caret().map(|(_, at)| at) == Some(mark.full.end)
                    && runs(lp, text).is_some_and(|runs| visible(&runs).contains("&amp;"))
            }
            _ => false,
        };
        let literal = code_index.is_some_and(|index| {
            let code = root.with(index).with("text");
            atoms_of(lp, code).is_some_and(|lens| lens.is_empty())
                && runs(lp, code).is_some_and(|runs| visible(&runs).contains("&amp;"))
        });
        println!(
            "selftest: entity drawn {drawn} hidden {source_hidden} revealed {entered:?} crossed {crossed} literal {literal}"
        );
        drawn && source_hidden && entered == Some(true) && crossed && literal
    };

    // A GFM table reads as a grid while the caret is away and as its source while it is
    // inside. The grid has a header row and a body row, and the delimiter row's `--:` is
    // the right alignment of the last column; entering shows the source bytes and leaving
    // comes back to the same geometry, so the blocks below do not drift.
    let table = {
        let source = "| a | bb |\n|:--|--:|\n| c | dd |\n\nafter\n";
        lp.app_mut().markdown_src = source.to_owned();
        frame(lp, size, None);
        let paragraph = root.with(1usize).with("text");
        // Park the caret outside the table first, so the grid state is the one drawn.
        if let Some(rect) = rect_of(lp, paragraph) {
            click(
                lp,
                size,
                Point::new(rect.origin.x + 4.0, rect.origin.y + rect.size.h * 0.5),
            );
        }
        let table_block = root.with(0usize);
        let table_text = table_block.with("text");
        let cell = |row: usize, column: usize| table_block.with("grid").with(("cell", row, column));
        // (a) two rows, header included, and the four cell texts.
        let two_rows = rect_of(lp, cell(0usize, 0usize)).is_some()
            && rect_of(lp, cell(1usize, 0usize)).is_some()
            && rect_of(lp, cell(2usize, 0usize)).is_none();
        let texts: Vec<String> = [(0, 0), (0, 1), (1, 0), (1, 1)]
            .into_iter()
            .map(|(row, column)| {
                runs(lp, cell(row, column))
                    .map(|runs| visible(&runs))
                    .unwrap_or_default()
            })
            .collect();
        let cell_texts = texts == ["a", "bb", "c", "dd"];
        // (b) the bold header and the regular body text in the last column end on the same
        // right edge, which only an end alignment gives their unequal ink widths; the
        // first column's text starts on its column's left edge.
        let right_aligned = match (
            text_ink_bounds(lp, cell(0usize, 1usize)),
            text_ink_bounds(lp, cell(1usize, 1usize)),
            text_ink_bounds(lp, cell(1usize, 0usize)),
        ) {
            (Some(header), Some(body), Some(first)) => {
                (header.1 - header.3).abs() < 0.5
                    && (body.1 - body.3).abs() < 0.5
                    && (first.0 - first.2).abs() < 0.5
            }
            _ => false,
        };
        let grid_height = rect_of(lp, table_block).map(|rect| rect.size.h);
        let grid_paragraph_y = rect_of(lp, paragraph).map(|rect| rect.origin.y);
        // (c) a press on a cell shows the whole source, delimiter row included.
        let entered = match rect_of(lp, cell(1usize, 0usize)) {
            Some(rect) => {
                click(
                    lp,
                    size,
                    Point::new(rect.origin.x + 4.0, rect.origin.y + rect.size.h * 0.5),
                );
                // Every byte of the table, cell text included: the source state is what the
                // caret edits, so a cell that vanishes there is a table drawn from the
                // projection without its content.
                runs(lp, table_text).is_some_and(|runs| {
                    let text = visible(&runs);
                    text.contains("| a | bb |")
                        && text.contains("|:--|--:|")
                        && text.contains("| c | dd |")
                }) && rect_of(lp, cell(0usize, 0usize)).is_none()
            }
            None => false,
        };
        let source_height = rect_of(lp, table_block).map(|rect| rect.size.h);
        // A press on the paragraph leaves the table and brings the grid back.
        let left = match rect_of(lp, paragraph) {
            Some(rect) => {
                click(
                    lp,
                    size,
                    Point::new(rect.origin.x + 4.0, rect.origin.y + rect.size.h * 0.5),
                );
                rect_of(lp, cell(0usize, 0usize)).is_some()
                    && runs(lp, cell(0usize, 1usize)).is_some_and(|runs| visible(&runs) == "bb")
            }
            None => false,
        };
        // The two states may differ in height; the paragraph below must return to the same
        // y after the round trip, or entering the table moved unrelated blocks for good.
        let steady = match (grid_paragraph_y, rect_of(lp, paragraph)) {
            (Some(before), Some(after)) => (before - after.origin.y).abs() < 0.5,
            _ => false,
        };
        // A table wider than the column shrinks its columns to it; without the shrink the
        // last cell's box would stick out past the table's own width.
        lp.app_mut().markdown_src = format!(
            "| first | {} |\n|---|---|\n| second | {} |\n\nafter\n",
            "x".repeat(80),
            "y".repeat(80)
        );
        frame(lp, size, None);
        if let Some(rect) = rect_of(lp, paragraph) {
            click(
                lp,
                size,
                Point::new(rect.origin.x + 4.0, rect.origin.y + rect.size.h * 0.5),
            );
        }
        let wide = match (rect_of(lp, cell(0usize, 1usize)), rect_of(lp, table_block)) {
            (Some(last), Some(table)) => {
                let inside = last.origin.x + last.size.w <= table.origin.x + table.size.w + 0.5;
                // The grid is showing: one body row after the header, and the caret outside.
                inside
                    && rect_of(lp, cell(1usize, 0usize)).is_some()
                    && rect_of(lp, cell(2usize, 0usize)).is_none()
            }
            _ => false,
        };
        println!(
            "selftest: table rows {two_rows} cells {cell_texts} {texts:?} aligned {right_aligned} entered {entered} left {left} steady {steady} wide {wide} height {grid_height:?}->{source_height:?}"
        );
        two_rows && cell_texts && right_aligned && entered && left && steady && wide
    };

    // The caret reveal in a viewport far shorter than the page: forty source lines in a
    // 120 px `Scroll`. The page's own viewport is checked separately by
    // `markdown_page_scroll`, where the window's height would make this sample slow.
    let scrolled = {
        let mut source = String::new();
        for line in 0..40 {
            source.push_str(&format!("line {line}\n"));
        }
        let mut lp = AppLoop::with_stack(ScrolledMarkdown { source }, TEXT_STACK);
        lp.register_font(
            TEXT_FAMILY,
            include_bytes!("../../../../assets/fonts/InterVariable.ttf")
                .as_slice()
                .into(),
        );
        let mut target = ui_platform::OffscreenTarget::new(260, 120);
        lp.frame(&mut target, None);
        let scroller = scrolled_markdown_scroller();
        let root = scrolled_markdown_root();
        let start = lp.scroll_of(scroller).unwrap_or(0.0);
        let mut early = start;
        if let Some(rect) = any_rect(&lp, root.with(0usize).with("text")) {
            let at = Point::new(rect.origin.x + 4.0, rect.origin.y + 4.0);
            let button = ui_core::PointerButton::Primary;
            lp.dispatch(ui_core::Event::PointerMoved { pos: at });
            lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
            lp.frame(&mut target, None);
            lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
            lp.frame(&mut target, None);
            // The first lines are still on screen. Without the caret reveal, the focus
            // move's whole-node request rolls this tall column straight to its bottom.
            for _ in 0..3 {
                lp.dispatch(ui_core::Event::KeyPressed {
                    key: ui_core::Key::Down,
                });
                lp.frame(&mut target, None);
            }
            early = lp.scroll_of(scroller).unwrap_or(0.0);
            for _ in 0..80 {
                lp.dispatch(ui_core::Event::KeyPressed {
                    key: ui_core::Key::Down,
                });
                lp.frame(&mut target, None);
            }
        }
        let end = lp.scroll_of(scroller).unwrap_or(0.0);
        // The last line's bottom meets the viewport's bottom, so the caret line is inside
        // the clip; this is the geometry the hand-scroll check in ui-platform pins too.
        let (extent, viewport) = lp
            .tree()
            .find_by_id(scroller)
            .and_then(|nid| lp.tree().get(nid))
            .map(|node| (node.scroll_extent.unwrap_or_default().h, node.rect.size.h))
            .unwrap_or_default();
        let travel = (extent - viewport).max(0.0);
        println!("selftest: markdown scroll start {start} early {early} end {end} travel {travel}");
        // The reveal stops on the caret line's own box, and a row carries half its leading
        // above that box: a few pixels of offset is the line's own spacing, not a jump.
        travel > 0.0
            && early.abs() < 24.0
            && (end - travel).abs() < 0.5
            && lp
                .caret()
                .is_some_and(|(owner, at)| owner == root && at > 0)
    };

    // A paragraph holding only an image draws the texture scaled to the column; the source
    // line replaces it at the picture's own height, and no texture keeps that line.
    let image = {
        let mut traced = AppLoop::with_stack(
            ImageMarkdown {
                source: "![alt](tex)\n".to_owned(),
                image: Some(sample_image),
            },
            TEXT_STACK,
        );
        install_image_sample(&mut traced);
        let mut target = ui_platform::OffscreenTarget::new(300, 320);
        traced.frame(&mut target, None);
        let root = image_root();
        let block = root.with(0usize);
        let picture = image_rect(&traced, block)
            .zip(any_rect(&traced, block))
            .is_some_and(|(prim, rect)| {
                let scaled = IMAGE_WIDTH * 0.5;
                (rect.size.w - IMAGE_WIDTH).abs() < 0.5
                    && (rect.size.h - scaled).abs() < 0.5
                    && (prim.size.w - IMAGE_WIDTH).abs() < 0.5
                    && (prim.size.h - scaled).abs() < 0.5
            });
        let entered = any_rect(&traced, block).is_some_and(|rect| {
            image_press(
                &mut traced,
                &mut target,
                Point::new(
                    rect.origin.x + rect.size.w * 0.5,
                    rect.origin.y + rect.size.h * 0.5,
                ),
            );
            let source = runs(&traced, block).is_some_and(|runs| visible(&runs) == "![alt](tex)");
            let gone = image_rect(&traced, block).is_none();
            let steady =
                any_rect(&traced, block).is_some_and(|now| (now.size.h - rect.size.h).abs() < 0.5);
            source && gone && steady
        });

        let mut missing = AppLoop::with_stack(
            ImageMarkdown {
                source: "![alt](tex)\n".to_owned(),
                image: Some(missing_image),
            },
            TEXT_STACK,
        );
        install_image_sample(&mut missing);
        missing.frame(&mut target, None);
        let source = runs(&missing, block).is_some_and(|runs| visible(&runs) == "![alt](tex)");
        let no_picture = image_rect(&missing, block).is_none();
        println!(
            "selftest: image picture {picture} entered {entered} missing {}",
            source && no_picture
        );
        picture && entered && source && no_picture
    };

    // A row whose cells spell every byte it draws starts at its own left edge: the marker an
    // Enter opens on gets no second indent, and a wrapped item's later line clears the marker
    // instead of sliding under it.
    let row_geometry = {
        let line_x = |lp: &AppLoop<Gallery>, id: Id, index: usize| -> Option<f32> {
            let nid = lp.tree().find_by_id(id)?;
            let node = lp.tree().get(nid)?;
            let ui_core::NodeData::Text { block, .. } = &node.data else {
                return None;
            };
            let layout = node.inline.as_deref()?;
            Some(
                layout
                    .line_origin(
                        index,
                        node.rect.origin,
                        node.rect.size.h,
                        node.wrap_width,
                        block.edges(),
                        block.align,
                    )
                    .x
                    - node.rect.origin.x,
            )
        };
        lp.app_mut().markdown_src = "- a\n- \n".to_owned();
        frame(lp, size, None);
        let list = |item: usize| root.with(0usize).with(item).with(0usize).with("text");
        let flat = match (line_x(lp, list(0), 0), line_x(lp, list(1), 0)) {
            (Some(text), Some(marker)) => text.abs() < 0.5 && (text - marker).abs() < 0.5,
            _ => false,
        };
        lp.app_mut().markdown_src = "- A bullet item long enough that it has to wrap onto a\
 second visual line inside this narrow card, so that the wrap can be read.\n"
            .to_owned();
        frame(lp, size, None);
        // The marker's room is the atom that stands in for it, drawn from the row's own
        // left edge.
        let room = lp
            .tree()
            .find_by_id(list(0))
            .and_then(|nid| lp.tree().get(nid))
            .and_then(|node| match &node.data {
                ui_core::NodeData::Text { block, .. } => {
                    block.atoms.first().map(|atom| atom.size.w)
                }
                _ => None,
            });
        let hung = match (line_x(lp, list(0), 0), line_x(lp, list(0), 1), room) {
            (Some(first), Some(second), Some(room)) => (second - first - room).abs() < 0.5,
            _ => false,
        };
        flat && hung
    };

    println!(
        "selftest: markdown editor value {value:?} whole {whole}, marker hidden {hidden}, card {card}, copied {copied}, revealed {revealed}, source edited {edited}, undo {undone} redo {redone}, later block edited {placed}, stepped {stepped} one caret {one_caret} ({carets} drawn) vertical {vertical} marker step {marker_step} crossed {crossed}, nested {nested:?}, marker label {label_away} drawn {marker_drawn} deleted {marker_deleted} steady {steady} aligned {aligned}, quote prefix away {prefix_away} in text {prefix_in_text} shown {prefix_shown} bar away {bar_away} text {bar_in_text} on marker {bar_gone} steady {quote_steady}, fences {fence_shown}, keyed {keyed:?}, empty item edited {item_edited}, nested quote {nested_quote} edited {nested_edited}, reading buffer {read_buffer:?} plain {read_marker} heading {same_heading} paragraph {same_paragraph} still {read_still}, tasks {tasks} toggled {toggled}, break {listing} indent {indented} outdented {outdented}, burst {burst}, blank {blank}, row geometry {row_geometry}, formula box {formula_box} source {formula_source}, quoted {quoted}, rule {rule}, math mode {math_mode}, entity {entity}, table {table}, image {image}, scrolled {scrolled}"
    );
    whole
        && read_buffer.is_none()
        && read_marker
        && same_heading
        && same_paragraph
        && read_still
        && tasks
        && toggled
        && history
        && commands
        && formula_box
        && formula_source
        && hidden
        && card
        && copied
        && revealed
        && edited
        && placed
        && stepped
        && one_caret
        && vertical
        && marker_step
        && crossed
        && reached
        && label_away
        && marker_drawn
        && marker_deleted
        && steady
        && aligned
        && prefix_away
        && prefix_in_text
        && prefix_shown
        && bar_away
        && bar_in_text
        && bar_gone
        && quote_steady
        && fence_shown
        && pressed
        && item_edited
        && nested_quote
        && nested_edited
        && burst
        && blank
        && row_geometry
        && quoted
        && rule
        && math_mode
        && entity
        && table
        && image
        && scrolled
}

/// Whether a formula atom of `len` bytes exists in `text`, and where it was laid out.
fn atom_rect(lp: &AppLoop<Gallery>, text: Id, len: usize) -> Option<Rect> {
    let nid = lp.tree().find_by_id(text)?;
    let node = lp.tree().get(nid)?;
    let inline = node.inline.as_ref()?;
    let origin = node.visual_rect().origin;
    inline.lines.iter().find_map(|line| {
        line.atoms
            .iter()
            .find(|atom| atom.len == len)
            .map(|atom| atom.rect(Point::new(origin.x, origin.y + line.y)))
    })
}

/// Build a control off-screen and read whether Source mode floated a preview for the
/// formula. `touched` puts the caret inside it; the preview's text is answered too, so the
/// caller can tell the formula's own source from a stale node.
fn source_preview(touched: bool, mode: ui_widgets::MathEditing) -> (bool, Option<String>) {
    let document = ui_richtext::parse("Inline $x^2$ mark\n");
    let Some(mark) = document
        .marks
        .iter()
        .find(|mark| mark.kind == ui_richtext::MarkKind::InlineMath)
    else {
        return (false, None);
    };
    let full = mark.full.clone();
    let root = Id::str("selftest-math-mode");
    let caret = if touched { full.start + 1 } else { 0 };
    let mut store = ui_core::AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 800, 600, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing {
        focused: Some(root),
        caret: Some((root, caret)),
        ..Default::default()
    });
    let cx = ui_widgets::Cx::new(&mut core, Env::root());
    let control = ui_widgets::Markdown {
        id: root,
        document: std::sync::Arc::new(document),
        prepared: None,
        editable: true,
        math_editing: mode,
        popover_fill: Color::TRANSPARENT,
        width: 400.0,
        size: 14.0,
        ink: Color::TRANSPARENT,
        marker_ink: Color::TRANSPARENT,
        link_ink: Color::TRANSPARENT,
        code_ink: Color::TRANSPARENT,
        code_fill: Color::TRANSPARENT,
        code_style: None,
        code: ui_widgets::CodeOptions::default(),
        image: None,
        muted: Color::TRANSPARENT,
        caret_ink: Color::TRANSPARENT,
        selection_fill: Color::TRANSPARENT,
        selection_painter: None,
    };
    let decl = control.resolve_in(&cx);
    let preview = root.with(("math-preview", full.start));
    let Some(node) = find_decl(&decl, preview) else {
        return (false, None);
    };
    let text = find_decl(node, preview.with("formula")).and_then(|formula| {
        let ui_core::NodeData::Text { block, .. } = &formula.data else {
            return None;
        };
        Some(block.runs.iter().map(|run| run.text.as_str()).collect())
    });
    (true, text)
}

fn find_decl(decl: &Decl, id: Id) -> Option<&Decl> {
    if decl.id == id {
        return Some(decl);
    }
    decl.children.iter().find_map(|child| find_decl(child, id))
}

/// A `Ctrl` chord on a letter, held for one frame: the engine resolves the shortcut table.
fn press_ctrl(lp: &mut AppLoop<Gallery>, size: (u32, u32), key: char) {
    lp.dispatch(ui_core::Event::Modifiers(ui_core::Modifiers {
        ctrl: true,
        ..ui_core::Modifiers::default()
    }));
    lp.dispatch(ui_core::Event::KeyPressed {
        key: ui_core::Key::Char(key),
    });
    frame(lp, size, None);
}

/// A text node's runs, as their drawn text and whether each is hidden.
/// The caret offset a text node draws, or none when it draws none.

fn caret_in<A: App>(lp: &AppLoop<A>, id: Id) -> Option<usize> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Text { caret, .. } => caret.map(|caret| caret.offset),
        _ => None,
    }
}

/// How many text nodes in `root`'s subtree draw a caret: one input, one caret.
fn carets_in<A: App>(lp: &AppLoop<A>, root: Id) -> usize {
    let Some(root) = lp.tree().find_by_id(root) else {
        return 0;
    };
    let mut stack = vec![root];
    let mut count = 0;
    while let Some(nid) = stack.pop() {
        let Some(node) = lp.tree().get(nid) else {
            continue;
        };
        if matches!(&node.data, ui_core::NodeData::Text { caret: Some(_), .. }) {
            count += 1;
        }
        stack.extend(lp.tree().children_of(Some(nid)).iter().copied());
    }
    count
}

fn runs<A: App>(lp: &AppLoop<A>, id: Id) -> Option<Vec<(String, bool)>> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Text { block, .. } => Some(
            block
                .runs
                .iter()
                .map(|run| (run.text.to_string(), !run.visibility.painted()))
                .collect(),
        ),
        _ => None,
    }
}

/// Whether the subtree at `id` paints: a state that shares a stack for its height is
/// transparent while it is not the one shown.
fn opaque<A: App>(lp: &AppLoop<A>, id: Id) -> bool {
    lp.tree()
        .find_by_id(id)
        .and_then(|nid| lp.tree().get(nid))
        .and_then(|node| node.data.params().map(|params| params.opacity))
        .is_some_and(|opacity| opacity > 0.5)
}

/// The text each of a text node's atoms draws.
fn atom_texts(lp: &AppLoop<Gallery>, id: Id) -> Option<Vec<String>> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Text { block, .. } => Some(
            block
                .atoms
                .iter()
                .map(|atom| match &atom.object {
                    Some(ui_core::InlineObject::Glyphs { text, .. }) => text.clone(),
                    _ => String::new(),
                })
                .collect(),
        ),
        _ => None,
    }
}

/// The byte length each atom of a text node stands for.
fn atoms_of(lp: &AppLoop<Gallery>, id: Id) -> Option<Vec<usize>> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Text { block, .. } => {
            Some(block.atoms.iter().map(|atom| atom.len).collect())
        }
        _ => None,
    }
}

/// A text node's first line as drawn: ink left and right, then column left and right.
/// Alignment lives on the block, so the line's x is recovered as the paint pass does.
fn text_ink_bounds(lp: &AppLoop<Gallery>, id: Id) -> Option<(f32, f32, f32, f32)> {
    let nid = lp.tree().find_by_id(id)?;
    let node = lp.tree().get(nid)?;
    let ui_core::NodeData::Text { block, .. } = &node.data else {
        return None;
    };
    let line = node.inline.as_ref()?.lines.first()?;
    let rect = node.visual_rect();
    let pad = node.box_model.padding;
    let left = rect.origin.x + pad.left;
    let right = rect.origin.x + rect.size.w - pad.right;
    let spare = (right - left - line.width).max(0.0);
    let offset = match block.align {
        ui_core::TextAlign::Start | ui_core::TextAlign::Justify => 0.0,
        ui_core::TextAlign::Center => spare * 0.5,
        ui_core::TextAlign::End => spare,
    };
    Some((left + offset, left + offset + line.width, left, right))
}

/// The right edge of the last glyph a text node draws, as the paint pass placed it.
fn last_glyph_right(lp: &AppLoop<Gallery>, id: Id) -> Option<f32> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    list.prims
        .glyphs
        .iter()
        .zip(&list.owners.glyphs)
        .rfind(|(_, owner)| **owner == nid)
        .map(|(glyph, _)| glyph.rect.origin.x + glyph.rect.size.w)
}

/// The left edge of the first glyph a text node paints: the first byte of its own text,
/// because the paint pass lays the line's glyphs before its atoms' stand-ins.
fn first_glyph_left(lp: &AppLoop<Gallery>, id: Id) -> Option<f32> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    list.prims
        .glyphs
        .iter()
        .zip(&list.owners.glyphs)
        .find(|(_, owner)| **owner == nid)
        .map(|(glyph, _)| glyph.rect.origin.x)
}

/// The room a row's markers take before its own text, in the form it draws now.
///
/// A rendered marker is an atom's stand-in over a zero-room copy of its bytes; a revealed
/// one is the bytes themselves in a room of the cell's width. Both add up to the same
/// column, which is what keeps the text still when the marker is revealed.
fn marker_room(lp: &AppLoop<Gallery>, id: Id) -> Option<f32> {
    let nid = lp.tree().find_by_id(id)?;
    let node = lp.tree().get(nid)?;
    let ui_core::NodeData::Text { block, .. } = &node.data else {
        return None;
    };
    let rooms = block
        .runs
        .iter()
        .filter_map(|run| run.room.map(|room| room.width))
        .sum::<f32>();
    let atoms = block.atoms.iter().map(|atom| atom.size.w).sum::<f32>();
    Some(rooms + atoms)
}

/// The buffer a text input declared, which is what the engine edits.
fn input_value(lp: &AppLoop<Gallery>, id: Id) -> Option<String> {
    let nid = lp.tree().find_by_id(id)?;
    Some(
        lp.tree()
            .get(nid)?
            .data
            .params()?
            .text_input
            .as_deref()?
            .to_owned(),
    )
}

/// The first byte of the sample's quoted line, as the source now stands.
fn quote_line(source: &str) -> usize {
    let document = ui_richtext::parse(source);
    document
        .blocks
        .get(3usize)
        .and_then(|block| block.children().first())
        .map(|child| line_start(source, child.full.start))
        .unwrap_or(0)
}

/// The id of the bar the sample's quote draws: the quote's line, as the source now stands.
fn quote_bar_id(root: Id, source: &str) -> Id {
    root.with(3usize).with(("bar", quote_line(source)))
}

/// The first byte of the line holding `at`, as the scene counts lines.
fn line_start(source: &str, at: usize) -> usize {
    let at = at.min(source.len());
    source.as_bytes()[..at]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1)
}

fn visible(runs: &[(String, bool)]) -> String {
    runs.iter()
        .filter(|(_, hidden)| !*hidden)
        .map(|(text, _)| text.as_str())
        .collect()
}

/// The editor inside an engine-owned scroller. The gallery page itself does not scroll,
/// so this sample is what proves the reveal moves a real `SCROLL_OWNED` container.
/// The page's own viewport: a document taller than the window scrolls, and the caret
/// reveal keeps the line it is on inside the clip.
///
/// Before the page had a `SCROLL_OWNED` container a long document ran past the window and
/// the caret left the screen with it, which the sample above could only approximate.
pub(crate) fn markdown_page_scroll(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    goto(lp, size, Page::Markdown);
    let mut source = String::new();
    for line in 0..80 {
        source.push_str(&format!("line {line}\n\n"));
    }
    lp.app_mut().markdown_src = source;
    frame(lp, size, None);

    let scroller = Id::str("page/view");
    let Some(rect) = rect_of(lp, markdown_edit_root().with(0usize).with("text")) else {
        println!("selftest: page scroll text missing");
        return false;
    };
    click(
        lp,
        size,
        Point::new(rect.origin.x + 8.0, rect.origin.y + 8.0),
    );
    for _ in 0..120 {
        lp.dispatch(ui_core::Event::KeyPressed {
            key: ui_core::Key::Down,
        });
        frame(lp, size, None);
    }
    let offset = lp.scroll_of(scroller).unwrap_or(0.0);
    let (extent, viewport) = lp
        .tree()
        .find_by_id(scroller)
        .and_then(|nid| lp.tree().get(nid))
        .map(|node| (node.scroll_extent.unwrap_or_default().h, node.rect.size.h))
        .unwrap_or_default();
    let travel = (extent - viewport).max(0.0);
    let ok = travel > 0.0 && offset > 0.0 && offset <= travel + 0.5;
    println!(
        "selftest: page scroll offset {offset} extent {extent} viewport {viewport} {}",
        if ok { "✓" } else { "✗" }
    );
    ok
}

/// The code card's three knobs, each one wiring a frame the other two must not depend on:
/// numbers in the gutter, no wrap with a sideways strip, and a cap with its own scroller.
pub(crate) fn markdown_code_options(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    goto(lp, size, Page::Markdown);
    let mut source = String::from("```rust\n");
    // Long enough that the card's own width cannot hold a line unbroken.
    for line in 0..12 {
        source.push_str(&format!(
            "let row_{line} = measure(&runs, gutter, width, hanging_indent, baseline, scale);\n"
        ));
    }
    source.push_str("```\n");
    lp.app_mut().markdown_src = source;
    let block = markdown_edit_root().with(0usize);
    let text_node = |lp: &AppLoop<Gallery>, id: Id| {
        lp.tree()
            .find_by_id(id)
            .and_then(|nid| lp.tree().get(nid))
            .and_then(|node| match &node.data {
                ui_core::NodeData::Text { block, .. } => Some((
                    block.line_numbers.is_some(),
                    node.inline
                        .as_ref()
                        .map(|layout| layout.lines.len())
                        .unwrap_or(0),
                    node.rect,
                )),
                _ => None,
            })
    };
    // The column is its own node now, so its numbers and the code's lines are two reads.
    let numbers = |lp: &AppLoop<Gallery>| {
        text_node(lp, block.with("row").with("numbers"))
            .map(|(numbers, ..)| numbers)
            .unwrap_or(false)
    };
    let lines = |lp: &AppLoop<Gallery>| text_node(lp, block.with("text"));

    lp.app_mut().markdown_code = ui_widgets::CodeOptions {
        line_numbers: true,
        wrap: true,
        max_height: Some(ui_widgets::CodeHeight::Lines(CODE_CAP_LINES)),
    };
    frame(lp, size, None);
    let Some((_, body_lines, body)) = lines(lp) else {
        println!("selftest: code card text missing ✗");
        lp.app_mut().markdown_code = ui_widgets::CodeOptions::default();
        return false;
    };
    let gutter = numbers(lp);
    let capped = rect_of(lp, block.with("capped"));
    let viewport = capped.map(|rect| rect.size.h).unwrap_or(0.0);
    let cap = CODE_CAP_LINES as usize;
    let mut ok = gutter && body_lines > cap && viewport < body.size.h;
    println!(
        "selftest: code numbers {gutter} lines {body_lines} viewport {viewport} body {} {}",
        body.size.h,
        if ok { "✓" } else { "✗" }
    );

    // The cap's own scroller takes the wheel: an upright step over the code moves it down.
    let at = Point::new(
        capped.map_or(body.origin.x, |r| r.origin.x) + 40.0,
        body.origin.y + 10.0,
    );
    lp.dispatch(ui_core::Event::wheel(at, 160.0));
    frame(lp, size, None);
    let scrolled = lp
        .scroll_offset(block.with("capped"))
        .map(|offset| offset.y)
        .unwrap_or(0.0);
    ok &= scrolled > 0.0;
    println!(
        "selftest: code cap scroll {scrolled} {}",
        if scrolled > 0.0 { "✓" } else { "✗" }
    );

    // Without wrap the line is laid out whole and the strip carries the overflow.
    lp.app_mut().markdown_code = ui_widgets::CodeOptions {
        line_numbers: true,
        wrap: false,
        max_height: None,
    };
    frame(lp, size, None);
    let Some((_, unbroken, wide)) = lines(lp) else {
        println!("selftest: unbroken code card text missing ✗");
        lp.app_mut().markdown_code = ui_widgets::CodeOptions::default();
        return false;
    };
    let gutter = numbers(lp);
    let strip = rect_of(lp, block.with("sideways"));
    let strip_w = strip.map(|rect| rect.size.w).unwrap_or(0.0);
    // The strip is clipped from the rule on, so no scrolled glyph is drawn under the numbers.
    let clears_rule = strip
        .zip(rect_of(lp, block.with("gutter-rule")))
        .is_some_and(|(strip, rule)| strip.origin.x >= rule.origin.x + rule.size.w);
    // The strip scrolls one direction, so a plain wheel is spent on it with no modifier.
    lp.dispatch(ui_core::Event::wheel(
        Point::new(wide.origin.x + 40.0, wide.origin.y + 8.0),
        160.0,
    ));
    frame(lp, size, None);
    let sideways = lp
        .scroll_offset(block.with("sideways"))
        .map(|offset| offset.x)
        .unwrap_or(0.0);
    let unwrapped_ok =
        gutter && unbroken == 12 && wide.size.w > strip_w && sideways > 0.0 && clears_rule;
    ok &= unwrapped_ok;
    println!(
        "selftest: code unbroken {unbroken} lines wide {} strip {strip_w} rule {clears_rule} scroll {sideways} {}",
        wide.size.w,
        if unwrapped_ok { "✓" } else { "✗" }
    );

    lp.app_mut().markdown_code = ui_widgets::CodeOptions::default();
    frame(lp, size, None);
    ok
}

struct ScrolledMarkdown {
    source: String,
}

fn scrolled_markdown_root() -> Id {
    Id::str("selftest/markdown/editor")
}

fn scrolled_markdown_scroller() -> Id {
    Id::str("selftest/markdown/scroll")
}

impl App for ScrolledMarkdown {
    fn view(&mut self, core: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let cx = Cx::new(core, root_env());
        let document = ui_richtext::parse(&self.source);
        let prepared = ui_richtext::Prepared::of(&document);
        let control = ui_widgets::Markdown {
            id: scrolled_markdown_root(),
            document: std::sync::Arc::new(document),
            prepared: Some(std::sync::Arc::new(prepared)),
            editable: true,
            math_editing: ui_widgets::MathEditing::Structural,
            popover_fill: Color::WHITE,
            width: 240.0,
            size: 14.0,
            ink: Color::from_rgba8(32, 35, 42, 255),
            marker_ink: Color::from_rgba8(60, 66, 78, 255),
            link_ink: Color::from_rgba8(20, 90, 200, 255),
            code_ink: Color::from_rgba8(60, 66, 78, 255),
            code_fill: Color::rgba(0.0, 0.0, 0.0, 0.06),
            code_style: None,
            code: ui_widgets::CodeOptions::default(),
            image: None,
            muted: Color::from_rgba8(120, 120, 120, 255),
            caret_ink: Color::from_rgba8(32, 35, 42, 255),
            selection_fill: Color::rgba(0.2, 0.4, 0.9, 0.3),
            selection_painter: None,
        };
        let scroller =
            ui_widgets::Scroll::fixed(scrolled_markdown_scroller(), 260.0, 120.0).no_scrollbar();
        Box::new(
            scroller
                .element_in(&cx)
                .child(ui_widgets::from_decl(control.resolve_in(&cx))),
        )
    }

    fn changed(&mut self, _id: Id, change: ui_widgets::Change) -> bool {
        if let ui_widgets::Change::Text(next) = change {
            self.source = next;
            return true;
        }
        false
    }
}

/// A document with one block image, drawn by a page that names its textures.
struct ImageMarkdown {
    source: String,
    image: Option<ui_widgets::ImageSource>,
}

const IMAGE_WIDTH: f32 = 260.0;
const IMAGE_TEXTURE: u64 = 0x696d_6167_655f_7365;

fn image_root() -> Id {
    Id::str("selftest/markdown/image-block")
}

fn sample_image(destination: &str) -> Option<ui_widgets::ImageTexture> {
    (destination == "tex").then_some(ui_widgets::ImageTexture {
        id: IMAGE_TEXTURE,
        width: 40,
        height: 20,
    })
}

fn missing_image(_destination: &str) -> Option<ui_widgets::ImageTexture> {
    None
}

impl App for ImageMarkdown {
    fn view(&mut self, core: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let cx = Cx::new(core, root_env());
        let document = ui_richtext::parse(&self.source);
        let control = ui_widgets::Markdown {
            id: image_root(),
            document: std::sync::Arc::new(document),
            prepared: None,
            editable: true,
            math_editing: ui_widgets::MathEditing::Structural,
            popover_fill: Color::WHITE,
            width: IMAGE_WIDTH,
            size: 14.0,
            ink: Color::from_rgba8(32, 35, 42, 255),
            marker_ink: Color::from_rgba8(60, 66, 78, 255),
            link_ink: Color::from_rgba8(20, 90, 200, 255),
            code_ink: Color::from_rgba8(60, 66, 78, 255),
            code_fill: Color::rgba(0.0, 0.0, 0.0, 0.06),
            code_style: None,
            code: ui_widgets::CodeOptions::default(),
            image: self.image,
            muted: Color::from_rgba8(120, 120, 120, 255),
            caret_ink: Color::from_rgba8(32, 35, 42, 255),
            selection_fill: Color::rgba(0.2, 0.4, 0.9, 0.3),
            selection_painter: None,
        };
        Box::new(ui_widgets::from_decl(control.resolve_in(&cx)))
    }

    fn changed(&mut self, _id: Id, change: ui_widgets::Change) -> bool {
        if let ui_widgets::Change::Text(next) = change {
            self.source = next;
            return true;
        }
        false
    }
}

/// The page's font and the 40x20 picture the sample refers to.
fn install_image_sample<A: App>(lp: &mut AppLoop<A>) {
    lp.register_font(
        TEXT_FAMILY,
        include_bytes!("../../../../assets/fonts/InterVariable.ttf")
            .as_slice()
            .into(),
    );
    lp.register_image(ui_core::draw::ImageEntry {
        id: IMAGE_TEXTURE,
        width: 40,
        height: 20,
        source: ui_core::draw::ImageSource::Pixels(std::sync::Arc::new(vec![0u8; 40 * 20 * 4])),
        colour: ui_core::colour::ContentColour::SDR,
        sampling: ui_core::draw::Sampling::Smooth,
    });
}

fn image_press<A: App>(lp: &mut AppLoop<A>, target: &mut ui_platform::OffscreenTarget, at: Point) {
    let button = ui_core::PointerButton::Primary;
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    lp.frame(target, None);
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    lp.frame(target, None);
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    lp.frame(target, None);
}

/// The rectangle the picture itself is painted in, if the node drew one.
fn image_rect<A: App>(lp: &AppLoop<A>, id: Id) -> Option<Rect> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    list.prims
        .images
        .iter()
        .zip(&list.owners.images)
        .find(|(_, owner)| **owner == nid)
        .map(|(prim, _)| prim.rect)
}

/// `rect_of` for a loop other than the gallery's.
fn any_rect<A: App>(lp: &AppLoop<A>, id: Id) -> Option<Rect> {
    let nid = lp.tree().find_by_id(id)?;
    lp.tree().get(nid).map(|node| node.visual_rect())
}
