//! Fully legal move generation.
//!
//! The primitive is [`Position::generate_moves`], which hands the caller one
//! [`MoveSet`] per origin — destinations as bitboards — and can be stopped
//! early. [`Position::legal_moves`] is the allocating wrapper over it.
//!
//! Legality is decided per position rather than per move. Once per node the
//! generator finds the [`checkers`](attackers_to) and the [`pinned_pieces`],
//! and those two bitboards make every non-king move legal by construction:
//!
//! - a piece that is not pinned cannot expose its own king by moving, since
//!   a pin is precisely the situation where it could;
//! - a pinned piece is masked to the line it is pinned on, which still lets
//!   it capture the pinner;
//! - under a single check, every non-king move is masked to capturing the
//!   checker or interposing, and a double check leaves only king moves.
//!
//! The king is what the test is about, so it is the one piece left, and it
//! is decided by a single bitboard of every square the opponent attacks —
//! see [`king_danger`] — rather than one attack test per destination.
//!
//! Pawn-drop-mate (打ち歩詰め) is excluded by simulating the drop and asking
//! whether the opponent has any reply at all.

use core::ops::ControlFlow;

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
    /// Returning [`ControlFlow::Break`] from `listener` stops generation
    /// early — useful to answer "is there any legal move?" without building
    /// the rest — and is reported back as the return value, so the caller
    /// can tell a full walk from an interrupted one. No empty [`MoveSet`]
    /// is ever passed.
    ///
    /// The opponent's king square is never a destination: in illegal
    /// positions where that king could already be captured (unreachable
    /// through legal play, but constructible via [`Position::new`]), no
    /// king-capture move is generated — the rest of the result is
    /// unspecified there, but this function does not panic.
    pub fn generate_moves(
        &self,
        listener: impl FnMut(MoveSet) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        generate_legal(self, listener)
    }

    /// All legal moves for the side to move.
    ///
    /// The compatibility wrapper over [`Position::generate_moves`]; prefer
    /// the callback form in hot code, which allocates nothing.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(128);
        // The listener never breaks, so the walk is always `Continue`.
        let _ = self.generate_moves(|set| {
            // A plain push loop rather than `extend`: `MoveSetIter` is not
            // `TrustedLen`, so `extend` re-checks the length bound per
            // element and measurably loses on drop-heavy positions.
            for mv in set {
                moves.push(mv);
            }
            ControlFlow::Continue(())
        });
        moves
    }

    /// Whether the side to move has at least one legal move.
    ///
    /// Stops at the first group found, so it is far cheaper than
    /// `!legal_moves().is_empty()`.
    pub fn has_legal_moves(&self) -> bool {
        self.generate_moves(|_| ControlFlow::Break(())).is_break()
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

pub(crate) fn generate_legal(
    position: &Position,
    mut listener: impl FnMut(MoveSet) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let us = position.side_to_move();
    let them = us.flip();
    let occupied = position.occupied();
    let our = position.player_bb(us);
    let king = position.king_square(us);
    let checkers = match king {
        Some(king) => attackers_to(position, king, occupied, them, position.player_bb(them)),
        None => Bitboard::EMPTY,
    };

    generate_normal(position, us, occupied, our, king, checkers, &mut listener)?;
    generate_drops(position, us, occupied, king, checkers, &mut listener)
}

/// Splits the destinations of one piece into its promoting and
/// non-promoting sets and hands them to `listener`.
#[inline]
fn emit_normal(
    us: Color,
    piece: Piece,
    from: Square,
    attacks: Bitboard,
    listener: &mut impl FnMut(MoveSet) -> ControlFlow<()>,
) -> ControlFlow<()> {
    if attacks.is_empty() {
        return ControlFlow::Continue(());
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
    listener: &mut impl FnMut(MoveSet) -> ControlFlow<()>,
) -> ControlFlow<()> {
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
                emit_normal(us, piece, from, attacks, listener)?;
            }
            return ControlFlow::Continue(());
        }
    };
    // A double check can only be answered by moving the king.
    if checkers.count() < 2 {
        // Under a single check every other piece must capture the checker
        // or interpose; otherwise anything goes.
        let check_mask = match checkers.into_iter().next() {
            Some(checker) => tables::between(king, checker) | Bitboard::single(checker),
            None => Bitboard::ALL,
        };
        let pinned = pinned_pieces(position, us, king, occupied);
        for from in our & !Bitboard::single(king) {
            let piece = position.piece_at(from).unwrap();
            let mut attacks = tables::attacks_of(piece, from, occupied) & targets & check_mask;
            // A pinned piece may only travel along the line it is pinned
            // on, which includes capturing the pinner.
            if pinned.contains(from) {
                attacks &= tables::line(king, from);
            }
            emit_normal(us, piece, from, attacks, listener)?;
        }
    }
    generate_king_moves(position, us, king, occupied, targets, listener)
}

/// The king is the one piece whose moves are not legal by construction, and
/// [`king_danger`] decides all of them at once.
fn generate_king_moves(
    position: &Position,
    us: Color,
    king: Square,
    occupied: Bitboard,
    targets: Bitboard,
    listener: &mut impl FnMut(MoveSet) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let candidates = tables::king_attacks(king) & targets;
    // A king boxed in by its own pieces is common enough to be worth the
    // test, and the danger bitboard is built here rather than in
    // `generate_legal` for the same reason: a walk that stops early — as
    // `has_legal_moves` does, and with it the pawn-drop-mate test — usually
    // finds its move in `generate_normal` and never pays for this at all.
    if candidates.is_empty() {
        return ControlFlow::Continue(());
    }
    let piece = position.piece_at(king).expect("king square holds a king");
    let danger = king_danger(position, us, king, occupied);
    emit_normal(us, piece, king, candidates & !danger, listener)
}

/// The squares the opponent attacks *around our king*, with our king lifted
/// out of `occupied`.
///
/// One such bitboard decides every king destination at once, and is exactly
/// equivalent to testing each destination on its own post-move occupancy,
/// which is what generation used to do (up to eight `attackers_to` calls per
/// node against this one pass over the opponent's pieces). Three properties
/// make the destinations collapse into a single mask:
///
/// - lifting the king out of `occupied` is *required*, or the king could
///   retreat along a checking ray while still blocking it with the body it
///   is trying to save — but it does not depend on the destination, so it
///   can be done once;
/// - adding the destination to `occupied` cannot change whether that square
///   is attacked, because a piece standing on a square only shortens rays
///   *beyond* it, and what reaches the square depends on the pieces
///   *between*;
/// - removing a captured piece cannot change it either: no shogi piece
///   attacks the square it stands on, so the piece being captured was never
///   among that square's attackers. Its own attacks change, but those are
///   about other squares.
///
/// **The result is only valid on the king's own neighbours**, which is all
/// [`generate_king_moves`] masks with. The whole set would have to come from
/// every enemy piece, and one pass over all ~20 of them is a *fixed* cost
/// where the test it replaces was paid per candidate square — at the initial
/// position the king has three, and paying for twenty measured +15 % on
/// `perft/startpos-cb/4`. Since only attacks landing next to the king can
/// ever survive the mask, a piece that moves a fixed number of steps can be
/// skipped outright unless it stands in [`tables::step_attacker_zone`];
/// sliders reach from anywhere and are always included.
///
/// A search wanting the opponent's *full* attack map — DESIGN.md's
/// 2026-07-29 entry names this function as where that would come from —
/// wants this filter dropped, which is a one-line change and a different
/// measurement. It is not dropped speculatively.
fn king_danger(position: &Position, us: Color, king: Square, occupied: Bitboard) -> Bitboard {
    let occupied = occupied ^ Bitboard::single(king);
    let sliders = position.piece_kind_bb(PieceKind::Lance)
        | position.piece_kind_bb(PieceKind::Bishop)
        | position.piece_kind_bb(PieceKind::Rook)
        | position.piece_kind_bb(PieceKind::ProBishop)
        | position.piece_kind_bb(PieceKind::ProRook);
    let relevant = position.player_bb(us.flip()) & (tables::step_attacker_zone(king) | sliders);
    let mut danger = Bitboard::EMPTY;
    // One dense pass with a mailbox lookup, in the shape of
    // `generate_normal`'s main loop. Walking the 13 piece-kind bitboards
    // instead was measured and rejected (DESIGN.md decision log).
    for square in relevant {
        let piece = position
            .piece_at(square)
            .expect("player_bb agrees with the mailbox");
        danger |= tables::attacks_of(piece, square, occupied);
    }
    danger
}

/// Our pieces that stand alone between the king and an enemy slider, so
/// moving them off that line would expose the king.
///
/// Snipers are found by asking which enemy sliders would reach the king on
/// an *empty* board, then counting the blockers in between. A dragon's or
/// horse's one-step sidesteps can never pin — nothing fits between them and
/// the king — so only the true slider lines are searched.
fn pinned_pieces(position: &Position, us: Color, king: Square, occupied: Bitboard) -> Bitboard {
    let their = position.player_bb(us.flip());
    let rooks = (position.piece_kind_bb(PieceKind::Rook)
        | position.piece_kind_bb(PieceKind::ProRook))
        & their;
    let bishops = (position.piece_kind_bb(PieceKind::Bishop)
        | position.piece_kind_bb(PieceKind::ProBishop))
        & their;
    let lances = position.piece_kind_bb(PieceKind::Lance) & their;

    let empty = Bitboard::EMPTY;
    let snipers = (tables::rook_attacks(king, empty) & rooks)
        | (tables::bishop_attacks(king, empty) & bishops)
        // A lance only attacks forwards, so the squares it could pin from
        // are the ones our own lance would attack from the king.
        | (tables::lance_attacks(us, king, empty) & lances);

    let mut pinned = Bitboard::EMPTY;
    for sniper in snipers {
        let blockers = tables::between(king, sniper) & occupied;
        if blockers.count() == 1 {
            pinned |= blockers;
        }
    }
    // Only our own pieces are pinned; one of theirs in the way is their
    // discovered-check opportunity, not our problem.
    pinned & position.player_bb(us)
}

fn generate_drops(
    position: &Position,
    us: Color,
    occupied: Bitboard,
    king: Option<Square>,
    checkers: Bitboard,
    listener: &mut impl FnMut(MoveSet) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let hand = position.hand(us);
    if hand == Hand::new() {
        return ControlFlow::Continue(());
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
        _ => return ControlFlow::Continue(()),
    };
    if targets.is_empty() {
        return ControlFlow::Continue(());
    }
    let has_pawn = hand.count(PieceKind::Pawn).unwrap_or(0) > 0;
    // Files that already contain one of our unpromoted pawns (nifu).
    let pawn_files = if has_pawn {
        let our_pawns = position.piece_kind_bb(PieceKind::Pawn) & position.player_bb(us);
        (1..=9).fold(Bitboard::EMPTY, |files, file| {
            let mask = Bitboard::file(file);
            if (our_pawns & mask).is_empty() {
                files
            } else {
                files | mask
            }
        })
    } else {
        Bitboard::EMPTY
    };
    let enemy_king = position.king_square(us.flip());
    for piece_kind in Hand::all_hand_pieces() {
        if hand.count(piece_kind).unwrap_or(0) == 0 {
            continue;
        }
        let piece = Piece::new(piece_kind, us);
        // A piece may not be dropped where it could never move again —
        // exactly the squares that would force promotion for a board move.
        let mut drops = targets & !tables::forced_promotion_zone(us, piece_kind);
        if piece_kind == PieceKind::Pawn {
            drops &= !pawn_files;
            // Only a pawn that gives check can be a pawn-drop mate, and by
            // the reverse-lookup trick exactly one square does that, so the
            // expensive simulation is reached at most once per position.
            if let Some(enemy_king) = enemy_king {
                let checking = drops & tables::pawn_attacks(us.flip(), enemy_king);
                if let Some(to) = checking.into_iter().next()
                    && is_pawn_drop_mate(position, piece, to)
                {
                    drops ^= Bitboard::single(to);
                }
            }
        }
        if !drops.is_empty() {
            listener(MoveSet::Drop { piece, to: drops })?;
        }
    }
    ControlFlow::Continue(())
}

/// Whether dropping `piece` (a checking pawn) on `to` leaves the opponent
/// with no legal moves. The pawn checks from an adjacent square, so the
/// opponent cannot block and no recursive pawn-drop arises.
///
/// [`Position::with_drop`] rather than clone-and-`do_move`: the simulated
/// position is thrown away, so it does not need an undo history, and
/// carrying one made this the only allocating step in move generation.
fn is_pawn_drop_mate(position: &Position, piece: Piece, to: Square) -> bool {
    !position.with_drop(piece, to).has_legal_moves()
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
            let _ = position.generate_moves(|set| {
                assert!(!set.is_empty(), "empty MoveSet in {sfen}");
                assert_eq!(set.len(), set.into_iter().count(), "len mismatch in {sfen}");
                counted += set.len();
                expanded.extend(set);
                ControlFlow::Continue(())
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
            let _ = position.generate_moves(|set| {
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
                ControlFlow::Continue(())
            });
        }
    }

    #[test]
    fn early_exit_stops_generation() {
        for sfen in CALLBACK_POSITIONS {
            let position = position_of(sfen);
            let mut seen = 0;
            let flow = position.generate_moves(|_| {
                seen += 1;
                ControlFlow::Break(())
            });
            assert!(flow.is_break(), "early exit was not reported to the caller");
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
