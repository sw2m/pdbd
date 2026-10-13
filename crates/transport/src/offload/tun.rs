use crate::ppp::{Bridge, MRU};
use crate::Phase;
use std::io;
use std::net::Ipv4Addr;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::watch;
use tun_rs::{AsyncDevice, DeviceBuilder};

pub struct Socket {
    device: AsyncDevice,
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
        Ok(Self {
            device: builder.build_async()?,
        })
    }
}

impl crate::Socket for Socket {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut buf = [0u8; MRU];
        let n = self.device.recv(&mut buf).await?;
        Ok((n > 0).then(|| buf[..n].to_vec()))
    }

    async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
        self.device.send(guest).await.map(|_| ())
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
    T: AsyncRead + AsyncWrite + Unpin,
{
    Bridge::new(transport, Socket::open(local, peer, name)?, phase)
        .run()
        .await
}

#[cfg(test)]
mod test;
