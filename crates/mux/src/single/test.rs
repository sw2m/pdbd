use super::*;
use crate::flow::{self, ABORTED};
use crate::frame::MAX;
use proptest::prelude::*;

#[derive(Debug, Default)]
struct Queue {
    items: VecDeque<Vec<u8>>,
    whole: bool,
    end: bool,
    fail: bool,
}

impl Queue {
    fn new(proto: Proto) -> Self {
        Self {
            whole: proto == Proto::Udp,
            ..Self::default()
        }
    }
}

impl Source for Queue {
    fn pull(&mut self, _: u32, buf: &mut [u8]) -> Pull {
        let Some(front) = self.items.front_mut() else {
            return match (self.fail, self.end) {
                (true, _) => Pull::Fail,
                (_, true) => Pull::End,
                _ => Pull::Wait,
            };
        };
        let n = match self.whole {
            true if front.len() > buf.len() => return Pull::Blocked,
            true => front.len(),
            false => front.len().min(buf.len()),
        };
        buf[..n].copy_from_slice(&front[..n]);
        match n == front.len() {
            true => drop(self.items.pop_front()),
            false => drop(front.drain(..n)),
        }
        Pull::Bytes(n)
    }
}

struct End {
    s: Single,
    q: Queue,
    seen: usize,
}

impl End {
    fn new(proto: Proto) -> Self {
        Self {
            s: Single::new(proto),
            q: Queue::new(proto),
            seen: 0,
        }
    }

    fn send(&mut self, data: &[u8]) -> Result<(), Error> {
        self.q.items.push_back(data.to_vec());
        self.s.ready(ID)
    }

    fn transmits(&mut self) -> Vec<Transmit<Vec<u8>>> {
        let mut out = vec![0; 2 * (2 + MAX as usize)];
        let mut all = Vec::new();
        loop {
            match self.s.pull(&mut out, &mut self.q) {
                crate::Transmit::Bytes(0) => return all,
                t => all.push(Transmit::from(t, &out)),
            }
        }
    }

    fn wire(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for t in self.transmits() {
            match t {
                Transmit::Bytes(b) => bytes.extend(b),
                t => panic!("{t:?}"),
            }
        }
        bytes
    }

    fn events(&mut self) -> Vec<Event<Vec<u8>>> {
        let mut all = Vec::new();
        while let Some(e) = self.s.poll() {
            all.push(match e {
                Event::Data(id, n) => {
                    let held: Vec<u8> =
                        self.s.held(ID).flatten().copied().collect();
                    let got = held[self.seen..self.seen + n as usize].to_vec();
                    self.seen += n as usize;
                    Event::Data(id, got)
                }
                e => e.map(|_| Vec::new()),
            });
        }
        all
    }

    fn consume(&mut self, n: u32) -> Result<(), Error> {
        self.s.consume(ID, n)?;
        self.seen -= n as usize;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Transmit<B> {
    Bytes(B),
    Shutdown,
    Abort,
}

impl Transmit<Vec<u8>> {
    fn from(t: crate::Transmit, out: &[u8]) -> Self {
        match t {
            crate::Transmit::Bytes(n) => Self::Bytes(out[..n].to_vec()),
            crate::Transmit::Shutdown => Self::Shutdown,
            crate::Transmit::Abort => Self::Abort,
        }
    }
}

// #58
#[test]
fn one() {
    let mut e = End::new(Proto::Tcp);
    assert_eq!(e.s.open(Proto::Tcp), Err(Error::Mode));
    assert_eq!(e.s.accept(ID), Err(Error::Mode));
    assert_eq!(e.s.ready(7), Err(Error::Unknown(7)));
    assert_eq!(e.s.close(7), Err(Error::Unknown(7)));
    assert!(!e.s.done());
    assert!(e.events().is_empty());
    assert!(e.transmits().is_empty());
}

// #58
#[test]
fn extras() {
    let mut e = End::new(Proto::Tcp);
    assert_eq!(e.s.stop(ID), Err(Error::Mode));
    assert_eq!(e.s.grow(ID, 1), Err(Error::Mode));
    e.s.tick(5);
    e.s.expire(1000, 1);
    assert!(!e.s.paused());
    assert!(e.events().is_empty());
    assert!(e.transmits().is_empty());
}

// #58
#[test]
fn bare() {
    let mut e = End::new(Proto::Tcp);
    let big = vec![7u8; 1 << 20];
    e.send(&big).unwrap();
    e.send(b"tail").unwrap();
    let mut want = big.clone();
    want.extend(b"tail");
    assert_eq!(e.wire(), want);
    e.s.feed(b"in").unwrap();
    e.s.feed(b"").unwrap();
    assert_eq!(e.events(), vec![Event::Data(ID, b"in".to_vec())]);
    assert_eq!(e.consume(2), Ok(()));
    assert_eq!(e.s.flush(ID), Ok(()));
    assert!(e.transmits().is_empty());
}

// #58 "Data path"
#[test]
fn empty() {
    let mut e = End::new(Proto::Tcp);
    e.send(b"").unwrap();
    assert!(e.transmits().is_empty());
}

// #58
#[test]
fn halfclose() {
    let mut e = End::new(Proto::Tcp);
    e.send(b"a").unwrap();
    assert_eq!(e.transmits(), vec![Transmit::Bytes(b"a".to_vec())]);
    e.s.close(ID).unwrap();
    assert_eq!(e.transmits(), vec![Transmit::Shutdown]);
    assert_eq!(e.s.ready(ID), Err(Error::Flow(ID, flow::Error::Closed)));
    assert_eq!(e.s.close(ID), Err(Error::Flow(ID, flow::Error::Closed)));
    assert!(!e.s.done());
    e.s.feed(b"still").unwrap();
    e.s.end().unwrap();
    assert_eq!(
        e.events(),
        vec![Event::Data(ID, b"still".to_vec()), Event::Closed(ID)]
    );
    assert!(!e.s.done());
    e.consume(5).unwrap();
    assert!(e.s.done());
    assert_eq!(e.s.feed(b"x"), Err(Error::Dead));
    assert_eq!(e.s.end(), Err(Error::Dead));
}

// #58 "Data path"
#[test]
fn finish() {
    let mut e = End::new(Proto::Tcp);
    e.send(b"last").unwrap();
    e.q.end = true;
    assert_eq!(
        e.transmits(),
        vec![Transmit::Bytes(b"last".to_vec()), Transmit::Shutdown]
    );
    assert_eq!(e.s.ready(ID), Err(Error::Flow(ID, flow::Error::Closed)));
}

// #58 "Data path"
#[test]
fn broke() {
    let mut e = End::new(Proto::Tcp);
    e.q.fail = true;
    e.s.ready(ID).unwrap();
    assert_eq!(e.transmits(), vec![Transmit::Abort]);
    assert!(e.s.done());
}

// #58 "Data path"
#[test]
fn overclaim() {
    let mut s = Single::new(Proto::Tcp);
    s.ready(ID).unwrap();
    let mut out = vec![0; 64];
    let mut liar = |_: u32, b: &mut [u8]| Pull::Bytes(b.len() + 1);
    let t = s.pull(&mut out, &mut liar);
    assert_eq!(Transmit::from(t, &out), Transmit::Abort);
    assert!(s.done());
}

// #58
#[test]
fn eof() {
    let mut e = End::new(Proto::Tcp);
    e.s.end().unwrap();
    assert_eq!(e.events(), vec![Event::Closed(ID)]);
    assert_eq!(e.s.feed(b"x"), Err(Error::Dead));
    e.send(b"y").unwrap();
    assert_eq!(e.wire(), b"y");
    e.s.close(ID).unwrap();
    assert!(e.s.done());
}

// #58
#[test]
fn reset() {
    let mut e = End::new(Proto::Tcp);
    e.send(b"unsent").unwrap();
    e.s.reset(ID, ABORTED).unwrap();
    assert_eq!(e.transmits(), vec![Transmit::Abort]);
    assert!(e.s.done());
    assert_eq!(
        e.s.reset(ID, ABORTED),
        Err(Error::Flow(ID, flow::Error::Closed))
    );
    assert_eq!(e.s.ready(ID), Err(Error::Flow(ID, flow::Error::Closed)));
    assert_eq!(e.s.feed(b"x"), Err(Error::Dead));
}

// #58
#[test]
fn fail() {
    let mut e = End::new(Proto::Tcp);
    e.s.fail();
    assert_eq!(e.events(), vec![Event::Reset(ID, ABORTED)]);
    assert!(e.s.done());
    e.s.fail();
    assert!(e.events().is_empty());
    assert_eq!(e.s.ready(ID), Err(Error::Flow(ID, flow::Error::Closed)));
}

// #58 "Single-channel mode"
#[test]
fn datagrams() {
    let mut e = End::new(Proto::Udp);
    e.send(b"one").unwrap();
    e.send(b"").unwrap();
    e.send(b"three").unwrap();
    let wire = e.wire();
    assert_eq!(wire, b"\x00\x03one\x00\x00\x00\x05three".to_vec());
    let mut f = End::new(Proto::Udp);
    f.s.feed(&wire).unwrap();
    assert_eq!(
        f.events(),
        vec![
            Event::Data(ID, b"one".to_vec()),
            Event::Data(ID, vec![]),
            Event::Data(ID, b"three".to_vec())
        ]
    );
}

// #58 "Data path"
#[test]
fn room() {
    let mut e = End::new(Proto::Udp);
    e.send(&[1; 100]).unwrap();
    e.send(b"x").unwrap();
    let mut out = vec![0; 50];
    let t = e.s.pull(&mut out, &mut e.q);
    assert_eq!(Transmit::from(t, &out), Transmit::Bytes(vec![]));
    let mut out = vec![0; 200];
    let t = e.s.pull(&mut out, &mut e.q);
    let mut want = vec![0, 100];
    want.extend([1; 100]);
    want.extend(b"\x00\x01x");
    assert_eq!(Transmit::from(t, &out), Transmit::Bytes(want));
}

// #58
#[test]
fn torn() {
    let mut s = Single::new(Proto::Udp);
    s.feed(b"\x00\x05ab").unwrap();
    assert_eq!(s.end(), Err(Error::Truncated));
}

proptest! {
    // #58
    #[test]
    fn passthrough(
        writes in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..256), 0..32),
        reads in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..256), 0..32),
    ) {
        let mut e = End::new(Proto::Tcp);
        let mut want = Vec::new();
        for w in &writes {
            prop_assert_eq!(e.send(w), Ok(()));
            want.extend(w);
        }
        prop_assert_eq!(e.wire(), want);
        for r in &reads {
            e.s.feed(r).unwrap();
        }
        let mut got = Vec::new();
        for ev in e.events() {
            match ev {
                Event::Data(ID, d) => got.extend(d),
                ev => prop_assert!(false, "{ev:?}"),
            }
        }
        prop_assert_eq!(got, reads.concat());
    }

    // #58 "Single-channel mode"
    #[test]
    fn whole(
        sizes in prop::collection::vec(prop_oneof![0usize..64, 0usize..=MAX as usize], 1..24),
        reads in prop::collection::vec(1usize..70_000, 1..16),
        hold in any::<bool>(),
    ) {
        let mut a = End::new(Proto::Udp);
        let mut sent = Vec::new();
        for (i, &n) in sizes.iter().enumerate() {
            let d: Vec<u8> = (0..n).map(|k| (i + k) as u8).collect();
            a.send(&d).unwrap();
            sent.push(d);
        }
        let wire = a.wire();
        let mut b = End::new(Proto::Udp);
        let mut got = Vec::new();
        let (mut at, mut turn) = (0, 0);
        while at < wire.len() {
            let s = b.s.spare();
            if s.is_empty() {
                prop_assert!(hold, "no room at {} of {}", at, wire.len());
                let all = b.seen as u32;
                b.consume(all).unwrap();
                continue;
            }
            let n = s.len().min(reads[turn % reads.len()]).min(wire.len() - at);
            turn += 1;
            s[..n].copy_from_slice(&wire[at..at + n]);
            b.s.fill(n).unwrap();
            at += n;
            for ev in b.events() {
                if let Event::Data(_, d) = ev {
                    got.push(d);
                }
            }
            if !hold {
                let all = b.seen as u32;
                b.consume(all).unwrap();
            }
        }
        prop_assert_eq!(got, sent);
    }
}

// #58 "Receiving lends"
#[test]
fn place() {
    let mut s = Single::new(Proto::Tcp);
    let b = s.spare();
    b[..3].copy_from_slice(b"raw");
    let at = b.as_ptr() as usize;
    s.fill(3).unwrap();
    assert_eq!(s.poll(), Some(Event::Data(ID, 3)));
    let lent: Vec<&[u8]> = s.held(ID).collect();
    assert_eq!(lent, vec![&b"raw"[..]]);
    assert_eq!(lent[0].as_ptr() as usize, at);
}

// #58 "Receiving lends"
#[test]
fn held() {
    let mut e = End::new(Proto::Tcp);
    for chunk in [&b"first"[..], b"second"] {
        let b = e.s.spare();
        b[..chunk.len()].copy_from_slice(chunk);
        e.s.fill(chunk.len()).unwrap();
    }
    assert_eq!(
        e.events(),
        vec![
            Event::Data(ID, b"first".to_vec()),
            Event::Data(ID, b"second".to_vec())
        ]
    );
    e.consume(6).unwrap();
    e.s.feed(b"!").unwrap();
    e.events();
    let left: Vec<u8> = e.s.held(ID).flatten().copied().collect();
    assert_eq!(left, b"econd!".to_vec());
    let over = flow::Error::Consume { n: 7, held: 6 };
    assert_eq!(e.s.consume(ID, 7), Err(Error::Flow(ID, over)));
}

// #58 "A holding flow never stalls another"
#[test]
fn lag() {
    let mut e = End::new(Proto::Tcp);
    let chunk = vec![4u8; 50_000];
    for round in 0..64 {
        let s = e.s.spare();
        assert!(!s.is_empty(), "no room in round {round}");
        let n = s.len().min(chunk.len());
        s[..n].copy_from_slice(&chunk[..n]);
        e.s.fill(n).unwrap();
        e.events();
        let held = e.seen as u32;
        e.consume(held - 1).unwrap();
    }
    let left: Vec<u8> = e.s.held(ID).flatten().copied().collect();
    assert_eq!(left, vec![4]);
}

// #58 "Receiving lends"
#[test]
fn other() {
    let mut s = Single::new(Proto::Tcp);
    s.feed(b"mine").unwrap();
    assert_eq!(s.held(ID).count(), 1);
    assert_eq!(s.held(7).count(), 0);
}

// #58
#[test]
fn cut() {
    let mut s = Single::new(Proto::Udp);
    s.feed(b"\x00\x05ab").unwrap();
    assert_eq!(s.end(), Err(Error::Truncated));
    assert_eq!(s.poll(), Some(Event::Reset(ID, ABORTED)));
    assert!(s.done());
}

// #58 "Receiving lends"
#[test]
fn unread() {
    let mut s = Single::new(Proto::Tcp);
    s.feed(b"left").unwrap();
    s.close(ID).unwrap();
    s.end().unwrap();
    assert!(!s.done());
    s.consume(ID, 4).unwrap();
    assert!(s.done());
}
