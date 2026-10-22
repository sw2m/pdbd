use std::fmt;
use std::ops::Range;

pub const HEADER: usize = 10;
pub const MAX: u32 = 65535; // one UDP datagram; also bounds a peer-driven allocation
pub const WHOLE: usize = HEADER + MAX as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    Tcp = 6,  // IANA protocol number
    Udp = 17, // IANA protocol number
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body<'a> {
    Data(&'a [u8]),
    Open(u32),
    Accept(u32),
    Window(u32),
    Close,
    Reset(u32),
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub flow: u32,
    pub proto: Proto,
    pub body: Body<'a>,
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

impl Body<'_> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    pub kind: u8,
    pub proto: Proto,
    pub flow: u32,
    pub field: u32,
}

impl Head {
    pub fn parse(buf: &[u8]) -> Result<Option<Head>, Error> {
        let Some(head) = buf.get(..HEADER) else {
            return Ok(None);
        };
        let word = |i: usize| {
            u32::from_be_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]])
        };
        let (kind, flow, field) = (head[0], word(2), word(6));
        if kind > 6 {
            return Err(Error::Type(kind));
        }
        let proto = Proto::parse(head[1])?;
        Self::check(kind, proto, flow, field)?;
        Ok(Some(Head {
            kind,
            proto,
            flow,
            field,
        }))
    }

    fn check(
        kind: u8,
        proto: Proto,
        flow: u32,
        field: u32,
    ) -> Result<(), Error> {
        match kind {
            _ if flow == 0 => Err(Error::Flow),
            0 if field > MAX => Err(Error::Oversize(field)),
            0 if field == 0 && proto == Proto::Tcp => Err(Error::Empty),
            3 if field == 0 => Err(Error::Increment),
            4 | 6 if field != 0 => Err(Error::Field(field)),
            _ => Ok(()),
        }
    }

    pub fn size(&self) -> usize {
        match self.kind {
            0 => HEADER + self.field as usize,
            _ => HEADER,
        }
    }

    pub fn data(out: &mut [u8], flow: u32, proto: Proto, len: usize) {
        out[0] = 0;
        out[1] = proto as u8;
        out[2..6].copy_from_slice(&flow.to_be_bytes());
        out[6..HEADER].copy_from_slice(&(len as u32).to_be_bytes());
    }

    pub fn body<'a>(&self, payload: &'a [u8]) -> Body<'a> {
        match self.kind {
            0 => Body::Data(payload),
            1 => Body::Open(self.field),
            2 => Body::Accept(self.field),
            3 => Body::Window(self.field),
            4 => Body::Close,
            5 => Body::Reset(self.field),
            _ => Body::Stop,
        }
    }
}

impl<'a> Frame<'a> {
    pub fn data(
        flow: u32,
        proto: Proto,
        payload: &'a [u8],
    ) -> Result<Self, Error> {
        let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        Head::check(0, proto, flow, len)?;
        Ok(Self {
            flow,
            proto,
            body: Body::Data(payload),
        })
    }

    pub fn size(&self) -> usize {
        match self.body {
            Body::Data(d) => HEADER + d.len(),
            _ => HEADER,
        }
    }

    pub fn put(&self, out: &mut [u8]) {
        out[0] = self.body.kind();
        out[1] = self.proto as u8;
        out[2..6].copy_from_slice(&self.flow.to_be_bytes());
        out[6..HEADER].copy_from_slice(&self.body.field().to_be_bytes());
        if let Body::Data(d) = self.body {
            out[HEADER..HEADER + d.len()].copy_from_slice(d);
        }
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        let at = out.len();
        out.resize(at + self.size(), 0);
        self.put(&mut out[at..]);
    }

    pub fn decode(buf: &'a [u8]) -> Result<Option<(Self, usize)>, Error> {
        let Some(head) = Head::parse(buf)? else {
            return Ok(None);
        };
        let Some(rest) = buf.get(HEADER..head.size()) else {
            return Ok(None);
        };
        let frame = Frame {
            flow: head.flow,
            proto: head.proto,
            body: head.body(rest),
        };
        Ok(Some((frame, head.size())))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Unit {
    #[default]
    Frame,
    Datagram,
    Bare,
}

impl Unit {
    fn header(self) -> usize {
        match self {
            Self::Frame => HEADER,
            Self::Datagram => 2,
            Self::Bare => 0,
        }
    }

    fn whole(self) -> usize {
        match self {
            Self::Frame => WHOLE,
            Self::Datagram => 2 + MAX as usize,
            Self::Bare => 0,
        }
    }

    fn room(self) -> usize {
        match self {
            Self::Bare => 1 << 16, // one socket read
            _ => 4 * self.whole(), // four whole units
        }
    }

    pub fn size(self, buf: &[u8]) -> Result<Option<usize>, Error> {
        match self {
            Self::Frame => Ok(Head::parse(buf)?.map(|h| h.size())),
            Self::Datagram => Ok(buf
                .get(..2)
                .map(|b| 2 + usize::from(u16::from_be_bytes([b[0], b[1]])))),
            Self::Bare => Ok((!buf.is_empty()).then_some(buf.len())),
        }
    }
}

#[derive(Debug, Default)]
pub struct Reader {
    unit: Unit,
    buf: Vec<u8>,
    end: usize,
    at: usize,
}

impl Reader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(unit: Unit) -> Self {
        Self {
            unit,
            ..Self::default()
        }
    }

    pub fn spare(&mut self, keep: usize) -> &mut [u8] {
        if keep == self.end {
            self.end = 0;
            self.at = 0;
        }
        if self.buf.is_empty() {
            self.buf.resize(self.unit.room(), 0);
        }
        let limit = self.limit();
        &mut self.buf[self.end..limit]
    }

    fn limit(&self) -> usize {
        let Some(edge) = self.buf.len().checked_sub(self.unit.whole()) else {
            return self.end;
        };
        if self.end < edge {
            return edge;
        }
        let mut at = self.at;
        while at < self.end {
            match self.unit.size(&self.buf[at..self.end]) {
                Ok(Some(size)) if at + size < self.end => at += size,
                Ok(Some(size)) => return at + size,
                Ok(None) => return at + self.unit.header(),
                Err(_) => break,
            }
        }
        self.end
    }

    pub fn fill(&mut self, n: usize) {
        assert!(n <= self.limit() - self.end, "filled past spare()");
        self.end += n;
    }

    pub fn grow(&mut self) {
        let size = self.buf.len().max(self.unit.room()) * 2;
        self.buf.resize(size, 0);
    }

    pub fn feed(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let keep = self.at;
            let room = self.spare(keep);
            if room.is_empty() {
                self.grow();
                continue;
            }
            let n = room.len().min(bytes.len());
            room[..n].copy_from_slice(&bytes[..n]);
            self.fill(n);
            bytes = &bytes[n..];
        }
    }

    pub fn advance(&mut self) -> Result<Option<Range<usize>>, Error> {
        let rest = self.peek();
        let Some(size) = self.unit.size(rest)? else {
            return Ok(None);
        };
        if rest.len() < size {
            return Ok(None);
        }
        let start = self.at;
        self.at += size;
        Ok(Some(start + self.unit.header()..start + size))
    }

    pub fn step(&mut self) -> Result<Option<(Head, usize)>, Error> {
        let Some(head) = Head::parse(self.peek())? else {
            return Ok(None);
        };
        Ok(self.advance()?.map(|range| (head, range.start)))
    }

    pub fn pull(&mut self) -> Result<Option<Frame<'_>>, Error> {
        let Some((head, payload)) = self.step()? else {
            return Ok(None);
        };
        let data = self.slice(payload..head.size() - HEADER + payload);
        Ok(Some(Frame {
            flow: head.flow,
            proto: head.proto,
            body: head.body(data),
        }))
    }

    pub fn slice(&self, range: Range<usize>) -> &[u8] {
        &self.buf[range]
    }

    pub fn whole(&self) -> bool {
        let rest = self.peek();
        let mut k = 0;
        while k < rest.len() {
            match self.unit.size(&rest[k..]) {
                Ok(Some(size)) => k += size,
                _ => return false,
            }
        }
        k == rest.len()
    }

    pub fn peek(&self) -> &[u8] {
        &self.buf[self.at..self.end]
    }

    pub fn at(&self) -> usize {
        self.at
    }

    pub fn pending(&self) -> usize {
        self.end - self.at
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
