//! The local channel on Unix: a `SOCK_SEQPACKET` socket at the address the app names.
//!
//! Sequenced packets carry message boundaries themselves, so nothing here frames a stream,
//! and the kernel drops a message rather than splicing two together when a peer misbehaves.

use std::cell::RefCell;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::PathBuf;
use std::rc::Rc;

use rustix::io::Errno;
use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType, accept_with,
    bind, listen, recv, send, socket_with,
};
use ui_platform::ipc::{Channel, Reply};

/// Largest message accepted. Pixels never travel here: they are a file or a shared buffer.
const MAX_MESSAGE: usize = 64 * 1024;

struct Peer {
    fd: OwnedFd,
}

/// Shared with the backend, which polls the same descriptors in its wait.
pub(crate) struct ServerState {
    listener: OwnedFd,
    peers: Vec<Peer>,
    /// Kept so the socket file is reported when binding fails, and removed with the daemon.
    path: PathBuf,
}

impl ServerState {
    pub(crate) fn fds<'a>(&'a self, out: &mut Vec<BorrowedFd<'a>>) {
        out.push(self.listener.as_fd());
        out.extend(self.peers.iter().map(|peer| peer.fd.as_fd()));
    }
}

/// Bind the daemon's side of the channel at `address`.
pub(crate) fn open(address: &str) -> Result<Rc<RefCell<ServerState>>, String> {
    let path = PathBuf::from(address);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    // A socket file left by a dead daemon would block the new one. The caller has already
    // checked that no daemon answers, so the file is stale.
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove {}: {e}", path.display()))?;
    }
    let listener = socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
        None,
    )
    .map_err(|e| format!("socket: {e}"))?;
    let address = SocketAddrUnix::new(path.as_os_str()).map_err(|e| format!("address: {e}"))?;
    bind(&listener, &address).map_err(|e| format!("bind {}: {e}", path.display()))?;
    listen(&listener, 16).map_err(|e| format!("listen: {e}"))?;
    Ok(Rc::new(RefCell::new(ServerState {
        listener,
        peers: Vec::new(),
        path,
    })))
}

/// The loop's handle on the daemon's channel.
pub(crate) struct UnixChannel {
    state: Rc<RefCell<ServerState>>,
}

impl UnixChannel {
    pub(crate) fn new(state: Rc<RefCell<ServerState>>) -> Self {
        Self { state }
    }
}

impl Channel for UnixChannel {
    fn poll(&mut self, on_message: &mut dyn FnMut(&mut dyn Reply, &[u8])) {
        let mut state = self.state.borrow_mut();
        loop {
            match accept_with(
                &state.listener,
                SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            ) {
                Ok(fd) => state.peers.push(Peer { fd }),
                Err(Errno::WOULDBLOCK) => break,
                // Out of descriptors, or a peer vanished mid-handshake: the next turn tries
                // again rather than ending the daemon.
                Err(_) => break,
            }
        }
        let mut buffer = vec![0u8; MAX_MESSAGE];
        let mut index = 0;
        while index < state.peers.len() {
            let mut closed = false;
            loop {
                match recv(&state.peers[index].fd, &mut buffer[..], RecvFlags::empty()) {
                    Ok((0, _)) => {
                        closed = true;
                        break;
                    }
                    Ok((received, _)) => {
                        let mut reply = PeerReply {
                            fd: &state.peers[index].fd,
                        };
                        on_message(&mut reply, &buffer[..received]);
                    }
                    Err(Errno::WOULDBLOCK) => break,
                    Err(Errno::INTR) => continue,
                    Err(_) => {
                        closed = true;
                        break;
                    }
                }
            }
            if closed {
                state.peers.remove(index);
            } else {
                index += 1;
            }
        }
    }
}

struct PeerReply<'a> {
    fd: &'a OwnedFd,
}

impl Reply for PeerReply<'_> {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        send(self.fd, bytes, SendFlags::NOSIGNAL).map_err(|e| format!("send: {e}"))?;
        Ok(())
    }

    fn deferred(&mut self) -> Option<Box<dyn Reply + Send>> {
        // The duplicate keeps the peer reachable after this handler returns; the caller
        // answers now when it cannot be made.
        let fd = rustix::io::dup(self.fd).ok()?;
        Some(Box::new(OwnedReply { fd }))
    }
}

/// A reply path for an answer the loop will send later.
struct OwnedReply {
    fd: OwnedFd,
}

impl Reply for OwnedReply {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        send(&self.fd, bytes, SendFlags::NOSIGNAL).map_err(|e| format!("send: {e}"))?;
        Ok(())
    }
}

impl Drop for ServerState {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
