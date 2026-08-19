//! Zobrist hashing keys.
//!
//! All keys are const-evaluated from a fixed seed with our own splitmix64, so
//! the tables are deterministic and self-generated (no external tables, per
//! the licensing policy).

use shogi_core::{Color, Piece, PieceKind, Square};

/// Maximum count of one piece kind in one hand (18 pawns).
const MAX_HAND_COUNT: usize = 18;
/// Piece kinds that can be held in hand (pawn..rook).
const HAND_KINDS: usize = 7;

struct Keys {
    /// Keyed by `[color][piece_kind][square]`.
    board: [[[u64; Square::NUM]; PieceKind::NUM]; Color::NUM],
    /// Keyed by `[color][piece_kind][count]`; the key of a hand holding `n`
    /// pieces is the XOR of entries `1..=n`, so adding/removing one piece
    /// XORs a single entry.
    hand: [[[u64; MAX_HAND_COUNT + 1]; HAND_KINDS]; Color::NUM],
    side: u64,
}

/// splitmix64 (public-domain algorithm by Sebastiano Vigna).
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// ⚠️ The draw order below fixes every key. Reordering the loops, or drawing
/// one extra value, renumbers the whole table — which is invisible here (any
/// distinct keys hash correctly) and rebaselines every transposition-table
/// result a consumer has recorded.
static KEYS: Keys = keys();

const fn keys() -> Keys {
    // b"shunsai" as a fixed seed.
    let mut state = 0x0073_6875_6e73_6169;
    let mut keys = Keys {
        board: [[[0; Square::NUM]; PieceKind::NUM]; Color::NUM],
        hand: [[[0; MAX_HAND_COUNT + 1]; HAND_KINDS]; Color::NUM],
        side: 0,
    };
    let mut color = 0;
    while color < Color::NUM {
        let mut piece_kind = 0;
        while piece_kind < PieceKind::NUM {
            let mut square = 0;
            while square < Square::NUM {
                keys.board[color][piece_kind][square] = splitmix64(&mut state);
                square += 1;
            }
            piece_kind += 1;
        }
        color += 1;
    }
    let mut color = 0;
    while color < Color::NUM {
        let mut piece_kind = 0;
        while piece_kind < HAND_KINDS {
            // Entry 0 stays zero: an empty hand contributes nothing.
            let mut count = 1;
            while count <= MAX_HAND_COUNT {
                keys.hand[color][piece_kind][count] = splitmix64(&mut state);
                count += 1;
            }
            piece_kind += 1;
        }
        color += 1;
    }
    keys.side = splitmix64(&mut state);
    keys
}

/// The key of `piece` sitting on `square`.
pub(crate) fn board_key(piece: Piece, square: Square) -> u64 {
    let (piece_kind, color) = piece.to_parts();
    KEYS.board[color.array_index()][piece_kind.array_index()][square.array_index()]
}

/// The key toggled when `color`'s hand goes between `count - 1` and `count`
/// pieces of `piece_kind` (`count` >= 1).
pub(crate) fn hand_key(color: Color, piece_kind: PieceKind, count: u8) -> u64 {
    debug_assert!(matches!(count, 1..=18));
    KEYS.hand[color.array_index()][piece_kind.array_index()][count as usize]
}

/// The key toggled on every side-to-move change.
pub(crate) fn side_key() -> u64 {
    KEYS.side
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let piece = Piece::new(PieceKind::Pawn, Color::Black);
        let square = Square::new(7, 7).unwrap();
        assert_eq!(board_key(piece, square), board_key(piece, square));
    }

    /// Pins the **draw order**, which [`KEYS`] warns is load-bearing: any
    /// distinct keys hash correctly, so renumbering the table breaks nothing
    /// here and silently rebaselines every transposition-table result a
    /// consumer has recorded. Three witnesses bracket the sequence — the
    /// first value drawn, the last one drawn into `hand`, and the last one
    /// drawn at all — so a change to any loop bound or to their order moves
    /// at least one of them.
    #[test]
    fn the_draw_order_is_fixed() {
        let first_square = Square::all().next().unwrap();
        assert_eq!(first_square.array_index(), 0);
        assert_eq!(
            board_key(Piece::new(PieceKind::Pawn, Color::Black), first_square),
            0xa4c4_9cae_2623_a134,
        );
        assert_eq!(
            hand_key(Color::White, PieceKind::Rook, MAX_HAND_COUNT as u8),
            0x3a35_82c7_4d76_3672,
        );
        assert_eq!(side_key(), 0x87bb_ee60_9028_3a29);
    }

    #[test]
    fn keys_are_distinct() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for piece in Piece::all() {
            for square in Square::all() {
                assert!(seen.insert(board_key(piece, square)));
            }
        }
        for color in Color::all() {
            for piece_kind in shogi_core::Hand::all_hand_pieces() {
                for count in 1..=18 {
                    assert!(seen.insert(hand_key(color, piece_kind, count)));
                }
            }
        }
        assert!(seen.insert(side_key()));
        assert!(!seen.contains(&0));
    }
}
