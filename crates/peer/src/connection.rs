use crate::message::PeerMessage;
use crate::state::DownloadState;
use bitvec::prelude::*;
use std::collections::HashSet;
use std::io::ErrorKind::InvalidData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use torrent::StatsUpdate;
use tracing::{error, info, warn};

enum PeerState {
    Idle,
    Connected,
    Active {
        bitfield: BitVec<u8, Msb0>,
        am_interested: bool,
        peer_choking: bool, // ключевой флаг
        peer_interested: bool,
        am_choking: bool,
    },
}

pub struct PeerConnection {
    peer_state: PeerState,
    stream: TcpStream,
    last_seen: Instant,
    peer_id: [u8; 20],
    info_hash: [u8; 20],
    num_pieces: usize,
    download_state: Arc<Mutex<DownloadState>>,
    stats_tx: Sender<StatsUpdate>,
    local_pending: HashSet<(u32, u32, u32)>,
}

impl PeerConnection {
    pub async fn new(
        addr: &SocketAddr,
        peer_id: [u8; 20],
        info_hash: [u8; 20],
        num_pieces: usize,
        download_state: Arc<Mutex<DownloadState>>,
        stats_tx: Sender<StatsUpdate>,
    ) -> Result<PeerConnection, std::io::Error> {
        let stream =
            tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr)).await??;
        Ok(Self {
            peer_state: PeerState::Idle,
            peer_id,
            info_hash,
            stream,
            num_pieces,
            download_state,
            stats_tx,
            last_seen: Instant::now(),
            local_pending: HashSet::new(),
        })
    }

    fn can_request(&self) -> bool {
        match &self.peer_state {
            PeerState::Active {
                peer_choking,
                am_interested,
                ..
            } => !peer_choking && *am_interested,
            _ => false,
        }
    }

    async fn try_request_next_block(
        &mut self,
        download_state: &Arc<Mutex<DownloadState>>,
    ) -> Result<(), std::io::Error> {
        if !self.can_request() {
            return Ok(());
        }

        let next = {
            let state = download_state.lock().expect("mutex poisoned");
            let bitfield = match &self.peer_state {
                PeerState::Active { bitfield, .. } => bitfield,
                _ => return Ok(()),
            };
            state.next_block_request(bitfield)
        };

        if let Some((idx, beg, len)) = next {
            {
                let mut state = download_state.lock().expect("mutex poisoned");
                state.pending.insert((idx, beg, len));
            }
            self.local_pending.insert((idx, beg, len));
            self.send_request(idx, beg, len).await?;
        }
        Ok(())
    }

    pub async fn read_message(&mut self) -> Result<PeerMessage, std::io::Error> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let length = u32::from_be_bytes(len_buf);

        // Keep-alive: длина 0
        if length == 0 {
            return Ok(PeerMessage::KeepAlive);
        }

        let mut payload = vec![0u8; length as usize];
        self.stream.read_exact(&mut payload).await?;
        match payload[0] {
            0 => Ok(PeerMessage::Choke),
            1 => Ok(PeerMessage::Unchoke),
            2 => Ok(PeerMessage::Interested),
            3 => Ok(PeerMessage::NotInterested),
            4 => {
                if payload.len() != 5 {
                    return Err(std::io::Error::new(InvalidData, "Invalid have length"));
                }
                let index = u32::from_be_bytes(payload[1..5].try_into().unwrap());
                Ok(PeerMessage::Have(index))
            }
            5 => {
                let bits = &payload[1..];
                let mut bifields = BitVec::<u8, Msb0>::with_capacity(self.num_pieces);
                for byte in bits {
                    bifields.extend_from_bitslice(&BitVec::<u8, Msb0>::from_element(*byte));
                }

                bifields.truncate(self.num_pieces);
                Ok(PeerMessage::Bitfield(bifields))
            }
            7 => {
                if payload.len() < 9 {
                    return Err(std::io::Error::new(InvalidData, "Piece too short"));
                }
                let index = u32::from_be_bytes([payload[1], payload[2], payload[3], payload[4]]);
                let begin = u32::from_be_bytes([payload[5], payload[6], payload[7], payload[8]]);
                let data = payload[9..].to_vec();
                Ok(PeerMessage::Piece(index, begin, data))
            }
            _ => Err(std::io::Error::new(InvalidData, "Unknown message type")),
        }
    }

    fn has_needed_pieces(
        peer_bitfield: &BitVec<u8, Msb0>,
        download_state: &Arc<Mutex<DownloadState>>,
    ) -> bool {
        let state = download_state.lock().unwrap();

        // Нужные куски = (есть у пира) AND (нет у нас)
        // Инвертируем have_pieces, чтобы получить "нужные"
        peer_bitfield
            .iter()
            .zip(state.have_pieces.iter())
            .any(|(p, h)| *p && !h)
    }

    pub async fn run(&mut self) -> Result<(), std::io::Error> {
        self.handshake().await?;
        let msg = self.read_message().await?;
        let available_pieces = match msg {
            PeerMessage::Bitfield(bifield) => bifield,
            _ => {
                error!(gotten = ?msg, "Unexpected message type");
                return Err(std::io::Error::new(InvalidData, "expected Bitfield"));
            }
        };

        info!(avalible_pieces = ?available_pieces.count_ones(), "Peer send bitfield");
        self.peer_state = PeerState::Active {
            bitfield: available_pieces,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            am_choking: true,
        };

        loop {
            match self.read_message().await? {
                PeerMessage::Choke => {
                    warn!("Peer choked us");
                    if let PeerState::Active { peer_choking, .. } = &mut self.peer_state {
                        *peer_choking = true;
                        {
                            let mut state = self.download_state.lock().expect("mutex poisoned");
                            for req in self.local_pending.iter() {
                                state.pending.remove(&req);
                            }
                        }
                        self.local_pending.clear();
                    }
                }
                PeerMessage::Unchoke => {
                    info!("Peer unchoked us");
                    if let PeerState::Active {
                        peer_choking,
                        am_interested,
                        ..
                    } = &mut self.peer_state
                    {
                        *peer_choking = false;
                        if *am_interested {
                            info!("Unchoked! Starting pipeline...");
                            let download_state = Arc::clone(&self.download_state);
                            self.start_requesting(&download_state).await?;
                        }
                    }
                    for _ in 0..5 {
                        let mut block = None;
                        {
                            let state = self.download_state.lock().expect("mutex poisoned");
                            if let PeerState::Active { bitfield, .. } = &self.peer_state {
                                block = state.next_block_request(bitfield);
                            }
                        }
                        if let Some((idx, bg, len)) = block {
                            {
                                let mut state = self.download_state.lock().expect("mutex poisoned");
                                state.pending.insert((idx, bg, len));
                            }
                            self.send_request(idx, bg, len).await?;
                        }
                    }
                }
                PeerMessage::Interested => {
                    info!("Peer interested us");
                    if let PeerState::Active {
                        peer_interested, ..
                    } = &mut self.peer_state
                    {
                        *peer_interested = true;
                    }
                }
                PeerMessage::NotInterested => {
                    if let PeerState::Active {
                        peer_interested, ..
                    } = &mut self.peer_state
                    {
                        *peer_interested = false;
                    }
                }
                PeerMessage::Have(n) => {
                    let mut need_send = false;
                    let mut can_request = false;
                    if let PeerState::Active {
                        bitfield,
                        am_interested,
                        peer_choking,
                        ..
                    } = &mut self.peer_state
                    {
                        let idx = n as usize;
                        if idx < bitfield.len() {
                            bitfield.set(idx, true);
                            let need_interested = {
                                let state = self.download_state.lock().unwrap();
                                !state.have_pieces[idx] && !*am_interested
                            };

                            if need_interested {
                                need_send = true;
                                *am_interested = true;
                            }
                            can_request = !*peer_choking && *am_interested;
                        }
                    }
                    if need_send {
                        self.send_interested().await?;
                    }
                    if can_request {
                        self.try_request_next_block(&self.download_state.clone())
                            .await?;
                    }
                }
                PeerMessage::Bitfield(bf) => {
                    self.peer_state = PeerState::Active {
                        bitfield: bf.clone(),
                        am_interested: false,
                        peer_choking: true,
                        peer_interested: false,
                        am_choking: true,
                    };
                    if PeerConnection::has_needed_pieces(&bf, &self.download_state.clone()) {
                        self.send_interested().await?;
                        if let PeerState::Active { am_interested, .. } = &mut self.peer_state {
                            *am_interested = true;
                        }
                    }
                }
                PeerMessage::Request(index, begin, length) => {
                    if let PeerState::Active { am_choking, .. } = &self.peer_state {
                        if *am_choking {
                            return Ok(());
                        }

                        let mut data = vec![];
                        {
                            let state = self.download_state.lock().unwrap();
                            if state.have_pieces[index as usize] {
                                // Читаем данные с диска
                                data = state.read_block(index, begin, length)?;
                            }

                        }
                        if !data.is_empty() {
                            self.send_piece(index, begin, &data).await?;
                        }
                    }
                }
                PeerMessage::Piece(index, begin, data) => {
                    let key = (index, begin, data.len() as u32);
                    self.local_pending.remove(&key);
                    {
                        let mut state = self.download_state.lock().expect("mutex poisoned");
                        state.pending.remove(&key);
                    }
                    {
                        let ds = self.download_state.clone();
                        let idx = index;
                        let beg = begin;
                        let d = data.clone();

                        let verified = tokio::task::spawn_blocking(move || {
                            let mut state = ds.lock().unwrap();
                            state.write_block_and_check(idx, beg, &d)
                        }).await.unwrap()?;
                        if verified {
                            info!("✅ Piece {} complete and verified!", index);
                            // TODO: отправить Have другим пирам
                            let piece_size = { self.download_state.lock().unwrap().piece_size(index) };
                            let _ = self.stats_tx.send(StatsUpdate::Downloaded(piece_size as u64)).await;
                        }
                    }
                    let mut next_block = None;
                    {
                        let state = self.download_state.lock().expect("mutex poisoned");
                        if let PeerState::Active { bitfield, .. } = &self.peer_state {
                            next_block = state.next_block_request(&bitfield);
                        }
                    }
                    if let Some((idx, bg, len)) = next_block {
                        {
                            let mut state = self.download_state.lock().expect("mutex poisoned");
                            state.pending.insert((idx, bg, len));
                        }
                        self.send_request(index, bg, len).await?;
                    }
                }
                PeerMessage::Cancel(index, begin, _) => {
                    info!("Peer cancelled request for piece {} at {}", index, begin);
                }
                PeerMessage::KeepAlive => {
                    info!("Got keep-alive message");
                }
            }
            self.last_seen = Instant::now();
        }
    }

    async fn start_requesting(
        &mut self,
        download_state: &Arc<Mutex<DownloadState>>,
    ) -> Result<(), std::io::Error> {
        const INITIAL_PIPELINE_SIZE: usize = 5;

        for _ in 0..INITIAL_PIPELINE_SIZE {
            if !self.can_request() {
                break;
            }
            self.try_request_next_block(download_state).await?;
        }

        Ok(())
    }

    pub async fn handshake(&mut self) -> Result<(), std::io::Error> {
        self.send_handshake().await?;
        let remote_id = self.read_handshake().await?;
        info!("Handshake successful with peer {}", hex::encode(remote_id));
        self.peer_state = PeerState::Connected; // Переходим в следующее состояние
        Ok(())
    }

    pub async fn send_handshake(&mut self) -> std::io::Result<()> {
        let mut buf: Vec<u8> = Vec::with_capacity(68);
        buf.push(19u8);
        buf.extend_from_slice(b"BitTorrent protocol");
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(&self.info_hash[..]);
        buf.extend_from_slice(&self.peer_id[..]);

        let _ = tokio::time::timeout(Duration::from_secs(5), self.stream.write_all(&buf)).await?;
        Ok(())
    }

    pub async fn send_request(
        &mut self,
        index: u32,
        begin: u32,
        length: u32,
    ) -> std::io::Result<()> {
        let mut buf = [0u8; 17];
        buf[0..4].copy_from_slice(&13u32.to_be_bytes());
        buf[4] = 6;
        buf[5..9].copy_from_slice(&index.to_be_bytes());
        buf[9..13].copy_from_slice(&begin.to_be_bytes());
        buf[13..17].copy_from_slice(&length.to_be_bytes());
        self.stream.write_all(&buf).await
    }

    pub async fn read_handshake(&mut self) -> Result<[u8; 20], std::io::Error> {
        let mut buf = [0u8; 68];
        let _ =
            tokio::time::timeout(Duration::from_secs(5), self.stream.read_exact(&mut buf)).await?;

        // Проверка длины протокола
        if buf[0] != 19 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid pstrlen",
            ));
        }

        // Проверка строки протокола
        if &buf[1..20] != b"BitTorrent protocol" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid protocol string",
            ));
        }

        // Извлекаем peer_id пира (может пригодиться для логирования)
        let remote_peer_id: [u8; 20] = buf[48..68].try_into().unwrap();

        // Самая важная проверка: совпадает ли info_hash?
        if &buf[28..48] != self.info_hash {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Info hash mismatch",
            ));
        }

        Ok(remote_peer_id)
    }

    async fn send_interested(&mut self) -> std::io::Result<()> {
        // [0, 0, 0, 1] = длина 1 (big-endian u32)
        // [2] = ID Interested
        self.stream.write_all(&[0, 0, 0, 1, 2]).await
    }

    async fn send_not_interested(&mut self) -> std::io::Result<()> {
        self.stream.write_all(&[0, 0, 0, 1, 3]).await
    }

    async fn send_piece(&mut self, index: u32, begin: u32, data: &[u8]) -> std::io::Result<()> {
        let len = 9 + data.len() as u32;
        let mut buf = Vec::with_capacity(len as usize + 4);

        buf.extend_from_slice(&len.to_be_bytes());
        buf.push(7); // ID Piece
        buf.extend_from_slice(&index.to_be_bytes());
        buf.extend_from_slice(&begin.to_be_bytes());
        buf.extend_from_slice(data);

        self.stream.write_all(&buf).await
    }
}
