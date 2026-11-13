//! Bidirectional text: split a paragraph into directional runs.
//!
//! The Unicode algorithm itself comes from `unicode-bidi`; this module only maps
//! its per-byte levels onto the runs a shaper needs, and reorders them for display.

use std::ops::Range;

use unicode_bidi::{BidiInfo, Level};

use crate::provider::Script;

/// Reading direction of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        matches!(self, Direction::RightToLeft)
    }

    /// Direction implied by an embedding level: odd levels are right-to-left.
    pub fn from_level(level: u8) -> Self {
        if level % 2 == 1 {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        }
    }
}

/// A maximal run of text at one embedding level, in logical order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelRun {
    /// Byte range into the original text.
    pub range: Range<usize>,
    pub level: u8,
    /// The script this run is shaped in; the shaper must not infer it from the first byte,
    /// or text after an emoji or a digit goes to the wrong face.
    pub script: Script,
}

impl LevelRun {
    pub fn direction(&self) -> Direction {
        Direction::from_level(self.level)
    }

    pub fn is_rtl(&self) -> bool {
        self.direction().is_rtl()
    }
}

/// Split `text` into level runs in logical order.
///
/// `base` forces the paragraph direction; `None` lets the algorithm infer it from
/// the first strong character, which is what a text editor normally wants.
pub fn itemize(text: &str, base: Option<Direction>) -> Vec<LevelRun> {
    if text.is_empty() {
        return Vec::new();
    }
    let para_level = base.map(|d| match d {
        Direction::LeftToRight => Level::ltr(),
        Direction::RightToLeft => Level::rtl(),
    });
    let info = BidiInfo::new(text, para_level);
    runs_from_levels(text, &info.levels)
}

/// The paragraph's base direction by UAX #9 P2/P3: the first strong character decides, and
/// a paragraph with none (digits, punctuation, spaces) is left-to-right.
///
/// The unit is the paragraph, not the line. A line inside a right-to-left paragraph that
/// opens with digits has no strong character of its own, so inferring per line would read it
/// as left-to-right and reverse the visual order of the rest of the line.
pub fn paragraph_direction(text: &str) -> Direction {
    let info = BidiInfo::new(text, None);
    match info.paragraphs.first() {
        Some(paragraph) if paragraph.level.is_rtl() => Direction::RightToLeft,
        _ => Direction::LeftToRight,
    }
}

/// One script per character, with the ones that have no script of their own resolved
/// from their neighbours: leading marks and digits take the script that follows them, and
/// later ones the script before them.
fn resolved_scripts(text: &str) -> Vec<Script> {
    let mut scripts: Vec<Option<Script>> = text.chars().map(Script::of_char).collect();
    let first = scripts
        .iter()
        .flatten()
        .next()
        .copied()
        .unwrap_or(Script::LATIN);
    let mut last = first;
    for slot in &mut scripts {
        match slot {
            Some(script) => last = *script,
            None => *slot = Some(last),
        }
    }
    scripts
        .into_iter()
        .map(|s| s.unwrap_or(Script::LATIN))
        .collect()
}

/// Group per-byte levels into maximal runs.
///
/// Levels are per *byte*, so a multi-byte character occupies several entries with
/// the same level; runs therefore always break on character boundaries.
fn runs_from_levels(text: &str, levels: &[Level]) -> Vec<LevelRun> {
    let mut runs: Vec<LevelRun> = Vec::new();
    let mut current: Option<LevelRun> = None;
    let scripts = resolved_scripts(text);

    for ((offset, ch), script) in text.char_indices().zip(scripts) {
        let level = levels.get(offset).map(|l| l.number()).unwrap_or(0);
        let end = offset + ch.len_utf8();
        match &mut current {
            Some(run) if run.level == level && run.script == script => run.range.end = end,
            _ => {
                if let Some(run) = current.take() {
                    runs.push(run);
                }
                current = Some(LevelRun {
                    range: offset..end,
                    level,
                    script,
                });
            }
        }
    }
    if let Some(run) = current {
        runs.push(run);
    }
    runs
}

/// Visual order of `runs`, as indices into the logical slice.
///
/// Applies UAX#9 rule L2: from the highest level down to the lowest odd level,
/// reverse every contiguous sequence of runs at or above that level.
pub fn visual_order(runs: &[LevelRun]) -> Vec<usize> {
    let levels: Vec<Level> = runs
        .iter()
        .map(|r| Level::new_explicit(r.level).unwrap_or_else(|_| Level::ltr()))
        .collect();
    BidiInfo::reorder_visual(&levels)
}

/// Runs in visual order, paired with the logical index they came from.
pub fn visual_runs(text: &str, base: Option<Direction>) -> Vec<(usize, LevelRun)> {
    let logical = itemize(text, base);
    visual_order(&logical)
        .into_iter()
        .filter_map(|i| logical.get(i).cloned().map(|r| (i, r)))
        .collect()
}

/// Whether `text` contains any right-to-left content.
pub fn has_rtl(text: &str) -> bool {
    itemize(text, None).iter().any(|r| r.is_rtl())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paragraph_direction_comes_from_its_first_strong_character() {
        // P2/P3: strong characters decide, and a paragraph of digits or marks has none.
        assert_eq!(paragraph_direction("abc"), Direction::LeftToRight);
        assert_eq!(paragraph_direction("שלום"), Direction::RightToLeft);
        assert_eq!(paragraph_direction("123 שלום"), Direction::RightToLeft);
        assert_eq!(paragraph_direction("123 abc"), Direction::LeftToRight);
        assert_eq!(paragraph_direction("123 !?"), Direction::LeftToRight);
        assert_eq!(paragraph_direction(""), Direction::LeftToRight);
    }

    #[test]
    fn a_digit_only_line_follows_its_paragraph_not_itself() {
        // The line that opens a wrapped RTL paragraph with a number has no strong character,
        // so reading it on its own would flip its neutrals to LTR; inside the paragraph the
        // base level keeps them RTL.
        let paragraph = "שלום עולם\n123 : 456";
        let base = paragraph_direction(paragraph);
        assert_eq!(base, Direction::RightToLeft);
        let line = "123 : 456";
        let alone = itemize(line, None);
        let inherited = itemize(line, Some(base));
        assert_ne!(
            alone.iter().map(|r| r.level).collect::<Vec<_>>(),
            inherited.iter().map(|r| r.level).collect::<Vec<_>>(),
            "the line must read differently under the paragraph's base level"
        );
    }

    #[test]
    fn a_script_change_splits_a_run_even_at_one_bidi_level() {
        // Latin and Han are both LTR, but they need different faces and shaping tables.
        let runs = itemize("abc\u{4e2d}\u{6587}", None);
        assert_eq!(runs.len(), 2, "one run per script: {runs:?}");
        assert_eq!(runs[0].script, Script::LATIN);
        assert_eq!(runs[1].script, Script::HAN);
    }

    #[test]
    fn characters_without_a_script_take_the_script_around_them() {
        // An emoji beside Han is Common; reading it as Latin used to send the Han text to a
        // Latin face and leave the pair to draw as tofu.
        let runs = itemize("\u{1f600}\u{4e2d}\u{6587}", None);
        assert_eq!(runs.len(), 1, "one LTR run: {runs:?}");
        assert_eq!(runs[0].script, Script::HAN);
    }

    #[test]
    fn mixed_text_splits_into_alternating_runs() {
        // Latin, then Arabic, then Latin: three runs in logical order.
        let text = "abc مرحبا xyz";
        let runs = itemize(text, Some(Direction::LeftToRight));
        assert!(runs.len() >= 3, "got {runs:?}");
        assert!(!runs[0].is_rtl(), "leading Latin is LTR");
        assert!(
            runs.iter().any(|r| r.is_rtl()),
            "the Arabic middle must be RTL, got {runs:?}"
        );
        // Runs must tile the text without gaps or overlap.
        let mut cursor = 0;
        for run in &runs {
            assert_eq!(run.range.start, cursor);
            cursor = run.range.end;
        }
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn runs_never_split_a_multibyte_character() {
        let text = "a中bمc";
        let runs = itemize(text, None);
        for run in &runs {
            assert!(
                text.is_char_boundary(run.range.start) && text.is_char_boundary(run.range.end),
                "run {run:?} split a character"
            );
        }
    }

    #[test]
    fn visual_runs_keeps_every_logical_index() {
        let text = "a مرحبا b שלום c";
        let logical = itemize(text, None);
        let visual = visual_runs(text, None);
        assert_eq!(visual.len(), logical.len());
        let mut seen: Vec<usize> = visual.iter().map(|(i, _)| *i).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..logical.len()).collect::<Vec<_>>());
    }
}
