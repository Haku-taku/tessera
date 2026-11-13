//! Detect runs of blank lines, so an editor can treat them as one unit.

use std::ops::Range;

/// Adjacent blank lines and the byte span they occupy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlankRun {
    /// First byte of the first blank line.
    pub start: usize,
    /// One past the last blank line, its newline included.
    pub end: usize,
    /// First-byte offset of every blank line, first to last.
    pub lines: Vec<usize>,
}

/// Blank runs whose lines overlap `range`; an empty range finds nothing.
pub fn blank_runs(source: &str, range: Range<usize>) -> Vec<BlankRun> {
    let start = range.start.min(source.len());
    let end = range.end.min(source.len());
    if start >= end {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let mut current: Option<BlankRun> = None;
    for (line_start, line_end) in line_spans(source) {
        let in_range = line_start < end && line_end > start;
        if in_range && is_blank(source, line_start, line_end) {
            match &mut current {
                Some(run) => {
                    run.end = line_end;
                    run.lines.push(line_start);
                }
                None => {
                    current = Some(BlankRun {
                        start: line_start,
                        end: line_end,
                        lines: vec![line_start],
                    });
                }
            }
        } else if let Some(finished) = current.take() {
            runs.push(finished);
        }
    }
    if let Some(finished) = current {
        runs.push(finished);
    }
    runs
}

/// The run containing the cursor at `at`, plus the line's index inside it.
///
/// At a line boundary the cursor belongs to the line that starts there.
pub fn blank_run_at(source: &str, at: usize) -> Option<(BlankRun, usize)> {
    if at > source.len() {
        return None;
    }
    let spans = line_spans(source);
    let line_start = source
        .as_bytes()
        .get(..at)?
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let index = spans.iter().position(|&(start, _)| start == line_start)?;
    let &(start, end) = spans.get(index)?;
    if !is_blank(source, start, end) {
        return None;
    }

    let mut first = index;
    while let Some(&(start, end)) = first
        .checked_sub(1)
        .and_then(|previous| spans.get(previous))
    {
        if !is_blank(source, start, end) {
            break;
        }
        first -= 1;
    }
    let mut last = index;
    while let Some(&(start, end)) = last.checked_add(1).and_then(|next| spans.get(next)) {
        if !is_blank(source, start, end) {
            break;
        }
        last += 1;
    }

    let first_span = spans.get(first)?;
    let last_span = spans.get(last)?;
    let run = BlankRun {
        start: first_span.0,
        end: last_span.1,
        lines: spans
            .get(first..=last)?
            .iter()
            .map(|&(start, _)| start)
            .collect(),
    };
    Some((run, index - first))
}

/// Full spans of every line, newline included; the final line may be empty.
fn line_spans(source: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            spans.push((start, index + 1));
            start = index + 1;
        }
    }
    spans.push((start, source.len()));
    spans
}

/// A line is blank when trimming its content leaves nothing, which covers `\r\n`.
fn is_blank(source: &str, start: usize, end: usize) -> bool {
    let content_end = if source.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\n') {
        end - 1
    } else {
        end
    };
    source
        .get(start..content_end)
        .is_some_and(|content| content.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{BlankRun, blank_run_at, blank_runs};

    #[test]
    fn runs_group_one_line_three_lines_and_the_document_start() {
        let source = "\n\nalpha\n\nbeta\n\n\n\ngamma\n";
        assert_eq!(
            blank_runs(source, 0..source.len()),
            vec![
                BlankRun {
                    start: 0,
                    end: 2,
                    lines: vec![0, 1],
                },
                BlankRun {
                    start: 8,
                    end: 9,
                    lines: vec![8],
                },
                BlankRun {
                    start: 14,
                    end: 17,
                    lines: vec![14, 15, 16],
                },
            ]
        );
    }

    #[test]
    fn blank_run_at_answers_the_run_and_index_for_every_line() {
        let source = "head\n\n\n\ntail";
        for (line, at) in [5, 6, 7].into_iter().enumerate() {
            let (run, index) = blank_run_at(source, at).expect("all three lines are blank");
            assert_eq!(
                run,
                BlankRun {
                    start: 5,
                    end: 8,
                    lines: vec![5, 6, 7],
                }
            );
            assert_eq!(index, line);
        }
        assert_eq!(blank_run_at(source, 4), None);
        assert_eq!(blank_run_at(source, 8), None);
    }
}
