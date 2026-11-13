//! Emoji presentation (UTS #51): which clusters a text face must not shape.
//!
//! The decision is Parley's `parley_emoji` DFA (adapted from c-emoji, the same algorithm
//! Blink's `EmojiSegmentation` uses); only the UTS #51 property bits come from `unicode-properties`.

use parley_emoji::{EmojiDFA, EmojiPresentationStyle, EmojiSegmentationCategory};
use unicode_properties::UnicodeEmoji;
use unicode_properties::emoji::EmojiStatus;

/// Whether one cluster is displayed as emoji: an explicit sequence, or an emoji-default base.
pub fn asks_emoji(cluster: &str) -> bool {
    presentation(cluster).is_emoji()
}

/// The presentation UTS #51 gives one cluster; the selector, if any, outranks the default.
pub fn presentation(cluster: &str) -> EmojiPresentationStyle {
    let mut dfa = EmojiDFA::new();
    let mut leading = false;
    for (i, c) in cluster.chars().enumerate() {
        if i == 0 {
            leading = defaults_to_emoji(c);
        }
        dfa.step_record(category(c));
    }
    dfa.presentation_style(leading)
}

/// The presentation of a whole run: `Some` when every cluster agrees, `None` when it mixes.
///
/// A mixed run may not go to one face: an emoji face would answer for the plain clusters too,
/// and its digits are empty keycap components that draw nothing.
pub fn uniform_presentation(text: &str) -> Option<bool> {
    let mut seen: Option<bool> = None;
    for (_, cluster) in ui_core::grapheme::clusters(text) {
        let emoji = asks_emoji(cluster);
        match seen {
            Some(prev) if prev != emoji => return None,
            _ => seen = Some(emoji),
        }
    }
    seen
}

/// One code point as the DFA's input, with its UTS #51 properties.
fn category(c: char) -> EmojiSegmentationCategory {
    let status = c.emoji_status();
    EmojiSegmentationCategory::from_codepoint(
        c as u32,
        !matches!(
            status,
            EmojiStatus::NonEmoji | EmojiStatus::NonEmojiButEmojiComponent
        ),
        defaults_to_emoji(c),
        matches!(
            status,
            EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
        ),
        // `EmojiPresentationAndModifierAndEmojiComponent` is the skin-tone modifier
        // itself (`Emoji_Modifier=YES`), not a base: counting it as a base hides the
        // modifier from the DFA and the pair falls back to plain text.
        matches!(
            status,
            EmojiStatus::EmojiModifierBase | EmojiStatus::EmojiPresentationAndModifierBase
        ),
        matches!(c, '\u{1f1e6}'..='\u{1f1ff}'),
    )
}

/// UTS #51 `Emoji_Presentation=Yes`: the character shows as emoji without a selector.
fn defaults_to_emoji(c: char) -> bool {
    matches!(
        c.emoji_status(),
        EmojiStatus::EmojiPresentation
            | EmojiStatus::EmojiPresentationAndModifierBase
            | EmojiStatus::EmojiPresentationAndEmojiComponent
            | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skin_tone_forms_one_emoji_sequence() {
        // A modifier follows its base inside one cluster; counting the skin tone as a base
        // instead hides it from the DFA and the pair draws as two text glyphs.
        assert!(asks_emoji("\u{270c}\u{1f3fd}"));
    }

    #[test]
    fn a_run_mixes_when_an_emoji_sits_beside_plain_text() {
        // The digit after a keycap is plain text, so the run may not go to one face.
        assert_eq!(uniform_presentation("abc"), Some(false));
        assert_eq!(uniform_presentation("\u{1f600}\u{1f600}"), Some(true));
        assert_eq!(uniform_presentation("3\u{fe0f}\u{20e3}4"), None);
        assert_eq!(uniform_presentation("\u{1f600} abc"), None);
        assert_eq!(uniform_presentation(""), None, "nothing to agree on");
    }
}
