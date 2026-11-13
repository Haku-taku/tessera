//! What a copy puts on the clipboard.

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
}
