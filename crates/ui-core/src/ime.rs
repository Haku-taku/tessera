//! Input-method composition: the preedit string, which the app never sees.

/// What an input is for, as an input method needs to know it.
///
/// The protocol calls these content purposes and has no search one: a field that is not
/// listed here sends `Normal`, and a secret buffer sends the hidden form whatever it named.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContentPurpose {
    /// Free text, the default.
    #[default]
    Normal,
    /// A time of day: the input method may offer its own time entry.
    Time,
}

/// The string an input method is composing, and where its own caret sits inside it.
///
/// The app's buffer stays committed-only: this is engine state, drawn after the caret and
/// reported to the compositor as the cursor position, never merged until it commits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// Bytes being composed, in the order the input method gave them.
    pub text: String,
    /// The input method's caret inside `text`, in bytes.
    pub cursor: usize,
}

impl Preedit {
    pub fn new(text: impl Into<String>, cursor: usize) -> Self {
        Self {
            text: text.into(),
            cursor,
        }
    }

    /// Nothing is being composed.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}
