//! Fully legal move generation.
//!
//! The primitive is [`Position::generate_moves`], which hands the caller one
//! [`MoveSet`] per origin — destinations as bitboards — and can be stopped
//! early. [`Position::legal_moves`] is the allocating wrapper over it.
//!
//! Legality is decided per position rather than per move. Once per node
//! [`check_info`] finds the checkers and the pinned pieces in a single scan,
//! and those two bitboards make every non-king move legal by construction:
//!
//! - a piece that is not pinned cannot expose its own king by moving, since
//!   a pin is precisely the situation where it could;
//! - a pinned piece is masked to the line it is pinned on, which still lets
//!   it capture the pinner;
//! - under a single check, every non-king move is masked to capturing the
//!   checker or interposing, and a double check leaves only king moves.
//!
//! The king is the one piece left, since it is what the test is about, and
//! [`king_danger`] decides all of its destinations with a single bitboard.
//!
//! Pawn-drop-mate (打ち歩詰め) is excluded by simulating the drop and asking
//! whether the opponent has any reply at all.

use core::ops::ControlFlow;

use shogi_core::{Color, Hand, Move, Piece, PieceKind, Square};

use crate::bitboard::Bitboard;
use crate::position::Position;
use crate::tables;

/// The most legal moves any shogi position has, which is the move count of
/// the max-moves fixture in DESIGN.md §6.
///
/// [`Position::legal_moves`] sizes its `Vec` from this so no caller pays a
/// growth realloc, which the drop-heavy positions would otherwise pay.
const MAX_LEGAL_MOVES: usize = 593;

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
        /// The piece being moved.
        piece: Piece,
        /// The square it stands on.
        from: Square,
        /// Destinations reached by promoting.
        promotions: Bitboard,
        /// Destinations reached without promoting.
        non_promotions: Bitboard,
    },
    /// Drops of `piece` from hand.
    Drop {
        /// The piece being dropped.
        piece: Piece,
        /// The squares it may be dropped on.
        to: Bitboard,
    },
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

    /// Appends every [`Move`] of this set to `out`.
    ///
    /// Same result as `out.extend(self)`, but the drop-versus-board and
    /// promote decisions are made once per set rather than once per move,
    /// which [`MoveSetIter`] cannot do. Prefer the iterator for consuming
    /// lazily or stopping early, and this for filling a buffer.
    ///
    /// Deliberately does not `reserve`. `Vec::push` checks capacity per
    /// element either way, so reserving only avoids a reallocation — and this
    /// method is for callers that own a sized buffer, where there is none to
    /// avoid. Measured and rejected; DECISIONS.md has it.
    #[inline]
    pub fn write_into(self, out: &mut Vec<Move>) {
        match self {
            MoveSet::Normal {
                from,
                promotions,
                non_promotions,
                ..
            } => {
                promotions.for_each_square(|to| {
                    out.push(Move::Normal {
                        from,
                        to,
                        promote: true,
                    })
                });
                non_promotions.for_each_square(|to| {
                    out.push(Move::Normal {
                        from,
                        to,
                        promote: false,
                    })
                });
            }
            MoveSet::Drop { piece, to } => {
                to.for_each_square(|to| out.push(Move::Drop { piece, to }));
            }
        }
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
        let mut moves = Vec::with_capacity(MAX_LEGAL_MOVES);
        // The listener never breaks, so the walk is always `Continue`.
        let _ = self.generate_moves(|set| {
            set.write_into(&mut moves);
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
    let occupied = position.occupied();
    let our = position.player_bb(us);
    let king = position.king_square(us);
    let info = match king {
        Some(king) => check_info(position, us, king, occupied),
        None => CheckInfo::NONE,
    };

    generate_normal(position, us, occupied, our, king, info, &mut listener)?;
    generate_drops(position, us, occupied, king, info.checkers, &mut listener)
}

/// Splits the destinations of one piece into its promoting and
/// non-promoting sets and hands them to `listener`.
///
/// Runs once per generated set, so both decisions are table lookups keyed on
/// what is already in hand: whether the piece can promote at all is a bitmask
/// test on the kind discriminant, and *where* it may promote is baked per
/// origin in [`tables::promotion_mask`].
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
    let promotions = if tables::PROMOTABLE_KINDS >> (piece.as_u8() & 15) & 1 != 0 {
        attacks & tables::promotion_mask(us, from)
    } else {
        Bitboard::EMPTY
    };
    // A piece that would have no move left must promote.
    let non_promotions = attacks & !tables::forced_promotion_zone(piece);
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
    info: CheckInfo,
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
    if info.checkers.count() < 2 {
        // Under a single check every other piece must capture the checker
        // or interpose; otherwise anything goes.
        let check_mask = match info.checkers.into_iter().next() {
            Some(checker) => tables::between(king, checker) | Bitboard::single(checker),
            None => Bitboard::ALL,
        };
        for from in our & !Bitboard::single(king) {
            let piece = position.piece_at(from).unwrap();
            let mut attacks = tables::attacks_of(piece, from, occupied) & targets & check_mask;
            // A pinned piece may only travel along the line it is pinned
            // on, which includes capturing the pinner.
            if info.pinned.contains(from) {
                attacks &= tables::line(king, from);
            }
            emit_normal(us, piece, from, attacks, listener)?;
        }
    }
    generate_king_moves(position, us, king, occupied, targets, listener)
}

/// The king is the one piece whose moves are not legal by construction, and
/// [`king_danger`] decides all of them at once.
///
/// The danger bitboard is built here rather than in [`generate_legal`] so a
/// walk that stops early — [`Position::has_legal_moves`], and with it the
/// pawn-drop-mate test — usually never pays for it.
fn generate_king_moves(
    position: &Position,
    us: Color,
    king: Square,
    occupied: Bitboard,
    targets: Bitboard,
    listener: &mut impl FnMut(MoveSet) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let candidates = tables::king_attacks(king) & targets;
    // A king boxed in by its own pieces is common enough to be worth the test.
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
/// equivalent to testing each destination on its own post-move occupancy.
/// Three properties make the destinations collapse into a single mask:
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
///   among that square's attackers.
///
/// **The result is only valid on the king's own neighbours**, which is all
/// [`generate_king_moves`] masks with. Only attacks landing there can survive
/// the mask, so every enemy piece is filtered on where it stands before its
/// attack set is built. A slider is *not* exempt: reaching from anywhere is
/// not the same as reaching from anywhere *to here*, and the squares a rook
/// or bishop can bear on a king neighbour from are as fixed by the king
/// square as a knight's are — [`tables::orthogonal_attacker_zone`] /
/// [`tables::diagonal_attacker_zone`].
///
/// ⚠️ **The filter may only ever be a *superset* of what can attack**:
/// under-report danger and a king move that walks into check becomes legal.
/// The empty-board rays give that direction for free, since occupancy only
/// ever shortens a ray, and `slider_attacker_zones_cover_every_slider` holds
/// the geometry to [`tables::attacks_of`] itself.
///
/// A search wanting the opponent's *full* attack map wants this filter
/// dropped, which is a one-line change and a different measurement. It is not
/// dropped speculatively — see DECISIONS.md.
fn king_danger(position: &Position, us: Color, king: Square, occupied: Bitboard) -> Bitboard {
    let occupied = occupied ^ Bitboard::single(king);
    let rooks = position.piece_kind_bb(PieceKind::Rook)
        | position.piece_kind_bb(PieceKind::ProRook)
        // A lance's ray is a subset of the rook's, so the orthogonal zone is
        // a superset for it and it needs no colour-keyed table of its own.
        | position.piece_kind_bb(PieceKind::Lance);
    let bishops =
        position.piece_kind_bb(PieceKind::Bishop) | position.piece_kind_bb(PieceKind::ProBishop);
    // ⚠️ The step term applies to *every* enemy piece, not only the
    // non-sliders: it is what covers a horse's orthogonal sidesteps and a
    // dragon's diagonal ones, which lie on neither piece's rays. Restricting
    // it to the non-sliders looks like tidying and is a silent bug — see
    // `tables::step_attacker_zone`.
    let relevant = position.player_bb(us.flip())
        & (tables::step_attacker_zone(king)
            | (rooks & tables::orthogonal_attacker_zone(king))
            | (bishops & tables::diagonal_attacker_zone(king)));
    let mut danger = Bitboard::EMPTY;
    for square in relevant {
        let piece = position
            .piece_at(square)
            .expect("player_bb agrees with the mailbox");
        danger |= tables::attacks_of(piece, square, occupied);
    }
    danger
}

/// What one scan around the king establishes about legality: which enemy
/// pieces check it, and which of ours are pinned against it.
#[derive(Clone, Copy)]
struct CheckInfo {
    checkers: Bitboard,
    pinned: Bitboard,
}

impl CheckInfo {
    /// For a side with no king on the board: nothing to check, nothing to pin.
    const NONE: Self = Self {
        checkers: Bitboard::EMPTY,
        pinned: Bitboard::EMPTY,
    };
}

/// The enemy pieces checking our king, and our pieces pinned against it.
///
/// Both come out of one scan, because for a slider they are the same
/// question asked at different blocker counts. Snipers are the enemy sliders
/// that would reach the king on an *empty* board; for each, count what
/// actually stands between:
///
/// - **nothing** — the slider reaches the king, so it is a checker;
/// - **exactly one piece** — that piece is what keeps the ray off the king,
///   so moving it off the line would expose it: a pin;
/// - **two or more** — neither, and moving one still leaves the other.
///
/// The empty-board pass subsumes a separate checker scan: a slider on `s`
/// attacks the king through `occupied` exactly when `s` is on a slider line
/// from the king *and* `between(king, s)` is clear, which is the zero-blocker
/// case above.
///
/// A dragon's or horse's one-step sidesteps can never pin — nothing fits
/// between them and the king — so they are left to the step lookups below,
/// while their sliding halves ride along in `rooks` and `bishops`.
fn check_info(position: &Position, us: Color, king: Square, occupied: Bitboard) -> CheckInfo {
    let their = position.player_bb(us.flip());
    let rooks = (position.piece_kind_bb(PieceKind::Rook)
        | position.piece_kind_bb(PieceKind::ProRook))
        & their;
    let bishops = (position.piece_kind_bb(PieceKind::Bishop)
        | position.piece_kind_bb(PieceKind::ProBishop))
        & their;
    let lances = position.piece_kind_bb(PieceKind::Lance) & their;

    // All three are `{rook,bishop,lance}_attacks(.., EMPTY)`. With no
    // occupancy to consult they are fixed by the king square alone — and by
    // `us` for the lance, which only attacks forwards — so each is one load
    // rather than a trip through the slider backend.
    let snipers = (tables::rook_rays(king) & rooks)
        | (tables::bishop_rays(king) & bishops)
        // A lance only attacks forwards, so the squares it could pin from
        // are the ones our own lance would attack from the king.
        | (tables::lance_rays(us, king) & lances);

    let mut checkers = Bitboard::EMPTY;
    let mut pinned = Bitboard::EMPTY;
    for sniper in snipers {
        let blockers = tables::between(king, sniper) & occupied;
        match blockers.count() {
            0 => checkers |= Bitboard::single(sniper),
            1 => pinned |= blockers,
            _ => {}
        }
    }

    // Every remaining checker moves a fixed number of steps, so the reverse
    // lookup of `attackers_to` is a plain table intersection — no occupancy
    // involved, and nothing here can pin.
    let golds = position.piece_kind_bb(PieceKind::Gold)
        | position.piece_kind_bb(PieceKind::ProPawn)
        | position.piece_kind_bb(PieceKind::ProLance)
        | position.piece_kind_bb(PieceKind::ProKnight)
        | position.piece_kind_bb(PieceKind::ProSilver);
    let mut steps = tables::pawn_attacks(us, king) & position.piece_kind_bb(PieceKind::Pawn);
    steps |= tables::knight_attacks(us, king) & position.piece_kind_bb(PieceKind::Knight);
    steps |= tables::silver_attacks(us, king) & position.piece_kind_bb(PieceKind::Silver);
    steps |= tables::gold_attacks(us, king) & golds;
    steps |= tables::orthogonal_attacks(king) & position.piece_kind_bb(PieceKind::ProBishop);
    steps |= tables::diagonal_attacks(king) & position.piece_kind_bb(PieceKind::ProRook);
    // Two kings cannot legally stand adjacent, but `Position::new` can build
    // it; kept so `checkers` agrees with `in_check` there.
    steps |= tables::king_attacks(king) & position.piece_kind_bb(PieceKind::King);
    checkers |= steps & their;

    CheckInfo {
        checkers,
        // Only our own pieces are pinned; one of theirs in the way is their
        // discovered-check opportunity, not our problem.
        pinned: pinned & position.player_bb(us),
    }
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
        let mut drops = targets & !tables::forced_promotion_zone(piece);
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
/// position is thrown away, so it needs no undo history — and carrying one
/// made this the only allocating step in move generation.
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

    /// Which fixture covers what, and whether anything would notice its
    /// removal. Four are individually held by a named liveness assertion; two
    /// jointly hold one; two are corpus breadth, and dropping either of those
    /// is silent.
    const CALLBACK_POSITIONS: &[&str] = &[
        // Breadth: the ordinary opening shape, and the only position where
        // nothing can promote.
        "startpos",
        // Breadth, and the corpus's bulk — supplies almost all of the
        // `dropped * 8 > nodes` margin in
        // `king_danger_agrees_with_the_unfiltered_scan`.
        "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1",
        // Breadth: drop-heavy, and most of the node count.
        "R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1",
        // In check (a real game position), and — after Bx6h+ — the corpus's
        // only enemy *horse* held by the step term alone. Held by
        // `assert!(horse_only > 0)`.
        "lnsgk2nl/1r4gs1/p1pppp1pp/6p2/1p5P1/2P6/PPbPPPP1P/2G2G1R1/LNS1K1SNL b b 15",
        // Two unblocked snipers checking at once (rook 5b, bishop 3c, king
        // 5e), which is what makes `check_info`'s `checkers |=` differ from
        // `=`. Held by `assert!(doubles > 0)`, jointly with the next-but-one.
        "8k/4r4/6b2/7G1/4K4/9/9/9/9 b - 1",
        // Two snipers pinning at once (rook 5a pins the gold, bishop 2b the
        // silver): the same demand on `pinned |=`. Held by
        // `assert!(double_pins > 0)`, which nothing else satisfies.
        "4r4/7b1/9/4GS3/4K4/9/9/9/8k b - 1",
        // Promoted sliders bearing on the king by *sliding* — a dragon on 4a
        // covers 4i, a horse on 1c covers 6h, both outside the step-attacker
        // box. Also the second source of `doubles`.
        "k4+R3/9/8+B/9/9/9/9/9/4K4 b - 1",
        // The corpus's only enemy *dragon* held by the step term alone: 3g is
        // inside `STEP_ATTACKER_ZONE` and outside `ORTHOGONAL_ATTACKER_ZONE`,
        // and what reaches 4h is its diagonal sidestep. Held by
        // `assert!(dragon_only > 0)`.
        "4k4/9/9/9/9/9/6+r2/9/4K4 b - 1",
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
                        assert!(tables::forced_promotion_zone(piece).contains(to));
                    }
                }
                ControlFlow::Continue(())
            });
        }
    }

    /// `write_into` must agree with the iterator **element for element and
    /// in order**, not merely as a set: a caller collecting into a buffer can
    /// depend on that.
    #[test]
    fn write_into_matches_the_iterator() {
        for sfen in CALLBACK_POSITIONS {
            let position = position_of(sfen);
            let mut sets = 0;
            let mut expected = Vec::new();
            let mut written = Vec::new();
            let _ = position.generate_moves(|set| {
                sets += 1;
                expected.extend(set);
                let before = written.len();
                set.write_into(&mut written);
                assert_eq!(
                    written.len() - before,
                    set.len(),
                    "write_into wrote a different count than MoveSet::len reports"
                );
                ControlFlow::Continue(())
            });
            assert!(sets > 0, "no move sets for {sfen}");
            assert_eq!(written, expected, "write_into disagrees on {sfen}");
            // Against `expected` (the iterator's list), not `written`:
            // `legal_moves` drives `write_into`, so that would be circular.
            assert_eq!(position.legal_moves(), expected, "legal_moves disagrees");
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

    /// The unfiltered danger scan: every enemy slider walked wherever it
    /// stands. A test-only oracle, the way `sliders::naive` is kept for the
    /// slider backends.
    fn king_danger_reference(
        position: &Position,
        us: Color,
        king: Square,
        occupied: Bitboard,
    ) -> Bitboard {
        let occupied = occupied ^ Bitboard::single(king);
        let sliders = position.piece_kind_bb(PieceKind::Lance)
            | position.piece_kind_bb(PieceKind::Bishop)
            | position.piece_kind_bb(PieceKind::Rook)
            | position.piece_kind_bb(PieceKind::ProBishop)
            | position.piece_kind_bb(PieceKind::ProRook);
        let relevant = position.player_bb(us.flip()) & (tables::step_attacker_zone(king) | sliders);
        let mut danger = Bitboard::EMPTY;
        for square in relevant {
            let piece = position.piece_at(square).expect("player_bb agrees");
            danger |= tables::attacks_of(piece, square, occupied);
        }
        danger
    }

    /// The filter may only drop pieces that could not have reached the king's
    /// neighbourhood, so the two scans must agree **on the king's neighbours**
    /// — the only place either is defined. Held over every position two plies
    /// deep from each fixture.
    ///
    /// Unlike perft, this does not depend on the tree happening to contain a
    /// position where the omitted piece bears on a king destination: it
    /// compares the two scans at every node, whether or not the difference
    /// would have changed a move.
    #[test]
    fn king_danger_agrees_with_the_unfiltered_scan() {
        /// `dragon_only` / `horse_only` count the nodes where an enemy dragon
        /// or horse is held by the step term alone — inside
        /// `STEP_ATTACKER_ZONE`, outside its own ray zone. That is the one
        /// configuration distinguishing "the step term applies to every enemy
        /// piece" from "... to the non-sliders", and the two halves come from
        /// different fixtures, so they are counted separately: one assertion
        /// over both would be satisfied by either alone.
        fn walk(
            position: &mut Position,
            depth: u32,
            nodes: &mut u64,
            dropped: &mut u64,
            dragon_only: &mut u64,
            horse_only: &mut u64,
        ) {
            let us = position.side_to_move();
            if let Some(king) = position.king_square(us) {
                let occupied = position.occupied();
                let neighbours = tables::king_attacks(king);
                assert_eq!(
                    king_danger(position, us, king, occupied) & neighbours,
                    king_danger_reference(position, us, king, occupied) & neighbours,
                    "king_danger disagrees with the unfiltered scan in\n{position:?}"
                );
                // Enemy sliders the filter refuses to walk. Counting them is
                // what makes the agreement above evidence of anything: on a
                // corpus where nothing is dropped, the two are trivially
                // equal. Split per family exactly as `king_danger` does.
                let their = position.player_bb(us.flip());
                let rooks = their
                    & (position.piece_kind_bb(PieceKind::Rook)
                        | position.piece_kind_bb(PieceKind::ProRook)
                        | position.piece_kind_bb(PieceKind::Lance));
                let bishops = their
                    & (position.piece_kind_bb(PieceKind::Bishop)
                        | position.piece_kind_bb(PieceKind::ProBishop));
                let steps = tables::step_attacker_zone(king);
                let orthogonal = tables::orthogonal_attacker_zone(king);
                let diagonal = tables::diagonal_attacker_zone(king);
                *dropped += (rooks & !(steps | orthogonal)).count() as u64
                    + (bishops & !(steps | diagonal)).count() as u64;
                // Only the two promoted kinds can land here: a rook, bishop or
                // lance bearing on a neighbour is in its ray zone by
                // construction, so the step term is never the only thing
                // holding one.
                let dragons = rooks & position.piece_kind_bb(PieceKind::ProRook);
                let horses = bishops & position.piece_kind_bb(PieceKind::ProBishop);
                if !(dragons & steps & !orthogonal).is_empty() {
                    *dragon_only += 1;
                }
                if !(horses & steps & !diagonal).is_empty() {
                    *horse_only += 1;
                }
                *nodes += 1;
            }
            if depth == 0 {
                return;
            }
            for mv in position.legal_moves() {
                let undo = position.do_move(mv);
                walk(position, depth - 1, nodes, dropped, dragon_only, horse_only);
                position.undo_move(mv, undo);
            }
        }
        let mut nodes = 0;
        let mut dropped = 0;
        let mut dragon_only = 0;
        let mut horse_only = 0;
        for sfen in CALLBACK_POSITIONS {
            let mut position = position_of(sfen);
            walk(
                &mut position,
                2,
                &mut nodes,
                &mut dropped,
                &mut dragon_only,
                &mut horse_only,
            );
        }
        assert!(nodes > 10_000, "test covered only {nodes} nodes");
        // A filter that has quietly become a no-op drops nothing; this fails
        // on that rather than on ordinary fixture drift.
        assert!(
            dropped * 8 > nodes,
            "only {dropped} sliders dropped over {nodes} nodes; the filter is barely exercised"
        );
        // Restricting the step term to the non-sliders is the tidying
        // `king_danger` warns against, and only these two configurations
        // distinguish it. Each comes from one fixture, so both are asserted:
        // dropping either fixture must fail here rather than quietly stop
        // covering its half.
        assert!(
            dragon_only > 0,
            "no enemy dragon was held by the step term alone; \
             the dragon fixture is no longer covering its case"
        );
        assert!(
            horse_only > 0,
            "no enemy horse was held by the step term alone; \
             the in-check fixture is no longer covering its case"
        );
    }

    /// The standalone pin scan, without the checker search fused into it —
    /// a test-only oracle, the way `sliders::naive` is kept for the slider
    /// backends.
    fn pinned_pieces_reference(
        position: &Position,
        us: Color,
        king: Square,
        occupied: Bitboard,
    ) -> Bitboard {
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
            | (tables::lance_attacks(us, king, empty) & lances);
        let mut pinned = Bitboard::EMPTY;
        for sniper in snipers {
            let blockers = tables::between(king, sniper) & occupied;
            if blockers.count() == 1 {
                pinned |= blockers;
            }
        }
        pinned & position.player_bb(us)
    }

    /// The enemy sliders that reach `king` with nothing in between — the
    /// zero-blocker case of `check_info`'s sniper loop.
    fn unblocked_snipers(position: &Position, us: Color, king: Square) -> Bitboard {
        let occupied = position.occupied();
        let their = position.player_bb(us.flip());
        let rooks = (position.piece_kind_bb(PieceKind::Rook)
            | position.piece_kind_bb(PieceKind::ProRook))
            & their;
        let bishops = (position.piece_kind_bb(PieceKind::Bishop)
            | position.piece_kind_bb(PieceKind::ProBishop))
            & their;
        let lances = position.piece_kind_bb(PieceKind::Lance) & their;
        let snipers = (tables::rook_rays(king) & rooks)
            | (tables::bishop_rays(king) & bishops)
            | (tables::lance_rays(us, king) & lances);
        let mut unblocked = Bitboard::EMPTY;
        for sniper in snipers {
            if (tables::between(king, sniper) & occupied).is_empty() {
                unblocked |= Bitboard::single(sniper);
            }
        }
        unblocked
    }

    /// `check_info` folds the slider half of the checker search into the pin
    /// scan, so *both* halves are held to an independent oracle: `checkers`
    /// against the general `attackers_to` reverse lookup, and `pinned`
    /// against the standalone scan above. Held over every position two plies deep
    /// from each fixture.
    #[test]
    fn check_info_agrees_with_attackers_to() {
        /// `doubles` / `double_pins` count the configurations that make the
        /// sniper loop's two `|=` meaningful, and both are counted over
        /// *snipers* rather than over the resulting bitboards: a double check
        /// by one slider and one step piece raises `checkers.count()` without
        /// ever running the loop twice, so counting checkers would let the
        /// `|=` go untested.
        fn walk(
            position: &mut Position,
            depth: u32,
            nodes: &mut u64,
            doubles: &mut u64,
            double_pins: &mut u64,
        ) {
            let us = position.side_to_move();
            if let Some(king) = position.king_square(us) {
                let occupied = position.occupied();
                let them = us.flip();
                let expected =
                    attackers_to(position, king, occupied, them, position.player_bb(them));
                let info = check_info(position, us, king, occupied);
                assert_eq!(
                    info.checkers, expected,
                    "checkers disagree in\n{position:?}"
                );
                assert_eq!(
                    info.pinned,
                    pinned_pieces_reference(position, us, king, occupied),
                    "pinned disagrees in\n{position:?}"
                );
                if unblocked_snipers(position, us, king).count() >= 2 {
                    *doubles += 1;
                }
                if info.pinned.count() >= 2 {
                    *double_pins += 1;
                }
                *nodes += 1;
            }
            if depth == 0 {
                return;
            }
            for mv in position.legal_moves() {
                let undo = position.do_move(mv);
                walk(position, depth - 1, nodes, doubles, double_pins);
                position.undo_move(mv, undo);
            }
        }
        let mut nodes = 0;
        let mut doubles = 0;
        let mut double_pins = 0;
        for sfen in CALLBACK_POSITIONS {
            let mut position = position_of(sfen);
            walk(&mut position, 2, &mut nodes, &mut doubles, &mut double_pins);
        }
        assert!(nodes > 10_000, "test covered only {nodes} nodes");
        // Both accumulations need more than one sniper to be distinguishable
        // from a plain assignment, and neither case occurs by accident:
        // without these fixtures, turning either `|=` into `=` passes the
        // whole suite including the deep perft values.
        assert!(doubles > 0, "no double check reached; the OR is untested");
        assert!(double_pins > 0, "no double pin reached; the OR is untested");
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
