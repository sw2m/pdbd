// see README §"Concurrency & flow control" (the bridge wire contract)

pub mod flow;
pub mod frame;
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
pub enum Event {
    Open(u32, Proto),
    Accepted(u32),
    Data(u32, Vec<u8>),
    Window(u32, u32),
    Closed(u32),
    Reset(u32, u32),
    Stopped(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transmit {
    Bytes(Vec<u8>),
    Shutdown,
    Abort,
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
        cap: Option<u32>,
        budget: u64,
    ) -> Self {
        Self::Multi(Multi::new(side, window, cap, budget))
    }

    pub fn mode(&self) -> Mode {
        match self {
            Self::Single(_) => Mode::Single,
            Self::Multi(_) => Mode::Multi,
        }
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

    pub fn poll(&mut self) -> Option<Event> {
        each!(self, m => m.poll())
    }

    pub fn transmit(&mut self) -> Option<Transmit> {
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
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
