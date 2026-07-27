//! Magic-bitboard slider attacks.
//!
//! Each strided line (the rank, and the two diagonals) gets its own magic:
//! the relevant occupancy is shifted down so it fits in a `u64`, multiplied
//! by a constant that gathers its scattered bits into a dense index, and
//! used to look up a precomputed attack set.
//!
//! Every line has at most 7 relevant squares, so each table is 128 entries
//! per square. The multipliers come from [`super::magics`], which our own
//! [`gen_magics`](../../examples/gen_magics.rs) generator brute-forces and
//! self-verifies; the tables below are const-evaluated from them, so no
//! table is ever transcribed from another project.
//!
//! The file direction is not done here — it is nine contiguous bits, so
//! [`super::file_attacks`] indexes it directly.

// A backend that the feature flags did not select is still compiled, so
// that the oracle tests and the A/B benchmarks can reach it.
#![allow(dead_code)]

use shogi_core::Square;

use super::magics::{DIAGONAL_DOWN_MAGICS, DIAGONAL_UP_MAGICS, RANK_MAGICS};
use super::{DIAGONAL_DOWN_LINE, DIAGONAL_UP_LINE, LineKind, RANK_LINE, relevant_mask, walk_line};
use crate::bitboard::Bitboard;

/// One square's magic for one line family.
pub(crate) struct Magic {
    /// The relevant occupancy, already shifted down by `shift_in`.
    pub(crate) mask: u64,
    pub(crate) magic: u64,
    /// How far to shift the board right so the relevant bits fit a `u64`.
    pub(crate) shift_in: u32,
    /// `64 - index_bits`.
    pub(crate) shift_out: u32,
}

/// 81 squares × 2^7 occupancies × 16 bytes = 162 KiB per line family.
type LineTable = [[Bitboard; 128]; 81];

static RANK_ATTACKS: LineTable = line_table(&RANK_MAGICS, RANK_LINE);
static DIAGONAL_UP_ATTACKS: LineTable = line_table(&DIAGONAL_UP_MAGICS, DIAGONAL_UP_LINE);
static DIAGONAL_DOWN_ATTACKS: LineTable = line_table(&DIAGONAL_DOWN_MAGICS, DIAGONAL_DOWN_LINE);

const fn line_table(magics: &[Magic; 81], line: LineKind) -> LineTable {
    let mut table = [[Bitboard::EMPTY; 128]; 81];
    let mut index = 0;
    while index < 81 {
        let magic = &magics[index];
        // The generated constants carry their own idea of which squares can
        // block. Checking it here means a `magics.rs` that has drifted from
        // the board geometry fails to compile rather than silently
        // producing wrong attacks.
        assert!(
            (magic.mask as u128) << magic.shift_in == relevant_mask(index, line),
            "generated magic mask disagrees with the line geometry"
        );
        assert!(magic.shift_out < 64, "magic shift_out would overflow");
        // Walk every subset of the mask with the carry-rippler trick.
        let mut occupancy = 0u64;
        loop {
            let occupied = (occupancy as u128) << magic.shift_in;
            let slot = (occupancy.wrapping_mul(magic.magic) >> magic.shift_out) as usize;
            table[index][slot] = Bitboard::from_bits(walk_line(index, occupied, line));
            occupancy = occupancy.wrapping_sub(magic.mask) & magic.mask;
            if occupancy == 0 {
                break;
            }
        }
        index += 1;
    }
    table
}

#[inline(always)]
fn line_attacks(index: usize, occupied: Bitboard, magics: &[Magic; 81], table: &LineTable) -> u128 {
    let magic = &magics[index];
    let relevant = ((occupied.bits() >> magic.shift_in) as u64) & magic.mask;
    let slot = (relevant.wrapping_mul(magic.magic) >> magic.shift_out) as usize;
    table[index][slot].bits()
}

pub(crate) fn bishop_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    let index = square.array_index();
    Bitboard::from_bits(
        line_attacks(index, occupied, &DIAGONAL_UP_MAGICS, &DIAGONAL_UP_ATTACKS)
            | line_attacks(
                index,
                occupied,
                &DIAGONAL_DOWN_MAGICS,
                &DIAGONAL_DOWN_ATTACKS,
            ),
    )
}

pub(crate) fn rook_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    let index = square.array_index();
    Bitboard::from_bits(
        line_attacks(index, occupied, &RANK_MAGICS, &RANK_ATTACKS)
            | super::file_attacks(square, occupied).bits(),
    )
}
