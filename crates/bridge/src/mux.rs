use crate::endpoint::Side;
use crate::flow::{self, Flow};
use crate::frame::{Body, Frame, Proto};
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
    peer: u32,
    flows: BTreeMap<u32, Flow>,
}

impl Mux {
    pub fn new(side: Side, window: u32, cap: Option<u32>) -> Self {
        let first = match side {
            Side::Client => 1,
            Side::Daemon => 2,
        };
        Self {
            side,
            window,
            cap,
            next: Some(first),
            peer: 0,
            flows: BTreeMap::new(),
        }
    }

    fn mine(&self, id: u32) -> bool {
        (id % 2 == 1) == (self.side == Side::Client)
    }

    fn past(&self, id: u32) -> bool {
        if self.mine(id) {
            self.next.is_none_or(|n| id < n)
        } else {
            id <= self.peer
        }
    }

    fn full(&self) -> Option<u32> {
        self.cap.filter(|c| self.flows.len() as u32 >= *c)
    }

    fn get(&mut self, id: u32) -> Result<&mut Flow, Error> {
        self.flows.get_mut(&id).ok_or(Error::Unknown(id))
    }

    fn reap(&mut self, id: u32) {
        if self.flows.get(&id).is_some_and(Flow::done) {
            self.flows.remove(&id);
        }
    }

    pub fn open(&mut self, proto: Proto) -> Result<(u32, Frame), Error> {
        if let Some(cap) = self.full() {
            return Err(Error::Cap(cap));
        }
        let id = self.next.ok_or(Error::Exhausted)?;
        self.next = id.checked_add(2);
        let (flow, frame) = Flow::open(id, proto, self.window);
        self.flows.insert(id, flow);
        Ok((id, frame))
    }

    pub fn receive(&mut self, frame: Frame) -> Result<(u32, Event), Error> {
        let id = frame.flow;
        if let Body::Open(_) = frame.body {
            if self.mine(id) {
                return Err(Error::Parity(id));
            }
            if id <= self.peer {
                return Err(Error::Duplicate(id));
            }
            self.peer = id;
            if self.full().is_some() {
                let refuse = Frame {
                    body: Body::Reset(0),
                    ..frame
                };
                return Ok((id, Event::Refused(refuse)));
            }
            let (flow, accept) = Flow::accept(&frame, self.window)
                .map_err(|e| Error::Flow(id, e))?;
            self.flows.insert(id, flow);
            return Ok((id, Event::Open(frame.proto, accept)));
        }
        let Some(flow) = self.flows.get_mut(&id) else {
            return match self.past(id) {
                true => Ok((id, Event::Stale)),
                false => Err(Error::Unknown(id)),
            };
        };
        let event = flow.receive(frame).map_err(|e| Error::Flow(id, e))?;
        self.reap(id);
        Ok((id, Event::Flow(event)))
    }

    pub fn send(
        &mut self,
        id: u32,
        data: &[u8],
    ) -> Result<(Frame, usize), Error> {
        self.get(id)?.send(data).map_err(|e| Error::Flow(id, e))
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<Option<Frame>, Error> {
        self.get(id)?.consume(n).map_err(|e| Error::Flow(id, e))
    }

    pub fn close(&mut self, id: u32) -> Result<Frame, Error> {
        let frame = self.get(id)?.close().map_err(|e| Error::Flow(id, e))?;
        self.reap(id);
        Ok(frame)
    }

    pub fn reset(&mut self, id: u32, code: u32) -> Result<Frame, Error> {
        let frame = self.get(id)?.reset(code);
        self.reap(id);
        Ok(frame)
    }

    pub fn flow(&self, id: u32) -> Option<&Flow> {
        self.flows.get(&id)
    }

    pub fn len(&self) -> usize {
        self.flows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.flows.is_empty()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cap(c) => write!(f, "bridge is at its cap of {c} flows"),
            Self::Exhausted => write!(f, "bridge flow ids exhausted"),
            Self::Parity(id) => {
                write!(f, "peer opened flow {id} with our parity")
            }
            Self::Duplicate(id) => write!(f, "peer reopened flow {id}"),
            Self::Unknown(id) => write!(f, "no such flow {id}"),
            Self::Flow(id, e) => write!(f, "flow {id}: {e}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
