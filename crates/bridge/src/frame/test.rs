use super::*;
use proptest::prelude::*;

fn bytes(f: &Frame) -> Vec<u8> {
    let mut out = Vec::new();
    f.encode(&mut out);
    out
}

fn frame(flow: u32, proto: Proto, body: Body) -> Frame {
    Frame { flow, proto, body }
}

// #19 — the wire layout is the cross-language contract; pin it byte for byte
#[test]
fn layout() {
    let cases = [
        (
            frame(1, Proto::Tcp, Body::Data(b"hi".to_vec())),
            vec![0x00, 0x06, 0, 0, 0, 1, 0, 0, 0, 2, b'h', b'i'],
        ),
        (
            frame(2, Proto::Udp, Body::Open(0x0004_0000)),
            vec![0x01, 0x11, 0, 0, 0, 2, 0, 0x04, 0, 0],
        ),
        (
            frame(0x0102_0304, Proto::Tcp, Body::Accept(7)),
            vec![0x02, 0x06, 1, 2, 3, 4, 0, 0, 0, 7],
        ),
        (
            frame(3, Proto::Udp, Body::Window(4096)),
            vec![0x03, 0x11, 0, 0, 0, 3, 0, 0, 0x10, 0],
        ),
        (
            frame(5, Proto::Tcp, Body::Close),
            vec![0x04, 0x06, 0, 0, 0, 5, 0, 0, 0, 0],
        ),
        (
            frame(5, Proto::Tcp, Body::Reset(1)),
            vec![0x05, 0x06, 0, 0, 0, 5, 0, 0, 0, 1],
        ),
        (
            frame(9, Proto::Udp, Body::Data(vec![])),
            vec![0x00, 0x11, 0, 0, 0, 9, 0, 0, 0, 0],
        ),
    ];
    for (f, want) in cases {
        assert_eq!(bytes(&f), want, "{f:?}");
        assert_eq!(Frame::decode(&want), Ok(Some((f, want.len()))));
    }
}

// #19 — a short buffer is "need more", never an error
#[test]
fn partial() {
    let whole = bytes(&frame(1, Proto::Tcp, Body::Data(b"hello".to_vec())));
    for n in 0..whole.len() {
        assert_eq!(Frame::decode(&whole[..n]), Ok(None), "{n}");
    }
}

// #19
#[test]
fn rejects() {
    let cases: &[(&[u8], Error)] = &[
        (&[0x06, 0x06, 0, 0, 0, 1, 0, 0, 0, 0], Error::Type(6)),
        (&[0xff, 0x06, 0, 0, 0, 1, 0, 0, 0, 0], Error::Type(0xff)),
        (&[0x00, 0x01, 0, 0, 0, 1, 0, 0, 0, 0], Error::Proto(1)),
        (&[0x00, 0x06, 0, 0, 0, 0, 0, 0, 0, 0], Error::Flow),
        (
            &[0x00, 0x06, 0, 0, 0, 1, 0, 1, 0, 0],
            Error::Oversize(65536),
        ),
        (
            &[0x00, 0x06, 0, 0, 0, 1, 0xff, 0xff, 0xff, 0xff],
            Error::Oversize(u32::MAX),
        ),
        (&[0x03, 0x06, 0, 0, 0, 1, 0, 0, 0, 0], Error::Increment),
        (&[0x04, 0x06, 0, 0, 0, 1, 0, 0, 0, 1], Error::Field(1)),
    ];
    for (b, want) in cases {
        assert_eq!(Frame::decode(b).as_ref(), Err(want), "{b:?}");
    }
}

// #19 — a header error surfaces before the body arrives
#[test]
fn rejects_on_header_alone() {
    assert_eq!(
        Frame::decode(&[0x00, 0x06, 0, 0, 0, 1, 0, 1, 0, 0]),
        Err(Error::Oversize(65536))
    );
}

// #19
#[test]
fn reader_drains_in_order() {
    let a = frame(1, Proto::Tcp, Body::Data(b"ab".to_vec()));
    let b = frame(2, Proto::Udp, Body::Close);
    let mut wire = bytes(&a);
    wire.extend(bytes(&b));
    let mut r = Reader::new();
    r.feed(&wire);
    assert_eq!(r.pull(), Ok(Some(a)));
    assert_eq!(r.pull(), Ok(Some(b)));
    assert_eq!(r.pull(), Ok(None));
}

// #19
#[test]
fn display() {
    assert_eq!(
        Error::Oversize(65536).to_string(),
        "bridge frame payload 65536 exceeds 65535"
    );
}

fn proto() -> impl Strategy<Value = Proto> {
    prop_oneof![Just(Proto::Tcp), Just(Proto::Udp)]
}

fn body() -> impl Strategy<Value = Body> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..512).prop_map(Body::Data),
        any::<u32>().prop_map(Body::Open),
        any::<u32>().prop_map(Body::Accept),
        (1..=u32::MAX).prop_map(Body::Window),
        Just(Body::Close),
        any::<u32>().prop_map(Body::Reset),
    ]
}

fn any_frame() -> impl Strategy<Value = Frame> {
    (1..=u32::MAX, proto(), body()).prop_map(|(flow, proto, body)| Frame {
        flow,
        proto,
        body,
    })
}

proptest! {
    // #19 — decode ∘ encode is the identity, consuming exactly the frame
    #[test]
    fn roundtrip(f in any_frame()) {
        let b = bytes(&f);
        prop_assert_eq!(Frame::decode(&b), Ok(Some((f, b.len()))));
    }

    // #19 — trailing bytes are left for the next frame
    #[test]
    fn trailing(f in any_frame(), tail in prop::collection::vec(any::<u8>(), 0..32)) {
        let mut b = bytes(&f);
        let n = b.len();
        b.extend(tail);
        prop_assert_eq!(Frame::decode(&b), Ok(Some((f, n))));
    }

    // #19 — any split of a frame stream into reads yields the same frames
    #[test]
    fn partial_reads(
        frames in prop::collection::vec(any_frame(), 0..16),
        cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..32),
    ) {
        let mut wire = Vec::new();
        for f in &frames {
            f.encode(&mut wire);
        }
        let mut at: Vec<usize> = cuts.iter().map(|i| i.index(wire.len() + 1)).collect();
        at.push(0);
        at.push(wire.len());
        at.sort_unstable();
        let mut r = Reader::new();
        let mut got = Vec::new();
        for w in at.windows(2) {
            r.feed(&wire[w[0]..w[1]]);
            while let Some(f) = r.pull().unwrap() {
                got.push(f);
            }
        }
        prop_assert_eq!(got, frames);
    }

    // #19 — arbitrary bytes never panic, and accepted frames re-encode
    #[test]
    fn arbitrary_bytes(b in prop::collection::vec(any::<u8>(), 0..64)) {
        if let Ok(Some((f, n))) = Frame::decode(&b) {
            prop_assert_eq!(bytes(&f), b[..n].to_vec());
        }
    }
}
