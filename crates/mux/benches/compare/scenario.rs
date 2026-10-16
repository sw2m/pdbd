use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

// see crates/mux/CLAUDE.md §"Benchmarks": every harness runs these same numbers
pub const CHUNK: usize = 64 * 1024;
pub const BULK: usize = 64 << 20;
pub const WINDOW: u32 = 256 * 1024;
pub const SPREADS: [usize; 2] = [16, 256];
pub const MSG: usize = 64;
pub const WARM: usize = 100;
pub const PINGS: usize = 2000;
pub const CHURN: usize = 1000;
pub const IDLE: usize = 1000;
pub const DGRAM: usize = 512;
pub const DGRAMS: usize = 200_000;
pub const RUNS: usize = 5;

pub static DATA: [u8; CHUNK] = [0x5a; CHUNK];

pub struct Count;

static LIVE: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: Layout,
        size: usize,
    ) -> *mut u8 {
        LIVE.fetch_add(
            size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        unsafe { System.realloc(ptr, layout, size) }
    }
}

impl Count {
    pub fn live() -> isize {
        LIVE.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scenario {
    Bulk,
    Flows16,
    Fair16,
    Flows256,
    Fair256,
    Pingpong,
    Churn,
    Idle,
    Udp,
}

impl Scenario {
    pub const ALL: [Scenario; 9] = [
        Self::Bulk,
        Self::Flows16,
        Self::Fair16,
        Self::Flows256,
        Self::Fair256,
        Self::Pingpong,
        Self::Churn,
        Self::Idle,
        Self::Udp,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Bulk => "bulk",
            Self::Flows16 => "flows16",
            Self::Fair16 => "fair16",
            Self::Flows256 => "flows256",
            Self::Fair256 => "fair256",
            Self::Pingpong => "pingpong",
            Self::Churn => "churn",
            Self::Idle => "idle",
            Self::Udp => "udp",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    pub fn head(self) -> &'static str {
        match self {
            Self::Bulk => "bulk MiB/s ↑",
            Self::Flows16 => "16 flows MiB/s ↑",
            Self::Fair16 => "16 flows spread % ↓",
            Self::Flows256 => "256 flows MiB/s ↑",
            Self::Fair256 => "256 flows spread % ↓",
            Self::Pingpong => "ping-pong µs ↓",
            Self::Churn => "open→close /s ↑",
            Self::Idle => "idle B/flow ↓",
            Self::Udp => "UDP dgram/s ↑",
        }
    }

    pub fn flows(n: usize) -> (Self, Self) {
        match n {
            16 => (Self::Flows16, Self::Fair16),
            _ => (Self::Flows256, Self::Fair256),
        }
    }
}

pub fn rate(bytes: usize, spent: Duration) -> f64 {
    bytes as f64 / (1 << 20) as f64 / spent.as_secs_f64()
}

pub fn spread(rates: &[f64]) -> f64 {
    let n = rates.len() as f64;
    let mean = rates.iter().sum::<f64>() / n;
    let var = rates.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    100.0 * var.sqrt() / mean
}

pub fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[derive(Default)]
pub struct Results {
    rows: Mutex<Vec<(String, Scenario, f64)>>,
}

impl Results {
    pub fn add(&self, mux: &str, scenario: Scenario, value: f64) {
        let mut rows = self.rows.lock().unwrap();
        rows.push((mux.to_string(), scenario, value));
    }

    pub fn load(&self, tsv: &str) {
        for line in tsv.lines() {
            let mut cols = line.split('\t');
            let (Some(mux), Some(key), Some(value)) =
                (cols.next(), cols.next(), cols.next())
            else {
                continue;
            };
            if let (Some(s), Ok(v)) = (Scenario::parse(key), value.parse()) {
                self.add(mux, s, v);
            }
        }
    }

    pub fn table(&self) -> String {
        let rows = self.rows.lock().unwrap();
        let mut muxes: Vec<&str> = Vec::new();
        for (mux, _, _) in rows.iter() {
            if !muxes.contains(&mux.as_str()) {
                muxes.push(mux);
            }
        }
        let mut out = String::from("| mux |");
        for s in Scenario::ALL {
            let _ = write!(out, " {} |", s.head());
        }
        out.push_str("\n| --- |");
        out.push_str(&" ---: |".repeat(Scenario::ALL.len()));
        out.push('\n');
        for mux in muxes {
            let _ = write!(out, "| {mux} |");
            for s in Scenario::ALL {
                let values: Vec<f64> = rows
                    .iter()
                    .filter(|(m, k, _)| m == mux && *k == s)
                    .map(|(_, _, v)| *v)
                    .collect();
                match values.is_empty() {
                    true => out.push_str(" — |"),
                    false => {
                        let _ =
                            write!(out, " {} |", Self::show(median(values)));
                    }
                }
            }
            out.push('\n');
        }
        out
    }

    fn show(v: f64) -> String {
        match v {
            v if v >= 100.0 => format!("{v:.0}"),
            v if v >= 10.0 => format!("{v:.1}"),
            v => format!("{v:.2}"),
        }
    }
}
