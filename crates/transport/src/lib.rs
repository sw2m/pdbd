//! See the crate README for the layer model.
//!
//! #12-A is the clean-link datapath; the retransmit/timer/keepalive layer
//! ppproto lacks is #12-B (owner/repo#24, upstream embassy-rs/ppproto#5).
//!
//! ppproto's IPCP accepts a peer address but never assigns one, so the TUN is
//! addressed statically from config rather than from IPCP.
//!
//! Everything is a socket: `ppp::Socket` and `tun::Socket` both read/write IP
//! packets, and `splice` copies packets between any two.

use std::io;

pub use ppproto::Phase;

/// A packet socket: read and write whole IP packets. Both ends of the datapath
/// are sockets; `splice` is the only thing that joins them.
#[allow(async_fn_in_trait)] // internal trait, used via generics — no `dyn`, no Send bound needed
pub trait Socket {
    /// The next inbound packet, or `None` once the socket has closed.
    async fn recv(&mut self) -> io::Result<Option<Vec<u8>>>;
    /// Send an outbound packet.
    async fn send(&mut self, pkt: &[u8]) -> io::Result<()>;
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

/// The datapath: copy IP packets both ways between two sockets until either end
/// closes. All the protocol/device machinery lives inside the sockets.
pub async fn splice<A, B>(mut a: A, mut b: B) -> io::Result<()>
where
    A: Socket,
    B: Socket,
{
    loop {
        tokio::select! {
            r = a.recv() => match r? {
                Some(pkt) => b.send(&pkt).await?,
                None => return Ok(()),
            },
            r = b.recv() => match r? {
                Some(pkt) => a.send(&pkt).await?,
                None => return Ok(()),
            },
        }
    }
}

pub mod ppp {
    use super::{AbortOnDrop, Phase, FRAME};
    use ppproto::pppos::{PPPoS, PPPoSAction};
    use ppproto::Config;
    use std::io;
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
    use tokio::sync::{mpsc, watch};

    #[derive(Debug)]
    pub(crate) enum Action {
        Idle,
        Tx(Vec<u8>),
        Ip(Vec<u8>),
    }

    /// The sync codec+pump: `ppproto` (the encapsulator) plus the pump's
    /// inlet/outlet buffers. `Socket`'s driver turns it over a byte transport.
    pub(crate) struct Core {
        ppp: PPPoS<'static>,
        tx: Vec<u8>,
        rx: Vec<u8>,
    }

    impl Core {
        pub(crate) fn new() -> Self {
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
        pub(crate) fn open(&mut self) {
            self.ppp
                .open()
                .expect("ppp::Core::open on a non-fresh core");
        }

        pub(crate) fn phase(&self) -> Phase {
            self.ppp.status().phase
        }

        pub(crate) fn consume(&mut self, data: &[u8]) -> usize {
            self.ppp.consume(data, &mut self.rx)
        }

        pub(crate) fn poll(&mut self) -> Action {
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
        pub(crate) fn send(&mut self, pkt: &[u8]) -> io::Result<Vec<u8>> {
            let n = self.ppp.send(pkt, &mut self.tx).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "IP packet exceeds PPP frame buffer",
                )
            })?;
            Ok(self.tx[..n].to_vec())
        }
    }

    impl Default for Core {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Phase-aware poll cadence. ppproto has no timers (#24), so a tick nudges
    /// negotiation forward; it ticks fast during bring-up, then relaxes once
    /// `Open` so a settled, quiet link isn't polled every 20ms.
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

    /// A PPP packet socket over a byte transport. Its driver task runs the pump
    /// (negotiation, framing, phase) so `recv`/`send` are just packet queues.
    pub struct Socket {
        inbound: mpsc::Receiver<Vec<u8>>,
        outbound: mpsc::Sender<Vec<u8>>,
        _driver: AbortOnDrop,
    }

    impl Socket {
        /// Bring up a PPP socket over `transport`, publishing negotiation phase
        /// to `phase`. The pump runs on an internal task that aborts on drop.
        pub fn connect<T>(transport: T, phase: watch::Sender<Phase>) -> Self
        where
            T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        {
            let (in_tx, in_rx) = mpsc::channel(64);
            let (out_tx, out_rx) = mpsc::channel(64);
            let driver = AbortOnDrop(tokio::spawn(async move {
                let _ = drive(transport, in_tx, out_rx, phase).await;
            }));
            Self {
                inbound: in_rx,
                outbound: out_tx,
                _driver: driver,
            }
        }
    }

    impl crate::Socket for Socket {
        async fn recv(&mut self) -> io::Result<Option<Vec<u8>>> {
            Ok(self.inbound.recv().await)
        }

        async fn send(&mut self, pkt: &[u8]) -> io::Result<()> {
            self.outbound.send(pkt.to_vec()).await.map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "ppp socket closed")
            })
        }
    }

    /// The pump task: drive `Core` to `Open` over `transport`, delivering decoded
    /// IP packets to `inbound` and framing packets pulled from `outbound`.
    async fn drive<T>(
        transport: T,
        inbound: mpsc::Sender<Vec<u8>>,
        mut outbound: mpsc::Receiver<Vec<u8>>,
        phase: watch::Sender<Phase>,
    ) -> io::Result<()>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let mut core = Core::new();
        let (mut rd, mut wr) = tokio::io::split(transport);
        let mut rbuf = [0u8; 2048];
        let mut inbox: Vec<u8> = Vec::new();
        let mut cadence = Cadence::new();

        core.open();
        pump(
            &mut core,
            &mut inbox,
            &mut wr,
            &inbound,
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
                    pump(&mut core, &mut inbox, &mut wr, &inbound, &phase, &mut cadence).await?;
                }
                Some(pkt) = outbound.recv() => {
                    // Nothing rides the link before Open; drop (kernel TCP retransmits).
                    if core.phase() != Phase::Open {
                        continue;
                    }
                    match core.send(&pkt) {
                        Ok(bytes) => wr.write_all(&bytes).await?,
                        Err(e) => eprintln!("transport: dropping unsendable packet: {e}"),
                    }
                }
                _ = cadence.wait() => {
                    pump(&mut core, &mut inbox, &mut wr, &inbound, &phase, &mut cadence).await?;
                }
            }
        }
    }

    // ppproto processes one frame per poll — poll and consume must interleave,
    // not drain-all-then-feed-all.
    async fn pump<W>(
        core: &mut Core,
        inbox: &mut Vec<u8>,
        wr: &mut W,
        inbound: &mpsc::Sender<Vec<u8>>,
        phase: &watch::Sender<Phase>,
        cadence: &mut Cadence,
    ) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        loop {
            loop {
                match core.poll() {
                    Action::Tx(bytes) => wr.write_all(&bytes).await?,
                    Action::Ip(pkt) => {
                        let _ = inbound.send(pkt).await;
                    }
                    Action::Idle => break,
                }
            }
            let now = core.phase();
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
            let c = core.consume(inbox.as_slice());
            if c == 0 {
                break; // a frame is pending; the next poll drains it, next pump() retries
            }
            inbox.drain(..c);
        }
        Ok(())
    }
}

pub mod tun {
    use super::{ppp, splice, AbortOnDrop, Phase, FRAME, MRU};
    use std::io;
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio::sync::{mpsc, watch};
    use tun_rs::DeviceBuilder;

    /// A point-to-point kernel TUN as a packet socket: `recv` reads off the
    /// device, `send` writes to it. The device-I/O tasks abort on drop.
    /// Privileged (CAP_NET_ADMIN).
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
    }

    impl crate::Socket for Socket {
        async fn recv(&mut self) -> io::Result<Option<Vec<u8>>> {
            Ok(self.from_tun.recv().await)
        }

        async fn send(&mut self, pkt: &[u8]) -> io::Result<()> {
            self.to_tun.send(pkt.to_vec()).await.map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "tun socket closed")
            })
        }
    }

    /// Convenience: splice a PPP socket (over `transport`) to a fresh kernel TUN.
    pub async fn run<T>(
        transport: T,
        local: Ipv4Addr,
        peer: Ipv4Addr,
        name: Option<&str>,
        phase: watch::Sender<Phase>,
    ) -> io::Result<()>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let tun = Socket::open(local, peer, name)?;
        let ppp = ppp::Socket::connect(transport, phase);
        splice(ppp, tun).await
    }
}

#[cfg(test)]
mod tests;
