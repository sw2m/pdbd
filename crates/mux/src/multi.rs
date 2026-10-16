use crate::flow::{self, Flow, State, ABORTED, REFUSED};
use crate::frame::{Body, Frame, Head, Proto, Reader, HEADER, MAX};
use crate::{Error, Event, Side, Transmit};
use std::collections::{BTreeMap, VecDeque};

const QUANTUM: usize = HEADER + MAX as usize; // one full DATA frame per flow per transmit

#[derive(Debug)]
pub struct Multi {
    side: Side,
    window: u32,
    cap: u32,
    budget: u64,
    used: u64,
    now: u64,
    paused: bool,
    next: Option<u32>,
    peer: u32,
    flows: BTreeMap<u32, Flow>,
    reader: Reader,
    events: VecDeque<Event>,
    control: Vec<u8>,
    lanes: BTreeMap<u32, Vec<u8>>,
    ring: VecDeque<u32>,
    dead: bool,
}

impl Multi {
    pub fn new(
        side: Side,
        window: u32,
        cap: u32,
        budget: u64,
    ) -> Result<Self, Error> {
        if budget < u64::from(MAX) {
            return Err(Error::Budget(budget));
        }
        let first = match side {
            Side::Client => 1,
            Side::Daemon => 2,
        };
        Ok(Self {
            side,
            window,
            cap,
            budget,
            used: 0,
            now: 0,
            paused: false,
            next: Some(first),
            peer: 0,
            flows: BTreeMap::new(),
            reader: Reader::new(),
            events: VecDeque::new(),
            control: Vec::new(),
            lanes: BTreeMap::new(),
            ring: VecDeque::new(),
            dead: false,
        })
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
        (self.flows.len() as u32 >= self.cap).then_some(self.cap)
    }

    fn get(&mut self, id: u32) -> Result<&mut Flow, Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        self.flows.get_mut(&id).ok_or(Error::Unknown(id))
    }

    fn signal(&mut self, frame: Frame) {
        frame.encode(&mut self.control);
    }

    fn lane(&mut self, id: u32) -> &mut Vec<u8> {
        if !self.lanes.contains_key(&id) {
            self.ring.push_back(id);
        }
        self.lanes.entry(id).or_default()
    }

    fn discard(&mut self, id: u32) {
        if self.lanes.remove(&id).is_some() {
            self.ring.retain(|&x| x != id);
        }
    }

    fn reap(&mut self, id: u32) {
        if let Some(flow) = self.flows.get(&id).filter(|f| f.done()) {
            self.used -= u64::from(flow.held());
            self.flows.remove(&id);
        }
    }

    fn mark(&mut self, id: u32) {
        let now = self.now;
        if let Some(flow) = self.flows.get_mut(&id) {
            flow.mark(now);
        }
    }

    fn abort(&mut self) {
        self.dead = true;
        self.used = 0;
        self.control.clear();
        self.lanes.clear();
        self.ring.clear();
        let flows = std::mem::take(&mut self.flows);
        self.events
            .extend(flows.into_keys().map(|id| Event::Reset(id, ABORTED)));
    }

    fn blocked(&self) -> bool {
        let Ok(Some(head)) = Head::parse(self.reader.peek()) else {
            return false;
        };
        let held = head.kind == 0
            && self.flows.get(&head.flow).is_some_and(|f| {
                f.state() == State::Open
                    && f.proto() == head.proto
                    && !f.stopped()
                    && !f.received()
                    && head.field <= f.granted()
            });
        held && self.used + u64::from(head.field) > self.budget
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
                self.signal(Frame {
                    body: Body::Reset(REFUSED),
                    ..frame
                });
                return Ok(());
            }
            let mut flow = Flow::offer(&frame, self.window)
                .map_err(|e| Error::Peer(id, e))?;
            flow.mark(self.now);
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
            flow::Event::Accepted => {
                flow.mark(self.now);
                if let Some(grant) = flow.flush() {
                    grant.encode(&mut self.control);
                }
                Event::Accepted(id)
            }
            flow::Event::Data(d) => {
                self.used += d.len() as u64;
                Event::Data(id, d)
            }
            flow::Event::Dropped(_) => return Ok(()),
            flow::Event::Window(n) => Event::Window(id, n),
            flow::Event::Closed => {
                flow.mark(self.now);
                Event::Closed(id)
            }
            flow::Event::Reset(code) => {
                self.discard(id);
                Event::Reset(id, code)
            }
            flow::Event::Stopped => {
                if let Ok(close) = flow.close() {
                    flow.mark(self.now);
                    self.discard(id);
                    close.encode(self.lane(id));
                }
                Event::Stopped(id)
            }
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
            self.paused = self.blocked();
            if self.paused {
                return Ok(());
            }
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
        match self.reader.whole() {
            true => Ok(()),
            false => Err(Error::Truncated),
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
        let mut out = std::mem::take(&mut self.control);
        for _ in 0..self.ring.len() {
            let Some(id) = self.ring.pop_front() else {
                break;
            };
            let Some(lane) = self.lanes.get_mut(&id) else {
                continue;
            };
            let mut k = 0;
            while let Ok(Some(head)) = Head::parse(&lane[k..]) {
                let size = head.size();
                if k > 0 && k + size > QUANTUM {
                    break;
                }
                k += size;
            }
            out.extend(lane.drain(..k));
            match lane.is_empty() {
                true => _ = self.lanes.remove(&id),
                false => self.ring.push_back(id),
            }
        }
        (!out.is_empty()).then_some(Transmit::Bytes(out))
    }

    pub fn open(&mut self, proto: Proto) -> Result<u32, Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        if let Some(cap) = self.full() {
            return Err(Error::Cap(cap));
        }
        let id = self.next.ok_or(Error::Exhausted)?;
        let (mut flow, frame) =
            Flow::open(id, proto, self.window).map_err(|e| match e {
                flow::Error::Window(w) => Error::Window(w),
                e => Error::Flow(id, e),
            })?;
        self.next = id.checked_add(2);
        flow.mark(self.now);
        self.flows.insert(id, flow);
        self.signal(frame);
        Ok(id)
    }

    pub fn accept(&mut self, id: u32) -> Result<(), Error> {
        let frame = self.get(id)?.accept().map_err(|e| Error::Flow(id, e))?;
        self.mark(id);
        self.signal(frame);
        Ok(())
    }

    pub fn send(&mut self, id: u32, data: &[u8]) -> Result<usize, Error> {
        let flow = self.get(id)?;
        let n = flow.send(data).map_err(|e| Error::Flow(id, e))?;
        let proto = flow.proto();
        Frame::write(self.lane(id), id, proto, &data[..n])
            .map_err(Error::Frame)?;
        Ok(n)
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<(), Error> {
        let frame = self.get(id)?.consume(n).map_err(|e| Error::Flow(id, e))?;
        self.used -= u64::from(n);
        if let Some(frame) = frame {
            self.signal(frame);
        }
        Ok(())
    }

    pub fn flush(&mut self, id: u32) -> Result<(), Error> {
        if let Some(frame) = self.get(id)?.flush() {
            self.signal(frame);
        }
        Ok(())
    }

    pub fn close(&mut self, id: u32) -> Result<(), Error> {
        let frame = self.get(id)?.close().map_err(|e| Error::Flow(id, e))?;
        self.mark(id);
        frame.encode(self.lane(id));
        self.reap(id);
        Ok(())
    }

    pub fn reset(&mut self, id: u32, code: u32) -> Result<(), Error> {
        let frame =
            self.get(id)?.reset(code).map_err(|e| Error::Flow(id, e))?;
        self.discard(id);
        self.signal(frame);
        self.reap(id);
        Ok(())
    }

    pub fn stop(&mut self, id: u32) -> Result<(), Error> {
        let flow = self.get(id)?;
        let held = flow.held();
        let frame = flow.stop().map_err(|e| Error::Flow(id, e))?;
        self.used -= u64::from(held);
        self.mark(id);
        self.signal(frame);
        Ok(())
    }

    pub fn grow(&mut self, id: u32, window: u32) -> Result<(), Error> {
        self.get(id)?.grow(window).map_err(|e| Error::Flow(id, e))
    }

    pub fn tick(&mut self, now: u64) {
        self.now = now;
    }

    pub fn expire(&mut self, now: u64, deadline: u64) {
        self.tick(now);
        let overdue: Vec<u32> = self
            .flows
            .iter()
            .filter(|(_, f)| f.overdue(now, deadline))
            .map(|(&id, _)| id)
            .collect();
        for id in overdue {
            if self.reset(id, ABORTED).is_ok() {
                self.events.push_back(Event::Reset(id, ABORTED));
            }
        }
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn used(&self) -> u64 {
        self.used
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
