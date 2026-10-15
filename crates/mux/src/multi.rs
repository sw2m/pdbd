use crate::flow::Flow;
use crate::frame::{Proto, Reader};
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
        let _ = (side, window, cap);
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
        let _ = (
            self.side,
            self.window,
            self.cap,
            self.next,
            self.peer,
            &self.reader,
            &self.events,
            &self.out,
            self.dead,
        );
        todo!()
    }
}

#[cfg(test)]
mod test;
