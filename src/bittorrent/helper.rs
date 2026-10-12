use crate::bencode::{BencodeValue, encoder};
use sha1::{Digest, Sha1};

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
    encoder::encode(info, &mut buf);
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