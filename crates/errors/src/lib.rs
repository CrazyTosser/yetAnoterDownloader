use thiserror::Error;
use bencode::BencodeParseError;
use tracker::RequestError;
use torrent::TorrentFileError;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] BencodeParseError),

    #[error("Torrent validation: {0}")]
    Validation(#[from] TorrentFileError),

    #[error("Tracker error: {0}")]
    Tracker(#[from] RequestError),
}