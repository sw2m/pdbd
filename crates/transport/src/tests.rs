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

// #12 — the async pump brings two links up and passes an IP packet across.
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
            if *a_ph.borrow_and_update() == Phase::Open
                && *b_ph.borrow_and_update() == Phase::Open
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

// C1 — an over-FRAME packet must err, not panic (HDLC escaping can double it).
#[test]
fn send_oversized_errs_not_panics() {
    let mut l = Link::new();
    let big = vec![0x7eu8; FRAME];
    assert!(l.send(&big).is_err());
}

// C1 — a full-MRU packet frames cleanly (the buffer is sized for it).
#[test]
fn send_full_mru_ok() {
    let mut l = Link::new();
    let pkt = vec![0x41u8; MRU];
    assert!(l.send(&pkt).is_ok());
}

// C2 — packets before Open are dropped; only a post-Open packet crosses.
#[tokio::test]
async fn packets_before_open_are_dropped() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (a_from_tx, a_from_rx) = mpsc::channel::<Vec<u8>>(8);
    let (a_to_tx, _a_to_rx) = mpsc::channel::<Vec<u8>>(8);
    let (_b_from_tx, b_from_rx) = mpsc::channel::<Vec<u8>>(8);
    let (b_to_tx, mut b_to_rx) = mpsc::channel::<Vec<u8>>(8);
    let (a_ph_tx, mut a_ph) = watch::channel(Phase::Dead);
    let (b_ph_tx, mut b_ph) = watch::channel(Phase::Dead);

    // Injected before either link is up — must be dropped by the Open gate.
    let early = vec![0x45u8, 0, 0, 20, 9, 9, 9, 9, 0, 0, 0, 0];
    a_from_tx.send(early.clone()).await.unwrap();

    tokio::spawn(run(Link::new(), a_io, a_from_rx, a_to_tx, a_ph_tx));
    tokio::spawn(run(Link::new(), b_io, b_from_rx, b_to_tx, b_ph_tx));

    timeout(Duration::from_secs(5), async {
        loop {
            if *a_ph.borrow_and_update() == Phase::Open
                && *b_ph.borrow_and_update() == Phase::Open
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

    let real = vec![0x45u8, 0, 0, 20, 1, 2, 3, 4, 5, 6, 7, 8];
    a_from_tx.send(real.clone()).await.unwrap();

    // The first (and only) packet delivered is the post-Open one.
    let got = timeout(Duration::from_secs(2), b_to_rx.recv())
        .await
        .expect("no packet delivered")
        .expect("channel closed");
    assert_eq!(got, real);
}

// H1 — a dead transport must end the pump, not hang (drives tun::run teardown).
#[tokio::test]
async fn run_terminates_on_dead_transport() {
    let (a_io, b_io) = tokio::io::duplex(64);
    drop(b_io); // closing one half → writes break / reads hit EOF
    let (_from_tx, from_rx) = mpsc::channel::<Vec<u8>>(1);
    let (to_tx, _to_rx) = mpsc::channel::<Vec<u8>>(1);
    let (ph_tx, _ph) = watch::channel(Phase::Dead);

    let r = timeout(
        Duration::from_secs(2),
        run(Link::new(), a_io, from_rx, to_tx, ph_tx),
    )
    .await;
    assert!(r.is_ok(), "pump hung on a dead transport");
}

// H2 — the phase watch fires on change, not every tick, then quiesces.
#[tokio::test]
async fn phase_watch_quiesces_when_stable() {
    let (a_io, _b_io) = tokio::io::duplex(4096); // _b_io kept open → no EOF
    let (_from_tx, from_rx) = mpsc::channel::<Vec<u8>>(1);
    let (to_tx, _to_rx) = mpsc::channel::<Vec<u8>>(1);
    let (ph_tx, mut ph) = watch::channel(Phase::Dead);
    tokio::spawn(run(Link::new(), a_io, from_rx, to_tx, ph_tx));

    // Let the initial Dead→Establish transition settle, then mark it seen.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = ph.borrow_and_update();

    // Stable phase → no notification (send_if_modified, not send_replace per tick).
    let fired = timeout(Duration::from_millis(300), ph.changed()).await;
    assert!(
        fired.is_err(),
        "phase watch kept firing while the phase was stable"
    );
}

// Garbage on the wire must not panic or spuriously open (bad frames fail FCS).
#[test]
fn garbage_bytes_do_not_panic_or_open() {
    let mut l = Link::new();
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

// H1 against a real kernel TUN — device create + bridge-task teardown on EOF.
#[tokio::test]
#[ignore = "privileged: creates a kernel TUN; run via the transport CI job"]
async fn tun_run_creates_device_and_tears_down() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (ph_tx, _ph) = watch::channel(Phase::Dead);
    let local = "10.9.9.1".parse().unwrap();
    let peer = "10.9.9.2".parse().unwrap();
    let h = tokio::spawn(tun::run(a_io, local, peer, Some("pdbdtest0"), ph_tx));

    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(b_io); // dead transport → pump returns → bridge tasks aborted
    let r = timeout(Duration::from_secs(2), h).await;
    assert!(
        matches!(r, Ok(Ok(_))),
        "tun::run did not tear down cleanly: {r:?}"
    );
}
