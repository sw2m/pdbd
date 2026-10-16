use crate::scenario::{
    rate, spread, Count, BULK, CHUNK, CHURN, DATA, IDLE, MSG, PINGS, WARM,
};
use std::future::Future;
use std::io;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub trait Flow: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Flow for T {}

pub trait Opener: Send + Sync + 'static {
    type Flow: Flow;
    fn open(&self) -> impl Future<Output = io::Result<Self::Flow>> + Send;
}

pub trait Acceptor: Send + 'static {
    type Flow: Flow;
    fn accept(&mut self)
        -> impl Future<Output = io::Result<Self::Flow>> + Send;
}

pub async fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (near, far) = tokio::join!(TcpStream::connect(addr), listener.accept());
    let (near, (far, _)) = (near.unwrap(), far.unwrap());
    near.set_nodelay(true).unwrap();
    far.set_nodelay(true).unwrap();
    (near, far)
}

async fn drain(mut flow: impl Flow, want: usize) -> Instant {
    let mut buf = vec![0; CHUNK];
    let mut got = 0;
    while got < want {
        match flow.read(&mut buf).await.unwrap() {
            0 => panic!("EOF after {got} of {want} bytes"),
            n => got += n,
        }
    }
    Instant::now()
}

async fn fill(mut flow: impl Flow, want: usize) -> impl Flow {
    let mut sent = 0;
    while sent < want {
        let n = CHUNK.min(want - sent);
        flow.write_all(&DATA[..n]).await.unwrap();
        sent += n;
    }
    flow.flush().await.unwrap();
    flow
}

pub async fn bulk<O: Opener, A: Acceptor>(o: &O, a: &mut A) -> (Duration, f64) {
    let start = Instant::now();
    let server = async { drain(a.accept().await.unwrap(), BULK).await };
    let client = async { fill(o.open().await.unwrap(), BULK).await };
    let (end, flow) = tokio::join!(server, client);
    drop(flow);
    let spent = end - start;
    (spent, rate(BULK, spent))
}

pub async fn flows<O: Opener, A: Acceptor>(
    o: &O,
    a: &mut A,
    n: usize,
) -> (Duration, f64, f64) {
    let share = BULK / n;
    let server = async {
        let mut flows = Vec::with_capacity(n);
        for _ in 0..n {
            flows.push(a.accept().await.unwrap());
        }
        flows
    };
    let client = async {
        let mut flows = Vec::with_capacity(n);
        for _ in 0..n {
            let mut f = o.open().await.unwrap();
            f.write_all(&DATA[..1]).await.unwrap();
            flows.push(f);
        }
        flows
    };
    let (inbound, outbound) = tokio::join!(server, client);
    let mut readers = Vec::with_capacity(n);
    for mut f in inbound {
        readers.push(tokio::spawn(async move {
            let mut one = [0];
            f.read_exact(&mut one).await.unwrap();
            drain(f, share).await
        }));
    }
    let start = Instant::now();
    let writers: Vec<_> = outbound
        .into_iter()
        .map(|f| tokio::spawn(fill(f, share)))
        .collect();
    let mut ends = Vec::with_capacity(n);
    for r in readers {
        ends.push(r.await.unwrap());
    }
    for w in writers {
        drop(w.await.unwrap());
    }
    let spent = ends.iter().max().unwrap().duration_since(start);
    let rates: Vec<f64> =
        ends.iter().map(|e| rate(share, *e - start)).collect();
    (spent, rate(share * n, spent), spread(&rates))
}

pub async fn pingpong<O: Opener, A: Acceptor>(
    o: &O,
    a: &mut A,
) -> (Duration, f64) {
    let (far, near) = tokio::join!(a.accept(), async {
        let mut f = o.open().await.unwrap();
        f.write_all(&DATA[..MSG]).await.unwrap();
        f
    });
    let (mut far, mut near) = (far.unwrap(), near);
    let echo = tokio::spawn(async move {
        let mut buf = [0; MSG];
        while far.read_exact(&mut buf).await.is_ok() {
            if far.write_all(&buf).await.is_err() {
                break;
            }
        }
    });
    let mut buf = [0; MSG];
    near.read_exact(&mut buf).await.unwrap();
    let mut trips = Vec::with_capacity(PINGS);
    for i in 0..WARM + PINGS {
        let start = Instant::now();
        near.write_all(&DATA[..MSG]).await.unwrap();
        near.read_exact(&mut buf).await.unwrap();
        if i >= WARM {
            trips.push(start.elapsed());
        }
    }
    near.shutdown().await.ok();
    drop(near);
    echo.abort();
    let total = trips.iter().sum();
    trips.sort();
    (total, trips[trips.len() / 2].as_secs_f64() * 1e6)
}

async fn answer(mut f: impl Flow) {
    let mut one = [0];
    f.read_exact(&mut one).await.unwrap();
    f.write_all(&one).await.unwrap();
    f.shutdown().await.unwrap();
    while f.read(&mut one).await.unwrap_or(0) > 0 {}
}

pub async fn churn<O: Opener, A: Acceptor>(
    o: &O,
    a: &mut A,
) -> (Duration, f64) {
    let start = Instant::now();
    let server = async {
        let mut tasks = Vec::with_capacity(CHURN);
        for _ in 0..CHURN {
            tasks.push(tokio::spawn(answer(a.accept().await.unwrap())));
        }
        for t in tasks {
            t.await.unwrap();
        }
    };
    let client = async {
        for _ in 0..CHURN {
            let mut f = o.open().await.unwrap();
            let mut one = [0];
            f.write_all(&DATA[..1]).await.unwrap();
            f.read_exact(&mut one).await.unwrap();
            f.shutdown().await.unwrap();
            while f.read(&mut one).await.unwrap_or(0) > 0 {}
        }
    };
    tokio::join!(server, client);
    let spent = start.elapsed();
    (spent, CHURN as f64 / spent.as_secs_f64())
}

pub async fn idle<O: Opener, A: Acceptor>(o: &O, a: &mut A) -> f64 {
    let before = Count::live();
    let server = async {
        let mut flows = Vec::with_capacity(IDLE);
        for _ in 0..IDLE {
            let mut f = a.accept().await.unwrap();
            let mut one = [0];
            f.read_exact(&mut one).await.unwrap();
            f.write_all(&one).await.unwrap();
            flows.push(f);
        }
        flows
    };
    let client = async {
        let mut flows = Vec::with_capacity(IDLE);
        for _ in 0..IDLE {
            let mut f = o.open().await.unwrap();
            let mut one = [0];
            f.write_all(&DATA[..1]).await.unwrap();
            f.read_exact(&mut one).await.unwrap();
            flows.push(f);
        }
        flows
    };
    let (far, near) = tokio::join!(server, client);
    tokio::time::sleep(Duration::from_millis(50)).await; // let the drivers settle their buffers
    let per = (Count::live() - before) as f64 / IDLE as f64;
    drop((far, near));
    per
}
