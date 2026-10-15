use crate::frame::{Body, Frame, Proto};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Opening,
    Open,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Accepted,
    Data(Vec<u8>),
    Window(u32),
    Closed,
    Reset(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Blocked,
    Empty,
    Oversize(usize),
    Closed,
    State,
    Proto,
    Credit { len: u32, credit: u32 },
    Overflow,
    Consume { n: u32, held: u32 },
}

#[derive(Debug, Clone)]
pub struct Flow {
    id: u32,
    proto: Proto,
    state: State,
    credit: u32,
    window: u32,
    granted: u32,
    held: u32,
    freed: u32,
    sent: bool,
    received: bool,
}

impl Flow {
    pub fn open(id: u32, proto: Proto, window: u32) -> (Self, Frame) {
        let _ = (id, proto, window, Body::Close);
        todo!()
    }

    pub fn accept(open: &Frame, window: u32) -> Result<(Self, Frame), Error> {
        let _ = (open, window);
        todo!()
    }

    pub fn send(&mut self, data: &[u8]) -> Result<(Frame, usize), Error> {
        let _ = data;
        todo!()
    }

    pub fn receive(&mut self, frame: Frame) -> Result<Event, Error> {
        let _ = frame;
        todo!()
    }

    pub fn consume(&mut self, n: u32) -> Result<Option<Frame>, Error> {
        let _ = n;
        todo!()
    }

    pub fn flush(&mut self) -> Option<Frame> {
        todo!()
    }

    pub fn close(&mut self) -> Result<Frame, Error> {
        todo!()
    }

    pub fn reset(&mut self, code: u32) -> Frame {
        let _ = code;
        todo!()
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn proto(&self) -> Proto {
        self.proto
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn credit(&self) -> u32 {
        self.credit
    }

    pub fn window(&self) -> u32 {
        self.window
    }

    pub fn granted(&self) -> u32 {
        self.granted
    }

    pub fn held(&self) -> u32 {
        self.held
    }

    pub fn freed(&self) -> u32 {
        self.freed
    }

    pub fn done(&self) -> bool {
        let _ = (self.sent, self.received);
        todo!()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
