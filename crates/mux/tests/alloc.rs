use mux::{single, Event, Multi, Proto, Pull, Side, Single, Transmit};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

struct Counter;

unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        COUNT.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: Layout,
        size: usize,
    ) -> *mut u8 {
        COUNT.with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counter = Counter;

const WINDOW: u32 = 1 << 18; // four full frames in flight per flow
const WARM: usize = 32; // rounds before counting: buffers reach their working size (the hold order peaks by round 20)
const ROUNDS: usize = 64;
const OUT: usize = 1 << 18; // one socket write's worth of frames

fn count() -> u64 {
    COUNT.with(Cell::get)
}

struct Tap {
    left: Vec<usize>,
    data: Vec<u8>,
}

impl Tap {
    fn source(&mut self) -> impl FnMut(u32, &mut [u8]) -> Pull + '_ {
        |id, buf| {
            let left = &mut self.left[(id as usize - 1) / 2];
            let n = (*left).min(buf.len()).min(self.data.len());
            if n == 0 {
                return Pull::Wait;
            }
            buf[..n].copy_from_slice(&self.data[..n]);
            *left -= n;
            Pull::Bytes(n)
        }
    }
}

fn drain(m: &mut Multi, keep: Option<u32>) {
    while let Some(event) = m.poll() {
        match event {
            Event::Data(id, n) => {
                let held: usize = m.held(id).map(<[u8]>::len).sum();
                assert!(held >= n as usize);
                if keep != Some(id) {
                    m.consume(id, n).unwrap();
                }
            }
            Event::Open(id, _) => m.accept(id).unwrap(),
            _ => {}
        }
    }
}

fn pump(from: &mut Multi, to: &mut Multi, tap: &mut Tap, keep: Option<u32>) {
    let mut source = tap.source();
    loop {
        drain(to, keep);
        let room = to.spare();
        assert!(!room.is_empty(), "no room with every event consumed");
        let n = from.pull(room, &mut source);
        if n == 0 {
            return;
        }
        to.fill(n).unwrap();
    }
}

fn round(
    c: &mut Multi,
    d: &mut Multi,
    ids: &[u32],
    tap: &mut Tap,
    back: &mut Tap,
    keep: Option<u32>,
) {
    let sending = || ids.iter().copied().filter(move |&id| Some(id) != keep);
    for id in sending() {
        tap.left[(id as usize - 1) / 2] = tap.data.len();
        c.ready(id).unwrap();
    }
    while sending().any(|id| tap.left[(id as usize - 1) / 2] > 0) {
        pump(c, d, tap, keep);
        pump(d, c, back, keep);
    }
    pump(c, d, tap, keep);
    pump(d, c, back, keep);
}

fn steady(flows: usize, size: usize) -> u64 {
    held(flows, size, None)
}

fn held(flows: usize, size: usize, keep: Option<u32>) -> u64 {
    let mut c = Multi::new(Side::Client, WINDOW, u32::MAX, u64::MAX).unwrap();
    let mut d = Multi::new(Side::Daemon, WINDOW, u32::MAX, u64::MAX).unwrap();
    let ids: Vec<u32> =
        (0..flows).map(|_| c.open(Proto::Tcp).unwrap()).collect();
    let mut tap = Tap {
        left: vec![0; flows],
        data: vec![7u8; size],
    };
    let mut back = Tap {
        left: Vec::new(),
        data: Vec::new(),
    };
    pump(&mut c, &mut d, &mut tap, keep);
    pump(&mut d, &mut c, &mut back, keep);
    if let Some(id) = keep {
        tap.left[(id as usize - 1) / 2] = WINDOW as usize;
        c.ready(id).unwrap();
    }
    for _ in 0..WARM {
        round(&mut c, &mut d, &ids, &mut tap, &mut back, keep);
    }
    let before = count();
    for _ in 0..ROUNDS {
        round(&mut c, &mut d, &ids, &mut tap, &mut back, keep);
    }
    count() - before
}

// #58 "Data path"
#[test]
fn one() {
    assert_eq!(steady(1, 1 << 20), 0);
}

// #58 "Data path"
#[test]
fn many() {
    assert_eq!(steady(64, WINDOW as usize), 0);
}

// #58 "A holding flow never stalls another"
#[test]
fn hold() {
    assert_eq!(held(4, WINDOW as usize, Some(1)), 0);
}

// #58 "Data path"
#[test]
fn bare() {
    for proto in [Proto::Tcp, Proto::Udp] {
        let (mut a, mut b) = (Single::new(proto), Single::new(proto));
        let data = vec![7u8; 1 << 15];
        let mut out = vec![0; OUT];
        let mut round = || {
            let mut left = 16;
            let mut source = |_: u32, buf: &mut [u8]| {
                if left == 0 {
                    return Pull::Wait;
                }
                if buf.len() < data.len() {
                    return Pull::Blocked;
                }
                left -= 1;
                buf[..data.len()].copy_from_slice(&data);
                Pull::Bytes(data.len())
            };
            a.ready(single::ID).unwrap();
            loop {
                let Transmit::Bytes(n) = a.pull(&mut out, &mut source) else {
                    panic!("no bytes")
                };
                if n == 0 {
                    break;
                }
                let mut w = &out[..n];
                while !w.is_empty() {
                    let s = b.spare();
                    let k = s.len().min(w.len());
                    assert!(k > 0, "no room with every event consumed");
                    s[..k].copy_from_slice(&w[..k]);
                    b.fill(k).unwrap();
                    w = &w[k..];
                    while let Some(Event::Data(_, n)) = b.poll() {
                        b.consume(single::ID, n).unwrap();
                    }
                }
            }
        };
        for _ in 0..WARM {
            round();
        }
        let before = count();
        for _ in 0..ROUNDS {
            round();
        }
        assert_eq!(count() - before, 0, "{proto:?}");
    }
}
