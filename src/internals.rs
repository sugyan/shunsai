//! Crate internals re-exposed for `benches/` (`bench-internals` feature).
//!
//! Not public API: hidden from docs, no stability guarantees. Wrapper
//! functions are used because `pub use` of a `pub(crate)` item is not
//! allowed (E0365); the wrapped signatures are exactly the M4 optimization
//! swap boundary (DESIGN.md decision log) plus the legality-test core.

use shogi_core::{Color, Piece, Square};

use crate::bitboard::Bitboard;
use crate::position::Position;

#[inline(always)]
pub fn lance_attacks(color: Color, square: Square, occupied: Bitboard) -> Bitboard {
    crate::tables::lance_attacks(color, square, occupied)
}

#[inline(always)]
pub fn bishop_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    crate::tables::bishop_attacks(square, occupied)
}

#[inline(always)]
pub fn rook_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    crate::tables::rook_attacks(square, occupied)
}

#[inline(always)]
pub fn attacks_of(piece: Piece, square: Square, occupied: Bitboard) -> Bitboard {
    crate::tables::attacks_of(piece, square, occupied)
}

/// The checker/attacker computation at the heart of legality testing.
#[inline(always)]
pub fn attackers_to(
    position: &Position,
    square: Square,
    occupied: Bitboard,
    attackers_color: Color,
    mask: Bitboard,
) -> Bitboard {
    crate::movegen::attackers_to(position, square, occupied, attackers_color, mask)
}
