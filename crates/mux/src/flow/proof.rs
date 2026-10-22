use super::*;

static ZERO: [u8; 300] = [0; 300];

fn proto() -> Proto {
    match kani::any() {
        true => Proto::Tcp,
        false => Proto::Udp,
    }
}

// #58 "Done"
#[kani::proof]
#[kani::unwind(5)]
fn credit() {
    let proto = proto();
    let (mut flow, _) = Flow::open(1, proto, MAX).unwrap();
    let accept: u32 = kani::any();
    let frame = |body| Frame {
        flow: 1,
        proto,
        body,
    };
    if flow.receive(frame(Body::Accept(accept))).is_err() {
        return;
    }
    let mut left = u64::from(accept);
    for _ in 0..4 {
        if kani::any() {
            let len: usize = kani::any();
            kani::assume(len <= ZERO.len());
            if let Ok(n) = flow.send(&ZERO[..len]) {
                assert!(n as u64 <= left);
                left -= n as u64;
            }
        } else {
            let inc: u32 = kani::any();
            if flow.receive(frame(Body::Window(inc))).is_ok() {
                left += u64::from(inc);
            }
        }
        assert_eq!(u64::from(flow.credit()), left);
    }
}

// #58 "Done"
#[kani::proof]
#[kani::unwind(5)]
fn replenish() {
    let proto = proto();
    let window: u32 = kani::any();
    let open = Frame {
        flow: 1,
        proto,
        body: Body::Open(MAX),
    };
    let Ok(mut flow) = Flow::offer(&open, window) else {
        return;
    };
    if flow.accept().is_err() {
        return;
    }
    let (mut consumed, mut granted) = (0u64, 0u64);
    for _ in 0..4 {
        match kani::any::<u8>() % 3 {
            0 => {
                let len: usize = kani::any();
                kani::assume(len <= ZERO.len());
                let data = Frame {
                    flow: 1,
                    proto,
                    body: Body::Data(&ZERO[..len]),
                };
                let _ = flow.receive(data);
            }
            1 => {
                let n: u32 = kani::any();
                if let Ok(frame) = flow.consume(n) {
                    consumed += u64::from(n);
                    if let Some(Frame {
                        body: Body::Window(inc),
                        ..
                    }) = frame
                    {
                        granted += u64::from(inc);
                    }
                }
            }
            _ => {
                if let Some(Frame {
                    body: Body::Window(inc),
                    ..
                }) = flow.flush()
                {
                    granted += u64::from(inc);
                }
            }
        }
        let room = u64::from(flow.granted())
            + u64::from(flow.held())
            + u64::from(flow.freed());
        assert_eq!(room, u64::from(window));
        assert_eq!(granted + u64::from(flow.freed()), consumed);
    }
}
