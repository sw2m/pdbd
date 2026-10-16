mod h2;
mod ours;
mod scenario;
mod smux;
mod stream;
mod yamux;

use criterion::{Criterion, SamplingMode};
use scenario::{Count, Results, Scenario, RUNS, SPREADS};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use stream::{Acceptor, Opener};
use tokio::runtime::Runtime;

#[global_allocator]
static ALLOC: Count = Count;

fn ours(c: &mut Criterion, results: &Results, tcp: bool) {
    let mux = match tcp {
        true => "pdbd mux, loopback (rust)",
        false => "pdbd mux, sans-IO core (rust)",
    };
    let mut g = c.benchmark_group(mux);
    g.sampling_mode(SamplingMode::Flat);
    let mut time =
        |name: Scenario, run: &dyn Fn(&mut ours::Duo) -> (Duration, f64)| {
            g.bench_function(name.key(), |b| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let (spent, value) = run(&mut ours::Duo::new(tcp));
                        results.add(mux, name, value);
                        total += spent;
                    }
                    total
                })
            });
        };
    time(Scenario::Bulk, &|d| d.bulk());
    for n in SPREADS {
        let (flows, fair) = Scenario::flows(n);
        time(flows, &|d| {
            let (spent, rate, spread) = d.flows(n);
            results.add(mux, fair, spread);
            (spent, rate)
        });
    }
    time(Scenario::Pingpong, &|d| d.pingpong());
    time(Scenario::Churn, &|d| d.churn());
    time(Scenario::Udp, &|d| d.udp());
    g.finish();
    for _ in 0..RUNS {
        results.add(mux, Scenario::Idle, ours::Duo::new(tcp).idle());
    }
}

fn theirs<O, A, S>(
    c: &mut Criterion,
    rt: &Runtime,
    results: &Arc<Results>,
    mux: &'static str,
    session: fn() -> S,
) where
    O: Opener,
    A: Acceptor,
    S: Future<Output = (O, A)> + Send + 'static,
{
    let mut g = c.benchmark_group(mux);
    g.sampling_mode(SamplingMode::Flat);
    let timed = |g: &mut criterion::BenchmarkGroup<_>, name: Scenario| {
        let results = results.clone();
        g.bench_function(name.key(), move |b| {
            b.iter_custom(|iters| {
                rt.block_on(async {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        let (o, mut a) = session().await;
                        let (spent, value) = match name {
                            Scenario::Bulk => stream::bulk(&o, &mut a).await,
                            Scenario::Pingpong => {
                                stream::pingpong(&o, &mut a).await
                            }
                            Scenario::Churn => stream::churn(&o, &mut a).await,
                            _ => {
                                let n = match name {
                                    Scenario::Flows16 => 16,
                                    _ => 256,
                                };
                                let (spent, rate, spread) =
                                    stream::flows(&o, &mut a, n).await;
                                results.add(mux, Scenario::flows(n).1, spread);
                                (spent, rate)
                            }
                        };
                        results.add(mux, name, value);
                        total += spent;
                    }
                    total
                })
            })
        });
    };
    for name in [
        Scenario::Bulk,
        Scenario::Flows16,
        Scenario::Flows256,
        Scenario::Pingpong,
        Scenario::Churn,
    ] {
        timed(&mut g, name);
    }
    g.finish();
    for _ in 0..RUNS {
        let per = rt.block_on(async {
            let (o, mut a) = session().await;
            stream::idle(&o, &mut a).await
        });
        results.add(mux, Scenario::Idle, per);
    }
}

fn main() {
    let results = Arc::new(Results::default());
    let rt = Runtime::new().unwrap();
    let mut c = Criterion::default()
        .configure_from_args()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2));
    ours(&mut c, &results, false);
    ours(&mut c, &results, true);
    theirs(&mut c, &rt, &results, "yamux 0.14.1 (rust)", yamux::session);
    theirs(
        &mut c,
        &rt,
        &results,
        "async_smux 0.4.0 (rust)",
        smux::session,
    );
    theirs(&mut c, &rt, &results, "h2 0.4.19 (rust)", h2::session);
    c.final_summary();
    if let Ok(dir) = std::env::var("MUX_PEERS") {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if let Ok(tsv) = std::fs::read_to_string(entry.path()) {
                results.load(&tsv);
            }
        }
    }
    let table = results.table();
    println!("\n{table}");
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();
        writeln!(f, "### mux benchmarks\n\n{table}").unwrap();
    }
}
