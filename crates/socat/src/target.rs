use crate::{Host, Port, Resource, Text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Connect,
    Listen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Web {
    Ws,
    Wss,
    Webtransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inet {
    Connect { host: Host, port: Port },
    Listen { host: Option<Host>, port: Port },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vsock {
    Connect { cid: u32, port: u32 },
    Listen { port: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(Text),
    Open(Text),
    Gopen(Text),
    Unix {
        mode: Mode,
        path: Text,
    },
    Abstract {
        mode: Mode,
        name: Text,
    },
    Vsock(Vsock),
    Net {
        net: Net,
        inet: Inet,
    },
    Web {
        web: Web,
        inet: Inet,
        resource: Resource,
    },
    Exec(Text),
    Stdio,
    Pty,
}

impl Target {
    pub fn mode(&self) -> Option<Mode> {
        todo!()
    }
}
