use super::*;
use proptest::prelude::*;

fn bytes(f: &Frame) -> Vec<u8> {
    let mut out = Vec::new();
    f.encode(&mut out);
    out
}

fn frame(flow: u32, proto: Proto, body: Body<'_>) -> Frame<'_> {
    Frame { flow, proto, body }
}

// #58 — the wire layout is the cross-language contract; pin it byte for byte
#[test]
fn layout() {
    let cases = [
        (
            frame(1, Proto::Tcp, Body::Data(b"hi")),
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
            frame(9, Proto::Udp, Body::Data(&[])),
            vec![0x00, 0x11, 0, 0, 0, 9, 0, 0, 0, 0],
        ),
        (
            frame(5, Proto::Tcp, Body::Stop),
            vec![0x06, 0x06, 0, 0, 0, 5, 0, 0, 0, 0],
        ),
    ];
    for (f, want) in cases {
        assert_eq!(bytes(&f), want, "{f:?}");
        assert_eq!(Frame::decode(&want), Ok(Some((f, want.len()))));
    }
}

// #58 — a short buffer is "need more", never an error
#[test]
fn partial() {
    let whole = bytes(&frame(1, Proto::Tcp, Body::Data(b"hello")));
    for n in 0..whole.len() {
        assert_eq!(Frame::decode(&whole[..n]), Ok(None), "{n}");
    }
}

// #58
#[test]
fn rejects() {
    let cases: &[(&[u8], Error)] = &[
        (&[0x07, 0x06, 0, 0, 0, 1, 0, 0, 0, 0], Error::Type(7)),
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
        (&[0x06, 0x06, 0, 0, 0, 1, 0, 0, 0, 1], Error::Field(1)),
        (&[0x00, 0x06, 0, 0, 0, 1, 0, 0, 0, 0], Error::Empty),
    ];
    for (b, want) in cases {
        assert_eq!(Frame::decode(b).as_ref(), Err(want), "{b:?}");
    }
}

// #58 — a header error surfaces before the body arrives
#[test]
fn header() {
    assert_eq!(
        Frame::decode(&[0x00, 0x06, 0, 0, 0, 1, 0, 1, 0, 0]),
        Err(Error::Oversize(65536))
    );
}

// #58
#[test]
fn order() {
    let a = frame(1, Proto::Tcp, Body::Data(b"ab"));
    let b = frame(2, Proto::Udp, Body::Close);
    let mut wire = bytes(&a);
    wire.extend(bytes(&b));
    let mut r = Reader::new();
    r.feed(&wire);
    assert_eq!(r.pull(), Ok(Some(a)));
    assert_eq!(r.pull(), Ok(Some(b)));
    assert_eq!(r.pull(), Ok(None));
    assert_eq!(r.pending(), 0);
}

// #58 — a stream chunk is never empty; an empty datagram is legal
#[test]
fn empty() {
    let tcp = [0x00, 0x06, 0, 0, 0, 1, 0, 0, 0, 0];
    assert_eq!(Frame::decode(&tcp), Err(Error::Empty));
    let udp = [0x00, 0x11, 0, 0, 0, 1, 0, 0, 0, 0];
    let want = frame(1, Proto::Udp, Body::Data(&[]));
    assert_eq!(Frame::decode(&udp), Ok(Some((want, 10))));
}

// #58 — DATA is written from the caller's payload, byte-identical to encode,
// into exactly the frame's size
#[test]
fn put() {
    let f = Frame::data(7, Proto::Tcp, b"abc").unwrap();
    let mut out = vec![0xaa; f.size() + 1];
    f.put(&mut out[..f.size()]);
    assert_eq!(
        out[..f.size()],
        bytes(&frame(7, Proto::Tcp, Body::Data(b"abc")))
    );
    assert_eq!(out[f.size()], 0xaa);
}

// #58 — a DATA frame the decoder would reject can't be built
#[test]
fn checked() {
    let big = vec![0u8; MAX as usize + 1];
    assert_eq!(
        Frame::data(7, Proto::Udp, &big),
        Err(Error::Oversize(MAX + 1))
    );
    assert_eq!(Frame::data(7, Proto::Tcp, b""), Err(Error::Empty));
    assert_eq!(Frame::data(0, Proto::Tcp, b"x"), Err(Error::Flow));
    assert!(Frame::data(7, Proto::Udp, b"").is_ok());
}

// #58 — parsing a coalesced burst is linear in its bytes, not quadratic
#[test]
fn many() {
    let n = 200_000;
    let mut wire = Vec::new();
    for i in 0..n {
        frame(1 + i, Proto::Tcp, Body::Window(1)).encode(&mut wire);
    }
    let start = std::time::Instant::now();
    let mut r = Reader::new();
    r.feed(&wire);
    let mut got = 0;
    while let Some(f) = r.pull().unwrap() {
        assert_eq!(f.flow, 1 + got);
        got += 1;
    }
    assert_eq!((got, r.pending()), (n, 0));
    let spent = start.elapsed();
    assert!(spent.as_secs() < 5, "{n} frames took {spent:?}"); // linear is ~0.1s in debug; the quadratic parser took ~24s
}

// #58
#[test]
fn display() {
    assert_eq!(
        Error::Oversize(65536).to_string(),
        "L4 bridge frame payload 65536 exceeds 65535"
    );
}

fn proto() -> impl Strategy<Value = Proto> {
    prop_oneof![Just(Proto::Tcp), Just(Proto::Udp)]
}

#[derive(Debug, Clone)]
pub struct Any {
    flow: u32,
    proto: Proto,
    body: Body<'static>,
    data: Vec<u8>,
}

impl Any {
    pub fn frame(&self) -> Frame<'_> {
        let body = match self.body {
            Body::Data(_) => Body::Data(&self.data),
            b => b,
        };
        frame(self.flow, self.proto, body)
    }
}

fn body() -> impl Strategy<Value = (Body<'static>, Vec<u8>)> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..512)
            .prop_map(|d| (Body::Data(&[]), d)),
        any::<u32>().prop_map(|n| (Body::Open(n), vec![])),
        any::<u32>().prop_map(|n| (Body::Accept(n), vec![])),
        (1..=u32::MAX).prop_map(|n| (Body::Window(n), vec![])),
        Just((Body::Close, vec![])),
        any::<u32>().prop_map(|n| (Body::Reset(n), vec![])),
        Just((Body::Stop, vec![])),
    ]
}

pub fn arbitrary() -> impl Strategy<Value = Any> {
    (1..=u32::MAX, proto(), body()).prop_map(
        |(flow, proto, (body, mut data))| {
            if matches!(body, Body::Data(_))
                && data.is_empty()
                && proto == Proto::Tcp
            {
                data.push(0);
            }
            Any {
                flow,
                proto,
                body,
                data,
            }
        },
    )
}

proptest! {
    // #58 — decode ∘ encode is the identity, consuming exactly the frame
    #[test]
    fn roundtrip(f in arbitrary()) {
        let f = f.frame();
        let b = bytes(&f);
        prop_assert_eq!(Frame::decode(&b), Ok(Some((f, b.len()))));
    }

    // #58 — trailing bytes are left for the next frame
    #[test]
    fn trailing(f in arbitrary(), tail in prop::collection::vec(any::<u8>(), 0..32)) {
        let f = f.frame();
        let mut b = bytes(&f);
        let n = b.len();
        b.extend(tail);
        prop_assert_eq!(Frame::decode(&b), Ok(Some((f, n))));
    }

    // #58 — any split of a frame stream into reads yields the same frames
    #[test]
    fn reads(
        frames in prop::collection::vec(arbitrary(), 0..16),
        cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..32),
    ) {
        let frames: Vec<Frame> = frames.iter().map(Any::frame).collect();
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
                got.push(bytes(&f));
            }
        }
        prop_assert_eq!(got, frames.iter().map(bytes).collect::<Vec<_>>());
        prop_assert_eq!(r.pending(), 0);
    }

    // #58 — arbitrary bytes never panic, and accepted frames re-encode
    #[test]
    fn noise(b in prop::collection::vec(any::<u8>(), 0..64)) {
        if let Ok(Some((f, n))) = Frame::decode(&b) {
            prop_assert_eq!(bytes(&f), b[..n].to_vec());
        }
    }
}
