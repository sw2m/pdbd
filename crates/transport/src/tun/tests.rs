use super::*;
use crate::Phase;
use tokio::sync::watch;
use tokio::time::{timeout, Duration};

// #12
#[tokio::test]
#[ignore = "privileged: creates a kernel TUN; run via the transport CI job"]
async fn tun_run_creates_device_and_tears_down() {
    let (a_io, b_io) = tokio::io::duplex(4096);
    let (ph_tx, _ph) = watch::channel(Phase::Dead);
    let local = "10.9.9.1".parse().unwrap();
    let peer = "10.9.9.2".parse().unwrap();
    let h = tokio::spawn(run(a_io, local, peer, Some("pdbdtest0"), ph_tx));

    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(b_io);
    let r = timeout(Duration::from_secs(2), h).await;
    assert!(
        matches!(r, Ok(Ok(_))),
        "tun::run did not tear down cleanly: {r:?}"
    );
}
