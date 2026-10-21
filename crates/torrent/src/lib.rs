use bencode::{encode, BencodeValue};
use thiserror::Error;
use sha1::{Digest, Sha1};

#[derive(Error, Debug, PartialEq)]
pub enum TorrentFileError {
    #[error("file root is not dict")]
    WrongRoot,
    #[error("no info field in file")]
    NoInfo,
    #[error("announce-list has wrong structure")]
    AnnounceListWrong,
    #[error("no required fields")]
    NoRequiredFields,
    #[error("no length and files fields")]
    NoLengthAndFilesFields,
    #[error("file contains the length and files fields.")]
    FileContainsLengthAndFilesFields,
    #[error("in files path field is not string")]
    FilesPathNotString,
    #[error("total_size/piece_length not equal to pieces/20")]
    PieceLengthNotEqual,
    #[error("pieces size is incompletely divisible by 20")]
    PieceLengthIncorrect,
    #[error("piece_length must be gt 0")]
    PieceLengthLwZero,
    #[error("name is not string")]
    NameNotString,
    #[error("{name} wrong type")]
    WrongType { name: String },
}

impl From<TorrentFileError> for std::io::Error {
    fn from(err: TorrentFileError) -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
    }
}

pub fn is_valid_file(torrent_file: &BencodeValue) -> Result<(), TorrentFileError> {
    torrent_file.as_dict().ok_or(TorrentFileError::WrongRoot)?;
    validate_announce(&torrent_file)?;
    let info = torrent_file.get(b"info").ok_or(TorrentFileError::NoInfo)?;
    validate_info(info)
}

fn compute_total_size(info: &BencodeValue) -> Result<i128, TorrentFileError> {
    let files = info
        .get(b"files")
        .and_then(|it| it.as_list())
        .ok_or(TorrentFileError::NoRequiredFields)?;

    let mut acc = 0;
    for file in files {
        let path = file
            .get(b"path")
            .and_then(|it| it.as_list())
            .ok_or_else(|| TorrentFileError::WrongType {
                name: "path".into(),
            })?;
        for p in path {
            match p.as_str() {
                Some(s) if !s.is_empty() => {}
                _ => {
                    return Err(TorrentFileError::WrongType {
                        name: "path".into(),
                    });
                }
            }
        }
        let size = file
            .get(b"length")
            .and_then(|it| it.as_integer())
            .ok_or_else(|| TorrentFileError::WrongType {
                name: "files.length".into(),
            })?;
        acc += size;
    }

    Ok(acc)
}

fn validate_info(info: &BencodeValue) -> Result<(), TorrentFileError> {
    info.get(b"name")
        .and_then(|it| it.as_str())
        .ok_or(TorrentFileError::NameNotString)?;
    let has_length = info.get(b"length").is_some();
    match (has_length, info.get(b"files").is_some()) {
        (true, true) => return Err(TorrentFileError::FileContainsLengthAndFilesFields),
        (false, false) => return Err(TorrentFileError::NoLengthAndFilesFields),
        _ => {}
    };

    let total_size = if has_length {
        info.get(b"length")
            .and_then(|it| it.as_integer())
            .ok_or_else(|| TorrentFileError::WrongType {
                name: "length".to_string(),
            })?
    } else {
        compute_total_size(info)?
    };

    let piece_length = info
        .get(b"piece length")
        .and_then(|it| it.as_integer())
        .ok_or(TorrentFileError::NoRequiredFields)?;
    if piece_length <= 0 {
        return Err(TorrentFileError::PieceLengthNotEqual);
    }
    let pieces_bytes = info
        .get(b"pieces")
        .and_then(|it| it.as_bytes())
        .ok_or(TorrentFileError::NoRequiredFields)?;
    if pieces_bytes.len() % 20 != 0 {
        return Err(TorrentFileError::PieceLengthIncorrect);
    }
    let num_pieces = pieces_bytes.len() as i128 / 20;
    if (total_size + piece_length - 1) / piece_length != num_pieces {
        return Err(TorrentFileError::PieceLengthIncorrect);
    }
    Ok(())
}

fn validate_announce(torrent_file: &BencodeValue) -> Result<(), TorrentFileError> {
    if let Some(lst) = torrent_file.get(b"announce-list").and_then(|x| x.as_list()) {
        for tier in lst {
            let urls = tier.as_list().ok_or(TorrentFileError::AnnounceListWrong)?;
            if !urls.iter().all(|it| it.as_str().is_some()) {
                return Err(TorrentFileError::AnnounceListWrong);
            }
        }
        Ok(())
    } else {
        if torrent_file
            .get(b"announce")
            .and_then(|it| it.as_str())
            .is_none()
        {
            return Err(TorrentFileError::AnnounceListWrong);
        }
        Ok(())
    }
}

pub fn gen_peer_id() -> [u8; 20] {
    // let mut id = [0u8; 20];
    // id[0] = b'-';
    // id[1..3].copy_from_slice(b"CT");      // твой код клиента
    // id[3..7].copy_from_slice(b"0001");    // версия
    // id[7] = b'-';
    //
    // for byte in &mut id[8..20] {
    //     // Генерируем printable ASCII для совместимости
    //     *byte = rand::random_range(b'0'..=b'z');
    // }
    // id

    let id = b"-CT0001-unZrN_@UUu=A";
    *id
}

pub fn calc_info_hash(info: &BencodeValue) -> [u8; 20] {
    let mut buf: Vec<u8> = Vec::new();
    encode(info, &mut buf);
    let mut hasher = Sha1::new();
    hasher.update(&buf);
    let info_hash: [u8; 20] = hasher.finalize().into();

    info_hash
}

pub fn percent_encode_binary(data: &[u8]) -> String {
    data.iter()
        .map(|&b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}