use super::*;
use crate::frame::MAX;
use proptest::prelude::*;
use std::collections::VecDeque;

fn pair(proto: Proto, a: u32, b: u32) -> (Flow, Flow) {
    let (mut opener, open) = Flow::open(1, proto, a).unwrap();
    let mut acceptor = Flow::offer(&open, b).unwrap();
    let accept = acceptor.accept().unwrap();
    assert_eq!(opener.receive(accept), Ok(Event::Accepted));
    (opener, acceptor)
}

fn frame(proto: Proto, body: Body<'_>) -> Frame<'_> {
    Frame {
        flow: 1,
        proto,
        body,
    }
}

fn data(proto: Proto, d: &[u8]) -> Frame<'_> {
    frame(proto, Body::Data(d))
}

// #58 — RFC 4254 §5.1: each side states its own initial receive window
#[test]
fn handshake() {
    let (mut opener, open) = Flow::open(1, Proto::Tcp, 100).unwrap();
    assert_eq!(open, frame(Proto::Tcp, Body::Open(100)));
    assert_eq!(opener.state(), State::Opening);
    assert_eq!((opener.credit(), opener.granted()), (0, 100));

    let mut acceptor = Flow::offer(&open, 40).unwrap();
    assert_eq!(acceptor.state(), State::Pending);
    assert_eq!((acceptor.credit(), acceptor.granted()), (100, 40));
    let accept = acceptor.accept().unwrap();
    assert_eq!(accept, frame(Proto::Tcp, Body::Accept(40)));
    assert_eq!(acceptor.state(), State::Open);
    assert_eq!(acceptor.accept().unwrap_err(), Error::State);

    assert_eq!(opener.receive(accept), Ok(Event::Accepted));
    assert_eq!(opener.credit(), 40);
    assert_eq!(opener.state(), State::Open);
}

// #58
#[test]
fn offer() {
    let close = frame(Proto::Tcp, Body::Close);
    assert_eq!(Flow::offer(&close, 1).unwrap_err(), Error::State);
}

// #58 — a datagram flow's window holds at least one maximum datagram
#[test]
fn small() {
    let w = MAX - 1;
    assert_eq!(Flow::open(1, Proto::Udp, w).unwrap_err(), Error::Window(w));
    assert!(Flow::open(1, Proto::Tcp, 0).is_ok());
    let open = frame(Proto::Udp, Body::Open(w));
    assert_eq!(Flow::offer(&open, MAX).unwrap_err(), Error::Window(w));
    let open = frame(Proto::Udp, Body::Open(MAX));
    let mut acceptor = Flow::offer(&open, w).unwrap();
    assert_eq!(acceptor.accept().unwrap_err(), Error::Window(w));
    let (mut opener, _) = Flow::open(1, Proto::Udp, MAX).unwrap();
    let accept = frame(Proto::Udp, Body::Accept(w));
    assert_eq!(opener.receive(accept), Err(Error::Window(w)));
}

// #58 — RFC 4254 §5.1: until the acceptor answers, neither side may write
#[test]
fn unanswered() {
    let (mut opener, open) = Flow::open(1, Proto::Tcp, 100).unwrap();
    assert_eq!(opener.send(b"x").unwrap_err(), Error::Blocked);
    assert_eq!(opener.close().unwrap_err(), Error::State);
    assert_eq!(opener.stop().unwrap_err(), Error::State);
    let mut acceptor = Flow::offer(&open, 10).unwrap();
    assert_eq!(acceptor.send(b"x").unwrap_err(), Error::State);
    assert_eq!(acceptor.close().unwrap_err(), Error::State);
    let d = data(Proto::Tcp, b"x");
    assert_eq!(acceptor.receive(d), Err(Error::State));
    assert_eq!(opener.receive(d), Err(Error::State));
    let stop = frame(Proto::Tcp, Body::Stop);
    assert_eq!(acceptor.receive(stop), Err(Error::State));
    assert_eq!(opener.receive(stop), Err(Error::State));
}

// #58 — an opener may abandon its open, and an acceptor may refuse it
#[test]
fn refuse() {
    let (mut opener, open) = Flow::open(1, Proto::Udp, MAX).unwrap();
    let mut acceptor = Flow::offer(&open, MAX).unwrap();
    let rst = acceptor.reset(REFUSED).unwrap();
    assert_eq!(rst, frame(Proto::Udp, Body::Reset(REFUSED)));
    assert!(acceptor.done());
    assert_eq!(opener.receive(rst), Ok(Event::Reset(REFUSED)));
    assert!(opener.done());

    let (mut opener, open) = Flow::open(1, Proto::Tcp, 10).unwrap();
    let mut acceptor = Flow::offer(&open, 10).unwrap();
    let rst = opener.reset(ABORTED).unwrap();
    assert_eq!(acceptor.receive(rst), Ok(Event::Reset(ABORTED)));
    assert!(acceptor.done());
}

// #58 — a received RESET code other than refused reads as aborted
#[test]
fn codes() {
    for (code, want) in
        [(0, REFUSED), (1, ABORTED), (2, ABORTED), (42, ABORTED)]
    {
        let (_, mut b) = pair(Proto::Tcp, 10, 10);
        let rst = frame(Proto::Tcp, Body::Reset(code));
        assert_eq!(b.receive(rst), Ok(Event::Reset(want)), "{code}");
    }
}

// #58 — a stream splits at the credit edge
#[test]
fn split() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 4);
    assert_eq!(a.send(b"abcdef"), Ok(4));
    assert_eq!(a.credit(), 0);
    assert_eq!(a.send(b"ef").unwrap_err(), Error::Blocked);
    let got = b.receive(data(Proto::Tcp, b"abcd"));
    assert_eq!(got, Ok(Event::Data(4)));
    assert_eq!((b.granted(), b.held()), (0, 4));
}

// #58 — a stream chunk never exceeds one frame
#[test]
fn chunk() {
    let (mut a, _) = pair(Proto::Tcp, 1, u32::MAX);
    let big = vec![0u8; MAX as usize + 10];
    assert_eq!(a.send(&big), Ok(MAX as usize));
}

// #58 — a datagram is never split: it waits for its whole length in credit
#[test]
fn datagram() {
    let (mut a, _) = pair(Proto::Udp, MAX, MAX + 4);
    assert_eq!(a.send(&vec![0u8; MAX as usize]), Ok(MAX as usize));
    assert_eq!(a.credit(), 4);
    assert_eq!(a.send(b"abcde").unwrap_err(), Error::Blocked);
    assert_eq!(a.send(b"abcd"), Ok(4));
    assert_eq!(a.send(b""), Ok(0));
    let big = vec![0u8; MAX as usize + 1];
    assert_eq!(a.send(&big).unwrap_err(), Error::Oversize(big.len()));
}

// #58
#[test]
fn empty() {
    let (mut a, _) = pair(Proto::Tcp, 10, 10);
    assert_eq!(a.send(b"").unwrap_err(), Error::Empty);
}

// #58 — RFC 9113 §6.9.1: a peer that overruns its credit is an error
#[test]
fn overrun() {
    let (_, mut b) = pair(Proto::Tcp, 10, 4);
    let f = data(Proto::Tcp, b"abcde");
    assert_eq!(b.receive(f), Err(Error::Credit { len: 5, credit: 4 }));
    assert_eq!((b.granted(), b.held()), (4, 0));
}

// #58 — consumption replenishes at half the window, by exactly what was freed
#[test]
fn replenish() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    assert_eq!(a.send(b"abcdefgh"), Ok(8));
    b.receive(data(Proto::Tcp, b"abcdefgh")).unwrap();
    assert_eq!(b.consume(4), Ok(None));
    assert_eq!((b.granted(), b.held(), b.freed()), (2, 4, 4));
    let w = b.consume(1).unwrap().unwrap();
    assert_eq!(w, frame(Proto::Tcp, Body::Window(5)));
    assert_eq!((b.granted(), b.held(), b.freed()), (7, 3, 0));
    assert_eq!(a.receive(w), Ok(Event::Window(5)));
    assert_eq!(a.credit(), 7);
}

// #58
#[test]
fn overconsume() {
    let (_, mut b) = pair(Proto::Tcp, 10, 10);
    assert_eq!(b.consume(1), Err(Error::Consume { n: 1, held: 0 }));
}

// #58
#[test]
fn flush() {
    let (mut a, mut b) = pair(Proto::Tcp, 100, 100);
    a.send(b"ab").unwrap();
    b.receive(data(Proto::Tcp, b"ab")).unwrap();
    assert_eq!(b.consume(2), Ok(None));
    assert_eq!(b.flush(), Some(frame(Proto::Tcp, Body::Window(2))));
    assert_eq!(b.flush(), None);
}

// #58 — a receiver may enlarge its window by over-granting; it never shrinks
#[test]
fn grow() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    assert_eq!(b.grow(30), Ok(()));
    assert_eq!((b.window(), b.granted(), b.freed()), (30, 10, 20));
    let w = b.flush().unwrap();
    assert_eq!(w, frame(Proto::Tcp, Body::Window(20)));
    assert_eq!(a.receive(w), Ok(Event::Window(20)));
    assert_eq!(a.credit(), 30);
    assert_eq!(b.grow(29), Err(Error::State));
    assert_eq!(b.grow(30), Ok(()));
    assert_eq!(b.flush(), None);
}

// #58 — growing before the ACCEPT goes out enlarges the initial grant
#[test]
fn early() {
    let (_, open) = Flow::open(1, Proto::Tcp, 10).unwrap();
    let mut b = Flow::offer(&open, 10).unwrap();
    b.grow(25).unwrap();
    assert_eq!((b.granted(), b.freed()), (25, 0));
    assert_eq!(b.accept(), Ok(frame(Proto::Tcp, Body::Accept(25))));
}

// #58 — RFC 4254 §5.2: the window never exceeds 2^32-1
#[test]
fn overflow() {
    let (mut a, _) = pair(Proto::Tcp, 1, u32::MAX);
    let w = frame(Proto::Tcp, Body::Window(1));
    assert_eq!(a.receive(w), Err(Error::Overflow));
    assert_eq!(a.credit(), u32::MAX);
    let (_, mut b) = pair(Proto::Tcp, u32::MAX, 1);
    assert_eq!(b.grow(u32::MAX), Ok(()));
}

// #58 — half-close each way, then done (yamux FIN, RFC 4254 §5.3)
#[test]
fn halfclose() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    let fin = a.close().unwrap();
    assert_eq!(fin.body, Body::Close);
    assert_eq!(a.send(b"x").unwrap_err(), Error::Closed);
    assert_eq!(a.close().unwrap_err(), Error::Closed);
    assert!(!a.done());
    assert_eq!(b.receive(fin), Ok(Event::Closed));
    assert_eq!(b.receive(fin), Err(Error::State));
    let late = data(Proto::Tcp, b"x");
    assert_eq!(b.receive(late), Err(Error::Closed));
    assert_eq!(b.send(b"still"), Ok(5));
    let got = a.receive(data(Proto::Tcp, b"still"));
    assert_eq!(got, Ok(Event::Data(5)));
    let fin = b.close().unwrap();
    assert!(b.done());
    assert_eq!(a.receive(fin), Ok(Event::Closed));
    assert!(a.done());
}

// #58 — RFC 9113 §6.9: a WINDOW may cross or follow this side's CLOSE
#[test]
fn crossing() {
    let (mut a, _) = pair(Proto::Tcp, 10, 10);
    a.close().unwrap();
    let w = frame(Proto::Tcp, Body::Window(3));
    assert_eq!(a.receive(w), Ok(Event::Window(3)));
}

// #58 — a closed receiver stops granting: no WINDOW after the peer's CLOSE
#[test]
fn drained() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    a.send(b"ab").unwrap();
    b.receive(data(Proto::Tcp, b"ab")).unwrap();
    b.receive(a.close().unwrap()).unwrap();
    assert_eq!(b.consume(2), Ok(None));
    assert_eq!(b.flush(), None);
}

// #58 — QUIC STOP_SENDING: a receiver stops reading one direction; data
// already in flight is discarded but still charged against its credit
#[test]
fn stop() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    a.send(b"abcd").unwrap();
    b.receive(data(Proto::Tcp, b"abcd")).unwrap();
    let stop = b.stop().unwrap();
    assert_eq!(stop, frame(Proto::Tcp, Body::Stop));
    assert_eq!((b.held(), b.granted()), (0, 6));
    assert_eq!(b.stop().unwrap_err(), Error::Closed);
    assert_eq!(b.consume(1), Err(Error::Consume { n: 1, held: 0 }));
    let late = b.receive(data(Proto::Tcp, b"efg"));
    assert_eq!(late, Ok(Event::Dropped(3)));
    assert_eq!((b.held(), b.granted(), b.flush()), (0, 3, None));
    let over = data(Proto::Tcp, b"hijk");
    assert_eq!(b.receive(over), Err(Error::Credit { len: 4, credit: 3 }));
    assert_eq!(b.send(b"other way"), Ok(9));
    b.receive(frame(Proto::Tcp, Body::Close)).unwrap();
    b.close().unwrap();
    assert!(b.done());
}

// #58 — the stopped sender gets a terminal result, never Blocked
#[test]
fn stopped() {
    let (mut a, _) = pair(Proto::Tcp, 10, 4);
    a.send(b"abcd").unwrap();
    assert_eq!(a.send(b"e").unwrap_err(), Error::Blocked);
    let stop = frame(Proto::Tcp, Body::Stop);
    assert_eq!(a.receive(stop), Ok(Event::Stopped));
    assert_eq!(a.send(b"e").unwrap_err(), Error::Stopped);
    assert_eq!(a.receive(stop), Err(Error::State));
    assert_eq!(a.close(), Ok(frame(Proto::Tcp, Body::Close)));
    assert_eq!(a.send(b"e").unwrap_err(), Error::Closed);

    let (mut a, _) = pair(Proto::Tcp, 10, 10);
    a.close().unwrap();
    let stop = frame(Proto::Tcp, Body::Stop);
    assert_eq!(a.receive(stop), Ok(Event::Stopped));
}

// #58 — a flow left opening, pending or half-closed past a deadline is
// overdue; an open or finished one never is
#[test]
fn overdue() {
    let (mut opener, open) = Flow::open(1, Proto::Tcp, 10).unwrap();
    opener.mark(100);
    assert!(!opener.overdue(149, 50));
    assert!(opener.overdue(150, 50));
    let mut acceptor = Flow::offer(&open, 10).unwrap();
    acceptor.mark(100);
    assert!(acceptor.overdue(150, 50));
    let accept = acceptor.accept().unwrap();
    acceptor.mark(120);
    assert!(!acceptor.overdue(1000, 50));
    opener.receive(accept).unwrap();
    let fin = opener.close().unwrap();
    opener.mark(200);
    assert!(!opener.overdue(249, 50));
    assert!(opener.overdue(250, 50));
    acceptor.receive(fin).unwrap();
    acceptor.mark(200);
    assert!(acceptor.overdue(250, 50));
    acceptor.close().unwrap();
    assert!(!acceptor.overdue(10_000, 50));

    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    b.stop().unwrap();
    b.mark(300);
    assert!(!b.overdue(349, 50));
    assert!(b.overdue(350, 50));
    b.receive(a.close().unwrap()).unwrap();
    b.mark(400);
    assert!(b.overdue(450, 50));
}

// #58
#[test]
fn reset() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    let rst = a.reset(ABORTED).unwrap();
    assert_eq!(rst.body, Body::Reset(ABORTED));
    assert!(a.done());
    assert_eq!(a.send(b"x").unwrap_err(), Error::Closed);
    assert_eq!(a.reset(ABORTED).unwrap_err(), Error::Closed);
    assert_eq!(b.receive(rst), Ok(Event::Reset(ABORTED)));
    assert!(b.done());
    assert_eq!(b.state(), State::Reset);
    assert_eq!(b.receive(rst), Err(Error::Closed));
}

// #58
#[test]
fn violations() {
    let (_, mut b) = pair(Proto::Tcp, 10, 10);
    let wrong = frame(Proto::Udp, Body::Window(1));
    assert_eq!(b.receive(wrong), Err(Error::Proto));
    let again = frame(Proto::Tcp, Body::Accept(1));
    assert_eq!(b.receive(again), Err(Error::State));
    let open = frame(Proto::Tcp, Body::Open(1));
    assert_eq!(b.receive(open), Err(Error::State));
    let other = Frame {
        flow: 3,
        proto: Proto::Tcp,
        body: Body::Close,
    };
    assert_eq!(b.receive(other), Err(Error::State));
}

// #58
#[test]
fn display() {
    let e = Error::Credit { len: 5, credit: 4 };
    assert_eq!(e.to_string(), "peer sent 5 bytes against 4 of credit");
}

#[derive(Debug, Clone)]
enum Op {
    Send(bool, usize),
    Deliver(bool),
    Consume(bool, u32),
    Flush(bool),
    Grow(bool, u32),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<bool>(), 0usize..300).prop_map(|(s, n)| Op::Send(s, n)),
        any::<bool>().prop_map(Op::Deliver),
        (any::<bool>(), 0u32..300).prop_map(|(s, n)| Op::Consume(s, n)),
        any::<bool>().prop_map(Op::Flush),
        (any::<bool>(), 0u32..100).prop_map(|(s, n)| Op::Grow(s, n)),
    ]
}

struct End {
    flow: Flow,
    inbox: VecDeque<Vec<u8>>,
    sent: Vec<u8>,
    got: Vec<u8>,
}

fn encode(f: &Frame) -> Vec<u8> {
    let mut out = Vec::new();
    f.encode(&mut out);
    out
}

fn decode(bytes: &[u8]) -> Frame<'_> {
    Frame::decode(bytes).unwrap().unwrap().0
}

fn inflight(q: &VecDeque<Vec<u8>>) -> (u64, u64) {
    q.iter().fold((0, 0), |(d, w), b| match &decode(b).body {
        Body::Data(x) => (d + x.len() as u64, w),
        Body::Window(n) => (d, w + u64::from(*n)),
        _ => (d, w),
    })
}

proptest! {
    // #58 — for an honest pair: a sender never exceeds credit, the receiver
    // never rejects, granted+held+freed stays the window, and credit is
    // conserved across the wire (updates replenish exactly)
    #[test]
    fn credit(
        udp in any::<bool>(),
        wa in 0u32..400,
        wb in 0u32..400,
        ops in prop::collection::vec(op(), 0..200),
    ) {
        let (proto, base, scale) = match udp {
            true => (Proto::Udp, MAX, 300),
            false => (Proto::Tcp, 0, 1),
        };
        let (fa, fb) = pair(proto, base + wa, base + wb);
        let mut ends = [
            End { flow: fa, inbox: VecDeque::new(), sent: vec![], got: vec![] },
            End { flow: fb, inbox: VecDeque::new(), sent: vec![], got: vec![] },
        ];
        let mut seq = 0u8;
        for op in ops {
            match op {
                Op::Send(s, n) => {
                    let (me, peer) = (usize::from(s), usize::from(!s));
                    let payload: Vec<u8> = (0..n * scale).map(|_| { seq = seq.wrapping_add(1); seq }).collect();
                    let before = ends[me].flow.credit();
                    match ends[me].flow.send(&payload) {
                        Ok(used) => {
                            prop_assert!(used as u64 <= u64::from(before));
                            prop_assert_eq!(ends[me].flow.credit(), before - used as u32);
                            if proto == Proto::Udp {
                                prop_assert_eq!(used, payload.len());
                            }
                            ends[me].sent.extend(&payload[..used]);
                            ends[peer].inbox.push_back(encode(&data(proto, &payload[..used])));
                        }
                        Err(Error::Blocked) => {
                            prop_assert!(
                                before == 0
                                    || (proto == Proto::Udp && payload.len() > before as usize)
                            );
                        }
                        Err(Error::Empty) => prop_assert!(payload.is_empty() && proto == Proto::Tcp),
                        Err(Error::Oversize(_)) => prop_assert!(payload.len() > MAX as usize),
                        Err(e) => prop_assert!(false, "send: {e}"),
                    }
                }
                Op::Deliver(s) => {
                    let me = usize::from(s);
                    if let Some(b) = ends[me].inbox.pop_front() {
                        let f = decode(&b);
                        match ends[me].flow.receive(f).unwrap() {
                            Event::Data(_) => {
                                let Body::Data(d) = f.body else { unreachable!() };
                                ends[me].got.extend(d);
                            }
                            Event::Window(_) => {}
                            e => prop_assert!(false, "event: {e:?}"),
                        }
                    }
                }
                Op::Consume(s, n) => {
                    let (me, peer) = (usize::from(s), usize::from(!s));
                    let n = n.min(ends[me].flow.held());
                    if let Some(w) = ends[me].flow.consume(n).unwrap() {
                        ends[peer].inbox.push_back(encode(&w));
                    }
                }
                Op::Flush(s) => {
                    let (me, peer) = (usize::from(s), usize::from(!s));
                    if let Some(w) = ends[me].flow.flush() {
                        ends[peer].inbox.push_back(encode(&w));
                    }
                }
                Op::Grow(s, n) => {
                    let me = usize::from(s);
                    let w = ends[me].flow.window().saturating_add(n);
                    ends[me].flow.grow(w).unwrap();
                }
            }
            for me in 0..2 {
                let peer = 1 - me;
                let f = &ends[me].flow;
                prop_assert_eq!(
                    u64::from(f.granted()) + u64::from(f.held()) + u64::from(f.freed()),
                    u64::from(f.window())
                );
                let (data, _) = inflight(&ends[me].inbox);
                let (_, window) = inflight(&ends[peer].inbox);
                prop_assert_eq!(
                    u64::from(ends[peer].flow.credit()) + data + window,
                    u64::from(f.granted())
                );
            }
        }
        prop_assert!(ends[1].sent.starts_with(&ends[0].got));
        prop_assert!(ends[0].sent.starts_with(&ends[1].got));
    }

    // #58 — a window update adds exactly its increment, or fails and adds nothing
    #[test]
    fn update(start in 0u32..=u32::MAX, inc in 1u32..=u32::MAX) {
        let (mut a, _) = pair(Proto::Tcp, 1, start);
        let w = frame(Proto::Tcp, Body::Window(inc));
        match start.checked_add(inc) {
            Some(sum) => {
                prop_assert_eq!(a.receive(w), Ok(Event::Window(inc)));
                prop_assert_eq!(a.credit(), sum);
            }
            None => {
                prop_assert_eq!(a.receive(w), Err(Error::Overflow));
                prop_assert_eq!(a.credit(), start);
            }
        }
    }
}
