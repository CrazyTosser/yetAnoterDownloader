pub mod connection;
mod message;
pub mod state;

use crate::state::DownloadState;
use config::MangerConfig;
use connection::PeerConnection;
use futures::{StreamExt, stream};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::select;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;
use torrent::StatsUpdate;

pub struct PeerManager {
    peers_rx: mpsc::Receiver<Vec<SocketAddr>>,
    stats_tx: Sender<StatsUpdate>,
    cfg: MangerConfig,
    download_state: Arc<Mutex<DownloadState>>,
    peers: HashMap<SocketAddr, PeerConnection>,
}

impl PeerManager {
    pub fn new(
        peers_rx: mpsc::Receiver<Vec<SocketAddr>>,
        stats_tx: Sender<StatsUpdate>,
        manager_cfg: MangerConfig,
    ) -> Self {
        Self {
            peers_rx,
            stats_tx,
            cfg: manager_cfg.clone(),
            download_state: Arc::new(Mutex::new(DownloadState::new(
                manager_cfg.num_pieces,
                manager_cfg.piece_length,
                manager_cfg.total_size,
                manager_cfg.output_path.clone(),
                manager_cfg.piece_hashes.clone(),
            ))),
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
        let cfg = self.cfg.clone();
        let processes: Vec<_> = peers
            .into_iter()
            .filter(|p| !self.peers.contains_key(p))
            .map(|peer| (peer, self.download_state.clone(), self.stats_tx.clone()))
            .map(|it| async move {
                let conn =
                    PeerConnection::new(&it.0, cfg.peer_id, cfg.info_hash, cfg.num_pieces, it.1, it.2).await;
                (it.0, conn)
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
