//! Magic-bitboard slider attacks.
//!
//! Each strided line (the rank, and the two diagonals) gets its own magic:
//! the relevant occupancy is shifted down so it fits in a `u64`, multiplied
//! by a constant that gathers its scattered bits into a dense index, and
//! used to look up a precomputed attack set.
//!
//! Every line has at most 7 relevant squares, so each table is 128 entries
//! per square.
//!
//! ## What is generated, and what is not
//!
//! Only the **multipliers** are generated ([`super::magics`], produced by our
//! own [`gen_magics`](../../examples/gen_magics.rs)) — a multiplier is the
//! result of a search and cannot be derived. Everything else about a [`Magic`]
//! *can* be, so it is: [`derive_magics`] recomputes the mask and both shifts
//! from [`super::relevant_mask`], the crate's own geometry, and the attack
//! tables are const-evaluated on top. So the generated file cannot drift out
//! of step with the board, and no table is ever transcribed from another
//! project.
//!
//! A multiplier that does *not* work is caught by [`line_table`], which
//! rejects at compile time any two occupancies that share a slot without
//! sharing an attack set.
//!
//! The file direction is not done here — it is nine contiguous bits, so
//! [`super::file_attacks`] indexes it directly.

// A backend that the feature flags did not select is still compiled, so
// that the oracle tests and the A/B benchmarks can reach it.
#![allow(dead_code)]

use shogi_core::Square;

use super::magics::{DIAGONAL_DOWN_MULTIPLIERS, DIAGONAL_UP_MULTIPLIERS, RANK_MULTIPLIERS};
use super::{DIAGONAL_DOWN_LINE, DIAGONAL_UP_LINE, LineKind, RANK_LINE, relevant_mask, walk_line};
use crate::bitboard::Bitboard;

/// One square's magic for one line family. Only `magic` is generated; the
/// rest is derived by [`derive_magics`].
pub(crate) struct Magic {
    /// The relevant occupancy, already shifted down by `shift_in`.
    pub(crate) mask: u64,
    pub(crate) magic: u64,
    /// How far to shift the board right so the relevant bits fit a `u64`.
    pub(crate) shift_in: u32,
    /// `64 - index_bits`.
    pub(crate) shift_out: u32,
}

const NO_MAGIC: Magic = Magic {
    mask: 0,
    magic: 0,
    shift_in: 0,
    shift_out: 63,
};

static RANK_MAGICS: [Magic; 81] = derive_magics(&RANK_MULTIPLIERS, RANK_LINE);
static DIAGONAL_UP_MAGICS: [Magic; 81] = derive_magics(&DIAGONAL_UP_MULTIPLIERS, DIAGONAL_UP_LINE);
static DIAGONAL_DOWN_MAGICS: [Magic; 81] =
    derive_magics(&DIAGONAL_DOWN_MULTIPLIERS, DIAGONAL_DOWN_LINE);

/// Rebuilds each square's mask and shifts from the line geometry, pairing
/// them with the generated multiplier.
const fn derive_magics(multipliers: &[u64; 81], line: LineKind) -> [Magic; 81] {
    let mut magics = [NO_MAGIC; 81];
    let mut index = 0;
    while index < 81 {
        let relevant = relevant_mask(index, line);
        // A short diagonal can have no blockable square at all, and
        // `trailing_zeros` of zero is the width of the type.
        let shift_in = if relevant == 0 {
            0
        } else {
            relevant.trailing_zeros()
        };
        assert!(
            128 - relevant.leading_zeros() - shift_in <= 64,
            "relevant squares do not fit a u64 once normalized"
        );
        let mask = (relevant >> shift_in) as u64;
        let bits = mask.count_ones();
        // The far end of each ray is excluded, so a 9-square line leaves at
        // most 7 blockable squares — which is what sizes `LineTable` and
        // what keeps every slot below 128.
        assert!(bits <= 7, "line has more blockable squares than expected");
        // A line with nothing to block still needs its one slot, and a
        // shift of 64 would be undefined.
        let shift_out = 64 - if bits == 0 { 1 } else { bits };
        magics[index] = Magic {
            mask,
            magic: multipliers[index],
            shift_in,
            shift_out,
        };
        index += 1;
    }
    magics
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
        // Two occupancies may share a slot only if they attack the same
        // squares — that is exactly what makes a multiplier valid. Checking
        // it here means a `magics.rs` whose numbers have been corrupted
        // fails to compile rather than silently returning wrong attacks.
        let mut filled = [false; 128];
        // Walk every subset of the mask with the carry-rippler trick.
        let mut occupancy = 0u64;
        loop {
            let occupied = (occupancy as u128) << magic.shift_in;
            let slot = (occupancy.wrapping_mul(magic.magic) >> magic.shift_out) as usize;
            let attacks = walk_line(index, occupied, line);
            assert!(
                !filled[slot] || table[index][slot].bits() == attacks,
                "magic multiplier maps occupancies with different attacks onto one slot"
            );
            filled[slot] = true;
            table[index][slot] = Bitboard::from_bits(attacks);
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
