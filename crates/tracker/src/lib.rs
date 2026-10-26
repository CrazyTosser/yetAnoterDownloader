pub mod announcer;
pub mod errors;

pub use crate::errors::RequestError;
use ::config::MangerConfig;
use bencode::BencodeValue;
use futures::StreamExt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use torrent::{compute_total_size, StatsUpdate, TorrentFileError, TransferStats};

pub struct Announcer {
    trackers: Vec<Vec<String>>,
    interval: Duration,
    peers_tx: mpsc::Sender<Vec<SocketAddr>>,
    stats_rx: mpsc::Receiver<StatsUpdate>,
    stats: TransferStats,
    complete_sent: bool,
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

pub struct Chan {
    pub peers_rx: mpsc::Receiver<Vec<SocketAddr>>,
    pub stats_tx: mpsc::Sender<StatsUpdate>,
}

pub async fn announcement(
    torrent: BencodeValue,
    port: u16,
) -> Result<(Announcer, Chan, MangerConfig), RequestError> {
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
            })
            .unwrap()
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
                .map(|tracker| gen_tracker_url(&info_hash, &peer_id, tracker, port))
                .collect()
        })
        .filter(|tier: &Vec<String>| !tier.is_empty())
        .collect();
    if prepared_urls.is_empty() {
        if let Some(url) = torrent.get(b"announce").and_then(|v| v.as_str()) {
            prepared_urls.push(vec![gen_tracker_url(&info_hash, &peer_id, url, port)]);
        } else {
            return Err(RequestError::AnnounceListSectionNotFound);
        }
    }
    let piece_length = info
        .get(b"piece length")
        .and_then(|it| it.as_integer())
        .ok_or(RequestError::InfoSectionNotFound)?;
    let pieces = info
        .get(b"pieces")
        .and_then(|v| v.as_bytes())
        .ok_or(RequestError::PiecesListEmpty)?
        .chunks_exact(20)
        .map(|it| {
            let mut hash = [0u8; 20];
            hash.copy_from_slice(it);
            hash
        })
        .collect();

    let (peers_tx, peers_rx) = mpsc::channel(32);
    let (stats_tx, stats_rx) = mpsc::channel(32);
    let service = Announcer::new(
        &prepared_urls,
        Duration::new(1800, 0),
        peers_tx,
        stats_rx,
        stats,
    );
    Ok((
        service,
        Chan { peers_rx, stats_tx },
        MangerConfig {
            peer_id,
            info_hash,
            total_size: total_size as u64,
            piece_length: piece_length as u64,
            num_pieces: ((total_size + piece_length - 1) / piece_length) as usize,
            output_path: PathBuf::new(),
            piece_hashes: pieces,
        },
    ))
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
