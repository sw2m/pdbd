use super::*;
use crate::flow::ABORTED;

fn bytes(m: &mut Mux) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(t) = m.transmit() {
        match t {
            Transmit::Bytes(b) => out.extend(b),
            t => panic!("{t:?}"),
        }
    }
    out
}

// #58 — the mode is fixed at creation
#[test]
fn modes() {
    assert_eq!(Mux::single().mode(), Mode::Single);
    let mut m = Mux::multi(Side::Client, 10, Some(4), 1 << 20);
    assert_eq!(m.mode(), Mode::Multi);
    m.open(Proto::Tcp).unwrap();
    assert_eq!(m.mode(), Mode::Multi);
}

// #58 — a single-channel mux bridges a raw stream peer byte for byte
#[test]
fn single() {
    let mut m = Mux::single();
    assert_eq!(m.open(Proto::Tcp), Err(Error::Mode));
    assert_eq!(m.send(single::ID, b"raw"), Ok(3));
    assert_eq!(bytes(&mut m), b"raw");
    m.feed(b"back").unwrap();
    assert_eq!(m.poll(), Some(Event::Data(single::ID, b"back".to_vec())));
    m.close(single::ID).unwrap();
    assert_eq!(m.transmit(), Some(Transmit::Shutdown));
    m.end().unwrap();
    assert_eq!(m.poll(), Some(Event::Closed(single::ID)));
    assert!(m.done());
}

// #58 — a multi-channel mux down to one flow stays multi-channel: still framed
#[test]
fn multi() {
    let mut c = Mux::multi(Side::Client, 10, None, 1 << 20);
    let mut d = Mux::multi(Side::Daemon, 10, None, 1 << 20);
    let a = c.open(Proto::Tcp).unwrap();
    let b = c.open(Proto::Tcp).unwrap();
    d.feed(&bytes(&mut c)).unwrap();
    d.accept(a).unwrap();
    d.accept(b).unwrap();
    c.feed(&bytes(&mut d)).unwrap();
    c.reset(b, ABORTED).unwrap();
    d.feed(&bytes(&mut c)).unwrap();
    while d.poll().is_some() {}
    assert_eq!(c.send(a, b"x"), Ok(1));
    let wire = bytes(&mut c);
    assert_eq!(wire.len(), frame::HEADER + 1);
    d.feed(&wire).unwrap();
    assert_eq!(d.poll(), Some(Event::Data(a, b"x".to_vec())));
    c.fail();
    assert!(c.done());
}

// #58
#[test]
fn display() {
    assert_eq!(
        Error::Peer(3, flow::Error::Overflow).to_string(),
        "L4 bridge flow 3: peer violation: window update overflows 2^32-1"
    );
    assert_eq!(
        Error::Cap(2).to_string(),
        "L4 bridge is at its cap of 2 flows"
    );
}
