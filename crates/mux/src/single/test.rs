use super::*;
use crate::flow::{self, ABORTED};
use proptest::prelude::*;

fn events(s: &mut Single) -> Vec<Event> {
    std::iter::from_fn(|| s.poll()).collect()
}

fn transmits(s: &mut Single) -> Vec<Transmit> {
    std::iter::from_fn(|| s.transmit()).collect()
}

// #58 — one channel, open from creation; it can never gain another
#[test]
fn one() {
    let mut s = Single::new();
    assert_eq!(s.open(Proto::Tcp), Err(Error::Mode));
    assert_eq!(s.accept(ID), Err(Error::Mode));
    assert_eq!(s.send(7, b"x"), Err(Error::Unknown(7)));
    assert_eq!(s.close(7), Err(Error::Unknown(7)));
    assert!(!s.done());
    assert!(events(&mut s).is_empty());
    assert!(transmits(&mut s).is_empty());
}

// #58 — no read-side stop, window, budget or expiry: the tunnel's TCP is
// the channel's flow control and its lifetime
#[test]
fn extras() {
    let mut s = Single::new();
    assert_eq!(s.stop(ID), Err(Error::Mode));
    assert_eq!(s.grow(ID, 1), Err(Error::Mode));
    s.tick(5);
    s.expire(1000, 1);
    assert!(!s.paused());
    assert!(events(&mut s).is_empty());
    assert!(transmits(&mut s).is_empty());
}

// #58 — bytes pass through bare: no header, no window, no credit limit
#[test]
fn bare() {
    let mut s = Single::new();
    let big = vec![7u8; 1 << 20];
    assert_eq!(s.send(ID, &big), Ok(big.len()));
    assert_eq!(s.send(ID, b"tail"), Ok(4));
    let mut want = big.clone();
    want.extend(b"tail");
    assert_eq!(transmits(&mut s), vec![Transmit::Bytes(want)]);
    s.feed(b"in").unwrap();
    s.feed(b"").unwrap();
    assert_eq!(events(&mut s), vec![Event::Data(ID, b"in".to_vec())]);
    assert_eq!(s.consume(ID, 2), Ok(()));
    assert_eq!(s.flush(ID), Ok(()));
    assert!(transmits(&mut s).is_empty());
}

// #58
#[test]
fn empty() {
    let mut s = Single::new();
    assert_eq!(s.send(ID, b""), Err(Error::Flow(ID, flow::Error::Empty)));
}

// #58 — half-close each way: local close is a write shutdown, tunnel EOF the peer's
#[test]
fn halfclose() {
    let mut s = Single::new();
    s.send(ID, b"a").unwrap();
    s.close(ID).unwrap();
    assert_eq!(
        transmits(&mut s),
        vec![Transmit::Bytes(b"a".to_vec()), Transmit::Shutdown]
    );
    assert_eq!(s.send(ID, b"x"), Err(Error::Flow(ID, flow::Error::Closed)));
    assert_eq!(s.close(ID), Err(Error::Flow(ID, flow::Error::Closed)));
    assert!(!s.done());
    s.feed(b"still").unwrap();
    s.end().unwrap();
    assert_eq!(
        events(&mut s),
        vec![Event::Data(ID, b"still".to_vec()), Event::Closed(ID)]
    );
    assert!(s.done());
    assert_eq!(s.feed(b"x"), Err(Error::Dead));
    assert_eq!(s.end(), Err(Error::Dead));
}

// #58 — the peer closes first; this side may keep writing
#[test]
fn eof() {
    let mut s = Single::new();
    s.end().unwrap();
    assert_eq!(events(&mut s), vec![Event::Closed(ID)]);
    assert_eq!(s.feed(b"x"), Err(Error::Dead));
    assert_eq!(s.send(ID, b"y"), Ok(1));
    s.close(ID).unwrap();
    assert!(s.done());
}

// #58 — a local reset aborts the tunnel and drops whatever was unsent
#[test]
fn reset() {
    let mut s = Single::new();
    s.send(ID, b"unsent").unwrap();
    s.reset(ID, ABORTED).unwrap();
    assert_eq!(transmits(&mut s), vec![Transmit::Abort]);
    assert!(s.done());
    assert_eq!(
        s.reset(ID, ABORTED),
        Err(Error::Flow(ID, flow::Error::Closed))
    );
    assert_eq!(s.send(ID, b"x"), Err(Error::Flow(ID, flow::Error::Closed)));
    assert_eq!(s.feed(b"x"), Err(Error::Dead));
}

// #58 — a failed tunnel resets the channel once
#[test]
fn fail() {
    let mut s = Single::new();
    s.fail();
    assert_eq!(events(&mut s), vec![Event::Reset(ID, ABORTED)]);
    assert!(s.done());
    s.fail();
    assert!(events(&mut s).is_empty());
    assert_eq!(s.send(ID, b"x"), Err(Error::Flow(ID, flow::Error::Closed)));
}

proptest! {
    // #58 — the tunnel carries exactly the caller's bytes, in order, whatever
    // the write and read sizes: zero overhead, so a raw stream peer interoperates
    #[test]
    fn passthrough(
        writes in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..256), 0..32),
        reads in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..256), 0..32),
    ) {
        let mut s = Single::new();
        let mut want = Vec::new();
        for w in &writes {
            prop_assert_eq!(s.send(ID, w), Ok(w.len()));
            want.extend(w);
        }
        let mut wire = Vec::new();
        for t in transmits(&mut s) {
            match t {
                Transmit::Bytes(b) => wire.extend(b),
                t => prop_assert!(false, "{t:?}"),
            }
        }
        prop_assert_eq!(wire, want);
        let mut got = Vec::new();
        for r in &reads {
            s.feed(r).unwrap();
        }
        for e in events(&mut s) {
            match e {
                Event::Data(ID, d) => got.extend(d),
                e => prop_assert!(false, "{e:?}"),
            }
        }
        prop_assert_eq!(got, reads.concat());
    }
}
