use crate::flow::{self, ABORTED};
use crate::frame::{Proto, Reader, Unit, MAX};
use crate::hold::Hold;
use crate::{Error, Event, Held, Pull, Source, Transmit};
use std::collections::VecDeque;

pub const ID: u32 = 0; // never a multi-channel flow id, which starts at 1

#[derive(Debug)]
pub struct Single {
    proto: Proto,
    reader: Reader,
    hold: Hold,
    held: u32,
    events: VecDeque<Event>,
    wants: bool,
    sent: bool,
    received: bool,
    reset: bool,
    shut: bool,
    abort: bool,
}

impl Single {
    pub fn new(proto: Proto) -> Self {
        let unit = match proto {
            Proto::Tcp => Unit::Bare,
            Proto::Udp => Unit::Datagram,
        };
        Self {
            proto,
            reader: Reader::with(unit),
            hold: Hold::default(),
            held: 0,
            events: VecDeque::new(),
            wants: false,
            sent: false,
            received: false,
            reset: false,
            shut: false,
            abort: false,
        }
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

    fn keep(&self) -> usize {
        self.hold
            .front()
            .map_or(self.reader.at(), |piece| piece.start)
    }

    pub fn spare(&mut self) -> &mut [u8] {
        if self.dead() {
            return &mut [];
        }
        let keep = self.keep();
        let stuck = keep < self.reader.at() && self.reader.pending() == 0;
        if stuck
            && self.reader.spare(keep).is_empty()
            && !self.events.iter().any(|e| matches!(e, Event::Data(..)))
        {
            self.hold.spill(&self.reader);
        }
        let keep = self.keep();
        self.reader.spare(keep)
    }

    pub fn fill(&mut self, n: usize) -> Result<(), Error> {
        if self.dead() {
            return Err(Error::Dead);
        }
        self.reader.fill(n);
        while let Some(range) = self.reader.advance().map_err(Error::Frame)? {
            let n = range.len() as u32;
            if n > 0 {
                self.hold.push(range);
                self.held += n;
            }
            self.events.push_back(Event::Data(ID, n));
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
                self.reader.grow();
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
        if !self.reader.whole() {
            self.fail();
            return Err(Error::Truncated);
        }
        self.received = true;
        self.events.push_back(Event::Closed(ID));
        Ok(())
    }

    pub fn fail(&mut self) {
        if self.reset {
            return;
        }
        self.halt();
        self.events.push_back(Event::Reset(ID, ABORTED));
    }

    fn halt(&mut self) {
        self.reset = true;
        self.wants = false;
        self.shut = false;
    }

    pub fn poll(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    pub fn held(&self, id: u32) -> Held<'_> {
        static EMPTY: Hold = Hold::EMPTY;
        match id {
            ID => self.hold.lend(&self.reader),
            _ => EMPTY.lend(&self.reader),
        }
    }

    pub fn ready(&mut self, id: u32) -> Result<(), Error> {
        self.writable(id)?;
        self.wants = true;
        Ok(())
    }

    pub fn pull(
        &mut self,
        out: &mut [u8],
        source: &mut impl Source,
    ) -> Transmit {
        let head = match self.proto {
            Proto::Tcp => 0,
            Proto::Udp => 2,
        };
        let mut at = 0;
        while self.wants && out.len() - at > head {
            let room = match self.proto {
                Proto::Tcp => out.len() - at,
                Proto::Udp => (out.len() - at - head).min(MAX as usize),
            };
            let body = &mut out[at + head..at + head + room];
            match source.pull(ID, body) {
                Pull::Bytes(n) if n > room => {
                    self.halt();
                    self.abort = true;
                }
                Pull::Bytes(0) if self.proto == Proto::Tcp => {
                    self.wants = false
                }
                Pull::Bytes(n) => {
                    if self.proto == Proto::Udp {
                        out[at..at + 2]
                            .copy_from_slice(&(n as u16).to_be_bytes());
                    }
                    at += head + n;
                }
                Pull::Blocked => break,
                Pull::Wait => self.wants = false,
                Pull::End => {
                    self.wants = false;
                    self.sent = true;
                    self.shut = true;
                }
                Pull::Fail => {
                    self.halt();
                    self.abort = true;
                }
            }
        }
        if at > 0 && !self.abort {
            return Transmit::Bytes(at);
        }
        if std::mem::take(&mut self.abort) {
            return Transmit::Abort;
        }
        if std::mem::take(&mut self.shut) {
            return Transmit::Shutdown;
        }
        Transmit::Bytes(0)
    }

    pub fn open(&mut self, _: Proto) -> Result<u32, Error> {
        Err(Error::Mode)
    }

    pub fn accept(&mut self, _: u32) -> Result<(), Error> {
        Err(Error::Mode)
    }

    pub fn consume(&mut self, id: u32, n: u32) -> Result<(), Error> {
        self.check(id)?;
        if n > self.held {
            let over = flow::Error::Consume { n, held: self.held };
            return Err(Error::Flow(ID, over));
        }
        self.held -= n;
        self.hold.consume(n as usize);
        Ok(())
    }

    pub fn flush(&mut self, id: u32) -> Result<(), Error> {
        self.check(id)
    }

    pub fn close(&mut self, id: u32) -> Result<(), Error> {
        self.writable(id)?;
        self.sent = true;
        self.wants = false;
        self.shut = true;
        Ok(())
    }

    pub fn reset(&mut self, id: u32, _: u32) -> Result<(), Error> {
        self.check(id)?;
        if self.reset {
            return Err(Error::Flow(ID, flow::Error::Closed));
        }
        self.halt();
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
        self.reset || (self.sent && self.received && self.held == 0)
    }
}

#[cfg(test)]
mod test;
