use super::*;
use crate::flow::{self, State, ABORTED, REFUSED};
use crate::frame::{self, Body, Frame};
use proptest::prelude::*;
use std::collections::BTreeMap;

fn wire(m: &mut Multi) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(t) = m.transmit() {
        match t {
            Transmit::Bytes(b) => out.extend(b),
            t => panic!("multi-channel never emits {t:?}"),
        }
    }
    out
}

fn link(a: &mut Multi, b: &mut Multi) {
    let bytes = wire(a);
    b.feed(&bytes).unwrap();
}

fn events(m: &mut Multi) -> Vec<Event> {
    std::iter::from_fn(|| m.poll()).collect()
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

fn pair(wc: u32, wd: u32) -> (Multi, Multi) {
    (
        Multi::new(Side::Client, wc, None),
        Multi::new(Side::Daemon, wd, None),
    )
}

fn established(c: &mut Multi, d: &mut Multi, proto: Proto) -> u32 {
    let id = c.open(proto).unwrap();
    link(c, d);
    assert_eq!(events(d), vec![Event::Open(id, proto)]);
    d.accept(id).unwrap();
    link(d, c);
    assert_eq!(events(c), vec![Event::Accepted(id)]);
    id
}

// #58 — yamux §StreamID: one parity per side, 0 reserved
#[test]
fn ids() {
    let (mut c, mut d) = pair(10, 10);
    assert_eq!(c.open(Proto::Tcp), Ok(1));
    assert_eq!(c.open(Proto::Udp), Ok(3));
    assert_eq!(d.open(Proto::Tcp), Ok(2));
    assert_eq!(d.open(Proto::Tcp), Ok(4));
    assert_eq!(c.len(), 2);
    assert_eq!(c.flow(1).unwrap().state(), State::Opening);
}

// #58 — an OPEN surfaces as an event; the caller accepts it with this end's window
#[test]
fn handshake() {
    let (mut c, mut d) = pair(10, 20);
    let id = c.open(Proto::Udp).unwrap();
    let open = wire(&mut c);
    let want = Frame {
        flow: id,
        proto: Proto::Udp,
        body: Body::Open(10),
    };
    assert_eq!(open, encode(&[want]));
    d.feed(&open).unwrap();
    assert_eq!(events(&mut d), vec![Event::Open(id, Proto::Udp)]);
    assert_eq!(d.flow(id).unwrap().state(), State::Pending);
    assert_eq!(d.flow(id).unwrap().credit(), 10);
    d.accept(id).unwrap();
    assert_eq!(d.accept(id), Err(Error::Flow(id, flow::Error::State)));
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Accepted(id)]);
    assert_eq!(c.flow(id).unwrap().credit(), 20);
    assert_eq!(c.send(id, b"dgram"), Ok(5));
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Data(id, b"dgram".to_vec())]);
}

// #58 — the acceptor may refuse; the opener sees a RESET(0)
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

// #58 — socat `max-children` is the cap; peer opens beyond it are refused
#[test]
fn cap() {
    let mut c = Multi::new(Side::Client, 10, None);
    let mut d = Multi::new(Side::Daemon, 10, Some(1));
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

// #58
#[test]
fn local() {
    let mut c = Multi::new(Side::Client, 10, Some(2));
    let a = c.open(Proto::Tcp).unwrap();
    c.open(Proto::Udp).unwrap();
    assert_eq!(c.open(Proto::Tcp), Err(Error::Cap(2)));
    assert_eq!(c.accept(9), Err(Error::Unknown(9)));
    assert_eq!(c.send(9, b"x"), Err(Error::Unknown(9)));
    assert_eq!(c.accept(a), Err(Error::Flow(a, flow::Error::State)));
    assert_eq!(c.send(a, b"x"), Err(Error::Flow(a, flow::Error::Blocked)));
}

// #58 — every peer violation is fatal to the tunnel, and stays fatal
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
            encode(&[tcp(1, Body::Open(1)), tcp(1, Body::Data(vec![0; 5]))]),
            Error::Peer(1, flow::Error::State),
        ),
        (
            vec![0xff, 6, 0, 0, 0, 1, 0, 0, 0, 0],
            Error::Frame(frame::Error::Type(0xff)),
        ),
    ];
    for (bytes, want) in cases {
        let mut d = Multi::new(Side::Daemon, 4, None);
        assert_eq!(d.feed(&bytes), Err(want.clone()), "{want:?}");
        assert_eq!(d.feed(&[]), Err(Error::Dead));
        assert_eq!(d.open(Proto::Tcp), Err(Error::Dead));
        assert!(d.done());
    }
}

// #58 — RFC 9113 §6.9.1: DATA beyond the granted credit
#[test]
fn overrun() {
    let (mut c, mut d) = pair(10, 4);
    let id = established(&mut c, &mut d, Proto::Tcp);
    let over = encode(&[tcp(id, Body::Data(vec![0; 5]))]);
    assert_eq!(
        d.feed(&over),
        Err(Error::Peer(id, flow::Error::Credit { len: 5, credit: 4 }))
    );
}

// #58 — a finished flow leaves the table; its id is never reused
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

// #58 — RFC 9113 §5.1 "closed": frames racing a flow's teardown are ignored
#[test]
fn stale() {
    let (mut c, mut d) = pair(10, 10);
    let id = established(&mut c, &mut d, Proto::Tcp);
    c.close(id).unwrap();
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Closed(id)]);
    d.send(id, b"abcdefgh").unwrap();
    link(&mut d, &mut c);
    c.consume(id, 8).unwrap();
    let late = wire(&mut c);
    assert!(!late.is_empty());
    d.close(id).unwrap();
    assert!(d.is_empty());
    d.feed(&late).unwrap();
    d.feed(&encode(&[tcp(id, Body::Data(vec![1]))])).unwrap();
    d.feed(&encode(&[tcp(id, Body::Reset(ABORTED))])).unwrap();
    assert!(events(&mut d).is_empty());
    let never = encode(&[tcp(5, Body::Close)]);
    assert_eq!(d.feed(&never), Err(Error::Stray(5)));
}

// #58 — a refused id is past, so the opener's racing frames are stale
#[test]
fn race() {
    let mut c = Multi::new(Side::Client, 10, None);
    let mut d = Multi::new(Side::Daemon, 10, Some(0));
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
    let mut c = Multi::new(Side::Client, 10, None);
    c.next = Some(u32::MAX);
    assert_eq!(c.open(Proto::Tcp), Ok(u32::MAX));
    assert_eq!(c.open(Proto::Tcp), Err(Error::Exhausted));
}

// #58 — tunnel EOF resets every live flow; a torn frame is an error
#[test]
fn end() {
    let (mut c, mut d) = pair(10, 10);
    let a = established(&mut c, &mut d, Proto::Tcp);
    let b = c.open(Proto::Udp).unwrap();
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

// #58 — a failed tunnel resets every live flow
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

// #58 — a window update surfaces so a blocked writer can retry
#[test]
fn window() {
    let (mut c, mut d) = pair(10, 4);
    let id = established(&mut c, &mut d, Proto::Tcp);
    assert_eq!(c.send(id, b"abcdef"), Ok(4));
    assert_eq!(
        c.send(id, b"ef"),
        Err(Error::Flow(id, flow::Error::Blocked))
    );
    link(&mut c, &mut d);
    assert_eq!(events(&mut d), vec![Event::Data(id, b"abcd".to_vec())]);
    d.consume(id, 1).unwrap();
    assert!(wire(&mut d).is_empty());
    d.flush(id).unwrap();
    link(&mut d, &mut c);
    assert_eq!(events(&mut c), vec![Event::Window(id, 1)]);
    assert_eq!(c.send(id, b"ef"), Ok(1));
}

#[derive(Debug, Clone)]
enum Op {
    Open(bool, bool),
    Accept(bool, usize),
    Send(bool, usize, usize),
    Deliver(bool, usize),
    Consume(bool, usize, u32),
    Close(bool, usize),
    Reset(bool, usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        1 => (any::<bool>(), any::<bool>()).prop_map(|(s, u)| Op::Open(s, u)),
        1 => (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Accept(s, i)),
        3 => (any::<bool>(), any::<usize>(), 0usize..200)
            .prop_map(|(s, i, n)| Op::Send(s, i, n)),
        4 => (any::<bool>(), 0usize..96).prop_map(|(s, n)| Op::Deliver(s, n)),
        2 => (any::<bool>(), any::<usize>(), 0u32..200)
            .prop_map(|(s, i, n)| Op::Consume(s, i, n)),
        1 => (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Close(s, i)),
        1 => (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Reset(s, i)),
    ]
}

fn pick(m: &Multi, i: usize) -> Option<u32> {
    let ids: Vec<u32> = m.ids().collect();
    (!ids.is_empty()).then(|| ids[i % ids.len()])
}

struct Net {
    m: [Multi; 2],
    wire: [Vec<u8>; 2],
    sent: [BTreeMap<u32, Vec<u8>>; 2],
    got: [BTreeMap<u32, Vec<u8>>; 2],
}

impl Net {
    fn new(wc: u32, wd: u32) -> Self {
        let (c, d) = pair(wc, wd);
        Self {
            m: [c, d],
            wire: Default::default(),
            sent: Default::default(),
            got: Default::default(),
        }
    }

    fn pump(&mut self) {
        for me in 0..2 {
            let bytes = wire(&mut self.m[me]);
            self.wire[1 - me].extend(bytes);
        }
    }

    fn deliver(&mut self, me: usize, n: usize) -> Result<(), TestCaseError> {
        let n = n.min(self.wire[me].len());
        let chunk: Vec<u8> = self.wire[me].drain(..n).collect();
        let r = self.m[me].feed(&chunk);
        prop_assert_eq!(r, Ok(()));
        while let Some(ev) = self.m[me].poll() {
            if let Event::Data(id, d) = ev {
                self.got[me].entry(id).or_default().extend(d);
            }
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<(), TestCaseError> {
        loop {
            self.pump();
            if self.wire.iter().all(Vec::is_empty) {
                return Ok(());
            }
            for me in 0..2 {
                self.deliver(me, usize::MAX)?;
            }
        }
    }
}

proptest! {
    // #58 — two honest muxes over one tunnel read in arbitrary pieces: no
    // protocol error ever, no sender exceeds its credit, every flow's bytes
    // arrive in order with no cross-flow mixing, and closing everything
    // empties both flow tables
    #[test]
    fn honest(
        wc in 0u32..300,
        wd in 0u32..300,
        ops in prop::collection::vec(op(), 0..300),
    ) {
        let mut net = Net::new(wc, wd);
        let mut seq = 0u8;
        for op in ops {
            match op {
                Op::Open(s, udp) => {
                    let proto = if udp { Proto::Udp } else { Proto::Tcp };
                    net.m[usize::from(s)].open(proto).unwrap();
                }
                Op::Accept(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.m[me], i) else { continue };
                    match net.m[me].accept(id) {
                        Ok(()) | Err(Error::Flow(_, flow::Error::State)) => {}
                        Err(e) => prop_assert!(false, "accept: {e}"),
                    }
                }
                Op::Send(s, i, n) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.m[me], i) else { continue };
                    let payload: Vec<u8> = (0..n).map(|_| { seq = seq.wrapping_add(1); seq }).collect();
                    let before = net.m[me].flow(id).unwrap().credit();
                    match net.m[me].send(id, &payload) {
                        Ok(used) => {
                            prop_assert!(used as u64 <= u64::from(before));
                            net.sent[me].entry(id).or_default().extend(&payload[..used]);
                        }
                        Err(Error::Flow(_, flow::Error::Blocked | flow::Error::Empty | flow::Error::Closed | flow::Error::State)) => {}
                        Err(e) => prop_assert!(false, "send: {e}"),
                    }
                }
                Op::Deliver(s, n) => net.deliver(usize::from(s), n)?,
                Op::Consume(s, i, n) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.m[me], i) else { continue };
                    let n = n.min(net.m[me].flow(id).unwrap().held());
                    net.m[me].consume(id, n).unwrap();
                }
                Op::Close(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.m[me], i) else { continue };
                    match net.m[me].close(id) {
                        Ok(()) | Err(Error::Flow(_, flow::Error::Closed | flow::Error::State)) => {}
                        Err(e) => prop_assert!(false, "close: {e}"),
                    }
                }
                Op::Reset(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&net.m[me], i) else { continue };
                    net.m[me].reset(id, ABORTED).unwrap();
                }
            }
            net.pump();
        }
        for me in 0..2 {
            for (id, bytes) in &net.got[me] {
                let from = net.sent[1 - me].get(id).map(Vec::as_slice).unwrap_or(&[]);
                prop_assert!(from.starts_with(bytes), "flow {}", id);
            }
        }
        for _ in 0..3 {
            net.drain()?;
            for me in 0..2 {
                let ids: Vec<u32> = net.m[me].ids().collect();
                for id in ids {
                    let _ = net.m[me].accept(id);
                    let _ = net.m[me].close(id);
                }
            }
        }
        net.drain()?;
        prop_assert!(net.m[0].is_empty(), "client left {:?}", net.m[0].ids().collect::<Vec<_>>());
        prop_assert!(net.m[1].is_empty(), "daemon left {:?}", net.m[1].ids().collect::<Vec<_>>());
    }

    // #58 — flow isolation: a receiver that never drains some flows stalls
    // only those; every other flow delivers all it is given, and a stalled
    // flow never receives more than the window it was granted
    #[test]
    fn isolation(
        window in 1u32..200,
        mask in prop::collection::vec(any::<bool>(), 1..6),
        target in 1usize..600,
    ) {
        let mut net = Net::new(window, window);
        let mut ids = Vec::new();
        for _ in &mask {
            let id = net.m[0].open(Proto::Tcp).unwrap();
            ids.push(id);
        }
        net.drain()?;
        for &id in &ids {
            net.m[1].accept(id).unwrap();
        }
        net.drain()?;
        let payload: Vec<u8> = (0..target).map(|i| i as u8).collect();
        for _ in 0..(4 * target + 16) {
            for (&id, &live) in ids.iter().zip(&mask) {
                let at = net.sent[0].get(&id).map_or(0, Vec::len);
                if at < target {
                    if let Ok(n) = net.m[0].send(id, &payload[at..]) {
                        net.sent[0].entry(id).or_default().extend(&payload[at..at + n]);
                    }
                }
                if live {
                    let held = net.m[1].flow(id).unwrap().held();
                    net.m[1].consume(id, held).unwrap();
                    net.m[1].flush(id).unwrap();
                }
            }
            net.drain()?;
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
