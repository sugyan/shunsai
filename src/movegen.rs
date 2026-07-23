//! Fully legal move generation.
//!
//! Correctness-first strategy (M1): generate pseudo-legal moves, then filter
//! with an attack test on the adjusted occupancy. King safety, pins and
//! discovered checks are all handled by the single [`attackers_to`] test;
//! the test is skipped only when it provably cannot fail (not in check, not
//! a king move, and the moving piece is not on a slider line to the king).
//! Pawn-drop-mate (打ち歩詰め) is excluded by simulating the drop.

use shogi_core::{Color, Hand, Move, Piece, PieceKind, Square};

use crate::bitboard::Bitboard;
use crate::position::Position;
use crate::tables;

impl Position {
    /// All legal moves for the side to move, including pawn-drop-mate
    /// exclusion.
    ///
    /// The opponent's king square is never a destination: in illegal
    /// positions where that king could already be captured (unreachable
    /// through legal play, but constructible via [`Position::new`]), no
    /// king-capture move is generated — the rest of the result is
    /// unspecified there, but this function does not panic.
    ///
    /// A callback-style API is planned (M3); this allocating form will stay
    /// as a compatibility wrapper.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(128);
        generate_legal(self, &mut moves);
        moves
    }

    /// Whether the side to move is in check.
    pub fn in_check(&self) -> bool {
        let us = self.side_to_move();
        match self.king_square(us) {
            Some(king) => !attackers_to(
                self,
                king,
                self.occupied(),
                us.flip(),
                self.player_bb(us.flip()),
            )
            .is_empty(),
            None => false,
        }
    }
}

/// The pieces of `attackers_color`, restricted to `mask`, that attack
/// `square` given `occupied`.
///
/// Uses the reverse-lookup trick: a piece of color `c` on `p` attacks
/// `square` if and only if `p` is attacked from `square` by the same piece
/// kind of the opposite color.
fn attackers_to(
    position: &Position,
    square: Square,
    occupied: Bitboard,
    attackers_color: Color,
    mask: Bitboard,
) -> Bitboard {
    let us = attackers_color.flip();
    let golds = position.piece_kind_bb(PieceKind::Gold)
        | position.piece_kind_bb(PieceKind::ProPawn)
        | position.piece_kind_bb(PieceKind::ProLance)
        | position.piece_kind_bb(PieceKind::ProKnight)
        | position.piece_kind_bb(PieceKind::ProSilver);
    let horses = position.piece_kind_bb(PieceKind::ProBishop);
    let dragons = position.piece_kind_bb(PieceKind::ProRook);
    let bishops = position.piece_kind_bb(PieceKind::Bishop) | horses;
    let rooks = position.piece_kind_bb(PieceKind::Rook) | dragons;

    let mut attackers = tables::pawn_attacks(us, square) & position.piece_kind_bb(PieceKind::Pawn);
    attackers |= tables::knight_attacks(us, square) & position.piece_kind_bb(PieceKind::Knight);
    attackers |= tables::silver_attacks(us, square) & position.piece_kind_bb(PieceKind::Silver);
    attackers |= tables::gold_attacks(us, square) & golds;
    attackers |= tables::king_attacks(square) & position.piece_kind_bb(PieceKind::King);
    attackers |= tables::orthogonal_attacks(square) & horses;
    attackers |= tables::diagonal_attacks(square) & dragons;
    attackers |=
        tables::lance_attacks(us, square, occupied) & position.piece_kind_bb(PieceKind::Lance);
    attackers |= tables::bishop_attacks(square, occupied) & bishops;
    attackers |= tables::rook_attacks(square, occupied) & rooks;
    attackers & mask
}

/// Whether our king is safe after moving a piece from `from` to `to`
/// (any capture on `to` no longer counts as an attacker).
fn king_safe_after(
    position: &Position,
    us: Color,
    king: Square,
    from: Square,
    to: Square,
    occupied: Bitboard,
) -> bool {
    let king_after = if from == king { to } else { king };
    let occupied_after = (occupied ^ Bitboard::single(from)) | Bitboard::single(to);
    let enemy_mask = position.player_bb(us.flip()) & !Bitboard::single(to);
    attackers_to(position, king_after, occupied_after, us.flip(), enemy_mask).is_empty()
}

pub(crate) fn generate_legal(position: &Position, out: &mut Vec<Move>) {
    let us = position.side_to_move();
    let them = us.flip();
    let occupied = position.occupied();
    let our = position.player_bb(us);
    let king = position.king_square(us);
    let checkers = match king {
        Some(king) => attackers_to(position, king, occupied, them, position.player_bb(them)),
        None => Bitboard::EMPTY,
    };

    generate_normal(position, us, occupied, our, king, checkers, out);
    generate_drops(position, us, occupied, king, checkers, out);
}

fn generate_normal(
    position: &Position,
    us: Color,
    occupied: Bitboard,
    our: Bitboard,
    king: Option<Square>,
    checkers: Bitboard,
    out: &mut Vec<Move>,
) {
    // Never target our own pieces, nor the opponent's king: the latter is
    // only reachable in illegal positions, and "capturing" it would try to
    // send a king to hand in do_move.
    let targets = match position.king_square(us.flip()) {
        Some(enemy_king) => !(our | Bitboard::single(enemy_king)),
        None => !our,
    };
    let king = match king {
        Some(king) => king,
        None => {
            // No king to endanger: every pseudo-legal move is legal.
            for from in our {
                let piece = position.piece_at(from).unwrap();
                let attacks = tables::attacks_of(piece, from, occupied) & targets;
                for to in attacks {
                    push_normal(us, piece.piece_kind(), from, to, out);
                }
            }
            return;
        }
    };
    let in_check = !checkers.is_empty();
    // Only pieces standing on a slider line from the king can expose it by
    // moving away; everything else can skip the attack test when not in
    // check.
    let pin_candidates =
        (tables::rook_attacks(king, occupied) | tables::bishop_attacks(king, occupied)) & our;
    for from in our {
        let piece = position.piece_at(from).unwrap();
        let is_king = from == king;
        let needs_test = in_check || is_king || pin_candidates.contains(from);
        let attacks = tables::attacks_of(piece, from, occupied) & targets;
        for to in attacks {
            if needs_test && !king_safe_after(position, us, king, from, to, occupied) {
                continue;
            }
            push_normal(us, piece.piece_kind(), from, to, out);
        }
    }
}

/// Pushes the promoting and/or non-promoting variants of a normal move,
/// respecting forced promotions (pieces may never be left without a legal
/// move).
fn push_normal(us: Color, piece_kind: PieceKind, from: Square, to: Square, out: &mut Vec<Move>) {
    let zone = 3;
    let can_promote = matches!(
        piece_kind,
        PieceKind::Pawn
            | PieceKind::Lance
            | PieceKind::Knight
            | PieceKind::Silver
            | PieceKind::Bishop
            | PieceKind::Rook
    ) && (to.relative_rank(us) <= zone || from.relative_rank(us) <= zone);
    if can_promote {
        out.push(Move::Normal {
            from,
            to,
            promote: true,
        });
    }
    let must_promote = match piece_kind {
        PieceKind::Pawn | PieceKind::Lance => to.relative_rank(us) == 1,
        PieceKind::Knight => to.relative_rank(us) <= 2,
        _ => false,
    };
    if !must_promote {
        out.push(Move::Normal {
            from,
            to,
            promote: false,
        });
    }
}

fn generate_drops(
    position: &Position,
    us: Color,
    occupied: Bitboard,
    king: Option<Square>,
    checkers: Bitboard,
    out: &mut Vec<Move>,
) {
    let hand = position.hand(us);
    if hand == Hand::new() {
        return;
    }
    let targets = match checkers.count() {
        0 => !occupied,
        // A drop can only resolve a single check by blocking it. Blocking
        // never exposes the king, so no further test is needed.
        1 => {
            let king = king.expect("in check without a king");
            let mut checkers = checkers;
            let checker = checkers.next().unwrap();
            tables::between(king, checker) & !occupied
        }
        _ => return,
    };
    if targets.is_empty() {
        return;
    }
    // Files that already contain one of our unpromoted pawns (nifu).
    let our_pawns = position.piece_kind_bb(PieceKind::Pawn) & position.player_bb(us);
    let mut pawn_files = Bitboard::EMPTY;
    for file in 1..=9 {
        let mask = Bitboard::file(file);
        if !(our_pawns & mask).is_empty() {
            pawn_files |= mask;
        }
    }
    let enemy_king = position.king_square(us.flip());
    for piece_kind in Hand::all_hand_pieces() {
        if hand.count(piece_kind).unwrap_or(0) == 0 {
            continue;
        }
        let piece = Piece::new(piece_kind, us);
        let mut targets = targets;
        if piece_kind == PieceKind::Pawn {
            targets &= !pawn_files;
        }
        // Ranks where the piece would never be able to move again.
        let min_rank = match piece_kind {
            PieceKind::Pawn | PieceKind::Lance => 2,
            PieceKind::Knight => 3,
            _ => 1,
        };
        for to in targets {
            if to.relative_rank(us) < min_rank {
                continue;
            }
            if piece_kind == PieceKind::Pawn
                && enemy_king.is_some_and(|king| tables::pawn_attacks(us, to).contains(king))
                && is_pawn_drop_mate(position, piece, to)
            {
                continue;
            }
            out.push(Move::Drop { piece, to });
        }
    }
}

/// Whether dropping `piece` (a checking pawn) on `to` leaves the opponent
/// with no legal moves. The pawn checks from an adjacent square, so the
/// opponent cannot block and no recursive pawn-drop arises.
fn is_pawn_drop_mate(position: &Position, piece: Piece, to: Square) -> bool {
    let mut next = position.clone();
    next.do_move(Move::Drop { piece, to });
    next.legal_moves().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shogi_core::PartialPosition;

    #[test]
    fn startpos_has_30_moves() {
        let position = Position::startpos();
        assert_eq!(position.legal_moves().len(), 30);
        assert!(!position.in_check());
    }

    #[test]
    fn evasions_only_when_in_check() {
        // 5i king in check from a dropped white rook on 5e.
        let mut partial = PartialPosition::empty();
        partial.piece_set(
            Square::new(5, 9).unwrap(),
            Some(Piece::new(PieceKind::King, Color::Black)),
        );
        partial.piece_set(
            Square::new(5, 5).unwrap(),
            Some(Piece::new(PieceKind::Rook, Color::White)),
        );
        partial.piece_set(
            Square::new(5, 1).unwrap(),
            Some(Piece::new(PieceKind::King, Color::White)),
        );
        let position = Position::new(partial);
        assert!(position.in_check());
        for mv in position.legal_moves() {
            let mut next = position.clone();
            next.do_move(mv);
            // After any evasion, the black king must no longer be attacked.
            assert!(!in_check_of(&next, Color::Black));
        }
    }

    fn in_check_of(position: &Position, color: Color) -> bool {
        match position.king_square(color) {
            Some(king) => !attackers_to(
                position,
                king,
                position.occupied(),
                color.flip(),
                position.player_bb(color.flip()),
            )
            .is_empty(),
            None => false,
        }
    }
}
