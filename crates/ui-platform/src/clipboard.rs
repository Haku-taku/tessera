//! Clipboard requests, in the portable half: the engine says what it wants, the platform
//! does it.

pub use ui_core::clipboard::ClipboardContent;

/// Which selection a request targets.
///
/// `Primary` is X11/Wayland's middle-click selection; backends that have none ignore it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardKind {
    /// The ordinary clipboard: Ctrl+C, Ctrl+V.
    Selection,
    /// The selection buffer; on Linux a mouse selection fills it as you drag.
    Primary,
}

/// One thing the engine asks the platform to do after a dispatch.
///
/// The engine never reads the clipboard itself: a read is a round trip, and `dispatch`
/// must not block. A read therefore comes back as [`ui_core::Event::Paste`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardCommand {
    /// Put `content` on the clipboard.
    Copy {
        content: ClipboardContent,
        kind: ClipboardKind,
    },
    /// Ask for the clipboard's text; the answer is an `Event::Paste`.
    RequestPaste { kind: ClipboardKind },
}

impl ClipboardCommand {
    /// The clipboard this command works on.
    pub fn kind(&self) -> ClipboardKind {
        match self {
            Self::Copy { kind, .. } | Self::RequestPaste { kind } => *kind,
        }
    }
}

/// A copy a surface hands to the loop, with the serial it must be published under.
///
/// A selection is only taken from a client that just received input, and the serial is the
/// surface's own; the surface therefore pairs the two before the request leaves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardCopy {
    pub content: ClipboardContent,
    pub kind: ClipboardKind,
    pub serial: u32,
}
