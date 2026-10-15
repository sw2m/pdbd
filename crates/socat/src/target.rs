use crate::Host;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Connect,
    Listen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    Tcp,
    Udp,
    Ws,
    Wss,
    Webtransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inet {
    Connect { host: Host, port: u16 },
    Listen { host: Option<Host>, port: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vsock {
    Connect { cid: u32, port: u32 },
    Listen { port: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(String),
    Unix { mode: Mode, path: String },
    Vsock(Vsock),
    Net { net: Net, inet: Inet },
    Exec(String),
    Stdio,
    Pty,
}

impl Target {
    pub fn mode(&self) -> Option<Mode> {
        todo!()
    }
}
