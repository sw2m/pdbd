use crate::flow::{self, ABORTED};
use crate::frame::Proto;
use crate::{Error, Event, Transmit};
use std::collections::VecDeque;

pub const ID: u32 = 0; // never a multi-channel flow id, which starts at 1

#[derive(Debug, Default)]
pub struct Single {
    sent: bool,
    received: bool,
    reset: bool,
    events: VecDeque<Event>,
    out: VecDeque<Transmit>,
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

    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if self.received || self.reset {
            return Err(Error::Dead);
        }
        if !bytes.is_empty() {
            self.events.push_back(Event::Data(ID, bytes.to_vec()));
        }
        Ok(())
    }

    pub fn end(&mut self) -> Result<(), Error> {
        if self.received || self.reset {
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
        self.out.clear();
        self.events.push_back(Event::Reset(ID, ABORTED));
    }

    pub fn poll(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    pub fn transmit(&mut self) -> Option<Transmit> {
        self.out.pop_front()
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
        match self.out.back_mut() {
            Some(Transmit::Bytes(b)) => b.extend_from_slice(data),
            _ => self.out.push_back(Transmit::Bytes(data.to_vec())),
        }
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
        self.out.push_back(Transmit::Shutdown);
        Ok(())
    }

    pub fn reset(&mut self, id: u32, _: u32) -> Result<(), Error> {
        self.check(id)?;
        if self.reset {
            return Err(Error::Flow(ID, flow::Error::Closed));
        }
        self.reset = true;
        self.out.clear();
        self.out.push_back(Transmit::Abort);
        Ok(())
    }

    pub fn stop(&mut self, id: u32) -> Result<(), Error> {
        let _ = id;
        todo!()
    }

    pub fn grow(&mut self, id: u32, window: u32) -> Result<(), Error> {
        let _ = (id, window);
        todo!()
    }

    pub fn tick(&mut self, now: u64) {
        let _ = now;
        todo!()
    }

    pub fn expire(&mut self, now: u64, deadline: u64) {
        let _ = (now, deadline);
        todo!()
    }

    pub fn paused(&self) -> bool {
        todo!()
    }

    pub fn done(&self) -> bool {
        self.reset || (self.sent && self.received)
    }
}

#[cfg(test)]
mod test;
