pub mod errors;
pub mod announcer;

pub use crate::errors::RequestError;
use bencode::BencodeValue;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::mpsc;
use torrent::{compute_total_size, TorrentFileError, TransferStats};

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

impl AnnouncementEvents {
    fn to_str(&self) -> &'static str {
        match self {
            AnnouncementEvents::Started => "started",
            AnnouncementEvents::Completed => "completed",
            AnnouncementEvents::Stopped => "stopped",
        }
    }
}

pub async fn announcement(
    torrent: BencodeValue,
) -> Result<(Announcer, (mpsc::Receiver<Vec<SocketAddr>>, mpsc::Sender<TransferStats>)), RequestError> {
    let info = torrent
        .get(b"info")
        .ok_or(RequestError::InfoSectionNotFound)?;
    let info_hash = torrent::calc_info_hash(info);
    let peer_id = torrent::gen_peer_id();
    let total_size = if info.get(b"length").is_some() {
        info.get(b"length")
            .and_then(|it| it.as_integer())
            .ok_or_else(|| TorrentFileError::WrongType {
                name: "length".to_string(),
            }).unwrap()
    } else {
        compute_total_size(info).unwrap()
    };
    let stats = TransferStats {
        downloaded: 0,
        uploaded: 0,
        left: total_size as u64,
    };

    let announce_list = torrent
        .get(b"announce-list")
        .ok_or(RequestError::AnnounceListSectionNotFound)?
        .as_list()
        .ok_or(RequestError::AnnounceListSectionNotFound)?;
    let mut prepared_urls: Vec<_> = announce_list
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
    if prepared_urls.is_empty() {
        if let Some(url) = torrent.get(b"announce").and_then(|v| v.as_str()) {
            prepared_urls.push(vec![gen_tracker_url(&info_hash, &peer_id, url, 6881)]);
        } else {
            return Err(RequestError::AnnounceListSectionNotFound);
        }
    }

    let (peers_tx, peers_rx) = mpsc::channel(32);
    let (stats_tx, stats_rx) = mpsc::channel(32);
    let service = Announcer::new(
        &prepared_urls,
        Duration::new(1800, 0),
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
