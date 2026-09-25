//! Zobrist hashing keys.
//!
//! All keys are const-evaluated from a fixed seed with our own splitmix64, so
//! the tables are deterministic and self-generated (no external tables, per
//! the licensing policy).

use shogi_core::{Color, Piece, PieceKind, Square};

/// The largest count a [`Hand`](shogi_core::Hand) can hold, so [`hand_key`]
/// is total over every hand a position can express.
const MAX_HAND_COUNT: usize = u8::MAX as usize;
/// The most of one kind a standard set can put in one hand (18 pawns), and
/// the last count drawn before [`KEYS`]'s `side`.
pub(crate) const STANDARD_HAND_COUNT: usize = 18;
/// Piece kinds that can be held in hand (pawn..rook).
const HAND_KINDS: usize = 7;

/// `repr(C)` keeps the fields in the order written, so the board keys lead.
#[repr(C)]
struct Keys {
    /// Keyed by `[color][piece_kind][square]`.
    board: [[[u64; Square::NUM]; PieceKind::NUM]; Color::NUM],
    /// Keyed by `[count][color][piece_kind]`, count-major so the counts a game
    /// reaches sit together rather than one row width apart. The key of a
    /// hand holding `n` pieces is the XOR of entries `1..=n`, so
    /// adding/removing one piece XORs a single entry.
    hand: [[[u64; HAND_KINDS]; Color::NUM]; MAX_HAND_COUNT + 1],
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

/// ⚠️ The draw order below fixes every key. Changing it is invisible here (any
/// distinct keys hash correctly) and rebaselines every transposition-table
/// result a consumer has recorded; `the_draw_order_is_fixed` pins it.
static KEYS: Keys = keys();

const fn keys() -> Keys {
    // b"shunsai" as a fixed seed.
    let mut state = 0x0073_6875_6e73_6169;
    let mut keys = Keys {
        board: [[[0; Square::NUM]; PieceKind::NUM]; Color::NUM],
        hand: [[[0; HAND_KINDS]; Color::NUM]; MAX_HAND_COUNT + 1],
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
            while count <= STANDARD_HAND_COUNT {
                keys.hand[count][color][piece_kind] = splitmix64(&mut state);
                count += 1;
            }
            piece_kind += 1;
        }
        color += 1;
    }
    keys.side = splitmix64(&mut state);
    // Counts past a standard set are drawn after `side`, so drawing them
    // moves no key above, and count-major, so every width is a prefix of
    // every wider one: moving `MAX_HAND_COUNT` adds or drops keys without
    // moving any it keeps.
    let mut count = STANDARD_HAND_COUNT + 1;
    while count <= MAX_HAND_COUNT {
        let mut color = 0;
        while color < Color::NUM {
            let mut piece_kind = 0;
            while piece_kind < HAND_KINDS {
                keys.hand[count][color][piece_kind] = splitmix64(&mut state);
                piece_kind += 1;
            }
            color += 1;
        }
        count += 1;
    }
    keys
}

/// The key of `piece` sitting on `square`.
pub(crate) fn board_key(piece: Piece, square: Square) -> u64 {
    let (piece_kind, color) = piece.to_parts();
    KEYS.board[color.array_index()][piece_kind.array_index()][square.array_index()]
}

/// The key toggled when `color`'s hand goes between `count - 1` and `count`
/// pieces of `piece_kind`.
///
/// Every count a hand can hold has one, so a count past what a standard set
/// can reach is unreachable in a legal position rather than out of range.
/// ⚠️ `count` 0 is not a key: an empty hand contributes nothing, so the slot
/// it would read is the one entry never drawn.
pub(crate) fn hand_key(color: Color, piece_kind: PieceKind, count: u8) -> u64 {
    debug_assert!(count > 0);
    KEYS.hand[count as usize][color.array_index()][piece_kind.array_index()]
}

/// The key toggled on every side-to-move change.
pub(crate) fn side_key() -> u64 {
    KEYS.side
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-64's offset and prime over `keys` in the order given, one whole key
    /// per step rather than one byte, so byte-wise FNV-1a does not reproduce
    /// these digests. The multiply after each XOR is what makes the digest
    /// read *which key sits in which slot* rather than the set of keys.
    fn fold(keys: &[u64]) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;

        keys.iter()
            .fold(OFFSET, |digest, key| (digest ^ key).wrapping_mul(PRIME))
    }

    /// Every key the table holds for hands up to `max_hand_count`: board,
    /// then hands color → kind → count, then `side`.
    fn walk(max_hand_count: u8) -> Vec<u64> {
        let mut keys = Vec::new();
        for color in Color::all() {
            for piece_kind in PieceKind::all() {
                for square in Square::all() {
                    keys.push(board_key(Piece::new(piece_kind, color), square));
                }
            }
        }
        for color in Color::all() {
            for piece_kind in shogi_core::Hand::all_hand_pieces() {
                for count in 1..=max_hand_count {
                    keys.push(hand_key(color, piece_kind, count));
                }
            }
        }
        keys.push(side_key());
        keys
    }

    /// Every key the table holds. `Keys` is all `u64` and `repr(C)`, so its
    /// size counts them, less the count-0 entries never drawn — which is what
    /// makes a key family missing from [`walk`] fail here.
    fn whole_table() -> Vec<u64> {
        let keys = walk(MAX_HAND_COUNT as u8);
        assert_eq!(
            (keys.len() + Color::NUM * HAND_KINDS) * size_of::<u64>(),
            size_of::<Keys>()
        );
        keys
    }

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
    /// The fold walks every entry in a fixed order and mixes
    /// order-sensitively, so it reads *which key sits in which slot* rather
    /// than the set of keys. Endpoints alone cannot do this: the first and
    /// last values drawn are fixed points of any re-nesting of the loops, so a
    /// re-nesting that renumbers nearly every board key leaves them where they
    /// were.
    ///
    /// Every key here reaches a consumer once released. Narrowing
    /// [`MAX_HAND_COUNT`] folds fewer of them and moves this digest without
    /// moving a key; any other change that moves it renumbers keys a consumer
    /// may hold.
    #[test]
    fn the_draw_order_is_fixed() {
        assert_eq!(fold(&whole_table()), 0x2e2e_3616_c5d3_2942);
    }

    /// Pins the keys of the 18-wide table, which every release through 0.1.2
    /// shipped, and nothing else. Its bound and its count are literals rather
    /// than constants because it states what those releases put in a
    /// consumer's transposition table, not what the table holds now.
    #[test]
    fn the_published_keys_have_not_moved() {
        let keys = walk(18);
        assert_eq!(keys.len(), 2521);
        assert_eq!(fold(&keys), 0x89ab_5be2_4ee9_75f4);
    }

    #[test]
    fn keys_are_distinct() {
        let keys = whole_table();
        let distinct: std::collections::HashSet<u64> = keys.iter().copied().collect();
        assert_eq!(distinct.len(), keys.len());
        assert!(!distinct.contains(&0));
    }
}
