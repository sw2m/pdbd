use super::*;
use crate::flow::{self, State, ABORTED, REFUSED};
use crate::frame::{self, Body, Frame, HEADER, MAX, WHOLE};
use proptest::prelude::*;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::{Deref, DerefMut};

const ROOM: u64 = u64::MAX; // a budget that never pauses
const ALL: u32 = u32::MAX; // a cap that never refuses
const OUT: usize = 2 * WHOLE; // a pull buffer that always fits a whole datagram frame

#[derive(Debug, Default)]
struct Queue {
    items: BTreeMap<u32, VecDeque<Vec<u8>>>,
    whole: BTreeSet<u32>,
    ends: BTreeSet<u32>,
    taken: BTreeMap<u32, Vec<u8>>,
}

impl Source for Queue {
    fn pull(&mut self, id: u32, buf: &mut [u8]) -> Pull {
        let items = self.items.entry(id).or_default();
        let Some(front) = items.front_mut() else {
            return match self.ends.remove(&id) {
                true => Pull::End,
                false => Pull::Wait,
            };
        };
        let n = match self.whole.contains(&id) {
            true if front.len() > buf.len() => return Pull::Blocked,
            true => front.len(),
            false => front.len().min(buf.len()),
        };
        buf[..n].copy_from_slice(&front[..n]);
        self.taken.entry(id).or_default().extend(&front[..n]);
        match n == front.len() {
            true => drop(items.pop_front()),
            false => drop(front.drain(..n)),
        }
        Pull::Bytes(n)
    }
}

#[derive(Debug)]
struct Peer {
    m: Multi,
    q: Queue,
    seen: BTreeMap<u32, usize>,
}

impl Deref for Peer {
    type Target = Multi;

    fn deref(&self) -> &Multi {
        &self.m
    }
}

impl DerefMut for Peer {
    fn deref_mut(&mut self) -> &mut Multi {
        &mut self.m
    }
}

impl Peer {
    fn new(side: Side, window: u32, cap: u32, budget: u64) -> Self {
        Self {
            m: Multi::new(side, window, cap, budget).unwrap(),
            q: Queue::default(),
            seen: BTreeMap::new(),
        }
    }

    fn send(&mut self, id: u32, data: &[u8]) -> Result<(), Error> {
        if self.m.flow(id).is_some_and(|f| f.proto() == Proto::Udp) {
            self.q.whole.insert(id);
        } else if data.is_empty() {
            return self.m.ready(id);
        }
        self.q.items.entry(id).or_default().push_back(data.to_vec());
        self.m.ready(id)
    }

    fn finish(&mut self, id: u32) -> Result<(), Error> {
        self.q.ends.insert(id);
        self.m.ready(id)
    }

    fn bytes(&self, id: u32) -> Vec<u8> {
        self.m.held(id).flatten().copied().collect()
    }

    fn consume(&mut self, id: u32, n: u32) -> Result<(), Error> {
        self.m.consume(id, n)?;
        let seen = self.seen.entry(id).or_default();
        *seen = seen.saturating_sub(n as usize);
        Ok(())
    }

    fn stop(&mut self, id: u32) -> Result<(), Error> {
        self.m.stop(id)?;
        self.seen.remove(&id);
        Ok(())
    }

    fn take(&mut self) -> Option<Event<Vec<u8>>> {
        let event = self.m.poll()?;
        Some(match event {
            Event::Data(id, n) => {
                let bytes = self.bytes(id);
                let seen = self.seen.entry(id).or_default();
                let got = bytes[*seen..*seen + n as usize].to_vec();
                *seen += n as usize;
                Event::Data(id, got)
            }
            e => e.map(|_| Vec::new()),
        })
    }
}

fn wire(p: &mut Peer) -> Vec<u8> {
    let mut out = vec![0; OUT];
    let mut all = Vec::new();
    loop {
        let n = p.m.pull(&mut out, &mut p.q);
        if n == 0 {
            return all;
        }
        all.extend(&out[..n]);
    }
}

fn link(a: &mut Peer, b: &mut Peer) {
    let bytes = wire(a);
    b.feed(&bytes).unwrap();
}

fn events(p: &mut Peer) -> Vec<Event<Vec<u8>>> {
    std::iter::from_fn(|| p.take()).collect()
}

fn frames(mut bytes: &[u8]) -> Vec<Frame<'_>> {
    let mut out = Vec::new();
    while let Some((f, n)) = Frame::decode(bytes).unwrap() {
        out.push(f);
        bytes = &bytes[n..];
    }
    assert!(bytes.is_empty(), "{} trailing bytes", bytes.len());
    out
}

fn encode(frames: &[Frame]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in frames {
        f.encode(&mut out);
    }
    out
}

fn tcp(flow: u32, body: Body) -> Frame {
    Frame {
        flow,
        proto: Proto::Tcp,
        body,
    }
}

fn pair(wc: u32, wd: u32) -> (Peer, Peer) {
    (
        Peer::new(Side::Client, wc, ALL, ROOM),
        Peer::new(Side::Daemon, wd, ALL, ROOM),
    )
}

fn established(c: &mut Peer, d: &mut Peer, proto: Proto) -> u32 {
    let id = c.open(proto).unwrap();
    link(c, d);
    assert_eq!(events(d), vec![Event::Open(id, proto)]);
    d.accept(id).unwrap();
    link(d, c);
    assert_eq!(events(c), vec![Event::Accepted(id)]);
    id
}

fn put(m: &mut Multi, bytes: &[u8]) {
    let s = m.spare();
    s[..bytes.len()].copy_from_slice(bytes);
    m.fill(bytes.len()).unwrap();
}

// #58
#[test]
fn ids() {
    let (mut c, mut d) = pair(10, 10);
    assert_eq!(c.open(Proto::Tcp), Ok(1));
    assert_eq!(c.open(Proto::Tcp), Ok(3));
    assert_eq!(d.open(Proto::Tcp), Ok(2));
    assert_eq!(d.open(Proto::Tcp), Ok(4));
    assert_eq!(c.len(), 2);
    assert_eq!(c.flow(1).unwrap().state(), State::Opening);
}

// #58
#[test]
fn handshake() {
    let (mut c, mut d) = pair(MAX, MAX + 10);
    let id = c.open(Proto::Udp).unwrap();
    let open = wire(&mut c);
    let want = Frame {
        flow: id,
        proto: Proto::Udp,
        body: Body::Open(MAX),
    };
    assert_eq!(open, encode(&[want]));
    d.feed(&open).unwrap();
    assert_eq!(events(&mut d), vec![Event::Open(id, Proto::Udp)]);
    assert_eq!(d.flow(id).unwrap().state(), State::Pending);
    assert_eq!(d.flow(id).unwrap().credit(), MAX);
    d.accept(id).unwrap();
    assert_eq!(d.accept(id), Err(Error::Flow(id, flow::Error::State)));
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Accepted(id)]);
    assert_eq!(c.flow(id).unwrap().credit(), MAX + 10);
    c.send(id, b"dgram").unwrap();
    c.send(id, b"").unwrap();
    link(&mut c, &mut d);
    assert_eq!(
        events(&mut d),
        vec![Event::Data(id, b"dgram".to_vec()), Event::Data(id, vec![])]
    );
}

// #58
#[test]
fn small() {
    let (mut c, mut d) = pair(MAX - 1, MAX - 1);
    assert_eq!(c.open(Proto::Udp), Err(Error::Window(MAX - 1)));
    let mut big = Peer::new(Side::Client, MAX, ALL, ROOM);
    let id = big.open(Proto::Udp).unwrap();
    link(&mut big, &mut d);
    assert_eq!(
        d.accept(id),
        Err(Error::Flow(id, flow::Error::Window(MAX - 1)))
    );
    d.reset(id, REFUSED).unwrap();
    let liar = encode(&[Frame {
        flow: 1,
        proto: Proto::Udp,
        body: Body::Open(9),
    }]);
    let mut d = Multi::new(Side::Daemon, MAX, ALL, ROOM).unwrap();
    assert_eq!(d.feed(&liar), Err(Error::Peer(1, flow::Error::Window(9))));
}

// #58
#[test]
fn refuse() {
    let (mut c, mut d) = pair(10, 10);
    let id = c.open(Proto::Tcp).unwrap();
    link(&mut c, &mut d);
    events(&mut d);
    d.reset(id, REFUSED).unwrap();
    assert!(d.is_empty());
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Reset(id, REFUSED)]);
    assert!(c.is_empty());
}

// #58
#[test]
fn codes() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.feed(&encode(&[tcp(id, Body::Reset(42))])).unwrap();
    assert_eq!(events(&mut c), vec![Event::Reset(id, ABORTED)]);
}

// #58
#[test]
fn cap() {
    let mut c = Peer::new(Side::Client, 10, ALL, ROOM);
    let mut d = Peer::new(Side::Daemon, 10, 1, ROOM);
    let a = c.open(Proto::Tcp).unwrap();
    let b = c.open(Proto::Tcp).unwrap();
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Open(a, Proto::Tcp)]);
    assert_eq!(d.len(), 1);
    assert_eq!(d.open(Proto::Tcp), Err(Error::Cap(1)));
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Reset(b, REFUSED)]);
    d.reset(a, ABORTED).unwrap();
    assert!(d.open(Proto::Tcp).is_ok());
}

// #58 "Data path"
#[test]
fn local() {
    let mut c = Peer::new(Side::Client, 10, 2, ROOM);
    let a = c.open(Proto::Tcp).unwrap();
    c.open(Proto::Tcp).unwrap();
    assert_eq!(c.open(Proto::Tcp), Err(Error::Cap(2)));
    assert_eq!(c.accept(9), Err(Error::Unknown(9)));
    assert_eq!(c.ready(9), Err(Error::Unknown(9)));
    assert_eq!(c.stop(9), Err(Error::Unknown(9)));
    assert_eq!(c.grow(9, 1), Err(Error::Unknown(9)));
    assert_eq!(c.accept(a), Err(Error::Flow(a, flow::Error::State)));
    assert_eq!(c.send(a, b"x"), Ok(()));
    let opens = wire(&mut c);
    assert!(frames(&opens)
        .iter()
        .all(|f| matches!(f.body, Body::Open(_))));
}

// #58
#[test]
fn violations() {
    let cases = [
        (encode(&[tcp(2, Body::Open(1))]), Error::Parity(2)),
        (
            encode(&[tcp(1, Body::Open(1)), tcp(1, Body::Open(1))]),
            Error::Duplicate(1),
        ),
        (
            encode(&[tcp(3, Body::Open(1)), tcp(1, Body::Open(1))]),
            Error::Duplicate(1),
        ),
        (encode(&[tcp(7, Body::Window(1))]), Error::Stray(7)),
        (encode(&[tcp(4, Body::Close)]), Error::Stray(4)),
        (
            encode(&[tcp(1, Body::Open(1)), tcp(1, Body::Data(&[0; 5]))]),
            Error::Peer(1, flow::Error::State),
        ),
        (
            vec![0xff, 6, 0, 0, 0, 1, 0, 0, 0, 0],
            Error::Frame(frame::Error::Type(0xff)),
        ),
    ];
    for (bytes, want) in cases {
        let mut d = Multi::new(Side::Daemon, 4, ALL, ROOM).unwrap();
        assert_eq!(d.feed(&bytes), Err(want.clone()), "{want:?}");
        assert_eq!(d.feed(&[]), Err(Error::Dead));
        assert_eq!(d.open(Proto::Tcp), Err(Error::Dead));
        assert!(d.done());
    }
}

// #58
#[test]
fn empty() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    let blank = encode(&[tcp(id, Body::Data(&[])), tcp(id, Body::Data(&[]))]);
    assert_eq!(d.feed(&blank), Err(Error::Frame(frame::Error::Empty)));
    assert_eq!(events(&mut d), vec![Event::Reset(id, ABORTED)]);
    assert_eq!(d.feed(&blank), Err(Error::Dead));
    assert!(events(&mut d).is_empty());
}

// #58 RFC 9113 §5.1.1
#[test]
fn retire() {
    let mut d = Peer::new(Side::Daemon, 10, ALL, ROOM);
    d.feed(&encode(&[tcp(3, Body::Open(10))])).unwrap();
    assert_eq!(events(&mut d), vec![Event::Open(3, Proto::Tcp)]);
    d.feed(&encode(&[tcp(1, Body::Close)])).unwrap();
    d.feed(&encode(&[tcp(1, Body::Window(5))])).unwrap();
    assert!(events(&mut d).is_empty());
    let late = encode(&[tcp(1, Body::Open(10))]);
    assert_eq!(d.feed(&late), Err(Error::Duplicate(1)));
}

// #58 RFC 9113 §6.9
#[test]
fn crossing() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.close(id).unwrap();
    c.feed(&encode(&[tcp(id, Body::Window(1))])).unwrap();
    assert_eq!(events(&mut c), vec![Event::Window(id, 1)]);
}

// #58 RFC 9113 §6.9.1
#[test]
fn overrun() {
    let (mut c, mut d) = pair(10, 4);
    let id = established(&mut c, &mut d, Proto::Tcp);
    let over = encode(&[tcp(id, Body::Data(&[0; 5]))]);
    assert_eq!(
        d.feed(&over),
        Err(Error::Peer(id, flow::Error::Credit { len: 5, credit: 4 }))
    );
}

// #58
#[test]
fn reap() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.close(id).unwrap();
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Closed(id)]);
    d.close(id).unwrap();
    assert!(d.is_empty());
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Closed(id)]);
    assert!(c.is_empty());
    assert_eq!(c.open(Proto::Tcp), Ok(3));
}

// #58 RFC 9113 §5.1
#[test]
fn stale() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.close(id).unwrap();
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Closed(id)]);
    d.send(id, b"abcdefgh").unwrap();
    link(&mut d, &mut c);
    events(&mut c);
    c.consume(id, 8).unwrap();
    let late = wire(&mut c);
    assert!(!late.is_empty());
    d.close(id).unwrap();
    assert!(d.is_empty());
    d.feed(&late).unwrap();
    d.feed(&encode(&[tcp(id, Body::Data(&[1]))])).unwrap();
    d.feed(&encode(&[tcp(id, Body::Reset(ABORTED))])).unwrap();
    assert!(events(&mut d).is_empty());
    let never = encode(&[tcp(5, Body::Close)]);
    assert_eq!(d.feed(&never), Err(Error::Stray(5)));
}

// #58
#[test]
fn race() {
    let mut c = Peer::new(Side::Client, 10, ALL, ROOM);
    let mut d = Peer::new(Side::Daemon, 10, 0, ROOM);
    let id = c.open(Proto::Tcp).unwrap();
    link(&mut c, &mut d);
    assert!(events(&mut d).is_empty());
    c.reset(id, ABORTED).unwrap();
    link(&mut c, &mut d);
    link(&mut d, &mut c);
    assert!(events(&mut c).is_empty());
    assert!(c.is_empty() && d.is_empty());
}

// #58
#[test]
fn exhausted() {
    let mut c = Multi::new(Side::Client, 10, ALL, ROOM).unwrap();
    c.next = Some(u32::MAX);
    assert_eq!(c.open(Proto::Tcp), Ok(u32::MAX));
    assert_eq!(c.open(Proto::Tcp), Err(Error::Exhausted));
}

// #58
#[test]
fn end() {
    let (mut c, mut d) = pair(10, 10);
    let a = established(&mut c, &mut d, Proto::Tcp);
    let b = c.open(Proto::Tcp).unwrap();
    assert_eq!(c.end(), Ok(()));
    assert_eq!(
        events(&mut c),
        vec![Event::Reset(a, ABORTED), Event::Reset(b, ABORTED)]
    );
    assert!(c.is_empty() && c.done());
    assert_eq!(c.feed(b"x"), Err(Error::Dead));
    assert_eq!(c.end(), Err(Error::Dead));

    d.feed(&[0, 6, 0]).unwrap();
    assert_eq!(d.end(), Err(Error::Truncated));
    assert_eq!(events(&mut d), vec![Event::Reset(a, ABORTED)]);
    assert!(d.done());
}

// #58
#[test]
fn fail() {
    let (mut c, mut d) = pair(10, 10);
    let a = established(&mut c, &mut d, Proto::Tcp);
    d.fail();
    assert_eq!(events(&mut d), vec![Event::Reset(a, ABORTED)]);
    assert!(d.done());
    d.fail();
    assert!(events(&mut d).is_empty());
}

// #58
#[test]
fn window() {
    let (mut c, mut d) = pair(10, 4);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.send(id, b"abcdef").unwrap();
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Data(id, b"abcd".to_vec())]);
    assert!(wire(&mut c).is_empty());
    d.consume(id, 1).unwrap();
    assert!(wire(&mut d).is_empty());
    d.flush(id).unwrap();
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Window(id, 1)]);
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Data(id, b"e".to_vec())]);
}

// #58
#[test]
fn grow() {
    let (mut c, mut d) = pair(10, 4);
    let id = established(&mut c, &mut d, Proto::Tcp);
    d.grow(id, 100).unwrap();
    d.flush(id).unwrap();
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Window(id, 96)]);
    assert_eq!(c.flow(id).unwrap().credit(), 100);
    assert_eq!(d.grow(id, 50), Err(Error::Flow(id, flow::Error::State)));
}

// #58
#[test]
fn stop() {
    let (mut c, mut d) = pair(10, 4);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.send(id, b"ab").unwrap();
    link(&mut c, &mut d);
    assert_eq!(d.used(), 2);
    c.send(id, b"cde").unwrap();
    let inflight = wire(&mut c);
    assert_eq!(frames(&inflight), vec![tcp(id, Body::Data(b"cd"))]);
    d.stop(id).unwrap();
    assert_eq!(d.used(), 0);
    assert_eq!(d.held(id).count(), 0);
    assert_eq!(d.stop(id), Err(Error::Flow(id, flow::Error::Closed)));
    d.feed(&inflight).unwrap();
    assert!(events(&mut d).is_empty());
    assert_eq!(d.used(), 0);
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Stopped(id)]);
    assert_eq!(c.ready(id), Err(Error::Flow(id, flow::Error::Closed)));
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Closed(id)]);
    d.send(id, b"back").unwrap();
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Data(id, b"back".to_vec())]);
    d.close(id).unwrap();
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Closed(id)]);
    assert!(d.is_empty());
    assert_eq!(c.len(), 1);
    c.consume(id, 4).unwrap();
    assert!(c.is_empty());
}

// #58
#[test]
fn discard() {
    let (mut c, mut d) = pair(100, 100);
    let id = established(&mut c, &mut d, Proto::Tcp);
    d.stop(id).unwrap();
    link(&mut d, &mut c);
    c.send(id, b"never").unwrap_err();
    let (mut c, mut d) = pair(100, 100);
    let id = established(&mut c, &mut d, Proto::Tcp);
    d.stop(id).unwrap();
    let stop = wire(&mut d);
    c.send(id, b"queued").unwrap();
    c.feed(&stop).unwrap();
    assert_eq!(frames(&wire(&mut c)), vec![tcp(id, Body::Close)]);
}

// #58
#[test]
fn budget() {
    let mut c = Peer::new(Side::Client, 70_000, ALL, ROOM);
    let mut d = Peer::new(Side::Daemon, 70_000, ALL, 70_000);
    let ids: Vec<u32> = (0..4)
        .map(|_| established(&mut c, &mut d, Proto::Udp))
        .collect();
    for &id in &ids {
        c.send(id, &[7; 30_000]).unwrap();
    }
    link(&mut c, &mut d);
    let got = events(&mut d);
    assert_eq!(got.len(), 2);
    assert_eq!((d.used(), d.paused()), (60_000, true));
    d.feed(&[]).unwrap();
    assert!(events(&mut d).is_empty());
    d.consume(ids[0], 30_000).unwrap();
    assert_eq!((d.used(), d.paused()), (60_000, true));
    assert_eq!(events(&mut d), vec![Event::Data(ids[2], vec![7; 30_000])]);
    d.reset(ids[1], ABORTED).unwrap();
    assert_eq!((d.used(), d.paused()), (60_000, false));
    assert_eq!(events(&mut d), vec![Event::Data(ids[3], vec![7; 30_000])]);
}

// #58
#[test]
fn floor() {
    let b = u64::from(MAX) - 1;
    let err = Multi::new(Side::Client, 10, ALL, b).unwrap_err();
    assert_eq!(err, Error::Budget(b));
    assert!(Multi::new(Side::Client, 10, ALL, u64::from(MAX)).is_ok());
}

fn spent() -> (Peer, Peer, u32) {
    let mut c = Peer::new(Side::Client, 30_000, ALL, ROOM);
    let mut d = Peer::new(Side::Daemon, 30_000, ALL, 70_000);
    let ids: Vec<u32> = (0..3)
        .map(|_| established(&mut c, &mut d, Proto::Tcp))
        .collect();
    for &id in &ids[..2] {
        c.send(id, &[1; 30_000]).unwrap();
    }
    let b = ids[2];
    link(&mut c, &mut d);
    events(&mut d);
    assert_eq!(d.used(), 60_000);
    (c, d, b)
}

// #58
#[test]
fn poison() {
    let (_, mut d, b) = spent();
    let mut head = vec![0x00, 0x06];
    head.extend(b.to_be_bytes());
    head.extend(70_000u32.to_be_bytes());
    assert_eq!(
        d.feed(&head),
        Err(Error::Frame(frame::Error::Oversize(70_000)))
    );
    assert!(!d.paused() && d.done());
}

// #58
#[test]
fn guilty() {
    let (mut c, mut d, b) = spent();
    c.close(b).unwrap();
    link(&mut c, &mut d);
    events(&mut d);
    let late = encode(&[tcp(b, Body::Data(&[2; 20_000]))]);
    assert_eq!(d.feed(&late), Err(Error::Peer(b, flow::Error::Closed)));

    let (_, mut d, b) = spent();
    let over = encode(&[tcp(b, Body::Data(&[2; 35_000]))]);
    let credit = flow::Error::Credit {
        len: 35_000,
        credit: 30_000,
    };
    assert_eq!(d.feed(&over), Err(Error::Peer(b, credit)));

    let (_, mut d, b) = spent();
    let fair = encode(&[tcp(b, Body::Data(&[2; 20_000]))]);
    assert_eq!(d.feed(&fair), Ok(()));
    assert!(d.paused() && !d.done());
}

// #58
#[test]
fn torn() {
    let (mut c, mut d, b) = spent();
    c.send(b, &[3; 20_000]).unwrap();
    let bytes = wire(&mut c);
    d.feed(&bytes[..HEADER + 100]).unwrap();
    assert!(d.paused());
    assert_eq!(d.end(), Err(Error::Truncated));

    let (mut c, mut d, b) = spent();
    c.send(b, &[3; 20_000]).unwrap();
    let bytes = wire(&mut c);
    d.feed(&bytes).unwrap();
    assert!(d.paused());
    assert_eq!(d.end(), Ok(()));
}

// #58
#[test]
fn opening() {
    let (mut c, mut d) = pair(10, 10);
    let id = c.open(Proto::Tcp).unwrap();
    c.grow(id, 50).unwrap();
    link(&mut c, &mut d);
    events(&mut d);
    d.accept(id).unwrap();
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Accepted(id)]);
    let bytes = wire(&mut c);
    let grant = frames(&bytes);
    assert_eq!(grant, vec![tcp(id, Body::Window(40))]);
    d.feed(&encode(&grant)).unwrap();
    assert_eq!(d.flow(id).unwrap().credit(), 50);
}

// #58
#[test]
fn dead() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.fail();
    assert_eq!(c.accept(id), Err(Error::Dead));
    assert_eq!(c.ready(id), Err(Error::Dead));
    assert_eq!(c.consume(id, 0), Err(Error::Dead));
    assert_eq!(c.flush(id), Err(Error::Dead));
    assert_eq!(c.close(id), Err(Error::Dead));
    assert_eq!(c.reset(id, ABORTED), Err(Error::Dead));
    assert_eq!(c.stop(id), Err(Error::Dead));
    assert_eq!(c.grow(id, 20), Err(Error::Dead));
    assert_eq!(c.open(Proto::Tcp), Err(Error::Dead));
    assert!(wire(&mut c).is_empty());
}

// #58
#[test]
fn unanswered() {
    let mut c = Peer::new(Side::Client, 10, ALL, ROOM);
    let mut d = Peer::new(Side::Daemon, 10, 1, ROOM);
    c.tick(0);
    d.tick(0);
    let id = established(&mut c, &mut d, Proto::Tcp);
    d.tick(500);
    d.stop(id).unwrap();
    wire(&mut d);
    d.expire(559, 60);
    assert!(events(&mut d).is_empty());
    d.expire(560, 60);
    assert_eq!(events(&mut d), vec![Event::Reset(id, ABORTED)]);
    assert_eq!(frames(&wire(&mut d)), vec![tcp(id, Body::Reset(ABORTED))]);
    assert!(d.is_empty());
    assert!(d.open(Proto::Tcp).is_ok());
}

// #58
#[test]
fn refund() {
    for end in 0..4 {
        let (mut c, mut d) = pair(100, 100);
        let id = established(&mut c, &mut d, Proto::Tcp);
        c.send(id, &[1; 30]).unwrap();
        link(&mut c, &mut d);
        assert_eq!(d.used(), 30);
        match end {
            0 => d.reset(id, ABORTED).unwrap(),
            1 => {
                c.reset(id, ABORTED).unwrap();
                link(&mut c, &mut d);
            }
            2 => {
                c.close(id).unwrap();
                link(&mut c, &mut d);
                d.close(id).unwrap();
                assert_eq!((d.used(), d.len()), (30, 1));
                assert_eq!(d.bytes(id), vec![1; 30]);
                d.consume(id, 30).unwrap();
                assert!(d.is_empty());
            }
            _ => d.fail(),
        }
        assert_eq!(d.used(), 0, "end {end}");
        assert_eq!(d.held(id).count(), 0, "end {end}");
    }
}

// #58
#[test]
fn expire() {
    let mut c = Peer::new(Side::Client, 10, 4, ROOM);
    let mut d = Peer::new(Side::Daemon, 10, ALL, ROOM);
    c.tick(0);
    d.tick(0);
    let open = established(&mut c, &mut d, Proto::Tcp);
    let half = established(&mut c, &mut d, Proto::Tcp);
    let pending = d.open(Proto::Tcp).unwrap();
    link(&mut d, &mut c);
    events(&mut c);
    let opening = c.open(Proto::Tcp).unwrap();
    c.tick(50);
    c.close(half).unwrap();
    wire(&mut c);
    assert_eq!(c.len(), 4);
    assert_eq!(c.open(Proto::Tcp), Err(Error::Cap(4)));
    c.expire(99, 100);
    assert!(events(&mut c).is_empty());
    c.expire(100, 100);
    assert_eq!(
        events(&mut c),
        vec![
            Event::Reset(pending, ABORTED),
            Event::Reset(opening, ABORTED)
        ]
    );
    let bytes = wire(&mut c);
    let resets: Vec<Frame> = frames(&bytes);
    assert!(resets.contains(&tcp(pending, Body::Reset(ABORTED))));
    assert!(resets.contains(&tcp(opening, Body::Reset(ABORTED))));
    c.expire(150, 100);
    assert_eq!(events(&mut c), vec![Event::Reset(half, ABORTED)]);
    assert_eq!(c.ids().collect::<Vec<_>>(), vec![open]);
    assert!(c.open(Proto::Tcp).is_ok());
}

// #58 "Data path"
#[test]
fn schedule() {
    let (mut c, mut d) = pair(1 << 20, 1 << 20);
    let busy = established(&mut c, &mut d, Proto::Tcp);
    let quiet = established(&mut c, &mut d, Proto::Tcp);
    let doomed = established(&mut c, &mut d, Proto::Tcp);
    c.send(busy, &vec![1; 4 * MAX as usize]).unwrap();
    c.send(quiet, b"hi").unwrap();
    c.reset(doomed, ABORTED).unwrap();
    let mut out = vec![0; 1 << 20];
    let n = c.m.pull(&mut out, &mut c.q);
    let sent = frames(&out[..n]);
    assert_eq!(sent[0], tcp(doomed, Body::Reset(ABORTED)));
    assert_eq!(sent[1].flow, busy);
    assert_eq!(sent[2], tcp(quiet, Body::Data(b"hi")));
    let slice = 1 << 14;
    let total: usize = sent[1..]
        .iter()
        .filter(|f| f.flow == busy)
        .map(|f| match f.body {
            Body::Data(b) => {
                assert!(b.len() <= slice);
                b.len()
            }
            b => panic!("{b:?}"),
        })
        .sum();
    assert_eq!(total, 4 * MAX as usize);
}

// #58 "Data path"
#[test]
fn bound() {
    let (mut c, mut d) = pair(1 << 20, 1 << 20);
    let ids: Vec<u32> = (0..5)
        .map(|_| established(&mut c, &mut d, Proto::Tcp))
        .collect();
    for (i, &id) in ids.iter().enumerate() {
        c.send(id, &vec![i as u8; (i + 1) * 40_000]).unwrap();
    }
    let all = wire(&mut c);
    let order: Vec<u32> = frames(&all).iter().map(|f| f.flow).collect();
    for &id in &ids {
        let at: Vec<usize> = order
            .iter()
            .enumerate()
            .filter(|(_, &f)| f == id)
            .map(|(k, _)| k)
            .collect();
        let live =
            |k: usize| ids.iter().filter(|&&o| order[k..].contains(&o)).count();
        for w in at.windows(2) {
            assert!(
                w[1] - w[0] <= live(w[0]),
                "flow {id} waited {}",
                w[1] - w[0]
            );
        }
    }
}

// #58 "Data path"
#[test]
fn finish() {
    let (mut c, mut d) = pair(100, 100);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.send(id, b"last").unwrap();
    c.finish(id).unwrap();
    let bytes = wire(&mut c);
    assert_eq!(
        frames(&bytes),
        vec![tcp(id, Body::Data(b"last")), tcp(id, Body::Close)]
    );
    assert_eq!(c.ready(id), Err(Error::Flow(id, flow::Error::Closed)));
    d.feed(&bytes).unwrap();
    assert_eq!(
        events(&mut d),
        vec![Event::Data(id, b"last".to_vec()), Event::Closed(id)]
    );
}

// #58 "Data path"
#[test]
fn broke() {
    let (mut c, mut d) = pair(100, 100);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.ready(id).unwrap();
    let mut out = vec![0; OUT];
    let n = c.m.pull(&mut out, &mut |_, _: &mut [u8]| Pull::Fail);
    assert_eq!(frames(&out[..n]), vec![tcp(id, Body::Reset(ABORTED))]);
    assert!(c.is_empty());
}

// #58 "Data path"
#[test]
fn overclaim() {
    let (mut c, mut d) = pair(8, 8);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.ready(id).unwrap();
    let mut out = vec![0; OUT];
    let n =
        c.m.pull(&mut out, &mut |_, b: &mut [u8]| Pull::Bytes(b.len() + 1));
    assert_eq!(frames(&out[..n]), vec![tcp(id, Body::Reset(ABORTED))]);
}

// #58 "Data path"
#[test]
fn nothing() {
    let (mut c, mut d) = pair(100, 100);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.ready(id).unwrap();
    let calls = Cell::new(0);
    let mut out = vec![0; OUT];
    let mut zero = |_: u32, _: &mut [u8]| {
        calls.set(calls.get() + 1);
        Pull::Bytes(0)
    };
    assert_eq!(c.m.pull(&mut out, &mut zero), 0);
    assert_eq!(c.m.pull(&mut out, &mut zero), 0);
    assert_eq!(calls.get(), 1);
    c.ready(id).unwrap();
    assert_eq!(c.m.pull(&mut out, &mut zero), 0);
    assert_eq!(calls.get(), 2);
}

// #58 "Data path"
#[test]
fn readiness() {
    let (mut c, mut d) = pair(100, 100);
    let id = c.open(Proto::Tcp).unwrap();
    link(&mut c, &mut d);
    events(&mut d);
    assert_eq!(d.ready(id), Err(Error::Flow(id, flow::Error::State)));
    d.accept(id).unwrap();
    assert_eq!(d.ready(id), Ok(()));
    d.close(id).unwrap();
    assert_eq!(d.ready(id), Err(Error::Flow(id, flow::Error::Closed)));
    link(&mut d, &mut c);
    events(&mut c);
    c.reset(id, ABORTED).unwrap();
    assert_eq!(c.ready(id), Err(Error::Unknown(id)));
}

// #58 "Data path"
#[test]
fn room() {
    let (mut c, mut d) = pair(MAX, MAX);
    let big = established(&mut c, &mut d, Proto::Udp);
    let other = established(&mut c, &mut d, Proto::Tcp);
    c.send(big, &[9; 1000]).unwrap();
    c.send(other, b"x").unwrap();
    let mut out = vec![0; 500];
    let n = c.m.pull(&mut out, &mut c.q);
    assert_eq!(frames(&out[..n]), vec![tcp(other, Body::Data(b"x"))]);
    c.send(other, b"y").unwrap();
    let mut out = vec![0; OUT];
    let n = c.m.pull(&mut out, &mut c.q);
    let sent = frames(&out[..n]);
    assert_eq!(sent[0].flow, big);
    assert_eq!(sent[1], tcp(other, Body::Data(b"y")));
}

// #58 "Data path"
#[test]
fn starved() {
    let (mut c, mut d) = pair(MAX, MAX);
    let id = established(&mut c, &mut d, Proto::Udp);
    c.send(id, &vec![1; MAX as usize - 100]).unwrap();
    c.send(id, &[2; 1000]).unwrap();
    link(&mut c, &mut d);
    assert_eq!(c.flow(id).unwrap().credit(), 100);
    let calls = Cell::new(0);
    let mut out = vec![0; OUT];
    let mut count = |id: u32, buf: &mut [u8]| {
        calls.set(calls.get() + 1);
        c.q.pull(id, buf)
    };
    assert_eq!(c.m.pull(&mut out, &mut count), 0);
    assert_eq!(c.m.pull(&mut out, &mut count), 0);
    assert!(
        calls.get() <= 1,
        "pulled {} times without new credit",
        calls.get()
    );
    let got = events(&mut d);
    assert_eq!(got.len(), 1);
    d.consume(id, MAX - 100).unwrap();
    d.flush(id).unwrap();
    link(&mut d, &mut c);
    events(&mut c);
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Data(id, vec![2; 1000])]);
}

// #58 "Receiving lends"
#[test]
fn place() {
    let (mut c, mut d) = pair(MAX, MAX);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.send(id, b"in place").unwrap();
    let bytes = wire(&mut c);
    let s = d.spare();
    s[..bytes.len()].copy_from_slice(&bytes);
    let at = s.as_ptr() as usize;
    d.fill(bytes.len()).unwrap();
    assert_eq!(d.poll(), Some(Event::Data(id, 8)));
    let lent: Vec<&[u8]> = d.held(id).collect();
    assert_eq!(lent, vec![&b"in place"[..]]);
    assert_eq!(lent[0].as_ptr() as usize, at + HEADER);
}

// #58 "Receiving lends"
#[test]
fn held() {
    let (mut c, mut d) = pair(MAX, MAX);
    let id = established(&mut c, &mut d, Proto::Tcp);
    for chunk in [&b"first"[..], b"second"] {
        c.send(id, chunk).unwrap();
        let bytes = wire(&mut c);
        put(&mut d, &bytes);
    }
    assert_eq!(
        events(&mut d),
        vec![
            Event::Data(id, b"first".to_vec()),
            Event::Data(id, b"second".to_vec())
        ]
    );
}

// #58 "Receiving lends"
#[test]
fn lend() {
    let (mut c, mut d) = pair(MAX, MAX);
    let a = established(&mut c, &mut d, Proto::Tcp);
    let b = established(&mut c, &mut d, Proto::Tcp);
    c.send(a, b"hold me").unwrap();
    link(&mut c, &mut d);
    assert_eq!(d.poll(), Some(Event::Data(a, 7)));
    for _ in 0..8 {
        c.send(b, &[5; 5000]).unwrap();
        c.send(a, b"!").unwrap();
        link(&mut c, &mut d);
        while let Some(e) = d.poll() {
            if let Event::Data(id, n) = e {
                if id == b {
                    d.m.consume(b, n).unwrap();
                }
            }
        }
    }
    assert_eq!(d.bytes(a), b"hold me!!!!!!!!".to_vec());
    assert!(d.held(a).count() > 1);
    d.m.consume(a, 5).unwrap();
    assert_eq!(d.bytes(a), b"me!!!!!!!!".to_vec());
    let over = flow::Error::Consume { n: 11, held: 10 };
    assert_eq!(d.m.consume(a, 11), Err(Error::Flow(a, over)));
    d.m.consume(a, 10).unwrap();
    assert_eq!(d.held(a).count(), 0);
}

// #58 "Receiving lends"
#[test]
fn unseen() {
    let (mut c, mut d) = pair(1 << 30, 1 << 30);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.send(id, &vec![3; 1 << 20]).unwrap();
    let bytes = wire(&mut c);
    let first = d.spare().as_ptr() as usize;
    let mut at = 0;
    while at < bytes.len() {
        let s = d.spare();
        if s.is_empty() {
            break;
        }
        let n = s.len().min(bytes.len() - at);
        s[..n].copy_from_slice(&bytes[at..at + n]);
        d.fill(n).unwrap();
        at += n;
    }
    assert!(at < bytes.len(), "the buffer never filled");
    assert!(d.spare().is_empty());
    let lent = d.held(id).next().unwrap().as_ptr() as usize;
    assert_eq!(lent, first + HEADER);
    let mut seen = 0;
    while let Some(Event::Data(_, n)) = d.poll() {
        seen += n;
    }
    d.m.consume(id, seen).unwrap();
    assert!(!d.spare().is_empty());
    assert_eq!(d.spare().as_ptr() as usize, first);
}

// #58 "A holding flow never stalls another"
#[test]
fn spill() {
    let window = 100_000;
    let (mut c, mut d) = pair(1 << 30, window);
    let hold = established(&mut c, &mut d, Proto::Tcp);
    let flow = established(&mut c, &mut d, Proto::Tcp);
    let kept: Vec<u8> = (0..window).map(|i| (i % 251) as u8).collect();
    c.send(hold, &kept).unwrap();
    link(&mut c, &mut d);
    let mut moved = 0;
    for round in 0..64u8 {
        c.send(flow, &vec![round; 60_000]).unwrap();
        let bytes = wire(&mut c);
        let mut at = 0;
        while at < bytes.len() {
            let s = d.spare();
            assert!(!s.is_empty(), "no room in round {round}");
            let n = s.len().min(bytes.len() - at);
            s[..n].copy_from_slice(&bytes[at..at + n]);
            d.fill(n).unwrap();
            at += n;
            while let Some(e) = d.poll() {
                if let Event::Data(id, n) = e {
                    if id == flow {
                        moved += n as usize;
                        d.m.consume(flow, n).unwrap();
                        d.flush(flow).unwrap();
                    }
                }
            }
        }
        link(&mut d, &mut c);
        events(&mut c);
    }
    assert_eq!(moved, 64 * 60_000);
    assert_eq!(d.bytes(hold), kept);
    assert_eq!(d.used(), u64::from(window));
}

// #58 "Receiving lends"
#[test]
fn stopped() {
    let (mut c, mut d) = pair(MAX, MAX);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.send(id, b"gone").unwrap();
    link(&mut c, &mut d);
    d.m.stop(id).unwrap();
    assert_eq!(d.held(id).count(), 0);
    assert_eq!(d.poll(), None);
}

// #58
#[test]
fn many() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    let n = 200_000;
    let burst = encode(&vec![tcp(id, Body::Window(1)); n]);
    let start = std::time::Instant::now();
    c.feed(&burst).unwrap();
    assert_eq!(events(&mut c).len(), n);
    let spent = start.elapsed();
    assert!(spent.as_secs() < 5, "{n} frames took {spent:?}"); // linear is well under 1s in debug; the quadratic parser took ~20s
}

// #58
#[test]
fn backpressure() {
    let mut c = Peer::new(Side::Client, MAX, ALL, ROOM);
    let mut d = Peer::new(Side::Daemon, MAX, ALL, u64::from(MAX));
    let ids: Vec<u32> = (0..8)
        .map(|_| established(&mut c, &mut d, Proto::Udp))
        .collect();
    for (i, &id) in ids.iter().enumerate() {
        c.send(id, &[i as u8; MAX as usize]).unwrap();
    }
    let mut bytes = wire(&mut c);
    loop {
        let s = d.spare();
        let n = s.len().min(bytes.len());
        if n == 0 {
            break;
        }
        s[..n].copy_from_slice(&bytes[..n]);
        d.fill(n).unwrap();
        bytes.drain(..n);
    }
    assert!(d.paused());
    assert!(!bytes.is_empty());
    assert_eq!(
        events(&mut d),
        vec![Event::Data(ids[0], vec![0; MAX as usize])]
    );
    d.consume(ids[0], MAX).unwrap();
    d.fill(0).unwrap();
    assert_eq!(
        events(&mut d),
        vec![Event::Data(ids[1], vec![1; MAX as usize])]
    );
}

fn datagrams() -> impl Strategy<Value = Vec<usize>> {
    prop::collection::vec(
        prop_oneof![0usize..64, 0usize..=MAX as usize, Just(MAX as usize)],
        1..40,
    )
}

proptest! {
    // #58 "Receiving lends"
    #[test]
    fn pieces(sizes in datagrams(), reads in prop::collection::vec(1usize..70_000, 1..16)) {
        let total: usize = sizes.iter().sum();
        let window = u32::try_from(total).unwrap().max(MAX);
        let (mut c, mut d) = pair(window, window);
        let id = established(&mut c, &mut d, Proto::Udp);
        let mut sent = Vec::new();
        for (i, &n) in sizes.iter().enumerate() {
            let payload: Vec<u8> = (0..n).map(|k| (i + k) as u8).collect();
            prop_assert_eq!(c.send(id, &payload), Ok(()));
            sent.push(payload);
        }
        let bytes = wire(&mut c);
        let mut got = Vec::new();
        let mut at = 0;
        let mut turn = 0;
        while at < bytes.len() {
            let s = d.spare();
            prop_assert!(!s.is_empty(), "no room at {} of {}", at, bytes.len());
            let n = s.len().min(reads[turn % reads.len()]).min(bytes.len() - at);
            turn += 1;
            s[..n].copy_from_slice(&bytes[at..at + n]);
            at += n;
            d.fill(n).unwrap();
            got.extend(events(&mut d));
        }
        let want: Vec<_> = sent.into_iter().map(|p| Event::Data(id, p)).collect();
        prop_assert_eq!(got, want);
    }
}

#[derive(Debug, Clone)]
enum Op {
    Open(bool, bool),
    Accept(bool, usize),
    Send(bool, usize, usize),
    Deliver(bool, usize, bool),
    Consume(bool, usize, u32),
    Close(bool, usize),
    Reset(bool, usize),
    Stop(bool, usize),
    Grow(bool, usize, u32),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        1 => (any::<bool>(), any::<bool>()).prop_map(|(s, u)| Op::Open(s, u)),
        1 => (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Accept(s, i)),
        3 => (any::<bool>(), any::<usize>(), 0usize..200)
            .prop_map(|(s, i, n)| Op::Send(s, i, n)),
        4 => (any::<bool>(), 0usize..96, any::<bool>())
            .prop_map(|(s, n, p)| Op::Deliver(s, n, p)),
        2 => (any::<bool>(), any::<usize>(), 0u32..200)
            .prop_map(|(s, i, n)| Op::Consume(s, i, n)),
        1 => (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Close(s, i)),
        1 => (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Reset(s, i)),
        1 => (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Stop(s, i)),
        1 => (any::<bool>(), any::<usize>(), 0u32..100)
            .prop_map(|(s, i, n)| Op::Grow(s, i, n)),
    ]
}

fn windows() -> impl Strategy<Value = u32> {
    prop_oneof![0u32..300, MAX..MAX + 300]
}

fn pick(m: &Multi, i: usize) -> Option<u32> {
    let ids: Vec<u32> = m.ids().collect();
    (!ids.is_empty()).then(|| ids[i % ids.len()])
}

struct Net {
    p: [Peer; 2],
    wire: [Vec<u8>; 2],
    got: [BTreeMap<u32, Vec<u8>>; 2],
}

impl Net {
    fn new(wc: u32, wd: u32, budget: u64) -> Self {
        Self {
            p: [
                Peer::new(Side::Client, wc, ALL, budget),
                Peer::new(Side::Daemon, wd, ALL, budget),
            ],
            wire: Default::default(),
            got: Default::default(),
        }
    }

    fn pump(&mut self) {
        for me in 0..2 {
            let bytes = wire(&mut self.p[me]);
            self.wire[1 - me].extend(bytes);
        }
    }

    fn deliver(
        &mut self,
        me: usize,
        n: usize,
        place: bool,
    ) -> Result<(), TestCaseError> {
        let n = n.min(self.wire[me].len());
        match place {
            true => {
                prop_assert_eq!(self.p[me].fill(0), Ok(()));
                let mut left = n;
                loop {
                    self.collect(me);
                    let s = self.p[me].spare();
                    let k = s.len().min(left);
                    if k == 0 {
                        break;
                    }
                    s[..k].copy_from_slice(&self.wire[me][..k]);
                    self.wire[me].drain(..k);
                    left -= k;
                    let r = self.p[me].fill(k);
                    prop_assert_eq!(r, Ok(()));
                }
            }
            false => {
                let chunk: Vec<u8> = self.wire[me].drain(..n).collect();
                let r = self.p[me].feed(&chunk);
                prop_assert_eq!(r, Ok(()));
            }
        }
        self.collect(me);
        Ok(())
    }

    fn collect(&mut self, me: usize) {
        while let Some(ev) = self.p[me].take() {
            if let Event::Data(id, d) = ev {
                self.got[me].entry(id).or_default().extend(d);
            }
        }
    }

    fn consume(&mut self) {
        for me in 0..2 {
            let ids: Vec<u32> = self.p[me].ids().collect();
            for id in ids {
                let held = self.p[me].flow(id).unwrap().held();
                self.p[me].consume(id, held).unwrap();
            }
        }
    }

    fn drain(&mut self) -> Result<(), TestCaseError> {
        loop {
            self.consume();
            self.pump();
            if self.wire.iter().all(Vec::is_empty)
                && !self.p.iter().any(|p| p.paused())
            {
                return Ok(());
            }
            for me in 0..2 {
                self.deliver(me, usize::MAX, me == 0)?;
            }
        }
    }

    fn budget(&self, budget: u64) -> Result<(), TestCaseError> {
        for p in &self.p {
            let held: u64 = p
                .ids()
                .map(|id| u64::from(p.flow(id).unwrap().held()))
                .sum();
            prop_assert_eq!(p.used(), held);
            prop_assert!(p.used() <= budget);
            for id in p.ids() {
                let lent: usize = p.held(id).map(<[u8]>::len).sum();
                prop_assert_eq!(lent, p.flow(id).unwrap().held() as usize);
            }
        }
        Ok(())
    }
}

proptest! {
    // #58
    #[test]
    fn honest(
        wc in windows(),
        wd in windows(),
        budget in prop_oneof![u64::from(MAX)..u64::from(MAX) + 2000, Just(ROOM)],
        ops in prop::collection::vec(op(), 0..300),
    ) {
        let mut net = Net::new(wc, wd, budget);
        let mut seq = 0u8;
        for op in ops {
            match op {
                Op::Open(s, udp) => {
                    let proto = if udp { Proto::Udp } else { Proto::Tcp };
                    match net.p[usize::from(s)].open(proto) {
                        Ok(_) | Err(Error::Window(_)) => {}
                        Err(e) => prop_assert!(false, "open: {e}"),
                    }
                }
                Op::Accept(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.p[me], i) else { continue };
                    match net.p[me].accept(id) {
                        Ok(()) | Err(Error::Flow(_, flow::Error::State)) => {}
                        Err(Error::Flow(_, flow::Error::Window(_))) => {
                            net.p[me].reset(id, REFUSED).unwrap();
                        }
                        Err(e) => prop_assert!(false, "accept: {e}"),
                    }
                }
                Op::Send(s, i, n) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.p[me], i) else { continue };
                    let payload: Vec<u8> = (0..n).map(|_| { seq = seq.wrapping_add(1); seq }).collect();
                    match net.p[me].send(id, &payload) {
                        Ok(()) | Err(Error::Flow(_, flow::Error::Closed | flow::Error::State | flow::Error::Stopped)) => {}
                        Err(e) => prop_assert!(false, "send: {e}"),
                    }
                }
                Op::Deliver(s, n, p) => net.deliver(usize::from(s), n, p)?,
                Op::Consume(s, i, n) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.p[me], i) else { continue };
                    let n = n.min(net.p[me].flow(id).unwrap().held());
                    net.p[me].consume(id, n).unwrap();
                }
                Op::Close(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.p[me], i) else { continue };
                    match net.p[me].close(id) {
                        Ok(()) | Err(Error::Flow(_, flow::Error::Closed | flow::Error::State)) => {}
                        Err(e) => prop_assert!(false, "close: {e}"),
                    }
                }
                Op::Reset(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.p[me], i) else { continue };
                    net.p[me].reset(id, ABORTED).unwrap();
                }
                Op::Stop(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.p[me], i) else { continue };
                    match net.p[me].stop(id) {
                        Ok(()) | Err(Error::Flow(_, flow::Error::Closed | flow::Error::State)) => {}
                        Err(e) => prop_assert!(false, "stop: {e}"),
                    }
                }
                Op::Grow(s, i, n) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.p[me], i) else { continue };
                    let w = net.p[me].flow(id).unwrap().window().saturating_add(n);
                    net.p[me].grow(id, w).unwrap();
                }
            }
            net.pump();
            net.budget(budget)?;
        }
        for me in 0..2 {
            for (id, bytes) in &net.got[me] {
                let from = net.p[1 - me].q.taken.get(id).map(Vec::as_slice).unwrap_or(&[]);
                prop_assert!(from.starts_with(bytes), "flow {}", id);
            }
        }
        for _ in 0..3 {
            net.drain()?;
            for me in 0..2 {
                let ids: Vec<u32> = net.p[me].ids().collect();
                for id in ids {
                    if net.p[me].accept(id).is_err() && net.p[me].flow(id).unwrap().state() == State::Pending {
                        net.p[me].reset(id, REFUSED).unwrap();
                        continue;
                    }
                    let _ = net.p[me].close(id);
                }
            }
        }
        net.drain()?;
        net.budget(budget)?;
        prop_assert!(net.p[0].is_empty(), "client left {:?}", net.p[0].ids().collect::<Vec<_>>());
        prop_assert!(net.p[1].is_empty(), "daemon left {:?}", net.p[1].ids().collect::<Vec<_>>());
    }

    // #58
    #[test]
    fn isolation(
        window in 1u32..200,
        mask in prop::collection::vec(any::<bool>(), 1..6),
        target in 1usize..600,
    ) {
        let mut net = Net::new(window, window, ROOM);
        let mut ids = Vec::new();
        for _ in &mask {
            let id = net.p[0].open(Proto::Tcp).unwrap();
            ids.push(id);
        }
        net.pump();
        net.deliver(1, usize::MAX, true)?;
        for &id in &ids {
            net.p[1].accept(id).unwrap();
        }
        net.pump();
        net.deliver(0, usize::MAX, false)?;
        let payload: Vec<u8> = (0..target).map(|i| i as u8).collect();
        for &id in &ids {
            net.p[0].send(id, &payload).unwrap();
        }
        for _ in 0..(4 * target + 16) {
            for (&id, &live) in ids.iter().zip(&mask) {
                if live {
                    let held = net.p[1].flow(id).unwrap().held();
                    net.p[1].consume(id, held).unwrap();
                    net.p[1].flush(id).unwrap();
                }
            }
            net.pump();
            for me in 0..2 {
                net.deliver(me, usize::MAX, me == 1)?;
            }
        }
        for (&id, &live) in ids.iter().zip(&mask) {
            let got = net.got[1].get(&id).map_or(0, Vec::len);
            if live {
                prop_assert_eq!(got, target, "live flow {}", id);
            } else {
                prop_assert_eq!(got, (window as usize).min(target), "stalled flow {}", id);
            }
        }
    }
}

// #58 "Receiving lends"
#[test]
fn resume() {
    let mut c = Peer::new(Side::Client, MAX, ALL, ROOM);
    let mut d = Peer::new(Side::Daemon, MAX, ALL, u64::from(MAX));
    let a = established(&mut c, &mut d, Proto::Udp);
    let b = established(&mut c, &mut d, Proto::Udp);
    c.send(a, &[1; 40_000]).unwrap();
    c.send(b, &[2; 30_000]).unwrap();
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Data(a, vec![1; 40_000])]);
    assert!(d.paused());
    d.consume(a, 40_000).unwrap();
    assert!(!d.paused());
    assert_eq!(events(&mut d), vec![Event::Data(b, vec![2; 30_000])]);
}
