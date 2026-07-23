//! Attack tables and naive slider attacks.
//!
//! Step-piece attacks are precomputed per square (initialized lazily at first
//! use); slider attacks walk rays square by square. Replacing these with
//! const/Qugiy/magic implementations is a later, benchmark-driven decision.

use std::sync::LazyLock;

use shogi_core::{Color, Piece, PieceKind, Square};

use crate::bitboard::Bitboard;

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

fn step_table(steps: &[(i8, i8)]) -> [Bitboard; 81] {
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

/// Builds `[Black's table, White's table]`; White's steps are the reflection
/// of Black's.
fn step_table_both(steps: &[(i8, i8)]) -> [[Bitboard; 81]; 2] {
    let flipped: Vec<(i8, i8)> = steps
        .iter()
        .map(|&(file_delta, rank_delta)| (-file_delta, -rank_delta))
        .collect();
    [step_table(steps), step_table(&flipped)]
}

static PAWN_ATTACKS: LazyLock<[[Bitboard; 81]; 2]> = LazyLock::new(|| step_table_both(&PAWN_STEPS));
static KNIGHT_ATTACKS: LazyLock<[[Bitboard; 81]; 2]> =
    LazyLock::new(|| step_table_both(&KNIGHT_STEPS));
static SILVER_ATTACKS: LazyLock<[[Bitboard; 81]; 2]> =
    LazyLock::new(|| step_table_both(&SILVER_STEPS));
static GOLD_ATTACKS: LazyLock<[[Bitboard; 81]; 2]> = LazyLock::new(|| step_table_both(&GOLD_STEPS));
static KING_ATTACKS: LazyLock<[Bitboard; 81]> = LazyLock::new(|| step_table(&KING_STEPS));
static ORTHOGONAL_ATTACKS: LazyLock<[Bitboard; 81]> =
    LazyLock::new(|| step_table(&ORTHOGONAL_STEPS));
static DIAGONAL_ATTACKS: LazyLock<[Bitboard; 81]> = LazyLock::new(|| step_table(&DIAGONAL_STEPS));

/// `BETWEEN[a][b]`: the squares strictly between `a` and `b` if they share a
/// rank, file or diagonal; empty otherwise.
static BETWEEN: LazyLock<Box<[[Bitboard; 81]; 81]>> = LazyLock::new(|| {
    let mut table = Box::new([[Bitboard::EMPTY; 81]; 81]);
    for from in Square::all() {
        for (file_delta, rank_delta) in KING_STEPS {
            let mut between = Bitboard::EMPTY;
            let mut current = from;
            while let Some(next) = current.shift(file_delta, rank_delta) {
                table[from.array_index()][next.array_index()] = between;
                between |= Bitboard::single(next);
                current = next;
            }
        }
    }
    table
});

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

/// The four orthogonally adjacent squares (the step part of a promoted rook's
/// diagonal complement is [`diagonal_attacks`]; this one belongs to the
/// promoted bishop).
pub(crate) fn orthogonal_attacks(square: Square) -> Bitboard {
    ORTHOGONAL_ATTACKS[square.array_index()]
}

/// The four diagonally adjacent squares.
pub(crate) fn diagonal_attacks(square: Square) -> Bitboard {
    DIAGONAL_ATTACKS[square.array_index()]
}

/// Squares strictly between `a` and `b` (empty when not aligned).
pub(crate) fn between(a: Square, b: Square) -> Bitboard {
    BETWEEN[a.array_index()][b.array_index()]
}

/// Walks each ray one square at a time, stopping at (and including) the first
/// occupied square.
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

pub(crate) fn lance_attacks(color: Color, square: Square, occupied: Bitboard) -> Bitboard {
    let rank_delta = if color == Color::Black { -1 } else { 1 };
    ray_attacks(square, occupied, &[(0, rank_delta)])
}

pub(crate) fn bishop_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    ray_attacks(square, occupied, &DIAGONAL_STEPS)
}

pub(crate) fn rook_attacks(square: Square, occupied: Bitboard) -> Bitboard {
    ray_attacks(square, occupied, &ORTHOGONAL_STEPS)
}

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
