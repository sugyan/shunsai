//! Perft: counts leaf nodes of the legal-move tree.
//!
//! Usage: `cargo run --release --example perft -- <depth> [sfen]`
//! (omit `sfen` for the initial position)
//!
//! Leaves are bulk-counted at depth 1 (the cross-library comparison
//! convention from DESIGN.md §4).

use std::time::Instant;

use shogi_core::PartialPosition;
use shogi_usi_parser::FromUsi;
use shunsai::Position;

fn perft(position: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = position.legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut nodes = 0;
    for mv in moves {
        position.do_move(mv);
        nodes += perft(position, depth - 1);
        position.undo_move(mv);
    }
    nodes
}

fn main() {
    let mut args = std::env::args().skip(1);
    let depth: u32 = args
        .next()
        .expect("usage: perft <depth> [sfen]")
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
    for d in 1..=depth {
        let start = Instant::now();
        let nodes = perft(&mut position, d);
        let elapsed = start.elapsed();
        println!(
            "perft({d}) = {nodes} ({:.3}s, {:.0} Mnps)",
            elapsed.as_secs_f64(),
            nodes as f64 / elapsed.as_secs_f64() / 1_000_000.0,
        );
    }
}
