use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::{select};
use futures::{StreamExt, stream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

enum PeerState {
    Idle,
}

#[derive(Clone, Copy)]
pub struct TransferStats {
    pub downloaded: u64,
    pub uploaded: u64,
    pub left: u64,
}

struct PeerConnection {
    state: PeerState,
    stream: TcpStream,
    last_seen: Instant,
}

impl PeerConnection {
    async fn new(addr: &SocketAddr) -> Result<PeerConnection, std::io::Error> {
        let stream =
            tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr)).await??;
        Ok(Self {
            state: PeerState::Idle,
            stream,
            last_seen: Instant::now(),
        })
    }
}

pub struct PeerManager {
    peers_rx: mpsc::Receiver<Vec<SocketAddr>>,
    stats_tx: Sender<TransferStats>,
    peers: HashMap<SocketAddr, PeerConnection>,
}

impl PeerManager {
    pub fn new(peers_rx: mpsc::Receiver<Vec<SocketAddr>>, stats_tx: Sender<TransferStats>) -> Self {
        Self {
            peers_rx,
            stats_tx,
            peers: HashMap::new(),
        }
    }

    pub async fn run(
        &mut self,
        cancellation_token: CancellationToken,
    ) -> Result<(), Box<dyn Error>> {
        loop {
            select! {
                Some(new_peers) = self.peers_rx.recv() => {
                    self.connect_new_peers(new_peers).await;
                }
                _ = cancellation_token.cancelled() => {
                    break Ok(());
                }
            }
        }
    }

    async fn connect_new_peers(&mut self, peers: Vec<SocketAddr>) {
        let processes: Vec<_> = peers
            .into_iter()
            .filter(|p| !self.peers.contains_key(p))
            .map(async move |peer| {
                let conn = PeerConnection::new(&peer).await;
                (peer, conn)
            })
            .collect();
        let results: Vec<_> = stream::iter(processes).buffer_unordered(10).collect().await;
        for (addr, res) in results {
            if let Ok(conn) = res {
                self.peers.insert(addr, conn);
            }
        }
    }
}
