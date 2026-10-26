use bitvec::prelude::Msb0;
use bitvec::vec::BitVec;
use std::cmp::min;
use std::collections::HashSet;
use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use sha1::{Digest, Sha1};
use tracing::warn;

const BLOCK_SIZE: u32 = 16384; // 16 KiB
#[derive(Clone)]
pub struct DownloadState {
    num_pieces: usize,
    piece_length: u64,
    total_size: u64,
    received_bytes_per_piece: Vec<u64>,
    pub output_path: PathBuf,
    pub have_pieces: BitVec<u8, Msb0>,
    pub pending: HashSet<(u32, u32, u32)>,
    pub pieces_hash: Vec<[u8;20]>,
}

impl DownloadState {
    pub(crate) fn new(num_pieces: usize, piece_length: u64, total_size: u64, output_path: PathBuf, pieces_hash: Vec<[u8;20]>) -> Self {
        Self {
            num_pieces,
            piece_length,
            total_size,
            output_path,
            pieces_hash,
            received_bytes_per_piece: vec![0u64; num_pieces],
            have_pieces: BitVec::new(),
            pending: HashSet::new(),
        }
    }

    pub fn write_block_and_check(&mut self, index: u32, begin: u32, data: &[u8]) -> Result<bool, std::io::Error> {
        let global_offset = (index as u64 * self.piece_length) + begin as u64;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.output_path)?;
        use std::io::{Seek, Write};
        file.seek(std::io::SeekFrom::Start(global_offset))?;
        file.write_all(data)?;
        file.flush()?;

        self.received_bytes_per_piece[index as usize] += data.len() as u64;

        let expected_size = self.piece_size(index);
        if self.received_bytes_per_piece[index as usize] >= expected_size as u64 {
            let hash = &self.pieces_hash[index as usize];
            if self.verify_piece(index, hash)? {
                self.have_pieces.set(index as usize, true);
                Ok(true)
            } else {
                self.received_bytes_per_piece[index as usize] = 0;
                warn!("Piece {} hash mismatch! Re-downloading.", index);
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    pub fn read_block(&self, index: u32, begin: u32, length: u32) -> Result<Vec<u8>, std::io::Error> {
        let global_offset = (index as u64 * self.piece_length) + begin as u64;
        let mut buf = vec![0u8; length as usize];

        let mut file = std::fs::File::open(&self.output_path)?;
        use std::io::SeekFrom;
        file.seek(SeekFrom::Start(global_offset))?;
        file.read_exact(&mut buf)?;

        Ok(buf)
    }

    pub fn verify_piece(&self, index: u32, expected_hash: &[u8; 20]) -> Result<bool, std::io::Error> {
        let start = index as u64 * self.piece_length;
        let len = if index as usize == self.num_pieces - 1 && self.total_size % self.piece_length != 0 {
            self.total_size % self.piece_length
        } else {
            self.piece_length
        } as usize;

        let mut file = std::fs::File::open(&self.output_path)?;
        use std::io::SeekFrom;
        file.seek(SeekFrom::Start(start))?;

        // Читаем кусок в буфер (или читаем чанками для экономии памяти)
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)?;

        // Считаем хеш
        let result = Sha1::digest(&buf);

        Ok(result.as_slice() == expected_hash)
    }

    pub fn next_block_request(&self, bitfield: &BitVec<u8, Msb0>) -> Option<(u32, u32, u32)> {
        for i in 0..self.num_pieces as u32 {
            if self.have_pieces[i as usize] {
                continue;
            }
            if !bitfield[i as usize] {
                continue;
            }
            let piece_size = self.piece_size(i);
            let mut offset = 0u32;
            while offset < piece_size {
                let block_len = min(BLOCK_SIZE, piece_size - offset);
                let key = (i, offset, block_len);
                if !self.pending.contains(&key) {
                    return Some(key)
                }
                offset += BLOCK_SIZE;
            }
        }
        None
    }

    pub fn piece_size(&self, i: u32) -> u32 {
        let last_piece = (self.num_pieces as u32) - 1;
        if i == last_piece && self.total_size % self.piece_length > 0 {
            (self.total_size % self.piece_length) as u32
        } else {
            self.piece_length as u32
        }
    }
}
