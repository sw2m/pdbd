//! The pdbd transport: the lower stack that moves bytes — RFC 1661/1332 PPP
//! (via `ppproto`), the L2 link, over an L1 byte pipe, terminated into a kernel
//! TUN so the kernel's IP stack runs on the link. (L1 establishment will live
//! here too; socat-style addressing may be a separate crate.)
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

/// PPP MRU we size buffers and the TUN for; ppproto defaults to 1500.
const MRU: usize = 1500;
/// Frame buffer size. Worst-case HDLC byte-stuffing can nearly double a frame
/// (every byte escaped) on top of protocol/FCS/flag overhead, so a full-MRU IP
/// packet must still fit once encoded. Sized so it never overflows at the MRU.
const FRAME: usize = 2 * (MRU + 8) + 8;

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
            tx: vec![0; FRAME],
            rx: vec![0; FRAME],
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

    /// Frame an IP packet (from the TUN) for transmission over L1. An oversized
    /// packet (beyond the frame buffer) is an error, not a panic — the caller
    /// drops it and the link survives.
    pub fn send(&mut self, pkt: &[u8]) -> io::Result<Vec<u8>> {
        let n = self.ppp.send(pkt, &mut self.tx).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "IP packet exceeds PPP frame buffer",
            )
        })?;
        Ok(self.tx[..n].to_vec())
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
                // Nothing rides the link before it reaches Open — drop until then.
                // (Expected during the brief negotiation window; the kernel's TCP
                // retransmits. Dropped silently to avoid log noise on bring-up.)
                if link.phase() != Phase::Open {
                    continue;
                }
                match link.send(&pkt) {
                    Ok(bytes) => wr.write_all(&bytes).await?,
                    Err(e) => eprintln!("transport: dropping unsendable packet: {e}"),
                }
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
        // Only notify on an actual phase change — not every poll tick.
        phase.send_if_modified(|cur| {
            let now = link.phase();
            if *cur != now {
                *cur = now;
                true
            } else {
                false
            }
        });
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

/// Kernel-TUN integration: create a point-to-point TUN and run the link over it.
pub mod tun {
    use super::*;

    /// Create a point-to-point kernel TUN (local ↔ peer, statically addressed) and
    /// run the link over `transport`, bridging IP packets between the TUN and the peer.
    ///
    /// Privileged: creating a TUN needs `CAP_NET_ADMIN`. Addresses are static (from
    /// config), not from IPCP — ppproto cannot assign one (see the module note).
    pub async fn run<T>(
        transport: T,
        local: Ipv4Addr,
        peer: Ipv4Addr,
        name: Option<&str>,
        phase: watch::Sender<Phase>,
    ) -> io::Result<()>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let mut builder = DeviceBuilder::new()
            .ipv4(local, 32u8, Some(peer))
            .mtu(MRU as u16);
        if let Some(n) = name {
            builder = builder.name(n);
        }
        let dev = Arc::new(builder.build_async()?);

        // recv/send take &self, so the two directions share the device.
        let (to_tun_tx, mut to_tun_rx) = mpsc::channel::<Vec<u8>>(64);
        let (from_tun_tx, from_tun_rx) = mpsc::channel::<Vec<u8>>(64);

        let dev_rx = dev.clone();
        let rx_task = tokio::spawn(async move {
            let mut buf = [0u8; FRAME];
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
        let tx_task = tokio::spawn(async move {
            while let Some(pkt) = to_tun_rx.recv().await {
                if dev_tx.send(&pkt).await.is_err() {
                    break;
                }
            }
        });

        // The device-read task parks on `recv()` and would otherwise outlive the
        // pump (it never observes the closed channel) — abort both on teardown so
        // nothing, and no TUN handle, leaks.
        let res = super::run(Link::new(), transport, from_tun_rx, to_tun_tx, phase).await;
        rx_task.abort();
        tx_task.abort();
        res
    }
}

#[cfg(test)]
mod tests;
