use bencode::*;
use tracker::*;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use ::errors::AppError;
use tracing_subscriber::{EnvFilter};

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

    let (mut announcer, manager_chan) = announcement(torrent_file).await?;
    let mut manager = peer::PeerManager::new(manager_chan.0, manager_chan.1);


    let cancellation = CancellationToken::new();
    let cancellation_announcer = cancellation.clone();
    let cancellation_manager = cancellation.clone();

    tokio::spawn(async move {
        manager.run(cancellation_manager).await.unwrap();
    });
    tokio::spawn(async move {
        announcer.updater_loop(cancellation_announcer).await.unwrap();
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            cancellation.cancel();
        }
    }

    Ok(())
}
