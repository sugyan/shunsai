//! Fast shogi legal move generation on [`shogi_core`] types.

mod bitboard;
mod movegen;
mod position;
mod tables;
mod zobrist;

pub use bitboard::Bitboard;
pub use position::Position;
pub use shogi_core;
