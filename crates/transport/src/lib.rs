//! See the crate README for the layer model.
//!
//! #12-A is the clean-link datapath; the retransmit/timer/keepalive layer
//! ppproto lacks is #12-B (owner/repo#24, upstream embassy-rs/ppproto#5).
//!
//! ppproto's IPCP accepts a peer address but never assigns one, so the TUN is
//! addressed statically from config rather than from IPCP.
//!
//! Datapath structure: `ppp::Socket` + `tun::Socket`, joined by `splice` (#41).

use std::io;
use std::time::Duration;

pub use ppproto::Phase;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};

/// PPP MRU we size buffers and the TUN for; fixed at the RFC 1661 default —
/// ppproto negotiates no MRU (#40), so both ends are pinned here.
const MRU: usize = 1500;
/// HDLC byte-stuffing can nearly double a frame, plus FCS/flag overhead — a
/// full-MRU packet must still fit once encoded.
const FRAME: usize = 2 * (MRU + 8) + 8;

pub mod ppp {
    use super::{Phase, FRAME};
    use ppproto::pppos::{PPPoS, PPPoSAction};
    use ppproto::Config;
    use std::io;

    #[derive(Debug)]
    pub enum Action {
        Idle,
        Tx(Vec<u8>),
        Ip(Vec<u8>),
    }

    /// The PPP socket: `ppproto` (the encapsulator) plus the pump's inlet/outlet
    /// buffers. Sync — `splice` drives it over a byte transport.
    pub struct Socket {
        ppp: PPPoS<'static>,
        tx: Vec<u8>,
        rx: Vec<u8>,
    }

    impl Socket {
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

        /// Begin negotiation; the first LCP Configure-Request goes out on the next poll.
        pub fn open(&mut self) {
            self.ppp
                .open()
                .expect("ppp::Socket::open on a non-fresh socket");
        }

        pub fn phase(&self) -> Phase {
            self.ppp.status().phase
        }

        pub fn consume(&mut self, data: &[u8]) -> usize {
            self.ppp.consume(data, &mut self.rx)
        }

        pub fn poll(&mut self) -> Action {
            match self.ppp.poll(&mut self.tx, &mut self.rx) {
                PPPoSAction::None => Action::Idle,
                PPPoSAction::Transmit(n) => Action::Tx(self.tx[..n].to_vec()),
                PPPoSAction::Received(range) => {
                    Action::Ip(self.rx[range].to_vec())
                }
            }
        }

        /// Frame an IP packet for L1. Oversized (beyond the frame buffer) is an
        /// error, not a panic — the caller drops it and the link survives.
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

    impl Default for Socket {
        fn default() -> Self {
            Self::new()
        }
    }
}

/// Phase-aware poll cadence. ppproto has no timers (#24), so a tick nudges
/// negotiation forward; it ticks fast during bring-up, then relaxes once `Open`
/// so a settled, quiet link isn't polled every 20ms.
struct Cadence {
    tick: tokio::time::Interval,
    open: bool,
}

impl Cadence {
    const FAST: Duration = Duration::from_millis(20);
    const SLOW: Duration = Duration::from_secs(1);

    fn new() -> Self {
        Self {
            tick: tokio::time::interval(Self::FAST),
            open: false,
        }
    }

    fn adjust(&mut self, phase: Phase) {
        let open = phase == Phase::Open;
        if open != self.open {
            self.open = open;
            let period = if open { Self::SLOW } else { Self::FAST };
            self.tick = tokio::time::interval(period);
        }
    }

    async fn wait(&mut self) {
        self.tick.tick().await;
    }
}

/// The datapath: drive a `ppp::Socket` over `transport` to `Open`, then splice IP
/// packets to/from the TUN side — `from_tun` carries packets read off the TUN,
/// `to_tun` receives packets decoded from the peer.
pub async fn splice<T>(
    mut sock: ppp::Socket,
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
    let mut cadence = Cadence::new();

    sock.open();
    process(
        &mut sock,
        &mut inbox,
        &mut wr,
        &to_tun,
        &phase,
        &mut cadence,
    )
    .await?;
    loop {
        tokio::select! {
            r = rd.read(&mut rbuf) => {
                let n = r?;
                if n == 0 {
                    return Ok(()); // transport closed
                }
                inbox.extend_from_slice(&rbuf[..n]);
                process(&mut sock, &mut inbox, &mut wr, &to_tun, &phase, &mut cadence).await?;
            }
            Some(pkt) = from_tun.recv() => {
                // Nothing rides the link before Open; drop silently (kernel TCP retransmits).
                if sock.phase() != Phase::Open {
                    continue;
                }
                match sock.send(&pkt) {
                    Ok(bytes) => wr.write_all(&bytes).await?,
                    Err(e) => eprintln!("transport: dropping unsendable packet: {e}"),
                }
            }
            _ = cadence.wait() => {
                process(&mut sock, &mut inbox, &mut wr, &to_tun, &phase, &mut cadence).await?;
            }
        }
    }
}

// ppproto processes one frame per poll — poll and consume must interleave,
// not drain-all-then-feed-all.
async fn process<W>(
    sock: &mut ppp::Socket,
    inbox: &mut Vec<u8>,
    wr: &mut W,
    to_tun: &mpsc::Sender<Vec<u8>>,
    phase: &watch::Sender<Phase>,
    cadence: &mut Cadence,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    loop {
        loop {
            match sock.poll() {
                ppp::Action::Tx(bytes) => wr.write_all(&bytes).await?,
                ppp::Action::Ip(pkt) => {
                    let _ = to_tun.send(pkt).await;
                }
                ppp::Action::Idle => break,
            }
        }
        let now = sock.phase();
        phase.send_if_modified(|cur| {
            if *cur != now {
                *cur = now;
                true
            } else {
                false
            }
        });
        cadence.adjust(now);
        if inbox.is_empty() {
            break;
        }
        let c = sock.consume(inbox.as_slice());
        if c == 0 {
            break; // a frame is pending; the next poll drains it, next process() retries
        }
        inbox.drain(..c);
    }
    Ok(())
}

pub mod tun {
    use super::{ppp, splice, Phase, FRAME, MRU};
    use std::io;
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio::sync::{mpsc, watch};
    use tokio::task::JoinHandle;
    use tun_rs::DeviceBuilder;

    /// Aborts its task on drop, so the device-I/O tasks never outlive the datapath.
    struct AbortOnDrop(JoinHandle<()>);

    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }

    /// A point-to-point kernel TUN presented as an IP-packet socket: `from_tun`
    /// carries packets read off the device, `to_tun` delivers packets to it. The
    /// device-I/O tasks are owned here and abort on drop. Privileged (CAP_NET_ADMIN).
    pub struct Socket {
        from_tun: mpsc::Receiver<Vec<u8>>,
        to_tun: mpsc::Sender<Vec<u8>>,
        _rx: AbortOnDrop,
        _tx: AbortOnDrop,
    }

    impl Socket {
        pub fn open(
            local: Ipv4Addr,
            peer: Ipv4Addr,
            name: Option<&str>,
        ) -> io::Result<Self> {
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
            let rx = AbortOnDrop(tokio::spawn(async move {
                let mut buf = [0u8; FRAME];
                loop {
                    match dev_rx.recv(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if from_tun_tx
                                .send(buf[..n].to_vec())
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }));
            let dev_tx = dev.clone();
            let tx = AbortOnDrop(tokio::spawn(async move {
                while let Some(pkt) = to_tun_rx.recv().await {
                    if dev_tx.send(&pkt).await.is_err() {
                        break;
                    }
                }
            }));

            Ok(Self {
                from_tun: from_tun_rx,
                to_tun: to_tun_tx,
                _rx: rx,
                _tx: tx,
            })
        }

        /// Splice a fresh PPP socket (over `transport`) to this TUN socket; the
        /// device-I/O tasks abort when this returns (the guards drop).
        pub async fn run<T>(
            self,
            transport: T,
            phase: watch::Sender<Phase>,
        ) -> io::Result<()>
        where
            T: AsyncRead + AsyncWrite + Unpin,
        {
            let Self {
                from_tun,
                to_tun,
                _rx,
                _tx,
            } = self;
            splice(ppp::Socket::new(), transport, from_tun, to_tun, phase).await
        }
    }

    /// Convenience: open a kernel TUN and splice it to a PPP socket over `transport`.
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
        Socket::open(local, peer, name)?.run(transport, phase).await
    }
}

#[cfg(test)]
mod tests;
