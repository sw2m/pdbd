use super::*;
use tokio::time::{timeout, Duration};

// One poll + one consume, interleaved — ppproto processes one frame per poll.
fn tick(l: &mut Link, inbox: &mut Vec<u8>, outbox: &mut Vec<u8>) {
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

// #12 — two links back-to-back over an in-memory byte pipe reach PPP Open.
#[test]
fn two_links_negotiate_to_open() {
    let mut a = Link::new();
    let mut b = Link::new();
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
        "links did not reach Open: a={:?} b={:?}",
        a.phase(),
        b.phase()
    );
}

// #12 — the async pump brings two links up over a duplex transport and
// shuttles an IP packet from one TUN side to the other.
#[tokio::test]
async fn pump_brings_link_up_and_passes_ip() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (a_from_tx, a_from_rx) = mpsc::channel::<Vec<u8>>(8);
    let (a_to_tx, _a_to_rx) = mpsc::channel::<Vec<u8>>(8);
    let (_b_from_tx, b_from_rx) = mpsc::channel::<Vec<u8>>(8);
    let (b_to_tx, mut b_to_rx) = mpsc::channel::<Vec<u8>>(8);
    let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
    let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);

    tokio::spawn(run(Link::new(), a_io, a_from_rx, a_to_tx, a_ph_tx));
    tokio::spawn(run(Link::new(), b_io, b_from_rx, b_to_tx, b_ph_tx));

    timeout(Duration::from_secs(5), async {
        loop {
            if *a_ph.borrow_and_update() == Phase::Open && *b_ph.borrow_and_update() == Phase::Open
            {
                return;
            }
            tokio::select! {
                _ = a_ph.changed() => {}
                _ = b_ph.changed() => {}
            }
        }
    })
    .await
    .expect("links did not reach Open");

    let pkt = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4, 5, 6, 7, 8];
    a_from_tx.send(pkt.clone()).await.unwrap();
    let got = timeout(Duration::from_secs(2), b_to_rx.recv())
        .await
        .expect("no packet delivered")
        .expect("channel closed");
    assert_eq!(got, pkt);
}
