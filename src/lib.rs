//! Fast shogi legal move generation on [`shogi_core`] types.

mod bitboard;
mod tables;
mod zobrist;

pub use bitboard::Bitboard;
pub use shogi_core;
