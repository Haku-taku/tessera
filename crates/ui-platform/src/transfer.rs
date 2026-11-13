//! Reading the process's selection, polled like a capture.
//!
//! The engine never reads a clipboard synchronously: a read is a pipe from the selection
//! owner and must not block a turn. A reader therefore hands over what the current selection
//! offers, starts a read of one MIME, and is polled each turn until the bytes arrive.
//! Splitting "paths" from "one blob" is the app's call — it owns the uri-list parser — so
//! the platform hands over the raw payload.

/// One MIME's payload, once its pipe has delivered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub mime: String,
    pub data: Vec<u8>,
}

/// The selection the platform can read. Backends normalise their clipboard forms into
/// [`Selection`]; the app decides what the MIME means.
pub trait SelectionReader {
    /// The MIME types the current selection offers.
    fn offers(&self) -> Vec<String>;

    /// Start reading `mime`; its bytes arrive through [`poll`](Self::poll). False when the
    /// platform has no selection, or no way to read one.
    fn read(&mut self, mime: &str) -> bool;

    /// Move a pending read along; the finished payload comes back here. `None` while the
    /// pipe is still open or empty.
    fn poll(&mut self) -> Option<Selection>;
}
