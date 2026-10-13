use super::*;
use tokio::sync::{mpsc, watch};
use tokio::time::{timeout, Duration};

fn tick(l: &mut ppp::Core, inbox: &mut Vec<u8>, outbox: &mut Vec<u8>) {
    if let ppp::Action::Tx(bytes) = l.poll() {
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
    let mut a = ppp::Core::new();
    let mut b = ppp::Core::new();
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
    let mut l = ppp::Core::new();
    let big = vec![0x7eu8; ppp::FRAME];
    assert!(l.send(&big).is_err());
}

// #12
#[test]
fn send_full_mru_ok() {
    let mut l = ppp::Core::new();
    let pkt = vec![0x41u8; ppp::MRU];
    assert!(l.send(&pkt).is_ok());
}

// #12
#[test]
fn garbage_bytes_do_not_panic_or_open() {
    let mut l = ppp::Core::new();
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

struct ChanSocket {
    rx: mpsc::Receiver<Vec<u8>>,
    tx: mpsc::Sender<Vec<u8>>,
}

impl Socket for ChanSocket {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.rx.recv().await)
    }
    async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
        self.tx
            .send(guest.to_vec())
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
    }
}

// #12
#[tokio::test]
async fn splice_copies_both_ways() {
    let (l_in_tx, l_in_rx) = mpsc::channel::<Vec<u8>>(8);
    let (l_out_tx, mut l_out_rx) = mpsc::channel::<Vec<u8>>(8);
    let (r_in_tx, r_in_rx) = mpsc::channel::<Vec<u8>>(8);
    let (r_out_tx, mut r_out_rx) = mpsc::channel::<Vec<u8>>(8);
    let left = ChanSocket {
        rx: l_in_rx,
        tx: l_out_tx,
    };
    let right = ChanSocket {
        rx: r_in_rx,
        tx: r_out_tx,
    };
    tokio::spawn(splice(left, right));

    l_in_tx.send(vec![1, 2, 3]).await.unwrap();
    let got = timeout(Duration::from_secs(1), r_out_rx.recv())
        .await
        .unwrap();
    assert_eq!(got, Some(vec![1, 2, 3]));

    r_in_tx.send(vec![4, 5, 6]).await.unwrap();
    let got = timeout(Duration::from_secs(1), l_out_rx.recv())
        .await
        .unwrap();
    assert_eq!(got, Some(vec![4, 5, 6]));
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
    let mut a = ppp::Socket::connect(a_io, a_ph_tx);
    let mut b = ppp::Socket::connect(b_io, b_ph_tx);

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
    let mut a = ppp::Socket::connect(a_io, a_ph_tx);
    let mut b = ppp::Socket::connect(b_io, b_ph_tx);

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
    let mut a = ppp::Socket::connect(a_io, ph_tx);

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
    let _a = ppp::Socket::connect(a_io, ph_tx); // keep the socket (driver) alive

    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = ph.borrow_and_update();

    let fired = timeout(Duration::from_millis(300), ph.changed()).await;
    assert!(
        fired.is_err(),
        "phase watch kept firing while the phase was stable"
    );
}

// #12
#[tokio::test]
#[ignore = "privileged: creates a kernel TUN; run via the transport CI job"]
async fn tun_run_creates_device_and_tears_down() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (ph_tx, _ph) = watch::channel(Phase::Dead);
    let local = "10.9.9.1".parse().unwrap();
    let peer = "10.9.9.2".parse().unwrap();
    let h = tokio::spawn(tun::run(a_io, local, peer, Some("pdbdtest0"), ph_tx));

    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(b_io);
    let r = timeout(Duration::from_secs(2), h).await;
    assert!(
        matches!(r, Ok(Ok(_))),
        "tun::run did not tear down cleanly: {r:?}"
    );
}
