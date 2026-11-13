//! Caret arithmetic for a single-line text input, independent of any font or widget.
//!
//! Every step is one grapheme cluster (UAX #29), so `3️⃣` or `👨‍👩‍👧` moves and deletes whole.

use crate::grapheme;
use crate::text::Structure;

/// `caret` brought to a cluster boundary of `value`, clamped to its length.
///
/// Callers may hold an offset from a previous frame; a buffer that shrank must not split a
/// character when the next edit is applied.
pub fn clamp(value: &str, caret: usize) -> usize {
    grapheme::snap(value, caret)
}

/// Move the caret `delta` clusters from `caret`; negative moves towards the start.
pub fn moved(value: &str, caret: usize, delta: isize) -> usize {
    let mut at = clamp(value, caret);
    if delta >= 0 {
        for _ in 0..delta {
            at = grapheme::next(value, at);
        }
    } else {
        for _ in 0..delta.unsigned_abs() {
            at = grapheme::prev(value, at);
        }
    }
    at
}

/// `value` with `insert` spliced in at the caret; the caret lands after the inserted text.
pub fn insert(value: &str, caret: usize, insert: &str) -> (String, usize) {
    let at = clamp(value, caret);
    let mut out = String::with_capacity(value.len() + insert.len());
    out.push_str(&value[..at]);
    out.push_str(insert);
    out.push_str(&value[at..]);
    (out, at + insert.len())
}

/// `value` with a live composition spliced in at the caret, plus where it starts in the
/// result. The caret is snapped like any edit's: a composition must never split a cluster.
pub fn composed(value: &str, preedit: &str, caret: usize) -> (String, usize) {
    let at = clamp(value, caret);
    let mut out = String::with_capacity(value.len() + preedit.len());
    out.push_str(&value[..at]);
    out.push_str(preedit);
    out.push_str(&value[at..]);
    (out, at)
}

/// `value` with the cluster before the caret removed; at the start nothing changes.
pub fn backspace(value: &str, caret: usize) -> (String, usize) {
    let at = clamp(value, caret);
    let prev = grapheme::prev(value, at);
    if prev == at {
        return (value.to_string(), at);
    }
    let mut out = String::with_capacity(value.len());
    out.push_str(&value[..prev]);
    out.push_str(&value[at..]);
    (out, prev)
}

/// The word around `at`: a run of alphanumeric clusters, or one cluster off a word.
///
/// CJK characters are alphanumeric, so a double click selects a whole run of them; ICU
/// word breaking would pick one word, and that is a later refinement, not this contract.
pub fn word_at(value: &str, at: usize) -> std::ops::Range<usize> {
    let at = clamp(value, at);
    if value.is_empty() {
        return at..at;
    }
    let mut start = at;
    while start > 0 {
        let prev = grapheme::prev(value, start);
        if !is_word_cluster(&value[prev..start]) {
            break;
        }
        start = prev;
    }
    let mut end = at;
    while end < value.len() {
        let next = grapheme::next(value, end);
        if !is_word_cluster(&value[end..next]) {
            break;
        }
        end = next;
    }
    if start == end {
        // Off a word: the cluster at the caret, or the one before it at the end.
        if at < value.len() {
            end = grapheme::next(value, at);
        } else {
            start = grapheme::prev(value, at);
        }
    }
    start..end
}

/// What `word_at` counts as a word cluster: the base character decides, its marks follow it.
fn is_word_cluster(cluster: &str) -> bool {
    cluster.chars().next().is_some_and(is_word_char)
}

/// What `word_at` counts as a word character.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word-wise neighbour of `caret`: past the separators and over one word, in `delta`'s
/// direction. From inside a word it stops at that word's edge, like every mature editor.
pub fn step_word(value: &str, caret: usize, delta: isize) -> usize {
    let mut at = clamp(value, caret);
    if delta >= 0 {
        while at < value.len() && !is_word_cluster(&value[at..grapheme::next(value, at)]) {
            at = grapheme::next(value, at);
        }
        while at < value.len() && is_word_cluster(&value[at..grapheme::next(value, at)]) {
            at = grapheme::next(value, at);
        }
    } else {
        while at > 0 && !is_word_cluster(&value[grapheme::prev(value, at)..at]) {
            at = grapheme::prev(value, at);
        }
        while at > 0 && is_word_cluster(&value[grapheme::prev(value, at)..at]) {
            at = grapheme::prev(value, at);
        }
    }
    at
}

/// `value` with `range` removed; the caret lands where the range began.
pub fn delete_range(value: &str, range: std::ops::Range<usize>) -> (String, usize) {
    let a = clamp(value, range.start);
    let b = clamp(value, range.end);
    let (start, end) = (a.min(b), a.max(b));
    let mut out = String::with_capacity(value.len());
    out.push_str(&value[..start]);
    out.push_str(&value[end..]);
    (out, start)
}

/// `value` with `range` replaced by `text`, keeping every block value whole.
///
/// Every member of a group is ordinary text: nothing is refused and no position is
/// moved. When the edit falls inside the member a block's source spells out and writes
/// only spaces, every other member of that group is rewritten to the value it would hold
/// at the new count: a line at least as deep as the old one moves by the same delta, a
/// shallower one keeps the little it has rather than gaining spaces the code would show.
/// The change is one edit. `None` means the edit touches no block value: apply it plainly.
pub fn edit_structural(
    value: &str,
    range: std::ops::Range<usize>,
    text: &str,
    structures: &[Structure],
) -> Option<(String, usize)> {
    let a = clamp(value, range.start);
    let b = clamp(value, range.end);
    let (start, end) = (a.min(b), a.max(b));
    let spelled = structures
        .iter()
        .find(|member| member.shown && member.range.start <= start && end <= member.range.end)?;
    let written: String = value.get(start..end)?.chars().chain(text.chars()).collect();
    if !written.chars().all(|c| c == ' ') {
        return None;
    }
    let delta = text.len() as isize - (end - start) as isize;
    if delta == 0 {
        return None;
    }
    let old = spelled.range.len() as isize;
    let new = (old + delta).max(0);
    let mut out = String::with_capacity(value.len() + text.len());
    out.push_str(&value[..start]);
    out.push_str(text);
    out.push_str(&value[end..]);
    let caret = start + text.len();
    // Members after the edit move with it; writing the list back to front keeps the
    // offsets of the ones still to come in the old buffer's terms.
    let mut others: Vec<(usize, usize, usize)> = structures
        .iter()
        .filter(|member| member.group == spelled.group && !member.shown)
        .map(|member| {
            let depth = (member.range.end - member.range.start) as isize;
            let width = match depth >= old {
                true => depth + delta,
                false => depth.min(new),
            };
            let shift = if member.range.start >= end { delta } else { 0 };
            (
                (member.range.start as isize + shift) as usize,
                (member.range.end as isize + shift) as usize,
                width.max(0) as usize,
            )
        })
        .filter(|(low, high, _)| *high <= out.len() && low <= high)
        .collect();
    others.sort_unstable();
    others.dedup();
    for (low, high, width) in others.into_iter().rev() {
        out.replace_range(low..high, &" ".repeat(width));
    }
    Some((out, caret))
}

/// `value` with the cluster after the caret removed; at the end nothing changes.
pub fn delete(value: &str, caret: usize) -> (String, usize) {
    let at = clamp(value, caret);
    let next = grapheme::next(value, at);
    if next == at {
        return (value.to_string(), at);
    }
    let mut out = String::with_capacity(value.len());
    out.push_str(&value[..at]);
    out.push_str(&value[next..]);
    (out, at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edit_lands_on_a_char_boundary_even_when_the_caret_is_stale() {
        // A buffer that shrank while the caret stayed put must not be split mid-character.
        let value = "aé";
        assert_eq!(clamp(value, 2), 1, "2 is inside `é`");
        assert_eq!(clamp(value, 99), 3, "past the end is the end");
        let (out, at) = insert(value, 2, "x");
        assert_eq!((out.as_str(), at), ("axé", 2));
    }

    #[test]
    fn a_composition_at_a_stale_caret_splices_on_a_boundary() {
        // A live IME composition splices at the caret like any edit; `é` must not be split
        // when the offset predates the buffer that now holds it.
        let (shown, at) = composed("aé", "か", 2);
        assert_eq!((shown.as_str(), at), ("aかé", 1));
    }

    #[test]
    fn a_caret_step_counts_characters_not_bytes() {
        // The caret moves over `é` in one step, and stops at both ends.
        let value = "aéb";
        assert_eq!(moved(value, 0, 1), 1);
        assert_eq!(moved(value, 1, 1), 3);
        assert_eq!(moved(value, 3, -1), 1);
        assert_eq!(moved(value, 3, -2), 0);
        assert_eq!(moved(value, 3, 9), 4);
        assert_eq!(moved(value, 0, -1), 0);
    }

    #[test]
    fn a_delete_removes_whole_clusters_only() {
        // Backspace and Delete take the cluster next to the caret, never a code point of it.
        let keycap = "3\u{fe0f}\u{20e3}";
        assert_eq!(backspace(keycap, keycap.len()), (String::new(), 0));
        assert_eq!(delete(keycap, 0), (String::new(), 0));
        assert_eq!(backspace("a👨‍👩‍👧", "a👨‍👩‍👧".len()).0, "a");
        assert_eq!(delete("👨‍👩‍👧b", 0).0, "b");
    }

    #[test]
    // The reversed range is the point of the second case, not a slip.
    #[allow(clippy::reversed_empty_ranges)]
    fn a_selection_is_removed_as_one_unit_and_the_caret_lands_at_its_start() {
        // Typing over a selection is one edit, not a delete followed by an insert.
        assert_eq!(
            delete_range("alpha bravo", 6..11),
            ("alpha ".to_string(), 6)
        );
        assert_eq!(
            delete_range("alpha bravo", 11..6),
            ("alpha ".to_string(), 6),
            "a backwards range still works"
        );
        assert_eq!(
            delete_range("aé", 0..2),
            ("é".to_string(), 0),
            "the end clamps to a character boundary"
        );
    }
}
