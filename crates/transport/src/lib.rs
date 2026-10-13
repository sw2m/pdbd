//! See the crate README for the layer model.
//!
//! #12-A is the clean-link datapath; the retransmit/timer/keepalive layer
//! ppproto lacks is #12-B (owner/repo#24, upstream embassy-rs/ppproto#5).
//!
//! ppproto's IPCP accepts a peer address but never assigns one, so the TUN is
//! addressed statically from config rather than from IPCP.
//!
//! Everything is a socket. `splice` copies *guests* (the IP payloads) between a
//! `peer` — another transport endpoint, via `ppp::Socket` — and an *offload*,
//! whoever takes responsibility for the guests (here `tun::Socket` → the
//! kernel). The core crate never handles L3 itself; it always offloads. The
//! *bridge* is the node in between: the two sockets and the `splice` joining
//! them. Inside a socket, the end facing outward is named for its party (`peer`
//! or `offload`); the end facing inward is named `bridge`.

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

pub mod ppp {
    use super::{AbortOnDrop, Phase, FRAME};
    use ppproto::pppos::{PPPoS, PPPoSAction};
    use ppproto::Config;
    use std::io;
    use std::time::Duration;
    use tokio::io::{
        AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf,
    };
    use tokio::sync::{mpsc, watch};

    #[derive(Debug)]
    pub(crate) enum Action {
        Idle,
        Tx(Vec<u8>),
        Ip(Vec<u8>),
    }

    /// The sync codec+pump: `ppproto` (the encapsulator) plus the pump's
    /// inlet/outlet buffers. The `Driver` turns it over a byte transport.
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

        /// Frame a guest for L1. Oversized (beyond the frame buffer) is an error,
        /// not a panic — the caller drops it and the link survives.
        pub(crate) fn send(&mut self, guest: &[u8]) -> io::Result<Vec<u8>> {
            let n = self.ppp.send(guest, &mut self.tx).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "guest exceeds PPP frame buffer",
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

    /// Turns a `Core` over a byte transport: the state machine that negotiates to
    /// `Open`, hands the peer's guests to the bridge, and frames the bridge's
    /// guests back to the peer.
    struct Driver<T> {
        core: Core,
        rd: ReadHalf<T>,
        wr: WriteHalf<T>,
        wire: Vec<u8>,
        bridge_tx: mpsc::Sender<Vec<u8>>,
        bridge_rx: mpsc::Receiver<Vec<u8>>,
        phase: watch::Sender<Phase>,
        cadence: Cadence,
    }

    impl<T: AsyncRead + AsyncWrite + Unpin> Driver<T> {
        fn new(
            transport: T,
            bridge_tx: mpsc::Sender<Vec<u8>>,
            bridge_rx: mpsc::Receiver<Vec<u8>>,
            phase: watch::Sender<Phase>,
        ) -> Self {
            let (rd, wr) = tokio::io::split(transport);
            Self {
                core: Core::new(),
                rd,
                wr,
                wire: Vec::new(),
                bridge_tx,
                bridge_rx,
                phase,
                cadence: Cadence::new(),
            }
        }

        async fn run(mut self) -> io::Result<()> {
            let mut rbuf = [0u8; 2048];
            self.core.open();
            self.pump().await?;
            loop {
                tokio::select! {
                    r = self.rd.read(&mut rbuf) => {
                        let n = r?;
                        if n == 0 {
                            return Ok(()); // transport closed
                        }
                        self.wire.extend_from_slice(&rbuf[..n]);
                        self.pump().await?;
                    }
                    Some(guest) = self.bridge_rx.recv() => {
                        // Nothing rides the link before Open; drop (kernel TCP retransmits).
                        if self.core.phase() != Phase::Open {
                            continue;
                        }
                        match self.core.send(&guest) {
                            Ok(bytes) => self.wr.write_all(&bytes).await?,
                            Err(e) => eprintln!("transport: dropping unsendable guest: {e}"),
                        }
                    }
                    _ = self.cadence.wait() => {
                        self.pump().await?;
                    }
                }
            }
        }

        // ppproto processes one frame per poll — poll and consume must interleave,
        // not drain-all-then-feed-all.
        async fn pump(&mut self) -> io::Result<()> {
            loop {
                loop {
                    match self.core.poll() {
                        Action::Tx(bytes) => self.wr.write_all(&bytes).await?,
                        Action::Ip(guest) => {
                            let _ = self.bridge_tx.send(guest).await;
                        }
                        Action::Idle => break,
                    }
                }
                let now = self.core.phase();
                self.phase.send_if_modified(|cur| {
                    if *cur != now {
                        *cur = now;
                        true
                    } else {
                        false
                    }
                });
                self.cadence.adjust(now);
                if self.wire.is_empty() {
                    break;
                }
                let c = self.core.consume(self.wire.as_slice());
                if c == 0 {
                    break; // a frame is pending; the next poll drains it, next pump() retries
                }
                self.wire.drain(..c);
            }
            Ok(())
        }
    }

    /// A PPP packet socket over a byte transport — your handle to the `peer`. A
    /// `Driver` task runs the protocol so `send`/`receive` are just guest queues.
    pub struct Socket {
        peer_rx: mpsc::Receiver<Vec<u8>>,
        peer_tx: mpsc::Sender<Vec<u8>>,
        _driver: AbortOnDrop,
    }

    impl Socket {
        /// Bring up a PPP socket over `transport`, publishing negotiation phase to
        /// `phase`. The driver runs on an internal task that aborts on drop.
        pub fn connect<T>(transport: T, phase: watch::Sender<Phase>) -> Self
        where
            T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        {
            let (bridge_tx, peer_rx) = mpsc::channel(64);
            let (peer_tx, bridge_rx) = mpsc::channel(64);
            let driver = AbortOnDrop(tokio::spawn(async move {
                let _ = Driver::new(transport, bridge_tx, bridge_rx, phase)
                    .run()
                    .await;
            }));
            Self {
                peer_rx,
                peer_tx,
                _driver: driver,
            }
        }
    }

    impl crate::Socket for Socket {
        async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
            Ok(self.peer_rx.recv().await)
        }

        async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
            self.peer_tx.send(guest.to_vec()).await.map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "ppp socket closed")
            })
        }
    }
}

pub mod tun {
    use super::{AbortOnDrop, Phase, FRAME, MRU};
    use std::io;
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio::sync::{mpsc, watch};
    use tun_rs::DeviceBuilder;

    /// A point-to-point kernel TUN as a packet socket — one `offload` for guests.
    /// `receive` reads off the device, `send` writes to it; the device-I/O tasks
    /// abort on drop. Privileged (CAP_NET_ADMIN).
    pub struct Socket {
        offload_rx: mpsc::Receiver<Vec<u8>>,
        offload_tx: mpsc::Sender<Vec<u8>>,
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
            let (bridge_tx, offload_rx) = mpsc::channel::<Vec<u8>>(64);
            let (offload_tx, mut bridge_rx) = mpsc::channel::<Vec<u8>>(64);

            let dev_rx = dev.clone();
            let rx = AbortOnDrop(tokio::spawn(async move {
                let mut buf = [0u8; FRAME];
                loop {
                    match dev_rx.recv(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if bridge_tx.send(buf[..n].to_vec()).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }));
            let dev_tx = dev.clone();
            let tx = AbortOnDrop(tokio::spawn(async move {
                while let Some(guest) = bridge_rx.recv().await {
                    if dev_tx.send(&guest).await.is_err() {
                        break;
                    }
                }
            }));

            Ok(Self {
                offload_rx,
                offload_tx,
                _rx: rx,
                _tx: tx,
            })
        }
    }

    impl crate::Socket for Socket {
        async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
            Ok(self.offload_rx.recv().await)
        }

        async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
            self.offload_tx.send(guest.to_vec()).await.map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "tun socket closed")
            })
        }
    }

    /// Convenience: splice a `peer` (PPP over `transport`) to a fresh kernel TUN.
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
        super::splice(
            super::ppp::Socket::connect(transport, phase),
            Socket::open(local, peer, name)?,
        )
        .await
    }
}

#[cfg(test)]
mod tests;
