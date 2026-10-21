use crate::flow::{self, Flow, State, ABORTED, REFUSED};
use crate::frame::{Body, Frame, Head, Proto, Reader, HEADER, MAX, WHOLE};
use crate::lane::Lane;
use crate::{Error, Event, Side, Transmit};
use std::collections::{BTreeMap, VecDeque};
use std::ops::Range;

const QUANTUM: usize = WHOLE; // one full DATA frame per flow per transmit

#[derive(Debug, Default)]
struct Control {
    buf: Vec<u8>,
    sent: usize,
}

impl Control {
    fn push(&mut self, frame: &Frame) {
        if self.sent == self.buf.len() {
            self.clear();
        }
        frame.encode(&mut self.buf);
    }

    fn ready(&self) -> bool {
        self.sent < self.buf.len()
    }

    fn lend(&mut self) -> &[u8] {
        let at = std::mem::replace(&mut self.sent, self.buf.len());
        &self.buf[at..]
    }

    fn clear(&mut self) {
        self.buf.clear();
        self.sent = 0;
    }
}

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
    events: VecDeque<Event<Range<usize>>>,
    control: Control,
    lanes: BTreeMap<u32, Lane>,
    ring: VecDeque<u32>,
    lent: Option<u32>,
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
            control: Control::default(),
            lanes: BTreeMap::new(),
            ring: VecDeque::new(),
            lent: None,
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
        self.control.push(&frame);
    }

    fn queue(&mut self, id: u32, frame: &Frame) {
        let lane = self.lanes.entry(id).or_default();
        if lane.is_empty() {
            self.ring.push_back(id);
        }
        frame.put(lane.reserve(frame.size()));
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
            self.prune(id);
        }
    }

    fn prune(&mut self, id: u32) {
        let idle = self.lanes.get(&id).is_some_and(Lane::is_empty);
        if idle && !self.flows.contains_key(&id) {
            self.lanes.remove(&id);
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
        self.lent = None;
        let flows = std::mem::take(&mut self.flows);
        self.events
            .extend(flows.into_keys().map(|id| Event::Reset(id, ABORTED)));
    }

    fn keep(&self) -> usize {
        self.events
            .iter()
            .find_map(|e| match e {
                Event::Data(_, range) => Some(range.start),
                _ => None,
            })
            .unwrap_or(self.reader.at())
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

    fn route(&mut self, head: Head, at: usize) -> Result<(), Error> {
        let id = head.flow;
        let payload = at..at + head.size() - HEADER;
        if let Body::Open(window) = head.body(&[]) {
            if self.mine(id) {
                return Err(Error::Parity(id));
            }
            if id <= self.peer {
                return Err(Error::Duplicate(id));
            }
            self.peer = id;
            let frame = Frame {
                flow: id,
                proto: head.proto,
                body: Body::Open(window),
            };
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
            self.events.push_back(Event::Open(id, head.proto));
            return Ok(());
        }
        let Some(flow) = self.flows.get_mut(&id) else {
            return match self.past(id) {
                true => Ok(()),
                false => Err(Error::Stray(id)),
            };
        };
        let frame = Frame {
            flow: id,
            proto: head.proto,
            body: head.body(self.reader.slice(payload.clone())),
        };
        let event = match flow.receive(frame).map_err(|e| Error::Peer(id, e))? {
            flow::Event::Accepted => {
                flow.mark(self.now);
                if let Some(grant) = flow.flush() {
                    self.control.push(&grant);
                }
                Event::Accepted(id)
            }
            flow::Event::Data(n) => {
                self.used += u64::from(n);
                Event::Data(id, payload)
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
                    self.queue(id, &close);
                }
                Event::Stopped(id)
            }
        };
        self.events.push_back(event);
        self.reap(id);
        Ok(())
    }

    pub fn spare(&mut self) -> &mut [u8] {
        if self.dead {
            return &mut [];
        }
        let keep = self.keep();
        self.reader.spare(keep)
    }

    pub fn fill(&mut self, n: usize) -> Result<(), Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        self.reader.fill(n);
        loop {
            self.paused = self.blocked();
            if self.paused {
                return Ok(());
            }
            let (head, at) = match self.reader.step() {
                Ok(Some(next)) => next,
                Ok(None) => return Ok(()),
                Err(e) => {
                    self.abort();
                    return Err(Error::Frame(e));
                }
            };
            if let Err(e) = self.route(head, at) {
                self.abort();
                return Err(e);
            }
        }
    }

    pub fn feed(&mut self, mut bytes: &[u8]) -> Result<(), Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        loop {
            let room = self.spare();
            if room.is_empty() && !bytes.is_empty() {
                self.reader.grow();
                continue;
            }
            let n = room.len().min(bytes.len());
            room[..n].copy_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            self.fill(n)?;
            if bytes.is_empty() {
                return Ok(());
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

    pub fn poll(&mut self) -> Option<Event<&[u8]>> {
        let event = self.events.pop_front()?;
        Some(event.map(|range| self.reader.slice(range)))
    }

    pub fn transmit(&mut self) -> Option<Transmit<&[u8]>> {
        if let Some(id) = self.lent.take() {
            self.prune(id);
        }
        if self.control.ready() {
            return Some(Transmit::Bytes(self.control.lend()));
        }
        self.control.clear();
        while let Some(id) = self.ring.pop_front() {
            let Some(lane) = self.lanes.get_mut(&id) else {
                continue;
            };
            let front = lane.front();
            let mut k = 0;
            while let Ok(Some(head)) = Head::parse(&front[k..]) {
                let size = head.size();
                if k > 0 && k + size > QUANTUM {
                    break;
                }
                k += size;
            }
            let range = lane.take(k);
            if !lane.is_empty() {
                self.ring.push_back(id);
            }
            self.lent = Some(id);
            return Some(Transmit::Bytes(self.lanes[&id].slice(range)));
        }
        None
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
        let frame =
            Frame::data(id, flow.proto(), &data[..n]).map_err(Error::Frame)?;
        self.queue(id, &frame);
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
        self.queue(id, &frame);
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
