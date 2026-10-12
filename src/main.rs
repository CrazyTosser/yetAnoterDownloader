mod bencode;
pub mod bittorrent;

use bencode::*;
use bittorrent::*;
use tokio_util::sync::CancellationToken;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<(), tokio::io::Error> {
    let data = std::fs::read("C:\\Users\\user\\RustroverProjects\\yetAnoterDownloader\\tests\\fixtures\\ubuntu-26.04.1-desktop-amd64.iso.torrent").unwrap();
    let parse_res = decode(&data).unwrap();
    let mut bittorrent_loader = request::announcement(parse_res).await.unwrap();
    
    let cancellation = CancellationToken::new();
    let cancellation_token = cancellation.clone();

    tokio::spawn(async move {
        bittorrent_loader.updater_loop(cancellation_token.clone()).await.unwrap();
    });

    tokio::select! {
        _ = signal::ctrl_c() => {
            cancellation.cancel();
        }
    }

    Ok(())
}
