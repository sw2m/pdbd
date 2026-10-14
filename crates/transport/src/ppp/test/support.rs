use crate::ppp::{Action, Bridge, Core};
use crate::{Phase, Socket};
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};

pub(crate) struct Channel(
    pub(crate) mpsc::Sender<Vec<u8>>,
    pub(crate) mpsc::Receiver<Vec<u8>>,
);

impl Channel {
    pub(crate) fn pair(depth: usize) -> (Self, Self) {
        let forward = mpsc::channel(depth);
        let back = mpsc::channel(depth);
        (Self(forward.0, back.1), Self(back.0, forward.1))
    }
}

impl Socket for Channel {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.1.recv().await)
    }

    async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
        self.0
            .send(guest.to_vec())
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
    }
}

pub(crate) fn bridge(
    peer: DuplexStream,
    phase: watch::Sender<Phase>,
) -> (Channel, JoinHandle<io::Result<()>>) {
    let (offload, bridge) = Channel::pair(8);
    (
        bridge,
        tokio::spawn(Bridge::new(peer, offload, phase).run()),
    )
}

pub(crate) async fn await_open(ph: &mut watch::Receiver<Phase>) {
    while *ph.borrow_and_update() != Phase::Open {
        ph.changed()
            .await
            .expect("phase sender dropped before Open");
    }
}

pub(crate) struct Remote {
    core: Core,
    io: DuplexStream,
    wire: Vec<u8>,
    guests: Vec<Vec<u8>>,
}

impl Remote {
    pub(crate) fn new(io: DuplexStream) -> Self {
        let mut core = Core::new();
        core.open();
        Self {
            core,
            io,
            wire: Vec::new(),
            guests: Vec::new(),
        }
    }

    pub(crate) async fn step(&mut self) {
        loop {
            match self.core.poll() {
                Action::Transmit(bytes) => {
                    self.io.write_all(&bytes).await.unwrap()
                }
                Action::Offload(guest) => self.guests.push(guest),
                Action::Idle => break,
            }
        }
        let mut buf = [0u8; 4096];
        if let Ok(Ok(n)) =
            timeout(Duration::from_millis(5), self.io.read(&mut buf)).await
        {
            self.wire.extend_from_slice(&buf[..n]);
        }
        if !self.wire.is_empty() {
            let n = self.core.consume(self.wire.as_slice());
            self.wire.drain(..n);
        }
    }

    pub(crate) async fn open(&mut self, ph: &watch::Receiver<Phase>) {
        timeout(Duration::from_secs(5), async {
            while *ph.borrow() != Phase::Open
                || self.core.phase() != Phase::Open
            {
                self.step().await;
            }
        })
        .await
        .expect("links did not reach Open");
    }

    pub(crate) async fn send(&mut self, guest: &[u8]) {
        let frame = self.core.send(guest).unwrap();
        self.io.write_all(&frame).await.unwrap();
    }

    pub(crate) async fn write(&mut self, bytes: &[u8]) {
        self.io.write_all(bytes).await.unwrap();
    }

    pub(crate) async fn receive(&mut self) -> Vec<u8> {
        timeout(Duration::from_secs(2), async {
            while self.guests.is_empty() {
                self.step().await;
            }
        })
        .await
        .expect("no guest reached the remote");
        self.guests.remove(0)
    }

    pub(crate) async fn silent(&mut self, span: Duration) -> bool {
        let _ = timeout(span, async {
            while self.guests.is_empty() {
                self.step().await;
            }
        })
        .await;
        self.guests.is_empty()
    }
}

pub(crate) fn hdlc(proto: u16, payload: &[u8]) -> Vec<u8> {
    let mut body = proto.to_be_bytes().to_vec();
    body.extend_from_slice(payload);
    bare(&body)
}

pub(crate) fn bare(payload: &[u8]) -> Vec<u8> {
    let mut body = vec![0xff, 0x03];
    body.extend_from_slice(payload);
    let mut fcs: u16 = 0xffff;
    for &b in &body {
        fcs ^= b as u16;
        for _ in 0..8 {
            fcs = if fcs & 1 != 0 {
                (fcs >> 1) ^ 0x8408
            } else {
                fcs >> 1
            };
        }
    }
    let fcs = !fcs;
    body.extend_from_slice(&fcs.to_le_bytes());
    let mut out = vec![0x7e];
    for b in body {
        if b == 0x7e || b == 0x7d || b < 0x20 {
            out.extend_from_slice(&[0x7d, b ^ 0x20]);
        } else {
            out.push(b);
        }
    }
    out.push(0x7e);
    out
}
