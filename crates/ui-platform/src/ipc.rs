//! The process's local channel: message boundaries and replies are portable, the transport
//! is the platform's.
//!
//! A second process finds the channel by name (a socket path on Unix, a pipe name on
//! Windows), sends whole messages and gets whole messages back. Framing is the transport's
//! business: a message in, a message out, never a partial read.

/// A reply path to the peer whose message is being handled.
pub trait Reply {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String>;

    /// A reply path that outlives this message's handler, for an answer produced on a
    /// worker. `None` when the transport cannot duplicate its peer.
    fn deferred(&mut self) -> Option<Box<dyn Reply + Send>> {
        None
    }
}

/// The process's named channel, as the loop sees it.
///
/// `poll` never blocks: it accepts what has already connected, reads what has already
/// arrived, and hands each message to the application with a reply path valid for that call.
pub trait Channel {
    fn poll(&mut self, on_message: &mut dyn FnMut(&mut dyn Reply, &[u8]));
}
