//! The obviously-correct slider backend: walk each ray one square at a time.
//!
//! Slow, but it is the oracle every other backend is tested against, so it
//! is kept compiled (and selectable via the `slider-naive` feature) forever.

// A backend that the feature flags did not select is still compiled, so
// that the oracle tests and the A/B benchmarks can reach it.
#![allow(dead_code)]

use shogi_core::Square;

use crate::bitboard::Bitboard;

/// `(file_delta, rank_delta)` for the four rook rays.
const ORTHOGONAL_STEPS: [(i8, i8); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
/// `(file_delta, rank_delta)` for the four bishop rays.
const DIAGONAL_STEPS: [(i8, i8); 4] = [(-1, -1), (1, -1), (-1, 1), (1, 1)];

/// Walks each ray one square at a time, stopping at (and including) the
/// first occupied square.
fn ray_attacks(square: Square, occupied: Bitboard, steps: &[(i8, i8)]) -> Bitboard {
    let mut attacks = Bitboard::EMPTY;
    for &(file_delta, rank_delta) in steps {
        let mut current = square;
        while let Some(next) = current.shift(file_delta, rank_delta) {
            attacks |= Bitboard::single(next);
            if occupied.contains(next) {
                break;
            }
            current = next;
        }
    }
    attacks
}

pub(crate) fn bishop_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    ray_attacks(square, occupied, &DIAGONAL_STEPS)
}

pub(crate) fn rook_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    ray_attacks(square, occupied, &ORTHOGONAL_STEPS)
}

/// Lance attacks, walked the same way. Only used to check the shared table
/// in [`super::lance_attacks`]; the live path never needs it.
#[cfg(test)]
pub(crate) fn lance_attacks(
    color: shogi_core::Color,
    square: Square,
    occupied: Bitboard,
) -> Bitboard {
    let rank_delta = if color == shogi_core::Color::Black {
        -1
    } else {
        1
    };
    ray_attacks(square, occupied, &[(0, rank_delta)])
}
