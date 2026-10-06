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

use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use ppproto::pppos::{PPPoS, PPPoSAction};
use ppproto::Config;
pub use ppproto::Phase;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};
use tun_rs::DeviceBuilder;

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

/// Run the link over a byte transport: drive PPP to `Open`, then shuttle IP
/// packets between the peer and the TUN side.
///
/// `from_tun` carries IP packets read from the local TUN (to send to the peer);
/// `to_tun` receives IP packets arriving from the peer (to write to the TUN);
/// `phase` publishes the current PPP phase.
pub async fn run<T>(
    mut link: Link,
    transport: T,
    mut from_tun: mpsc::Receiver<Vec<u8>>,
    to_tun: mpsc::Sender<Vec<u8>>,
    phase: watch::Sender<Phase>,
) -> io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let (mut rd, mut wr) = tokio::io::split(transport);
    let mut rbuf = [0u8; 2048];
    let mut inbox: Vec<u8> = Vec::new();

    link.open();
    // ppproto has no timers (that is #12-B); a light poll cadence drives
    // negotiation forward between I/O events on a clean link.
    let mut tick = tokio::time::interval(Duration::from_millis(20));

    process(&mut link, &mut inbox, &mut wr, &to_tun, &phase).await?;
    loop {
        tokio::select! {
            r = rd.read(&mut rbuf) => {
                let n = r?;
                if n == 0 {
                    return Ok(()); // transport closed
                }
                inbox.extend_from_slice(&rbuf[..n]);
                process(&mut link, &mut inbox, &mut wr, &to_tun, &phase).await?;
            }
            Some(pkt) = from_tun.recv() => {
                let bytes = link.send_ip(&pkt);
                wr.write_all(&bytes).await?;
            }
            _ = tick.tick() => {
                process(&mut link, &mut inbox, &mut wr, &to_tun, &phase).await?;
            }
        }
    }
}

// Drain outputs (poll until Idle), then consume one frame's worth of the inbox;
// repeat. Mirrors ppproto's one-frame-per-poll contract (poll and consume must
// interleave, not drain-all-then-feed-all).
async fn process<W>(
    link: &mut Link,
    inbox: &mut Vec<u8>,
    wr: &mut W,
    to_tun: &mpsc::Sender<Vec<u8>>,
    phase: &watch::Sender<Phase>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    loop {
        loop {
            match link.poll() {
                Action::Tx(bytes) => wr.write_all(&bytes).await?,
                Action::Ip(pkt) => {
                    let _ = to_tun.send(pkt).await;
                }
                Action::Idle => break,
            }
        }
        phase.send_replace(link.phase());
        if inbox.is_empty() {
            break;
        }
        let c = link.consume(inbox.as_slice());
        if c == 0 {
            break; // a frame is pending; the next poll drains it, next process() retries
        }
        inbox.drain(..c);
    }
    Ok(())
}

/// Create a point-to-point kernel TUN (local ↔ peer, statically addressed) and
/// run the link over `transport`, bridging IP packets between the TUN and the peer.
///
/// Privileged: creating a TUN needs `CAP_NET_ADMIN`. Addresses are static (from
/// config), not from IPCP — ppproto cannot assign one (see the module note).
pub async fn run_tun<T>(
    transport: T,
    local: Ipv4Addr,
    peer: Ipv4Addr,
    name: Option<&str>,
    phase: watch::Sender<Phase>,
) -> io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let mut builder = DeviceBuilder::new().ipv4(local, 32u8, Some(peer));
    if let Some(n) = name {
        builder = builder.name(n);
    }
    let dev = Arc::new(builder.build_async()?);

    // recv/send take &self, so the two directions share the device.
    let (to_tun_tx, mut to_tun_rx) = mpsc::channel::<Vec<u8>>(64);
    let (from_tun_tx, from_tun_rx) = mpsc::channel::<Vec<u8>>(64);

    let dev_rx = dev.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            match dev_rx.recv(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if from_tun_tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    let dev_tx = dev.clone();
    tokio::spawn(async move {
        while let Some(pkt) = to_tun_rx.recv().await {
            if dev_tx.send(&pkt).await.is_err() {
                break;
            }
        }
    });

    run(Link::new(), transport, from_tun_rx, to_tun_tx, phase).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    // One poll + one consume, interleaved — ppproto processes one frame per poll.
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

    // #12 — the async pump brings two links up over a duplex transport and
    // shuttles an IP packet from one TUN side to the other.
    #[tokio::test]
    async fn pump_brings_link_up_and_passes_ip() {
        let (a_io, b_io) = tokio::io::duplex(4096);
        let (a_from_tx, a_from_rx) = mpsc::channel::<Vec<u8>>(8);
        let (a_to_tx, _a_to_rx) = mpsc::channel::<Vec<u8>>(8);
        let (_b_from_tx, b_from_rx) = mpsc::channel::<Vec<u8>>(8);
        let (b_to_tx, mut b_to_rx) = mpsc::channel::<Vec<u8>>(8);
        let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
        let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);

        tokio::spawn(run(Link::new(), a_io, a_from_rx, a_to_tx, a_ph_tx));
        tokio::spawn(run(Link::new(), b_io, b_from_rx, b_to_tx, b_ph_tx));

        timeout(Duration::from_secs(5), async {
            loop {
                if *a_ph.borrow_and_update() == Phase::Open
                    && *b_ph.borrow_and_update() == Phase::Open
                {
                    return;
                }
                tokio::select! {
                    _ = a_ph.changed() => {}
                    _ = b_ph.changed() => {}
                }
            }
        })
        .await
        .expect("links did not reach Open");

        let pkt = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4, 5, 6, 7, 8];
        a_from_tx.send(pkt.clone()).await.unwrap();
        let got = timeout(Duration::from_secs(2), b_to_rx.recv())
            .await
            .expect("no packet delivered")
            .expect("channel closed");
        assert_eq!(got, pkt);
    }
}
