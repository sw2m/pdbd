// see sw2m/pdbd-bench mux/README.md: ours, raced as `core` and `loopback` by the mux runner
use mux::{Event, Multi, Proto, Pull, Side};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::exit;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Params {
    window: u32,
    budget: u64,
    chunk: usize,
    bulk: usize,
    flows: usize,
    message: usize,
    pings: usize,
    warm: usize,
    load: usize,
    churn: usize,
    idle: usize,
    datagrams: usize,
    datagram: usize,
    limit: u32,
    timeout: f64,
}

impl Params {
    fn parse(json: &str) -> Self {
        let field = |key: &str, otherwise: f64| -> f64 {
            let needle = format!("\"{key}\":");
            let Some(at) = json.find(&needle) else {
                return otherwise;
            };
            let rest = json[at + needle.len()..].trim_start();
            let end = rest
                .find(|c: char| !"-0123456789.eE".contains(c))
                .unwrap_or(rest.len());
            rest[..end].parse().unwrap_or(otherwise)
        };
        Params {
            window: field("window", 262144.0) as u32,
            budget: field("budget", 67108864.0) as u64,
            chunk: field("chunk", 65536.0) as usize,
            bulk: field("bulk", 268435456.0) as usize,
            flows: field("flows", 16.0) as usize,
            message: field("message", 64.0) as usize,
            pings: field("pings", 10000.0) as usize,
            warm: field("warm", 100.0) as usize,
            load: field("load", 4.0) as usize,
            churn: field("churn", 1000.0) as usize,
            idle: field("idle", 1000.0) as usize,
            datagrams: field("datagrams", 200000.0) as usize,
            datagram: field("datagram", 512.0) as usize,
            limit: field("limit", 1001.0) as u32,
            timeout: field("timeout", 60.0),
        }
    }
}

#[repr(C)]
struct Rusage {
    user: [i64; 2],
    system: [i64; 2],
    rest: [i64; 14],
}

extern "C" {
    fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    #[cfg(target_env = "gnu")]
    fn malloc_trim(pad: usize) -> i32;
}

fn cpu() -> f64 {
    let mut u = Rusage {
        user: [0; 2],
        system: [0; 2],
        rest: [0; 14],
    };
    unsafe { getrusage(0, &mut u) };
    (u.user[0] + u.system[0]) as f64 * 1e3
        + (u.user[1] + u.system[1]) as f64 / 1e3
}

fn status(field: &str) -> f64 {
    let raw = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    raw.lines()
        .find_map(|l| l.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|rest| rest.split_whitespace().next()?.parse::<f64>().ok())
        .map(|kb| kb * 1024.0)
        .unwrap_or_else(|| fail(&format!("/proc/self/status has no {field}")))
}

fn rss() -> f64 {
    #[cfg(target_env = "gnu")]
    unsafe {
        malloc_trim(0);
    }
    status("VmRSS")
}

fn fail(message: &str) -> ! {
    eprintln!("muxbench: {message}");
    exit(2)
}

struct Socket {
    tcp: TcpStream,
    out: Vec<u8>,
    at: usize,
}

impl Socket {
    fn send(&mut self, mut bytes: &[u8]) {
        if self.at < self.out.len() {
            self.out.extend_from_slice(bytes);
            return;
        }
        while !bytes.is_empty() {
            match self.tcp.write(bytes) {
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => fail(&format!("loopback write: {e}")),
            }
        }
        self.out.clear();
        self.at = 0;
        self.out.extend_from_slice(bytes);
    }

    fn busy(&self) -> bool {
        self.at < self.out.len()
    }

    fn write(&mut self) {
        while self.at < self.out.len() {
            match self.tcp.write(&self.out[self.at..]) {
                Ok(n) => self.at += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => fail(&format!("loopback write: {e}")),
            }
        }
    }
}

#[derive(Clone, Default)]
struct End {
    got: usize,
    stalled: bool,
    accepted: bool,
}

struct Tally<'a> {
    side: usize,
    ids: &'a [u32],
    ends: &'a mut [End],
    arrived: &'a mut Vec<Instant>,
}

impl Tally<'_> {
    fn find(&self, id: u32) -> Option<usize> {
        let i = (id as usize).checked_sub(1)? / 2;
        (self.ids.get(i) == Some(&id)).then_some(i)
    }

    fn take(&mut self, m: &mut Multi) {
        while let Some(event) = m.poll() {
            match event {
                Event::Open(id, _) if self.side == 1 => {
                    m.accept(id)
                        .unwrap_or_else(|e| fail(&format!("accept: {e:?}")));
                    self.arrived.push(Instant::now());
                }
                Event::Accepted(id) if self.side == 0 => {
                    if let Some(i) = self.find(id) {
                        self.ends[i].accepted = true;
                    }
                }
                Event::Data(id, n) => {
                    let Some(i) = self.find(id) else {
                        continue;
                    };
                    self.ends[i].got += n as usize;
                    if !self.ends[i].stalled {
                        m.consume(id, n).unwrap_or_else(|e| {
                            fail(&format!("consume: {e:?}"))
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

struct Duo {
    near: Multi,
    far: Multi,
    link: Option<[Socket; 2]>,
    pending: [Vec<usize>; 2],
    unit: Vec<usize>,
    out: Vec<u8>,
    data: Vec<u8>,
    p: Params,
    ids: Vec<u32>,
    ends: [Vec<End>; 2],
    arrived: Vec<Instant>,
}

impl Duo {
    fn new(loopback: bool, p: Params) -> Self {
        let multi = |side| {
            Multi::new(side, p.window, p.limit, p.budget)
                .unwrap_or_else(|e| fail(&format!("{e:?}")))
        };
        let link = loopback.then(|| {
            let listener = TcpListener::bind("127.0.0.1:0")
                .unwrap_or_else(|e| fail(&format!("listen: {e}")));
            let near = TcpStream::connect(listener.local_addr().unwrap())
                .unwrap_or_else(|e| fail(&format!("connect: {e}")));
            let (far, _) = listener
                .accept()
                .unwrap_or_else(|e| fail(&format!("accept: {e}")));
            [near, far].map(|tcp| {
                tcp.set_nodelay(true).unwrap();
                tcp.set_nonblocking(true).unwrap();
                Socket {
                    tcp,
                    out: Vec::new(),
                    at: 0,
                }
            })
        });
        Duo {
            near: multi(Side::Client),
            far: multi(Side::Daemon),
            link,
            pending: [Vec::new(), Vec::new()],
            unit: Vec::new(),
            out: vec![0; 1 << 18],
            data: vec![0x5a; p.chunk.max(p.datagram)],
            p,
            ids: Vec::new(),
            ends: [Vec::new(), Vec::new()],
            arrived: Vec::new(),
        }
    }

    fn pump(&mut self) {
        match &mut self.link {
            None => {
                self.carry(0);
                self.carry(1);
            }
            Some(link) => {
                for (side, socket) in link.iter_mut().enumerate() {
                    socket.write();
                    let m = if side == 0 {
                        &mut self.near
                    } else {
                        &mut self.far
                    };
                    let mut pull =
                        source(&mut self.pending[side], &self.unit, &self.data);
                    while !socket.busy() {
                        let n = m.pull(&mut self.out, &mut pull);
                        if n == 0 {
                            break;
                        }
                        socket.send(&self.out[..n]);
                    }
                }
                self.read(1);
                self.read(0);
            }
        }
        self.drain(1);
        self.drain(0);
    }

    fn read(&mut self, side: usize) {
        loop {
            let m = if side == 0 {
                &mut self.near
            } else {
                &mut self.far
            };
            let Some(link) = &mut self.link else { return };
            let room = m.spare();
            if room.is_empty() {
                return;
            }
            match link[side].tcp.read(room) {
                Ok(0) => return,
                Ok(n) => {
                    m.fill(n).unwrap_or_else(|e| fail(&format!("fill: {e:?}")))
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(e) => fail(&format!("loopback read: {e}")),
            }
            self.drain(side);
        }
    }

    fn carry(&mut self, from: usize) {
        let to = 1 - from;
        let (src, dst) = if from == 0 {
            (&mut self.near, &mut self.far)
        } else {
            (&mut self.far, &mut self.near)
        };
        let mut tally = Tally {
            side: to,
            ids: &self.ids,
            ends: &mut self.ends[to],
            arrived: &mut self.arrived,
        };
        let mut pull = source(&mut self.pending[from], &self.unit, &self.data);
        let mut retried = false;
        loop {
            let n = src.pull(dst.spare(), &mut pull);
            if n > 0 {
                dst.fill(n)
                    .unwrap_or_else(|e| fail(&format!("fill: {e:?}")));
                retried = false;
            } else if retried {
                return;
            } else {
                tally.take(dst);
                retried = true;
            }
        }
    }

    fn drain(&mut self, side: usize) {
        let (m, ends) = if side == 0 {
            (&mut self.near, &mut self.ends[0])
        } else {
            (&mut self.far, &mut self.ends[1])
        };
        Tally {
            side,
            ids: &self.ids,
            ends,
            arrived: &mut self.arrived,
        }
        .take(m);
    }

    fn until(&mut self, mut done: impl FnMut(&Duo) -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs_f64(self.p.timeout);
        while !done(self) {
            self.pump();
            if Instant::now() > end {
                return false;
            }
        }
        true
    }

    // see sw2m/pdbd-bench mux/README.md "Scenarios"
    fn opened(&mut self, proto: Proto, n: usize) -> Vec<usize> {
        let first = self.ids.len();
        for _ in 0..n {
            let id = self
                .near
                .open(proto)
                .unwrap_or_else(|e| fail(&format!("open: {e:?}")));
            self.ids.push(id);
            self.ends[0].push(End::default());
            self.ends[1].push(End::default());
            self.pending[0].push(0);
            self.pending[1].push(0);
            self.unit.push(match proto {
                Proto::Tcp => 0,
                Proto::Udp => self.p.datagram,
            });
        }
        let idx: Vec<usize> = (first..first + n).collect();
        if !self.until(|d| idx.iter().all(|&i| d.ends[0][i].accepted)) {
            fail(&format!("accepting {n} flows missed the deadline"));
        }
        let one = self.unit[first].max(1);
        for &i in &idx {
            self.send(0, i, one);
        }
        let mut echoed = vec![false; n];
        let end = Instant::now() + Duration::from_secs_f64(self.p.timeout);
        while idx.iter().any(|&i| self.ends[0][i].got < one) {
            for (k, &i) in idx.iter().enumerate() {
                if !echoed[k] && self.ends[1][i].got >= one {
                    self.send(1, i, one);
                    echoed[k] = true;
                }
            }
            self.pump();
            if Instant::now() > end {
                fail(&format!("priming {n} flows missed the deadline"));
            }
        }
        idx
    }

    fn send(&mut self, side: usize, i: usize, n: usize) {
        let id = self.ids[i];
        let m = if side == 0 {
            &mut self.near
        } else {
            &mut self.far
        };
        self.pending[side][i] = self.pending[side][i].saturating_add(n);
        m.ready(id)
            .unwrap_or_else(|e| fail(&format!("ready: {e:?}")));
    }

    fn fill(&mut self, idx: &[usize], share: usize, sent: &mut [usize]) {
        for (k, &i) in idx.iter().enumerate() {
            if sent[k] < share {
                self.send(0, i, share - sent[k]);
                sent[k] = share;
            }
        }
    }
}

fn source<'a>(
    pending: &'a mut [usize],
    unit: &'a [usize],
    data: &'a [u8],
) -> impl FnMut(u32, &mut [u8]) -> Pull + 'a {
    move |id, buf| {
        let i = (id as usize - 1) / 2;
        let Some(left) = pending.get_mut(i) else {
            return Pull::Wait;
        };
        let n = match unit[i] {
            _ if *left == 0 => return Pull::Wait,
            0 => (*left).min(buf.len()).min(data.len()),
            u if u <= buf.len() => u,
            _ => return Pull::Blocked,
        };
        buf[..n].copy_from_slice(&data[..n]);
        *left -= n;
        Pull::Bytes(n)
    }
}

fn mib(bytes: usize, spent: Duration) -> f64 {
    bytes as f64 / (1 << 20) as f64 / spent.as_secs_f64()
}

fn percentiles(
    mut micros: Vec<f64>,
    prefix: &str,
    out: &mut Vec<(String, f64)>,
) {
    micros.sort_by(f64::total_cmp);
    let at = |q: f64| micros[(q * (micros.len() - 1) as f64) as usize];
    out.push((format!("{prefix}p50 µs"), at(0.50)));
    out.push((format!("{prefix}p99 µs"), at(0.99)));
    if prefix.is_empty() {
        out.push(("p999 µs".into(), at(0.999)));
    }
}

struct Outcome {
    values: Vec<(String, f64)>,
    ok: bool,
}

fn spread(
    d: &mut Duo,
    idx: &[usize],
    share: usize,
) -> Option<(Instant, Vec<Instant>)> {
    let start = Instant::now();
    let mut sent = vec![0; idx.len()];
    let mut ends: Vec<Option<Instant>> = vec![None; idx.len()];
    let deadline = start + Duration::from_secs_f64(d.p.timeout);
    while ends.iter().any(Option::is_none) {
        d.fill(idx, share, &mut sent);
        d.pump();
        for (k, &i) in idx.iter().enumerate() {
            if ends[k].is_none() && d.ends[1][i].got > share {
                ends[k] = Some(Instant::now());
            }
        }
        if Instant::now() > deadline {
            return None;
        }
    }
    Some((start, ends.into_iter().map(Option::unwrap).collect()))
}

fn bulk(d: &mut Duo) -> Outcome {
    let idx = d.opened(Proto::Tcp, 1);
    let Some((start, ends)) = spread(d, &idx, d.p.bulk) else {
        fail("bulk missed the deadline")
    };
    Outcome {
        values: vec![("MiB/s".into(), mib(d.p.bulk, ends[0] - start))],
        ok: true,
    }
}

fn flows(d: &mut Duo) -> Outcome {
    let n = d.p.flows;
    let share = d.p.bulk / n;
    let idx = d.opened(Proto::Tcp, n);
    let Some((start, ends)) = spread(d, &idx, share) else {
        fail("flows missed the deadline")
    };
    let rates: Vec<f64> = ends.iter().map(|e| mib(share, *e - start)).collect();
    let sum: f64 = rates.iter().sum();
    let squares: f64 = rates.iter().map(|r| r * r).sum();
    let mut sorted = rates.clone();
    sorted.sort_by(f64::total_cmp);
    let last = *ends.iter().max().unwrap();
    Outcome {
        values: vec![
            ("MiB/s".into(), mib(share * n, last - start)),
            ("jain".into(), sum * sum / (n as f64 * squares)),
            ("slowest".into(), sorted[0] / sorted[n / 2]),
        ],
        ok: true,
    }
}

// see sw2m/pdbd-bench mux/README.md "Scenarios"
fn isolation(d: &mut Duo, stalled: bool) -> Outcome {
    let begin = status("VmRSS");
    let n = d.p.flows;
    let share = d.p.bulk / n;
    let idx = d.opened(Proto::Tcp, n);
    d.ends[1][idx[0]].stalled = true;
    let mut quiet = [0];
    let Some((start, ends)) = ({
        let others = &idx[1..];
        let begun = Instant::now();
        let mut sent = vec![0; others.len()];
        let mut done: Vec<Option<Instant>> = vec![None; others.len()];
        let deadline = begun + Duration::from_secs_f64(d.p.timeout);
        let mut missed = false;
        while done.iter().any(Option::is_none) {
            if stalled {
                d.fill(&idx[..1], share, &mut quiet);
            }
            d.fill(others, share, &mut sent);
            d.pump();
            for (k, &i) in others.iter().enumerate() {
                if done[k].is_none() && d.ends[1][i].got > share {
                    done[k] = Some(Instant::now());
                }
            }
            if Instant::now() > deadline {
                missed = true;
                break;
            }
        }
        (!missed).then(|| {
            (
                begun,
                done.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
            )
        })
    }) else {
        return Outcome {
            values: vec![],
            ok: false,
        };
    };
    let last = *ends.iter().max().unwrap();
    Outcome {
        values: vec![
            ("MiB/s".into(), mib(share * (n - 1), last - start)),
            ("peak B".into(), status("VmHWM") - begin),
        ],
        ok: true,
    }
}

// see sw2m/pdbd-bench mux/README.md "Scenarios"
fn pingpong(d: &mut Duo, load: usize) -> Outcome {
    let idx = d.opened(Proto::Tcp, 1 + load);
    let ping = idx[0];
    let busy = idx[1..].to_vec();
    let mut trips = Vec::with_capacity(d.p.pings);
    for round in 0..d.p.warm + d.p.pings {
        let start = Instant::now();
        let (out, back) = (
            d.ends[1][ping].got + d.p.message,
            d.ends[0][ping].got + d.p.message,
        );
        d.send(0, ping, d.p.message);
        d.pump();
        let mut echoed = false;
        let mut fresh = vec![0; busy.len()];
        while d.ends[0][ping].got < back {
            d.fill(&busy, usize::MAX, &mut fresh);
            if d.ends[1][ping].got >= out && !echoed {
                d.send(1, ping, d.p.message);
                echoed = true;
            }
            d.pump();
            if start.elapsed().as_secs_f64() > d.p.timeout {
                fail("a round trip missed the deadline");
            }
        }
        if round >= d.p.warm {
            trips.push(start.elapsed().as_secs_f64() * 1e6);
        }
    }
    let mut values = Vec::new();
    percentiles(trips, "", &mut values);
    Outcome { values, ok: true }
}

fn churn(d: &mut Duo) -> Outcome {
    let mut waits = Vec::with_capacity(d.p.churn);
    let start = Instant::now();
    for _ in 0..d.p.churn {
        let before = d.arrived.len();
        let opened = Instant::now();
        let i = d.opened(Proto::Tcp, 1)[0];
        waits.push((d.arrived[before] - opened).as_secs_f64() * 1e6);
        let back = d.ends[0][i].got + 1;
        d.send(1, i, 1);
        if !d.until(|d| d.ends[0][i].got >= back) {
            fail("an echo missed the deadline");
        }
        let id = d.ids[i];
        d.near
            .close(id)
            .unwrap_or_else(|e| fail(&format!("close: {e:?}")));
        d.far
            .close(id)
            .unwrap_or_else(|e| fail(&format!("close: {e:?}")));
        if !d.until(|d| d.near.flow(id).is_none() && d.far.flow(id).is_none()) {
            fail("a close missed the deadline");
        }
    }
    let spent = start.elapsed();
    let mut values =
        vec![("cycles/s".into(), d.p.churn as f64 / spent.as_secs_f64())];
    percentiles(waits, "open ", &mut values);
    Outcome { values, ok: true }
}

fn idle(d: &mut Duo) -> Outcome {
    let before = rss();
    d.opened(Proto::Tcp, d.p.idle);
    std::thread::sleep(Duration::from_millis(50)); // see sw2m/pdbd-bench mux/README.md "Scenarios"
    let after = rss();
    Outcome {
        values: vec![("B/flow".into(), (after - before) / d.p.idle as f64)],
        ok: true,
    }
}

fn udp(d: &mut Duo) -> Outcome {
    let (size, total) = (d.p.datagram, d.p.datagrams);
    if size == 0 {
        fail("udp needs datagrams of at least 1 byte");
    }
    let i = d.opened(Proto::Udp, 1)[0];
    let start = Instant::now();
    let mut datagrams = 0;
    let mut seen = d.ends[1][i].got;
    d.send(0, i, size * total);
    while datagrams < total {
        d.pump();
        let got = d.ends[1][i].got;
        datagrams += (got - seen) / size;
        seen += (got - seen) / size * size;
        if start.elapsed().as_secs_f64() > d.p.timeout {
            fail("udp missed the deadline");
        }
    }
    Outcome {
        values: vec![(
            "datagrams/s".into(),
            total as f64 / start.elapsed().as_secs_f64(),
        )],
        ok: true,
    }
}

fn once(loopback: bool, scenario: &str, p: Params) -> Outcome {
    let mut d = Duo::new(loopback, p);
    match scenario {
        "bulk" => bulk(&mut d),
        "flows" => flows(&mut d),
        "isolation" => isolation(&mut d, true),
        "calm" => isolation(&mut d, false),
        "pingpong" => pingpong(&mut d, 0),
        "loaded" => pingpong(&mut d, p.load),
        "churn" => churn(&mut d),
        "idle" => idle(&mut d),
        "udp" => udp(&mut d),
        other => fail(&format!("unsupported scenario {other}")),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.len() != 4 || !matches!(args[1].as_str(), "core" | "loopback") {
        fail("usage: muxbench <core|loopback> <scenario> <params json> | muxbench version <impl>");
    }
    let (loopback, scenario, p) = (
        args[1] == "loopback",
        args[2].as_str(),
        Params::parse(&args[3]),
    );
    // see sw2m/pdbd-bench mux/README.md "Statistics"
    if scenario == "isolation" {
        once(loopback, "calm", p);
    }
    once(loopback, scenario, p);
    let mut calm = None;
    if scenario == "isolation" {
        let baseline = once(loopback, "calm", p);
        if !baseline.ok {
            fail("the calm baseline missed the deadline");
        }
        calm = Some(baseline.values[0].1);
        std::fs::write("/proc/self/clear_refs", "5")
            .unwrap_or_else(|e| fail(&format!("resetting the peak RSS: {e}")));
    }
    let before = cpu();
    let mut outcome = once(loopback, scenario, p);
    outcome.values.push(("cpu ms".into(), cpu() - before));
    if let (Some(calm), true) = (calm, outcome.ok) {
        let stalled = outcome.values[0].1;
        outcome.values.push(("calm MiB/s".into(), calm));
        outcome.values.push(("ratio".into(), stalled / calm));
    }
    let values: Vec<String> = outcome
        .values
        .iter()
        .map(|(k, v)| format!("\"{k}\":{v}"))
        .collect();
    let verdict = if outcome.ok { "ok" } else { "err" };
    println!(
        "{{\"values\":{{{}}},\"verdict\":\"{verdict}\"}}",
        values.join(",")
    );
}
