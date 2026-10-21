use crate::flow::{self, Flow, State, ABORTED, REFUSED};
use crate::frame::{Body, Frame, Head, Proto, Reader, HEADER, MAX};
use crate::hold::Hold;
use crate::{Error, Event, Held, Pull, Side, Source};
use std::collections::{BTreeMap, VecDeque};

const SLICE: usize = 1 << 14; // stream frame payload: fastest of 8–64 KiB in the 2026-10-21 sweep, nghttp2's default

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

    fn clear(&mut self) {
        self.buf.clear();
        self.sent = 0;
    }

    fn drain(&mut self, out: &mut [u8]) -> usize {
        let n = (self.buf.len() - self.sent).min(out.len());
        out[..n].copy_from_slice(&self.buf[self.sent..self.sent + n]);
        self.sent += n;
        if self.sent == self.buf.len() {
            self.clear();
        }
        n
    }
}

#[derive(Debug)]
struct Slot {
    flow: Flow,
    hold: Hold,
}

impl Slot {
    fn new(flow: Flow) -> Self {
        Self {
            flow,
            hold: Hold::default(),
        }
    }

    fn over(&self) -> bool {
        let reset = self.flow.state() == State::Reset;
        self.flow.done() && (reset || self.flow.held() == 0)
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
    flows: BTreeMap<u32, Slot>,
    reader: Reader,
    events: VecDeque<Event>,
    order: VecDeque<(u32, usize)>,
    control: Control,
    due: VecDeque<u32>,
    later: Vec<u32>,
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
            order: VecDeque::new(),
            control: Control::default(),
            due: VecDeque::new(),
            later: Vec::new(),
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

    fn slot(&mut self, id: u32) -> Result<&mut Slot, Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        self.flows.get_mut(&id).ok_or(Error::Unknown(id))
    }

    fn get(&mut self, id: u32) -> Result<&mut Flow, Error> {
        self.slot(id).map(|s| &mut s.flow)
    }

    fn signal(&mut self, frame: Frame) {
        self.control.push(&frame);
    }

    fn wake(due: &mut VecDeque<u32>, id: u32, flow: &mut Flow) {
        if flow.wants() && !flow.queued() && flow.sendable() {
            flow.queue(true);
            due.push_back(id);
        }
    }

    fn reap(&mut self, id: u32) {
        if !self.flows.get(&id).is_some_and(Slot::over) {
            return;
        }
        if let Some(slot) = self.flows.remove(&id) {
            self.used -= u64::from(slot.flow.held());
        }
        self.events
            .retain(|e| !matches!(e, Event::Data(i, _) if *i == id));
    }

    fn mark(&mut self, id: u32) {
        let now = self.now;
        if let Some(slot) = self.flows.get_mut(&id) {
            slot.flow.mark(now);
        }
    }

    fn abort(&mut self) {
        self.dead = true;
        self.used = 0;
        self.control.clear();
        self.order.clear();
        self.events.retain(|e| !matches!(e, Event::Data(..)));
        let flows = std::mem::take(&mut self.flows);
        self.events
            .extend(flows.into_keys().map(|id| Event::Reset(id, ABORTED)));
    }

    fn keep(&mut self) -> usize {
        while let Some(&(id, end)) = self.order.front() {
            let piece = self.flows.get(&id).and_then(|s| s.hold.front());
            match piece {
                Some(piece) if piece.end <= end => return piece.start,
                _ => drop(self.order.pop_front()),
            }
        }
        self.reader.at()
    }

    fn spill(&mut self) {
        for slot in self.flows.values_mut() {
            slot.hold.spill(&self.reader);
        }
        self.order.clear();
    }

    fn blocked(&self) -> bool {
        let Ok(Some(head)) = Head::parse(self.reader.peek()) else {
            return false;
        };
        let held = head.kind == 0
            && self.flows.get(&head.flow).is_some_and(
                |Slot { flow: f, .. }| {
                    f.state() == State::Open
                        && f.proto() == head.proto
                        && !f.stopped()
                        && !f.received()
                        && head.field <= f.granted()
                },
            );
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
            self.flows.insert(id, Slot::new(flow));
            self.events.push_back(Event::Open(id, head.proto));
            return Ok(());
        }
        let Some(slot) = self.flows.get_mut(&id) else {
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
        let Slot { flow, hold } = slot;
        let event = match flow.receive(frame).map_err(|e| Error::Peer(id, e))? {
            flow::Event::Accepted => {
                flow.mark(self.now);
                if let Some(grant) = flow.flush() {
                    self.control.push(&grant);
                }
                Self::wake(&mut self.due, id, flow);
                Event::Accepted(id)
            }
            flow::Event::Data(n) => {
                self.used += u64::from(n);
                if n > 0 {
                    self.order.push_back((id, payload.end));
                    hold.push(payload);
                }
                Event::Data(id, n)
            }
            flow::Event::Dropped(_) => return Ok(()),
            flow::Event::Window(n) => {
                Self::wake(&mut self.due, id, flow);
                Event::Window(id, n)
            }
            flow::Event::Closed => {
                flow.mark(self.now);
                Event::Closed(id)
            }
            flow::Event::Reset(code) => Event::Reset(id, code),
            flow::Event::Stopped => {
                if let Ok(close) = flow.close() {
                    flow.mark(self.now);
                    self.control.push(&close);
                }
                Event::Stopped(id)
            }
        };
        let over = slot.over();
        self.events.push_back(event);
        if over {
            self.reap(id);
        }
        Ok(())
    }

    pub fn spare(&mut self) -> &mut [u8] {
        if self.dead {
            return &mut [];
        }
        let keep = self.keep();
        let stuck = keep < self.reader.at() && self.reader.pending() == 0;
        if stuck
            && self.reader.spare(keep).is_empty()
            && !self.events.iter().any(|e| matches!(e, Event::Data(..)))
        {
            self.spill();
        }
        let keep = self.keep();
        self.reader.spare(keep)
    }

    pub fn fill(&mut self, n: usize) -> Result<(), Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        self.reader.fill(n);
        self.decode()
    }

    fn decode(&mut self) -> Result<(), Error> {
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

    pub fn poll(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    pub fn held(&self, id: u32) -> Held<'_> {
        match self.flows.get(&id) {
            Some(slot) => slot.hold.lend(&self.reader),
            None => {
                static EMPTY: Hold = Hold::EMPTY;
                EMPTY.lend(&self.reader)
            }
        }
    }

    pub fn ready(&mut self, id: u32) -> Result<(), Error> {
        if self.dead {
            return Err(Error::Dead);
        }
        let flow = &mut self.flows.get_mut(&id).ok_or(Error::Unknown(id))?.flow;
        flow.ready().map_err(|e| Error::Flow(id, e))?;
        flow.want(true);
        Self::wake(&mut self.due, id, flow);
        Ok(())
    }

    pub fn pull(&mut self, out: &mut [u8], source: &mut impl Source) -> usize {
        if self.dead {
            return 0;
        }
        let mut at = self.control.drain(out);
        if self.control.ready() {
            return at;
        }
        while out.len() - at > HEADER {
            let Some(id) = self.due.pop_front() else {
                break;
            };
            let Some(Slot { flow, .. }) = self.flows.get_mut(&id) else {
                continue;
            };
            flow.queue(false);
            if !flow.sendable() {
                continue;
            }
            let proto = flow.proto();
            let space = out.len() - at - HEADER;
            let cap = match proto {
                Proto::Udp => MAX as usize,
                Proto::Tcp => SLICE,
            }
            .min(flow.credit() as usize);
            let room = space.min(cap);
            let body = &mut out[at + HEADER..at + HEADER + room];
            match source.pull(id, body) {
                Pull::Bytes(n) if n > room => {
                    flow.want(false);
                    let _ = self.reset(id, ABORTED);
                }
                Pull::Bytes(0) if proto == Proto::Tcp => flow.want(false),
                Pull::Bytes(n) => {
                    if flow.send(&body[..n]) != Ok(n) {
                        flow.want(false);
                        let _ = self.reset(id, ABORTED);
                        continue;
                    }
                    Head::data(&mut out[at..], id, proto, n);
                    at += HEADER + n;
                    Self::wake(&mut self.due, id, flow);
                }
                Pull::Blocked if space < cap => {
                    flow.queue(true);
                    self.later.push(id);
                }
                Pull::Blocked => {}
                Pull::Wait => flow.want(false),
                Pull::End => {
                    flow.want(false);
                    let _ = self.close(id);
                }
                Pull::Fail => {
                    flow.want(false);
                    let _ = self.reset(id, ABORTED);
                }
            }
        }
        for id in self.later.drain(..).rev() {
            self.due.push_front(id);
        }
        at + self.control.drain(&mut out[at..])
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
        self.flows.insert(id, Slot::new(flow));
        self.signal(frame);
        Ok(id)
    }

    pub fn accept(&mut self, id: u32) -> Result<(), Error> {
        let frame = self.get(id)?.accept().map_err(|e| Error::Flow(id, e))?;
        self.mark(id);
        self.signal(frame);
        Ok(())
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<(), Error> {
        let slot = self.slot(id)?;
        let frame = slot.flow.consume(n).map_err(|e| Error::Flow(id, e))?;
        slot.hold.consume(n as usize);
        let over = slot.over();
        self.used -= u64::from(n);
        if over {
            self.reap(id);
        }
        if let Some(frame) = frame {
            self.signal(frame);
        }
        self.resume()
    }

    fn resume(&mut self) -> Result<(), Error> {
        if !self.paused || self.dead {
            return Ok(());
        }
        self.decode()
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
        self.signal(frame);
        self.reap(id);
        self.resume()
    }

    pub fn reset(&mut self, id: u32, code: u32) -> Result<(), Error> {
        let frame =
            self.get(id)?.reset(code).map_err(|e| Error::Flow(id, e))?;
        self.signal(frame);
        self.reap(id);
        self.resume()
    }

    pub fn stop(&mut self, id: u32) -> Result<(), Error> {
        let slot = self.slot(id)?;
        let held = slot.flow.held();
        let frame = slot.flow.stop().map_err(|e| Error::Flow(id, e))?;
        slot.hold.clear();
        self.used -= u64::from(held);
        self.events
            .retain(|e| !matches!(e, Event::Data(i, _) if *i == id));
        self.mark(id);
        self.signal(frame);
        self.resume()
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
            .filter(|(_, s)| s.flow.overdue(now, deadline))
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
        self.flows.get(&id).map(|s| &s.flow)
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
