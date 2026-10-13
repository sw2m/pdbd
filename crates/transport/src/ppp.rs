use super::{Phase, Socket};
use ppproto::pppos::{PPPoS, PPPoSAction};
use ppproto::Config;
use std::io;
use std::time::Duration;
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf,
};
use tokio::sync::watch;

pub(crate) const MRU: usize = 1500; // RFC 1661 default; ppproto negotiates no MRU (#40)
pub(crate) const FRAME: usize = 2 * (MRU + 8) + 8; // HDLC stuffing can nearly double a frame, plus FCS/flags

#[derive(Debug)]
pub(crate) enum Action {
    Idle,
    Transmit(Vec<u8>),
    Offload(Vec<u8>),
}

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

    pub(crate) fn open(&mut self) {
        self.ppp.open().expect("L2 (PPP) link opened twice");
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
            PPPoSAction::Transmit(n) => Action::Transmit(self.tx[..n].to_vec()),
            PPPoSAction::Received(range) => {
                Action::Offload(self.rx[range].to_vec())
            }
        }
    }

    pub(crate) fn send(&mut self, guest: &[u8]) -> io::Result<Vec<u8>> {
        let n = self.ppp.send(guest, &mut self.tx).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "larger than the L2 (PPP) frame buffer",
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

struct Cadence {
    tick: tokio::time::Interval,
    open: bool,
}

impl Cadence {
    const FAST: Duration = Duration::from_millis(20); // advances ppproto's phase machine during bring-up; not a retransmit timer (#24)
    const SLOW: Duration = Duration::from_secs(1); // once Open nothing needs polling; no keepalive exists yet (#24)

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

struct Peer<T> {
    reader: ReadHalf<T>,
    writer: WriteHalf<T>,
    queue: Vec<u8>,
}

impl<T: AsyncRead + AsyncWrite> Peer<T> {
    const CAP: usize = 16 * FRAME; // ~16 full frames buffered before the offload is backpressured
    const FLUSH: Duration = Duration::from_secs(10); // > a full CAP queue at 115200 baud (~4.2 s)

    fn new(transport: T) -> Self {
        let (reader, writer) = tokio::io::split(transport);
        Self {
            reader,
            writer,
            queue: Vec::new(),
        }
    }
}

pub struct Bridge<T, O> {
    core: Core,
    peer: Peer<T>,
    offload: O,
    wire: Vec<u8>,
    phase: watch::Sender<Phase>,
    cadence: Cadence,
}

impl<T, O> Bridge<T, O>
where
    T: AsyncRead + AsyncWrite + Unpin,
    O: Socket,
{
    pub fn new(peer: T, offload: O, phase: watch::Sender<Phase>) -> Self {
        Self {
            core: Core::new(),
            peer: Peer::new(peer),
            offload,
            wire: Vec::new(),
            phase,
            cadence: Cadence::new(),
        }
    }

    pub async fn run(mut self) -> io::Result<()> {
        let mut rbuf = [0u8; 2048];
        self.core.open();
        self.pump().await?;
        loop {
            tokio::select! {
                r = self.peer.reader.read(&mut rbuf) => {
                    let n = r?;
                    if n == 0 {
                        return self.closed();
                    }
                    self.wire.extend_from_slice(&rbuf[..n]);
                    self.pump().await?;
                }
                r = self.peer.writer.write(&self.peer.queue), if !self.peer.queue.is_empty() => {
                    let n = r?;
                    if n == 0 {
                        return Err(io::ErrorKind::WriteZero.into());
                    }
                    self.peer.queue.drain(..n);
                }
                r = self.offload.receive(), if self.peer.queue.len() < Peer::<T>::CAP => match r? {
                    Some(guest) => self.frame(&guest),
                    None => return self.flush().await,
                },
                _ = self.cadence.wait() => {
                    self.pump().await?;
                }
            }
        }
    }

    fn closed(&self) -> io::Result<()> {
        if self.core.phase() == Phase::Dead {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "L1 transport closed while the L2 (PPP) link was up",
        ))
    }

    async fn flush(&mut self) -> io::Result<()> {
        let write = self.peer.writer.write_all(&self.peer.queue);
        match tokio::time::timeout(Peer::<T>::FLUSH, write).await {
            Ok(r) => r,
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "L1 transport stopped accepting L2 (PPP) frames during shutdown",
            )),
        }
    }

    fn frame(&mut self, guest: &[u8]) {
        // Nothing rides the link before Open; drop (kernel TCP retransmits).
        if self.core.phase() != Phase::Open {
            return;
        }
        match self.core.send(guest) {
            Ok(bytes) => self.peer.queue.extend_from_slice(&bytes),
            Err(e) => eprintln!(
                "transport: L2 (PPP) dropped an outbound L3 (IP) packet: {e}"
            ),
        }
    }

    // ppproto processes one frame per poll — poll and consume must interleave,
    // not drain-all-then-feed-all.
    async fn pump(&mut self) -> io::Result<()> {
        loop {
            loop {
                match self.core.poll() {
                    Action::Transmit(bytes) => {
                        self.peer.queue.extend_from_slice(&bytes)
                    }
                    Action::Offload(guest) => {
                        match self.offload.send(&guest).await {
                            Err(e)
                                if e.kind() == io::ErrorKind::InvalidInput =>
                            {
                                eprintln!("transport: L3 offload rejected an inbound L3 (IP) packet: {e}")
                            }
                            r => r?,
                        }
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
            assert_ne!(
                c, 0,
                "L2 (PPP) decoder took no input after its output drained"
            );
            self.wire.drain(..c);
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test;
