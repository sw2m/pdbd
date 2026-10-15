use super::*;
use crate::flow::Event as Flow;
use crate::frame::Body;
use proptest::prelude::*;
use std::collections::{BTreeMap, VecDeque};

// #19 — yamux §StreamID: one parity per side, 0 reserved
#[test]
fn ids_by_side() {
    let mut c = Mux::new(Side::Client, 10, None);
    let mut d = Mux::new(Side::Daemon, 10, None);
    assert_eq!(c.open(Proto::Tcp).unwrap().0, 1);
    assert_eq!(c.open(Proto::Udp).unwrap().0, 3);
    assert_eq!(d.open(Proto::Tcp).unwrap().0, 2);
    assert_eq!(d.open(Proto::Tcp).unwrap().0, 4);
    assert_eq!(c.len(), 2);
}

// #19 — an Open is accepted with this end's window and routed thereafter
#[test]
fn open_accept_route() {
    let mut c = Mux::new(Side::Client, 10, None);
    let mut d = Mux::new(Side::Daemon, 20, None);
    let (id, open) = c.open(Proto::Udp).unwrap();
    let (got, ev) = d.receive(open).unwrap();
    let accept = match ev {
        Event::Open(Proto::Udp, accept) => accept,
        e => panic!("{e:?}"),
    };
    assert_eq!(got, id);
    assert_eq!(accept.body, Body::Accept(20));
    assert_eq!(d.flow(id).unwrap().credit(), 10);
    assert_eq!(
        c.receive(accept).unwrap(),
        (id, Event::Flow(Flow::Accepted))
    );
    let (f, _) = c.send(id, b"dgram").unwrap();
    assert_eq!(
        d.receive(f).unwrap(),
        (id, Event::Flow(Flow::Data(b"dgram".to_vec())))
    );
}

// #19 — a socat `max-children` is the mux cap
#[test]
fn cap_bounds_live_flows() {
    let mut c = Mux::new(Side::Client, 10, Some(1));
    let (id, _) = c.open(Proto::Tcp).unwrap();
    assert_eq!(c.open(Proto::Tcp).unwrap_err(), Error::Cap(1));
    let mut d = Mux::new(Side::Daemon, 10, Some(1));
    let (_, o1) = Mux::new(Side::Client, 10, None).open(Proto::Tcp).unwrap();
    assert!(matches!(d.receive(o1).unwrap().1, Event::Open(..)));
    let mut far = Mux::new(Side::Client, 10, None);
    far.open(Proto::Tcp).unwrap();
    let (id2, o2) = far.open(Proto::Tcp).unwrap();
    assert_eq!(
        d.receive(o2).unwrap(),
        (
            id2,
            Event::Refused(Frame {
                flow: id2,
                proto: Proto::Tcp,
                body: Body::Reset(0)
            })
        )
    );
    let _ = c.reset(id, 0).unwrap();
    assert!(c.open(Proto::Tcp).is_ok());
}

// #19 — a peer must open with its own parity, once
#[test]
fn peer_open_checks() {
    let mut d = Mux::new(Side::Daemon, 10, None);
    let even = Frame {
        flow: 2,
        proto: Proto::Tcp,
        body: Body::Open(1),
    };
    assert_eq!(d.receive(even).unwrap_err(), Error::Parity(2));
    let odd = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Open(1),
    };
    d.receive(odd.clone()).unwrap();
    assert_eq!(d.receive(odd).unwrap_err(), Error::Duplicate(1));
    let stray = Frame {
        flow: 7,
        proto: Proto::Tcp,
        body: Body::Window(1),
    };
    assert_eq!(d.receive(stray).unwrap_err(), Error::Unknown(7));
}

// #19 — flow errors carry their flow id
#[test]
fn flow_errors_are_tagged() {
    let mut d = Mux::new(Side::Daemon, 4, None);
    let open = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Open(1),
    };
    d.receive(open).unwrap();
    let over = Frame {
        flow: 1,
        proto: Proto::Tcp,
        body: Body::Data(vec![0; 5]),
    };
    assert_eq!(
        d.receive(over).unwrap_err(),
        Error::Flow(1, flow::Error::Credit { len: 5, credit: 4 })
    );
    assert_eq!(d.send(9, b"x").unwrap_err(), Error::Unknown(9));
}

// #19 — a finished flow leaves the table; its id is never reused
#[test]
fn done_flows_are_reaped() {
    let mut c = Mux::new(Side::Client, 10, None);
    let mut d = Mux::new(Side::Daemon, 10, None);
    let (id, open) = c.open(Proto::Tcp).unwrap();
    let (_, ev) = d.receive(open).unwrap();
    let Event::Open(_, accept) = ev else { panic!() };
    c.receive(accept).unwrap();
    let fin = c.close(id).unwrap();
    assert_eq!(d.receive(fin).unwrap(), (id, Event::Flow(Flow::Closed)));
    let fin = d.close(id).unwrap();
    assert!(d.is_empty());
    c.receive(fin).unwrap();
    assert!(c.is_empty());
    assert_eq!(c.open(Proto::Tcp).unwrap().0, 3);
}

// #19 — RFC 9113 §6.9: a window update racing a flow's teardown is ignored
#[test]
fn stale_window_is_ignored() {
    let mut c = Mux::new(Side::Client, 10, None);
    let mut d = Mux::new(Side::Daemon, 10, None);
    let (id, open) = c.open(Proto::Tcp).unwrap();
    let (_, ev) = d.receive(open).unwrap();
    let Event::Open(_, accept) = ev else { panic!() };
    c.receive(accept).unwrap();
    d.receive(c.close(id).unwrap()).unwrap();
    let (data, _) = d.send(id, b"abcdefgh").unwrap();
    let fin = d.close(id).unwrap();
    assert!(d.is_empty());
    c.receive(data).unwrap();
    let w = c.consume(id, 8).unwrap().unwrap();
    c.receive(fin).unwrap();
    assert_eq!(d.receive(w).unwrap(), (id, Event::Stale));
    let late = Frame {
        flow: id,
        proto: Proto::Tcp,
        body: Body::Data(vec![1]),
    };
    assert_eq!(d.receive(late).unwrap_err(), Error::Unknown(id));
}

// #19
#[test]
fn ids_exhaust() {
    let mut c = Mux::new(Side::Client, 10, None);
    c.next = Some(u32::MAX);
    assert_eq!(c.open(Proto::Tcp).unwrap().0, u32::MAX);
    assert_eq!(c.open(Proto::Tcp).unwrap_err(), Error::Exhausted);
}

#[derive(Debug, Clone)]
enum Op {
    Open(bool, bool),
    Send(bool, usize, usize),
    Deliver(bool),
    Consume(bool, usize, u32),
    Close(bool, usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<bool>(), any::<bool>()).prop_map(|(s, u)| Op::Open(s, u)),
        (any::<bool>(), any::<usize>(), 0usize..200)
            .prop_map(|(s, i, n)| Op::Send(s, i, n)),
        any::<bool>().prop_map(Op::Deliver),
        (any::<bool>(), any::<usize>(), 0u32..200)
            .prop_map(|(s, i, n)| Op::Consume(s, i, n)),
        (any::<bool>(), any::<usize>()).prop_map(|(s, i)| Op::Close(s, i)),
    ]
}

proptest! {
    // #19 — two honest muxes over one ordered tunnel: no protocol error ever,
    // and every flow's bytes arrive in order (no cross-flow mixing)
    #[test]
    fn honest_pair(
        wc in 0u32..300,
        wd in 0u32..300,
        ops in prop::collection::vec(op(), 0..300),
    ) {
        let mut m = [Mux::new(Side::Client, wc, None), Mux::new(Side::Daemon, wd, None)];
        let mut wire: [VecDeque<Frame>; 2] = [VecDeque::new(), VecDeque::new()];
        let mut sent: [BTreeMap<u32, Vec<u8>>; 2] = Default::default();
        let mut got: [BTreeMap<u32, Vec<u8>>; 2] = Default::default();
        let mut seq = 0u8;
        let pick = |m: &Mux, i: usize| -> Option<u32> {
            let ids: Vec<u32> = m.flows.keys().copied().collect();
            (!ids.is_empty()).then(|| ids[i % ids.len()])
        };
        for op in ops {
            match op {
                Op::Open(s, udp) => {
                    let me = usize::from(s);
                    let proto = if udp { Proto::Udp } else { Proto::Tcp };
                    let (_, f) = m[me].open(proto).unwrap();
                    wire[1 - me].push_back(f);
                }
                Op::Send(s, i, n) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&m[me], i) else { continue };
                    let payload: Vec<u8> = (0..n).map(|_| { seq = seq.wrapping_add(1); seq }).collect();
                    match m[me].send(id, &payload) {
                        Ok((f, used)) => {
                            sent[me].entry(id).or_default().extend(&payload[..used]);
                            wire[1 - me].push_back(f);
                        }
                        Err(Error::Flow(_, flow::Error::Blocked | flow::Error::Empty | flow::Error::Closed)) => {}
                        Err(e) => prop_assert!(false, "send: {e}"),
                    }
                }
                Op::Deliver(s) => {
                    let me = usize::from(s);
                    let Some(f) = wire[me].pop_front() else { continue };
                    let (id, ev) = m[me].receive(f).unwrap();
                    match ev {
                        Event::Open(_, accept) => wire[1 - me].push_back(accept),
                        Event::Flow(Flow::Data(d)) => got[me].entry(id).or_default().extend(d),
                        Event::Flow(_) | Event::Stale => {}
                        Event::Refused(_) => prop_assert!(false, "refused without a cap"),
                    }
                }
                Op::Consume(s, i, n) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&m[me], i) else { continue };
                    let n = n.min(m[me].flow(id).unwrap().held());
                    if let Some(w) = m[me].consume(id, n).unwrap() {
                        wire[1 - me].push_back(w);
                    }
                }
                Op::Close(s, i) => {
                    let me = usize::from(s);
                    let Some(id) = pick(&m[me], i) else { continue };
                    match m[me].close(id) {
                        Ok(f) => wire[1 - me].push_back(f),
                        Err(Error::Flow(_, flow::Error::Closed)) => {}
                        Err(e) => prop_assert!(false, "close: {e}"),
                    }
                }
            }
        }
        for me in 0..2 {
            for (id, bytes) in &got[me] {
                let from = &sent[1 - me][id];
                prop_assert!(from.starts_with(bytes), "flow {}", id);
            }
        }
    }
}
