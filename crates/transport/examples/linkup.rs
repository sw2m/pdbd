use std::net::Ipv4Addr;

use clap::Parser;
use tokio::sync::watch;
use tokio_serial::SerialPortBuilderExt;
use transport::{tun, Phase};

#[derive(Parser)]
struct Args {
    dev: String,
    local: Ipv4Addr,
    peer: Ipv4Addr,
    name: Option<String>,
    #[arg(short, long, env = "PDBD_BAUD", default_value_t = 115200)]
    baud: u32,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok(); // .env -> process env, before clap reads #[arg(env)]
    let args = Args::parse();

    let transport = tokio_serial::new(&args.dev, args.baud)
        .open_native_async()
        .expect("open serial device");

    let (ph_tx, mut ph) = watch::channel(Phase::Dead);
    tokio::spawn(async move {
        while ph.changed().await.is_ok() {
            eprintln!("phase: {:?}", *ph.borrow());
        }
    });

    tun::run(
        transport,
        args.local,
        args.peer,
        args.name.as_deref(),
        ph_tx,
    )
    .await
    .expect("tun::run");
}
