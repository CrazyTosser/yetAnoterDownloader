pub mod errors;

pub use crate::errors::TorrentFileError;
use bencode::{BencodeValue, encode};
use sha1::{Digest, Sha1};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};


#[derive(Clone, Copy)]
pub struct TransferStats {
    pub downloaded: u64,
    pub uploaded: u64,
    pub left: u64,
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

pub fn extract_peers(responce: BencodeValue) -> Vec<SocketAddr> {
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