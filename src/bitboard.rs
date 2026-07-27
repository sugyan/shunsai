//! A simple `u128`-backed bitboard.

use core::fmt;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

use shogi_core::Square;

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
