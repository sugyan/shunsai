//! Position management: do/undo moves with incremental Zobrist keys.

use core::fmt;

use shogi_core::{Color, Hand, Move, PartialPosition, Piece, PieceKind, Square, ToUsi};

use crate::bitboard::Bitboard;
use crate::zobrist;

/// Per-move information needed by [`Position::undo_move`].
#[derive(Clone)]
struct State {
    captured: Option<Piece>,
    key: u64,
}

/// A shogi position with fast do/undo and an incrementally maintained
/// Zobrist key.
///
/// The board state is stored redundantly (mailbox + bitboards); moves are
/// assumed to be legal (generate them with [`Position::legal_moves`]).
/// A side may have no king (as in tsume-shogi problems).
#[derive(Clone)]
pub struct Position {
    board: [Option<Piece>; Square::NUM],
    /// One bitboard per [`PieceKind`], both colors together.
    piece_bb: [Bitboard; PieceKind::NUM],
    color_bb: [Bitboard; Color::NUM],
    hands: [Hand; Color::NUM],
    side_to_move: Color,
    ply: u16,
    kings: [Option<Square>; Color::NUM],
    key: u64,
    states: Vec<State>,
}

impl Position {
    /// Builds a [`Position`] from a [`PartialPosition`].
    pub fn new(partial: PartialPosition) -> Self {
        let hands = [
            partial.hand_of_a_player(Color::Black),
            partial.hand_of_a_player(Color::White),
        ];
        let mut position = Self {
            board: [None; Square::NUM],
            piece_bb: [Bitboard::EMPTY; PieceKind::NUM],
            color_bb: [Bitboard::EMPTY; Color::NUM],
            hands,
            side_to_move: partial.side_to_move(),
            ply: partial.ply(),
            kings: [None; Color::NUM],
            key: 0,
            states: Vec::new(),
        };
        for square in Square::all() {
            if let Some(piece) = partial.piece_at(square) {
                position.put_piece(square, piece);
            }
        }
        for color in Color::all() {
            for piece_kind in Hand::all_hand_pieces() {
                let count = hands[color.array_index()].count(piece_kind).unwrap_or(0);
                for i in 1..=count {
                    position.key ^= zobrist::hand_key(color, piece_kind, i);
                }
            }
        }
        if position.side_to_move == Color::White {
            position.key ^= zobrist::side_key();
        }
        position
    }

    /// The initial position.
    pub fn startpos() -> Self {
        Self::new(PartialPosition::startpos())
    }

    /// The piece on `square`, if any.
    #[inline(always)]
    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        self.board[square.array_index()]
    }

    /// The player to move.
    #[inline(always)]
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    /// The current ply (1 at the start of a game).
    #[inline(always)]
    pub fn ply(&self) -> u16 {
        self.ply
    }

    /// The hand of `color`.
    #[inline(always)]
    pub fn hand(&self, color: Color) -> Hand {
        self.hands[color.array_index()]
    }

    /// The square of `color`'s king, if it is on the board.
    #[inline(always)]
    pub fn king_square(&self, color: Color) -> Option<Square> {
        self.kings[color.array_index()]
    }

    /// The Zobrist key (board, hands and side to move).
    #[inline(always)]
    pub fn key(&self) -> u64 {
        self.key
    }

    /// The squares occupied by pieces of either color.
    #[inline(always)]
    pub fn occupied(&self) -> Bitboard {
        self.color_bb[0] | self.color_bb[1]
    }

    /// The squares occupied by `color`'s pieces.
    #[inline(always)]
    pub fn player_bb(&self, color: Color) -> Bitboard {
        self.color_bb[color.array_index()]
    }

    /// The squares occupied by pieces of `piece_kind`, either color.
    #[inline(always)]
    pub fn piece_kind_bb(&self, piece_kind: PieceKind) -> Bitboard {
        self.piece_bb[piece_kind.array_index()]
    }

    /// Makes `mv` on the board. `mv` must be legal in this position.
    pub fn do_move(&mut self, mv: Move) {
        let side = self.side_to_move;
        let captured = match mv {
            Move::Normal { to, .. } => self.piece_at(to),
            Move::Drop { .. } => None,
        };
        self.states.push(State {
            captured,
            key: self.key,
        });
        match mv {
            Move::Normal { from, to, promote } => {
                let piece = self.piece_at(from).expect("do_move: no piece to move");
                debug_assert_eq!(piece.color(), side);
                if let Some(captured) = captured {
                    debug_assert_ne!(captured.color(), side);
                    self.remove_piece(to, captured);
                    let piece_kind = captured.piece_kind();
                    self.add_to_hand(side, piece_kind.unpromote().unwrap_or(piece_kind));
                }
                self.remove_piece(from, piece);
                let placed = if promote {
                    piece.promote().expect("do_move: piece cannot promote")
                } else {
                    piece
                };
                self.put_piece(to, placed);
            }
            Move::Drop { piece, to } => {
                debug_assert_eq!(piece.color(), side);
                debug_assert!(self.piece_at(to).is_none());
                self.remove_from_hand(side, piece.piece_kind());
                self.put_piece(to, piece);
            }
        }
        self.side_to_move = side.flip();
        self.key ^= zobrist::side_key();
        self.ply = self.ply.wrapping_add(1);
    }

    /// Undoes `mv`, the last move made with [`Position::do_move`].
    pub fn undo_move(&mut self, mv: Move) {
        let state = self.states.pop().expect("undo_move: no move to undo");
        let side = self.side_to_move.flip();
        match mv {
            Move::Normal { from, to, promote } => {
                let placed = self.piece_at(to).expect("undo_move: no piece to put back");
                self.remove_piece(to, placed);
                let piece = if promote {
                    placed
                        .unpromote()
                        .expect("undo_move: piece was not promoted")
                } else {
                    placed
                };
                self.put_piece(from, piece);
                if let Some(captured) = state.captured {
                    let piece_kind = captured.piece_kind();
                    self.remove_from_hand(side, piece_kind.unpromote().unwrap_or(piece_kind));
                    self.put_piece(to, captured);
                }
            }
            Move::Drop { piece, to } => {
                self.remove_piece(to, piece);
                self.add_to_hand(side, piece.piece_kind());
            }
        }
        self.side_to_move = side;
        self.ply = self.ply.wrapping_sub(1);
        // Cheaper than reversing every XOR above.
        self.key = state.key;
    }

    fn put_piece(&mut self, square: Square, piece: Piece) {
        let (piece_kind, color) = piece.to_parts();
        debug_assert!(self.board[square.array_index()].is_none());
        self.board[square.array_index()] = Some(piece);
        let single = Bitboard::single(square);
        self.piece_bb[piece_kind.array_index()] |= single;
        self.color_bb[color.array_index()] |= single;
        if piece_kind == PieceKind::King {
            self.kings[color.array_index()] = Some(square);
        }
        self.key ^= zobrist::board_key(piece, square);
    }

    fn remove_piece(&mut self, square: Square, piece: Piece) {
        let (piece_kind, color) = piece.to_parts();
        debug_assert_eq!(self.board[square.array_index()], Some(piece));
        self.board[square.array_index()] = None;
        let single = Bitboard::single(square);
        self.piece_bb[piece_kind.array_index()] ^= single;
        self.color_bb[color.array_index()] ^= single;
        if piece_kind == PieceKind::King {
            self.kings[color.array_index()] = None;
        }
        self.key ^= zobrist::board_key(piece, square);
    }

    fn add_to_hand(&mut self, color: Color, piece_kind: PieceKind) {
        let hand = &mut self.hands[color.array_index()];
        *hand = hand.added(piece_kind).expect("hand overflow");
        let count = hand.count(piece_kind).expect("not a hand piece");
        self.key ^= zobrist::hand_key(color, piece_kind, count);
    }

    fn remove_from_hand(&mut self, color: Color, piece_kind: PieceKind) {
        let hand = &mut self.hands[color.array_index()];
        let count = hand.count(piece_kind).expect("not a hand piece");
        debug_assert!(count > 0);
        *hand = hand.removed(piece_kind).expect("hand underflow");
        self.key ^= zobrist::hand_key(color, piece_kind, count);
    }
}

impl fmt::Debug for Position {
    /// Draws the board as seen by Black, with USI piece letters.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in 1..=9 {
            for file in (1..=9).rev() {
                let square = Square::new(file, rank).unwrap();
                match self.piece_at(square) {
                    Some(piece) => {
                        let usi = piece.to_usi_owned();
                        write!(f, "{:>3}", usi)?;
                    }
                    None => write!(f, "  .")?,
                }
            }
            writeln!(f)?;
        }
        let hand_string = |hand: Hand| {
            let mut s = String::new();
            for piece_kind in Hand::all_hand_pieces() {
                let count = hand.count(piece_kind).unwrap_or(0);
                if count > 0 {
                    let usi = Piece::new(piece_kind, Color::Black).to_usi_owned();
                    s.push_str(&format!("{usi}{count} "));
                }
            }
            s.trim_end().to_owned()
        };
        writeln!(
            f,
            "hands: [B: [{}], W: [{}]]",
            hand_string(self.hands[0]),
            hand_string(self.hands[1])
        )?;
        write!(
            f,
            "side: {:?}, ply: {}, key: {:016x}",
            self.side_to_move, self.ply, self.key
        )
    }
}

/// Equality of the position itself: the undo history is not compared.
impl PartialEq for Position {
    fn eq(&self, other: &Self) -> bool {
        self.board == other.board
            && self.piece_bb == other.piece_bb
            && self.color_bb == other.color_bb
            && self.hands == other.hands
            && self.side_to_move == other.side_to_move
            && self.ply == other.ply
            && self.kings == other.kings
            && self.key == other.key
    }
}

impl Eq for Position {}

impl Default for Position {
    fn default() -> Self {
        Self::startpos()
    }
}

impl From<PartialPosition> for Position {
    fn from(partial: PartialPosition) -> Self {
        Self::new(partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(from: (u8, u8), to: (u8, u8), promote: bool) -> Move {
        Move::Normal {
            from: Square::new(from.0, from.1).unwrap(),
            to: Square::new(to.0, to.1).unwrap(),
            promote,
        }
    }

    #[test]
    fn startpos_basics() {
        let position = Position::startpos();
        assert_eq!(position.side_to_move(), Color::Black);
        assert_eq!(position.ply(), 1);
        assert_eq!(position.occupied().count(), 40);
        assert_eq!(
            position.piece_at(Square::new(7, 7).unwrap()),
            Some(Piece::new(PieceKind::Pawn, Color::Black))
        );
        assert_eq!(
            position.king_square(Color::Black),
            Some(Square::new(5, 9).unwrap())
        );
        assert_eq!(
            position.king_square(Color::White),
            Some(Square::new(5, 1).unwrap())
        );
        assert_eq!(position.piece_kind_bb(PieceKind::Pawn).count(), 18);
    }

    #[test]
    fn do_undo_restores_everything() {
        // 7g7f 3c3d 8h2b+ (bishop exchange) 3a2b, then B*4e.
        let moves = [
            mv((7, 7), (7, 6), false),
            mv((3, 3), (3, 4), false),
            mv((8, 8), (2, 2), true),
            mv((3, 1), (2, 2), false),
            Move::Drop {
                piece: Piece::new(PieceKind::Bishop, Color::Black),
                to: Square::new(4, 5).unwrap(),
            },
        ];
        let mut position = Position::startpos();
        let mut snapshots = vec![position.clone()];
        for &m in &moves {
            position.do_move(m);
            snapshots.push(position.clone());
        }
        // The bishop exchange put a bishop in each hand; Black dropped theirs.
        assert_eq!(
            position.hand(Color::Black).count(PieceKind::Bishop),
            Some(0)
        );
        assert_eq!(
            position.hand(Color::White).count(PieceKind::Bishop),
            Some(1)
        );
        for &m in moves.iter().rev() {
            assert_eq!(&position, snapshots.last().unwrap());
            snapshots.pop();
            position.undo_move(m);
        }
        assert_eq!(&position, &snapshots[0]);
        assert_eq!(position, Position::startpos());
    }

    #[test]
    fn transposition_same_key() {
        let mut a = Position::startpos();
        for m in [
            mv((7, 7), (7, 6), false),
            mv((3, 3), (3, 4), false),
            mv((2, 7), (2, 6), false),
        ] {
            a.do_move(m);
        }
        let mut b = Position::startpos();
        for m in [
            mv((2, 7), (2, 6), false),
            mv((3, 3), (3, 4), false),
            mv((7, 7), (7, 6), false),
        ] {
            b.do_move(m);
        }
        assert_eq!(a.key(), b.key());
        assert_ne!(a.key(), Position::startpos().key());
    }

    #[test]
    fn promotion_and_capture_key_consistency() {
        // Reaching the same position via do_move vs. from a PartialPosition
        // must produce the same key.
        let moves = [
            mv((7, 7), (7, 6), false),
            mv((3, 3), (3, 4), false),
            mv((8, 8), (2, 2), true),
        ];
        let mut position = Position::startpos();
        let mut partial = PartialPosition::startpos();
        for &m in &moves {
            position.do_move(m);
            partial.make_move(m).unwrap();
        }
        let rebuilt = Position::new(partial);
        assert_eq!(position.key(), rebuilt.key());
        assert_eq!(position, rebuilt);
    }
}
