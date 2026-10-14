use super::*;
use crate::ppp::test::support::Remote;
use crate::Phase;
use std::io;
use tokio::sync::watch;
use tokio::time::{timeout, Duration};

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = bytes
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]) as u32)
        .sum();
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn echo_request(src: Ipv4Addr, dst: Ipv4Addr, id: u16) -> Vec<u8> {
    let mut icmp = vec![8, 0, 0, 0];
    icmp.extend_from_slice(&id.to_be_bytes());
    icmp.extend_from_slice(&1u16.to_be_bytes());
    icmp.extend_from_slice(b"pdbd");
    let c = checksum(&icmp);
    icmp[2..4].copy_from_slice(&c.to_be_bytes());

    let mut ip = vec![0x45, 0];
    ip.extend_from_slice(&((20 + icmp.len()) as u16).to_be_bytes());
    ip.extend_from_slice(&[0, 1, 0x40, 0, 64, 1, 0, 0]);
    ip.extend_from_slice(&src.octets());
    ip.extend_from_slice(&dst.octets());
    let c = checksum(&ip);
    ip[10..12].copy_from_slice(&c.to_be_bytes());
    ip.extend_from_slice(&icmp);
    ip
}

fn is_echo_reply(guest: &[u8], from: Ipv4Addr, id: u16) -> bool {
    guest.len() >= 28
        && guest[0] >> 4 == 4
        && guest[9] == 1
        && guest[12..16] == from.octets()
        && guest[20] == 0
        && guest[24..26] == id.to_be_bytes()
}

// #12
#[tokio::test]
#[ignore = "privileged: creates a kernel TUN; run via the transport CI job"]
async fn tun_carries_a_kernel_icmp_echo_both_ways() {
    let (a_io, p_io) = tokio::io::duplex(4096);
    let (ph_tx, ph) = watch::channel(Phase::Dead);
    let local: Ipv4Addr = "10.9.9.1".parse().unwrap();
    let peer: Ipv4Addr = "10.9.9.2".parse().unwrap();
    let task = tokio::spawn(run(a_io, local, peer, Some("pdbdtest0"), ph_tx));
    let mut remote = Remote::new(p_io);
    remote.open(&ph).await;

    remote.send(&echo_request(peer, local, 0x5044)).await;
    timeout(Duration::from_secs(5), async {
        while !is_echo_reply(&remote.receive().await, local, 0x5044) {}
    })
    .await
    .expect("the kernel's echo reply never came back through the TUN");

    drop(remote);
    let r = timeout(Duration::from_secs(2), task).await;
    assert!(
        matches!(&r, Ok(Ok(Err(e))) if e.kind() == io::ErrorKind::UnexpectedEof),
        "tun::run did not end on peer EOF: {r:?}"
    );
}
