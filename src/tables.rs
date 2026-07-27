//! Attack tables and naive slider attacks.
//!
//! Step-piece attacks are const-evaluated per square; slider attacks walk
//! rays square by square. Replacing the latter with Qugiy/magic
//! implementations is a later, benchmark-driven decision.
//!
//! The table builders work on raw [`Square::array_index`] arithmetic rather
//! than [`Square::shift`], which is not a `const fn`. Every const table is
//! checked against a `Square`-based reference implementation in the tests
//! below.

use shogi_core::{Color, Piece, PieceKind, Square};

use crate::bitboard::{Bitboard, file_of, index_of, on_board, rank_of};

/// `(file_delta, rank_delta)` steps, from Black's point of view
/// (Black moves toward rank 1, i.e. negative rank delta).
const PAWN_STEPS: [(i8, i8); 1] = [(0, -1)];
const KNIGHT_STEPS: [(i8, i8); 2] = [(-1, -2), (1, -2)];
const SILVER_STEPS: [(i8, i8); 5] = [(-1, -1), (0, -1), (1, -1), (-1, 1), (1, 1)];
const GOLD_STEPS: [(i8, i8); 6] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (0, 1)];
const ORTHOGONAL_STEPS: [(i8, i8); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
const DIAGONAL_STEPS: [(i8, i8); 4] = [(-1, -1), (1, -1), (-1, 1), (1, 1)];
const KING_STEPS: [(i8, i8); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

const fn step_table(steps: &[(i8, i8)]) -> [Bitboard; 81] {
    let mut table = [Bitboard::EMPTY; 81];
    let mut index = 0;
    while index < 81 {
        let (file, rank) = (file_of(index), rank_of(index));
        let mut bits = 0u128;
        let mut i = 0;
        while i < steps.len() {
            let (file_delta, rank_delta) = steps[i];
            let (to_file, to_rank) = (file + file_delta, rank + rank_delta);
            if on_board(to_file, to_rank) {
                bits |= 1 << index_of(to_file, to_rank);
            }
            i += 1;
        }
        table[index] = Bitboard::from_bits(bits);
        index += 1;
    }
    table
}

/// Reflects Black's steps into White's (rotate the board 180°).
const fn flip_steps<const N: usize>(steps: [(i8, i8); N]) -> [(i8, i8); N] {
    let mut flipped = steps;
    let mut i = 0;
    while i < N {
        let (file_delta, rank_delta) = steps[i];
        flipped[i] = (-file_delta, -rank_delta);
        i += 1;
    }
    flipped
}

/// Builds `[Black's table, White's table]`.
const fn step_table_both<const N: usize>(steps: [(i8, i8); N]) -> [[Bitboard; 81]; 2] {
    [step_table(&steps), step_table(&flip_steps(steps))]
}

static PAWN_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(PAWN_STEPS);
static KNIGHT_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(KNIGHT_STEPS);
static SILVER_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(SILVER_STEPS);
static GOLD_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(GOLD_STEPS);
static KING_ATTACKS: [Bitboard; 81] = step_table(&KING_STEPS);
static ORTHOGONAL_ATTACKS: [Bitboard; 81] = step_table(&ORTHOGONAL_STEPS);
static DIAGONAL_ATTACKS: [Bitboard; 81] = step_table(&DIAGONAL_STEPS);

/// `BETWEEN[a][b]`: the squares strictly between `a` and `b` if they share a
/// rank, file or diagonal; empty otherwise.
static BETWEEN: [[Bitboard; 81]; 81] = between_table();

const fn between_table() -> [[Bitboard; 81]; 81] {
    let mut table = [[Bitboard::EMPTY; 81]; 81];
    let mut from = 0;
    while from < 81 {
        let mut step = 0;
        while step < KING_STEPS.len() {
            let (file_delta, rank_delta) = KING_STEPS[step];
            let (mut file, mut rank) = (file_of(from), rank_of(from));
            let mut between = 0u128;
            loop {
                file += file_delta;
                rank += rank_delta;
                if !on_board(file, rank) {
                    break;
                }
                let to = index_of(file, rank);
                table[from][to] = Bitboard::from_bits(between);
                between |= 1 << to;
            }
            step += 1;
        }
        from += 1;
    }
    table
}

pub(crate) fn pawn_attacks(color: Color, square: Square) -> Bitboard {
    PAWN_ATTACKS[color.array_index()][square.array_index()]
}

pub(crate) fn knight_attacks(color: Color, square: Square) -> Bitboard {
    KNIGHT_ATTACKS[color.array_index()][square.array_index()]
}

pub(crate) fn silver_attacks(color: Color, square: Square) -> Bitboard {
    SILVER_ATTACKS[color.array_index()][square.array_index()]
}

pub(crate) fn gold_attacks(color: Color, square: Square) -> Bitboard {
    GOLD_ATTACKS[color.array_index()][square.array_index()]
}

pub(crate) fn king_attacks(square: Square) -> Bitboard {
    KING_ATTACKS[square.array_index()]
}

/// The four orthogonally adjacent squares (the promoted bishop's extra steps).
pub(crate) fn orthogonal_attacks(square: Square) -> Bitboard {
    ORTHOGONAL_ATTACKS[square.array_index()]
}

/// The four diagonally adjacent squares (the promoted rook's extra steps).
pub(crate) fn diagonal_attacks(square: Square) -> Bitboard {
    DIAGONAL_ATTACKS[square.array_index()]
}

/// `LINE[a][b]`: every square of the rank, file or diagonal through `a` and
/// `b`, both included and extended to the board edges; empty when they are
/// not aligned (and on the diagonal, `LINE[a][a]`).
///
/// This is exactly where a piece pinned against its king may still move: a
/// pin means the piece stands on such a line, and staying on it — including
/// capturing the pinner — keeps the line blocked.
static LINE: [[Bitboard; 81]; 81] = line_table();

/// The four axes; walking each one both ways covers the whole line.
const AXES: [(i8, i8); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

const fn line_table() -> [[Bitboard; 81]; 81] {
    let mut table = [[Bitboard::EMPTY; 81]; 81];
    let mut a = 0;
    while a < 81 {
        let mut axis = 0;
        while axis < 4 {
            let (file_delta, rank_delta) = AXES[axis];
            let mut line = 1u128 << a;
            let mut direction = 0;
            while direction < 2 {
                let sign: i8 = if direction == 0 { 1 } else { -1 };
                let (mut file, mut rank) = (file_of(a), rank_of(a));
                loop {
                    file += sign * file_delta;
                    rank += sign * rank_delta;
                    if !on_board(file, rank) {
                        break;
                    }
                    line |= 1 << index_of(file, rank);
                }
                direction += 1;
            }
            // Every other square of this line shares it.
            let mut rest = line & !(1 << a);
            while rest != 0 {
                let b = rest.trailing_zeros() as usize;
                table[a][b] = Bitboard::from_bits(line);
                rest &= rest - 1;
            }
            axis += 1;
        }
        a += 1;
    }
    table
}

/// Squares strictly between `a` and `b` (empty when not aligned).
pub(crate) fn between(a: Square, b: Square) -> Bitboard {
    BETWEEN[a.array_index()][b.array_index()]
}

/// The full line through `a` and `b` (empty when not aligned).
#[inline(always)]
pub(crate) fn line(a: Square, b: Square) -> Bitboard {
    LINE[a.array_index()][b.array_index()]
}

/// Squares at relative rank `1..=max_relative_rank` for `color`, i.e. the
/// `max_relative_rank` ranks nearest the opponent.
const fn relative_rank_mask(color: usize, max_relative_rank: i8) -> Bitboard {
    let mut bits = 0u128;
    let mut index = 0;
    while index < 81 {
        let rank = rank_of(index);
        // Black advances toward rank 1, White toward rank 9.
        let relative = if color == 0 { rank } else { 10 - rank };
        if relative <= max_relative_rank {
            bits |= 1 << index;
        }
        index += 1;
    }
    Bitboard::from_bits(bits)
}

const fn relative_rank_masks(max_relative_rank: i8) -> [Bitboard; 2] {
    [
        relative_rank_mask(0, max_relative_rank),
        relative_rank_mask(1, max_relative_rank),
    ]
}

/// The three ranks where moving in or out enables promotion.
static PROMOTION_ZONE: [Bitboard; 2] = relative_rank_masks(3);
/// The last rank: a pawn or lance landing here must promote.
static LAST_RANK: [Bitboard; 2] = relative_rank_masks(1);
/// The last two ranks: a knight landing here must promote.
static LAST_TWO_RANKS: [Bitboard; 2] = relative_rank_masks(2);

#[inline(always)]
pub(crate) fn promotion_zone(color: Color) -> Bitboard {
    PROMOTION_ZONE[color.array_index()]
}

/// The ranks on which `piece_kind` would have no move left, so promotion is
/// compulsory. Empty for pieces that are never forced.
#[inline(always)]
pub(crate) fn forced_promotion_zone(color: Color, piece_kind: PieceKind) -> Bitboard {
    match piece_kind {
        PieceKind::Pawn | PieceKind::Lance => LAST_RANK[color.array_index()],
        PieceKind::Knight => LAST_TWO_RANKS[color.array_index()],
        _ => Bitboard::EMPTY,
    }
}

// Slider attacks live in `crate::sliders`, which is the swap boundary for
// the M4 backends; these are the crate-wide entry points.
pub(crate) use crate::sliders::active::{bishop_attacks, rook_attacks};
pub(crate) use crate::sliders::lance_attacks;

/// The squares attacked by `piece` on `square`, given `occupied` (relevant to
/// sliders only).
pub(crate) fn attacks_of(piece: Piece, square: Square, occupied: Bitboard) -> Bitboard {
    let (piece_kind, color) = piece.to_parts();
    match piece_kind {
        PieceKind::Pawn => pawn_attacks(color, square),
        PieceKind::Lance => lance_attacks(color, square, occupied),
        PieceKind::Knight => knight_attacks(color, square),
        PieceKind::Silver => silver_attacks(color, square),
        PieceKind::Gold
        | PieceKind::ProPawn
        | PieceKind::ProLance
        | PieceKind::ProKnight
        | PieceKind::ProSilver => gold_attacks(color, square),
        PieceKind::King => king_attacks(square),
        PieceKind::Bishop => bishop_attacks(square, occupied),
        PieceKind::Rook => rook_attacks(square, occupied),
        PieceKind::ProBishop => bishop_attacks(square, occupied) | orthogonal_attacks(square),
        PieceKind::ProRook => rook_attacks(square, occupied) | diagonal_attacks(square),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(file: u8, rank: u8) -> Square {
        Square::new(file, rank).unwrap()
    }

    /// Reference implementation of [`step_table`] in terms of
    /// [`Square::shift`] — the const builders index raw `array_index`
    /// arithmetic instead, so every table is cross-checked against this.
    fn step_table_reference(steps: &[(i8, i8)]) -> [Bitboard; 81] {
        let mut table = [Bitboard::EMPTY; 81];
        for square in Square::all() {
            for &(file_delta, rank_delta) in steps {
                if let Some(to) = square.shift(file_delta, rank_delta) {
                    table[square.array_index()] |= Bitboard::single(to);
                }
            }
        }
        table
    }

    fn step_table_both_reference(steps: &[(i8, i8)]) -> [[Bitboard; 81]; 2] {
        let flipped: Vec<(i8, i8)> = steps
            .iter()
            .map(|&(file_delta, rank_delta)| (-file_delta, -rank_delta))
            .collect();
        [step_table_reference(steps), step_table_reference(&flipped)]
    }

    #[test]
    fn const_index_arithmetic_matches_square() {
        for square in Square::all() {
            let index = square.array_index();
            assert_eq!(file_of(index), square.file() as i8);
            assert_eq!(rank_of(index), square.rank() as i8);
            assert_eq!(index_of(square.file() as i8, square.rank() as i8), index);
        }
    }

    #[test]
    fn const_step_tables_match_reference() {
        for (name, table, steps) in [
            ("king", &KING_ATTACKS, &KING_STEPS[..]),
            ("orthogonal", &ORTHOGONAL_ATTACKS, &ORTHOGONAL_STEPS[..]),
            ("diagonal", &DIAGONAL_ATTACKS, &DIAGONAL_STEPS[..]),
        ] {
            assert_eq!(*table, step_table_reference(steps), "{name}");
        }
        for (name, table, steps) in [
            ("pawn", &PAWN_ATTACKS, &PAWN_STEPS[..]),
            ("knight", &KNIGHT_ATTACKS, &KNIGHT_STEPS[..]),
            ("silver", &SILVER_ATTACKS, &SILVER_STEPS[..]),
            ("gold", &GOLD_ATTACKS, &GOLD_STEPS[..]),
        ] {
            assert_eq!(*table, step_table_both_reference(steps), "{name}");
        }
    }

    #[test]
    fn const_between_table_matches_reference() {
        let mut reference = Box::new([[Bitboard::EMPTY; 81]; 81]);
        for from in Square::all() {
            for (file_delta, rank_delta) in KING_STEPS {
                let mut between = Bitboard::EMPTY;
                let mut current = from;
                while let Some(next) = current.shift(file_delta, rank_delta) {
                    reference[from.array_index()][next.array_index()] = between;
                    between |= Bitboard::single(next);
                    current = next;
                }
            }
        }
        assert_eq!(BETWEEN, *reference);
    }

    #[test]
    fn pawn_moves_forward() {
        assert_eq!(
            pawn_attacks(Color::Black, sq(5, 5)),
            Bitboard::single(sq(5, 4))
        );
        assert_eq!(
            pawn_attacks(Color::White, sq(5, 5)),
            Bitboard::single(sq(5, 6))
        );
        // No squares beyond the last rank.
        assert!(pawn_attacks(Color::Black, sq(5, 1)).is_empty());
        assert!(pawn_attacks(Color::White, sq(5, 9)).is_empty());
    }

    #[test]
    fn knight_edges() {
        let attacks = knight_attacks(Color::Black, sq(5, 5));
        assert_eq!(
            attacks,
            Bitboard::single(sq(4, 3)) | Bitboard::single(sq(6, 3))
        );
        // Knights on file edges have a single destination.
        assert_eq!(
            knight_attacks(Color::Black, sq(1, 5)),
            Bitboard::single(sq(2, 3))
        );
        assert_eq!(
            knight_attacks(Color::White, sq(9, 5)),
            Bitboard::single(sq(8, 7))
        );
    }

    #[test]
    fn step_counts_center() {
        assert_eq!(silver_attacks(Color::Black, sq(5, 5)).count(), 5);
        assert_eq!(gold_attacks(Color::Black, sq(5, 5)).count(), 6);
        assert_eq!(king_attacks(sq(5, 5)).count(), 8);
        assert_eq!(king_attacks(sq(1, 1)).count(), 3);
    }

    #[test]
    fn color_symmetry() {
        for square in Square::all() {
            let flipped = square.flip();
            for (black, white) in [
                (
                    silver_attacks(Color::Black, square),
                    silver_attacks(Color::White, flipped),
                ),
                (
                    gold_attacks(Color::Black, square),
                    gold_attacks(Color::White, flipped),
                ),
            ] {
                let mirrored =
                    white.fold(Bitboard::EMPTY, |acc, s| acc | Bitboard::single(s.flip()));
                assert_eq!(black, mirrored);
            }
        }
    }

    #[test]
    fn sliders_empty_board() {
        assert_eq!(rook_attacks(sq(5, 5), Bitboard::EMPTY).count(), 16);
        assert_eq!(bishop_attacks(sq(5, 5), Bitboard::EMPTY).count(), 16);
        assert_eq!(bishop_attacks(sq(1, 1), Bitboard::EMPTY).count(), 8);
        assert_eq!(
            lance_attacks(Color::Black, sq(5, 9), Bitboard::EMPTY).count(),
            8
        );
        assert!(lance_attacks(Color::Black, sq(5, 1), Bitboard::EMPTY).is_empty());
    }

    #[test]
    fn sliders_stop_at_blockers() {
        // A blocker on 5c: the rook on 5g sees 5c but not beyond.
        let occupied = Bitboard::single(sq(5, 3));
        let attacks = rook_attacks(sq(5, 7), occupied);
        assert!(attacks.contains(sq(5, 3)));
        assert!(!attacks.contains(sq(5, 2)));
        assert!(attacks.contains(sq(5, 6)));
    }

    #[test]
    fn between_pairs() {
        assert_eq!(between(sq(1, 1), sq(9, 1)).count(), 7);
        assert_eq!(between(sq(1, 1), sq(9, 9)).count(), 7);
        assert!(between(sq(1, 1), sq(1, 2)).is_empty());
        // Not aligned.
        assert!(between(sq(1, 1), sq(5, 4)).is_empty());
        assert!(between(sq(2, 3), sq(3, 5)).is_empty());
        // Symmetry.
        for &(a, b) in &[(sq(2, 2), sq(8, 8)), (sq(3, 9), sq(3, 1))] {
            assert_eq!(between(a, b), between(b, a));
        }
    }

    #[test]
    fn promoted_slider_attacks() {
        let horse = attacks_of(
            Piece::new(PieceKind::ProBishop, Color::Black),
            sq(5, 5),
            Bitboard::EMPTY,
        );
        assert_eq!(horse.count(), 16 + 4);
        let dragon = attacks_of(
            Piece::new(PieceKind::ProRook, Color::White),
            sq(5, 5),
            Bitboard::EMPTY,
        );
        assert_eq!(dragon.count(), 16 + 4);
    }
}
