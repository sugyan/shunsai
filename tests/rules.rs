//! Focused tests for the trickiest legality rules.

use shogi_core::{Color, Move, PartialPosition, Piece, PieceKind, Square};
use shogi_usi_parser::FromUsi;
use shunsai::Position;

fn sq(file: u8, rank: u8) -> Square {
    Square::new(file, rank).unwrap()
}

fn pawn_drop(to: Square) -> Move {
    Move::Drop {
        piece: Piece::new(PieceKind::Pawn, Color::Black),
        to,
    }
}

/// White king on 1a, black gold on 3b covering 2a/2b, black lance on 1e
/// defending 1b: P*1b would be checkmate, so it must not be generated.
#[test]
fn pawn_drop_mate_is_excluded() {
    let mut partial = PartialPosition::empty();
    partial.piece_set(sq(1, 1), Some(Piece::new(PieceKind::King, Color::White)));
    partial.piece_set(sq(3, 2), Some(Piece::new(PieceKind::Gold, Color::Black)));
    partial.piece_set(sq(1, 5), Some(Piece::new(PieceKind::Lance, Color::Black)));
    let hand = partial.hand_of_a_player_mut(Color::Black);
    *hand = hand.added(PieceKind::Pawn).unwrap();
    let position = Position::new(partial);

    let moves = position.legal_moves();
    assert!(
        !moves.contains(&pawn_drop(sq(1, 2))),
        "uchifuzume generated"
    );
    // Non-mating pawn drops on the same file are fine.
    assert!(moves.contains(&pawn_drop(sq(1, 3))));
}

/// Same position without the lance: the king can just capture the pawn,
/// so P*1b is a legal (non-mating) check.
#[test]
fn pawn_drop_check_without_mate_is_legal() {
    let mut partial = PartialPosition::empty();
    partial.piece_set(sq(1, 1), Some(Piece::new(PieceKind::King, Color::White)));
    partial.piece_set(sq(3, 2), Some(Piece::new(PieceKind::Gold, Color::Black)));
    let hand = partial.hand_of_a_player_mut(Color::Black);
    *hand = hand.added(PieceKind::Pawn).unwrap();
    let position = Position::new(partial);

    assert!(position.legal_moves().contains(&pawn_drop(sq(1, 2))));
}

/// A pawn already on the file forbids dropping another (nifu), but a
/// promoted pawn does not count.
#[test]
fn nifu_is_excluded() {
    let mut partial = PartialPosition::empty();
    partial.piece_set(sq(1, 1), Some(Piece::new(PieceKind::King, Color::White)));
    partial.piece_set(sq(5, 5), Some(Piece::new(PieceKind::Pawn, Color::Black)));
    partial.piece_set(sq(4, 5), Some(Piece::new(PieceKind::ProPawn, Color::Black)));
    // A white pawn does not forbid black drops either.
    partial.piece_set(sq(3, 5), Some(Piece::new(PieceKind::Pawn, Color::White)));
    let hand = partial.hand_of_a_player_mut(Color::Black);
    *hand = hand.added(PieceKind::Pawn).unwrap();
    let position = Position::new(partial);

    let moves = position.legal_moves();
    assert!(
        !moves
            .iter()
            .any(|m| matches!(m, Move::Drop { to, .. } if to.file() == 5))
    );
    assert!(moves.contains(&pawn_drop(sq(4, 4))));
    assert!(moves.contains(&pawn_drop(sq(3, 4))));
}

/// Pawns, lances and knights may not be dropped (or left unpromoted) where
/// they could never move again.
#[test]
fn dead_piece_squares_are_excluded() {
    let sfen = "9/9/9/9/9/9/9/9/K7k b PLN 1";
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    let position = Position::new(partial);
    for mv in position.legal_moves() {
        if let Move::Drop { piece, to } = mv {
            let min_rank = match piece.piece_kind() {
                PieceKind::Pawn | PieceKind::Lance => 2,
                PieceKind::Knight => 3,
                _ => 1,
            };
            assert!(to.rank() >= min_rank, "dead drop generated: {mv:?}");
        }
    }
}

/// A pinned piece may only move along the pin line.
#[test]
fn pinned_piece_moves_are_restricted() {
    // Black: K5i, G5e; White: R5a, K1a. The gold is pinned on the file.
    let sfen = "4r3k/9/9/9/4G4/9/9/9/4K4 b - 1";
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    let position = Position::new(partial);
    let gold_moves: Vec<_> = position
        .legal_moves()
        .into_iter()
        .filter(|m| m.from() == Some(sq(5, 5)))
        .collect();
    // Only straight ahead/behind on file 5.
    assert!(!gold_moves.is_empty());
    for mv in gold_moves {
        assert_eq!(mv.to().file(), 5, "pinned gold left the pin line: {mv:?}");
    }
}
