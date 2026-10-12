use crate::bencode::BencodeValue;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Ord, PartialEq, Eq, PartialOrd)]
pub enum Peer {
    IPv4((String, u16)),
    IPv6((String, u16)),
}

impl std::fmt::Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Peer::IPv4((ip, port)) => {
                write!(f, "{}:{}", ip, port)
            }
            Peer::IPv6((ip, port)) => {
                write!(f, "[{}]:{}", ip, port)
            }
        }
    }
}

pub fn extract_peers(responce: BencodeValue) -> Vec<Peer> {
    let mut res = Vec::new();
    if let Some(peers_value) = responce.get(b"peers") {
        match peers_value {
            BencodeValue::Bytes(raw) => {
                raw.chunks(6).for_each(|chunck| {
                    if chunck.len() != 6 {
                        return;
                    }
                    let ip = Ipv4Addr::new(chunck[0], chunck[1], chunck[2], chunck[3]).to_string();
                    let port = u16::from_be_bytes([chunck[4], chunck[5]]);
                    res.push(Peer::IPv4((ip, port)));
                });
            }
            BencodeValue::List(peers) => {
                peers.iter().for_each(|v| {
                    let ip = v
                        .get(b"ip")
                        .and_then(|i| i.as_str())
                        .unwrap()
                        .to_string()
                        .parse::<IpAddr>();
                    let port = v.get(b"port").and_then(|i| i.as_integer()).unwrap() as u16;
                    if let Ok(ip) = ip {
                        if ip.is_ipv4() {
                            res.push(Peer::IPv4((ip.to_string(), port)));
                        } else if ip.is_ipv6() {
                            res.push(Peer::IPv6((ip.to_string(), port)));
                        }
                    }
                });
            }
            _ => {}
        }
    }
    if let Some(peers6) = responce.get(b"peers6") {
        if let Some(peers) = peers6.as_bytes() {
            peers.chunks(18).for_each(|chunck| {
                if chunck.len() != 18 {
                    return;
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
                res.push(Peer::IPv6((ip.to_string(), port)));
            });
        }
    }
    res
}