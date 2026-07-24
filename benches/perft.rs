//! Perft throughput (nodes/sec) on the fixed positions of DESIGN.md §4.

mod common;

use std::time::Duration;

use criterion::{
    BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group, criterion_main,
};
use shunsai::Position;

/// Identical to `tests/perft.rs` (leaf bulk counting).
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
        // Guard: the measured work is exactly the claimed node count.
        assert_eq!(perft(&mut position, depth), nodes);
        group.throughput(Throughput::Elements(nodes));
        group.bench_with_input(BenchmarkId::new(name, depth), &depth, |b, &depth| {
            b.iter(|| perft(&mut position, depth))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_perft);
criterion_main!(benches);
