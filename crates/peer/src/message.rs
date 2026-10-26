use bitvec::order::Msb0;
use bitvec::prelude::BitVec;

#[derive(Debug)]
pub enum PeerMessage {
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),           
    Bitfield(BitVec<u8, Msb0>),
    Request(u32, u32, u32),
    Piece(u32, u32, Vec<u8>),
    Cancel(u32, u32, u32),
    KeepAlive,
}