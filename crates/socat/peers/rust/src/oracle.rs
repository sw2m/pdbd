use crate::corpus::Entry;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub const NAMES: [&str; 4] = ["socat", "websocat", "oittaa", "tocat"];

const LISTEN: Duration = Duration::from_millis(400); // past socat's parse, an open that blocks (a LISTEN) has accepted the syntax
const STUCK: Duration = Duration::from_secs(5); // a stderr drain that takes longer is a hung oracle
const RESOLVE: Duration = Duration::from_secs(3); // websocat resolves host names while building the spec: one still running has parsed it
const WORKERS: usize = 16; // socat/websocat run one process per entry

const SYNTAX: &[&str] = &[
    "unknown device/address",
    "wrong number of parameters",
    "parseopts",
    "unknown option",
    "not supported with this address type",
    "not allowed without",
    "syntax error",
    "unbalanced",
    "missing numerical value",
    "nestlex",
    "parse left address",
    "parse right address",
];

pub struct Oracles {
    pub socat: PathBuf,
    pub websocat: PathBuf,
    pub oittaa: PathBuf,
    pub sandbox: PathBuf,
    pub versions: [String; 4],
}

impl Oracles {
    pub fn judge(&self, entries: &[Entry]) -> Vec<[bool; 4]> {
        let socat = pool(entries, |s| self.ask(&self.socat, s));
        let websocat = pool(entries, |s| self.dump(s));
        let oittaa = pool(entries, |s| self.ask(&self.oittaa, s));
        let tocat: Vec<bool> = thread::scope(|scope| {
            thread::Builder::new()
                .name("tocat".into())
                .spawn_scoped(scope, || {
                    entries.iter().map(|e| tocat(&e.text)).collect()
                })
                .expect("spawn tocat")
                .join()
                .expect("tocat thread")
        });
        (0..entries.len())
            .map(|i| [socat[i], websocat[i], oittaa[i], tocat[i]])
            .collect()
    }

    fn ask(&self, program: &PathBuf, s: &str) -> bool {
        let mut child = Command::new(program)
            .args(["-d", "-d", s, "/dev/null"])
            .env_clear()
            .env("PATH", "/nonexistent")
            .current_dir(&self.sandbox)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {program:?}: {e}"));
        let mut err = child.stderr.take().expect("socat stderr");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut text = String::new();
            let _ = err.read_to_string(&mut text);
            let _ = tx.send(text);
        });
        let start = Instant::now();
        while child.try_wait().expect("wait socat").is_none() {
            if start.elapsed() > LISTEN {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        !syntax(&rx.recv_timeout(STUCK).unwrap_or_default())
    }

    fn dump(&self, s: &str) -> bool {
        let mut child = Command::new(&self.websocat)
            .args(["-t", "-E", "--dump-spec", s, "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn websocat");
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect("wait websocat") {
                return status.success();
            }
            if start.elapsed() > RESOLVE {
                let _ = child.kill();
                let _ = child.wait();
                return true;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn pool(entries: &[Entry], f: impl Fn(&str) -> bool + Sync) -> Vec<bool> {
    let next = AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<bool>> =
        (0..entries.len()).map(|_| false.into()).collect();
    thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(e) = entries.get(i) else { break };
                *slots[i].lock().expect("slot") = f(&e.text);
            });
        }
    });
    slots
        .into_iter()
        .map(|s| s.into_inner().expect("slot"))
        .collect()
}

fn syntax(stderr: &str) -> bool {
    stderr.lines().any(|l| SYNTAX.iter().any(|p| l.contains(p)))
}

fn tocat(s: &str) -> bool {
    let s = s.to_string();
    std::panic::catch_unwind(move || {
        s.parse::<tocat_core::endpoint::EndpointSpec>().is_ok()
    })
    .unwrap_or(false)
}
