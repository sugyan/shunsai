//! Zobrist hashing keys.
//!
//! All keys are generated at first use from a fixed seed with our own
//! splitmix64, so the tables are deterministic and self-generated (no
//! external tables, per the licensing policy).

use std::sync::LazyLock;

use shogi_core::{Color, Piece, PieceKind, Square};

/// Maximum count of one piece kind in one hand (18 pawns).
const MAX_HAND_COUNT: usize = 18;
/// Piece kinds that can be held in hand (pawn..rook).
const HAND_KINDS: usize = 7;

struct Keys {
    /// Keyed by `[color][piece_kind][square]`.
    board: [[[u64; 81]; 14]; 2],
    /// Keyed by `[color][piece_kind][count]`; the key of a hand holding `n`
    /// pieces is the XOR of entries `1..=n`, so adding/removing one piece
    /// XORs a single entry.
    hand: [[[u64; MAX_HAND_COUNT + 1]; HAND_KINDS]; 2],
    side: u64,
}

/// splitmix64 (public-domain algorithm by Sebastiano Vigna).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

static KEYS: LazyLock<Box<Keys>> = LazyLock::new(|| {
    // b"shunsai" as a fixed seed.
    let mut state = 0x0073_6875_6e73_6169;
    let mut keys = Box::new(Keys {
        board: [[[0; 81]; 14]; 2],
        hand: [[[0; MAX_HAND_COUNT + 1]; HAND_KINDS]; 2],
        side: 0,
    });
    for color in &mut keys.board {
        for piece_kind in color {
            for square in piece_kind {
                *square = splitmix64(&mut state);
            }
        }
    }
    for color in &mut keys.hand {
        for piece_kind in color {
            // Entry 0 stays zero: an empty hand contributes nothing.
            for count in &mut piece_kind[1..] {
                *count = splitmix64(&mut state);
            }
        }
    }
    keys.side = splitmix64(&mut state);
    keys
});

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
