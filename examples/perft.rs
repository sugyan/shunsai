//! Perft: counts leaf nodes of the legal-move tree.
//!
//! Usage: `cargo run --release --example perft -- [--materialize] <depth> [sfen]`
//! (omit `sfen` for the initial position)
//!
//! Leaves are bulk-counted at depth 1 (the cross-library comparison
//! convention from BENCHMARKS.md). Two ways of obtaining that count are
//! implemented, because the engines we compare against do not all do it the
//! same way — see `perft` and `perft_materialize` below.

use std::ops::ControlFlow;
use std::time::Instant;

use shogi_core::{Move, PartialPosition};
use shogi_usi_parser::FromUsi;
use shunsai::Position;

/// The most legal moves any shogi position has — the count of the max-moves
/// position in DESIGN.md §6.
const MAX_LEGAL_MOVES: usize = 593;

fn perft(position: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    if depth == 1 {
        // Bulk counting: the leaf parents never build a single `Move`.
        let mut nodes = 0;
        let _ = position.generate_moves(|set| {
            nodes += set.len() as u64;
            ControlFlow::Continue(())
        });
        return nodes;
    }
    // Deeper nodes still need the moves themselves, and `do_move` cannot
    // run while the callback holds the position borrowed. `write_into`
    // rather than `extend`: the internal nodes are a small share of this
    // tree, but they are not free, and the `--materialize` driver below has
    // used `write_into` since it was added.
    let mut moves = Vec::with_capacity(MAX_LEGAL_MOVES);
    let _ = position.generate_moves(|set| {
        set.write_into(&mut moves);
        ControlFlow::Continue(())
    });
    let mut nodes = 0;
    for mv in moves {
        position.do_move(mv);
        nodes += perft(position, depth - 1);
        position.undo_move(mv);
    }
    nodes
}

/// The same tree, but leaf parents obtain their count by **materializing**
/// every legal move, which is what every engine in the cross-engine harness
/// does: YaneuraOu counts `MoveList<LEGAL_ALL>(pos).size()`, the apery driver
/// `MoveList<LegalAll>`, and yasai / rshogi / cshogi their respective
/// legal-move lists. `perft` above counts two popcounts per origin off the
/// destination bitboards and constructs nothing.
///
/// Both are kept because they answer different questions. Counting without
/// building moves is what `MoveSet::len()` is for and is the faster path, but
/// it is a perft-only advantage: a search must have the moves. So this driver
/// is the one whose numbers are comparable to the other engines', and the
/// difference between the two is the size of that advantage (BENCHMARKS.md).
///
/// One buffer is threaded through the whole tree, and each ply takes a slice
/// off the end and truncates back on the way out, so the walk allocates
/// nothing — the other engines' leaf move lists are stack arrays, and paying
/// a `malloc` per node here would measure our driver rather than our
/// generator. The caller sizes that buffer from the depth
/// ([`MAX_LEGAL_MOVES`] per ply), so the claim holds for whatever depth is
/// asked for on the command line, not merely for the harness's presets.
fn perft_materialize(position: &mut Position, depth: u32, buf: &mut Vec<Move>) -> u64 {
    if depth == 0 {
        return 1;
    }
    let base = buf.len();
    let _ = position.generate_moves(|set| {
        // `write_into` rather than `buf.extend(set)`: same moves in the same
        // order, but it decides drop-versus-board once per set instead of
        // once per move. The gain sorts by moves per set and is largest on
        // the dense positions; `movegen/*-wi` against `movegen/*-buf` is the
        // instrument, and DECISIONS.md has the figures.
        set.write_into(buf);
        ControlFlow::Continue(())
    });
    if depth == 1 {
        let nodes = (buf.len() - base) as u64;
        buf.truncate(base);
        return nodes;
    }
    let mut nodes = 0;
    let mut i = base;
    while i < buf.len() {
        let mv = buf[i];
        position.do_move(mv);
        nodes += perft_materialize(position, depth - 1, buf);
        position.undo_move(mv);
        i += 1;
    }
    buf.truncate(base);
    nodes
}

fn main() {
    let mut materialize = false;
    let mut positional = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--materialize" {
            materialize = true;
        } else {
            positional.push(arg);
        }
    }
    let mut args = positional.into_iter();
    let depth: u32 = args
        .next()
        .expect("usage: perft [--materialize] <depth> [sfen]")
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
    // Allocated once, outside every timed region: the point of the
    // materializing driver is that the tree walk itself never allocates.
    // Every ply from the root down to the leaf parents holds its own move
    // list at the same time, and no position exceeds MAX_LEGAL_MOVES, so
    // this cannot grow at the requested depth.
    let mut buf = Vec::with_capacity(MAX_LEGAL_MOVES * depth as usize);
    println!(
        "convention: leaf-parent bulk counting, {}",
        if materialize {
            "moves materialized into a reused buffer"
        } else {
            "counted off the destination bitboards"
        }
    );
    for d in 1..=depth {
        let start = Instant::now();
        let nodes = if materialize {
            perft_materialize(&mut position, d, &mut buf)
        } else {
            perft(&mut position, d)
        };
        let elapsed = start.elapsed();
        // Six decimals: shallow depths now finish in single-digit
        // milliseconds, where three would quantize the result away.
        println!(
            "perft({d}) = {nodes} ({:.6}s, {:.0} Mnps)",
            elapsed.as_secs_f64(),
            nodes as f64 / elapsed.as_secs_f64() / 1_000_000.0,
        );
    }
}
