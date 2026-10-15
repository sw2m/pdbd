use super::*;
use crate::frame::MAX;
use proptest::prelude::*;
use std::collections::VecDeque;

fn pair(proto: Proto, a: u32, b: u32) -> (Flow, Flow) {
    let (mut opener, open) = Flow::open(1, proto, a);
    let (acceptor, accept) = Flow::accept(&open, b).unwrap();
    assert_eq!(opener.receive(accept), Ok(Event::Accepted));
    (opener, acceptor)
}

fn data(f: &Frame) -> &[u8] {
    match &f.body {
        Body::Data(d) => d,
        b => panic!("not data: {b:?}"),
    }
}

// #19 — RFC 4254 §5.1: each side states its own initial receive window
#[test]
fn handshake_grants_both_windows() {
    let (mut opener, open) = Flow::open(1, Proto::Tcp, 100);
    assert_eq!(
        open,
        Frame {
            flow: 1,
            proto: Proto::Tcp,
            body: Body::Open(100)
        }
    );
    assert_eq!(opener.state(), State::Opening);
    assert_eq!(opener.credit(), 0);
    assert_eq!(opener.granted(), 100);

    let (acceptor, accept) = Flow::accept(&open, 40).unwrap();
    assert_eq!(
        accept,
        Frame {
            flow: 1,
            proto: Proto::Tcp,
            body: Body::Accept(40)
        }
    );
    assert_eq!(acceptor.credit(), 100);
    assert_eq!(acceptor.granted(), 40);
    assert_eq!(acceptor.state(), State::Open);

    assert_eq!(opener.receive(accept), Ok(Event::Accepted));
    assert_eq!(opener.credit(), 40);
    assert_eq!(opener.state(), State::Open);
}

// #19
#[test]
fn accept_needs_an_open() {
    let f = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Close,
    };
    assert_eq!(Flow::accept(&f, 1).unwrap_err(), Error::State);
}

// #19 — no credit before the peer accepts
#[test]
fn opener_blocks_until_accepted() {
    let (mut f, _) = Flow::open(1, Proto::Tcp, 100);
    assert_eq!(f.send(b"x").unwrap_err(), Error::Blocked);
}

// #19 — a stream splits at the credit edge
#[test]
fn stream_splits_at_credit() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 4);
    let (frame, n) = a.send(b"abcdef").unwrap();
    assert_eq!((data(&frame), n), (&b"abcd"[..], 4));
    assert_eq!(a.credit(), 0);
    assert_eq!(a.send(b"ef").unwrap_err(), Error::Blocked);
    assert_eq!(b.receive(frame), Ok(Event::Data(b"abcd".to_vec())));
    assert_eq!((b.granted(), b.held()), (0, 4));
}

// #19 — a stream chunk never exceeds one frame
#[test]
fn stream_splits_at_frame() {
    let (mut a, _) = pair(Proto::Tcp, 1, u32::MAX);
    let big = vec![0u8; MAX as usize + 10];
    let (frame, n) = a.send(&big).unwrap();
    assert_eq!((data(&frame).len(), n), (MAX as usize, MAX as usize));
}

// #19 — a datagram is never split: it waits for credit
#[test]
fn datagram_is_atomic() {
    let (mut a, _) = pair(Proto::Udp, 10, 4);
    assert_eq!(a.send(b"abcde").unwrap_err(), Error::Blocked);
    assert_eq!(a.credit(), 4);
    let (frame, n) = a.send(b"abcd").unwrap();
    assert_eq!((data(&frame), n), (&b"abcd"[..], 4));
    let (frame, n) = a.send(b"").unwrap();
    assert_eq!((data(&frame), n), (&b""[..], 0));
    let big = vec![0u8; MAX as usize + 1];
    assert_eq!(a.send(&big).unwrap_err(), Error::Oversize(big.len()));
}

// #19
#[test]
fn empty_stream_write_is_an_error() {
    let (mut a, _) = pair(Proto::Tcp, 10, 10);
    assert_eq!(a.send(b"").unwrap_err(), Error::Empty);
}

// #19 — RFC 9113 §6.9.1: a peer that overruns its credit is a flow-control error
#[test]
fn overrun_is_rejected() {
    let (_, mut b) = pair(Proto::Tcp, 10, 4);
    let f = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Data(b"abcde".to_vec()),
    };
    assert_eq!(b.receive(f), Err(Error::Credit { len: 5, credit: 4 }));
    assert_eq!((b.granted(), b.held()), (4, 0));
}

// #19 — consumption replenishes at half the window, by exactly what was freed
#[test]
fn consume_replenishes() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    let (f, _) = a.send(b"abcdefgh").unwrap();
    b.receive(f).unwrap();
    assert_eq!(b.consume(4), Ok(None));
    assert_eq!((b.granted(), b.held(), b.freed()), (2, 4, 4));
    let w = b.consume(1).unwrap().unwrap();
    assert_eq!(
        w,
        Frame {
            flow: 1,
            proto: Proto::Tcp,
            body: Body::Window(5)
        }
    );
    assert_eq!((b.granted(), b.held(), b.freed()), (7, 3, 0));
    assert_eq!(a.receive(w), Ok(Event::Window(5)));
    assert_eq!(a.credit(), 7);
}

// #19
#[test]
fn consume_more_than_held() {
    let (_, mut b) = pair(Proto::Tcp, 10, 10);
    assert_eq!(b.consume(1), Err(Error::Consume { n: 1, held: 0 }));
}

// #19
#[test]
fn flush_sends_any_freed_credit() {
    let (mut a, mut b) = pair(Proto::Tcp, 100, 100);
    let (f, _) = a.send(b"ab").unwrap();
    b.receive(f).unwrap();
    assert_eq!(b.consume(2), Ok(None));
    assert_eq!(
        b.flush(),
        Some(Frame {
            flow: 1,
            proto: Proto::Tcp,
            body: Body::Window(2)
        })
    );
    assert_eq!(b.flush(), None);
}

// #19 — RFC 4254 §5.2: the window never exceeds 2^32-1
#[test]
fn window_overflow() {
    let (mut a, _) = pair(Proto::Tcp, 1, u32::MAX);
    let w = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Window(1),
    };
    assert_eq!(a.receive(w), Err(Error::Overflow));
    assert_eq!(a.credit(), u32::MAX);
}

// #19 — half-close each way, then done (yamux FIN, RFC 4254 §5.3 EOF+CLOSE)
#[test]
fn close_both_ways() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    let fin = a.close().unwrap();
    assert_eq!(fin.body, Body::Close);
    assert_eq!(a.send(b"x").unwrap_err(), Error::Closed);
    assert_eq!(a.close().unwrap_err(), Error::Closed);
    assert!(!a.done());
    assert_eq!(b.receive(fin), Ok(Event::Closed));
    let late = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Data(b"x".to_vec()),
    };
    assert_eq!(b.receive(late), Err(Error::Closed));
    let (f, _) = b.send(b"still").unwrap();
    assert_eq!(a.receive(f), Ok(Event::Data(b"still".to_vec())));
    let fin = b.close().unwrap();
    assert!(b.done());
    assert_eq!(a.receive(fin), Ok(Event::Closed));
    assert!(a.done());
}

// #19
#[test]
fn reset_ends_both() {
    let (mut a, mut b) = pair(Proto::Tcp, 10, 10);
    let rst = a.reset(2);
    assert_eq!(rst.body, Body::Reset(2));
    assert!(a.done());
    assert_eq!(a.send(b"x").unwrap_err(), Error::Closed);
    assert_eq!(b.receive(rst), Ok(Event::Reset(2)));
    assert!(b.done());
    assert_eq!(b.state(), State::Reset);
}

// #19
#[test]
fn protocol_violations() {
    let (mut opener, _) = Flow::open(1, Proto::Tcp, 10);
    let d = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Data(b"x".to_vec()),
    };
    assert_eq!(opener.receive(d), Err(Error::State));

    let (_, mut b) = pair(Proto::Tcp, 10, 10);
    let wrong = Frame {
        flow: 1,
        proto: Proto::Udp,
        body: Body::Window(1),
    };
    assert_eq!(b.receive(wrong), Err(Error::Proto));
    let again = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Accept(1),
    };
    assert_eq!(b.receive(again), Err(Error::State));
    let open = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Open(1),
    };
    assert_eq!(b.receive(open), Err(Error::State));
}

#[derive(Debug, Clone)]
enum Op {
    Send(bool, usize),
    Deliver(bool),
    Consume(bool, u32),
    Flush(bool),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<bool>(), 0usize..300).prop_map(|(s, n)| Op::Send(s, n)),
        any::<bool>().prop_map(Op::Deliver),
        (any::<bool>(), 0u32..300).prop_map(|(s, n)| Op::Consume(s, n)),
        any::<bool>().prop_map(Op::Flush),
    ]
}

struct End {
    flow: Flow,
    inbox: VecDeque<Frame>,
    sent: Vec<u8>,
    got: Vec<u8>,
}

fn inflight(q: &VecDeque<Frame>) -> (u64, u64) {
    q.iter().fold((0, 0), |(d, w), f| match &f.body {
        Body::Data(x) => (d + x.len() as u64, w),
        Body::Window(n) => (d, w + u64::from(*n)),
        _ => (d, w),
    })
}

proptest! {
    // #19 — for an honest pair: a sender never exceeds credit, the receiver
    // never rejects, granted+held+freed stays the window, and credit is
    // conserved across the wire (updates replenish exactly)
    #[test]
    fn credit_invariants(
        proto in prop_oneof![Just(Proto::Tcp), Just(Proto::Udp)],
        wa in 0u32..400,
        wb in 0u32..400,
        ops in prop::collection::vec(op(), 0..200),
    ) {
        let (fa, fb) = pair(proto, wa, wb);
        let mut ends = [
            End { flow: fa, inbox: VecDeque::new(), sent: vec![], got: vec![] },
            End { flow: fb, inbox: VecDeque::new(), sent: vec![], got: vec![] },
        ];
        let mut seq = 0u8;
        for op in ops {
            match op {
                Op::Send(s, n) => {
                    let (me, peer) = (usize::from(s), usize::from(!s));
                    let payload: Vec<u8> = (0..n).map(|_| { seq = seq.wrapping_add(1); seq }).collect();
                    let before = ends[me].flow.credit();
                    match ends[me].flow.send(&payload) {
                        Ok((f, used)) => {
                            let d = data(&f).to_vec();
                            prop_assert!(d.len() as u64 <= u64::from(before));
                            prop_assert_eq!(&d[..], &payload[..used]);
                            prop_assert_eq!(ends[me].flow.credit(), before - used as u32);
                            if proto == Proto::Udp {
                                prop_assert_eq!(used, payload.len());
                            }
                            ends[me].sent.extend(&d);
                            ends[peer].inbox.push_back(f);
                        }
                        Err(Error::Blocked) => {
                            prop_assert!(
                                before == 0
                                    || (proto == Proto::Udp && payload.len() > before as usize)
                            );
                        }
                        Err(Error::Empty) => prop_assert!(payload.is_empty() && proto == Proto::Tcp),
                        Err(e) => prop_assert!(false, "send: {e}"),
                    }
                }
                Op::Deliver(s) => {
                    let me = usize::from(s);
                    if let Some(f) = ends[me].inbox.pop_front() {
                        match ends[me].flow.receive(f).unwrap() {
                            Event::Data(d) => ends[me].got.extend(d),
                            Event::Window(_) => {}
                            e => prop_assert!(false, "event: {e:?}"),
                        }
                    }
                }
                Op::Consume(s, n) => {
                    let (me, peer) = (usize::from(s), usize::from(!s));
                    let n = n.min(ends[me].flow.held());
                    if let Some(w) = ends[me].flow.consume(n).unwrap() {
                        ends[peer].inbox.push_back(w);
                    }
                }
                Op::Flush(s) => {
                    let (me, peer) = (usize::from(s), usize::from(!s));
                    if let Some(w) = ends[me].flow.flush() {
                        ends[peer].inbox.push_back(w);
                    }
                }
            }
            for me in 0..2 {
                let peer = 1 - me;
                let f = &ends[me].flow;
                prop_assert_eq!(
                    u64::from(f.granted()) + u64::from(f.held()) + u64::from(f.freed()),
                    u64::from(f.window())
                );
                let (to_me_data, _) = inflight(&ends[me].inbox);
                let (_, to_peer_window) = inflight(&ends[peer].inbox);
                prop_assert_eq!(
                    u64::from(ends[peer].flow.credit()) + to_me_data + to_peer_window,
                    u64::from(f.granted())
                );
            }
        }
        prop_assert!(ends[1].sent.starts_with(&ends[0].got));
        prop_assert!(ends[0].sent.starts_with(&ends[1].got));
    }

    // #19 — a window update adds exactly its increment, or fails and adds nothing
    #[test]
    fn update_adds_exactly(start in 0u32..=u32::MAX, inc in 1u32..=u32::MAX) {
        let (mut a, _) = pair(Proto::Tcp, 1, start);
        let w = Frame { flow: 1, proto: Proto::Tcp, body: Body::Window(inc) };
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
