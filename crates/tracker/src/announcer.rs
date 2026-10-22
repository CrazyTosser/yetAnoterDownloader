use crate::errors::RequestError;
use crate::{AnnouncementEvents, Announcer};
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;
use reqwest::get;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use torrent::TransferStats;

async fn try_announce_url(url: &str) -> Result<Vec<SocketAddr>, RequestError> {
    match get(url).await {
        Ok(resp) => match resp.bytes().await {
            Ok(val) => match bencode::decode(&val) {
                Ok(val) => {
                    let peers = torrent::extract_peers(val);
                    if peers.is_empty() {
                        return Err(RequestError::PeersListEmpty);
                    }
                    Ok(peers)
                }
                Err(e) => Err(RequestError::BencodeParseError(e)),
            },
            Err(e) => Err(RequestError::InnerReqwestError(e.to_string())),
        },
        Err(e) => Err(RequestError::ReqwestError(e.to_string())),
    }
}

impl Announcer {
    pub fn new(
        trackers: &Vec<Vec<String>>,
        interval: Duration,
        tx: mpsc::Sender<Vec<SocketAddr>>,
        rx: mpsc::Receiver<TransferStats>,
        stats: TransferStats,
    ) -> Self {
        Self {
            trackers: trackers.to_vec(),
            interval,
            peers_tx: tx,
            stats_rx: rx,
            stats,
        }
    }

    async fn announce_tier(&self, urls: Vec<String>) -> Result<Vec<SocketAddr>, RequestError> {
        let cnl = CancellationToken::new();
        let mut handlers = vec![];
        for url in urls {
            let token = cnl.clone();
            handlers.push(tokio::spawn(async move {
                select! {
                    res = try_announce_url(&url) => res,
                    _ = token.cancelled() => Err(RequestError::Cancelled),
                }
            }));
        }
        for handle in handlers {
            match handle.await {
                Ok(Ok(peers)) => {
                    cnl.cancel();
                    return Ok(peers);
                }
                _ => continue,
            }
        }

        Err(RequestError::AllRequestsFailed)
    }

    async fn update_peers(
        &mut self,
        event: &Option<AnnouncementEvents>,
    ) -> Result<(), Box<dyn Error>> {
        for tier in &self.trackers {
            let mut shuffle = tier.clone();
            shuffle.shuffle(&mut ThreadRng::default());
            let mut urls = vec![];

            for url in &shuffle {
                urls.push(format!(
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
                ));
            }
            match self.announce_tier(urls).await {
                Ok(peers) => match self.peers_tx.send(peers).await {
                    Err(e) => return Err(Box::new(e)),
                    _ => return Ok(()),
                },
                Err(_) => {},
                //Err(e) => return Err(e.into()),
            }
        }

        Err(Box::new(RequestError::AllRequestsFailed))
    }

    pub async fn updater_loop(&mut self, token: CancellationToken) -> Result<(), Box<dyn Error>> {
        self.update_peers(&Some(AnnouncementEvents::Started)).await?;
        let interval = self.interval;
        loop {
            select! {
                _ = token.cancelled() => {
                    self.update_peers(&Some(AnnouncementEvents::Stopped)).await?;
                    break Ok(());
                },
                _ = tokio::time::sleep(interval) =>
                    return if let Err(e) = self.update_peers(&None).await {
                        token.cancel();
                        Err(e)
                    } else {
                    Ok(())
                },
            }
        }
    }
}
