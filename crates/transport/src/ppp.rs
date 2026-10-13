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
    Tx(Vec<u8>),
    Ip(Vec<u8>),
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
            PPPoSAction::Received(range) => Action::Ip(self.rx[range].to_vec()),
        }
    }

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

struct Cadence {
    tick: tokio::time::Interval,
    open: bool,
}

impl Cadence {
    const FAST: Duration = Duration::from_millis(20); // stands in for ppproto's missing retransmit timer during bring-up (#24)
    const SLOW: Duration = Duration::from_secs(1); // once Open, only keepalive-paced polling is needed

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
                        return Ok(());
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
                    None => {
                        self.peer.writer.write_all(&self.peer.queue).await?;
                        return Ok(());
                    }
                },
                _ = self.cadence.wait() => {
                    self.pump().await?;
                }
            }
        }
    }

    fn frame(&mut self, guest: &[u8]) {
        // Nothing rides the link before Open; drop (kernel TCP retransmits).
        if self.core.phase() != Phase::Open {
            return;
        }
        match self.core.send(guest) {
            Ok(bytes) => self.peer.queue.extend_from_slice(&bytes),
            Err(e) => eprintln!("transport: dropping unsendable guest: {e}"),
        }
    }

    // ppproto processes one frame per poll — poll and consume must interleave,
    // not drain-all-then-feed-all.
    async fn pump(&mut self) -> io::Result<()> {
        loop {
            loop {
                match self.core.poll() {
                    Action::Tx(bytes) => {
                        self.peer.queue.extend_from_slice(&bytes)
                    }
                    Action::Ip(guest) => self.offload.send(&guest).await?,
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

#[cfg(test)]
mod test;
