//! A fixed-depth alpha-beta search, written against **only** the public API.
//!
//! Usage: `cargo run --release --example search -- <depth> [sfen]`
//! (omit `sfen` for the initial position)
//!
//! Perft exercises generation and do/undo, but not the way a search uses them:
//! it never keeps a key, never cuts off a subtree, and never asks what a move
//! captures. This example is the standing check that the published surface is
//! enough to build a search on — the only `shunsai` item it imports is
//! `Position`. No `_bench-internals`, no crate internals.
//! Anything a search needs and cannot get from here is an API gap, which is
//! worth finding while the surface is still free to change.
//!
//! It is deliberately a weak player — material only, no quiescence, no
//! iterative-deepening move ordering beyond captures-first. Strength belongs
//! to the search crate, not to a compile check that lives here.

use std::ops::ControlFlow;
use std::time::Instant;

use shogi_core::{Hand, Move, PartialPosition, PieceKind, ToUsi};
use shogi_usi_parser::FromUsi;
use shunsai::Position;

/// The most legal moves any shogi position has — the count of the max-moves
/// position in DESIGN.md §6.
const MAX_LEGAL_MOVES: usize = 593;

/// Beyond any material score, so a mate always outranks a material gain.
const MATE: i32 = 1 << 20;

/// Deeper than this example is ever asked to go; bounds the mate window.
const MAX_PLY: i32 = 128;

/// Rough material values. Only the ratios matter, and that every one of them
/// is far below [`MATE`].
fn value(piece_kind: PieceKind) -> i32 {
    match piece_kind {
        PieceKind::Pawn => 100,
        PieceKind::Lance => 350,
        PieceKind::Knight => 450,
        PieceKind::Silver => 550,
        PieceKind::Gold => 600,
        PieceKind::Bishop => 950,
        PieceKind::Rook => 1100,
        // Both sides always have one, or the position is already decided.
        PieceKind::King => 0,
        PieceKind::ProPawn | PieceKind::ProLance | PieceKind::ProKnight | PieceKind::ProSilver => {
            600
        }
        PieceKind::ProBishop => 1150,
        PieceKind::ProRook => 1300,
    }
}

/// Material, from the side to move's point of view.
fn evaluate(position: &Position) -> i32 {
    let us = position.side_to_move();
    let them = us.flip();
    let mut score = 0;
    for piece_kind in PieceKind::all() {
        let occupied = position.piece_kind_bb(piece_kind);
        let ours = (occupied & position.player_bb(us)).count() as i32;
        let theirs = (occupied & position.player_bb(them)).count() as i32;
        score += value(piece_kind) * (ours - theirs);
    }
    for piece_kind in Hand::all_hand_pieces() {
        let ours = position.hand(us).count(piece_kind).unwrap_or(0) as i32;
        let theirs = position.hand(them).count(piece_kind).unwrap_or(0) as i32;
        score += value(piece_kind) * (ours - theirs);
    }
    score
}

#[derive(Clone, Copy)]
enum Bound {
    Exact,
    /// The true score is at least `score` — the search cut off.
    Lower,
    /// The true score is at most `score` — nothing beat alpha.
    Upper,
}

#[derive(Clone, Copy)]
struct Entry {
    key: u64,
    depth: u32,
    score: i32,
    bound: Bound,
}

/// A fixed-size, always-replace transposition table keyed on
/// [`Position::key`] — the one thing a search wants from the crate that perft
/// never touches.
struct Table {
    entries: Vec<Option<Entry>>,
    mask: usize,
}

impl Table {
    fn new(bits: u32) -> Self {
        let len = 1usize << bits;
        Self {
            entries: vec![None; len],
            mask: len - 1,
        }
    }

    fn probe(&self, key: u64, depth: u32, alpha: i32, beta: i32) -> Option<i32> {
        let entry = self.entries[key as usize & self.mask]?;
        if entry.key != key || entry.depth < depth {
            return None;
        }
        match entry.bound {
            Bound::Exact => Some(entry.score),
            Bound::Lower if entry.score >= beta => Some(entry.score),
            Bound::Upper if entry.score <= alpha => Some(entry.score),
            _ => None,
        }
    }

    fn store(&mut self, key: u64, depth: u32, score: i32, alpha: i32, beta: i32) {
        // A mate score counts plies from the node that found it, so storing
        // one unadjusted would hand a node at another depth a distance
        // measured from somewhere else. Skipping them keeps the table honest
        // without the usual store/probe ply correction.
        if score.abs() >= MATE - MAX_PLY {
            return;
        }
        let bound = if score <= alpha {
            Bound::Upper
        } else if score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        self.entries[key as usize & self.mask] = Some(Entry {
            key,
            depth,
            score,
            bound,
        });
    }
}

/// Every legal move, captures first.
///
/// `piece_at(to)` is all a caller needs to tell a capture from a quiet move,
/// and the ordering is what makes alpha-beta cut off early enough to be worth
/// running.
fn ordered_moves(position: &Position) -> Vec<Move> {
    let mut moves = Vec::with_capacity(MAX_LEGAL_MOVES);
    let _ = position.generate_moves(|set| {
        set.write_into(&mut moves);
        ControlFlow::Continue(())
    });
    moves.sort_by_key(|mv| match *mv {
        Move::Normal { to, .. } => u8::from(position.piece_at(to).is_none()),
        Move::Drop { .. } => 1,
    });
    moves
}

fn search(
    position: &mut Position,
    depth: u32,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    table: &mut Table,
    nodes: &mut u64,
) -> i32 {
    *nodes += 1;
    if depth == 0 {
        return evaluate(position);
    }
    let key = position.key();
    if let Some(score) = table.probe(key, depth, alpha, beta) {
        return score;
    }

    let moves = ordered_moves(position);
    // Shogi has no stalemate: a side with no legal move has lost, and the
    // sooner that happens the better for the other side.
    if moves.is_empty() {
        return -MATE + ply;
    }

    let original_alpha = alpha;
    let mut best = -MATE;
    for mv in &moves {
        let mv = *mv;
        let undo = position.do_move(mv);
        let score = -search(position, depth - 1, ply + 1, -beta, -alpha, table, nodes);
        position.undo_move(mv, undo);
        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break;
        }
    }
    table.store(key, depth, best, original_alpha, beta);
    best
}

/// The root, which differs from an interior node only in having to name the
/// move it chose.
fn search_root(
    position: &mut Position,
    depth: u32,
    table: &mut Table,
    nodes: &mut u64,
) -> (i32, Option<Move>) {
    *nodes += 1;
    let mut alpha = -MATE;
    let mut best_move = None;
    for mv in &ordered_moves(position) {
        let mv = *mv;
        let undo = position.do_move(mv);
        let score = -search(position, depth - 1, 1, -MATE, -alpha, table, nodes);
        position.undo_move(mv, undo);
        if best_move.is_none() || score > alpha {
            alpha = score;
            best_move = Some(mv);
        }
    }
    (alpha, best_move)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let depth: u32 = args
        .next()
        .expect("usage: search <depth> [sfen]")
        .parse()
        .expect("depth must be an integer");
    let rest: Vec<String> = args.collect();
    let partial = if rest.is_empty() {
        PartialPosition::startpos()
    } else {
        let sfen = format!("sfen {}", rest.join(" "));
        PartialPosition::from_usi(&sfen).expect("invalid SFEN")
    };
    let mut position = Position::new(partial);
    let mut table = Table::new(20);

    for d in 1..=depth {
        let mut nodes = 0;
        let start = Instant::now();
        let (score, best) = search_root(&mut position, d, &mut table, &mut nodes);
        let elapsed = start.elapsed();
        println!(
            "depth {d}  score {score:>7}  nodes {nodes:>10}  ({:.3}s)  best {}",
            elapsed.as_secs_f64(),
            best.map_or_else(|| "(none)".to_owned(), |mv| mv.to_usi_owned()),
        );
    }
}
