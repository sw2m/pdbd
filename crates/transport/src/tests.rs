use super::*;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

struct ChanSocket {
    rx: mpsc::Receiver<Vec<u8>>,
    tx: mpsc::Sender<Vec<u8>>,
}

impl Socket for ChanSocket {
    async fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.rx.recv().await)
    }
    async fn send(&mut self, guest: &[u8]) -> io::Result<()> {
        self.tx
            .send(guest.to_vec())
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
    }
}

// #12
#[tokio::test]
async fn splice_copies_both_ways() {
    let (l_in_tx, l_in_rx) = mpsc::channel::<Vec<u8>>(8);
    let (l_out_tx, mut l_out_rx) = mpsc::channel::<Vec<u8>>(8);
    let (r_in_tx, r_in_rx) = mpsc::channel::<Vec<u8>>(8);
    let (r_out_tx, mut r_out_rx) = mpsc::channel::<Vec<u8>>(8);
    let left = ChanSocket {
        rx: l_in_rx,
        tx: l_out_tx,
    };
    let right = ChanSocket {
        rx: r_in_rx,
        tx: r_out_tx,
    };
    tokio::spawn(splice(left, right));

    l_in_tx.send(vec![1, 2, 3]).await.unwrap();
    let got = timeout(Duration::from_secs(1), r_out_rx.recv())
        .await
        .unwrap();
    assert_eq!(got, Some(vec![1, 2, 3]));

    r_in_tx.send(vec![4, 5, 6]).await.unwrap();
    let got = timeout(Duration::from_secs(1), l_out_rx.recv())
        .await
        .unwrap();
    assert_eq!(got, Some(vec![4, 5, 6]));
}
