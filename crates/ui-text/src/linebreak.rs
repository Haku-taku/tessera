//! Greedy line breaking over UAX #14 opportunities.
//!
//! Layout measures with this and paint positions with this, so the two cannot disagree
//! about where the lines are.

use std::ops::Range;

use unicode_linebreak::BreakOpportunity::Mandatory;

/// One visual line: a byte range into the source plus its advance width.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The line's bytes: a hard-broken line keeps the spaces before its break, because a
    /// caret stands inside them; a wrapped line hangs them, so they belong to no line.
    pub range: Range<usize>,
    pub width: f32,
    /// True when this line ends with an ellipsis appended by `wrap`.
    pub ellipsis: bool,
}

/// Break `text` into lines no wider than `max_width`.
///
/// `width_of` is measured once per segment and width, so the greedy pass stays linear;
/// kerning across a break opportunity is therefore not counted, which no line count
/// depends on.
pub fn break_lines<F>(text: &str, max_width: f32, width_of: F) -> Vec<Line>
where
    F: FnMut(&str, Range<usize>) -> f32,
{
    break_lines_indented(text, max_width, 0.0, 0.0, width_of)
}

/// Widest unbreakable segment: the narrowest width this text can be laid out at.
pub fn min_content_width<F>(text: &str, mut width_of: F) -> f32
where
    F: FnMut(&str, Range<usize>) -> f32,
{
    let mut widest = 0.0f32;
    let mut segment = |start: usize, end: usize| {
        let visible = trim_end(text, start, end);
        if visible > start {
            width_of(&text[start..visible], start..visible)
        } else {
            0.0
        }
    };
    let mut start = 0usize;
    for (index, _) in unicode_linebreak::linebreaks(text) {
        if index <= start {
            continue;
        }
        widest = widest.max(segment(start, index));
        start = index;
    }
    if start < text.len() {
        widest = widest.max(segment(start, text.len()));
    }
    widest
}

/// Like [`break_lines`], but the lines have less room at their left edge: the first line
/// gives up `indent`, every later one `hanging`.
///
/// Both must participate in wrapping, or a line overflows into the room it was offset by
/// and the paragraph looks right-aligned by accident.
pub fn break_lines_indented<F>(
    text: &str,
    max_width: f32,
    indent: f32,
    hanging: f32,
    mut width_of: F,
) -> Vec<Line>
where
    F: FnMut(&str, Range<usize>) -> f32,
{
    if text.is_empty() {
        return vec![Line {
            range: 0..0,
            width: 0.0,
            ellipsis: false,
        }];
    }

    // Segment at every opportunity; `ink` trims the trailing whitespace, `body` does not.
    // A hard segment drops only its break, so the spaces a line ends at keep their room.
    let mut segments: Vec<(Range<usize>, f32, f32, bool)> = Vec::new();
    let mut start = 0usize;
    for (index, opportunity) in unicode_linebreak::linebreaks(text) {
        if index <= start {
            continue;
        }
        let mandatory = opportunity == Mandatory;
        let visible = trim_end(text, start, index);
        let body = if mandatory {
            trim_break(text, start, index)
        } else {
            index
        };
        segments.push((
            start..index,
            width_of(&text[start..body], start..body),
            width_of(&text[start..visible], start..visible),
            mandatory,
        ));
        start = index;
    }
    if start < text.len() {
        let visible = trim_end(text, start, text.len());
        let body = trim_break(text, start, text.len());
        segments.push((
            start..text.len(),
            width_of(&text[start..body], start..body),
            width_of(&text[start..visible], start..visible),
            true,
        ));
    }

    let wrap = max_width.is_finite() && max_width > 0.0;
    let first_limit = (max_width - indent.max(0.0)).max(0.0);
    let body_limit = (max_width - hanging.max(0.0)).max(0.0);
    // Count lines, not segments: `first` is a segment index and stays 0 when the text has
    // no break opportunity, which would apply the indent to every line.
    let mut line_no = 0usize;
    let mut lines = Vec::new();
    let mut first = 0usize;
    // Width of the segments already on this line, including the spaces between them.
    let mut prefix = 0.0f32;
    let mut ink = 0.0f32;
    let mut last_end = segments.first().map_or(0, |s| s.0.start);

    for (i, (range, body, trimmed, mandatory)) in segments.iter().enumerate() {
        let starts_line = i == first;
        // Wrapping asks about ink: a line may end in spaces that would not fit.
        let candidate = prefix + trimmed;
        let limit = if line_no == 0 {
            first_limit
        } else {
            body_limit
        };
        if !starts_line && wrap && candidate > limit {
            lines.push(Line {
                range: segments[first].0.start..last_end,
                width: ink,
                ellipsis: false,
            });
            line_no += 1;
            first = i;
            prefix = 0.0;
        }
        let candidate = if i == first {
            *trimmed
        } else {
            prefix + trimmed
        };
        prefix += body;
        ink = candidate;
        last_end = if *mandatory {
            trim_break(text, range.start, range.end)
        } else {
            trim_end(text, range.start, range.end)
        };
        if *mandatory {
            lines.push(Line {
                range: segments[first].0.start..last_end,
                width: prefix,
                ellipsis: false,
            });
            line_no += 1;
            first = i + 1;
            prefix = 0.0;
            ink = 0.0;
        }
    }
    if first < segments.len() {
        lines.push(Line {
            range: segments[first].0.start..last_end,
            width: ink,
            ellipsis: false,
        });
    } else if segments
        .last()
        .is_some_and(|s| text[s.0.clone()].ends_with('\n'))
    {
        // The last segment closed on a hard break at the buffer end, so text lives after it:
        // `white-space: pre` keeps that empty final line, and the editor's caret needs it.
        lines.push(Line {
            range: text.len()..text.len(),
            width: 0.0,
            ellipsis: false,
        });
    }

    // A single segment wider than the limit still has to fit: cut it by cluster.
    if wrap {
        let mut out = Vec::with_capacity(lines.len());
        for (index, line) in lines.into_iter().enumerate() {
            let limit = if index == 0 { first_limit } else { body_limit };
            // The spaces a line ends at take room but hang: they may not wrap the line.
            let ink = if line.width <= limit {
                line.width
            } else {
                let visible = trim_end(text, line.range.start, line.range.end);
                width_of(&text[line.range.start..visible], line.range.start..visible)
            };
            if ink <= limit {
                out.push(line);
                continue;
            }
            out.extend(split_by_cluster_with(
                text,
                line.range,
                limit,
                body_limit,
                &mut width_of,
            ));
        }
        lines = out;
    }
    lines
}

/// Appended to the last line when the text does not fit in `max_lines`.
pub const ELLIPSIS: &str = "…";

/// Like [`break_lines`], and if `max_lines` is exceeded, end the last line with an ellipsis.
pub fn wrap<F>(
    text: &str,
    max_width: f32,
    max_lines: Option<u32>,
    ellipsis_width: f32,
    width_of: F,
) -> Vec<Line>
where
    F: FnMut(&str, Range<usize>) -> f32,
{
    wrap_indented(
        text,
        max_width,
        max_lines,
        0.0,
        0.0,
        ellipsis_width,
        width_of,
    )
}

/// Like [`wrap`], with the same narrower left edge as [`break_lines_indented`].
///
/// The truncated line's `range` excludes the ellipsis; painters append `ELLIPSIS` when
/// `Line::ellipsis` is set, so measurement and painting use the same string.
///
/// `ellipsis_width` is the advance of [`ELLIPSIS`] in the block's own style: the source has
/// no ellipsis to measure, so the measurer reports it rather than the wrap guessing.
pub fn wrap_indented<F>(
    text: &str,
    max_width: f32,
    max_lines: Option<u32>,
    indent: f32,
    hanging: f32,
    ellipsis_width: f32,
    mut width_of: F,
) -> Vec<Line>
where
    F: FnMut(&str, Range<usize>) -> f32,
{
    let mut lines = break_lines_indented(text, max_width, indent, hanging, &mut width_of);
    let Some(limit) = max_lines.filter(|l| *l > 0) else {
        return lines;
    };
    let limit = limit as usize;
    if lines.len() <= limit {
        return lines;
    }

    // Keep the first `limit - 1` lines and fit what remains into one ellipsized line. The
    // cut never leaves the last kept line: a hard break ends it, and the next line's text is
    // not part of this one.
    lines.truncate(limit);
    let kept = lines[limit - 1].range.clone();
    let start = kept.start;
    // The kept line narrows by its own offset, exactly as in the untruncated pass.
    let room = if limit == 1 {
        (max_width - indent.max(0.0)).max(0.0)
    } else {
        (max_width - hanging.max(0.0)).max(0.0)
    };
    let mut cut = start;
    for (offset, cluster) in ui_core::grapheme::clusters(&text[kept]) {
        let end = start + offset + cluster.len();
        if width_of(&text[start..end], start..end) + ellipsis_width > room {
            break;
        }
        cut = end;
    }
    lines[limit - 1] = Line {
        range: start..cut,
        width: width_of(&text[start..cut], start..cut) + ellipsis_width,
        ellipsis: true,
    };
    lines
}

/// Split one over-long range at cluster boundaries, allowing a narrower first piece.
fn split_by_cluster_with<F>(
    text: &str,
    range: Range<usize>,
    first_width: f32,
    max_width: f32,
    width_of: &mut F,
) -> Vec<Line>
where
    F: FnMut(&str, Range<usize>) -> f32,
{
    let mut out = Vec::new();
    let mut start = range.start;
    let mut last_fit = range.start;
    for (offset, cluster) in ui_core::grapheme::clusters(&text[range.clone()]) {
        let end = range.start + offset + cluster.len();
        let width = width_of(&text[start..end], start..end);
        let limit = if out.is_empty() {
            first_width
        } else {
            max_width
        };
        if width > limit && end > start {
            // A cluster wider than the limit still takes a line of its own; never split it.
            let first = ui_core::grapheme::next(text, start);
            let cut = last_fit.max(first);
            out.push(Line {
                range: start..cut,
                width: width_of(&text[start..cut], start..cut),
                ellipsis: false,
            });
            start = cut;
            last_fit = start;
            continue;
        }
        last_fit = end;
    }
    if start < range.end {
        out.push(Line {
            range: start..range.end,
            width: width_of(&text[start..range.end], start..range.end),
            ellipsis: false,
        });
    }
    out
}

/// Whether trailing white of this kind hangs past the line's end.
///
/// CSS Text 4 §4.3.2 hangs a space, a tab and an ideographic space; a no-break space is
/// ink that stays on the line, and so does any other separator it does not name.
fn hangs(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{3000}')
}

/// End of `text[start..end]` with hanging whitespace removed.
fn trim_end(text: &str, start: usize, end: usize) -> usize {
    let slice = &text[start..end];
    start + slice.trim_end_matches(hangs).len()
}

/// End of `text[start..end]` with only its line break removed: the spaces stay with the line.
fn trim_break(text: &str, start: usize, end: usize) -> usize {
    let slice = &text[start..end];
    start + slice.trim_end_matches(['\n', '\r']).len()
}

/// Where a justified line adds space: one gap at every break opportunity inside it.
///
/// `extra` is what the line is short of its target width, shared equally between the
/// gaps. The opportunity at the end of the line separates nothing, so it is dropped; a
/// line with no opportunity at all — one long word — cannot be stretched and adds none.
/// Latin lines widen at their spaces and CJK lines between their characters, which is
/// exactly what UAX #14 reports for each.
pub fn justify_gaps(line: &str, extra: f32) -> Vec<(usize, f32)> {
    if !(extra > 0.0) || line.is_empty() {
        return Vec::new();
    }
    let mut at: Vec<usize> = Vec::new();
    for (index, _) in unicode_linebreak::linebreaks(line) {
        if index > 0 && index < line.len() {
            at.push(index);
        }
    }
    if at.is_empty() {
        return Vec::new();
    }
    let each = extra / at.len() as f32;
    at.into_iter().map(|index| (index, each)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten units per character, so expected widths are easy to state.
    fn ten(s: &str, _: Range<usize>) -> f32 {
        s.chars().count() as f32 * 10.0
    }

    fn lines(text: &str, max: f32) -> Vec<(Range<usize>, f32)> {
        break_lines(text, max, ten)
            .into_iter()
            .map(|l| (l.range, l.width))
            .collect()
    }

    /// The truncation must reserve the ellipsis it is about to draw, or the line overflows.
    #[test]
    fn a_truncated_line_reserves_room_for_its_ellipsis() {
        // The real measurers read the range, not the slice, so this closure does too.
        let by_range = |_: &str, r: Range<usize>| r.end.saturating_sub(r.start) as f32 * 10.0;
        let out = wrap_indented("aaaa bbbb", 49.0, Some(1), 0.0, 0.0, 15.0, by_range);
        assert_eq!(&"aaaa bbbb"[out[0].range.clone()], "aaa");
        assert_eq!(out[0].width, 45.0, "three characters plus the ellipsis");
        assert!(out[0].ellipsis);
    }

    #[test]
    fn a_truncated_line_stops_at_its_hard_break() {
        // Keeping one line keeps *that* line: the next line is not glued on behind the
        // ellipsis, and its newline is not shaped as ink.
        let by_range = |_: &str, r: Range<usize>| r.end.saturating_sub(r.start) as f32 * 10.0;
        let out = wrap_indented("aaaa\nbbbb", 1000.0, Some(1), 0.0, 0.0, 15.0, by_range);
        assert_eq!(out.len(), 1);
        assert_eq!(&"aaaa\nbbbb"[out[0].range.clone()], "aaaa");
        assert_eq!(out[0].width, 55.0, "four characters plus the ellipsis");
        assert!(out[0].ellipsis);
    }

    #[test]
    fn an_indent_narrows_only_the_first_line() {
        // Spaces are what make this test meaningful: without a break opportunity the text
        // takes the character-splitting path, where the indent is applied elsewhere.
        // Every char (space included) is 10 wide, so "aaa bbb ccc" is 110.
        let width = |s: &str, _r: std::ops::Range<usize>| s.chars().count() as f32 * 10.0;
        let plain = break_lines("aaa bbb ccc", 110.0, width);
        let indented = break_lines_indented("aaa bbb ccc", 110.0, 40.0, 0.0, width);
        assert_eq!(plain.len(), 1, "without an indent the whole line fits");
        assert!(indented.len() > 1, "the indent must force a second line");
    }

    #[test]
    fn a_hanging_indent_narrows_every_later_line() {
        // The first line keeps the wide box; the lines under it give the hanging room to the
        // marker, so "cc dd" no longer fits one line there. Every char is 10 wide.
        let width = |s: &str, _r: std::ops::Range<usize>| s.chars().count() as f32 * 10.0;
        let flat = break_lines_indented("aa bb cc dd", 55.0, 0.0, 0.0, width);
        assert_eq!(
            flat.len(),
            2,
            "without a hanging indent the tail fits one line"
        );
        let hung = break_lines_indented("aa bb cc dd", 55.0, 0.0, 15.0, width);
        assert_eq!(hung.len(), 3, "the hanging room must force a third line");
        let texts: Vec<&str> = hung
            .iter()
            .map(|line| &"aa bb cc dd"[line.range.clone()])
            .collect();
        assert_eq!(texts, ["aa bb", "cc", "dd"], "the first line keeps its box");
    }

    #[test]
    fn wraps_at_spaces_not_inside_words() {
        // "aa bb cc" advances 20 + 10 + 20 + 10 + 20; at 50 the third word must move down.
        let got = lines("aa bb cc", 50.0);
        assert_eq!(got.len(), 2);
        assert_eq!(&"aa bb cc"[got[0].0.clone()], "aa bb");
        assert_eq!(&"aa bb cc"[got[1].0.clone()], "cc");
        assert_eq!(got[0].1, 50.0, "the space between the words must count");
    }

    #[test]
    fn a_no_break_space_is_ink_not_hanging_white() {
        // UAX #14 L1 hangs a space, never a no-break space: the glyph stays inside the
        // line's range so a caret can sit after it and its advance counts.
        let got = lines("abc\u{a0}", 1000.0);
        assert_eq!(got.len(), 1);
        assert_eq!(&"abc\u{a0}"[got[0].0.clone()], "abc\u{a0}");
        assert_eq!(got[0].1, 40.0, "the nbsp advance is part of the line");
    }

    #[test]
    fn a_hard_break_keeps_the_spaces_before_it_but_a_wrap_hangs_them() {
        // Every space in the source owns a caret byte, so a source break keeps them; a
        // wrap is the layout's own, and hanging there keeps long space runs from wrapping.
        let hard = lines("ab  \ncd", 1000.0);
        assert_eq!(&"ab  \ncd"[hard[0].0.clone()], "ab  ");
        assert_eq!(hard[0].1, 40.0, "the spaces count in the line's width");

        let soft = lines("aa bb cc", 50.0);
        assert_eq!(
            &"aa bb cc"[soft[0].0.clone()],
            "aa bb",
            "the wrap hangs its space"
        );
        assert_eq!(soft[0].1, 50.0, "and the hung space adds no width");
    }

    #[test]
    fn a_trailing_newline_keeps_its_empty_line_box() {
        // The editor's `\n` at the end of a buffer still owns the line after it, so a caret
        // on that empty line has somewhere to sit. CSS `white-space: pre` keeps the box too.
        let got = lines("a\n", 1000.0);
        assert_eq!(got.len(), 2, "a trailing hard break opens one more line");
        assert_eq!(&"a\n"[got[0].0.clone()], "a");
        assert_eq!(
            got[1].0.clone(),
            2..2,
            "the last line is empty at the buffer end"
        );
        assert_eq!(
            lines("\n", 1000.0).len(),
            2,
            "a lone newline is two empty lines"
        );
        assert_eq!(lines("a\n\n", 1000.0).len(), 3, "so is the line before it");
        // Without the trailing break the count must not change.
        assert_eq!(lines("a", 1000.0).len(), 1);
        assert_eq!(lines("a\nb", 1000.0).len(), 2);
    }

    #[test]
    fn an_overlong_word_is_cut_by_character() {
        // "abcdefgh" is 80 wide and has no opportunity, so it must be split to fit 30.
        let got = lines("abcdefgh", 30.0);
        assert!(got.len() >= 3, "expected a character split, got {got:?}");
        assert!(
            got.iter().all(|(_, w)| *w <= 30.0),
            "every piece must fit: {got:?}"
        );
        let joined: String = got.iter().map(|(r, _)| &"abcdefgh"[r.clone()]).collect();
        assert_eq!(joined, "abcdefgh", "splitting must not lose text");
    }

    /// A justified line widens at its own opportunities: at each space, and between CJK
    /// characters; it never stretches the opportunity at its end, and a run with none is
    /// left alone.
    #[test]
    fn justification_widens_every_opportunity_inside_the_line() {
        let latin = justify_gaps("ab cd ef", 30.0);
        assert_eq!(latin.len(), 2, "two spaces inside the line: {latin:?}");
        assert!(latin.iter().all(|(at, _)| *at == 3 || *at == 6));
        assert!(latin.iter().all(|(_, w)| (*w - 15.0).abs() < 0.01));

        let cjk = justify_gaps("中文字", 20.0);
        assert_eq!(cjk.len(), 2, "between the three characters: {cjk:?}");
        assert!(cjk.iter().all(|(_, w)| (*w - 10.0).abs() < 0.01));

        assert!(
            justify_gaps("abcdef", 30.0).is_empty(),
            "no opportunity, no gaps"
        );
        assert!(justify_gaps("ab cd", 0.0).is_empty(), "nothing to add");
    }
}
