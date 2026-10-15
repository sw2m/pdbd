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
    Stop,
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
    Empty,
}

impl Proto {
    fn parse(b: u8) -> Result<Self, Error> {
        match b {
            6 => Ok(Self::Tcp),
            17 => Ok(Self::Udp),
            _ => Err(Error::Proto(b)),
        }
    }
}

impl Body {
    fn kind(&self) -> u8 {
        match self {
            Self::Data(_) => 0,
            Self::Open(_) => 1,
            Self::Accept(_) => 2,
            Self::Window(_) => 3,
            Self::Close => 4,
            Self::Reset(_) => 5,
            Self::Stop => 6,
        }
    }

    fn field(&self) -> u32 {
        match self {
            Self::Data(d) => d.len() as u32,
            Self::Open(n)
            | Self::Accept(n)
            | Self::Window(n)
            | Self::Reset(n) => *n,
            Self::Close | Self::Stop => 0,
        }
    }
}

impl Frame {
    pub fn write(out: &mut Vec<u8>, flow: u32, proto: Proto, payload: &[u8]) {
        let _ = (out, flow, proto, payload);
        todo!()
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.body.kind());
        out.push(self.proto as u8);
        out.extend_from_slice(&self.flow.to_be_bytes());
        out.extend_from_slice(&self.body.field().to_be_bytes());
        if let Body::Data(d) = &self.body {
            out.extend_from_slice(d);
        }
    }

    pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, Error> {
        let Some(head) = buf.get(..HEADER) else {
            return Ok(None);
        };
        let word = |i: usize| {
            u32::from_be_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]])
        };
        let (kind, flow, field) = (head[0], word(2), word(6));
        if kind > 5 {
            return Err(Error::Type(kind));
        }
        let proto = Proto::parse(head[1])?;
        if flow == 0 {
            return Err(Error::Flow);
        }
        let body = match kind {
            0 if field > MAX => return Err(Error::Oversize(field)),
            0 => {
                let Some(d) = buf.get(HEADER..HEADER + field as usize) else {
                    return Ok(None);
                };
                Body::Data(d.to_vec())
            }
            1 => Body::Open(field),
            2 => Body::Accept(field),
            3 if field == 0 => return Err(Error::Increment),
            3 => Body::Window(field),
            4 if field != 0 => return Err(Error::Field(field)),
            4 => Body::Close,
            _ => Body::Reset(field),
        };
        let n = match &body {
            Body::Data(d) => HEADER + d.len(),
            _ => HEADER,
        };
        Ok(Some((Frame { flow, proto, body }, n)))
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
        self.buf.extend_from_slice(bytes);
    }

    pub fn pull(&mut self) -> Result<Option<Frame>, Error> {
        let Some((frame, n)) = Frame::decode(&self.buf)? else {
            return Ok(None);
        };
        self.buf.drain(..n);
        Ok(Some(frame))
    }

    pub fn pending(&self) -> usize {
        self.buf.len()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Type(t) => write!(f, "L4 bridge frame type {t:#04x} unknown"),
            Self::Proto(p) => write!(f, "L4 bridge flow proto {p} unknown"),
            Self::Flow => write!(f, "L4 bridge flow id 0 is reserved"),
            Self::Oversize(n) => {
                write!(f, "L4 bridge frame payload {n} exceeds {MAX}")
            }
            Self::Increment => write!(f, "L4 bridge window increment of 0"),
            Self::Field(v) => {
                write!(
                    f,
                    "L4 bridge CLOSE or STOP frame carries nonzero field {v}"
                )
            }
            Self::Empty => write!(f, "L4 bridge empty DATA on a stream flow"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
