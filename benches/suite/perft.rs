//! Perft throughput (nodes/sec) on the fixed positions of DESIGN.md §4.

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group};
use shunsai::Position;

use crate::common;

/// The M1 allocating `Vec` API (leaf bulk counting).
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

/// The M3 callback API, as `examples/perft.rs` uses it: leaf parents count
/// straight off the destination bitboards and never build a `Move`.
fn perft_cb(position: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    if depth == 1 {
        let mut nodes = 0;
        position.generate_moves(|set| {
            nodes += set.len() as u64;
            false
        });
        return nodes;
    }
    let mut moves = Vec::with_capacity(128);
    position.generate_moves(|set| {
        moves.extend(set);
        false
    });
    let mut nodes = 0;
    for mv in moves {
        position.do_move(mv);
        nodes += perft_cb(position, depth - 1);
        position.undo_move(mv);
    }
    nodes
}

fn bench_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("perft");
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(20)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(10));
    for (name, sfen, depth, nodes) in [
        ("startpos", "startpos", 4u32, 719_731u64),
        ("matsuri", common::MATSURI_SFEN, 3, 4_809_015),
        ("maxmoves", common::MAX_MOVES_SFEN, 2, 105_677),
    ] {
        let mut position = common::position(sfen);
        // Guard: the measured work is exactly the claimed node count, and
        // both APIs walk the same tree.
        assert_eq!(perft(&mut position, depth), nodes);
        assert_eq!(perft_cb(&mut position, depth), nodes);
        group.throughput(Throughput::Elements(nodes));
        group.bench_with_input(BenchmarkId::new(name, depth), &depth, |b, &depth| {
            b.iter(|| perft(&mut position, depth))
        });
        group.bench_with_input(
            BenchmarkId::new(format!("{name}-cb"), depth),
            &depth,
            |b, &depth| b.iter(|| perft_cb(&mut position, depth)),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_perft);
