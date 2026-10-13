use super::*;
use crate::Phase;
use std::io;
use tokio::io::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream,
};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};

fn tick(l: &mut Core, inbox: &mut Vec<u8>, outbox: &mut Vec<u8>) {
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

// #12
#[test]
fn two_links_negotiate_to_open() {
    let mut a = Core::new();
    let mut b = Core::new();
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
        "cores did not reach Open: a={:?} b={:?}",
        a.phase(),
        b.phase()
    );
}

// #12
#[test]
fn send_oversized_errs_not_panics() {
    let mut l = Core::new();
    let big = vec![0x7eu8; FRAME];
    assert!(l.send(&big).is_err());
}

// #12
#[test]
fn send_full_mru_ok() {
    let mut l = Core::new();
    let pkt = vec![0x41u8; MRU];
    assert!(l.send(&pkt).is_ok());
}

// #12
#[test]
fn garbage_bytes_do_not_panic_or_open() {
    let mut l = Core::new();
    l.open();
    let mut inbox: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    for _ in 0..2000 {
        let _ = l.poll();
        if inbox.is_empty() {
            break;
        }
        let n = l.consume(inbox.as_slice());
        if n == 0 {
            break;
        }
        inbox.drain(..n);
    }
    assert_ne!(l.phase(), Phase::Open, "garbage must not bring the link up");
}

struct Channel(mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>);

impl Channel {
    fn pair() -> (Self, Self) {
        let forward = mpsc::channel(8);
        let back = mpsc::channel(8);
        (Self(forward.0, back.1), Self(back.0, forward.1))
    }
}

impl crate::Socket for Channel {
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

fn spawn<T>(
    peer: T,
    phase: watch::Sender<Phase>,
) -> (Channel, JoinHandle<io::Result<()>>)
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (offload, bridge) = Channel::pair();
    (
        bridge,
        tokio::spawn(Bridge::new(peer, offload, phase).run()),
    )
}

async fn await_open(ph: &mut watch::Receiver<Phase>) {
    while *ph.borrow_and_update() != Phase::Open {
        ph.changed()
            .await
            .expect("phase sender dropped before Open");
    }
}

// #12
#[tokio::test]
async fn bridges_open_and_pass_ip() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
    let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);
    let (mut a, _a_task) = spawn(a_io, a_ph_tx);
    let (mut b, _b_task) = spawn(b_io, b_ph_tx);

    timeout(Duration::from_secs(5), async {
        await_open(&mut a_ph).await;
        await_open(&mut b_ph).await;
    })
    .await
    .expect("links did not reach Open");

    let pkt = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4, 5, 6, 7, 8];
    a.send(&pkt).await.unwrap();
    let got = timeout(Duration::from_secs(2), b.receive())
        .await
        .expect("no packet delivered")
        .expect("bridge errored");
    assert_eq!(got, Some(pkt));
}

// #12
#[tokio::test]
async fn packets_before_open_are_dropped() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
    let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);
    let (mut a, _a_task) = spawn(a_io, a_ph_tx);
    let (mut b, _b_task) = spawn(b_io, b_ph_tx);

    let early = vec![0x45u8, 0, 0, 20, 9, 9, 9, 9, 0, 0, 0, 0];
    a.send(&early).await.unwrap();

    timeout(Duration::from_secs(5), async {
        await_open(&mut a_ph).await;
        await_open(&mut b_ph).await;
    })
    .await
    .expect("links did not reach Open");

    let real = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4, 5, 6, 7, 8];
    a.send(&real).await.unwrap();

    let got = timeout(Duration::from_secs(2), b.receive())
        .await
        .expect("no packet delivered")
        .expect("bridge errored");
    assert_eq!(got, Some(real));
}

// #12
#[tokio::test]
async fn bridge_ends_on_dead_transport() {
    let (a_io, b_io) = tokio::io::duplex(64);
    drop(b_io);
    let (ph_tx, _ph) = watch::channel(Phase::Dead);
    let (mut a, task) = spawn(a_io, ph_tx);

    let r = timeout(Duration::from_secs(2), task).await;
    assert!(
        matches!(r, Ok(Ok(_))),
        "bridge did not end on a dead transport: {r:?}"
    );
    assert_eq!(a.receive().await.unwrap(), None);
}

// #12
#[tokio::test]
async fn phase_watch_quiesces_when_stable() {
    let (a_io, _b_io) = tokio::io::duplex(4096); // _b_io kept open → no EOF
    let (ph_tx, mut ph) = watch::channel(Phase::Dead);
    let (_a, _task) = spawn(a_io, ph_tx); // _a kept → the offload stays open and the bridge keeps running

    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = ph.borrow_and_update();

    let fired = timeout(Duration::from_millis(300), ph.changed()).await;
    assert!(
        fired.is_err(),
        "phase watch kept firing while the phase was stable"
    );
}

async fn step(peer: &mut Core, io: &mut DuplexStream, wire: &mut Vec<u8>) {
    loop {
        match peer.poll() {
            Action::Tx(bytes) => io.write_all(&bytes).await.unwrap(),
            Action::Ip(_) => {}
            Action::Idle => break,
        }
    }
    let mut buf = [0u8; 2048];
    if let Ok(Ok(n)) =
        timeout(Duration::from_millis(5), io.read(&mut buf)).await
    {
        wire.extend_from_slice(&buf[..n]);
    }
    if !wire.is_empty() {
        let n = peer.consume(wire.as_slice());
        wire.drain(..n);
    }
}

// #12
#[tokio::test]
async fn inbound_flows_while_outbound_is_blocked() {
    let (a_io, mut p_io) = tokio::io::duplex(256);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (mut a, _task) = spawn(a_io, ph_tx);

    let mut peer = Core::new();
    peer.open();
    let mut wire = Vec::new();
    timeout(Duration::from_secs(5), async {
        while *ph.borrow() != Phase::Open || peer.phase() != Phase::Open {
            step(&mut peer, &mut p_io, &mut wire).await;
        }
    })
    .await
    .expect("links did not reach Open");

    let flood = a.0.clone();
    tokio::spawn(async move {
        for _ in 0..64 {
            if flood.send(vec![0x45u8; 1000]).await.is_err() {
                break;
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let guest = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4, 5, 6, 7, 8];
    p_io.write_all(&peer.send(&guest).unwrap()).await.unwrap();
    let got = timeout(Duration::from_secs(2), a.receive())
        .await
        .expect("inbound stalled behind the blocked outbound write")
        .unwrap();
    assert_eq!(got, Some(guest));
}
