//! Fully legal move generation.
//!
//! The primitive is [`Position::generate_moves`], which hands the caller one
//! [`MoveSet`] per origin — destinations as bitboards — and can be stopped
//! early. [`Position::legal_moves`] is the allocating wrapper over it.
//!
//! Legality strategy (still M1): generate pseudo-legal moves, then filter
//! with an attack test on the adjusted occupancy. King safety, pins and
//! discovered checks are all handled by the single [`attackers_to`] test;
//! the test is skipped only when it provably cannot fail (not in check, not
//! a king move, and the moving piece is not on a slider line to the king).
//! Pawn-drop-mate (打ち歩詰め) is excluded by simulating the drop and asking
//! whether the opponent has any reply at all.

use shogi_core::{Color, Hand, Move, Piece, PieceKind, Square};

use crate::bitboard::Bitboard;
use crate::position::Position;
use crate::tables;

/// A group of legal moves that share an origin, as handed to
/// [`Position::generate_moves`].
///
/// Destinations arrive as bitboards rather than one [`Move`] at a time, so
/// a caller that only needs to count them (perft, mobility) never has to
/// build a `Move` at all. A `MoveSet` is never empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveSet {
    /// Board moves of `piece` standing on `from`.
    ///
    /// The two destination sets overlap wherever promotion is optional; a
    /// square in `promotions` alone is a compulsory promotion, and one in
    /// `non_promotions` alone cannot promote at all.
    Normal {
        piece: Piece,
        from: Square,
        promotions: Bitboard,
        non_promotions: Bitboard,
    },
    /// Drops of `piece` from hand.
    Drop { piece: Piece, to: Bitboard },
}

impl MoveSet {
    /// How many [`Move`]s this set expands to.
    #[inline(always)]
    pub fn len(&self) -> usize {
        match *self {
            MoveSet::Normal {
                promotions,
                non_promotions,
                ..
            } => (promotions.count() + non_promotions.count()) as usize,
            MoveSet::Drop { to, .. } => to.count() as usize,
        }
    }

    /// Always `false`: generation never yields an empty set.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Expands a [`MoveSet`] into individual [`Move`]s.
#[derive(Clone, Debug)]
pub struct MoveSetIter {
    piece: Piece,
    /// `None` for drops.
    from: Option<Square>,
    promotions: Bitboard,
    others: Bitboard,
}

impl Iterator for MoveSetIter {
    type Item = Move;

    #[inline]
    fn next(&mut self) -> Option<Move> {
        match self.from {
            Some(from) => {
                if let Some(to) = self.promotions.pop() {
                    return Some(Move::Normal {
                        from,
                        to,
                        promote: true,
                    });
                }
                self.others.pop().map(|to| Move::Normal {
                    from,
                    to,
                    promote: false,
                })
            }
            None => self.others.pop().map(|to| Move::Drop {
                piece: self.piece,
                to,
            }),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = (self.promotions.count() + self.others.count()) as usize;
        (count, Some(count))
    }
}

impl ExactSizeIterator for MoveSetIter {}

impl IntoIterator for MoveSet {
    type Item = Move;
    type IntoIter = MoveSetIter;

    fn into_iter(self) -> MoveSetIter {
        match self {
            MoveSet::Normal {
                piece,
                from,
                promotions,
                non_promotions,
            } => MoveSetIter {
                piece,
                from: Some(from),
                promotions,
                others: non_promotions,
            },
            MoveSet::Drop { piece, to } => MoveSetIter {
                piece,
                from: None,
                promotions: Bitboard::EMPTY,
                others: to,
            },
        }
    }
}

impl Position {
    /// Calls `listener` with every group of legal moves for the side to
    /// move, including pawn-drop-mate exclusion.
    ///
    /// Returning `true` from `listener` stops generation early — useful to
    /// answer "is there any legal move?" without building the rest. No
    /// empty [`MoveSet`] is ever passed.
    ///
    /// The opponent's king square is never a destination: in illegal
    /// positions where that king could already be captured (unreachable
    /// through legal play, but constructible via [`Position::new`]), no
    /// king-capture move is generated — the rest of the result is
    /// unspecified there, but this function does not panic.
    pub fn generate_moves(&self, listener: impl FnMut(MoveSet) -> bool) {
        generate_legal(self, listener);
    }

    /// All legal moves for the side to move.
    ///
    /// The compatibility wrapper over [`Position::generate_moves`]; prefer
    /// the callback form in hot code, which allocates nothing.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(128);
        self.generate_moves(|set| {
            moves.extend(set);
            false
        });
        moves
    }

    /// Whether the side to move has at least one legal move.
    ///
    /// Stops at the first group found, so it is far cheaper than
    /// `!legal_moves().is_empty()`.
    pub fn has_legal_moves(&self) -> bool {
        let mut any = false;
        self.generate_moves(|_| {
            any = true;
            true
        });
        any
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
pub(crate) fn attackers_to(
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

pub(crate) fn generate_legal(position: &Position, mut listener: impl FnMut(MoveSet) -> bool) {
    let us = position.side_to_move();
    let them = us.flip();
    let occupied = position.occupied();
    let our = position.player_bb(us);
    let king = position.king_square(us);
    let checkers = match king {
        Some(king) => attackers_to(position, king, occupied, them, position.player_bb(them)),
        None => Bitboard::EMPTY,
    };

    if generate_normal(position, us, occupied, our, king, checkers, &mut listener) {
        return;
    }
    generate_drops(position, us, occupied, king, checkers, &mut listener);
}

/// Splits the destinations of one piece into its promoting and
/// non-promoting sets and hands them to `listener`. Returns whether the
/// listener asked to stop.
#[inline]
fn emit_normal(
    us: Color,
    piece: Piece,
    from: Square,
    attacks: Bitboard,
    listener: &mut impl FnMut(MoveSet) -> bool,
) -> bool {
    if attacks.is_empty() {
        return false;
    }
    let piece_kind = piece.piece_kind();
    let promotions = if piece_kind.promote().is_some() {
        // Promotion is allowed when the move starts or ends in the zone,
        // so leaving it covers every destination at once.
        let zone = tables::promotion_zone(us);
        if zone.contains(from) {
            attacks
        } else {
            attacks & zone
        }
    } else {
        Bitboard::EMPTY
    };
    // A piece that would have no move left must promote.
    let non_promotions = attacks & !tables::forced_promotion_zone(us, piece_kind);
    listener(MoveSet::Normal {
        piece,
        from,
        promotions,
        non_promotions,
    })
}

fn generate_normal(
    position: &Position,
    us: Color,
    occupied: Bitboard,
    our: Bitboard,
    king: Option<Square>,
    checkers: Bitboard,
    listener: &mut impl FnMut(MoveSet) -> bool,
) -> bool {
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
                if emit_normal(us, piece, from, attacks, listener) {
                    return true;
                }
            }
            return false;
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
        let mut attacks = tables::attacks_of(piece, from, occupied) & targets;
        if needs_test {
            let mut safe = Bitboard::EMPTY;
            for to in attacks {
                if king_safe_after(position, us, king, from, to, occupied) {
                    safe |= Bitboard::single(to);
                }
            }
            attacks = safe;
        }
        if emit_normal(us, piece, from, attacks, listener) {
            return true;
        }
    }
    false
}

fn generate_drops(
    position: &Position,
    us: Color,
    occupied: Bitboard,
    king: Option<Square>,
    checkers: Bitboard,
    listener: &mut impl FnMut(MoveSet) -> bool,
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
        let mut drops = Bitboard::EMPTY;
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
            drops |= Bitboard::single(to);
        }
        if !drops.is_empty() && listener(MoveSet::Drop { piece, to: drops }) {
            return;
        }
    }
}

/// Whether dropping `piece` (a checking pawn) on `to` leaves the opponent
/// with no legal moves. The pawn checks from an adjacent square, so the
/// opponent cannot block and no recursive pawn-drop arises.
fn is_pawn_drop_mate(position: &Position, piece: Piece, to: Square) -> bool {
    let mut next = position.clone();
    next.do_move(Move::Drop { piece, to });
    !next.has_legal_moves()
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

    const CALLBACK_POSITIONS: &[&str] = &[
        "startpos",
        // Matsuri midgame position: heavy hands, many drops.
        "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1",
        // Max legal moves.
        "R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1",
        // In check, so the evasion path is covered too.
        "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1",
    ];

    fn position_of(sfen: &str) -> Position {
        if sfen == "startpos" {
            return Position::startpos();
        }
        let partial =
            <PartialPosition as shogi_usi_parser::FromUsi>::from_usi(&format!("sfen {sfen}"))
                .expect("test SFEN must parse");
        Position::new(partial)
    }

    /// The set semantics the callback API promises: sets expand to exactly
    /// `legal_moves()`, `len()` matches the expansion, and none is empty.
    #[test]
    fn move_sets_expand_to_legal_moves() {
        for sfen in CALLBACK_POSITIONS {
            let position = position_of(sfen);
            let mut expanded = Vec::new();
            let mut counted = 0;
            position.generate_moves(|set| {
                assert!(!set.is_empty(), "empty MoveSet in {sfen}");
                assert_eq!(set.len(), set.into_iter().count(), "len mismatch in {sfen}");
                counted += set.len();
                expanded.extend(set);
                false
            });
            let mut expected = position.legal_moves();
            assert_eq!(counted, expected.len(), "count mismatch in {sfen}");
            expanded.sort_by_key(|&mv| move_key(mv));
            expected.sort_by_key(|&mv| move_key(mv));
            assert_eq!(expanded, expected, "move mismatch in {sfen}");
        }
    }

    /// `promotions` / `non_promotions` must encode optional, compulsory and
    /// impossible promotion correctly, which `Position::do_move` will only
    /// accept if the piece really can promote.
    #[test]
    fn promotion_sets_are_consistent() {
        for sfen in CALLBACK_POSITIONS {
            let position = position_of(sfen);
            position.generate_moves(|set| {
                if let MoveSet::Normal {
                    piece,
                    promotions,
                    non_promotions,
                    ..
                } = set
                {
                    if piece.piece_kind().promote().is_none() {
                        assert!(promotions.is_empty(), "unpromotable piece with promotions");
                    }
                    // A compulsory promotion is one that never appears as a
                    // non-promoting move.
                    for to in promotions & !non_promotions {
                        assert!(
                            tables::forced_promotion_zone(piece.color(), piece.piece_kind())
                                .contains(to)
                        );
                    }
                }
                false
            });
        }
    }

    #[test]
    fn early_exit_stops_generation() {
        for sfen in CALLBACK_POSITIONS {
            let position = position_of(sfen);
            let mut seen = 0;
            position.generate_moves(|_| {
                seen += 1;
                true
            });
            assert_eq!(seen, 1, "generation did not stop after the first set");
            assert_eq!(
                position.has_legal_moves(),
                !position.legal_moves().is_empty()
            );
        }
    }

    /// A checkmated side has no legal moves and no move sets at all.
    #[test]
    fn has_legal_moves_is_false_when_mated() {
        // Black king on 5i, mated by a gold on 5h backed by a rook on 5a.
        // The gold covers every escape square, and taking it is illegal
        // because the rook defends it down an otherwise empty file — so the
        // white king has to stand well clear of file 5.
        let mut partial = PartialPosition::empty();
        for (file, rank, piece_kind, color) in [
            (5, 9, PieceKind::King, Color::Black),
            (5, 8, PieceKind::Gold, Color::White),
            (5, 1, PieceKind::Rook, Color::White),
            (1, 1, PieceKind::King, Color::White),
        ] {
            partial.piece_set(
                Square::new(file, rank).unwrap(),
                Some(Piece::new(piece_kind, color)),
            );
        }
        let position = Position::new(partial);
        assert!(position.in_check());
        assert!(position.legal_moves().is_empty());
        assert!(!position.has_legal_moves());
    }

    fn move_key(mv: Move) -> (u8, u8, u8, u8) {
        match mv {
            Move::Normal { from, to, promote } => (0, from.index(), to.index(), promote as u8),
            Move::Drop { piece, to } => (1, piece.as_u8(), to.index(), 0),
        }
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
