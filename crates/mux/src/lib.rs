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
}

#[derive(Debug)]
pub enum Mux {
    Single(Single),
    Multi(Multi),
}

impl Mux {
    pub fn single() -> Self {
        todo!()
    }

    pub fn multi(side: Side, window: u32, cap: Option<u32>) -> Self {
        let _ = (side, window, cap);
        todo!()
    }

    pub fn mode(&self) -> Mode {
        todo!()
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let _ = bytes;
        todo!()
    }

    pub fn end(&mut self) -> Result<(), Error> {
        todo!()
    }

    pub fn fail(&mut self) {
        todo!()
    }

    pub fn poll(&mut self) -> Option<Event> {
        todo!()
    }

    pub fn transmit(&mut self) -> Option<Transmit> {
        todo!()
    }

    pub fn open(&mut self, proto: Proto) -> Result<u32, Error> {
        let _ = proto;
        todo!()
    }

    pub fn accept(&mut self, id: u32) -> Result<(), Error> {
        let _ = id;
        todo!()
    }

    pub fn send(&mut self, id: u32, data: &[u8]) -> Result<usize, Error> {
        let _ = (id, data);
        todo!()
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<(), Error> {
        let _ = (id, n);
        todo!()
    }

    pub fn flush(&mut self, id: u32) -> Result<(), Error> {
        let _ = id;
        todo!()
    }

    pub fn close(&mut self, id: u32) -> Result<(), Error> {
        let _ = id;
        todo!()
    }

    pub fn reset(&mut self, id: u32, code: u32) -> Result<(), Error> {
        let _ = (id, code);
        todo!()
    }

    pub fn done(&self) -> bool {
        todo!()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
