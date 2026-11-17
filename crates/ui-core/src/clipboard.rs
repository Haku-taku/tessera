//! What a copy puts on the clipboard.

use crate::dnd::DropEffect;

/// One clipboard payload.
///
/// The clipboard itself is the platform's: a copy only says what to hand over, and a backend
/// offers the matching MIME types and serves these bytes when a peer asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardContent {
    /// Text, under the platform's text MIME types.
    Text(String),
    /// Opaque bytes under one MIME type, such as `image/png`. Shared, because every peer
    /// that pastes reads the same payload and none of them may copy it.
    Bytes {
        mime: String,
        data: std::sync::Arc<Vec<u8>>,
    },
    /// A list of paths, as `text/uri-list`; on Windows, `CF_HDROP`.
    ///
    /// The effect is what a paste does with them: `Copy` leaves the originals, `Move`
    /// consumes them. A platform that has a cut marker writes it from this field; one that
    /// does not still tells its own drag and paste handlers.
    Files {
        paths: Vec<std::path::PathBuf>,
        effect: DropEffect,
    },
}

impl ClipboardContent {
    /// The payload as text, when it is text.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Bytes { .. } | Self::Files { .. } => None,
        }
    }
}
