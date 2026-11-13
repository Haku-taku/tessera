//! Grapheme cluster boundaries (UAX #29): the unit the caret and shaping both step by.
//!
//! A user perceives `3️⃣`, `👨‍👩‍👧` and `e\u{301}` as one character, so every place that walks
//! a string by `char` here walks it by cluster instead.

use unicode_segmentation::UnicodeSegmentation;

/// `text`'s clusters as `(byte offset, cluster)`, in order.
pub fn clusters(text: &str) -> impl Iterator<Item = (usize, &str)> + '_ {
    text.grapheme_indices(true)
}

/// The cluster boundary at or before `at`; `at` may be any byte offset, even mid-character.
pub fn snap(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    let mut floor = 0;
    for (i, g) in clusters(text) {
        if i + g.len() > at {
            break;
        }
        floor = i + g.len();
    }
    floor
}

/// The cluster boundary at or after `at`; a boundary stays put, the end caps it.
pub fn ceil(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    match snap(text, at) == at {
        true => at,
        false => next(text, at),
    }
}

/// The boundary after the cluster that contains `at`; the end when there is none.
pub fn next(text: &str, at: usize) -> usize {
    let from = snap(text, at);
    text[from..]
        .graphemes(true)
        .next()
        .map_or(text.len(), |g| from + g.len())
}

/// The boundary before the cluster that contains `at`; the start when there is none.
pub fn prev(text: &str, at: usize) -> usize {
    let at = snap(text, at);
    text[..at]
        .graphemes(true)
        .next_back()
        .map_or(0, |g| at - g.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One keycap, one ZWJ family, one flag and one decomposed `é`, in that order.
    const MIXED: [&str; 4] = [
        "3\u{fe0f}\u{20e3}",
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
        "\u{1f1e8}\u{1f1f3}",
        "e\u{301}",
    ];

    fn mixed() -> String {
        MIXED.concat()
    }

    #[test]
    fn an_offset_inside_a_cluster_snaps_to_its_start_and_steps_past_it() {
        // A stale or hand-made offset must never split a cluster on the next edit.
        let text = mixed();
        let keycap = MIXED[0].len();
        assert_eq!(snap(&text, 3), 0, "inside the keycap");
        assert_eq!(snap(&text, keycap), keycap, "a boundary stays put");
        assert_eq!(snap(&text, 999), text.len(), "past the end is the end");
        assert_eq!(next(&text, 3), keycap, "forward leaves the whole cluster");
        assert_eq!(prev(&text, 3), 0, "backward leaves the whole cluster");
        assert_eq!(snap(&text, 2), 0, "mid-character still snaps");
        assert_eq!(
            ceil(&text, 2),
            keycap,
            "an IME byte range grows to a whole cluster"
        );
        assert_eq!(ceil(&text, keycap), keycap, "a boundary stays put");
        assert_eq!(ceil(&text, 999), text.len(), "past the end is the end");
    }
}
