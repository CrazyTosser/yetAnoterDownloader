use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum RequestError {
    #[error("info section not found")]
    InfoSectionNotFound,
    #[error("announce-list section not found")]
    AnnounceListSectionNotFound,
    #[error("peers list is empty")]
    PeersListEmpty,
    #[error("intervals section not found")]
    IntervalsSectionNotFound,
    #[error("all requests failed")]
    AllRequestsFailed,
    #[error("request error (0)")]
    ReqwestError(String),
    #[error("inner reqwest error (0)")]
    InnerReqwestError(String),
    #[error("Torrent validation: {0}")]
    BencodeParseError(#[from] bencode::BencodeParseError),
    #[error("cancelled request")]
    Cancelled,
}