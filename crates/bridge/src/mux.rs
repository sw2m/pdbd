use crate::endpoint::Side;
use crate::flow::{self, Flow};
use crate::frame::{Frame, Proto};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Open(Proto, Frame),
    Refused(Frame),
    Flow(flow::Event),
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Cap(u32),
    Exhausted,
    Parity(u32),
    Duplicate(u32),
    Unknown(u32),
    Flow(u32, flow::Error),
}

#[derive(Debug)]
pub struct Mux {
    side: Side,
    window: u32,
    cap: Option<u32>,
    next: Option<u32>,
    flows: BTreeMap<u32, Flow>,
}

impl Mux {
    pub fn new(side: Side, window: u32, cap: Option<u32>) -> Self {
        let _ = (side, window, cap);
        todo!()
    }

    pub fn open(&mut self, proto: Proto) -> Result<(u32, Frame), Error> {
        let _ = proto;
        todo!()
    }

    pub fn receive(&mut self, frame: Frame) -> Result<(u32, Event), Error> {
        let _ = frame;
        todo!()
    }

    pub fn send(
        &mut self,
        id: u32,
        data: &[u8],
    ) -> Result<(Frame, usize), Error> {
        let _ = (id, data);
        todo!()
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<Option<Frame>, Error> {
        let _ = (id, n);
        todo!()
    }

    pub fn close(&mut self, id: u32) -> Result<Frame, Error> {
        let _ = id;
        todo!()
    }

    pub fn reset(&mut self, id: u32, code: u32) -> Result<Frame, Error> {
        let _ = (id, code);
        todo!()
    }

    pub fn flow(&self, id: u32) -> Option<&Flow> {
        self.flows.get(&id)
    }

    pub fn len(&self) -> usize {
        let _ = (self.side, self.window, self.cap, self.next);
        self.flows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.flows.is_empty()
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
