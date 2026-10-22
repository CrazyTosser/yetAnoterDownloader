use bencode::*;
use torrent::*;
use tracker::*;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use ::errors::AppError;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let data = std::fs::read("C:\\Users\\user\\RustroverProjects\\yetAnoterDownloader\\tests\\fixtures\\ubuntu-26.04.1-desktop-amd64.iso.torrent")?;
    let torrent_file = decode(&data)?;
    is_valid_file(&torrent_file)?;
    let (mut announcer, manager_chan) = announcement(torrent_file).await.unwrap();
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
