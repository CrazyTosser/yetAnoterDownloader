pub mod peer_manager;

use crate::peer_manager::{PeerManager, TransferStats};
use bencode::BencodeValue;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use thiserror::Error;
use tokio::select;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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

impl Announcer {
    pub fn new(
        trackers: &Vec<Vec<String>>,
        interval: Duration,
        tx: mpsc::Sender<Vec<SocketAddr>>,
        rx: mpsc::Receiver<TransferStats>,
        stats: TransferStats,
    ) -> (Self) {
        Self {
            trackers: trackers.to_vec(),
            interval,
            peers_tx: tx,
            stats_rx: rx,
            stats,
        }
    }

    pub async fn update_peers(
        &mut self,
        event: &Option<AnnouncementEvents>,
    ) -> Result<(), Box<dyn Error>> {
        for tier in &self.trackers {
            let mut shuffle = tier.clone();
            shuffle.shuffle(&mut ThreadRng::default());

            for url in &shuffle {
                let url = format!(
                    "{}&downloaded={}&uploaded={}&left={}{}",
                    url,
                    self.stats.downloaded,
                    self.stats.uploaded,
                    self.stats.left,
                    match event {
                        None => String::new(),
                        Some(event) => format!(
                            "&event={}",
                            match event {
                                AnnouncementEvents::Started => "started",
                                AnnouncementEvents::Completed => "completed",
                                AnnouncementEvents::Stopped => "stopped",
                            }
                        ),
                    }
                );
            }
        }

        for tier in &self.trackers {
            for url in &tier {
                let response = reqwest::get(url).await?.bytes().await?;
                let value = bencode::decode(response.as_ref())?;
                peers.insert(&mut peer::extract_peers(value));
            }
        }

        let _ = self.peers_tx.send(peers.iter().collect());

        Ok(())
    }

    pub async fn updater_loop(&mut self, token: CancellationToken) -> Result<(), Box<dyn Error>> {
        let interval = self.interval;
        loop {
            select! {
                _ = token.cancelled() => break Ok(()),
                _ = tokio::time::sleep(interval) =>
                    return if let Err(e) = self.update_peers().await {
                        token.cancel();
                        Err(e)
                    } else {
                    Ok(())
                },
            }
        }
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum RequestError {
    #[error("info section not found")]
    InfoSectionNotFound,
    #[error("announce-list section not found")]
    AnnounceListSectionNotFound,
    #[error("intervals section not found")]
    IntervalsSectionNotFound,
}

pub async fn announcement(
    torrent: BencodeValue,
) -> Result<(Announcer, PeerManager), Box<dyn Error>> {
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
    let mut service = Announcer::new(
        &prepared_urls,
        Duration::from_secs(interval as u64),
        peers_tx,
        stats_rx,
        stats,
    );
    let manager = PeerManager::new(peers_rx, stats_tx);
    service.update_peers(&None).await?;
    Ok((service, manager))
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

fn extract_peers(responce: BencodeValue) -> Vec<SocketAddr> {
    let mut res = Vec::new();
    if let Some(peers_value) = responce.get(b"peers") {
        match peers_value {
            BencodeValue::Bytes(raw) => {
                for chunck in raw.chunks(6) {
                    if chunck.len() != 6 {
                        continue;
                    }
                    let ip = Ipv4Addr::new(chunck[0], chunck[1], chunck[2], chunck[3]);
                    let port = u16::from_be_bytes([chunck[4], chunck[5]]);
                    res.push(SocketAddr::new(IpAddr::from(ip), port));
                };
            }
            BencodeValue::List(peers) => {
                for v in peers {
                    let ip = match v.get(b"ip").and_then(|it| it.as_str()).and_then(|s| s.parse::<IpAddr>().ok()) {
                        Some(ip) => ip,
                        None => continue,
                    };
                    let port = match v.get(b"port").and_then(|it| it.as_integer()) {
                        Some(p) if p> 0 && p <=u16::MAX as i128 => p as u16,
                        _ => continue,
                    };
                    res.push(SocketAddr::new(IpAddr::from(ip), port));
                }
            }
            _ => {}
        }
    }
    if let Some(peers6) = responce.get(b"peers6") {
        if let Some(peers) = peers6.as_bytes() {
            for chunck in peers.chunks(18) {
                if chunck.len() != 18 {
                    continue;
                }
                let ip = Ipv6Addr::new(
                    u16::from_be_bytes([chunck[0], chunck[1]]),
                    u16::from_be_bytes([chunck[2], chunck[3]]),
                    u16::from_be_bytes([chunck[4], chunck[5]]),
                    u16::from_be_bytes([chunck[6], chunck[7]]),
                    u16::from_be_bytes([chunck[8], chunck[9]]),
                    u16::from_be_bytes([chunck[10], chunck[11]]),
                    u16::from_be_bytes([chunck[12], chunck[13]]),
                    u16::from_be_bytes([chunck[14], chunck[15]]),
                );
                let port = u16::from_be_bytes([chunck[16], chunck[17]]);
                res.push(SocketAddr::new(IpAddr::from(ip), port));
            };
        }
    }
    res
}