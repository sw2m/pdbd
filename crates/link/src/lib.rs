//! The pdbd link: RFC 1661/1332 PPP (via `ppproto`) over an L1 byte pipe,
//! terminated into a kernel TUN so the kernel's IP stack runs on the link.
//!
//! #12-A is the clean-link datapath (serial ↔ ppproto ↔ TUN ↔ ping). Lossy
//! robustness — the retransmit/timer/keepalive layer ppproto lacks — is #12-B
//! (owner/repo#24, upstream embassy-rs/ppproto#5).
//!
//! Addressing note: ppproto's IPCP *accepts* a peer's address but never *assigns*
//! one, so a pdbd↔pdbd link reaches `Open` with 0.0.0.0; pdbd programs the TUN
//! addresses statically from config rather than from IPCP (the effectful shell).

use ppproto::pppos::{PPPoS, PPPoSAction};
use ppproto::Config;
pub use ppproto::Phase;

/// The result of advancing the link one step.
#[derive(Debug)]
pub enum Action {
    /// Nothing to do this turn.
    Idle,
    /// Bytes to write to the L1 transport.
    Tx(Vec<u8>),
    /// An IP packet received from the peer (to deliver to the TUN).
    Ip(Vec<u8>),
}

/// A PPP link over a byte transport — the sync core that drives `ppproto`.
/// The async transport I/O and the kernel TUN are the effectful shell around it.
pub struct Link {
    ppp: PPPoS<'static>,
    tx: Vec<u8>,
    rx: Vec<u8>,
}

impl Link {
    pub fn new() -> Self {
        // pdbd secures the transport, not the PPP layer → no PAP credentials.
        Self {
            ppp: PPPoS::new(Config {
                username: b"",
                password: b"",
            }),
            tx: vec![0; 2048],
            rx: vec![0; 2048],
        }
    }

    /// Begin negotiation (first LCP Configure-Request is emitted on the next poll).
    pub fn open(&mut self) {
        self.ppp.open().expect("Link::open on a non-fresh link");
    }

    /// Current PPP phase; `Phase::Open` once the link is up.
    pub fn phase(&self) -> Phase {
        self.ppp.status().phase
    }

    /// Feed bytes received from L1; returns how many were consumed.
    pub fn consume(&mut self, data: &[u8]) -> usize {
        self.ppp.consume(data, &mut self.rx)
    }

    /// Advance the machine and return the next action.
    pub fn poll(&mut self) -> Action {
        match self.ppp.poll(&mut self.tx, &mut self.rx) {
            PPPoSAction::None => Action::Idle,
            PPPoSAction::Transmit(n) => Action::Tx(self.tx[..n].to_vec()),
            PPPoSAction::Received(range) => Action::Ip(self.rx[range].to_vec()),
        }
    }

    /// Frame an IP packet (from the TUN) for transmission over L1.
    pub fn send_ip(&mut self, pkt: &[u8]) -> Vec<u8> {
        let n = self
            .ppp
            .send(pkt, &mut self.tx)
            .expect("tx buffer too small");
        self.tx[..n].to_vec()
    }
}

impl Default for Link {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One poll + one consume, interleaved — ppproto processes one frame per poll,
    // so poll and consume must alternate (not drain-all-then-feed-all).
    fn tick(l: &mut Link, inbox: &mut Vec<u8>, outbox: &mut Vec<u8>) {
        if let Action::Tx(bytes) = l.poll() {
            outbox.extend_from_slice(&bytes);
        }
        if !inbox.is_empty() {
            let n = l.consume(inbox.as_slice());
            if n > 0 {
                inbox.drain(..n);
            }
        }
    }

    // #12 — two links back-to-back over an in-memory byte pipe reach PPP Open.
    #[test]
    fn two_links_negotiate_to_open() {
        let mut a = Link::new();
        let mut b = Link::new();
        a.open();
        b.open();
        let mut a2b = Vec::new();
        let mut b2a = Vec::new();

        for _ in 0..2000 {
            tick(&mut a, &mut b2a, &mut a2b);
            tick(&mut b, &mut a2b, &mut b2a);
            if a.phase() == Phase::Open && b.phase() == Phase::Open {
                return;
            }
        }
        panic!(
            "links did not reach Open: a={:?} b={:?}",
            a.phase(),
            b.phase()
        );
    }
}
