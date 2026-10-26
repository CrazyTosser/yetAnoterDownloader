use crate::errors::RequestError;
use crate::{AnnouncementEvents, Announcer};
use rand::seq::SliceRandom;
use rand::rngs::StdRng;
use reqwest::get;
use std::error::Error;
use std::net::SocketAddr;
use std::time::{Duration};
use tokio::select;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use torrent::{TransferStats, StatsUpdate};
use tracing::{error, info, warn};

async fn try_announce_url(url: &str) -> Result<(Vec<SocketAddr>, Duration), RequestError> {
    match get(url).await {
        Ok(resp) => match resp.bytes().await {
            Ok(val) => match bencode::decode(&val) {
                Ok(val) => {
                    let peers = torrent::extract_peers(&val);
                    if peers.is_empty() {
                        return Err(RequestError::PeersListEmpty);
                    }
                    let inst = Duration::from_secs(if val.get(b"interval").is_some() {
                        val.get(b"interval")
                            .ok_or(RequestError::IntervalsSectionNotFound)?
                            .as_integer()
                            .ok_or(RequestError::IntervalsSectionNotFound)? as u64
                    } else { 1800 });

                    Ok((peers, inst))
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
        rx: mpsc::Receiver<StatsUpdate>,
        stats: TransferStats,
    ) -> Self {
        Self {
            trackers: trackers.to_vec(),
            interval,
            peers_tx: tx,
            stats_rx: rx,
            stats,
            complete_sent: false,
        }
    }

    async fn announce_tier(&self, urls: Vec<String>) -> Result<(Vec<SocketAddr>, Duration), RequestError> {
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
        let mut rng: StdRng = rand::make_rng();
        for tier in &self.trackers {
            let mut shuffle: Vec<_> = tier.iter().collect();
            shuffle.shuffle(&mut rng);
            let urls = shuffle
                .iter()
                .map(|it| {
                    let event_str = match event {
                        Some(event) => format!("&event={}", event.to_str()),
                        None => String::new(),
                    };
                    format!(
                        "{}&downloaded={}&uploaded={}&left={}{}",
                        it, self.stats.downloaded, self.stats.uploaded, self.stats.left, event_str
                    )
                })
                .collect();

            match self.announce_tier(urls).await {
                Ok(peers) => {
                    self.interval = peers.1;
                    if !peers.0.is_empty() {
                        self.peers_tx.send(peers.0).await?;
                        return Ok(());
                    }
                }
                Err(e) => {
                    warn!(error = ?e, "cannot announce tier");
                }
            }
        }

        Err(RequestError::AllRequestsFailed.into())
    }

    pub async fn updater_loop(&mut self, token: CancellationToken) -> Result<(), Box<dyn Error>> {
        self.update_peers(&Some(AnnouncementEvents::Started))
            .await?;
        let interval = self.interval;
        loop {
            select! {
                _ = token.cancelled() => {
                    self.update_peers(&Some(AnnouncementEvents::Stopped)).await?;
                    break Ok(());
                },
                _ = tokio::time::sleep(interval) =>{
                    let mut event = None;
                    if self.stats.left == 0 {
                        if self.complete_sent {
                            return Ok(());
                        }
                        event = Some(AnnouncementEvents::Completed);
                        self.complete_sent = true;
                    }
                    match self.update_peers(&event).await {
                        Ok(_) => info!("update peers successful"),
                        Err(e) => error!(error = ?e, "update peers error"),
                    }
                },
                stats = self.stats_rx.recv() => {
                    match stats {
                        Some(stats) => match stats {
                            StatsUpdate::Downloaded(up) => {
                                self.stats.downloaded += up;
                                self.stats.left -= up;
                            }
                            StatsUpdate::Uploaded(up) => {
                                self.stats.uploaded += up;
                            }
                        }
                        _ => {
                            error!(error = ?stats, "cannot get stats from announce channel");
                        }
                    }
                }
            }
        }
    }
}
