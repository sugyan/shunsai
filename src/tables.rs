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

/// Every square within `file_radius` files and `rank_radius` ranks.
const fn box_table(file_radius: i8, rank_radius: i8) -> [Bitboard; 81] {
    let mut table = [Bitboard::EMPTY; 81];
    let mut index = 0;
    while index < 81 {
        let (file, rank) = (file_of(index), rank_of(index));
        let mut bits = 0u128;
        let mut file_delta = -file_radius;
        while file_delta <= file_radius {
            let mut rank_delta = -rank_radius;
            while rank_delta <= rank_radius {
                let (to_file, to_rank) = (file + file_delta, rank + rank_delta);
                if on_board(to_file, to_rank) {
                    bits |= 1 << index_of(to_file, to_rank);
                }
                rank_delta += 1;
            }
            file_delta += 1;
        }
        table[index] = Bitboard::from_bits(bits);
        index += 1;
    }
    table
}

static PAWN_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(PAWN_STEPS);
static KNIGHT_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(KNIGHT_STEPS);
static SILVER_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(SILVER_STEPS);
static GOLD_ATTACKS: [[Bitboard; 81]; 2] = step_table_both(GOLD_STEPS);
static KING_ATTACKS: [Bitboard; 81] = step_table(&KING_STEPS);
static ORTHOGONAL_ATTACKS: [Bitboard; 81] = step_table(&ORTHOGONAL_STEPS);
static DIAGONAL_ATTACKS: [Bitboard; 81] = step_table(&DIAGONAL_STEPS);

/// `STEP_ATTACKS[piece.as_u8() & 31][square]`: the attacks of `piece` on
/// `square` that do not depend on occupancy.
///
/// One table indexed by the *piece* — colour and kind together — so
/// [`attacks_of`] can serve every non-slider with a single indexed load
/// instead of dispatching on [`PieceKind`]. `Piece::as_u8` is
/// `discriminant + 16 * colour`, i.e. `1..=14` and `17..=30`, so masking with
/// 31 lands in a 32-row table and the index is provably in range without a
/// bounds check.
///
/// Three rows per colour are empty because their piece has no
/// occupancy-independent attack at all: lance, bishop and rook. The two
/// promoted sliders hold only their *step* half — a horse's orthogonal
/// neighbours, a dragon's diagonal ones — which [`attacks_of`] ORs onto the
/// sliding half. The unused indices (0, 15, 16, 31) stay empty.
///
/// This duplicates the per-kind tables above rather than replacing them: the
/// reverse-lookup scans in `movegen` want a specific kind's table for a known
/// colour, where a two-row table is the better shape. Cost of the duplication
/// is 40.5 KiB of `.rodata`, which matters only to the deferred
/// magic-versus-qugiy re-run under cache pressure — see DESIGN.md.
static STEP_ATTACKS: [[Bitboard; 81]; 32] = step_attacks_table();

const fn step_attacks_table() -> [[Bitboard; 81]; 32] {
    let mut table = [[Bitboard::EMPTY; 81]; 32];
    let pawn = step_table_both(PAWN_STEPS);
    let knight = step_table_both(KNIGHT_STEPS);
    let silver = step_table_both(SILVER_STEPS);
    let gold = step_table_both(GOLD_STEPS);
    let king = step_table(&KING_STEPS);
    let orthogonal = step_table(&ORTHOGONAL_STEPS);
    let diagonal = step_table(&DIAGONAL_STEPS);
    let mut color = 0;
    while color < 2 {
        // `Piece::as_u8` puts White 16 above Black; the offsets are the
        // `PieceKind` discriminants, and `piece_index_matches_as_u8` holds
        // this arithmetic to `Piece::as_u8` itself.
        let base = color * 16;
        table[base + PieceKind::Pawn as usize] = pawn[color];
        table[base + PieceKind::Knight as usize] = knight[color];
        table[base + PieceKind::Silver as usize] = silver[color];
        table[base + PieceKind::Gold as usize] = gold[color];
        table[base + PieceKind::King as usize] = king;
        table[base + PieceKind::ProPawn as usize] = gold[color];
        table[base + PieceKind::ProLance as usize] = gold[color];
        table[base + PieceKind::ProKnight as usize] = gold[color];
        table[base + PieceKind::ProSilver as usize] = gold[color];
        table[base + PieceKind::ProBishop as usize] = orthogonal;
        table[base + PieceKind::ProRook as usize] = diagonal;
        color += 1;
    }
    table
}

/// The [`PieceKind`] discriminants whose attacks depend on `occupied`, as a
/// bitmask — the one test [`attacks_of`] makes before its table load.
const SLIDER_KINDS: u32 = (1 << PieceKind::Lance as u32)
    | (1 << PieceKind::Bishop as u32)
    | (1 << PieceKind::Rook as u32)
    | (1 << PieceKind::ProBishop as u32)
    | (1 << PieceKind::ProRook as u32);

/// `STEP_ATTACKER_ZONE[king]`: every square from which a piece that moves a
/// *fixed* number of steps could attack a square adjacent to `king`.
///
/// Two files by three ranks, because the longest-reaching such piece is the
/// knight (one file, two ranks) and the squares that matter — the king's
/// eight neighbours — are themselves one step away. It is deliberately a
/// superset rather than the exact set: the point is only that anything
/// *outside* it provably cannot bear on a king destination, which
/// `step_attacker_zone_covers_every_step_piece` checks against the attack
/// tables themselves.
static STEP_ATTACKER_ZONE: [Bitboard; 81] = box_table(2, 3);

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

/// Where a step piece has to stand to attack a neighbour of `king`; see
/// [`STEP_ATTACKER_ZONE`]. Sliders are unbounded and are never covered by
/// this.
#[inline(always)]
pub(crate) fn step_attacker_zone(king: Square) -> Bitboard {
    STEP_ATTACKER_ZONE[king.array_index()]
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

/// `PROMOTION_MASK[color][from]`: the destinations a promotable piece
/// standing on `from` may promote on.
///
/// Promotion is legal when the move *starts or ends* in the zone, which is
/// two conditions on one set of destinations: a piece already inside the zone
/// may promote everywhere, and one outside it only on the zone itself. That is
/// a per-origin fact, not a per-destination one, so it is baked per origin
/// here rather than tested per set — `emit_normal` used to load the zone and
/// then branch on `zone.contains(from)`.
static PROMOTION_MASK: [[Bitboard; 81]; 2] = promotion_mask_table();

const fn promotion_mask_table() -> [[Bitboard; 81]; 2] {
    let mut table = [[Bitboard::EMPTY; 81]; 2];
    let mut color = 0;
    while color < 2 {
        let zone = PROMOTION_ZONE[color];
        let mut index = 0;
        while index < 81 {
            // Raw bits rather than `contains`: the builders index
            // `array_index` arithmetic directly, `Square::new` not being const.
            table[color][index] = if zone.bits() & (1 << index) != 0 {
                Bitboard::ALL
            } else {
                zone
            };
            index += 1;
        }
        color += 1;
    }
    table
}

#[inline(always)]
pub(crate) fn promotion_mask(color: Color, from: Square) -> Bitboard {
    PROMOTION_MASK[color.array_index()][from.array_index()]
}

/// The [`PieceKind`] discriminants that can promote, as a bitmask — gold,
/// king and everything already promoted are absent.
pub(crate) const PROMOTABLE_KINDS: u32 = (1 << PieceKind::Pawn as u32)
    | (1 << PieceKind::Lance as u32)
    | (1 << PieceKind::Knight as u32)
    | (1 << PieceKind::Silver as u32)
    | (1 << PieceKind::Bishop as u32)
    | (1 << PieceKind::Rook as u32);

/// `FORCED_PROMOTION[piece.as_u8() & 31]`: the ranks on which `piece` would
/// have no move left, so promotion is compulsory. Empty for the pieces that
/// are never forced.
///
/// Indexed by piece for the same reason as [`STEP_ATTACKS`]: it replaces a
/// `match` on the kind followed by a colour-indexed load, in a function that
/// runs once per generated set.
static FORCED_PROMOTION: [Bitboard; 32] = forced_promotion_table();

const fn forced_promotion_table() -> [Bitboard; 32] {
    let mut table = [Bitboard::EMPTY; 32];
    let mut color = 0;
    while color < 2 {
        let base = color * 16;
        table[base + PieceKind::Pawn as usize] = LAST_RANK[color];
        table[base + PieceKind::Lance as usize] = LAST_RANK[color];
        table[base + PieceKind::Knight as usize] = LAST_TWO_RANKS[color];
        color += 1;
    }
    table
}

#[inline(always)]
pub(crate) fn forced_promotion_zone(piece: Piece) -> Bitboard {
    FORCED_PROMOTION[(piece.as_u8() & 31) as usize]
}

// Slider attacks live in `crate::sliders`, which is the swap boundary for
// the M4 backends; these are the crate-wide entry points.
pub(crate) use crate::sliders::active::{bishop_attacks, rook_attacks};
pub(crate) use crate::sliders::lance_attacks;

/// The squares attacked by `piece` on `square`, given `occupied` (relevant to
/// sliders only).
///
/// Called once per origin in generation's dense pass and once per relevant
/// enemy piece in [`king_danger`](crate::movegen), so the *dispatch* is on the
/// hot path in its own right. It used to be a ten-arm `match` on
/// [`PieceKind`], which LLVM lowers to a jump table: an indirect branch whose
/// target is whatever piece the mailbox happened to yield, and therefore
/// poorly predicted. Nine of the fourteen kinds want nothing but a table
/// lookup, so those are folded into one [`STEP_ATTACKS`] row and reached
/// through a single well-predicted *direct* branch instead. Only the five
/// slider kinds still dispatch, and there are few of them per node.
#[inline]
pub(crate) fn attacks_of(piece: Piece, square: Square, occupied: Bitboard) -> Bitboard {
    let index = (piece.as_u8() & 31) as usize;
    // The low four bits are the `PieceKind` discriminant for either colour.
    if SLIDER_KINDS >> (index & 15) & 1 == 0 {
        return STEP_ATTACKS[index][square.array_index()];
    }
    let (piece_kind, color) = piece.to_parts();
    match piece_kind {
        PieceKind::Lance => lance_attacks(color, square, occupied),
        PieceKind::Bishop => bishop_attacks(square, occupied),
        PieceKind::Rook => rook_attacks(square, occupied),
        PieceKind::ProBishop => {
            bishop_attacks(square, occupied) | STEP_ATTACKS[index][square.array_index()]
        }
        // `ProRook`; `SLIDER_KINDS` admits nothing else, and
        // `attacks_of_matches_the_per_kind_tables` covers every kind.
        _ => rook_attacks(square, occupied) | STEP_ATTACKS[index][square.array_index()],
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
    fn const_line_table_matches_reference() {
        let mut reference = Box::new([[Bitboard::EMPTY; 81]; 81]);
        for from in Square::all() {
            // One axis walked both ways is one full line through `from`.
            for (file_delta, rank_delta) in AXES {
                let mut line = Bitboard::single(from);
                for sign in [1, -1] {
                    let mut current = from;
                    while let Some(next) = current.shift(sign * file_delta, sign * rank_delta) {
                        line |= Bitboard::single(next);
                        current = next;
                    }
                }
                for to in line {
                    if to != from {
                        reference[from.array_index()][to.array_index()] = line;
                    }
                }
            }
        }
        assert_eq!(LINE, *reference);
    }

    /// The properties `movegen` actually relies on when it masks a pinned
    /// piece to `line(king, from)`: the mask must let it capture the pinner
    /// and shuffle anywhere between, and must be empty for an unaligned pair
    /// so a pin can never be inferred where there is no line.
    #[test]
    fn line_supports_the_pin_mask() {
        for a in Square::all() {
            assert!(line(a, a).is_empty(), "LINE[a][a] must be empty");
            for b in Square::all() {
                let aligned = a != b
                    && (a.file() == b.file()
                        || a.rank() == b.rank()
                        || (a.file() as i8 - b.file() as i8).abs()
                            == (a.rank() as i8 - b.rank() as i8).abs());
                let line = line(a, b);
                assert_eq!(aligned, !line.is_empty(), "alignment vs LINE: {a:?} {b:?}");
                if !aligned {
                    continue;
                }
                assert!(
                    line.contains(a) && line.contains(b),
                    "endpoints on the line"
                );
                assert_eq!(line, self::line(b, a), "LINE must be symmetric");
                // A pinned piece on `b` may capture the pinner and occupy any
                // square between, so both must survive the `& line(a, b)` mask.
                assert_eq!(between(a, b) & !line, Bitboard::EMPTY, "between ⊆ line");
                // Every square of the line describes the same line.
                for c in line {
                    if c != a {
                        assert_eq!(self::line(a, c), line, "{a:?} {b:?} {c:?}");
                    }
                }
            }
        }
    }

    /// The one property `king_danger` relies on when it drops distant
    /// pieces from its loop: if a piece whose attacks do not depend on
    /// occupancy attacks any neighbour of the king, it stands inside
    /// `STEP_ATTACKER_ZONE[king]`. Checked exhaustively — every king square,
    /// every neighbour, every origin, every non-slider kind and colour —
    /// against `attacks_of` itself, so widening a step piece's reach (or
    /// shrinking the box) fails here.
    #[test]
    fn step_attacker_zone_covers_every_step_piece() {
        let step_kinds = [
            PieceKind::Pawn,
            PieceKind::Knight,
            PieceKind::Silver,
            PieceKind::Gold,
            PieceKind::King,
            PieceKind::ProPawn,
            PieceKind::ProLance,
            PieceKind::ProKnight,
            PieceKind::ProSilver,
        ];
        let mut covered = 0;
        for king in Square::all() {
            let zone = step_attacker_zone(king);
            for to in king_attacks(king) {
                for from in Square::all() {
                    for color in Color::all() {
                        for piece_kind in step_kinds {
                            let piece = Piece::new(piece_kind, color);
                            // Occupancy-independent by construction, which
                            // is what makes `EMPTY` the whole story here.
                            if attacks_of(piece, from, Bitboard::EMPTY).contains(to) {
                                assert!(
                                    zone.contains(from),
                                    "{piece:?} on {from:?} attacks {to:?} next to king {king:?}, \
                                     but {from:?} is outside the zone"
                                );
                                covered += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(covered > 10_000, "test exercised only {covered} attacks");
    }

    /// The zone is a *superset*, so it may not be so tight that the sliders
    /// it excludes were the only reason it looked correct — and it must not
    /// be the whole board either, or it would filter nothing.
    #[test]
    fn step_attacker_zone_is_bounded() {
        // 5 files x 7 ranks in the middle, less at the edges.
        assert_eq!(step_attacker_zone(sq(5, 5)).count(), 35);
        assert_eq!(step_attacker_zone(sq(1, 1)).count(), 3 * 4);
        for king in Square::all() {
            assert!(step_attacker_zone(king).contains(king));
            assert!(step_attacker_zone(king).count() < 81);
        }
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

    /// [`STEP_ATTACKS`] is indexed by `Piece::as_u8() & 31`, and the const
    /// builder writes each row at `16 * colour + discriminant`. Nothing in the
    /// type system ties those two together — `as_u8` is an upstream
    /// representation this crate is reading through a mask — so pin it.
    ///
    /// Also pins the mask itself: every real piece must land in `0..32` and no
    /// two pieces may collide, or one would silently serve the other's
    /// attacks.
    #[test]
    fn piece_index_matches_as_u8() {
        let mut seen = [None; 32];
        for piece in Piece::all() {
            let (piece_kind, color) = piece.to_parts();
            let expected = 16 * color.array_index() + piece_kind as usize;
            let index = (piece.as_u8() & 31) as usize;
            assert_eq!(index, expected, "index arithmetic disagrees for {piece:?}");
            assert_eq!(index, piece.as_u8() as usize, "the mask dropped a bit");
            assert!(
                seen[index].replace(piece).is_none(),
                "index {index} is shared by two pieces"
            );
            // The low nibble must recover the kind for both colours, which is
            // what the `SLIDER_KINDS` test reads.
            assert_eq!(index & 15, piece_kind as usize);
        }
    }

    /// The two promotion tables replaced code that read the rules directly —
    /// `piece_kind.promote().is_some()`, `zone.contains(from)`, and a `match`
    /// on the kind — so hold each to the rule it encodes, over every piece and
    /// square.
    ///
    /// `PROMOTABLE_KINDS` against `PieceKind::promote()` catches a wrong bit
    /// in the mask; `promotion_mask` against the start-or-end-in-the-zone rule
    /// catches a colour swap or an inverted `contains`; `forced_promotion_zone`
    /// against the per-kind ranks catches a row filed under the wrong kind —
    /// the failure mode the `attacks_of` sabotage found the perft values blind
    /// to.
    #[test]
    fn promotion_tables_match_the_rules() {
        for piece in Piece::all() {
            let (piece_kind, color) = piece.to_parts();
            assert_eq!(
                PROMOTABLE_KINDS >> (piece.as_u8() & 15) & 1 != 0,
                piece_kind.promote().is_some(),
                "PROMOTABLE_KINDS disagrees for {piece:?}"
            );
            let forced = match piece_kind {
                PieceKind::Pawn | PieceKind::Lance => LAST_RANK[color.array_index()],
                PieceKind::Knight => LAST_TWO_RANKS[color.array_index()],
                _ => Bitboard::EMPTY,
            };
            assert_eq!(
                forced_promotion_zone(piece),
                forced,
                "forced_promotion_zone disagrees for {piece:?}"
            );
            for from in Square::all() {
                let zone = PROMOTION_ZONE[color.array_index()];
                // Promotion is legal iff the move starts or ends in the zone.
                let expected = if zone.contains(from) {
                    Bitboard::ALL
                } else {
                    zone
                };
                assert_eq!(
                    promotion_mask(color, from),
                    expected,
                    "promotion_mask disagrees for {color:?} from {from:?}"
                );
            }
        }
    }

    /// `attacks_of` no longer dispatches per kind, so hold the folded form to
    /// the per-kind tables it replaced — over **every** piece and square, on
    /// an empty board and on populated ones.
    ///
    /// This is the test that catches a mis-filed [`STEP_ATTACKS`] row (a gold
    /// row under `ProSilver`, a colour swapped, `orthogonal` and `diagonal`
    /// exchanged between horse and dragon) and a wrong bit in
    /// [`SLIDER_KINDS`] — a slider treated as a step piece returns its step
    /// half alone, and a step piece treated as a slider falls into the
    /// `_ => rook_attacks(..)` arm.
    #[test]
    fn attacks_of_matches_the_per_kind_tables() {
        fn reference(piece: Piece, square: Square, occupied: Bitboard) -> Bitboard {
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
                PieceKind::ProBishop => {
                    bishop_attacks(square, occupied) | orthogonal_attacks(square)
                }
                PieceKind::ProRook => rook_attacks(square, occupied) | diagonal_attacks(square),
            }
        }

        let mut occupancies = vec![Bitboard::EMPTY, Bitboard::ALL];
        let mut state = 0x9e3779b97f4a7c15u64;
        for _ in 0..64 {
            let mut bits = 0u128;
            for _ in 0..2 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                bits = (bits << 64) | state as u128;
            }
            occupancies.push(Bitboard::ALL & Bitboard::from_bits(bits & ((1 << 81) - 1)));
        }
        for piece in Piece::all() {
            for square in Square::all() {
                for &occupied in &occupancies {
                    assert_eq!(
                        attacks_of(piece, square, occupied),
                        reference(piece, square, occupied),
                        "{piece:?} on {square:?} with {occupied:?}"
                    );
                }
            }
        }
    }
}
