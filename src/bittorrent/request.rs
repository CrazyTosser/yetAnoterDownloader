use crate::bencode;
use crate::bencode::BencodeValue;
use crate::bittorrent::{helper, peer};
use std::error::Error;
use std::time::Duration;
use thiserror::Error;
use tokio::select;
use tokio_util::sync::CancellationToken;

pub struct Requester {
    trackers: Vec<String>,
    peers: Vec<peer::Peer>,
    interval: Duration,
}

impl Requester {
    pub fn new(trackers: Vec<String>, interval: Duration) -> Self {
        Self { trackers, peers: vec![], interval }
    }

    pub async fn update_peers(&mut self) -> Result<(), Box<dyn Error>> {
        for url in &self.trackers {
            let response = reqwest::get(url).await?.bytes().await?;
            let value = bencode::decode(response.as_ref())?;
            self.peers.append(&mut peer::extract_peers(value));
        }

        self.peers.sort();
        self.peers.dedup();

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

pub async fn announcement(torrent: BencodeValue) -> Result<Requester, Box<dyn Error>> {
    let info = torrent
        .get(b"info")
        .ok_or(RequestError::InfoSectionNotFound)?;
    let info_hash = helper::calc_info_hash(info);
    let peer_id = helper::gen_peer_id();

    let mut prepared_urls = Vec::new();
    let announce_list = torrent
        .get(b"announce-list")
        .ok_or(RequestError::AnnounceListSectionNotFound)?
        .as_list()
        .ok_or(RequestError::AnnounceListSectionNotFound)?;
    for urls in announce_list {
        for url in urls
            .as_list()
            .ok_or(RequestError::AnnounceListSectionNotFound)?
        {
            if let Some(tracker_url) = url.as_str() {
                prepared_urls.push(format!(
                    "{}?info_hash={}&peer_id={}&port={}&uploaded=0&downloaded=0&left={}&event=started",
                    tracker_url,
                    helper::percent_encode_binary(&info_hash),
                    helper::percent_encode_binary(&peer_id),
                    6881,
                    info.get(b"length").ok_or(RequestError::InfoSectionNotFound)?.as_integer().ok_or(RequestError::InfoSectionNotFound)?
                ));
            }
        }
    }

    let interval = info
        .get(b"intervals")
        .ok_or(RequestError::IntervalsSectionNotFound)?
        .as_integer()
        .ok_or(RequestError::IntervalsSectionNotFound)?;
    let mut service = Requester::new(prepared_urls.clone(), Duration::from_secs(interval as u64));
    service.update_peers().await?;
    Ok(service)
}
