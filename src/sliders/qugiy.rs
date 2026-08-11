//! Table-free slider attacks by `u128` arithmetic.
//!
//! Both strided line directions are resolved with the classic `o - 2r`
//! subtraction trick, the same idea the Qugiy appeal document popularized
//! for shogi (written here from the published description of the technique,
//! not from any engine's source).
//!
//! For a line mask `m` that excludes the origin square `s`, let
//! `o = occupied & m`. Subtracting `2·2^s` from `o` flips exactly the bits
//! from `s + 1` up to and including the lowest blocker above `s`, so
//! `o ^ (o - 2·2^s)` is the "upward" ray. Mirroring the board turns the
//! downward ray into the same computation, and XOR-ing the two results
//! yields both rays at once (the shared `o` term cancels):
//!
//! ```text
//! attacks = ((o - 2·2^s) ^ mirror(mirror(o) - 2·2^(80 - s))) & m
//! ```
//!
//! The file direction is not done here — it is nine contiguous bits, so
//! [`super::file_attacks`] indexes it directly.

use shogi_core::Square;

use super::{DIAGONAL_DOWN_LINE, DIAGONAL_UP_LINE, LineKind, RANK_LINE, walk_line};
use crate::bitboard::Bitboard;

/// Squares sharing a rank with the origin, excluding the origin itself.
static RANK_MASK: [Bitboard; 81] = line_masks(RANK_LINE);
/// The `file + 1, rank + 1` diagonal (index stride 10), excluding the origin.
static DIAGONAL_UP_MASK: [Bitboard; 81] = line_masks(DIAGONAL_UP_LINE);
/// The `file + 1, rank - 1` diagonal (index stride 8), excluding the origin.
static DIAGONAL_DOWN_MASK: [Bitboard; 81] = line_masks(DIAGONAL_DOWN_LINE);

/// The full line through each square (no blockers), excluding the origin.
const fn line_masks(line: LineKind) -> [Bitboard; 81] {
    let mut table = [Bitboard::EMPTY; 81];
    let mut index = 0;
    while index < 81 {
        table[index] = Bitboard::from_bits(walk_line(index, 0, line));
        index += 1;
    }
    table
}

/// Mirrors the 81-square board (square `i` becomes square `80 - i`).
#[inline(always)]
const fn mirror(bits: u128) -> u128 {
    bits.reverse_bits() >> 47
}

/// Attacks along one line, both directions, stopping at the first blocker.
#[inline(always)]
fn line_attacks(index: usize, occupied: Bitboard, mask: Bitboard) -> u128 {
    let mask = mask.bits();
    let occupied = occupied.bits() & mask;
    // `1 << 81` never overflows a u128, so neither shift below can wrap.
    let upward = occupied.wrapping_sub(1u128 << (index + 1));
    let downward = mirror(mirror(occupied).wrapping_sub(1u128 << (81 - index)));
    (upward ^ downward) & mask
}

pub(crate) fn bishop_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    let index = square.array_index();
    Bitboard::from_bits(
        line_attacks(index, occupied, DIAGONAL_UP_MASK[index])
            | line_attacks(index, occupied, DIAGONAL_DOWN_MASK[index]),
    )
}

pub(crate) fn rook_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    let index = square.array_index();
    Bitboard::from_bits(
        line_attacks(index, occupied, RANK_MASK[index])
            | super::file_attacks(square, occupied).bits(),
    )
}
