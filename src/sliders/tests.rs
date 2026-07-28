//! Every backend is held against [`naive`], the ray-walking oracle.
//!
//! The checks are *exhaustive over the relevant occupancy* of each line:
//! only the squares strictly between the origin and the board edge can
//! change a ray's result, so enumerating all 2^k subsets of those squares
//! (k <= 7 per line) covers every distinguishable input. Squares outside the
//! lines are then fuzzed on top to confirm they are correctly ignored.

use shogi_core::{Color, Square};

use super::{lance_attacks, magic, naive, qugiy};
use crate::bitboard::Bitboard;

/// splitmix64 (public-domain algorithm by Sebastiano Vigna).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The squares that can block a ray from `square` in the given directions:
/// every square on those rays except the last one of each.
fn relevant_squares(square: Square, steps: &[(i8, i8)]) -> Vec<Square> {
    let mut squares = Vec::new();
    for &(file_delta, rank_delta) in steps {
        let mut current = square;
        while let Some(next) = current.shift(file_delta, rank_delta) {
            // The far end of a ray is attacked whether or not it is
            // occupied, so it is never relevant.
            if next.shift(file_delta, rank_delta).is_some() {
                squares.push(next);
            }
            current = next;
        }
    }
    squares
}

/// Runs `check` over every subset of the relevant squares for each of the
/// 81 origins.
fn for_each_relevant_occupancy(steps: &[(i8, i8)], mut check: impl FnMut(Square, Bitboard)) {
    for square in Square::all() {
        let relevant = relevant_squares(square, steps);
        assert!(relevant.len() <= 28, "occupancy enumeration would blow up");
        for subset in 0u32..(1 << relevant.len()) {
            let mut occupied = Bitboard::single(square);
            for (bit, &blocker) in relevant.iter().enumerate() {
                if subset & (1 << bit) != 0 {
                    occupied |= Bitboard::single(blocker);
                }
            }
            check(square, occupied);
        }
    }
}

const ORTHOGONAL: [(i8, i8); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
const DIAGONAL: [(i8, i8); 4] = [(-1, -1), (1, -1), (-1, 1), (1, 1)];

#[test]
fn rook_backends_match_naive_exhaustively() {
    for_each_relevant_occupancy(&ORTHOGONAL, |square, occupied| {
        let expected = naive::rook_attacks(square, occupied);
        assert_eq!(
            qugiy::rook_attacks(square, occupied),
            expected,
            "qugiy rook on {square:?} with occupancy {occupied:?}"
        );
        assert_eq!(
            magic::rook_attacks(square, occupied),
            expected,
            "magic rook on {square:?} with occupancy {occupied:?}"
        );
    });
}

#[test]
fn bishop_backends_match_naive_exhaustively() {
    for_each_relevant_occupancy(&DIAGONAL, |square, occupied| {
        let expected = naive::bishop_attacks(square, occupied);
        assert_eq!(
            qugiy::bishop_attacks(square, occupied),
            expected,
            "qugiy bishop on {square:?} with occupancy {occupied:?}"
        );
        assert_eq!(
            magic::bishop_attacks(square, occupied),
            expected,
            "magic bishop on {square:?} with occupancy {occupied:?}"
        );
    });
}

// There is no test that the magic masks and shifts match the line geometry:
// they are now *derived* from it (`magic::derive_magics`), so the two cannot
// disagree. Whether each generated multiplier actually works is asserted at
// compile time by `magic::line_table`, and whether the committed multipliers
// are the ones our generator produces is checked in CI by
// `cargo run --release --example gen_magics -- --check`.

#[test]
fn shared_lance_table_matches_naive_exhaustively() {
    for color in Color::all() {
        let steps = [(0, if color == Color::Black { -1 } else { 1 })];
        for_each_relevant_occupancy(&steps, |square, occupied| {
            assert_eq!(
                lance_attacks(color, square, occupied),
                naive::lance_attacks(color, square, occupied),
                "{color:?} lance on {square:?} with occupancy {occupied:?}"
            );
        });
    }
}

/// Occupancy outside the rays must not change anything, and full-board
/// random occupancies must agree too.
#[test]
fn backends_agree_on_random_full_occupancies() {
    let mut state = 0x5150_1234_abcd_ef01;
    for _ in 0..20_000 {
        let bits = ((splitmix64(&mut state) as u128) << 64) | splitmix64(&mut state) as u128;
        // Bias toward realistic densities as well as sparse/dense extremes.
        let bits = match state % 3 {
            0 => bits,
            1 => bits & (((splitmix64(&mut state) as u128) << 64) | splitmix64(&mut state) as u128),
            _ => bits | (Bitboard::ALL.bits() & !bits & (bits >> 1)),
        };
        let occupied = Bitboard::from_bits(bits & Bitboard::ALL.bits());
        for square in Square::all() {
            let occupied = occupied | Bitboard::single(square);
            let rook = naive::rook_attacks(square, occupied);
            assert_eq!(
                qugiy::rook_attacks(square, occupied),
                rook,
                "qugiy rook on {square:?}"
            );
            assert_eq!(
                magic::rook_attacks(square, occupied),
                rook,
                "magic rook on {square:?}"
            );
            let bishop = naive::bishop_attacks(square, occupied);
            assert_eq!(
                qugiy::bishop_attacks(square, occupied),
                bishop,
                "qugiy bishop on {square:?}"
            );
            assert_eq!(
                magic::bishop_attacks(square, occupied),
                bishop,
                "magic bishop on {square:?}"
            );
            for color in Color::all() {
                assert_eq!(
                    lance_attacks(color, square, occupied),
                    naive::lance_attacks(color, square, occupied),
                    "{color:?} lance on {square:?}"
                );
            }
        }
    }
}
