use crate::frame::{Body, Frame, Proto, MAX};
use std::fmt;

pub const REFUSED: u32 = 0; // RESET code: the peer declined or hit its cap
pub const ABORTED: u32 = 1; // RESET code: the local endpoint failed or the tunnel ended

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Opening,
    Pending,
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
    Stopped,
    Dropped(u32),
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
    Stopped,
    Window(u32),
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
    stopped: bool,
    halted: bool,
    since: u64,
}

impl Flow {
    fn new(
        id: u32,
        proto: Proto,
        state: State,
        credit: u32,
        window: u32,
    ) -> Self {
        Self {
            id,
            proto,
            state,
            credit,
            window,
            granted: window,
            held: 0,
            freed: 0,
            sent: false,
            received: false,
            stopped: false,
            halted: false,
            since: 0,
        }
    }

    fn frame(&self, body: Body) -> Frame {
        Frame {
            flow: self.id,
            proto: self.proto,
            body,
        }
    }

    pub fn open(
        id: u32,
        proto: Proto,
        window: u32,
    ) -> Result<(Self, Frame), Error> {
        let flow = Self::new(id, proto, State::Opening, 0, window);
        let frame = flow.frame(Body::Open(window));
        Ok((flow, frame))
    }

    pub fn offer(open: &Frame, window: u32) -> Result<Self, Error> {
        let Body::Open(credit) = open.body else {
            return Err(Error::State);
        };
        Ok(Self::new(
            open.flow,
            open.proto,
            State::Pending,
            credit,
            window,
        ))
    }

    pub fn accept(&mut self) -> Result<Frame, Error> {
        if self.state != State::Pending {
            return Err(Error::State);
        }
        self.state = State::Open;
        Ok(self.frame(Body::Accept(self.window)))
    }

    pub fn send(&mut self, data: &[u8]) -> Result<usize, Error> {
        if self.sent || self.state == State::Reset {
            return Err(Error::Closed);
        }
        if self.state == State::Pending {
            return Err(Error::State);
        }
        let len = data.len();
        if self.proto == Proto::Udp && len > MAX as usize {
            return Err(Error::Oversize(len));
        }
        if self.state == State::Opening {
            return Err(Error::Blocked);
        }
        let credit = self.credit as usize;
        let n = match self.proto {
            Proto::Tcp if len == 0 => return Err(Error::Empty),
            Proto::Tcp => len.min(credit).min(MAX as usize),
            Proto::Udp if len > credit => return Err(Error::Blocked),
            Proto::Udp => len,
        };
        if n == 0 && self.proto == Proto::Tcp {
            return Err(Error::Blocked);
        }
        self.credit -= n as u32;
        Ok(n)
    }

    pub fn receive(&mut self, frame: Frame) -> Result<Event, Error> {
        if frame.flow != self.id {
            return Err(Error::State);
        }
        if frame.proto != self.proto {
            return Err(Error::Proto);
        }
        if self.state == State::Reset {
            return Err(Error::Closed);
        }
        let open = self.state == State::Open;
        match frame.body {
            Body::Open(_) => Err(Error::State),
            Body::Stop => todo!(),
            Body::Reset(code) => {
                self.state = State::Reset;
                Ok(Event::Reset(code))
            }
            Body::Accept(window) if self.state == State::Opening => {
                self.credit = window;
                self.state = State::Open;
                Ok(Event::Accepted)
            }
            _ if !open => Err(Error::State),
            Body::Accept(_) => Err(Error::State),
            Body::Data(_) if self.received => Err(Error::Closed),
            Body::Data(d) => {
                let len = d.len() as u32;
                if len > self.granted {
                    return Err(Error::Credit {
                        len,
                        credit: self.granted,
                    });
                }
                self.granted -= len;
                self.held += len;
                Ok(Event::Data(d))
            }
            Body::Window(inc) => {
                self.credit =
                    self.credit.checked_add(inc).ok_or(Error::Overflow)?;
                Ok(Event::Window(inc))
            }
            Body::Close if self.received => Err(Error::State),
            Body::Close => {
                self.received = true;
                Ok(Event::Closed)
            }
        }
    }

    pub fn consume(&mut self, n: u32) -> Result<Option<Frame>, Error> {
        if n > self.held {
            return Err(Error::Consume { n, held: self.held });
        }
        self.held -= n;
        self.freed += n;
        if self.freed >= self.window.div_ceil(2) {
            return Ok(self.flush());
        }
        Ok(None)
    }

    pub fn flush(&mut self) -> Option<Frame> {
        if self.freed == 0 || self.received || self.state != State::Open {
            return None;
        }
        let inc = std::mem::take(&mut self.freed);
        self.granted += inc;
        Some(self.frame(Body::Window(inc)))
    }

    pub fn close(&mut self) -> Result<Frame, Error> {
        if self.sent || self.state == State::Reset {
            return Err(Error::Closed);
        }
        if self.state != State::Open {
            return Err(Error::State);
        }
        self.sent = true;
        Ok(self.frame(Body::Close))
    }

    pub fn reset(&mut self, code: u32) -> Result<Frame, Error> {
        if self.state == State::Reset {
            return Err(Error::Closed);
        }
        self.state = State::Reset;
        Ok(self.frame(Body::Reset(code)))
    }

    pub fn stop(&mut self) -> Result<Frame, Error> {
        let _ = (self.stopped, self.halted);
        todo!()
    }

    pub fn grow(&mut self, window: u32) -> Result<(), Error> {
        let _ = window;
        todo!()
    }

    pub fn mark(&mut self, now: u64) {
        let _ = now;
        todo!()
    }

    pub fn overdue(&self, now: u64, deadline: u64) -> bool {
        let _ = (now, deadline, self.since);
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
        self.state == State::Reset || (self.sent && self.received)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blocked => write!(f, "flow has no credit for this write"),
            Self::Empty => write!(f, "empty write on a stream flow"),
            Self::Oversize(n) => {
                write!(f, "datagram of {n} bytes exceeds one frame ({MAX})")
            }
            Self::Closed => write!(f, "flow is closed"),
            Self::State => write!(f, "out of order for the flow's state"),
            Self::Proto => write!(f, "frame proto differs from the flow's"),
            Self::Credit { len, credit } => {
                write!(f, "peer sent {len} bytes against {credit} of credit")
            }
            Self::Overflow => write!(f, "window update overflows 2^32-1"),
            Self::Consume { n, held } => {
                write!(f, "consumed {n} bytes but only {held} are held")
            }
            Self::Stopped => write!(f, "the peer stopped reading this flow"),
            Self::Window(w) => {
                write!(
                    f,
                    "datagram flow window {w} is below one datagram ({MAX})"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
