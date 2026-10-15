use crate::flow::{self, Flow, ABORTED, REFUSED};
use crate::frame::{Body, Frame, Proto, Reader};
use crate::{Error, Event, Side, Transmit};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug)]
pub struct Multi {
    side: Side,
    window: u32,
    cap: Option<u32>,
    next: Option<u32>,
    peer: u32,
    flows: BTreeMap<u32, Flow>,
    reader: Reader,
    events: VecDeque<Event>,
    out: Vec<u8>,
    dead: bool,
}

impl Multi {
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
            reader: Reader::new(),
            events: VecDeque::new(),
            out: Vec::new(),
            dead: false,
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

    fn emit(&mut self, frame: Frame) {
        frame.encode(&mut self.out);
    }

    fn get(&mut self, id: u32) -> Result<&mut Flow, Error> {
        self.flows.get_mut(&id).ok_or(Error::Unknown(id))
    }

    fn reap(&mut self, id: u32) {
        if self.flows.get(&id).is_some_and(Flow::done) {
            self.flows.remove(&id);
        }
    }

    fn abort(&mut self) {
        self.dead = true;
        let flows = std::mem::take(&mut self.flows);
        self.events
            .extend(flows.into_keys().map(|id| Event::Reset(id, ABORTED)));
    }

    fn route(&mut self, frame: Frame) -> Result<(), Error> {
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
                self.emit(Frame {
                    body: Body::Reset(REFUSED),
                    ..frame
                });
                return Ok(());
            }
            let flow = Flow::offer(&frame, self.window)
                .map_err(|e| Error::Peer(id, e))?;
            self.flows.insert(id, flow);
            self.events.push_back(Event::Open(id, frame.proto));
            return Ok(());
        }
        let Some(flow) = self.flows.get_mut(&id) else {
            return match self.past(id) {
                true => Ok(()),
                false => Err(Error::Stray(id)),
            };
        };
        let event = match flow.receive(frame).map_err(|e| Error::Peer(id, e))? {
            flow::Event::Accepted => Event::Accepted(id),
            flow::Event::Data(d) => Event::Data(id, d),
            flow::Event::Window(n) => Event::Window(id, n),
            flow::Event::Closed => Event::Closed(id),
            flow::Event::Reset(code) => Event::Reset(id, code),
        };
        self.events.push_back(event);
        self.reap(id);
        Ok(())
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        self.reader.feed(bytes);
        loop {
            let frame = match self.reader.pull() {
                Ok(Some(frame)) => frame,
                Ok(None) => return Ok(()),
                Err(e) => {
                    self.abort();
                    return Err(Error::Frame(e));
                }
            };
            if let Err(e) = self.route(frame) {
                self.abort();
                return Err(e);
            }
        }
    }

    pub fn end(&mut self) -> Result<(), Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        self.abort();
        match self.reader.pending() {
            0 => Ok(()),
            _ => Err(Error::Truncated),
        }
    }

    pub fn fail(&mut self) {
        if !self.dead {
            self.abort();
        }
    }

    pub fn poll(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    pub fn transmit(&mut self) -> Option<Transmit> {
        (!self.out.is_empty())
            .then(|| Transmit::Bytes(std::mem::take(&mut self.out)))
    }

    pub fn open(&mut self, proto: Proto) -> Result<u32, Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        if let Some(cap) = self.full() {
            return Err(Error::Cap(cap));
        }
        let id = self.next.ok_or(Error::Exhausted)?;
        self.next = id.checked_add(2);
        let (flow, frame) = Flow::open(id, proto, self.window);
        self.flows.insert(id, flow);
        self.emit(frame);
        Ok(id)
    }

    pub fn accept(&mut self, id: u32) -> Result<(), Error> {
        let frame = self.get(id)?.accept().map_err(|e| Error::Flow(id, e))?;
        self.emit(frame);
        Ok(())
    }

    pub fn send(&mut self, id: u32, data: &[u8]) -> Result<usize, Error> {
        let (frame, n) =
            self.get(id)?.send(data).map_err(|e| Error::Flow(id, e))?;
        self.emit(frame);
        Ok(n)
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<(), Error> {
        let frame = self.get(id)?.consume(n).map_err(|e| Error::Flow(id, e))?;
        if let Some(frame) = frame {
            self.emit(frame);
        }
        Ok(())
    }

    pub fn flush(&mut self, id: u32) -> Result<(), Error> {
        if let Some(frame) = self.get(id)?.flush() {
            self.emit(frame);
        }
        Ok(())
    }

    pub fn close(&mut self, id: u32) -> Result<(), Error> {
        let frame = self.get(id)?.close().map_err(|e| Error::Flow(id, e))?;
        self.emit(frame);
        self.reap(id);
        Ok(())
    }

    pub fn reset(&mut self, id: u32, code: u32) -> Result<(), Error> {
        let frame =
            self.get(id)?.reset(code).map_err(|e| Error::Flow(id, e))?;
        self.emit(frame);
        self.reap(id);
        Ok(())
    }

    pub fn flow(&self, id: u32) -> Option<&Flow> {
        self.flows.get(&id)
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.flows.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.flows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.flows.is_empty()
    }

    pub fn done(&self) -> bool {
        self.dead
    }
}

#[cfg(test)]
mod test;
