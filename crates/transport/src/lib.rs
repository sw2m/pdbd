//! See README.md — *The layer model* and *The socket model*.
//!
//! #12-A is the clean-link datapath; the retransmit/timer/keepalive layer
//! ppproto lacks is #12-B (owner/repo#24, upstream embassy-rs/ppproto#5).
//!
//! ppproto's IPCP accepts a peer address but never assigns one, so the TUN is
//! addressed statically from config rather than from IPCP.

use std::io;

pub use ppproto::Phase;

#[allow(async_fn_in_trait)] // internal trait, used via generics — no `dyn`, no Send bound needed
pub trait Socket {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>>;
    async fn send(&mut self, guest: &[u8]) -> io::Result<()>;
}

pub mod ppp;
pub mod tun;
