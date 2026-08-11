//! Slider attacks — the M4 optimization swap boundary.
//!
//! Every backend implements the same three functions (`lance_attacks`,
//! `bishop_attacks`, `rook_attacks`); `tables.rs` re-exports whichever one
//! the feature flags select. `magic` and `qugiy` are always *compiled*, and
//! `naive` whenever something can reach it, so the tests can hold each of
//! them against `naive`, the obviously-correct ray-walking reference.
//!
//! ## Layout facts the fast backends exploit
//!
//! The bitboard is file-major: `array_index = (file - 1) * 9 + (rank - 1)`.
//! So one **file** is 9 *contiguous* bits, while a **rank** is 9 bits at
//! stride 9 and the two diagonals are at stride 10 and 8.
//!
//! The contiguous file direction is therefore the easy case, and it is
//! handled here — identically for every backend — by a direct occupancy
//! index (no multiply, no magic). Backends differ only in how they do the
//! strided lines: the rank (rook) and the two diagonals (bishop).
//!
//! ## Relevant occupancy
//!
//! A blocker on the last square of a ray never changes the result (that
//! square is attacked either way), so the two end squares of every line are
//! excluded from the occupancy index. A 9-square line therefore has at most
//! 7 relevant bits.

use shogi_core::{Color, Square};

use crate::bitboard::{Bitboard, file_of, index_of, on_board, rank_of};

/// Identifies one of the three strided line families by its rank delta per
/// file step: `0` is the rank, `1` the "+10" diagonal, `-1` the "+8" one.
pub(crate) type LineKind = i8;
pub(crate) const RANK_LINE: LineKind = 0;
pub(crate) const DIAGONAL_UP_LINE: LineKind = 1;
pub(crate) const DIAGONAL_DOWN_LINE: LineKind = -1;

/// The squares a slider on `index` reaches along `line`, stopping at (and
/// including) the first square set in `occupied`. Excludes the origin.
pub(crate) const fn walk_line(index: usize, occupied: u128, line: LineKind) -> u128 {
    let mut bits = 0u128;
    let mut direction = 0;
    while direction < 2 {
        let sign: i8 = if direction == 0 { 1 } else { -1 };
        let (mut file, mut rank) = (file_of(index), rank_of(index));
        loop {
            file += sign;
            rank += sign * line;
            if !on_board(file, rank) {
                break;
            }
            let to = index_of(file, rank);
            bits |= 1 << to;
            if occupied & (1 << to) != 0 {
                break;
            }
        }
        direction += 1;
    }
    bits
}

/// The squares of `line` whose occupancy can change the result: every
/// square the slider reaches except the last one of each ray, which is
/// attacked whether or not something stands on it.
pub(crate) const fn relevant_mask(index: usize, line: LineKind) -> u128 {
    let mut bits = 0u128;
    let mut direction = 0;
    while direction < 2 {
        let sign: i8 = if direction == 0 { 1 } else { -1 };
        let (mut file, mut rank) = (file_of(index), rank_of(index));
        loop {
            file += sign;
            rank += sign * line;
            if !on_board(file, rank) {
                break;
            }
            // Only a square with another square beyond it can block.
            if on_board(file + sign, rank + sign * line) {
                bits |= 1 << index_of(file, rank);
            }
        }
        direction += 1;
    }
    bits
}

pub(crate) mod magic;
mod magics;
// The oracle is compiled when something can actually reach it: the tests
// always, the feature that selects it as the live backend, and the bench
// suite that measures it as the A/B floor.
#[cfg(any(test, feature = "slider-naive", feature = "bench-internals"))]
pub(crate) mod naive;
pub(crate) mod qugiy;
#[cfg(test)]
mod tests;

// Exactly one backend is live. Cargo features are additive, so the
// selection is a priority order rather than a set of exclusive options,
// and `magic` — the one the M4 bake-off adopted — is the unflagged default.
#[cfg(not(any(feature = "slider-naive", feature = "slider-qugiy")))]
pub(crate) use magic as active;
#[cfg(feature = "slider-naive")]
pub(crate) use naive as active;
#[cfg(all(feature = "slider-qugiy", not(feature = "slider-naive")))]
pub(crate) use qugiy as active;

/// The 9-bit attack pattern within one file for a slider on rank
/// `rank_index` (0-based), given the file's 7 relevant occupancy bits.
///
/// Indexed by rank rather than by square because a file is a pure
/// translation of any other: the pattern is shifted into place by
/// `(file - 1) * 9`. 9 × 128 × 2 bytes = 2.3 KiB, so it stays L1-resident.
static FILE_ATTACKS: [[u16; 128]; 9] = file_attacks_table();

/// As [`FILE_ATTACKS`], but only the forward direction: `[color][rank][occ]`.
static LANCE_ATTACKS: [[[u16; 128]; 9]; 2] = lance_attacks_table();

/// Walks one file. `rank_index` is 0-based, `occupancy` holds the file's
/// nine squares in bits 0..9, and `toward_rank9` / `toward_rank1` select the
/// directions to walk.
const fn walk_file(
    rank_index: usize,
    occupancy: u16,
    toward_rank1: bool,
    toward_rank9: bool,
) -> u16 {
    let mut attacks = 0u16;
    if toward_rank1 {
        let mut rank = rank_index;
        while rank > 0 {
            rank -= 1;
            attacks |= 1 << rank;
            if occupancy & (1 << rank) != 0 {
                break;
            }
        }
    }
    if toward_rank9 {
        let mut rank = rank_index + 1;
        while rank < 9 {
            attacks |= 1 << rank;
            if occupancy & (1 << rank) != 0 {
                break;
            }
            rank += 1;
        }
    }
    attacks
}

const fn file_attacks_table() -> [[u16; 128]; 9] {
    let mut table = [[0u16; 128]; 9];
    let mut rank = 0;
    while rank < 9 {
        let mut relevant = 0;
        while relevant < 128 {
            // The relevant bits are ranks 2..=8, i.e. bits 1..=7.
            table[rank][relevant] = walk_file(rank, (relevant as u16) << 1, true, true);
            relevant += 1;
        }
        rank += 1;
    }
    table
}

const fn lance_attacks_table() -> [[[u16; 128]; 9]; 2] {
    let mut table = [[[0u16; 128]; 9]; 2];
    let mut rank = 0;
    while rank < 9 {
        let mut relevant = 0;
        while relevant < 128 {
            let occupancy = (relevant as u16) << 1;
            // Black advances toward rank 1, White toward rank 9.
            table[0][rank][relevant] = walk_file(rank, occupancy, true, false);
            table[1][rank][relevant] = walk_file(rank, occupancy, false, true);
            relevant += 1;
        }
        rank += 1;
    }
    table
}

/// The file-direction (vertical) attacks of a rook on `square`.
///
/// Shared by every backend: one file is nine contiguous bits, so the
/// occupancy index needs no multiply.
#[inline(always)]
pub(crate) fn file_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    let index = square.array_index();
    let (file_base, rank) = (index - index % 9, index % 9);
    let relevant = ((occupied.bits() >> (file_base + 1)) & 0x7f) as usize;
    Bitboard::from_bits((FILE_ATTACKS[rank][relevant] as u128) << file_base)
}

/// Lance attacks. Backend-independent for the same reason as
/// [`file_attacks`].
#[inline(always)]
pub(crate) fn lance_attacks(color: Color, square: Square, occupied: Bitboard) -> Bitboard {
    let index = square.array_index();
    let (file_base, rank) = (index - index % 9, index % 9);
    let relevant = ((occupied.bits() >> (file_base + 1)) & 0x7f) as usize;
    let pattern = LANCE_ATTACKS[color.array_index()][rank][relevant];
    Bitboard::from_bits((pattern as u128) << file_base)
}
