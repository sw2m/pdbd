use crate::flow::{self, ABORTED};
use crate::frame::Proto;
use crate::{Error, Event, Transmit};
use std::collections::VecDeque;
use std::ops::Range;

pub const ID: u32 = 0; // never a multi-channel flow id, which starts at 1
const ROOM: usize = 1 << 16; // one socket read

#[derive(Debug, Default)]
pub struct Single {
    sent: bool,
    received: bool,
    reset: bool,
    events: VecDeque<Event<Range<usize>>>,
    inbox: Vec<u8>,
    end: usize,
    out: Vec<u8>,
    at: usize,
    shut: bool,
    abort: bool,
}

impl Single {
    pub fn new() -> Self {
        Self::default()
    }

    fn check(&self, id: u32) -> Result<(), Error> {
        match id {
            ID => Ok(()),
            _ => Err(Error::Unknown(id)),
        }
    }

    fn writable(&self, id: u32) -> Result<(), Error> {
        self.check(id)?;
        match self.sent || self.reset {
            true => Err(Error::Flow(ID, flow::Error::Closed)),
            false => Ok(()),
        }
    }

    fn dead(&self) -> bool {
        self.received || self.reset
    }

    pub fn spare(&mut self) -> &mut [u8] {
        if self.dead() {
            return &mut [];
        }
        if !self.events.iter().any(|e| matches!(e, Event::Data(..))) {
            self.end = 0;
        }
        if self.inbox.is_empty() {
            self.inbox.resize(ROOM, 0);
        }
        &mut self.inbox[self.end..]
    }

    pub fn fill(&mut self, n: usize) -> Result<(), Error> {
        if self.dead() {
            return Err(Error::Dead);
        }
        assert!(self.end + n <= self.inbox.len(), "filled past spare()");
        if n > 0 {
            self.events
                .push_back(Event::Data(ID, self.end..self.end + n));
            self.end += n;
        }
        Ok(())
    }

    pub fn feed(&mut self, mut bytes: &[u8]) -> Result<(), Error> {
        if self.dead() {
            return Err(Error::Dead);
        }
        while !bytes.is_empty() {
            let room = self.spare();
            if room.is_empty() {
                let size = self.inbox.len() * 2;
                self.inbox.resize(size, 0);
                continue;
            }
            let n = room.len().min(bytes.len());
            room[..n].copy_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            self.fill(n)?;
        }
        Ok(())
    }

    pub fn end(&mut self) -> Result<(), Error> {
        if self.dead() {
            return Err(Error::Dead);
        }
        self.received = true;
        self.events.push_back(Event::Closed(ID));
        Ok(())
    }

    pub fn fail(&mut self) {
        if self.reset {
            return;
        }
        self.reset = true;
        self.discard();
        self.events.push_back(Event::Reset(ID, ABORTED));
    }

    fn discard(&mut self) {
        self.out.clear();
        self.at = 0;
        self.shut = false;
    }

    pub fn poll(&mut self) -> Option<Event<&[u8]>> {
        let event = self.events.pop_front()?;
        Some(event.map(|range| &self.inbox[range]))
    }

    pub fn transmit(&mut self) -> Option<Transmit<&[u8]>> {
        if self.at < self.out.len() {
            let at = std::mem::replace(&mut self.at, self.out.len());
            return Some(Transmit::Bytes(&self.out[at..]));
        }
        self.out.clear();
        self.at = 0;
        if std::mem::take(&mut self.shut) {
            return Some(Transmit::Shutdown);
        }
        std::mem::take(&mut self.abort).then_some(Transmit::Abort)
    }

    pub fn open(&mut self, _: Proto) -> Result<u32, Error> {
        Err(Error::Mode)
    }

    pub fn accept(&mut self, _: u32) -> Result<(), Error> {
        Err(Error::Mode)
    }

    pub fn send(&mut self, id: u32, data: &[u8]) -> Result<usize, Error> {
        self.writable(id)?;
        if data.is_empty() {
            return Err(Error::Flow(ID, flow::Error::Empty));
        }
        if self.at == self.out.len() {
            self.out.clear();
            self.at = 0;
        }
        self.out.extend_from_slice(data);
        Ok(data.len())
    }

    pub fn consume(&mut self, id: u32, _: u32) -> Result<(), Error> {
        self.check(id)
    }

    pub fn flush(&mut self, id: u32) -> Result<(), Error> {
        self.check(id)
    }

    pub fn close(&mut self, id: u32) -> Result<(), Error> {
        self.writable(id)?;
        self.sent = true;
        self.shut = true;
        Ok(())
    }

    pub fn reset(&mut self, id: u32, _: u32) -> Result<(), Error> {
        self.check(id)?;
        if self.reset {
            return Err(Error::Flow(ID, flow::Error::Closed));
        }
        self.reset = true;
        self.discard();
        self.abort = true;
        Ok(())
    }

    pub fn stop(&mut self, _: u32) -> Result<(), Error> {
        Err(Error::Mode)
    }

    pub fn grow(&mut self, _: u32, _: u32) -> Result<(), Error> {
        Err(Error::Mode)
    }

    pub fn tick(&mut self, _: u64) {}

    pub fn expire(&mut self, _: u64, _: u64) {}

    pub fn paused(&self) -> bool {
        false
    }

    pub fn done(&self) -> bool {
        self.reset || (self.sent && self.received)
    }
}

#[cfg(test)]
mod test;
