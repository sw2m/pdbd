use super::*;
use crate::flow::ABORTED;

const ALL: u32 = u32::MAX; // a cap that never refuses

fn bytes(m: &mut Mux, source: &mut impl Source) -> Vec<u8> {
    let mut out = vec![0; 2 * frame::WHOLE];
    let mut all = Vec::new();
    loop {
        match m.pull(&mut out, source) {
            Transmit::Bytes(0) => return all,
            Transmit::Bytes(n) => all.extend(&out[..n]),
            t => panic!("{t:?}"),
        }
    }
}

fn nothing(_: u32, _: &mut [u8]) -> Pull {
    Pull::Wait
}

fn once(data: &'static [u8]) -> impl FnMut(u32, &mut [u8]) -> Pull {
    let mut left = data;
    move |_, buf| {
        if left.is_empty() {
            return Pull::Wait;
        }
        let n = left.len().min(buf.len());
        buf[..n].copy_from_slice(&left[..n]);
        left = &left[n..];
        Pull::Bytes(n)
    }
}

fn data(m: &mut Mux) -> Option<(u32, Vec<u8>)> {
    let Some(Event::Data(id, n)) = m.poll() else {
        return None;
    };
    let held: Vec<u8> = m.held(id).flatten().copied().collect();
    m.consume(id, n).unwrap();
    Some((id, held))
}

// #58
#[test]
fn modes() {
    assert_eq!(Mux::single(Proto::Tcp).mode(), Mode::Single);
    let mut m = Mux::multi(Side::Client, 10, 4, 1 << 20).unwrap();
    assert_eq!(m.mode(), Mode::Multi);
    m.open(Proto::Tcp).unwrap();
    assert_eq!(m.mode(), Mode::Multi);
}

// #58
#[test]
fn single() {
    let mut m = Mux::single(Proto::Tcp);
    assert_eq!(m.open(Proto::Tcp), Err(Error::Mode));
    m.ready(single::ID).unwrap();
    assert_eq!(bytes(&mut m, &mut once(b"raw")), b"raw");
    m.feed(b"back").unwrap();
    assert_eq!(data(&mut m), Some((single::ID, b"back".to_vec())));
    m.close(single::ID).unwrap();
    let mut out = [0; 8];
    assert_eq!(m.pull(&mut out, &mut nothing), Transmit::Shutdown);
    m.end().unwrap();
    assert_eq!(m.poll(), Some(Event::Closed(single::ID)));
    assert!(m.done());
}

// #58
#[test]
fn multi() {
    let mut c = Mux::multi(Side::Client, 10, ALL, 1 << 20).unwrap();
    let mut d = Mux::multi(Side::Daemon, 10, ALL, 1 << 20).unwrap();
    let a = c.open(Proto::Tcp).unwrap();
    let b = c.open(Proto::Tcp).unwrap();
    d.feed(&bytes(&mut c, &mut nothing)).unwrap();
    d.accept(a).unwrap();
    d.accept(b).unwrap();
    c.feed(&bytes(&mut d, &mut nothing)).unwrap();
    c.reset(b, ABORTED).unwrap();
    d.feed(&bytes(&mut c, &mut nothing)).unwrap();
    while d.poll().is_some() {}
    c.ready(a).unwrap();
    let wire = bytes(&mut c, &mut once(b"x"));
    assert_eq!(wire.len(), frame::HEADER + 1);
    d.feed(&wire).unwrap();
    assert_eq!(data(&mut d), Some((a, b"x".to_vec())));
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
