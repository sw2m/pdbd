use std::fmt;

pub const HEADER: usize = 10;
pub const MAX: u32 = 65535; // one UDP datagram; also bounds a peer-driven allocation

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Tcp = 6,  // IANA protocol number
    Udp = 17, // IANA protocol number
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Data(Vec<u8>),
    Open(u32),
    Accept(u32),
    Window(u32),
    Close,
    Reset(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub flow: u32,
    pub proto: Proto,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Type(u8),
    Proto(u8),
    Flow,
    Oversize(u32),
    Increment,
    Field(u32),
}

impl Frame {
    pub fn encode(&self, out: &mut Vec<u8>) {
        let _ = out;
        todo!()
    }

    pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, Error> {
        let _ = buf;
        todo!()
    }
}

#[derive(Debug, Default)]
pub struct Reader {
    buf: Vec<u8>,
}

impl Reader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let _ = (bytes, &self.buf);
        todo!()
    }

    pub fn pull(&mut self) -> Result<Option<Frame>, Error> {
        todo!()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
