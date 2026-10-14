pub(crate) mod support;

use super::*;
use crate::{Phase, Socket};
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use support::{await_open, bare, bridge, hdlc, Channel, Remote};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::watch;
use tokio::time::{sleep, timeout, Duration};

fn tick(l: &mut Core, inbox: &mut Vec<u8>, outbox: &mut Vec<u8>) {
    if let Action::Transmit(bytes) = l.poll() {
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

// #12
#[tokio::test]
async fn bridges_open_and_pass_ip() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
    let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);
    let (mut a, _a_task) = bridge(a_io, a_ph_tx);
    let (mut b, _b_task) = bridge(b_io, b_ph_tx);

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
    let (mut a, _a_task) = bridge(a_io, a_ph_tx);
    let (mut b, _b_task) = bridge(b_io, b_ph_tx);

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
    let (mut a, task) = bridge(a_io, ph_tx);

    let r = timeout(Duration::from_secs(2), task).await;
    assert!(
        matches!(r, Ok(Ok(Err(_)))),
        "bridge did not fail on a dead transport: {r:?}"
    );
    assert_eq!(a.receive().await.unwrap(), None);
}

// #12
#[tokio::test]
async fn phase_watch_quiesces_when_stable() {
    let (a_io, _b_io) = tokio::io::duplex(4096); // _b_io kept open → no EOF
    let (ph_tx, mut ph) = watch::channel(Phase::Dead);
    let (_a, _task) = bridge(a_io, ph_tx); // _a kept → the offload stays open and the bridge keeps running

    sleep(Duration::from_millis(300)).await;
    let _ = ph.borrow_and_update();

    let fired = timeout(Duration::from_millis(300), ph.changed()).await;
    assert!(
        fired.is_err(),
        "phase watch kept firing while the phase was stable"
    );
}

// #12
#[tokio::test]
async fn inbound_flows_while_outbound_is_blocked() {
    let (a_io, p_io) = tokio::io::duplex(256);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (mut a, _task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    let flood = a.0.clone();
    tokio::spawn(async move {
        for _ in 0..64 {
            if flood.send(vec![0x45u8; 1000]).await.is_err() {
                break;
            }
        }
    });
    timeout(Duration::from_secs(5), async {
        while a.0.capacity() > 0 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("outbound never backed up: the peer queue never reached CAP");

    let guest = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4, 5, 6, 7, 8];
    remote.send(&guest).await;
    let got = timeout(Duration::from_secs(2), a.receive())
        .await
        .expect("inbound stalled behind the blocked outbound write")
        .unwrap();
    assert_eq!(got, Some(guest));
}

struct Flood {
    armed: watch::Receiver<bool>,
    pulled: Arc<AtomicUsize>,
}

impl Socket for Flood {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        let _ = self.armed.wait_for(|armed| *armed).await;
        self.pulled.fetch_add(1, Ordering::SeqCst);
        Ok(Some(vec![0x45u8; 1000]))
    }

    async fn send(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
}

async fn settle(pulled: &AtomicUsize) -> usize {
    timeout(Duration::from_secs(5), async {
        loop {
            let before = pulled.load(Ordering::SeqCst);
            sleep(Duration::from_millis(200)).await;
            let after = pulled.load(Ordering::SeqCst);
            if before == after {
                return after;
            }
        }
    })
    .await
    .expect("offload intake never stopped: no backpressure")
}

// #12
#[tokio::test]
async fn offload_intake_stops_at_cap_and_resumes() {
    let (a_io, p_io) = tokio::io::duplex(256);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (arm, armed) = watch::channel(false);
    let pulled = Arc::new(AtomicUsize::new(0));
    let flood = Flood {
        armed,
        pulled: pulled.clone(),
    };
    let _task = tokio::spawn(Bridge::new(a_io, flood, ph_tx).run());
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    arm.send_replace(true);
    let plateau = settle(&pulled).await;
    let cap = Peer::<tokio::io::DuplexStream>::CAP / 1000;
    assert!(
        plateau >= cap / 2 && plateau <= cap + 4,
        "intake stopped at {plateau} guests, expected about {cap}"
    );

    timeout(Duration::from_secs(5), async {
        while pulled.load(Ordering::SeqCst) <= plateau {
            remote.step().await;
        }
    })
    .await
    .expect("intake did not resume once the peer drained the queue");
}

struct Picky {
    channel: Channel,
    rejected: bool,
}

impl Socket for Picky {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        self.channel.receive().await
    }

    async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
        if !self.rejected {
            self.rejected = true;
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.channel.send(guest).await
    }
}

// #12
#[tokio::test]
async fn offload_rejecting_a_packet_drops_it_and_continues() {
    let (a_io, p_io) = tokio::io::duplex(4096);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (channel, mut a) = Channel::pair(8);
    let picky = Picky {
        channel,
        rejected: false,
    };
    let task = tokio::spawn(Bridge::new(a_io, picky, ph_tx).run());
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    remote.send(&[0x45u8, 0, 0, 20, 9, 9, 9, 9]).await;
    let kept = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4];
    remote.send(&kept).await;
    let got = timeout(Duration::from_secs(2), a.receive())
        .await
        .expect("bridge stopped delivering after the offload rejected a packet")
        .unwrap();
    assert_eq!(got, Some(kept));
    assert!(!task.is_finished());
}

// #12
#[tokio::test]
async fn offload_close_flushes_the_peer_queue() {
    let (a_io, p_io) = tokio::io::duplex(256);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (a, task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    for _ in 0..20 {
        a.0.send(vec![0x45u8; 1000]).await.unwrap();
    }
    let marker = vec![0x45u8, 0, 0, 20, 7, 7, 7, 7];
    a.0.send(marker.clone()).await.unwrap();
    while a.0.capacity() < 8 {
        sleep(Duration::from_millis(10)).await;
    }
    drop(a);

    let mut last = Vec::new();
    timeout(Duration::from_secs(5), async {
        while last != marker {
            last = remote.receive().await;
        }
    })
    .await
    .expect("queued frames were not flushed to the peer");
    let r = timeout(Duration::from_secs(2), task).await;
    assert!(
        matches!(r, Ok(Ok(Ok(())))),
        "bridge did not end cleanly: {r:?}"
    );
}

// #12
#[tokio::test(start_paused = true)]
async fn offload_close_gives_up_on_a_stalled_peer() {
    let (a_io, p_io) = tokio::io::duplex(256);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (a, task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    for _ in 0..20 {
        a.0.send(vec![0x45u8; 1000]).await.unwrap();
    }
    while a.0.capacity() < 8 {
        sleep(Duration::from_millis(10)).await;
    }
    drop(a);

    let r = timeout(Duration::from_secs(60), task).await;
    assert!(
        matches!(&r, Ok(Ok(Err(e))) if e.kind() == io::ErrorKind::TimedOut),
        "bridge hung flushing to a stalled peer: {r:?}"
    );
    drop(remote);
}

// #12
#[tokio::test]
async fn peer_eof_while_open_is_an_error() {
    let (a_io, p_io) = tokio::io::duplex(4096);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (_a, task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;
    drop(remote);

    let r = timeout(Duration::from_secs(2), task).await;
    assert!(
        matches!(&r, Ok(Ok(Err(e))) if e.kind() == io::ErrorKind::UnexpectedEof),
        "peer EOF while Open was not reported: {r:?}"
    );
}

// #12
#[tokio::test]
async fn peer_eof_mid_frame_is_an_error() {
    let (a_io, p_io) = tokio::io::duplex(4096);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (_a, task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;
    let frame = hdlc(0x0021, &[0x45u8; 40]);
    remote.write(&frame[..frame.len() / 2]).await;
    drop(remote);

    let r = timeout(Duration::from_secs(2), task).await;
    assert!(
        matches!(&r, Ok(Ok(Err(e))) if e.kind() == io::ErrorKind::UnexpectedEof),
        "peer EOF mid-frame was not reported: {r:?}"
    );
}

struct ZeroWriter;

impl AsyncRead for ZeroWriter {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

impl AsyncWrite for ZeroWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(0))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

// #12
#[tokio::test]
async fn peer_accepting_no_bytes_is_an_error() {
    let (ph_tx, _ph) = watch::channel(Phase::Dead);
    let (offload, _a) = Channel::pair(8);
    let r = timeout(
        Duration::from_secs(2),
        Bridge::new(ZeroWriter, offload, ph_tx).run(),
    )
    .await;
    assert!(
        matches!(&r, Ok(Err(e)) if e.kind() == io::ErrorKind::WriteZero),
        "a transport writing zero bytes was not reported: {r:?}"
    );
}

// #12
#[tokio::test]
async fn oversized_inbound_frame_is_dropped() {
    let (a_io, p_io) = tokio::io::duplex(16384);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (mut a, _task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    remote.write(&hdlc(0x0021, &[0x45u8; 4000])).await;
    let kept = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4];
    remote.send(&kept).await;
    let got = timeout(Duration::from_secs(2), a.receive())
        .await
        .expect("a valid frame after an oversized one was not delivered")
        .unwrap();
    assert_eq!(got, Some(kept));
    assert_eq!(*ph.borrow(), Phase::Open);
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

async fn survives(hostile: Vec<u8>) {
    let (a_io, p_io) = tokio::io::duplex(16384);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let (mut a, task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    remote.write(&hostile).await;
    let kept = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4];
    remote.send(&kept).await;
    let got = timeout(Duration::from_secs(2), a.receive())
        .await
        .expect("the bridge stopped delivering after a hostile frame")
        .unwrap();
    assert_eq!(got, Some(kept));
    assert_eq!(*ph.borrow(), Phase::Open);
    assert!(!task.is_finished());
}

// #45 — embassy-rs/ppproto#6 case A: an Echo-Reply larger than the reply buffer
#[tokio::test]
async fn survives_oversized_echo_request() {
    let mut payload = hex("092a00d000000000");
    payload.resize(0xd0, b'A');
    survives(hdlc(0xc021, &payload)).await;
}

// #45 — embassy-rs/ppproto#6 case A: a Protocol-Reject of a large non-IPv4 packet
#[tokio::test]
async fn survives_oversized_unknown_protocol() {
    let mut payload = vec![0x60u8];
    payload.resize(200, 0);
    survives(hdlc(0x0057, &payload)).await;
}

// #45 — embassy-rs/ppproto#6 case B
#[tokio::test]
async fn survives_frame_without_protocol() {
    survives(bare(&[])).await;
}

// #45 — embassy-rs/ppproto#6 case C
#[tokio::test]
async fn survives_lcp_length_below_header() {
    survives(hdlc(0xc021, &hex("010500020000"))).await;
}

// #45 — embassy-rs/ppproto#6 case D
#[tokio::test]
async fn survives_malformed_option_list() {
    survives(hdlc(0xc021, &hex("010600060301"))).await;
}

// #45 — embassy-rs/ppproto#6 case E
#[tokio::test]
async fn survives_too_many_options() {
    survives(hdlc(
        0xc021,
        &hex("010700142002210222022302240225022602270200"),
    ))
    .await;
}

// #45 — embassy-rs/ppproto#6 case F
#[tokio::test]
async fn survives_overlong_option() {
    survives(hdlc(0xc021, &hex("0108000c3008010203040506"))).await;
}

// #45 — embassy-rs/ppproto#6 case G
#[tokio::test]
async fn survives_pap_length_off_by_two() {
    survives(hdlc(0xc023, &hex("0201000600"))).await;
}

// #12
#[tokio::test]
async fn lcp_terminate_takes_the_link_down() {
    let (a_io, p_io) = tokio::io::duplex(16384);
    let (ph_tx, mut ph) = watch::channel(Phase::Dead);
    let (a, _task) = bridge(a_io, ph_tx);
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    remote.write(&hdlc(0xc021, &hex("05070004"))).await;
    timeout(Duration::from_secs(5), async {
        while *ph.borrow_and_update() == Phase::Open {
            remote.step().await;
        }
    })
    .await
    .expect("the phase stayed Open after the peer terminated LCP");

    a.0.send(vec![0x45u8, 0, 0, 20, 1, 2, 3, 4]).await.unwrap();
    assert!(
        remote.silent(Duration::from_secs(1)).await,
        "a guest was framed to a peer that terminated the link"
    );
}
