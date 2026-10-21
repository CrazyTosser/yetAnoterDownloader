use bencode::BencodeValue;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

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