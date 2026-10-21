use mux::{single, Error, Event, Multi, Proto, Side, Single, Transmit};
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
const WARM: usize = 4; // rounds before counting: buffers reach their working size
const ROUNDS: usize = 64;

fn count() -> u64 {
    COUNT.with(Cell::get)
}

fn drain(m: &mut Multi) {
    loop {
        let (id, n) = match m.poll() {
            Some(Event::Data(id, d)) => (id, Some(d.len() as u32)),
            Some(Event::Open(id, _)) => (id, None),
            Some(_) => continue,
            None => return,
        };
        match n {
            Some(n) => m.consume(id, n).unwrap(),
            None => m.accept(id).unwrap(),
        }
    }
}

fn pump(from: &mut Multi, to: &mut Multi) {
    while let Some(t) = from.transmit() {
        let Transmit::Bytes(mut b) = t else {
            panic!("multi-channel never emits {t:?}")
        };
        while !b.is_empty() {
            let s = to.spare();
            let n = s.len().min(b.len());
            assert!(n > 0, "no room with every event polled");
            s[..n].copy_from_slice(&b[..n]);
            to.fill(n).unwrap();
            b = &b[n..];
            drain(to);
        }
    }
}

fn round(c: &mut Multi, d: &mut Multi, ids: &[u32], data: &[u8]) {
    for &id in ids {
        let mut at = 0;
        while at < data.len() {
            match c.send(id, &data[at..]) {
                Ok(n) => at += n,
                Err(Error::Flow(_, mux::flow::Error::Blocked)) => {
                    pump(c, d);
                    pump(d, c);
                }
                Err(e) => panic!("{e}"),
            }
        }
    }
    pump(c, d);
    pump(d, c);
}

fn steady(flows: usize, size: usize) -> u64 {
    let mut c = Multi::new(Side::Client, WINDOW, u32::MAX, u64::MAX).unwrap();
    let mut d = Multi::new(Side::Daemon, WINDOW, u32::MAX, u64::MAX).unwrap();
    let ids: Vec<u32> =
        (0..flows).map(|_| c.open(Proto::Tcp).unwrap()).collect();
    pump(&mut c, &mut d);
    pump(&mut d, &mut c);
    let data = vec![7u8; size];
    for _ in 0..WARM {
        round(&mut c, &mut d, &ids, &data);
    }
    let before = count();
    for _ in 0..ROUNDS {
        round(&mut c, &mut d, &ids, &data);
    }
    count() - before
}

// #58 "Minimal copy" — no per-frame allocation on the data path
#[test]
fn one() {
    assert_eq!(steady(1, 1 << 20), 0);
}

// #58 "Minimal copy" — nor as flows share the tunnel
#[test]
fn many() {
    assert_eq!(steady(64, WINDOW as usize), 0);
}

// #58 "Minimal copy" — nor in single-channel mode
#[test]
fn bare() {
    let (mut a, mut b) = (Single::new(), Single::new());
    let data = vec![7u8; 1 << 16];
    let mut round = || {
        for _ in 0..16 {
            assert_eq!(a.send(single::ID, &data), Ok(data.len()));
        }
        while let Some(t) = a.transmit() {
            let Transmit::Bytes(mut w) = t else {
                panic!("{t:?}")
            };
            while !w.is_empty() {
                let s = b.spare();
                let n = s.len().min(w.len());
                assert!(n > 0, "no room with every event polled");
                s[..n].copy_from_slice(&w[..n]);
                b.fill(n).unwrap();
                w = &w[n..];
                while b.poll().is_some() {}
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
    assert_eq!(count() - before, 0);
}
