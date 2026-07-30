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

/// A position where the opponent's king is en prise cannot occur in legal
/// play, but it can be constructed. King captures must never be generated
/// (do_move would even panic trying to send the king to hand), and playing
/// the generated moves must not panic.
#[test]
fn king_capture_is_never_generated() {
    // Black rook on 5e attacks the white king on 5a along the open file.
    let sfen = "4k4/9/9/9/4R4/9/9/9/4K4 b - 1";
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    let position = Position::new(partial);
    let moves = position.legal_moves();
    assert!(!moves.is_empty());
    for mv in moves {
        assert_ne!(mv.to(), sq(5, 1), "king capture generated: {mv:?}");
        let mut next = position.clone();
        next.do_move(mv);
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

/// A pinned piece must not be allowed to capture the checker: doing so
/// would still expose the king to the piece pinning it.
#[test]
fn pinned_piece_cannot_capture_a_different_checker() {
    // Black king 5i pinned down file 5 by a white lance on 5a through the
    // black gold on 5e; a white knight on 4g gives check from the side.
    // G-4g would capture the checker but abandons the file.
    let mut partial = PartialPosition::empty();
    partial.piece_set(sq(5, 9), Some(Piece::new(PieceKind::King, Color::Black)));
    partial.piece_set(sq(5, 5), Some(Piece::new(PieceKind::Gold, Color::Black)));
    partial.piece_set(sq(5, 1), Some(Piece::new(PieceKind::Lance, Color::White)));
    partial.piece_set(sq(4, 7), Some(Piece::new(PieceKind::Knight, Color::White)));
    partial.piece_set(sq(1, 1), Some(Piece::new(PieceKind::King, Color::White)));
    let position = Position::new(partial);
    assert!(
        position.in_check(),
        "knight on 4g must check the king on 5i"
    );

    for mv in position.legal_moves() {
        if let Move::Normal { from, to, .. } = mv {
            assert!(
                !(from == sq(5, 5) && to == sq(4, 7)),
                "pinned gold captured the checker and exposed its king"
            );
        }
    }
}

/// The king must not step backwards along a checking ray: the square behind
/// it is still covered once the king stops blocking.
#[test]
fn king_cannot_retreat_along_a_checking_ray() {
    // White rook on 5a checks the black king on 5e down the open file.
    let mut partial = PartialPosition::empty();
    partial.piece_set(sq(5, 5), Some(Piece::new(PieceKind::King, Color::Black)));
    partial.piece_set(sq(5, 1), Some(Piece::new(PieceKind::Rook, Color::White)));
    partial.piece_set(sq(1, 9), Some(Piece::new(PieceKind::King, Color::White)));
    let position = Position::new(partial);
    assert!(position.in_check());

    let king_destinations = king_destinations(&position, sq(5, 5));
    assert!(
        !king_destinations.contains(&sq(5, 6)),
        "king retreated along the rook's file and stayed in check"
    );
    assert!(
        king_destinations.contains(&sq(4, 6)),
        "sideways escape must be legal"
    );
}

/// The king may not capture a piece that is defended — including when the
/// defender is a slider whose ray *stops on* the captured piece. Removing
/// the capture does not remove the attack on its square, which is why one
/// danger bitboard can decide every destination without rebuilding
/// occupancy per capture.
#[test]
fn king_cannot_capture_a_defended_attacker() {
    // White pawn on 5e checks the black king on 5f, and a white rook on 5a
    // defends the pawn down the otherwise empty file.
    let sfen = "4r3k/9/9/9/4p4/4K4/9/9/9 b - 1";
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    let position = Position::new(partial);
    assert!(position.in_check(), "the pawn on 5e must check the king");
    assert!(
        !king_destinations(&position, sq(5, 6)).contains(&sq(5, 5)),
        "king captured a pawn defended by the rook behind it"
    );
}

/// The mirror image: with nothing defending it, capturing the checking pawn
/// is legal. Guards against a danger bitboard that is merely conservative —
/// the piece being captured must not count as an attacker of its own square.
#[test]
fn king_may_capture_an_undefended_attacker() {
    let sfen = "8k/9/9/9/4p4/4K4/9/9/9 b - 1";
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    let position = Position::new(partial);
    assert!(position.in_check(), "the pawn on 5e must check the king");
    assert!(
        king_destinations(&position, sq(5, 6)).contains(&sq(5, 5)),
        "king was not allowed to capture the undefended checking pawn"
    );
}

/// The furthest a non-slider can stand and still cover a king escape
/// square: a knight two files and three ranks away, which is the exact
/// corner of the box `king_danger` uses to skip distant pieces. One rank or
/// file tighter and this escape would be generated.
#[test]
fn distant_knight_still_covers_a_king_escape() {
    // Black king on 5e; white knight on 3b attacks 4d and 2d, and 4d is
    // adjacent to the king.
    let sfen = "9/6n2/9/9/4K4/9/9/9/k8 b - 1";
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    let position = Position::new(partial);
    assert!(!position.in_check(), "the knight must not check the king");
    let destinations = king_destinations(&position, sq(5, 5));
    assert!(
        !destinations.contains(&sq(4, 4)),
        "king stepped onto 4d, which the knight on 3b covers"
    );
    // The mirror square is not covered, so the king really can move.
    assert!(
        destinations.contains(&sq(6, 4)),
        "6d is uncovered and must stay legal"
    );
}

/// A double check can only be answered by moving the king — capturing one
/// checker leaves the other. Both checkers here are *sliders*, which is the
/// case where the checker search has to accumulate: with one sniper the
/// distinction between collecting and overwriting is invisible.
#[test]
fn double_check_by_two_sliders_leaves_only_king_moves() {
    // Black king on 5e, checked down file 5 by a white rook on 5b and along
    // the diagonal by a white bishop on 3c. The black gold on 2d attacks 3c,
    // so capturing a checker looks available and must not be generated.
    let sfen = "8k/4r4/6b2/7G1/4K4/9/9/9/9 b - 1";
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    let position = Position::new(partial);
    assert!(position.in_check());

    let moves = position.legal_moves();
    for mv in &moves {
        assert_eq!(
            mv.from(),
            Some(sq(5, 5)),
            "non-king move generated under double check: {mv:?}"
        );
    }
    let mut destinations = king_destinations(&position, sq(5, 5));
    destinations.sort_by_key(|s| (s.file(), s.rank()));
    // 5d/5f stay on the rook's file and 4d/6f on the bishop's diagonal, both
    // of which still cover those squares once the king stops blocking.
    assert_eq!(
        destinations,
        vec![sq(4, 5), sq(4, 6), sq(6, 4), sq(6, 5)],
        "wrong escape set under double check"
    );
}

/// The squares the king on `from` may legally move to.
fn king_destinations(position: &Position, from: Square) -> Vec<Square> {
    position
        .legal_moves()
        .into_iter()
        .filter_map(|mv| match mv {
            Move::Normal {
                from: origin, to, ..
            } if origin == from => Some(to),
            _ => None,
        })
        .collect()
}
