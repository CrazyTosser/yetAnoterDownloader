pub mod errors;
pub mod announcer;

pub use crate::errors::RequestError;
use bencode::BencodeValue;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::mpsc;
use torrent::TransferStats;

pub struct Announcer {
    trackers: Vec<Vec<String>>,
    interval: Duration,
    peers_tx: mpsc::Sender<Vec<SocketAddr>>,
    stats_rx: mpsc::Receiver<TransferStats>,
    stats: TransferStats,
}

enum AnnouncementEvents {
    Started,
    Completed,
    Stopped,
}


pub async fn announcement(
    torrent: BencodeValue,
) -> Result<(Announcer, (mpsc::Receiver<Vec<SocketAddr>>, mpsc::Sender<TransferStats>)), Box<dyn Error>> {
    let info = torrent
        .get(b"info")
        .ok_or(RequestError::InfoSectionNotFound)?;
    let info_hash = torrent::calc_info_hash(info);
    let peer_id = torrent::gen_peer_id();
    let stats = TransferStats {
        downloaded: 0,
        uploaded: 0,
        left: info
            .get(b"length")
            .ok_or(RequestError::InfoSectionNotFound)?
            .as_integer()
            .ok_or(RequestError::InfoSectionNotFound)? as u64,
    };

    let announce_list = torrent
        .get(b"announce-list")
        .ok_or(RequestError::AnnounceListSectionNotFound)?
        .as_list()
        .ok_or(RequestError::AnnounceListSectionNotFound)?;
    let prepared_urls: Vec<_> = announce_list
        .iter()
        .filter_map(|tier| tier.as_list())
        .map(|urls| {
            urls.iter()
                .filter_map(|url| url.as_str())
                .map(|tracker| gen_tracker_url(&info_hash, &peer_id, tracker, 6881))
                .collect()
        })
        .filter(|tier: &Vec<String>| !tier.is_empty())
        .collect();

    let interval = info
        .get(b"intervals")
        .ok_or(RequestError::IntervalsSectionNotFound)?
        .as_integer()
        .ok_or(RequestError::IntervalsSectionNotFound)?;

    let (peers_tx, peers_rx) = mpsc::channel(32);
    let (stats_tx, stats_rx) = mpsc::channel(32);
    let service = Announcer::new(
        &prepared_urls,
        Duration::from_secs(interval as u64),
        peers_tx,
        stats_rx,
        stats,
    );
    Ok((service, (peers_rx, stats_tx)))
}

fn gen_tracker_url(info_hash: &[u8], peer_id: &[u8], tracker_url: &str, port: u16) -> String {
    format!(
        "{}?info_hash={}&peer_id={}&port={}",
        tracker_url,
        torrent::percent_encode_binary(&info_hash),
        torrent::percent_encode_binary(&peer_id),
        port,
    )
}
