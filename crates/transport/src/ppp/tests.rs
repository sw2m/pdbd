use super::*;
use crate::Phase;
use crate::Socket as _;
use tokio::sync::watch;
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

async fn await_open(ph: &mut watch::Receiver<Phase>) {
    while *ph.borrow_and_update() != Phase::Open {
        ph.changed()
            .await
            .expect("phase sender dropped before Open");
    }
}

// #12
#[tokio::test]
async fn pump_brings_link_up_and_passes_ip() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
    let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);
    let mut a = Socket::connect(a_io, a_ph_tx);
    let mut b = Socket::connect(b_io, b_ph_tx);

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
        .expect("ppp socket errored");
    assert_eq!(got, Some(pkt));
}

// #12
#[tokio::test]
async fn packets_before_open_are_dropped() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
    let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);
    let mut a = Socket::connect(a_io, a_ph_tx);
    let mut b = Socket::connect(b_io, b_ph_tx);

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
        .expect("ppp socket errored");
    assert_eq!(got, Some(real));
}

// #12
#[tokio::test]
async fn ppp_socket_closes_on_dead_transport() {
    let (a_io, b_io) = tokio::io::duplex(64);
    drop(b_io);
    let (ph_tx, _ph) = watch::channel(Phase::Dead);
    let mut a = Socket::connect(a_io, ph_tx);

    let r = timeout(Duration::from_secs(2), a.receive()).await;
    assert!(
        matches!(r, Ok(Ok(None))),
        "ppp::Socket did not close on a dead transport: {r:?}"
    );
}

// #12
#[tokio::test]
async fn phase_watch_quiesces_when_stable() {
    let (a_io, _b_io) = tokio::io::duplex(4096); // _b_io kept open → no EOF
    let (ph_tx, mut ph) = watch::channel(Phase::Dead);
    let _a = Socket::connect(a_io, ph_tx); // keep the socket (driver) alive

    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = ph.borrow_and_update();

    let fired = timeout(Duration::from_millis(300), ph.changed()).await;
    assert!(
        fired.is_err(),
        "phase watch kept firing while the phase was stable"
    );
}
