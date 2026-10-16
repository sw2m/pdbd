use crate::scenario::{
    rate, spread, Count, BULK, CHUNK, CHURN, DATA, DGRAM, DGRAMS, IDLE, MSG,
    PINGS, WARM, WINDOW,
};
use mux::{Event, Multi, Proto, Side, Transmit};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

const CAP: u32 = 4096; // above the largest scenario's flow count
const BUDGET: u64 = 1 << 30; // never the bottleneck: every flow's window fits

struct Socket {
    tcp: TcpStream,
    out: Vec<u8>,
    at: usize,
}

impl Socket {
    fn write(&mut self) {
        while self.at < self.out.len() {
            match self.tcp.write(&self.out[self.at..]) {
                Ok(n) => self.at += n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("loopback write: {e}"),
            }
        }
        if self.at == self.out.len() {
            self.out.clear();
            self.at = 0;
        }
    }

    fn read(&mut self, scratch: &mut [u8], into: &mut Multi) {
        loop {
            match self.tcp.read(scratch) {
                Ok(0) => break,
                Ok(n) => into.feed(&scratch[..n]).unwrap(),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("loopback read: {e}"),
            }
        }
    }
}

pub struct Duo {
    pub c: Multi,
    pub d: Multi,
    link: Option<[Socket; 2]>,
    scratch: Vec<u8>,
}

impl Duo {
    pub fn new(tcp: bool) -> Self {
        let c = Multi::new(Side::Client, WINDOW, CAP, BUDGET).unwrap();
        let d = Multi::new(Side::Daemon, WINDOW, CAP, BUDGET).unwrap();
        let link = tcp.then(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let near =
                TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (far, _) = listener.accept().unwrap();
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
        Self {
            c,
            d,
            link,
            scratch: vec![0; 1 << 18],
        }
    }

    pub fn pump(&mut self) {
        let Some(link) = &mut self.link else {
            while let Some(Transmit::Bytes(b)) = self.c.transmit() {
                self.d.feed(&b).unwrap();
            }
            while let Some(Transmit::Bytes(b)) = self.d.transmit() {
                self.c.feed(&b).unwrap();
            }
            return;
        };
        while let Some(Transmit::Bytes(b)) = self.c.transmit() {
            link[0].out.extend(b);
        }
        while let Some(Transmit::Bytes(b)) = self.d.transmit() {
            link[1].out.extend(b);
        }
        link[0].write();
        link[1].write();
        link[1].read(&mut self.scratch, &mut self.d);
        link[0].read(&mut self.scratch, &mut self.c);
    }

    fn quiet(m: &mut Multi) {
        while m.poll().is_some() {}
    }

    pub fn open(&mut self, proto: Proto, n: usize) -> Vec<u32> {
        let ids: Vec<u32> =
            (0..n).map(|_| self.c.open(proto).unwrap()).collect();
        let mut left = n;
        while left > 0 {
            self.pump();
            while let Some(e) = self.d.poll() {
                if let Event::Open(id, _) = e {
                    self.d.accept(id).unwrap();
                }
            }
            while let Some(e) = self.c.poll() {
                if let Event::Accepted(_) = e {
                    left -= 1;
                }
            }
        }
        ids
    }

    pub fn close(&mut self, ids: &[u32]) {
        for &id in ids {
            self.c.close(id).unwrap();
            self.d.close(id).unwrap();
        }
        while !(self.c.is_empty() && self.d.is_empty()) {
            self.pump();
            Self::quiet(&mut self.c);
            Self::quiet(&mut self.d);
        }
    }

    fn take(m: &mut Multi, got: &mut [usize], ids: &[u32]) {
        while let Some(e) = m.poll() {
            if let Event::Data(id, b) = e {
                m.consume(id, b.len() as u32).unwrap();
                if let Some(i) = ids.iter().position(|&x| x == id) {
                    got[i] += b.len();
                }
            }
        }
    }

    pub fn bulk(&mut self) -> (Duration, f64) {
        let ids = self.open(Proto::Tcp, 1);
        let start = Instant::now();
        let (mut sent, mut got) = (0, [0]);
        while got[0] < BULK {
            while sent < BULK {
                match self.c.send(ids[0], &DATA[..CHUNK.min(BULK - sent)]) {
                    Ok(n) => sent += n,
                    Err(_) => break,
                }
            }
            self.pump();
            Self::take(&mut self.d, &mut got, &ids);
            Self::quiet(&mut self.c);
        }
        let spent = start.elapsed();
        self.close(&ids);
        (spent, rate(BULK, spent))
    }

    pub fn flows(&mut self, n: usize) -> (Duration, f64, f64) {
        let ids = self.open(Proto::Tcp, n);
        let share = BULK / n;
        let start = Instant::now();
        let mut sent = vec![0; n];
        let mut got = vec![0; n];
        let mut done = vec![None; n];
        while done.iter().any(Option::is_none) {
            for (i, &id) in ids.iter().enumerate() {
                if sent[i] < share {
                    if let Ok(k) =
                        self.c.send(id, &DATA[..CHUNK.min(share - sent[i])])
                    {
                        sent[i] += k;
                    }
                }
            }
            self.pump();
            Self::take(&mut self.d, &mut got, &ids);
            Self::quiet(&mut self.c);
            for i in 0..n {
                if done[i].is_none() && got[i] == share {
                    done[i] = Some(start.elapsed());
                }
            }
        }
        let spent = start.elapsed();
        let rates: Vec<f64> =
            done.iter().map(|t| rate(share, t.unwrap())).collect();
        self.close(&ids);
        (spent, rate(share * n, spent), spread(&rates))
    }

    pub fn pingpong(&mut self) -> (Duration, f64) {
        let ids = self.open(Proto::Tcp, 1);
        let id = ids[0];
        let mut trips = Vec::with_capacity(PINGS);
        for i in 0..WARM + PINGS {
            let start = Instant::now();
            self.c.send(id, &DATA[..MSG]).unwrap();
            let mut got = [0];
            while got[0] < MSG {
                self.pump();
                Self::take(&mut self.d, &mut got, &ids);
            }
            self.d.send(id, &DATA[..MSG]).unwrap();
            let mut got = [0];
            while got[0] < MSG {
                self.pump();
                Self::take(&mut self.c, &mut got, &ids);
            }
            if i >= WARM {
                trips.push(start.elapsed());
            }
        }
        self.close(&ids);
        let total = trips.iter().sum();
        trips.sort();
        (total, trips[trips.len() / 2].as_secs_f64() * 1e6)
    }

    pub fn churn(&mut self) -> (Duration, f64) {
        let start = Instant::now();
        for _ in 0..CHURN {
            let ids = self.open(Proto::Tcp, 1);
            self.c.send(ids[0], &DATA[..1]).unwrap();
            let mut got = [0];
            while got[0] < 1 {
                self.pump();
                Self::take(&mut self.d, &mut got, &ids);
            }
            self.d.send(ids[0], &DATA[..1]).unwrap();
            let mut got = [0];
            while got[0] < 1 {
                self.pump();
                Self::take(&mut self.c, &mut got, &ids);
            }
            self.close(&ids);
        }
        let spent = start.elapsed();
        (spent, CHURN as f64 / spent.as_secs_f64())
    }

    pub fn idle(&mut self) -> f64 {
        let before = Count::live();
        let ids = self.open(Proto::Tcp, IDLE);
        for &id in &ids {
            self.c.send(id, &DATA[..1]).unwrap();
        }
        let mut got = vec![0; IDLE];
        while got.iter().any(|&g| g < 1) {
            self.pump();
            Self::take(&mut self.d, &mut got, &ids);
        }
        for &id in &ids {
            self.d.send(id, &DATA[..1]).unwrap();
        }
        let mut got = vec![0; IDLE];
        while got.iter().any(|&g| g < 1) {
            self.pump();
            Self::take(&mut self.c, &mut got, &ids);
        }
        let per = (Count::live() - before) as f64 / IDLE as f64;
        self.close(&ids);
        per
    }

    pub fn udp(&mut self) -> (Duration, f64) {
        let ids = self.open(Proto::Udp, 1);
        let start = Instant::now();
        let (mut sent, mut got) = (0, 0);
        while got < DGRAMS {
            while sent < DGRAMS && self.c.send(ids[0], &DATA[..DGRAM]).is_ok() {
                sent += 1;
            }
            self.pump();
            while let Some(e) = self.d.poll() {
                if let Event::Data(id, b) = e {
                    self.d.consume(id, b.len() as u32).unwrap();
                    got += 1;
                }
            }
            Self::quiet(&mut self.c);
        }
        let spent = start.elapsed();
        self.close(&ids);
        (spent, DGRAMS as f64 / spent.as_secs_f64())
    }
}
