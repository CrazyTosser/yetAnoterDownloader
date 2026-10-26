use std::path::PathBuf;

#[derive(Clone)]
pub struct MangerConfig {
    pub peer_id: [u8; 20],
    pub info_hash: [u8; 20],
    pub num_pieces: usize,
    pub piece_length: u64,
    pub total_size: u64,
    pub output_path: PathBuf,
    pub piece_hashes: Vec<[u8; 20]>,
}