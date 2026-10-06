//! Manual harness binary: bring up the pdbd link over a serial device + TUN.
//! Usage: linkup <device> <local-ip> <peer-ip> [tun-name]

use std::net::Ipv4Addr;

use link::{run_tun, Phase};
use tokio::sync::watch;
use tokio_serial::SerialPortBuilderExt;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dev = args[1].clone();
    let local: Ipv4Addr = args[2].parse().expect("local ip");
    let peer: Ipv4Addr = args[3].parse().expect("peer ip");
    let name = args.get(4).cloned();

    let transport = tokio_serial::new(&dev, 115200)
        .open_native_async()
        .expect("open serial device");

    let (ph_tx, mut ph) = watch::channel(Phase::Dead);
    tokio::spawn(async move {
        while ph.changed().await.is_ok() {
            eprintln!("phase: {:?}", *ph.borrow());
        }
    });

    run_tun(transport, local, peer, name.as_deref(), ph_tx)
        .await
        .expect("run_tun");
}
