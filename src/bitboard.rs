//! A simple `u128`-backed bitboard.

use core::fmt;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

use shogi_core::Square;

/// The file (1..=9) of the square with array index `index`.
#[inline(always)]
pub(crate) const fn file_of(index: usize) -> i8 {
    (index / 9) as i8 + 1
}

/// The rank (1..=9) of the square with array index `index`.
#[inline(always)]
pub(crate) const fn rank_of(index: usize) -> i8 {
    (index % 9) as i8 + 1
}

/// The array index of `(file, rank)`; both must be on the board.
#[inline(always)]
pub(crate) const fn index_of(file: i8, rank: i8) -> usize {
    ((file - 1) * 9 + (rank - 1)) as usize
}

#[inline(always)]
pub(crate) const fn on_board(file: i8, rank: i8) -> bool {
    1 <= file && file <= 9 && 1 <= rank && rank <= 9
}

/// A set of squares, one bit per square.
///
/// Bit `i` corresponds to the square whose [`Square::array_index`] is `i`,
/// i.e. the same file-major order as `shogi_core` (file 1 occupies bits
/// 0..9, file 2 occupies bits 9..18, ...).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Bitboard(u128);

impl Bitboard {
    /// The empty set.
    pub const EMPTY: Self = Self(0);
    /// All 81 squares.
    pub const ALL: Self = Self((1 << 81) - 1);

    /// A set containing only `square`.
    #[inline(always)]
    pub const fn single(square: Square) -> Self {
        Self(1 << square.array_index())
    }

    /// The raw bits: bit `i` is the square with [`Square::array_index`] `i`.
    #[inline(always)]
    pub(crate) const fn bits(self) -> u128 {
        self.0
    }

    /// A set from raw bits. Bits 81.. must be clear; the tables built with
    /// this are const-evaluated, so a violation is a compile error.
    #[inline(always)]
    pub(crate) const fn from_bits(bits: u128) -> Self {
        debug_assert!(bits >> 81 == 0);
        Self(bits)
    }

    /// Whether `square` is in the set.
    #[inline(always)]
    pub const fn contains(self, square: Square) -> bool {
        self.0 & (1 << square.array_index()) != 0
    }

    /// Whether the set has no squares.
    #[inline(always)]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The number of squares in the set.
    #[inline(always)]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// All squares of `file` (1..=9).
    #[inline(always)]
    pub const fn file(file: u8) -> Self {
        debug_assert!(matches!(file, 1..=9));
        Self(0x1ff << ((file - 1) * 9))
    }

    /// Removes and returns the square with the smallest array index.
    #[inline(always)]
    pub fn pop(&mut self) -> Option<Square> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Square::from_u8(index + 1)
    }

    /// Calls `f` with every square in the set, lowest array index first.
    ///
    /// The bulk form of [`Bitboard::pop`], faster only when a caller drains a
    /// whole set. It walks **one 64-bit word at a time** rather than popping
    /// from the `u128` — on aarch64 both `u128::trailing_zeros` and
    /// `x & (x - 1)` cost roughly twice their 64-bit counterparts, and the 81
    /// bits are contiguous, so the high word is usually empty — and it builds
    /// each [`Square`] unchecked, where `pop` re-validates a range this
    /// type's invariant already guarantees.
    #[inline(always)]
    pub(crate) fn for_each_square(self, mut f: impl FnMut(Square)) {
        let mut low = self.0 as u64;
        while low != 0 {
            let index = low.trailing_zeros() as u8;
            low &= low - 1;
            // Safety: bit `i` is the square whose `array_index()` is `i`, so
            // `index + 1` is in `1..=64`.
            f(unsafe { Square::from_u8_unchecked(index + 1) });
        }
        let mut high = (self.0 >> 64) as u64;
        while high != 0 {
            let index = high.trailing_zeros() as u8;
            high &= high - 1;
            // Safety: no operation can set a bit at 81 or above, so
            // `index + 65` is in `65..=81`. `ALL` and `Not` mask,
            // `single`/`file` are in range by construction, and the bitwise
            // ops preserve it — `bits_stay_within_the_board` guards that
            // much. `from_bits` is the one way in that is not structural.
            // Its const callers in `tables`/`sliders` turn its
            // `debug_assert!` into a compile error; its four runtime callers
            // in `sliders` are in range by construction but held to it by
            // nothing else, which is why a **debug** `cargo test` — where
            // perft drives every one of them — is part of this function's
            // guard and not only a correctness run.
            f(unsafe { Square::from_u8_unchecked(index + 65) });
        }
    }
}

impl BitAnd for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl BitOr for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitXor for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn bitxor(self, rhs: Self) -> Self {
        Self(self.0 ^ rhs.0)
    }
}

impl BitAndAssign for Bitboard {
    #[inline(always)]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitOrAssign for Bitboard {
    #[inline(always)]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitXorAssign for Bitboard {
    #[inline(always)]
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl Not for Bitboard {
    type Output = Self;
    /// Complement within the 81 squares of the board.
    #[inline(always)]
    fn not(self) -> Self {
        Self(!self.0 & Self::ALL.0)
    }
}

/// Iterates over the squares in ascending order of array index.
impl Iterator for Bitboard {
    type Item = Square;

    #[inline(always)]
    fn next(&mut self) -> Option<Square> {
        self.pop()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = self.count() as usize;
        (count, Some(count))
    }
}

impl ExactSizeIterator for Bitboard {}

impl fmt::Debug for Bitboard {
    /// Draws the board as seen by Black: file 9 at the left, rank `a` at the top.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in 1..=9 {
            for file in (1..=9).rev() {
                let square = Square::new(file, rank).unwrap();
                f.write_str(if self.contains(square) { " *" } else { " ." })?;
            }
            if rank < 9 {
                f.write_str("\n")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_contains() {
        for square in Square::all() {
            let bb = Bitboard::single(square);
            assert_eq!(bb.count(), 1);
            for other in Square::all() {
                assert_eq!(bb.contains(other), square == other);
            }
        }
    }

    #[test]
    fn all_and_not() {
        assert_eq!(Bitboard::ALL.count(), 81);
        assert_eq!(!Bitboard::ALL, Bitboard::EMPTY);
        assert_eq!(!Bitboard::EMPTY, Bitboard::ALL);
        for square in Square::all() {
            let bb = Bitboard::single(square);
            assert!(!(!bb).contains(square));
            assert_eq!((!bb).count(), 80);
        }
    }

    #[test]
    fn iteration_order() {
        let squares = [
            Square::new(1, 1).unwrap(),
            Square::new(5, 5).unwrap(),
            Square::new(9, 9).unwrap(),
        ];
        let bb = squares.iter().fold(Bitboard::EMPTY, |acc, &square| {
            acc | Bitboard::single(square)
        });
        assert_eq!(bb.collect::<Vec<_>>(), squares);
        assert_eq!(bb.len(), 3);
    }

    #[test]
    fn for_each_square_matches_pop() {
        // Both words, and the boundary between them (array indices 63/64,
        // i.e. the only place the two loops in `for_each_square` meet).
        let mut cases = vec![Bitboard::EMPTY, Bitboard::ALL];
        for square in Square::all() {
            cases.push(Bitboard::single(square));
            cases.push(!Bitboard::single(square));
        }
        for file in 1..=9 {
            cases.push(Bitboard::file(file));
        }
        let mut state = 0x9e3779b97f4a7c15u64;
        for _ in 0..2000 {
            let mut bits = 0u128;
            for _ in 0..2 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                bits = (bits << 64) | state as u128;
            }
            cases.push(Bitboard::ALL & Bitboard::from_bits(bits & ((1 << 81) - 1)));
        }
        for bb in cases {
            let mut seen = Vec::new();
            bb.for_each_square(|square| seen.push(square));
            assert_eq!(seen, bb.collect::<Vec<_>>(), "mismatch on {bb:?}");
        }
    }

    /// `for_each_square` builds squares with `from_u8_unchecked`, which is
    /// sound only because no `Bitboard` ever holds a bit at 81 or above.
    #[test]
    fn bits_stay_within_the_board() {
        let inputs = [
            Bitboard::EMPTY,
            Bitboard::ALL,
            Bitboard::single(Square::new(1, 1).unwrap()),
            Bitboard::single(Square::new(9, 9).unwrap()),
            Bitboard::file(1),
            Bitboard::file(9),
        ];
        let mut all = Vec::new();
        for a in inputs {
            all.push(!a);
            for b in inputs {
                all.extend([a & b, a | b, a ^ b]);
            }
        }
        for bb in all {
            assert_eq!(bb.bits() >> 81, 0, "out-of-board bit set in {bb:?}");
        }
    }

    #[test]
    fn file_masks() {
        for file in 1..=9 {
            let bb = Bitboard::file(file);
            assert_eq!(bb.count(), 9);
            for square in bb {
                assert_eq!(square.file(), file);
            }
        }
    }
}
