use clap::{Parser, Subcommand};
use control::{Client, Offer, Server};
use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Endpoint;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    role: Role,
}

#[derive(Subcommand)]
enum Role {
    Serve {
        addr: SocketAddr,
        #[arg(long, default_value = "pdbd")]
        id: String,
        #[arg(long)]
        feature: Vec<String>,
    },
    Dial {
        addr: SocketAddr,
        #[arg(long, default_value = "pdb")]
        id: String,
        #[arg(long)]
        feature: Vec<String>,
    },
}

const PATIENCE: Duration = Duration::from_secs(10); // kernel TCP over a fresh PPP link

#[tokio::main]
async fn main() -> ExitCode {
    match Args::parse().role {
        Role::Serve { addr, id, feature } => {
            let listener = TcpListener::bind(addr)
                .await
                .expect("L4 (TCP): bind the control address");
            Server::new(Offer::new(&id, feature))
                .serve(TcpListenerStream::new(listener))
                .await
                .expect("L7 (control): serve");
            ExitCode::SUCCESS
        }
        Role::Dial { addr, id, feature } => {
            let channel = Endpoint::from_shared(format!("http://{addr}"))
                .expect("L7 (control): endpoint URI")
                .connect_timeout(PATIENCE)
                .timeout(PATIENCE)
                .connect()
                .await;
            let channel = match channel {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("L4 (TCP): dial {addr}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match Client::new(channel).hello(&Offer::new(&id, feature)).await {
                Ok(a) => {
                    println!(
                        "L7 (control) Hello: {} at pdbd.v1 revision {}, features {:?}",
                        a.implementation, a.wire, a.features
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
