use super::ppp::MRU;
use super::{AbortOnDrop, Phase};
use std::io;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, watch};
use tun_rs::DeviceBuilder;

pub struct Socket {
    offload_rx: mpsc::Receiver<Vec<u8>>,
    offload_tx: mpsc::Sender<Vec<u8>>,
    _rx: AbortOnDrop,
    _tx: AbortOnDrop,
}

impl Socket {
    /// Requires CAP_NET_ADMIN.
    pub fn open(
        local: Ipv4Addr,
        peer: Ipv4Addr,
        name: Option<&str>,
    ) -> io::Result<Self> {
        let mut builder = DeviceBuilder::new()
            .ipv4(local, 32u8, Some(peer))
            .mtu(MRU as u16);
        if let Some(n) = name {
            builder = builder.name(n);
        }
        let dev = Arc::new(builder.build_async()?);

        let (bridge_tx, offload_rx) = mpsc::channel::<Vec<u8>>(64);
        let (offload_tx, mut bridge_rx) = mpsc::channel::<Vec<u8>>(64);

        let dev_rx = dev.clone();
        let rx = AbortOnDrop(tokio::spawn(async move {
            let mut buf = [0u8; MRU];
            loop {
                match dev_rx.recv(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if bridge_tx.send(buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }));
        let dev_tx = dev.clone();
        let tx = AbortOnDrop(tokio::spawn(async move {
            while let Some(guest) = bridge_rx.recv().await {
                if dev_tx.send(&guest).await.is_err() {
                    break;
                }
            }
        }));

        Ok(Self {
            offload_rx,
            offload_tx,
            _rx: rx,
            _tx: tx,
        })
    }
}

impl crate::Socket for Socket {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.offload_rx.recv().await)
    }

    async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
        self.offload_tx.send(guest.to_vec()).await.map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "tun socket closed")
        })
    }
}

pub async fn run<T>(
    transport: T,
    local: Ipv4Addr,
    peer: Ipv4Addr,
    name: Option<&str>,
    phase: watch::Sender<Phase>,
) -> io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    super::splice(
        super::ppp::Socket::connect(transport, phase),
        Socket::open(local, peer, name)?,
    )
    .await
}
