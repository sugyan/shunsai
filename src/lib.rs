//! Fast shogi legal move generation on [`shogi_core`] types.

mod bitboard;
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub mod internals;
mod movegen;
mod position;
mod sliders;
mod tables;
mod zobrist;

pub use bitboard::Bitboard;
pub use movegen::{MoveSet, MoveSetIter};
pub use position::Position;
pub use shogi_core;
