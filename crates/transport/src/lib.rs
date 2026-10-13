//! See the repo README: *The layer model*, and *The socket model* for the
//! peer / bridge / offload / guest vocabulary used here.
//!
//! #12-A is the clean-link datapath; the retransmit/timer/keepalive layer
//! ppproto lacks is #12-B (owner/repo#24, upstream embassy-rs/ppproto#5).
//!
//! ppproto's IPCP accepts a peer address but never assigns one, so the TUN is
//! addressed statically from config rather than from IPCP.

use std::io;

pub use ppproto::Phase;

/// A packet socket: send and receive *guests* (whole IP packets). Named from the
/// socket's own side — `peer.send(g)` sends a guest to the peer, `offload.receive()`
/// takes a guest from the offload. `splice` is the only thing that joins two.
#[allow(async_fn_in_trait)] // internal trait, used via generics — no `dyn`, no Send bound needed
pub trait Socket {
    /// The next guest, or `None` once the socket has closed.
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>>;
    /// Hand a guest to this socket.
    async fn send(&mut self, guest: &[u8]) -> io::Result<()>;
}

/// PPP MRU we size buffers and the TUN for; fixed at the RFC 1661 default —
/// ppproto negotiates no MRU (#40), so both ends are pinned here.
pub(crate) const MRU: usize = 1500;
/// HDLC byte-stuffing can nearly double a frame, plus FCS/flag overhead — a
/// full-MRU packet must still fit once encoded.
pub(crate) const FRAME: usize = 2 * (MRU + 8) + 8;

/// Aborts its task on drop, so a socket's driver tasks never outlive it.
pub(crate) struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The datapath: carry guests both ways between a `peer` and an `offload` until
/// either end closes. All protocol/device machinery lives inside the sockets.
pub async fn splice<P, O>(mut peer: P, mut offload: O) -> io::Result<()>
where
    P: Socket,
    O: Socket,
{
    loop {
        tokio::select! {
            r = peer.receive() => match r? {
                Some(guest) => offload.send(&guest).await?,
                None => return Ok(()),
            },
            r = offload.receive() => match r? {
                Some(guest) => peer.send(&guest).await?,
                None => return Ok(()),
            },
        }
    }
}

pub mod ppp;
pub mod tun;

#[cfg(test)]
mod tests;
