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
        let _ = (
            self.sent,
            self.received,
            self.reset,
            &self.events,
            &self.out,
        );
        todo!()
    }
}

#[cfg(test)]
mod test;
