// see README §"Concurrency & flow control" (the bridge wire contract)

pub mod flow;
pub mod frame;
mod lane;
pub mod multi;
pub mod single;

pub use flow::Flow;
pub use frame::{Body, Frame, Proto, Reader};
pub use multi::Multi;
pub use single::Single;

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Client,
    Daemon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Single,
    Multi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<D> {
    Open(u32, Proto),
    Accepted(u32),
    Data(u32, D),
    Window(u32, u32),
    Closed(u32),
    Reset(u32, u32),
    Stopped(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transmit<B> {
    Bytes(B),
    Shutdown,
    Abort,
}

impl<D> Event<D> {
    pub fn map<E>(self, f: impl FnOnce(D) -> E) -> Event<E> {
        match self {
            Self::Open(id, proto) => Event::Open(id, proto),
            Self::Accepted(id) => Event::Accepted(id),
            Self::Data(id, d) => Event::Data(id, f(d)),
            Self::Window(id, n) => Event::Window(id, n),
            Self::Closed(id) => Event::Closed(id),
            Self::Reset(id, code) => Event::Reset(id, code),
            Self::Stopped(id) => Event::Stopped(id),
        }
    }
}

#[cfg(test)]
impl Event<&[u8]> {
    pub fn owned(self) -> Event<Vec<u8>> {
        self.map(<[u8]>::to_vec)
    }
}

#[cfg(test)]
impl Transmit<&[u8]> {
    pub fn owned(self) -> Transmit<Vec<u8>> {
        match self {
            Self::Bytes(b) => Transmit::Bytes(b.to_vec()),
            Self::Shutdown => Transmit::Shutdown,
            Self::Abort => Transmit::Abort,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Mode,
    Cap(u32),
    Exhausted,
    Unknown(u32),
    Flow(u32, flow::Error),
    Peer(u32, flow::Error),
    Frame(frame::Error),
    Parity(u32),
    Duplicate(u32),
    Stray(u32),
    Truncated,
    Dead,
    Window(u32),
    Budget(u64),
}

#[derive(Debug)]
pub enum Mux {
    Single(Single),
    Multi(Multi),
}

macro_rules! each {
    ($self:ident, $m:ident => $e:expr) => {
        match $self {
            Mux::Single($m) => $e,
            Mux::Multi($m) => $e,
        }
    };
}

impl Mux {
    pub fn single() -> Self {
        Self::Single(Single::new())
    }

    pub fn multi(
        side: Side,
        window: u32,
        cap: u32,
        budget: u64,
    ) -> Result<Self, Error> {
        Multi::new(side, window, cap, budget).map(Self::Multi)
    }

    pub fn mode(&self) -> Mode {
        match self {
            Self::Single(_) => Mode::Single,
            Self::Multi(_) => Mode::Multi,
        }
    }

    pub fn spare(&mut self) -> &mut [u8] {
        each!(self, m => m.spare())
    }

    pub fn fill(&mut self, n: usize) -> Result<(), Error> {
        each!(self, m => m.fill(n))
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), Error> {
        each!(self, m => m.feed(bytes))
    }

    pub fn end(&mut self) -> Result<(), Error> {
        each!(self, m => m.end())
    }

    pub fn fail(&mut self) {
        each!(self, m => m.fail())
    }

    pub fn poll(&mut self) -> Option<Event<&[u8]>> {
        each!(self, m => m.poll())
    }

    pub fn transmit(&mut self) -> Option<Transmit<&[u8]>> {
        each!(self, m => m.transmit())
    }

    pub fn open(&mut self, proto: Proto) -> Result<u32, Error> {
        each!(self, m => m.open(proto))
    }

    pub fn accept(&mut self, id: u32) -> Result<(), Error> {
        each!(self, m => m.accept(id))
    }

    pub fn send(&mut self, id: u32, data: &[u8]) -> Result<usize, Error> {
        each!(self, m => m.send(id, data))
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<(), Error> {
        each!(self, m => m.consume(id, n))
    }

    pub fn flush(&mut self, id: u32) -> Result<(), Error> {
        each!(self, m => m.flush(id))
    }

    pub fn close(&mut self, id: u32) -> Result<(), Error> {
        each!(self, m => m.close(id))
    }

    pub fn reset(&mut self, id: u32, code: u32) -> Result<(), Error> {
        each!(self, m => m.reset(id, code))
    }

    pub fn stop(&mut self, id: u32) -> Result<(), Error> {
        each!(self, m => m.stop(id))
    }

    pub fn grow(&mut self, id: u32, window: u32) -> Result<(), Error> {
        each!(self, m => m.grow(id, window))
    }

    pub fn tick(&mut self, now: u64) {
        each!(self, m => m.tick(now))
    }

    pub fn expire(&mut self, now: u64, deadline: u64) {
        each!(self, m => m.expire(now, deadline))
    }

    pub fn paused(&self) -> bool {
        each!(self, m => m.paused())
    }

    pub fn done(&self) -> bool {
        each!(self, m => m.done())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mode => write!(f, "L4 bridge is single-channel"),
            Self::Cap(c) => write!(f, "L4 bridge is at its cap of {c} flows"),
            Self::Exhausted => write!(f, "L4 bridge flow ids exhausted"),
            Self::Unknown(id) => write!(f, "L4 bridge has no flow {id}"),
            Self::Flow(id, e) => write!(f, "L4 bridge flow {id}: {e}"),
            Self::Peer(id, e) => {
                write!(f, "L4 bridge flow {id}: peer violation: {e}")
            }
            Self::Frame(e) => write!(f, "{e}"),
            Self::Parity(id) => {
                write!(f, "L4 bridge peer opened flow {id} with our parity")
            }
            Self::Duplicate(id) => {
                write!(f, "L4 bridge peer reopened or reordered flow {id}")
            }
            Self::Stray(id) => {
                write!(f, "L4 bridge peer sent a frame for unopened flow {id}")
            }
            Self::Truncated => write!(f, "L4 bridge tunnel ended mid-frame"),
            Self::Dead => write!(f, "L4 bridge tunnel has ended"),
            Self::Window(w) => write!(
                f,
                "L4 bridge datagram flow needs a window of at least 65535, not {w}"
            ),
            Self::Budget(b) => write!(
                f,
                "L4 bridge receive budget {b} is below one frame (65535)"
            ),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
