use std::path::PathBuf;
use ::errors::AppError;
use bencode::*;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use tracker::*;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let path = "C:\\Users\\user\\RustroverProjects\\yetAnoterDownloader\\crates\\bencode\\tests\\fixtures\\ubuntu-26.04.1-desktop-amd64.iso.torrent";

    let torrent_file = match std::fs::read(path) {
        Ok(data) => {
            info!(path = %path, "file red");
            match decode(&data) {
                Ok(torrent) => {
                    info!(path = %path, "file decoded");
                    torrent
                }
                Err(e) => {
                    error!(error = ?e, "decode error");
                    return Err(e.into());
                }
            }
        }
        Err(e) => {
            error!(error = ?e, "IO file");
            return Err(e.into());
        }
    };

    let (mut announcer, chan, mut manager_cfg) = announcement(torrent_file, 6881u16).await?;
    manager_cfg.output_path = PathBuf::from("C:\\Users\\user\\RustroverProjects\\yetAnoterDownloader\\crates\\bencode\\tests\\fixtures\\ubuntu-26.04.1-desktop-amd64.iso.torrent");
    let mut manager = peer::PeerManager::new(
        chan.peers_rx,
        chan.stats_tx,
        manager_cfg,
    );

    let cancellation = CancellationToken::new();
    let cancellation_announcer = cancellation.clone();
    let cancellation_manager = cancellation.clone();

    tokio::spawn(async move {
        manager.run(cancellation_manager).await.unwrap();
    });
    tokio::spawn(async move {
        announcer
            .updater_loop(cancellation_announcer)
            .await
            .unwrap();
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            cancellation.cancel();
        }
    }

    Ok(())
}
