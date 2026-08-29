//! Fast shogi legal move generation on [`shogi_core`] types.
//!
//! Everything [`Position`] generates is **fully legal**, pawn-drop mate
//! (打ち歩詰め) included, so a caller never filters what it is handed.
//!
//! [`Position::legal_moves`] is the convenient way to take them:
//!
//! ```
//! use shunsai::Position;
//!
//! let position = Position::startpos();
//! assert_eq!(position.legal_moves().len(), 30);
//! ```
//!
//! [`Position::generate_moves`] is the fast way. It yields one [`MoveSet`]
//! per origin, destinations still packed as a [`Bitboard`], so a caller that
//! only needs to count them never builds a [`Move`](shogi_core::Move) at all
//! — and can stop early by returning [`ControlFlow::Break`]:
//!
//! ```
//! use core::ops::ControlFlow;
//! use shunsai::Position;
//!
//! let position = Position::startpos();
//! let mut moves = 0;
//! let _ = position.generate_moves(|set| {
//!     moves += set.len();
//!     ControlFlow::Continue(())
//! });
//! assert_eq!(moves, 30);
//! ```
//!
//! Moves are played and taken back with [`Position::do_move`] and
//! [`Position::undo_move`], which maintain the Zobrist [`Position::key`]
//! incrementally.
//!
//! [`ControlFlow::Break`]: core::ops::ControlFlow::Break

#![deny(missing_docs)]

mod bitboard;
#[cfg(feature = "_bench-internals")]
#[doc(hidden)]
pub mod internals;
mod movegen;
mod position;
mod sliders;
mod tables;
mod zobrist;

pub use bitboard::Bitboard;
pub use movegen::{MAX_LEGAL_MOVES, MoveSet, MoveSetIter};
pub use position::{Position, Undo};
pub use shogi_core;
