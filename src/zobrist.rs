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
    /// consumer has recorded.
    ///
    /// The fold walks every entry in canonical index order and mixes
    /// order-sensitively, so it reads *which key sits in which slot* rather
    /// than the set of keys. Endpoints alone cannot do this: the first and
    /// last values drawn are fixed points of any re-nesting of the loops, so a
    /// swap that renumbers all 2268 board keys leaves them where they were.
    #[test]
    fn the_draw_order_is_fixed() {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;

        let mut digest = OFFSET;
        let mut folded = 0;
        let mut mix = |key: u64, folded: &mut u32| {
            *folded += 1;
            digest = (digest ^ key).wrapping_mul(PRIME);
        };
        for color in Color::all() {
            for piece_kind in PieceKind::all() {
                for square in Square::all() {
                    mix(
                        board_key(Piece::new(piece_kind, color), square),
                        &mut folded,
                    );
                }
            }
        }
        for color in Color::all() {
            for piece_kind in shogi_core::Hand::all_hand_pieces() {
                for count in 1..=MAX_HAND_COUNT as u8 {
                    mix(hand_key(color, piece_kind, count), &mut folded);
                }
            }
        }
        mix(side_key(), &mut folded);

        // Every key the table holds is in the fold, so nothing can be
        // renumbered outside it.
        assert_eq!(
            folded as usize,
            Color::NUM * PieceKind::NUM * Square::NUM
                + Color::NUM * HAND_KINDS * MAX_HAND_COUNT
                + 1
        );
        assert_eq!(digest, 0x89ab_5be2_4ee9_75f4);
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
