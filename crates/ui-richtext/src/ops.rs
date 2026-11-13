//! The edits a document editor applies at the caret: a line break that continues the list
//! or quote the caret is in, and an indent that nests the caret's line.
//!
//! Each answers with what the source becomes, never with a buffer: the caller applies the
//! edit through its input so the engine records it in the history and moves the caret.

use std::ops::Range;

/// A replacement of `range` with `text`, and where the caret lands once it is applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// The bytes the edit replaces; empty for an insertion.
    pub range: Range<usize>,
    pub text: String,
    /// The caret's offset in the buffer the edit produces.
    pub caret: usize,
    /// The selection the edit leaves, as `(anchor, caret)` in the buffer it produces. `None`
    /// leaves a collapsed caret at `caret`; an indent keeps a selection over its lines.
    pub selection: Option<(usize, usize)>,
}

/// The bytes opening the caret's line, when they open a construct that a line break
/// continues: a list item's indent, marker and spacing, or one or more quote markers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Prefix {
    range: Range<usize>,
    /// What the next line repeats, already carrying any increment a number needs.
    repeat: String,
    /// The innermost construct is a quote, whose paragraphs a break separates rather than
    /// lines of one paragraph.
    quote: bool,
}

/// A line break over `selection`: on a line that opens a list or a quote it continues that
/// construct, and on one whose content is empty it ends the construct instead.
///
/// One break writes one line, whatever the construct: the next line repeats the prefix the
/// current one opens with. There is no construct-specific blank line and no two-line spell
/// of a paragraph break — a second break, on the now empty line, closes the construct.
pub fn line_break(source: &str, selection: Range<usize>) -> Edit {
    if selection.start != selection.end {
        return Edit {
            range: selection.clone(),
            text: "\n".to_owned(),
            caret: selection.start + 1,
            selection: None,
        };
    }
    let at = selection.start.min(source.len());
    let line = line_at(source, at);
    let prefix = prefix(&source[line.clone()]);
    let Some(prefix) = prefix else {
        return Edit {
            range: at..at,
            text: "\n".to_owned(),
            caret: at + 1,
            selection: None,
        };
    };
    let content = &source[line.start + prefix.range.len()..line.end];
    if content.trim().is_empty() {
        // An empty item or quote line ends the construct: its marker goes and the line the
        // caret is on stays empty, so what follows starts outside.
        if !prefix.quote
            && let Some(tail) = renumber(source, line.clone(), true)
        {
            return Edit {
                range: line.start + prefix.range.start..tail.end,
                text: tail.text,
                caret: at.saturating_sub(prefix.range.len()),
                selection: None,
            };
        }
        return Edit {
            range: line.start + prefix.range.start..line.start + prefix.range.end,
            text: String::new(),
            caret: at.saturating_sub(prefix.range.len()),
            selection: None,
        };
    }
    let text = format!("\n{}", prefix.repeat);
    // A new ordered item renumbers the items after it, or the list would read 1, 2, 2, 3.
    // The one edit then spans the caret to the end of the last item it renumbers, and the
    // bytes between come along unchanged, so the undo history still takes one step.
    if !prefix.quote
        && let Some(tail) = renumber(source, line.clone(), false)
    {
        let caret = at + text.len();
        return Edit {
            range: at..tail.end,
            text: text + &source[at..line.end] + &tail.text,
            caret,
            selection: None,
        };
    }
    Edit {
        range: at..at,
        text: text.clone(),
        caret: at + text.len(),
        selection: None,
    }
}

/// The bytes of a list after `line` that a break renumbers, and where they end.
struct Tail {
    end: usize,
    /// The bytes from the line's end on, each item's marker renumbered.
    text: String,
}

/// An ordered item's marker as it opens a line: the bytes before its number, the number
/// and the delimiter.
struct Ordered<'a> {
    lead: &'a str,
    /// Where the number starts, and where its digits end.
    start: usize,
    digits_end: usize,
    number: u64,
    delimiter: char,
}

/// The marker of an ordered item at the start of `line`, under any quote markers and
/// indentation: a number, `.` or `)`, and the space a marker needs.
fn ordered(line: &str) -> Option<Ordered<'_>> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let mut at = indent;
    while let Some(after) = line[at..].strip_prefix('>') {
        let space = after.len() - after.trim_start_matches(' ').len();
        at += 1 + space.min(1);
    }
    let rest = &line[at..];
    let digits_end = at
        + rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
    if digits_end == at || digits_end - at > 9 {
        return None;
    }
    let delimiter = line[digits_end..].chars().next()?;
    if delimiter != '.' && delimiter != ')' {
        return None;
    }
    let space = line[digits_end + 1..].len() - line[digits_end + 1..].trim_start_matches(' ').len();
    if space == 0 {
        return None;
    }
    Some(Ordered {
        lead: &line[..at],
        start: at,
        digits_end,
        number: line[at..digits_end].parse().ok()?,
        delimiter,
    })
}

/// The items after `line` with their numbers written again from `first`, so a break that
/// adds or removes an item leaves one sequence. `None` when `line` opens no ordered item,
/// or when no item follows it.
fn renumber(source: &str, line: Range<usize>, removed: bool) -> Option<Tail> {
    let head = ordered(&source[line.clone()])?;
    let (lead, delimiter) = (head.lead, head.delimiter);
    // The item the break starts takes the number after this one, so the following items
    // start one past that; a removed item shifts them onto its own number.
    let first = if removed {
        head.number
    } else {
        head.number + 2
    };
    let mut following: Vec<Range<usize>> = Vec::new();
    let mut cursor = line.end;
    let mut blank = false;
    while cursor < source.len() {
        let next = line_at(source, cursor + 1);
        let body = &source[next.clone()];
        if body.trim().is_empty() {
            following.push(next.clone());
            blank = true;
            cursor = next.end;
            continue;
        }
        match ordered(body) {
            Some(item) if item.lead == lead && item.delimiter == delimiter => {
                following.push(next.clone());
                blank = false;
                cursor = next.end;
            }
            // A blank line ends the list; without one the line continues the item above.
            _ if blank => break,
            _ => {
                following.push(next.clone());
                blank = false;
                cursor = next.end;
            }
        }
    }
    while following
        .last()
        .is_some_and(|line| source[line.clone()].trim().is_empty())
    {
        following.pop();
    }
    if !following.iter().any(|line| {
        ordered(&source[line.clone()])
            .is_some_and(|item| item.lead == lead && item.delimiter == delimiter)
    }) {
        return None;
    }
    let mut text = String::new();
    let mut number = first;
    for line in &following {
        let body = &source[line.clone()];
        text.push('\n');
        if body.trim().is_empty() {
            continue;
        }
        match ordered(body) {
            Some(item) if item.lead == lead && item.delimiter == delimiter => {
                text.push_str(&body[..item.start]);
                text.push_str(&number.to_string());
                number += 1;
                text.push_str(&body[item.digits_end..]);
            }
            _ => text.push_str(body),
        }
    }
    Some(Tail {
        end: following.last().map_or(line.end, |line| line.end),
        text,
    })
}

/// An indent of every line the selection covers by one step, or an outdent by one step.
///
/// A line indents by the width that nests it under the item above: that item's own marker,
/// trailing space included, or two spaces when the line above is not an item. A selection
/// uses its first line's step for every line, so the lines keep one indentation. An
/// outdent removes each line's own leading spaces, up to that step; a line with none
/// leaves the edit without bytes to remove.
pub fn indent(source: &str, selection: Range<usize>, outdent: bool) -> Option<Edit> {
    let start = selection.start.min(source.len());
    let end = selection.end.min(source.len());
    let (start, end) = (start.min(end), start.max(end));
    if start == end {
        return indent_line(source, line_at(source, start), start, outdent);
    }
    let first = line_at(source, start);
    let width = step(source, first.clone());
    let end_line = line_at(source, end);
    // A selection that stops at a line's first byte does not touch that line: the line
    // break before it is where the selection ends, as in any editor.
    let last_at = if end == end_line.start {
        end_line.start.saturating_sub(1)
    } else {
        end
    };
    let lines = lines_between(source, first.start, line_at(source, last_at).start);
    if outdent {
        let mut text = String::new();
        let mut edits = Vec::with_capacity(lines.len());
        let mut changed = false;
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            let room = source[line.start..line.end]
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .count()
                .min(width);
            changed |= room > 0;
            text.push_str(&source[line.start + room..line.end]);
            edits.push((line.start, -(room as isize)));
        }
        if !changed {
            return None;
        }
        let range = first.start..lines.last()?.end;
        return Some(Edit {
            range,
            text,
            caret: map_offset(end, &edits),
            selection: Some((map_offset(start, &edits), map_offset(end, &edits))),
        });
    }
    let padding = " ".repeat(width);
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(&padding);
        text.push_str(&source[line.clone()]);
    }
    let range = first.start..lines.last()?.end;
    let edits: Vec<(usize, isize)> = lines
        .iter()
        .map(|line| (line.start, width as isize))
        .collect();
    let new_start = map_offset(start, &edits);
    let new_end = map_offset(end, &edits);
    Some(Edit {
        range,
        text,
        caret: new_end,
        selection: Some((new_start, new_end)),
    })
}

/// An indent or outdent of one line: the caret keeps its place relative to the line's start.
fn indent_line(source: &str, line: Range<usize>, at: usize, outdent: bool) -> Option<Edit> {
    let start = line.start;
    if outdent {
        let width = source[start..line.end]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count()
            .min(step(source, line));
        if width == 0 {
            return None;
        }
        return Some(Edit {
            range: start..start + width,
            text: String::new(),
            caret: at.saturating_sub(width),
            selection: None,
        });
    }
    let width = step(source, line);
    let text = " ".repeat(width);
    Some(Edit {
        range: start..start,
        text: text.clone(),
        caret: at + width,
        selection: None,
    })
}

/// The line ranges from `from` (a line's start) through the line starting at `last_start`.
fn lines_between(source: &str, from: usize, last_start: usize) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut at = from;
    loop {
        let line = line_at(source, at);
        let done = line.start == last_start || line.end >= source.len();
        lines.push(line.clone());
        if done {
            return lines;
        }
        at = line.end + 1;
    }
}

/// Where a byte offset lands after edits at line starts: `(position, delta)`, a positive
/// delta inserting and a negative one deleting. A position inside deleted bytes clamps to
/// the deletion's start.
fn map_offset(at: usize, edits: &[(usize, isize)]) -> usize {
    let mut out = at as isize;
    for &(position, delta) in edits {
        if delta >= 0 {
            if position <= at {
                out += delta;
            }
        } else {
            let removed = (-delta) as usize;
            if at >= position + removed {
                out -= removed as isize;
            } else if at > position {
                out -= (at - position) as isize;
            }
        }
    }
    out.max(0) as usize
}

/// How far a line of `source` indents: the marker above it, else two spaces.
///
/// Nested content must be indented at least as far as the parent item's content, so a
/// marker of `1. ` nests a line by three and one of `- ` by two. Two is the floor for a
/// paragraph, where CommonMark reads any indent of two spaces and more the same.
fn step(source: &str, line: Range<usize>) -> usize {
    let above = source[..line.start].trim_end_matches('\n');
    let above_start = above.rfind('\n').map_or(0, |at| at + 1);
    match prefix(&source[above_start..above.len()]) {
        Some(prefix) if !prefix.repeat.trim_start().starts_with('>') => prefix.repeat.len(),
        _ => 2,
    }
}

/// The line holding byte `at`: its start, and the offset of its line break or the end.
fn line_at(source: &str, at: usize) -> Range<usize> {
    let start = source[..at].rfind('\n').map_or(0, |at| at + 1);
    let end = source[at..]
        .find('\n')
        .map_or(source.len(), |offset| at + offset);
    start..end
}

/// The construct opening `line`, if a break continues one.
///
/// Quote markers repeat as written, so a nested `> > ` keeps both levels and a `>  ` keeps
/// its extra space. A list marker may follow them; it is the innermost construct a line
/// break continues.
fn prefix(line: &str) -> Option<Prefix> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let mut at = indent;
    while let Some(after) = line[at..].strip_prefix('>') {
        let space = after.len() - after.trim_start_matches(' ').len();
        at += 1 + space.min(1);
    }
    let quoted = at > indent;
    if let Some(marker) = list_marker(&line[at..]) {
        let end = at + marker.len();
        return Some(Prefix {
            range: 0..end,
            repeat: format!("{}{}", &line[..at], marker_repeat(&marker)),
            quote: false,
        });
    }
    quoted.then(|| Prefix {
        range: 0..at,
        repeat: line[..at].to_owned(),
        quote: true,
    })
}

/// The bytes of a bullet or an ordered marker and the space after it.
fn list_marker(rest: &str) -> Option<String> {
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    let digits = match first {
        '-' | '+' | '*' => {
            let space = rest[1..].len() - rest[1..].trim_start_matches(' ').len();
            return (space > 0).then(|| rest[..1 + space].to_owned());
        }
        c if c.is_ascii_digit() => {
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            (1..=9).contains(&end).then_some(end)?
        }
        _ => return None,
    };
    let delim = rest[digits..].chars().next()?;
    if delim != '.' && delim != ')' {
        return None;
    }
    let space = rest[digits + 1..].len() - rest[digits + 1..].trim_start_matches(' ').len();
    (space > 0).then(|| rest[..digits + 1 + space].to_owned())
}

/// The marker a continued item writes: a bullet stays, a number counts up.
fn marker_repeat(marker: &str) -> String {
    let digits = marker.len()
        - marker
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    if digits == 0 {
        return marker.to_owned();
    }
    let number: u64 = marker[..digits].parse().unwrap_or(1);
    format!("{}{}", number + 1, &marker[digits..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(caret: usize) -> Range<usize> {
        caret..caret
    }

    fn applied(source: &str, edit: &Edit) -> String {
        let mut out = source.to_owned();
        out.replace_range(edit.range.clone(), &edit.text);
        out
    }

    #[test]
    fn a_break_continues_the_list_with_the_marker_of_its_own_line() {
        // The caret sits mid-item: the item splits there and the rest keeps the marker.
        let source = "- A bullet holds blocks\n- Nesting works:\n";
        let caret = source.find("works").unwrap();
        let edit = line_break(source, at(caret));
        assert_eq!(
            applied(source, &edit),
            "- A bullet holds blocks\n- Nesting \n- works:\n"
        );
        assert_eq!(edit.caret, caret + 3);
    }

    #[test]
    fn a_break_on_an_empty_item_ends_the_list() {
        let source = "- first\n- \n";
        let edit = line_break(source, at(source.len() - 1));
        assert_eq!(applied(source, &edit), "- first\n\n");
        assert_eq!(edit.caret, source.len() - 3);
    }

    #[test]
    fn a_break_writes_one_line_whatever_the_construct() {
        let source = "1. one\n";
        let edit = line_break(source, at(source.len() - 1));
        assert_eq!(applied(source, &edit), "1. one\n2. \n");

        // A quote continues the same way: one break, one line, carrying the prefix of the
        // line it came from.
        let source = "> outer\n> > inner\n";
        let caret = source.find("inner").unwrap();
        let edit = line_break(source, at(caret));
        assert_eq!(applied(source, &edit), "> outer\n> > \n> > inner\n");

        // A quote line with no content ends the quote, as an empty item ends its list.
        let source = "> \n";
        let edit = line_break(source, at(1));
        assert_eq!(applied(source, &edit), "\n");
    }

    #[test]
    fn a_break_on_an_empty_quote_line_closes_the_quote() {
        // The line the break opened is empty: a second break takes its marker away, as a
        // second break on an empty item ends its list.
        let source = "> said\n> ";
        let edit = line_break(source, at(source.len()));
        assert_eq!(applied(source, &edit), "> said\n");
        assert_eq!(edit.caret, 7);

        // A blank quote line between two paragraphs stays: it is content there.
        let source = "> said\n> \n> more\n";
        let edit = line_break(source, at(8));
        assert_eq!(applied(source, &edit), "> said\n\n> more\n");
    }

    #[test]
    fn a_break_renumbers_the_ordered_items_after_it() {
        // The new item takes the next number, so the items below count on from there
        // instead of repeating it.
        let source = "1. one\n2. two\n3. three\n";
        let edit = line_break(source, at(source.find("one").unwrap() + 3));
        assert_eq!(applied(source, &edit), "1. one\n2. \n3. two\n4. three\n");

        // A nested list under the item is not part of this one's sequence, so it comes
        // along unchanged while the item below counts on.
        let source = "1. one\n   - a\n2. two\n";
        let edit = line_break(source, at(source.find("one").unwrap() + 3));
        assert_eq!(applied(source, &edit), "1. one\n2. \n   - a\n3. two\n");

        // No item below: the break is the plain insert of the next marker.
        let source = "1. one\n";
        let edit = line_break(source, at(6));
        assert_eq!(applied(source, &edit), "1. one\n2. \n");

        // An empty item leaves, and the items after it count down onto its number.
        let source = "1. one\n2. \n3. three\n";
        let edit = line_break(source, at(source.find("2. ").unwrap() + 3));
        assert_eq!(applied(source, &edit), "1. one\n\n2. three\n");
    }

    #[test]
    fn a_tab_nests_by_the_marker_above_and_shift_tab_takes_it_back() {
        let source = "- first\n- second\n";
        let caret = source.find("second").unwrap();
        let edit = indent(source, at(caret), false).expect("an indent");
        assert_eq!(applied(source, &edit), "- first\n  - second\n");
        assert_eq!(edit.caret, caret + 2);

        let nested = applied(source, &edit);
        let caret = edit.caret;
        let back = indent(&nested, at(caret), true).expect("an outdent");
        assert_eq!(applied(&nested, &back), source);

        // An ordered item above nests by its own marker, three bytes.
        let source = "1. one\n2. two\n";
        let caret = source.find("two").unwrap();
        let edit = indent(source, at(caret), false).expect("an indent");
        assert_eq!(applied(source, &edit), "1. one\n   2. two\n");
    }

    #[test]
    fn an_indent_of_a_selection_reaches_every_line_and_leaves_the_selection_on_them() {
        // Three lines with a blank one between: every covered line takes the same step, and
        // the blank line is no exception. One edit spans them all, so one undo restores them.
        let source = "one\n\ntwo\n";
        let edit = indent(source, 0..8, false).expect("an indent");
        assert_eq!(applied(source, &edit), "  one\n  \n  two\n");
        assert_eq!(edit.selection, Some((2, 14)));

        // The selection still covers the same three lines, so Shift+Tab takes the same step
        // back from every one of them.
        let indented = applied(source, &edit);
        let (anchor, caret) = edit.selection.expect("a selection");
        let back = indent(&indented, anchor..caret, true).expect("an outdent");
        assert_eq!(applied(&indented, &back), source);
        assert_eq!(back.selection, Some((0, 8)));
    }

    #[test]
    fn an_outdent_at_the_margin_changes_nothing() {
        let source = "- first\n";
        assert!(indent(source, at(3), true).is_none());
    }
}
